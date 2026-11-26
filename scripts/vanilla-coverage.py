#!/usr/bin/env python3
"""Merge and report the execution-coverage files written by tools/vanilla-agent (scripts/vanilla-agent.sh).

Usage:
  scripts/vanilla-coverage.py diff A.cov.gz B.cov.gz        methods whose entered/not-entered state differs between two runs
  scripts/vanilla-coverage.py merge OUT.tsv.gz IN...        union of runs; IN may be run files or earlier merges (OUT may be one of IN)
  scripts/vanilla-coverage.py report MERGED UNIVERSE CLASS_MAP OUT_DIR
                                                            vanilla-covered.tsv, unreached.tsv, areas.tsv in OUT_DIR

UNIVERSE is `scripts/vanilla-agent.sh universe` output, CLASS_MAP the class map table (tab separated, `class` column with
dotted names and `status`, `rust_files` columns).

Output: `diff` prints the differing methods and exits 1 when there are any; `merge` writes rows
`class name desc access n hexmask runs` (marker 0 = method entry; `runs` = number of runs that entered the method);
`report` writes the three tables. The report's reason codes are a first pass: `client-only` (package, or the class map's reason says client), `debug-flag` (the method is
only called behind a SharedConstants.DEBUG_* switch, or lives in a debug package), `unexplained` (everything else).
Area rules are by package prefix, first match wins (see AREAS).
"""
import csv
import gzip
import os
import sys
from collections import Counter, defaultdict

# First match wins; the area of a class is a package-prefix guess for reading the report, not a claim about ownership.
AREAS = [
    ("tools", ("net/minecraft/gametest/", "net/minecraft/server/gametest/", "net/minecraft/server/rcon/", "net/minecraft/util/profiling/",
            "net/minecraft/util/debugchart/", "net/minecraft/server/network/Filtered", "net/minecraft/server/network/TextFilter")),
    ("data-fixers", ("net/minecraft/util/datafix/", "net/minecraft/nbt/", "net/minecraft/world/level/chunk/storage/")),
    ("commands-loot", ("net/minecraft/commands/", "net/minecraft/advancements/", "net/minecraft/world/level/storage/loot/", "net/minecraft/stats/",
            "net/minecraft/server/commands/", "net/minecraft/world/scores/", "net/minecraft/network/chat/", "net/minecraft/server/bossevents/")),
    ("menus", ("net/minecraft/world/inventory/", "net/minecraft/world/item/crafting/")),
    ("entities", ("net/minecraft/world/entity/projectile/", "net/minecraft/world/entity/vehicle/", "net/minecraft/world/entity/decoration/",
            "net/minecraft/world/entity/item/", "net/minecraft/world/entity/Display", "net/minecraft/world/entity/Marker")),
    ("items", ("net/minecraft/world/item/", "net/minecraft/core/dispenser/", "net/minecraft/world/food/")),
    ("blocks", ("net/minecraft/world/level/block/", "net/minecraft/world/level/material/", "net/minecraft/world/level/redstone/")),
    ("world-server", ("net/minecraft/world/entity/player/", "net/minecraft/server/", "net/minecraft/world/level/", "net/minecraft/network/")),
    ("mobs", ("net/minecraft/world/entity/",)),
]


def area_of(cls):
    for area, prefixes in AREAS:
        if cls.startswith(prefixes):
            return area
    return "shared"


def open_text(path):
    return gzip.open(path, "rt") if path.endswith(".gz") else open(path)


def mask_int(mask):
    return int(mask[::-1], 16) if mask else 0


