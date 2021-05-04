use crate::*;
use Flag::*;
#[cfg(feature="async")]
use core::hint::spin_loop;
#[cfg(feature="async")]
use core::{future::Future, task::Poll};
#[cfg(feature="async")]
use futures_micro::poll_fn;

/// A unique means of sending a message on a oneshot channel.
pub struct Sender<T> {
    channel: *mut Channel<T>,
    done: bool,
}

impl<T> Sender<T> {
    /// Creates a new Sender. Care must be taken to ensure no more
    /// than one Sender and one Receiver are created.
    #[inline(always)]
    pub unsafe fn new(channel: *mut Channel<T>) -> Self {
        Sender { channel, done: false }
    }

    /// True if the channel is still open. If we discover the channel
    /// to be closed, marks it such internally.
    #[inline(always)]
    pub fn check_open(&mut self) -> bool {
        if self.done { return false; }
        if ReceiverDropped.any(self.chan().read()) { return false; }
        self.done = true;
        true
    }

    // Turn our icky *mut into a nice const reference.
    #[inline(always)]
    fn chan(&self) -> &Channel<T> { unsafe { &*(self.channel as *const Channel<T>) } }
}

macro_rules! drop_if_closed {
    ($state:expr, $channel:expr, $ret:expr) => {
        if ReceiverDropped.any($state) {
            // We need to drop the channel to avoid a leak.
            // Safe because we are the only referent now the Receiver dropped.
            let _box = unsafe { Box::from_raw($channel) };
            return $ret;
        }
    }
}    

#[cfg(feature="async")] // #[cfg(any(feature="async",feature="parking"))]
// Drop the channel if the Receiver has also dropped
macro_rules! err_dropped {
    ($state:expr, $channel:expr) => {
        drop_if_closed!($state, $channel, Err(Closed))
    }
}    

#[cfg(feature="async")] // #[cfg(any(feature="async",feature="parking"))]
// Drop the channel if the Receiver has also dropped
macro_rules! poll_dropped {
    ($state:expr, $channel:expr, $done:expr) => {
        if ReceiverDropped.any($state) {
            // Set ourselves done so we don't clean up twice
            $done = true;
            // We need to drop the channel to avoid a leak.
            // Safe because we are the only referent now the Receiver dropped.
            let _box = unsafe { Box::from_raw($channel) };
            return Poll::Ready(Err(Closed));
        }
    }
}    

#[cfg(not(feature="async"))] // #[cfg(all(not(feature="async"),not(feature="parking")))]
impl<T> Sender<T> {
    /// Sends a message on the channel. Fails if we already sent a
    /// message or the Receiver drops.
    #[inline(always)]
    // If we don't have to worry about waiters, the job gets a lot easier.
    pub fn send(&mut self, value: T) -> Result<(), Closed> {
        // If we're done, this function should not have been called at all.
        if self.done { return Err(Closed) }
        // We will be done after we've called this
        self.done = true;

        // We are the only writer, only trying once. Set the value,
        // announce and pray.
        unsafe { self.chan().set_value(value); }
        let state = self.chan().set(SenderDropped);

        // We're responsible for cleanup if the Receiver already dropped.
        err_dropped!(state, self.channel);
        Ok(())
    }

}

#[cfg(feature="async")] // #[cfg(any(feature="async",feature="parking"))]
impl<T> Sender<T> {
    /// Sends a message on the channel. Fails if we already sent a
    /// message or the Receiver drops.
    // Now we have async, we have to worry about wakers. In the fast
    // case, it will be a single atomic bit operation like before
    // (plus possibly waking a waiter). In the slow case, we have a
    // spinloop...
    #[inline(always)]
    pub fn send(&mut self, value: T) -> Result<(), Closed> {
        // If we're done, this function should not have been called at all.
        if self.done { return Err(Closed) }
        // We will be done after we've called this.
        self.done = true;

        // We are the only writer, only trying once. Set the value and pray.
        unsafe { self.chan().set_value(value); }

        let mut state = unsafe { self.chan().set(Locked) };
        err_dropped!(state, self.channel);

        // Now we have to make sure the Receiver didn't already have
        // the lock. They could close at any point.
        while Locked.any(state) {
            // drat. pause and try again.
            spin_loop();
            state = unsafe { self.chan().set(Locked) };
            // Always clean up if they dropped.
            err_dropped!(state, self.channel);
        }
        if ReceiverWaker.any(state) {
            let waker = unsafe { self.chan().receiver_waiting() }.unwrap();
            state = unsafe { self.chan().flip(Locked | SenderDropped) };
            err_dropped!(state, self.channel);
            waker.wake();
        } else {
            state = unsafe { self.chan().flip(Locked | SenderDropped) };
            err_dropped!(state, self.channel);
        }
        Ok(())
    }
}

