#!/usr/bin/env python3
"""Generates corpus/60-entity.txt (entity data, mob variants, profile). Seeded: output is reproducible.
Run: python3 gen_corpus_entity.py  (from this directory); then gen.sh corpus/60-entity.txt."""
import json, random, os

rng = random.Random(60)
HERE = os.path.dirname(os.path.abspath(__file__))
lines = []


def add(kind, name, payload):
    assert "\t" not in payload and "\n" not in payload, payload
    lines.append(f"{kind}\t{name}\t{payload}")


def js(v):
    return json.dumps(v, separators=(",", ":"), ensure_ascii=False)


def sq(s):
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n").replace("\t", "\\t").replace("\r", "\\r") + '"'


def snbt(v, wild=True):
    """JSON-ish python value -> SNBT with randomly chosen numeric suffixes."""
    if isinstance(v, bool):
        return rng.choice(["true", "false"] if not v else ["true", "1b"]) if wild else ("true" if v else "false")
    if isinstance(v, int):
        if not wild:
            return str(v)
        s = rng.choice(["", "", "b", "s", "L", "I"])
        if s == "b" and not -128 <= v <= 127:
            s = ""
        if s == "s" and not -32768 <= v <= 32767:
            s = ""
        return f"{v}{'' if s == 'I' else s}"
    if isinstance(v, float):
        return rng.choice([f"{v}", f"{v}f", f"{v}d"]) if wild else f"{v}d"
    if isinstance(v, str):
        return sq(v)
    if v is None:
        return "[]"
    if isinstance(v, list):
        if v and all(isinstance(x, int) and not isinstance(x, bool) for x in v) and wild and rng.random() < 0.4:
            return "[I;" + ",".join(str(x) for x in v) + "]"
        return "[" + ",".join(snbt(x, wild) for x in v) + "]"
    if isinstance(v, dict):
        return "{" + ",".join(f"{sq(k)}:{snbt(x, wild)}" for k, x in v.items()) + "}"
    raise TypeError(v)


def both(name, v):
    """a JSON value tested through the json and snbt kinds"""
    add("json", name, js(v))
    add("snbt", name, snbt(v))


SCALARS = [None, True, False, 0, 1, -1, 2, 99, 100, 127, 128, 255, 256, 2147483647, -2147483648, 4294967296,
           1.5, 0.0, -0.0, 1e10, "", " ", "x", "0", "minecraft:", ":", "A", "minecraft:Plains", "mine craft:a",
           [], [1], ["a"], [[]], [None], {}, {"a": 1}, {"id": "minecraft:pig"}, [1, 2, 3, 4], ["minecraft:plains"]]

# ---------------------------------------------------------------- registry variants --------------------------
HOLDERS = {
    "villager/variant": ["plains", "desert", "jungle", "savanna", "snow", "swamp", "taiga"],
    "wolf/variant": ["ashen", "black", "chestnut", "pale", "rusty", "snowy", "spotted", "striped", "woods"],
    "wolf/sound_variant": ["angry", "big", "classic", "cute", "grumpy", "puglin", "sad"],
    "pig/variant": ["cold", "temperate", "warm"],
    "pig/sound_variant": ["big", "classic", "mini"],
    "cow/variant": ["cold", "temperate", "warm"],
    "cow/sound_variant": ["classic", "moody"],
    "chicken/variant": ["cold", "temperate", "warm"],
    "chicken/sound_variant": ["classic", "picky"],
    "zombie_nautilus/variant": ["temperate", "warm"],
    "frog/variant": ["cold", "temperate", "warm"],
    "painting/variant": ["alban", "aztec", "kebab", "pointer", "wasteland", "fighters", "earth", "skull_and_roses"],
    "cat/variant": ["all_black", "black", "british_shorthair", "calico", "jellie", "persian", "ragdoll", "red", "siamese", "tabby", "white"],
    "cat/sound_variant": ["classic", "royal"],
}
for comp, names in HOLDERS.items():
    n = "minecraft:" + comp
    for x in names[:6]:
        both(n, "minecraft:" + x)
    for x in names[-3:]:
        both(n, x)  # default namespace
    for bad in ["minecraft:nope", "nope", "", "#minecraft:x", "Minecraft:" + names[0], "minecraft:" + names[0].upper(),
                "minecraft:" + names[0] + " ", "other:" + names[0], "minecraft:" + names[0] + ":x"]:
        both(n, bad)
    for s in rng.sample(SCALARS, 14):
        both(n, s)
    add("snbt", n, '{"id":"minecraft:' + names[0] + '"}')
    add("snbt", n, "'minecraft:" + names[0] + "'")
    add("snbt", n, "minecraft:" + names[0])
    add("snbt", n, "1b")
    add("json", n, '{"asset_id":"minecraft:x","width":1,"height":1}')  # inline definitions are not allowed (fixed codec)
    add("json", n, 'minecraft:' + names[0])  # not JSON
    add("json", n, '"minecraft:' + names[0] + '"  ')
    add("json", n, '["minecraft:' + names[0] + '"]')
    # wire: ids in range, boundary, out of range, bad varints
    count = len(names)
    for i in list(range(0, count + 2)) + [127]:
        add("wire", n, f"{i:02x}")
    for h in ["", "80", "8001", "ffffffff0f", "ffffffffff", "80808080808080", "7f00", "00ff", "ff01", "e807", "0100",
              "ffffffff07", "8080808008"]:
        add("wire", n, h)

