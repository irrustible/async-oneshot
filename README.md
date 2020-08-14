# async-oneshot

[![License](https://img.shields.io/crates/l/async-oneshot.svg)](https://github.com/irrustible/async-oneshot/blob/main/LICENSE)
[![Package](https://img.shields.io/crates/v/async-oneshot.svg)](https://crates.io/crates/async-oneshot)
[![Documentation](https://docs.rs/async-oneshot/badge.svg)](https://docs.rs/async-oneshot)

A fast and small async-aware oneshot channel.

Features:

* Incredibly fast - comparable with `tokio` and much faster than `futures`.
* Tiny code, only one dependency and a blazing quick build.
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

As of version 0.3.0, we're about even with `tokio`. We've always been
somewhat faster than `futures`.

Here are some crap benchmark numbers from my shitty 2015 macbook
pro. 

```
test create                ... bench:         122 ns/iter (+/- 12)
test create_send           ... bench:         122 ns/iter (+/- 21)
test create_send_recv      ... bench:         126 ns/iter (+/- 8)
test create_wait_send_recv ... bench:         232 ns/iter (+/- 29)
```

Most of the time is actually taken up by measurement overhead and the
last bench is probably benching `futures_lite::future::block_on` more
than it's benching this library. We are, in short, very fast.

## Note on safety

Yes, this crate uses unsafe. 10 times. Not all of it is performance
gaming. Please audit carefully!

# See Also

* [async-oneshot-local](https://github.com/irrustible/async-oneshot-local) (single threaded)
* [async-channel](https://github.com/stjepang/async-channel) (MPMC)

## Copyright and License

Copyright (c) 2020 James Laver, async-oneshot contributors.

This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at http://mozilla.org/MPL/2.0/.
