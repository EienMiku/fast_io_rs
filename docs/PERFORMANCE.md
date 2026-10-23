# Performance snapshots

These are paired, local microbenchmarks, not a claim of parity with C++ fast_io
or a prediction of uncached disk, terminal, or network throughput.

## Environment and reproduction

- Windows x86-64, Intel Core i7-11800H @ 2.30 GHz.
- Rust 1.99.1 (`58cf8f9c3`, 2026-10-13), stable, default release profile.
- Five samples for the original memory benchmarks and loopback sockets, nine
  for derived types, three for file output; report medians.
- Formatting runs alternate implementation order; the current scanner benchmark
  rotates it. Older scanner snapshots below used a fixed order without a warm-up.
- No CPU affinity, fixed frequency, or isolated machine was used. Compare paired
  results in one run; differences between snapshots include machine noise.

```text
cargo run --release --example throughput -- 500000
cargo run --release --example scan_throughput -- 200000
cargo run --release --example file_throughput -- 100000
cargo run --release --example file_throughput -- 1000 16384
cargo run --release --features derive --example derive_throughput -- 500000
cargo run --release --example socket_throughput -- 10000 32
cargo run --release --example socket_throughput -- 1000 16384
cargo run --release --example read_throughput -- 100000 32
cargo run --release --example read_throughput -- 1000 16384
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

## Derived records

The derive benchmark cycles through 1,024 prebuilt values with varying integers,
booleans and three string lengths. It checks every output against `writeln!`,
warms up all three paths, and rotates their measurement order across nine samples
of 500,000 records each. Input allocation and construction are outside timing;
formatting into the reused Vec and reading the next record are inside timing.
The enum dataset mixes full records (75%) with compact records (25%). Both the
standard and direct-argument baselines perform the same runtime variant match.

Initial measurement with the `70f8938` implementation:

| Dataset | std ns/record | direct arguments ns/record | derived Encode ns/record |
| --- | ---: | ---: | ---: |
| Struct | 80.56 | 43.69 | 51.07 |
| Mixed enum | 69.55 | 35.01 | 50.50 |

This exposes a measurable cost in these derived paths, particularly for enums.
Compile-time capability selection alone does not prove identical generated code.

Inlining the thin reserved-output adapters and the recursive argument-list step
removed separately emitted argument-list encoders from this benchmark's release
LLVM IR. Before that change, these encoders required a stack-resident list of
field pointers. A subsequent paired run measured:

| Dataset | std ns/record | direct arguments ns/record | derived Encode ns/record |
| --- | ---: | ---: | ---: |
| Struct | 82.11 | 41.70 | 38.34 |
| Mixed enum | 67.90 | 33.92 | 32.81 |

For these records, derived formatting now costs about the same as direct
arguments. The small differences within a row are not evidence that derive is
inherently faster. Inlining can also increase code size; larger layouts and
different compiler targets need separate measurement.

## Integer scanning

The workload contains 200,000 deterministic decimal `u64` tokens (4,079,493 bytes).
The current baseline validates the byte buffer as UTF-8, splits on ASCII
whitespace, and uses Rust's integer parser. The other paths use `Scanner` over a
slice or `Input` backed by the same in-memory bytes. The example checks checksums
before reporting timing. The two historical snapshots below validated UTF-8
outside the timed section and measured parsers in a fixed order, so they have
weaker controls than the current benchmark.

| Commit / implementation | std ns/token | slice ns/token | buffered ns/token |
| --- | ---: | ---: | ---: |
| `daa6ed9`, initial scanner | 42.41 | 52.67 | 55.73 |
| `d394988`, streamlined scanning and short-integer parsing | 36.90 | 28.18 | 26.71 |
| Warm-up + rotated order, timed UTF-8 validation (current recipe) | 24.00 | 24.91 | 27.62 |

The more controlled run puts this slice scanner close to the standard baseline,
with no demonstrated speed advantage. The earlier apparent advantage should not
be generalized; both benchmark method and machine conditions changed.

The parser validates digits and uses checked arithmetic for values that could
overflow. Short token lengths that prove overflow impossible use a simpler
arithmetic loop. This dataset does not characterize floats, Unicode processing,
error-heavy inputs, many tiny source buffers, or real I/O latency.

## Buffered file output

The file example writes the same mixed records through 8192-byte buffers:
`std::io::BufWriter<File>` with `writeln!` versus `Buffered<Io<File>, 8192>` with
`fast_io::println!`. It creates two fresh temporary files, warms up, alternates
three paired samples, and compares every output byte. File creation, seek and
truncation are outside timing; buffer setup and final flush are inside timing.
Files are removed afterwards. It does not call `sync_all`, so OS caching is a
major factor and these are not durable-storage bandwidth measurements.

For 100,000 records / 7,977,658 matching bytes in this environment:

| Implementation | ns/record | MB/s |
| --- | ---: | ---: |
| std BufWriter + writeln! | 117.00 | 681.82 |
| fast_io Buffered + println! | 75.01 | 1063.59 |

The result supports the contiguous-buffer formatting path for this workload.
Different filesystems, devices, buffer sizes and scheduling can change it.
The optional second argument replaces the short name with a payload of that
many bytes, exercising records larger than the persistent buffer.

With 1,000 records containing 16,384-byte payloads (16,451,732 total bytes), the
existing streaming fallback measured 9,785.80 ns/record for std and 9,939.10 for
fast_io. A trial routing these records through a new scatter batch measured
11,010.10 for std and 24,724.30 for fast_io; that change was not adopted. The
functional availability of scatter output alone does not establish that it is
the efficient path for a device. In particular, splitting a large record into
separate header/payload/trailer writes can defeat buffer coalescing.

## Loopback TCP output

The socket example creates a private loopback connection per sample, warms up
all paths and rotates five samples. All four paths use `TCP_NODELAY`; buffered
paths use 8192-byte buffers. The timer includes formatting, sending, shutting
down the write half, and completion of the receiving thread. Connections and
expected bytes are prepared before timing. Every received byte is compared
afterwards. Results include scheduler and local TCP-stack effects and do not
predict a remote network's performance.

| Implementation | 32-byte payload, ns/record | 16384-byte payload, ns/record |
| --- | ---: | ---: |
| std BufWriter + writeln! | 173.88 | 15406.90 |
| fast_io Buffered + println! | 92.16 | 18378.90 |
| std unbuffered writeln! | 58486.42 | 49448.00 |
| fast_io unbuffered scatter + println! | 9038.10 | 25591.70 |

The small case has 10,000 records / 548,890 bytes; the large case has 1,000
records / 16,405,890 bytes. Combining one record's fields into a scatter write
helps relative to unbuffered field-by-field formatting in this run. Persistent
buffering is much more effective for many small records because it can combine
multiple records per send. The large-payload buffered result does not show a
fast_io advantage. These are different batching policies, so their timing is not
evidence that an individual formatting routine or system call has that speedup.

## Binary file input

The input example reads an 8-byte header followed by a configurable payload for
each record, from one temporary file. It compares std BufReader, Input (backed by
std BufReader), inline BufferedInput, and an Input wrapper that deliberately
omits the optional direct-read hook. Every received byte is verified after each
sample. All paths have 8192-byte buffers, a warm-up, and seven rotated samples;
file generation, rewind, allocations and output initialization are outside
timing. The file is OS-cached, and reads target reused preallocated memory.

Initial results with the `104bc94` implementation:

| Implementation | 32-byte payload, ns/record | 16384-byte payload, ns/record |
| --- | ---: | ---: |
| std BufReader | 22.02 | 7251.40 |
| fast_io Input, direct hook | 32.59 | 8741.30 |
| fast_io BufferedInput, inline buffer | 28.74 | 8253.10 |
| fast_io source-buffer fallback | 26.91 | 7332.40 |

The small case has 100,000 records / 4,000,000 bytes; the large case has 1,000
records / 16,392,000 bytes. There is no demonstrated fast_io speed advantage in
this run. A direct-read capability can avoid an intermediate copy, but that does
not guarantee lower total cost: small headers, prefetch state and call overhead
also affect the result.

## What remains to measure

- Unbuffered files, pipes and non-loopback sockets under varied buffering policies.
- Formatting adapters and longer/more deeply nested derived types.
- Small source buffers, near-limit tokens and floating-point inputs.
- Allocation counts, generated machine code, and comparable C++ fast_io cases.

Benchmarks deliberately have no pass/fail speed thresholds. Correctness tests do
not depend on wall-clock performance.
