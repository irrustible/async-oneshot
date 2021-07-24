#![cfg(feature="async")]
use async_hatch::*;
use futures_micro::prelude::*;
use core::mem::drop;
use wookie::*;

#[test]
fn sendnow_full() {
    for _ in 0..1_000 {
        let hatch = Hatch::default();
        pin!(hatch);
        let (mut s, mut r) = ref_hatch::<i32>(hatch.as_mut());
        assert_eq!(Ok(None), s.send(42).now());
        assert_eq!(Err(SendError::Existing(420)), s.send(420).now());
        assert_eq!(Ok(Some(42)), r.receive().now());
        assert_eq!(Ok(None), s.send(420).now());
        assert_eq!(Ok(Some(420)), r.receive().now());
    }
}

#[test]
fn send_full() {
    unsafe {
        for _ in 0..1_000 {
            let hatch = Hatch::default();
            pin!(hatch);
            let (mut s, mut r) = ref_hatch::<i32>(hatch.as_mut());
            assert_eq!(Ok(None), s.send(42).now());
            woke!(f: s.send(420));
            assert_eq!(Poll::Pending, f.as_mut().poll());
            assert_eq!(Ok(Some(42)), r.receive().now());
            assert_eq!(Poll::Ready(Ok(())), f.as_mut().poll());
            assert_eq!(Ok(Some(420)), r.receive().now());

        }
    }
}

#[test]
fn sendnow_dropped() {
    for _ in 0..1_000 {
        let hatch = Hatch::default();
        pin!(hatch);
        let (mut s, r) = ref_hatch::<i32>(hatch.as_mut());
        drop(r);
        assert_eq!(Err(SendError::Closed(42)), s.send(42).now());
    }
}

#[test]
fn send_dropped() {
    unsafe {
        for _ in 0..1_000 {
            let hatch = Hatch::default();
            pin!(hatch);
            let (mut s, r) = ref_hatch::<i32>(hatch.as_mut());
            drop(r);
            woke!(f: s.send(42));
            assert_eq!(Poll::Ready(Err(SendError::Closed(42))), f.as_mut().poll());
        }
    }
}

#[test]
fn sendnow_closed() {
    for _ in 0..1_000 {
        let hatch = Hatch::default();
        pin!(hatch);
        let (mut s, mut r) = ref_hatch::<i32>(hatch.as_mut());
        // we have to get the receiver to close.
        assert_eq!(Ok(None), s.send(42).now());
        assert_eq!(Ok(Some(42)), r.receive().close_on_receive(true).now());
        assert_eq!(Err(SendError::Closed(42)), s.send(42).now());
    }
}

#[test]
fn send_closed() {
    unsafe {
        for _ in 0..1_000 {
            let hatch = Hatch::default();
            pin!(hatch);
            let (mut s, mut r) = ref_hatch::<i32>(hatch.as_mut());
            // we have to get the receiver to close.
            assert_eq!(Ok(None), s.send(42).now());
            assert_eq!(Ok(Some(42)), r.receive().close_on_receive(true).now());
            woke!(f: s.send(420));
            assert_eq!(Poll::Ready(Err(SendError::Closed(420))), f.as_mut().poll());
        }
    }
}

#[test]
fn send_overwrite_full() {
    unsafe {
        for _ in 0..1_000 {
            let hatch = Hatch::default();
            pin!(hatch);
            let (mut s, mut r) = ref_hatch::<i32>(hatch.as_mut());
            assert_eq!(Ok(None), s.send(42).now());
            woke!(f: s.send(420).overwrite(true));
            assert_eq!(Poll::Pending, f.as_mut().poll());
            assert_eq!(Ok(Some(42)), r.receive().now());
            assert_eq!(Poll::Ready(Ok(())), f.as_mut().poll());
            assert_eq!(Ok(Some(420)), r.receive().now());
        }
    }
}

#[test]
fn sendnow_overwrite_receivenow() {
    for _ in 0..1_000 {
        let hatch = Hatch::default();
        pin!(hatch);
        let (mut s, mut r) = ref_hatch::<i32>(hatch.as_mut());
        assert_eq!(Ok(None), s.send(42).now());
        assert_eq!(Ok(Some(42)), s.send(420).overwrite(true).now());
        assert_eq!(Ok(Some(420)), r.receive().now());
    }
}

#[test]
fn sendnow_overwrite_receive() {
    unsafe {
       for _ in 0..1_000 {
           let hatch = Hatch::default();
           pin!(hatch);
           let (mut s, mut r) = ref_hatch::<i32>(hatch.as_mut());
           assert_eq!(Ok(None), s.send(42).now());
           assert_eq!(Ok(Some(42)), s.send(420).overwrite(true).now());
           woke!(f: r.receive());
           assert_eq!(Poll::Ready(Ok(420)), f.as_mut().poll());
           assert_eq!(0, f.count());
       }
    }
}

