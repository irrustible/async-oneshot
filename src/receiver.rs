use crate::*;
use core::{marker::PhantomData, panic::UnwindSafe};
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
/// #[cfg(feature="alloc")] {
/// use async_hatch::hatch;
///
/// let (mut sender, mut receiver) = hatch::<usize>();
/// sender.send(42).now().unwrap();
/// assert_eq!(receiver.receive().now(), Ok(Some(42)));
/// }
/// ```
#[derive(Debug)]
pub struct Receiver<T, H: HatchRef<T>> {
    hatch: H::Stored,
    flags: Flags,
    _phan: PhantomData<T>,
}

#[repr(transparent)]
#[derive(Clone,Copy,Debug,Default,Eq,PartialEq)]
struct Flags(u8);

impl Flags {
    #[inline(always)]
    fn advise(&mut self, of: HatchFlags) { self.0 |= of.sender_closed_bit() }
    #[inline(always)]
    fn maybe_close(&mut self, when: Flags) { self.0 |= r_closes(when.0) }
    #[inline(always)]
    fn maybe_close_hatch(self, mut on: HatchFlags) -> HatchFlags {
        on.0 |= r_closes(self.0);
        on
    }
}

use crate::hatch::flags::*;

// Get rid of the boilerplate for flag methods.
macro_rules! flags {
    ($typ:ty { $( $flag:ident : [$is:ident, $with:ident, $set:ident] ),* $(,)? }) => {
        impl $typ {
            $(
                #[inline(always)]
                pub fn $is(self) -> bool { any_flag(self.0, $flag) }
                #[allow(dead_code)]
                #[inline(always)]
                pub fn $with(self, on: bool) -> Self { Self(toggle_flag(self.0, $flag, on)) }
                #[inline(always)]
                pub fn $set(&mut self, on: bool) -> &mut Self { self.0 = toggle_flag(self.0, $flag, on); self }
            )*
        }
    }
}

flags! {
    Flags {
        R_CLOSE:          [is_receiver_closed,  receiver_closed,  set_receiver_closed],
        S_CLOSE:          [is_sender_closed,    sender_closed,    set_sender_closed],
        MARK_ON_DROP:     [is_mark_on_drop,     mark_on_drop,     set_mark_on_drop],
        CLOSE_ON_SUCCESS: [is_close_on_success, close_on_success, set_close_on_success],
    }
}

#[cfg(feature="async")]
flags! {
    Flags {
        WAITING:          [is_waiting,          waiting,          set_waiting],
    }
}

// A `Receiver<T>` is Send+Sync if T is Send.
unsafe impl<T: Send, H: HatchRef<T>> Send for Receiver<T, H> {}
// This is okay because we require a mut ref to do anything useful.
unsafe impl<T: Send, H: HatchRef<T>> Sync for Receiver<T, H> {}

macro_rules! recover_if_they_closed {
    ($flags:expr, $self:expr) => {
        if $flags.is_sender_closed() { // Safe because they closed.
            return Ok(unsafe { $self.recover_unchecked() }) // Safe because they closed.
        }
    }
}

impl<T, H: HatchRef<T>> Receiver<T, H> {
    /// Creates a new Receiver.
    ///
    /// ## Safety
    ///
    /// You must not permit multiple live receivers to exist.
    #[inline(always)]
    pub(crate) unsafe fn new(hatch: H::Stored) -> Self {
        Self { hatch, flags: Flags::default(), _phan: PhantomData }
    }

    #[must_use = "`receive` returns an operation object which you need to call `.now` on or poll as a Future."]
    /// Returns a disposable object for a single receive operation.
    pub fn receive(&mut self) -> Receiving<T, H> {
        let flags = self.flags;
        Receiving { receiver: self, flags }
    }

