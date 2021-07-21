use criterion::*;
// use futures_micro::or;
// use futures_lite::future::{block_on, FutureExt};
// use core::task::{Poll, Context};
use async_hatch::*;
use wookie::*;

pub fn create_destroy(c: &mut Criterion) {
    c.bench_function(
        "boxed/create_destroy",
        |b| b.iter_batched(|| (), |_| hatch::<usize>(), BatchSize::SmallInput)
    );
}

#[allow(unused_must_use)]
pub fn send_now(c: &mut Criterion) {
    let mut group = c.benchmark_group("boxed/send_now");
    group.bench_function(
        "empty",
        |b| b.iter_batched_ref(
            || hatch::<usize>(),
            |(ref mut s, _r)| { s.send(42).now().unwrap() },
            BatchSize::SmallInput
        )
    );
    // receiver listening turns out to be quite tricky, so we won't bother.
    // group.bench_function(
    //     "success - overwrite, receiver not listening",
    //     |b| b.iter_batched_ref(
    //         || {
    //             let (mut send, recv) = hatch::<usize>();
    //             send.overwrites(true).send(42).now().unwrap();
    //             (send, recv)
    //         },
    //         |(ref mut send, _recv)| { send.send(42).now().unwrap() },
    //         BatchSize::SmallInput
    //     )
    // );
    group.bench_function(
        "full",
        |b| b.iter_batched_ref(
            || {
                let (mut s, r) = hatch::<usize>();
                s.send(42).now();
                (s, r)
            },
            |(ref mut s, _r)| s.send(42).now().unwrap_err(),
            BatchSize::SmallInput
        )
    );
    group.bench_function(
        "receiver_dropped",
        |b| b.iter_batched_ref(
            || hatch::<usize>().0,
            |s| s.send(42).now().unwrap_err(),
            BatchSize::SmallInput
        )
    );
    group.bench_function(
        "receiver_closed",
        |b| b.iter_batched_ref(
            || {
                // this is annoying because we have to get the
                // receiver to close.
                let (mut s, mut r) = hatch::<usize>();
                let f = r.receive().close_on_receive(true);
                woke!(f);
                s.send(42).now().unwrap();
                unsafe {
                    f.as_mut().poll();
                }
                (s, r)
            },
            |(ref mut s, _r)| s.send(42).now().unwrap_err(),
            BatchSize::SmallInput
        )
    );
    group.bench_function(
        "lonely",
        |b| b.iter_batched_ref(
            || {
                // this is annoying because we have to get the
                // receiver to close.
                let (mut s, mut r) = hatch::<usize>();
                let f = r.receive().close_on_receive(true);
                woke!(f);
                s.send(42).now().unwrap();
                unsafe {
                    f.as_mut().poll();
                }
                s.send(42).now();
                (s, r)
            },
            |(ref mut s, _r)| s.send(42).now().unwrap_err(),
            BatchSize::SmallInput
        )
    );
}

#[allow(unused_must_use)]
pub fn send_now_closing(c: &mut Criterion) {
    let mut group = c.benchmark_group("boxed/send_now_closing");
    group.bench_function(
        "empty",
        |b| b.iter_batched_ref(
            || hatch::<usize>(),
            |(ref mut send, _recv)| { send.send(42).close_on_send(true).now().unwrap() },
            BatchSize::SmallInput
        )
    );
    group.bench_function(
        "full",
        |b| b.iter_batched_ref(
            || {
                let (mut s, r) = hatch::<usize>();
                s.send(42).now();
                (s, r)
            },
            |(ref mut s, _r)| s.send(42).close_on_send(true).now().unwrap_err(),
            BatchSize::SmallInput
        )
    );
    group.bench_function(
        "receiver_dropped",
        |b| b.iter_batched_ref(
            || hatch::<usize>().0,
            |send| send.send(42).close_on_send(true).now().unwrap_err(),
            BatchSize::SmallInput
        )
    );
    group.bench_function(
        "receiver_closed",
        |b| b.iter_batched_ref(
            || {
                // this is annoying because we have to get the
                // receiver to close.
                let (mut s, mut r) = hatch::<usize>();
                let f = r.receive().close_on_receive(true);
                woke!(f);
                s.send(42).now();
                unsafe {
                    f.as_mut().poll();
                }
                (s, r)
            },
            |(ref mut send, _r)| send.send(42).close_on_send(true).now().unwrap_err(),
            BatchSize::SmallInput
        )
    );
}

