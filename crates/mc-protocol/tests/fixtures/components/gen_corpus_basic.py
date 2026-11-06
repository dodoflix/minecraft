#!/usr/bin/env python3
"""Generates corpus/10-basic-a.txt and corpus/10-basic-b.txt (seeded, deterministic).

Hand-written valid/invalid values per component plus seeded mutations (type confusion, dropped/added fields, number
edge cases) in both strict JSON and SNBT with typed numbers. Multi-entry enchantment maps are only emitted when
vanilla must reject them: Object2IntOpenHashMap iterates in identity-hash order, so valid ones have no stable golden.

Usage: python3 crates/mc-protocol/tests/fixtures/components/gen_corpus_basic.py && crates/mc-protocol/tests/fixtures/components/gen.sh corpus/10-basic-a.txt corpus/10-basic-b.txt
Output: crates/mc-protocol/tests/fixtures/components/corpus/10-basic-a.txt, crates/mc-protocol/tests/fixtures/components/corpus/10-basic-b.txt
"""
import json
import math
import os
import random
import sys

if sys.argv[1:] in (["-h"], ["--help"]):
    print(__doc__)
    sys.exit(0)

HERE = os.path.dirname(os.path.abspath(__file__))
R = random.Random(2610)


class Raw:
    """Text emitted verbatim in both syntaxes."""

    def __init__(self, text):
        self.text = text


class Num:
    """A number whose SNBT suffix is fixed (JSON drops it)."""

    def __init__(self, v, suffix):
        self.v, self.suffix = v, suffix


def js(o):
    if isinstance(o, Raw):
        return o.text
    if isinstance(o, Num):
        return js(o.v)
    if isinstance(o, dict):
        return "{" + ",".join(json.dumps(k) + ":" + js(v) for k, v in o.items()) + "}"
    if isinstance(o, list):
        return "[" + ",".join(js(v) for v in o) + "]"
    if isinstance(o, float):
        if math.isnan(o) or math.isinf(o):
            return "1e999" if o > 0 else "-1e999"
        return repr(o)
    return json.dumps(o, ensure_ascii=R.random() < 0.5)


def sstr(s):
    q = R.choice(['"', "'"])
    out = s.replace("\\", "\\\\").replace(q, "\\" + q)
    return q + out + q


def snbt(o):
    if isinstance(o, Raw):
        return o.text
    if isinstance(o, Num):
        return js(o.v) + o.suffix
    if isinstance(o, dict):
        parts = []
        for k, v in o.items():
            key = k if k.replace("_", "").replace("-", "").replace(".", "").replace("+", "").isalnum() and R.random() < 0.5 else sstr(k)
            parts.append(key + ":" + snbt(v))
        return "{" + ",".join(parts) + "}"
    if isinstance(o, list):
        return "[" + ",".join(snbt(v) for v in o) + "]"
    if o is True:
        return R.choice(["true", "1b"])
    if o is False:
        return R.choice(["false", "0b"])
    if o is None:
        return "{}"
    if isinstance(o, int):
        if -128 <= o <= 127:
            return str(o) + R.choice(["", "", "b", "s", "L"])
        if -32768 <= o <= 32767:
            return str(o) + R.choice(["", "", "s", "L"])
        if -(2**31) <= o < 2**31:
            return str(o) + R.choice(["", "", "L"])
        return str(o) + "L"
    if isinstance(o, float):
        if math.isnan(o):
            return "NaNf"
        if math.isinf(o):
            return "Infinityf" if o > 0 else "-Infinityf"
        return repr(o) + R.choice(["f", "d", ""])
    if isinstance(o, str):
        return sstr(o)
    raise TypeError(o)


JUNK = [None, True, False, 0, 1, -1, 5, 255, 256, 2**31, 2**31 - 1, -(2**31), 2**63, 1.5, -0.0, 1e10, "", "x", "minecraft:stone",
        "#minecraft:logs", [], {}, [1], [1, 2, 3], ["a"], {"a": 1}, [[]], [{}], Raw("1e999"), Raw("0.1"), Raw("-0")]


def walk(o, path=()):
    yield path, o
    if isinstance(o, dict):
        for k, v in o.items():
            yield from walk(v, path + (k,))
    elif isinstance(o, list):
        for i, v in enumerate(o):
            yield from walk(v, path + (i,))


def get(o, path):
    for p in path:
        o = o[p]
    return o


def put(o, path, v):
    if not path:
        return v
    o = json.loads(json.dumps(o, default=lambda x: x.text if isinstance(x, Raw) else (x.v if isinstance(x, Num) else None)))
    t = o
    for p in path[:-1]:
        t = t[p]
    if v is _DELETE:
        if isinstance(t, dict):
            del t[path[-1]]
        else:
            t.pop(path[-1])
    else:
        t[path[-1]] = v
    return o


_DELETE = object()


def mutate(o):
    nodes = list(walk(o))
    path, node = R.choice(nodes)
    k = R.randrange(7)
    if k == 0:
        return put(o, path, R.choice(JUNK))
    if k == 1 and path:
        return put(o, path, _DELETE)
    if k == 2 and isinstance(node, dict):
        n = json.loads(json.dumps(node, default=lambda x: 0))
        n[R.choice(["extra", "id", "type", "value", ""])] = R.choice(JUNK)
        return put(o, path, n)
    if k == 3:
        return put(o, path, [node])
    if k == 4 and isinstance(node, list) and node:
        return put(o, path, node[0])
    if k == 5 and isinstance(node, (int, float)) and not isinstance(node, bool):
        return put(o, path, R.choice([node + 1, node - 1, -node, node * 2, str(node), node + 0.5, 2**31, -(2**31) - 1, 0, [node]]))
    if k == 6 and isinstance(node, str):
        return put(o, path, R.choice([node.upper(), node + "x", node[:-1], "minecraft:" + node, "#" + node, [node], ""]))
    return put(o, path, R.choice(JUNK))


class Out:
    def __init__(self, name):
        self.name = name
        self.lines = []

    def add(self, kind, comp, payload):
        assert "\t" not in payload and "\n" not in payload, payload
        if comp != "-" and ":" not in comp:
            comp = "minecraft:" + comp
        self.lines.append(f"{kind}\t{comp}\t{payload}")

    def both(self, comp, o):
        self.add("json", comp, js(o))
        self.add("snbt", comp, snbt(o))

    def raw(self, comp, text):
        self.add("json", comp, text)
        self.add("snbt", comp, text)

    def mutants(self, comp, base, n):
        for _ in range(n):
            m = mutate(base)
            self.both(comp, m)

    def save(self):
        seen, uniq = set(), []
        for l in self.lines:
            if l not in seen:
                seen.add(l)
                uniq.append(l)
        path = os.path.join(HERE, "corpus", self.name)
        with open(path + ".tmp", "w") as f:
            f.write("\n".join(uniq) + "\n")
        os.replace(path + ".tmp", path)
        print(self.name, len(uniq))


def vi(n):
    n &= 0xFFFFFFFF
    out = b""
    while True:
        if n & ~0x7F == 0:
            out += bytes([n])
            return out.hex()
        out += bytes([(n & 0x7F) | 0x80])
        n >>= 7


def be32(n):
    return (n & 0xFFFFFFFF).to_bytes(4, "big").hex()


def f32hex(x):
    import struct
    return struct.pack(">f", x).hex()


def utf(s):
    b = s.encode()
    return vi(len(b)) + b.hex()


def wires(o, comp, hexes):
    for h in hexes:
        o.add("wire", comp, h)


