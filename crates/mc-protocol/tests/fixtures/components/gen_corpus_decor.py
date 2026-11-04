#!/usr/bin/env python3
"""Generates corpus/50-decor.txt (writable_book_content, written_book_content, firework_explosion, fireworks,
banner_patterns, pot_decorations, sign_text_front/back, instrument, jukebox_playable, trim, provides_trim_material,
provides_banner_patterns, provides_pottery_pattern).
Run from this directory: ./gen_corpus_decor.py && ./gen.sh corpus/50-decor.txt"""
import json
import random
import corpus_lib as L

P = "minecraft:"
j = lambda v: json.dumps(v, ensure_ascii=False, separators=(",", ":"))

txt = lambda s: {"text": s}
sound_direct = {"sound_id": "custom:horn", "range": 16.0}
instrument_direct = {"sound_event": P + "item.goat_horn.sound.0", "use_duration": 7.0, "range": 256.0, "description": {"translate": "x"}}
material_direct = {"palette_id": "custom:pal", "description": {"text": "Mat", "color": "red"}}
pattern_direct = {"asset_id": "custom:pat", "description": "Pat", "decal": True}
banner_direct = {"asset_id": "custom:b", "translation_key": "block.custom.b"}
brick = {"id": P + "brick", "count": 1}
named_brick = {"id": P + "brick", "count": 2, "components": {P + "custom_name": "N", P + "enchantments": {P + "unbreaking": 1}, P + "custom_data": {"k": "v"}}}

writable_bases = [{}, {"pages": []}, {"pages": ["hello", "wörld", "😀"]}, {"pages": [{"raw": "a", "filtered": "b"}, {"raw": "c"}, "d"]},
                  {"pages": ["x" * 1024]}, {"pages": ["😀" * 512]}]
written_bases = [
    {"title": "T", "author": "me"},
    {"title": {"raw": "Title", "filtered": "Filtered"}, "author": "a", "generation": 2, "pages": ["text", {"text": "x", "color": "red", "bold": True}], "resolved": True},
    {"title": "t", "author": "", "generation": 3, "pages": [{"raw": {"text": "raw"}, "filtered": {"translate": "f"}}, ["a", "b"], {"text": "c", "extra": ["d"]}]},
    {"title": "x" * 32, "author": "a", "pages": [{"selector": "@p"}, {"score": {"name": "a", "objective": "b"}}, {"keybind": "key.jump"}]},
    {"title": "t", "author": "a", "pages": [{"nbt": "x", "block": "1 2 3"}, 5, True, "§aLegacy"], "resolved": False},
    {"title": "t", "author": "a", "pages": [{"text": "p" * 100, "hover_event": {"action": "show_text", "value": "hi"}, "click_event": {"action": "open_url", "url": "https://x.y"}}]},
]
explosion = lambda **k: dict({"shape": "small_ball"}, **k)
explosion_bases = [explosion(), explosion(shape="star", colors=[16711680, 255], fade_colors=[0], has_trail=True, has_twinkle=True),
                   explosion(shape="burst", colors=[-1, 2**31 - 1, -2**31]), explosion(shape="creeper", fade_colors=[1, 2, 3, 4, 5]),
                   explosion(shape="large_ball", has_trail=False, has_twinkle=False, colors=[], fade_colors=[])]
fireworks_bases = [{}, {"flight_duration": 1}, {"flight_duration": 255, "explosions": [explosion(shape="star")]},
                   {"explosions": [explosion(), explosion(shape="burst", colors=[1])]}, {"flight_duration": 0, "explosions": []},
                   {"explosions": [explosion()] * 256}]
banner_bases = [[], [{"pattern": P + "base", "color": "red"}], [{"pattern": P + "stripe_top", "color": "light_blue"}, {"pattern": P + "creeper", "color": "black"}],
                [{"pattern": banner_direct, "color": "white"}], [{"pattern": P + "border", "color": "lime"}] * 7]
pot_bases = [{}, {"back": P + "brick"}, {"back": P + "brick", "left": {"id": P + "brick", "count": 3}, "right": P + "angler_pottery_sherd", "front": P + "skull_pottery_sherd"},
             {"front": {"id": P + "brick", "components": {P + "damage": 0}}}, {"back": named_brick, "left": named_brick}]
