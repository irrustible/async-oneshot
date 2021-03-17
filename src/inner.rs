use core::cell::UnsafeCell;
#[cfg(not(debug_assertions))]
use core::marker::PhantomData;
use core::mem::MaybeUninit;
use core::ptr::drop_in_place;
use core::sync::atomic::{AtomicU8, Ordering::{Acquire, AcqRel}};
use core::task::Waker;

#[derive(Debug)]
pub(crate) struct Inner<T> {
    // This one is easy.
    state: AtomicU8,
    // This is where it all starts to go a bit wrong.
    value: UnsafeCell<MaybeUninit<T>>,
    sender:    UnsafeCell<MaybeUninit<Waker>>,
    receiver:  UnsafeCell<MaybeUninit<Waker>>,
}

const CLOSED: u8 = 0b1000;
const SEND:   u8 = 0b0100;
const RECV:   u8 = 0b0010;
const READY:  u8 = 0b0001;

impl<T> Inner<T> {
    #[inline(always)]
    pub(crate) fn new() -> Self {
        Inner {
            state:    AtomicU8::new(0),
            value:    UnsafeCell::new(MaybeUninit::uninit()),
            sender:   UnsafeCell::new(MaybeUninit::uninit()),
            receiver: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }

    // Gets the current state
    #[inline(always)]
    pub(crate) fn state(&self) -> State<T> {
        self.make_state(self.state.load(Acquire))
    }

    // Gets the receiver's waker.
    #[inline(always)]
    pub(crate) fn receiver_waker(&self, proof: HasReceiverWaker<T>) -> &Waker {
        proof.check(self as *const Inner<T>);
        unsafe { &*(*self.receiver.get()).as_ptr() }
    }

    // Sets the receiver's waker.
    #[inline(always)]
    pub(crate) fn set_receiver_waker(&self, waker: Waker) -> State<T> {
        let recv = self.receiver.get();
        // safe because we are only writing it.
        unsafe { (*recv).as_mut_ptr().write(waker) }
        self.make_state(self.state.fetch_or(RECV, AcqRel))
    }

    // Gets the sender's waker.
    #[inline(always)]
    pub(crate) fn sender_waker(&self, proof: HasSenderWaker<T>) -> &Waker {
        proof.check(self as *const Inner<T>);
        // debug_assert!(self.state().has_sender_waker().is_some());
        unsafe { &*(*self.sender.get()).as_ptr() }
    }

    // Sets the sender's waker.
    #[inline(always)]
    pub(crate) fn set_sender_waker(&self, waker: Waker) -> State<T> {
        let send = self.sender.get();
        // safe because we are only writing it.
        unsafe { (*send).as_mut_ptr().write(waker) }
        self.make_state(self.state.fetch_or(SEND, AcqRel))
    }

    #[inline(always)]
    pub(crate) fn take_value(&self, proof: IsReady<T>) -> T {
        proof.check(self as *const Inner<T>);
        unsafe { (*self.value.get()).as_ptr().read() }
    }

    #[inline(always)]
    pub(crate) unsafe fn unsafe_take_value(&self) -> T {
        debug_assert!(self.state().ready());
        (*self.value.get()).as_ptr().read()
    }

    #[inline(always)]
    pub(crate) fn set_value(&self, value: T) -> State<T> {
        debug_assert!(!self.state().ready());
        let val = self.value.get();
        unsafe { (*val).as_mut_ptr().write(value) }
        self.make_state(self.state.fetch_or(READY, AcqRel))
    }

    #[inline(always)]
    pub(crate) fn close(&self) -> State<T> {
        self.make_state(self.state.fetch_or(CLOSED, AcqRel))
    }

    #[cfg(debug_assertions)]
    #[inline(always)]
    fn make_state(&self, value: u8) -> State<T> {
        State(value, self as *const Inner<T>)
    }

    #[cfg(not(debug_assertions))]
    #[inline(always)]
    fn make_state(&self, value: u8) -> State<T> {
        State(value, PhantomData)
    }
}

impl<T> Drop for Inner<T> {
    #[inline(always)]
    fn drop(&mut self) {
        let st = { *self.state.get_mut() as u8 };
        let state = self.make_state(st);
        // Drop the wakers if they are present
        if let Some(proof) = state.has_receiver_waker() {
            proof.check(self as *const Inner<T>);
            unsafe { drop_in_place((&mut *self.receiver.get()).as_mut_ptr()); }
        }
        if let Some(proof) = state.has_sender_waker() {
            proof.check(self as *const Inner<T>);
            unsafe { drop_in_place(
                (&mut *self.sender.get()).as_mut_ptr()
            ); }
        }
    }
}

unsafe impl<T: Send> Send for Inner<T> {}
unsafe impl<T: Sync> Sync for Inner<T> {}

#[cfg(not(debug_assertions))]
pub(crate) struct State<T>(u8, PhantomData<T>);

#[cfg(debug_assertions)]
pub(crate) struct State<T>(u8, *const Inner<T>);


#[cfg(debug_assertions)]
macro_rules! proof {
    ($name:ident) => {
        pub(crate) struct $name<T>(*const Inner<T>);
        impl<T> $name<T> {
            pub(crate) fn check(self, other: *const Inner<T>) {
                debug_assert!(self.0 ==other);
            }
        }
    }
}

#[cfg(not(debug_assertions))]
macro_rules! proof {
    ($name:ident) => {
        pub(crate) struct $name<T>(PhantomData<T>);
        impl<T> $name<T> {
            #[inline(always)]
            pub(crate) fn check(self, _other: *const Inner<T>) {}
        }
    }
}

proof!(IsOpen);
proof!(IsClosed);
proof!(IsReady);
proof!(IsNotReady);
proof!(HasReceiverWaker);
proof!(HasSenderWaker);

impl<T> State<T> {
    #[inline(always)]
    pub(crate) fn is_open(&self) -> Result<IsOpen<T>, IsClosed<T>> {
        #[cfg(debug_assertions)]
        if self.closed() { Err(IsClosed(self.1)) } else { Ok(IsOpen(self.1)) }
        #[cfg(not(debug_assertions))]
        if self.closed() { Err(IsClosed(PhantomData)) } else { Ok(IsOpen(PhantomData)) }
    }
    #[inline(always)]
    pub(crate) fn is_ready(&self) -> Result<IsReady<T>, IsNotReady<T>> {
        #[cfg(debug_assertions)]
        if self.ready() { Ok(IsReady(self.1)) } else { Err(IsNotReady(self.1)) }
        #[cfg(not(debug_assertions))]
        if self.ready() { Ok(IsReady(PhantomData)) } else { Err(IsNotReady(PhantomData)) }
    }
    #[inline(always)]
    pub(crate) fn has_receiver_waker(&self) -> Option<HasReceiverWaker<T>> {
        #[cfg(debug_assertions)]
        if self.recv() { Some(HasReceiverWaker(self.1))} else { None }
        #[cfg(not(debug_assertions))]
        if self.recv() { Some(HasReceiverWaker(PhantomData))} else { None }
    }
    #[inline(always)]
    pub(crate) fn has_sender_waker(&self) -> Option<HasSenderWaker<T>> {
        #[cfg(debug_assertions)]
        if self.send() { Some(HasSenderWaker(self.1)) } else { None }
        #[cfg(not(debug_assertions))]
        if self.send() { Some(HasSenderWaker(PhantomData)) } else { None }
    }
    #[inline(always)]
    pub(crate) fn closed(&self) -> bool { (self.0 & CLOSED) == CLOSED }
    #[inline(always)]
    pub(crate) fn ready(&self)  -> bool { (self.0 & READY ) == READY  }
    #[inline(always)]
    pub(crate) fn send(&self)   -> bool { (self.0 & SEND  ) == SEND   }
    #[inline(always)]
    pub(crate) fn recv(&self)   -> bool { (self.0 & RECV  ) == RECV   }
}
