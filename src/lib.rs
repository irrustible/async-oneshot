#![no_std]
extern crate alloc;
use alloc::sync::Arc;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};
use futures_lite::{future::poll_fn, Future};
use ping_pong_cell::PingPongCell;

mod delay;

enum State<T> {
    Waker(Waker),
    Ready(T),
    Closed,
}

type Inner<T> = PingPongCell<State<T>>;

pub fn oneshot<T: Send>() -> (Sender<T>, Receiver<T>) {
    let inner = Arc::new(PingPongCell::new(None));
    let sender = Sender {
        inner: inner.clone(),
    };
    let receiver = Receiver { inner: inner };
    (sender, receiver)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Closed();

#[derive(Debug)]
pub enum WaitError {
    AlreadyAwaited(Waker),
}

pub struct Sender<T: Send> {
    inner: Arc<Inner<T>>,
}

pub struct Receiver<T: Send> {
    inner: Arc<Inner<T>>,
}

// Can be polled as a Future to wait for a receiver to be listening.
impl<T: Send> Sender<T> {
    /// Waits for a Receiver to be waiting for us to send something
    /// (i.e. allows you to produce a value to send on demand).
    pub async fn wait(self) -> Result<Self, Closed> {
        loop {
            let wake_me = poll_fn(|c| Poll::Ready(c.waker().clone())).await;
            let ret = self.inner.transact(|state| match state.take() {
                Some(State::Closed) => Poll::Ready(Err(Closed())),
                Some(State::Waker(waker)) => {
                    *state = Some(State::Waker(waker));
                    Poll::Ready(Ok(()))
                }
                _ => {
                    *state = Some(State::Waker(wake_me));
                    Poll::Pending
                }
            });
            match ret {
                Poll::Pending => delay::delay().await,
                Poll::Ready(res) => return res.map(|()| self),
            }
        }
    }

    /// Sends a message on the channel
    pub fn send(self, value: T) -> Result<(), Closed> {
        self.inner
            .transact(|state| {
                match state.take() {
                    Some(State::Closed) => Err(Closed()),
                    Some(State::Waker(waker)) => Ok(Some(waker)),
                    _ => Ok(None),
                }
                .map(|maybe_waker| {
                    *state = Some(State::Ready(value));
                    maybe_waker
                })
            })
            .map(|maybe_waker| {
                if let Some(waker) = maybe_waker {
                    waker.wake();
                }
            })
    }
}

fn transact_and_wake<T, F>(inner: &Inner<T>, f: F)
where
    T: Send,
    F: FnOnce(&mut Option<State<T>>) -> Option<Waker>,
{
    if let Some(waker) = inner.transact(f) {
        waker.wake();
    }
}

impl<T: Send> Drop for Sender<T> {
    fn drop(&mut self) {
        transact_and_wake(&self.inner, |state| {
            let (next_state, next_waker) = match state.take() {
                Some(State::Waker(waker)) => (State::Closed, Some(waker)),
                // Could be Ready or Closed, either is fine
                Some(other) => (other, None),
                None => (State::Closed, None),
            };
            *state = Some(next_state);
            next_waker
        });
    }
}

impl<T: Send> Drop for Receiver<T> {
    fn drop(&mut self) {
        transact_and_wake(&self.inner, |state| {
            if let Some(State::Waker(waker)) = core::mem::replace(&mut *state, Some(State::Closed))
            {
                Some(waker)
            } else {
                None
            }
        });
    }
}

impl<T: Send> Future for Receiver<T> {
    type Output = Result<T, Closed>;
    fn poll(self: Pin<&mut Self>, context: &mut Context) -> Poll<Result<T, Closed>> {
        let this = Pin::into_inner(self);
        this.inner.transact(|state| match state.take() {
            Some(State::Closed) => Poll::Ready(Err(Closed())),
            Some(State::Ready(value)) => Poll::Ready(Ok(value)),
            Some(State::Waker(waker)) => {
                *state = Some(State::Waker(context.waker().clone()));
                waker.wake();
                Poll::Pending
            }
            None => {
                *state = Some(State::Waker(context.waker().clone()));
                Poll::Pending
            }
        })
    }
}
