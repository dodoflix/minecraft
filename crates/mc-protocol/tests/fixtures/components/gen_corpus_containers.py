#!/usr/bin/env python3
"""Generates corpus/40-containers.txt (container, bundle_contents, charged_projectiles, container_loot, block_state,
bees, sulfur_cube_content, debug_stick_state, recipes, lodestone_tracker, map_decorations, lock).
Run from this directory: ./gen_corpus_containers.py && ./gen.sh corpus/40-containers.txt"""
import json
import random
import corpus_lib as L

P = "minecraft:"


def item(id_, count=None, comps=None):
    d = {"id": P + id_}
    if count is not None:
        d["count"] = count
    if comps is not None:
        d["components"] = comps
    return d


def slot(i, it):
    return {"slot": i, "item": it}


stone, apple = item("stone"), item("apple", 3)
dmg = item("diamond_sword", 1, {P + "damage": 5})
inner = [slot(0, item("stone", 2))]
nested_container = item("shulker_box", 1, {P + "container": inner})
nested_bundle = item("bundle", 1, {P + "bundle_contents": [item("stone", 8)]})
stack_override = lambda n: item("stone", 1, {P + "max_stack_size": n})

container_bases = [
    [],
    [slot(0, stone)],
    [slot(0, "minecraft:stone")],
    [slot(5, apple), slot(2, dmg)],
    [slot(255, "minecraft:apple")],
    [slot(1, nested_container), slot(2, nested_bundle)],
    [slot(0, item("stone", 64)), slot(0, item("apple", 2))],
    [slot(3, item("stone", 1, {P + "max_stack_size": 16, P + "unbreakable": {}}))],
    [slot(i, "minecraft:stone") for i in range(0, 256, 17)],
    [slot(i, "minecraft:dirt") for i in range(8)],
]
bundle_bases = [
    [], ["minecraft:stone"], [item("stone", 64), item("apple", 3)], [nested_bundle, "minecraft:apple"],
    [item("diamond_sword", 1, {P + "damage": 1})], [stack_override(97), stack_override(89)],
    [item("bundle", 1, {P + "bundle_contents": [item("bundle", 1, {P + "bundle_contents": ["minecraft:stone"]})]})],
]
charged_bases = [[], ["minecraft:arrow"], [item("arrow", 3), item("firework_rocket", 1)], [item("arrow", 64, {})], ["minecraft:arrow"] * 5]
loot_bases = [{"loot_table": P + "chests/simple_dungeon"}, {"loot_table": P + "chests/end_city_treasure", "seed": 1234567890123},
              {"loot_table": "custom:a/b", "seed": 0}, {"loot_table": P + "x", "seed": -5}]
state_bases = [{}, {"facing": "north"}, {"facing": "north", "lit": "true", "honey_level": "3"}, {"a": "", "": "b"},
               {f"prop{i}": f"value{i}" for i in range(12)}, {f"k{i * 7}": str(i) for i in range(30)}, {"é": "ü", "中": "文"}]
sulfur_bases = ["minecraft:stone", item("stone", 4), item("oak_log", 1, {P + "max_stack_size": 2})]
debug_bases = [{}, {P + "oak_stairs": "facing"}, {P + "lever": "powered", P + "oak_stairs": "half"}, {P + "stone": "facing"},
               {P + "oak_fence": "north", P + "oak_door": "hinge", P + "repeater": "delay"}]
recipes_bases = [[], [P + "stick"], [P + "stick", "custom:thing/x", P + "stick"]]
lodestone_bases = [{"target": {"dimension": P + "overworld", "pos": [1, 2, 3]}, "tracked": False}, {"tracked": True},
                   {"target": {"dimension": P + "the_nether", "pos": [-30000000, 319, 29999999]}}, {}]