    /// Returns a new [`Sender`] if the old one has closed and we haven't.
    pub fn recover(&mut self) -> Result<sender::Sender<T, H>, RecoverError> {
        if self.flags.is_receiver_closed() { return Err(RecoverError::Closed); }
        let hatch = unsafe { H::hatch_ref(self.hatch) };
        recover_if_they_closed!(self.flags, self);
        // We don't know so we have to check.
        let flags = hatch.flags.load(Ordering::Acquire);
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
    pub unsafe fn recover_unchecked(&mut self) -> sender::Sender<T, H> {
        let hatch = H::hatch_ref(self.hatch);
        hatch.reclaim_unchecked(); // Reset the state and issue a release store.
        self.flags.set_sender_closed(false); // Reset the flag.
        sender::Sender::new(self.hatch) // Return the new sender.
    }

    /// Gets the value of the `mark_on_drop` option. See the [`Receiver`] docs for an explanation.
    pub fn get_mark_on_drop(&self) -> bool { self.flags.is_mark_on_drop() }

    /// Sets the `mark_on_drop` option in the builder style. See the [`Receiver`] docs for an explanation.
    pub fn mark_on_drop(mut self, on: bool) -> Self { self.flags.set_mark_on_drop(on); self }

    /// Sets the `mark_on_drop` option by mut ref. See the [`Receiver`] docs for an explanation.
    pub fn set_mark_on_drop(&mut self, on: bool) -> &mut Self { self.flags.set_mark_on_drop(on); self }

    /// Gets the value of the `close_on_receive` option. the [`Receiver`] docs for an explanation.
    pub fn get_close_on_receive(&self) -> bool { self.flags.is_close_on_success() }

    /// Sets the `close_on_receive` option in the builder style. See the [`Receiver`] docs for an explanation.
    pub fn close_on_receive(mut self, on: bool) -> Self { self.flags.set_close_on_success(on); self }

    /// Sets the `close_on_receive` option by mut ref. See the [`Receiver`] docs for an explanation.
    pub fn set_close_on_receive(&mut self, on: bool) -> &mut Self { self.flags.set_close_on_success(on); self }

    // Performs a lock on the hatch flags, copying the sender close
    // flag if it is set.
    #[inline(always)]
    fn lock(&mut self, ordering: Ordering) -> HatchFlags {
        let hatch = unsafe { H::hatch_ref(self.hatch) };
        let flags = hatch.lock(ordering);
        self.flags.advise(flags);
        flags
    }
    
    // Performs a fetch_xor on the hatch flags, copying the sender
    // close flag if it is set.
    #[cfg(not(feature="async"))]
    #[inline(always)]
    fn xor(&mut self, value: HatchFlags) -> HatchFlags {
        let hatch = unsafe { H::hatch_ref(self.hatch) };
        let flags = hatch.flags.fetch_xor(value, Ordering::AcqRel);
        self.flags.advise(flags);
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
    pub unsafe fn leak(mut receiver: Self) { receiver.flags.set_receiver_closed(true); }
}

impl<T, H: HatchRef<T>> Drop for Receiver<T, H> {
    fn drop(&mut self) {
        if self.flags.is_receiver_closed() { return; } // nothing to do
        let hatch = unsafe { H::hatch_ref(self.hatch) };
        if self.flags.is_sender_closed() { // safe because they closed
            return unsafe { <H as HatchRef<T>>::free(self.hatch, self.flags.is_mark_on_drop()) }
        }
        #[cfg(not(feature="async"))] {
            // Without async, we just set the flag and check we won.
            // Acquire: in case they closed.
            let flags = hatch.flags.fetch_or(HatchFlags::default().receiver_closed(true), Ordering::Acquire);
            if flags.is_sender_closed() { // safe because they closed
                return unsafe { <H as HatchRef<T>>::free(self.hatch, self.flags.is_mark_on_drop()) }
            }
        }
        #[cfg(feature="async")] {
            // For async, we have to lock and maybe wake the sender
            let flags = hatch.lock(Ordering::Acquire);
            if flags.is_sender_closed() { // safe because they closed
                return unsafe { <H as HatchRef<T>>::free(self.hatch, self.flags.is_mark_on_drop()) }
            }
            let shared = unsafe { &mut *hatch.inner.get() };
            let _recv = shared.receiver.take(); // we won't be waiting any more
            let sender = shared.sender.take();  // we might need to wake them
            // mark us closed and release the lock in one smooth action.
            hatch.flags.store(flags.receiver_closed(true), Ordering::Release);
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
/// #[cfg(feature="alloc")] {
/// use async_hatch::hatch;
///
/// let (mut sender, mut receiver) = hatch::<usize>();
/// sender.send(42).now().unwrap();
/// assert_eq!(receiver.receive().now(), Ok(Some(42)));
/// }
/// ```
pub struct Receiving<'a, T, H: HatchRef<T>> {
    receiver: &'a mut Receiver<T, H>,
    flags:    Flags,
}

unsafe impl<'a, T: Send, H: HatchRef<T>> Send for Receiving<'a, T, H> {}
unsafe impl<'a, T: Send, H: HatchRef<T>> Sync for Receiving<'a, T, H> {}

impl<'a, T, H: HatchRef<T>> UnwindSafe for Receiving<'a, T, H> {}

impl<'a, T, H: HatchRef<T>> Receiving<'a, T, H> {
    /// Gets the value of the `close_on_receive` option in the builder style. the [`Receiving`] docs for an explanation.
    pub fn get_close_on_receive(&self) -> bool { self.flags.is_close_on_success() }

    /// Sets the `close_on_receive` option by mut ref. See the [`Receiving`] docs for an explanation.
    pub fn close_on_receive(mut self, on: bool) -> Self { self.flags.set_close_on_success(on); self }
}

impl<'a, T, H: HatchRef<T>> Receiving<'a, T, H> {
    /// Attempts to receive a message synchronously.
    ///
    /// In async mode, wakes the sender if they are waiting.
    pub fn now(self) -> Result<Option<T>, Closed> {
        // We must not be closed to proceed.
        if self.receiver.flags.is_receiver_closed() { return Err(Closed) }
        let hatch = unsafe { H::hatch_ref(self.receiver.hatch) };
        // Now we must take a lock to access the value
        let flags = self.receiver.lock(Ordering::Acquire);
        let shared = unsafe { &mut *hatch.inner.get() };
        let value = shared.value.take();
        if flags.is_sender_closed() { return value.map(Some).ok_or(Closed); }
        #[cfg(not(feature="async"))] {
            // Dropping does not require taking a lock, so we can't just do a store.
            if value.is_some() {
                let flags = self.flags.maybe_close_hatch(HatchFlags::default().lock(true));
                self.receiver.xor(flags); // Maybe close, branchlessly.
                self.receiver.flags.maybe_close(self.flags);
            } else {
                let flags = self.receiver.xor(HatchFlags::default().lock(true)); // No closing.
                if flags.is_sender_closed() { return value.map(Some).ok_or(Closed); }
            }
            Ok(value)
        }
        #[cfg(feature="async")] {
            let sender =  shared.sender.take(); // Move the wake out of the critical region.
            let flags = self.flags.maybe_close_hatch(flags);
            hatch.flags.store(flags, Ordering::Release); // Unlock
            self.receiver.flags.maybe_close(self.flags); // Propagate any close.
            if let Some(waker) = sender { waker.wake(); } // Wake
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
impl<'a, T, H: HatchRef<T>> Future for Receiving<'a, T, H> {
    type Output = Result<T, Closed>;
    fn poll(self: Pin<&mut Self>, ctx: &mut Context) -> Poll<Self::Output> {
        // Safe because we do not move out of self.
        let this = unsafe { self.project() };
        // If we are closed, we have nothing to do
        if this.receiver.flags.is_receiver_closed() { return Poll::Ready(Err(Closed)); }
        // We need to take a lock for exclusive access.
        let hatch = unsafe { H::hatch_ref(this.receiver.hatch) };
        let flags = this.receiver.lock(Ordering::Acquire);
        let shared = unsafe { &mut *hatch.inner.get() };
        let value = shared.value.take();
        // If the sender is closed, we're done.
        if flags.is_sender_closed() { return Poll::Ready(value.ok_or(Closed)); }
        // Take the sender's waker
        let waker = shared.sender.take();
        if let Some(v) = value {
            // We don't need to wait anymore, yank our own waker.
            let _delay_drop = shared.receiver.take();
            // If we are set to close on success, we add our close flag.
            // store is enough because we have the lock.
            let flags = this.flags.maybe_close_hatch(flags);
            hatch.flags.store(flags, Ordering::Release);
            this.flags.set_waiting(false); // Disable our destructor.
            this.receiver.flags.maybe_close(this.flags); // Copy back the closed flag if set.
            if let Some(waker) = waker { waker.wake(); } // wake if necessary
            Poll::Ready(Ok(v))
        } else {
            // If we are set to close on success, we add our close flag.
            let _delay_drop = shared.receiver.replace(ctx.waker().clone());
            // Store is enough because we have the lock.
            hatch.flags.store(flags, Ordering::Release);
            this.flags.set_waiting(true); // Enable our destructor
            if let Some(waker) = waker { waker.wake(); } // Wake if required.
            Poll::Pending
        }
    }
}

#[cfg(all(feature="async", not(feature="messy")))]
impl<'a, T, H: HatchRef<T>> Drop for Receiving<'a, T, H> {
    // Clean out the waker to avoid a spurious wake. This would not
    // pose a correctness problem, but we strongly suspect this drop
    // method is faster than your executor can schedule.
    fn drop(&mut self) {
        // It's only worth proceeding if we've set a waker and the other side might wake us.
        if !self.flags.is_waiting() { return; }
        if self.receiver.flags.is_sender_closed() { return; }
        // We'll need a lock as usual. Acquire: in case the other side took our waker in the meantime.
        let flags = self.receiver.lock(Ordering::Acquire);
        if !flags.is_sender_closed() {
            // Our cleanup is to remove the waker. Also release the lock.
            let hatch = unsafe { H::hatch_ref(self.receiver.hatch) };
            let shared = unsafe { &mut *hatch.inner.get() };
            let _delay_drop = shared.receiver.take();
            hatch.flags.store(flags, Ordering::Release);
        }
    }
}
