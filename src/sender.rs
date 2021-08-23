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
    hatch: Holder<'a, Hatch<T>>,
    flags: Flags,
}

// A `Sender<T>` is Send+Sync if T is Send
unsafe impl<'a, T: Send> Send for Sender<'a, T> {}
unsafe impl<'a, T: Send> Sync for Sender<'a, T> {}

impl<'a, T> Sender<'a, T> {
    /// Creates a new Sender.
    ///
    /// # Safety
    ///
    /// You must not permit multiple live senders to exist.
    #[inline(always)]
    pub(crate) unsafe fn new(hatch: Holder<'a, Hatch<T>>) -> Self {
        Sender { hatch, flags: DEFAULT }
    }

    /// Returns a disposable object for a single send operation.
    #[inline(always)]
    #[must_use = "Sending does nothing unless you call `.now` or poll it as a Future."]
    pub fn send<'b>(&'b mut self, value: T) -> Sending<'a, 'b, T> {
        let flags = self.flags;
        Sending { sender: self, value: Some(value), flags }
    }

    #[cfg(feature="async")]
    // #[must_use = "Wait does nothing unless you poll it as a Future."]
    /// Creates a disposable object that can wait for the [`Receiver`] to be listening.
    pub fn wait<'b>(&'b mut self) -> Wait<'a, 'b, T> {
        let flags = self.flags;
        Wait { sender: self, flags }
    }

    /// Returns a new [`Receiver`] after the old one has closed.
    pub fn recover(&mut self) -> Result<receiver::Receiver<'a, T>, RecoverError> {
        if any_flag(self.flags, S_CLOSE) { return Err(RecoverError::Closed); }
        if any_flag(self.flags, R_CLOSE) { // safe because they closed
            Ok(unsafe { self.recover_unchecked() })
        } else {
            // We don't know so we have to check.
            let flags = self.hatch.flags.load(orderings::LOAD);
            if any_flag(flags, R_CLOSE) {
                Ok(unsafe { self.recover_unchecked() })
            } else {
                Err(RecoverError::Live)
            }
        }
    }

    /// Returns a new [`Receiver`] without checking the old one has closed.
    ///
    /// ## Safety
    ///
    /// You must not permit multiple live Receivers to exist.
    pub unsafe fn recover_unchecked(&mut self) -> receiver::Receiver<'a, T> {
        self.hatch.recycle(); // A release store
        self.flags &= !R_CLOSE; // reset flag
        receiver::Receiver::new(self.hatch)
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
    /// You must not permit multiple live senders to exist.
    pub unsafe fn leak(mut sender: Sender<T>) { sender.flags |= S_CLOSE; }
}