#[allow(unused_must_use)]
pub fn receive_now(c: &mut Criterion) {
    let mut group = c.benchmark_group("boxed/receive_now");
    group.bench_function(
        "empty",
        |b| b.iter_batched_ref(
            || hatch::<usize>(),
            |(_, ref mut r)| r.receive().now().unwrap(),
            BatchSize::SmallInput
        )
    );
    group.bench_function(
        "full",
        |b| b.iter_batched_ref(
            || {
                let (mut send, recv) = hatch::<usize>();
                send.send(42).now().unwrap();
                (send, recv)
            },
            |(_, ref mut r)| r.receive().now().unwrap(),
            BatchSize::SmallInput
        )
    );
    group.bench_function(
        "empty_sender_dropped",
        |b| b.iter_batched_ref(
            || hatch::<usize>().1,
            |ref mut r| r.receive().now().unwrap_err(),
            BatchSize::SmallInput
        )
    );
    group.bench_function(
        "full_sender_closed",
        |b| b.iter_batched_ref(
            || {
                let (mut s, r) = hatch::<usize>();
                s.send(42).close_on_send(true).now().unwrap();
                (s, r)
            },
            |(_, ref mut r)| r.receive().now().unwrap(),
            BatchSize::SmallInput
        )
    );
    group.bench_function(
        "full_sender_dropped",
        |b| b.iter_batched_ref(
            || {
                let (mut send, recv) = hatch::<usize>();
                send.send(42).close_on_send(true).now().unwrap();
                recv
            },
            |ref mut recv| recv.receive().now(),
            BatchSize::SmallInput
        )
    );
    group.bench_function(
        "lonely",
        |b| b.iter_batched_ref(
            || {
                let (mut send, mut recv) = hatch::<usize>();
                send.send(42).close_on_send(true).now().unwrap();
                recv.receive().now();
                (send, recv)
            },
            |(_, ref mut r)| r.receive().now().unwrap_err(),
            BatchSize::SmallInput
        )
    );
}

#[allow(unused_must_use)]
pub fn receive_await(c: &mut Criterion) {
    let mut group = c.benchmark_group("boxed/receive_await");
    group.bench_function(
        "empty",
        |b| b.iter_batched_ref(
            || hatch::<usize>(),
            |(_, ref mut r)| {
                let f = r.receive();
                woke!(f);
                unsafe { f.as_mut().poll() };
            },
            BatchSize::SmallInput
        )
    );
    group.bench_function(
        "full",
        |b| b.iter_batched_ref(
            || {
                let (mut s, r) = hatch::<usize>();
                s.send(42).now().unwrap();
                (s, r)
            },
            |(_, ref mut r)| {
                let f = r.receive();
                woke!(f);
                unsafe { f.as_mut().poll() };
            },
            BatchSize::SmallInput
        )
    );
    group.bench_function(
        "empty_sender_dropped",
        |b| b.iter_batched_ref(
            || hatch::<usize>().1,
            |ref mut r| {
                let f = r.receive();
                woke!(f);
                unsafe { f.as_mut().poll() };
            },
            BatchSize::SmallInput
        )
    );
    group.bench_function(
        "full_sender_closed",
        |b| b.iter_batched_ref(
            || {
                let (mut s, r) = hatch::<usize>();
                s.send(42).close_on_send(true).now().unwrap();
                (s, r)
            },
            |(_, ref mut r)| {
                let f = r.receive();
                woke!(f);
                unsafe { f.as_mut().poll() };
            },
            BatchSize::SmallInput
        )
    );
    group.bench_function(
        "full_sender_dropped",
        |b| b.iter_batched_ref(
            || {
                let (mut s, r) = hatch::<usize>();
                s.send(42).close_on_send(true).now().unwrap();
                r
            },
            |ref mut r| {
                let f = r.receive();
                woke!(f);
                unsafe { f.as_mut().poll() };
            },
            BatchSize::SmallInput
        )
    );
    group.bench_function(
        "lonely",
        |b| b.iter_batched_ref(
            || {
                let (mut send, mut recv) = hatch::<usize>();
                send.send(42).close_on_send(true).now().unwrap();
                let f = recv.receive();
                woke!(f);
                unsafe { f.as_mut().poll() };
                (send, recv)
            },
            |(_, ref mut recv)| {
                let f = recv.receive();
                woke!(f);
                unsafe { f.as_mut().poll() };
            },
            BatchSize::SmallInput
        )
    );
}

// pub fn wait(c: &mut Criterion) {
//     let mut group = c.benchmark_group("async.wait");
//     group.bench_function(
//         "already_waiting",
//         |b| b.iter_batched(
//             || (oneshot::<usize>(), waker_fn(|| ())),
//             |((mut send, mut recv),waker)| {
//                 let mut f = Box::pin(
//                     or!(async { recv.recv().await.unwrap(); 1 },
//                         async { send.wait().await.unwrap(); 2 }
//                     )
//                 );
//                 let mut ctx = Context::from_waker(&waker);
//                 assert_eq!(f.poll(&mut ctx), Poll::Ready(2));
//             },
//             BatchSize::SmallInput
//         )
//     );
//      group.bench_function(
//         "will_wait",
//         |b| b.iter_batched(
//             || oneshot::<usize>(),
//             |(mut send, mut recv)| block_on(
//                 or!(async { send.wait().await.unwrap(); 2 },
//                     async { recv.recv().await.unwrap(); 1 }
//                 )
//             ),
//             BatchSize::SmallInput
//         )
//     );
//     group.bench_function(
//         "closed",
//         |b| b.iter_batched(
//             || oneshot::<usize>().0,
//             |mut send| block_on(send.wait()).unwrap_err(),
//             BatchSize::SmallInput
//         )
//     );
// }

criterion_group!(
    benches,
    create_destroy,
    send_now,
    send_now_closing,
    receive_now,
    receive_await
    // wait,
);
criterion_main!(benches);
