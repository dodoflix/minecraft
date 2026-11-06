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
Listed tests that now pass are reported so their line can be deleted. Each new failure is re-run once on its own: a
pass there is reported as FLAKY (a `::warning::` annotation on GitHub) and does not fail the run; fix flaky tests.
"""
import json
import os
import re
import subprocess
import sys
from pathlib import Path

if sys.argv[1:2] in (["-h"], ["--help"]):
    print(__doc__)
    sys.exit(0)

ROOT = Path(__file__).resolve().parent.parent
KNOWN = ROOT / "tests" / "known-failures.txt"
# colour codes would split "Running" from the rest of the line (CI sets CARGO_TERM_COLOR=always)
CARGO_ENV = {**os.environ, "CARGO_TERM_COLOR": "never"}


def packages():
    meta = json.loads(subprocess.check_output(["cargo", "metadata", "--no-deps", "--format-version", "1"], cwd=ROOT))
    return {p["name"]: Path(p["manifest_path"]).parent for p in meta["packages"]}


def run_package(package, cargo_args, libtest_args):
    """Returns ({key: "ok" | "FAILED" | "ignored"}, combined output) for one `cargo test` run."""
    cmd = ["cargo", "test", "--locked", "--no-fail-fast", "-p", package, *cargo_args, "--", *libtest_args]
    print("+", " ".join(cmd), flush=True)
    proc = subprocess.run(cmd, cwd=ROOT, env=CARGO_ENV, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
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
    return results, proc.stdout


def details(key, output):
    """The libtest failure block of a failed test ("---- name stdout ----" up to the next block), else the output tail."""
    name = key.split(" ", 2)[2]
    m = re.search(rf"^---- {re.escape(name)} stdout ----\n(.*?)(?=^---- |^failures:|\Z)", output, re.M | re.S)
    return m[1].strip()[:6000] if m else output[-4000:]


def retry_passes(key):
    """Re-runs one failed test alone; True if it passes. Crashes, cargo errors and doc-tests are not retried."""
    package, binary, name = key.split(" ", 2)
    if name.startswith("<") or not binary.startswith(("tests/", "unittests")):
        return False
    target = ["--lib"] if binary == "unittests" else ["--test", binary.split("/")[1].removesuffix(".rs")]
    cmd = ["cargo", "test", "--locked", "-p", package, *target, "--", "--exact", name, "--include-ignored"]
    proc = subprocess.run(cmd, cwd=ROOT, env=CARGO_ENV, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    return f"test {name} ... ok" in proc.stdout


def main():
    argv = sys.argv[1:]
    libtest_args = argv[argv.index("--") + 1 :] if "--" in argv else []
    argv = argv[: argv.index("--")] if "--" in argv else argv
    oracle = "--oracle" in argv
    only = {argv[i + 1] for i, a in enumerate(argv) if a == "-p"}
    if oracle:
        libtest_args = ["--ignored", *libtest_args]

    results, outputs = {}, {}
    for name, directory in sorted(packages().items()):
        if only and name not in only:
            continue
        if oracle and not (directory / "tests" / "oracle" / "main.rs").exists():
            continue
        results[name], outputs[name] = run_package(name, ["--test", "oracle"] if oracle else [], libtest_args)

    known = set()
    if KNOWN.exists():
        known = {l.strip() for l in KNOWN.read_text().splitlines() if l.strip() and not l.startswith("#")}
    results = {k: v for r in results.values() for k, v in r.items()}
    failed = {k for k, v in results.items() if v == "FAILED"}
    new = sorted(failed - known)
    for k in sorted(k for k in known if results.get(k) == "ok"):
        print(f"now passing, delete from tests/known-failures.txt: {k}")
    flaky = [k for k in new if retry_passes(k)]
    for k in flaky:
        print(f"::warning title=flaky test::FLAKY (failed, then passed alone): {k}")
    new = [k for k in new if k not in flaky]
    for k in new:
        print(f"\nNEW FAILURE: {k}\n{details(k, outputs[k.split(' ', 1)[0]])}")
    print(f"{len(results)} tests, {len(failed)} failed: {len(failed) - len(new) - len(flaky)} known, {len(flaky)} flaky, {len(new)} new")
    return 1 if new else 0


if __name__ == "__main__":
    sys.exit(main())
