#!/usr/bin/env python3
"""Generates corpus/70-predicates.txt (can_place_on / can_break and the whole predicate tree behind them).

Hand-written valid/invalid values for every codec form, then seeded structural mutations (type confusion, dropped /
unknown / null fields, number edge cases) of the valid seeds, each emitted as both `json` and `snbt` records.
Usage: python3 crates/mc-protocol/tests/fixtures/components/gen_corpus_predicates.py && crates/mc-protocol/tests/fixtures/components/gen.sh corpus/70-predicates.txt
Output: crates/mc-protocol/tests/fixtures/components/corpus/70-predicates.txt (reads ref/extracted/registries.json; run scripts/extract.sh first)
"""
import json
import os
import random
import struct
import sys

if sys.argv[1:] in (["-h"], ["--help"]):
    print(__doc__)
    sys.exit(0)

rnd = random.Random(70)
OUT = []
COMPONENTS = ["minecraft:can_place_on", "minecraft:can_break"]


def add(kind, comp, payload):
    assert "\t" not in payload and "\n" not in payload, payload
    OUT.append(f"{kind}\t{comp}\t{payload}")


def js(o):
    return json.dumps(o, separators=(",", ":"), ensure_ascii=False)


# ---- SNBT rendering of a JSON-ish python value -------------------------------------------------------------------
def snbt_key(k):
    return k if k.replace("_", "a").replace(".", "a").replace("-", "a").replace("+", "a").isalnum() else '"' + k.replace("\\", "\\\\").replace('"', '\\"') + '"'


def snbt_str(s):
    q = rnd.choice(['"', "'"])
    return q + s.replace("\\", "\\\\").replace(q, "\\" + q) + q


def snbt(o):
    if isinstance(o, bool):
        return rnd.choice(["true" if o else "false", "1b" if o else "0b", "1" if o else "0"])
    if isinstance(o, int):
        c = ["", "L", "s"]
        if -128 <= o <= 127:
            c.append("b")
        if abs(o) < 2 ** 24:
            c += ["f", "d", ".0d", ".0f"]
        s = rnd.choice(c)
        return str(o) + s
    if isinstance(o, float):
        return repr(o) + rnd.choice(["", "f", "d"])
    if isinstance(o, str):
        return snbt_str(o)
    if o is None:
        return '""'
    if isinstance(o, list):
        return "[" + ",".join(snbt(v) for v in o) + "]"
    if isinstance(o, dict):
        return "{" + ",".join(snbt_key(k) + ":" + snbt(v) for k, v in o.items()) + "}"
    raise TypeError(o)


def both(comp, obj):
    add("json", comp, js(obj))
    add("snbt", comp, snbt(obj))


# ---- seeds: valid values of every sub-codec -----------------------------------------------------------------------
INTS = [0, 5, -3, 2147483647, -2147483648, {}, {"min": 1}, {"max": 10}, {"min": 1, "max": 5}, {"min": 3, "max": 3}, {"min": -5}]
DOUBLES = [0, 1.5, -2.25, 1e10, {}, {"min": 0.5}, {"max": 3}, {"min": 1.5, "max": 1.5}, {"min": 1, "max": 2.5}]
BLOCKS = ["minecraft:stone", "minecraft:dirt", "minecraft:oak_log", "minecraft:air", "minecraft:grass_block", "stone"]
BLOCK_SETS = ["minecraft:stone", ["minecraft:stone"], ["minecraft:stone", "minecraft:dirt"], [], "#minecraft:logs", "#minecraft:mineable/pickaxe", "#logs"]
ENCH = ["minecraft:sharpness", "minecraft:mending", "minecraft:fortune"]
ENCH_SETS = ["minecraft:sharpness", ["minecraft:sharpness", "minecraft:mending"], "#minecraft:curse", []]
POTION_SETS = ["minecraft:healing", ["minecraft:healing", "minecraft:water"], []]
EFFECTS = ["minecraft:speed", "minecraft:poison", "minecraft:regeneration"]
MOB_EFFECT_PRED = [{}, {"amplifier": 1}, {"duration": {"min": 20}, "ambient": True}, {"visible": False, "amplifier": {"min": 0, "max": 3}}]
STATES = [
    {"facing": "north"}, {"age": {"min": "1", "max": "3"}}, {"age": 2}, {"waterlogged": "true"}, {"a": "1", "b": {"min": "x"}, "c": {"max": "z"}},
    {"age": {}}, {}, {"age": {"min": "1"}}, {"p0": "v", "p1": "v", "p2": "v", "p3": "v", "p4": "v", "p5": "v", "p6": "v", "p7": "v", "p8": "v", "p9": "v", "p10": "v", "p11": "v", "p12": "v", "p13": "v"},
    {"delay": "1", "facing": "up", "powered": "false", "lit": {"min": "false"}},
]
NBTS = ["{}", "{a:1}", "{Items:[{id:\"minecraft:stone\"}]}", {}, {"a": 1}, {"a": {"b": [1, 2, 3]}}, "{a:1b,b:[I;1,2]}", "{x:1.5f}"]
SHAPES = ["small_ball", "large_ball", "star", "creeper", "burst"]


