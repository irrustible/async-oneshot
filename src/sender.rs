use crate::*;
use alloc::sync::Arc;
use core::task::Poll;
use futures_micro::poll_state;

/// The sending half of a oneshot channel.
#[derive(Debug)]
pub struct Sender<T> {
    inner: Arc<Inner<T>>,
    done: bool,
}

impl<T> Sender<T> {
    pub(crate) fn new(inner: Arc<Inner<T>>) -> Self {
        Sender { inner, done: false }
    }

    /// Closes the channel by causing an immediate drop
    pub fn close(self) {}

    /// true if the channel is closed
    pub fn is_closed(&self) -> bool {
        self.inner.state().closed()
    }

    /// Waits for a Receiver to be waiting for us to send something
    /// (i.e. allows you to produce a value to send on demand).
    /// Fails if the Receiver is dropped.
    pub async fn wait(self) -> Result<Self, Closed> {
        poll_state(Some(self), |this, ctx| {
            let mut that = this.take().unwrap();
            let state = that.inner.state();
            if state.closed() {
                that.done = true;
                Poll::Ready(Err(Closed()))
            } else if state.recv() {
                Poll::Ready(Ok(that))
            } else {
                that.inner.set_send(ctx.waker().clone());
                *this = Some(that);
                Poll::Pending
            }
        })
        .await
    }

    /// Sends a message on the channel. Fails if the Receiver is dropped.
    pub fn send(mut self, value: T) -> Result<(), Closed> {
        self.done = true;
        let inner = &mut self.inner;
        let state = inner.set_value(value);
        if !state.closed() {
            if state.recv() {
                inner.recv().wake_by_ref();
                Ok(())
            } else {
                Ok(())
            }
        } else {
            inner.take_value(); // force drop.
            Err(Closed())
        }
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        if !self.done {
            let state = self.inner.state();
            if !state.closed() {
                let old = self.inner.close();
                if old.recv() {
                    self.inner.recv().wake_by_ref();
                }
            }
        }
    }
}
