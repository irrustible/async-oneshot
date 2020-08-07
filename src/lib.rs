//! A fast, small, full-featured async-aware oneshot channel
//!
//! Unique feature: wait for receiver to be waiting.
//!
//! Also supports the full range of things you'd expect.
#![no_std]
extern crate alloc;
use alloc::sync::Arc;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};
use futures_micro::*;

mod inner;
pub(crate) use inner::Inner;

mod sender;
pub use sender::Sender;

mod receiver;
pub use receiver::Receiver;

/// Create a new oneshot channel pair.
pub fn oneshot<T>() -> (Sender<T>, Receiver<T>) {
    let inner = Arc::new(Inner::new());
    let sender = Sender::new(inner.clone());
    let receiver = Receiver::new(inner);
    (sender, receiver)
}

/// An empty struct that signifies the channel is closed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Closed();

/// We couldn't receive a message.
#[derive(Debug)]
pub enum TryRecvError<T> {
    NotReady(Receiver<T>),
    Closed,
}

pub(crate) fn maybe_wake(maybe: Option<Waker>) {
    if let Some(waker) = maybe { waker.wake(); }
}