impl<'a, T> Drop for Sender<'a, T> {
    fn drop(&mut self) {
        if any_flag(self.flags, S_CLOSE) { return; }
        if any_flag(self.flags, R_CLOSE) { // safe because they closed
            return unsafe { self.hatch.cleanup(any_flag(self.flags, MARK_ON_DROP)); }
        }
        #[cfg(not(feature="async"))] {
            // Without async, we just set the flag and check we won.
            let flags = self.hatch.flags.fetch_or(S_CLOSE, orderings::MODIFY);
            if any_flag(flags, R_CLOSE) {
                // Safe because we have seen the receiver closed
                unsafe { self.hatch.cleanup(any_flag(self.flags, MARK_ON_DROP)); }
            } 
        }
        #[cfg(feature="async")] {
            // For async, we have to lock and maybe wake the receiver
            let flags = self.hatch.lock();
            if any_flag(flags, R_CLOSE) { // safe because they closed
                unsafe { self.hatch.cleanup(any_flag(self.flags, MARK_ON_DROP)) };
            } else {
                let shared = unsafe { &mut *self.hatch.inner.get() };
                let _send = shared.sender.take();  // we won't be waiting any more
                let recv = shared.receiver.take(); // we might need to wake them
                // simultaneously mark us closed and release the lock.
                self.hatch.flags.store(flags | S_CLOSE, orderings::STORE);
                // Finally, wake the receiver if they are waiting.
                if let Some(waker) = recv { waker.wake(); }
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
/// `Future` impl ignores the `overwrite` option (`now` does not).
#[derive(Debug)]
pub struct Sending<'a, 'b, T> {
    sender: &'b mut Sender<'a, T>,
    value: Option<T>,
    flags: Flags,
}

unsafe impl<'a, 'b, T: Send> Send for Sending<'a, 'b, T> {}
unsafe impl<'a, 'b, T: Send> Sync for Sending<'a, 'b, T> {}

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
        // Check for life
        if any_flag(self.sender.flags, S_CLOSE) { return Err(SendError::Closed(value)); }
        let value = self.value.take().unwrap();
        // Take a lock so we can store a value.
        let flags = self.sender.hatch.lock();
        self.sender.flags |= flags & R_CLOSE;
        if any_flag(flags, R_CLOSE) { return Err(SendError::Closed(value)); }
        // Still here? Let's go.
        let shared = unsafe { &mut *hatch.inner.get() };
        if shared.value.is_none() || any_flag(self.flags, OVERWRITE) {
            let value = shared.value.replace(value);
            // Dropping does not require taking a lock, so we can't just do a `store` as in
            // the async case. We compose a mask to xor with it that unlocks and may also
            // close.
            let closes = s_closes(self.flags);
            let flags = hatch.flags.fetch_xor(LOCK | closes, orderings::MODIFY);
            self.sender.flags |= flags & R_CLOSE;
            // All that was because they might close on us.
            if any_flag(flags, R_CLOSE) {
                Err(SendError::Closed(shared.value.take().unwrap()));
            } else {
                self.sender.flags |= closes;
                return Ok(value)
            }
        } else {
            hatch.flags.store(flags, orderings::STORE);
            return Err(SendError::Existing(value))
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
        let value = self.value.take().unwrap();
        // Check for life
        if any_flag(self.sender.flags, S_CLOSE | R_CLOSE) {
            return Err(SendError::Closed(value));
        }
        // Now we must take a lock to access the value
        let flags = self.sender.hatch.lock();
        self.sender.flags |= flags & R_CLOSE;
        if any_flag(flags, R_CLOSE) { return Err(SendError::Closed(value)); }
        let shared = unsafe { &mut *self.sender.hatch.inner.get() };
        if shared.value.is_none() || any_flag(self.flags, OVERWRITE) {
            // We are going to succeed!
            let value = shared.value.replace(value);
            let receiver = shared.receiver.take();
            // Release the lock, setting the close flag if we close on success
            let closes = s_closes(self.flags);
            let flags = flags | closes;
            self.sender.hatch.flags.store(flags, orderings::STORE);
            self.sender.flags |= closes;
            // If the receiver is waiting, wake them.
            if let Some(waker) = receiver { waker.wake(); }
            Ok(value)
        } else {
            // We found a value and we do not overwrite. Unlock.
            self.sender.hatch.flags.store(flags, orderings::STORE);
            Err(SendError::Existing(value))
        }
    }

    // pin-project-lite does not let us define a Drop impl
    // https://github.com/taiki-e/pin-project-lite/issues/62#issuecomment-884188885
    fn project(self: Pin<&mut Self>) -> &mut Self {
        unsafe { Pin::get_unchecked_mut(self) }
    }
}

#[cfg(feature="async")]
impl<'a, 'b, T> Drop for Sending<'a, 'b, T> {
    // Clean out the waker. This is a calculated bet that this drop is faster than your async
    // executor (probably true in prod, sadly not in our benchmarks).
    fn drop(&mut self) {
        // If we are lonely or not waiting, we don't have to clean up
        if WAITING != (self.flags | self.sender.flags) & (S_CLOSE | WAITING) { return; }
        let flags = self.sender.hatch.lock();
        self.sender.flags |= flags & R_CLOSE;
        if no_flag(flags, R_CLOSE) {
            // Our cleanup is to remove the waker. Also release the lock.
            let shared = unsafe { &mut *self.sender.hatch.inner.get() };
            let _delay_drop = shared.sender.take();
            self.sender.hatch.flags.store(flags, orderings::STORE);
        }
    }
}

#[cfg(feature="async")]
impl<'a, 'b, T> Future for Sending<'a, 'b, T> {
    type Output = Result<(), SendError<T>>;
    fn poll(self: Pin<&mut Self>, ctx: &mut Context) -> Poll<Self::Output> {
        let this = self.project();
        let value = this.value.take().unwrap();
        // Check for life.
        if any_flag(this.sender.flags, S_CLOSE) { return Poll::Ready(Err(SendError::Closed(value))); }
        // Take a lock so we can store a value.
        let flags = this.sender.hatch.lock();
        this.sender.flags |= R_CLOSE;
        if any_flag(flags, R_CLOSE) { return Poll::Ready(Err(SendError::Closed(value))); }
        let shared = unsafe { &mut *this.sender.hatch.inner.get() };
        if shared.value.is_none() {
            shared.value = Some(value);
            let receiver = shared.receiver.take();
            // release the lock and maybe close
            let closes = s_closes(this.flags);
            let flags = flags | closes;
            this.sender.hatch.flags.store(flags, orderings::STORE);
            this.sender.flags |= closes;
            // And since we delivered, we should wake the receiver.
            if let Some(waker) = receiver { waker.wake(); }
            Poll::Ready(Ok(()))
        } else {
            // set a waker and release the lock
            let _waker = shared.sender.replace(ctx.waker().clone());
            this.sender.hatch.flags.store(flags, orderings::STORE);
            // put the value back for the next poll
            this.value.replace(value);
            Poll::Pending
        }
    }
}

/// A `Future` that waits for there to be capacity to send and a
/// `Receiver` listening. This allows computing an expensive value on
/// demand (lazy send).
#[cfg(feature="async")]
pub struct Wait<'a, 'b, T> {
    sender: &'b mut Sender<'a, T>,
    flags:  Flags,
}

#[cfg(feature="async")]
impl<'a, 'b, T> Wait<'a, 'b, T> {
    // pin-project-lite does not let us define a Drop impl
    // https://github.com/taiki-e/pin-project-lite/issues/62#issuecomment-884188885
    unsafe fn project(self: Pin<&mut Self>) -> &mut Self {
        Pin::get_unchecked_mut(self)
    }
}

#[cfg(feature="async")]
impl<'a, 'b, T> Drop for Wait<'a, 'b, T> {
    // clean out the waker. essentially this makes our benchmarks worse but should improve real
    // world performance, since we assume that an async executor can't match our performance.
    fn drop(&mut self) {
        if WAITING != (self.flags | self.sender.flags) & (S_CLOSE | WAITING) { return; }
        // We'll need a lock as usual
        let flags = self.sender.hatch.lock();
        self.sender.flags |= flags & R_CLOSE; // propagate any close.
        // Our cleanup is to remove the waker. Also release the lock.
        let shared = unsafe { &mut *self.sender.hatch.inner.get() };
        let _delay_drop = shared.sender.take();
        self.sender.hatch.flags.store(flags, orderings::STORE);
    }
}

#[cfg(feature="async")]
impl<'a, 'b, T> Future for Wait<'a, 'b, T> {
    type Output = Result<(), Closed>;
    fn poll(self: Pin<&mut Self>, ctx: &mut Context) -> Poll<Self::Output> {
        // Safe because we won't move out of self.
        let this = unsafe { self.project() };
        // Check for life
        if any_flag(this.sender.flags | this.flags, S_CLOSE | R_CLOSE) { return Poll::Ready(Err(Closed)); }
        // We'll need a lock as usual
        let flags = this.sender.hatch.lock();
        this.sender.flags |= flags & R_CLOSE;
        if any_flag(flags, R_CLOSE) { return Poll::Ready(Err(Closed)); }
        // Okay, we're good.
        let shared = unsafe { &mut *this.sender.hatch.inner.get() };
        if shared.receiver.is_some() && shared.value.is_none() {
            // Woohoo! Take the sender and release the lock.
            let _delay_drop = shared.sender.take(); // No need to wait.
            this.sender.hatch.flags.store(flags, orderings::STORE);
            this.flags &= !WAITING; // Unset the flag
            Poll::Ready(Ok(()))
        } else {
            // Set a waker and unlock.
            let _delay_drop = shared.sender.replace(ctx.waker().clone());
            this.sender.hatch.flags.store(flags, orderings::STORE);
            this.flags |= WAITING;
            Poll::Pending
        }
    }
}
