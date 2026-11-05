#!/usr/bin/env python3
"""Generates the bulk mob scenarios of the entity oracle (the hand written ones live next to this file as mob_*.txt).
Run from this directory:  python3 gen_mob_scenarios.py        (rewrites mob_trades.txt mob_raid_waves.txt mob_finalize_equipment.txt
mob_local_difficulty.txt mob_spawnpass_*.txt mob_animals_idle.txt), then
  for n in mob_trades mob_raid_waves mob_finalize_equipment mob_local_difficulty mob_spawnpass_plains_night mob_spawnpass_plains_day \\
           mob_spawnpass_biomes mob_animals_idle; do scripts/entity-oracle.sh crates/mc-game/tests/oracle/scenarios/$n.txt \\
           -o crates/mc-game/tests/fixtures/entity_oracle/$n.txt; done
(`scripts/entity-oracle.sh --regen` regenerates every fixture from its scenario, generated or not)."""
import os

HERE = os.path.dirname(os.path.abspath(__file__))


def emit(name, lines):
    with open(os.path.join(HERE, name + ".txt"), "w") as f:
        f.write("\n".join(lines) + "\n")


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
        biomes += [f"biome {-30 + i * 8} -64 -48 {-23 + i * 8} 320 -33 minecraft:{b}"]
    out = base + ["time 18000"] + biomes + ["tick 3"]
    for i in range(8):
        out.append(f"spawnpass {100 + i * 13} chunk={(-30 + i * 8) // 16},-3 cats=monster,creature")
        out.append("tick 1")
    out.append("dump spawn_state")
    emit("mob_spawnpass_biomes", out)


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
