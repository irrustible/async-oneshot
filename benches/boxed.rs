#![allow(unused_must_use,unused_mut)]
use criterion::*;
use async_hatch::*;
use core::pin::Pin;
use core::mem::ManuallyDrop;
use core::ops::DerefMut;

#[cfg(feature="async")]
use wookie::*;

fn create_destroy(c: &mut Criterion) {
    let mut group = c.benchmark_group("boxed");
    group.throughput(Throughput::Elements(1));
    group.bench_function(
        "create_destroy",
        |b| b.iter(|| hatch::<usize>())
    );
}

fn send_now(c: &mut Criterion) {
    let mut group = c.benchmark_group("boxed/send_now");
    group.throughput(Throughput::Elements(1));
    group.bench_function(
        "empty",
        |b| b.iter_batched_ref(
            || hatch::<usize>(),
            |(ref mut s, _r)| s.send(42).now(),
            BatchSize::SmallInput
        )
    );
    group.bench_function(
        "waited",
        |b| b.iter_batched_ref(
            || {
                let (mut s, mut r) = hatch::<usize>();
                {
                    leaky_dummy!(f: r.receive());
                    f.poll();
                }
                (s, r)
            },
            |(ref mut send, _recv)| { send.send(42).now() },
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
            |(ref mut s, _r)| s.send(42).now(),
            BatchSize::SmallInput
        )
    );
    group.bench_function(
        "dropped",
        |b| b.iter_batched_ref(
            || hatch::<usize>().0,
            |s| s.send(42).now(),
            BatchSize::SmallInput
        )
    );
    group.bench_function(
        "closed",
        |b| b.iter_batched_ref(
            || {
                // this is annoying because we have to get the
                // receiver to close.
                let (mut s, mut r) = hatch::<usize>();
                {
                    let f = r.receive().close_on_receive(true);
                    dummy!(f);
                    s.send(42).now();
                    f.as_mut().poll();
                }
                (s, r)
            },
            |(ref mut s, _r)| s.send(42).now(),
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
                {
                    dummy!(f: r.receive().close_on_receive(true));
                    s.send(42).now();
                    f.as_mut().poll();
                }
                s.send(42).now();
                (s, r)
            },
            |(ref mut s, _r)| s.send(42).now(),
            BatchSize::SmallInput
        )
    );
 }

