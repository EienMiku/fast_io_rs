# Development notes

## Architecture

- `Presence<R>` attaches availability to a hook's return type. Ordinary results
  are present, private `Missing<R>` values are absent.
- `Encode` provides streaming, borrowed, reserved and composite batch paths.
- `Sink` provides all-or-error-prefix writes, optional scatter output, and an
  optional whole-argument hook. Memory sinks use the latter for one contiguous
  reservation; other sinks retain the scatter path.
- `Batch` stores borrowed slices or scratch offsets. It does not store references
  into itself. Adjacent scratch regions merge into one fragment.
- `Arguments` is a statically typed list used by print macros. Its constant
  `RESERVABLE` propagates field capabilities through composite formatters,
  including joins and optional values.
- `Source` exposes a buffered byte slice. `Scanner` parses contiguous tokens in
  place and joins split tokens into bounded scratch storage.

All memory access in this crate is currently safe Rust. Optional dependencies
retain their own implementation and licensing.

## Implemented

Presence and forwarding; slice/Vec/std sinks; integer/float/string formatting;
radix, quoted strings and byte-width alignment; output batches and composition;
persistent buffering; locked stdio and owned formatting macros; std fmt adapters;
bounded scanning, parameterized parsers and base-2-through-36 integers; borrowed
slice tokens; bounded records/UTF-8 lines; buffered input; explicit-endian
binary primitive I/O; stream copy and byte-limited source views; UTF-16/UTF-8
adapters; optional struct/enum derive; examples
and reproducible formatting/scanning/file-output microbenchmarks.

The derive supports per-type/per-variant layouts, skipping fields, transparent
wrappers, generics and renamed dependencies. Structs propagate reserve hooks;
enums combine their alternatives' capabilities through a balanced sum type.
Every variant must support reservation for the enum to advertise it. Arbitrary format
specifiers and empty enums are not supported.

## Important contracts

- A reserved size is an upper bound, not necessarily the exact output size.
  Exceeding a declared bound panics through checked indexing. No memory-safety
  invariant depends on an external formatter being correct.
- Optional hook bodies are not executed by `provided`. Absent hooks still have
  well-typed return values; extracting one panics. Both arms of a normal `if`
  must type-check, even with an inline const condition.
- Sinks may write a prefix before returning an error. Buffered output and batch
  drains discard the failed pending operation to avoid replaying that prefix.
- `Batch::finish` writes pending fragments; it does not flush the device.
  `Buffered::flush`/`finish` also flush the inner device. Buffered Drop is best
  effort only; Batch Drop performs no I/O.
- Invalid/oversized scan tokens are consumed completely; an I/O failure can lose
  a partially consumed token. `Input::into_inner` retains its `BufReader` so
  callers keep unread prefetched bytes.
- Slice token views borrow the original input, not the scanner. Binary helpers
  distinguish clean EOF from a truncated value.

## Validation policy

Use small, focused tests for I/O correctness, boundary values and dispatch.
CI covers stable Rust on Linux, Windows and macOS, plus MSRV and nightly on Linux.
Do not duplicate implementation details in tests or invent performance thresholds.

The current focused suite covers capability dispatch, numeric boundaries,
cross-buffer input, partial/interrupted writes, panic rollback, composition,
derive, padding, UTF-16 and binary I/O. Use `cargo test --workspace --all-features`,
`cargo test --no-default-features`, and a minimal-feature derive test when changing
shared contracts. Clippy runs with warnings denied; rustdoc also builds cleanly
with warnings denied. The last local checkpoint passed stable tests, Clippy,
rustdoc and a nightly all-target check.

See [PERFORMANCE.md](PERFORMANCE.md) for measured snapshots and limitations.
See [DESIGN.md](DESIGN.md) for the optional-hook mechanism and a codegen probe.

## Next priorities

- Measure pipe/socket/scatter paths before adding native handles or more complex
  OS specialization; the buffered file benchmark already checks matching bytes.
- Treat scatter presence as a functional capability, not a performance promise.
  Routing oversized buffered records through scatter regressed on Windows and
  was not adopted; see the large-record measurements before revisiting it.
- Improve usability and document sharp edges before broadening the API further.
- Native handles, async I/O, locale rules and additional encodings remain outside
  the implemented scope. The crate is unpublished and its API remains provisional.
