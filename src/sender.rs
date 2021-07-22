use crate::*;
#[cfg(feature="async")]
use core::{future::Future, pin::Pin, task::{Context, Poll}};
use core::ops::Deref;

#[derive(Debug,Eq,PartialEq)]
/// The reason we couldn't send a message. And our value back.
pub enum SendError<T> {
    Closed(T),
    Existing(T),
}

impl<T> SendError<T> {
    /// Get the value that failed to send back.
    pub fn into_inner(self) -> T {
        match self {
            SendError::Closed(t) => t,
            SendError::Existing(t) => t,
        }
    }
}
impl<T> Deref for SendError<T> {
    type Target = T;
    fn deref(&self) -> &T {
        match self {
            SendError::Closed(t) => t,
            SendError::Existing(t) => t,
        }
    }
}

/// A unique means of sending a message on a Hatch.
///
/// ## Options (all false by default)
///
/// * `overwrite` - when true, overwrites any existing value.
/// * `close_on_send` - when true, closes during the next send
///   operation that successfully delivers a message.
/// * `mark_on_drop` - when true, marks the atomic with a sentinel value
///   for an external process (not provided!) to reclaim it.
///
/// The first two of these options may also be set directly on a
/// [`Sending`] object, in which case they only apply to that operation.
#[derive(Debug)]
pub struct Sender<'a, T> {
    hatch: Option<Holder<'a, Hatch<T>>>,
    flags: Flags,
}

impl<'a, T> Sender<'a, T> {
    /// Creates a new Sender.
    ///
    /// # Safety
    ///
    /// To avoid logic errors, you must ensure no other (live) Sender exists and the hatch is not in
    /// a dirty state (i.e. is new or has been reclaimed).
    ///
    /// There is a possible use after free during drop if the Hatch is managed by us (is a
    /// SharedBoxPtr) and you allow a second live Sender to exist.
    #[inline(always)]
    pub(crate) unsafe fn new(hatch: Holder<'a, Hatch<T>>) -> Self {
        Sender { hatch: Some(hatch), flags: DEFAULT }
    }

    /// Returns a disposable object for a single send operation.
    #[inline(always)]
    #[must_use = "Sending does nothing unless you call `.now` or poll it as a Future."]
    pub fn send<'b>(&'b mut self, value: T) -> Sending<'a, 'b, T> {
        let flags = self.flags;
        Sending { sender: Some(self), value: Some(value), flags }
    }

    #[cfg(feature="async")]
    #[must_use = "Wait does nothing unless you oll it as a Future."]
    /// Creates a 
    pub fn wait<'b>(&'b mut self) -> Wait<'a, 'b, T> {
        Wait { sender: Some(self) }
    }

    /// Returns a new Receiver after the old one has closed.
    pub fn recover(&mut self) -> Result<receiver::Receiver<'a, T>, RecoverError> {
        self.hatch.ok_or(RecoverError::Closed).and_then(|_| {
            if any_flag(self.flags, LONELY) {
                // We have observed Sender close so we have exclusive access
                Ok(unsafe { self.recover_unchecked().unwrap() })
            } else {
                // TODO: attempt with atomic
                Err(RecoverError::Live)
            }
        })
    }

    /// Returns a new [`Sender`] without checking the old one has closed.
    ///
    /// ## Safety
    ///
    /// To avoid logic errors, you must ensure no other (live) Receiver exists and the hatch is not
    /// in a dirty state (i.e. is new or hasn been reclaimed).
    ///
    /// There is a possible use after free during drop if the Hatch is managed by us (is a
    /// SharedBoxPtr) and you allow a second live Receiver to exist).
    pub unsafe fn recover_unchecked(&mut self) -> Result<receiver::Receiver<'a, T>, Closed> {
        self.hatch.map(|h| {
            h.recycle(); // A release store
            self.flags &= !LONELY; // reset flag
            receiver::Receiver::new(h)
        }).ok_or(Closed)
    }

    /// Gets the value of the `overwrite` option. See the [`Sender`] docs for an explanation.
    #[inline(always)]
    pub fn get_overwrite(&self) -> bool { any_flag(self.flags, OVERWRITE) }

    /// Sets the `overwrite` option. See the [`Sender`] docs for an explanation.
    #[inline(always)]
    pub fn overwrite(mut self, on: bool) -> Self {
        self.flags = toggle_flag(self.flags, OVERWRITE, on);
        self
    }

    /// Gets the value of the `close_on_send` option. See the [`Sender`] docs for an explanation.
    #[inline(always)]
    pub fn get_close_on_send(&self) -> bool { any_flag(self.flags, CLOSE_ON_SUCCESS) }

    /// Sets the `close_on_send` option. See the [`Sender`] docs for an explanation.
    #[inline(always)]
    pub fn close_on_send(mut self, on: bool) -> Self {
        self.flags = toggle_flag(self.flags, CLOSE_ON_SUCCESS, on);
        self
    }

    /// Gets the value of the `mark_on_drop` option. See the [`Sender`] docs for an explanation.
    #[inline(always)]
    pub fn get_mark_on_drop(&self) -> bool { any_flag(self.flags, MARK_ON_DROP) }

    /// Sets the `mark_on_drop` option. See the [`Sender`] docs for an explanation.
    #[inline(always)]
    pub fn mark_on_drop(mut self, on: bool) -> Self {
        self.flags = toggle_flag(self.flags, MARK_ON_DROP, on);
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
    /// To avoid logic errors, you must ensure no other (live) Sender exists and the hatch is not in
    /// a dirty state (i.e. is new or has been reclaimed).
    ///
    /// There is a possible use after free during drop if the Hatch is managed by us (is a
    /// SharedBoxPtr) and you allow a second live Sender to exist.
    pub unsafe fn leak(mut sender: Sender<T>) { sender.hatch.take(); }
}