def rand_hex(n):
    return bytes(R.randrange(256) for _ in range(n)).hex()


ENCHANTS = ["aqua_affinity", "bane_of_arthropods", "binding_curse", "blast_protection", "breach", "channeling", "density", "depth_strider",
            "efficiency", "feather_falling", "fire_aspect", "fire_protection", "flame", "fortune", "frost_walker", "impaling", "infinity",
            "knockback", "looting", "loyalty", "luck_of_the_sea", "lunge", "lure", "mending", "multishot", "piercing", "power",
            "projectile_protection", "protection", "punch", "quick_charge", "respiration", "riptide", "sharpness", "silk_touch", "smite",
            "soul_speed", "sweeping_edge", "swift_sneak", "thorns", "unbreaking", "vanishing_curse", "wind_burst"]
DYES = ["white", "orange", "magenta", "light_blue", "yellow", "lime", "pink", "gray", "light_gray", "cyan", "purple", "blue", "brown",
        "green", "red", "black"]
DYE_COMPONENTS = ["dye", "base_color", "wolf/collar", "cat/collar", "sheep/color", "shulker/color", "tropical_fish/base_color",
                  "tropical_fish/pattern_color", "cushion/color"]
SOUNDS = ["minecraft:entity.item.break", "minecraft:block.anvil.land", "minecraft:ui.button.click", "block.note_block.harp",
          "minecraft:entity.player.levelup"]
DAMAGE_TYPES = ["minecraft:arrow", "minecraft:generic", "minecraft:fall", "minecraft:magic", "minecraft:wither_skull", "in_fire"]
COMPONENT_NAMES = ["minecraft:enchantments", "minecraft:unbreakable", "minecraft:lore", "minecraft:attribute_modifiers", "minecraft:damage",
                   "minecraft:custom_data", "minecraft:creative_slot_lock", "minecraft:additional_trade_cost", "minecraft:dyed_color",
                   "minecraft:food", "minecraft:can_break", "minecraft:potion_contents", "minecraft:trim", "enchantments", "tooltip_display"]
IDS = ["minecraft:stone", "minecraft:diamond_sword", "custom:thing", "a", "a:b", "minecraft:a/b/c", "x.y-z_0:p.q-r_9/s", "", "minecraft:",
       ":x", "Minecraft:x", "a:b:c", "a b", "minecraft:Stone", "%", "minecraft:é", "a" * 300, "minecraft:" + "a" * 100, "minecraft:a//b", "..",
       "a:../b", "ns:path with space"]


def c(path, s):
    return "minecraft:" + path


# ---------------------------------------------------------------- file a
a = Out("10-basic-a.txt")

# scalars
for comp, vals in [
    ("max_stack_size", [1, 16, 64, 99, 100, 0, -1, 1.5, "5", True, None, [], {}, 2**31, Raw("1e2"), Raw("1.0"), Raw("99.9"), Raw("0.5")]),
    ("max_damage", [1, 100, 2**31 - 1, 0, -1, 2**31, 1.9, "x", []]),
    ("damage", [0, 1, 1561, 2**31 - 1, -1, 2**31, Raw("3.7"), "1", False, {}]),
    ("repair_cost", [0, 1, 2, 31, 2**31 - 1, -1, -2**31, 2**31, 2**40, Raw("1.5"), Raw("-0.5"), Raw("1e3"), "1", True, None, [], [1], {}, {"value": 1}]),
    ("minimum_attack_charge", [0, 1, 0.5, 1.0, 0.0, Raw("-0.0"), 0.25, Raw("1.0000001"), Raw("1.0000000001"), Raw("-0.00001"), 2, -1, Raw("1e-50"), Raw("1e999"),
                               "0.5", True, [], [0.5], {}, None]),
    ("potion_duration_scale", [0, 1, 0.5, 2.5, Raw("3.4028235e38"), Raw("3.5e38"), Raw("1e39"), Raw("-0.0"), Raw("0.0"), -1, Raw("-1e-3"), 100,
                               "1", [], {}, None, Raw("1e-46")]),
    ("enchantment_glint_override", [True, False, 0, 1, 2, -1, Raw("0.5"), Raw("1.5"), Raw("0.0"), "true", "false", "", None, [], [True], {}]),
    ("ominous_bottle_amplifier", [0, 1, 2, 3, 4, 5, -1, 100, 2**31, Raw("2.5"), Raw("4.9"), Raw("5.1"), "1", True, [], {}, {"value": 1}, None]),
    ("map_id", [0, 1, 7, -1, 2**31 - 1, -2**31, 2**31, Raw("1.5"), Raw("-1.5"), "1", True, [], {}, {"id": 1}, None]),
]:
    for v in vals:
        a.both(comp, v)
        a.add("snbt", comp, snbt(v)) if R.random() < 0.3 else None

for t in ["1b", "1s", "1L", "-1b", "5b", "100b", "127b", "128", "0b", "1f", "1.0f", "1d", "0.5f", "1.5f", "-1.5d", "255", "100s", "2147483647",
          "2147483648", "9223372036854775807L", "1e0", "1.0e1", "2.0e0f", "NaNf", "NaNd", "Infinityd", "-Infinityf", "\"1\"", "\"x\"", "true", "false",
          "[1]", "[I;1]", "[B;1b]", "{}", "{value:1}", ""]:
    for comp in ["max_stack_size", "repair_cost", "minimum_attack_charge", "ominous_bottle_amplifier", "map_id", "enchantment_glint_override",
                 "potion_duration_scale", "damage", "max_damage"]:
        if R.random() < 0.55:
            a.add("snbt", c(comp, comp) if False else "minecraft:" + comp, t)

# units
for comp in ["unbreakable", "intangible_projectile", "glider", "waxed", "creative_slot_lock"]:
    for v in [{}, {"a": 1}, {"": ""}, [], 1, True, "x", "{}", None, [{}], {"a": {"b": {}}}]:
        a.both("minecraft:" + comp if False else comp, v)
    for t in ["{}", "{a:1b}", "[]", "1b", "\"\"", "{a:{}}", "[{}]", "{show_in_tooltip:false}", "{show_in_tooltip:0b}"]:
        a.add("snbt", comp, t)
    a.add("json", comp, '{"show_in_tooltip":false}')
    wires(a, comp, ["", "00", "01", "ff"])

# rarity
for v in ["common", "uncommon", "rare", "epic", "Common", "EPIC", "legendary", "", " common", "common ", 0, 1, 3, 4, True, [], ["rare"], {}, None,
          "minecraft:rare", {"rarity": "rare"}]:
    a.both("rarity", v)
wires(a, "rarity", ["00", "01", "02", "03", "04", "05", "ff", "ffffffff0f", "8001", "", "e807", "80", "0100", "ffffffffff"])

# identifiers
for comp in ["item_model", "tooltip_style", "note_block_sound"]:
    for v in IDS + [1, True, None, [], {}, ["minecraft:stone"], {"id": "minecraft:stone"}]:
        a.both(comp, v)
    for h in [utf("minecraft:stone"), utf("a:b"), utf(""), utf("x y"), utf("Minecraft:x"), utf("a" * 40), "", "01", "ff", utf("é:é"), utf("a:b") + "00"]:
        a.add("wire", comp, h)