msgs = lambda *m: list(m)
sign_bases = [{"messages": ["", "", "", ""]}, {"messages": ["a", "b", "c", "d"], "color": "red", "has_glowing_text": True},
              {"messages": [txt("1"), txt("2"), {"text": "3", "color": "gold"}, {"translate": "x"}], "filtered_messages": ["w", "x", "y", "z"]},
              {"messages": ["a", "b", "c", "d"], "filtered_messages": ["a", "b", "c", "d"], "color": "black", "has_glowing_text": False},
              {"messages": ["a", "b", "c", "d"], "filtered_messages": ["a", "b", "c"]},
              {"messages": ["a", "b", "c", "d"], "filtered_messages": "nope", "color": "light_gray"}]
instrument_bases = [P + "ponder_goat_horn", P + "admire_goat_horn", instrument_direct,
                    dict(instrument_direct, sound_event=sound_direct, durability_damage=3),
                    dict(instrument_direct, sound_event={"sound_id": P + "x"}, use_duration=0.0, range=0.5)]
jukebox_bases = [P + "cat", P + "11", P + "creator"]
trim_bases = [{"material": P + "gold", "pattern": P + "coast"}, {"material": material_direct, "pattern": pattern_direct},
              {"material": P + "amethyst", "pattern": pattern_direct}, {"material": material_direct, "pattern": P + "dune"}]
trim_material_bases = [P + "diamond", material_direct]
provides_banner_bases = ["#minecraft:pattern_item/flower", [P + "base"], [P + "base", P + "border"], P + "circle", [], "#minecraft:pattern_item/creeper"]
pottery_bases = [P + "angler", P + "snort", P + "heartbreak"]

components = {
    P + "writable_book_content": (writable_bases, [{"raw": "x", "filtered": 5}, {"raw": {"a": 1}}]),
    P + "written_book_content": (written_bases, [{"raw": "x"}, {"text": "t"}, "title"]),
    P + "firework_explosion": (explosion_bases, ["star", "burst"]),
    P + "fireworks": (fireworks_bases, [explosion()]),
    P + "banner_patterns": (banner_bases, [{"pattern": P + "nope", "color": "red"}, {"pattern": P + "base", "color": "bad"}]),
    P + "pot_decorations": (pot_bases, ["minecraft:brick", {"id": P + "brick", "count": 0}]),
    P + "sign_text_front": (sign_bases, [["a", "b", "c", "d"]]),
    P + "sign_text_back": (sign_bases[:3], []),
    P + "instrument": (instrument_bases, ["#minecraft:x"]),
    P + "jukebox_playable": (jukebox_bases, [{"sound_event": P + "x", "description": "d", "length_in_seconds": 1.0, "comparator_output": 1}]),
    P + "trim": (trim_bases, ["minecraft:gold"]),
    P + "provides_trim_material": (trim_material_bases, ["#minecraft:x"]),
    P + "provides_banner_patterns": (provides_banner_bases, [{"a": 1}, "#minecraft:does_not_exist"]),
    P + "provides_pottery_pattern": (pottery_bases, ["#minecraft:x"]),
}

raw = []
def r(kind, name, payload):
    raw.append((kind, P + name, payload))