deco = lambda t, x, z, r: {"type": P + t, "x": x, "z": z, "rotation": r}
map_bases = [{}, {"a": deco("player", 1.5, 2, 90)}, {"banner_1": deco("red_x", -10.25, 100.0, 0.5), "b": deco("frame", 0, 0, 359.99)},
             {"": deco("target_x", 1e300, -1e-300, 3.4e38)}]
bee_entity = {"id": P + "bee", "Age": 0, "HasNectar": 0, "Pos": [1.0, 2.0, 3.0]}
bees_bases = [[], [{"entity_data": bee_entity, "ticks_in_hive": 5, "min_ticks_in_hive": 600}],
              [{"entity_data": {"id": P + "bee"}, "ticks_in_hive": 0, "min_ticks_in_hive": 2400}] * 2]

components = {
    P + "container": (container_bases, [slot(0, "minecraft:stone"), {"slot": 3}, {"item": "minecraft:stone"}]),
    P + "bundle_contents": (bundle_bases, [item("stone", 100), item("air")]),
    P + "charged_projectiles": (charged_bases, []),
    P + "container_loot": (loot_bases, []),
    P + "block_state": (state_bases, []),
    P + "sulfur_cube_content": (sulfur_bases, []),
    P + "debug_stick_state": (debug_bases, [P + "birch_stairs"]),
    P + "recipes": (recipes_bases, ["Not A Valid Id", "a:b:c"]),
    P + "lodestone_tracker": (lodestone_bases, [[1, 2, 3], [1, 2], {"dimension": "x", "pos": [1, 2, 3, 4]}]),
    P + "map_decorations": (map_bases, [deco("nope", 0, 0, 0)]),
    P + "bees": (bees_bases, ["id"]),
}

raw = []
def r(kind, name, payload):
    raw.append((kind, P + name, payload))

