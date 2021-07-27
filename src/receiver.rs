use crate::*;
#[cfg(feature="async")]
use core::{future::Future, pin::Pin, task::{Context, Poll}};

/// A unique means of receiving messages through a hatch.
///
/// ## Options (all false by default)
///
/// * `close_on_receive` - when true, closes when we successfully receive a message.
/// * `mark_on_drop` - when true, cleanup marks the atomic with a sentinel value for an external
///   process (not provided!) to reclaim it.
///
/// `close_on_receive` may also be set directly on a [`Receiving`] object, in which case it only
/// applies to that object and not any future operations (although if it succeeds, it won't matter).
#[derive(Debug)]
pub struct Receiver<'a, T> {
    hatch: Option<Holder<'a, Hatch<T>>>,
    flags: Flags,
}

/// A `Receiver<T>` is Send if T is Send.
unsafe impl<'a, T: Send> Send for Receiver<'a, T> {}

/// A Receiver is always Sync because it requires a mut ref for mutation.
unsafe impl<'a, T> Sync for Receiver<'a, T> {}

impl<'a, T> Receiver<'a, T> {
    /// Creates a new Receiver.
    ///
    /// ## Safety
    ///
    /// You must not permit multiple live receivers to exist.
    #[inline(always)]
    pub(crate) unsafe fn new(hatch: Holder<'a, Hatch<T>>) -> Self {
        Self { hatch: Some(hatch), flags: DEFAULT }
    }

    #[must_use = "Receiving does nothing unless you call `.now` or poll it as a Future."]
    /// Returns a disposable object for a single receive operation.
    pub fn receive<'b>(&'b mut self) -> Receiving<'a, 'b, T> {
        let flags = self.flags;
        Receiving { receiver: Some(self), flags }
    }

    /// Returns a new [`Sender`] if the old one has closed and we haven't.
    pub fn recover(&mut self) -> Result<sender::Sender<'a, T>, RecoverError> {
        self.hatch.ok_or(RecoverError::Closed).and_then(|hatch| {
            if any_flag(self.flags, LONELY) {
                // We have observed Sender close so we have exclusive access
                Ok(unsafe { self.recover_unchecked().unwrap() })
            } else {
                // We don't know so we have to check.
                let flags = hatch.flags.load(orderings::LOAD);
                if any_flag(flags, S_CLOSE) {
                    Ok(unsafe { self.recover_unchecked().unwrap() })
                } else {
                    Err(RecoverError::Live)
                }
            }
        })
    }

    /// Returns a new [`Sender`] without checking the old one has closed.
    ///
    /// ## Safety
    ///
    /// You must not permit multiple live Senders to exist.
    pub unsafe fn recover_unchecked(&mut self) -> Result<sender::Sender<'a, T>, Closed> {
        self.hatch.map(|h| {
            h.recycle(); // A release store
            self.flags &= !LONELY; // reset flag
            sender::Sender::new(h)
        }).ok_or(Closed)
    }

    /// Gets the value of the `mark_on_drop` option. See the [`Receiver`] docs for an explanation.
    pub fn get_mark_on_drop(&self) -> bool { any_flag(self.flags, MARK_ON_DROP) }

    /// Sets the `mark_on_drop` option. See the [`Receiver`] docs for an explanation.
    pub fn mark_on_drop(mut self, on: bool) -> Self {
        self.flags = toggle_flag(self.flags, MARK_ON_DROP, on);
        self
    }

    /// Gets the value of the `close_on_receive` option. the [`Receiver`] docs for an explanation.
    pub fn get_close_on_receive(&self) -> bool { any_flag(self.flags, CLOSE_ON_SUCCESS) }

    /// Sets the `close_on_receive` option. See the [`Receiver`] docs for an explanation.
    pub fn close_on_receive(mut self, on: bool) -> Self {
        self.flags = toggle_flag(self.flags, CLOSE_ON_SUCCESS, on);
        self
    }

    /// Drops our reference to the Hatch without any synchronisation.
    /// Useful with external synchronisation to avoid a lock cycle.
    ///
    /// Correct use of this generally implies both the Sender and
    /// Receiver leaking and some external management of the Hatch.
    ///
    /// # Safety
    ///
    /// This isn't technically unsafe, but...
    /// * It could cause a memory leak.
    /// * It could cause the Sender to wait forever.
    pub unsafe fn leak(mut receiver: Receiver<T>) { receiver.hatch.take(); }
}

