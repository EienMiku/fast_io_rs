#!/usr/bin/env python3
"""Compare u128 memory formatting with an external, unmodified C++ fast_io.

The current Windows/MSVC STL rejects upstream's u128 octal instantiation;
this driver deliberately compares only bases 2 and 16. No native I/O timing.
"""
import argparse
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

ROOT = Path(__file__).resolve().parents[1]
REVISION = "7a6fa9b680de8355f9f743e0f8950492cf5a42f0"


def run(command, *, cwd=ROOT, timeout=30):
    result = subprocess.run([str(x) for x in command], cwd=cwd, timeout=timeout,
                            text=True, capture_output=True, check=True,
                            creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
    return result.stdout.strip()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--upstream", type=Path, required=True)
    parser.add_argument("--cxx", default="clang++")
    parser.add_argument("--count", type=int, default=200_000)
    parser.add_argument("--samples", type=int, default=15)
    parser.add_argument("--output", type=Path, default=ROOT / "target/radix-comparison.json")
    args = parser.parse_args()
    if min(args.count, args.samples) <= 0:
        parser.error("count and samples must be positive")
    upstream = args.upstream.resolve()
    if run(["git", "rev-parse", "HEAD"], cwd=upstream) != REVISION:
        parser.error(f"upstream must be pinned to {REVISION}")
    if run(["git", "status", "--porcelain", "--untracked-files=no"], cwd=upstream):
        parser.error("upstream has tracked modifications")
    cxx = shutil.which(args.cxx)
    if not cxx:
        parser.error(f"compiler not found: {args.cxx}")
    build = ROOT / "target/radix-compare"
    build.mkdir(parents=True, exist_ok=True)
    suffix = ".exe" if os.name == "nt" else ""
    cpp = build / ("radix-cpp" + suffix)
    rust = build / "rust/release/examples" / ("radix_throughput" + suffix)
    cpp_command = [cxx, "-std=c++26", "-O3", "-DNDEBUG", "-D_CRT_SECURE_NO_WARNINGS",
                   "-I", upstream / "include", ROOT / "tools/radix_compare.cpp", "-o", cpp]
    if os.name == "nt":
        cpp_command.append("-lntdll")
    rust_command = ["cargo", "+stable", "build", "--release", "--example", "radix_throughput",
                    "--target-dir", build / "rust"]
    report = {
        "started_at_utc": datetime.now(timezone.utc).isoformat(),
        "upstream_revision": REVISION, "rust_revision": run(["git", "rev-parse", "HEAD"]),
        "rust_worktree_status": run(["git", "status", "--porcelain"]),
        "platform": platform.platform(), "cpu": platform.processor(), "bases": [2, 16],
        "count": args.count, "rounds": args.samples,
        "commands": [[str(x) for x in c] for c in [cpp_command, rust_command]],
        "cxx_version": run([cxx, "--version"]), "rust_version": run(["rustc", "+stable", "-vV"]),
        "environment": {key: os.environ.get(key) for key in ["RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS",
            "CARGO_PROFILE_RELEASE_LTO", "CARGO_PROFILE_RELEASE_CODEGEN_UNITS", "CARGO_PROFILE_RELEASE_PANIC"]},
        "contract": "130-byte fixed buffer; no prefixes; 1024 prebuilt u128 values; 10000 warmup values; input creation, verification and startup outside timer; no CPU pinning; C++ static bases and Rust runtime radix API",
    }
    try:
        run(cpp_command, timeout=60)
        run(rust_command, timeout=60)
        report["binaries"] = {key: {"path": str(path), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}
                              for key, path in [("cpp", cpp), ("rust", rust)]}
        fixed = [0, 1, 255, 65535, 2**32 - 1, 2**64 - 1, 2**128 - 1]
        values = [fixed[i] if i < len(fixed) else i * 0xda942042e4dd58b59e3779b97f4a7c15 % 2**128
                  for i in range(1024)]
        expected_bytes = {}
        report["verification"] = {}
        with tempfile.TemporaryDirectory(prefix="fast-io-radix-") as temporary:
            for base, spec in [(2, "b"), (16, "x")]:
                lines = [format(value, spec).encode() for value in values]
                expected = b"\n".join(lines) + b"\n"
                whole, remainder = divmod(args.count, len(values))
                expected_bytes[base] = whole * sum(map(len, lines)) + sum(map(len, lines[:remainder]))
                for language, binary in [("cpp", cpp), ("rust", rust)]:
                    path = Path(temporary) / f"{language}-{base}.bin"
                    run([binary, "verify", base, path], timeout=10)
                    if path.read_bytes() != expected:
                        raise RuntimeError(f"{language} base {base} differs from all 1024 Python encodings")
                report["verification"][base] = {"bytes": len(expected), "sha256": hashlib.sha256(expected).hexdigest()}
        cases = [(language, mode, base) for base in [2, 16] for language, modes in
                 [("cpp", ["print"]), ("rust", ["std", "write", "print"])] for mode in modes]
        samples = {f"{language}/{mode}/{base}": [] for language, mode, base in cases}
        for round_index in range(args.samples):
            for step in range(len(cases)):
                language, mode, base = cases[(round_index + step) % len(cases)]
                result = json.loads(run([cpp if language == "cpp" else rust, mode, base, args.count]))
                if result["units"] != args.count or result["bytes"] != expected_bytes[base] or result["elapsed_ns"] <= 0:
                    raise RuntimeError(f"invalid measured counts: {result}")
                samples[f"{language}/{mode}/{base}"].append(result)
            print(f"Sample {round_index + 1}/{args.samples}", flush=True)
        report["samples"] = samples
        report["median_ns_per_value"] = {key: statistics.median(s["elapsed_ns"] / s["units"] for s in runs)
                                          for key, runs in samples.items()}
        for key, value in report["median_ns_per_value"].items():
            print(f"{key:18} {value:.2f} ns/value")
        report["status"] = "complete"
    except (subprocess.SubprocessError, RuntimeError, OSError) as error:
        report["status"] = "failed"
        report["error"] = str(error)
        report["stdout"] = str(getattr(error, "stdout", "") or "")
        report["stderr"] = str(getattr(error, "stderr", "") or "")
        raise
    finally:
        report["finished_at_utc"] = datetime.now(timezone.utc).isoformat()
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
