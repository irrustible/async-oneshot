use crate::*;
use core::mem::forget;
#[cfg(feature="async")]
use core::{future::Future, pin::Pin, task::{Context, Poll}};

#[derive(Debug,Eq,PartialEq)]
/// The reason we couldn't send a message. And our value back.
pub struct SendError<T> {
    pub kind: SendErrorKind,
    pub value: T,
}

impl<T> SendError<T> {
    /// Creates a new error from a [`SendErrorKind`] and value.
    #[inline(always)]
    fn new(kind: SendErrorKind, value: T) -> Self { SendError { kind, value } }

    /// Creates a new `SendError::Closed` from a value.
    #[inline(always)]
    fn closed(value: T) -> Self { Self::new(SendErrorKind::Closed, value) }

    /// Creates a new `SendError::Full` from a value.
    #[inline(always)]
    fn full(value: T) -> Self { Self::new(SendErrorKind::Full, value) }
}


/// The reason a send operation failed.
#[derive(Debug,Eq,PartialEq)]
pub enum SendErrorKind {
    Closed,
    Full,
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
///
/// ## Examples
///
/// ```
/// use async_hatch::hatch;
///
/// let (mut sender, mut receiver) = hatch::<usize>();
/// sender.send(42).now().unwrap();
/// assert_eq!(receiver.receive().now(), Ok(Some(42)));
/// ```
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