# direct painting variants only exist on the wire (ByteBufCodecs.holder)
add("wire", "minecraft:painting/variant", "00" + "0102" + "0a" + "mc:x".encode().hex() + "00" + "00")
add("wire", "minecraft:painting/variant", "00" + "0102" + "0b" + "minecraft:x".encode().hex() + "00" + "00")
add("wire", "minecraft:painting/variant", "00" + "0102" + "0b" + "minecraft:x".encode().hex() + "01" + "0800" + "0161" + "00")
add("wire", "minecraft:painting/variant", "00" + "0102" + "0b" + "minecraft:x".encode().hex() + "01" + "0a00" + "00")
add("wire", "minecraft:painting/variant", "00" + "0000" + "0b" + "minecraft:x".encode().hex() + "0000")
add("wire", "minecraft:painting/variant", "00" + "ffffffff0f" + "01" + "78" + "00" + "00")
add("wire", "minecraft:painting/variant", "00" + "0101" + "01" + "78" + "01" + "0800026869" + "01" + "0800026869")
add("wire", "minecraft:painting/variant", "0001")
add("wire", "minecraft:painting/variant", "00")

# ---------------------------------------------------------------- enums --------------------------------------------
ENUMS = {
    "fox/variant": (["red", "snow"], "zero"),
    "salmon/size": (["small", "medium", "large"], "clamp"),
    "parrot/variant": (["red_blue", "blue", "green", "yellow_blue", "gray"], "clamp"),
    "tropical_fish/pattern": (["kob", "sunstreak", "snooper", "dasher", "brinely", "spotty", "flopper", "stripey", "glitter",
                               "blockfish", "betty", "clayfish"], "sparse"),
    "mooshroom/variant": (["red", "brown"], "clamp"),
    "rabbit/variant": (["brown", "white", "black", "white_splotched", "gold", "salt", "evil"], "sparse"),
    "horse/variant": (["white", "creamy", "chestnut", "brown", "black", "gray", "dark_brown"], "wrap"),
    "llama/variant": (["creamy", "white", "brown", "gray"], "clamp"),
    "axolotl/variant": (["lucy", "wild", "gold", "cyan", "blue"], "zero"),
}


def varint(n):
    n &= 0xFFFFFFFF
    out = b""
    while True:
        b = n & 0x7F
        n >>= 7
        if n:
            out += bytes([b | 0x80])
        else:
            return (out + bytes([b])).hex()


for comp, (names, strat) in ENUMS.items():
    n = "minecraft:" + comp
    for x in names:
        both(n, x)
    for bad in ["", "nope", "minecraft:" + names[0], names[0].upper(), " " + names[0], names[0] + " ", names[0] + "\n",
                "-".join(names[:1]), "0", "1", "Red", "minecraft:red"]:
        both(n, bad)
    for s in rng.sample(SCALARS, 14):
        both(n, s)
    add("snbt", n, names[0])  # unquoted SNBT string
    add("snbt", n, "'" + names[-1] + "'")
    add("snbt", n, "{}")
    add("snbt", n, "0b")
    add("snbt", n, "[I;0]")
    add("json", n, '"' + names[0] + '" ')
    add("json", n, "'" + names[0] + "'")
    # wire
    for i in [0, 1, 2, 3, 4, 5, 6, 7, 8, 12, 99, 100, 256, 257, 512, 768, 1280, 1281, -1, -7, 2**31 - 1, -2**31]:
        add("wire", n, varint(i))
    for h in ["", "80", "8001", "ffffffffff", "808080808001", "0000", "ff01"]:
        add("wire", n, h)

