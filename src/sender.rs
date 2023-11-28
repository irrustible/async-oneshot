use crate::*;
use alloc::sync::Arc;
use core::future::{poll_fn, Future};
use core::task::Poll;

/// The sending half of a oneshot channel.
#[derive(Debug)]
pub struct Sender<T> {
    inner: Arc<Inner<T>>,
    did_send: bool,
}

impl<T> Sender<T> {
    pub(crate) fn new(inner: Arc<Inner<T>>) -> Self {
        Sender {
            inner,
            did_send: false,
        }
    }

    /// Closes the channel by causing an immediate drop
    pub fn close(self) {}

    /// true if the channel is closed
    ///
    /// NOTE: This performs an atomic load, but the result may be
    /// instantly be out of date if it returns false.
    pub fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }

    /// Waits for a Receiver to be waiting for us to send something
    /// (i.e. allows you to produce a value to send on demand).
    /// Fails if the Receiver is dropped.
    pub fn wait(self) -> impl Future<Output = Result<Self, Closed>> {
        let mut fut_state = Some(self);
        poll_fn(move |ctx| {
            let this = fut_state.take().unwrap();

            // Attempt lock free check
            if this.is_closed() {
                return Poll::Ready(Err(Closed()));
            }

            let recv_lock = this.inner.lock_recv();
            if recv_lock.get().is_some() {
                drop(recv_lock);

                // A receiver is waiting for us
                fut_state = None;
                return Poll::Ready(Ok(this));
            }

            // Keep the receiver locked while we set a waker
            let mut send_lock = this.inner.lock_send();
            send_lock.emplace(ctx.waker().clone());

            // Drop both locks, we have a waker registered now
            drop(send_lock);
            drop(recv_lock);

            fut_state = Some(this);
            Poll::Pending
        })
    }

    /// Sends a message on the channel. Fails if the Receiver is dropped.
    pub fn send(&mut self, value: T) -> Result<(), Closed> {
        if self.did_send {
            Err(Closed())
        } else {
            self.did_send = true;

            let inner = &mut self.inner;
            inner.emplace_value(value);

            // Attempt to wake up a receiver
            let mut recv_lock = inner.lock_recv();
            if let Some(waker) = recv_lock.take() {
                waker.wake();
            }

            if inner.is_closed() {
                Err(Closed())
            } else {
                Ok(())
            }
        }
    }
}

impl<T> Drop for Sender<T> {
    #[inline(always)]
    fn drop(&mut self) {
        if !self.did_send {
            // Mark as closed
            self.inner.mark_closed();

            // Attempt to wake up a receiver
            let mut recv_lock = self.inner.lock_recv();
            if let Some(waker) = recv_lock.take() {
                waker.wake();
            }
        }
    }
}
