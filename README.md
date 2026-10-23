# fast_io_rs

Composable, statically dispatched I/O and formatting for stable Rust, inspired by
[C++ fast_io](https://github.com/cppfastio/fast_io).

This is an early, independent implementation. APIs are not stable, the crate is
not published, and feature/performance parity with upstream is not claimed.

```rust
use fast_io::{Buffered, Io, Sink};

let stdout = std::io::stdout();
let mut out = Buffered::<_>::new(Io::new(stdout.lock()));
fast_io::println!(&mut out;
    "answer=", 42, " hex=", fast_io::hex(255u32).prefixed(),
    " values=[", fast_io::join(&[1, 2, 3], ", "), "]"
)?;
out.flush()?;
# Ok::<(), std::io::Error>(())
```

## Current functionality

- Integers through 128 bits, UTF-8 strings/chars, raw bytes, bools and optional values.
- Binary/octal/hexadecimal/general radix output and JSON-compatible quoted strings.
- Optional shortest round-trip float formatting through [zmij](https://github.com/dtolnay/zmij).
- Borrowed fragments and generated scratch data combined into bounded scatter batches.
- Nested `concat!` and `join` values flatten into the outer batch.
- Fixed-capacity output, growable byte vectors, std writers and persistent buffering.
- `Display`, `Debug`, and `format_args!` interoperability.
- Bounded, whitespace-delimited scanning with integer overflow checking and buffered input.
- Stable Rust, `no_std`, and explicit allocation/std feature gates.

## Custom formatters

Implement `Encode::encode` for a streaming fallback. Optional methods return
`impl Presence<R>`; return the ordinary result to enable the capability.
Defaults represent absence. There is no separate capability flag to maintain.

| Optional hook | Purpose |
| --- | --- |
| `borrowed_bytes` | Borrow the complete representation |
| `reserve_size` + `encode_reserved` | Encode directly into a reserved scratch region |
| `encode_batch` | Add borrowed fields and generated data to an outer batch |

The dispatch uses RPITIT, associated constants and inline const blocks. Both arms
still type-check; it does not require specialization or nightly features.
See [the custom formatter example](examples/custom.rs) when available.

## Input

```rust
let mut input = fast_io::Scanner::new(b"123 -45 true".as_slice());
let (a, b, c) = fast_io::scan!(&mut input; u32, i32, bool)?;
assert_eq!((a, b, c), (123, -45, true));
# Ok::<(), fast_io::ScanError<core::convert::Infallible>>(())
```

`Scanner` uses ASCII whitespace and a 256-byte token limit by default.
`Scanner::<_, N>::with_capacity(source)` changes that limit. Contiguous tokens
are parsed in place; tokens crossing source buffers use bounded scratch storage.
Oversized and invalid tokens are consumed so later scans can recover.

## Features and checks

Default features: `std`, `float`. `std` enables `alloc`.
Disable defaults for the dependency-free `no_std` core; enable `alloc` for
growable memory output or `float` for zmij-based formatting.

```text
cargo test --all-features
cargo test --no-default-features
cargo clippy --all-targets --all-features -- -D warnings
cargo run --example hello
cargo run --example scan_sum
cargo run --release --example throughput -- 500000
```

The benchmark measures formatting into a reused Vec, not disk or OS throughput.
Performance is being measured and tuned; no universal speedup is claimed.

## Scope

Native OS handle types, transcoding, locale-aware formatting, asynchronous I/O,
and the full C++ fast_io customization surface are not implemented.
Output errors can follow a partially written prefix; callers must not retry the
whole operation blindly. Explicitly flush/finish persistent buffers to observe errors.

MIT licensed. No upstream C++ source is vendored.
