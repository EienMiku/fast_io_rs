# Performance snapshots

These are paired, local microbenchmarks, not a claim of parity with C++ fast_io
or a prediction of disk, terminal, or network throughput.

## Environment and reproduction

- Windows x86-64, Intel Core i7-11800H @ 2.30 GHz.
- Rust 1.99.1 (`58cf8f9c3`, 2026-10-13), stable, default release profile.
- Five samples after warm-up; report each implementation's median.
- Runs alternate implementation order to reduce order effects.
- No CPU affinity, fixed frequency, or isolated machine was used. Compare paired
  results in one run; differences between snapshots include machine noise.

```text
cargo run --release --example throughput -- 500000
cargo run --release --example scan_throughput -- 200000
```

## Formatting

The workload formats a dynamic unsigned integer, signed integer, a static name,
a bool, separators and a newline into a reused `Vec<u8>`. Each record reuses the
allocation. The standard-library baseline uses `writeln!`; fast_io uses
`fast_io::println!`. The example checks matching output before measurement.

| Commit / implementation | std ns/record | fast_io ns/record |
| --- | ---: | ---: |
| `1b40e2c`, generic scatter planning for Vec | 115.39 | 199.97 |
| `b09b6f8`, memory sink bypasses scatter planning | 62.07 | 59.65 |
| `62b5057`, reserve and encode the whole argument list | 66.51 | 49.41 |

The first measurement exposed unnecessary scatter bookkeeping for memory output.
The device's optional `write_arguments` hook now chooses a contiguous reservation
when every value supplies a usable bound. A scatter-capable device still receives
borrowed fragments and generated scratch data. Formatting performance depends on
the values, buffer size, device and available hooks.

## Integer scanning

The workload contains 200,000 deterministic decimal `u64` tokens (4,079,493 bytes).
The baseline splits on ASCII whitespace and uses UTF-8 validation plus Rust's
integer parser. The other paths use `Scanner` over a slice or `Input` backed by
the same in-memory bytes. The example checks checksums before reporting timing.

| Commit / implementation | std ns/token | slice ns/token | buffered ns/token |
| --- | ---: | ---: | ---: |
| `daa6ed9`, initial scanner | 42.41 | 52.67 | 55.73 |
| `d394988`, streamlined scanning and short-integer parsing | 36.90 | 28.18 | 26.71 |

The parser validates digits and uses checked arithmetic for values that could
overflow. Short token lengths that prove overflow impossible use a simpler
arithmetic loop. This dataset does not characterize floats, Unicode processing,
error-heavy inputs, many tiny source buffers, or real I/O latency.

## What remains to measure

- File, pipe and socket output, including partial writes and scatter batching.
- Mixed-length strings, derived types and formatting adapters.
- Small source buffers, near-limit tokens and floating-point inputs.
- Allocation counts, generated machine code, and comparable C++ fast_io cases.

Benchmarks deliberately have no pass/fail speed thresholds. Correctness tests do
not depend on wall-clock performance.
