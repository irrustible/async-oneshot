//! An easy-to-use, high-performance single-message async channel
//!
//! ## Examples
//!
//! Simple:
//!
//! ```
//! use async_hatch::*;
//!
//! let (mut sender, mut receiver) = hatch::<usize>();
//! sender.send(42).now().unwrap();
//! assert_eq!(receiver.receive().now(), Ok(Some(42)));
//! ```
//!
//! Lazy send / on-demand computation:
//!
//! ```
//! use core::task::Poll;
//! use async_hatch::*;
//! use wookie::wookie; // a stepping futures executor.
//!
//! let (mut sender, mut receiver) = hatch::<usize>();
//! // The sender has a large message to send, it wants to wait until
//! // the receiver is available
//! wookie!(r: receiver.receive());
//! { // Contains the scope of `s` so we can reuse the sender in a minute
//!     wookie!(s: sender.wait());
//!     assert_eq!(s.poll(), Poll::Pending);
//!     // The receiver comes along and waits.
//!     assert_eq!(r.poll(), Poll::Pending);
//!     // That wakes the sender.
//!     assert_eq!(s.woken(), 1);
//!     // The sender completes its wait.
//!     assert_eq!(s.poll(), Poll::Ready(Ok(())));
//! }
//! // It can now calculate its expensive value and send it.
//! assert_eq!(sender.send(42).now(), Ok(None));
//! // And finally, the receiver can receive it.
//! assert_eq!(r.poll(), Poll::Ready(Ok(42)));
//! ```
#![no_std]

#[cfg(feature="alloc")]
extern crate alloc;

#[cfg(feature="alloc")]
use alloc::boxed::Box;
use core::{pin::Pin, ptr::NonNull};

// Things shared between the sender and receiver
mod shared;
pub use shared::{Closed, Hatch};
use shared::*;

pub mod sender;
pub mod receiver;

pub use sender::*;
pub use receiver::*;

/// This is a convenience alias where the lifetime is static.
pub type Receiver<T> = receiver::Receiver<'static, T>;
/// This is a convenience alias where the lifetime is static.
pub type Sender<T> = sender::Sender<'static, T>;

#[cfg(feature="alloc")]
/// Creates a new hatch backed by a box. Returns a [`Sender`]/[`Receiver`] pair.
///
/// ## Examples
///
/// ```
/// use async_hatch::*;
///
/// let (mut sender, mut receiver) = hatch::<usize>();
/// sender.send(42).now().unwrap();
/// assert_eq!(receiver.receive().now(), Ok(Some(42)));
/// ```
pub fn hatch<T>() -> (Sender<T>, Receiver<T>) {
    let unbox = Box::leak(Box::new(Hatch::default()));
    let non = unsafe { NonNull::new_unchecked(unbox) };
    let holder = Holder::SharedBoxPtr(non);
    // Safe because we have exclusive access
    unsafe { (Sender::new(holder), Receiver::new(holder)) }
}

#[cfg(feature="alloc")]
/// A single-use version of [`hatch`].
pub fn oneshot<T>() -> (Sender<T>, Receiver<T>) {
    let (s, r) = hatch();
    (s.close_on_send(true), r.close_on_receive(true))
}

/// Creates a new hatch backed by a ref to an existing Hatch. Unlike
/// [`hatch`], this is not allocated on the heap and is bound by the
/// lifetime of the passed mut ref.
pub fn ref_hatch<T>(
    hatch: Pin<&mut Hatch<T>>
) -> (sender::Sender<'_, T>, receiver::Receiver<'_, T>) {
    // Safe because we aren't going to move out of it
    let hatch = unsafe { Pin::into_inner_unchecked(hatch) };
    let holder = Holder::Ref(hatch);
    // Safe because we have exclusive access
    unsafe { (sender::Sender::new(holder), receiver::Receiver::new(holder)) }
}

/// Like [`ref_hatch`], but takes an immutable ref.
///
/// ## Safety
///
/// You must not permit multiple live senders or receivers to exist.
pub unsafe fn ref_hatch_unchecked<T>(
    hatch: &Hatch<T>
) -> (sender::Sender<T>, receiver::Receiver<T>) {
    let holder = Holder::Ref(hatch);
    (sender::Sender::new(holder), receiver::Receiver::new(holder))
}
