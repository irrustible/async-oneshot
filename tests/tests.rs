use async_hatch::{*, sender::SendErrorKind};
use futures_micro::prelude::*;
use core::mem::drop;
#[cfg(feature="async")]
use wookie::*;

fn test(f: impl Fn(i32, sender::Sender<i32>, receiver::Receiver<i32>)) {
    let limit = if cfg!(miri) { 10 } else { 10_000 };
    for i in 0..limit {
        #[cfg(feature="alloc")] {
            // First try with a box.
            let (s, r) = hatch();
            f(i, s, r);
        }
        // Then a ref.
        let hatch = Hatch::default();
        pin!(hatch);
        let (s, r) = ref_hatch::<i32>(hatch.as_mut());
        f(i, s, r);
    }
}

#[test]
fn sendnow_full() {
    test(|_idx, mut s, mut r| {
        assert_eq!(Ok(None), s.send(42).now());
        let e = s.send(420).now().unwrap_err();
        assert_eq!(SendErrorKind::Full, e.kind);
        assert_eq!(420, e.value);
        assert_eq!(Ok(Some(42)), r.receive().now());
        assert_eq!(Ok(None), s.send(420).now());
        assert_eq!(Ok(Some(420)), r.receive().now());
    })
}

#[test]
#[cfg(feature="async")]
fn send_full() {
    test(|_idx, mut s, mut r| {
        assert_eq!(Ok(None), s.send(42).now());
        wookie!(f: s.send(420));
        assert_pending!(f.poll());
        assert_eq!(Ok(Some(42)), r.receive().now());
        assert_ready!(Ok(()), f.poll());
        assert_eq!(Ok(Some(420)), r.receive().now());
    })
}

#[test]
fn sendnow_dropped() {
    test(|_idx, mut s, r| {
        drop(r);
        let e = s.send(42).now().unwrap_err();
        assert_eq!(SendErrorKind::Closed, e.kind);
        assert_eq!(42, e.value);
    })
}

#[test]
#[cfg(feature="async")]
fn send_dropped() {
    test(|_idx, mut s, r| {
        drop(r);
        wookie!(f: s.send(42));
        let e = assert_ready!(f.poll()).unwrap_err();
        assert_eq!(SendErrorKind::Closed, e.kind);
        assert_eq!(42, e.value);
    })
}

#[test]
fn sendnow_closed() {
    test(|_idx, mut s, mut r| {
        // we have to get the receiver to close.
        assert_eq!(Ok(None), s.send(42).now());
        assert_eq!(Ok(Some(42)), r.receive().close_on_receive(true).now());
        let e = s.send(42).now().unwrap_err();
        assert_eq!(SendErrorKind::Closed, e.kind);
        assert_eq!(42, e.value);
    })
}

#[test]
#[cfg(feature="async")]
fn send_closed() {
    test(|_idx, mut s, mut r| {
        // we have to get the receiver to close.
        assert_eq!(Ok(None), s.send(42).now());
        assert_eq!(Ok(Some(42)), r.receive().close_on_receive(true).now());
        wookie!(f: s.send(420));
        let e = assert_ready!(f.poll()).unwrap_err();
        assert_eq!(SendErrorKind::Closed, e.kind);
        assert_eq!(420, e.value);
    })
}

#[test]
#[cfg(feature="async")]
fn send_overwrite_full() {
    test(|_idx, mut s, mut r| {
        assert_eq!(Ok(None), s.send(42).now());
        wookie!(f: s.send(420).overwrite(true));
        assert_pending!(f.poll());
        assert_eq!(Ok(Some(42)), r.receive().now());
        assert_ready!(Ok(()), f.poll());
        assert_eq!(Ok(Some(420)), r.receive().now());
    })
}

#[test]
fn sendnow_overwrite_receivenow() {
    test(|_idx, mut s, mut r| {
        assert_eq!(Ok(None), s.send(42).now());
        assert_eq!(Ok(Some(42)), s.send(420).overwrite(true).now());
        assert_eq!(Ok(Some(420)), r.receive().now());
    })
}

#[test]
#[cfg(feature="async")]
fn sendnow_overwrite_receive() {
    test(|_idx, mut s, mut r| {
        assert_eq!(Ok(None), s.send(42).now());
        assert_eq!(Ok(Some(42)), s.send(420).overwrite(true).now());
        wookie!(f: r.receive());
        assert_ready!(Ok(420), f.poll());
        assert_eq!(0, f.woken());
    })
}

#[test]
fn receivenow_sendnow() {
    test(|_idx, mut s, mut r| {
        assert_eq!(Ok(None), r.receive().now());
        assert_eq!(Ok(None), r.receive().now());
        assert_eq!(Ok(None), s.send(42).now());
        assert_eq!(Ok(Some(42)), r.receive().now());
    })
}

#[test]
#[cfg(feature="async")]
fn receive_sendnow() {
    test(|_idx, mut s, mut r| {
        {
            wookie!(f: r.receive());
            assert_pending!(f.poll());
            assert_pending!(f.poll());
            assert_eq!(Ok(None), s.send(42).now());
            assert_ready!(Ok(42), f.poll());
            assert_eq!(1, f.woken());
        }
        wookie!(f: r.receive());
        assert_eq!(Ok(None), s.send(420).now());
        assert_ready!(Ok(420), f.poll());
        assert_eq!(0, f.woken());
    })
}

#[test]
#[cfg(feature="async")]
fn receivenow_send() {
    test(|_idx, mut s, mut r| {
        assert_eq!(Ok(None), r.receive().now());
        assert_eq!(Ok(None), r.receive().now());
        wookie!(f: s.send(42));
        assert_ready!(Ok(()), f.poll());
        assert_eq!(Ok(Some(42)), r.receive().now());
    })
}

