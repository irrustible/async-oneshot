#![cfg(all(feature="alloc", not(feature="async")))]
use async_hatch::*;
use core::mem::drop;

#[test]
fn sendnow_full() {
    for _ in 0..1_000 {
        let (mut s, mut r) = hatch::<i32>();
        assert_eq!(Ok(None), s.send(42).now());
        assert_eq!(Err(SendError::Existing(420)), s.send(420).now());
        assert_eq!(Ok(Some(42)), r.receive().now());
        assert_eq!(Ok(None), s.send(420).now());
        // assert_eq!(Ok(Some(420)), r.receive().now());
    }
}

#[test]
fn sendnow_dropped() {
    for _ in 0..1_000 {
        let (mut s, r) = hatch::<i32>();
        drop(r);
        assert_eq!(Err(SendError::Closed(42)), s.send(42).now());
    }
}

#[test]
fn sendnow_closed() {
    for _ in 0..1_000 {
        let (mut s, mut r) = hatch::<i32>();
        // we have to get the receiver to close.
        assert_eq!(Ok(None), s.send(42).now());
        assert_eq!(Ok(Some(42)), r.receive().close_on_receive(true).now());
        assert_eq!(Err(SendError::Closed(42)), s.send(42).now());
    }
}

#[test]
fn sendnow_overwrite_receivenow() {
   for _ in 0..1_000 {
       let (mut s, mut r) = hatch::<i32>();
       assert_eq!(Ok(None), s.send(42).now());
       assert_eq!(Ok(Some(42)), s.send(420).overwrite(true).now());
       assert_eq!(Ok(Some(420)), r.receive().now());
    }
}

#[test]
fn receivenow_sendnow() {
    for _ in 0..1_000 {
        let (mut s, mut r) = hatch::<i32>();
        assert_eq!(Ok(None), r.receive().now());
        assert_eq!(Ok(None), r.receive().now());
        assert_eq!(Ok(None), s.send(42).now());
        assert_eq!(Ok(Some(42)), r.receive().now());
    }
}

#[test]
fn receivenow_full_dropped() {
    for _ in 0..1_000 {
        let (s, mut r) = hatch::<i32>();
        drop(s);
        // The Receiver doesn't know how lonely they really are.
        assert_eq!(Err(Closed), r.receive().now());
    }
}

#[test]
fn receivenow_full_dropped_lonely() {
    for _ in 0..1_000 {
        let (mut s, mut r) = hatch::<i32>();
        assert_eq!(Ok(None), s.send(42).now());
        drop(s);
        assert_eq!(Ok(Some(42)), r.receive().now());
        assert_eq!(Err(Closed), r.receive().now());
    }
}

#[test]
fn receivenow_closed_lonely() {
    for _ in 0..1_000 {
        let (mut s, mut r) = hatch::<i32>();
        assert_eq!(Ok(None), r.receive().now());
        assert_eq!(Ok(None), s.send(42).close_on_send(true).now());
        assert_eq!(Ok(Some(42)), r.receive().now());
        assert_eq!(Err(Closed), r.receive().now());
    }
}

#[test]
fn receivenow_empty_dropped() {
    for _ in 0..1_000 {
        let (s, mut r) = hatch::<i32>();
        drop(s);
        assert_eq!(Err(Closed), r.receive().now());
    }
}