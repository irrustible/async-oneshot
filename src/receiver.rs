use crate::*;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

/// The receiving half of a oneshot channel.
#[derive(Debug)]
pub struct Receiver<T> {
    inner: Arc<Inner<T>>,
    done: bool,
}

impl<T> Receiver<T> {
    pub(crate) fn new(inner: Arc<Inner<T>>) -> Self {
        Receiver { inner, done: false }
    }

    /// Closes the channel by causing an immediate drop.
    pub fn close(self) {}

    /// Attempts to receive. On failure, if the channel is not closed,
    /// returns self to try again.
    pub fn try_recv(mut self) -> Result<T, TryRecvError<T>> {
        let state = self.inner.state();
        if state.ready() {
            self.done = true;
            Ok(self.inner.take_value())
        } else if state.closed() {
            self.done = true;
            Err(TryRecvError::Closed)
        } else {
            Err(TryRecvError::Empty(self))
        }
    }
}

impl<T> Future for Receiver<T> {
    type Output = Result<T, Closed>;
    fn poll(self: Pin<&mut Self>, ctx: &mut Context) -> Poll<Result<T, Closed>> {
        let this = Pin::into_inner(self);
        let state = this.inner.state();
        if state.ready() {
            this.done = true;
            Poll::Ready(Ok(this.inner.take_value()))
        } else if state.closed() {
            this.done = true;
            Poll::Ready(Err(Closed()))
        } else {
            let state = this.inner.set_recv(ctx.waker().clone());
            if state.ready() {
                this.done = true;
                Poll::Ready(Ok(this.inner.take_value()))
            } else {
                if state.send() {
                    this.inner.send().wake_by_ref();
                }
                Poll::Pending
            }
        }
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        if !self.done {
            let state = self.inner.state();
            if !state.closed() && !state.ready() {
                let old = self.inner.close();
                if old.send() {
                    self.inner.send().wake_by_ref();
                }
            }
        }
    }
}