# ---------------------------------------------------------------- typed entity data -------------------------------
ENTITIES = ["minecraft:pig", "minecraft:zombie", "minecraft:cow", "minecraft:armor_stand", "minecraft:item", "minecraft:player",
            "minecraft:villager", "minecraft:creeper", "minecraft:wolf", "minecraft:painting", "minecraft:marker", "minecraft:area_effect_cloud",
            "pig", "zombie", "minecraft:fishing_bobber", "minecraft:lightning_bolt", "minecraft:ender_dragon"]
BLOCK_ENTITIES = ["minecraft:chest", "minecraft:furnace", "minecraft:sign", "minecraft:hanging_sign", "minecraft:banner", "minecraft:beehive",
                  "minecraft:mob_spawner", "minecraft:skull", "minecraft:command_block", "minecraft:barrel", "minecraft:bed", "minecraft:shulker_box",
                  "minecraft:jukebox", "minecraft:lectern", "minecraft:decorated_pot", "minecraft:trial_spawner", "chest", "minecraft:vault"]
EXTRAS = [
    {}, {"Health": 5.5}, {"IsBaby": True}, {"Pos": [1.0, 2.0, 3.0]}, {"Tags": ["a", "b"]}, {"CustomName": "\"x\""},
    {"nested": {"a": {"b": {"c": [1, 2, 3]}}}}, {"Items": [{"Slot": 0, "id": "minecraft:stone", "count": 1}]},
    {"x": 1, "y": 2, "z": 3}, {"id": "minecraft:nested"}, {"Lock": ""}, {"": 1}, {"a b": "c"}, {"unicode": "äöü\U0001F600"},
    {"big": 2147483647, "neg": -2147483648, "long": 4294967296}, {"empty": [], "empty2": {}}, {"list": [[1], [2, 3]]},
    {"CustomData": {"k": 1}}, {"uuid": [1, 2, 3, 4]}, {"t": True, "f": False},
]


