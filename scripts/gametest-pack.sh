#!/usr/bin/env bash
# Regenerates the structure files of the game test data pack used by the /test corpora.
#
#   scripts/gametest-pack.sh
#
# Output: crates/mc-game/tests/fixtures/commands_corpus/gametest/gtpack/data/gt/structure/*.nbt (gzip NBT, byte-stable:
#         no timestamp, fixed compression level). The JSON files and functions of the pack are written by hand; test
#         structures hold test blocks (start, accept, fail, log) and one structure is written at an old data version so
#         that loading it runs the structure data fixers.
# Needs: python3.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need python3

python3 - "$ROOT/crates/mc-game/tests/fixtures/commands_corpus/gametest/gtpack/data/gt/structure" <<'PY'
import gzip
import struct
import sys

out_dir = sys.argv[1]


# ---- a typed NBT writer: (tag id, value); a compound is a dict name -> typed value, a list is (element id, [values]) ----
def b(v): return (1, v)
def i(v): return (3, v)
def s(v): return (8, v)
def lst(t, vs): return (9, (t, vs))
def comp(**kw): return (10, kw)
def ints(*vs): return lst(3, list(vs))


def payload(t, v):
    if t == 1:
        return struct.pack(">b", v)
    if t == 2:
        return struct.pack(">h", v)
    if t == 3:
        return struct.pack(">i", v)
    if t == 8:
        raw = v.encode("utf-8")
        return struct.pack(">H", len(raw)) + raw
    if t == 9:
        et, items = v
        return struct.pack(">bi", et if items else 0, len(items)) + b"".join(payload(et, x) for x in items)
    if t == 10:
        return b"".join(bytes([ct]) + payload(8, k) + payload(ct, cv) for k, (ct, cv) in v.items()) + b"\0"
    raise ValueError(t)


def write_gz(path, root):
    data = bytes([10]) + payload(8, "") + payload(10, root)
    with open(path, "wb") as f:
        with gzip.GzipFile(filename="", mode="wb", fileobj=f, compresslevel=9, mtime=0) as g:
            g.write(data)


# ---- structures: blocks are (x, y, z, name, properties, block entity data) ----
def test_block(x, y, z, mode, message=""):
    return (x, y, z, "minecraft:test_block", {"mode": mode},
            {"id": s("minecraft:test_block"), "mode": s(mode), "message": s(message), "powered": b(0)})


def structure(name, size, blocks, version=5023):
    palette, entries = [], []
    for x, y, z, block, props, nbt in blocks:
        key = (block, tuple(sorted(props.items())))
        if key not in palette:
            palette.append(key)
        e = {"pos": ints(x, y, z), "state": i(palette.index(key))}
        if nbt:
            e["nbt"] = comp(**nbt)
        entries.append(e)
    # the block state field names were renamed (Name to id, Properties to properties) at data version 5006 (a newer structure with the old names loads empty)
    kn, kp = ("id", "properties") if version >= 5006 else ("Name", "Properties")
    pal = [{kn: s(n), **({kp: comp(**{k: s(v) for k, v in p})} if p else {})} for n, p in palette]
    root = {"DataVersion": i(version), "size": ints(*size), "palette": lst(10, pal),
            "blocks": lst(10, entries), "entities": lst(10, [])}
    write_gz(f"{out_dir}/{name}.nbt", root)


structure("pass", (3, 1, 1), [test_block(0, 0, 0, "start"), test_block(1, 0, 0, "accept")])
structure("fail", (4, 1, 1), [test_block(0, 0, 0, "start"), test_block(1, 0, 0, "fail", "boom"), test_block(3, 0, 0, "accept")])
structure("log", (3, 1, 1), [test_block(0, 0, 0, "log", "hello log"), test_block(1, 0, 0, "start"), test_block(2, 0, 0, "accept")])
structure("timeout", (4, 1, 1), [test_block(0, 0, 0, "start"), test_block(3, 0, 0, "accept")])
structure("rot", (3, 2, 4), [test_block(0, 0, 0, "start"), test_block(0, 0, 1, "accept"),
                              (2, 1, 3, "minecraft:stone", {}, None)])
structure("no_start", (2, 1, 1), [test_block(0, 0, 0, "accept"), test_block(1, 0, 0, "log", "no start")])
structure("two_start", (3, 1, 1), [test_block(0, 0, 0, "start"), test_block(1, 0, 0, "start"), test_block(2, 0, 0, "accept")])
structure("no_accept", (2, 1, 1), [test_block(0, 0, 0, "start"), test_block(1, 0, 0, "log", "no accept")])
# the chain was renamed to iron_chain and the furnace fields were renamed after this data version
structure("old_structure", (4, 1, 1), [
    test_block(0, 0, 0, "start"), test_block(1, 0, 0, "accept"),
    (2, 0, 0, "minecraft:chain", {"axis": "y", "waterlogged": "false"}, None),
    (3, 0, 0, "minecraft:furnace", {"facing": "north", "lit": "false"},
     {"id": s("minecraft:furnace"), "BurnTime": (2, 100), "CookTime": (2, 5), "CookTimeTotal": (2, 200),
      "Items": lst(10, [])}),
], version=4000)
PY
