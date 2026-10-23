#!/usr/bin/env python3
"""Build and compare independent Rust/C++ drivers. Python 3.10+, git, cargo, clang++.

Pass an external upstream checkout; this script neither downloads nor edits it.
See docs/UPSTREAM_BENCHMARK.md for the pinned revision and measurement contract.
"""
import argparse
import ctypes
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import statistics
import subprocess
import tempfile

UPSTREAM_REVISION = "7a6fa9b680de8355f9f743e0f8950492cf5a42f0"
ROOT = Path(__file__).resolve().parents[1]
MASK = (1 << 64) - 1


def run(command, *, cwd=ROOT):
    result = subprocess.run(
        [str(arg) for arg in command], cwd=cwd, check=True, text=True,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0,
    )
    if result.stderr:
        print(result.stderr.rstrip(), flush=True)
    return result.stdout.strip()


def reference(payload):
    records, numbers = [], []
    for index in range(1024):
        value = [0, MASK, 1 << 63][index] if index < 3 else index * 6364136223846793005 & MASK
        signed = value if value < 1 << 63 else value - (1 << 64)
        name = chr(ord("a") + index % 26) * (payload or [1, 12, 96][index % 3])
        active = "true" if index % 3 else "false"
        records.append(f"id={value} delta={signed} name={name} active={active}\n".encode())
        numbers.append(value)
    return records, numbers


def repeated_total(values, count):
    whole, remainder = divmod(count, len(values))
    return whole * sum(values) + sum(values[:remainder])


def pin_cpu(cpu):
    """Constrain only this runner and its future benchmark children."""
    if cpu < 0:
        raise ValueError("CPU index must be nonnegative")
    if os.name == "nt":
        if cpu >= ctypes.sizeof(ctypes.c_size_t) * 8:
            raise ValueError("CPU index exceeds the current Windows processor group")
        kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel.GetCurrentProcess.restype = ctypes.c_void_p
        kernel.SetProcessAffinityMask.argtypes = [ctypes.c_void_p, ctypes.c_size_t]
        kernel.SetProcessAffinityMask.restype = ctypes.c_int
        if not kernel.SetProcessAffinityMask(kernel.GetCurrentProcess(), 1 << cpu):
            raise ctypes.WinError(ctypes.get_last_error())
    elif hasattr(os, "sched_setaffinity"):
        os.sched_setaffinity(0, {cpu})
    else:
        raise RuntimeError("CPU affinity is unavailable on this platform")


