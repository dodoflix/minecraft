#!/usr/bin/env python3
"""Run the workspace tests and fail only on failures that tests/known-failures.txt does not list.

Usage: scripts/test-known-failures.py [--oracle] [-p PACKAGE]... [-- LIBTEST_ARGS...]

Default: `cargo test --no-fail-fast` for every workspace package (T0/T1).
--oracle: the oracle tier instead: `--test oracle -- --ignored` for every package that has a tests/oracle/main.rs
          (set MC_ACCEPT_EULA=1; it starts the vanilla server).
-p:       only these packages.

Each package is run on its own so that every result has the key "<package> <test binary> <test name>", for example
`mc-game tests/golden/main.rs attack_golden::game_modes_and_reach` (binary = the path cargo prints after `Running`,
`doctests` for doc-tests). A binary that aborts (stack overflow, OOM kill) prints no result line: it counts as the
failure "<package> <binary> <crashed>", and a cargo error without a failed test (compile error) as
"<package> <cargo> <exit status>". Exit status: 0 when every failure is listed, 1 on a new failure.
Listed tests that now pass are reported so their line can be deleted.
"""
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
KNOWN = ROOT / "tests" / "known-failures.txt"


def packages():
    meta = json.loads(subprocess.check_output(["cargo", "metadata", "--no-deps", "--format-version", "1"], cwd=ROOT))
    return {p["name"]: Path(p["manifest_path"]).parent for p in meta["packages"]}


def run_package(package, cargo_args, libtest_args):
    """Returns {key: "ok" | "FAILED" | "ignored"} for one `cargo test` run."""
    cmd = ["cargo", "test", "--locked", "--no-fail-fast", "-p", package, *cargo_args, "--", *libtest_args]
    print("+", " ".join(cmd), flush=True)
    proc = subprocess.run(cmd, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    results, binary, unfinished = {}, "?", set()
    for line in proc.stdout.splitlines():
        if m := re.match(r"\s*Running (\S+)", line):
            binary = m[1]
            unfinished.add(binary)
        elif re.match(r"\s*Doc-tests ", line):
            binary = "doctests"
            unfinished.add(binary)
        elif line.startswith("test result:"):
            unfinished.discard(binary)
        elif m := re.match(r"test (.+?) \.\.\. (ok|FAILED|ignored)", line):
            results[f"{package} {binary} {m[1]}"] = m[2]
    for binary in unfinished:
        results[f"{package} {binary} <crashed>"] = "FAILED"
    if proc.returncode and "FAILED" not in results.values():
        results[f"{package} <cargo> <exit status>"] = "FAILED"
    if proc.returncode:
        print(proc.stdout[-4000:], flush=True)  # the failure details; the full log is megabytes
    return results


def main():
    argv = sys.argv[1:]
    libtest_args = argv[argv.index("--") + 1 :] if "--" in argv else []
    argv = argv[: argv.index("--")] if "--" in argv else argv
    oracle = "--oracle" in argv
    only = {argv[i + 1] for i, a in enumerate(argv) if a == "-p"}
    if oracle:
        libtest_args = ["--ignored", *libtest_args]

    results = {}
    for name, directory in sorted(packages().items()):
        if only and name not in only:
            continue
        if oracle and not (directory / "tests" / "oracle" / "main.rs").exists():
            continue
        results |= run_package(name, ["--test", "oracle"] if oracle else [], libtest_args)

    known = set()
    if KNOWN.exists():
        known = {l.strip() for l in KNOWN.read_text().splitlines() if l.strip() and not l.startswith("#")}
    failed = {k for k, v in results.items() if v == "FAILED"}
    new = sorted(failed - known)
    for k in sorted(k for k in known if results.get(k) == "ok"):
        print(f"now passing, delete from tests/known-failures.txt: {k}")
    for k in new:
        print(f"NEW FAILURE: {k}")
    print(f"{len(results)} tests, {len(failed)} failed, {len(failed) - len(new)} known, {len(new)} new")
    return 1 if new else 0


if __name__ == "__main__":
    sys.exit(main())
