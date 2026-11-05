#!/usr/bin/env python3
"""Multi-entry corpora for the maps vanilla iterates in hash order (see STATUS.md "Hash-order findings").

  90-hash-order-identity.txt  identity-hashed maps (ItemEnchantments, decoded predicate maps): JVM-run-random order in
                              vanilla's json/snbt/wire output, compared semantically (components_golden.rs `relaxed_fields`)
  91-hash-order-exactpred.txt DataComponentExactPredicate / DataComponentMatchers built from JSON/SNBT: wire order = input
                              order (strict), json output order = identity HashMap (random, compared as a set)
  92-hash-order-deterministic.txt insertion-ordered or stable-hash maps (patches, stacks, block_state, tooltip_display, ...):
                              compared byte-exact; only `hashed=` (client-side HashedPatchMap, IdentityHashMap) is relaxed
  93-hash-order-click.txt     ServerboundContainerClickPacket with multi-component hashed stacks (wire compared by decoding)
Regenerate: python3 gen_corpus_hash_order.py && ./gen.sh corpus/90-hash-order-identity.txt corpus/91-hash-order-exactpred.txt corpus/92-hash-order-deterministic.txt corpus/93-hash-order-click.txt
"""
import json
import os
import random
from corpus_lib import to_snbt

HERE = os.path.dirname(os.path.abspath(__file__))
rng = random.Random(260305)

ENCH = [l.rstrip("\n").split("\t")[1].split(",") for l in open(os.path.join(HERE, "registries.golden")) if l.startswith("minecraft:enchantment\t")][0]
LEVELS = [1, 2, 3, 4, 5, 10, 127, 255]


def varint(v):
    out = b""
    while True:
        if v & ~0x7F == 0:
            return out + bytes([v])
        out += bytes([(v & 0x7F) | 0x80])
        v >>= 7


def utf(s):
    b = s.encode()
    return varint(len(b)) + b


def jd(v):
    return json.dumps(v, separators=(",", ":"), ensure_ascii=False)


def sn(v):
    return to_snbt(v, rng, "plain")


def shuffled(xs):
    xs = list(xs)
    rng.shuffle(xs)
    return xs


class Corpus:
    def __init__(self):
        self.lines = []

    def add(self, kind, name, payload):
        assert "\t" not in payload and "\n" not in payload, repr(payload)
        self.lines.append(f"{kind}\t{name}\t{payload}")

    def both(self, name, value):
        self.add("json", name, jd(value))
        self.add("snbt", name, sn(value))

    def write(self, fname):
        with open(os.path.join(HERE, "corpus", fname), "w") as f:
            f.write("\n".join(self.lines) + "\n")


def enchant_map(n, levels=LEVELS):
    return {k: rng.choice(levels) for k in rng.sample(ENCH, n)}


# ---- 90: identity-hashed ---------------------------------------------------------------------------------------------
c90 = Corpus()
for comp in ("minecraft:enchantments", "minecraft:stored_enchantments"):
    for n in list(range(2, 13)) + [len(ENCH)]:
        for _ in range(3):
            m = enchant_map(n)
            c90.both(comp, m)
            # explicit wire: count, (id, level) pairs in a random order, plus a duplicate key and a bad level
            pairs = shuffled((ENCH.index(k), v) for k, v in m.items())
            c90.add("wire", comp, (varint(len(pairs)) + b"".join(varint(i) + varint(l) for i, l in pairs)).hex())
    # invalid members inside a multi-entry map: the whole map must fail the same way
    for bad in (0, 256, -1, 1000):
        m = enchant_map(4)
        m[rng.choice(list(m))] = bad
        c90.both(comp, m)
    c90.both(comp, {**enchant_map(3), "minecraft:nope": 1})
    c90.both(comp, {**enchant_map(3), "minecraft:sharpness": "5"})
    dup = [(ENCH.index("minecraft:sharpness"), 1), (ENCH.index("minecraft:unbreaking"), 3), (ENCH.index("minecraft:sharpness"), 5)]
    c90.add("wire", comp, (varint(3) + b"".join(varint(i) + varint(l) for i, l in dup)).hex())
    c90.add("wire", comp, (varint(2) + b"".join(varint(i) + varint(0) for i, l in dup[:2])).hex())

BASE = {"minecraft:custom_name": "N", "minecraft:damage": 3, "minecraft:repair_cost": 2, "minecraft:unbreakable": {},
        "minecraft:max_damage": 100, "minecraft:rarity": "epic", "minecraft:lore": ["a", "b"], "minecraft:glider": {},
        "minecraft:enchantment_glint_override": True, "minecraft:item_name": "n", "minecraft:custom_data": {"a": 1, "b": [1, 2]},
        "minecraft:custom_model_data": {"floats": [1.0]}}
