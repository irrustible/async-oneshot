use crate::*;

use core::cell::UnsafeCell;
use core::mem;
use core::panic::{RefUnwindSafe, UnwindSafe};
#[cfg(feature="alloc")]
use core::ptr::NonNull;

pub(crate) mod flags;
use flags::*;

#[cfg(feature="alloc")]
use alloc::boxed::Box;

#[cfg(feature="async")]
use core::task::Waker;

#[cfg(not(feature="disable_spin_loop_hint"))]
use core::hint::spin_loop;
#[cfg(feature="disable_spin_loop_hint")]
#[inline(always)] fn spin_loop() {}

mod sealed {
    pub trait HatchRef {}
}

/// An abstraction over storage that allows either a box or a ref to
/// be used.
///
/// This trait is sealed, you may not implement it for your own types.
pub trait HatchRef<T> : sealed::HatchRef {
    type Stored : Copy;
    /// Returns a shared reference to the hatch
    ///
    /// # Safety
    ///
    /// There must be no possibility of a mut ref being created to the
    /// hatch while the returned ref exists. It should also live as
    /// long as the declared lifetime.
    unsafe fn hatch_ref<'a>(stored: Self::Stored) -> &'a Hatch<T>;

    /// Frees the memory associated with hatch, or otherwise cleans up.
    ///
    /// # Safety
    ///
    /// There must be no possibility of a mut ref being created to the
    /// hatch while the returned ref exists. It should also live as
    /// long as the declared lifetime.
    unsafe fn free(stored: Self::Stored, mark: bool);
}

#[cfg(feature="alloc")]
impl<T> sealed::HatchRef for Box<Hatch<T>> {}

#[cfg(feature="alloc")]
impl<T> HatchRef<T> for Box<Hatch<T>> {
    type Stored = NonNull<Hatch<T>>;
    unsafe fn hatch_ref<'a>(stored: Self::Stored) -> &'a Hatch<T> { stored.as_ref() }
    unsafe fn free(stored: Self::Stored, _mark: bool) { Box::from_raw(stored.as_ptr()); }
}

impl<'a, T> sealed::HatchRef for &'a Hatch<T> {}

impl<'a, T> HatchRef<T> for &'a Hatch<T> {
    type Stored = Self;
    unsafe fn hatch_ref<'b>(stored: Self::Stored) -> &'b Hatch<T> {
        // Ugh, we aren't allowed to place extra restrictions on 'b,
        // but it's required by the trait. yummy.
        (stored as *const Hatch<T>).as_ref().unwrap()
    }
    unsafe fn free(stored: Self::Stored, mark: bool) {
        // If mark on drop is enabled, we must clean it up and mark it
        // as reclaimable.
        if mark {
            mem::take(&mut *stored.inner.get());
            stored.flags.store(HatchFlags::reclaimable(), Ordering::Release)
        }
    }
}

/// A unit error indicating the hatch is closed.
#[derive(Debug,Eq,PartialEq)]
pub struct Closed;

#[derive(Debug,Eq,PartialEq)]
pub enum RecoverError {
    /// The other side is still alive.
    Live,
    /// We no longer have a handle to the hatch.
    Closed,
}

/// The shared internal hatch object. You probably want [`hatch`]
/// unless you are managing memory yourself.
#[derive(Debug)]
pub struct Hatch<T> {
    pub(crate) flags: AtomicHatchFlags,
    pub(crate) inner: UnsafeCell<Shared<T>>,
}

impl<T> RefUnwindSafe for Hatch<T> {}
impl<T> UnwindSafe for Hatch<T> {}

impl<T> Hatch<T> {
    /// Takes the lock, returning the pre-modification flags when
    /// either when the lock has been taken or a closed flag was seen.
    #[inline(always)]
    pub(crate) fn lock(&self, ordering: Ordering) -> HatchFlags {
        let lock = HatchFlags::default().lock(true);
        loop {
            let flags = self.flags.fetch_or(lock, ordering);
            // We hope that this is obvious enough for llvm to rewrite
            // it to the single comparison it used to be. This is
            // certainly more readable...
            if flags.is_receiver_closed() { return flags; }
            if flags.is_sender_closed() { return flags; }
            if !flags.is_lock() { return flags; }
            spin_loop();
        }
    }

    /// Resets the hatch so it can be reused. Checks the atomic to
    /// ensure the hatch is marked for reclamation. Returns true on
    /// success.
    pub fn reclaim(&self) -> bool {
        let should = self.flags.load(Ordering::Acquire).is_reclaimable();
        // Safe because it's no longer in use.
        if should { unsafe { self.reclaim_unchecked(); } }
        should
    }

    /// Like [`Hatch::reclaim`], but does not check the atomic.
    ///
    /// # Safety
    ///
    /// Safe if the Sender and Receiver have both closed.
    #[inline(always)]
    pub unsafe fn reclaim_unchecked(&self) {
        let _delay_drop = mem::take(self.inner.get().as_mut().unwrap());
        self.flags.store(HatchFlags::default(), Ordering::Release)
    }
}

impl<T> Default for Hatch<T> {
    #[inline(always)]
    fn default() -> Self {
        Self { flags: AtomicHatchFlags::default(), inner: UnsafeCell::new(Shared::default()) }
    }
}

/// Mutually exclusive mutable storage for the channel.
#[derive(Debug)]
pub(crate) struct Shared<T> {
    #[cfg(feature="async")]
    pub(crate) sender:   Option<Waker>,
    #[cfg(feature="async")]
    pub(crate) receiver: Option<Waker>,
    pub(crate) value:    Option<T>,
}

// To avoid T: Default
impl<T> Default for Shared<T> {
    fn default() -> Self {
        Self {
            #[cfg(feature="async")] sender: None,
            #[cfg(feature="async")] receiver: None,
            value: None,
        }
    }
}