def int_mask(value, n):
    return "".join(format((value >> (4 * i)) & 15, "x") for i in range((n + 3) // 4))


def entered(mask):
    return int(mask[0], 16) & 1


def load_run(path):
    out = {}
    with open_text(path) as f:
        for line in f:
            if line[0] == "#":
                continue
            x = line.rstrip("\n").split("\t")
            out[(x[0], x[1], x[2])] = (int(x[3]), int(x[4]), x[5])
    return out


def diff(a_path, b_path):
    a, b = load_run(a_path), load_run(b_path)
    bad = 0
    for k in sorted(set(a) | set(b)):
        ea = k in a and entered(a[k][2])
        eb = k in b and entered(b[k][2])
        if ea != eb:
            bad += 1
            print("\t".join(k), "A" if ea else "-", "B" if eb else "-")
    print(f"{len(a)} / {len(b)} methods, {bad} differ in entered state", file=sys.stderr)
    return 1 if bad else 0


def merge(out_path, ins):
    acc = {}
    for path in ins:
        with open_text(path) as f:
            is_merge = f.readline().startswith("#merged")
            f.seek(0)
            for line in f:
                if line[0] == "#":
                    continue
                x = line.rstrip("\n").split("\t")
                k = (x[0], x[1], x[2])
                access, n, mask = int(x[3]), int(x[4]), mask_int(x[5])
                runs = int(x[6]) if is_merge else (1 if entered(x[5]) else 0)
                cur = acc.get(k)
                if cur is None:
                    acc[k] = [access, n, mask, runs]
                elif cur[1] == n:
                    cur[2] |= mask
                    cur[3] += runs
                else:  # a shadowed class with another body: only the entry marker is comparable
                    cur[2] |= mask & 1
                    cur[3] += runs
    tmp = out_path + ".tmp"
    with gzip.open(tmp, "wt", compresslevel=6) as f:
        f.write("#merged 1\n")
        for k in sorted(acc):
            access, n, mask, runs = acc[k]
            f.write("\t".join(k) + f"\t{access}\t{n}\t{int_mask(mask, n)}\t{runs}\n")
    os.replace(tmp, out_path)


def report(merged_path, universe_path, class_map_path, out_dir):
    merged = {}
    with open_text(merged_path) as f:
        next(f)
        for line in f:
            x = line.rstrip("\n").split("\t")
            merged[(x[0], x[1], x[2])] = (int(x[4]), mask_int(x[5]), int(x[6]))
    class_map = {}
    with open(class_map_path) as f:
        for r in csv.DictReader(f, delimiter="\t"):
            class_map[r["class"]] = (r["status"], r["rust_files"], r["reason"])
    covered, unreached = [], []
    area_stats = defaultdict(Counter)
    with open_text(universe_path) as f:
        for line in f:
            cls, name, desc, access, n, callers, debug_only = line.rstrip("\n").split("\t")
            n = int(n)
            key = (cls, name, desc)
            area = area_of(cls)
            dotted = cls.replace("/", ".")
            status, rust, why = class_map.get(dotted, class_map.get(dotted.split("$")[0], ("", "", "")))
            got = merged.get(key)
            if got is not None and got[1] & 1:
                # a shadowed class has another body: its mask does not number this method's blocks
                hit = bin(got[1]).count("1") if got[0] == n else 1
                covered.append((cls, name, desc, hit, got[0], got[2]))
                area_stats[area]["entered"] += 1
                area_stats[area]["blocks_hit"] += hit
            else:
                segments = cls.split("/")
                if "client" in segments or "client-side" in why or "client only" in why:
                    reason = "client-only"
                elif debug_only == "1" or "debug" in segments or "debugchart" in segments:
                    reason = "debug-flag"
                else:
                    reason = "unexplained"
                unreached.append((cls, name, desc, n, reason, area, status, rust))
                area_stats[area]["unreached"] += 1
                area_stats[area]["reason:" + reason] += 1
            area_stats[area]["methods"] += 1
            area_stats[area]["blocks"] += n
    with open(f"{out_dir}/vanilla-covered.tsv", "w") as f:
        f.write("class\tmethod\tdesc\tblocks_hit\tblocks\truns\n")
        for r in covered:
            f.write("\t".join(map(str, r)) + "\n")
    with open(f"{out_dir}/unreached.tsv", "w") as f:
        f.write("class\tmethod\tdesc\tblocks\treason\tarea\tmap_status\trust_files\n")
        for r in unreached:
            f.write("\t".join(map(str, r)) + "\n")
    reasons = ["client-only", "debug-flag", "unexplained"]
    with open(f"{out_dir}/areas.tsv", "w") as f:
        f.write("area\tmethods\tentered\tentered_pct\tblocks\tblocks_hit\tblocks_pct\t" + "\t".join(reasons) + "\n")
        total = Counter()
        for area in sorted(area_stats):
            s = area_stats[area]
            total.update(s)
            f.write(summary_row(area, s, reasons))
        f.write(summary_row("total", total, reasons))
    print(f"{len(covered)} entered, {len(unreached)} unreached of {len(covered) + len(unreached)} methods", file=sys.stderr)


def summary_row(area, s, reasons):
    pct = lambda a, b: f"{100 * a / b:.1f}" if b else "0.0"
    return (f"{area}\t{s['methods']}\t{s['entered']}\t{pct(s['entered'], s['methods'])}\t{s['blocks']}\t{s['blocks_hit']}\t"
            f"{pct(s['blocks_hit'], s['blocks'])}\t" + "\t".join(str(s["reason:" + r]) for r in reasons) + "\n")


def main(argv):
    if len(argv) >= 4 and argv[1] == "diff":
        return diff(argv[2], argv[3])
    if len(argv) >= 4 and argv[1] == "merge":
        merge(argv[2], argv[3:])
        return 0
    if len(argv) == 6 and argv[1] == "report":
        report(*argv[2:])
        return 0
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
