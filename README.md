# async-oneshot

[![License](https://img.shields.io/crates/l/async-oneshot.svg)](https://github.com/irrustible/async-oneshot/blob/main/LICENSE)
[![Package](https://img.shields.io/crates/v/async-oneshot.svg)](https://crates.io/crates/async-oneshot)
[![Documentation](https://docs.rs/async-oneshot/badge.svg)](https://docs.rs/async-oneshot)

A fast, small, full-featured, async-aware oneshot channel.

Features:

* Blazing fast! See `Performance` section below.
* Tiny code, only one dependency and a lightning quick build.
* Complete `no_std` support (with `alloc` for `Arc`).
* Unique feature: sender may wait for a receiver to be waiting.

## Usage

```rust
#[test]
fn success_one_thread() {
    let (s,r) = oneshot::<bool>();
    assert_eq!((), s.send(true).unwrap());
    assert_eq!(Ok(true), future::block_on(r));
}
```

## Performance

async-oneshot comes with a benchmark suite which you can run with
`cargo bench`.

All benches are single-threaded and take double digit nanoseconds on
my machine. async benches use `futures_lite::future::block_on` as an
executor.

### Numbers from my machine

Here are benchmark numbers from my primary machine, a Ryzen 9 3900X
running alpine linux 3.12 that I attempted to peg at maximum cpu:

```
create_destroy          time:   [54.134 ns 54.150 ns 54.167 ns]
send/success            time:   [15.399 ns 15.673 ns 15.929 ns]
send/closed             time:   [45.022 ns 45.177 ns 45.321 ns]
try_recv/success        time:   [43.752 ns 43.924 ns 44.078 ns]
try_recv/empty          time:   [14.934 ns 15.244 ns 15.544 ns]
try_recv/closed         time:   [45.867 ns 46.048 ns 46.211 ns]
async.recv/success      time:   [45.851 ns 46.004 ns 46.141 ns]
async.recv/closed       time:   [45.501 ns 45.730 ns 45.948 ns]
async.wait/success      time:   [77.326 ns 77.372 ns 77.419 ns]
async.wait/closed       time:   [47.672 ns 47.841 ns 48.002 ns]
```

In short, we are very fast. Close to optimal, I think.

### Compared to other libraries

The oneshot channel in `futures` isn't very fast by comparison.

Tokio put up an excellent fight and made us work hard to improve. In
general I'd say we're slightly faster overall, but it's incredibly
tight.

## Note on safety

This crate uses UnsafeCell and manually synchronises with atomic
bitwise ops for performance. We believe it is now correct, but we
would welcome more eyes on it.

# See Also

* [async-oneshot-local](https://github.com/irrustible/async-oneshot-local) (single threaded)
* [async-spsc](https://github.com/irrustible/async-spsc) (SPSC)
* [async-channel](https://github.com/stjepang/async-channel) (MPMC)

## Changelog

### (unreleased)

* Better benchmarks, based on criterion.

### v0.4.0

Breaking changes:

* `Sender.wait()`'s function signature has changed to be a non-`async
  fn` returning an `impl Future`. This reduces binary size, runtime
  and possibly memory usage too. Thanks @zserik!

Fixes:

 * Race condition where the sender closes in a narrow window during
   receiver poll and doesn't wake the Receiver. Thanks @zserik!

Improvements:

 * Static assertions. Thanks @zserik!

### v0.3.3

Improvements:

* Update `futures-micro` and improve the tests

### v0.3.2

Fixes:

* Segfault when dropping receiver. Caused by a typo, d'oh! Thanks @boardwalk!

### v0.3.1

Improvements:

* Remove redundant use of ManuallyDrop with UnsafeCell. Thanks @cynecx!

### v0.3.0

Improvements:

* Rewrote, benchmarked and optimised.

### v0.2.0

* First real release.

## Copyright and License

Copyright (c) 2020 James Laver, async-oneshot contributors.

This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at http://mozilla.org/MPL/2.0/.
