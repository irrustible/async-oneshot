use crate::*;
#[cfg(feature="async")]
use core::{future::Future, pin::Pin, task::{Context, Poll}};

/// A unique handle to receive messages through a [`crate::Hatch`].
#[derive(Debug)]
pub struct Receiver<'a, T> {
    hatch: Option<Holder<'a, Hatch<T>>>,
    flags: Flags,
}

/// A `Receiver<T>` is Send if T is Send.
unsafe impl<'a, T: Send> Send for Receiver<'a, T> {}

/// A Receiver is always Sync - it requires a mut ref for mutation.
unsafe impl<'a, T> Sync for Receiver<'a, T> {}

impl<'a, T> Receiver<'a, T> {
    // Creates a new Receiver.
    //
    // You must ensure the following:
    // * No other (live) Receiver exists
    // * The hatch has been reset if it was already used.
    #[inline(always)]
    pub(crate) unsafe fn new(hatch: Holder<'a, Hatch<T>>) -> Self {
        Self { hatch: Some(hatch), flags: DEFAULT }
    }

    #[cfg(feature="async")]
    /// Returns a future which attempts to receive a value through the hatch.
    ///
    /// Optional flags (default false):
    ///
    /// * `close_on_success`: If we should close the channel as part of the next
    ///    receive operation. When true, allows to elide a lock cycle
    ///    in the destructor.
    /// * `mark_on_drop`: If we are last to close, should we mark the channel
    ///    as closed for our memory manager? Cost: atomic store
    pub fn receive<'b>(&'b mut self) -> Receiving<'a, 'b, T> {
        Receiving { receiver: Some(self), flags: DEFAULT }
    }

    /// Returns a new [`Sender`] when the old one has closed.
    pub fn recover(&mut self) -> Result<sender::Sender<'a, T>, RecoverError> {
        if let Some(h) = self.hatch {
            if any_flag(self.flags, LONELY) {
                // We have observed Sender close.
                self.flags &= !LONELY; // reset flag
                unsafe { h.recycle() }; // a release store
                Ok(unsafe { sender::Sender::new(h) })
            } else {
                // TODO: attempt with atomic
                Err(RecoverError::Live)
            }
        } else {
            Err(RecoverError::Closed)
        }
    }

    /// Returns a new [`Sender`] when the old one has closed.
    pub unsafe fn recover_unchedked(&mut self) -> Result<sender::Sender<'a, T>, RecoverError> {
        if let Some(h) = self.hatch {
            if any_flag(self.flags, LONELY) {
                // We have observed Sender close.
                self.flags &= !LONELY; // reset flag
                h.recycle(); // a release store
                Ok(sender::Sender::new(h))
            } else {
                // TODO: attempt with atomic
                Err(RecoverError::Live)
            }
        } else {
            Err(RecoverError::Closed)
        }
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
            // If we are LONELY, we have exclusive access
            if any_flag(self.flags, LONELY) {
                // we already know they dropped because the flag is set
                return unsafe { hatch.cleanup(any_flag(self.flags, MARK_ON_DROP)); }
            }
            // Without async, we just set the flag and check we won.
            #[cfg(not(feature="async"))]
            {
                let flags = hatch.flags.fetch_or(R_CLOSE, orderings::MODIFY);
                if any_flag(flags, S_CLOSE) {
                    unsafe { hatch.cleanup(any_flag(self.flags, MARK_ON_DROP)); }
                }
            }
            // For async, we have to lock and maybe wake the sender
            #[cfg(feature="async")] {
                let flags = hatch.lock();
                if any_flag(flags, S_CLOSE) {
                    return unsafe { hatch.cleanup(any_flag(self.flags, MARK_ON_DROP)); }
                } else {
                    let shared = unsafe { &mut *hatch.inner.get() };
                    let _recv = shared.receiver.take();
                    let sender = shared.sender.take();
                    // simultaneously mark us closed and release the lock.
                    hatch.flags.fetch_xor(R_CLOSE | LOCK, orderings::MODIFY);
                    // Finally, wake the receiver if they are waiting.
                    if let Some(waker) = sender {
                        waker.wake();
                    }
                    return
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
    /// Gets the value of the `mark_on_drop` option. See the [`receive`] docs for an explanation.
    pub fn get_mark_on_drop(&self) -> bool { any_flag(self.flags, MARK_ON_DROP) }

    /// Sets the `mark_on_drop` option. See the [`Receiver`] docs for an explanation.
    pub fn mark_on_drop(mut self, on: bool) -> Self {
        self.flags = toggle_flag(self.flags, MARK_ON_DROP, on);
        self
    }

    /// Gets the value of the `close_on_receive` option. the [`Receiver`] docs for an explanation.
    pub fn get_close_on_receive(&self) -> bool { any_flag(self.flags, CLOSE_ON_SUCCESS) }

    /// Sets the `close_on_receive` flag. See the [`Receiving`] docs for an explanation.
    pub fn close_on_receive(mut self, on: bool) -> Self {
        self.flags = toggle_flag(self.flags, CLOSE_ON_SUCCESS, on);
        self
    }
}

