use crate::*;
use core::marker::PhantomData;
use core::mem::forget;
use core::panic::UnwindSafe;
#[cfg(feature="async")]
use core::{future::Future, pin::Pin, task::{Context, Poll}};

#[derive(Copy,Clone,Debug,Eq,PartialEq)]
/// The reason we couldn't send a message. And our value back.
pub struct SendError<T> {
    pub kind: SendErrorKind,
    pub value: T,
}

impl<T> SendError<T> {
    /// Creates a new error from a [`SendErrorKind`] and value.
    #[inline(always)]
    fn new(kind: SendErrorKind, value: T) -> Self { SendError { kind, value } }

    /// Creates a new [`SendError`] with [`SendErrorKind::Closed`] from a value.
    #[inline(always)]
    fn closed(value: T) -> Self { Self::new(SendErrorKind::Closed, value) }

    /// Creates a new [`SendError`] with [`SendErrorKind::Full`] from a value.
    #[inline(always)]
    fn full(value: T) -> Self { Self::new(SendErrorKind::Full, value) }
}


/// The reason a send operation failed.
#[derive(Clone,Copy,Debug,Eq,PartialEq)]
pub enum SendErrorKind {
    /// Messages may not be sent on a closed channel.
    Closed,
    /// There is already an item in the channel and the `overwrite` flag is not enabled.
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
/// #[cfg(feature="alloc")] {
/// use async_hatch::hatch;
///
/// let (mut sender, mut receiver) = hatch::<usize>();
/// sender.send(42).now().unwrap();
/// assert_eq!(receiver.receive().now(), Ok(Some(42)));
/// }
/// ```
#[derive(Debug)]
pub struct Sender<T, H: HatchRef<T>> {
    hatch: H::Stored,
    flags: Flags,
    _phan: PhantomData<T>,
}

#[repr(transparent)]
#[derive(Clone,Copy,Debug,Default,Eq,PartialEq)]
struct Flags(u8);

use hatch::flags::*;

// Get rid of the boilerplate for flag methods.
macro_rules! flags {
    ($typ:ty { $( $flag:ident : [$is:ident, $with:ident, $set:ident] ),* $(,)? }) => {
        #[allow(dead_code)]
        impl $typ {
            $(
                #[inline(always)]
                fn $is(self) -> bool { any_flag(self.0, $flag) }
                #[inline(always)]
                fn $with(self, on: bool) -> Self { Self(toggle_flag(self.0, $flag, on)) }
                #[inline(always)]
                fn $set(&mut self, on: bool) -> &mut Self { self.0 = toggle_flag(self.0, $flag, on); self }
            )*
        }
    }
}

flags! {
    Flags {
        R_CLOSE:          [is_receiver_closed,  receiver_closed,  set_receiver_closed],
        S_CLOSE:          [is_sender_closed,    sender_closed,    set_sender_closed],
        OVERWRITE:        [is_overwrite,        overwrite,        set_overwrite],
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
impl Flags {
    #[inline(always)]
    fn advise(&mut self, of: HatchFlags) { self.0 |= of.receiver_closed_bit() }
    #[inline(always)]
    fn maybe_close(&mut self, when: Flags) { self.0 |= s_closes(when.0) }
    fn maybe_close_hatch(self, mut on: HatchFlags) -> HatchFlags {
        on.0 |= s_closes(self.0);
        on
    }

}
// A `Sender<H>` is Send+Sync if T is Send
unsafe impl<T, H: HatchRef<T>> Send for Sender<T, H> {}
unsafe impl<T, H: HatchRef<T>> Sync for Sender<T, H> {}

impl<T, H: HatchRef<T>> Sender<T, H> {
    /// Creates a new Sender.
    ///
    /// # Safety
    ///
    /// You must not permit multiple live senders to exist.
    #[inline(always)]
    pub(crate) unsafe fn new(hatch: H::Stored) -> Self {
        Sender { hatch, flags: Flags::default(), _phan: PhantomData }
    }

    /// Creates a send operation object which can be used to send a single message.
    #[inline(always)]
    #[must_use = "`send` returns an operation object which you need to call `.now` on or poll as a Future."]
    pub fn send(&mut self, value: T) -> Sending<T, H> {
        let flags = self.flags;
        Sending { sender: self, value: Some(value), flags }
    }

    /// Creates an operation object that can wait for the [`Receiver`]
    /// to be listening. Enables a "lazy send" semantics where you can
    /// delay producing an expensive value until it's required.
    #[cfg(feature="async")]
    #[inline(always)]
    #[must_use = "`wait` returns an operation object which you need to poll as a Future."]
    pub fn wait(&mut self) -> Wait<T, H> {
        let flags = self.flags;
        Wait { sender: self, flags }
    }

    /// Returns a new [`Receiver`] after the old one has closed.
    pub fn recover(&mut self) -> Result<receiver::Receiver<T, H>, RecoverError> {
        // If we don't have a handle to the hatch, it's too late to recover
        if self.flags.is_sender_closed() { return Err(RecoverError::Closed); }
        // If we know they are closed, we don't need to check the atomic
        if self.flags.is_receiver_closed() {
            return Ok(unsafe { self.recover_unchecked() });
        }
        // We will have to go back to the atomic and check if they're closed.
        let hatch = unsafe { H::hatch_ref(self.hatch) };
        let flags = hatch.flags.load(Ordering::Relaxed);
        if flags.is_receiver_closed() {
            // We're the last one with access and we should clean up.
            Ok(unsafe { self.recover_unchecked() })
        } else {
            // The channel is not in need of recovery.
            Err(RecoverError::Live)
        }
    }

    /// Returns a new [`Receiver`] without checking the old one has closed.
    ///
    /// ## Safety
    ///
    /// You must not permit multiple live Receivers to exist.
    pub unsafe fn recover_unchecked(&mut self) -> receiver::Receiver<T, H> {
        let hatch = H::hatch_ref(self.hatch);
        hatch.reclaim_unchecked(); // A release store
        self.flags.set_receiver_closed(false);
        receiver::Receiver::new(self.hatch)
    }

    /// Gets the value of the `overwrite` option. See the [`Sender`] docs for an explanation.
    #[inline(always)]
    pub fn get_overwrite(&self) -> bool { self.flags.is_overwrite() }

    /// Sets the `overwrite` option in the builder style. See the [`Sender`] docs for an explanation.
    #[inline(always)]
    pub fn overwrite(mut self, on: bool) -> Self { self.flags.set_overwrite(on); self }

    /// Sets the `overwrite` option from a mut ref. See the [`Sender`] docs for an explanation.
    #[inline(always)]
    pub fn set_overwrite(&mut self, on: bool) -> &mut Self { self.flags.set_overwrite(on); self }

    /// Gets the value of the `close_on_send` option. See the [`Sender`] docs for an explanation.
    #[inline(always)]
    pub fn get_close_on_send(&self) -> bool { self.flags.is_close_on_success() }

    /// Sets the `close_on_send` option in the builder style. See the [`Sender`] docs for an explanation.
    #[inline(always)]
    pub fn close_on_send(mut self, on: bool) -> Self { self.flags.set_close_on_success(on); self }

    /// Sets the `close_on_send` option from a mut ref. See the [`Sender`] docs for an explanation.
    #[inline(always)]
    pub fn set_close_on_send(&mut self, on: bool) -> &mut Self { self.flags.set_close_on_success(on); self }

    /// Gets the value of the `mark_on_drop` option. See the [`Sender`] docs for an explanation.
    #[inline(always)]
    pub fn get_mark_on_drop(&self) -> bool { self.flags.is_mark_on_drop() }

    /// Sets the `mark_on_drop` option in the builder style. See the [`Sender`] docs for an explanation.
    #[inline(always)]
    pub fn mark_on_drop(mut self, on: bool) -> Self { self.flags.set_mark_on_drop(on); self }

    /// Sets the `mark_on_drop` option from a mut ref. See the [`Sender`] docs for an explanation.
    #[inline(always)]
    pub fn set_mark_on_drop(&mut self, on: bool) -> &mut Self { self.flags.set_mark_on_drop(on); self }

    // Performs a lock on the hatch flags, copying the receiver
    // close flag if it is set.
    #[inline(always)]
    fn lock(&mut self, ordering: Ordering) -> HatchFlags {
        let hatch = unsafe { H::hatch_ref(self.hatch) };
        let flags = hatch.lock(ordering);
        self.flags.advise(flags);
        flags
    }

    // Performs a fetch_xor on the hatch flags, copying the receiver
    // close flag if it is set.
    #[cfg(not(feature="async"))]
    #[inline(always)]
    fn xor(&mut self, value: HatchFlags, ordering: Ordering) -> HatchFlags {
        let hatch = unsafe { H::hatch_ref(self.hatch) };
        let flags = hatch.flags.fetch_xor(value, ordering);
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
    /// You must not permit multiple live senders to exist.
    pub unsafe fn leak(mut sender: Self) { sender.flags.set_sender_closed(true); }
}

impl<T, H: HatchRef<T>> Drop for Sender<T, H> {
    fn drop(&mut self) {
        if self.flags.is_sender_closed() { return; } // nothing to do
        let hatch = unsafe { H::hatch_ref(self.hatch) };
        if self.flags.is_receiver_closed() { // safe because they closed
            return unsafe { <H as HatchRef<T>>::free(self.hatch, self.flags.is_mark_on_drop()) }
        }
        #[cfg(not(feature="async"))] { // Set a flag, hope we won.
            let flags = hatch.flags.fetch_or(HatchFlags::default().sender_closed(true), Ordering::Relaxed);
            if flags.is_receiver_closed() { // safe because they closed
                return unsafe { <H as HatchRef<T>>::free(self.hatch, self.flags.is_mark_on_drop()) }
            }
        }
        #[cfg(feature="async")] { // Lock cycle
            // Lock for the wakers. Acquire: the wakers.
            let flags = hatch.lock(Ordering::Acquire);
            if flags.is_receiver_closed() { // safe because they closed
                return unsafe { <H as HatchRef<T>>::free(self.hatch, self.flags.is_mark_on_drop()) }
            }
            let shared = unsafe { &mut *hatch.inner.get() };
            let _send = shared.sender.take();  // we won't be waiting any more
            let recv = shared.receiver.take(); // we might need to wake them
            // Unlock. Release: the wakers
            hatch.flags.store(flags.sender_closed(true), Ordering::Release);
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
/// #[cfg(feature="alloc")] {
/// use async_hatch::hatch;
///
/// let (mut sender, mut receiver) = hatch::<usize>();
/// sender.send(42).now().unwrap();
/// assert_eq!(receiver.receive().now(), Ok(Some(42)));
/// }
/// ```
pub struct Sending<'a, T, H: HatchRef<T>> {
    sender: &'a mut Sender<T, H>,
    value: Option<T>,
    flags: Flags,
}

unsafe impl<'a, T: Send, H: HatchRef<T>> Send for Sending<'a, T, H> {}
unsafe impl<'a, T: Send, H: HatchRef<T>> Sync for Sending<'a, T, H> {}

impl<'a, T, H: HatchRef<T>> UnwindSafe for Sending<'a, T, H> {}

impl<'a, T, H: HatchRef<T>> Sending<'a, T, H> {
    /// Gets the value of the `overwrite` option. See the [`Sending`] docs for an explanation.
    #[inline(always)]
    pub fn get_overwrite(&self) -> bool { self.flags.is_overwrite() }

    /// Sets the `overwrite` option in the builder style. See the [`Sending`] docs for an explanation.
    #[inline(always)]
    pub fn overwrite(mut self, on: bool) -> Self { self.flags.set_overwrite(on); self }

    /// Sets the `overwrite` option from a mut ref. See the [`Sending`] docs for an explanation.
    #[inline(always)]
    pub fn set_overwrite(&mut self, on: bool) -> &mut Self { self.flags.set_overwrite(on); self }

    /// Gets the value of the `close_on_send` option. See the [`Sending`] docs for an explanation.
    #[inline(always)]
    pub fn get_close_on_send(&self) -> bool { self.flags.is_close_on_success() }

    /// Sets the `close_on_send` option in the builder style. See the [`Sending`] docs for an explanation.
    #[inline(always)]
    pub fn close_on_send(mut self, on: bool) -> Self { self.flags.set_close_on_success(on); self }

    /// Sets the `close_on_send` option from a mut ref. See the [`Sending`] docs for an explanation.
    #[inline(always)]
    pub fn set_close_on_send(mut self, on: bool) -> Self { self.flags.set_close_on_success(on); self }

    // pin-project-lite does not let us define a Drop impl
    // https://github.com/taiki-e/pin-project-lite/issues/62#issuecomment-884188885
    #[cfg(feature="async")]
    fn project(self: Pin<&mut Self>) -> &mut Self {
        unsafe { Pin::get_unchecked_mut(self) }
    }
}

impl<'a, T, H: HatchRef<T>> Sending<'a, T, H> {
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
        if self.sender.flags.is_sender_closed() { return Err(SendError::closed(value)); }
        // Take a lock so that we have exclusive access to the mutable storage.
        let flags = self.sender.lock(Ordering::Acquire);
        // They must be open to proceed.
        if flags.is_receiver_closed() {
            forget(self); // Disable our destructor.
            return Err(SendError::closed(value));
        }
        // If we're still here, we got the lock (and thus exclusive access).
        let hatch = unsafe { H::hatch_ref(self.sender.hatch) };
        let shared = unsafe { &mut *hatch.inner.get() };
        if shared.value.is_none() || self.flags.is_overwrite() {
            // Perform the write.
            let value = shared.value.replace(value);
            #[cfg(not(feature="async"))] {
                // In synchronous mode, having the lock does not stop
                // the other side from dropping. Thus we use an xor to
                // allow for it having changed and check if they
                // closed in between.
                let flags = self.flags.maybe_close_hatch(HatchFlags::default().lock(true));
                let flags = self.sender.xor(flags, Ordering::Release);
                if flags.is_receiver_closed() {
                    forget(self); // Disable our destructor.
                    Err(SendError::closed(shared.value.take().unwrap()))
                } else {
                    // If we closed, that will have an effect beyond this
                    // object, so we need to copy the flag to the sender.
                    self.sender.flags.maybe_close(self.flags);
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
                let flags = self.flags.maybe_close_hatch(flags);
                hatch.flags.store(flags, Ordering::Release);
                // If we closed, that will have an effect beyond this
                // object, so we need to copy the flag to the sender.
                self.sender.flags.maybe_close(self.flags);
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
                let flags = self.sender.xor(HatchFlags::default().lock(true), Ordering::Relaxed);
                if flags.is_receiver_closed() {
                    return Err(SendError::closed(shared.value.take().unwrap()));
                }
            }
            #[cfg(feature="async")] {
                // In async mode, taking the lock means the other side
                // cannot do anything, even close. We are thus free to
                // use just a store to set the flags.
                hatch.flags.store(flags, Ordering::Relaxed);
                forget(self); // Disable our destructor.
            }
            Err(SendError::full(value))
        }
    }
}

#[cfg(feature="async")]
impl<'a, T, H: HatchRef<T>> Future for Sending<'a, T, H> {
    type Output = Result<(), SendError<T>>;
    fn poll(self: Pin<&mut Self>, ctx: &mut Context) -> Poll<Self::Output> {
        let this = self.project();
        let value = this.value.take().unwrap();
        // Check for life.
        if this.sender.flags.is_sender_closed() { return Poll::Ready(Err(SendError::closed(value))); }
        // Take a lock so we can store a value.
        let flags = this.sender.lock(Ordering::Acquire);
        if flags.is_receiver_closed() { return Poll::Ready(Err(SendError::closed(value))); }
        let hatch = unsafe { H::hatch_ref(this.sender.hatch) };
        let shared = unsafe { &mut *hatch.inner.get() };
        if shared.value.is_none() {
            shared.value = Some(value);
            let receiver = shared.receiver.take();
            // release the lock and maybe close
            let flags = this.flags.maybe_close_hatch(flags);
            hatch.flags.store(flags, Ordering::Release);
            this.sender.flags.maybe_close(this.flags);
            // And since we delivered, we should wake the receiver.
            if let Some(waker) = receiver { waker.wake(); }
            Poll::Ready(Ok(()))
        } else {
            // set a waker and release the lock
            let _waker = shared.sender.replace(ctx.waker().clone());
            hatch.flags.store(flags, Ordering::Release);
            // put the value back for the next poll
            this.value.replace(value);
            Poll::Pending
        }
    }
}

#[cfg(all(feature="async", not(feature="messy")))]
impl<'a, T, H: HatchRef<T>> Drop for Sending<'a, T, H> {
    // Clean out the waker to avoid a spurious wake. This would not
    // pose a correctness problem, but we strongly suspect this drop
    // method is faster than your executor.
    fn drop(&mut self) {
        // If we are closed or not waiting, we don't have to clean up
        if !self.flags.is_waiting() { return; }
        if self.sender.flags.is_sender_closed() { return; }
        // Take the lock so we can use the mutable state.
        let flags = self.sender.lock(Ordering::Acquire);
        // If they closed in the meantime, we needn't clean up.
        if !flags.is_receiver_closed() {
            let hatch = unsafe { H::hatch_ref(self.sender.hatch) };
            let shared = unsafe { &mut *hatch.inner.get() };
            let _delay_drop = shared.sender.take();  // Remove our waker.
            hatch.flags.store(flags, Ordering::Release); // Release the lock.
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
/// #[cfg(feature="alloc")] {
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
/// }
/// ```
#[cfg(feature="async")]
pub struct Wait<'a, T, H: HatchRef<T>> {
    sender: &'a mut Sender<T, H>,
    flags:  Flags,
}

#[cfg(feature="async")]
impl<'a, T, H: HatchRef<T>> UnwindSafe for Wait<'a, T, H> {}

#[cfg(feature="async")]
impl<'a, T, H: HatchRef<T>> Wait<'a, T, H> {
    // pin-project-lite does not let us define a Drop impl
    // https://github.com/taiki-e/pin-project-lite/issues/62#issuecomment-884188885
    unsafe fn project(self: Pin<&mut Self>) -> &mut Self {
        Pin::get_unchecked_mut(self)
    }
}

#[cfg(all(feature="async", not(feature="messy")))]
impl<'a, T, H: HatchRef<T>> Drop for Wait<'a, T, H> {
    fn drop(&mut self) {
        // If we are closed or not waiting, we don't have to clean up
        if !self.flags.is_waiting() { return; }
        if self.sender.flags.is_sender_closed() { return; }
        // Take a lock as we need mutable access
        let flags = self.sender.lock(Ordering::Acquire);
        // If they closed in the meantime, we needn't clean up.
        if !flags.is_receiver_closed() {
            let hatch = unsafe { H::hatch_ref(self.sender.hatch) };
            let shared = unsafe { &mut *hatch.inner.get() };
            let _delay_drop = shared.sender.take();  // Remove our waker.
            hatch.flags.store(flags, Ordering::Release); // Release the lock.
        }
    }
}

#[cfg(feature="async")]
impl<'a, T, H: HatchRef<T>> Future for Wait<'a, T, H> {
    type Output = Result<(), Closed>;
    fn poll(self: Pin<&mut Self>, ctx: &mut Context) -> Poll<Self::Output> {
        // Safe because we won't move out of self.
        let this = unsafe { self.project() };
        // If either side is closed, nothing to do.
        if this.sender.flags.is_sender_closed() { return Poll::Ready(Err(Closed)); }
        if this.sender.flags.is_receiver_closed() { return Poll::Ready(Err(Closed)); }
        // Lock so that we may use the mutable storage. Acquire: our waker.
        let flags = this.sender.lock(Ordering::Acquire);
        // If they closed, there is nothing to do.
        if flags.is_receiver_closed() { return Poll::Ready(Err(Closed)); }
        let hatch = unsafe { H::hatch_ref(this.sender.hatch) };
        let shared = unsafe { &mut *hatch.inner.get() };
        if shared.receiver.is_some() && shared.value.is_none() {
            // Woohoo! Take the sender and release the lock.
            let _delay_drop = shared.sender.take(); // No need to wait.
            hatch.flags.store(flags, Ordering::Release);
            this.flags.set_waiting(false); // Unset the flag
            Poll::Ready(Ok(()))
        } else {
            // Set a waker and unlock.
            let _delay_drop = shared.sender.replace(ctx.waker().clone());
            hatch.flags.store(flags, Ordering::Release);
            this.flags.set_waiting(true);
            Poll::Pending
        }
    }
}
