#![allow(unused_mut)]
use async_hatch::{*, hatch::*, sender::SendErrorKind};
use futures_micro::prelude::*;
use core::mem::drop;
#[cfg(feature="async")]
use wookie::*;

macro_rules! tests {
    (($s:ident, $r:ident) { $( $code:tt )* }) => {
        let limit = if cfg!(miri) { 10 } else { 10_000 };
        for _ in 0..limit {
            #[cfg(feature="alloc")] {
                let (mut $s, mut $r) = hatch::<i32>();
                $( $code )*
            }
            {
                let hatch = Hatch::default();
                pin!(hatch);
                let (mut $s, mut $r) = ref_hatch::<i32>(hatch);
                $( $code )*
            }
        }
    }
}

#[test]
fn sendnow_full() {
    tests! {
        (s,r) {
            assert_eq!(Ok(None), s.send(42).now());
            let e = s.send(420).now().unwrap_err();
            assert_eq!(SendErrorKind::Full, e.kind);
            assert_eq!(420, e.value);
            assert_eq!(Ok(Some(42)), r.receive().now());
            assert_eq!(Ok(None), s.send(420).now());
            assert_eq!(Ok(Some(420)), r.receive().now());
        }
    }
}

#[test]
#[cfg(feature="async")]
fn send_full() {
    tests! {
        (s,r) {
            assert_eq!(Ok(None), s.send(42).now());
            wookie!(f: s.send(420));
            assert_pending!(f.poll());
            assert_eq!(Ok(Some(42)), r.receive().now());
            assert_ready!(Ok(()), f.poll());
            assert_eq!(Ok(Some(420)), r.receive().now());
        }
    }
}

#[test]
fn sendnow_dropped() {
    tests! {
        (s,r) {
            drop(r);
            let e = s.send(42).now().unwrap_err();
            assert_eq!(SendErrorKind::Closed, e.kind);
            assert_eq!(42, e.value);
        }
    }
}

#[test]
#[cfg(feature="async")]
fn send_dropped() {
    tests! {
        (s,r) {
             drop(r);
             wookie!(f: s.send(42));
             let e = assert_ready!(f.poll()).unwrap_err();
             assert_eq!(SendErrorKind::Closed, e.kind);
             assert_eq!(42, e.value);
        }
    }
}

#[test]
fn sendnow_closed() {
    tests! {
        (s,r) {
            // we have to get the receiver to close.
            assert_eq!(Ok(None), s.send(42).now());
            assert_eq!(Ok(Some(42)), r.receive().close_on_receive(true).now());
            let e = s.send(42).now().unwrap_err();
            assert_eq!(SendErrorKind::Closed, e.kind);
            assert_eq!(42, e.value);
        }
    }
}

#[test]
#[cfg(feature="async")]
fn send_closed() {
    tests! {
        (s,r) {
            // we have to get the receiver to close.
            assert_eq!(Ok(None), s.send(42).now());
            assert_eq!(Ok(Some(42)), r.receive().close_on_receive(true).now());
            wookie!(f: s.send(420));
            let e = assert_ready!(f.poll()).unwrap_err();
            assert_eq!(SendErrorKind::Closed, e.kind);
            assert_eq!(420, e.value);
        }
    }
}

#[test]
#[cfg(feature="async")]
fn send_overwrite_full() {
    tests! {
        (s,r) {
            assert_eq!(Ok(None), s.send(42).now());
            wookie!(f: s.send(420).overwrite(true));
            assert_pending!(f.poll());
            assert_eq!(Ok(Some(42)), r.receive().now());
            assert_ready!(Ok(()), f.poll());
            assert_eq!(Ok(Some(420)), r.receive().now());
        }
    }
}

#[test]
fn sendnow_overwrite_receivenow() {
    tests! {
        (s,r) {
           assert_eq!(Ok(None), s.send(42).now());
           assert_eq!(Ok(Some(42)), s.send(420).overwrite(true).now());
           assert_eq!(Ok(Some(420)), r.receive().now());
        }
    }
}

#[test]
#[cfg(feature="async")]
fn sendnow_overwrite_receive() {
    tests! {
        (s,r) {
            assert_eq!(Ok(None), s.send(42).now());
            assert_eq!(Ok(Some(42)), s.send(420).overwrite(true).now());
            wookie!(f: r.receive());
            assert_ready!(Ok(420), f.poll());
            assert_eq!(0, f.woken());
        }
    }
}

