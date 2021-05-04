use crate::*;
use core::cell::UnsafeCell;
use core::ptr;
use core::sync::atomic::{AtomicU8, Ordering::{Acquire, AcqRel}};

/// This is the internal object, shared by the `Sender` and `Receiver`
/// that actually does the work.
#[derive(Debug)]
pub struct Channel<T> {
    /// Our shared synchronisation bitset.
    state: AtomicU8,

    /// The sent value, if any.
    value:    UnsafeCell<Option<T>>,

    #[cfg(feature="async")] // #[cfg(any(feature="async",feature="parking"))]
    /// The sender's `Waker`, if any
    sender:   UnsafeCell<Option<Waker>>,

    #[cfg(feature="async")] // #[cfg(any(feature="async",feature="parking"))]
    /// The receiver's `Waker`, if any
    receiver: UnsafeCell<Option<Waker>>,
}

impl<T> Channel<T> {
    #[inline(always)]
    /// Creates a new Channel for use by the Sender and Receiver
    pub fn new() -> Self {
        Channel {
            state:    AtomicU8::new(0),
            value:    UnsafeCell::new(None),
            sender:   UnsafeCell::new(None),
            receiver: UnsafeCell::new(None),
        }
    }

    // this one might happen for no-alloc no-std
    // #[inline(always)]
    // pub unsafe fn reset(&mut self) {
    //     self.state.store(0, Release);
    // }
}

// We're Send if our messages are. We're *not* Sync.
unsafe impl<T: Send> Send for Channel<T> {}

impl<T> Channel<T> {

    /// Returns the current flag bit mask.
    #[inline(always)]
    pub(crate) fn read(&self) -> u8 { self.state.load(Acquire) }

    /// Atomically ORs the provided flags into the state.
    ///
    /// The unsafety possibilities here are numerous because you are
    /// modifying potentially all of the flags, but minimally:
    ///
    /// * You promise to check if the other half has set the dropped flag.
    ///   * If so, you must drop the channel.
    ///   * If not, and you set the dropped flag, you must not touch
    ///     the channel again
    /// * If setting a lock, you promise to check the other half did not have it
    ///   * If so, you must either spinloop or give up
    ///   * Otherwise, you promise to unlock very quickly.
    #[inline(always)]
    pub(crate) unsafe fn set<F: Into<Flags>>(&self, flags: F) -> u8 {
        self.state.fetch_or(flags.into().into(), AcqRel)
    }

    /// Atomically ANDs the provided flags into the state.
    ///
    /// The unsafety possibilities here are numerous because you are
    /// modifying potentially all of the flags, but minimally:
    ///
    /// * You promise to check if the other half has set the dropped flag.
    /// * If so, you must drop the channel.
    #[inline(always)]
    pub(crate) unsafe fn unset<F: Into<Flags>>(&self, flags: F) -> u8 {
        self.state.fetch_and(flags.into().into(), AcqRel)
    }

    /// Atomically XORs the provided flags into the state.
    ///
    /// The unsafety possibilities here are numerous because you are
    /// modifying potentially all of the flags, but minimally:
    ///
    /// * You promise to check if the other half has set the dropped flag.
    ///   * If so, you must drop the channel.
    ///   * If not, and you set the dropped flag, you must not touch
    ///     the channel again.
    /// * If setting a lock, you promise to check the other half did not have it
    ///   * If so, you must either spinloop or give up.
    ///   * Otherwise, you promise to unlock very quickly.
    #[inline(always)]
    pub(crate) unsafe fn flip<F: Into<Flags>>(&self, flags: F) -> u8 {
        self.state.fetch_xor(flags.into().into(), AcqRel)
    }

    /// Takes the value from the channel, if there is one.
    ///
    /// You promise to be the Receiver and to have seen the
    /// SenderDropped flag set.
    #[inline(always)]
    pub(crate) unsafe fn value(&self) -> Option<T> {
        (*self.value.get()).take()
    }

    /// Put a value in to the channel, returning the old one if there was one.
    ///
    /// You promise to be the Sender and to have not yet set SenderDropped.
    #[inline(always)]
    pub(crate) unsafe fn set_value(&self, value: T) -> Option<T> {
        (*self.value.get()).replace(value)
    }

    /// Takes the Receiver Waker, if any.
    ///
    /// You promise to have taken the lock.
    #[cfg(feature="async")] // #[cfg(any(feature="async",feature="parking"))]
    #[inline(always)]
    pub(crate) unsafe fn receiver_waiting(&self) -> Option<Waker> {
        (*self.receiver.get()).take()
    }

    /// Stores the Receiver Waker, returning the old one, if any.
    ///
    /// You promise to have taken the lock.
    #[cfg(feature="async")] // #[cfg(any(feature="async",feature="parking"))]
    #[inline(always)]
    pub(crate) unsafe fn set_receiver_waiting(&self, waker: Waker) -> Option<Waker> {
        (*self.receiver.get()).replace(waker)
    }

    /// Takes the Sender Waker, if any.
    ///
    /// You promise to have taken the lock.
    #[cfg(feature="async")] // #[cfg(any(feature="async",feature="parking"))]
    #[inline(always)]
    pub(crate) unsafe fn sender_waiting(&self) -> Option<Waker> {
        (*self.sender.get()).take()
    }

    /// Stores the Sender Waker, returning the old one, if any.
    ///
    /// You promise to have taken the lock.
    #[cfg(feature="async")] // #[cfg(any(feature="async",feature="parking"))]
    #[inline(always)]
    pub(crate) unsafe fn set_sender_waiting(&self, waker         : Waker) -> Option<Waker> {
        (*self.sender.get()).replace(waker)
    }
}

impl<T> Drop for Channel<T> {
    #[inline(always)]
    fn drop(&mut self) {
        unsafe {
            ptr::drop_in_place(self.value.get());
            #[cfg(feature="async")] // #[cfg(any(feature="async",feature="parking"))]
            ptr::drop_in_place(self.receiver.get());
            #[cfg(feature="async")] // #[cfg(any(feature="async",feature="parking"))]
            ptr::drop_in_place(self.sender.get());
        }
    }
}
