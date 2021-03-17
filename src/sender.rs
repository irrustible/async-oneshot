use crate::*;
use crate::inner::State;
use alloc::sync::Arc;
use core::{future::Future, task::Poll};
use futures_micro::poll_fn;

/// The sending half of a oneshot channel.
#[derive(Debug)]
pub struct Sender<T> {
    inner: Arc<Inner<T>>,
    // Set when the value has been sent or the channel has been closed.
    done: bool,
}

impl<T> Sender<T> {
    #[inline(always)]
    pub(crate) fn new(inner: Arc<Inner<T>>) -> Self {
        Sender { inner, done: false }
    }

    /// Closes the channel by causing an immediate drop
    #[inline(always)]
    pub fn close(self) { }

    /// true if the channel is closed
    #[inline(always)]
    pub fn is_closed(&self) -> bool { self.inner.state().closed() }

    /// Waits for a Receiver to be waiting for us to send something
    /// (i.e. allows you to produce a value to send on demand).
    /// Fails if the Receiver is dropped.
    #[inline]
    pub fn wait(self) -> impl Future<Output = Result<Self, Closed>> {
        // TODO: check done for a fast path, since send now takes only
        // a mut ref instead of by value.
        let mut this = Some(self);
        poll_fn(move |ctx| {
            let mut that = this.take().unwrap();
            let state: State<T> = that.inner.state();
            match state.is_open() {
                Ok(p) => {
                    p.check(&*that.inner as *const Inner<T>);
                    if let Some(q) = state.has_receiver_waker() {
                        // If the Receiver has set a Waker, they must
                        // be listening, since we can only send
                        // once.. We're done
                        q.check(&*that.inner as *const Inner<T>);
                        Poll::Ready(Ok(that))
                    } else {
                        // We'll set our waker then.
                        // TODO: check if they set theirs in the meantime.
                        that.inner.set_sender_waker(ctx.waker().clone());
                        this = Some(that);
                        Poll::Pending
                    }
                }
                // We are done because the Listener has closed the channel.
                Err(proof) => {
                    proof.check(&*that.inner as *const Inner<T>);
                    that.done = true;
                    Poll::Ready(Err(Closed))
                }
            }
        })
    }

    /// Sends a message on the channel. Fails if the Receiver is dropped.
    #[inline]
    pub fn send(&mut self, value: T) -> Result<(), Closed> {
        if !self.done {
            // Whether we succeed or the Receiver has closed the
            // Channel, we're done.
            self.done = true;
            // Optimistically set the value on the basis the receiver
            // probably hasn't dropped. This allows us to elide a
            // separate atomic load of the state.
            let state = self.inner.set_value(value);
            match state.is_open() {
                // The Receiver might still care.
                Ok(proof) => {
                    proof.check(&*self.inner as *const Inner<T>);
                    // If they're waiting, we ought to wake them up.
                    if let Some(proof) = state.has_receiver_waker() {
                        self.inner.receiver_waker(proof).wake_by_ref();
                    }
                    Ok(())
                }
                // Our bet apparently did not pay off. The receiver
                // must have closed the channel because if we had,
                // done would have been true above.
                Err(proof) => {
                    proof.check(&*self.inner as *const Inner<T>);
                    // Force drop of the optimistically set
                    // value. Safe because we just set it and the
                    // Receiver has closed the Channel.
                    unsafe { self.inner.unsafe_take_value(); }
                    Err(Closed)
                }
            }
        } else {
            // You cannot send a message on a channel that either has
            // a value already or was closed.
            Err(Closed)
        }
    }
}

impl<T> Drop for Sender<T> {
    #[inline(always)]
    fn drop(&mut self) {
        // If we are not done, we did not send a value or observe the
        // channel being closed. As we are not going to send a value,
        // we shall close it.
        if !self.done {
            // If the channel is not closed, we save an atomic
            // load. If it is closed, we swap an atomic load for an
            // atomic store.
            let state: State<T> = self.inner.close();
            match state.is_open() {
                Ok(proof) => {
                    proof.check(&*self.inner as *const Inner<T>);
                    // If the Receiver is waiting, we ought to wake them up.
                    if let Some(proof) = state.has_receiver_waker() {
                        self.inner.receiver_waker(proof).wake_by_ref();
                    }
                }
                // If the state was already closed, the Receiver must
                // have done it or done would have been true. There is
                // therefore no need to wake them.
                Err(proof) => {
                    proof.check(&*self.inner as *const Inner<T>);
                }
            }
        }
    }
}
