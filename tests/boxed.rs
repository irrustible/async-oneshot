use async_hatch::*;
use futures_micro::prelude::*;
use core::mem::drop;
use wookie::*;

#[test]
fn sendnow_full() {
    for _ in 0..1_000 {
        let (mut s, _r) = hatch::<i32>();
        assert_eq!(Ok(None), s.send(42).now());
        assert_eq!(Err(SendError::Existing(420)), s.send(420).now());
    }
}

#[test]
fn send_full() {
    unsafe {
        for _ in 0..1_000 {
            let (mut s, mut r) = hatch::<i32>();
            assert_eq!(Ok(None), s.send(42).now());
            let f = s.send(420);
            woke!(f);
            assert_eq!(Poll::Pending, f.as_mut().poll());
            assert_eq!(Ok(Some(42)), r.receive().now());
            assert_eq!(Poll::Ready(Ok(())), f.as_mut().poll());
        }
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
fn send_dropped() {
    unsafe {
        for _ in 0..1_000 {
            let (mut s, r) = hatch::<i32>();
            drop(r);
            let f = s.send(42);
            woke!(f);
            assert_eq!(Poll::Ready(Err(SendError::Closed(42))), f.as_mut().poll());
        }
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
fn send_closed() {
    unsafe {
        for _ in 0..1_000 {
            let (mut s, mut r) = hatch::<i32>();
            // we have to get the receiver to close.
            assert_eq!(Ok(None), s.send(42).now());
            assert_eq!(Ok(Some(42)), r.receive().close_on_receive(true).now());
            let f = s.send(420);
            woke!(f);
            assert_eq!(Poll::Ready(Err(SendError::Closed(420))), f.as_mut().poll());
        }
    }
}

#[test]
fn send_overwrite_full() {
    unsafe {
        for _ in 0..1_000 {
            let (mut s, mut r) = hatch::<i32>();
            assert_eq!(Ok(None), s.send(42).now());
            let f = s.send(420).overwrite(true);
            woke!(f);
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
       let (mut s, mut r) = hatch::<i32>();
       assert_eq!(Ok(None), s.send(42).now());
       assert_eq!(Ok(Some(42)), s.send(420).overwrite(true).now());
       assert_eq!(Ok(Some(420)), r.receive().now());
    }
}

#[test]
fn sendnow_overwrite_receive() {
    unsafe {
       for _ in 0..1_000 {
           let (mut s, mut r) = hatch::<i32>();
           assert_eq!(Ok(None), s.send(42).now());
           assert_eq!(Ok(Some(42)), s.send(420).overwrite(true).now());
           let f = r.receive();
           woke!(f);
           assert_eq!(Poll::Ready(Ok(420)), f.as_mut().poll());
           assert_eq!(0, f.count());
       }
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
fn receive_sendnow() {
    unsafe {
        for _ in 0..1_000 {
            let (mut s, mut r) = hatch::<i32>();
            let f = r.receive();
            woke!(f);
            assert_eq!(Poll::Pending, f.as_mut().poll());
            assert_eq!(Poll::Pending, f.as_mut().poll());

            assert_eq!(Ok(None), s.send(42).now());
            assert_eq!(Poll::Ready(Ok(42)), f.as_mut().poll());
            assert_eq!(1, f.count());

            let f = r.receive();
            woke!(f);
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
            let (mut s, mut r) = hatch::<i32>();
            assert_eq!(Ok(None), r.receive().now());
            assert_eq!(Ok(None), r.receive().now());
            assert_eq!(Ok(None), s.send(42).now());
            assert_eq!(Ok(Some(42)), r.receive().now());
        }
    }
}
 

#[test]
fn receive_send() {
    unsafe {
        for _ in 0..1_000 {
            let (mut s, mut r) = hatch::<i32>();
            let f = r.receive();
            woke!(f);
            assert_eq!(Poll::Pending, f.as_mut().poll());
            assert_eq!(Poll::Pending, f.as_mut().poll());
            // The first one gets to finish instantly
            let g = s.send(42);
            woke!(g);
            assert_eq!(Poll::Ready(Ok(())), g.as_mut().poll());
            // The second has to wait
            let g = s.send(420);
            woke!(g);
            assert_eq!(Poll::Pending, g.as_mut().poll());
            // And the Receiver can now take the first message
            assert_eq!(Poll::Ready(Ok(42)), f.as_mut().poll());
            assert_eq!(1, f.count());
            // The sender can send the next message
            assert_eq!(Poll::Ready(Ok(())), g.as_mut().poll());
            assert_eq!(1, g.count());
            // And the receiver can receive it.
            let f = r.receive();
            woke!(f);
            assert_eq!(Poll::Ready(Ok(420)), f.as_mut().poll());
        }
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
fn receive_full_dropped() {
    unsafe {
        for _ in 0..1_000 {
            let (mut s, mut r) = hatch::<i32>();
            let f = r.receive();
            woke!(f);
            assert_eq!(Ok(None), s.send(42).now());
            assert_eq!(Poll::Ready(Ok(42)), f.as_mut().poll());
            assert_eq!(0, f.count());
            drop(s);
            // The Receiver doesn't know how lonely they really are.
            let f = r.receive();
            woke!(f);
            assert_eq!(Poll::Ready(Err(Closed)), f.as_mut().poll());
            assert_eq!(0, f.count());
        }
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
fn receive_full_dropped_lonely() {
    unsafe {
        for _ in 0..1_000 {
            let (mut s, mut r) = hatch::<i32>();
            assert_eq!(Ok(None), s.send(42).now());
            drop(s);

            let f = r.receive();
            woke!(f);
            assert_eq!(Poll::Ready(Ok(42)), f.as_mut().poll());
            assert_eq!(0, f.count());
            // The Receiver should now be LONELY
            let f = r.receive();
            woke!(f);
            assert_eq!(Poll::Ready(Err(Closed)), f.as_mut().poll());
            assert_eq!(0, f.count());
        }
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
fn receive_closed_lonely() {
    unsafe {
        for _ in 0..1_000 {
            let (mut s, mut r) = hatch::<i32>();
            let f = r.receive();
            woke!(f);
            assert_eq!(Poll::Pending, f.as_mut().poll());
            assert_eq!(Ok(None), s.send(42).close_on_send(true).now());
            assert_eq!(Poll::Ready(Ok(42)), f.as_mut().poll());
            assert_eq!(1, f.count());
            let f = r.receive();
            woke!(f);
            assert_eq!(Poll::Ready(Err(Closed)), f.as_mut().poll());
            assert_eq!(0, f.count());
        }
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

#[test]
fn receive_empty_dropped() {
    unsafe {
        for _ in 0..1_000 {
            let (s, mut r) = hatch::<i32>();
            drop(s);
            let f = r.receive();
            woke!(f);
            assert_eq!(Poll::Ready(Err(Closed)), f.as_mut().poll());
            assert_eq!(0, f.count());
        }
    }
}









// #[test]
// fn wait_close() {
//     let (mut s, r) = hatch::<bool>();
//     wookie!(woken);
//     let f = s.wait();
//     woke!(f);
//     unsafe {
//         assert_eq!(f.as_mut().poll(), Poll::Pending);
//         assert_eq!(f.count, 0);
//         drop(r); should trigger a wake
//         assert_eq!(f.as_mut().poll(), Poll::Ready(Err(Closed)));
//         assert_eq!(f.count(), 1);
//     }
// }

// #[test]
// fn wait_wait() {
//     let (mut s, _r) = hatch::<i32>();
//     let f = s.wait();
//     woke!(f);
//     unsafe {
//         eprintln!("0");
//         assert_eq!(f.as_mut().poll(), Poll::Pending);
//         assert_eq!(f.count(), 0);
//         eprintln!("1");
//         assert_eq!(f.as_mut().poll(), Poll::Pending);
//         assert_eq!(f.count(), 0);
//         eprintln!("2");
//     }
// }


// #[test]
// fn wait_recv_close() {
//     let (mut s, mut r) = hatch::<bool>();
//     assert_eq!(
//         block_on(
//             zip!(async { s.wait().await.unwrap().close(); println!("closed"); }, r)
//         ),
//         ((), Err(Closed))
//     )
// }

// #[test]
// fn wait_recv_send() {
//     let (mut s, mut r) = hatch::<i32>();
//     assert_eq!(
//         block_on(
//             zip!(async { let mut s = s.wait().await?; s.send(42) }, r)
//         ),
//         (Ok(()), Ok(42))
//     )
// }

// #[test]
// fn close_wait() {
//     let (mut s, r) = hatch::<bool>();
//     core::drop(r);
//     assert_eq!(Closed, block_on(s.wait()).unwrap_err());
// }
