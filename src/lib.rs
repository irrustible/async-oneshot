//! An easy-to-use, flexible, fast, small, no-std (with alloc)
//! compatible, async-aware oneshot channel.
// If the user has turned off std, we should enable no_std.
#![cfg_attr(not(feature="std"), no_std)]

// // If you have enabled the nightly flag and you're running tests, we'd
// // like to run some extra tests that need the generators feature and
// // we're taking this as an assertion you have a nightly compiler.
// #![cfg_attr(all(test, feature="nightly"), feature(generators, generator_trait))]
// // #![cfg_attr(all(test, feature="nightly"), recursion_limit=4096)]
// #[cfg(all(test, feature="nightly"))]
// mod nightly_tests;

// If we're allowed an allocator but not std, we need alloc to import
// Box. std includes Box in the prelude.
#[cfg(not(feature="std"))] //#[cfg(all(not(feature="std"),feature="alloc"))]
extern crate alloc;
#[cfg(not(feature="std"))] //#[cfg(all(not(feature="std"),feature="alloc"))]
use alloc::boxed::Box;

use core::task::Waker;

// Typesafe flags API

mod flags;
use flags::{Flag, Flags, Test};

// The shared internal object
mod channel;
use channel::*;

// The two halves
mod sender;
mod receiver;
pub use sender::*;
pub use receiver::*;

// If you've opted in, there are some extra logic tests based around generators we'd like to run.

// #[cfg(any(feature="std",feature="alloc"))]
pub fn oneshot<T>() -> (Sender<T>, Receiver<T>) {
    let channel = Box::into_raw(Box::new(Channel::new()));
    let sender = unsafe { Sender::new(channel) };
    let receiver = unsafe { Receiver::new(channel) };
    (sender, receiver)
}
 
/// The channel is closed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Closed;

/// The reason we could not receive a message.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TryReceiveError {
    /// The Sender didn't send us a message yet.
    Empty,
    /// The Sender has dropped.
    Closed,
}
