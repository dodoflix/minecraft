#!/usr/bin/env python3
"""Generates blk_aquatic_kelp.txt: kelp heads random-ticked at every height they can have (one level seed per round).
Usage: python3 gen_aquatic_kelp.py
Output: blk_aquatic_kelp.txt (next to this script)
"""
from pathlib import Path

PREFIX = '# Kelp heads and plants (GrowingPlantHeadBlock random-tick growth with level random draws, KelpBlock.canAttachTo / canGrowInto, survival\n# on magma/ice/sand, water block replacement when kelp is broken, updateShape turning a body into a head, water spread into the column).\nlevel flat\npreload 3\nlevel_seed 5\n# a tall basin 0..20 x 0..4, floor y=-61 (grass), water to y=-30\nfill -1 -61 -1 23 -28 -1 minecraft:stone\nfill -1 -61 5 23 -28 5 minecraft:stone\nfill -1 -61 0 -1 -28 4 minecraft:stone\nfill 23 -61 0 23 -28 4 minecraft:stone\nfill 0 -60 0 22 -30 4 minecraft:water[level=0]\n# heads of several ages on different floors\nblock 0 -60 0 minecraft:kelp[age=0]\nblock 2 -60 0 minecraft:kelp[age=10]\nblock 4 -60 0 minecraft:kelp[age=23]\nblock 6 -60 0 minecraft:kelp[age=24]\nblock 8 -60 0 minecraft:kelp[age=25]\nblock 10 -61 0 minecraft:sand\nblock 10 -60 0 minecraft:kelp[age=5]\nblock 12 -61 0 minecraft:magma_block\nblock 12 -60 0 minecraft:kelp[age=5]\nblock 14 -61 0 minecraft:ice\nblock 14 -60 0 minecraft:kelp[age=5]\nblock 16 -61 0 minecraft:dirt\nblock 16 -60 0 minecraft:kelp_plant\nblock 16 -59 0 minecraft:kelp_plant\nblock 16 -58 0 minecraft:kelp[age=20]\nblock 18 -61 0 minecraft:stone\nblock 18 -60 0 minecraft:kelp[age=5]\nblock 18 -59 0 minecraft:stone\nblock 20 -60 0 minecraft:kelp[age=5]\nblock 20 -59 0 minecraft:water[level=1]\nblock 22 -60 0 minecraft:kelp[age=5]\nblock 22 -59 0 minecraft:air\ndump blocks:-1,-61,-1,23,-50,5 ticks events\n'

SUFFIX = 'note removing the floor under kelp: it breaks (scheduled tick) and leaves water; stone placed on a head turns the body below it into a head\nblock 10 -61 0 minecraft:air\nblock 4 -60 3 minecraft:kelp_plant\nblock 4 -59 3 minecraft:kelp_plant\nblock 4 -58 3 minecraft:kelp[age=3]\nblock 4 -57 3 minecraft:stone\ntick 5\ndump blocks:-1,-61,-1,23,-40,5 ticks events\n'

HEADS = [(0, 0), (2, 0), (4, 0), (6, 0), (8, 0), (10, 0), (12, 0), (14, 0), (16, 0), (20, 0), (22, 0)]
ROUNDS = 40

out = [PREFIX]
for rnd in range(ROUNDS):
    out.append(f"level_seed {100 + rnd}\n")
    for x, z in HEADS:
        out.extend(f"random_tick {x} {y} {z}\n" for y in range(-60, -44))
    if rnd % 10 == 9:
        out.append("tick 2\n")
out.append("dump blocks:-1,-61,-1,23,-40,5 ticks events\n")
out.append(SUFFIX)
(Path(__file__).parent / "blk_aquatic_kelp.txt").write_text("".join(out))