def ints():
    return rnd.choice(INTS)


def pick_set(sets):
    return rnd.choice(sets)


def pred_values():
    """One valid value per data_component_predicate_type (and AnyValue) -> list of (type name, value)."""
    items = lambda: {"items": rnd.choice(["minecraft:stone", ["minecraft:stone", "minecraft:dirt"], "#minecraft:logs"]), "count": ints()}
    coll = lambda elem: rnd.choice([
        {}, {"contains": [elem()]}, {"contains": [elem(), elem()]}, {"contains": []}, {"size": ints()},
        {"count": [{"test": elem(), "count": ints()}]}, {"count": [], "size": {"min": 1}},
        {"contains": [elem()], "count": [{"test": elem(), "count": 1}], "size": 2},
    ])
    out = [
        ("minecraft:damage", rnd.choice([{}, {"durability": ints()}, {"damage": ints()}, {"durability": ints(), "damage": ints()}])),
        ("minecraft:enchantments", [rnd.choice([{}, {"enchantments": pick_set(ENCH_SETS)}, {"levels": ints()}, {"enchantments": pick_set(ENCH_SETS), "levels": ints()}]) for _ in range(rnd.randint(0, 2))]),
        ("minecraft:stored_enchantments", [{"enchantments": pick_set(ENCH_SETS), "levels": ints()}]),
        ("minecraft:potion_contents", rnd.choice([{}, {"potions": pick_set(POTION_SETS)}, {"effects": coll(lambda: {rnd.choice(EFFECTS): rnd.choice(MOB_EFFECT_PRED)})}, {"potions": "minecraft:water", "effects": {}}])),
        ("minecraft:custom_data", rnd.choice(NBTS)),
        ("minecraft:container", rnd.choice([{}, {"items": coll(items)}, {"items": coll(lambda: {"components": {"minecraft:damage": 0}})}, {"items": coll(lambda: {"predicates": {"minecraft:damage": {"durability": ints()}}})}])),
        ("minecraft:bundle_contents", rnd.choice([{}, {"items": coll(items)}])),
        ("minecraft:firework_explosion", rnd.choice([{}, {"shape": rnd.choice(SHAPES)}, {"has_twinkle": rnd.random() < .5, "has_trail": rnd.random() < .5}, {"shape": "star", "has_trail": True}])),
        ("minecraft:fireworks", rnd.choice([{}, {"flight_duration": ints()}, {"explosions": coll(lambda: {"shape": rnd.choice(SHAPES)})}])),
        ("minecraft:writable_book_content", rnd.choice([{}, {"pages": coll(lambda: rnd.choice(["hello", "", "§a"]))}])),
        ("minecraft:written_book_content", rnd.choice([{}, {"author": "a"}, {"title": "t", "generation": ints()}, {"resolved": True}, {"pages": coll(lambda: rnd.choice(["page", {"text": "x"}, {"translate": "k"}, ["a", "b"]]))}, {"author": "x", "title": "y", "generation": {"min": 1}, "resolved": False}])),
        ("minecraft:attribute_modifiers", rnd.choice([{}, {"modifiers": coll(lambda: rnd.choice([
            {}, {"attribute": "minecraft:armor"}, {"attribute": ["minecraft:armor", "minecraft:max_health"]}, {"id": "minecraft:foo"},
            {"amount": rnd.choice(DOUBLES)}, {"operation": rnd.choice(["add_value", "add_multiplied_base", "add_multiplied_total"])},
            {"slot": rnd.choice(["any", "mainhand", "offhand", "hand", "feet", "legs", "chest", "head", "armor", "body", "saddle"])},
            {"attribute": "minecraft:attack_damage", "id": "minecraft:base_attack_damage", "amount": {"min": 1}, "operation": "add_value", "slot": "mainhand"},
        ]))}])),
        ("minecraft:trim", rnd.choice([{}, {"material": "minecraft:iron"}, {"pattern": ["minecraft:coast", "minecraft:eye"]}, {"material": ["minecraft:gold", "minecraft:copper"], "pattern": "minecraft:sentry"}])),
        ("minecraft:jukebox_playable", rnd.choice([{}, {"song": "minecraft:cat"}, {"song": ["minecraft:cat", "minecraft:13"]}, {"song": []}])),
        ("minecraft:villager/variant", rnd.choice(["minecraft:plains", ["minecraft:plains", "minecraft:desert"], "minecraft:swamp", []])),
        ("minecraft:custom_name", {}),
        ("minecraft:rarity", {}),
        ("minecraft:unbreakable", {}),
        ("minecraft:max_damage", {"x": 1}),
    ]
    return out