impl<'a, T> Drop for Sender<'a, T> {
    fn drop(&mut self) {
        if let Some(hatch) = self.hatch.take() {
            if any_flag(self.flags, LONELY) {
                // If we are LONELY, we have exclusive access
                return unsafe { hatch.cleanup(any_flag(self.flags, MARK_ON_DROP)); }
            }
            #[cfg(not(feature="async"))] {
                // Without async, we just set the flag and check we won.
                let flags = hatch.flags.fetch_or(S_CLOSE, orderings::MODIFY);
                if any_flag(flags, R_CLOSE) {
                    // Safe because we have seen the receiver closed
                    unsafe { hatch.cleanup(any_flag(self.flags, MARK_ON_DROP)); }
                } 
            }
            #[cfg(feature="async")] {
                // For async, we have to lock and maybe wake the receiver
                let flags = hatch.lock();
                if any_flag(flags, R_CLOSE) {
                    // Safe because we have seen the receiver closed
                    unsafe { hatch.cleanup(any_flag(self.flags, MARK_ON_DROP)) };
                } else {
                    let shared = unsafe { &mut *hatch.inner.get() };
                    let _send = shared.sender.take();  // we won't be waiting any more
                    let recv = shared.receiver.take(); // we might need to wake them
                    // simultaneously mark us closed and release the lock.
                    hatch.flags.store(flags | S_CLOSE, orderings::STORE);
                    // Finally, wake the receiver if they are waiting.
                    if let Some(waker) = recv { waker.wake(); }
                }
            }
        }
    }
}

/// This is a disposable object which performs a single send operation.
///
/// The `now` method attempts to synchronously send a value, but
/// there may not be space for a message right now.
///
/// ## Options (all false by default)
///
/// * `overwrite` - when true, overwrites any existing value
/// * `close_on_send` - when true, closes during the next send
///   operation that successfully delivers a message.
/// * `mark_on_drop` - when true, marks the atomic with a sentinel value
///   for an external process (not provided!) to reclaim it.
///
/// The options are copied at construction time from the
/// [`Sender`]. You may locally modify the first two on this object,
/// in which case they only apply to this operation.
///
/// ## Async support (feature `async`, default enabled)
///
/// This struct is a `Future` which polls ready when either the
/// receive was successful or the `Sender` has closed. Note that the
/// `Future` impl ignores the `overwrite` flag (`now` does not).
#[derive(Debug)]
pub struct Sending<'a, 'b, T> {
    sender: Option<&'b mut Sender<'a, T>>,
    value: Option<T>,
    flags: Flags,
}

impl<'a, 'b, T> Sending<'a, 'b, T> {
    /// Gets the value of the `overwrite` option. See the [`Sending`] docs for an explanation.
    #[inline(always)]
    pub fn get_overwrite(&self) -> bool { any_flag(self.flags, OVERWRITE) }

    /// Sets the `overwrite` option. See the [`Sending`] docs for an explanation.
    #[inline(always)]
    pub fn overwrite(mut self, on: bool) -> Self {
        self.flags = toggle_flag(self.flags, OVERWRITE, on);
        self
    }