# additional trade cost (network only)
wires(a, "additional_trade_cost", [vi(0), vi(1), vi(63), vi(64), vi(127), vi(128), vi(2**31 - 1), vi(-1), vi(-2**31), "", "80", "ffffffffff0f", "ffffffff0f", "0000"])
wires(a, "map_post_processing", [vi(0), vi(1), vi(2), vi(-1), vi(1000), "", "80", "0001", "ffffffff0f"])
wires(a, "creative_slot_lock", ["", "00"])
for comp in ["repair_cost", "max_damage", "damage", "max_stack_size", "map_id", "ominous_bottle_amplifier", "villager_food", "enchantable"]:
    wires(a, comp, [vi(0), vi(1), vi(5), vi(99), vi(100), vi(127), vi(128), vi(300), vi(-1), vi(2**31 - 1), vi(-2**31), "", "80", "ffffffffff0f", vi(1) + "00"])
for comp in ["minimum_attack_charge", "potion_duration_scale"]:
    for x in [0.0, 1.0, 0.5, -0.0, 2.0, -1.0, float("inf"), float("nan"), 3.4028235e38, 1e-45]:
        a.add("wire", comp, f32hex(x))
    wires(a, comp, ["", "00", "000000", "3f800000" + "00"])
wires(a, "enchantment_glint_override", ["00", "01", "02", "ff", "", "0100"])
wires(a, "dyed_color", [be32(0), be32(0xFFFFFF), be32(-1), be32(0xA06540), "", "000000", be32(1) + "00"])

# dye colors
for comp in DYE_COMPONENTS:
    for v in DYES[: 6 if comp != "dye" else 16] + ["White", "RED", "grey", "light gray", "minecraft:red", "", 0, 1, 15, 16, -1, True, None, [], ["red"], {}, {"color": "red"}]:
        a.both(comp, v)
    wires(a, comp, [vi(0), vi(1), vi(15), vi(16), vi(17), vi(-1), vi(1000), "", "80", "0001", "ffffffff0f"])

# custom name / item name
TEXTS = ["hello", "", "§cred", {"text": "a"}, {"text": ""}, {"text": "a", "color": "red", "bold": True}, {"text": "a", "color": "#FF00FF"},
         {"text": "a", "color": "bad"}, {"translate": "item.minecraft.stone"}, {"translate": "x", "with": ["a", 1, 2.5]}, {"translate": "x", "fallback": "y"},
         [{"text": "a"}, {"text": "b"}], [], ["a", "b", {"text": "c"}], ["a"], {"text": "a", "extra": ["b", {"text": "c"}]},
         {"keybind": "key.jump"}, {"score": {"name": "@s", "objective": "o"}}, {"selector": "@a"}, {"text": "a", "italic": False}, 1, 1.5, True, False, None,
         {}, {"text": 1}, {"text": []}, {"text": "a", "color": 1}, {"text": "a", "font": "minecraft:uniform"}, {"text": "a", "font": "bad font"},
         {"text": "a", "shadow_color": 16711680}, {"text": "a", "shadow_color": [1.0, 0.0, 0.0, 1.0]}, {"type": "text", "text": "x"},
         {"type": "translatable", "translate": "a"}, {"type": "nope"}, {"text": "a", "click_event": {"action": "open_url", "url": "https://x.y"}},
         {"text": "a", "click_event": {"action": "run_command", "command": "say hi"}}, {"text": "a", "hover_event": {"action": "show_text", "value": "x"}},
         {"text": "a", "insertion": "z"}, {"text": "a", "extra": []}, {"text": "a", "unknown": 1}, [[]], [[{"text": "a"}]],
         {"text": "x" * 300}, "x" * 2000, [1, 2], ["a", 1], {"translate": "x", "with": []}, {"translate": 1}]
for comp in ["custom_name", "item_name"]:
    for v in TEXTS:
        a.both(comp, v)
    for t in ["'hello'", "{text:\"a\"}", "{text:\"a\",color:\"red\",bold:1b}", "{text:\"a\",bold:true}", "[\"a\",{text:\"b\"}]", "[]", "{}", "1", "1b", "{text:1b}",
              "[{text:\"a\"},\"b\"]", "{translate:\"x\",with:[1,2.5f,\"a\",{text:\"y\"}]}", "{text:\"a\",extra:[1b]}", "[I;1]", "{text:\"a\",color:\"red\",bold:\"true\"}",
              "[1,\"a\"]", "[\"a\",1]", "{text:\"\"}", "''"]:
        a.add("snbt", comp, t)
    for h in ["08000161", "0a000004" + "74657874" + "08000161" + "00" if False else "0a0008" + "74657874" + "00" if False else "08000161", "0a00", "0a0000", "08ffff", "", "00", "07000100", "0900000000" + "00",
              "0a0804" + "74657874" + "0001" + "61" + "00", "0801", "0a" + "08" + "0004" + "74657874" + "0001" + "78" + "00"]:
        a.add("wire", comp, h)

# lore
LINES = ["a", {"text": "b", "color": "gold"}, ["x", "y"], {"text": ""}, "", {"translate": "k"}, 1, None, {"text": 1}, {}, True]
a.both("lore", [])
for _ in range(60):
    n = R.choice([1, 1, 2, 3, 5])
    a.both("lore", [R.choice(LINES) for _ in range(n)])
for n in [255, 256, 257, 300]:
    a.both("lore", ["l"] * n)
    a.both("lore", [{"text": "t"}] * n)
for v in ["a", {"text": "a"}, 1, None, True, {}, [[]], [[{"text": "a"}]], ["a", ["b", "c"]], Raw("[\"a\",]"), Raw("[,\"a\"]"), Raw("[\"a\" \"b\"]")]:
    a.both("lore", v)
for t in ["[]", "['a']", "[\"a\",{text:\"b\",italic:0b}]", "[[\"a\"]]", "\"a\"", "{}", "[1b]", "[{text:'a'},{text:'b'}]", "[I;1]", "[\"a\",1]"]:
    a.add("snbt", "lore", t)
for h in [vi(0), vi(1) + "08000161", vi(2) + "08000161" + "08000162", vi(1), vi(-1), vi(256) + "08000161" * 256, vi(257), "", vi(1) + "0a00", vi(300)]:
    a.add("wire", "lore", h)

# custom_data / bucket_entity_data
CD = [{}, {"a": 1}, {"a": 1, "b": 2.5, "c": "s", "d": True, "e": None}, {"a": {"b": {"c": [1, 2, {"d": []}]}}}, {"": 1}, {"minecraft:x": [1, 2, 3]},
      {"big": 2**40, "neg": -2**31, "f": 1.0, "ff": Raw("1e300"), "z": Raw("-0.0")}, {"l": [[]], "m": [{}], "n": [[1], [2, 3]]}, {"list": [1, "a", True]},
      {"a": [1, 2.5]}, {"k": Raw("123456789012345678901234567890")}, {"é": "é", "\u0000": "x"}, {str(i): i for i in range(30)}, 1, "x", [], [1], True, None, 1.5,
      "{}", "{a:1}", "{a:1b,b:2s}", "{a:", "{a:1}x", "{a:1} ", " {a:1}", "[]", "[1]", "1", "", "{'a':\"b\"}", "{a:[I;1,2],b:[B;1b],c:[L;1L]}", "{a:1,a:2}",
      "{a:{b:{c:{d:{}}}}}", "{a:1.5f,b:2d,c:3L,d:4s,e:5b,f:'g'}", "{a:true,b:false}", "{a:1e3,b:0x10,c:0b101,d:1_000}", "{a:\"\\u00e9\"}", "{ a : 1 , b : 2 }",
      "{a:[1,2,3],b:[]}", "{a:[1,2.5]}", "{a:[1b,2]}", "{a:[{},1]}", "{a:[[],{}]}", "{\"a b\":1}", "{a:NaNd}", "{a:Infinityf}", "{a:-Infinityd}",
      "{a:1.0E5f}", "{a:.5}", "{a:5.}", "{a:+1}", "{a:-0.0f}", "{a:9223372036854775808}", "{a:'\\n'}", "{a:\"\"}", "{a:{}}", "{a:[]}", "{a:[I;]}", "{a:[I;1b]}",
      "{a:[B;300]}", "{a:[B;true]}", "{a:'\\x'}", "{a:unquoted_string}", "{a:minecraft:x}", "{a:1 b:2}", "{a:1,}", "{,a:1}", "{a::1}", "{a=1}"]
