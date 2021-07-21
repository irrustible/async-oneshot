//! An easy-to-use, high-performance single-message async channel for
//! a single sender/receiver pair.
// #![no_std]

#[cfg(feature="alloc")]
extern crate alloc;

#[cfg(feature="alloc")]
use alloc::boxed::Box;
use core::ptr::NonNull;

// Things shared between the sender and receiver
mod shared;
pub use shared::{Closed, Hatch};
use shared::*;

pub mod sender;
pub mod receiver;

pub use sender::SendError;

/// This is a convenience alias where the lifetime is static
pub type Receiver<T> = receiver::Receiver<'static, T>;
/// This is a convenience alias where the lifetime is static
pub type Sender<T> = sender::Sender<'static, T>;

#[cfg(feature="alloc")]
/// Creates a new hatch backed by a box. Returns a [`Sender`]/[`Receiver`] pair.
pub fn hatch<T>() -> (Sender<T>, Receiver<T>) {
    let unbox = Box::leak(Box::new(Hatch::default()));
    let non = unsafe { NonNull::new_unchecked(unbox) };
    let holder = Holder::SharedBoxPtr(non);
    // Safe because we have exclusive access
    unsafe { (Sender::new(holder), Receiver::new(holder)) }
}

/// Creates a new hatch backed by a ref to an existing Hatch. Unlike
/// [`hatch`], this is not allocated on the stack and is bound by the
/// lifetime of the passed mut ref.
pub fn ref_hatch<T>(
    hatch: &mut Hatch<T>
) -> (sender::Sender<T>, receiver::Receiver<T>) {
    let holder = Holder::Ref(hatch);
    // Safe because we have exclusive access
    unsafe { (sender::Sender::new(holder), receiver::Receiver::new(holder)) }
}

/// Creates a new hatch backed by a pointer to an existing [`Hatch`]
/// whose lifetime is asserted to be whatever lifetime you say it is.
///
/// # Safety
///
/// * The provided pointer must outlive lifetime `'a`.
/// * There must be no active `Sender` or `Receiver` on the Hatch.
pub unsafe fn ptr_hatch<'a, T>(
    hatch: NonNull<Hatch<T>>
) -> (sender::Sender<'a, T>, receiver::Receiver<'a, T>) {
    let holder = Holder::BorrowedPtr(hatch);
    // Safe because we have exclusive access
    (sender::Sender::new(holder), receiver::Receiver::new(holder))
}

