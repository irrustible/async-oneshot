# async-oneshot

<!--
[![License](https://img.shields.io/crates/l/async-oneshot.svg)](https://github.com/irrustible/async-oneshot/blob/main/LICENSE)
[![Package](https://img.shields.io/crates/v/async-oneshot.svg)](https://crates.io/crates/async-oneshot)
[![Documentation](https://docs.rs/async-oneshot/badge.svg)](https://docs.rs/async-oneshot)

A fast, small, full-featured, async-aware oneshot channel.

Features:

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

## Note on safety

This crate uses UnsafeCell and manually synchronises with atomic
bitwise ops for performance. We believe it is correct, but we
would welcome more eyes on it.

# See Also

* [async-oneshot-local](https://github.com/irrustible/async-oneshot-local) (single threaded)
* [async-spsc](https://github.com/irrustible/async-spsc) (SPSC)
* [async-channel](https://github.com/stjepang/async-channel) (MPMC)

## Note on benchmarking

The benchmarks are synthetic and a bit of fun.

## Changelog

### v0.5.0

Breaking changes:

* Make `Sender.send()` only take a mut ref instead of move.

### v0.4.2

Improvements:

* Added some tests to cover repeated fix released in last version.
* Inline more aggressively for some nice benchmark boosts.

### v0.4.1

Fixes:

* Remove some overzealous `debug_assert`s that caused crashes in
  development in case of repeated waking. Thanks @nazar-pc!

Improvements:

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

-->
## Copyright and License

Copyright (c) 2020 James Laver, async-oneshot contributors.

This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at http://mozilla.org/MPL/2.0/.