def typed(name, ids):
    n = "minecraft:" + name
    for i in ids:
        for e in rng.sample(EXTRAS, 4):
            v = {"id": i, **e}
            if rng.random() < 0.5:
                both(n, v)
            else:
                add("json", n, js(v))
    for i in ids:
        add("json", n, js({"id": i}))
        add("snbt", n, "{id:" + sq(i) + "}")
        add("json", n, js({"id": i, "x": 1}))
        add("json", n, js({"x": 1, "id": i}))
        add("json", n, sq("{id:" + sq(i) + "}")[0:0] + js("{id:" + js(i) + "}"))  # flattened SNBT form (string)
        add("json", n, js("{id:" + js(i) + ",Health:5.0f}"))
        add("snbt", n, sq("{id:" + js(i) + "}"))
        add("json", n, js({"id": i, "id2": i}))
    bad_ids = ["", "nope", "minecraft:nope", "Minecraft:pig", "#minecraft:pig", " minecraft:pig", "minecraft:pig ", 0, 1, True, None, 1.5,
               [], ["minecraft:pig"], {}, {"id": "minecraft:pig"}, ["a", "b"], [1, 2], "minecraft:", ":"]
    for b in bad_ids:
        both(n, {"id": b})
        both(n, {"id": b, "x": 1})
    for s in SCALARS:
        both(n, s)
    both(n, {})
    both(n, {"x": 1})
    both(n, {"Id": ids[0]})
    both(n, {"ID": ids[0]})
    both(n, [ids[0]])
    both(n, [{"id": ids[0]}])
    add("snbt", n, "{id:1b}")
    add("snbt", n, "{id:" + ids[0].replace("minecraft:", "") + "}")  # unquoted, no namespace
    add("snbt", n, "{id:'" + ids[0] + "',a:[B;1,2,3],b:[L;1L],c:[I;1],d:1.5e3,e:'x',f:\"y\"}")
    add("snbt", n, "{id:'" + ids[0] + "',l:[1,2,3]}")
    add("snbt", n, "{id:'" + ids[0] + "',l:[1,'a']}")  # heterogeneous list
    add("snbt", n, "{id:'" + ids[0] + "',nan:NaNd,inf:Infinityf,ninf:-Infinityd}")
    add("snbt", n, "{id:'" + ids[0] + "',z:-0.0}")
    add("snbt", n, "{id:'" + ids[0] + "',t:true,f:false}")
    add("snbt", n, "{id:'" + ids[0] + "',big:9223372036854775807L,small:-9223372036854775808L}")
    add("snbt", n, "{id:'" + ids[0] + "',over:99999999999999999999}")
    add("snbt", n, "{id:'" + ids[0] + "',str:\"a\\nb\\tc\\u00e4\"}")
    add("snbt", n, "{id:'" + ids[0] + "',}")
    add("snbt", n, "{id:'" + ids[0] + "'")
    add("snbt", n, "[{id:'" + ids[0] + "'}]")
    add("snbt", n, "'not snbt'")
    add("snbt", n, "''")
    add("snbt", n, "\"{}\"")
    add("snbt", n, "\"{id:'" + ids[0] + "'}\"")
    add("snbt", n, "\"[1]\"")
    add("snbt", n, "\"{id:\"")
    add("json", n, js("{id:\"" + ids[0] + "\""))
    add("json", n, js("[id]"))
    add("json", n, js(""))
    add("json", n, js("{}"))
    # wire
    for i in [0, 1, 5, 50, 127, 128, 300, 5000, 100000, -1, 2**31 - 1]:
        add("wire", n, varint(i) + "0a00")
        add("wire", n, varint(i) + "0a0800026964000a" + "00")
    for i in [0, 1, 3, 7, 20, 30]:
        # id key inside the data is stripped
        key = "0a" + "0800" + "02" + "6964" + "000b" + ("minecraft:pig".encode().hex()) + "00"
        add("wire", n, varint(i) + key)
        add("wire", n, varint(i) + "0a" + "0800026964000d" + ("minecraft:pig!!!".encode()[:13]).hex() + "00")
        add("wire", n, varint(i) + "0a" + "0300017800000001" + "00")  # x:int=1
        add("wire", n, varint(i) + "0a" + "0300026964000000070300017800000001" + "00")  # id:int=7 and x
        add("wire", n, varint(i) + "0a" + "0a000100" + "00" + "00")  # nested compound
        add("wire", n, varint(i) + "0a" + "09000161" + "0100000002" + "0102" + "00")
    for h in ["", "00", "0a", "000a", "0000", "00 08".replace(" ", ""), "00" + "09000100", "0007", "00ff", "00" + "0a" + "ff", "00" + "0b00",
              "000a0000", "0a00", "00" + "0a" + "0800" + "0100", "00" + "0a" + "0100" + "00", "00" + "08" + "0001" + "61",
              "00" + "0a" + "0800026964" + "ffff"]:
        add("wire", n, h)


typed("entity_data", ENTITIES)
typed("block_entity_data", BLOCK_ENTITIES)

# ---------------------------------------------------------------- profile ------------------------------------------
UU = [[1, 2, 3, 4], [0, 0, 0, 0], [-1, -1, -1, -1], [2147483647, -2147483648, 0, 1], [305419896, -1432778632, 19088743, 2023406814]]
PROP = [{"name": "textures", "value": "eyJ0ZXh0dXJlcyI6e319"}, {"name": "textures", "value": "abc", "signature": "sig"},
        {"name": "a", "value": ""}, {"name": "", "value": "x"}, {"name": "n" * 64, "value": "v"}, {"name": "n" * 65, "value": "v"},
        {"name": "n", "value": "v", "signature": "s" * 1024},
        {"name": "n", "value": "v", "signature": "s" * 1025}, {"name": "n", "value": "v", "signature": ""},
        {"name": "n"}, {"value": "v"}, {"name": 1, "value": "v"}, {"name": "n", "value": 1}, {"name": "n", "value": "v", "signature": 1},
        {"name": "n", "value": "v", "extra": 1}, {"name": "\U0001F600" * 32, "value": "v"}, {"name": "\U0001F600" * 33, "value": "v"}]
NAMES = ["Steve", "Alex", "a", "", "A" * 16, "A" * 17, "Name With Space", "name!", "n\u00e4me", "\U0001F600", "x" * 16, "\U0001F600" * 8,
         "\U0001F600" * 9, "~~~", "a~b", "\x7f", "a b", "\t", "ab\u00a0", "Notch", "jeb_", "Dinnerbone", "0", "_", "\u00e9" * 16]
