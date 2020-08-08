# async-oneshot

[![License](https://img.shields.io/crates/l/async-oneshot.svg)](https://github.com/irrustible/async-oneshot/blob/main/LICENSE)
[![Package](https://img.shields.io/crates/v/async-oneshot.svg)](https://crates.io/crates/async-oneshot)
[![Documentation](https://docs.rs/async-oneshot/badge.svg)](https://docs.rs/async-oneshot)

A fast and small async-aware oneshot channel.

Features:

* Fast and small, with easy to understand code.
* Only two dependencies, both mine and with no deps.
* Complete `no_std` support (with `alloc` for `Arc`).

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

Crap numbers from my shitty 2015 macbook pro:

```
test create                ... bench:         122 ns/iter (+/- 12)
test create_send           ... bench:         122 ns/iter (+/- 21)
test create_send_recv      ... bench:         126 ns/iter (+/- 8)
test create_wait_send_recv ... bench:         232 ns/iter (+/- 29)
```

The measurement overhead seems to be a huge part of these times.

## Note on safety

Yes, this crate uses unsafe. 10 times. Not all of it is performance
gaming. Please audit carefully!

## Copyright and License

Copyright (c) 2020 James Laver, async-oneshot contributors.

This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at http://mozilla.org/MPL/2.0/.