def block_predicate():
    bp = {}
    if rnd.random() < .6:
        bp["blocks"] = pick_set(BLOCK_SETS if rnd.random() < .4 else BLOCK_SETS[:3])
    if rnd.random() < .3:
        bp["state"] = rnd.choice(STATES)
    if rnd.random() < .25:
        bp["nbt"] = rnd.choice(NBTS)
    if rnd.random() < .2:
        bp["components"] = rnd.choice([{}, {"minecraft:damage": 5}, {"minecraft:max_stack_size": 16}, {"minecraft:unbreakable": {}}] )
    if rnd.random() < .3:
        t, v = rnd.choice(pred_values())
        bp["predicates"] = {t: v}
    return bp


def component_value():
    r = rnd.random()
    if r < .45:
        return block_predicate()
    n = rnd.choice([0, 1, 1, 2, 3])
    return [block_predicate() for _ in range(n)]


# ---- structural mutations -----------------------------------------------------------------------------------------
JUNK = [None, True, False, 0, 1, -1, 2147483648, -2147483649, 1.5, 1e308, "", "x", "minecraft:stone", "#minecraft:logs", "{}", "{a:1}", [], [1], ["a"], {}, {"a": 1}, [{}], 99999999999, 0.0, -0.0, "1", "true"]


def paths(o, p=()):
    yield p
    if isinstance(o, dict):
        for k, v in o.items():
            yield from paths(v, p + (k,))
    elif isinstance(o, list):
        for i, v in enumerate(o):
            yield from paths(v, p + (i,))


def get(o, p):
    for k in p:
        o = o[k]
    return o


def setp(o, p, v):
    if not p:
        return v
    get(o, p[:-1])[p[-1]] = v
    return o


def delp(o, p):
    parent = get(o, p[:-1])
    if isinstance(parent, dict):
        del parent[p[-1]]
    else:
        parent.pop(p[-1])


def mutate(o):
    import copy
    o = copy.deepcopy(o)
    for _ in range(rnd.choice([1, 1, 1, 2])):
        ps = list(paths(o))
        p = rnd.choice(ps)
        kind = rnd.choice(["replace", "replace", "drop", "unknown", "wrap", "unwrap", "dup", "num"])
        v = get(o, p) if p else o
        if kind == "replace":
            o = setp(o, p, rnd.choice(JUNK))
        elif kind == "drop" and p:
            delp(o, p)
        elif kind == "unknown" and isinstance(v, dict):
            v[rnd.choice(["zzz", "Blocks", "state2", "minecraft:foo"])] = rnd.choice(JUNK)
        elif kind == "wrap":
            o = setp(o, p, [v])
        elif kind == "unwrap" and isinstance(v, list) and v:
            o = setp(o, p, v[0])
        elif kind == "dup" and isinstance(v, list) and v:
            v.append(copy.deepcopy(rnd.choice(v)))
        elif kind == "num" and isinstance(v, (int, float)) and not isinstance(v, bool):
            o = setp(o, p, rnd.choice([v + 1, -v, v * 1000, 2 ** 31, -2 ** 31 - 1, 0, 1e-5]))
    return o


