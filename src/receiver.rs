use crate::inner::InnerValue;
use crate::*;
use core::task::{Context, Poll};
use core::{future::Future, pin::Pin};

/// The receiving half of a oneshot channel.
#[derive(Debug)]
pub struct Receiver<T> {
    inner: Arc<Inner<T>>,
    did_receive: bool,
}

impl<T> Receiver<T> {
    pub(crate) fn new(inner: Arc<Inner<T>>) -> Self {
        Receiver {
            inner,
            did_receive: false,
        }
    }

    /// Closes the channel by causing an immediate drop.
    pub fn close(self) {}

    /// Attempts to receive. On failure, if the channel is not closed,
    /// returns self to try again.
    pub fn try_recv(self) -> Result<T, TryRecvError<T>> {
        match self.inner.try_take() {
            InnerValue::Present(v) => Ok(v),
            InnerValue::Pending => Err(TryRecvError::Empty(self)),
            InnerValue::Closed => Err(TryRecvError::Closed),
        }
    }
}

impl<T> Future for Receiver<T> {
    type Output = Result<T, Closed>;
    fn poll(self: Pin<&mut Self>, ctx: &mut Context) -> Poll<Result<T, Closed>> {
        let this = Pin::into_inner(self);

        // Attempt lock free take
        match this.inner.try_take() {
            InnerValue::Present(v) => {
                this.did_receive = true;
                return Poll::Ready(Ok(v));
            }
            InnerValue::Pending => {}
            InnerValue::Closed => {
                this.did_receive = true;
                return Poll::Ready(Err(Closed()));
            }
        };

        // No value yet, register a waker
        let mut recv_lock = this.inner.lock_recv();
        recv_lock.emplace(ctx.waker().clone());

        // Check again
        match this.inner.try_take() {
            InnerValue::Present(v) => {
                // Clear the recv waker
                recv_lock.take();

                this.did_receive = true;
                return Poll::Ready(Ok(v));
            }
            InnerValue::Pending => {}
            InnerValue::Closed => {
                // Clear the recv waker
                recv_lock.take();

                this.did_receive = true;
                return Poll::Ready(Err(Closed()));
            }
        };

        // Drop the lock, waker has been registered and we will always return
        // pending now
        drop(recv_lock);

        // If set, notify the sender that we are waiting
        let send_lock = this.inner.lock_send();
        if let Some(send_waker) = send_lock.get() {
            send_waker.wake_by_ref();
        }

        Poll::Pending
    }
}

impl<T> Drop for Receiver<T> {
    #[inline(always)]
    fn drop(&mut self) {
        if !self.did_receive {
            // Mark as closed
            self.inner.mark_closed();

            // Make sure to remove the waker we registered - the sender uses it to determine
            // if we are waiting.
            let mut recv_lock = self.inner.lock_recv();
            recv_lock.take();
            drop(recv_lock);

            // Since the channel is now marked as closed, we try to wake the sender
            // if it is waiting.
            let mut send_lock = self.inner.lock_send();
            if let Some(sender) = send_lock.take() {
                sender.wake();
            }
        }
    }
}
