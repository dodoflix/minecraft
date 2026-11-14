#!/usr/bin/env python3
"""Generates the bulk mob scenarios of the entity oracle (the hand written ones live next to this file as mob_*.txt).
It rewrites mob_trades.txt mob_raid_waves.txt mob_finalize_equipment.txt mob_local_difficulty.txt mob_spawnpass_*.txt mob_animals_idle.txt, then
  for n in mob_trades mob_raid_waves mob_finalize_equipment mob_local_difficulty mob_spawnpass_plains_night mob_spawnpass_plains_day \\
           mob_spawnpass_biomes mob_spawnpass_peaceful mob_spawnpass_hard_night mob_spawnpass_water mob_spawnpass_cap \\
           mob_spawnpass_respawn_point mob_spawnpass_structures_swamp_hut mob_spawnpass_structures_monument mob_spawnpass_structures_outpost \\
           mob_spawnpass_fortress mob_animals_idle; do scripts/entity-oracle.sh crates/mc-game/tests/oracle/scenarios/$n.txt \\
           -o crates/mc-game/tests/fixtures/entity_oracle/$n.txt; done
(`scripts/entity-oracle.sh --regen` regenerates every fixture from its scenario, generated or not).

Usage: python3 crates/mc-game/tests/oracle/scenarios/gen_mob_scenarios.py   (takes no arguments; each file is replaced only on success)
Output: crates/mc-game/tests/oracle/scenarios/{mob_trades,mob_raid_waves,mob_finalize_equipment,mob_local_difficulty,mob_spawnpass_*,mob_animals_idle}.txt"""
import os
import sys

if sys.argv[1:] in (["-h"], ["--help"]):
    print(__doc__)
    sys.exit(0)

HERE = os.path.dirname(os.path.abspath(__file__))


def emit(name, lines):
    path = os.path.join(HERE, name + ".txt")
    with open(path + ".tmp", "w") as f:
        f.write("\n".join(lines) + "\n")
    os.replace(path + ".tmp", path)


PROFESSIONS = ["armorer", "butcher", "cartographer", "cleric", "farmer", "fisherman", "fletcher", "leatherworker", "librarian", "mason", "shepherd",
               "toolsmith", "weaponsmith"]
TYPES = ["plains", "desert", "jungle", "savanna", "snow", "swamp", "taiga"]


def trades():
    out = ["# Villager trade generation: profession x level x seed (Villager.updateTrades -> TradeSet loading, entity random seeded per line).",
           "# Dumps the generated MerchantOffers (OFFER lines: costs, result with canonical component SNBT, uses/max/xp/multiplier).",
           "level void"]
    n = 0
    for p in PROFESSIONS:
        for lvl in range(1, 6):
            for seed in (1, 77):
                n += 1
                vtype = TYPES[n % len(TYPES)]
                out.append(f"trades V{n} minecraft:{p} {lvl} {seed * 1000 + n} type=minecraft:{vtype}")
    # the wandering trader rolls its trade set at the first getOffers()
    for seed in range(1, 9):
        out.append(f"mobspawn W{seed} minecraft:wandering_trader {seed}.5 -60 5.5")
        out.append(f"entity_seed W{seed} {seed * 7919}")
        out.append(f"dump offers:W{seed}")
    emit("mob_trades", out)


def raids():
    out = ["# Raid wave composition (getDefaultNumSpawns + getPotentialBonusSpawns with the raid's RNG seeded) per difficulty x omen level x seed.",
           "level void"]
    for diff in ("easy", "normal", "hard"):
        for omen in range(0, 6):
            for seed in (3, 99, 12345):
                out.append(f"raid_waves {diff} {omen} {seed}")
    emit("mob_raid_waves", out)


FINALIZE = [
    "zombie", "husk", "drowned", "zombie_villager", "skeleton", "stray", "bogged", "wither_skeleton", "piglin", "piglin_brute", "zombified_piglin",
    "pillager", "vindicator", "evoker", "vex", "witch", "spider", "cave_spider", "creeper", "slime", "magma_cube", "enderman", "silverfish",
    "blaze", "ghast", "phantom", "sheep", "pig", "cow", "chicken", "rabbit", "horse", "donkey", "mule", "llama", "wolf", "cat", "fox", "panda",
    "villager", "iron_golem", "snow_golem", "turtle", "frog", "axolotl", "goat", "bee", "parrot", "ocelot", "polar_bear", "camel", "sniffer",
]