# books
r("json", "writable_book_content", j({"pages": ["x" * 1025]}))
r("json", "writable_book_content", j({"pages": ["😀" * 513]}))
r("json", "writable_book_content", j({"pages": ["p"] * 100}))
r("json", "writable_book_content", j({"pages": ["p"] * 101}))
r("json", "writable_book_content", j({"pages": [{"raw": "x" * 1024, "filtered": "y" * 1024}]}))
r("json", "writable_book_content", j({"pages": [{"raw": "x" * 1025, "filtered": "y"}]}))
r("json", "writable_book_content", '{"pages":null}')
r("json", "writable_book_content", '{"pages":"a"}')
r("json", "writable_book_content", '[]')
r("snbt", "writable_book_content", '{pages:["a",{raw:"b",filtered:"c"}]}')
r("snbt", "writable_book_content", '{pages:[{raw:"b",filtered:1}]}')
r("snbt", "writable_book_content", '{pages:[1b]}')
r("json", "written_book_content", j({"title": "x" * 33, "author": "a"}))
r("json", "written_book_content", j({"title": {"raw": "x" * 32, "filtered": "y" * 33}, "author": "a"}))
r("json", "written_book_content", j({"title": "t", "author": "a", "generation": 4}))
r("json", "written_book_content", j({"title": "t", "author": "a", "generation": -1}))
r("json", "written_book_content", j({"title": "t", "author": "a", "generation": 1.5}))
r("json", "written_book_content", j({"title": "t"}))
r("json", "written_book_content", j({"author": "t"}))
r("json", "written_book_content", j({"title": "t", "author": "a", "pages": [{"text": "x" * 40000}]}))
r("json", "written_book_content", j({"title": "t", "author": "a", "pages": ["x" * 32767]}))
r("json", "written_book_content", j({"title": "t", "author": "a", "pages": [None]}))
r("json", "written_book_content", j({"title": "t", "author": "a", "pages": [{"raw": "r"}], "resolved": 1}))
r("snbt", "written_book_content", '{title:"t",author:"a",generation:1b,pages:[{text:"x"}],resolved:1b}')
r("snbt", "written_book_content", '{title:"t",author:"a",generation:3L}')
r("snbt", "written_book_content", '{title:{raw:"t",filtered:"u"},author:"a",pages:[{raw:{text:"r"},filtered:"f"},"plain",["a","b"],{text:"c",bold:1b}]}')
r("snbt", "written_book_content", '{title:"t",author:"a",pages:[{raw:"r",filtered:{text:"f",color:"red"}}]}')
r("snbt", "written_book_content", '{title:"t",author:"a",pages:[1,2L,1.5d]}')
# fireworks
r("json", "firework_explosion", '{"shape":"star","colors":[1.5]}')
r("json", "firework_explosion", '{"shape":"star","colors":[2147483648]}')
r("json", "firework_explosion", '{"shape":"star","colors":[1,2,3],"fade_colors":null}')
r("json", "firework_explosion", '{"shape":"Star"}')
r("json", "firework_explosion", '{"colors":[1]}')
r("json", "firework_explosion", '{"shape":"star","has_trail":"true"}')
r("snbt", "firework_explosion", '{shape:"burst",colors:[I;1,2],fade_colors:[B;1b]}')
r("snbt", "firework_explosion", '{shape:"burst",colors:[1b,2s,3L,4.5f]}')
r("snbt", "firework_explosion", '{shape:"burst",colors:[1,2],has_trail:1b,has_twinkle:0b}')
r("snbt", "firework_explosion", '{shape:"burst",colors:[1,2],has_trail:2b}')
r("json", "fireworks", j({"flight_duration": 256}))
r("json", "fireworks", j({"flight_duration": -1}))
r("json", "fireworks", j({"flight_duration": 255}))
r("json", "fireworks", j({"flight_duration": 1.5}))
r("json", "fireworks", j({"explosions": [explosion()] * 257}))
r("json", "fireworks", j({"explosions": explosion()}))
r("snbt", "fireworks", '{flight_duration:300}')
r("snbt", "fireworks", '{flight_duration:200b}')
r("snbt", "fireworks", '{flight_duration:-56b}')
r("snbt", "fireworks", '{flight_duration:3.5f,explosions:[]}')
r("snbt", "fireworks", '{flight_duration:2L}')
# banner patterns
r("json", "banner_patterns", '[{"pattern":"minecraft:base","color":"red"}]')
r("json", "banner_patterns", '[{"pattern":"base","color":"red"}]')
r("json", "banner_patterns", '[{"pattern":"minecraft:base"}]')
r("json", "banner_patterns", '[{"pattern":{"asset_id":"a:b"},"color":"red"}]')
r("json", "banner_patterns", '{"pattern":"minecraft:base","color":"red"}')
r("json", "banner_patterns", '[{"pattern":"#minecraft:x","color":"red"}]')
r("snbt", "banner_patterns", '[{pattern:"minecraft:base",color:"red"},{pattern:{asset_id:"a:b",translation_key:"k"},color:"blue"}]')
r("snbt", "banner_patterns", '[{pattern:"minecraft:base",color:5}]')
# pot decorations
r("json", "pot_decorations", '{"back":"minecraft:air"}')
r("json", "pot_decorations", '{"back":null}')
r("json", "pot_decorations", '{"back":"minecraft:brick","front":"minecraft:brick","left":"minecraft:brick","right":"minecraft:brick"}')
r("json", "pot_decorations", '{"back":["minecraft:brick"]}')
r("snbt", "pot_decorations", '{back:{id:"minecraft:brick",count:5}}')
r("snbt", "pot_decorations", '{back:"minecraft:brick",left:"minecraft:bad"}')
# sign text
for k in ("front", "back"):
    r("json", "sign_text_" + k, '{"messages":["a","b","c"]}')
    r("json", "sign_text_" + k, '{"messages":["a","b","c","d","e"]}')
    r("json", "sign_text_" + k, '{"messages":["a","b","c","d"],"filtered_messages":["a","b","c","d","e"]}')
    r("json", "sign_text_" + k, '{"messages":["a","b","c","d"],"filtered_messages":[1,2,3,4]}')
    r("json", "sign_text_" + k, '{"messages":["a","b","c","d"],"color":"nope"}')
    r("json", "sign_text_" + k, '{"messages":["a","b","c","d"],"has_glowing_text":1}')
    r("json", "sign_text_" + k, '{"filtered_messages":["a","b","c","d"]}')
    r("json", "sign_text_" + k, '{"messages":["a","b","c","d"],"filtered_messages":["a","b","c","d"]}')
    r("json", "sign_text_" + k, '{"messages":[["a","b"],"c","d","e"]}')
    r("snbt", "sign_text_" + k, '{messages:["a","b","c","d"],color:"red",has_glowing_text:1b}')
    r("snbt", "sign_text_" + k, '{messages:[{text:"a"},{text:"b"},{text:"c"},{text:"d"}],filtered_messages:["a","b","c","e"]}')
    r("snbt", "sign_text_" + k, '{messages:["a","b","c","d"],has_glowing_text:5b}')
    r("snbt", "sign_text_" + k, '{messages:["a","b","c","d"],has_glowing_text:"true"}')