#[cfg(feature="async")]
fn send_await(c: &mut Criterion) {
    let mut group = c.benchmark_group("boxed/send_await");
    group.throughput(Throughput::Elements(1));
    group.bench_function(
        "empty/first",
        |b| b.iter_batched_ref(
            || hatch::<usize>(),
            |(ref mut s, _r)| {
                dummy!(f: s.send(42));
                f.poll()
            },
            BatchSize::SmallInput
        )
    );
    group.bench_function(
        "empty/second",
        |b| b.iter_batched_ref(
            || {
                let (mut s, mut r) = hatch::<usize>();
                // first we have to fill the channel up
                {
                    dummy!(f: s.send(42));
                    f.poll();
                }
                // then we have to poll it to fail
                {
                    leaky_dummy!(f: s.send(42));
                    f.poll();
                }
                // then we have to line it up to succeed
                {
                    dummy!(f: r.receive());
                    f.poll();
                }
                (s, r)
            },
            |(ref mut s, _r)| {
                dummy!(f: s.send(42));
                f.poll()
            },
            BatchSize::SmallInput
        )
    );
    group.bench_function(
        "waited/first",
        |b| b.iter_batched_ref(
            || {
                let (mut s, mut r) = hatch::<usize>();
                {
                    leaky_dummy!(f: r.receive());
                    f.poll();
                }
                (s, r)
            },
            |(ref mut s, _r)| {
                dummy!(f: s.send(42));
                f.poll()
            },
            BatchSize::SmallInput
        )
    );
    group.bench_function(
        "waited/second",
        |b| b.iter_batched_ref(
            || {
                let (mut s, mut r) = hatch::<usize>();
                // first we have to fill the channel up
                {
                    dummy!(f: s.send(42));
                    f.poll();
                }
                // then we have to poll it to fail
                {
                    leaky_dummy!(f: s.send(42));
                    f.poll();
                }
                // then we have to line it up to succeed
                {
                    dummy!(f: r.receive());
                    f.poll();
                }
                {
                    leaky_dummy!(f: r.receive());
                    f.poll();
                }
                (s, r)
            },
            |(ref mut s, _r)| {
                dummy!(f: s.send(42));
                f.poll()
            },
            BatchSize::SmallInput
        )
    );
    group.bench_function(
        "full/first",
        |b| b.iter_batched_ref(
            || {
                let (mut s, r) = hatch::<usize>();
                s.send(42).now();
                (s, r)
            },
            |(ref mut s, _r)| {
                dummy!(f: s.send(42));
                f.poll()
            },
            BatchSize::SmallInput
        )
    );
    group.bench_function(
        "full/second",
        |b| b.iter_batched_ref(
            || {
                let (mut s, r) = hatch::<usize>();
                s.send(42).now();
                {
                    leaky_dummy!(f: s.send(42));
                    f.poll();
                }
                (s, r)
            },
            |(ref mut s, _r)| {
                dummy!(f: s.send(42));
                f.poll()
            },
            BatchSize::SmallInput
        )
    );
    group.bench_function(
        "dropped/first",
        |b| b.iter_batched_ref(
            || hatch::<usize>().0,
            |s| {
                dummy!(f: s.send(42));
                f.poll()
            },
            BatchSize::SmallInput
        )
    );
    group.bench_function(
        "dropped/second",
        |b| b.iter_batched_ref(
            || {
                let (mut s, _r) = hatch::<usize>();
                s.send(42).now();
                {
                    leaky_dummy!(f: s.send(42));
                    f.poll();
                }
                s
            },
            |s| {
                dummy!(f: s.send(42));
                f.poll()
            },
            BatchSize::SmallInput
        )
    );
    group.bench_function(
        "closed",
        |b| b.iter_batched_ref(
            || {
                let (mut s, mut r) = hatch::<usize>();
                {
                    // the only way to get a close without a drop is to succeed.
                    dummy!(f: r.receive().close_on_receive(true));
                    s.send(42).now();
                    f.as_mut().poll();
                }
                (s, r)
            },
            |(ref mut s, _r)| {
                dummy!(f: s.send(42));
                f.poll()
            },
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
                {
                    dummy!(f: r.receive().close_on_receive(true));
                    s.send(42).now();
                    f.as_mut().poll();
                }
                s.send(42).now();
                (s, r)
            },
            |(ref mut s, _r)| {
                dummy!(f: s.send(42));
                f.poll()
            },
            BatchSize::SmallInput
        )
    );
}

fn send_now_closing(c: &mut Criterion) {
    let mut group = c.benchmark_group("boxed/send_now_closing");
    group.throughput(Throughput::Elements(1));
    group.bench_function(
        "empty",
        |b| b.iter_batched_ref(
            || hatch::<usize>(),
            |(ref mut send, _recv)| { send.send(42).close_on_send(true).now() },
            BatchSize::SmallInput
        )
    );
    group.bench_function(
        "waited",
        |b| b.iter_batched_ref(
            || {
                let (mut s, mut r) = hatch::<usize>();
                {
                    leaky_dummy!(f: r.receive());
                    f.poll();
                }
                (s, r)
            },
            |(ref mut send, _recv)| { send.send(42).close_on_send(true).now() },
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
            |(ref mut s, _r)| s.send(42).close_on_send(true).now(),
            BatchSize::SmallInput
        )
    );
    group.bench_function(
        "dropped",
        |b| b.iter_batched_ref(
            || hatch::<usize>().0,
            |send| send.send(42).close_on_send(true).now(),
            BatchSize::SmallInput
        )
    );
    group.bench_function(
        "closed",
        |b| b.iter_batched_ref(
            || {
                // this is annoying because we have to get the
                // receiver to close.
                let (mut s, mut r) = hatch::<usize>();
                {
                    dummy!(f: r.receive().close_on_receive(true));
                    s.send(42).now();
                    f.as_mut().poll();
                }
                (s, r)
            },
            |(ref mut send, _r)| send.send(42).close_on_send(true).now(),
            BatchSize::SmallInput
        )
    );
}