def finalize():
    out = ["# Mob.finalizeSpawn pipeline: attribute bonus, handedness, equipment/armor/enchant rolls (RNG of the entity), variants, baby chances.",
           "# One block per (type, difficulty, reason): rich E line + the saved NBT (UUID removed).",
           "level void", "time 30000"]
    n = 0
    for diff in ("easy", "hard"):
        out.append(f"difficulty {diff}")
        for t in FINALIZE:
            for reason in ("natural", "spawner"):
                n += 1
                name = f"F{n}"
                out.append(f"mobspawn {name} minecraft:{t} {n % 40}.5 -60 {-6 + (n // 40) * 3}.5 reason={reason}")
                out.append(f"dump mob:{name} nbt:{name}")
    emit("mob_finalize_equipment", out)


def local_difficulty():
    out = ["# DifficultyInstance (local difficulty): base difficulty x world time x moon phase x chunk inhabited time (via /time and ticks).",
           "level flat", "spawn_player Steve survival 0.5 -60 0.5 0 0"]
    for diff in ("peaceful", "easy", "normal", "hard"):
        out.append(f"difficulty {diff}")
        for t in (0, 6000, 13000, 18000, 24000 * 1, 24000 * 8 + 18000, 24000 * 20 + 18000, 24000 * 80):
            out.append(f"time {t}")
            out.append("tick 2")
            out.append("dump difficulty:0,-60,0 difficulty:200,-60,200")
    emit("mob_local_difficulty", out)


def spawnpass(name, header, passes, ticks_before=3, extra=''):
    out = header + ["tick %d" % ticks_before]
    for i, (seed, cx, cz) in enumerate(passes):
        out.append(f"spawnpass {seed} chunk={cx},{cz}{extra}")
        out.append("tick 1")
    out.append("dump spawn_state")
    emit(name, out)


def spawnpass_lines(passes, extra=''):
    """Passes that also dump the level random after each one (`rng:level`: a pass that spawns nothing still draws)."""
    out = []
    for seed, cx, cz in passes:
        out.append(f"spawnpass {seed} chunk={cx},{cz}{extra}")
        out.append("dump rng:level")
        out.append("tick 1")
    return out


def water():
    # the flat world's sea level is -63: the pools are dug (water -64..-62 through the bedrock floor, air above) so that the water animal
    # rules (from 13 blocks below the sea level up to it) can hold; the biome of a pool decides its spawn table, clay under the lush pool
    out = ["level flat", "difficulty normal", "spawn_player Steve survival -64.5 -60 30.5 0 0", "time 18000"]
    # (fish and the like spawn only within 64 blocks of a player: Steve stands south of the pools, more than 24 blocks from them)
    pools = [("ocean", -6, -2), ("warm_ocean", -5, -2), ("cold_ocean", -4, -2), ("river", -3, -2), ("frozen_river", -6, -1),
             ("lukewarm_ocean", -5, -1), ("deep_ocean", -4, -1), ("lush_caves", -3, -1)]
    for b, cx, cz in pools:
        x, z = cx * 16, cz * 16
        out.append(f"fill {x} -61 {z} {x + 15} -61 {z + 15} minecraft:air")
        out.append(f"fill {x} -64 {z} {x + 15} -62 {z + 15} minecraft:water")
        out.append(f"biome {x} -64 {z} {x + 15} -58 {z + 15} minecraft:{b}")
        if b == "lush_caves":
            out.append(f"fill {x} -64 {z} {x + 15} -64 {z + 15} minecraft:clay")
    out.append("tick 3")
    seed = 5
    where = {b: (cx, cz) for b, cx, cz in pools}
    # rivers keep 2 % of their ambient spawns (the reduced water ambient spawns): many passes for the first fish
    out += spawnpass_lines([(seed + k * 13, *where["river"]) for k in range(120)], " cats=water_ambient")
    # the caps (water ambient 20, water creature 5) fill up with the first groups: the pools take turns
    for cats, order in (("water_ambient,underground_water_creature,axolotls,monster",
                         ["warm_ocean", "lukewarm_ocean", "lush_caves", "cold_ocean", "ocean", "deep_ocean", "frozen_river"]),
                        ("water_creature", ["warm_ocean", "lukewarm_ocean", "ocean", "river", "deep_ocean", "cold_ocean"])):
        for rnd in range(3):
            for b in order:
                seed += 29
                out += spawnpass_lines([(seed, *where[b])], f" cats={cats}")
        out.append("tick 2")
    out += ["dump mobs", "tick 40", "dump mobs", "dump spawn_state"]
    emit("mob_spawnpass_water", out)


