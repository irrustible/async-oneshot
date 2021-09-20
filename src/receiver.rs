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
pub struct Receiver<'a, T> {
    hatch: Holder<'a, Hatch<T>>,
    flags: Flags,
}

// A `Receiver<T>` is Send+Sync if T is Send.
unsafe impl<'a, T: Send> Send for Receiver<'a, T> {}
// This is okay because we require a mut ref to do anything useful.
unsafe impl<'a, T: Send> Sync for Receiver<'a, T> {}

macro_rules! recover_if_they_closed {
    ($flags:expr, $self:expr) => {
        if any_flag($flags, S_CLOSE) { // Safe because they closed.
            return Ok(unsafe { $self.recover_unchecked() }) // Safe because they closed.
        }
    }
}

impl<'a, T> Receiver<'a, T> {
    /// Creates a new Receiver.
    ///
    /// ## Safety
    ///
    /// You must not permit multiple live receivers to exist.
    #[inline(always)]
    pub(crate) unsafe fn new(hatch: Holder<'a, Hatch<T>>) -> Self {
        Self { hatch, flags: DEFAULT }
    }

    #[must_use = "`receive` returns an operation object which you need to call `.now` on or poll as a Future."]
    /// Returns a disposable object for a single receive operation.
    pub fn receive<'b>(&'b mut self) -> Receiving<'a, 'b, T> {
        let flags = self.flags;
        Receiving { receiver: self, flags }
    }

    /// Returns a new [`Sender`] if the old one has closed and we haven't.
    pub fn recover(&mut self) -> Result<sender::Sender<'a, T>, RecoverError> {
        if any_flag(self.flags, R_CLOSE) { return Err(RecoverError::Closed); }
        recover_if_they_closed!(self.flags, self);
        // We don't know so we have to check.
        let flags = self.hatch.flags.load(Ordering::Acquire);
        recover_if_they_closed!(flags, self);
        Err(RecoverError::Live)
    }

    /// Returns a new [`Sender`] without checking the old one has
    /// closed or that we are not closed.
    ///
    /// ## Safety
    ///
    /// * You must not permit multiple live Senders to exist.
    /// * You must not call this after we have closed.
    pub unsafe fn recover_unchecked(&mut self) -> sender::Sender<'a, T> {
        self.hatch.recycle();   // Reset the state and issue a release store.
        self.flags &= !S_CLOSE; // Reset the flag
        sender::Sender::new(self.hatch)
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

    // Performs a lock on the hatch flags, copying the sender close
    // flag if it is set.
    #[inline(always)]
    fn lock(&mut self) -> Flags {
        let flags = self.hatch.lock();
        self.flags |= flags & S_CLOSE;
        flags
    }
    
    // Performs a fetch_xor on the hatch flags, copying the sender
    // close flag if it is set.
    #[cfg(not(feature="async"))]
    #[inline(always)]
    fn xor(&mut self, value: usize) -> usize {
        let flags = self.hatch.flags.fetch_xor(value, Ordering::AcqRel);
        self.flags |= flags & S_CLOSE;
        flags
    }

    // Performs a fetch_and on the hatch flags, copying the sender
    // close flag if it is set.
    #[cfg(not(feature="async"))]
    #[inline(always)]
    fn and(&mut self, value: usize) -> usize {
        let flags = self.hatch.flags.fetch_and(value, Ordering::Release);
        self.flags |= flags & S_CLOSE;
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
    /// This isn't technically unsafe, but...
    /// * It could cause a memory leak.
    /// * It could cause the Sender to wait forever.
    pub unsafe fn leak(mut receiver: Receiver<T>) { receiver.flags |= R_CLOSE; }
}

macro_rules! cleanup_if_they_closed {
    ($flags:expr, $self:expr) => {
        if any_flag($flags, S_CLOSE) { // safe because they closed
            return unsafe { $self.hatch.cleanup(any_flag($self.flags, MARK_ON_DROP)); }
        }
    }
}

impl<'a, T> Drop for Receiver<'a, T> {
    fn drop(&mut self) {
        if any_flag(self.flags, R_CLOSE) { return; } // nothing to do
        cleanup_if_they_closed!(self.flags, self);
        #[cfg(not(feature="async"))] {
            // Without async, we just set the flag and check we won.
            let flags = self.hatch.flags.fetch_or(R_CLOSE, Ordering::Acquire);
            cleanup_if_they_closed!(flags, self);
        }
        #[cfg(feature="async")] {
            // For async, we have to lock and maybe wake the sender
            let flags = self.hatch.lock();
            cleanup_if_they_closed!(flags, self);
            let shared = unsafe { &mut *self.hatch.inner.get() };
            let _recv = shared.receiver.take(); // we won't be waiting any more
            let sender = shared.sender.take();  // we might need to wake them
            // mark us closed and release the lock in one smooth action.
            self.hatch.flags.store(flags | R_CLOSE, Ordering::Release);
            // Finally, wake the sender if they are waiting.
            if let Some(waker) = sender { waker.wake(); }
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
pub struct Receiving<'a, 'b, T> {
    receiver: &'b mut Receiver<'a, T>,
    flags:    Flags,
}

unsafe impl<'a, 'b, T: Send> Send for Receiving<'a, 'b, T> {}
unsafe impl<'a, 'b, T: Send> Sync for Receiving<'a, 'b, T> {}

impl<'a, 'b, T> Receiving<'a, 'b, T> {
    /// Gets the value of the `close_on_receive` option. the [`Receiving`] docs for an explanation.
    pub fn get_close_on_receive(&self) -> bool { any_flag(self.flags, CLOSE_ON_SUCCESS) }