# ---- hand-written records ------------------------------------------------------------------------------------------
BP_CASES = [
    {}, {"blocks": "minecraft:stone"}, {"blocks": ["minecraft:stone"]}, {"blocks": ["minecraft:stone", "minecraft:dirt"]}, {"blocks": "#minecraft:logs"},
    {"blocks": "#minecraft:does_not_exist"}, {"blocks": "minecraft:nope"}, {"blocks": ["minecraft:stone", "minecraft:nope"]}, {"blocks": []},
    {"blocks": ["#minecraft:logs"]}, {"blocks": 5}, {"blocks": None}, {"blocks": {}}, {"blocks": "STONE"}, {"blocks": "minecraft:"}, {"blocks": ":stone"},
    {"state": {"facing": "north"}}, {"state": {"age": {"min": "1", "max": "3"}}}, {"state": {"age": 2}}, {"state": {"age": 2.5}}, {"state": {"a": True}},
    {"state": {"a": None}}, {"state": {"a": []}}, {"state": []}, {"state": "x"}, {"state": {"a": {"min": 1}}}, {"state": {"a": {"min": "1", "max": 3}}},
    {"state": {"b": "1", "a": "2"}}, {"state": {"zz": "1", "aa": "2", "mm": "3"}}, {"state": {"a": {"min": "1", "x": 2}}},
    {"nbt": "{a:1}"}, {"nbt": {"a": 1}}, {"nbt": "{"}, {"nbt": "[1]"}, {"nbt": "1"}, {"nbt": 5}, {"nbt": []}, {"nbt": [1, 2]}, {"nbt": None}, {"nbt": ""},
    {"nbt": {"a": {"b": [1, 2, {"c": 1.5}]}}}, {"nbt": "{a:1b,b:\"x\",c:[B;1,2,3],d:[L;1L]}"}, {"nbt": {"a": 1.5e300}},
    {"components": {"minecraft:damage": 5}}, {"components": {"minecraft:damage": -1}}, {"components": {"minecraft:nope": 1}}, {"components": []}, {"components": {}},
    {"components": {"minecraft:max_stack_size": 0}}, {"components": None},
    {"components": {"minecraft:custom_data": {"a": 1}}}, {"components": {"minecraft:custom_name": "x"}},
    {"predicates": {}}, {"predicates": []}, {"predicates": None}, {"predicates": {"minecraft:nope": {}}}, {"predicates": {"nope": {}}},
    {"predicates": {"minecraft:damage": {}}}, {"predicates": {"minecraft:damage": {"durability": {"min": 1}}}}, {"predicates": {"minecraft:damage": {"durability": {"min": 5, "max": 1}}}},
    {"predicates": {"minecraft:damage": 5}}, {"predicates": {"minecraft:damage": []}}, {"predicates": {"damage": {"damage": 3}}},
    {"predicates": {"minecraft:custom_name": {}}}, {"predicates": {"minecraft:custom_name": {"a": 1}}}, {"predicates": {"minecraft:custom_name": 1}}, {"predicates": {"minecraft:custom_name": []}},
    {"predicates": {"minecraft:custom_data": "{a:1}"}}, {"predicates": {"minecraft:custom_data": {"a": 1}}}, {"predicates": {"minecraft:custom_data": 5}},
    {"blocks": "minecraft:stone", "state": {"a": "1"}, "nbt": "{b:2}", "components": {"minecraft:damage": 3}, "predicates": {"minecraft:damage": {"damage": 3}}},
    {"unknown": 1}, {"blocks": "minecraft:stone", "unknown": [1]},
]
for t, v in [(t, v) for _ in range(2) for t, v in pred_values()]:
    BP_CASES.append({"predicates": {t: v}})
