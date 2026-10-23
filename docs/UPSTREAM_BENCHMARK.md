# Comparing C++ fast_io and this Rust implementation

The drivers in `tools/upstream_compare.cpp` and `examples/upstream_compare.rs`
implement the same workloads. The C++ library remains an external checkout,
under its own license; no upstream source is vendored or patched here.

## Reproduce

Use Python 3.10+, stable Rust, clang++ with C++26 support and a working C++
standard library/linker. On Windows the driver also links ntdll. Clone upstream
outside this repository and pin its revision:

```text
git clone https://github.com/cppfastio/fast_io.git <external-directory>
git -C <external-directory> checkout 7a6fa9b680de8355f9f743e0f8950492cf5a42f0
python tools/compare_upstream.py --upstream <external-directory>
```

The default run takes nine rotated samples, formatting 2,000,000 records and
scanning 1,000,000 tokens per sample. `--count`, `--scan-count`, `--samples`, and
`--payload` customize the run. Payload zero selects mixed name lengths of
1/12/96 bytes; a positive value selects that fixed length. All lengths are bytes.
Results, raw timings, compiler versions, revisions, commands and relevant Rust
profile environment overrides are saved in `target/upstream-results.json`.
The runner rejects a modified upstream checkout or an unexpected revision;
`--allow-other-revision` permits an explicitly chosen revision and records it.

For a paired Rust before/after comparison, first copy the old driver from
`target/upstream-compare/rust/release/examples/upstream_compare` (`.exe` on Windows)
to a separate path. After editing the implementation, pass that saved path with
`--baseline-rust`. The runner rebuilds current Rust, verifies all three programs,
rotates all three in the same run, and records executable SHA-256 hashes. Keep
the driver workload unchanged between those builds.

`--cpu N` optionally pins the runner and future benchmark children to logical
CPU N on Windows or Linux. Compilation happens before pinning; no global CPU,
frequency or power settings change. The selected CPU is recorded in the report.
This reduces migration noise but does not isolate that CPU from other work.

`--modes scan-u8 scan-u32 scan` selects short, medium and full-width unsigned
decimal inputs. Values are masked to 8/32/64 bits, but all three parse into u64.
`--modes` can also select `slice` and `grow`. Additional scan modes require a
baseline driver built with the same mode support; the original three modes
retain their initial workloads.

## Measurement contract

- A deterministic table of 1,024 records includes zero, unsigned MAX, signed MIN,
  mixed signs, varying name lengths and booleans. Each output is
  `id=<u64> delta=<i64> name=<text> active=<true|false>\n`.
- `slice`: reuse already allocated storage, comparing Rust `SliceSink` with C++
  `obuffer_view`. Clear the logical length and format one record per iteration.
- `grow`: reuse reserved capacity, comparing Rust `Vec<u8>` with upstream's
  `fast_io::string` / `ostring_ref_fast_io`. Container bookkeeping, including
  Rust's initialization and C++'s terminator handling, remains in the timed work.
- `scan`: scan a contiguous input of unsigned decimal tokens followed by LF;
  compare Rust `Scanner<&[u8]>` with upstream's freestanding scan implementation
  over `ibuffer_view`. Both must visit all tokens and produce the same wrapping
  checksum. This measures valid inputs; error-recovery semantics differ.
- Input construction, allocations/reservation, process startup, verification
  and warm-up are outside the timer. Memory paths warm up 10,000 records; scanning
  warms up the whole input. Memory timing includes record selection, clear,
  formatting, the optimizer barrier and accumulating the written byte count.
- Rust uses `black_box`; C++ uses clang's empty inline assembly with memory
  clobbers. Each input record is opaque and each output byte range is exposed
  to the barrier. These prevent dead-code elimination; they are not hardware
  fences and need not have identical compiler effects.
- Before timing, each driver checks both memory paths against its standard
  library's formatting. The runner independently compares all 1,024 records
  byte-for-byte with Python's output and checks measured byte counts/checksums.
- Each sample is a separate process with internal timing. The runner rotates
  execution across implementations/workloads, reports medians and ranges, and
  retains every sample. CPU pinning is opt-in; frequency/load are uncontrolled.

The C++ build uses `-std=c++26 -O3 -DNDEBUG`, with exceptions enabled. Rust uses
the default stable release profile. Neither build requests native CPU tuning,
PGO or explicit LTO. Compiler/LLVM versions and optimization pipelines may
differ: this compares these library/compiler combinations, not language speed.
Safe checked Rust and the C++ upstream also offer different failure contracts.

## Measured comparisons

### Latest complete comparison

After the retained decimal input/output changes, the same three programs ran
in 15 rotated samples at Rust commit `7340dab`. The saved original Rust executable
is from `0c3358d`; the driver workloads are unchanged. Each memory sample formats
2,000,000 records and each scan sample parses 1,000,000 tokens.

| Workload | C++ median ns/unit | Original Rust | Current Rust | Current Rust range |
| --- | ---: | ---: | ---: | ---: |
| Reused fixed buffer | 31.27 | 40.17 | 39.98 | 32.35–57.31 |
| Reused growing container | 42.87 | 44.21 | 39.22 | 35.47–56.80 |
| Unsigned decimal scanning | 13.39 | 35.01 | 23.80 | 19.78–35.02 |

