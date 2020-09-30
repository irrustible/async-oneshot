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
        if let Some(x) = self.x.take() {
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

    fn handle_state(inner: &Inner<T>, state: crate::inner::State) -> Poll<Result<T, Closed>> {
        if state.ready() {
            Poll::Ready(Ok(inner.take_value()))
        } else if state.closed() {
            Poll::Ready(Err(Closed()))
        } else {
            Poll::Pending
        }
    }
}

impl<T> Future for Receiver<T> {
    type Output = Result<T, Closed>;
    fn poll(self: Pin<&mut Self>, ctx: &mut Context) -> Poll<Result<T, Closed>> {
        let this = Pin::into_inner(self);
        let inner = match this.x.take() {
            Some(x) => x,
            None => return Poll::Ready(Err(Closed())),
        };
        match Self::handle_state(&inner, inner.state()) {
            Poll::Pending => {}
            x => return x,
        }
        let state = inner.set_recv(ctx.waker().clone());
        match Self::handle_state(&inner, state) {
            Poll::Pending => {}
            x => return x,
        }
        if state.send() {
            inner.send().wake_by_ref();
        }
        this.x = Some(inner);
        Poll::Pending
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
