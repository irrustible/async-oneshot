use async_oneshot::*;
use futures_lite::*;
use std::thread::spawn;

#[test]
fn success_one_thread() {
    let (s,r) = oneshot::<bool>();
    assert_eq!((), s.send(true).unwrap());
    assert_eq!(Ok(true), future::block_on(r));
}

#[test]
fn close_sender_one_thread() {
    let (s,r) = oneshot::<bool>();
    s.close();
    assert_eq!(Err(Closed()), future::block_on(r));
}

#[test]
fn close_receiver_one_thread() {
    let (s,r) = oneshot::<bool>();
    r.close();
    assert_eq!(Err(Closed()), s.send(true));
}

#[test]
fn success_two_threads() {
    let (s,r) = oneshot::<bool>();
    let t = spawn(|| future::block_on(r));
    assert_eq!((), s.send(true).unwrap());
    assert_eq!(Ok(true), t.join().unwrap());
}

#[test]
fn close_sender_two_threads() {
    let (s,r) = oneshot::<bool>();
    let j = spawn(|| future::block_on(r));
    s.close();
    assert_eq!(Err(Closed()), j.join().unwrap());
}

#[test]
fn wait_for_receiver() {
    let (s,r) = oneshot::<bool>();
    let j = spawn(move || future::block_on(async {
        let s = s.wait().await?;
        s.send(true)
    }));
    assert_eq!(Ok(true), future::block_on(r));
    assert_eq!(Ok(()), j.join().unwrap());
}
