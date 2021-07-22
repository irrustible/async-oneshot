use criterion::*;
// use futures_micro::or;
// use futures_lite::future::{block_on, FutureExt};
// use core::task::{Poll, Context};
use core::pin::Pin;
use async_hatch::*;

pub fn create_destroy(c: &mut Criterion) {
    // c.bench_function(
    //     "ref/create_destroy",
    //     |b| b.iter_batched(|| (), |_| {
    //         let mut h = Hatch::default();
    //         ref_hatch::<usize>(unsafe { Pin::new_unchecked(&mut h) })
    //     }, BatchSize::SmallInput)
    // );
}

// #[allow(unused_must_use)]
// pub fn send(c: &mut Criterion) {
//     let mut group = c.benchmark_group("send");
//     group.bench_function(
//         "success - receiver not listening",
//         |b| b.iter_batched(
//             || oneshot::<usize>(),
//             |(mut send, recv)| { (send.send(1).unwrap(), recv) },
//             BatchSize::SmallInput
//         )
//     );
//     group.bench_function(
//         "failure - receiver dropped",
//         |b| b.iter_batched(
//             || oneshot::<usize>().0,
//             |mut send| send.send(1).unwrap_err(),
//             BatchSize::SmallInput
//         )
//     );
// }

// pub fn try_recv(c: &mut Criterion) {
//     let mut group = c.benchmark_group("try_recv");
//     group.bench_function(
//         "success",
//         |b| b.iter_batched(
//             || {
//                 let (mut send, recv) = oneshot::<usize>();
//                 send.send(1).unwrap();
//                 recv
//             },
//             |mut recv| recv.try_recv().unwrap(),
//             BatchSize::SmallInput
//         )
//     );
//     group.bench_function(
//         "empty",
//         |b| b.iter_batched(
//             || oneshot::<usize>(),
//             |(send, mut recv)| (recv.try_recv().unwrap_err(), send),
//             BatchSize::SmallInput
//         )
//     );
//     group.bench_function(
//         "dropped",
//         |b| b.iter_batched(
//             || oneshot::<usize>().1,
//             |mut recv| recv.try_recv().unwrap_err(),
//             BatchSize::SmallInput
//         )
//     );
// }

// pub fn recv(c: &mut Criterion) {
//     let mut group = c.benchmark_group("async.recv");
//     group.bench_function(
//         "success",
//         |b| b.iter_batched(
//             || {
//                 let (mut send, recv) = oneshot::<usize>();
//                 send.send(42).unwrap();
//                 recv
//             },
//             |mut recv| block_on(recv.recv()).unwrap(),
//             BatchSize::SmallInput
//         )
//     );
//     group.bench_function(
//         "dropped",
//         |b| b.iter_batched(
//             || oneshot::<usize>().1,
//             |mut recv| block_on(recv.recv()).unwrap_err(),
//             BatchSize::SmallInput
//         )
//     );
// }

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
    // send,
    // try_receive,
    // receive,
    // wait,
);
criterion_main!(benches);
