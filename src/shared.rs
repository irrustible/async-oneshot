use core::cell::UnsafeCell;
use core::ops::Deref;
use core::ptr::NonNull;

use core::sync::atomic::AtomicUsize;

#[cfg(feature="alloc")]
use alloc::boxed::Box;

#[cfg(feature="async")]
use core::task::Waker;

#[cfg(not(feature="disable_spin_loop_hint"))]
use core::hint::spin_loop;
#[cfg(feature="disable_spin_loop_hint")]
#[inline(always)] fn spin_loop() {}

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
    pub(crate) flags: AtomicFlags,
    pub(crate) inner: UnsafeCell<Shared<T>>,
}

impl<T> Hatch<T> {
    /// Takes the lock, returning the pre-modification flags when
    /// either when the lock has been taken or a closed flag was seen.
    #[inline(always)]
    pub(crate) fn lock(&self) -> Flags {
        loop {
            let flags = self.flags.fetch_or(LOCK, Ordering::Acquire);
            if LOCK != flags & (LOCK | R_CLOSE | S_CLOSE) { return flags }
            spin_loop();
        }
    }

    /// Resets the hatch so it can be reused. Checks the stamp to
    /// ensure the hatch is marked for reclamation. Returns true on success.
    pub fn reclaim(&self) -> bool {
        let should = RECLAIMABLE == self.flags.load(Ordering::Acquire);
        // Safe because it's no longer in use.
        if should { unsafe { self.reclaim_unchecked(); } }
        should        
    }

    /// Like [`reclaim`], but does not check the stamp, eliding an
    /// atomic load.
    ///
    /// # Safety
    ///
    /// Safe if the Sender and Receiver have both closed.
    #[inline(always)]
    pub unsafe fn reclaim_unchecked(&self) {
        self.inner.get().as_mut().unwrap().reset();
        self.flags.store(0, Ordering::Release)
    }
}

impl<T> Default for Hatch<T> {
    fn default() -> Self {
        Self { flags: AtomicFlags::new(0), inner: UnsafeCell::new(Shared::default()) }
    }
}

/// This object represents some sort of link to the hatch. One each is
/// kept by [`Sender`] and [`Receiver`].
///
/// The purpose is to continue to allow Box-based hatches to work
/// while adding options for users without an allocator - to pass by
/// reference or NonNull.
///
/// Use of this structure is not unsafe by itself as it will never
/// call a `Drop` impl. This also means if you're using it with boxes,
/// it will leak by default!
#[derive(Debug)]
pub enum Holder<'a, T> {
    // Lifetime-bound reference.
    Ref(&'a T),
    // A pointer produced from [`Box::leak`] that's potentially
    // shared with other holders.
    #[cfg(feature="alloc")]
    SharedBoxPtr(NonNull<T>),
}

impl<'a, T> Holder<'a, Hatch<T>> {

    // Safe only if we are the last referent to the hatch
    pub(crate) unsafe fn cleanup(self, mark: bool) {
        #[cfg(feature="alloc")]
        if let Self::SharedBoxPtr(p) = self {
            // For a box, we just recreate it and drop it.
            Box::from_raw(p.as_ptr());
            return;
        }
        if mark {
            // To reclaim a borrowed hatch, we clean the state and set
            // the flags to our sentinel 'reclaimable' value.
            (*self.inner.get()).reset();
            self.flags.store(MARK_ON_DROP, Ordering::Release)
        }
        // Otherwise there's nothing to do
    }

    // Safe only if we are the last active referent to the hatch
    pub(crate) unsafe fn recycle(self) {
        (*self.inner.get()).reset();
        self.flags.store(0, Ordering::Release);
    }
}

impl<'a, T> Clone for Holder<'a, T> {
    fn clone(&self) -> Self {
        match self {
            Holder::Ref(r) => Holder::Ref(r),
            #[cfg(feature="alloc")]
            Holder::SharedBoxPtr(r) => Holder::SharedBoxPtr(*r),
        }
    }
}

impl<'a, T> Copy for Holder<'a, T> {}

impl<'a, T> Deref for Holder<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        match self {
            Holder::Ref(b) => b,
            #[cfg(feature="alloc")]
            Holder::SharedBoxPtr(ptr) => unsafe { ptr.as_ref() },
        }
    }
}

/// Mutually exclusive Storage for the channel.
#[derive(Debug)]
pub struct Shared<T> {
    #[cfg(feature="async")]
    pub sender:   Option<Waker>,
    #[cfg(feature="async")]
    pub receiver: Option<Waker>,
    pub value:    Option<T>,
}

impl<T> Shared<T> {
    // Takes all values from the options
    fn reset(&mut self) {
        #[cfg(feature="async")] {
            self.receiver.take();
            self.sender.take();
        }
        self.value.take();
    }
}

impl<T> Default for Shared<T> {
    fn default() -> Self {
        Self {
            #[cfg(feature="async")] sender: None,
            #[cfg(feature="async")] receiver: None,
            value: None,
        }
    }
}

pub type AtomicFlags = AtomicUsize;
pub type Flags = usize;

//// Atomic flags ////

/// This magic value is used to notify an external memory manager that both sides have
/// closed and the hatch may be reclaimed.
pub const RECLAIMABLE: Flags = usize::MAX;

/// Exclusive access to the shared data.
pub const LOCK:    Flags = 1;
/// Receiver has closed.
pub const R_CLOSE: Flags = 1 << 1;
/// Sender has closed.
pub const S_CLOSE: Flags = 1 << 2;

//// Sender/Receiver local flags ////

/// Whether we should close the hatch during the next successful send/receive operation.
pub const CLOSE_ON_SUCCESS: Flags = 1 << 3;

/// Whether we should mark the hatch as ready for reuse when we clean it up. Allows
/// external memory management to reclaim closed hatches.
///
/// Has no effect if the hatch is backed by a box.
pub const MARK_ON_DROP: Flags = 1 << 4;

/// (Sender) Whether a message can be overwritten
pub const OVERWRITE: Flags = 1 << 5;

/// When set, we know we have set a waker. It doesn't mean it's definitely still there,
/// but it *might* be and thus we have some interest in cleaning it up.
#[cfg(feature="async")]
pub const WAITING: Flags = 1 << 6;

/// By default, we don't set any options
pub const DEFAULT: Flags = 0;

#[inline(always)]
/// Returns R_CLOSE if CLOSE_ON_SUCCESS is set, branchlessly.
pub fn r_closes(flags: Flags) -> Flags { (flags & CLOSE_ON_SUCCESS) >> 2 }

#[inline(always)]
/// Returns S_CLOSE if CLOSE_ON_SUCCESS is set, branchlessly.
pub fn s_closes(flags: Flags) -> Flags { (flags & CLOSE_ON_SUCCESS) >> 1 }

#[inline(always)]
pub fn toggle_flag(haystack: Flags, needle: Flags, on: bool) -> Flags {
    haystack ^ (needle * on as Flags)
}

#[inline(always)]
pub fn any_flag(haystack: Flags, needle: Flags) -> bool {
    (haystack & needle) != 0
}

#[inline(always)]
#[cfg(feature="async")]
pub fn no_flag(haystack: Flags, needle: Flags) -> bool {
    (haystack & needle) == 0
}

// #[inline(always)]
// pub fn all_flags(haystack: Flags, needle: Flags) -> bool {
//     (haystack & needle) == needle
// }
