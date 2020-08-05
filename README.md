# async-oneshot

<!-- [![License](https://img.shields.io/crates/l/async-oneshot.svg)](https://github.com/irrustible/async-oneshot/blob/main/LICENSE) -->
<!-- [![Package](https://img.shields.io/crates/v/async-oneshot.svg)](https://crates.io/crates/async-oneshot) -->
<!-- [![Documentation](https://docs.rs/async-oneshot/badge.svg)](https://docs.rs/async-oneshot) -->

A fast and small async-aware oneshot channel.

Features:

* Fast and small.
* Complete `no_std` support.

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
test create           ... bench:         131 ns/iter (+/- 10)
test create_send      ... bench:         142 ns/iter (+/- 6)
test create_send_recv ... bench:         156 ns/iter (+/- 33)
```

So even on this thing, 6.5 million create-send-receives a second?

## Copyright and License

Copyright (c) 2020 James Laver, async-oneshot contributors.

This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at http://mozilla.org/MPL/2.0/.