for comp in COMPONENTS:
    for bp in BP_CASES:
        both(comp, bp)
        both(comp, [bp])
    for v in [[], [{}], [{}, {}], {}, "minecraft:stone", None, 5, True, "", [None], [[]], [[{}]], [1], ["minecraft:stone"], {"blocks": "minecraft:stone"}, [{"blocks": "minecraft:stone"}, {"blocks": "minecraft:dirt"}]]:
        both(comp, v)

# bounds edge cases, through a damage predicate
for v in [5, 0, -1, 1.5, 1.0, 2147483647, 2147483648, -2147483648, -2147483649, 1e10, 1e-5, "5", "", True, None, [], [5], {}, {"min": 1}, {"max": 1}, {"min": 1, "max": 1}, {"min": 2, "max": 1},
          {"min": 1.5}, {"min": "1"}, {"min": None}, {"min": True}, {"min": [1]}, {"min": 1, "max": None}, {"min": 1, "x": 2}, {"exactly": 2}, {"min": 2147483648}, {"min": -0.0},
          {"min": 1.0e0, "max": 2.0e0}, {"min": 3, "max": 3, "extra": 1}]:
    for comp in COMPONENTS[:1]:
        both(comp, {"predicates": {"minecraft:damage": {"durability": v}}})
        both(comp, {"predicates": {"minecraft:attribute_modifiers": {"modifiers": {"contains": [{"amount": v}]}}}})
for v in [5, 1.5, "1.5", -0.0, 0.0, {"min": 1.5, "max": 1.5}, {"min": 0.0, "max": -0.0}, {"min": -0.0, "max": 0.0}, {"min": -0.0, "max": -0.0}, {"min": 2.5, "max": 1.5}, 1e308, 1e-320, 4.9e-324, 1e400]:
    both(COMPONENTS[1], {"predicates": {"minecraft:attribute_modifiers": {"modifiers": {"contains": [{"amount": v}]}}}})
# SNBT-only number forms (NaN / infinity / typed numbers)
for n in ["NaNd", "Infinityd", "-Infinityd", "NaNf", "Infinityf", "1b", "1s", "1L", "1.5f", "-0.0d", "0.0d", "1e5", "1e5d", "1d", "5b", "true", "false", "0xFF", "0b11", "1_000", "+5", "-5", ".5", "5.", "1.5e3f", "128b", "'5'", '"5"']:
    for key in ["min", "max"]:
        add("snbt", COMPONENTS[1], "{predicates:{\"minecraft:attribute_modifiers\":{modifiers:{contains:[{amount:{%s:%s}}]}}}}" % (key, n))
        add("snbt", COMPONENTS[0], "{predicates:{\"minecraft:damage\":{damage:{%s:%s}}}}" % (key, n))
    add("snbt", COMPONENTS[0], "{predicates:{\"minecraft:damage\":{damage:%s}}}" % n)
    add("snbt", COMPONENTS[1], "{predicates:{\"minecraft:attribute_modifiers\":{modifiers:{contains:[{amount:%s}]}}}}" % n)
add("snbt", COMPONENTS[0], "{predicates:{\"minecraft:attribute_modifiers\":{modifiers:{contains:[{amount:{min:NaNd,max:NaNd}}]}}}}")
add("snbt", COMPONENTS[0], "{predicates:{\"minecraft:attribute_modifiers\":{modifiers:{contains:[{amount:{min:NaNd,max:1.0d}}]}}}}")
add("snbt", COMPONENTS[0], "{predicates:{\"minecraft:attribute_modifiers\":{modifiers:{contains:[{amount:{min:1.0d,max:NaNd}}]}}}}")

