use crate::*;
#[cfg(feature="async")]
use core::{future::Future, pin::Pin, task::{Context, Poll}};
use core::ops::Deref;

#[derive(Debug,Eq,PartialEq)]
pub enum SendError<T> {
    Closed(T),
    Existing(T),
}

impl<T> SendError<T> {
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

/// A unique means of sending a message on a hatch.
///
/// ## Options (all false by default)
///
/// * `overwrite` - when true, overwrites any existing value
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
    /// You must ensure that:
    /// * No other (live) Sender exists
    /// * The Hatch is either fresh or recycled.
    #[inline(always)]
    pub(crate) unsafe fn new(hatch: Holder<'a, Hatch<T>>) -> Self {
        Sender { hatch: Some(hatch), flags: DEFAULT }
    }

    /// Returns a disposable object for a single send operation.
    #[inline(always)]
    pub fn send<'b>(&'b mut self, value: T) -> Sending<'a, 'b, T> {
        let flags = self.flags;
        Sending { sender: Some(self), value: Some(value), flags }
    }

    #[cfg(feature="async")]
    /// Creates a 
    pub fn wait<'b>(&'b mut self) -> Wait<'a, 'b, T> {
        Wait { sender: Some(self) }
    }

    /// Returns a new Receiver after the old one has closed.
    pub fn recover(&mut self) -> Result<receiver::Receiver<'a, T>, RecoverError> {
        if let Some(h) = self.hatch {
            // This flag communicates that we have exclusive access to
            // do the recycling. So we're down to a simple atomic store.
            if any_flag(self.flags, LONELY) {
                unsafe { h.recycle() };
                self.flags &= !LONELY; // reset flag
                Ok(unsafe { receiver::Receiver::new(h) })
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
    /// * It could cause the Receiver to wait forever.
    pub unsafe fn leak(mut sender: Sender<T>) { sender.hatch.take(); }

    /// Gets the value of the `overwrite` flag. See the [`Sender`] docs
    /// for an explanation.
    #[inline(always)]
    pub fn get_overwrite(&self) -> bool { any_flag(self.flags, OVERWRITE) }

    /// Sets the `overwrite` flag. See the [`Sender`] docs for an explanation.
    #[inline(always)]
    pub fn overwrite(mut self, on: bool) -> Self {
        self.flags = toggle_flag(self.flags, OVERWRITE, on);
        self
    }

    /// Gets the value of the `close_on_send` flag. See the
    /// [`Sending`] docs for an explanation.
    #[inline(always)]
    pub fn get_close_on_send(&self) -> bool { any_flag(self.flags, CLOSE_ON_SUCCESS) }

    /// Sets the `close_on_send` flag. See the [`Sender`] docs for an explanation.
    #[inline(always)]
    pub fn close_on_send(mut self, on: bool) -> Self {
        self.flags = toggle_flag(self.flags, CLOSE_ON_SUCCESS, on);
        self
    }

    /// Gets the value of the `mark_on_drop` flag. See the
    /// [`Sender`] docs for an explanation.
    #[inline(always)]
    pub fn get_mark_on_drop(&self) -> bool { any_flag(self.flags, MARK_ON_DROP) }

    /// Sets the `mark_on_drop` flag. See the [`Sender`] docs for an explanation.
    #[inline(always)]
    pub fn mark_on_drop(mut self, on: bool) -> Self {
        self.flags = toggle_flag(self.flags, MARK_ON_DROP, on);
        self
    }
}

impl<'a, T> Drop for Sender<'a, T> {
    fn drop(&mut self) {
        // If we don't have a hatch, there's nothing to do.
        if let Some(hatch) = self.hatch.take() {
            // If we are marked LONELY, we have exclusive access.
            if any_flag(self.flags, LONELY) {
                return unsafe { hatch.cleanup(any_flag(self.flags, MARK_ON_DROP)); }
            }
            // Without async, we just set the flag and check we won.
            #[cfg(not(feature="async"))] {
                let flags = hatch.flags.fetch_or(S_CLOSE, orderings::MODIFY);
                if any_flag(flags, R_CLOSE) {
                    unsafe { hatch.cleanup(any_flag(self.flags, MARK_ON_DROP)); }
                } 
            }
            // For async, we have to lock and maybe wake the receiver
            #[cfg(feature="async")] {
                let flags = hatch.lock();
                if any_flag(flags, R_CLOSE) {
                    unsafe { hatch.cleanup(any_flag(self.flags, MARK_ON_DROP)) };
                } else {
                    let shared = unsafe { &mut *hatch.inner.get() };
                    let _send = shared.sender.take();
                    let recv = shared.receiver.take();
                    // simultaneously mark us closed and release the lock.
                    hatch.flags.store(flags | S_CLOSE, orderings::STORE);
                    // Finally, wake the receiver if they are waiting.
                    if let Some(waker) = recv {
                        waker.wake();
                    }
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
    /// Gets the value of the `overwrite` flag. See the [`Sending`] docs
    /// for an explanation.
    #[inline(always)]
    pub fn get_overwrite(&self) -> bool { any_flag(self.flags, OVERWRITE) }

    /// Sets the `OVERWRITE` flag. See the [`Sending`] docs for an explanation.
    #[inline(always)]
    pub fn overwrite(mut self, on: bool) -> Self {
        self.flags = toggle_flag(self.flags, OVERWRITE, on);
        self
    }

    /// Gets the value of the `CLOSE_ON_SUCCESS` flag. See the
    /// [`Sending`] docs for an explanation.
    #[inline(always)]
    pub fn get_close_on_send(&self) -> bool { any_flag(self.flags, CLOSE_ON_SUCCESS) }

    /// Sets the `CLOSE_ON_SUCCESS` flag. See the [`Sending`] docs for an explanation.
    #[inline(always)]
    pub fn close_on_send(mut self, on: bool) -> Self {
        self.flags = toggle_flag(self.flags, CLOSE_ON_SUCCESS, on);
        self
    }

    // /// Gets the value of the `CLOSE_ON_FAILURE` flag. See the
    // /// [`Sending`] docs for an explanation.
    // #[inline(always)]
    // pub fn get_close_on_failure(&self) -> bool { any_flag(self.flags, CLOSE_ON_SUCCESS) }

    // /// Sets the `CLOSE_ON_FAILURE` flag. See the [`Sending`] docs for an
    // /// explanation.
    // #[inline(always)]
    // pub fn close_on_success(mut self, on: bool) -> Self {
    //     self.flags = toggle_flag(self.flags, CLOSE_ON_SUCCESS, on);
    //     self
    // }

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
        let sender = self.sender.take().unwrap();
        let value = self.value.take().unwrap();
        if let Some(hatch) = sender.hatch.as_ref() {
            let flags = hatch.lock();
            if any_flag(flags, R_CLOSE) {
                sender.flags |= LONELY;
                Err(SendError::Closed(value))
            } else {
                let shared = unsafe { &mut *hatch.inner.get() };
                if any_flag(self.flags, OVERWRITE) || shared.value.is_none() {
                    let value = shared.value.replace(value);
                    let mask = LOCK | s_closes(self.flags);
                    let flags = hatch.flags.fetch_xor(mask, orderings::MODIFY);
                    if any_flag(flags, R_CLOSE) {
                        sender.flags |= LONELY;
                        Err(SendError::Closed(value))
                    } else {
                        if any_flag(self.flags, CLOSE_ON_SUCCESS) { sender.hatch.take(); }
                        Ok(value)
                    }
                } else {
                    hatch.flags.store(flags, orderings::STORE);
                    Err(SendError::Existing(value))
                }
            }
        } else {
            Err(SendError::Closed(value))
        }
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
        let sender = self.sender.take().unwrap();
        let value = self.value.take().unwrap();
        if no_flag(sender.flags, LONELY) {
            if let Some(hatch) = sender.hatch.as_ref() {
                let flags = hatch.lock();
                return if any_flag(flags, R_CLOSE) {
                    sender.flags |= LONELY;
                    Err(SendError::Closed(value))
                } else {
                    let shared = unsafe { &mut *hatch.inner.get() };
                    if any_flag(self.flags, OVERWRITE) || shared.value.is_none() {
                        let value = shared.value.replace(value);
                        let receiver = shared.receiver.take();
                        // this is a branchless way of closing
                        let flags = flags | s_closes(self.flags);
                        hatch.flags.store(flags, orderings::STORE);
                        if any_flag(self.flags, CLOSE_ON_SUCCESS) { sender.hatch.take(); }
                        if let Some(waker) = receiver { waker.wake(); }
                        Ok(value)
                    } else {
                        hatch.flags.store(flags, orderings::STORE);
                        Err(SendError::Existing(value))
                    }
                }
            }
        }
        Err(SendError::Closed(value))
    }

    // pin-project-lite does not let us define a Drop impl
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
                    let flags = hatch.lock();
                    return if any_flag(flags, R_CLOSE) {
                        sender.flags |= LONELY;
                        Poll::Ready(Err(SendError::Closed(value)))
                    } else {
                        let shared = unsafe { &mut *hatch.inner.get() };
                        if shared.value.is_none() {
                            shared.value = Some(value);
                            let receiver = shared.receiver.take();
                            // this is a branchless way of closing
                            let flags = flags | s_closes(this.flags);
                            hatch.flags.store(flags, orderings::STORE);
                            if any_flag(this.flags, CLOSE_ON_SUCCESS) { sender.hatch.take(); }
                            if let Some(waker) = receiver { waker.wake(); }
                            Poll::Ready(Ok(()))
                        } else {
                            // set  a waker and release the lock
                            let _waker = shared.sender.replace(ctx.waker().clone());
                            hatch.flags.store(flags, orderings::STORE);
                            this.value.replace(value);
                            Poll::Pending
                        }
                    }
                }
            }
        }
        Poll::Ready(Err(SendError::Closed(value)))
    }
}

/// A `Future` that waits for there to be capacity to send and a
/// `Receiver` listening. This allows computing an expensive value on
/// demand.
#[cfg(feature="async")]
pub struct Wait<'a, 'b, T> {
    sender: Option<&'b mut Sender<'a, T>>,
}

#[cfg(feature="async")]
impl<'a, 'b, T> Wait<'a, 'b, T> {
    // pin-project-lite does not let us define a Drop impl
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