    /// Gets the value of the `close_on_send` option. See the [`Sending`] docs for an explanation.
    #[inline(always)]
    pub fn get_close_on_send(&self) -> bool { any_flag(self.flags, CLOSE_ON_SUCCESS) }

    /// Sets the `close_on_send` option. See the [`Sending`] docs for an explanation.
    #[inline(always)]
    pub fn close_on_send(mut self, on: bool) -> Self {
        self.flags = toggle_flag(self.flags, CLOSE_ON_SUCCESS, on);
        self
    }
}
#[cfg(not(feature="async"))]
impl<'a, 'b, T> Sending<'a, 'b, T> {
    
    /// Attempts to send a message synchronously.
    ///
    /// Returns the existing message (if there was one and overwrite is enabled).
    ///
    /// Fails if the Receiver has closed or there was an exiisting
    /// message and overwrite is not enabled.
    pub fn now(mut self) -> Result<Option<T>, SendError<T>> {
        // We need this either way
        let value = self.value.take().unwrap();
        // Check for life
        if no_flag(self.flags, LONELY) {
            let sender = self.sender.take().unwrap();
            if let Some(hatch) = sender.hatch.as_ref() {
                // Take a lock so we can store a value.
                let flags = hatch.lock();
                if any_flag(flags, R_CLOSE) {
                    // No need to release the lock, just set us lonely.
                    sender.flags |= LONELY;
                    return Err(SendError::Closed(value));
                }
                if any_flag(self.flags, OVERWRITE) || shared.value.is_none() {
                    let shared = unsafe { &mut *hatch.inner.get() };
                    let value = shared.value.replace(value);
                    // Dropping does not require taking a lock, so we can't just do a `store` as in
                    // the async case. We compose a mask to xor with it that unlocks and may also
                    // close.
                    let mask = LOCK | s_closes(self.flags);
                    let flags = hatch.flags.fetch_xor(mask, orderings::MODIFY);
                    // All that was because they might close on us.
                    if any_flag(flags, R_CLOSE) {
                        sender.flags |= LONELY;
                        return Err(SendError::Closed(value));
                    }
                    if any_flag(self.flags, CLOSE_ON_SUCCESS) { sender.hatch.take(); }
                    return Ok(value)
                }
                hatch.flags.store(flags, orderings::STORE);
                Err(SendError::Existing(value))
            }
        }
        Err(SendError::Closed(value))
    }
}

#[cfg(feature="async")]
impl<'a, 'b, T> Sending<'a, 'b, T> {

    /// Attempts to send a message synchronously.
    ///
    /// Returns the existing message (if there was one and overwrite is enabled).
    ///
    /// Fails if the Receiver has closed or there was an existing
    /// message and overwrite is not enabled.
    pub fn now(mut self) -> Result<Option<T>, SendError<T>> {
        // We need these either way
        let value = self.value.take().unwrap();
        let sender = self.sender.take().unwrap();
        // Check for life
        if no_flag(sender.flags, LONELY) {
            if let Some(hatch) = sender.hatch.as_ref() {
                let flags = hatch.lock();
                if any_flag(flags, R_CLOSE) {
                    sender.flags |= LONELY;
                    return Err(SendError::Closed(value))
                }
                let shared = unsafe { &mut *hatch.inner.get() };
                if any_flag(self.flags, OVERWRITE) || shared.value.is_none() {
                    let value = shared.value.replace(value);
                    let receiver = shared.receiver.take();
                    // Release the lock, setting the close flag if we close on success
                    let flags = flags | s_closes(self.flags);
                    hatch.flags.store(flags, orderings::STORE);
                    // If we just closed, we need to clean up
                    if any_flag(self.flags, CLOSE_ON_SUCCESS) { sender.hatch.take(); }
                    // If the receiver is waiting, wake them.
                    if let Some(waker) = receiver { waker.wake(); }
                    return Ok(value)
                }
                hatch.flags.store(flags, orderings::STORE);
                return Err(SendError::Existing(value))
            }
        }
        Err(SendError::Closed(value))
    }

    // pin-project-lite does not let us define a Drop impl
    // https://github.com/taiki-e/pin-project-lite/issues/62#issuecomment-884188885
    fn project(self: Pin<&mut Self>) -> &mut Self {
        unsafe { Pin::get_unchecked_mut(self) }
    }
}