#[cfg(feature="async")]
fn send_await_closing(c: &mut Criterion) {
    let mut group = c.benchmark_group("boxed/send_await_closing");
    group.throughput(Throughput::Elements(1));
    group.bench_function(
        "empty",
        |b| b.iter_batched_ref(
            || hatch::<usize>(),
            |(ref mut s, _recv)| {
                dummy!(f: s.send(42).close_on_send(true));
                f.poll()
            },
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
            |(ref mut s, _recv)| {
                dummy!(f: s.send(42).close_on_send(true));
                f.poll()
            },
            BatchSize::SmallInput
        )
    );
    group.bench_function(
        "dropped",
        |b| b.iter_batched_ref(
            || hatch::<usize>().0,
            |ref mut s| {
                dummy!(f: s.send(42).close_on_send(true));
                f.poll()
            },
            BatchSize::SmallInput
        )
    );
    group.bench_function(
        "closed",
        |b| b.iter_batched_ref(
            || {
                // this is annoying because we have to get the
                // receiver to close.
                let (mut s, mut r) = hatch::<usize>();
                {
                    dummy!(f: r.receive().close_on_receive(true));
                    s.send(42).now();
                    f.as_mut().poll();
                }
                (s, r)
            },
            |(ref mut s, _recv)| {
                dummy!(f: s.send(42).close_on_send(true));
                f.poll()
            },
            BatchSize::SmallInput
        )
    );
}

fn receive_now(c: &mut Criterion) {
    let mut group = c.benchmark_group("boxed/receive_now");
    group.throughput(Throughput::Elements(1));
    group.bench_function(
        "empty",
        |b| b.iter_batched_ref(
            || hatch::<usize>(),
            |(_, ref mut r)| r.receive().now(),
            BatchSize::SmallInput
        )
    );
    group.bench_function(
        "full",
        |b| b.iter_batched_ref(
            || {
                let (mut send, recv) = hatch::<usize>();
                send.send(42).now();
                (send, recv)
            },
            |(_, ref mut r)| r.receive().now(),
            BatchSize::SmallInput
        )
    );
    group.bench_function(
        "empty_dropped",
        |b| b.iter_batched_ref(
            || hatch::<usize>().1,
            |ref mut r| r.receive().now(),
            BatchSize::SmallInput
        )
    );
    group.bench_function(
        "full_closed",
        |b| b.iter_batched_ref(
            || {
                let (mut s, r) = hatch::<usize>();
                s.send(42).close_on_send(true).now();
                (s, r)
            },
            |(_, ref mut r)| r.receive().now(),
            BatchSize::SmallInput
        )
    );
    group.bench_function(
        "full_dropped",
        |b| b.iter_batched_ref(
            || {
                let (mut send, recv) = hatch::<usize>();
                send.send(42).close_on_send(true).now();
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
                send.send(42).close_on_send(true).now();
                recv.receive().now();
                (send, recv)
            },
            |(_, ref mut r)| r.receive().now(),
            BatchSize::SmallInput
        )
    );
}

fn receive_await(c: &mut Criterion) {
    let mut group = c.benchmark_group("boxed/receive_await");
    group.throughput(Throughput::Elements(1));
    group.bench_function(
        "empty",
        |b| b.iter_batched_ref(
            || hatch::<usize>(),
            |(_, ref mut r)| {
                let mut fut = ManuallyDrop::new(Dummy::new(r.receive()));
                let mut f = unsafe { Pin::new_unchecked((&mut fut).deref_mut()) };
                f.as_mut().poll()
            },
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
            |(_, ref mut r)| {
                dummy!(f: r.receive());
                f.as_mut().poll()
            },
            BatchSize::SmallInput
        )
    );
    group.bench_function(
        "empty_sender_dropped",
        |b| b.iter_batched_ref(
            || hatch::<usize>().1,
            |ref mut r| {
                dummy!(f: r.receive());
                f.as_mut().poll()
            },
            BatchSize::SmallInput
        )
    );
    group.bench_function(
        "full_sender_closed",
        |b| b.iter_batched_ref(
            || {
                let (mut s, r) = hatch::<usize>();
                s.send(42).close_on_send(true).now();
                (s, r)
            },
            |(_, ref mut r)| {
                dummy!(f: r.receive());
                f.as_mut().poll()
            },
            BatchSize::SmallInput
        )
    );
    group.bench_function(
        "full_sender_dropped",
        |b| b.iter_batched_ref(
            || {
                let (mut s, r) = hatch::<usize>();
                s.send(42).close_on_send(true).now();
                r
            },
            |ref mut r| {
                dummy!(f: r.receive());
                f.as_mut().poll()
            },
            BatchSize::SmallInput
        )
    );
    group.bench_function(
        "lonely",
        |b| b.iter_batched_ref(
            || {
                let (mut send, mut recv) = hatch::<usize>();
                send.send(42).close_on_send(true).now();
                {
                    dummy!(f: recv.receive());
                    f.as_mut().poll();
                }
                (send, recv)
            },
            |(_, ref mut recv)| {
                dummy!(f: recv.receive());
                f.as_mut().poll()
            },
            BatchSize::SmallInput
        )
    );
}

