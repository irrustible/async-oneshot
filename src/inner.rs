use core::cell::UnsafeCell;
use core::mem::{ManuallyDrop, MaybeUninit};
use core::sync::atomic::{AtomicUsize, Ordering::{Acquire, AcqRel}};
use core::task::Waker;

#[derive(Debug)]
pub struct Inner<T> {
    // This one is easy.
    state: AtomicUsize,
    // This is where it all starts to go a bit wrong.
    value: UnsafeCell<ManuallyDrop<MaybeUninit<T>>>,
    // Yes, these are subtly different from the last just to confuse you.
    send: ManuallyDrop<UnsafeCell<MaybeUninit<Waker>>>,
    recv: ManuallyDrop<UnsafeCell<MaybeUninit<Waker>>>,
}

const CLOSED: usize = 0b1000;
const SEND: usize   = 0b0100;
const RECV: usize   = 0b0010;
const READY: usize  = 0b0001;

impl<T> Inner<T> {

    pub fn new() -> Self {
        Inner {
            state: AtomicUsize::new(0),
            value: UnsafeCell::new(ManuallyDrop::new(MaybeUninit::uninit())),
            send: ManuallyDrop::new(UnsafeCell::new(MaybeUninit::uninit())),
            recv: ManuallyDrop::new(UnsafeCell::new(MaybeUninit::uninit())),
        }
    }

    // Gets the current state
    pub fn state(&self) -> State { State(self.state.load(Acquire)) }

    // Gets the receiver's waker. You *must* check the state to ensure
    // it is set. This would be unsafe if it were public.
    pub fn recv(&self) -> &Waker { // MUST BE SET
        unsafe { &*(*self.recv.get()).as_ptr() }
    }

    // Sets the receiver's waker.
    pub fn set_recv(&self, waker: Waker) -> State {
        let recv = self.recv.get();
        unsafe { (*recv).as_mut_ptr().write(waker) } // !
        State(self.state.fetch_or(RECV, AcqRel))
    }

    // Gets the sender's waker. You *must* check the state to ensure
    // it is set. This would be unsafe if it were public.
    pub fn send(&self) -> &Waker {
        unsafe { &*(*self.send.get()).as_ptr() }
    }

    // Sets the sender's waker.
    pub fn set_send(&self, waker: Waker) -> State {
        let send = self.send.get();
        unsafe { (*send).as_mut_ptr().write(waker) } // !
        State(self.state.fetch_or(SEND, AcqRel))
    }

    pub fn take_value(&self) -> T { // MUST BE SET
        unsafe { ManuallyDrop::take(&mut *self.value.get()).assume_init() }
    }

    pub fn set_value(&self, value: T) -> State {
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
            unsafe { ManuallyDrop::take(&mut self.recv).into_inner().assume_init(); }
        }
        if state.send() {
            unsafe { ManuallyDrop::take(&mut self.send).into_inner().assume_init(); }
        }
    }
}

unsafe impl<T: Send> Send for Inner<T> {}
unsafe impl<T: Sync> Sync for Inner<T> {}

pub struct State(usize);

impl State {

    pub fn closed(&self) -> bool { (self.0 & CLOSED) == CLOSED }

    pub fn ready(&self)  -> bool { (self.0 & READY) == READY }

    pub fn send(&self)   -> bool { (self.0 & SEND) == SEND }

    pub fn recv(&self)   -> bool { (self.0 & RECV) == RECV }

}    
