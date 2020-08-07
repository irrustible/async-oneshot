//! A fast, small, full-featured async-aware oneshot channel
//!
//! Unique feature: delay producing a message until a receiver is
//! waiting.
//!
//! Also supports the full range of things you'd expect.
#![no_std]
extern crate alloc;
use alloc::sync::Arc;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};
use ping_pong_cell::PingPongCell;
use futures_micro::*;

/// Create a new oneshot channel pair.
pub fn oneshot<T>() -> (Sender<T>, Receiver<T>) {
    let inner = Arc::new(Inner { cell: PingPongCell::new(None) });
    let sender = Sender { inner: Some(inner.clone()) };
    let receiver = Receiver { inner: Some(inner) };
    (sender, receiver)
}

/// An empty struct that signifies the channel is closed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Closed();

/// The sending half of a oneshot channel.
pub struct Sender<T> {
    inner: Option<Arc<Inner<T>>>,
}

/// the receiving half of a oneshot channel.
pub struct Receiver<T> {
    inner: Option<Arc<Inner<T>>>,
}

impl<T> Receiver<T> {
    /// Closes the channel by causing an immediate drop.
    pub fn close(self) { }
}

enum State<T> {
    Waker(Waker), // Someone is waiting.
    Ready(T), // A value can be received.
    Closed, // End of messages.
}

impl<T> State<T> {
    fn into_waker(self) -> Option<Waker> {
        if let State::Waker(waker) = self { Some(waker) }
        else { None }
    }
}

struct Inner<T> {
    cell: PingPongCell<State<T>>,
}

impl<T> Sender<T> {

    /// Closes the channel by causing an immediate drop
    pub fn close(self) { }

    /// true if the channel is closed
    pub fn is_closed(&self) -> bool {
        if let Some(ref inner) = &self.inner {
            inner.cell.transact(|state| {
                match &state {
                    Some(State::Closed) => true,
                    _ => false,
                }
            })
        } else { true }
    }

    /// Waits for a Receiver to be waiting for us to send something
    /// (i.e. allows you to produce a value to send on demand).
    /// Fails if the Receiver is dropped.
    pub async fn wait(self) -> Result<Self, Closed> {
        while let Some(ref inner) = &self.inner {
            let wake_me = waker().await;
            let ret = inner.cell.transact(|state| {
                match state.take() {
                    Some(State::Closed) => Poll::Ready(Err(Closed())),
                    Some(State::Waker(waker)) => {
                        *state = Some(State::Waker(waker));
                        Poll::Ready(Ok(()))
                    }
                    _ => {
                        *state = Some(State::Waker(wake_me));
                        Poll::Pending
                    }
                }
            });
            match ret {
                Poll::Pending => sleep().await,
                Poll::Ready(Ok(())) => { return Ok(self); }
                Poll::Ready(Err(Closed())) => { return Err(Closed()); }
            }
        }
        Err(Closed())
    }

    /// Sends a message on the channel. Fails if the Receiver is dropped.
    pub fn send(self, value: T) -> Result<(), Closed> {
        if let Some(ref inner) = &self.inner {
            inner.cell.transact(|state| {
                match state.take() {
                    Some(State::Closed) => Err(Closed()),
                    other => {
                        *state = Some(State::Ready(value));
                        Ok(other.and_then(|s| s.into_waker()))
                    }
                }
            }).map(maybe_wake)
        } else {
            Err(Closed()) // not sure how you got here tbh
        }
    }
}

fn maybe_wake(waker: Option<Waker>) {
    if let Some(waker) = waker { waker.wake(); }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        if let Some(inner) = &self.inner {
            maybe_wake(inner.cell.transact(|state| {
                match state.take() {
                    Some(State::Waker(waker)) => {
                        *state = Some(State::Closed);
                        Some(waker)
                    }
                    other => {
                        *state = other.or(Some(State::Closed));
                        None
                    }
                }
            }));
        }
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        if let Some(inner) = &self.inner {
            maybe_wake(inner.cell.transact(|state| {
                let st = state.take();
                *state = Some(State::Closed);
                st.and_then(|s| s.into_waker())
            }));
        }
    }
}

impl<T> Future for Receiver<T> {
    type Output = Result<T, Closed>;
    fn poll(self: Pin<&mut Self>, context: &mut Context) -> Poll<Result<T, Closed>> {
        let this = Pin::into_inner(self);
        if let Some(inner) = &this.inner {
            let (ret, waker) = inner.cell.transact(|state| {
                match state.take() {
                    Some(State::Closed) => (Poll::Ready(Err(Closed())), None),
                    Some(State::Ready(value)) => (Poll::Ready(Ok(value)), None),
                    other => {
                        *state = Some(State::Waker(context.waker().clone()));
                        (Poll::Pending, other.and_then(|s| s.into_waker()))
                    }
                }
            });
            maybe_wake(waker);
            ret
        } else {
            Poll::Ready(Err(Closed()))
        }
    }
}
