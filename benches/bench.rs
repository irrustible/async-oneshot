#![feature(test)]

extern crate test;
use test::{black_box, Bencher};
use async_oneshot::*;
use futures_lite::future::block_on;
// use std::pin::Pin;
// use waker_fn::waker_fn;

#[bench]
fn create(b: &mut Bencher) {
    b.iter(|| {
        black_box(oneshot::<bool>());
    })
}

#[bench]
fn create_send(b: &mut Bencher) {
    b.iter(|| {
        let (send, recv) = oneshot::<bool>();
        send.send(true).unwrap();
        black_box(recv)
    })
}

#[bench]
fn create_send_recv(b: &mut Bencher) {
    b.iter(|| {
        let (send, recv) = oneshot::<bool>();
        send.send(true).unwrap();
        block_on(recv).unwrap();
    })
}

// #[bench]
// fn create_wait_send_recv(b: &mut Bencher) {
//     let fake_waker = waker_fn(|| ());
//     b.iter(|| {
//         let (send, mut recv) = oneshot::<bool>();
//         block_on(async {
//             let mut wait = send.wait();
//             poll_once(&mut wait);
//             poll_once(&mut recv);
//             wait.await;
//             send.send(true).unwrap();
//             black_box(recv.await);
//         });
//     })
// }