#[cfg(feature="async")]
impl<T> Sender<T> {
    /// Waits for a Receiver to be waiting for us to send something
    /// (i.e. allows you to produce a value to send on demand).
    /// Fails if the Receiver is dropped.
    #[inline(always)]
    pub fn wait(&mut self) -> impl Future<Output = Result<(), Closed>> + '_ {
        poll_fn(move |ctx| {
            // If the Sender is done, we shouldn't have been polled at all. Naughty user.
            if self.done { return Poll::Ready(Err(Closed)); }
            let mut state = unsafe { self.chan().set(Locked) };
            poll_dropped!(state, self.channel, self.done);
            while Locked.any(state) {
                spin_loop();
                state = unsafe { self.chan().set(Locked) };
                poll_dropped!(state, self.channel, self.done);
            }
            if ReceiverWaker.any(state) {
                // If a Receiver is waiting, we don't need to
                // wait. Pull our waker if we have one and be sure
                // we're marked as not waiting.
                let _pulled = unsafe { self.chan().sender_waiting() };
                state = unsafe { self.chan().unset(Locked | SenderWaker) };
                poll_dropped!(state, self.channel, self.done);
                return Poll::Ready(Ok(()));
            }
            // There isn't a Receiver waiting, so we'll have to wait.
            let _old = unsafe { self.chan().set_sender_waiting(ctx.waker().clone()) };
            if SenderWaker.any(state) {
                state = unsafe { self.chan().unset(Locked) };
            } else {
                state = unsafe { self.chan().flip(Locked | SenderWaker) };
            }
            poll_dropped!(state, self.channel, self.done);
            Poll::Pending
        })
    }
}

#[cfg(not(feature="async"))] // #[cfg(all(not(feature="async"),not(feature="parking")))]
impl<T> Drop for Sender<T> {
    #[inline(always)]
    fn drop(&mut self) {
        // If we're done, we have no further cleanup to do.
        if self.done { return; }
        // Flag ourselves as dropped
        let state = unsafe { self.chan().set(SenderDropped) };
        // Check the receiver didn't beat us to it.
        drop_if_closed!(state, self.channel, ());
    }
}

#[cfg(feature="async")] // #[cfg(any(feature="async",feature="parking"))]
impl<T> Drop for Sender<T> {
    #[inline(always)]
    fn drop(&mut self) {
        // If we're done, we have no further cleanup to do.
        if self.done { return; }
        // We need to set Locked and SenderDropped, If we just
        // set SenderDropped and there is a waker, there is a risk the
        // Receiver will free us while we're trying to wake it.
        let mut state = unsafe { self.chan().set(Locked) };
        // Check the receiver didn't beat us to it.
        drop_if_closed!(state, self.channel, ());
        while Locked.any(state) {
            // drat. pause and try again.
            spin_loop();
            state = unsafe { self.chan().set(Locked) };
            // Always clean up if they dropped.
            drop_if_closed!(state, self.channel, ());
        }
        if ReceiverWaker.any(state) {
            let receiver = unsafe { self.chan().receiver_waiting() };
            state = unsafe { self.chan().flip(Locked | SenderDropped) };
            drop_if_closed!(state, self.channel, ());
            if let Some(waker) = receiver {
                waker.wake();
            }
        } else {
            state = unsafe { self.chan().flip(Locked | SenderDropped) };
            drop_if_closed!(state, self.channel, ());
        }
    }
}
