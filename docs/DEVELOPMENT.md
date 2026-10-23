# Development handoff

## Session constraints

Work was authorized through 2026-10-24 01:00 UTC+08:00 (2026-10-23 17:00 UTC).
Implement as much useful fast_io functionality as feasible, using focused tests and
frequent commits. Upgrade both stable and nightly toolchains. Stable is the implementation target.

Repository: D:\\Coding\\fast_io_rs
Conversation: 01a16423-850f-7f82-8dcf-2964c75b44cd
Requested remote: https://github.com/EienMiku/fast_io_rs

## Design decisions

The user proposed a verified stable-Rust capability mechanism:
- Trait hooks return impl Presence<R>.
- Default implementations return a private Missing<R> marker.
- An ordinary R implements Presence<R> with PRESENT = true.
- A const helper infers the return type from a function item without calling it.
- An inline const condition dispatches; both branches remain type checked.
- Borrowed outputs, generic methods, downstream implementations and forwarding were verified.

Formatting exposes a mandatory streaming fallback plus optional borrowed-byte and
reserved-buffer paths. A batch planner stores borrowed fragments and scratch offsets
without self-references, then flushes to a sink. Device fast paths also use Presence.
Avoid nightly generic const expressions; fixed bounded scratch storage is acceptable.

## Initial implementation sequence

1. Presence protocol and basic sink/formatter contracts.
2. Built-in strings, bytes, integers; std output and in-memory sinks.
3. Scratch/scatter planner and heterogeneous print macros.
4. Buffering and formatter adapters.
5. Focused boundary tests, examples, small reproducible benchmarks, CI.
6. Additional useful functionality if time remains.

## Progress

- Independent repository initialized.
- Toolchain upgrades running.
- No implementation yet.