for comp in ["custom_data", "bucket_entity_data"]:
    for v in CD:
        if isinstance(v, str) and (v.startswith("{") or v.startswith("[") or v in ("1", "", "{}")):
            a.add("json", comp, json.dumps(v))  # SNBT string in JSON
            a.add("snbt", comp, v)  # compound / tag directly
            a.add("snbt", comp, sstr(v))  # SNBT string inside SNBT
        else:
            a.both(comp, v)
    for _ in range(40):
        a.both(comp, mutate(R.choice(CD[:12])))
    for h in ["0a00" + "00", "0a00" + "0300" + "01" + "61" + "00000001" + "00" if False else "0a00" + "03000161" + "00000001" + "00", "0a0000", "0800" + "057b613a317d", "0a", "", "00",
              "0a00" + "0a0001" + "61" + "0a0001" + "62" + "00" + "00" + "00", "0a000b" + "0001" + "61" + "00000000" + "00", "0b00", "0900" + "00" + "00000000", "0a00" + "08" + "0001" + "61" + "0001" + "62" + "00",
              "0800" + "07" + "7b613a317d00"[0:0] + "0004" + "7b7d" if False else "08000a7b613a317d", "0800047b3a7d", "0a" + "00" * 3]:
        a.add("wire", comp, h)
for comp in ["custom_data", "bucket_entity_data"]:
    for t in [h for h in ["08000a7b613a317d", "0800047b3a7d", "0800027b7d", "0800015b" if False else "0800025b5d"]]:
        a.add("wire", comp, t)
a.save()

# ---------------------------------------------------------------- file b
b = Out("10-basic-b.txt")

E = lambda n: "minecraft:" + n
LV = [1, 2, 5, 10, 255, 0, 256, -1, 1000, 2**31, Raw("1.5"), Raw("2.0"), Raw("255.9"), "5", True, None, [], {}, [1], Raw("1e2")]

for comp in ["enchantments", "stored_enchantments"]:
    b.both(comp, {})
    for e in ENCHANTS[:]:
        b.both(comp, {E(e): R.choice([1, 2, 3, 5, 10, 255])})
    for e in ["sharpness", "mending", "unbreaking"]:
        b.both(comp, {e: 3})
    for lv in LV:
        b.both(comp, {E("sharpness"): lv})
    for t in ['{"minecraft:sharpness":5}', "{'minecraft:sharpness':5b}", '{"minecraft:sharpness":5s}', '{"minecraft:sharpness":5L}', '{"minecraft:sharpness":1.5f}',
              '{"minecraft:sharpness":5.0d}', '{"minecraft:sharpness":"5"}', '{"minecraft:sharpness":255}', '{"minecraft:sharpness":256}', '{"minecraft:sharpness":0b}',
              '{"minecraft:sharpness":-1}', '{"minecraft:sharpness":true}', "{sharpness:1}", "{\"minecraft:sharpness\":[I;1]}", "{\"minecraft:sharpness\":{}}",
              "{\"minecraft:nope\":1}", "{\"#minecraft:x\":1}", '{"":1}', "{\"minecraft:sharpness\":1,\"minecraft:nope\":1}", "{\"minecraft:nope\":1,\"minecraft:sharpness\":1}",
              '{"minecraft:sharpness":1,"sharpness":2}', "{levels:{}}", "{levels:{\"minecraft:sharpness\":1}}", "{levels:{\"minecraft:sharpness\":1},show_in_tooltip:0b}", "[]", "[1]", "1", "\"x\"",
              "{\"minecraft:sharpness\":1,\"minecraft:sharpness\":2}", "{\"Minecraft:sharpness\":1}", "{\"minecraft:Sharpness\":1}", "{\"a:b:c\":1}", "{minecraft:sharpness:1}"]:
        b.add("snbt", comp, t)
        b.add("json", comp, t)
    for v in [[], [1], 1, "x", None, True, {"minecraft:sharpness": 1, "minecraft:nope": 1}, {"minecraft:nope": 1, "minecraft:sharpness": 1}, {"minecraft:nope": 1},
              {"levels": {}}, {"levels": {E("sharpness"): 1}}, {"levels": {E("sharpness"): 1}, "show_in_tooltip": False}, {"minecraft:sharpness": 1, "minecraft:smite": 0},
              {"minecraft:sharpness": 1, "minecraft:smite": 256}, {"minecraft:sharpness": 1, "x": "y"}]:
        b.both(comp, v)
    # duplicates after identifier normalisation are only possible via a duplicate JSON key, which Gson replaces in place
    b.raw(comp, '{"minecraft:sharpness":1,"minecraft:sharpness":2}')
    b.raw(comp, '{"minecraft:sharpness":1,"sharpness":2}')
    n = len(ENCHANTS)
    wires(b, comp, [vi(0), vi(1) + vi(0) + vi(1), vi(1) + vi(n - 1) + vi(255), vi(1) + vi(n) + vi(1), vi(1) + vi(0) + vi(256), vi(1) + vi(0) + vi(-1),
                    vi(1) + vi(0) + vi(0), vi(1) + vi(5) + vi(7), vi(1) + vi(-1) + vi(1), vi(-1), vi(1), vi(1) + vi(0), "", vi(2) + vi(3) + vi(1) + vi(3) + vi(2),
                    vi(2) + vi(3) + vi(1) + vi(3) + vi(256), vi(1) + vi(2**31 - 1) + vi(1), vi(1) + vi(0) + vi(1) + "00", vi(2**31 - 1), vi(70000)])

# tooltip_display
TD = [{}, {"hide_tooltip": True}, {"hide_tooltip": False}, {"hidden_components": []}, {"hidden_components": [COMPONENT_NAMES[0]]},
      {"hide_tooltip": True, "hidden_components": [COMPONENT_NAMES[0], COMPONENT_NAMES[1]]}, {"hidden_components": COMPONENT_NAMES[:]},
      {"hidden_components": ["minecraft:enchantments", "minecraft:enchantments"]}, {"hidden_components": ["minecraft:enchantments", "enchantments"]},
      {"hidden_components": ["minecraft:nope"]}, {"hidden_components": "minecraft:enchantments"}, {"hidden_components": [1]}, {"hidden_components": [None]},
      {"hidden_components": [["minecraft:enchantments"]]}, {"hide_tooltip": 1}, {"hide_tooltip": 0}, {"hide_tooltip": 2}, {"hide_tooltip": "true"},
      {"hide_tooltip": None}, {"hide_tooltip": []}, {"hidden_components": {}}, {"hidden_components": None}, {"extra": 1}, {"hide_tooltip": True, "extra": []},
      [], 1, "x", None, True, {"hide_tooltip": False, "hidden_components": []}, {"hidden_components": ["#minecraft:x"]}, {"hidden_components": [""]},
      {"hidden_components": ["minecraft:creative_slot_lock", "minecraft:additional_trade_cost", "minecraft:map_post_processing"]},
      {"hidden_components": ["MINECRAFT:ENCHANTMENTS"]}, {"hide_tooltip": Raw("1.5")}, {"hide_tooltip": Raw("0.0")}]
