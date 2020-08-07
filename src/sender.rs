use crate::*;
use alloc::sync::Arc;

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
    pub fn close(self) { }

    /// true if the channel is closed
    pub fn is_closed(&self) -> bool { self.inner.state().is_closed() }

    /// Waits for a Receiver to be waiting for us to send something
    /// (i.e. allows you to produce a value to send on demand).
    /// Fails if the Receiver is dropped.
    pub async fn wait(mut self) -> Result<Self, Closed> {
        loop {
            if self.done {
                return Err(Closed())
            } else {
                let wake_me = waker().await;
                let state = self.inner.state();
                if state.is_closed() {
                    self.done = true;
                    return Err(Closed())
                } else if state.is_recv() {
                    return Ok(self);
                } else {
                    self.inner.set_send(Some(wake_me));
                    sleep().await; // Wait for the waker to be used
                }
            }
        }
    }

    /// Sends a message on the channel. Fails if the Receiver is dropped.
    pub fn send(mut self, value: T) -> Result<(), Closed> {
        if self.done {
            Err(Closed())
        } else {
            self.done = true;
            let state = self.inner.state();
            if state.is_closed() {
                Err(Closed())
            } else {
                let waker = if state.is_recv() { self.inner.recv() } else { None };
                if self.inner.set_value(value).is_closed() {
                    Err(Closed())
                } else {
                    maybe_wake(waker);
                    Ok(())
                }
            }
        }
    }        
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        if !self.done {
            let state = self.inner.state();
            if !state.is_closed() {
                let old = self.inner.close();
                if old.is_recv() { maybe_wake(self.inner.recv()) }
            }
        }
    }
}
