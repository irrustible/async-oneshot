use crate::mutex::{Mutex, MutexGuard};
use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicUsize, Ordering};
use core::task::Waker;

const SEND_LOCKED_BIT: usize = 0;
const SEND_PRESENT_BIT: usize = 1;
const RECV_LOCKED_BIT: usize = 2;
const RECV_PRESENT_BIT: usize = 3;
const VALUE_PRESENT_BIT: usize = 4;
const CLOSED_BIT: usize = 5;

/// State of the value after taking it.
pub(crate) enum InnerValue<T> {
    Present(T),
    Pending,
    Closed,
}

#[derive(Debug)]
pub(crate) struct Inner<T> {
    // Carries the state of the mutexes and value.
    state: AtomicUsize,

    // Waker for sender and receiver.
    send: Mutex<Waker, SEND_LOCKED_BIT, SEND_PRESENT_BIT>,
    recv: Mutex<Waker, RECV_LOCKED_BIT, RECV_PRESENT_BIT>,

    // Value of the channel (present if VALUE_PRESENT_BIT is set)
    value: UnsafeCell<MaybeUninit<T>>,
}

impl<T> Inner<T> {
    #[inline(always)]
    pub(crate) fn new() -> Self {
        Inner {
            state: AtomicUsize::new(0),
            send: Mutex::new(),
            recv: Mutex::new(),
            value: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }

    /// Attempts to take the value from the channel.
    pub fn try_take(&self) -> InnerValue<T> {
        // Load the state and clear the present bit
        let state_snapshot = self
            .state
            .fetch_and(!(1 << VALUE_PRESENT_BIT), Ordering::Acquire);

        if state_snapshot & (1 << VALUE_PRESENT_BIT) == 0 {
            if self.state.load(Ordering::Acquire) & (1 << CLOSED_BIT) != 0 {
                // Closed bit is set
                InnerValue::Closed
            } else {
                InnerValue::Pending
            }
        } else {
            // SAFETY: We just checked that the value is present and cleared the present bit.
            InnerValue::Present(unsafe { (&mut *self.value.get()).assume_init_read() })
        }
    }

    /// Sets the value of the channel.
    pub fn emplace_value(&self, value: T) {
        // Assert that the value is not present yet.
        debug_assert!(self.state.load(Ordering::Acquire) & (1 << VALUE_PRESENT_BIT) == 0);

        // This could leak if this method is ever called twice - its the responsibility of the
        // sender to ensure that this is not the case.
        unsafe { (&mut *self.value.get()).write(value) };
        self.state
            .fetch_or(1 << VALUE_PRESENT_BIT, Ordering::Release);
    }

    pub fn lock_send(&self) -> MutexGuard<'_, Waker, SEND_LOCKED_BIT, SEND_PRESENT_BIT> {
        // SAFETY: The state bits are used only by this mutex.
        unsafe { self.send.lock(&self.state) }
    }

    pub fn lock_recv(&self) -> MutexGuard<'_, Waker, RECV_LOCKED_BIT, RECV_PRESENT_BIT> {
        // SAFETY: The state bits are used only by this mutex.
        unsafe { self.recv.lock(&self.state) }
    }

    pub fn mark_closed(&self) {
        self.state.fetch_or(1 << CLOSED_BIT, Ordering::Acquire);
    }

    pub fn is_closed(&self) -> bool {
        self.state.load(Ordering::Acquire) & (1 << CLOSED_BIT) != 0
    }
}

impl<T> Drop for Inner<T> {
    fn drop(&mut self) {
        // Make sure to release drop the mutexes.
        self.send.drop(&self.state);
        self.recv.drop(&self.state);

        // Drop the value if present.
        if self.state.load(Ordering::Acquire) & (1 << VALUE_PRESENT_BIT) != 0 {
            // SAFETY: We just checked that the value is present.
            unsafe { (&mut *self.value.get()).assume_init_drop() };
        }
    }
}

unsafe impl<T: Send> Send for Inner<T> {}
unsafe impl<T: Send> Sync for Inner<T> {}
