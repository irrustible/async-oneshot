# async-hatch

[![License](https://img.shields.io/crates/l/async-hatch.svg)](https://github.com/irrustible/async-hatch/blob/main/LICENSE)
[![Package](https://img.shields.io/crates/v/async-hatch.svg)](https://crates.io/crates/async-hatch)
[![Documentation](https://docs.rs/async-hatch/badge.svg)](https://docs.rs/async-hatch)

Easy-to-use single-message async channel. Formerly async-oneshot.

## Status: beta

This is now a mature project that we have confidence in the
correctness of. We expect to do a 1.0 release soon.

## Introduction

`async-hatch` is a swiss army knife for synchronisation between two
threads or async tasks.

`channels` are a popular way of passing messages between async
tasks. Each channel has a `Sender` for sending messages and a
`Receiver` for receiving them.

`async-hatch` is a channel that can hold one message at a time. It is
small, easy to use, feature rich and *very* fast.

Summary:

* Easy, convenient, powerful API.
* Small, low-dependency, fast to compile.
* Blazing fast! See `Performance` section below.
* Full no-std support (with or without an allocator!).

Features:

* Sender can wait for Receiver to listen (lazy send).
* 

## Usage

Simple example:

```rust
async fn simple() {
  let (mut s, mut r) = async_hatch::hatch::<i32>();
  s.send(42).now();
  assert_eq!(r.receive().await, Ok(42));
  s.send(420).now();
  assert_eq!(r.receive().await, Ok(420));
}
```

While the defaults will suit most people, there are many ways to use
`async-hatch` - it's a great building block for *lots* of async
patterns!

Hatches come with recovery support. When side closes, the other side
may create a new peer:

```
async fn recovery() {
  let (s, mut r) = async_hatch::hatch::<i32>();
  mem::drop(r);
  assert_eq!(s.send(42).now(), Err(SendError::Closed(42)))
  let r = s.recycle();
  mem::drop(s);
  assert_eq!(r.receive().await, Err(Closed));
  let s = r.recycle();
}
```

In overwrite mode, the `Sender` can overwrite an existing
message. This is useful if you only care about the most recent value:

```rust
async fn sender_overwrite() {
  let (mut s, mut r) = async_hatch::hatch::<i32>();
  s.send(42).now();
  assert_eq!(s.set_overwrites(true).send(420).now(), Ok(()));
  assert_eq!(r.receive().await, Ok(420));
}
```

### Managing memory yourself


* `RECLAIMS` (`set_reclaims`)
  
  This flag is checked when we observe the other side has closed and
  `RECYCLES` is not set (i.e. during cleanup). It has no effect if the
  hatch is backed by a `Box`, only if you are managing the memory yourself.
  
  A `true` indicates that we should stamp the atomic flags in the
  `Hatch` with a special `RECLAIMABLE` flag indicating that it is no
  longer in use. 



### Crate features

Default features: `alloc`, `async`, `spin_loop_hint`

`alloc`: enables boxes via the global allocator.
`async`: enables async features.
`spin_loop_hint`: enables calling `core::hint::spin_loop` in spin loops.

You probably want to leave these as they are. However...

* Disabling `alloc` will let you compile without a global allocator at
  the cost of convenience.
* Disabling `async` makes closing slightly cheaper at the cost of all
  async functionality.
* Disabling `spin_loop_hint` will probably increase your power
  consumption and is only advised for unusual circumstances. As a
  general rule, contention on two-party channels is already low, so we
  expect spinning to be minimal.

## Performance

tl; dr: probably strictly faster than whatever you're using right now.

This library has been carefully optimised to achieve excellent
performance. It has extra features that can enable you to squeeze even
more performance out of it in some situations.

### Performance Tips

Note: we are very very fast. We are probably not your bottleneck.

* Make use of the ability to reuse and recycle - they're cheaper than
  calling the destructors.
* Setting `closes(true)` before your last (or only!) operation makes
  the destructor cheaper.
* The unsafe API allows you to gain more performance in some
  situations by reducing synchronisation. Much of it is only useful in
  the presence of external synchronisation or by exploiting knowledge
  of your program's structure.

### Microbenchmarks

Microbenchmarks are terrible. You can really only use them as a vague
indication of performance and to guide library design. If you need
more than this, benchmark production workloads!

Nonetheless, here are some incredibly unscientific numbers from my
primary dev machine, a Ryzen 9 3900X running alpine linux edge.

```

```

Note: I had emacs and firefox and various things open and i'm using an
ondemand cpu scheduler, you could do better with this hardware!

### Implementation notes

Our performance largely derives from the following:

* Fixed size of shared storage:
  * 1 value, 2 wakers
* Minimal synchronisation:
  * Single atomic for refcount and lock.
  * All ops are sub-CAS, sub-SeqCst.
  * Maximise use of local state
  * Spinlocks and short critical sections with minimal branching.

Some of this falls out quite naturally from the basic problem
structure, some of it's just careful design.

On top of this, we wanted to be able to support situational
performance - extra performance you can tap into if your situation
permits it:

* Reuse or recycling may allow you to avoid the synchronisation cost
  of the destructors.
* Externally managed memory support provides full allocator control.
* Extensive unsafe API for when your situation permits avoiding checks
  and synchronisation.

## Copyright and License

Copyright (c) 2021 James Laver, async-hatch contributors.

[Licensed](LICENSE) under Apache License, Version 2.0 (https://www.apache.org/licenses/LICENSE-2.0),
with LLVM Exceptions (https://spdx.org/licenses/LLVM-exception.html).

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
licensed as above, without any additional terms or conditions.