for v in TD:
    b.both("tooltip_display", v)
for _ in range(60):
    b.both("tooltip_display", mutate(R.choice(TD[:8])))
for t in ["{hide_tooltip:1b}", "{hide_tooltip:true,hidden_components:[\"minecraft:enchantments\"]}", "{hide_tooltip:2b}", "{hide_tooltip:0.5f}", "{hide_tooltip:1}",
          "{hide_tooltip:'true'}", "{hidden_components:[]}", "{hidden_components:[I;1]}", "{hidden_components:\"minecraft:lore\"}", "{hidden_components:[\"lore\",\"lore\"]}",
          "{hidden_components:[1b]}", "{hidden_components:[{}]}", "{}", "{hide_tooltip:0b,hidden_components:[]}", "{hide_tooltip:0b}"]:
    b.add("snbt", "tooltip_display", t)
nc = 100
wires(b, "tooltip_display", ["00" + vi(0), "01" + vi(0), "00" + vi(1) + vi(0), "00" + vi(2) + vi(0) + vi(0), "00" + vi(2) + vi(0) + vi(1), "02" + vi(0), "ff" + vi(0),
                             "00" + vi(1) + vi(nc), "00" + vi(1) + vi(1000), "00" + vi(1) + vi(-1), "00" + vi(-1), "00", "", "01" + vi(1), "01" + vi(3) + vi(5) + vi(6) + vi(5),
                             "00" + vi(1) + vi(0) + "00", "00" + vi(2**31 - 1)])

# custom_model_data
CM = [{}, {"floats": [1.0]}, {"floats": [1, 2, 3]}, {"floats": [0.5, -1.5, Raw("1e10")]}, {"flags": [True, False]}, {"strings": ["a", "b", ""]},
      {"colors": [1, 2, 3]}, {"colors": [[1.0, 0.5, 0.0]]}, {"colors": [[1, 0, 0]]}, {"colors": [[1.0, 0.5]]}, {"colors": [[1.0, 0.5, 0.0, 1.0]]},
      {"colors": [[]]}, {"colors": [[2.0, -1.0, 0.5]]}, {"colors": [16777215, [0.1, 0.2, 0.3]]}, {"colors": [-1, 2**31 - 1, -2**31]}, {"colors": [2**31]},
      {"colors": [Raw("1.5")]}, {"colors": [["a", 1, 2]]}, {"colors": 5}, {"colors": [5, "x"]}, {"floats": [1], "flags": [True], "strings": ["x"], "colors": [7]},
      {"floats": "x"}, {"floats": [None]}, {"floats": 1.0}, {"flags": [0, 1]}, {"flags": [2]}, {"flags": [Raw("0.5")]}, {"flags": ["true"]}, {"strings": [1]},
      {"strings": "a"}, {"strings": [True]}, {"strings": [["a"]]}, {"floats": []}, {"flags": []}, {"strings": []}, {"colors": []}, {"unknown": 1}, [], 1, 5.0, "x", None,
      {"floats": [Raw("1e999")]}, {"floats": [Raw("1e-999")]}, {"colors": [[Raw("1e999"), 0, 0]]}, {"colors": [[0.003921569, 0.99999, 1.0]]}, {"colors": [[-0.0, 0.0, 0.0]]},
      {"colors": [[1, 2, 3, 4]]}, {"colors": [[0.5, 0.5, 0.5], [1, 1, 1]]}, {"floats": [Num(1, "b")]}, {"floats": [Num(1, "L")]}]
for v in CM:
    b.both("custom_model_data", v)
for _ in range(70):
    b.both("custom_model_data", mutate(R.choice(CM[:22])))
for t in ["{floats:[1.0f,2.5d,3]}", "{floats:[1b]}", "{floats:[1L]}", "{flags:[1b,0b]}", "{flags:[true]}", "{colors:[I;1,2]}", "{colors:[[I;1,2,3]]}", "{colors:[[1.0f,0.5f,0.0f]]}",
          "{colors:[[1,2,3]]}", "{colors:[1b]}", "{colors:[1L]}", "{colors:[[1b,2b,3b]]}", "{strings:['a',\"b\"]}", "{floats:[B;1b]}", "{floats:[I;1]}", "{floats:[NaNf]}", "{floats:[Infinityf]}"]:
    b.add("snbt", "custom_model_data", t)
wires(b, "custom_model_data", [vi(0) * 4, vi(1) + f32hex(1.5) + vi(0) * 3, vi(0) + vi(2) + "0001" + vi(0) + vi(0), vi(0) + vi(0) + vi(1) + utf("a") + vi(0), vi(0) * 3 + vi(1) + be32(255),
                               vi(0) * 3 + vi(2) + be32(1) + be32(2), vi(0) + vi(1) + "02" + vi(0) * 2, vi(0) * 4 + "00", vi(-1) + vi(0) * 3, vi(0) * 2 + vi(1) + vi(40000) + "00",
                               vi(1) + f32hex(float("nan")) + vi(0) * 3, vi(1), "", vi(0) * 3, vi(0) * 2 + vi(1) + utf("é") + vi(0), vi(0) * 3 + vi(1) + be32(-1)])

# dyed_color
DY = [0, 1, 255, 16777215, 16777216, 0xA06540, -1, -6265536, 2**31 - 1, -2**31, 2**31, Raw("1.5"), Raw("16777215.9"), [1.0, 1.0, 1.0], [0.0, 0.0, 0.0], [1, 1, 1], [0.5, 0.25, 0.125],
      [Raw("0.003921568"), 0.0, 0.0], [2.0, 2.0, 2.0], [-1.0, -1.0, -1.0], [Raw("1e30"), 0, 0], [1.0, 1.0], [1.0, 1.0, 1.0, 1.0], [], [[1, 2, 3]], ["a", "b", "c"], [None, 1, 2],
      "red", "#ff0000", True, None, {}, {"rgb": 1}, [True, False, True], [0.1, 0.2, "x"], [Raw("0.99999994"), Raw("0.5000001"), Raw("0.49999997")],
      [Raw("1.0000001"), 0, 0], [1.0, 0.0, 0.0, 0.0, 0.0], [Raw("1e999"), 0, 0], [Raw("0.0039215686"), Raw("0.0078431373"), Raw("0.0117647059")]]
for v in DY:
    b.both("dyed_color", v)
for _ in range(30):
    b.both("dyed_color", mutate(R.choice(DY[:20])))
for t in ["1b", "1s", "1L", "-1b", "[1.0f,0.5f,0.0f]", "[1.0d,0.5d,0.0d]", "[1,2,3]", "[1b,2b,3b]", "[I;1,2,3]", "[B;1b,2b,3b]", "[1.0f,0.5f]", "[NaNf,0f,0f]", "[Infinityf,0f,0f]", "[-Infinityf,0f,0f]",
          "16777215", "16777216", "0x10", "1.5f", "2.0d", "[1L,2L,3L]", "{}", "\"1\"", "true"]:
    b.add("snbt", "dyed_color", t)
wires(b, "dyed_color", [be32(0), be32(1), be32(-1), be32(0xFFFFFF), be32(-6265536), "00", "000000", "", be32(5) + "00"])