TEX = ["minecraft:entity/player/wide/steve", "steve", "a:b/c", "Bad Id", "", "minecraft:", "minecraft:a/../b", "a:b:c", "a", "x" * 300]
PROFILES = []
for nm in NAMES:
    PROFILES.append(nm)
    PROFILES.append({"name": nm})
    PROFILES.append({"name": nm, "id": UU[0]})
    PROFILES.append({"id": UU[1], "name": nm})
    PROFILES.append({"id": UU[2], "name": nm, "properties": [PROP[0]]})
for u in UU + [[1, 2, 3], [1, 2, 3, 4, 5], [], ["a", "b", "c", "d"], [1.5, 2, 3, 4], "00000000-0000-0000-0000-000000000001", 5, None, {}]:
    PROFILES.append({"id": u})
    PROFILES.append({"id": u, "name": "Steve"})
    PROFILES.append({"id": u, "properties": [PROP[0]]})
    PROFILES.append({"id": u, "name": "Steve", "properties": {"textures": ["a", "b"]}})
for p in PROP:
    PROFILES.append({"name": "Steve", "properties": [p]})
    PROFILES.append({"id": UU[0], "name": "Steve", "properties": [p, PROP[1]]})
    PROFILES.append({"id": UU[0], "properties": [p]})
PROFILES += [
    {}, {"properties": []}, {"properties": {}}, {"properties": [PROP[0]]}, {"properties": {"a": ["x"]}}, {"properties": {"a": ["x", "y"], "b": []}},
    {"properties": {"a": "x"}}, {"properties": {"a": [1]}}, {"properties": {"a": [None]}}, {"properties": {"a": {"b": "c"}}}, {"properties": None},
    {"properties": [None]}, {"properties": 1}, {"properties": "x"}, {"properties": {str(i): ["v"] for i in range(16)}},
    {"properties": {str(i): ["v"] for i in range(17)}}, {"properties": [{"name": str(i % 3), "value": "v" + str(i)} for i in range(16)]},
    {"properties": [{"name": str(i % 3), "value": "v" + str(i)} for i in range(17)]}, {"properties": [{"name": "a", "value": "1"}, {"name": "b", "value": "2"}, {"name": "a", "value": "3"}]},
    {"id": UU[0], "name": "Steve", "properties": [{"name": "a", "value": "1"}, {"name": "b", "value": "2"}, {"name": "a", "value": "3"}]},
    {"name": "Steve", "properties": {"b": ["1"], "a": ["2", "3"]}}, {"properties": {"a": []}}, {"name": "Steve", "properties": {"a": []}},
    {"id": UU[0], "name": "Steve", "properties": {"a": []}}, {"id": UU[0], "name": "Steve", "properties": []},
    {"name": "Steve", "properties": [], "texture": "a"}, {"unknown": 1}, {"name": "Steve", "unknown": 1}, {"Name": "Steve"}, {"NAME": "Steve"},
    {"name": "Steve", "name2": "x"},
    {"texture": "minecraft:a"}, {"cape": "minecraft:a"}, {"elytra": "minecraft:a"}, {"model": "slim"}, {"model": "wide"}, {"model": "default"},
    {"model": "Slim"}, {"model": ""}, {"model": 1}, {"model": True}, {"model": None}, {"model": []},
    {"texture": "a", "cape": "b", "elytra": "c", "model": "slim"}, {"name": "Steve", "texture": "a", "cape": "b", "elytra": "c", "model": "wide"},
    {"id": UU[0], "name": "Steve", "texture": "a", "model": "slim"}, {"id": UU[0], "texture": "a"}, {"texture": 1}, {"cape": []}, {"elytra": {}},
    {"texture": None}, {"name": "Steve", "texture": None},
]
for t in TEX:
    PROFILES.append({"name": "Steve", "texture": t})
    PROFILES.append({"cape": t})
    PROFILES.append({"id": UU[0], "name": "Steve", "elytra": t})
for s in SCALARS:
    PROFILES.append(s)
    PROFILES.append({"name": s})
    PROFILES.append({"id": s, "name": "Steve"})
    PROFILES.append({"name": "Steve", "properties": s})
    PROFILES.append({"name": "Steve", "model": s})
    PROFILES.append({"name": "Steve", "texture": s})
    PROFILES.append([s])