ITEMS = ["minecraft:diamond_sword", "minecraft:enchanted_book", "minecraft:stone", "minecraft:iron_pickaxe"]
for i in range(60):
    comps = {k: BASE[k] for k in rng.sample(list(BASE), rng.randint(0, 9))}
    comps["minecraft:enchantments"] = enchant_map(rng.randint(2, 8))
    if rng.random() < 0.4:
        comps["minecraft:stored_enchantments"] = enchant_map(rng.randint(2, 6))
    if rng.random() < 0.3:
        comps["!minecraft:damage"] = {}
    comps = dict(shuffled(comps.items()))
    stack = {"id": rng.choice(ITEMS), "count": rng.randint(1, 64), "components": comps}
    c90.add("mstack-json", "-", jd(stack))
    c90.add("mstack-snbt", "-", sn(stack))
    c90.add("mpatch-json", "-", jd(comps))
    c90.add("mpatch-snbt", "-", sn(comps))

# predicates decoded from the wire into a HashMap<Type, ...> (identity order) and re-encoded
PARTIAL = {
    "minecraft:damage": {"damage": {"min": 1}}, "minecraft:custom_data": {"a": 1}, "minecraft:firework_explosion": {"shape": "star"},
    "minecraft:trim": {"material": "minecraft:iron"}, "minecraft:potion_contents": {"potions": "minecraft:water"},
    "minecraft:jukebox_playable": {"song": "minecraft:cat"}, "minecraft:fireworks": {"flight_duration": {"min": 1}},
    "minecraft:written_book_content": {"author": "a"}, "minecraft:enchantments": [{"enchantments": "minecraft:sharpness", "levels": {"min": 1}}],
    "minecraft:attribute_modifiers": {"modifiers": {"contains": [{"amount": {"min": 1}}]}},
}
EXACT = {"minecraft:damage": 5, "minecraft:max_stack_size": 16, "minecraft:unbreakable": {}, "minecraft:custom_name": "x",
         "minecraft:custom_data": {"a": 1}, "minecraft:repair_cost": 2, "minecraft:rarity": "epic", "minecraft:glider": {},
         "minecraft:max_damage": 100, "minecraft:item_name": "n", "minecraft:enchantment_glint_override": False}


def pick(d, n):
    return {k: d[k] for k in rng.sample(list(d), n)}


# DataComponentMatchers.partial: JSON/SNBT decode into an ordered map (wire/json output keeps that order) but the vanilla
# stream codec decodes into a `Collectors.toMap` HashMap keyed by `DataComponentPredicate.Type` (identity hash); the
# ComponentGolden replays the vanilla encoding through that decoder (`ojson`/`osnbt`: unmutated wire only, see ComponentGolden).
for i in range(60):
    for comp in ("minecraft:can_break", "minecraft:can_place_on"):
        bp = {"blocks": "minecraft:stone", "predicates": pick(PARTIAL, rng.randint(2, len(PARTIAL)))}
        c90.add("ojson", comp, jd(bp))
        c90.add("osnbt", comp, sn(bp))
    bp = {"predicates": pick(PARTIAL, rng.randint(2, 6)), "components": pick(EXACT, rng.randint(0, 3))}
    c90.add("ojson", "minecraft:can_break", jd(bp))
    c90.add("osnbt", "minecraft:can_break", sn(bp))

# ---- 91: exact predicates --------------------------------------------------------------------------------------------
c91 = Corpus()
for comp in ("minecraft:can_break", "minecraft:can_place_on"):
    for n in range(2, 9):
        for _ in range(3):
            bp = {"blocks": "minecraft:stone", "components": pick(EXACT, n)}
            c91.both(comp, bp)
            c91.both(comp, [bp, {"components": pick(EXACT, 3)}])
    c91.both(comp, {"components": {**pick(EXACT, 3), "minecraft:nope": 1}})
    c91.both(comp, {"components": {**pick(EXACT, 3), "minecraft:damage": -1}})

# ---- 92: deterministic -----------------------------------------------------------------------------------------------
c92 = Corpus()
PROPS = ["facing", "lit", "waterlogged", "half", "shape", "axis", "age", "powered", "open", "hinge", "type", "level", "honey_level",
         "bites", "candles", "delay", "distance", "eggs", "face", "mode", "note", "rotation", "stage", "up", "down"]
for n in list(range(1, 25)) + [30, 40]:
    for _ in range(3):
        keys = rng.sample(PROPS, min(n, len(PROPS))) if n <= len(PROPS) else [f"p{i}" for i in range(n)]
        m = {k: rng.choice(["true", "false", "north", "1", "x", "bottom"]) for k in keys}
        c92.both("minecraft:block_state", m)
        c92.add("wire", "minecraft:block_state", (varint(len(m)) + b"".join(utf(k) + utf(v) for k, v in shuffled(m.items()))).hex())