# container: limits, shorthand and rejections
r("json", "container", json.dumps([slot(i, "minecraft:stone") for i in range(256)], separators=(",", ":")))
r("json", "container", json.dumps([slot(0, "minecraft:stone")] * 257, separators=(",", ":")))
r("json", "container", json.dumps([slot(i % 256, "minecraft:stone") for i in range(256)], separators=(",", ":")))
r("json", "container", '[{"slot":256,"item":"minecraft:stone"}]')
r("json", "container", '[{"slot":-1,"item":"minecraft:stone"}]')
r("json", "container", '[{"slot":0,"item":"minecraft:air"}]')
r("json", "container", '[{"slot":0,"item":{"id":"minecraft:stone","count":0}}]')
r("json", "container", '[{"slot":0,"item":{"id":"minecraft:stone","count":100}}]')
r("json", "container", '[{"slot":0,"item":{"id":"minecraft:stone","count":99}}]')
r("json", "container", '[{"slot":0.0,"item":"minecraft:stone"}]')
r("json", "container", '[{"slot":1,"slot":2,"item":"minecraft:stone"}]')
r("json", "container", '{"slot":0,"item":"minecraft:stone"}')
r("json", "container", 'null')
r("json", "container", '"minecraft:stone"')
r("snbt", "container", '[{slot:0b,item:"minecraft:stone"}]')
r("snbt", "container", '[{slot:1L,item:"minecraft:stone"}]')
r("snbt", "container", '[{slot:1.0f,item:{id:"minecraft:stone",count:2b}}]')
r("snbt", "container", '[{slot:1,item:{id:"minecraft:stone",count:2L}}]')
r("snbt", "container", '[I;1,2]')
r("snbt", "container", '[{slot:3,item:{id:"minecraft:stone",components:{"minecraft:damage":1}}}]')
r("snbt", "container", '[{slot:3,item:{id:"minecraft:stone",components:{"!minecraft:max_stack_size":{}}}}]')
r("snbt", "container", '[{slot:3,item:{id:"minecraft:stone",components:{"minecraft:max_stack_size":1}}}]')
r("snbt", "container", '[{slot:3,item:{id:"minecraft:stone",components:{"minecraft:nope":1}}}]')
# bundle / charged
r("json", "bundle_contents", '[{"id":"minecraft:stone","count":99}]')
r("json", "bundle_contents", '[{"id":"minecraft:stone","count":100}]')
r("json", "bundle_contents", '[{"id":"minecraft:stone","count":-1}]')
r("json", "bundle_contents", '["minecraft:air"]')
r("json", "bundle_contents", 'null')
r("json", "bundle_contents", '"minecraft:stone"')
r("json", "bundle_contents", json.dumps(["minecraft:stone"] * 300, separators=(",", ":")))
r("json", "charged_projectiles", json.dumps(["minecraft:arrow"] * 1024, separators=(",", ":")))
r("json", "charged_projectiles", json.dumps(["minecraft:arrow"] * 1025, separators=(",", ":")))
r("snbt", "charged_projectiles", '[{id:"minecraft:arrow",count:1b}]')
r("snbt", "charged_projectiles", '["minecraft:arrow",{id:"minecraft:arrow"}]')
# block_state
r("json", "block_state", '{"a":1}')
r("json", "block_state", '{"a":"b","a":"c"}')
r("json", "block_state", '[]')
r("json", "block_state", '{"a":null}')
r("snbt", "block_state", '{a:"b"}')
r("snbt", "block_state", '{a:1b}')
r("snbt", "block_state", '{"":""}')
# container_loot
r("json", "container_loot", '{"loot_table":"minecraft:x","seed":1.5}')
r("json", "container_loot", '{"loot_table":"minecraft:x","seed":9223372036854775807}')
r("json", "container_loot", '{"loot_table":"minecraft:x","seed":9223372036854775808}')
r("json", "container_loot", '{"seed":1}')
r("json", "container_loot", '{"loot_table":"Bad Id"}')
r("json", "container_loot", '{"loot_table":""}')
r("snbt", "container_loot", '{loot_table:"minecraft:x",seed:5b}')
r("snbt", "container_loot", '{loot_table:"minecraft:x",seed:1.9d}')
r("snbt", "container_loot", '{loot_table:"minecraft:x",seed:-1L}')
# lodestone
r("json", "lodestone_tracker", '{"target":{"dimension":"minecraft:overworld","pos":[1,2,3]},"tracked":"yes"}')
r("json", "lodestone_tracker", '{"target":null}')
r("json", "lodestone_tracker", '{"tracked":0}')
r("json", "lodestone_tracker", '{"tracked":1}')
r("snbt", "lodestone_tracker", '{target:{dimension:"minecraft:overworld",pos:[I;1,2,3]},tracked:0b}')
r("snbt", "lodestone_tracker", '{target:{dimension:"minecraft:overworld",pos:[1,2,3]}}')
r("snbt", "lodestone_tracker", '{target:{dimension:"minecraft:overworld",pos:[1L,2L,3L]}}')
r("snbt", "lodestone_tracker", '{target:{dimension:"minecraft:overworld",pos:[B;1,2,3]}}')
r("snbt", "lodestone_tracker", '{target:{dimension:"minecraft:overworld",pos:[L;1,2,3]}}')
r("snbt", "lodestone_tracker", '{target:{dimension:"minecraft:overworld",pos:[1.5f,2,3]}}')
r("snbt", "lodestone_tracker", '{target:{dimension:"minecraft:overworld",pos:[1,2,3,4]}}')
# map decorations
r("snbt", "map_decorations", '{a:{type:"minecraft:player",x:1,z:2,rotation:3}}')
r("snbt", "map_decorations", '{a:{type:"minecraft:player",x:1b,z:2s,rotation:3L}}')
r("snbt", "map_decorations", '{a:{type:"minecraft:player",x:NaNd,z:Infinityd,rotation:-Infinityf}}')
r("json", "map_decorations", '{"a":{"type":"minecraft:player","x":1,"z":2}}')
r("json", "map_decorations", '{"a":{"type":"minecraft:player","x":"1","z":2,"rotation":0}}')
r("json", "map_decorations", '{"a":"minecraft:player"}')
r("json", "map_decorations", '[]')
# debug stick
r("json", "debug_stick_state", '{"minecraft:stone":"facing"}')
r("json", "debug_stick_state", '{"stone":"facing","minecraft:stone":"facing"}')
r("json", "debug_stick_state", '{"minecraft:oak_stairs":"facing","minecraft:oak_stairs":"half"}')
r("json", "debug_stick_state", '{"minecraft:oak_stairs":1}')
r("json", "debug_stick_state", '{"minecraft:nope":"facing"}')
# recipes
r("json", "recipes", '["minecraft:stick","minecraft:stick"]')
r("json", "recipes", '"minecraft:stick"')
r("json", "recipes", '[""]')
r("json", "recipes", '[1]')
# bees
r("json", "bees", '[{"entity_data":{"id":"minecraft:bee"},"ticks_in_hive":1,"min_ticks_in_hive":2}]')
r("json", "bees", '[{"entity_data":{"id":"minecraft:bee"},"ticks_in_hive":1}]')
r("json", "bees", '[{"entity_data":{},"ticks_in_hive":1,"min_ticks_in_hive":2}]')
r("json", "bees", '[{"entity_data":"{id:\\"minecraft:bee\\",Age:5}","ticks_in_hive":1,"min_ticks_in_hive":2}]')
r("json", "bees", '[{"entity_data":"{id:\\"minecraft:nope\\"}","ticks_in_hive":1,"min_ticks_in_hive":2}]')
r("json", "bees", '[{"entity_data":"not snbt","ticks_in_hive":1,"min_ticks_in_hive":2}]')
r("snbt", "bees", '[{entity_data:{id:"minecraft:bee",x:1b},ticks_in_hive:1,min_ticks_in_hive:2}]')
r("snbt", "bees", '[{entity_data:{id:5},ticks_in_hive:1,min_ticks_in_hive:2}]')
r("snbt", "bees", '[{entity_data:{id:"minecraft:bee"},ticks_in_hive:1L,min_ticks_in_hive:2b}]')
r("snbt", "bees", '[{entity_data:{id:"minecraft:bee",l:[1,2],c:{d:1.5f}},ticks_in_hive:1,min_ticks_in_hive:2}]')
# sulfur cube
r("json", "sulfur_cube_content", '"minecraft:air"')
r("json", "sulfur_cube_content", '{"id":"minecraft:stone","count":0}')
r("json", "sulfur_cube_content", '[]')


