#![no_std]
extern crate alloc;
use alloc::sync::Arc;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};
use ping_pong_cell::PingPongCell;

pub fn oneshot<T: Send>() -> (Sender<T>, Receiver<T>) {
    let inner = Arc::new(Inner { cell: PingPongCell::new(None) });
    let sender = Sender { inner: inner.clone() };
    let receiver = Receiver { inner };
    (sender, receiver)
}

pub struct Closed();

pub enum WaitError {
    AlreadyAwaited(Waker),
}

pub struct Sender<T> {
    inner: Arc<Inner<T>>,
}

// Can be polled as a Future to wait for a receiver to be listening.
impl<T: Send> Sender<T> {
    pub async fn send(self, value: T) -> Result<(), Closed> {
        if Arc::strong_count(&self.inner) == 1 { return Err(Closed()); }
        let ret = self.inner.cell.transact(|state| {
            if let Some(State::Waker(waker)) = state.take() {
                *state = Some(State::Ready(value));
                Some(waker)
            } else {
                *state = Some(State::Ready(value));
                None
            }
        });
        if let Some(waker) = ret {
            waker.wake();
        }
        Ok(())
    }
}

pub struct Receiver<T> {
    inner: Arc<Inner<T>>,
}

enum State<T> {
    Waker(Waker),
    Ready(T),
}

struct Inner<T> {
    cell: PingPongCell<State<T>>,
}

impl<T: Send> Future for Sender<T> {
    type Output = ();
    fn poll(self: Pin<&mut Self>, context: &mut Context) -> Poll<()> {
        let this = Pin::into_inner(self);
        this.inner.cell.transact(|state| {
            if let Some(State::Waker(waker)) = state {
                waker.wake_by_ref();
                Poll::Ready(())
            } else {
                *state = Some(State::Waker(context.waker().clone()));
                Poll::Pending
            }
        })
    }
}

impl<T: Send> Future for Receiver<T> {
    type Output = Result<T, Closed>;
    fn poll(self: Pin<&mut Self>, context: &mut Context) -> Poll<Result<T, Closed>> {
        let this = Pin::into_inner(self);
        this.inner.cell.transact(|state| {
            match state.take() {
                Some(State::Ready(value)) => Poll::Ready(Ok(value)),
                Some(State::Waker(waker)) => {
                    *state = Some(State::Waker(context.waker().clone()));
                    waker.wake();
                    Poll::Pending
                }
                None => {
                    if Arc::strong_count(&this.inner) == 1 {
                        Poll::Ready(Err(Closed()))
                    } else {
                        *state = Some(State::Waker(context.waker().clone()));
                        Poll::Pending
                    }
                }
            }
        })
    }
}

// #[cfg(test)]
// mod tests {
//     #[test]
//     fn it_works() {
//         assert_eq!(2 + 2, 4);
//     }
// }