# top-level SNBT shapes
for s in ["[]", "[{}]", "{}", "{blocks:\"minecraft:stone\"}", "{blocks:[\"minecraft:stone\"]}", "{blocks:['minecraft:stone']}", "[{blocks:\"minecraft:stone\"},{}]",
          "{blocks:\"#minecraft:logs\"}", "{blocks:\"#minecraft:nope\"}", "{blocks:1}", "{blocks:[1]}", "{blocks:[I;1]}", "{state:{age:{min:\"1\"}}}", "{state:{age:1}}",
          "{state:{age:1b}}", "{state:{age:\"1\"}}", "{nbt:\"{a:1}\"}", "{nbt:{a:1}}", "{nbt:{a:1b}}", "{nbt:[]}", "{nbt:\"[]\"}", "{nbt:\"\"}", "{nbt:\"{a:\\\"b\\\"}\"}",
          "{components:{\"minecraft:damage\":5}}", "{components:{\"minecraft:damage\":5b}}", "{components:{\"minecraft:damage\":5.7f}}", "{components:{\"minecraft:damage\":\"5\"}}",
          "{predicates:{\"minecraft:damage\":{}}}", "{predicates:{\"minecraft:damage\":{damage:1b}}}", "{predicates:{\"minecraft:damage\":{damage:{min:1L}}}}",
          "{predicates:{\"minecraft:villager/variant\":\"minecraft:plains\"}}", "{predicates:{\"minecraft:villager/variant\":[\"minecraft:plains\"]}}",
          "{predicates:{\"minecraft:villager/variant\":\"#minecraft:nope\"}}", "{predicates:{\"minecraft:villager/variant\":[]}}",
          "{predicates:{\"minecraft:fireworks\":{flight_duration:1}}}", "{predicates:{\"minecraft:firework_explosion\":{has_trail:1b,shape:\"star\"}}}",
          "{predicates:{\"minecraft:firework_explosion\":{has_trail:true}}}", "{predicates:{\"minecraft:firework_explosion\":{has_trail:2b}}}", "{predicates:{\"minecraft:firework_explosion\":{has_trail:\"true\"}}}",
          "{predicates:{\"minecraft:written_book_content\":{pages:{contains:['{\"text\":\"x\"}']}}}}", "{predicates:{\"minecraft:written_book_content\":{pages:{contains:[{text:\"x\"}]}}}}",
          "{predicates:{\"minecraft:written_book_content\":{pages:{contains:[\"x\"]}}}}", "{predicates:{\"minecraft:written_book_content\":{resolved:1b}}}",
          "{predicates:{\"minecraft:custom_data\":\"{a:1}\"}}", "{predicates:{\"minecraft:custom_data\":{a:1}}}", "{predicates:{\"minecraft:custom_data\":\"{\"}}",
          "{predicates:{\"minecraft:custom_name\":{}}}", "{predicates:{\"minecraft:custom_name\":[]}}", "{predicates:{\"minecraft:custom_name\":{a:1}}}"]:
    for comp in COMPONENTS:
        add("snbt", comp, s)
        add("snbt", comp, "[" + s + "]")


# ---- wire records ---------------------------------------------------------------------------------------------------
def varint(n):
    n &= 0xFFFFFFFF
    out = b""
    while True:
        if n & ~0x7F == 0:
            return out + bytes([n])
        out += bytes([(n & 0x7F) | 0x80])
        n >>= 7


def utf(s):
    b = s.encode("utf-8")
    return varint(len(b)) + b


BOOL = lambda v: b"\x01" if v else b"\x00"
sys.path.insert(0, os.path.dirname(__file__))


def reg_ids():
    import json as _j
    d = _j.load(open(os.path.join(os.path.dirname(__file__), "../../../../../ref/extracted/registries.json")))
    return {k: {n: i for i, n in enumerate(v["entries"])} for k, v in d.items()}


REG = reg_ids()


def block_wire(blocks=None, props=None, nbt=None, exact=b"\x00", partial=b"\x00"):
    out = b""
    if blocks is None:
        out += b"\x00"
    elif isinstance(blocks, str):  # tag
        out += b"\x01" + varint(0) + utf(blocks)
    else:
        out += b"\x01" + varint(len(blocks) + 1) + b"".join(varint(REG["minecraft:block"][b]) for b in blocks)
    if props is None:
        out += b"\x00"
    else:
        out += b"\x01" + varint(len(props)) + b"".join(utf(n) + (b"\x01" + utf(v) if isinstance(v, str) else b"\x00" + BOOL(v[0] is not None) + (utf(v[0]) if v[0] is not None else b"") + BOOL(v[1] is not None) + (utf(v[1]) if v[1] is not None else b"")) for n, v in props)
    out += b"\x00" if nbt is None else b"\x01" + nbt
    return out + exact + partial