# enchantable
EN = [{"value": 1}, {"value": 15}, {"value": 2**31 - 1}, {"value": 0}, {"value": -1}, {"value": 2**31}, {"value": Raw("1.5")}, {"value": Raw("0.5")}, {"value": "5"}, {"value": True},
      {"value": None}, {"value": []}, {}, {"valu": 1}, {"value": 1, "extra": 1}, {"value": 1, "value2": 2}, 5, "5", [], [5], None, True, {"VALUE": 1}, {"value": Raw("1e2")}, {"value": [1]}]
for v in EN:
    b.both("enchantable", v)
for t in ["{value:1b}", "{value:1s}", "{value:1L}", "{value:1.0f}", "{value:1.9d}", "{value:0b}", "{value:'1'}", "{value:[I;1]}", "{value:true}", "{value:{}}", "{}", "5", "{value:1,value:2}"]:
    b.add("snbt", "enchantable", t)
b.mutants("enchantable", EN[0], 15)

# villager_food
for v in [{"nutrition": 1}, {"nutrition": 4}, {"nutrition": 2**31 - 1}, {"nutrition": 0}, {"nutrition": -1}, {"nutrition": 2**31}, {"nutrition": Raw("2.5")}, {"nutrition": "1"}, {"nutrition": True},
          {"nutrition": None}, {}, {"value": 1}, 1, [], None, {"nutrition": 1, "x": 1}, {"nutrition": [1]}, {"nutrition": Raw("1e1")}]:
    b.both("villager_food", v)
for t in ["{nutrition:1b}", "{nutrition:1L}", "{nutrition:1.5f}", "{nutrition:0}", "{nutrition:'x'}", "{nutrition:[1]}", "{}", "1"]:
    b.add("snbt", "villager_food", t)
b.mutants("villager_food", {"nutrition": 4}, 12)

# repairable
RP = ["minecraft:stone", "diamond", ["minecraft:stone"], ["minecraft:stone", "minecraft:dirt"], [], ["minecraft:stone", "minecraft:stone"], "#minecraft:planks", "#minecraft:nope",
      "#nope", "#", "#minecraft:", ["#minecraft:planks"], ["minecraft:stone", "#minecraft:planks"], "minecraft:nope", ["minecraft:nope"], ["minecraft:stone", "minecraft:nope"], "",
      [""], 1, True, None, {}, {"items": []}, [1], [None], [[]], "air", "minecraft:air", "#minecraft:logs", "#minecraft:logs_that_burn", ["minecraft:air"], "minecraft:stone ",
      "Minecraft:stone", ["a"] * 3, ["minecraft:stone"] * 40]
for it in RP:
    b.both("repairable", {"items": it})
b.both("repairable", {})
for v in ["minecraft:stone", [], 1, None, {"items": "minecraft:stone", "x": 1}, {"item": "minecraft:stone"}, {"items": {"a": 1}}, {"items": Raw("{}")}, {"items": ["minecraft:stone"], "items2": 1}]:
    b.both("repairable", v)
for _ in range(25):
    b.both("repairable", mutate({"items": R.choice(RP[:8])}))
for t in ["{items:\"minecraft:stone\"}", "{items:[\"minecraft:stone\"]}", "{items:\"#minecraft:planks\"}", "{items:[]}", "{items:[1]}", "{items:[I;1]}", "{items:{}}", "{items:'minecraft:stone'}", "{}"]:
    b.add("snbt", "repairable", t)
# ids of stone / dirt / air in the item registry are resolved by the generator's own wire records; hand-written ones probe the framing
wires(b, "repairable", [vi(0) + utf("minecraft:planks"), vi(0) + utf("minecraft:nope"), vi(0) + utf("bad id"), vi(0), vi(1), vi(1) + vi(0), vi(1) + vi(1), vi(2) + vi(1) + vi(2),
                        vi(1) + vi(10000), vi(1) + vi(-1), vi(-1), vi(-2), vi(3) + vi(1), "", vi(1) + vi(1) + "00", vi(2**31 - 1)])

# mob_visibility
MV = [{"targeting_entity_types": "minecraft:zombie", "visibility": 0.5}, {"targeting_entity_types": ["minecraft:zombie", "minecraft:skeleton"], "visibility": 1.0},
      {"targeting_entity_types": "#minecraft:skeletons", "visibility": 10}, {"targeting_entity_types": [], "visibility": 0}, {"targeting_entity_types": "minecraft:zombie", "visibility": 10.0000001},
      {"targeting_entity_types": "minecraft:zombie", "visibility": 11}, {"targeting_entity_types": "minecraft:zombie", "visibility": -0.1}, {"targeting_entity_types": "minecraft:zombie", "visibility": Raw("-0.0")},
      {"targeting_entity_types": "minecraft:nope", "visibility": 1}, {"targeting_entity_types": "#minecraft:nope", "visibility": 1}, {"targeting_entity_types": "minecraft:zombie"},
      {"visibility": 1}, {}, {"targeting_entity_types": 1, "visibility": 1}, {"targeting_entity_types": "minecraft:zombie", "visibility": "1"}, {"targeting_entity_types": "minecraft:zombie", "visibility": True},
      {"targeting_entity_types": ["minecraft:zombie"], "visibility": Raw("1e-50")}, {"targeting_entity_types": "minecraft:zombie", "visibility": Raw("1e999")},
      {"targeting_entity_types": "minecraft:zombie", "visibility": 3, "extra": 1}, {"targeting_entity_types": [None], "visibility": 1}, {"targeting_entity_types": [1], "visibility": 1},
      {"targeting_entity_types": "minecraft:player", "visibility": 2.5}, {"targeting_entity_types": ["minecraft:player"] * 3, "visibility": 2.5}, {"targeting_entity_types": "minecraft:pig", "visibility": 0.25},
      {"targeting_entity_types": "#minecraft:aquatic", "visibility": 5}, {"targeting_entity_types": ["#minecraft:aquatic"], "visibility": 5}, [], 1, None, "x",
      {"targeting_entity_types": "minecraft:zombie", "visibility": Num(5, "b")}, {"targeting_entity_types": "minecraft:zombie", "visibility": Raw("10.000000001")}]
for v in MV:
    b.both("mob_visibility", v)
for _ in range(40):
    b.both("mob_visibility", mutate(R.choice(MV[:5])))
for t in ["{targeting_entity_types:\"minecraft:zombie\",visibility:5b}", "{targeting_entity_types:\"minecraft:zombie\",visibility:NaNf}", "{targeting_entity_types:\"minecraft:zombie\",visibility:Infinityf}",
          "{targeting_entity_types:\"minecraft:zombie\",visibility:2.5d}", "{targeting_entity_types:[\"minecraft:zombie\"],visibility:1L}", "{targeting_entity_types:{},visibility:1}",
          "{targeting_entity_types:\"minecraft:zombie\",visibility:'1'}"]:
    b.add("snbt", "mob_visibility", t)
wires(b, "mob_visibility", [vi(0) + utf("minecraft:skeletons") + f32hex(1.0), vi(1) + vi(0) + f32hex(0.5), vi(1) + vi(0) + f32hex(11.0), vi(1) + vi(0) + f32hex(-1.0), vi(1) + vi(0) + f32hex(float("nan")),
                            vi(1) + vi(9999) + f32hex(1.0), vi(1) + vi(0), vi(0) + utf("minecraft:nope") + f32hex(1.0), vi(-1), "", vi(2) + vi(0) + vi(1) + f32hex(10.0), vi(1) + vi(0) + f32hex(1.0) + "00"])

