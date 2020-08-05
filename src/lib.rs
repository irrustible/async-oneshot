#![no_std]
extern crate alloc;
use alloc::sync::Arc;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};
use ping_pong_cell::PingPongCell;

pub fn oneshot<T: Send>() -> (Sender<T>, Receiver<T>) {
    let inner = Arc::new(Inner { cell: PingPongCell::new(None) });
    let sender = Sender { inner: Some(inner.clone()) };
    let receiver = Receiver { inner: Some(inner) };
    (sender, receiver)
}

pub struct Closed();

pub enum WaitError {
    AlreadyAwaited(Waker),
}

pub struct Sender<T: Send> {
    inner: Option<Arc<Inner<T>>>,
}

// Can be polled as a Future to wait for a receiver to be listening.
impl<T: Send> Sender<T> {

    /// Closes the channel by causing an immediate drop
    pub fn close(self) { }

    /// Sends a message on the channel
    pub fn send(self, value: T) -> Result<(), Closed> {
        if let Some(ref inner) = &self.inner {
            let ret = inner.cell.transact(|state| {
                match state.take() {
                    Some(State::Closed) => Err(Closed()),
                    Some(State::Waker(waker)) => {
                        *state = Some(State::Ready(value));
                        Ok(Some(waker))
                    }
                    _ => {
                        *state = Some(State::Ready(value));
                        Ok(None)
                    }
                }
            });
            match ret {
                Ok(Some(waker)) => {
                    waker.wake();
                    Ok(())
                }
                Ok(None) => Ok(()),
                Err(e) => Err(e),
            }
        } else {
            Err(Closed()) // not sure how you got here tbh
        }
    }
}

pub struct Receiver<T: Send> {
    inner: Option<Arc<Inner<T>>>,
}

impl<T: Send> Receiver<T> {

    /// Closes the channel by causing an immediate drop
    pub fn close(self) { }

}

enum State<T> {
    Waker(Waker),
    Ready(T),
    Closed,
}

struct Inner<T> {
    cell: PingPongCell<State<T>>,
}

impl<T: Send> Drop for Sender<T> {
    fn drop(&mut self) {
        if let Some(inner) = &self.inner {
            let ret = inner.cell.transact(|state| {
                match state.take() {
                    Some(State::Waker(waker)) => {
                        *state = Some(State::Closed);
                        Some(waker)
                    }
                    // Could be Ready or Closed, either is fine
                    Some(other) => {
                        *state = Some(other);
                        None
                    }
                    None => {
                        *state = Some(State::Closed);
                        None
                    }
                }
            });
            if let Some(waker) = ret {
                waker.wake();
            }
        }
    }
}

impl<T: Send> Drop for Receiver<T> {
    fn drop(&mut self) {
        if let Some(inner) = &self.inner {
            let ret = inner.cell.transact(|state| {
                if let Some(State::Waker(waker)) = state.take() {
                    *state = Some(State::Closed);
                    Some(waker)
                } else {
                    *state = Some(State::Closed);
                    None
                }
            });
            if let Some(waker) = ret {
                waker.wake();
            }
        }
    }
}

impl<T: Send> Future for Sender<T> {
    type Output = Result<Waker, Closed>;
    fn poll(self: Pin<&mut Self>, context: &mut Context) -> Poll<Self::Output> {
        let this = Pin::into_inner(self);
        if let Some(inner) = &this.inner {
            inner.cell.transact(|state| {
                match state.take() {
                    Some(State::Waker(waker)) => Poll::Ready(Ok(waker)),
                    Some(State::Closed) => Poll::Ready(Err(Closed())),
                    _ => {
                        *state = Some(State::Waker(context.waker().clone()));
                        Poll::Pending
                    }
                }
            })
        } else {
            Poll::Ready(Err(Closed()))
        }
    }
}

impl<T: Send> Future for Receiver<T> {
    type Output = Result<T, Closed>;
    fn poll(self: Pin<&mut Self>, context: &mut Context) -> Poll<Result<T, Closed>> {
        let this = Pin::into_inner(self);
        if let Some(inner) = &this.inner {
            inner.cell.transact(|state| {
                match state.take() {
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
                }
            })
        } else {
            Poll::Ready(Err(Closed()))
        }
    }
}

// #[cfg(test)]
// mod tests {
//     #[test]
//     fn it_works() {
//         assert_eq!(2 + 2, 4);
//     }
// }
