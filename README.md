# fast_io_rs

A Rust implementation of composable, statically dispatched I/O and formatting,
inspired by the architecture of [C++ fast_io](https://github.com/cppfastio/fast_io).
Early development; APIs are not stable and the crate is not published.

The core design uses return-position impl Trait to advertise optional fast paths:
implement a method returning its ordinary result, and its return type carries the
capability flag. No nightly features or separate capability tags are required.

## Goals

- Stable Rust, a no_std core, optional allocation and std adapters.
- Borrow existing bytes, reserve formatter space, and batch scatter writes.
- Keep custom formatters small: a required streaming fallback, optional fast paths.
- Correct partial-write and error handling before throughput claims.
- Small, reviewable commits and focused tests.

## Status

The repository is being built incrementally. See [development notes](docs/DEVELOPMENT.md)
for architecture, current progress, and the next implementation steps.

MIT licensed. This is an independent implementation, not an official upstream port.