# break_sound
BS = ["minecraft:entity.item.break", "block.anvil.land", "minecraft:nope", "nope", "", "#minecraft:x", 1, True, None, [], ["minecraft:entity.item.break"],
      {"sound_id": "minecraft:entity.item.break"}, {"sound_id": "minecraft:custom.thing"}, {"sound_id": "minecraft:custom.thing", "range": 16.0}, {"sound_id": "custom:x", "range": 0},
      {"sound_id": "custom:x", "range": -1}, {"sound_id": "custom:x", "range": Raw("1e10")}, {"sound_id": "custom:x", "range": "x"}, {"sound_id": "custom:x", "range": None},
      {"sound_id": "custom:x", "range": True}, {"sound_id": "custom:x", "extra": 1}, {"range": 1}, {}, {"sound_id": 1}, {"sound_id": "bad id"}, {"sound_id": ""},
      {"sound_id": "minecraft:entity.item.break", "range": 4}, {"sound_id": "custom:x", "range": Raw("1.5")}, {"sound_id": "custom:x", "range": [1]}, {"sound_id": ["custom:x"]},
      {"sound_id": "Custom:x"}, {"sound_id": "custom:x", "range": Raw("-0.0")}, {"sound_id": "custom:x", "range": Raw("1e999")}] + ["minecraft:" + s for s in
      ["ui.button.click", "block.note_block.bell", "item.armor.equip_iron", "entity.zombie.ambient"]]
for v in BS:
    b.both("break_sound", v)
for _ in range(30):
    b.both("break_sound", mutate(R.choice(BS[:14])))
for t in ["\"minecraft:entity.item.break\"", "{sound_id:\"custom:x\",range:16f}", "{sound_id:\"custom:x\",range:16b}", "{sound_id:\"custom:x\",range:'x'}", "{sound_id:'custom:x'}", "{}", "1", "{sound_id:1b}"]:
    b.add("snbt", "break_sound", t)
wires(b, "break_sound", [vi(1), vi(2), vi(1000), vi(0) + utf("custom:x") + "00", vi(0) + utf("custom:x") + "01" + f32hex(16.0), vi(0) + utf("bad id") + "00", vi(0) + utf("custom:x") + "02",
                         vi(0) + utf("custom:x"), vi(0) + utf("custom:x") + "01", vi(-1), vi(0), "", vi(5) + "00", vi(0) + utf("custom:x") + "01" + f32hex(float("nan")), vi(0) + utf("") + "00",
                         vi(0) + utf("minecraft:entity.item.break") + "00"])

# damage_type
DT = DAMAGE_TYPES + ["minecraft:nope", "nope", "", "#minecraft:is_fire", 1, True, None, [], ["minecraft:arrow"], {}, {"message_id": "x", "scaling": "never", "exhaustion": 0.1},
                     {"id": "minecraft:arrow"}, "minecraft:arrow ", "MINECRAFT:ARROW", "minecraft:Arrow", "bad id"]
for v in DT:
    b.both("damage_type", v)
for t in ["'minecraft:arrow'", "{}", "1b", "[]", "{message_id:'x',scaling:'never',exhaustion:0f}", "\"\""]:
    b.add("snbt", "damage_type", t)
wires(b, "damage_type", [vi(0), vi(1), vi(5), vi(40), vi(41), vi(100), vi(-1), "", "80", vi(1) + "00", vi(2**31 - 1)])

# block_transformer
BT = ["minecraft:axe", "minecraft:hoe", "minecraft:shovel", "axe", "hoe", "minecraft:pickaxe", "minecraft:nope", "", "#minecraft:x", 1, True, None, [], ["minecraft:axe"], {},
      {"transforms": []}, {"block_state_provider": {}}, "minecraft:Axe", "MINECRAFT:AXE", "minecraft:axe ", "bad id", [{"block_state_provider": {}}]]
for v in BT:
    b.both("block_transformer", v)
for t in ["'minecraft:axe'", "{}", "[]", "1b", "\"\""]:
    b.add("snbt", "block_transformer", t)
wires(b, "block_transformer", [vi(0), vi(1), vi(2), vi(3), vi(4), vi(-1), "", "80", vi(1) + "00"])

# compostable / cooking_fuel / brewing_fuel
RI = [1, 0, -1, 2**31 - 1, -2**31, 2**31, Raw("1.5"), Raw("2.9"), Raw("-1.5"), Raw("1e3"), "minecraft:compostable/low", "minecraft:cooking/time_coal", "minecraft:nope", "nope", "x y", "", "Minecraft:x",
      "a:b:c", "5", True, False, None, [], [1], {}, {"value": 1}, "minecraft:brewing/uses_default", "#minecraft:x", "minecraft:", ["minecraft:x"], "a:b", Raw("1.0"), Raw("0.0"),
      Raw("123456789012"), Num(5, "b")]
RF = [1.0, 0, 1, 0.5, -1.5, Raw("1e30"), Raw("1e39"), Raw("-0.0"), Raw("1e-50"), "minecraft:cooking/speed_default", "minecraft:brewing/speed_default", "minecraft:nope", "x y", "", "1.5", True,
      None, [], [1.0], {}, {"value": 1.0}, Raw("1e999"), 2**31, 2**40, "a:b", Raw("0.1"), Num(1, "L")]
for v in RI:
    b.both("compostable", {"layers": v})
    b.both("cooking_fuel", {"burn_time": v, "speed_multiplier": R.choice(RF)})
    b.both("brewing_fuel", {"uses": v, "speed_multiplier": R.choice(RF)})
for v in RF:
    b.both("cooking_fuel", {"burn_time": R.choice(RI[:4] + RI[10:12]), "speed_multiplier": v})
    b.both("brewing_fuel", {"uses": R.choice(RI[:4] + RI[10:12]), "speed_multiplier": v})
for comp, f1 in [("compostable", "layers"), ("cooking_fuel", "burn_time"), ("brewing_fuel", "uses")]:
    base = {"layers": "minecraft:compostable/low"} if comp == "compostable" else {f1: 100, "speed_multiplier": 1.5}
    b.both(comp, {})
    b.both(comp, {"speed_multiplier": 1.0})
    b.both(comp, {f1: 1})
    b.both(comp, 5)
    b.both(comp, "minecraft:compostable/low")
    b.both(comp, [])
    b.both(comp, None)
    b.both(comp, {**base, "extra": 1})
    for _ in range(25):
        b.both(comp, mutate(base))
    for t in ["{layers:5}", "{layers:5b}", "{layers:5L}", "{layers:1.5f}", "{layers:'minecraft:x'}", "{layers:[1]}", "{layers:{}}", "{burn_time:1,speed_multiplier:1.0f}", "{burn_time:1,speed_multiplier:1}",
              "{burn_time:'a:b',speed_multiplier:'c:d'}", "{uses:1,speed_multiplier:NaNf}", "{uses:1b,speed_multiplier:1b}", "{uses:true,speed_multiplier:true}", "{burn_time:1,speed_multiplier:Infinityf}"]:
        b.add("snbt", comp, t)
RIW = [vi(0)] + ["01" + be32(5), "01" + be32(-1), "00" + utf("minecraft:cooking/time_coal"), "00" + utf("bad id"), "00" + utf(""), "02" + be32(1), "ff" + be32(1), "01", "00", "", "01" + be32(1) + "00", "00" + utf("a:b")]
RIW = RIW[1:]
RFW = ["01" + f32hex(1.5), "01" + f32hex(float("nan")), "00" + utf("minecraft:cooking/speed_default"), "00" + utf("bad id"), "02" + f32hex(1.0), "01", "", "00"]
wires(b, "compostable", RIW)
for x in RIW[:6]:
    for y in RFW:
        b.add("wire", "cooking_fuel", x + y)
        b.add("wire", "brewing_fuel", x + y)
