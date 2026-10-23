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
- Decimal output checks its output span once and fills complete pairs with a
  reverse chunk iterator; an odd leading digit uses the iterator's remainder.
  Values fitting u32 use a narrow converter. Larger u64 values split into
  eight-digit chunks, with zero-padded tails converted using u32 arithmetic.
  Nondecimal output uses mask/shift for power-of-two bases; general bases retain
  checked output spans and division. The dedicated radix example compares
  direct writes, whole-argument reservations and std on identical u128 inputs.
- `Arguments` is a statically typed list used by print macros. Its constant
  `RESERVABLE` propagates field capabilities through composite formatters,
  including joins and optional values.
  Its recursive reserved-output step and thin adapters inline deliberately:
  leaving these as separate functions materialized field-pointer lists and
  measurably slowed derived structs/enums in the local benchmark.
  Derived layouts build balanced argument trees; print macros group long calls
  in chunks of eight. This keeps ordinary wide records below default compiler
  recursion limits without asking users for a crate-level attribute.
  An owned argument node keeps temporary radix/Debug wrappers alive through
  deferred reserved-output queries without returning references to locals.
- `Source` exposes a buffered byte slice. `Scanner` parses contiguous tokens in
  place and joins split tokens into bounded scratch storage. `scan_fold` instead
  visits borrowed chunks with no token scratch array, retaining the length limit
  and consuming failed tokens before reporting callback/length errors.
  The optional `read_into` Presence hook lets raw reads bypass intermediate
  buffering. `Input` preserves std BufReader prefetch before bypassing; `Take`
  and mutable references preserve direct reads. Reported lengths are checked.
  A second optional hook, `buffered_bytes`, exposes cached input without I/O.
  Exact reads fitting that cache copy and consume directly; other reads use a
  separate refill loop. Slice sources, both buffer wrappers and Take preserve
  this capability. Empty cache and end of input are distinct conditions.
- `Scan` retains a complete-token parser and adds an optional `scan_stream`
  Presence hook. Decimal integer implementations parse contiguous tokens
  directly and fold split tokens without `[u8; N]` scratch. A shared boundary
  loop serves the generic `scan_fold` and these integer parsers, preserving
  token limits, byte error offsets and oversized-token recovery. Even a
  `usize::MAX` token limit is usable for primitive integer scans. `ScanRadix`
  has an equivalent optional `scan_radix_stream` hook, so primitive explicit
  bases 2..36 also avoid token scratch, with decimal delegating to `Scan`.
  Complete-token custom parsers (`scan_with`, or `Scan`/`ScanRadix` without
  streaming hooks) still allocate N stack bytes for split tokens.
  The tiny-buffer benchmark compares streaming and copied tokens.
  Complete decimal parsers accumulate a provably fitting prefix without overflow
  checks and check each remaining digit. This also applies to long tokens rather
  than repeating overflow checks for the safe prefix of every 20-digit u64.
  On 64-bit targets this fitting prefix converts validated groups of eight ASCII
  digits using byte lanes in a u64. Invalid groups fall back to scalar validation
  for the original error offset; smaller targets retain scalar accumulation.
  Cached decimal integer scans combine numeric conversion and delimiter
  detection, avoiding a separate whitespace pass on valid contiguous tokens.
  Missing cache, errors, oversized tokens and buffer boundaries fall back to
  the existing framing/recovery path. Signed values accumulate negatively when
  needed so their minimum value remains representable. The speculative cached
  path examines at most N+1 token bytes, even for arbitrarily long leading zeros.
  With alloc, String/Vec stream split tokens into their result instead of an
  N-byte scratch array; String validates UTF-8 once the token is complete.
- `Reader` is the minimal raw-device input trait. `BufferedInput<R, N>` adds
  fixed-capacity buffering without std/alloc, exposes `Source`, and bypasses an
  empty buffer for large direct reads. `Io<R>` adapts std readers and retries
  interruptions. `BufferedInput::into_inner` discards its unread buffer; inspect
  `buffer()`, keep the wrapper, or use `into_parts` to retain unread bytes.
  With std, both input wrappers implement std Read/BufRead when the device error
  is std::io::Error, so consumers can switch APIs without dropping prefetch.

