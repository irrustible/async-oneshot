use crate::*;

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
    pub fn close(self) { }

    /// Attempts to receive. On failure, if the channel is not closed,
    /// returns self to try again.
    pub fn try_recv(mut self) -> Result<T, TryRecvError<T>> {
        if self.done {
            Err(TryRecvError::Closed)
        } else {
            let state = self.inner.state();
            if state.is_closed() {
                self.done = true;
                Err(TryRecvError::Closed)
            } else if state.is_ready() {
                self.done = true;
                Ok(self.inner.take_value().unwrap())
            } else {
                Err(TryRecvError::NotReady(self))
            }
        }
    }
}



impl<T> Future for Receiver<T> {
    type Output = Result<T, Closed>;
    fn poll(self: Pin<&mut Self>, ctx: &mut Context) -> Poll<Result<T, Closed>> {
        let this = Pin::into_inner(self);
        if this.done {
            Poll::Ready(Err(Closed()))
        } else {
            let state = this.inner.state();
            if state.is_closed() {
                this.done = true;
                Poll::Ready(Err(Closed()))
            } else if state.is_ready() {
                this.done = true;
                Poll::Ready(Ok(this.inner.take_value().unwrap()))
            } else {
                let state = this.inner.set_recv(Some(ctx.waker().clone()));
                if state.is_closed() {
                    this.done = true;
                    Poll::Ready(Err(Closed()))
                } else if state.is_ready() {
                    this.done = true;
                    Poll::Ready(Ok(this.inner.take_value().unwrap()))
                } else {
                    if state.is_send() { maybe_wake(this.inner.send()); }
                    Poll::Pending
                }                
            }
        }
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        if !self.done {
            let state = self.inner.state();
            if !state.is_closed() && !state.is_ready() {
                let old = self.inner.close();
                if old.is_recv() { maybe_wake(self.inner.send()) }
            }
        }
    }
}