c92.add("wire", "minecraft:block_state", (varint(3) + utf("a") + utf("1") + utf("b") + utf("2") + utf("a") + utf("3")).hex())
COMPS = ["minecraft:enchantments", "minecraft:damage", "minecraft:custom_name", "minecraft:lore", "minecraft:tool", "minecraft:food",
         "minecraft:unbreakable", "minecraft:rarity", "minecraft:max_damage", "minecraft:glider", "minecraft:item_name", "minecraft:repair_cost"]
for n in list(range(1, 10)):
    for _ in range(3):
        hidden = shuffled(COMPS)[:n]
        td = {"hide_tooltip": rng.random() < 0.5, "hidden_components": hidden}
        c92.both("minecraft:tooltip_display", td)
        td["hidden_components"] = hidden + rng.sample(hidden, min(2, len(hidden)))  # duplicates collapse (LinkedOpenHashSet)
        c92.both("minecraft:tooltip_display", td)
c92.both("minecraft:tooltip_display", {"hidden_components": ["minecraft:nope", "minecraft:damage"]})
c92.both("minecraft:tooltip_display", {"hidden_components": ["minecraft:damage", "minecraft:transient_not_persistent"]})
DECO = {"type": "minecraft:player", "x": 1.0, "z": 2.0, "rotation": 90.0}
for n in range(2, 10):
    for _ in range(3):
        c92.both("minecraft:map_decorations", {f"d{rng.randint(0, 99)}": {**DECO, "x": rng.random() * 10} for _ in range(n)})
c92.both("minecraft:map_decorations", {"a": DECO, "b": {**DECO, "type": "minecraft:nope"}})
BLOCKS = {"minecraft:oak_stairs": "facing", "minecraft:oak_slab": "type", "minecraft:lever": "face", "minecraft:oak_door": "hinge",
          "minecraft:repeater": "delay", "minecraft:note_block": "note", "minecraft:oak_log": "axis", "minecraft:redstone_wire": "power"}
for n in range(2, 9):
    for _ in range(2):
        c92.both("minecraft:debug_stick_state", pick(BLOCKS, n))
c92.both("minecraft:debug_stick_state", {"minecraft:oak_stairs": "nope", "minecraft:lever": "face"})
# patches / stacks: ArrayMap insertion order = input order, removals ("!") after additions on the wire
POOL = {**BASE, "minecraft:block_state": {"facing": "north", "lit": "true"}, "minecraft:tooltip_display": {"hide_tooltip": True},
        "minecraft:ominous_bottle_amplifier": 2,
        "minecraft:intangible_projectile": {}, "minecraft:potion_contents": {"potion": "minecraft:water"}}
for i in range(120):
    comps = {k: POOL[k] for k in rng.sample(list(POOL), rng.randint(2, 14))}
    for k in rng.sample(list(POOL), rng.randint(0, 3)):
        comps.setdefault("!" + k, {})
    comps = dict(shuffled(comps.items()))
    stack = {"id": rng.choice(ITEMS), "count": rng.randint(1, 64), "components": comps}
    c92.add("mstack-json", "-", jd(stack))
    c92.add("mstack-snbt", "-", sn(stack))
    c92.add("mpatch-json", "-", jd(comps))
    c92.add("mpatch-snbt", "-", sn(comps))

# ---- 93: serverbound container click (changed slots Int2ObjectOpenHashMap, hashed stacks) ---------------------------------
c93 = Corpus()


def rand_stack(multi=True):
    if rng.random() < 0.08:
        return {}
    comps = {k: POOL[k] for k in rng.sample(list(POOL), rng.randint(0, 12 if multi else 1))}
    if rng.random() < 0.6:
        comps["minecraft:enchantments"] = enchant_map(rng.randint(1, 6))
    for k in rng.sample(list(POOL), rng.randint(0, 2)):
        comps.setdefault("!" + k, {})
    return {"id": rng.choice(ITEMS), "count": rng.randint(1, 64), "components": dict(shuffled(comps.items()))}


for i in range(150):
    n = rng.choice([0, 1, 2, 3, 5, 8, 13, 20, 40])
    slots = rng.sample(list(range(-3, 50)) + [32767, -32768, 255, 256, 1000], n)
    c93.add("click", "-", jd({"c": rng.choice([0, 1, 7, 127]), "s": rng.randint(0, 100000), "slot": rng.choice([-999, -1, 0, 5, 36, 44]),
                              "button": rng.choice([0, 1, 40, -1]), "input": rng.choice(["PICKUP", "QUICK_MOVE", "SWAP", "CLONE", "THROW", "QUICK_CRAFT", "PICKUP_ALL"]),
                              "changed": [[sl, rand_stack()] for sl in slots], "carried": rand_stack()}))

c90.write("90-hash-order-identity.txt")
c91.write("91-hash-order-exactpred.txt")
c92.write("92-hash-order-deterministic.txt")
c93.write("93-hash-order-click.txt")
print(len(c90.lines), len(c91.lines), len(c92.lines))
