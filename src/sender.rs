use crate::*;
use alloc::sync::Arc;
use core::task::Poll;
use futures_micro::poll_state;

/// The sending half of a oneshot channel.
#[derive(Debug)]
pub struct Sender<T> {
    x: Option<Arc<Inner<T>>>,
}

impl<T> Sender<T> {
    pub(crate) fn new(inner: Arc<Inner<T>>) -> Self {
        Sender { x: Some(inner) }
    }

    /// Closes the channel by causing an immediate drop
    pub fn close(self) {}

    /// true if the channel is closed
    pub fn is_closed(&self) -> bool {
        self.x.as_ref().unwrap().state().closed()
    }

    /// Waits for a Receiver to be waiting for us to send something
    /// (i.e. allows you to produce a value to send on demand).
    /// Fails if the Receiver is dropped.
    pub async fn wait(self) -> Result<Self, Closed> {
        poll_state(Some(self), |this, ctx| {
            let that = this.take().unwrap().x.take().unwrap();
            let state = that.state();
            if state.closed() {
                Poll::Ready(Err(Closed()))
            } else if state.recv() {
                Poll::Ready(Ok(Self { x: Some(that) }))
            } else {
                that.set_send(ctx.waker().clone());
                *this = Some(Self { x: Some(that) });
                Poll::Pending
            }
        })
        .await
    }

    /// Sends a message on the channel. Fails if the Receiver is dropped.
    pub fn send(mut self, value: T) -> Result<(), Closed> {
        let inner = self.x.take().unwrap();
        let state = inner.set_value(value);
        if !state.closed() {
            if state.recv() {
                inner.recv().wake_by_ref();
            }
            Ok(())
        } else {
            inner.take_value(); // force drop.
            Err(Closed())
        }
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        if let Some(x) = self.x.take() {
            let state = x.state();
            if !state.closed() {
                let old = x.close();
                if old.recv() {
                    x.recv().wake_by_ref();
                }
            }
        }
    }
}
