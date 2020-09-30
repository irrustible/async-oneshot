use async_oneshot::*;
use futures_lite::future::block_on;
use futures_micro::prelude::*;

#[test]
fn send_recv() {
    let (s, r) = oneshot::<i32>();
    assert_eq!(
        block_on(zip!(async { s.send(42).unwrap() }, r)),
        ((), Ok(42))
    )
}
#[test]
fn recv_send() {
    let (s, r) = oneshot::<i32>();
    assert_eq!(
        block_on(zip!(r, async { s.send(42).unwrap() })),
        (Ok(42), ())
    )
}

#[test]
fn close_recv() {
    let (s, r) = oneshot::<i32>();
    s.close();
    assert_eq!(Err(Closed()), block_on(r));
}

#[test]
fn close_send() {
    let (s, r) = oneshot::<bool>();
    r.close();
    assert_eq!(Err(Closed()), s.send(true));
}

#[test]
fn send_close() {
    let (s, r) = oneshot::<bool>();
    s.send(true).unwrap();
    r.close();
}

#[test]
fn recv_close() {
    let (s, r) = oneshot::<bool>();
    assert_eq!(block_on(zip!(r, async { s.close() })), (Err(Closed()), ()))
}

#[test]
fn wait_close() {
    let (s, r) = oneshot::<bool>();
    assert_eq!(
        block_on(zip!(async { s.wait().await.unwrap_err() }, async {
            r.close()
        })),
        (Closed(), ())
    )
}

#[test]
fn wait_recv_close() {
    let (s, r) = oneshot::<bool>();
    assert_eq!(
        block_on(zip!(
            async {
                s.wait().await.unwrap().close();
                println!("closed");
            },
            r
        )),
        ((), Err(Closed()))
    )
}

#[test]
fn wait_recv_send() {
    let (s, r) = oneshot::<i32>();
    assert_eq!(
        block_on(zip!(
            async {
                let s = s.wait().await?;
                s.send(42)
            },
            r
        )),
        (Ok(()), Ok(42))
    )
}

#[test]
fn close_wait() {
    let (s, r) = oneshot::<bool>();
    r.close();
    assert_eq!(Closed(), block_on(s.wait()).unwrap_err());
}
