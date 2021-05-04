use crate::*;
use crate::Flag::*;
#[cfg(feature="async")]
use core::hint::spin_loop;
#[cfg(feature="async")]
use core::{future::Future};
#[cfg(feature="async")]
use core::task::{Context, Poll};
#[cfg(feature="async")]
use futures_micro::poll_fn;

/// A Unique means of Receiving a message on a oneshot channel.
#[derive(Debug)]
pub struct Receiver<T> {
    channel: *mut Channel<T>,
    done: bool,
}

macro_rules! drop_if_any {
    ($state:expr, $channel:expr, $flags:expr, $ret:expr) => {
        if $flags.any($state) {
            // We need to drop the channel to avoid a leak.
            // Safe because we are the only referent now the Receiver dropped.
            let _box = unsafe { Box::from_raw($channel) };
            return $ret;
        }
    }
}    

#[cfg(feature="async")] // #[cfg(any(feature="async",feature="parking"))]
// If the sender has dropped, drop the channel.
macro_rules! poll_dropped {
    ($state:expr, $channel:expr, $done:expr) => {
        if SenderDropped.any($state) {
            $done = true;
            let val = unsafe { $channel.read().value() };
            // Safe: the sender will not touch the inner again.
            let _box = unsafe { Box::from_raw($channel) };
            return Poll::Ready(val.ok_or(Closed));
        }
    }
}    

impl<T> Receiver<T> {
    #[inline(always)]
    pub unsafe fn new(channel: *mut Channel<T>) -> Self {
        Receiver { channel, done: false }
    }

    // /// Whether the channel is closed.
    // #[inline(always)]
    // pub fn is_closed(&self) -> bool {
    //     if self.done { true }
    //     else { SenderDropped.any(unsafe { self.chan().read() }) }
    // }

    /// Attempts to receive the value. Fails immediately if the
    /// channel is closed or there is not yet a value present.
    // Pretty good, only 1 atomic either way.
    #[inline(always)]
    pub fn try_recv(&mut self) -> Result<T, TryReceiveError> {
        if self.done { return Err(TryReceiveError::Closed); }
        let state = self.chan().read();
        // If the sender has set a value or dropped, it promises never
        // to touch the inner again and we promise to clean up.
        if SenderDropped.any(state) {
            self.done = true;
            let value = unsafe { self.channel.read().value() };
            let _box = unsafe { Box::from_raw(self.channel) };
            return value.ok_or(TryReceiveError::Closed);
        }
        return Err(TryReceiveError::Empty);
    }

    #[cfg(feature="async")]
    #[inline(always)]
    pub fn recv(&mut self) -> impl Future<Output=Result<T, Closed>> + '_ {
        poll_fn(move |ctx| {
            // If we're done, we should not have been called.
            if self.done { return Poll::Ready(Err(Closed)); }
            // We will need to lock to set our waiting. And possibly to wake the sender.
            let mut state = unsafe { self.chan().set(Locked) };
            poll_dropped!(state, self.channel, self.done);
            while Locked.any(state) {
                spin_loop();
                state = unsafe { self.chan().set(Locked) };
                poll_dropped!(state, self.channel, self.done);
            }
            // Okay, now we can wait
            let _old = unsafe { self.chan().set_receiver_waiting(ctx.waker().clone()) };
            // We want ReceiverWaker to go/stay on while we set
            // Locked off, so we are going to need flip aka xor to do
            // it in a single atomic operation.
            let mask =
                if ReceiverWaker.any(state) { Locked.into() }
                else { Locked | ReceiverWaker };
            if SenderWaker.any(state) {
                // We will need to wake them and toggle their flag off
                let waiting = unsafe { self.chan().sender_waiting() };
                state = unsafe { self.chan().flip(mask | SenderWaker) };
                poll_dropped!(state, self.channel, self.done);
                if let Some(waker) = waiting { waker.wake(); }
            } else {
                state = unsafe { self.chan().flip(mask) };
                poll_dropped!(state, self.channel, self.done);
            }
            Poll::Pending
        })
    }

    // Turn our icky *mut into a nice const reference.
    #[inline(always)]
    fn chan(&self) -> &Channel<T> { unsafe { &*(self.channel as *const Channel<T>) } }

}

// macro_rules! pending {
//     ($e:expr) => {
//         if let Poll::Ready(x) = $e {
//             return Poll::Ready(x);
//         }
//     }
// }
// impl<T> Future for Receiver<T> {
//     type Output = Result<T, Closed>;
//     fn poll(self: Pin<&mut Self>, ctx: &mut Context) -> Poll<Result<T, Closed>> {
//         let this = Pin::into_inner(self);
//         if this.done {
//             // The channel has either been closed or we've already
//             // received a value, which we'll count as closed. Either
//             // way, we won't be receiving anything on it now.
//             Poll::Ready(Err(Closed))
//         } else {
//             // We'll start off by reading the state
//             let state = this.inner.state();
//             pending!(this.handle_state(&state));
//             let state = this.inner.set_receiver_waker(&state, ctx.waker().clone());
//             pending!(this.handle_state(&state));
//             if let Some(proof) = state.has_sender_waker() {
//                 this.inner.sender_waker(proof).wake_by_ref();
//             }
//             Poll::Pending
//         }
//     
// }

impl<T> Drop for Receiver<T> {
    #[inline(always)]
    // Pretty good if we don't have to spin, two atomic ops
    fn drop(&mut self) {
        // If we are done, the sender will clean up.
        if self.done { return; }
        // We may need to wake the sender and in any case it
        // simplifies our job if we can stop them waiting for us while
        // we're finishing up.
        let mut state = unsafe { self.chan().set(Locked) };
        drop_if_any!(state, self.channel, SenderDropped, ());
        while Locked.any(state) {
            eprintln!("receiverspin");
            spin_loop();
            state = unsafe { self.chan().set(Locked) };
            drop_if_any!(state, self.channel, SenderDropped, ());
        }
        if SenderWaker.any(state) {
            let sender = unsafe { self.chan().sender_waiting() };
            state = unsafe { self.chan().flip(Locked | ReceiverDropped) };
            drop_if_any!(state, self.channel, SenderDropped, ());
            if let Some(waker) = sender {
                waker.wake();
            }
            return;
        }
        state = unsafe { self.chan().flip(Locked | ReceiverDropped) };
        drop_if_any!(state, self.channel, SenderDropped, ());
    }
}
