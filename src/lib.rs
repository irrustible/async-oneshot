//! An easy-to-use, high-performance single-message async channel
//!
//! ## Examples
//!
//! Synchronous send and receive
//!
//! ```
//! #[cfg(feature="alloc")] {
//! use async_hatch::hatch;
//!
//! let (mut sender, mut receiver) = hatch::<usize>();
//! sender.send(42).now().unwrap();
//! assert_eq!(receiver.receive().now(), Ok(Some(42)));
//! // You can reuse it too.
//! sender.send(420).now().unwrap();
//! assert_eq!(receiver.receive().now(), Ok(Some(420)));
//! }
//! ```
//!
//! Async receive:
//!
//! ```
//! #[cfg(all(feature="alloc",feature="async"))] {
//! use core::task::Poll;
//! use async_hatch::hatch;
//! use wookie::wookie; // a stepping futures executor.
//!
//! let (mut sender, mut receiver) = hatch::<usize>();
//! wookie!(r: receiver.receive()); // Prepare a receive future for execution.
//! assert_eq!(r.poll(), Poll::Pending); // The receiver has to wait, there is no value.
//! assert_eq!(sender.send(42).now(), Ok(None)); // Sender sends.
//! assert_eq!(r.woken(), 1); // That wakes the receiver.
//! assert_eq!(r.poll(), Poll::Ready(Ok(42))); // Who gets the value.
//! }
//! ```
//!
//! Async send:
//!
//! ```
//! #[cfg(all(feature="alloc",feature="async"))] {
//! use core::task::Poll;
//! use async_hatch::hatch;
//! use wookie::wookie; // a stepping futures executor.
//!
//! let (mut sender, mut receiver) = hatch::<usize>();
//! // prepare our operation futures
//! sender.send(42).now().unwrap(); // Fill the channel so we can't send.
//! wookie!(s: sender.send(42));
//! wookie!(r: receiver.receive());
//! assert_eq!(s.poll(), Poll::Pending); // Channel full, sender waits.
//! assert_eq!(r.poll(), Poll::Ready(Ok(42))); // Receiver gets value.
//! assert_eq!(s.woken(), 1); // That wakes the sender
//! assert_eq!(s.poll(), Poll::Ready(Ok(()))); // Sender sends.
//! }
//! ```
//!
//! Async lazy send / on-demand computation:
//!
//! ```
//! #[cfg(all(feature="alloc",feature="async"))] {
//! use core::task::Poll;
//! use async_hatch::hatch;
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
//! }
//! ```
#![no_std]

#[cfg(feature="alloc")]
extern crate alloc;

#[cfg(feature="alloc")]
use alloc::boxed::Box;
use core::pin::Pin;
#[cfg(feature="alloc")]
use core::ptr::NonNull;
use core::sync::atomic::Ordering;

pub mod hatch;
use hatch::*;

pub mod sender;
pub mod receiver;

pub use sender::*;
pub use receiver::*;

#[cfg(feature="alloc")]
pub mod heap {
    use alloc::boxed::Box;
    use crate::{sender, receiver, hatch::Hatch};
    pub type Sender<T> = sender::Sender<T, Box<Hatch<T>>>;
    pub type Receiver<T> = receiver::Receiver<T, Box<Hatch<T>>>;
    pub type Sending<'a, T> = sender::Sending<'a, T, Box<Hatch<T>>>;
    #[cfg(feature="async")]
    pub type Wait<'a, T> = sender::Wait<'a, T, Box<Hatch<T>>>;
    pub type Receiving<'a, T> = receiver::Receiving<'a, T, Box<Hatch<T>>>;
}

/// Creates a new hatch on the heap. Returns its [`Sender`] and [`Receiver`].
///
/// ## Example
///
/// ```
/// use async_hatch::hatch;
///
/// let (mut sender, mut receiver) = hatch::<usize>();
/// sender.send(42).now().unwrap();
/// assert_eq!(receiver.receive().now(), Ok(Some(42)));
/// ```
///
/// See the documentation at the crate root for more examples.
#[cfg(feature="alloc")]
pub fn hatch<T>() -> (heap::Sender<T>, heap::Receiver<T>) {
    let unbox = Box::leak(Box::new(Hatch::default()));
    let non = unsafe { NonNull::new_unchecked(unbox) };
    // Safe because we have exclusive access
    unsafe { (Sender::new(non), Receiver::new(non)) }
}

/// A single-use version of [`hatch`].
///
/// ## Example
///
/// ```
/// use async_hatch::oneshot;
///
/// let (mut sender, mut receiver) = oneshot::<usize>();
/// sender.send(42).now().unwrap();
/// assert_eq!(receiver.receive().now(), Ok(Some(42)));
/// ```
///
/// See the documentation at the crate root for more examples.
#[cfg(feature="alloc")]
pub fn oneshot<T>() -> (heap::Sender<T>, heap::Receiver<T>) {
    let (s, r) = hatch();
    (s.close_on_send(true), r.close_on_receive(true))
}

/// Creates a sender-receiver pair from a pinned mut ref to an existing hatch.
///
/// ## Example
///
/// ```
/// use core::pin::Pin;
/// use async_hatch::{hatch::Hatch, ref_hatch};
/// // We will pin it on the stack for this example.
/// let mut hatch = Hatch::default();
/// let mut h = unsafe { Pin::new_unchecked(&mut hatch) };
/// let (mut sender, mut receiver) = ref_hatch(h);
/// sender.send(42).now().unwrap();
/// assert_eq!(receiver.receive().now(), Ok(Some(42)));
/// ```
pub fn ref_hatch<T>(
    hatch: Pin<&mut Hatch<T>>
) -> (sender::Sender<T, &Hatch<T>>, receiver::Receiver<T, &Hatch<T>>) {
    // Safe because we aren't going to move out of it
    let hatch = &*unsafe { Pin::into_inner_unchecked(hatch) };
    // Safe because we have exclusive access
    unsafe { (sender::Sender::new(hatch), receiver::Receiver::new(hatch)) }
}

/// Like [`ref_hatch`], but takes an immutable ref.
///
/// ## Safety
///
/// You must not permit multiple live senders or receivers to exist.
pub unsafe fn ref_hatch_unchecked<T>(
    hatch: &Hatch<T>
) -> (sender::Sender<T, &Hatch<T>>, receiver::Receiver<T, &Hatch<T>>) {
    (sender::Sender::new(hatch), receiver::Receiver::new(hatch))
}