    /// Creates a send operation object which can be used to send a single message.
    #[inline(always)]
    #[must_use = "`send` returns an operation object which you need to call `.now` on or poll as a Future."]
    pub fn send<'b>(&'b mut self, value: T) -> Sending<'a, 'b, T> {
        let flags = self.flags;
        Sending { sender: self, value: Some(value), flags }
    }

    /// Creates an operation object that can wait for the [`Receiver`]
    /// to be listening. Enables a "lazy send" semantics where you can
    /// delay producing an expensive value until it's required.
    #[cfg(feature="async")]
    #[inline(always)]
    #[must_use = "`wait` returns an operation object which you need to poll as a Future."]
    pub fn wait<'b>(&'b mut self) -> Wait<'a, 'b, T> {
        let flags = self.flags;
        Wait { sender: self, flags }
    }

    /// Returns a new [`Receiver`] after the old one has closed.
    pub fn recover(&mut self) -> Result<receiver::Receiver<'a, T>, RecoverError> {
        // First check our local flags because they're cheap to access.
        if any_flag(self.flags, S_CLOSE) { return Err(RecoverError::Closed); }
        if any_flag(self.flags, R_CLOSE) { // Safe because they closed.
            return Ok(unsafe { self.recover_unchecked() });
        }
        // If we're still here, we'll have to check the atomic.
        let flags = self.hatch.flags.load(orderings::LOAD);
        if any_flag(flags, R_CLOSE) { // Safe because they closed.
            Ok(unsafe { self.recover_unchecked() })
        } else {
            Err(RecoverError::Live)
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

    /// Sets the `overwrite` option in the builder style. See the [`Sender`] docs for an explanation.
    #[inline(always)]
    pub fn overwrite(mut self, on: bool) -> Self {
        self.flags = toggle_flag(self.flags, OVERWRITE, on);
        self
    }

    /// Sets the `overwrite` option from a mut ref. See the [`Sender`] docs for an explanation.
    #[inline(always)]
    pub fn set_overwrite(&mut self, on: bool) -> &mut Self {
        self.flags = toggle_flag(self.flags, OVERWRITE, on);
        self
    }

    /// Gets the value of the `close_on_send` option. See the [`Sender`] docs for an explanation.
    #[inline(always)]
    pub fn get_close_on_send(&self) -> bool { any_flag(self.flags, CLOSE_ON_SUCCESS) }

    /// Sets the `close_on_send` option in the builder style. See the [`Sender`] docs for an explanation.
    #[inline(always)]
    pub fn close_on_send(mut self, on: bool) -> Self {
        self.flags = toggle_flag(self.flags, CLOSE_ON_SUCCESS, on);
        self
    }

    /// Sets the `close_on_send` option from a mut ref. See the [`Sender`] docs for an explanation.
    #[inline(always)]
    pub fn set_close_on_send(&mut self, on: bool) -> &mut Self {
        self.flags = toggle_flag(self.flags, CLOSE_ON_SUCCESS, on);
        self
    }

    /// Gets the value of the `mark_on_drop` option. See the [`Sender`] docs for an explanation.
    #[inline(always)]
    pub fn get_mark_on_drop(&self) -> bool { any_flag(self.flags, MARK_ON_DROP) }

    /// Sets the `mark_on_drop` option in the builder style. See the [`Sender`] docs for an explanation.
    #[inline(always)]
    pub fn mark_on_drop(mut self, on: bool) -> Self {
        self.flags = toggle_flag(self.flags, MARK_ON_DROP, on);
        self
    }

    /// Sets the `mark_on_drop` option from a mut ref. See the [`Sender`] docs for an explanation.
    #[inline(always)]
    pub fn set_mark_on_drop(&mut self, on: bool) -> &mut Self {
        self.flags = toggle_flag(self.flags, MARK_ON_DROP, on);
        self
    }

    // Performs a lock on the hatch flags, copying the receiver
    // close flag if it is set.
    #[inline(always)]
    fn lock(&mut self) -> Flags {
        let flags = self.hatch.lock();
        self.flags |= flags & R_CLOSE;
        flags
    }

    // Performs a fetch_xor on the hatch flags, copying the receiver
    // close flag if it is set.
    #[cfg(not(feature="async"))]
    #[inline(always)]
    fn xor(&mut self, value: usize) -> usize {
        let flags = self.hatch.flags.fetch_xor(value, orderings::MODIFY);
        self.flags |= flags & R_CLOSE;
        flags
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

macro_rules! cleanup_if_they_closed {
    ($flags:expr, $self:expr) => {
        if any_flag($flags, R_CLOSE) { // safe because they closed
            return unsafe { $self.hatch.cleanup(any_flag($self.flags, MARK_ON_DROP)); }
        }
    }
}

impl<'a, T> Drop for Sender<'a, T> {
    fn drop(&mut self) {
        if any_flag(self.flags, S_CLOSE) { return; } // nothing to do
        cleanup_if_they_closed!(self.flags, self);
        #[cfg(not(feature="async"))] { // Set a flag, hope we won.
            let flags = self.hatch.flags.fetch_or(S_CLOSE, orderings::MODIFY);
            cleanup_if_they_closed!(flags, self);
        }
        #[cfg(feature="async")] { // Lock cycle
            let flags = self.hatch.lock();
            cleanup_if_they_closed!(flags, self);
            let shared = unsafe { &mut *self.hatch.inner.get() };
            let _send = shared.sender.take();  // we won't be waiting any more
            let recv = shared.receiver.take(); // we might need to wake them
            // release the lock, mark us closed.
            self.hatch.flags.store(flags | S_CLOSE, orderings::STORE);
            // Finally, wake the receiver if they are waiting.
            if let Some(waker) = recv { waker.wake(); }
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
///
/// ## Examples
///
/// ```
/// use async_hatch::hatch;
///
/// let (mut sender, mut receiver) = hatch::<usize>();
/// sender.send(42).now().unwrap();
/// assert_eq!(receiver.receive().now(), Ok(Some(42)));
/// ```
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

    /// Sets the `overwrite` option in the builder style. See the [`Sending`] docs for an explanation.
    #[inline(always)]
    pub fn overwrite(mut self, on: bool) -> Self {
        self.flags = toggle_flag(self.flags, OVERWRITE, on);
        self
    }

    /// Sets the `overwrite` option from a mut ref. See the [`Sending`] docs for an explanation.
    #[inline(always)]
    pub fn set_overwrite(&mut self, on: bool) -> &mut Self {
        self.flags = toggle_flag(self.flags, OVERWRITE, on);
        self
    }

    /// Gets the value of the `close_on_send` option. See the [`Sending`] docs for an explanation.
    #[inline(always)]
    pub fn get_close_on_send(&self) -> bool { any_flag(self.flags, CLOSE_ON_SUCCESS) }

    /// Sets the `close_on_send` option in the builder style. See the [`Sending`] docs for an explanation.
    #[inline(always)]
    pub fn close_on_send(mut self, on: bool) -> Self {
        self.flags = toggle_flag(self.flags, CLOSE_ON_SUCCESS, on);
        self
    }

    /// Sets the `close_on_send` option from a mut ref. See the [`Sending`] docs for an explanation.
    #[inline(always)]
    pub fn set_close_on_send(mut self, on: bool) -> Self {
        self.flags = toggle_flag(self.flags, CLOSE_ON_SUCCESS, on);
        self
    }

    // pin-project-lite does not let us define a Drop impl
    // https://github.com/taiki-e/pin-project-lite/issues/62#issuecomment-884188885
    #[cfg(feature="async")]
    fn project(self: Pin<&mut Self>) -> &mut Self {
        unsafe { Pin::get_unchecked_mut(self) }
    }
}

impl<'a, 'b, T> Sending<'a, 'b, T> {
    /// Attempts to send a message synchronously.
    ///
    /// If async is enabled and the receiver is waiting, will wake them.
    ///
    /// Returns an error and your value back when:
    /// * the Receiver has closed.
    /// * there was an existing message and overwrite is not enabled.
    pub fn now(mut self) -> Result<Option<T>, SendError<T>> {
        // We must be open to proceed.
        let value = self.value.take().unwrap();
        if any_flag(self.sender.flags, S_CLOSE) { return Err(SendError::closed(value)); }
        // Take a lock so that we have exclusive access to the mutable storage.
        let flags = self.sender.lock();
        // They must be open to proceed.
        if any_flag(flags, R_CLOSE) {
            forget(self); // Disable our destructor.
            return Err(SendError::closed(value));
        }
        // If we're still here, we got the lock (and thus exclusive access).
        let shared = unsafe { &mut *self.sender.hatch.inner.get() };
        if shared.value.is_none() || any_flag(self.flags, OVERWRITE) {
            // Perform the write.
            let value = shared.value.replace(value);
            let closes = s_closes(self.flags); // branchless close mask
            #[cfg(not(feature="async"))] {
                // In synchronous mode, having the lock does not stop
                // the other side from dropping. Thus we use an xor to
                // allow for it having changed and check if they
                // closed in between.
                let flags = self.sender.xor(LOCK | closes);
                if any_flag(flags, R_CLOSE) {
                    forget(self); // Disable our destructor.
                    Err(SendError::closed(shared.value.take().unwrap()))
                } else {
                    // If we closed, that will have an effect beyond this
                    // object, so we need to copy the flag to the sender.
                    self.sender.flags |= closes;
                    forget(self); // Disable our destructor.
                    Ok(value)
                }
            }
            #[cfg(feature="async")] {
                // In async mode, taking the lock means the other side
                // cannot do anything, even close. We are thus free to
                // use just a store to set the flags.
                //
                // To keep the critical section short, we don't wake
                // until after we've released the lock.
                let receiver = shared.receiver.take();
                let flags = flags | closes;
                self.sender.hatch.flags.store(flags, orderings::STORE);
                // If we closed, that will have an effect beyond this
                // object, so we need to copy the flag to the sender.
                self.sender.flags |= closes; // backport the closed flag
                forget(self); // Disable our destructor.
                // Now we wake, if they were waiting.
                if let Some(waker) = receiver { waker.wake(); }
                Ok(value)
            }
        } else {
            // We found a value and we do not overwrite. Unlock.
            #[cfg(not(feature="async"))] {
                // In synchronous mode, having the lock does not stop
                // the other side from dropping. Thus we use an xor to
                // allow for it having changed and check if they
                // closed in between.
                let flags = self.sender.xor(LOCK);
                if any_flag(flags, R_CLOSE) {
                    forget(self); // Disable our destructor.
                    return Err(SendError::closed(shared.value.take().unwrap()));
                }
            }
            #[cfg(feature="async")] {
                // In async mode, taking the lock means the other side
                // cannot do anything, even close. We are thus free to
                // use just a store to set the flags.
                self.sender.hatch.flags.store(flags, orderings::STORE);
                forget(self); // Disable our destructor.
            }
            Err(SendError::full(value))
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
        if any_flag(this.sender.flags, S_CLOSE) { return Poll::Ready(Err(SendError::closed(value))); }
        // Take a lock so we can store a value.
        let flags = this.sender.lock();
        if any_flag(flags, R_CLOSE) { return Poll::Ready(Err(SendError::closed(value))); }
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

#[cfg(all(feature="async", not(feature="messy")))]
impl<'a, 'b, T> Drop for Sending<'a, 'b, T> {
    // Clean out the waker to avoid a spurious wake. This would not
    // pose a correctness problem, but we strongly suspect this drop
    // method is faster than your executor.
    fn drop(&mut self) {
        // If we are closed or not waiting, we don't have to clean up
        if no_flag(self.flags, WAITING) { return; }
        if any_flag(self.sender.flags, S_CLOSE) { return; }
        // Take the lock so we can use the mutable state.
        let flags = self.sender.lock();
        // If they closed in the meantime, we needn't clean up.
        if no_flag(flags, R_CLOSE) {
            let shared = unsafe { &mut *self.sender.hatch.inner.get() };
            let _delay_drop = shared.sender.take();  // Remove our waker.
            self.sender.hatch.flags.store(flags, orderings::STORE); // Release the lock.
        }
    }
}

/// A `Future` that waits for there to be capacity to send and a
/// `Receiver` listening. This allows computing an expensive value on
/// demand (lazy send).
///
/// ## Examples
///
/// ```
/// use core::task::Poll;
/// use async_hatch::hatch;
/// use wookie::wookie; // a stepping futures executor.
///
/// let (mut sender, mut receiver) = hatch::<usize>();
/// // The sender has a large message to send, it wants to wait until
/// // the receiver is available
/// wookie!(r: receiver.receive());
/// { // Contains the scope of `s` so we can reuse the sender in a minute
///     wookie!(s: sender.wait());
///     assert_eq!(s.poll(), Poll::Pending);
///     // The receiver comes along and waits.
///     assert_eq!(r.poll(), Poll::Pending);
///     // That wakes the sender.
///     assert_eq!(s.woken(), 1);
///     // The sender completes its wait.
///     assert_eq!(s.poll(), Poll::Ready(Ok(())));
/// }
/// // It can now calculate its expensive value and send it.
/// assert_eq!(sender.send(42).now(), Ok(None));
/// // And finally, the receiver can receive it.
/// assert_eq!(r.poll(), Poll::Ready(Ok(42)));
/// ```
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

#[cfg(all(feature="async", not(feature="messy")))]
impl<'a, 'b, T> Drop for Wait<'a, 'b, T> {
    fn drop(&mut self) {
        // If we are closed or not waiting, we don't have to clean up
        if no_flag(self.flags, WAITING) { return; }
        if any_flag(self.sender.flags, S_CLOSE) { return; }
        // Take a lock as we need mutable access
        let flags = self.sender.lock();
        // If they closed in the meantime, we needn't clean up.
        if no_flag(flags, R_CLOSE) {
            let shared = unsafe { &mut *self.sender.hatch.inner.get() };
            let _delay_drop = shared.sender.take();  // Remove our waker.
            self.sender.hatch.flags.store(flags, orderings::STORE); // Release the lock.
        }
    }
}

#[cfg(feature="async")]
impl<'a, 'b, T> Future for Wait<'a, 'b, T> {
    type Output = Result<(), Closed>;
    fn poll(self: Pin<&mut Self>, ctx: &mut Context) -> Poll<Self::Output> {
        // Safe because we won't move out of self.
        let this = unsafe { self.project() };
        // If either side is closed, nothing to do. We have to source
        // the sender flags because R_CLOSE isn't updated locally.
        if any_flag(this.sender.flags | this.flags, S_CLOSE | R_CLOSE) { return Poll::Ready(Err(Closed)); }
        // Lock so that we may use the mutable storage.
        let flags = this.sender.lock();
        // If they closed, there is nothing to do.
        if any_flag(flags, R_CLOSE) { return Poll::Ready(Err(Closed)); }
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