#[test]
fn receivenow_sendnow() {
    tests! {
        (s,r) {
            assert_eq!(Ok(None), r.receive().now());
            assert_eq!(Ok(None), r.receive().now());
            assert_eq!(Ok(None), s.send(42).now());
            assert_eq!(Ok(Some(42)), r.receive().now());
        }
    }
}

#[test]
#[cfg(feature="async")]
fn receive_sendnow() {
    tests! {
        (s,r) {
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
        }
    }
}

#[test]
#[cfg(feature="async")]
fn receivenow_send() {
    tests! {
        (s,r) {
            assert_eq!(Ok(None), r.receive().now());
            assert_eq!(Ok(None), r.receive().now());
            wookie!(f: s.send(42));
            assert_ready!(Ok(()), f.poll());
            assert_eq!(Ok(Some(42)), r.receive().now());
        }
    }
}

#[test]
#[cfg(feature="async")]
fn receive_send() {
    tests! {
        (s,r) {
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
        }
    }
}

#[test]
fn receivenow_full_dropped() {
    tests! {
        (s,r) {
            drop(s);
            // The Receiver doesn't know how lonely they really are.
            assert_eq!(Err(Closed), r.receive().now());
        }
    }
}

#[test]
#[cfg(feature="async")]
fn receive_full_dropped() {
    tests! {
        (s,r) {
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
        }
    }
}

#[test]
fn receivenow_full_dropped_lonely() {
    tests! {
        (s,r) {
            assert_eq!(Ok(None), s.send(42).now());
            drop(s);
            assert_eq!(Ok(Some(42)), r.receive().now());
            assert_eq!(Err(Closed), r.receive().now());
        }
    }
}

#[test]
#[cfg(feature="async")]
fn receive_full_dropped_lonely() {
    tests! {
        (s,r) {
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
        }
    }
}

#[test]
fn receivenow_closed_lonely() {
    tests! {
        (s,r) {
            assert_eq!(Ok(None), r.receive().now());
            assert_eq!(Ok(None), s.send(42).close_on_send(true).now());
            assert_eq!(Ok(Some(42)), r.receive().now());
            assert_eq!(Err(Closed), r.receive().now());
        }
    }
}

#[test]
#[cfg(feature="async")]
fn receive_closed_lonely() {
    tests! {
        (s,r) {
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
        }
    }
}

#[test]
fn receivenow_empty_dropped() {
    tests! {
        (s,r) {
            drop(s);
            assert_eq!(Err(Closed), r.receive().now());
        }
    }
}

#[test]
#[cfg(feature="async")]
fn receive_empty_dropped() {
    tests! {
        (s,r) {
            drop(s);
            wookie!(f: r.receive());
            assert_ready!(Err(Closed), f.poll());
            assert_eq!(0, f.woken());
        }
    }
}

#[test]
#[cfg(feature="async")]
fn wait() {
    tests! {
        (s,r) {
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
        }
    }
}

#[test]
#[cfg(feature="async")]
fn wait_drop() {
    tests! {
        (s, r) {
            wookie!(f: s.wait());
            assert_pending!(f.poll());
            drop(r);
            assert_ready!(Err(Closed), f.poll());
            assert_eq!(1, f.woken());
        }
    }
}

#[test]
#[cfg(feature="async")]
fn wait_close() {
    tests! {
        (s,r) {
            assert_eq!(Ok(None), s.send(42).now());
            wookie!(f: s.wait());
            assert_pending!(f.poll());
            assert_eq!(Ok(Some(42)), r.receive().close_on_receive(true).now());
            assert_ready!(Err(Closed), f.poll());
            assert_eq!(1, f.woken());
        }
    }
}

#[test]
#[cfg(feature="async")]
fn wait_closed() {
    tests! {
        (s,r) {
            assert_eq!(Ok(None), s.send(42).now());
            assert_eq!(Ok(Some(42)), r.receive().close_on_receive(true).now());
            wookie!(f: s.wait());
            assert_ready!(Err(Closed), f.poll());
            assert_eq!(0, f.woken());
        }
    }
}
