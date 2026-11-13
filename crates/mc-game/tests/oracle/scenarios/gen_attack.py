#!/usr/bin/env python3
"""Generates tests/oracle/scenarios/attack_*.txt (melee attack pipeline scenarios for scripts/entity-oracle.sh).
Victims are NoAI mobs (they never move or tick physics), one fresh victim per attack, the attacker is moved next to it with `snap`.
Fixtures: scripts/entity-oracle.sh --regen.

Usage: python3 crates/mc-game/tests/oracle/scenarios/gen_attack.py   (takes no arguments; writes several files, each replaced only on success)
Output: crates/mc-game/tests/oracle/scenarios/attack_{weapons,cooldown,crit_sweep,enchant,armor,shield,modes,entities,mob,mace}.txt"""
import os
import sys

if sys.argv[1:] in (["-h"], ["--help"]):
    print(__doc__)
    sys.exit(0)

HERE = os.path.dirname(os.path.abspath(__file__))
NOAI = "{NoAI:1b,PersistenceRequired:1b}"


class Scenario:
    def __init__(self, title, spacing=3, per_row=20):
        self.lines = [f"# {title}", "level flat", "difficulty normal"]
        self.spacing, self.per_row = spacing, per_row
        self.seed = 100
        self.n = 0

    def add(self, *lines):
        self.lines.extend(lines)

    def tough(self, ref):
        """Victims never die (death drops / experience are not what these scenarios test)."""
        self.add(f"attribute {ref} minecraft:max_health 1000", f"set {ref} health 1000")

    def seeded(self, ref):
        self.seed += 1
        self.add(f"entity_seed {ref} {self.seed}")

    def slot(self):
        """Position of the next attacker/victim pair: (attacker x, attacker z, victim z)."""
        i = self.n
        self.n += 1
        x = self.spacing * (i % self.per_row) + 0.5
        z = 10 * (i // self.per_row) + 0.5
        return x, z

    def mob(self, ref, kind, x, z, dz=1.5, yaw=180, items=()):
        self.add(f"spawn {ref} minecraft:{kind} {x} -60 {z + dz} {yaw} 0 {NOAI}")
        self.seeded(ref)
        self.tough(ref)
        for slot, item in items:
            self.add(f"give {ref} {slot} {item}")

    def save(self, name):
        path = os.path.join(HERE, f"attack_{name}.txt")
        with open(path + ".tmp", "w") as f:
            f.write("\n".join(self.lines) + "\n")
        os.replace(path + ".tmp", path)


def weapons():
    s = Scenario("Melee attack damage by held item: attack damage/speed attributes, full-strength hit on a husk (armor 2), durability, exhaustion, knockback, spears (piercing weapons are not attackable by punch).")
    items = ["", "minecraft:stick", "minecraft:wooden_sword", "minecraft:stone_sword", "minecraft:iron_sword", "minecraft:diamond_sword", "minecraft:netherite_sword", "minecraft:golden_sword",
             "minecraft:wooden_axe", "minecraft:iron_axe", "minecraft:diamond_axe", "minecraft:netherite_axe", "minecraft:wooden_pickaxe", "minecraft:diamond_pickaxe", "minecraft:diamond_shovel",
             "minecraft:diamond_hoe", "minecraft:trident", "minecraft:bow", "minecraft:cobblestone", "minecraft:iron_spear", "minecraft:netherite_spear"]
    pairs = []
    for i, item in enumerate(items):
        x, z = s.slot()
        s.mob(f"H{i}", "husk", x, z)
        pairs.append((x, z, item))
    s.add("spawn_player A survival 0.5 -60 0.5 0 0")
    s.seeded("A")
    s.add("tick 2")
    for i, (x, z, item) in enumerate(pairs):
        s.add(f"snap A {x} -60 {z} 0 0")
        if item:
            s.add(f"give A mainhand {item}")
        s.add("tick 40", f"attack A H{i}", f"dump player:A entity:H{i} inventory:A")
    s.save("weapons")


def cooldown():
    s = Scenario("Attack strength scale: swing (ticker reset) then wait k ticks, then hit. Sword (speed 1.6), axe (speed 1.0), hand; hotbar swap of a different item resets the ticker.")
    ks = [0, 1, 2, 3, 4, 5, 6, 8, 10, 11, 12, 13, 14, 16, 19, 20, 25]
    rows = []
    for item in ["minecraft:diamond_sword", "minecraft:diamond_axe", ""]:
        for k in ks:
            x, z = s.slot()
            s.mob(f"H{len(rows)}", "husk", x, z)
            rows.append((x, z, item, k))
    s.add("spawn_player A survival 0.5 -60 0.5 0 0")
    s.seeded("A")
    for i, (x, z, item, k) in enumerate(rows):
        s.add(f"snap A {x} -60 {z} 0 0")
        if i == 0 or rows[i - 1][2] != item:
            s.add(f"give A mainhand {item}" if item else "give A mainhand minecraft:air")
            s.add("tick 60")
        s.add("swing A")
        if k:
            s.add(f"tick {k}")
        s.add(f"attack A H{i}", f"dump player:A entity:H{i} inventory:A")
    # hotbar swap: same item in two slots keeps the ticker, a different item resets it
    x, z = s.slot()
    s.mob("S1", "husk", x, z)
    s.add(f"snap A {x} -60 {z} 0 0", "give A hotbar0 minecraft:diamond_sword", "give A hotbar1 minecraft:diamond_sword", "give A hotbar2 minecraft:diamond_axe", "select A 0", "tick 40",
          "select A 1", "tick 1", "attack A S1", "dump player:A entity:S1")
    x, z = s.slot()
    s.mob("S2", "husk", x, z)
    s.add(f"snap A {x} -60 {z} 0 0", "tick 40", "select A 2", "tick 1", "attack A S2", "dump player:A entity:S2")
    s.save("cooldown")


def crit_sprint_sweep():
    s = Scenario("Critical hits (falling, airborne, not sprinting/blind/in water), sprint knockback hits, weak/strong sounds, sweep attacks (sword, on ground, full strength, slow, no crit/sprint) with sweeping edge and bystanders.", spacing=8, per_row=12)
    def row(ref, item, setup, extra_victims=(), post=lambda x, z: (), kind="husk"):
        x, z = s.slot()
        s.mob(ref, kind, x, z)
        extra = []
        for j, (dx, dz, k2, items2) in enumerate(extra_victims):
            r2 = f"{ref}n{j}"
            s.add(f"spawn {r2} minecraft:{k2} {x + dx} -60 {z + 1.5 + dz} 180 0 {NOAI}")
            s.seeded(r2)
            s.tough(r2)
            for slot, it in items2:
                s.add(f"give {r2} {slot} {it}")
            extra.append(r2)
        s.add(f"snap A {x} -60 {z} 0 0", f"give A mainhand {item}", "tick 40")
        for line in setup(x, z):
            s.add(line)
            if line.startswith("move_packet"):
                s.add("client_tick_end A")
        s.add(f"attack A {ref}", f"dump player:A entity:{ref} " + " ".join(f"entity:{r}" for r in extra) + " inventory:A")
        post_lines = list(post(x, z))
        for line in post_lines:
            s.add(line)
            if line.startswith("move_packet"):
                s.add("client_tick_end A")
    s.add("spawn_player A survival 0.5 -60 0.5 0 0")
    s.seeded("A")
    s.add("tick 2")
    air = lambda x, z: [f"move_packet A {x} -59.5 {z} false", "set A fall_distance 2"]
    land = lambda x, z: [f"move_packet A {x} -60 {z} true"]
    row("C1", "minecraft:diamond_sword", air, post=land)
    row("C2", "minecraft:diamond_axe", air, post=land)
    row("C3", "minecraft:diamond_sword", lambda x, z: air(x, z) + ["pcommand A START_SPRINTING"], post=lambda x, z: ["pcommand A STOP_SPRINTING"] + land(x, z))
    row("C4", "minecraft:diamond_sword", lambda x, z: air(x, z) + ["effect A minecraft:blindness 0 400"], post=lambda x, z: land(x, z) + ["cmd effect clear A minecraft:blindness"])
    row("C5", "minecraft:diamond_sword", lambda x, z: [f"block {int(x)} -60 {int(z)} minecraft:water[level=0]", f"block {int(x)} -59 {int(z)} minecraft:water[level=0]", "tick 2"] + air(x, z), post=lambda x, z: land(x, z) + [f"block {int(x)} -60 {int(z)} minecraft:air", f"block {int(x)} -59 {int(z)} minecraft:air"])
    row("C6", "minecraft:diamond_sword", lambda x, z: ["set A fall_distance 2"], post=lambda x, z: [])  # on the ground: fall distance alone is no crit
    row("C7", "minecraft:diamond_sword", lambda x, z: air(x, z) + ["set A fall_distance 0"], post=land)  # fallDistance must be > 0
    row("K1", "minecraft:diamond_sword", lambda x, z: ["pcommand A START_SPRINTING"], post=lambda x, z: ["pcommand A STOP_SPRINTING"])
    row("K2", "minecraft:diamond_axe", lambda x, z: ["pcommand A START_SPRINTING"], post=lambda x, z: ["pcommand A STOP_SPRINTING"])
    row("K3", "minecraft:diamond_sword", lambda x, z: ["swing A", "tick 3", "pcommand A START_SPRINTING"], post=lambda x, z: ["pcommand A STOP_SPRINTING"])  # weak sprint hit: no knockback bonus
    row("K4", "minecraft:diamond_sword", lambda x, z: ["swing A", "tick 6"])  # weak, no sweep
    side = [(1.0, 0, "husk", ()), (-1.2, 0.3, "husk", ()), (0.0, 1.0, "spider", ()), (3.0, 0, "husk", ())]
    row("W1", "minecraft:iron_sword", lambda x, z: [], side)
    row("W2", "minecraft:iron_sword[enchantments={\"minecraft:sweeping_edge\":1}]", lambda x, z: [], side)
    row("W3", "minecraft:iron_sword[enchantments={\"minecraft:sweeping_edge\":3}]", lambda x, z: [], side)
    row("W4", "minecraft:diamond_sword[enchantments={\"minecraft:sweeping_edge\":3,\"minecraft:sharpness\":5}]", lambda x, z: [], [(1.0, 0, "husk", (("head", "minecraft:iron_helmet"), ("chest", "minecraft:iron_chestplate"))), (-1.2, 0.3, "husk", ())])
    row("W5", "minecraft:diamond_axe", lambda x, z: [], side)  # axes do not sweep
    row("W6", "minecraft:diamond_sword", lambda x, z: [f"move_packet A {x + 0.1} -60 {z} true"], side, post=lambda x, z: [f"move_packet A {x} -60 {z} true"])  # slow enough
    row("W7", "minecraft:diamond_sword", lambda x, z: [f"move_packet A {x + 0.4} -60 {z} true"], side, post=lambda x, z: [f"move_packet A {x} -60 {z} true"])  # too fast: no sweep
    row("W8", "minecraft:diamond_sword", lambda x, z: ["pcommand A START_SPRINTING"], side, post=lambda x, z: ["pcommand A STOP_SPRINTING"])  # sprint: knockback attack, no sweep
    row("W9", "minecraft:diamond_sword", lambda x, z: ["swing A", "tick 5"], side)  # weak: no sweep
    row("W10", "minecraft:stick", lambda x, z: [], side)  # stick is not a sword
    row("W11", "minecraft:diamond_sword", lambda x, z: air(x, z), side, post=land)  # crit: no sweep
    s.save("crit_sweep")


def enchant():
    s = Scenario("Weapon enchantments on a husk (undead), a spider (arthropod) and a cow: sharpness, smite, bane of arthropods, fire aspect, knockback, unbreaking; plus attribute attack_damage/attack_knockback.")
    s.add("level_seed 77")
    enchants = [("sharpness", 1), ("sharpness", 3), ("sharpness", 5), ("smite", 3), ("smite", 5), ("bane_of_arthropods", 3), ("bane_of_arthropods", 5),
                ("fire_aspect", 1), ("fire_aspect", 2), ("knockback", 1), ("knockback", 2), ("unbreaking", 3), ("impaling", 3), ("density", 4)]
    rows = []
    for name, lvl in enchants:
        for kind in ["husk", "spider", "cow"]:
            x, z = s.slot()
            s.mob(f"V{len(rows)}", kind, x, z)
            rows.append((x, z, name, lvl))
    s.add("spawn_player A survival 0.5 -60 0.5 0 0")
    s.seeded("A")
    s.add("tick 2")
    for i, (x, z, name, lvl) in enumerate(rows):
        item = "minecraft:diamond_sword"
        s.add(f"snap A {x} -60 {z} 0 0", f"give A mainhand {item}[enchantments={{\"minecraft:{name}\":{lvl}}}]", "tick 40", f"attack A V{i}", f"dump player:A entity:V{i} inventory:A")
    # attributes of the attacker
    for j, (attr, val) in enumerate([("minecraft:attack_damage", 10), ("minecraft:attack_knockback", 2), ("minecraft:attack_speed", 4)]):
        x, z = s.slot()
        s.mob(f"B{j}", "husk", x, z)
        s.add("attribute A minecraft:attack_damage 1", "attribute A minecraft:attack_knockback 0", "attribute A minecraft:attack_speed 4", f"attribute A {attr} {val}", f"snap A {x} -60 {z} 0 0",
              "give A mainhand minecraft:iron_sword", "tick 40", f"attack A B{j}", f"dump player:A entity:B{j} inventory:A")
    s.save("enchant")


def armor():
    s = Scenario("Armored / resistant / absorbing player victims (full hit with an iron sword, diamond axe): armor + toughness, protection, resistance, absorption, armor durability, hurt/knockback state.", spacing=4)
    s.add("level_seed 5")
    sets = {
        "none": [],
        "leather": [("head", "minecraft:leather_helmet"), ("chest", "minecraft:leather_chestplate"), ("legs", "minecraft:leather_leggings"), ("feet", "minecraft:leather_boots")],
        "iron": [("head", "minecraft:iron_helmet"), ("chest", "minecraft:iron_chestplate"), ("legs", "minecraft:iron_leggings"), ("feet", "minecraft:iron_boots")],
        "diamond": [("head", "minecraft:diamond_helmet"), ("chest", "minecraft:diamond_chestplate"), ("legs", "minecraft:diamond_leggings"), ("feet", "minecraft:diamond_boots")],
        "netherite": [("head", "minecraft:netherite_helmet"), ("chest", "minecraft:netherite_chestplate"), ("legs", "minecraft:netherite_leggings"), ("feet", "minecraft:netherite_boots")],
        "protection": [(sl, f"{it}[enchantments={{\"minecraft:protection\":4}}]") for sl, it in [("head", "minecraft:iron_helmet"), ("chest", "minecraft:iron_chestplate"), ("legs", "minecraft:iron_leggings"), ("feet", "minecraft:iron_boots")]],
        "thorns": [(sl, f"{it}[enchantments={{\"minecraft:thorns\":3}}]") for sl, it in [("head", "minecraft:iron_helmet"), ("chest", "minecraft:iron_chestplate"), ("legs", "minecraft:iron_leggings"), ("feet", "minecraft:iron_boots")]],
    }
    effects = [[], ["effect {v} minecraft:resistance 1 400"], ["effect {v} minecraft:resistance 4 400"], ["effect {v} minecraft:absorption 1 400"], ["effect {v} minecraft:weakness 1 400"]]
    rows = []
    for sname, pieces in sets.items():
        for ei, fx in enumerate(effects):
            if sname not in ("none", "iron", "thorns") and ei != 0:
                continue
            x, z = s.slot()
            ref = f"P{len(rows)}"
            s.add(f"spawn_player {ref} survival {x} -60 {z + 1.5} 180 0")
            s.seeded(ref)
            for slot, item in pieces:
                s.add(f"give {ref} {slot} {item}")
            for e in fx:
                s.add(e.format(v=ref))
            rows.append((x, z, ref))
    s.add("spawn_player A survival 0.5 -60 0.5 0 0")
    s.seeded("A")
    s.add("tick 2")
    for item in ["minecraft:iron_sword", "minecraft:diamond_axe"]:
        for i, (x, z, ref) in enumerate(rows):
            s.add(f"snap A {x} -60 {z} 0 0", f"give A mainhand {item}", "tick 40")
            # the thorns rolls and the armor slot pick draw from the level random, which the attack replay does not tick
            s.add(f"level_seed {5 + i}", f"attack A {ref}", f"dump player:A player:{ref} inventory:{ref}", "set A health 20", f"set {ref} health 20")
            s.add("tick 12")
    s.save("armor")


def shield():
    s = Scenario("Blocking player victims: a shield in front blocks the hit (no damage, shield durability, knockback of the attacker side), behind it does not; an axe disables the shield; thorns.", spacing=5)
    rows = []
    for item in ["minecraft:iron_sword", "minecraft:diamond_axe", "minecraft:netherite_axe"]:
        for facing in ["front", "behind", "front_delay"]:
            x, z = s.slot()
            ref = f"V{len(rows)}"
            yaw = 0 if facing == "behind" else 0  # victim at z+1.5 looks at -z = yaw 180 (front) / +z = yaw 0 (behind)
            yaw = 180 if facing != "behind" else 0
            s.add(f"spawn_player {ref} survival {x} -60 {z + 1.5} {yaw} 0", f"give {ref} offhand minecraft:shield")
            s.seeded(ref)
            rows.append((x, z, ref, item, facing))
    s.add("spawn_player A survival 0.5 -60 0.5 0 0")
    s.seeded("A")
    s.add("tick 2")
    for x, z, ref, item, facing in rows:
        s.add(f"snap A {x} -60 {z} 0 0", f"give A mainhand {item}", f"use {ref} off")
        s.add("tick 1" if facing == "front_delay" else "tick 40")
        s.add(f"attack A {ref}", f"dump player:A player:{ref} inventory:{ref} packets:{ref}", "tick 2", f"dump player:{ref} packets:{ref}")
    s.save("shield")


def modes():
    s = Scenario("Game modes and reach: creative attacker (reach 5, invulnerable victims), spectator attacker (no attack), survival reach 3 (+3 buffer on the packet check), creative/spectator/adventure victims, attacking at the edge of reach.", spacing=6)
    rows = []
    for attacker, victim, dz in [("survival", "survival", 1.5), ("creative", "survival", 1.5), ("spectator", "survival", 1.5), ("adventure", "survival", 1.5), ("survival", "creative", 1.5),
                                 ("survival", "spectator", 1.5), ("survival", "survival", 2.9), ("survival", "survival", 3.2), ("survival", "survival", 5.0), ("survival", "survival", 6.5), ("creative", "survival", 4.9),
                                 ("creative", "survival", 7.9), ("creative", "survival", 8.2), ("creative", "creative", 1.5)]:
        x, z = s.slot()
        a, v = f"A{len(rows)}", f"V{len(rows)}"
        s.add(f"spawn_player {a} {attacker} {x} -60 {z} 0 0", f"spawn_player {v} {victim} {x} -60 {z + dz} 180 0", f"give {a} mainhand minecraft:diamond_sword")
        s.seeded(a)
        s.seeded(v)
        rows.append((a, v))
    s.add("tick 40")
    for a, v in rows:
        s.add(f"attack {a} {v}", f"dump player:{a} player:{v} inventory:{a}")
    s.save("modes")


def entities():
    s = Scenario("Attacking non-living and decoration entities: armor stands (break/knock), item frames (skip attack interaction), cows with a hand; creative and survival attackers.", spacing=6)
    s.add("spawn_player A survival 0.5 -60 0.5 0 0", "spawn_player C creative 0.5 -60 4.5 0 0")
    s.seeded("A")
    s.seeded("C")
    s.add("give A mainhand minecraft:diamond_sword", "give C mainhand minecraft:diamond_sword")
    rows = []
    for kind, extra in [("armor_stand", ""), ("armor_stand", "{Marker:1b}"), ("armor_stand", "{Invisible:1b}"), ("item_frame", "{Facing:2b}"), ("glow_item_frame", "{Facing:2b}"), ("cow", "")]:
        for who in ["A", "C"]:
            x, z = s.slot()
            ref = f"E{len(rows)}"
            zz = z if who == "A" else z + 4
            if kind.endswith("item_frame"):
                s.add(f"block {int(x)} -60 {int(zz + 2)} minecraft:stone")
                s.add(f"spawn {ref} minecraft:{kind} {x} -59.5 {zz + 1} 0 0 {extra}")
            else:
                s.add(f"spawn {ref} minecraft:{kind} {x} -60 {zz + 1.5} 180 0 {extra if extra else NOAI}")
            s.seeded(ref)
            if kind == "cow":
                s.tough(ref)
            rows.append((x, zz, ref, who))
    s.add("tick 40")
    for x, z, ref, who in rows:
        s.add(f"snap {who} {x} -60 {z} 0 0", f"attack {who} {ref}", f"dump player:{who} entity:{ref}", "tick 3", f"dump entity:{ref}")
    s.save("entities")


def mob():
    s = Scenario("Mob.doHurtTarget (mob melee): ATTACK_DAMAGE attribute, held weapon enchantments, knockback attribute/enchant, armor of the target; targets players and husks.", spacing=5)
    s.add("level_seed 9", "time 18000")
    rows = []
    cfgs = [("", None), ("minecraft:iron_sword", None), ("minecraft:diamond_sword[enchantments={\"minecraft:sharpness\":5}]", None), ("minecraft:diamond_sword[enchantments={\"minecraft:knockback\":2,\"minecraft:fire_aspect\":1}]", None),
            ("minecraft:diamond_axe", None), ("minecraft:diamond_sword", ("attack_damage", 7.0)), ("", ("attack_knockback", 1.5))]
    for item, attr in cfgs:
        for tgt in ["player", "armored", "zombie", "shield"]:
            x, z = s.slot()
            m = f"M{len(rows)}"
            t = f"T{len(rows)}"
            s.add(f"spawn {m} minecraft:zombie {x} -60 {z} 0 0 {NOAI}")
            s.seeded(m)
            if item:
                s.add(f"give {m} mainhand {item}")
            if attr:
                s.add(f"attribute {m} minecraft:{attr[0]} {attr[1]}")
            if tgt == "zombie":
                s.add(f"spawn {t} minecraft:zombie {x} -60 {z + 1.5} 180 0 {NOAI}")
            else:
                s.add(f"spawn_player {t} survival {x} -60 {z + 1.5} 180 0")
            s.seeded(t)
            if tgt == "zombie":
                s.tough(t)
            if tgt == "armored":
                for slot, it in [("head", "minecraft:iron_helmet"), ("chest", "minecraft:diamond_chestplate"), ("legs", "minecraft:iron_leggings"), ("feet", "minecraft:iron_boots")]:
                    s.add(f"give {t} {slot} {it}")
            if tgt == "shield":
                s.add(f"give {t} offhand minecraft:shield", f"use {t} off")
            rows.append((m, t, tgt))
    s.add("tick 40")
    for m, t, tgt in rows:
        s.add(f"mob_attack {m} {t}", f"dump entity:{m} " + (f"player:{t}" if tgt != "zombie" else f"entity:{t}") + (f" inventory:{t}" if tgt != "zombie" else ""))
    s.save("mob")


def mace():
    s = Scenario("Mace smash attack: fall-based bonus damage (fall distance <= 1.5 none, 3, 8, 12 blocks), density/breach, mace_smash source, smash sounds, area knockback of bystanders, fall damage immunity afterwards, fall distance reset.", spacing=8, per_row=12)
    rows = []
    for fall, ench, grounded in [(0.5, "", False), (1.6, "", False), (3.0, "", False), (5.5, "", False), (9.0, "", False), (20.0, "", False), (5.5, "density\":3", False), (5.5, "breach\":3", False), (5.5, "", True), (9.0, "", True)]:
        x, z = s.slot()
        if grounded:
            # a player victim stands on the ground (NoAI mobs never get an on-ground flag): smash_ground sounds
            s.add(f"spawn_player M{len(rows)} survival {x} -60 {z + 1.5} 180 0")
            s.seeded(f"M{len(rows)}")
        else:
            s.mob(f"M{len(rows)}", "zombie", x, z)
        s.add(f"spawn M{len(rows)}n0 minecraft:zombie {x + 1.5} -60 {z + 1.5} 180 0 {NOAI}")
        s.seeded(f"M{len(rows)}n0")
        s.tough(f"M{len(rows)}n0")
        s.add(f"spawn M{len(rows)}n1 minecraft:cow {x - 2.0} -60 {z + 1.0} 180 0 {NOAI}")
        s.seeded(f"M{len(rows)}n1")
        s.tough(f"M{len(rows)}n1")
        rows.append((x, z, fall, ench, grounded))
    s.add("spawn_player A survival 0.5 -60 0.5 0 0")
    s.seeded("A")
    s.add("tick 2")
    for i, (x, z, fall, ench, grounded) in enumerate(rows):
        item = "minecraft:mace" + (f"[enchantments={{\"minecraft:{ench}}}]" if ench else "")
        s.add(f"snap A {x} -60 {z} 0 0", f"give A mainhand {item}", "tick 40", f"move_packet A {x} -59.5 {z} false", "client_tick_end A", f"set A fall_distance {fall}")
        s.add(f"attack A M{i}", f"dump player:A {'player' if grounded else 'entity'}:M{i} entity:M{i}n0 entity:M{i}n1 inventory:A", "tick 1", f"move_packet A {x} -60 {z} true", "client_tick_end A", "dump player:A")
    s.save("mace")


weapons()
cooldown()
crit_sprint_sweep()
enchant()
armor()
shield()
modes()
entities()
mob()
mace()
