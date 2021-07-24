#![cfg(not(feature="async"))]
use async_hatch::*;
use core::mem::drop;
use core::ptr::NonNull;
use futures_micro::prelude::*;

#[test]
fn sendnow_full() {
    unsafe {
        for _ in 0..1_000 {
            let mut hatch = Hatch::default();
            let hatch = NonNull::from(&mut hatch);
            let (mut s, mut r) = borrowed_ptr_hatch::<i32>(hatch);
            assert_eq!(Ok(None), s.send(42).now());
            assert_eq!(Err(SendError::Existing(420)), s.send(420).now());
            assert_eq!(Ok(Some(42)), r.receive().now());
            assert_eq!(Ok(None), s.send(420).now());
            assert_eq!(Ok(Some(420)), r.receive().now());
        }
    }
}

#[test]
fn sendnow_dropped() {
    unsafe {
        for _ in 0..1_000 {
            let mut hatch = Hatch::default();
            let hatch = NonNull::from(&mut hatch);
            let (mut s, mut r) = borrowed_ptr_hatch::<i32>(hatch);
            drop(r);
            assert_eq!(Err(SendError::Closed(42)), s.send(42).now());
        }
    }
}

#[test]
fn sendnow_closed() {
    unsafe {
        for _ in 0..1_000 {
            let mut hatch = Hatch::default();
            let hatch = NonNull::from(&mut hatch);
            let (mut s, mut r) = borrowed_ptr_hatch::<i32>(hatch);
            // we have to get the receiver to close.
            assert_eq!(Ok(None), s.send(42).now());
            assert_eq!(Ok(Some(42)), r.receive().close_on_receive(true).now());
            assert_eq!(Err(SendError::Closed(42)), s.send(42).now());
        }
    }
}

#[test]
fn sendnow_overwrite_receivenow() {
    unsafe {
        for _ in 0..1_000 {
            let mut hatch = Hatch::default();
            let hatch = NonNull::from(&mut hatch);
            let (mut s, mut r) = borrowed_ptr_hatch::<i32>(hatch);
            assert_eq!(Ok(None), s.send(42).now());
            assert_eq!(Ok(Some(42)), s.send(420).overwrite(true).now());
            assert_eq!(Ok(Some(420)), r.receive().now());
        }
    }
}

#[test]
fn receivenow_sendnow() {
    unsafe {
        for _ in 0..1_000 {
            let mut hatch = Hatch::default();
            let hatch = NonNull::from(&mut hatch);
            let (mut s, mut r) = borrowed_ptr_hatch::<i32>(hatch);
            assert_eq!(Ok(None), r.receive().now());
            assert_eq!(Ok(None), r.receive().now());
            assert_eq!(Ok(None), s.send(42).now());
            assert_eq!(Ok(Some(42)), r.receive().now());
        }
    }
}

#[test]
fn receivenow_full_dropped() {
    unsafe {
        for _ in 0..1_000 {
            let mut hatch = Hatch::default();
            let hatch = NonNull::from(&mut hatch);
            let (mut s, mut r) = borrowed_ptr_hatch::<i32>(hatch);
            drop(s);
            // The Receiver doesn't know how lonely they really are.
            assert_eq!(Err(Closed), r.receive().now());
        }
    }
}

#[test]
fn receivenow_full_dropped_lonely() {
    unsafe {
        for _ in 0..1_000 {
            let mut hatch = Hatch::default();
            let hatch = NonNull::from(&mut hatch);
            let (mut s, mut r) = borrowed_ptr_hatch::<i32>(hatch);
            assert_eq!(Ok(None), s.send(42).now());
            drop(s);
            assert_eq!(Ok(Some(42)), r.receive().now());
            assert_eq!(Err(Closed), r.receive().now());
        }
    }
}

#[test]
fn receivenow_closed_lonely() {
    unsafe {
        for _ in 0..1_000 {
            let mut hatch = Hatch::default();
            let hatch = NonNull::from(&mut hatch);
            let (mut s, mut r) = borrowed_ptr_hatch::<i32>(hatch);
            assert_eq!(Ok(None), r.receive().now());
            assert_eq!(Ok(None), s.send(42).close_on_send(true).now());
            assert_eq!(Ok(Some(42)), r.receive().now());
            assert_eq!(Err(Closed), r.receive().now());
        }
    }
}

#[test]
fn receivenow_empty_dropped() {
    unsafe {
        for _ in 0..1_000 {
            let mut hatch = Hatch::default();
            let hatch = NonNull::from(&mut hatch);
            let (mut s, mut r) = borrowed_ptr_hatch::<i32>(hatch);
            drop(s);
            assert_eq!(Err(Closed), r.receive().now());
        }
    }
}