def main():
    started_at = datetime.now(timezone.utc).isoformat()
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--upstream", type=Path, required=True)
    parser.add_argument("--allow-other-revision", action="store_true")
    parser.add_argument("--cxx", default="clang++")
    parser.add_argument("--count", type=int, default=2_000_000)
    parser.add_argument("--scan-count", type=int, default=1_000_000)
    parser.add_argument("--samples", type=int, default=9)
    parser.add_argument("--payload", type=int, default=0)
    parser.add_argument("--baseline-rust", type=Path, help="also run a previously built Rust driver")
    parser.add_argument("--cpu", type=int, help="pin this runner and benchmark children to one logical CPU")
    parser.add_argument("--modes", nargs="+", choices=("slice", "grow", "scan", "scan-u8", "scan-u32",
                                                     "scan-i64", "scan-i8", "scan-i32"),
                        default=("slice", "grow", "scan"))
    parser.add_argument("--output", type=Path, default=ROOT / "target/upstream-results.json")
    args = parser.parse_args()
    if min(args.count, args.scan_count, args.samples) <= 0 or args.payload < 0:
        parser.error("counts and samples must be positive; payload must be nonnegative")
    upstream = args.upstream.resolve()
    revision = run(["git", "rev-parse", "HEAD"], cwd=upstream)
    if revision != UPSTREAM_REVISION and not args.allow_other_revision:
        parser.error(f"expected upstream {UPSTREAM_REVISION}, found {revision}")
    if run(["git", "status", "--porcelain", "--untracked-files=no"], cwd=upstream):
        parser.error("upstream must have no tracked modifications")
    cxx = shutil.which(args.cxx)
    if not cxx:
        parser.error(f"compiler not found: {args.cxx}")
    build = ROOT / "target/upstream-compare"
    build.mkdir(parents=True, exist_ok=True)
    suffix = ".exe" if os.name == "nt" else ""
    binaries = {
        "cpp": build / ("upstream-cpp" + suffix),
        "rust": build / "rust/release/examples" / ("upstream_compare" + suffix),
    }
    if args.baseline_rust:
        baseline = args.baseline_rust.resolve()
        if not baseline.is_file() or baseline == binaries["rust"]:
            parser.error("baseline must be a separate, existing Rust driver executable")
        binaries["rust-baseline"] = baseline
    cpp_command = [cxx, "-std=c++26", "-O3", "-DNDEBUG", "-I", upstream / "include",
                   ROOT / "tools/upstream_compare.cpp", "-o", binaries["cpp"]]
    if os.name == "nt":
        cpp_command.append("-lntdll")
    rust_command = ["cargo", "+stable", "build", "--release", "--example", "upstream_compare",
                    "--target-dir", build / "rust"]
    print("Building C++ and Rust drivers...", flush=True)
    run(cpp_command)
    run(rust_command)
    if args.cpu is not None:
        pin_cpu(args.cpu)
    records, numbers = reference(args.payload)
    expected = b"".join(records)
    with tempfile.TemporaryDirectory(prefix="fast-io-verify-") as scratch:
        for language, binary in binaries.items():
            output = Path(scratch) / f"{language}.bin"
            run([binary, "verify", 1024, args.payload, output])
            if output.read_bytes() != expected:
                raise RuntimeError(f"{language}: output differs from independent Python reference")
    print(f"Verified both memory paths: {len(expected)} bytes, 1024 distinct records.", flush=True)
    cases = [(language, mode) for mode in args.modes for language in binaries]
    samples = {f"{language}/{mode}": [] for language, mode in cases}
    for round_index in range(args.samples):
        # Rotate the first process across both implementations and all workloads.
        for step in range(len(cases)):
            language, mode = cases[(round_index + step) % len(cases)]
            scanning = mode.startswith("scan")
            count = args.scan_count if scanning else args.count
            result = json.loads(run([binaries[language], mode, count, args.payload]))
            bits = {"scan-u8": 8, "scan-i8": 8, "scan-u32": 32, "scan-i32": 32}.get(mode, 64)
            mask = (1 << bits) - 1
            values = [value & mask for value in numbers]
            if mode.startswith("scan-i"):
                values = [value - (1 << bits) if value >= (1 << (bits - 1)) else value
                          for value in values]
            byte_count = repeated_total(
                [len(str(value)) + 1 for value in values] if scanning
                else [len(record) for record in records], count)
            checksum = repeated_total(values, count) & MASK if scanning else 0
            if result["units"] != count or result["bytes"] != byte_count or result["checksum"] != checksum:
                raise RuntimeError(f"{language}/{mode}: unexpected counts/checksum: {result}")
            if result["elapsed_ns"] <= 0:
                raise RuntimeError("timer did not advance; increase the workload")
            samples[f"{language}/{mode}"].append(result)
        print(f"Completed sample {round_index + 1}/{args.samples}", flush=True)
    medians = {}
    for case, runs in samples.items():
        timings = [sample["elapsed_ns"] / sample["units"] for sample in runs]
        medians[case] = statistics.median(timings)
        print(f"{case:14} {medians[case]:9.2f} ns/unit (range {min(timings):.2f}..{max(timings):.2f})")
    report = {
        "started_at_utc": started_at,
        "finished_at_utc": datetime.now(timezone.utc).isoformat(),
        "upstream_revision": revision,
        "rust_revision": run(["git", "rev-parse", "HEAD"]),
        "rust_worktree_status": run(["git", "status", "--porcelain"]),
        "platform": platform.platform(),
        "cpu": platform.processor(),
        "pinned_logical_cpu": args.cpu,
        "cxx_version": run([cxx, "--version"]),
        "rust_version": run(["rustc", "+stable", "-vV"]),
        "commands": [[str(arg) for arg in command] for command in (cpp_command, rust_command)],
        "binaries": {name: {"path": str(path), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}
                     for name, path in binaries.items()},
        "environment": {name: os.environ.get(name) for name in
                        ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "CARGO_PROFILE_RELEASE_LTO",
                         "CARGO_PROFILE_RELEASE_CODEGEN_UNITS", "CARGO_PROFILE_RELEASE_PANIC")},
        "payload": args.payload,
        "verified_bytes": len(expected),
        "verification_sha256": hashlib.sha256(expected).hexdigest(),
        "samples": samples,
        "median_ns_per_unit": medians,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(f"Saved {args.output}", flush=True)


if __name__ == "__main__":
    try:
        main()
    except subprocess.CalledProcessError as error:
        print(error.stdout or "", end="")
        print(error.stderr or "", end="")
        raise SystemExit(f"command failed ({error.returncode}): {error.cmd}") from error