The growing-container results are close relative to sample variation. The
fixed-buffer gap to C++ remains, and the small Rust before/after difference here
is not conclusive. Scanning improved about 32% versus the original Rust binary
in this run, while still taking about 1.78 times as long as C++. This supports further work on the
parser implementation; it does not establish overhead inherent to the Presence
protocol. C++ ranges were 27.91–47.90, 35.59–57.70 and 11.28–17.71 ns/unit.
See [all samples and build metadata](benchmarks/upstream-final-2026-10-24.json).
An earlier complete pass overlapped with a stuck diagnostic process; that pass
was discarded and repeated after stopping it. The linked report is the rerun.
Only documentation changes were pending during this measurement.

### Original baseline

On 2026-10-24 (UTC+8), Windows x86-64 / Intel Core i7-11800H, using clang 22.1.6
and stable Rust 1.99.1 (LLVM 23.1.1), the default nine-sample run at Rust commit
`0c3358d` produced the following. Lower is better; memory units are records and
scan units are tokens.

| Workload | C++ median ns/unit | Rust median ns/unit | C++ range | Rust range |
| --- | ---: | ---: | ---: | ---: |
| Reused fixed buffer | 33.02 | 43.71 | 28.29–42.10 | 37.89–59.53 |
| Reused growing container | 44.31 | 45.22 | 39.08–50.57 | 43.30–93.64 |
| Unsigned decimal scanning | 15.14 | 33.62 | 12.46–17.16 | 30.14–46.77 |

The growing-container results are close relative to the observed variation.
The fixed-buffer median favors C++; scanning shows the clearest gap, with Rust
taking about 2.22 times as long in these paired samples. This is a baseline for
further investigation, not evidence of overall fast_io parity or a language-wide
performance difference. The machine was not isolated and individual samples
were noisy. [Raw samples and build metadata](benchmarks/upstream-baseline-2026-10-24.json)
are retained so later improvements can be compared with the same workload.

### Decimal prefix experiment

The initial Rust parser checked overflow on every digit if the *whole* token
was long. A follow-up reuses the proven-fitting prefix even for long tokens,
then checks each remaining digit. It keeps first-error ordering, signed MIN,
leading zeroes, and the same stream recovery rules. A nine-sample run using
`--baseline-rust` with the unchanged `0c3358d` executable measured:

| Scanner | Median ns/token | Range |
| --- | ---: | ---: |
| C++ upstream | 13.34 | 11.27–17.46 |
| Rust before | 32.52 | 29.07–36.97 |
| Rust with fitting prefix | 29.38 | 25.18–36.06 |

This run shows a modest local improvement (about 10% in the medians), with
overlapping ranges and a substantial remaining gap to C++. The preserved
[raw comparison](benchmarks/upstream-prefix-2026-10-24.json) includes the baseline
binary hash and the uncommitted `src/scan.rs` status from the experiment.

An additional 64-bit path converts the fitting prefix in groups of eight ASCII
digits, with scalar fallback for invalid groups. Its separate paired run measured
26.59 ns/token versus 32.70 for the scalar-prefix binary (about 19% lower median);
C++ measured 15.79. Ranges were 21.63–44.37, 26.21–41.81 and 11.62–35.39 respectively.
The [raw group-conversion comparison](benchmarks/upstream-chunks-2026-10-24.json)
again retains every sample. These noisy snapshots do not establish an improvement
on every input or target. A focused byte-lane check covers all 256 byte values in
each of the eight positions, including non-ASCII bytes and invalid controls.

Word-at-a-time whitespace searches were also tried separately from decimal
conversion. Broader `scan-u8`/`scan-u32`/`scan` comparisons found regressions on
short tokens despite improvements on full-width tokens. Those framing changes
were not retained; the shipping scanner keeps its scalar whitespace search.
The expanded workloads remain available to evaluate future changes.

### Decimal output iteration

The decimal formatter now fills complete two-byte chunks of its checked output
region. This removes per-iteration output slicing and the separate final-pair
case while retaining safe Rust. Values around every representable decimal power
are checked against standard formatting. A 15-sample paired run measured:

| Memory destination | C++ ns/record | Rust before | Rust after |
| --- | ---: | ---: | ---: |
| Fixed buffer | 31.68 | 40.82 | 37.11 |
| Growing container | 41.72 | 46.06 | 40.84 |

The local median reductions were about 9% and 11%; the corresponding ranges
still overlap. See the [raw output comparison](benchmarks/upstream-output-2026-10-24.json)
for all samples, exact builds and baseline executable hash. These results do
not show a universal improvement or parity with upstream on other workloads.

## Current upstream build limitation

At revision `7a6fa9b680de8355f9f743e0f8950492cf5a42f0`, including `fast_io.h`
with clang 22.1.6 on Windows/MSVC failed in `win32_network/socket_file.h` at an
unguarded `catch throws(::std::error)`. Including the std string adapter also
exposed missing `fast_io::basic_concat` declarations. The successful driver uses
the public `fast_io_freestanding.h` and `fast_io_dsal/string.h` headers, and calls
the same underlying formatting/scanning entry points without hosted I/O.

These results therefore do not cover upstream native file, socket or pipe I/O.
The limitation is a build observation for this pinned compiler/platform, not a
claim that the library cannot build in its intended custom compiler environment.
