#!/usr/bin/env python3
"""Build and compare independent Rust/C++ drivers. Python 3.10+, git, cargo, clang++.

Pass an external upstream checkout; this script neither downloads nor edits it.
See docs/UPSTREAM_BENCHMARK.md for the pinned revision and measurement contract.
"""
import argparse
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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--upstream", type=Path, required=True)
    parser.add_argument("--allow-other-revision", action="store_true")
    parser.add_argument("--cxx", default="clang++")
    parser.add_argument("--count", type=int, default=2_000_000)
    parser.add_argument("--scan-count", type=int, default=1_000_000)
    parser.add_argument("--samples", type=int, default=9)
    parser.add_argument("--payload", type=int, default=0)
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
    cpp_command = [cxx, "-std=c++26", "-O3", "-DNDEBUG", "-I", upstream / "include",
                   ROOT / "tools/upstream_compare.cpp", "-o", binaries["cpp"]]
    if os.name == "nt":
        cpp_command.append("-lntdll")
    rust_command = ["cargo", "+stable", "build", "--release", "--example", "upstream_compare",
                    "--target-dir", build / "rust"]
    print("Building C++ and Rust drivers...", flush=True)
    run(cpp_command)
    run(rust_command)
    records, numbers = reference(args.payload)
    expected = b"".join(records)
    with tempfile.TemporaryDirectory(prefix="fast-io-verify-") as scratch:
        for language, binary in binaries.items():
            output = Path(scratch) / f"{language}.bin"
            run([binary, "verify", 1024, args.payload, output])
            if output.read_bytes() != expected:
                raise RuntimeError(f"{language}: output differs from independent Python reference")
    print(f"Verified both memory paths: {len(expected)} bytes, 1024 distinct records.", flush=True)
    cases = [(language, mode) for mode in ("slice", "grow", "scan") for language in binaries]
    samples = {f"{language}/{mode}": [] for language, mode in cases}
    for round_index in range(args.samples):
        # Rotate the first process across both implementations and all workloads.
        for step in range(len(cases)):
            language, mode = cases[(round_index + step) % len(cases)]
            count = args.scan_count if mode == "scan" else args.count
            result = json.loads(run([binaries[language], mode, count, args.payload]))
            byte_count = repeated_total(
                [len(str(value)) + 1 for value in numbers] if mode == "scan"
                else [len(record) for record in records], count)
            checksum = repeated_total(numbers, count) & MASK if mode == "scan" else 0
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
        "upstream_revision": revision,
        "rust_revision": run(["git", "rev-parse", "HEAD"]),
        "rust_worktree_status": run(["git", "status", "--porcelain"]),
        "platform": platform.platform(),
        "cpu": platform.processor(),
        "cxx_version": run([cxx, "--version"]),
        "rust_version": run(["rustc", "+stable", "-vV"]),
        "commands": [[str(arg) for arg in command] for command in (cpp_command, rust_command)],
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
