use crate::*;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

/// The receiving half of a oneshot channel.
#[derive(Debug)]
pub struct Receiver<T> {
    x: Option<Arc<Inner<T>>>,
}

impl<T> Receiver<T> {
    pub(crate) fn new(inner: Arc<Inner<T>>) -> Self {
        Receiver { x: Some(inner) }
    }

    /// Closes the channel by causing an immediate drop.
    pub fn close(self) {}

    /// Attempts to receive. On failure, if the channel is not closed,
    /// returns self to try again.
    pub fn try_recv(mut self) -> Result<T, TryRecvError<T>> {
        let x = self.x.take();
        if let Some(x) = x {
            let state = x.state();
            if state.ready() {
                Ok(x.take_value())
            } else if state.closed() {
                Err(TryRecvError::Closed)
            } else {
                Err(TryRecvError::Empty(Self { x: Some(x) }))
            }
        } else {
            Err(TryRecvError::Closed)
        }
    }
}

impl<T> Future for Receiver<T> {
    type Output = Result<T, Closed>;
    fn poll(self: Pin<&mut Self>, ctx: &mut Context) -> Poll<Result<T, Closed>> {
        let this = Pin::into_inner(self);
        let this = match &mut this.x {
            Some(x) => x,
            None => return Poll::Ready(Err(Closed())),
        };
        let state = this.state();
        if state.ready() {
            Poll::Ready(Ok(this.take_value()))
        } else if state.closed() {
            Poll::Ready(Err(Closed()))
        } else {
            let state = this.set_recv(ctx.waker().clone());
            if state.ready() {
                Poll::Ready(Ok(this.take_value()))
            } else {
                if state.send() {
                    this.send().wake_by_ref();
                }
                Poll::Pending
            }
        }
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        if let Some(x) = self.x.take() {
            let state = x.state();
            if !state.closed() && !state.ready() {
                let old = x.close();
                if old.send() {
                    x.send().wake_by_ref();
                }
            }
        }
    }
}