for p in PROFILES:
    both("minecraft:profile", p)
# random mutations of profiles
SEEDS = [{"id": UU[0], "name": "Steve", "properties": [PROP[1]], "texture": "a", "model": "slim"},
         {"name": "Steve", "properties": {"a": ["x"]}}, {"id": UU[3]}, {"name": "Steve", "id": UU[4], "cape": "b:c"}]


def mutate(v, depth=0):
    r = rng.random()
    if isinstance(v, dict) and v:
        k = rng.choice(list(v))
        if r < 0.2:
            return {kk: vv for kk, vv in v.items() if kk != k}
        if r < 0.4:
            return {**v, k: rng.choice(SCALARS)}
        if r < 0.5:
            return {**v, rng.choice(["name", "id", "properties", "texture", "model", "cape", "elytra", "x"]): rng.choice(SCALARS)}
        if r < 0.6:
            return {kk: (mutate(vv, depth + 1) if kk == k else vv) for kk, vv in v.items()}
        if r < 0.7:
            return {**v, k.upper(): v[k]}
        return {kk: (mutate(vv, depth + 1) if rng.random() < 0.5 else vv) for kk, vv in v.items()}
    if isinstance(v, list) and v:
        i = rng.randrange(len(v))
        if r < 0.3:
            return v[:i] + v[i + 1:]
        if r < 0.5:
            return v + [rng.choice(v)]
        if r < 0.7:
            return v[:i] + [rng.choice(SCALARS)] + v[i + 1:]
        return [mutate(x, depth + 1) if rng.random() < 0.5 else x for x in v]
    if isinstance(v, str):
        if r < 0.3:
            return v + "x"
        if r < 0.5:
            return v[:-1]
        if r < 0.7:
            return v * 3
        return rng.choice(SCALARS)
    if isinstance(v, int) and not isinstance(v, bool):
        return rng.choice([v + 1, v - 1, -v, 0, 2147483647, -2147483648, 4294967296, 1.5, str(v)])
    return rng.choice(SCALARS)


for _ in range(110):
    both("minecraft:profile", mutate(rng.choice(SEEDS)))
for _ in range(50):
    both("minecraft:profile", mutate(mutate(rng.choice(SEEDS))))

add("json", "minecraft:profile", js({"name": "Steve", "properties": [{"name": "n", "value": "v" * 32767}]}))
add("json", "minecraft:profile", js({"name": "Steve", "properties": [{"name": "n", "value": "v" * 32768}]}))
add("snbt", "minecraft:profile", snbt({"name": "Steve", "properties": [{"name": "n", "value": "v" * 32768}]}))
# SNBT-only shapes of profiles
for s in ["{id:[I;1,2,3,4],name:'Steve'}", "{id:[1,2,3,4],name:'Steve'}", "{id:[L;1L,2L],name:'Steve'}", "{id:[B;1,2,3,4],name:'Steve'}",
          "{id:[I;1,2,3,4]}", "{id:[I;1,2,3,4],name:\"Steve\",properties:[{name:'a',value:'b'}]}", "{name:'Steve'}", "Steve", "'Steve'", "\"Steve\"",
          "{name:Steve}", "{name:1b}", "{name:'Steve',id:[I;1,2,3]}", "{properties:{a:['x']}}", "{properties:{a:'x'}}",
          "{name:'Steve',model:'slim',texture:'a:b'}", "{name:'Steve',model:0b}", "[I;1,2,3,4]", "[]", "{}", "1b", "1.5f", "true",
          "{id:[I;1,2,3,4],name:'Steve',properties:{}}", "{id:[I;1,2,3,4],name:'Steve',properties:[]}",
          "{id:[I;1,2,3,4],name:'Steve',properties:[{name:'a',value:'b',signature:'c'}]}", "{name:'Steve',properties:[{name:'a',value:'b'}]}",
          "{properties:[{name:'a',value:'b'},{name:'c',value:'d'},{name:'a',value:'e'}]}", "{name:'a',id:[I;0,0,0,0]}",
          "{name:'a',id:[I;0,0,0,0],properties:{}}", "{name:'a',id:[I;0,0,0,0],properties:[]}", "{name:'a',properties:{x:['1']}}"]:
    add("snbt", "minecraft:profile", s)
