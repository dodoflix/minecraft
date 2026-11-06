#!/usr/bin/env python3
"""Generates scenarios/brewing_cross.txt: one brewing stand per (reagent, 3 containers) of the full cross product
reagent x {potion, splash_potion, lingering_potion} x every potion (+ junk containers/reagents), 400+ ticks, then `data get`
of every stand.

Usage: python3 crates/mc-game/tests/oracle/gen_brewing_cross.py crates/mc-game/tests/oracle/scenarios/brewing_cross.txt
Output: crates/mc-game/tests/oracle/scenarios/brewing_cross.txt (replaced only when generation succeeds; needs ref/)."""
import glob
import json
import os
import sys

if sys.argv[1:] in (["-h"], ["--help"]):
    print(__doc__)
    sys.exit(0)
if len(sys.argv) != 2:
    sys.exit(__doc__)
reg = json.load(open('ref/generated/reports/registries.json'))
potions = list(reg['minecraft:potion']['entries'])
reagents = set()
for f in glob.glob('ref/src/data/minecraft/recipe/brewing/*.json'):
    reagents.add(json.load(open(f))['reagent']['item'])
reagents = sorted(reagents) + ['minecraft:stick', 'minecraft:diamond', 'minecraft:redstone_block']

def potion(item, p):
    return item, '{"minecraft:potion_contents":{potion:"%s"}}' % p

containers = [potion(i, p) for i in ('minecraft:potion', 'minecraft:splash_potion', 'minecraft:lingering_potion') for p in potions]
containers += [
    ('minecraft:potion', '{}'),
    ('minecraft:glass_bottle', '{}'),
    ('minecraft:splash_potion', '{"minecraft:potion_contents":{custom_effects:[{id:"minecraft:speed",duration:600}]}}'),
    ('minecraft:potion', '{"minecraft:potion_contents":{potion:"minecraft:awkward",custom_effects:[{id:"minecraft:speed",duration:600}]}}'),
    ('minecraft:potion', '{"minecraft:potion_contents":{potion:"minecraft:water",custom_name:"x"}}'),
    ('minecraft:dragon_breath', '{}'),
    ('minecraft:tipped_arrow', '{"minecraft:potion_contents":{potion:"minecraft:water"}}'),
]
stands = []
for r in reagents:
    for k in range(0, len(containers), 3):
        stands.append((r, containers[k:k + 3]))
side = 40
out = ['level flat', 'preload 8']
pos = []
for n, (r, cs) in enumerate(stands):
    x, z = n % side, n // side
    pos.append((x, z))
    items = []
    for slot, (item, comp) in enumerate(cs):
        items.append('{Slot:%db,id:"%s",count:1,components:%s}' % (slot, item, comp))
    items.append('{Slot:3b,id:"%s",count:1}' % r)
    out.append('block %d -60 %d minecraft:brewing_stand' % (x, z))
    out.append('cmd data merge block %d -60 %d {Fuel:20,Items:[%s]}' % (x, z, ','.join(items)))
sample = [p for i, p in enumerate(pos) if i % 9 == 0]
out.append('tick 1')
for x, z in sample:
    out.append('cmd data get block %d -60 %d' % (x, z))
out.append('tick 199')
for x, z in sample:
    out.append('cmd data get block %d -60 %d' % (x, z))
out.append('tick 201')
for x, z in pos:
    out.append('cmd data get block %d -60 %d' % (x, z))
out.append('dump blocks:0,-60,0,%d,-60,%d' % (side - 1, (len(pos) - 1) // side))
with open(sys.argv[1] + '.tmp', 'w') as f:
    f.write('\n'.join(out) + '\n')
os.replace(sys.argv[1] + '.tmp', sys.argv[1])