#[cfg(feature="async")]
impl<'a, 'b, T> Future for Sending<'a, 'b, T> {
    type Output = Result<(), SendError<T>>;
    fn poll(self: Pin<&mut Self>, ctx: &mut Context) -> Poll<Self::Output> {
        let this = self.project();
        let value = this.value.take().unwrap();
        if let Some(sender) = this.sender.as_mut() {
            if no_flag(this.flags, LONELY) {
                if let Some(hatch) = sender.hatch.as_ref() {
                    // Take a lock so we can store a value.
                    let flags = hatch.lock();
                    if any_flag(flags, R_CLOSE) {
                        // No need to release the lock, just set us lonely.
                        sender.flags |= LONELY;
                        return Poll::Ready(Err(SendError::Closed(value)));
                    }
                    let shared = unsafe { &mut *hatch.inner.get() };
                    if shared.value.is_none() {
                        shared.value = Some(value);
                        let receiver = shared.receiver.take();
                        // release the lock and maybe close
                        let flags = flags | s_closes(this.flags);
                        hatch.flags.store(flags, orderings::STORE);
                        // If we just closed, we'd better clean up
                        if any_flag(this.flags, CLOSE_ON_SUCCESS) { sender.hatch.take(); }
                        // And since we delivered, we should wake the receiver.
                        if let Some(waker) = receiver { waker.wake(); }
                        return Poll::Ready(Ok(()))
                    } else {
                        // set a waker and release the lock
                        let _waker = shared.sender.replace(ctx.waker().clone());
                        hatch.flags.store(flags, orderings::STORE);
                        // put the value back for the next poll
                        this.value.replace(value);
                        return Poll::Pending
                    }
                }
            }
        }
        Poll::Ready(Err(SendError::Closed(value)))
    }
}

/// A `Future` that waits for there to be capacity to send and a
/// `Receiver` listening. This allows computing an expensive value on
/// demand (lazy send).
#[cfg(feature="async")]
pub struct Wait<'a, 'b, T> {
    sender: Option<&'b mut Sender<'a, T>>,
}

#[cfg(feature="async")]
impl<'a, 'b, T> Wait<'a, 'b, T> {
    // pin-project-lite does not let us define a Drop impl
    // https://github.com/taiki-e/pin-project-lite/issues/62#issuecomment-884188885
    fn project(self: Pin<&mut Self>) -> &mut Self {
        unsafe { Pin::get_unchecked_mut(self) }
    }
}

#[cfg(feature="async")]
impl<'a, 'b, T> Drop for Wait<'a, 'b, T> {
    fn drop(&mut self) {
        if let Some(s) = self.sender.take() {
            todo!()
        }
    }
}

#[cfg(feature="async")]
impl<'a, 'b, T> Future for Wait<'a, 'b, T> {
    type Output = Result<(), Closed>;
    fn poll(self: Pin<&mut Self>, ctx: &mut Context) -> Poll<Self::Output> {
        let this = self.project(); 
        todo!()
    }
}

// #[cfg(feature="async")]
// impl<T> Future for Wait<T> {
//     /// Waits for a Receiver to be waiting for us to send something
//     /// (i.e. allows you to produce a value to send on demand).
//     /// Fails if the Receiver is dropped.
//     #[inline(always)]
//     pub fn wait(&mut self) -> impl Future<Output = Result<(), Closed>> + '_ {
//         poll_fn(move |ctx| {
//             // If the Sender is done, we shouldn't have been polled at all. Naughty user.
//             if self.done { return Poll::Ready(Err(Closed)); }
//             let mut state = unsafe { self.chan().set(Locked) };
//             poll_dropped!(state, self.hatch, self.done);
//             while Locked.any(state) {
//                 spin_loop();
//                 state = unsafe { self.chan().set(Locked) };
//                 poll_dropped!(state, self.hatch, self.done);
//             }
//             if ReceiverWaiting.any(state) {
//                 // If a Receiver is waiting, we don't need to
//                 // wait. Pull our waker if we have one and be sure
//                 // we're marked as not waiting.
//                 let _pulled = unsafe { self.chan().sender_waker() };
//                 state = unsafe { self.chan().unset(Locked | SenderWaiting) };
//                 poll_dropped!(state, self.hatch, self.done);
//                 return Poll::Ready(Ok(()));
//             }
//             // There isn't a Receiver waiting, so we'll have to wait.
//             let _old = unsafe { self.chan().set_sender_waker(ctx.waker().clone()) };
//             if SenderWaiting.any(state) {
//                 state = unsafe { self.chan().unset(Locked) };
//             } else {
//                 state = unsafe { self.chan().flip(Locked | SenderWaiting) };
//             }
//             poll_dropped!(state, self.hatch, self.done);
//             Poll::Pending
//         })
//     }
// }
