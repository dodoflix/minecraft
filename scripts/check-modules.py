#!/usr/bin/env python3
"""Fail on .rs files that no crate root reaches through `mod` declarations (rustc and clippy never see them).

Usage: scripts/check-modules.py
Roots: every package's lib/bin/test/example/bench targets (declared or auto-discovered) and build.rs.
Follows `mod x;` (with `#[path]`); files under a `fixtures` directory are data, not modules.
"""
import re
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MOD = re.compile(r'^[ \t]*(?:#\[path\s*=\s*"([^"]+)"\]\s*)?(?:pub(?:\([^)]*\))?\s+)?mod\s+(\w+)\s*;', re.M)
COMMENT = re.compile(r"//[^\n]*|/\*.*?\*/", re.S)


def tracked_rs():
    out = subprocess.run(["git", "ls-files", "-z", "*.rs"], cwd=ROOT, capture_output=True, check=True).stdout
    return {(ROOT / p).resolve() for p in out.decode().split("\0") if p and "/fixtures/" not in p}


def roots(pkg: Path):
    manifest = tomllib.loads((pkg / "Cargo.toml").read_text())
    found = set()
    for key in ("lib", "bin", "test", "example", "bench"):
        entry = manifest.get(key)
        for t in [entry] if isinstance(entry, dict) else entry or []:
            if "path" in t:
                found.add((pkg / t["path"]).resolve())
    found |= {pkg / "src/lib.rs", pkg / "src/main.rs", pkg / "build.rs"}
    for d in ("src/bin", "tests", "examples", "benches"):
        base = pkg / d
        if base.is_dir():
            found |= set(base.glob("*.rs")) | set(base.glob("*/main.rs"))
    return {p.resolve() for p in found if p.is_file()}


def children(file: Path, is_root: bool):
    """Files declared by `mod x;` in `file`."""
    text = COMMENT.sub("", file.read_text(errors="replace"))
    base = file.parent if is_root or file.name == "mod.rs" else file.parent / file.stem
    for path_attr, name in MOD.findall(text):
        cands = [file.parent / path_attr] if path_attr else [base / f"{name}.rs", base / name / "mod.rs"]
        yield from (c.resolve() for c in cands if c.is_file())


def main():
    reached, todo = set(), []
    for manifest in subprocess.run(["git", "ls-files", "-z", "**Cargo.toml"], cwd=ROOT, capture_output=True,
                                   check=True).stdout.decode().split("\0"):
        if manifest:
            todo += [(r, True) for r in roots((ROOT / manifest).parent)]
    while todo:
        f, is_root = todo.pop()
        if f in reached:
            continue
        reached.add(f)
        todo += [(c, False) for c in children(f, is_root)]
    orphans = sorted(tracked_rs() - reached)
    for o in orphans:
        print(f"orphan (not reachable from any crate root): {o.relative_to(ROOT)}")
    return 1 if orphans else 0


if __name__ == "__main__":
    sys.exit(main())
