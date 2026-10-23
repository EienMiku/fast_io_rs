# Development notes

## Architecture

- `Presence<R>` attaches availability to a hook's return type. Ordinary results
  are present, private `Missing<R>` values are absent.
- `Encode` provides streaming, borrowed, reserved and composite batch paths.
- `Sink` provides all-or-error-prefix writes and optional scatter output.
- `Batch` stores borrowed slices or scratch offsets. It does not store references
  into itself. Adjacent scratch regions merge into one fragment.
- `Arguments` is a statically typed list used by print macros.
- `Source` exposes a buffered byte slice. `Scanner` parses contiguous tokens in
  place and joins split tokens into bounded scratch storage.

All memory access in this crate is currently safe Rust. Optional dependencies
retain their own implementation and licensing.

## Implemented

Presence and forwarding; basic sinks; integer/float/string formatting; radix and
quoted strings; output batches and composition; persistent buffering; std fmt
adapters; bounded scanning; standard buffered input; examples and an initial
reproducible in-memory benchmark.

## Validation policy

Use small, focused tests for I/O correctness, boundary values and dispatch.
CI covers stable Rust on Linux, Windows and macOS, plus MSRV and nightly on Linux.
Do not duplicate implementation details in tests or invent performance thresholds.

## Next priorities

- Measure and optimize memory targets independently from scatter-capable devices.
- Add an ergonomic custom formatter example and optional derive support.
- Expand useful formatting adapters, documenting exact behavior.
- Document benchmark methodology and limitations, and test relevant feature sets.
- Consider native backends/transcoding only after the core contracts are settled.