#[cfg(not(feature="async"))]
impl<'a, 'b, T> Receiving<'a, 'b, T> {
    // This one's a bit awkward because doing a close doesn't require taking
    // a lock, so the other side can close even while we have the lock.
    pub fn now(mut self) -> Result<Option<T>, Closed> {
        if let Some(receiver) = self.receiver.as_mut() {
            if !any_flag(receiver.flags, LONELY) {
                if let Some(hatch) = receiver.hatch {
                    let flags = hatch.lock();
                    let shared = unsafe { &mut *hatch.inner.get() };
                    let ret = shared.value.take();
                    if any_flag(flags, S_CLOSE) {
                        // no need to release the lock
                        receiver.flags |= LONELY;
                        self.receiver.take();
                    } else {
                        
                        let flags = hatch.flags.fetch_xor(flags, orderings::MODIFY);
                        if any_flag(flags, S_CLOSE) {
                            receiver.flags |= LONELY;
                            self.receiver.take()
                        }
                        return match ret {
                            Some(val) => Ok(Some(val)),
                            None => Err(Closed),
                        }
                    }
                }
            }
        }
        Err(Closed)
    }
}

#[cfg(feature="async")]
impl<'a, 'b, T> Receiving<'a, 'b, T> {
    /// Attempts to receive a message synchronously. Wakes the Sender
    /// on success if they are waiting.
    pub fn now(mut self) -> Result<Option<T>, Closed> {
        if let Some(receiver) = self.receiver.take() {
            if let Some(hatch) = receiver.hatch {
                let flags = hatch.lock();
                let shared = unsafe { &mut *hatch.inner.get() };
                let ret = shared.value.take();
                if any_flag(flags, S_CLOSE) {
                    // No need to release the lock or wake, just mark us lonely.
                    receiver.flags |= LONELY;
                    return match ret {
                        Some(val) => {
                            if any_flag(self.flags, CLOSE_ON_SUCCESS) { receiver.hatch.take(); }
                            Ok(Some(val))
                        }
                        None => Err(Closed),
                    }
                } else {
                    // Release the lock and wake
                    let sender =  shared.sender.take();
                    hatch.flags.store(flags | r_closes(self.flags), orderings::STORE);
                    return Ok(ret.map(|val| {
                        if let Some(waker) = sender { waker.wake(); }
                        if any_flag(self.flags, CLOSE_ON_SUCCESS) { receiver.hatch.take(); }
                        val
                    }))
                }
            }
        }
        Err(Closed)
    }

    // pin-project-lite does not let us define a destructor.
    fn project(self: Pin<&mut Self>) -> &mut Self {
        // safe because this method is private and we're going to be
        // careful not to move it.
        unsafe { Pin::get_unchecked_mut(self) }
    }
}

#[cfg(feature="async")]
impl<'a, 'b, T> Future for Receiving<'a, 'b, T> {
    type Output = Result<T, Closed>;
    fn poll(self: Pin<&mut Self>, ctx: &mut Context) -> Poll<Self::Output> {
        let this = self.project();
        if let Some(receiver) = this.receiver.as_mut() {
            if let Some(hatch) = receiver.hatch {
                let flags = hatch.lock();
                let shared = unsafe { &mut *hatch.inner.get() };
                let value = shared.value.take();
                if any_flag(flags, S_CLOSE) {
                    receiver.flags |= LONELY;
                    this.receiver.take();
                    return Poll::Ready(value.ok_or(Closed));
                } else {
                    let sender = shared.sender.take();
                    let _recv = shared.receiver.replace(ctx.waker().clone());
                    hatch.flags.store(flags | r_closes(this.flags), orderings::STORE);
                    if let Some(waker) = sender {
                        waker.wake();
                    }
                    return match value {
                        Some(v) => {
                            if any_flag(this.flags, CLOSE_ON_SUCCESS) {
                                receiver.hatch.take();
                                this.receiver.take();
                            }
                            Poll::Ready(Ok(v))
                        }
                        _ => Poll::Pending,
                    }
                }
            }
        }
        // all fall throughs end up here
        Poll::Ready(Err(Closed))
    }
}