# ---- hand-built wire cases (ItemStackTemplate = item id, count, patch(added, removed)) ----------------------------------
tmpl = lambda item_id, count, added=0, removed=0: L.vi(item_id) + L.vi(count) + L.vi(added) + L.vi(removed)
STONE, APPLE = 1, 2  # [INFERENCE] small ids are valid items; unknown ones just make both sides ERR
wire_cases = {
    "container": [
        L.vi(0), L.vi(1) + "00", L.vi(1) + "01" + tmpl(STONE, 5), L.vi(256) + "00" * 256, L.vi(257) + "00" * 257, L.vi(-1), L.vi(2) + "0101",
        L.vi(2) + "01" + tmpl(STONE, 0), L.vi(1) + "01" + tmpl(0, 1), L.vi(1) + "01" + tmpl(100000, 1), L.vi(1) + "01" + tmpl(STONE, -3),
        L.vi(1) + "01" + tmpl(STONE, 1) + "ff", L.vi(1) + "02", L.vi(3) + "00" + "01" + tmpl(APPLE, 64) + "00",
        L.vi(1) + "01" + tmpl(STONE, 1, 1) + L.vi(0), L.vi(1) + "01" + tmpl(STONE, 1, 0, 1) + L.vi(0),
    ],
    "bundle_contents": [
        L.vi(0), L.vi(1) + tmpl(STONE, 1), L.vi(2) + tmpl(STONE, 99) + tmpl(APPLE, 100), L.vi(1) + tmpl(STONE, 0), L.vi(-1),
        L.vi(1) + tmpl(STONE, -1), L.vi(300) + tmpl(STONE, 1) * 300, L.vi(2) + tmpl(STONE, 1),
    ],
    "charged_projectiles": [L.vi(0), L.vi(1) + tmpl(STONE, 1), L.vi(1024) + tmpl(STONE, 1) * 1024, L.vi(1025) + tmpl(STONE, 1) * 1025, L.vi(1) + tmpl(0, 1)],
    "sulfur_cube_content": [tmpl(STONE, 1), tmpl(STONE, 0), tmpl(0, 1), "", tmpl(STONE, 1) + "00"],
    "block_state": [L.vi(0), L.vi(1) + L.utf("a") + L.utf("b"), L.vi(2) + L.utf("a") + L.utf("b") + L.utf("a") + L.utf("c"), L.vi(1) + L.utf("a"),
                    L.vi(-1), L.vi(2) + L.utf("") + L.utf("") + L.utf("z") + L.utf("é"), L.vi(1) + "ff01" + "61"],
    "lodestone_tracker": ["00" + "01", "00" + "00", "01" + L.ident("minecraft:overworld") + (1 << 26 | 2).to_bytes(8, "big").hex() + "01", "01" + L.ident("bad id") + "00" * 8 + "01", "02" + "01", "00"],
    "bees": [L.vi(0), L.vi(1) + L.vi(5) + "0a00" + L.vi(3) + L.vi(4), L.vi(1) + L.vi(5) + "0a00", L.vi(1) + L.vi(100000) + "0a00" + "00" + "00", L.vi(1) + L.vi(5) + "08" + "0001" + "61" + "00" + "00"],
}
for comp, hexes in wire_cases.items():
    for h in hexes:
        raw.append(("wire", P + comp, h))

