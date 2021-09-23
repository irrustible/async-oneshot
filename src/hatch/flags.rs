use super::*;
use core::sync::atomic::AtomicU8;

#[derive(Debug,Default)]
pub struct AtomicHatchFlags(AtomicU8);

impl AtomicHatchFlags {
    #[inline(always)]
    pub fn load(&self, ordering: Ordering) -> HatchFlags { 
        HatchFlags(self.0.load(ordering))
    }

    #[inline(always)]
    pub fn store(&self, flags: HatchFlags, ordering: Ordering) {
        self.0.store(flags.0, ordering)
    }

    #[inline(always)]
    pub fn fetch_or(&self, flags: HatchFlags, ordering: Ordering) -> HatchFlags {
        HatchFlags(self.0.fetch_or(flags.0, ordering))
    }

    #[cfg(not(feature="async"))]
    #[inline(always)]
    pub fn fetch_xor(&self, flags: HatchFlags, ordering: Ordering) -> HatchFlags {
        HatchFlags(self.0.fetch_xor(flags.0, ordering))
    }
}

#[repr(transparent)]
#[derive(Clone,Copy,Debug,Default,Eq,PartialEq)]
pub struct HatchFlags(pub(crate) u8);

impl HatchFlags {
    #[inline(always)]
    pub fn reclaimable() -> Self { HatchFlags(RECLAIMABLE) }
    #[inline(always)]
    pub fn is_lock(self) -> bool { any_flag(self.0, LOCK) }
    #[inline(always)]
    pub fn is_receiver_closed(self) -> bool { any_flag(self.0, R_CLOSE) }
    #[inline(always)]
    pub fn is_sender_closed(self) -> bool { any_flag(self.0, S_CLOSE) }
    #[inline(always)]
    pub fn is_reclaimable(self) -> bool { self.0 == RECLAIMABLE }

    #[inline(always)]
    pub fn lock(self, on: bool)            -> Self { Self(toggle_flag(self.0, LOCK, on)) }
    #[inline(always)]
    pub fn receiver_closed(self, on: bool) -> Self { Self(toggle_flag(self.0, R_CLOSE, on)) }
    #[inline(always)]
    pub fn sender_closed(self, on: bool)   -> Self { Self(toggle_flag(self.0, S_CLOSE, on)) }
    #[inline(always)]
    pub fn sender_closed_bit(self)   -> u8 { self.0 & S_CLOSE }
    #[inline(always)]
    pub fn receiver_closed_bit(self) -> u8 { self.0 & R_CLOSE }
}

pub type Flags = u8;

//// Atomic flags ////

/// This magic value is used to notify an external memory manager that both sides have
/// closed and the hatch may be reclaimed.
pub const RECLAIMABLE: Flags = Flags::MAX;

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