impl<'a, T> Drop for Receiver<'a, T> {
    fn drop(&mut self) {
        if let Some(hatch) = self.hatch.take() {
            if any_flag(self.flags, LONELY) {
                // If we are LONELY, we have exclusive access
                return unsafe { hatch.cleanup(any_flag(self.flags, MARK_ON_DROP)); }
            }
            #[cfg(not(feature="async"))]
            {
                // Without async, we just set the flag and check we won.
                let flags = hatch.flags.fetch_or(R_CLOSE, orderings::MODIFY);
                if any_flag(flags, S_CLOSE) {
                    // Safe because we have seen the sender closed
                    unsafe { hatch.cleanup(any_flag(self.flags, MARK_ON_DROP)); }
                }
            }
            #[cfg(feature="async")] {
                // For async, we have to lock and maybe wake the sender
                let flags = hatch.lock();
                if any_flag(flags, S_CLOSE) {
                    // Safe because we have seen the sender closed
                    return unsafe { hatch.cleanup(any_flag(self.flags, MARK_ON_DROP)); }
                } else {
                    let shared = unsafe { &mut *hatch.inner.get() };
                    let _recv = shared.receiver.take(); // we won't be waiting any more
                    let sender = shared.sender.take();  // we might need to wake them
                    // mark us closed and release the lock in one smooth action.
                    hatch.flags.store(flags | R_CLOSE, orderings::STORE);
                    // Finally, wake the sender if they are waiting.
                    if let Some(waker) = sender { waker.wake(); }
                }
            }
        }
    }
}

/// This is a disposable object which performs a single receive operation.
///
/// The `now` method attempts synchronously to receive a value, but
/// there may not be a value in the hatch.
///
/// ## Options (all false by default)
///
/// * `close_on_receive` - when true, closes when we successfully receive a message.
/// * `mark_on_drop` - when true, cleanup marks the atomic with a sentinel value for an external
///   process (not provided!) to reclaim it.
///
/// Notes:
/// * Setting `close_on_receive` on this object will only affect this object.
/// * `mark_on_drop` must be set on the `Receiver`.
///
/// ## Async support (feature `async`, default enabled)
///
/// This struct is a `Future` which polls ready when either the
/// receive was successful or the `Sender` has closed.
#[derive(Debug)]
pub struct Receiving<'a, 'b, T> {
    receiver: Option<&'b mut Receiver<'a, T>>,
    flags: Flags,
}

impl<'a, 'b, T> Receiving<'a, 'b, T> {
    /// Gets the value of the `close_on_receive` option. the [`Receiving`] docs for an explanation.
    pub fn get_close_on_receive(&self) -> bool { any_flag(self.flags, CLOSE_ON_SUCCESS) }

    /// Sets the `close_on_receive` option. See the [`Receiving`] docs for an explanation.
    pub fn close_on_receive(mut self, on: bool) -> Self {
        self.flags = toggle_flag(self.flags, CLOSE_ON_SUCCESS, on);
        self
    }
}

#[cfg(not(feature="async"))]
impl<'a, 'b, T> Receiving<'a, 'b, T> {
    /// Attempts to receive a message synchronously.
    pub fn now(mut self) -> Result<Option<T>, Closed> {
        // First, we check for life.
        if no_flag(self.flags, LONELY) {
            if let Some(receiver) = self.receiver.take() {
                if let Some(hatch) = receiver.hatch {
                    // Now we must take a lock to access the value
                    let flags = hatch.lock();
                    let shared = unsafe { &mut *hatch.inner.get() };
                    let value = shared.value.take();
                    if any_flag(flags, S_CLOSE) {
                        // As the sender has closed, there's no need to release the lock. We can
                        // just mark the Receiver as lonely.
                        receiver.flags |= LONELY;
                        return value.map(Some).ok_or(Closed);
                    } else {
                        if value.is_some() { 
                            // Dropping does not require taking a lock, so we can't just do a
                            // `store` as in the async case. We compose a mask to xor with it that
                            // unlocks and may also close.
                            let mask = LOCK | r_closes(self.flags);
                            let flags = hatch.flags.fetch_xor(mask, orderings::MODIFY);
                            // And finally we check the sender didn't close in between.
                            if any_flag(flags, S_CLOSE) { receiver.flags |= LONELY; }
                        } else {
                            // Similar to before, but simpler as we don't close
                            let flags = hatch.flags.fetch_and(!LOCK, orderings::MODIFY);
                            // And check the sender didn't close again.
                            if any_flag(flags, S_CLOSE) {
                                receiver.flags |= LONELY;
                                return value.map(Some).ok_or(Closed);
                            }
                        }
                        return Ok(value);
                    }                    
                }
            }
        }
        Err(Closed)
    }
}

