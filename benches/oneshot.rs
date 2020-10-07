use criterion::*;
use futures_lite::future::block_on;
use async_oneshot::oneshot;

pub fn create_destroy(c: &mut Criterion) {
    c.bench_function(
        "create_destroy",
        |b| b.iter(|| oneshot::<usize>())
    );
}

#[allow(unused_must_use)]
pub fn send_succeed(c: &mut Criterion) {
    c.bench_function(
        "send_succeed",
        |b| b.iter_batched(
            || oneshot::<usize>(),
            |(send, recv)| {
                send.send(1).unwrap();
                recv
            },
            BatchSize::PerIteration
        )
    );
}

pub fn send_fail(c: &mut Criterion) {
    c.bench_function(
        "send_fail",
        |b| b.iter_batched(
            || oneshot::<usize>(),
            |(send, recv)| {
                black_box(recv);
                send.send(1).unwrap_err();
            },
            BatchSize::PerIteration
        )
    );
}

pub fn try_recv_succeed(c: &mut Criterion) {
    c.bench_function(
        "try_recv_succeed",
        |b| b.iter_batched(
            || {
                let (send, recv) = oneshot::<usize>();
                send.send(1).unwrap();
                recv
            },
            |recv| recv.try_recv().unwrap(),
            BatchSize::PerIteration
        )
    );
}

pub fn try_recv_empty(c: &mut Criterion) {
    c.bench_function(
        "try_recv_empty",
        |b| b.iter_batched(
            || oneshot::<usize>(),
            |(send, recv)| (recv.try_recv().unwrap_err(), send),
            BatchSize::SmallInput
        )
    );
}

pub fn try_recv_fail(c: &mut Criterion) {
    c.bench_function(
        "try_recv_fail",
        |b| b.iter_batched(
            || oneshot::<usize>().1,
            |recv| recv.try_recv().unwrap_err(),
            BatchSize::PerIteration
        )
    );
}

criterion_group!(
    benches
        , create_destroy
        , send_succeed
        , send_fail
        , try_recv_succeed
        , try_recv_empty
        , try_recv_fail
);
criterion_main!(benches);
