use core::cell::UnsafeCell;
// use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicUsize, Ordering::{Acquire, AcqRel}};
use core::task::Waker;

#[derive(Debug)]
pub struct Inner<T> {
    state: AtomicUsize,
    value: UnsafeCell<Option<T>>,
    send: UnsafeCell<Option<Waker>>,
    recv: UnsafeCell<Option<Waker>>,
}

const CLOSED: usize = 0b1000;
const SEND: usize   = 0b0100;
const RECV: usize   = 0b0010;
const READY: usize  = 0b0001;

impl<T> Inner<T> {

    pub fn new() -> Self {
        Inner {
            state: AtomicUsize::new(0),
            value: UnsafeCell::new(None),
            // send: UnsafeCell::new(MaybeUninit::uninit()),
            // recv: UnsafeCell::new(MaybeUninit::uninit()),
            send: UnsafeCell::new(None),
            recv: UnsafeCell::new(None),
        }
    }

    pub fn state(&self) -> State { State(self.state.load(Acquire)) }

    pub fn set_send(&self, waker: Option<Waker>) -> State {
        // let send = self.send.get();
        // unsafe { *((&mut *send).as_mut_ptr()) = waker; }
        unsafe { *self.send.get() = waker; }
        State(self.state.fetch_or(SEND, AcqRel))
    }

    pub fn set_recv(&self, waker: Option<Waker>) -> State {
        // let recv = self.recv.get();
        // unsafe { *((&mut *recv).as_mut_ptr()) = waker; }
        unsafe { *self.recv.get() = waker; }
        State(self.state.fetch_or(RECV, AcqRel))
    }

    pub fn set_value(&self, value: T) -> State {
        unsafe { *self.value.get() = Some(value); }
        State(self.state.fetch_or(READY, AcqRel))
    }

    pub fn take_value(&self) -> Option<T> {
        unsafe { &mut *self.value.get() }.take()
    }

    pub fn recv(&self) -> Option<Waker> { // if this were public, this method would be unsafe
        unsafe { (*self.recv.get()).take() }
        // unsafe { &*(*self.recv.get()).as_ptr() }
    }

    pub fn send(&self) -> Option<Waker> { // if this were public, this method would be unsafe
        unsafe { (*self.send.get()).take() }
        // unsafe { &*(*self.send.get()).as_ptr() }
    }

    pub fn close(&self) -> State {
        State(self.state.fetch_or(CLOSED, AcqRel))
    }

}

unsafe impl<T: Send> Send for Inner<T> {}
unsafe impl<T: Sync> Sync for Inner<T> {}

pub struct State(usize);

impl State {

    pub fn is_closed(&self) -> bool { (self.0 & CLOSED) == CLOSED }

    pub fn is_ready(&self) -> bool { (self.0 & READY) == READY }

    pub fn is_send(&self)  -> bool { (self.0 & SEND) == SEND }

    pub fn is_recv(&self)  -> bool { (self.0 & RECV) == RECV }

}    