#[test]
#[cfg(feature="async")]
fn receive_send() {
    test(|_idx, mut s, mut r| {
        {
            wookie!(f: r.receive());
            assert_pending!(f.poll());
            assert_pending!(f.poll());
            // The first one gets to finish instantly
            {
                wookie!(g: s.send(42));
                assert_ready!(Ok(()), g.poll());
            }
            // The second has to wait
            wookie!(g: s.send(420));
            assert_pending!(g.poll());
            // And the Receiver can now take the first message
            assert_ready!(Ok(42), f.poll());
            assert_eq!(1, f.woken());
            // The sender can send the next message
            assert_ready!(Ok(()), g.poll());
            assert_eq!(1, g.woken());
        }
        // And the receiver can receive it.
        wookie!(f: r.receive());
        assert_ready!(Ok(420), f.poll());
    })
}

#[test]
fn receivenow_full_dropped() {
    test(|_idx, s, mut r| {
        drop(s);
        // The Receiver doesn't know how lonely they really are.
        assert_eq!(Err(Closed), r.receive().now());
    })
}

#[test]
#[cfg(feature="async")]
fn receive_full_dropped() {
    test(|_idx, mut s, mut r| {
        {
            wookie!(f: r.receive());
            assert_eq!(Ok(None), s.send(42).now());
            assert_ready!(Ok(42), f.poll());
            assert_eq!(0, f.woken());
        }
        drop(s);
        // The Receiver doesn't know how lonely they really are.
        wookie!(f: r.receive());
        assert_ready!(Err(Closed), f.poll());
        assert_eq!(0, f.woken());
    })
}

#[test]
fn receivenow_full_dropped_lonely() {
    test(|_idx, mut s, mut r| {
        assert_eq!(Ok(None), s.send(42).now());
        drop(s);
        assert_eq!(Ok(Some(42)), r.receive().now());
        assert_eq!(Err(Closed), r.receive().now());
    })
}

#[test]
#[cfg(feature="async")]
fn receive_full_dropped_lonely() {
    test(|_idx, mut s, mut r| {
        assert_eq!(Ok(None), s.send(42).now());
        drop(s);
        {
            wookie!(f: r.receive());
            assert_ready!(Ok(42), f.poll());
            assert_eq!(0, f.woken());
        }
        // The Receiver should now be LONELY
        wookie!(f: r.receive());
        assert_ready!(Err(Closed), f.poll());
        assert_eq!(0, f.woken());
    })
}

#[test]
fn receivenow_closed_lonely() {
    test(|_idx, mut s, mut r| {
        assert_eq!(Ok(None), r.receive().now());
        assert_eq!(Ok(None), s.send(42).close_on_send(true).now());
        assert_eq!(Ok(Some(42)), r.receive().now());
        assert_eq!(Err(Closed), r.receive().now());
    })
}

#[test]
#[cfg(feature="async")]
fn receive_closed_lonely() {
    test(|_idx, mut s, mut r| {
        {
            wookie!(f: r.receive());
            assert_pending!(f.poll());
            assert_eq!(Ok(None), s.send(42).close_on_send(true).now());
            assert_ready!(Ok(42), f.poll());
            assert_eq!(1, f.woken());
        }
        wookie!(f: r.receive());
        assert_ready!(Err(Closed), f.poll());
        assert_eq!(0, f.woken());
    })
}

#[test]
fn receivenow_empty_dropped() {
    test(|_idx, s, mut r| {
        drop(s);
        assert_eq!(Err(Closed), r.receive().now());
    })
}

#[test]
#[cfg(feature="async")]
fn receive_empty_dropped() {
    test(|_idx, s, mut r| {
        drop(s);
        wookie!(f: r.receive());
        assert_ready!(Err(Closed), f.poll());
        assert_eq!(0, f.woken());
    })
}

#[test]
#[cfg(feature="async")]
fn wait() {
    test(|_idx, mut s, mut r| {
        wookie!(g: r.receive());
        {
            wookie!(f: s.wait());
            assert_pending!(f.poll());
            assert_pending!(f.poll());
            assert_pending!(g.poll());
            assert_ready!(Ok(()), f.poll());
            assert_eq!(1, f.woken());
        }
        assert_eq!(Ok(None), s.send(42).now());
        assert_ready!(Ok(42), g.poll());
        assert_eq!(1, g.woken());
    })
}

#[test]
#[cfg(feature="async")]
fn wait_drop() {
    test(|_idx, mut s, r| {
        wookie!(f: s.wait());
        assert_pending!(f.poll());
        drop(r);
        assert_ready!(Err(Closed), f.poll());
        assert_eq!(1, f.woken());
    })
}

#[test]
#[cfg(feature="async")]
fn wait_close() {
    test(|_idx, mut s, mut r| {
        assert_eq!(Ok(None), s.send(42).now());
        wookie!(f: s.wait());
        assert_pending!(f.poll());
        assert_eq!(Ok(Some(42)), r.receive().close_on_receive(true).now());
        assert_ready!(Err(Closed), f.poll());
        assert_eq!(1, f.woken());
    })
}

#[test]
#[cfg(feature="async")]
fn wait_closed() {
    test(|_idx, mut s, mut r| {
        assert_eq!(Ok(None), s.send(42).now());
        assert_eq!(Ok(Some(42)), r.receive().close_on_receive(true).now());
        wookie!(f: s.wait());
        assert_ready!(Err(Closed), f.poll());
        assert_eq!(0, f.woken());
    })
}
