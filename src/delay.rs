use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

pub fn delay() -> Delay {
    Delay { done: false }
}
pub struct Delay {
    done: bool
}

impl Future for Delay {
    type Output = ();
    fn poll(self: Pin<&mut Self>, _ctx: &mut Context) -> Poll<()> {
        let mut this = Pin::into_inner(self);
        if this.done {
            Poll::Ready(())
        } else {
            this.done = true;
            Poll::Pending
        }
    }
}