# stacks embedding the components (validity + max stack checks)
stack = []
def s(snbt):
    stack.append(f"stack-snbt\t-\t{snbt}")
s('{id:"minecraft:shulker_box",count:1,components:{"minecraft:container":[{slot:0,item:{id:"minecraft:stone",count:64}}]}}')
s('{id:"minecraft:shulker_box",count:1,components:{"minecraft:container":[{slot:0,item:{id:"minecraft:stone",count:99}}]}}')
s('{id:"minecraft:shulker_box",count:1,components:{"minecraft:container":[{slot:0,item:{id:"minecraft:diamond_sword",count:2}}]}}')
s('{id:"minecraft:shulker_box",count:1,components:{"minecraft:container":[{slot:0,item:{id:"minecraft:stone",count:5,components:{"minecraft:max_stack_size":4}}}]}}')
s('{id:"minecraft:shulker_box",count:1,components:{"minecraft:container":[{slot:0,item:{id:"minecraft:ender_pearl",count:17}}]}}')
s('{id:"minecraft:shulker_box",count:1,components:{"minecraft:container":[{slot:0,item:{id:"minecraft:ender_pearl",count:16}}]}}')
s('{id:"minecraft:bundle",count:1,components:{"minecraft:bundle_contents":[{id:"minecraft:ender_pearl",count:17}]}}')
s('{id:"minecraft:bundle",count:1,components:{"minecraft:bundle_contents":[{id:"minecraft:stone",count:64},{id:"minecraft:stone",count:64}]}}')
s('{id:"minecraft:bundle",count:1,components:{"minecraft:bundle_contents":[{id:"minecraft:bundle",count:1,components:{"minecraft:bundle_contents":["minecraft:stone"]}}]}}')
s('{id:"minecraft:bundle",count:1,components:{"minecraft:bundle_contents":[{id:"minecraft:bundle",count:1,components:{"minecraft:bundle_contents":[{id:"minecraft:ender_pearl",count:99}]}}]}}')
s('{id:"minecraft:bundle",count:1,components:{"minecraft:bundle_contents":["minecraft:beehive"]}}')
s('{id:"minecraft:crossbow",count:1,components:{"minecraft:charged_projectiles":[{id:"minecraft:arrow",count:1}]}}')
s('{id:"minecraft:crossbow",count:1,components:{"minecraft:charged_projectiles":[{id:"minecraft:arrow",count:65}]}}')
s('{id:"minecraft:crossbow",count:1,components:{"minecraft:charged_projectiles":[{id:"minecraft:diamond_sword",count:2}]}}')
s('{id:"minecraft:crossbow",count:1,components:{"minecraft:charged_projectiles":[]}}')
s('{id:"minecraft:stone",count:1,components:{"minecraft:block_state":{facing:"north",lit:"true"}}}')
s('{id:"minecraft:compass",count:1,components:{"minecraft:lodestone_tracker":{target:{dimension:"minecraft:overworld",pos:[I;1,2,3]}}}}')
s('{id:"minecraft:filled_map",count:1,components:{"minecraft:map_decorations":{a:{type:"minecraft:player",x:1.0d,z:2.0d,rotation:3.0f}}}}')
s('{id:"minecraft:chest",count:1,components:{"minecraft:container_loot":{loot_table:"minecraft:chests/simple_dungeon",seed:12345L}}}')
s('{id:"minecraft:debug_stick",count:1,components:{"minecraft:debug_stick_state":{"minecraft:oak_stairs":"facing"}}}')
s('{id:"minecraft:knowledge_book",count:1,components:{"minecraft:recipes":["minecraft:stick"]}}')
s('{id:"minecraft:bee_nest",count:1,components:{"minecraft:bees":[{entity_data:{id:"minecraft:bee"},ticks_in_hive:0,min_ticks_in_hive:600}]}}')
s('{id:"minecraft:sulfur_cube_spawn_egg",count:1,components:{"minecraft:sulfur_cube_content":{id:"minecraft:stone",count:2}}}')