// it turns out to be quite hard to write a lot of these :/
#[cfg(feature="async")]
fn wait(c: &mut Criterion) {
    let mut group = c.benchmark_group("boxed/wait");
    group.throughput(Throughput::Elements(1));
    group.bench_function(
        "unwaited/leaky/first",
        |b| b.iter_batched_ref(
            || hatch::<usize>(),
            |(ref mut s, _)| {
                leaky_dummy!(f: s.wait());
                f.poll()
            },
            BatchSize::SmallInput
        )
    );
    group.bench_function(
        "unwaited/leaky/second",
        |b| b.iter_batched_ref(
            || {
                let (mut s, r) = hatch::<usize>();
                {
                    leaky_dummy!(f: s.wait());
                    f.poll();
                }
                (s, r)
            },
            |(ref mut s, _)| {
                leaky_dummy!(f: s.wait());
                f.poll()
            },
            BatchSize::SmallInput
        )
    );
    group.bench_function(
        "unwaited/cancelling/first",
        |b| b.iter_batched_ref(
            || hatch::<usize>(),
            |(ref mut s, _)| {
                dummy!(f: s.wait());
                f.poll()
            },
            BatchSize::SmallInput
        )
    );
    group.bench_function(
        "unwaited/cancelling/second",
        |b| b.iter_batched_ref(
            || {
                let (mut s, r) = hatch::<usize>();
                {
                    leaky_dummy!(f: s.wait());
                    f.poll();
                }
                (s, r)
            },
            |(ref mut s, _)| {
                dummy!(f: s.wait());
                f.poll()
            },
            BatchSize::SmallInput
        )
    );
    group.bench_function(
        "waited/first",
        |b| b.iter_batched_ref(
            || {
                let (s, mut r) = hatch::<usize>();
                {
                    leaky_dummy!(f: r.receive());
                    f.poll();
                }
                (s, r)
            },
            |(ref mut s, _)| {
                dummy!(f: s.wait());
                f.poll()
            },
            BatchSize::SmallInput
        )
    );
    group.bench_function(
        "waited/second",
        |b| b.iter_batched_ref(
            || {
                let (mut s, mut r) = hatch::<usize>();
                {
                    leaky_dummy!(f: s.wait());
                    f.poll();
                    leaky_dummy!(g: r.receive()); 
                    g.poll();
                }
                (s, r)
            },
            |(ref mut s, _)| {
                dummy!(f: s.wait());
                f.poll()
            },
            BatchSize::SmallInput
        )
    );
    group.bench_function(
        "dropped",
        |b| b.iter_batched_ref(
            || hatch::<usize>().0,
            |send| {
                dummy!(f: send.wait());
                f.poll()
            },
            BatchSize::SmallInput
        )
    );
    group.bench_function(
        "closed",
        |b| b.iter_batched_ref(
            || {
                let (mut s, mut r) = hatch::<usize>();
                s.send(42).now();
                r.receive().close_on_receive(true).now();
                (s, r)
            },
            |(ref mut s, _)| {
                dummy!(f: s.wait());
                f.poll()
            },
            BatchSize::SmallInput
        )
    );
}

#[cfg(feature="async")]
fn one_shot(c: &mut Criterion) {
    let mut group = c.benchmark_group("boxed/oneshot");
    group.throughput(Throughput::Elements(1));
    group.bench_function(
        "send_now_receive_await",
        |b| b.iter(|| {
            let (mut s, mut r) = oneshot::<usize>();
            s.send(42).now();
            dummy!(f: r.receive());
            f.poll()
        })
    );
}


#[cfg(feature="async")]
criterion_group!(
    benches,
    create_destroy,
    send_now,
    send_now_closing,
    send_await,
    send_await_closing,
    receive_now,
    receive_await,
    wait,
    one_shot,
);

#[cfg(not(feature="async"))]
criterion_group!(
    benches,
    create_destroy,
    send_now,
    send_now_closing,
    receive_now,
);

criterion_main!(benches);
