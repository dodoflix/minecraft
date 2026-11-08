#!/usr/bin/env python3
"""Generates blk_furnace_cook.txt (every cooking recipe in its furnace type, plus furnace edge cases) and
blk_furnace_fuels.txt (every distinct cooking fuel in each furnace type): block-oracle scenarios replayed by
tests/golden/blocks_furnaces_golden.rs.
Fixtures: scripts/block-oracle.sh blk_furnace_cook blk_furnace_fuels

Usage: python3 crates/mc-game/tests/oracle/scenarios/gen_furnace_scenarios.py
Output: crates/mc-game/tests/oracle/scenarios/blk_furnace_cook.txt and blk_furnace_fuels.txt (needs ref/)."""
import collections
import glob
import json
import os
import sys

if sys.argv[1:] in (["-h"], ["--help"]):
    print(__doc__)
    sys.exit(0)
if len(sys.argv) != 1:
    sys.exit(__doc__)

HERE = os.path.dirname(os.path.abspath(__file__))
REF = os.path.join(HERE, "../../../../../ref")
RECIPES = os.path.join(REF, "generated/data/minecraft/recipe")
TAGS = os.path.join(REF, "generated/data/minecraft/tags/item")

# recipe type -> (furnace block, default cooking time)
TYPES = {
    "minecraft:smelting": ("furnace", 200),
    "minecraft:blasting": ("blast_furnace", 100),
    "minecraft:smoking": ("smoker", 100),
}
COOKING_TIMES = {"furnace": 200, "blast_furnace": 100, "smoker": 100}
FUEL_INPUTS = {"furnace": "minecraft:raw_iron", "blast_furnace": "minecraft:raw_iron", "smoker": "minecraft:beef"}
ROW = 40  # furnaces per row; rows are 2 blocks apart (z)

EDGE_CASES = [
    ("lit state without burn time", "furnace[facing=north,lit=true]", "{}"),
    (
        "burn the remaining lit time, then cool down",
        "furnace[facing=north,lit=false]",
        '{Items:[{Slot:0b,id:"minecraft:raw_iron",count:3}],lit_time_remaining:5,lit_total_time:5,cooking_time_spent:2,cooking_total_time:200}',
    ),
    (
        "cooking progress decays without fuel",
        "furnace[facing=north,lit=false]",
        '{Items:[{Slot:0b,id:"minecraft:raw_iron",count:3}],cooking_time_spent:100,cooking_total_time:200}',
    ),
    (
        "progress above the total time decays to the total",
        "furnace[facing=north,lit=false]",
        '{Items:[{Slot:0b,id:"minecraft:raw_iron",count:3}],cooking_time_spent:300,cooking_total_time:200}',
    ),
    (
        "wet sponge turns the bucket left by lava into water",
        "furnace[facing=north,lit=false]",
        '{Items:[{Slot:0b,id:"minecraft:wet_sponge",count:2},{Slot:1b,id:"minecraft:lava_bucket",count:1}],cooking_total_time:200}',
    ),
    (
        "lava bucket stack drops the bucket",
        "furnace[facing=north,lit=false]",
        '{Items:[{Slot:0b,id:"minecraft:raw_iron",count:3},{Slot:1b,id:"minecraft:lava_bucket",count:2}],cooking_total_time:200}',
    ),
    (
        "full result stack blocks cooking",
        "furnace[facing=north,lit=false]",
        '{Items:[{Slot:0b,id:"minecraft:raw_iron",count:3},{Slot:1b,id:"minecraft:coal",count:2},{Slot:2b,id:"minecraft:iron_ingot",count:64}],cooking_total_time:200}',
    ),
    (
        "result stack one below the maximum takes one more",
        "furnace[facing=north,lit=false]",
        '{Items:[{Slot:0b,id:"minecraft:raw_iron",count:3},{Slot:1b,id:"minecraft:coal",count:2},{Slot:2b,id:"minecraft:iron_ingot",count:63}],cooking_total_time:1}',
    ),
    (
        "other item in the result slot blocks cooking",
        "furnace[facing=north,lit=false]",
        '{Items:[{Slot:0b,id:"minecraft:raw_iron",count:3},{Slot:1b,id:"minecraft:coal",count:2},{Slot:2b,id:"minecraft:stick",count:1}],cooking_total_time:200}',
    ),
    (
        "input without recipe keeps the fuel",
        "furnace[facing=north,lit=false]",
        '{Items:[{Slot:0b,id:"minecraft:stick",count:3},{Slot:1b,id:"minecraft:coal",count:2}]}',
    ),
    (
        "total time zero cooks at once",
        "furnace[facing=north,lit=false]",
        '{Items:[{Slot:0b,id:"minecraft:raw_iron",count:3},{Slot:1b,id:"minecraft:coal",count:2}]}',
    ),
    (
        "stored speed multiplier, relight rescales the progress",
        "furnace[facing=north,lit=false]",
        '{Items:[{Slot:0b,id:"minecraft:raw_iron",count:3},{Slot:1b,id:"minecraft:coal",count:2}],cooking_time_spent:100,cooking_total_time:200,speed_multiplier:2.0f}',
    ),
    (
        "a non-fuel in the fuel slot does not light",
        "furnace[facing=north,lit=false]",
        '{Items:[{Slot:0b,id:"minecraft:raw_iron",count:3},{Slot:1b,id:"minecraft:stone",count:2}],cooking_total_time:200}',
    ),
    (
        "custom name survives",
        "furnace[facing=north,lit=false]",
        '{CustomName:"Hot",Items:[{Slot:0b,id:"minecraft:raw_iron",count:3},{Slot:1b,id:"minecraft:coal",count:2}],cooking_total_time:200}',
    ),
]