for s in ['"Steve"', '{"name":"Steve"}', '"S"', '""', "'Steve'", '{"name":"Steve",}', '{"name":"Steve","name":"Alex"}', '{"id":[1,2,3,4],"id":[0,0,0,0]}',
          '{"properties":{"a":["x"],"a":["y"]}}', '{"id":[1,2,3,4],"name":"x","properties":{"a":["1","2"],"b":["3"]}}', "nul", "{", "[", '"Steve', "Steve"]:
    add("json", "minecraft:profile", s)
# wire
STR = lambda s: varint(len(s.encode())) + s.encode().hex()
UUB = lambda n: n.to_bytes(16, "big").hex()
OPT = lambda present, body="": ("01" + body) if present else "00"
SKIN0 = "00000000"
wire_profiles = []
# left = full GameProfile: bool 1, uuid, name, props
for nm in ["Steve", "", "A" * 16, "A" * 17, "\u00e4" * 16, "\U0001F600" * 8]:
    for props in ["00", "01" + STR("a") + STR("b") + "00", "01" + STR("a") + STR("b") + "01" + STR("c"),
                  "03" + STR("a") + STR("1") + "00" + STR("b") + STR("2") + "00" + STR("a") + STR("3") + "00", "11" + ("00" * 3) * 0]:
        wire_profiles.append("01" + UUB(rng.getrandbits(128)) + STR(nm) + props)
# right = partial: bool 0, optional name, optional uuid, props
for nm in [None, "Steve", "", "A" * 16, "A" * 17]:
    for u in [None, 1, 2 ** 128 - 1]:
        for props in ["00", "01" + STR("a") + STR("b") + "00", "02" + STR("a") + STR("b") + "00" + STR("a") + STR("c") + "00"]:
            body = "00" + OPT(nm is not None, STR(nm) if nm is not None else "") + OPT(u is not None, UUB(u) if u is not None else "") + props
            wire_profiles.append(body)
skins = [SKIN0, "01" + STR("a:b") + "000000", "00" + "01" + STR("a:b") + "0000", "000001" + STR("x") + "00", "00000001" + "00", "00000001" + "01",
         "00000001" + "02", "00000001" + "ff", "01" + STR("a:b") + "01" + STR("c:d") + "01" + STR("e:f") + "0101", "02" + "00" * 3, "01" + STR("A:B") + "000000"]
for w in wire_profiles:
    for s in rng.sample(skins, 1):
        add("wire", "minecraft:profile", w + s)
    add("wire", "minecraft:profile", w + SKIN0)
for h in ["", "00", "01", "0000000000", "02", "ff", "01" + "00" * 16, "01" + "00" * 16 + "00", "00" + "01" + STR("x"), "00" + "00" + "00" + "11"]:
    add("wire", "minecraft:profile", h)
add("wire", "minecraft:profile", "00" + "00" + "00" + varint(17) + "00000000")
add("wire", "minecraft:profile", "00" + "00" + "00" + varint(-1) + "00000000")
add("wire", "minecraft:profile", "00" + "00" + "00" + varint(16) + "00000000")
add("wire", "minecraft:profile", "00" + "00" + "00" + "01" + STR("n" * 64) + STR("v") + "00" + SKIN0)
add("wire", "minecraft:profile", "00" + "00" + "00" + "01" + STR("n" * 65) + STR("v") + "00" + SKIN0)
add("wire", "minecraft:profile", "00" + "00" + "00" + "01" + STR("n") + STR("v" * 32768) + "00" + SKIN0)
add("wire", "minecraft:profile", "00" + "00" + "00" + "01" + STR("n") + STR("v") + "01" + STR("s" * 1024) + SKIN0)
add("wire", "minecraft:profile", "00" + "00" + "00" + "01" + STR("n") + STR("v") + "01" + STR("s" * 1025) + SKIN0)

# ---------------------------------------------------------------- stacks -------------------------------------------
def stack(item, comps, count=1):
    return "{id:" + sq(item) + ",count:" + str(count) + ",components:{" + ",".join(f"{sq('minecraft:' + k)}:{v}" for k, v in comps) + "}}"


for comp, names in HOLDERS.items():
    add("stack-snbt", "-", stack("minecraft:stone", [(comp, sq("minecraft:" + names[0]))]))
    add("stack-snbt", "-", stack("minecraft:stone", [(comp, sq("minecraft:nope"))]))
    add("stack-snbt", "-", stack("minecraft:stone", [("!" + comp, "{}")]) if False else stack("minecraft:stone", [(comp, "1")]))