# instrument / jukebox / trim / provides
r("json", "instrument", j(dict(instrument_direct, use_duration=-1.0)))
r("json", "instrument", j(dict(instrument_direct, range=0.0)))
r("json", "instrument", j(dict(instrument_direct, durability_damage=-1)))
r("json", "instrument", j(dict(instrument_direct, durability_damage=1.5)))
r("json", "instrument", j({k: v for k, v in instrument_direct.items() if k != "description"}))
r("json", "instrument", j(dict(instrument_direct, sound_event={"sound_id": "a:b", "range": "x"})))
r("json", "instrument", j(dict(instrument_direct, sound_event={"sound_id": "a:b", "range": 5})))
r("json", "instrument", '"minecraft:nope"')
r("json", "instrument", '"ponder_goat_horn"')
r("json", "instrument", 'null')
r("snbt", "instrument", '{sound_event:"minecraft:item.goat_horn.sound.0",use_duration:1b,range:2s,description:"d"}')
r("snbt", "instrument", '{sound_event:{sound_id:"a:b"},use_duration:1.0f,range:2.0d,durability_damage:5L,description:{text:"d"}}')
r("snbt", "instrument", '"minecraft:ponder_goat_horn"')
r("json", "jukebox_playable", '"minecraft:nope"')
r("json", "jukebox_playable", '"#minecraft:cat"')
r("json", "jukebox_playable", '["minecraft:cat"]')
r("snbt", "jukebox_playable", '"minecraft:cat"')
r("snbt", "jukebox_playable", '{sound_event:"minecraft:x",description:"d",length_in_seconds:1.0f,comparator_output:1}')
r("json", "trim", '{"material":"minecraft:gold"}')
r("json", "trim", '{"pattern":"minecraft:coast"}')
r("json", "trim", '{"material":"minecraft:nope","pattern":"minecraft:coast"}')
r("json", "trim", '{"material":{"palette_id":"a:b"},"pattern":"minecraft:coast"}')
r("json", "trim", '{"material":"minecraft:gold","pattern":{"asset_id":"a:b","description":"d"}}')
r("json", "trim", '{"material":"minecraft:gold","pattern":{"asset_id":"a:b","description":"d","decal":"x"}}')
r("snbt", "trim", '{material:"minecraft:gold",pattern:"minecraft:coast"}')
r("snbt", "trim", '{material:{palette_id:"a:b",description:{text:"m"}},pattern:{asset_id:"a:b",description:"d",decal:1b}}')
r("json", "provides_trim_material", '"minecraft:nope"')
r("json", "provides_trim_material", '{"palette_id":"a:b"}')
r("snbt", "provides_trim_material", '{palette_id:"a:b",description:"d"}')
r("json", "provides_banner_patterns", '"#minecraft:pattern_item/flower"')
r("json", "provides_banner_patterns", '"minecraft:base"')
r("json", "provides_banner_patterns", '["minecraft:base","minecraft:base"]')
r("json", "provides_banner_patterns", '[{"asset_id":"a:b","translation_key":"k"}]')
r("json", "provides_banner_patterns", '["#minecraft:pattern_item/flower"]')
r("json", "provides_banner_patterns", '"#minecraft:pattern_item/flower "')
r("json", "provides_banner_patterns", '[]')
r("snbt", "provides_banner_patterns", '["minecraft:base"]')
r("snbt", "provides_banner_patterns", '"#minecraft:pattern_item/globe"')
r("snbt", "provides_banner_patterns", '[1]')
r("json", "provides_pottery_pattern", '"minecraft:nope"')
r("json", "provides_pottery_pattern", '"#minecraft:angler"')
r("json", "provides_pottery_pattern", '{"asset_id":"a:b"}')
r("snbt", "provides_pottery_pattern", '"minecraft:blade"')


