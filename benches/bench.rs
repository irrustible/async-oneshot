#![feature(test)]

extern crate test;
use test::{black_box, Bencher};
use async_oneshot::*;
use futures_lite::future::{block_on, join};

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
        black_box(send.send(true).unwrap());
        black_box(recv)
    })
}

#[bench]
fn create_send_recv(b: &mut Bencher) {
    b.iter(|| {
        let (send, recv) = oneshot::<bool>();
        black_box(send.send(true).unwrap());
        black_box(block_on(recv).unwrap());
    })
}

#[bench]
fn create_wait_send_recv(b: &mut Bencher) {
    b.iter(|| {
        let (send, recv) = oneshot::<bool>();
        black_box(
            block_on(
                join(
                    async {
                        let send = send.wait().await.unwrap();
                        send.send(true).unwrap()
                    },
                    async { recv.await.unwrap() }
                )
            )
        );
    });
}