    /// Sets the `close_on_receive` option. See the [`Receiving`] docs for an explanation.
    pub fn close_on_receive(mut self, on: bool) -> Self {
        self.flags = toggle_flag(self.flags, CLOSE_ON_SUCCESS, on);
        self
    }
}

impl<'a, 'b, T> Receiving<'a, 'b, T> {
    /// Attempts to receive a message synchronously.
    ///
    /// In async mode, wakes the sender if they are waiting.
    pub fn now(self) -> Result<Option<T>, Closed> {
        // We must not be closed to proceed.
        if any_flag(self.receiver.flags, R_CLOSE) { return Err(Closed) }
        // Now we must take a lock to access the value
        let flags = self.receiver.lock();
        let closes = r_closes(self.flags);
        let shared = unsafe { &mut *self.receiver.hatch.inner.get() };
        let value = shared.value.take();
        if any_flag(flags, S_CLOSE) { return value.map(Some).ok_or(Closed); }
        #[cfg(not(feature="async"))] {
            // Dropping does not require taking a lock, so we can't just do a store.
            if value.is_some() { 
                self.receiver.xor(LOCK | closes); // Maybe close, branchlessly.
                self.receiver.flags |= closes;
            } else {
                let flags = self.receiver.and(!LOCK); // No closing.
                if any_flag(flags, S_CLOSE) { return value.map(Some).ok_or(Closed); }
            }
            Ok(value)
        }
        #[cfg(feature="async")] {
            let sender =  shared.sender.take(); // Move the wake out of the critical region.
            self.receiver.hatch.flags.store(flags | closes, Ordering::Release); // Unlock
            self.receiver.flags |= closes;                                     // Propagate any close.
            if let Some(waker) = sender { waker.wake(); }               // Wake
            Ok(value)
        }
    }

    // pin-project-lite does not let us define a destructor.
    // https://github.com/taiki-e/pin-project-lite/issues/62#issuecomment-884188885
    #[inline(always)]
    #[cfg(feature="async")]
    unsafe fn project(self: Pin<&mut Self>) -> &mut Self { Pin::get_unchecked_mut(self) }
}

#[cfg(feature="async")]
impl<'a, 'b, T> Future for Receiving<'a, 'b, T> {
    type Output = Result<T, Closed>;
    fn poll(self: Pin<&mut Self>, ctx: &mut Context) -> Poll<Self::Output> {
        // Safe because we do not move out of self.
        let this = unsafe { self.project() };
        // If we are closed, we have nothing to do
        if any_flag(this.flags, R_CLOSE) { return Poll::Ready(Err(Closed)); }
        // We need to take a lock for exclusive access.
        let flags = this.receiver.lock();
        let shared = unsafe { &mut *this.receiver.hatch.inner.get() };
        let value = shared.value.take();
        // If the sender is closed, we're done.
        if any_flag(flags, S_CLOSE) { return Poll::Ready(value.ok_or(Closed)); }
        // Take the sender's waker
        let waker = shared.sender.take();
        if let Some(v) = value {
            // We don't need to wait anymore, yank our own waker.
            let _delay_drop = shared.receiver.take();
            // If we are set to close on success, we add our close flag.
            let closes = r_closes(this.flags);
            // store is enough because we have the lock.
            this.receiver.hatch.flags.store(flags | closes, Ordering::Release);
            this.flags &= !WAITING; // Disable our destructor.
            this.receiver.flags |= closes; // Copy back the closed flag if set.
            if let Some(waker) = waker { waker.wake(); } // wake if necessary
            Poll::Ready(Ok(v))
        } else {
            // If we are set to close on success, we add our close flag.
            let _delay_drop = shared.receiver.replace(ctx.waker().clone());
            // Store is enough because we have the lock.
            this.receiver.hatch.flags.store(flags, Ordering::Release);
            this.flags |= WAITING; // Enable our destructor
            if let Some(waker) = waker { waker.wake(); } // Wake if required.
            Poll::Pending
        }
    }
}

#[cfg(all(feature="async", not(feature="messy")))]
impl<'a, 'b, T> Drop for Receiving<'a, 'b, T> {
    // Clean out the waker to avoid a spurious wake. This would not
    // pose a correctness problem, but we strongly suspect this drop
    // method is faster than your executor can schedule.
    fn drop(&mut self) {
        // If we are not waiting, we don't need to do anythning.
        if no_flag(self.flags, WAITING) { return; }
        // Likewise if the other side is closed.
        if any_flag(self.receiver.flags, S_CLOSE) { return; }
        // We'll need a lock as usual
        let flags = self.receiver.lock();
        if no_flag(flags, S_CLOSE) {
            // Our cleanup is to remove the waker. Also release the lock.
            let shared = unsafe { &mut *self.receiver.hatch.inner.get() };
            let _delay_drop = shared.receiver.take();
            self.receiver.hatch.flags.store(flags, Ordering::Release);
        }
    }
}