# ---- hand-built wire cases -------------------------------------------------------------------------------------------
tmpl = lambda item_id, count, added=0, removed=0: L.vi(item_id) + L.vi(count) + L.vi(added) + L.vi(removed)
four = L.nbt_str("a") * 4
wire_cases = {
    "writable_book_content": [L.vi(0), L.vi(1) + L.utf("page") + "00", L.vi(1) + L.utf("page") + "01" + L.utf("f"), L.vi(100) + (L.utf("x") + "00") * 100,
                              L.vi(101) + (L.utf("x") + "00") * 101, L.vi(1) + L.vi(1025) + "61" * 1025 + "00", L.vi(1) + L.vi(1024) + "61" * 1024 + "00", L.vi(-1), L.vi(1) + "02"],
    "written_book_content": [L.utf("t") + "00" + L.utf("a") + L.vi(0) + L.vi(0) + "00", L.utf("t") + "01" + L.utf("f") + L.utf("a") + L.vi(3) + L.vi(1) + L.nbt_str("p") + "00" + "01",
                             L.utf("t") + "00" + L.utf("a") + L.vi(4) + L.vi(0) + "00", L.utf("t") + "00" + L.utf("a") + L.vi(-1) + L.vi(0) + "00",
                             L.utf("x" * 32) + "00" + L.utf("a") + L.vi(0) + L.vi(0) + "00", L.utf("x" * 33) + "00" + L.utf("a") + L.vi(0) + L.vi(0) + "00",
                             L.utf("t") + "00" + L.utf("a") + L.vi(0) + L.vi(1) + "0a00" + "00" + "00", L.utf("t") + "00" + L.utf("a") + L.vi(0) + L.vi(1) + L.nbt_str("p") + "02" + "00"],
    "firework_explosion": [L.vi(0) + L.vi(0) + L.vi(0) + "0000", L.vi(2) + L.vi(1) + L.be32(16711680) + L.vi(2) + L.be32(1) + L.be32(-1) + "0101", L.vi(5) + L.vi(0) + L.vi(0) + "0000",
                           L.vi(100) + L.vi(0) + L.vi(0) + "0000", L.vi(-1) + L.vi(0) + L.vi(0) + "0000", L.vi(1) + L.vi(1) + "00" + L.vi(0) + "0000", L.vi(1) + L.vi(0) + L.vi(0) + "0200",
                           L.vi(1) + L.vi(-1) + L.vi(0) + "0000", L.vi(1) + L.vi(0) + L.vi(0) + "0000" + "99"],
    "fireworks": [L.vi(0) + L.vi(0), L.vi(255) + L.vi(1) + L.vi(1) + L.vi(0) + L.vi(0) + "0000", L.vi(256) + L.vi(0), L.vi(-1) + L.vi(0), L.vi(1000000) + L.vi(0),
                  L.vi(1) + L.vi(257) + (L.vi(0) * 2 + "0000") * 257, L.vi(1) + L.vi(256) + (L.vi(0) * 2 + "0000") * 256, L.vi(1) + L.vi(-1)],
    "banner_patterns": [L.vi(0), L.vi(1) + L.vi(1) + L.vi(14), L.vi(1) + L.vi(0) + L.ident("a:b") + L.utf("k") + L.vi(3), L.vi(1) + L.vi(1000) + L.vi(1), L.vi(1) + L.vi(1) + L.vi(99),
                       L.vi(1) + L.vi(0) + L.ident("a:b"), L.vi(-1), L.vi(2) + L.vi(1) + L.vi(0) + L.vi(2) + L.vi(15)],
    "pot_decorations": ["00000000", "01" + tmpl(1, 1) + "000000", "01" + tmpl(1, 1) + "01" + tmpl(1, 2) + "01" + tmpl(2, 3) + "01" + tmpl(1, 1), "02" + "00000000", "01" + tmpl(0, 1) + "000000", "01" + tmpl(1, 0) + "000000", "00"],
    "sign_text_front": [four + "00" + L.vi(0) + "00", four + "01" + four + L.vi(15) + "01", four + "01" + L.nbt_str("a") * 3, four + "00" + L.vi(16) + "00", four + "00" + L.vi(-1) + "00",
                        L.nbt_str("a") * 3, four + "02" + L.vi(0) + "00", four + "00" + L.vi(0) + "05", four + "00" + L.vi(0) + "00" + "ff", "0a00" * 4 + "00" + L.vi(0) + "00"],
    "instrument": [L.vi(1), L.vi(8), L.vi(200), L.vi(0) + L.vi(1) + "3f800000" + "3f800000" + L.vi(0) + L.nbt_str("d"),
                   L.vi(0) + L.vi(0) + L.ident("a:b") + "00" + "3f800000" + "3f800000" + L.vi(2) + L.nbt_str("d"), L.vi(0) + L.vi(0) + L.ident("a:b") + "01" + "41200000" + "00000000" + "00000000" + L.vi(-1) + L.nbt_str("d"),
                   L.vi(0) + L.vi(1) + "3f800000", L.vi(-1), L.vi(0) + L.vi(1) + "7fc00000" + "ff800000" + L.vi(0) + L.nbt_str("d")],
    "jukebox_playable": [L.vi(1), L.vi(5), L.vi(300), L.vi(0) + L.vi(1) + L.nbt_str("d") + "41a00000" + L.vi(15), L.vi(0) + L.vi(1) + L.nbt_str("d") + "00000000" + L.vi(99), L.vi(0) + L.vi(0) + L.ident("a:b") + "00" + L.nbt_str("d") + "3f800000" + L.vi(0)],
    "trim": [L.vi(1) + L.vi(1), L.vi(0) + L.ident("a:b") + L.nbt_str("m") + L.vi(1), L.vi(1) + L.vi(0) + L.ident("a:b") + L.nbt_str("p") + "01", L.vi(1) + L.vi(0) + L.ident("a:b") + L.nbt_str("p") + "02",
             L.vi(1000) + L.vi(1), L.vi(1) + L.vi(1000), L.vi(1), L.vi(-1) + L.vi(1)],
    "provides_trim_material": [L.vi(1), L.vi(0) + L.ident("a:b") + L.nbt_str("m"), L.vi(0) + L.ident("a:b") + "0a00", L.vi(1000), L.vi(-1)],
    "provides_banner_patterns": [L.vi(0) + L.ident("minecraft:pattern_item/flower"), L.vi(0) + L.ident("minecraft:nope"), L.vi(1), L.vi(2) + L.vi(0) + L.vi(1), L.vi(2) + L.vi(0) + L.vi(1000),
                                 L.vi(3) + L.vi(0) + L.vi(0), L.vi(-1), L.vi(-5), L.vi(0) + L.ident("bad id"), L.vi(0) + L.ident("a:b") + "ff"],
    "provides_pottery_pattern": [L.vi(0), L.vi(1), L.vi(22), L.vi(23), L.vi(1000), L.vi(-1)],
}
for comp, hexes in wire_cases.items():
    for h in hexes:
        raw.append(("wire", P + comp, h))

