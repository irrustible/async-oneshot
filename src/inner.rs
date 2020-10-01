use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::ptr::drop_in_place;
use core::sync::atomic::{AtomicUsize, Ordering::{Acquire, AcqRel}};
use core::task::Waker;

#[derive(Debug)]
pub struct Inner<T> {
    // This one is easy.
    state: AtomicUsize,
    // This is where it all starts to go a bit wrong.
    value: UnsafeCell<MaybeUninit<T>>,
    // Yes, these are subtly different from the last just to confuse you.
    send: UnsafeCell<MaybeUninit<Waker>>,
    recv: UnsafeCell<MaybeUninit<Waker>>,
}

const CLOSED: usize = 0b1000;
const SEND: usize   = 0b0100;
const RECV: usize   = 0b0010;
const READY: usize  = 0b0001;

impl<T> Inner<T> {
    pub fn new() -> Self {
        Inner {
            state: AtomicUsize::new(0),
            value: UnsafeCell::new(MaybeUninit::uninit()),
            send: UnsafeCell::new(MaybeUninit::uninit()),
            recv: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }

    // Gets the current state
    pub fn state(&self) -> State { State(self.state.load(Acquire)) }

    // Gets the receiver's waker. You *must* check the state to ensure
    // it is set. This would be unsafe if it were public.
    pub fn recv(&self) -> &Waker { // MUST BE SET
        debug_assert!(self.state().recv());
        unsafe { &*(*self.recv.get()).as_ptr() }
    }

    // Sets the receiver's waker.
    pub fn set_recv(&self, waker: Waker) -> State {
        debug_assert!(!self.state().recv());
        let recv = self.recv.get();
        unsafe { (*recv).as_mut_ptr().write(waker) } // !
        State(self.state.fetch_or(RECV, AcqRel))
    }

    // Gets the sender's waker. You *must* check the state to ensure
    // it is set. This would be unsafe if it were public.
    pub fn send(&self) -> &Waker {
        debug_assert!(self.state().send());
        unsafe { &*(*self.send.get()).as_ptr() }
    }

    // Sets the sender's waker.
    pub fn set_send(&self, waker: Waker) -> State {
        debug_assert!(!self.state().send());
        let send = self.send.get();
        unsafe { (*send).as_mut_ptr().write(waker) } // !
        State(self.state.fetch_or(SEND, AcqRel))
    }

    pub fn take_value(&self) -> T { // MUST BE SET
        debug_assert!(self.state().ready());
        unsafe { (*self.value.get()).as_ptr().read() }
    }

    pub fn set_value(&self, value: T) -> State {
        debug_assert!(!self.state().ready());
        let val = self.value.get();
        unsafe { (*val).as_mut_ptr().write(value) }
        State(self.state.fetch_or(READY, AcqRel))
    }

    pub fn close(&self) -> State {
        State(self.state.fetch_or(CLOSED, AcqRel))
    }
}

impl<T> Drop for Inner<T> {
    fn drop(&mut self) {
        let state = State(*self.state.get_mut());
        // Drop the wakers if they are present
        if state.recv() {
            unsafe { drop_in_place((&mut *self.recv.get()).as_mut_ptr()); }
        }
        if state.send() {
            unsafe { drop_in_place((&mut *self.send.get()).as_mut_ptr()); }
        }
    }
}

unsafe impl<T: Send> Send for Inner<T> {}
unsafe impl<T: Sync> Sync for Inner<T> {}

#[derive(Clone, Copy)]
pub struct State(usize);

impl State {
    pub fn closed(&self) -> bool { (self.0 & CLOSED) == CLOSED }
    pub fn ready(&self)  -> bool { (self.0 & READY ) == READY  }
    pub fn send(&self)   -> bool { (self.0 & SEND  ) == SEND   }
    pub fn recv(&self)   -> bool { (self.0 & RECV  ) == RECV   }
}