def cap():
    # a second player far away doubles the global caps while the per-player caps stay: 70 zombies around Steve block the
    # monster category near him only, 70 more around Alex fill the global cap; 10 / 20 cows do the same for the creature category
    out = ["level flat", "difficulty normal", "spawn_player Steve survival 60.5 -60 60.5 0 0",
           "spawn_player Alex survival 300.5 -60 60.5 0 0", "time 18000"]
    n = 0
    for j in range(7):
        for i in range(10):
            out.append(f"mobspawn Z{n} minecraft:zombie {30.5 + i * 8} -60 {30.5 + j * 8} reason=natural {{NoAI:1b}}")
            n += 1
    for i in range(10):
        out.append(f"mobspawn C{i} minecraft:cow {36.5 + i * 8} -60 20.5 reason=natural {{NoAI:1b}}")
    out.append("tick 3")
    steve = [(s * 7 + 1, 2 + s % 5, 2 + s // 5 % 4) for s in range(10)]
    alex = [(s * 7 + 3, 17 + s % 4, 1 + s // 4 % 5) for s in range(10)]
    both = [p for pair in zip(steve, alex) for p in pair]
    out += spawnpass_lines(both) + ["dump spawn_state"]
    out += spawnpass_lines([(s * 5 + 2, cx, cz) for s, cx, cz in both], " persistent=true")
    for j in range(7):
        for i in range(10):
            out.append(f"mobspawn Y{n} minecraft:zombie {300.5 - 40 + i * 8} -60 {30.5 + j * 8} reason=natural {{NoAI:1b}}")
            n += 1
    for i in range(10):
        out.append(f"mobspawn D{i} minecraft:cow {260.5 + i * 8} -60 20.5 reason=natural {{NoAI:1b}}")
    out.append("tick 2")
    out += spawnpass_lines([(s * 3 + 9, cx, cz) for s, cx, cz in both], " persistent=true")
    out += ["dump spawn_state", "dump mobs"]
    emit("mob_spawnpass_cap", out)


def respawn_point():
    # no spawn within 24 blocks of the respawn position (first the flat world's default, then a moved one) and none within 24 of a player
    out = ["level flat", "difficulty normal", "command_env", "spawn_player Steve survival 100.5 -60 100.5 0 0", "time 18000", "tick 3"]
    near = [(s * 7 + 5, (s % 4) - 2, (s // 4) - 2) for s in range(16)]
    out += spawnpass_lines(near, " cats=monster")
    out += spawnpass_lines([(s * 7 + 6, 4 + s % 4, 4 + s // 4) for s in range(16)], " cats=monster")
    out += ["cmd setworldspawn 150 -60 150", "tick 1"]
    out += spawnpass_lines([(s * 7 + 7, 6 + s % 4, 6 + s // 4) for s in range(16)], " cats=monster")
    out += spawnpass_lines([(s * 7 + 8, (s % 4) - 2, (s // 4) - 2) for s in range(16)], " cats=monster")
    out += ["dump spawn_state", "dump mobs"]
    emit("mob_spawnpass_respawn_point", out)


def structure_world(name, comment, seed, player, body, header=(), tail=()):
    """A seeded normal world with structures; the player stands near the structure (its chunks load around him), the body spawns."""
    out = [comment, "level normal", "structures true", f"seed {seed}", "preload 0", "difficulty normal", *header,
           f"spawn_player Steve survival {player} 0 0", "time 18000", "tick 60"]
    emit(name, out + body + list(tail))


def structures():
    # the spawn overrides of structures (`spawn_overrides` of the structure's data: the pieces' or the whole bounding box's own spawn list)
    body = []
    # swamp hut (the witch spawns only inside a piece): attempts at floor level in and around the hut, then passes over its chunk
    # (the hut's own witch and cat are killed first: they wander inside it and a spawn attempt checks collisions with them)
    body = ["cmd kill @e[type=minecraft:witch]", "cmd kill @e[type=minecraft:cat]", "tick 5"]
    # (no ticks between the attempts: the witches' own movement would differ from run to run)
    for k, (x, z) in enumerate((x, z) for z in range(-689, -678, 2) for x in range(-226, -214, 2)):
        body += [f"spawnat {k * 7 + 1} monster {x} 65 {z}"]
    body += ["tick 1"]
    structure_world("mob_spawnpass_structures_swamp_hut", "# Swamp hut: the spawn override of its pieces (witch) and the swamp's own monster list around it.",
                    5, "-180.5 66 -688.5", body, ["command_env"], ["dump mobs"])
    # ocean monument (guardians spawn inside its whole bounding box, in water)
    body = []
    k = 0
    for y in (46, 54):
        for z in range(184, 236, 8):
            for x in range(232, 284, 8):
                body += [f"spawnat {k * 7 + 1} monster {x} {y} {z}", "tick 1"]
                k += 1
    body += spawnpass_lines([(j * 5 + 2, 14 + j % 4, 11 + (j // 4) % 4) for j in range(48)], " cats=monster")
    structure_world("mob_spawnpass_structures_monument", "# Ocean monument: the guardians of its spawn override, and the ocean's lists around it.",
                    6, "310.5 62 208.5", body)
    # pillager outpost (the pillager spawns inside its bounding box, on the ground): attempts one block above the terrain of a grid of columns
    tops = [[67, 65, 65, 65, 64, 64, 65, 68, 66, 67, 68, 68], [67, 65, 65, 64, 64, 64, 66, 67, 66, 66, 67, 67],
            [66, 65, 65, 67, 66, 64, 65, 65, 65, 66, 66, 67], [64, 65, 67, 67, 64, 63, 63, 63, 63, 65, 66, 66],
            [64, 65, 65, 63, 65, 76, 76, 64, 63, 64, 66, 67], [65, 63, 63, 65, 63, 83, 83, 82, 63, 65, 66, 67],
            [65, 63, 62, 64, 63, 83, 83, 82, 63, 64, 66, 66], [65, 65, 65, 63, 63, 76, 76, 63, 63, 65, 66, 66],
            [65, 69, 66, 64, 63, 63, 63, 66, 65, 65, 65, 65], [66, 64, 65, 64, 64, 63, 65, 67, 65, 69, 65, 65],
            [64, 66, 68, 66, 65, 63, 63, 64, 64, 65, 64, 64], [64, 64, 66, 65, 64, 63, 63, 64, 66, 66, 64, 64]]
    body = []
    for j, row in enumerate(tops):
        for i, top in enumerate(row):
            body += [f"spawnat {(j * 12 + i) * 5 + 2} monster {130 + i * 4} {top + 1} {-319 + j * 4}", "tick 1"]
    structure_world("mob_spawnpass_structures_outpost", "# Pillager outpost: the pillagers of its spawn override on the outpost's ground.",
                    9, "205.5 70 -300.5", body)


def fortress():
    # a nether fortress (the `FORTRESS_ENEMIES` list replaces the biome's monsters on nether bricks inside it): attempts one block above its brick
    # floors; seed 1's nearest fortress starts at chunk -6,9, the player stands south of it (the attempts stay 24 blocks away from him)
    floors = [
        (-122, 66, 144), (-120, 66, 144), (-118, 58, 144), (-106, 58, 144), (-104, 58, 144), (-98, 58, 144),
        (-86, 58, 144), (-84, 58, 144), (-120, 59, 146), (-118, 58, 146), (-116, 59, 146), (-108, 63, 146),
        (-106, 58, 146), (-104, 58, 146), (-98, 58, 146), (-86, 58, 146), (-84, 58, 146), (-120, 59, 148),
        (-118, 58, 148), (-116, 59, 148), (-108, 58, 148), (-106, 58, 148), (-104, 58, 148), (-102, 58, 148),
        (-98, 58, 148), (-86, 58, 148), (-84, 58, 148), (-118, 58, 150), (-106, 58, 150), (-104, 58, 150),
        (-98, 58, 150), (-86, 58, 150), (-84, 58, 150), (-118, 58, 152), (-106, 58, 152), (-104, 58, 152),
        (-98, 58, 152), (-86, 58, 152), (-84, 58, 152), (-128, 58, 154), (-126, 58, 154), (-124, 58, 154),
        (-122, 58, 154), (-120, 58, 154), (-118, 58, 154), (-116, 58, 154), (-114, 58, 154), (-112, 58, 154),
        (-110, 58, 154), (-108, 58, 154), (-106, 58, 154), (-104, 58, 154), (-102, 58, 154), (-100, 58, 154),
        (-98, 58, 154), (-96, 58, 154), (-94, 58, 154), (-92, 58, 154), (-90, 58, 154), (-88, 58, 154),
        (-86, 58, 154), (-84, 58, 154), (-82, 58, 154), (-80, 58, 154), (-78, 58, 154), (-76, 58, 154),
        (-74, 58, 154), (-72, 58, 154), (-70, 58, 154), (-68, 58, 154), (-66, 58, 154), (-64, 58, 154),
        (-62, 58, 154), (-126, 58, 156), (-124, 58, 156), (-122, 58, 156), (-120, 58, 156), (-118, 58, 156),
        (-116, 58, 156), (-114, 58, 156), (-112, 58, 156), (-110, 58, 156), (-108, 58, 156), (-108, 63, 156),
        (-106, 58, 156), (-104, 58, 156), (-102, 58, 156), (-100, 58, 156), (-98, 58, 156), (-96, 58, 156),
        (-94, 58, 156), (-92, 58, 156), (-90, 58, 156), (-88, 58, 156), (-86, 58, 156), (-84, 58, 156),
        (-82, 58, 156), (-80, 58, 156), (-78, 58, 156), (-76, 58, 156), (-74, 58, 156), (-70, 58, 156),
        (-68, 58, 156), (-64, 58, 156), (-62, 58, 156), (-118, 58, 158), (-108, 63, 158), (-106, 58, 158),
        (-104, 58, 158), (-98, 58, 158), (-86, 58, 158), (-84, 58, 158), (-118, 58, 160), (-116, 59, 160),
        (-106, 58, 160), (-104, 58, 160), (-98, 58, 160), (-86, 58, 160), (-84, 58, 160), (-118, 58, 162),
        (-116, 59, 162), (-106, 58, 162), (-104, 58, 162), (-98, 58, 162), (-86, 58, 162), (-84, 58, 162),
        (-120, 59, 164), (-118, 58, 164), (-116, 59, 164), (-106, 58, 164), (-104, 58, 164), (-98, 58, 164),
        (-86, 58, 164), (-84, 58, 164), (-128, 61, 166), (-126, 61, 166), (-124, 62, 166), (-122, 61, 166),
        (-120, 58, 166), (-118, 58, 166), (-116, 58, 166), (-106, 58, 166), (-104, 58, 166), (-98, 58, 166),
        (-128, 61, 168), (-126, 61, 168), (-124, 60, 168), (-122, 58, 168), (-120, 58, 168), (-118, 58, 168),
        (-116, 58, 168), (-106, 58, 168), (-104, 58, 168), (-98, 58, 168), (-128, 61, 170), (-126, 61, 170),
        (-124, 62, 170), (-122, 61, 170), (-120, 58, 170), (-118, 58, 170), (-116, 58, 170), (-106, 58, 170),
        (-104, 58, 170), (-98, 58, 170), (-120, 59, 172), (-118, 58, 172), (-116, 59, 172), (-106, 58, 172),
        (-104, 58, 172), (-98, 58, 172), (-120, 59, 174), (-118, 58, 174), (-116, 59, 174), (-106, 58, 174),
        (-104, 58, 174), (-98, 58, 174), (-120, 59, 176), (-118, 58, 176), (-116, 59, 176), (-106, 58, 176),
        (-104, 58, 176), (-98, 58, 176), (-120, 59, 178), (-118, 58, 178), (-116, 59, 178), (-100, 61, 178),
        (-98, 58, 178), (-96, 61, 178), (-120, 59, 180), (-118, 58, 180), (-116, 59, 180), (-100, 62, 180),
        (-98, 60, 180), (-96, 62, 180),
    ]
    out = ["# Nether fortress: the fortress monster list on nether bricks.", "level normal", "structures true", "seed 1", "dimension the_nether", "preload 0",
           "difficulty normal", "spawn_player Steve survival -96.5 70 205.5 0 0", "tick 100"]
    for k, (x, y, z) in enumerate(floors):
        out.append(f"spawnat {k * 7 + 1} monster {x} {y} {z}")
    emit("mob_spawnpass_fortress", out + ["tick 1", "dump mobs"])


def spawnpasses():
    # the server's own spawning stays off (spawn_mobs false, the oracle default): the `spawnpass` command runs the passes explicitly
    base = ["level flat", "difficulty normal",
            # the player far away from the chunks that spawn (spawn needs distance >= 24 and < 128 to the nearest player)
            "spawn_player Steve survival 60.5 -60 60.5 0 0"]
    passes = [(s * 7 + 1, (s % 5) - 2, (s // 5) - 2) for s in range(25)]
    spawnpass("mob_spawnpass_plains_night", base + ["time 18000"], passes)
    spawnpass("mob_spawnpass_plains_day", base + ["time 6000"], passes, 3, " persistent=true")
    biomes = []
    for i, b in enumerate(["desert", "swamp", "jungle", "snowy_plains", "mushroom_fields", "dark_forest", "savanna", "cherry_grove"]):
        biomes += [f"biome {-30 + i * 8} -64 -48 {-23 + i * 8} 64 -33 minecraft:{b}"]
    out = base + ["time 18000"] + biomes + ["tick 3"]
    for i in range(8):
        out.append(f"spawnpass {100 + i * 13} chunk={(-30 + i * 8) // 16},-3 cats=monster,creature")
        out.append("tick 1")
    out.append("dump spawn_state")
    emit("mob_spawnpass_biomes", out)
    # peaceful: the monster pass still runs and draws (nothing spawns). The spawning flag is the spawn rules, not the difficulty: the
    # rules are only on around one pass each (the ticks would spawn naturally, `enemies=flag` reads the flag the rules set)
    peaceful = ["level flat", "difficulty peaceful", "spawn_player Steve survival 60.5 -60 60.5 0 0", "time 18000", "tick 3"]
    peaceful += spawnpass_lines(passes[:12])
    peaceful += ["time 6000", "tick 2"] + spawnpass_lines(passes[12:], " persistent=true")
    for monsters in ("true", "false", "true", "false"):
        peaceful += ["gamerule spawn_mobs true", f"gamerule spawn_monsters {monsters}", f"spawnpass {len(peaceful)} chunk=-2,-2 enemies=flag persistent=true",
                     "dump rng:level", "gamerule spawn_mobs false", "tick 1"]
    peaceful += ["dump spawn_state", "dump mobs"]
    emit("mob_spawnpass_peaceful", peaceful)
    # hard: night packs on a wider grid, twice (the mob cap fills up and the local difficulty rises)
    wide = [(s * 11 + 3, (s % 7) - 3, (s // 7) % 7 - 3) for s in range(49)]
    hard = base[:1] + ["difficulty hard"] + base[2:] + ["time 18000", "tick 3"]
    hard += spawnpass_lines(wide, " cats=monster") + ["dump spawn_state", "dump mobs"]
    emit("mob_spawnpass_hard_night", hard)
    water()
    cap()
    respawn_point()
    structures()
    fortress()


def animals_idle():
    out = ["# Passive animals with real AI on flat grass (goal selection, strolls, look-around, panic after a hit), rich E line every 5 ticks.",
           "level flat", "difficulty normal", "spawn_player Steve creative 0.5 -60 0.5 0 0",
           "mobspawn Pig minecraft:pig 8.5 -60 8.5", "mobspawn Cow minecraft:cow 8.5 -60 -8.5", "mobspawn Sheep minecraft:sheep -8.5 -60 8.5",
           "mobspawn Chicken minecraft:chicken -8.5 -60 -8.5", "tick 2", "dump mobs"]
    for i in range(60):
        out.append("tick 5")
        out.append("dump mobs")
    out.append("hurt Pig minecraft:generic 1")
    for i in range(20):
        out.append("tick 2")
        out.append("dump mob:Pig")
    emit("mob_animals_idle", out)


if __name__ == "__main__":
    trades()
    raids()
    finalize()
    local_difficulty()
    spawnpasses()
    animals_idle()