#[cfg(feature="async")]
impl<'a, 'b, T> Receiving<'a, 'b, T> {
    /// Attempts to receive a message synchronously.
    ///
    /// If we succeed and the Sender is waiting, wakes them - they may be waiting for there to be
    /// capacity in the channel.
    pub fn now(mut self) -> Result<Option<T>, Closed> {
        // Check for signs of life
        if no_flag(self.flags, LONELY) {
            if let Some(receiver) = self.receiver.take() {
                if let Some(hatch) = receiver.hatch {
                    // Take a lock so we can access the innards
                    let flags = hatch.lock();
                    // We're going to take these whether we locked or closed.
                    let shared = unsafe { &mut *hatch.inner.get() };
                    let value = shared.value.take();
                    if any_flag(flags, S_CLOSE) {
                        // No need to release the lock or wake, just mark the receiver lonely.
                        receiver.flags |= LONELY;
                        return value.map(Some).ok_or(Closed);
                    } else {
                        // Release the lock and wake
                        let sender =  shared.sender.take();
                        hatch.flags.store(flags | r_closes(self.flags), orderings::STORE);
                        return Ok(value.map(|val| {
                            // Wake the sender if they are waiting - they might want to know there
                            // is capacity to send.
                            if let Some(waker) = sender { waker.wake(); }
                            if any_flag(self.flags, CLOSE_ON_SUCCESS) { receiver.hatch.take(); }
                            val
                        }))
                     }
                 }
             }
        }
        Err(Closed)
    }

    // pin-project-lite does not let us define a destructor.
    // https://github.com/taiki-e/pin-project-lite/issues/62#issuecomment-884188885
    #[inline(always)]
    unsafe fn project(self: Pin<&mut Self>) -> &mut Self {
        Pin::get_unchecked_mut(self)
    }
}

impl<'a, 'b, T> Drop for Receiving<'a, 'b, T> {
    fn drop(&mut self) {
        // we only need a Drop impl on async, but we want to keep it the same across the two
        #[cfg(feature="async")]
        if let Some(receiver) = self.receiver.take() {
            // If we are not waiting, we don't need to do anythning.
            if any_flag(self.flags, WAITING) {
                if let Some(hatch) = receiver.hatch {
                    // We'll need a lock as usual
                    let flags = hatch.lock();
                    if any_flag(flags, S_CLOSE) {
                        // No need to release the lock
                        receiver.flags |= LONELY;
                    } else {
                        // Our cleanup is to remove the waker. Also release the lock.
                        let shared = unsafe { &mut *hatch.inner.get() };
                        let _delay_drop = shared.receiver.take();
                        hatch.flags.store(flags, orderings::STORE);
                    }
                }
            }
        }
    }
}
#[cfg(feature="async")]
impl<'a, 'b, T> Future for Receiving<'a, 'b, T> {
    type Output = Result<T, Closed>;
    fn poll(self: Pin<&mut Self>, ctx: &mut Context) -> Poll<Self::Output> {
        // Safe because we do not move out of self.
        let this = unsafe { self.project() };
        // Check for signs of life
        if no_flag(this.flags, LONELY) {
            if let Some(receiver) = this.receiver.as_mut() {
                if let Some(hatch) = receiver.hatch {
                    // We need to take a lock to have access to the innards
                    let flags = hatch.lock();
                    let shared = unsafe { &mut *hatch.inner.get() };
                    let value = shared.value.take();
                    if any_flag(flags, S_CLOSE) {
                        // No need to release the lock or wake
                        receiver.flags |= LONELY; // Stop trying to do things.
                        this.receiver.take();     // Disable the destructor.
                        return Poll::Ready(value.ok_or(Closed));
                    }
                    // Okay, we're good to go.
                    let sender = shared.sender.take();
                    if let Some(v) = value {
                        // We don't need to wait anymore
                        let _waker = shared.receiver.take();
                        // We have exclusive access because even dropping takes a lock in async mode.
                        // We branchlessly add the close flag if we are set to close on success.
                        hatch.flags.store(flags | r_closes(this.flags), orderings::STORE);
                        if any_flag(this.flags, CLOSE_ON_SUCCESS) {
                            receiver.hatch.take(); // we just closed, tidy up
                        }
                        this.receiver.take(); // disable destructor
                        if let Some(waker) = sender { waker.wake(); } // wake if necessary
                        return Poll::Ready(Ok(v))
                    } else {
                        // We need to wait and unlock.
                        let _waker = shared.receiver.replace(ctx.waker().clone());
                        // We have exclusive access because even dropping takes a lock in async mode.
                        // We branchlessly add the close flag if we are set to close on success.
                        hatch.flags.store(flags, orderings::STORE);
                        this.flags |= WAITING;
                        // We have to wake the sender
                        if let Some(waker) = sender { waker.wake(); }
                        return Poll::Pending;
                    };
                }
            }
        }
        Poll::Ready(Err(Closed))
    }
}