for comp, (names, _) in ENUMS.items():
    add("stack-snbt", "-", stack("minecraft:stone", [(comp, sq(names[-1]))]))
    add("stack-snbt", "-", stack("minecraft:stone", [(comp, sq("bad"))]))
    add("stack-snbt", "-", stack("minecraft:stone", [(comp, "0")]))
add("stack-snbt", "-", stack("minecraft:pig_spawn_egg", [("entity_data", "{id:'minecraft:pig',Health:3.0f}")]))
add("stack-snbt", "-", stack("minecraft:wolf_spawn_egg", [("entity_data", "{id:'minecraft:wolf'}"), ("wolf/variant", "'minecraft:pale'")]))
add("stack-snbt", "-", stack("minecraft:chest", [("block_entity_data", "{id:'minecraft:chest',Items:[]}")]))
add("stack-snbt", "-", stack("minecraft:chest", [("block_entity_data", "{id:'minecraft:furnace',BurnTime:5s}")]))
add("stack-snbt", "-", stack("minecraft:chest", [("block_entity_data", "{Items:[]}")]))
add("stack-snbt", "-", stack("minecraft:player_head", [("profile", "'Notch'")]))
add("stack-snbt", "-", stack("minecraft:player_head", [("profile", "{name:'Notch',properties:[{name:'textures',value:'abc'}]}")]))
add("stack-snbt", "-", stack("minecraft:player_head", [("profile", "{id:[I;1,2,3,4],name:'Notch'}")]))
add("stack-snbt", "-", stack("minecraft:player_head", [("profile", "{id:[I;1,2,3,4]}")]))
add("stack-snbt", "-", stack("minecraft:player_head", [("profile", "{name:'Notch',model:'slim'}")]))
add("stack-snbt", "-", stack("minecraft:player_head", [("profile", "{name:'bad name!'}")]))
add("stack-snbt", "-", stack("minecraft:zombie_head", [("profile", "{}")]))
add("stack-snbt", "-", stack("minecraft:painting", [("painting/variant", "'minecraft:aztec'"), ("entity_data", "{id:'minecraft:painting'}")]))
add("stack-snbt", "-", stack("minecraft:axolotl_bucket", [("axolotl/variant", "'blue'"), ("bucket_entity_data", "{Variant:4}")]))
add("stack-snbt", "-", stack("minecraft:tropical_fish_bucket", [("tropical_fish/pattern", "'clayfish'"), ("tropical_fish/base_color", "'red'")]))
add("stack-snbt", "-", stack("minecraft:salmon_bucket", [("salmon/size", "'large'")]))
add("stack-snbt", "-", stack("minecraft:stone", [("fox/variant", "'snow'"), ("rabbit/variant", "'evil'"), ("horse/variant", "'dark_brown'"), ("llama/variant", "'gray'")]))
add("stack-snbt", "-", stack("minecraft:stone", [("parrot/variant", "'gray'"), ("mooshroom/variant", "'brown'"), ("villager/variant", "'taiga'")]))
add("stack-snbt", "-", stack("minecraft:stone", [("cat/variant", "'tabby'"), ("cat/sound_variant", "'royal'"), ("frog/variant", "'warm'"), ("pig/sound_variant", "'mini'")]))
add("stack-snbt", "-", stack("minecraft:stone", [("cow/variant", "'cold'"), ("cow/sound_variant", "'moody'"), ("chicken/variant", "'warm'"), ("chicken/sound_variant", "'picky'"), ("zombie_nautilus/variant", "'warm'"), ("wolf/sound_variant", "'cute'")]))
add("patch-snbt", "-", "{\"minecraft:fox/variant\":'snow',\"!minecraft:profile\":{},\"minecraft:entity_data\":{id:'minecraft:pig'}}")
add("patch-snbt", "-", "{\"!minecraft:entity_data\":{},\"!minecraft:villager/variant\":{}}")
add("patch-snbt", "-", "{\"minecraft:profile\":{name:'x'},\"minecraft:pig/variant\":'warm'}")
add("patch-json", "-", '{"minecraft:rabbit/variant":"evil","!minecraft:painting/variant":{}}')

with open(os.path.join(HERE, "corpus", "60-entity.txt"), "w") as f:
    f.write("\n".join(lines) + "\n")
print(len(lines), "lines")