# overflow of the total bundle weight: coprime max stack sizes (1/97 + 1/89 + ...)
for primes in ([97, 89, 83, 79, 73], [97, 89, 83, 79, 73, 71, 67], [2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31], [97, 97, 97]):
    items = ",".join(f'{{id:"minecraft:stone",count:1,components:{{"minecraft:max_stack_size":{p}}}}}' for p in primes)
    stack.append(f'stack-snbt\t-\t{{id:"minecraft:bundle",count:1,components:{{"minecraft:bundle_contents":[{items}]}}}}')
# many bundles nested
deep = '"minecraft:stone"'
for _ in range(6):
    deep = f'{{id:"minecraft:bundle",count:1,components:{{"minecraft:bundle_contents":[{deep}]}}}}'
stack.append(f'stack-snbt\t-\t{{id:"minecraft:bundle",count:1,components:{{"minecraft:bundle_contents":[{deep}]}}}}')

# extra random stacks: container/bundle with random counts of random simple items
rng = random.Random(4041)
simple = ["stone", "apple", "ender_pearl", "diamond_sword", "snowball", "egg", "arrow", "bundle", "white_banner", "oak_sign", "stick"]
for _ in range(40):
    n = rng.randrange(1, 4)
    its = ",".join(f'{{id:"minecraft:{rng.choice(simple)}",count:{rng.choice([1, 2, 16, 17, 64, 65, 99])}}}' for _ in range(n))
    comp = rng.choice(["container", "bundle_contents", "charged_projectiles"])
    body = f'[{its}]' if comp != "container" else "[" + ",".join(f"{{slot:{i},item:{x}}}" for i, x in enumerate(its.replace("},{", "}|{").split("|"))) + "]"
    stack.append(f'stack-snbt\t-\t{{id:"minecraft:{rng.choice(["shulker_box", "bundle", "crossbow", "stone"])}",count:{rng.choice([1, 1, 2, 64])},components:{{"minecraft:{comp}":{body}}}}}')

L.build("corpus/40-containers.txt", 40, components, raw, mutations_per_base=5, stack_lines=stack)
