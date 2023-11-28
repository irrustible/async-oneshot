//! Tiny implementation of a no-std mutex based on spinlocks.
//!
//! This mutex is special because it stores its locking state
//! in an externally supplied atomic state.

use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicUsize, Ordering};

/// A mutex that can be used in no_std environments and internally is
/// based on spinlocks.
///
/// This mutex stores its state in an externally supplied atomic usize.
#[derive(Debug)]
pub(crate) struct Mutex<T, const PRESENT_BIT: usize, const LOCKED_BIT: usize> {
    /// Actual value of the mutex.
    value: UnsafeCell<MaybeUninit<T>>,
}

impl<T, const PRESENT_BIT: usize, const LOCKED_BIT: usize> Mutex<T, PRESENT_BIT, LOCKED_BIT> {
    /// Creates a new mutex with the given value.
    pub(crate) const fn new() -> Self {
        Mutex {
            value: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }

    /// Locks the mutex and returns a guard that unlocks it when dropped.
    ///
    /// # Safety
    ///
    /// This function is unsafe because it requires the caller to ensure that the same state
    /// is used for all operations on this mutex and that the bits used by this mutex are not
    /// used by any other means.
    pub(crate) unsafe fn lock<'a>(
        &'a self,
        state: &'a AtomicUsize,
    ) -> MutexGuard<'_, T, PRESENT_BIT, LOCKED_BIT> {
        // Try to lock the mutex.
        while state.fetch_or(1 << LOCKED_BIT, Ordering::Acquire) & (1 << LOCKED_BIT) != 0 {
            // If we failed, wait until the mutex is unlocked.
            while state.load(Ordering::Acquire) & (1 << LOCKED_BIT) != 0 {
                core::hint::spin_loop();
            }
        }

        // SAFETY: We just locked the mutex.
        MutexGuard { mutex: self, state }
    }

    /// Needs to be called in order to drop the mutex without leaking the value.
    ///
    /// This can be called multiple times, but only the first call will actually drop the value if
    /// it is present.
    pub(crate) fn drop(&mut self, state: &AtomicUsize) {
        if state.fetch_and(!(1 << PRESENT_BIT), Ordering::Acquire) & (1 << PRESENT_BIT) != 0 {
            // SAFETY: We own a mutable reference to self and the present bit is set.
            unsafe {
                // Value is present, drop in place
                core::ptr::drop_in_place((&mut *self.value.get()).as_mut_ptr());
            }
        }
    }
}

/// A guard for a held mutex.
///
/// NOTE: The code should never panic while holding this guard!
pub(crate) struct MutexGuard<'a, T, const PRESENT_BIT: usize, const LOCKED_BIT: usize> {
    mutex: &'a Mutex<T, PRESENT_BIT, LOCKED_BIT>,
    state: &'a AtomicUsize,
}

impl<'a, T, const PRESENT_BIT: usize, const LOCKED_BIT: usize>
    MutexGuard<'a, T, PRESENT_BIT, LOCKED_BIT>
{
    pub(crate) fn get(&self) -> Option<&T> {
        if self.state.load(Ordering::Acquire) & (1 << PRESENT_BIT) == 0 {
            None
        } else {
            // SAFETY: When the mutex created this guard, it set locked to 1 before
            // and present bit is set.
            Some(unsafe { (&*self.mutex.value.get()).assume_init_ref() })
        }
    }

    pub(crate) fn take(&mut self) -> Option<T> {
        if self.state.fetch_and(!(1 << PRESENT_BIT), Ordering::Acquire) & (1 << PRESENT_BIT) == 0 {
            None
        } else {
            // SAFETY: When the mutex created this guard, it set locked to 1 before and
            // present bit is set.
            Some(unsafe { (&mut *self.mutex.value.get()).assume_init_read() })
        }
    }

    pub(crate) fn emplace(&mut self, value: T) {
        if self.state.fetch_or(1 << PRESENT_BIT, Ordering::Acquire) & (1 << PRESENT_BIT) != 0 {
            // SAFETY: When the mutex created this guard, it set locked to 1 before and present
            // bit is set.
            unsafe {
                // Value is present already, drop in place
                core::ptr::drop_in_place((&mut *self.mutex.value.get()).as_mut_ptr());
            }
        }

        // SAFETY: When the mutex created this guard, it set locked to 1 before.
        unsafe {
            (&mut *self.mutex.value.get()).write(value);
        }
    }
}

impl<'a, T, const PRESENT_BIT: usize, const LOCKED_BIT: usize> Drop
    for MutexGuard<'a, T, PRESENT_BIT, LOCKED_BIT>
{
    fn drop(&mut self) {
        self.state.fetch_and(!(1 << LOCKED_BIT), Ordering::Release);
    }
}
