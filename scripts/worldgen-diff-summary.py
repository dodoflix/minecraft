#!/usr/bin/env python3
"""Markdown summary of scripts/worldgen-diff.sh runs: one row per case of a cases file (default scripts/worldgen-diff-cases-report.txt).

  scripts/worldgen-diff-summary.py [CASES_FILE] [--runs run/wgdiff]

Columns: chunks compared / differing, then the number of differences per category (blocks, block_entities, biomes,
heightmaps, structures = structure_starts + structure_references, entities, ticks, post_processing, other)."""
import json, sys
from pathlib import Path

root = Path(__file__).resolve().parent.parent
args = [a for a in sys.argv[1:] if not a.startswith("--")]
runs = root / "run/wgdiff"
if "--runs" in sys.argv:
    runs = Path(sys.argv[sys.argv.index("--runs") + 1])
    args = [a for a in args if a != str(runs)]
cases = Path(args[0]) if args else root / "scripts/worldgen-diff-cases-report.txt"
COLS = ["blocks", "block_entities", "biomes", "heightmaps", "structures", "entities", "ticks", "post_processing", "other"]
MERGE = {"structure_starts": "structures", "structure_references": "structures"}

print("| seed | dim | center (blocks) | label | radius | chunks compared | differing | " + " | ".join(COLS) + " |")
print("|" + "---|" * (7 + len(COLS)))
for line in cases.read_text().splitlines():
    if not line.strip() or line.startswith("#"):
        continue
    seed, dim, x, z, radius, *label = line.split()
    d = runs / f"{seed}-{dim}-{int(x) // 16}_{int(z) // 16}" / "report.json"
    row = f"| {seed} | {dim} | {x},{z} | {' '.join(label)} | {radius} | "
    if not d.exists():
        print(row + "not run |" + " |" * (len(COLS) + 1))
        continue
    dims = json.loads(d.read_text())["dimensions"]
    compared = sum(r["compared"] for r in dims)
    differing = sum(r["differing"] for r in dims)
    counts = dict.fromkeys(COLS, 0)
    for r in dims:
        for cat, v in r["category_totals"].items():
            counts[MERGE.get(cat, cat) if MERGE.get(cat, cat) in counts else "other"] += v["differences"]
    print(row + f"{compared} | {differing} | " + " | ".join(str(counts[c]) for c in COLS) + " |")