All memory access in this crate is currently safe Rust. Optional dependencies
retain their own implementation and licensing.

## Implemented

Presence and forwarding; slice/Vec/std sinks; integer/float/string formatting;
fixed/scientific float precision with core::fmt conversion and reserved bounds
(fixed-point bounds use the value's binary exponent, including rounding carry);
radix, quoted strings and byte-width alignment; output batches and composition;
persistent buffering; locked stdio and owned formatting macros; std fmt adapters;
bounded scanning, incremental token folds, parameterized parsers and base-2-through-36 integers; borrowed
slice tokens; bounded records/UTF-8 lines; buffered input; explicit-endian
binary primitive I/O; stream copy and byte-limited source views; UTF-16/UTF-8
adapters; raw partial/exact reads; existing BufReader reuse; optional struct/enum derive; examples
and reproducible formatting/scanning/binary-file-input/file-output/pipe/loopback-TCP
microbenchmarks. The binary input benchmark checks both buffering implementations
against std and against a source wrapper without the direct-read hook.

The derive supports per-type/per-variant layouts, skipping fields, transparent
wrappers, generics and renamed dependencies. Adjacent literal text is merged
at expansion time to reduce fragments and streaming writes. Structs propagate reserve hooks;
enums combine their alternatives' capabilities through a balanced sum type.
Every variant must support reservation for the enum to advertise it. Arbitrary
field widths, dynamic precision and empty enums are not supported. Explicit field
formats `x`, `X`, `b`, `o` (optionally prefixed with `#`) use the unsigned radix
wrappers; `?` uses Debug. Bounds follow the chosen formatter, so a Debug-only
field need not implement Encode. Radix layouts retain reserved output; Debug
layouts use streaming/batching. Uppercase hexadecimal uses `0X` consistently
with the direct wrapper. Unsupported specifiers are compile errors.
With float enabled, `.N`, `.Ne`, and `.NE` use the fixed/scientific formatters
with a u16 literal precision. They retain reserved output and add only the Float
bound to the affected field; unused float support does not affect no_std builds.

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
- `Buffered::into_parts` and `BufferedInput::into_parts` recover the owned
  device plus `BufferedBytes<N>` without I/O. The returned bytes implement
  Source/Encode and preserve the logical pending range without allocation.
  Output bytes already discarded after a failed drain cannot be recovered.
- Invalid/oversized scan tokens are consumed completely; an I/O failure can lose
  a partially consumed token. `Input::into_inner` retains its `BufReader` so
  callers keep unread prefetched bytes.
- Slice token views borrow the original input, not the scanner. Binary helpers
  distinguish clean EOF from a truncated value.

## Validation policy

Use small, focused tests for I/O correctness, boundary values and dispatch.
CI covers stable Rust on Linux, Windows and macOS, plus MSRV and nightly on Linux.
Stable CI also checks `thumbv6m-none-eabi` (Cortex-M0, no pointer atomics), both
the dependency-free core and the optional alloc/float/derive combination. These
are cross-compilation checks, not execution on embedded hardware.
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

- Compare the upstream C++ implementation with matched workloads using
  `tools/compare_upstream.py`; see `UPSTREAM_BENCHMARK.md` for the external
  pinned checkout, byte checks, and the current hosted-header build limitation.
  Test short as well as full-width integers before changing token framing:
  word-at-a-time whitespace-search experiments helped long tokens but regressed
  1–3 digit inputs, and were not adopted.
- Measure non-loopback sockets and varied pipe workloads before adding native
  handles or more complex OS specialization. File, pipe and loopback TCP
  benchmarks check matching bytes and distinguish per-record scatter from
  persistent buffering. The pipe receiver validates in a child process.
- Treat scatter presence as a functional capability, not a performance promise.
  Routing oversized buffered records through scatter regressed on Windows and
  was not adopted; see the large-record measurements before revisiting it.
- Improve usability and document sharp edges before broadening the API further.
- Native handles, async I/O, locale rules and additional encodings remain outside
  the implemented scope. The crate is unpublished and its API remains provisional.