#[test]
fn receivenow_sendnow() {
    for _ in 0..1_000 {
        let hatch = Hatch::default();
        pin!(hatch);
        let (mut s, mut r) = ref_hatch::<i32>(hatch.as_mut());
        assert_eq!(Ok(None), r.receive().now());
        assert_eq!(Ok(None), r.receive().now());
        assert_eq!(Ok(None), s.send(42).now());
        assert_eq!(Ok(Some(42)), r.receive().now());
    }
}

#[test]
fn receive_sendnow() {
    unsafe {
        for _ in 0..1_000 {
        let hatch = Hatch::default();
            pin!(hatch);
            let (mut s, mut r) = ref_hatch::<i32>(hatch.as_mut());
            {
                woke!(f: r.receive());
                assert_eq!(Poll::Pending, f.as_mut().poll());
                assert_eq!(Poll::Pending, f.as_mut().poll());
    
                assert_eq!(Ok(None), s.send(42).now());
                assert_eq!(Poll::Ready(Ok(42)), f.as_mut().poll());
                assert_eq!(1, f.count());
            }
            woke!(f: r.receive());
            assert_eq!(Ok(None), s.send(420).now());
            assert_eq!(Poll::Ready(Ok(420)), f.as_mut().poll());
            assert_eq!(0, f.count());
        }
    }
}

#[test]
fn receivenow_send() {
    unsafe {
        for _ in 0..1_000 {
            let hatch = Hatch::default();
            pin!(hatch);
            let (mut s, mut r) = ref_hatch::<i32>(hatch.as_mut());
            assert_eq!(Ok(None), r.receive().now());
            assert_eq!(Ok(None), r.receive().now());
            woke!(f: s.send(42));
            assert_eq!(Poll::Ready(Ok(())), f.as_mut().poll());
            assert_eq!(Ok(Some(42)), r.receive().now());
        }
    }
}

#[test]
fn receive_send() {
    unsafe {
        for _ in 0..1_000 {
            let hatch = Hatch::default();
            pin!(hatch);
            let (mut s, mut r) = ref_hatch::<i32>(hatch.as_mut());
            {
                woke!(f: r.receive());
                assert_eq!(Poll::Pending, f.as_mut().poll());
                assert_eq!(Poll::Pending, f.as_mut().poll());
                // The first one gets to finish instantly
                {
                    woke!(g: s.send(42));
                    assert_eq!(Poll::Ready(Ok(())), g.as_mut().poll());
                }
                // The second has to wait
                woke!(g: s.send(420));
                assert_eq!(Poll::Pending, g.as_mut().poll());
                // And the Receiver can now take the first message
                assert_eq!(Poll::Ready(Ok(42)), f.as_mut().poll());
                assert_eq!(1, f.count());
                // The sender can send the next message
                assert_eq!(Poll::Ready(Ok(())), g.as_mut().poll());
                assert_eq!(1, g.count());
            }
            // And the receiver can receive it.
            woke!(f: r.receive());
            assert_eq!(Poll::Ready(Ok(420)), f.as_mut().poll());
        }
    }
}

#[test]
fn receivenow_full_dropped() {
    for _ in 0..1_000 {
        let hatch = Hatch::default();
        pin!(hatch);
        let (s, mut r) = ref_hatch::<i32>(hatch.as_mut());
        drop(s);
        // The Receiver doesn't know how lonely they really are.
        assert_eq!(Err(Closed), r.receive().now());
    }
}

#[test]
fn receive_full_dropped() {
    unsafe {
        for _ in 0..1_000 {
            let hatch = Hatch::default();
            pin!(hatch);
            let (mut s, mut r) = ref_hatch::<i32>(hatch.as_mut());
            {
                woke!(f: r.receive());
                assert_eq!(Ok(None), s.send(42).now());
                assert_eq!(Poll::Ready(Ok(42)), f.as_mut().poll());
                assert_eq!(0, f.count());
            }
            drop(s);
            // The Receiver doesn't know how lonely they really are.
            woke!(f: r.receive());
            assert_eq!(Poll::Ready(Err(Closed)), f.as_mut().poll());
            assert_eq!(0, f.count());
        }
    }
}

#[test]
fn receivenow_full_dropped_lonely() {
    for _ in 0..1_000 {
        let hatch = Hatch::default();
        pin!(hatch);
        let (mut s, mut r) = ref_hatch::<i32>(hatch.as_mut());
        assert_eq!(Ok(None), s.send(42).now());
        drop(s);
        assert_eq!(Ok(Some(42)), r.receive().now());
        assert_eq!(Err(Closed), r.receive().now());
    }
}