wires(b, "cooking_fuel", RIW[:3])

# stack-snbt embedding the components
STACKS = [
    '{id:"minecraft:diamond_sword",count:1,components:{"minecraft:enchantments":{"minecraft:sharpness":5}}}',
    '{id:"minecraft:diamond_sword",count:1,components:{"minecraft:lore":["a",{"text":"b"}]}}',
    '{id:"minecraft:diamond_sword",count:1,components:{"minecraft:custom_name":{"text":"Excalibur","color":"gold"}}}',
    '{id:"minecraft:diamond_sword",count:1,components:{"minecraft:repair_cost":3}}',
    '{id:"minecraft:enchanted_book",count:1,components:{"minecraft:stored_enchantments":{"minecraft:mending":1}}}',
    '{id:"minecraft:stone",count:64,components:{"minecraft:custom_data":{a:1b,b:[1,2,3],c:{d:"e"}}}}',
    '{id:"minecraft:stone",count:1,components:{"minecraft:custom_data":"{a:1}"}}',
    '{id:"minecraft:stone",count:1,components:{"minecraft:custom_data":{}}}',
    '{id:"minecraft:stone",count:1,components:{"minecraft:item_name":"Name"}}',
    '{id:"minecraft:stone",count:1,components:{"minecraft:item_model":"minecraft:diamond"}}',
    '{id:"minecraft:stone",count:1,components:{"minecraft:tooltip_style":"custom:style"}}',
    '{id:"minecraft:stone",count:1,components:{"minecraft:rarity":"epic"}}',
    '{id:"minecraft:stone",count:1,components:{"minecraft:rarity":"bad"}}',
    '{id:"minecraft:leather_chestplate",count:1,components:{"minecraft:dyed_color":16711680}}',
    '{id:"minecraft:leather_chestplate",count:1,components:{"minecraft:dyed_color":[1.0f,0.0f,0.0f]}}',
    '{id:"minecraft:leather_chestplate",count:1,components:{"minecraft:dyed_color":[1.0f,0.0f]}}',
    '{id:"minecraft:stone",count:1,components:{"minecraft:tooltip_display":{hide_tooltip:1b,hidden_components:["minecraft:enchantments","minecraft:lore"]}}}',
    '{id:"minecraft:stone",count:1,components:{"minecraft:tooltip_display":{hidden_components:["minecraft:bad"]}}}',
    '{id:"minecraft:stone",count:1,components:{"!minecraft:lore":{}}}',
    '{id:"minecraft:stone",count:1,components:{"!minecraft:tooltip_display":{}}}',
    '{id:"minecraft:stone",count:1,components:{"!minecraft:custom_data":{}}}',
    '{id:"minecraft:iron_axe",count:1,components:{"minecraft:enchantable":{value:10}}}',
    '{id:"minecraft:iron_axe",count:1,components:{"minecraft:repairable":{items:"#minecraft:planks"}}}',
    '{id:"minecraft:iron_axe",count:1,components:{"minecraft:break_sound":"minecraft:entity.item.break"}}',
    '{id:"minecraft:iron_axe",count:1,components:{"minecraft:enchantable":{value:0}}}',
    '{id:"minecraft:iron_axe",count:1,components:{"minecraft:max_damage":10,"minecraft:max_stack_size":2}}',
    '{id:"minecraft:filled_map",count:1,components:{"minecraft:map_id":5}}',
    '{id:"minecraft:ominous_bottle",count:1,components:{"minecraft:ominous_bottle_amplifier":3}}',
    '{id:"minecraft:ominous_bottle",count:1,components:{"minecraft:ominous_bottle_amplifier":5}}',
    '{id:"minecraft:stone",count:1,components:{"minecraft:custom_model_data":{floats:[1.5f],flags:[1b],strings:["a"],colors:[16711680,[1.0f,0.0f,0.0f]]}}}',
    '{id:"minecraft:stone",count:1,components:{"minecraft:unbreakable":{}}}',
    '{id:"minecraft:stone",count:1,components:{"minecraft:glider":{}}}',
    '{id:"minecraft:stone",count:1,components:{"minecraft:intangible_projectile":{}}}',
    '{id:"minecraft:stone",count:1,components:{"minecraft:enchantment_glint_override":1b}}',
    '{id:"minecraft:stone",count:1,components:{"minecraft:creative_slot_lock":{}}}',
    '{id:"minecraft:stone",count:1,components:{"minecraft:waxed":{}}}',
    '{id:"minecraft:stone",count:1,components:{"minecraft:dye":"red"}}',
    '{id:"minecraft:stone",count:1,components:{"minecraft:wolf/collar":"red"}}',
    '{id:"minecraft:stone",count:1,components:{"minecraft:tropical_fish/pattern_color":"gray"}}',
    '{id:"minecraft:bucket",count:1,components:{"minecraft:bucket_entity_data":{NoAI:1b,Health:3.5f}}}',
    '{id:"minecraft:bucket",count:1,components:{"minecraft:bucket_entity_data":"{NoAI:1b}"}}',
    '{id:"minecraft:stone",count:1,components:{"minecraft:villager_food":{nutrition:4}}}',
    '{id:"minecraft:stone",count:1,components:{"minecraft:compostable":{layers:"minecraft:compostable/low"}}}',
    '{id:"minecraft:coal",count:1,components:{"minecraft:cooking_fuel":{burn_time:1600,speed_multiplier:1.0f}}}',
    '{id:"minecraft:coal",count:1,components:{"minecraft:brewing_fuel":{uses:3,speed_multiplier:"minecraft:brewing/speed_default"}}}',
    '{id:"minecraft:stone",count:1,components:{"minecraft:mob_visibility":{targeting_entity_types:"minecraft:zombie",visibility:0.5f}}}',
    '{id:"minecraft:stone",count:1,components:{"minecraft:damage_type":"minecraft:arrow"}}',
    '{id:"minecraft:stone",count:1,components:{"minecraft:block_transformer":"minecraft:axe"}}',
    '{id:"minecraft:stone",count:1,components:{"minecraft:minimum_attack_charge":0.5f}}',
    '{id:"minecraft:stone",count:1,components:{"minecraft:potion_duration_scale":2.0f}}',
    '{id:"minecraft:stone",count:1,components:{"minecraft:minimum_attack_charge":2.0f}}',
    '{id:"minecraft:stone",count:1,components:{"minecraft:additional_trade_cost":1}}',
    '{id:"minecraft:stone",count:1,components:{"minecraft:map_post_processing":"lock"}}',
    '{id:"minecraft:stone",count:1,components:{"minecraft:enchantments":{"minecraft:nope":1}}}',
    '{id:"minecraft:stone",count:1,components:{"minecraft:lore":[1,2,3]}}',
    '{id:"minecraft:stone",count:1,components:{"minecraft:custom_name":1}}',
]
for s in STACKS:
    b.add("stack-snbt", "-", s)
    b.add("ostack-snbt", "-", s)
    b.add("stack-json", "-", js({"id": "minecraft:stone", "count": 1}) if False else s.replace(":{", ":{").replace("'", '"') if False else s)
for _ in range(15):
    s = R.choice(STACKS)
    b.add("stack-snbt", "-", s.replace("count:1", "count:" + str(R.choice([0, 1, 2, 64, 99, 100, -1]))))
b.save()
