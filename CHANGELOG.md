# Changelog

## v1.0.0 (unreleased)

For this release, we wanted to achieve quite a lot and it basically amounted to a ground-up rewrite.

* Close control:
  * Multiple message support, you may re-use Sender/Receiver.
  * Optionally close on success as a microoptimisation.
* Recovery (Sender can replace a dropped Receiver and vice versa).
* External memory management support:
  * Use without an allocator.
  * Reclamation (allow manual reuse of a Hatch).
* Improve and gain confidence in our use of `unsafe`.
* Sender overwrite support.

Since we now support reuse, `oneshot` didn't seem like a good name
anymore, so we renamed it.

## v0.5.0 (async-oneshot)

Breaking changes:

* Make `Sender.send()` only take a mut ref instead of move `self`.

## v0.4.2 (async-oneshot)

Improvements:

* Added some tests to cover repeated fix released in last version.
* Inline more aggressively for some nice benchmark boosts.

## v0.4.1 (async-oneshot)

Fixes:

* Remove some overzealous `debug_assert`s that caused crashes in
  development in case of repeated waking. Thanks @nazar-pc!

Improvements:

* Better benchmarks, based on criterion.

## v0.4.0

Breaking changes:

* `Sender.wait()`'s function signature has changed to be a non-`async
  fn` returning an `impl Future`. This reduces binary size, runtime
  and possibly memory usage too. Thanks @zserik!

Fixes:

* Race condition where the sender closes in a narrow window during
  receiver poll and doesn't wake the Receiver. Thanks @zserik!

Improvements:

 * Static assertions. Thanks @zserik!

## v0.3.3

Improvements:

* Update `futures-micro` and improve the tests

## v0.3.2

Fixes:

* Segfault when dropping receiver. Caused by a typo, d'oh! Thanks @boardwalk!

## v0.3.1

Improvements:

* Remove redundant use of ManuallyDrop with UnsafeCell. Thanks @cynecx!

## v0.3.0

Improvements:

* Rewrote, benchmarked and optimised.

## v0.2.0

* First real release.