def tag_items(name):
    out = []
    for v in json.load(open(os.path.join(TAGS, name + ".json")))["values"]:
        if isinstance(v, dict):
            v = v["id"]
        out += tag_items(v[1:].split(":")[1]) if v.startswith("#") else [v]
    return out


def first_item(ingredient):
    if isinstance(ingredient, list):
        ingredient = ingredient[0]
    if ingredient.startswith("#"):
        return tag_items(ingredient[1:].split(":")[1])[0]
    return ingredient


def position(i):
    return (i % ROW, -60, i // ROW * 2)


def samples(ticks, cases):
    out, last = [], 0
    be = " ".join("be:%d,%d,%d" % p for p in cases)
    for t in ticks:
        out.append("tick %d" % (t - last))
        last = t
        out.append("dump %s blocks:0,-60,0,%d,-60,%d" % (be, ROW - 1, max(p[2] for p in cases)))
    return out


def write(name, lines):
    path = os.path.join(HERE, name)
    with open(path + ".tmp", "w") as f:
        f.write("\n".join(lines) + "\n")
    os.replace(path + ".tmp", path)


recipes = []
for f in sorted(glob.glob(os.path.join(RECIPES, "*.json"))):
    d = json.load(open(f))
    if d["type"] in TYPES:
        block, default = TYPES[d["type"]]
        recipes.append((block, first_item(d["ingredient"]), d.get("cookingtime", default), os.path.basename(f)[:-5]))
recipes.sort()

lines = ["level flat", "preload 2", "# Every cooking recipe in its furnace type with 8 coal, then edge cases: progress, lit toggling, results."]
cases = []
for i, (block, item, time, name) in enumerate(recipes):
    p = position(i)
    cases.append(p)
    lines.append("# " + name)
    lines.append("block %d %d %d minecraft:%s[facing=north,lit=false]" % (p + (block,)))
    lines.append('be_nbt %d %d %d {Items:[{Slot:0b,id:"%s",count:1},{Slot:1b,id:"minecraft:coal",count:8}],cooking_total_time:%d}' % (p + (item, time)))
lines.append("level_seed 99")
for k, (note, state, nbt) in enumerate(EDGE_CASES):
    p = position(len(recipes) + k)
    cases.append(p)
    lines.append("# " + note)
    lines.append("block %d %d %d minecraft:%s" % (p + (state,)))
    lines.append("be_nbt %d %d %d %s" % (p + (nbt,)))
lines += samples([1, 2, 3, 4, 5, 6, 7, 10, 20, 50, 100, 150, 199, 200, 201, 205, 250, 300, 400, 500], cases)
lines.append("dump entities")
write("blk_furnace_cook.txt", lines)

items = json.load(open(os.path.join(REF, "extracted/items.json")))
fuels = collections.OrderedDict()
for item in items:
    components = item.get("components") or {}
    if "minecraft:cooking_fuel" in components:
        key = (json.dumps(components["minecraft:cooking_fuel"], sort_keys=True), item.get("crafting_remainder"))
        fuels.setdefault(key, item["name"])
fuels = sorted(fuels.values())

lines = ["level flat", "preload 2", "# Every distinct cooking fuel (burn time, speed multiplier, remainder) in each furnace type, cooking 8 items."]
cases = []
for block in COOKING_TIMES:
    for fuel in fuels:
        p = position(len(cases))
        cases.append(p)
        lines.append("# %s %s" % (block, fuel))
        lines.append("block %d %d %d minecraft:%s[facing=north,lit=false]" % (p + (block,)))
        lines.append(
            'be_nbt %d %d %d {Items:[{Slot:0b,id:"%s",count:8},{Slot:1b,id:"%s",count:1}],cooking_total_time:%d}'
            % (p + (FUEL_INPUTS[block], fuel, COOKING_TIMES[block]))
        )
lines += samples([1, 2, 3, 10, 50, 99, 100, 101, 150, 199, 200, 201, 300, 400, 600, 800, 1000, 1200, 1600], cases)
lines.append("dump entities")
write("blk_furnace_fuels.txt", lines)
