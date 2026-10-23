# fast_io_rs

Composable, statically dispatched I/O and formatting for stable Rust, inspired by
[C++ fast_io](https://github.com/cppfastio/fast_io).

This is an early, independent implementation. APIs are not stable, the crate is
not published, and feature/performance parity with upstream is not claimed.

```toml
[dependencies]
fast-io = { git = "https://github.com/EienMiku/fast_io_rs", features = ["derive"] }
```

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
- Byte-width alignment and padding, with conditional reserved output.
- Optional shortest round-trip float formatting through [zmij](https://github.com/dtolnay/zmij).
- Borrowed fragments and generated scratch data combined into bounded scatter batches.
- Nested `concat!` and `join` values flatten into the outer batch.
- Compositions, joins and optional values propagate their fields' reserve support.
- Fixed-capacity output, growable byte vectors, std writers and persistent buffering.
- `Display`, `Debug`, and `format_args!` interoperability.
- Bounded, whitespace-delimited scanning with integer overflow checking and buffered input.
- Zero-copy token views for slice inputs; explicit-endian binary integer/float I/O.
- Bounded delimiter-separated records and UTF-8 lines with error recovery.
- Stream copying through existing source buffers and byte-limited source views.
- Strict and lossy UTF-16 to UTF-8 conversion and UTF-16 LE/BE byte output.
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
See [the custom formatter example](examples/custom.rs).
The [protocol design notes](docs/DESIGN.md) explain capability propagation and
include a reproducible inspection of unoptimized code generation.

With the optional `derive` feature, structs can declare their layout directly:

```rust
#[derive(fast_io::Encode)]
#[encode("User({name}, {id})")]
struct User<'a> {
    name: &'a str,
    id: u64,
}
```

The derive generates streaming and shared-batch implementations, and advertises
reserved output exactly when all formatted fields support it. It supports named
and tuple structs, `{field}` / `{0}` placeholders, `{{` / `}}` escapes,
`#[encode(skip)]` fields and `#[encode(transparent)]` single-field wrappers.
Use `#[encode(crate = "renamed_dependency")]` if the crate has been renamed.
Raw identifiers use their plain field name in placeholders (`r#type` becomes `{type}`).
Enums support per-variant format/transparent attributes. They advertise reserved
output when all their variants' formatted fields support it; otherwise streaming
and shared-batch output remain available. Empty enums, unions and format
specifiers inside placeholders currently require a manual implementation.

With `std`, `fast_io::println!("answer=", 42)?` locks stdout for the operation;
`eprint!` / `eprintln!` target stderr. With `alloc`, `format_bytes!` returns a Vec,
and `format_text!` validates UTF-8 and returns a Result containing a String.

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
Use `scanner.scan_radix::<u64>(16)` for an explicit base from 2 through 36
(no `0x` prefix), or `scan_with(parser)` to supply a parser with captured context.
For a slice source, `scan_bytes()` and `scan_str()` return views into the original
input that remain valid while later tokens are read.

`read_record(&mut source, delimiter, &mut buffer)` reads a bounded record and
returns its length, excluding the delimiter. `read_line(&mut source, &mut buffer)`
returns a UTF-8 view with LF/CRLF removed. Both consume oversized records before
returning an error, allowing the next call to recover.

`copy(&mut source, &mut sink)` transfers bytes through the source's existing
buffer. Wrap a source with `take(source, byte_limit)` to read or copy a bounded
payload without consuming the following data. Copying does not flush the sink.

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
cargo run --example custom --features derive
cargo run --release --example throughput -- 500000
cargo run --release --example scan_throughput -- 200000
cargo run --release --example file_throughput -- 100000
```

The examples measure reused-Vec formatting, in-memory scanning, and buffered file
output separately. The file benchmark includes OS caching and does not sync to disk.
Performance is being measured and tuned; no universal speedup is claimed.
See [measured snapshots and methodology](docs/PERFORMANCE.md).

## Scope

Native OS handle types, codecs beyond UTF-8/UTF-16, locale-aware formatting, asynchronous I/O,
and the full C++ fast_io customization surface are not implemented.
Output errors can follow a partially written prefix; callers must not retry the
whole operation blindly. Explicitly flush/finish persistent buffers to observe errors.

MIT licensed. No upstream C++ source is vendored.
