#!/usr/bin/env python3
"""Print the nextest filterset that push CI runs: everything except the tests listed in tests/known-failures.txt
and tests/slow-tests.txt (known failures are tracked by the nightly run, slow goldens run there too).

Usage: cargo nextest run -E "$(scripts/nextest-filter.py)" ...

A key is "<package> <test binary path> <test name>" (see docs/testing.md section 8). `mc-game tests/golden/main.rs a::b`
becomes `package(mc-game) & binary(golden) & test(=a::b)`; `unittests` is the lib and bin targets of the package.
A name `<crashed>` excludes the whole binary and `<exit status>` the whole package. Doc-tests are not run by nextest.
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LISTS = ["tests/known-failures.txt", "tests/slow-tests.txt"]


def term(key):
    package, binary, name = key.split(" ", 2)
    if binary == "doctests":
        return None
    if name == "<exit status>":
        return f"package({package})"
    if binary == "unittests":
        target = "(kind(lib) | kind(bin))"
    elif m := re.fullmatch(r"tests/(?:([^/]+)/main|([^/]+))\.rs", binary):
        target = f"binary({m[1] or m[2]})"
    else:
        raise SystemExit(f"nextest-filter: unknown test binary in key: {key}")
    if name == "<crashed>":
        return f"(package({package}) & {target})"
    return f"(package({package}) & {target} & test(={name}))"


def main():
    keys = [l.strip() for f in LISTS for l in (ROOT / f).read_text().splitlines() if l.strip() and not l.startswith("#")]
    terms = [t for t in map(term, keys) if t]
    print(f"not ({' | '.join(terms)})" if terms else "all()")


if __name__ == "__main__":
    if sys.argv[1:2] in (["-h"], ["--help"]):
        print(__doc__)
    else:
        main()