def tag_compound_empty():
    return b"\x0a\x00"


def tag_compound_str(k, v):
    kb, vb = k.encode(), v.encode()
    return b"\x0a" + b"\x08" + struct.pack(">H", len(kb)) + kb + struct.pack(">H", len(vb)) + vb + b"\x00"


def tag_compound_int(k, v):
    kb = k.encode()
    return b"\x0a" + b"\x03" + struct.pack(">H", len(kb)) + kb + struct.pack(">i", v) + b"\x00"


def partial_single(pt_name, nbt_payload):
    return varint(1) + b"\x01" + varint(REG["minecraft:data_component_predicate_type"][pt_name]) + nbt_payload


def partial_any(comp, nbt_payload):
    return varint(1) + b"\x00" + varint(REG["minecraft:data_component_type"][comp]) + nbt_payload


def hexs(b):
    return b.hex()


W = []
W.append(varint(0))
W.append(varint(1) + block_wire())
W.append(varint(2) + block_wire() + block_wire())
W.append(varint(1) + block_wire(["minecraft:stone"]))
W.append(varint(1) + block_wire(["minecraft:stone", "minecraft:dirt", "minecraft:oak_log"]))
W.append(varint(1) + block_wire("minecraft:logs"))
W.append(varint(1) + block_wire([]))
W.append(varint(1) + b"\x01" + varint(1) + varint(99999) + b"\x00\x00\x00\x00")
W.append(varint(1) + b"\x01" + varint(-5 + 2 ** 32) + b"\x00\x00\x00\x00")
W.append(varint(1) + block_wire(props=[("age", "1"), ("facing", "north")]))
W.append(varint(1) + block_wire(props=[("age", (None, None)), ("a", ("1", None)), ("b", (None, "2")), ("c", ("1", "2"))]))
W.append(varint(1) + block_wire(props=[("age", "1"), ("age", "2")]))
W.append(varint(1) + block_wire(props=[]))
W.append(varint(1) + block_wire(nbt=tag_compound_empty()))
W.append(varint(1) + block_wire(nbt=tag_compound_str("a", "b")))
W.append(varint(1) + block_wire(nbt=b"\x08\x00\x01x"))  # not a compound
W.append(varint(1) + block_wire(nbt=b"\x00"))
W.append(varint(1) + block_wire(exact=varint(1) + varint(REG["minecraft:data_component_type"]["minecraft:damage"]) + varint(7)))
W.append(varint(1) + block_wire(exact=varint(1) + varint(REG["minecraft:data_component_type"]["minecraft:unbreakable"])))
# two exact components are not golden-testable: vanilla encodes them through a HashMap keyed by identity hashes
W.append(varint(1) + block_wire(exact=varint(1) + varint(9999)))
W.append(varint(1) + block_wire(partial=partial_single("minecraft:damage", tag_compound_empty())))
W.append(varint(1) + block_wire(partial=partial_single("minecraft:damage", tag_compound_int("damage", 5))))
W.append(varint(1) + block_wire(partial=partial_single("minecraft:villager/variant", b"\x08\x00\x10minecraft:plains")))
W.append(varint(1) + block_wire(partial=partial_single("minecraft:custom_data", tag_compound_int("a", 1))))
W.append(varint(1) + block_wire(partial=partial_single("minecraft:custom_data", b"\x08\x00\x05{a:1}")))
W.append(varint(1) + block_wire(partial=partial_single("minecraft:enchantments", b"\x09\x0a\x00\x00\x00\x01\x00")))
W.append(varint(1) + block_wire(partial=partial_single("minecraft:enchantments", b"\x09\x00\x00\x00\x00\x00")))
W.append(varint(1) + block_wire(partial=partial_any("minecraft:custom_name", tag_compound_empty())))
W.append(varint(1) + block_wire(partial=partial_any("minecraft:custom_name", tag_compound_int("a", 1))))
W.append(varint(1) + block_wire(partial=partial_any("minecraft:damage", tag_compound_empty())))
W.append(varint(1) + block_wire(partial=partial_any("minecraft:custom_name", b"\x01\x00")))
W.append(varint(1) + block_wire(partial=varint(2) + b"\x01" + varint(0) + tag_compound_empty() + b"\x01" + varint(0) + tag_compound_empty()))
W.append(varint(1) + block_wire(partial=varint(2) + b"\x00" + varint(REG["minecraft:data_component_type"]["minecraft:custom_name"]) + tag_compound_empty() + b"\x00" + varint(REG["minecraft:data_component_type"]["minecraft:custom_name"]) + tag_compound_empty()))
W.append(varint(1) + block_wire(partial=varint(1) + b"\x01" + varint(15) + tag_compound_empty()))
W.append(varint(1) + block_wire(partial=varint(1) + b"\x00" + varint(9999) + tag_compound_empty()))
W.append(varint(1) + block_wire(partial=varint(65) + b""))
W.append(varint(1) + block_wire(partial=varint(64) + b""))
W.append(varint(1) + block_wire(partial=varint(-1 + 2 ** 32)))
W.append(varint(1) + block_wire(partial=b"\x01\x01\x00" + b""))
W.append(varint(2 ** 31 - 1) + b"")
W.append(varint(-1 + 2 ** 32))
for w in W:
    for comp in COMPONENTS:
        add("wire", comp, hexs(w))
    add("stack-wire", "-", hexs(varint(1) + varint(REG["minecraft:item"]["minecraft:stone"]) + varint(1) + varint(0) + varint(REG["minecraft:data_component_type"]["minecraft:can_break"]) + w))
    add("stack-wire", "-", hexs(varint(1) + varint(REG["minecraft:item"]["minecraft:stone"]) + varint(1) + varint(0) + varint(REG["minecraft:data_component_type"]["minecraft:can_place_on"]) + w))
    add("stack-wire-untrusted", "-", hexs(varint(1) + varint(REG["minecraft:item"]["minecraft:stone"]) + varint(1) + varint(0) + varint(REG["minecraft:data_component_type"]["minecraft:can_break"]) + w))

