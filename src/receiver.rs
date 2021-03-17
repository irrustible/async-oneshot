use crate::*;
use core::{future::Future, pin::Pin};
use core::task::{Context, Poll};

/// The receiving half of a oneshot channel.
#[derive(Debug)]
pub struct Receiver<T> {
    inner: Arc<Inner<T>>,
    // Set when the value has been received or the channel has been closed.
    done: bool,
}

impl<T> Receiver<T> {
    #[inline(always)]
    pub(crate) fn new(inner: Arc<Inner<T>>) -> Self {
        Receiver { inner, done: false }
    }

    /// Closes the channel by causing an immediate drop.
    #[inline(always)]
    pub fn close(self) { }

    /// true if the channel is closed
    #[inline(always)]
    pub fn is_closed(&self) -> bool { self.inner.state().closed() }

    #[inline(always)]
    fn handle_state(&mut self, state: &crate::inner::State<T>) -> Poll<Result<T, Closed>> {
        match state.is_ready() {
            // If the value is ready, we are safe to take it because
            // the sender never untakes it and we haven't yet taken it
            // (or done would be set and we checked in the caller).
            Ok(proof) => {
                self.done = true;
                Poll::Ready(Ok(self.inner.take_value(proof)))
            }
            Err(proof) => {
                proof.check(&*self.inner as *const Inner<T>);
                match state.is_open() {
                    // Because we might be either sync or async here,
                    // we can't do anything.
                    Ok(proof) => {
                        proof.check(&*self.inner as *const Inner<T>);
                        Poll::Pending
                    }
                    // If the channel is closed, the Sender has done
                    // it because we haven't yet taken it (or done
                    // would be set and we checked in the caller).
                    Err(proof) => {
                        proof.check(&*self.inner as *const Inner<T>);
                        self.done = true;
                        Poll::Ready(Err(Closed))
                    }
                }
            }
        }
    }

    /// Attempts to receive the value.
    #[inline]
    pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
        if self.done {
            // The channel has either been closed or we've already
            // received a value, which we'll count as closed. Either
            // way, we won't be receiving anything on it now.
            Err(TryRecvError::Closed)
        } else {
            let state = self.inner.state();
            match self.handle_state(&state) {
                Poll::Ready(Ok(x)) => Ok(x),
                Poll::Ready(Err(Closed)) => Err(TryRecvError::Closed),
                Poll::Pending => Err(TryRecvError::Empty),
            }
        }
    }
}

impl<T> Future for Receiver<T> {
    type Output = Result<T, Closed>;
    fn poll(self: Pin<&mut Self>, ctx: &mut Context) -> Poll<Result<T, Closed>> {
        let this = Pin::into_inner(self);
        if this.done {
            // The channel has either been closed or we've already
            // received a value, which we'll count as closed. Either
            // way, we won't be receiving anything on it now.
            Poll::Ready(Err(Closed))
        } else {
            let state = this.inner.state();
            match this.handle_state(&state) {
                Poll::Pending => {},
                x => return x,
            }
            let state = this.inner.set_receiver_waker(ctx.waker().clone());
            match this.handle_state(&state) {
                Poll::Pending => {},
                x => return x,
            }
            if let Some(proof) = state.has_sender_waker() {
                this.inner.sender_waker(proof).wake_by_ref();
            }
            Poll::Pending
        }
    }
}

impl<T> Drop for Receiver<T> {
    #[inline(always)]
    fn drop(&mut self) {
        if !self.done {
            let state = self.inner.state();
            match (state.is_open(), state.is_ready()) {
                (Ok(p),Ok(q)) => {
                    p.check(&*self.inner as *const Inner<T>);
                    q.check(&*self.inner as *const Inner<T>);
                }
                (Ok(p),Err(q)) => {
                    p.check(&*self.inner as *const Inner<T>);
                    q.check(&*self.inner as *const Inner<T>);
                    let old = self.inner.close();
                    if let Some(r) = old.has_sender_waker() {
                        self.inner.sender_waker(r).wake_by_ref();
                    }
                }
                (Err(p),Ok(q)) => {
                    p.check(&*self.inner as *const Inner<T>);
                    q.check(&*self.inner as *const Inner<T>);
                }
                (Err(p),Err(q)) => {
                    p.check(&*self.inner as *const Inner<T>);
                    q.check(&*self.inner as *const Inner<T>);
                }
            }
        }
    }
}
