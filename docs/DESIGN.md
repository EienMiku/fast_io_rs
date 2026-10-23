# Optional capabilities on stable Rust

The core goal is to let ordinary formatters implement only streaming output,
while richer formatters expose information a device can use to plan a write.
The same value can therefore format into a fixed buffer, a Vec, a buffered file,
or a scatter-capable writer without building an intermediate String.

## Presence is attached to a return type

Each optional hook has a return-position `impl Presence<R>` result. Its default
returns a private, zero-sized `Missing<R>` with `PRESENT = false`. An override
can simply return an `R`: the blanket implementation marks ordinary results as
present and returns them unchanged from `into_value()`.

`provided(T::borrowed_bytes)` uses the method's function-item type to infer its
return type. It reads the associated constant; it never calls the method or
constructs its result. The `Copy` bound on the function item avoids generic
destruction during const evaluation.

```rust
if const { provided(T::borrowed_bytes) } {
    sink.write_all(value.borrowed_bytes().into_value())
} else {
    value.encode(sink)
}
```

This is an ordinary Rust `if` with a const-evaluated condition. Both arms must
type-check. All types have the same hook signatures, so the absent arm remains
well-typed; extracting `Missing` would panic if called. No trait specialization,
generic const expressions, nightly feature, or user-maintained capability flag
is required.

The public protocols target static dispatch. Generic methods and opaque return
types mean `Encode` and `Sink` are not interfaces for `dyn` trait objects.

## What the planner knows

| Information | Hook | Use |
| --- | --- | --- |
| Existing complete byte representation | `borrowed_bytes` | Retain a borrow in a scatter batch or copy directly to memory |
| Upper bound and direct encoder | `reserve_size` + `encode_reserved` | Reserve one region and generate bytes into it |
| Composite fields | `encode_batch` | Flatten children into the outer batch |
| Streaming implementation only | `encode` | Emit through the sink without whole-value planning |
| Device can optimize the argument list | `Sink::write_arguments` | Use a contiguous memory/buffer path |
| Device can accept multiple fragments | `Sink::write_scatter` | Use vectored writes with partial-write handling |

A bound need not be exact, and it may depend on the value. The runtime retains
bounds checks around user code. An incorrect bound can produce a panic, not an
unchecked memory write. The library does not infer length or borrowing behavior
from arbitrary encoder bodies.

## Composition and device choice

The print macros borrow their arguments into a heterogeneous tree of
`Cons`/`End` leaves and `Joined` subtrees. Long invocations group eight values per
step; derive builds a balanced tree directly. This avoids a separate macro and
trait recursion level for every field in a wide record. Values are evaluated
once, left to right, and temporary arguments live through the operation. The
tree's `RESERVABLE` constant is a conjunction of its fields' capabilities.

The reserved encoder inlines the recursive tree steps and thin capability
adapters. A local benchmark found that leaving these as separate functions
materialized a stack list of field pointers and slowed derived values. The
conversion routines themselves can still remain separate functions. This is a
code-generation choice with potential code-size costs, not a stronger type-level
guarantee; see [the measurements](PERFORMANCE.md#derived-records).

Vec and fixed-buffer sinks can reserve a complete argument list once. Persistent
buffers do the same when the upper bound fits; they drain when more room is
needed. A fixed buffer falls back to streaming when the upper bound is too
large, since the actual representation may still fit.

Other devices use a bounded `Batch`. It retains external borrows and represents
generated fragments as offsets into scratch storage, avoiding self-references.
Neighboring scratch fragments merge. Filling the byte or fragment capacity
drains the batch. Oversized transient writes pass through after pending output.

`concat!`, `join`, `Option` and derived structs propagate reservation support.
Derived enums use a balanced `ReservedChoice` sum type: the return type is
present only when every possible variant's fields support reservation. The
selected variant supplies the actual bound and encoder. This also uses stable
Rust without separate capability declarations in user code.

## Inspecting the generated branch

The small [presence probe](../tools/presence_probe.rs) is a manual code-generation
inspection aid, not a runtime test. From the repository root:

```text
cargo +stable build --release --no-default-features
rustc +stable --edition=2024 --crate-type=lib -C opt-level=0 --emit=llvm-ir,asm --out-dir target --extern fast_io=target/release/libfast_io.rlib -L dependency=target/release/deps tools/presence_probe.rs
```

On Rust 1.99.1 / Windows x86-64, `choose::<str>` stored 10 and `choose::<u64>`
stored 20 in their unoptimized LLVM IR. Each used an unconditional branch;
neither called `provided` nor emitted a conditional branch on `PRESENT`.
This verifies the capability decision in this probe, not the absence of all
overhead in a complete I/O operation. Capacity checks, partial-write loops,
formatting and runtime enum selection still have work to do.

## Error and lifetime boundaries

- All paths for one value must emit the same bytes. Size queries must not consume
  it. A fallback that measures output may require a repeatable encoder.
- Borrowed batch fragments live until the batch drains. Temporary byte arrays
  must be submitted through `write_all`, which copies or immediately writes.
- A sink error may follow a written prefix. Batch and buffered drains clear
  pending state before calling the sink, so a caught panic cannot replay it.
- Finishing a batch emits its fragments; flushing a persistent buffer also
  flushes the underlying device. Explicit finish/flush is how errors are observed.
- Invalid or oversized input records/tokens are consumed before returning an
  error. A source error can interrupt a partly consumed token or record.

The implementation currently uses safe Rust throughout its own runtime and
derive code. Dependencies retain their own implementations. See
[DEVELOPMENT.md](DEVELOPMENT.md) for scope and [PERFORMANCE.md](PERFORMANCE.md)
for measured workloads.