#[test]
fn receive_full_dropped_lonely() {
    unsafe {
        for _ in 0..1_000 {
            let hatch = Hatch::default();
            pin!(hatch);
            let (mut s, mut r) = ref_hatch::<i32>(hatch.as_mut());
            assert_eq!(Ok(None), s.send(42).now());
            drop(s);
            {
                woke!(f: r.receive());
                assert_eq!(Poll::Ready(Ok(42)), f.as_mut().poll());
                assert_eq!(0, f.count());
            }
            // The Receiver should now be LONELY
            woke!(f: r.receive());
            assert_eq!(Poll::Ready(Err(Closed)), f.as_mut().poll());
            assert_eq!(0, f.count());
        }
    }
}

#[test]
fn receivenow_closed_lonely() {
    for _ in 0..1_000 {
        let hatch = Hatch::default();
        pin!(hatch);
        let (mut s, mut r) = ref_hatch::<i32>(hatch.as_mut());
        assert_eq!(Ok(None), r.receive().now());
        assert_eq!(Ok(None), s.send(42).close_on_send(true).now());
        assert_eq!(Ok(Some(42)), r.receive().now());
        assert_eq!(Err(Closed), r.receive().now());
    }
}

#[test]
fn receive_closed_lonely() {
    unsafe {
        for _ in 0..1_000 {
            let hatch = Hatch::default();
            pin!(hatch);
            let (mut s, mut r) = ref_hatch::<i32>(hatch.as_mut());
            {
                woke!(f: r.receive());
                assert_eq!(Poll::Pending, f.as_mut().poll());
                assert_eq!(Ok(None), s.send(42).close_on_send(true).now());
                assert_eq!(Poll::Ready(Ok(42)), f.as_mut().poll());
                assert_eq!(1, f.count());
            }
            woke!(f: r.receive());
            assert_eq!(Poll::Ready(Err(Closed)), f.as_mut().poll());
            assert_eq!(0, f.count());
        }
    }
}

#[test]
fn receivenow_empty_dropped() {
    for _ in 0..1_000 {
        let hatch = Hatch::default();
        pin!(hatch);
        let (s, mut r) = ref_hatch::<i32>(hatch.as_mut());
        drop(s);
        assert_eq!(Err(Closed), r.receive().now());
    }
}

#[test]
fn receive_empty_dropped() {
    unsafe {
        for _ in 0..1_000 {
            let hatch = Hatch::default();
            pin!(hatch);
            let (s, mut r) = ref_hatch::<i32>(hatch.as_mut());
            drop(s);
            woke!(f: r.receive());
            assert_eq!(Poll::Ready(Err(Closed)), f.as_mut().poll());
            assert_eq!(0, f.count());
        }
    }
}

#[test]
fn wait() {
    unsafe {
        for _ in 0..1_000 {
            let hatch = Hatch::default();
            pin!(hatch);
            let (mut s, mut r) = ref_hatch::<i32>(hatch.as_mut());
            woke!(g: r.receive());
            {
                woke!(f: s.wait());
                assert_eq!(Poll::Pending, f.as_mut().poll());
                assert_eq!(Poll::Pending, f.as_mut().poll());
                assert_eq!(Poll::Pending, g.as_mut().poll());
                assert_eq!(Poll::Ready(Ok(())), f.as_mut().poll());
                assert_eq!(1, f.count());
            }
            assert_eq!(Ok(None), s.send(42).now());
            assert_eq!(Poll::Ready(Ok(42)), g.as_mut().poll());
            assert_eq!(1, g.count());
        }
    }
}

#[test]
fn wait_drop() {
    unsafe {
        for _ in 0..1_000 {
            let hatch = Hatch::default();
            pin!(hatch);
            let (mut s, r) = ref_hatch::<i32>(hatch.as_mut());
            woke!(f: s.wait());
            assert_eq!(Poll::Pending, f.as_mut().poll());
            drop(r);
            assert_eq!(Poll::Ready(Err(Closed)), f.as_mut().poll());
            assert_eq!(1, f.count());
        }
    }
}


#[test]
fn wait_close() {
    unsafe {
        for _ in 0..1_000 {
            let hatch = Hatch::default();
            pin!(hatch);
            let (mut s, mut r) = ref_hatch::<i32>(hatch.as_mut());
            assert_eq!(Ok(None), s.send(42).now());
            woke!(f: s.wait());
            assert_eq!(Poll::Pending, f.as_mut().poll());
            assert_eq!(Ok(Some(42)), r.receive().close_on_receive(true).now());
            assert_eq!(Poll::Ready(Err(Closed)), f.as_mut().poll());
            assert_eq!(1, f.count());
        }
    }
}

#[test]
fn wait_closed() {
    unsafe {
        for _ in 0..1_000 {
            let hatch = Hatch::default();
            pin!(hatch);
            let (mut s, mut r) = ref_hatch::<i32>(hatch.as_mut());
            assert_eq!(Ok(None), s.send(42).now());
            assert_eq!(Ok(Some(42)), r.receive().close_on_receive(true).now());
            
            woke!(f: s.wait());
            assert_eq!(Poll::Ready(Err(Closed)), f.as_mut().poll());
            assert_eq!(0, f.count());
        }
    }
}