# ---- stacks ---------------------------------------------------------------------------------------------------------
for i in range(60):
    c = rnd.choice(COMPONENTS)
    v = component_value()
    add("stack-snbt", "-", '{id:"minecraft:stone",count:%d,components:{"%s":%s}}' % (rnd.randint(1, 64), c, snbt(v)))
for s in ['{id:"minecraft:stone",count:1,components:{"minecraft:can_break":[{blocks:"minecraft:stone"}],"minecraft:can_place_on":{blocks:["minecraft:dirt"]}}}',
          '{id:"minecraft:stone",count:1,components:{"!minecraft:can_break":{}}}',
          '{id:"minecraft:stone",count:1,components:{"minecraft:can_break":[]}}',
          '{id:"minecraft:diamond_pickaxe",count:1,components:{"minecraft:can_break":{blocks:"#minecraft:logs"}}}']:
    add("stack-snbt", "-", s)
    add("ostack-snbt", "-", s)
for p in ['{"minecraft:can_break":{blocks:"minecraft:stone"}}', '{"minecraft:can_break":[]}', '{"!minecraft:can_break":{}}', '{"minecraft:can_place_on":[{},{blocks:"#minecraft:logs"}]}']:
    add("patch-snbt", "-", p)
add("patch-json", "-", '{"minecraft:can_break":{"blocks":"minecraft:stone","state":{"a":"1"}}}')
add("patch-json", "-", '{"minecraft:can_break":[{"blocks":"minecraft:stone"},{"nbt":"{a:1}"}]}')

# ---- random seeded -------------------------------------------------------------------------------------------------
N_RANDOM = int(os.environ.get("N_RANDOM", "260"))
for i in range(N_RANDOM):
    c = rnd.choice(COMPONENTS)
    v = component_value()
    if i % 3 == 0:
        v = mutate(v)
    both(c, v)
for i in range(N_RANDOM // 2):
    c = rnd.choice(COMPONENTS)
    t, v = rnd.choice(pred_values())
    bp = {"predicates": {t: v}}
    if i % 2:
        bp = mutate(bp)
    both(c, bp)

path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "corpus", "70-predicates.txt")
with open(path + ".tmp", "w") as f:
    f.write("\n".join(OUT) + "\n")
os.replace(path + ".tmp", path)
print(len(OUT), "lines ->", path)