# stacks
stack = []
def s(snbt):
    stack.append(f"stack-snbt\t-\t{snbt}")
s('{id:"minecraft:writable_book",count:1,components:{"minecraft:writable_book_content":{pages:["a",{raw:"b",filtered:"c"}]}}}')
s('{id:"minecraft:written_book",count:1,components:{"minecraft:written_book_content":{title:"T",author:"A",pages:[{text:"p"}],generation:1,resolved:1b}}}')
s('{id:"minecraft:firework_rocket",count:3,components:{"minecraft:fireworks":{flight_duration:2b,explosions:[{shape:"star",colors:[I;16711680]}]}}}')
s('{id:"minecraft:firework_star",count:1,components:{"minecraft:firework_explosion":{shape:"creeper",has_trail:1b}}}')
s('{id:"minecraft:white_banner",count:1,components:{"minecraft:banner_patterns":[{pattern:"minecraft:base",color:"red"}]}}')
s('{id:"minecraft:decorated_pot",count:1,components:{"minecraft:pot_decorations":{back:"minecraft:brick",front:{id:"minecraft:angler_pottery_sherd",count:1}}}}')
# one component per stack: the order of a patch's components in `hashed=` is the identity-hash order of vanilla's map
s('{id:"minecraft:oak_sign",count:1,components:{"minecraft:sign_text_front":{messages:["a","b","c","d"],color:"red"}}}')
s('{id:"minecraft:oak_sign",count:1,components:{"minecraft:sign_text_back":{messages:["","","",""],has_glowing_text:1b}}}')
s('{id:"minecraft:goat_horn",count:1,components:{"minecraft:instrument":"minecraft:ponder_goat_horn"}}')
s('{id:"minecraft:music_disc_cat",count:1,components:{"minecraft:jukebox_playable":"minecraft:cat"}}')
s('{id:"minecraft:iron_chestplate",count:1,components:{"minecraft:trim":{material:"minecraft:gold",pattern:"minecraft:coast"}}}')
s('{id:"minecraft:amethyst_shard",count:1,components:{"minecraft:provides_trim_material":"minecraft:amethyst"}}')
s('{id:"minecraft:flower_banner_pattern",count:1,components:{"minecraft:provides_banner_patterns":"#minecraft:pattern_item/flower"}}')
s('{id:"minecraft:angler_pottery_sherd",count:1,components:{"minecraft:provides_pottery_pattern":"minecraft:angler"}}')
s('{id:"minecraft:written_book",count:1,components:{"minecraft:written_book_content":{title:"T",author:"A",generation:9}}}')
s('{id:"minecraft:goat_horn",count:1,components:{"minecraft:instrument":{sound_event:"minecraft:item.goat_horn.sound.0",use_duration:7.0f,range:256.0f,description:{translate:"x"}}}}')
s('{id:"minecraft:music_disc_cat",count:1,components:{"minecraft:jukebox_playable":{sound_event:"minecraft:x"}}}')
s('{id:"minecraft:firework_rocket",count:1,components:{"minecraft:fireworks":{explosions:[{shape:"bad"}]}}}')
s('{id:"minecraft:white_banner",count:2,components:{"!minecraft:banner_patterns":{}}}')

L.build("corpus/50-decor.txt", 50, components, raw, mutations_per_base=4, stack_lines=stack)
