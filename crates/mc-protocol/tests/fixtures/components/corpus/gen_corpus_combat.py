#!/usr/bin/env python3
"""Generates corpus/30-combat.txt (components: tool, weapon, attack_range, blocks_attacks, kinetic_weapon, piercing_weapon,
attack_animation, interact_animation, damage_resistant, equippable). Uses the helpers of gen_corpus_effects.py.

Usage: python3 crates/mc-protocol/tests/fixtures/components/corpus/gen_corpus_combat.py && crates/mc-protocol/tests/fixtures/components/gen.sh corpus/30-combat.txt   (deterministic)
Output: crates/mc-protocol/tests/fixtures/components/corpus/30-combat.txt
"""
import os
import random
import sys

if sys.argv[1:] in (["-h"], ["--help"]):
    print(__doc__)
    sys.exit(0)

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from gen_corpus_effects import Out, emit_component, write_corpus  # noqa: E402

BLOCKS = ["stone", "dirt", "oak_log", "cobweb", "obsidian", "glass", "sand", "netherrack", "nope", "air", "bedrock"]
BLOCK_TAGS = ["mineable/pickaxe", "mineable/axe", "mineable/shovel", "mineable/hoe", "logs", "leaves", "wool", "planks", "needs_diamond_tool",
              "incorrect_for_wooden_tool", "nope", "sword_instantly_mines", "sword_efficient"]
DAMAGE_TYPES = ["arrow", "cactus", "fall", "in_fire", "on_fire", "lava", "explosion", "player_attack", "mob_attack", "magic", "wither", "out_of_world", "nope"]
DAMAGE_TAGS = ["is_fire", "is_explosion", "is_projectile", "bypasses_armor", "is_fall", "no_knockback", "is_player_attack", "bypasses_shield",
               "avoids_guardian_thorns", "is_drowning", "nope"]
ENTITIES = ["horse", "llama", "trader_llama", "zombie", "player", "pig", "nope", "ender_dragon", "happy_ghast"]
ENTITY_TAGS = ["skeletons", "zombies", "can_equip_saddle", "can_equip_harness", "undead", "arrows", "nope"]
SOUNDS = ["entity.generic.eat", "item.armor.equip_iron", "item.shield.block", "item.shield.break", "item.trident.hit", "entity.arrow.hit",
          "item.armor.equip_generic", "item.shears.snip", "item.totem.use"]
SLOTS = ["mainhand", "offhand", "feet", "legs", "chest", "head", "body", "saddle"]


def block_sets(rng):
    s = ["minecraft:stone", ["minecraft:stone", "minecraft:dirt"], "#minecraft:mineable/pickaxe", "#minecraft:nope", [], "stone", ["stone"], "",
         "#", "#minecraft:", ["#minecraft:logs"], ["minecraft:nope"], ["minecraft:stone", "minecraft:stone"], 5, None, {}, True, ["minecraft:stone", 5],
         "minecraft:air", ["minecraft:air"], "Minecraft:Stone", ["minecraft:stone", None]]
    s += ["minecraft:" + b for b in rng.sample(BLOCKS, 4)]
    s += ["#minecraft:" + b for b in rng.sample(BLOCK_TAGS, 4)]
    s += [["minecraft:" + b for b in rng.sample(BLOCKS, 3)]]
    return s


def tool_seeds(rng):
    s = [{"rules": []}, {}, {"rules": {}}, {"rules": "x"}, {"rules": [{}]}, {"rules": [None]},
         {"rules": [{"blocks": "minecraft:stone", "speed": 2.0, "correct_for_drops": True}]},
         {"rules": [{"blocks": "minecraft:stone"}]}, {"rules": [{"blocks": "minecraft:stone", "speed": 0}]},
         {"rules": [{"blocks": "minecraft:stone", "speed": -1}]}, {"rules": [{"blocks": "minecraft:stone", "speed": "x"}]},
         {"rules": [{"blocks": "minecraft:stone", "speed": 1e39}]}, {"rules": [{"blocks": "minecraft:stone", "speed": 3.4028235e38}]},
         {"rules": [{"blocks": "minecraft:stone", "correct_for_drops": False}]}, {"rules": [{"blocks": "minecraft:stone", "correct_for_drops": 1}]},
         {"rules": [{"blocks": "minecraft:stone", "correct_for_drops": None}]}, {"rules": [{"speed": 2.0}]},
         {"rules": [{"blocks": "minecraft:stone", "extra": 1}]},
         {"rules": [], "default_mining_speed": 1.0}, {"rules": [], "default_mining_speed": 0}, {"rules": [], "default_mining_speed": -2.5},
         {"rules": [], "default_mining_speed": 1e39}, {"rules": [], "default_mining_speed": "x"}, {"rules": [], "default_mining_speed": 1.5},
         {"rules": [], "damage_per_block": 0}, {"rules": [], "damage_per_block": 2}, {"rules": [], "damage_per_block": -1}, {"rules": [], "damage_per_block": 1.5},
         {"rules": [], "damage_per_block": 2**31}, {"rules": [], "damage_per_block": "2"},
         {"rules": [], "can_destroy_blocks_in_creative": False}, {"rules": [], "can_destroy_blocks_in_creative": True}, {"rules": [], "can_destroy_blocks_in_creative": 0},
         {"rules": [], "can_destroy_blocks_in_creative": "false"}, {"rules": [], "unknown": 1}, [], None, 5, "x"]
    for b in block_sets(rng):
        s.append({"rules": [{"blocks": b, "speed": 4.0, "correct_for_drops": True}]})
    s.append({"rules": [{"blocks": "#minecraft:mineable/pickaxe", "speed": 2.0, "correct_for_drops": True}, {"blocks": ["minecraft:cobweb"], "speed": 15.0},
                        {"blocks": "#minecraft:incorrect_for_wooden_tool", "correct_for_drops": False}], "default_mining_speed": 1.5, "damage_per_block": 2,
              "can_destroy_blocks_in_creative": False})
    return s


def weapon_seeds(rng):
    return [{}, {"item_damage_per_attack": 2}, {"item_damage_per_attack": 0}, {"item_damage_per_attack": -1}, {"item_damage_per_attack": 1}, {"item_damage_per_attack": 2.5},
            {"item_damage_per_attack": "2"}, {"item_damage_per_attack": 2**31}, {"item_damage_per_attack": None},
            {"disable_blocking_for_seconds": 5.0}, {"disable_blocking_for_seconds": 5}, {"disable_blocking_for_seconds": 0}, {"disable_blocking_for_seconds": -0.0},
            {"disable_blocking_for_seconds": -1}, {"disable_blocking_for_seconds": 3.4028235e38}, {"disable_blocking_for_seconds": 1e39}, {"disable_blocking_for_seconds": "x"},
            {"item_damage_per_attack": 1, "disable_blocking_for_seconds": 0.0}, {"item_damage_per_attack": 3, "disable_blocking_for_seconds": 2.5},
            {"unknown": 1}, [], None, 5, "x", {"item_damage_per_attack": [2]}]


def attack_range_seeds(rng):
    s = [{}, {"min_reach": 1.5}, {"max_reach": 4.5}, {"min_creative_reach": 2}, {"max_creative_reach": 6.5}, {"hitbox_margin": 0.125}, {"mob_factor": 0.5},
         {"min_reach": 0, "max_reach": 3, "min_creative_reach": 0, "max_creative_reach": 5, "hitbox_margin": 0.3, "mob_factor": 1},
         {"min_reach": 0.0, "max_reach": 3.0, "min_creative_reach": 0.0, "max_creative_reach": 5.0, "hitbox_margin": 0.30000001192092896, "mob_factor": 1.0},
         {"min_reach": 64}, {"min_reach": 64.5}, {"min_reach": -0.5}, {"min_reach": -0.0}, {"max_reach": 64.0}, {"max_reach": 65}, {"max_reach": -1},
         {"min_creative_reach": 64.0001}, {"max_creative_reach": 100}, {"hitbox_margin": 1}, {"hitbox_margin": 1.1}, {"hitbox_margin": -0.1},
         {"mob_factor": 2}, {"mob_factor": 2.1}, {"mob_factor": -0.5}, {"mob_factor": 0}, {"mob_factor": "x"}, {"min_reach": "x"}, {"max_reach": None},
         {"min_reach": 1e39}, {"mob_factor": 1e39}, {"min_reach": [1]}, {"unknown": 1}, [], None, 5, "x"]
    for _ in range(8):
        s.append({"min_reach": round(rng.uniform(0, 4), 2), "max_reach": round(rng.uniform(0, 8), 2), "min_creative_reach": round(rng.uniform(0, 4), 2),
                  "max_creative_reach": round(rng.uniform(0, 10), 2), "hitbox_margin": round(rng.uniform(0, 1), 2), "mob_factor": round(rng.uniform(0, 2), 2)})
    return s


def damage_sets(rng):
    s = ["minecraft:fall", ["minecraft:fall", "minecraft:cactus"], "#minecraft:is_fire", "#minecraft:nope", [], "fall", ["fall"], "", "#", ["#minecraft:is_fire"],
         ["minecraft:nope"], ["minecraft:fall", "minecraft:fall"], 5, None, {}, True, ["minecraft:fall", 5], "Minecraft:Fall", ["minecraft:fall", None]]
    s += ["minecraft:" + b for b in rng.sample(DAMAGE_TYPES, 4)]
    s += ["#minecraft:" + b for b in rng.sample(DAMAGE_TAGS, 5)]
    s += [["minecraft:" + b for b in rng.sample(DAMAGE_TYPES, 3)]]
    return s


def blocks_attacks_seeds(rng):
    red = {"base": 0.0, "factor": 1.0}
    s = [{}, {"block_delay_seconds": 0.25}, {"block_delay_seconds": 0}, {"block_delay_seconds": -1}, {"block_delay_seconds": "x"}, {"block_delay_seconds": 1e39},
         {"disable_cooldown_scale": 0.5}, {"disable_cooldown_scale": 1}, {"disable_cooldown_scale": -1}, {"disable_cooldown_scale": 0},
         {"damage_reductions": []}, {"damage_reductions": [red]}, {"damage_reductions": [{"horizontal_blocking_angle": 90.0, "base": 0.0, "factor": 1.0}]},
         {"damage_reductions": [{"base": 0, "factor": 1}]}, {"damage_reductions": [{"base": 0}]}, {"damage_reductions": [{"factor": 1}]}, {"damage_reductions": [{}]},
         {"damage_reductions": [{"horizontal_blocking_angle": 0, "base": 0, "factor": 1}]}, {"damage_reductions": [{"horizontal_blocking_angle": -5, "base": 0, "factor": 1}]},
         {"damage_reductions": [{"horizontal_blocking_angle": 180, "base": 1.5, "factor": 0.5}]}, {"damage_reductions": [{"horizontal_blocking_angle": 1e39, "base": 0, "factor": 1}]},
         {"damage_reductions": [{"base": "x", "factor": 1}]}, {"damage_reductions": [{"base": 0, "factor": None}]}, {"damage_reductions": {}}, {"damage_reductions": "x"},
         {"damage_reductions": [red, red]}, {"damage_reductions": [None]},
         {"item_damage": {"threshold": 3.0, "base": 1.0, "factor": 0.5}}, {"item_damage": {"threshold": 1.0, "base": 0.0, "factor": 1.0}}, {"item_damage": {"threshold": 1, "base": 0, "factor": 1}},
         {"item_damage": {"threshold": -1, "base": 0, "factor": 1}}, {"item_damage": {"base": 0, "factor": 1}}, {"item_damage": {"threshold": 1, "factor": 1}},
         {"item_damage": {"threshold": 1, "base": 0}}, {"item_damage": {}}, {"item_damage": []}, {"item_damage": None}, {"item_damage": 5},
         {"bypassed_by": "#minecraft:bypasses_shield"}, {"bypassed_by": "minecraft:fall"}, {"bypassed_by": []}, {"bypassed_by": "#minecraft:nope"}, {"bypassed_by": None},
         {"block_sound": "minecraft:item.shield.block"}, {"block_sound": {"sound_id": "a:b"}}, {"block_sound": {"sound_id": "a:b", "range": 5}}, {"block_sound": "minecraft:nope"},
         {"block_sound": 5}, {"block_sound": None}, {"block_sound": {}}, {"disabled_sound": "minecraft:item.shield.break"}, {"disabled_sound": {"sound_id": "a:b"}},
         {"disabled_sound": "nope"}, {"disable_sound": "minecraft:item.shield.break"}, {"unknown": 1}, [], None, 5, "x"]
    for d in damage_sets(rng):
        s.append({"bypassed_by": d})
        s.append({"damage_reductions": [{"type": d, "base": 1, "factor": 0.5}]})
    s.append({"block_delay_seconds": 0.25, "disable_cooldown_scale": 0.5, "damage_reductions": [{"horizontal_blocking_angle": 90.0, "type": "#minecraft:is_projectile", "base": 0, "factor": 1},
                                                                                                   {"horizontal_blocking_angle": 45, "base": 1, "factor": 0.25}],
              "item_damage": {"threshold": 3.0, "base": 1.0, "factor": 1.0}, "bypassed_by": "#minecraft:bypasses_shield", "block_sound": "minecraft:item.shield.block",
              "disabled_sound": "minecraft:item.shield.break"})
    return s


def condition_seeds():
    return [{"max_duration_ticks": 10}, {"max_duration_ticks": 0}, {"max_duration_ticks": -1}, {"max_duration_ticks": 10, "min_speed": 1.5}, {"max_duration_ticks": 10, "min_relative_speed": 0.5},
            {"max_duration_ticks": 10, "min_speed": -1.0, "min_relative_speed": -1.0}, {"max_duration_ticks": 10, "min_speed": "x"}, {"min_speed": 1.0}, {},
            {"max_duration_ticks": 1.5}, {"max_duration_ticks": "x"}, {"max_duration_ticks": 2**31}, {"max_duration_ticks": 10, "min_speed": 1e39}, None, [], 5]


def kinetic_weapon_seeds(rng):
    s = [{}, {"contact_cooldown_ticks": 5}, {"contact_cooldown_ticks": 0}, {"contact_cooldown_ticks": -1}, {"delay_ticks": 12}, {"delay_ticks": -1},
         {"forward_movement": 0.38}, {"forward_movement": -0.0}, {"forward_movement": 0}, {"forward_movement": 1e39}, {"forward_movement": "x"}, {"forward_movement": -3},
         {"damage_multiplier": 0.7}, {"damage_multiplier": 1}, {"damage_multiplier": -0.0}, {"damage_multiplier": 0}, {"damage_multiplier": None},
         {"sound": "minecraft:item.trident.hit"}, {"hit_sound": "minecraft:entity.arrow.hit"}, {"sound": {"sound_id": "a:b", "range": 2}}, {"sound": "nope"}, {"sound": 5},
         {"hit_sound": {}}, {"unknown": 1}, [], None, 5, "x"]
    for c in condition_seeds():
        for key in ("dismount_conditions", "knockback_conditions", "damage_conditions"):
            s.append({key: c})
    s.append({"contact_cooldown_ticks": 10, "delay_ticks": 15, "dismount_conditions": {"max_duration_ticks": 300, "min_speed": 14.0},
              "knockback_conditions": {"max_duration_ticks": 200, "min_speed": 5.1}, "damage_conditions": {"max_duration_ticks": 300, "min_relative_speed": 4.6},
              "forward_movement": 0.38, "damage_multiplier": 0.7, "sound": "minecraft:item.trident.hit", "hit_sound": "minecraft:entity.arrow.hit"})
    return s


def piercing_weapon_seeds(rng):
    s = [{}, {"deals_knockback": False}, {"deals_knockback": True}, {"dismounts": True}, {"dismounts": False}, {"deals_knockback": 1}, {"dismounts": "true"}, {"deals_knockback": None},
         {"sound": "minecraft:item.trident.hit"}, {"hit_sound": "minecraft:entity.arrow.hit"}, {"sound": {"sound_id": "a:b"}}, {"sound": {"sound_id": "a:b", "range": 7.5}},
         {"sound": {"sound_id": "a:b", "range": "bad"}}, {"sound": "minecraft:nope"}, {"sound": 5}, {"sound": None}, {"sound": {}}, {"sound": ""}, {"sound": "A:B"},
         {"deals_knockback": False, "dismounts": True, "sound": "minecraft:item.shield.block", "hit_sound": {"sound_id": "mymod:hit", "range": 1.0}}, {"unknown": 1}, [], None, 5, "x"]
    for snd in SOUNDS[:4]:
        s.append({"sound": "minecraft:" + snd, "hit_sound": "minecraft:" + SOUNDS[-1]})
    return s


def swing_seeds(rng):
    s = [{}, {"type": "none"}, {"type": "whack"}, {"type": "stab"}, {"type": "whack", "duration": 6}, {"type": "stab", "duration": 13}, {"duration": 0}, {"duration": 1}, {"duration": -1},
         {"duration": 2**31 - 1}, {"duration": 2**31}, {"duration": 1.5}, {"duration": "6"}, {"duration": None}, {"duration": [6]}, {"type": "STAB"}, {"type": "poke"}, {"type": 1},
         {"type": None}, {"type": ["stab"]}, {"unknown": 1}, [], None, 5, "x", {"type": "none", "duration": 0}, {"type": "stab", "duration": 255}, {"type": "whack", "duration": 6, "extra": True}]
    return s


def damage_resistant_seeds(rng):
    s = [{"types": d} for d in damage_sets(rng)]
    s += [{}, {"types": None}, {"type": "#minecraft:is_fire"}, {"types": "#minecraft:is_fire", "extra": 1}, "#minecraft:is_fire", ["minecraft:fall"], None, 5, []]
    return s


def sound_variants(rng):
    return ["minecraft:" + rng.choice(SOUNDS), {"sound_id": "mymod:s"}, {"sound_id": "mymod:s", "range": 4.0}, {"sound_id": "mymod:s", "range": "x"}, "minecraft:nope", 5, None, {}, "", {"range": 3}]


def entity_sets(rng):
    s = ["minecraft:horse", ["minecraft:horse", "minecraft:llama"], "#minecraft:skeletons", "#minecraft:nope", [], "horse", ["horse"], "", None, 5, {}, True, ["minecraft:nope"],
         ["minecraft:horse", "minecraft:horse"], "minecraft:nope", ["#minecraft:skeletons"]]
    s += ["minecraft:" + b for b in rng.sample(ENTITIES, 3)]
    s += ["#minecraft:" + b for b in rng.sample(ENTITY_TAGS, 3)]
    return s


def equippable_seeds(rng):
    s = [{"slot": sl} for sl in SLOTS]
    s += [{}, {"slot": "nope"}, {"slot": "MAINHAND"}, {"slot": 0}, {"slot": None}, {"slot": ["head"]}, {"slot": "any"}, {"slot": "hand"}, {"slot": "armor"}, [], None, 5, "head",
          {"slot": "head", "unknown": 1}]
    for snd in sound_variants(rng):
        s.append({"slot": "head", "equip_sound": snd})
        s.append({"slot": "feet", "shearing_sound": snd})
    s.append({"slot": "head", "equip_sound": "minecraft:item.armor.equip_generic"})
    s.append({"slot": "head", "shearing_sound": "minecraft:item.shears.snip"})
    for aid in ["minecraft:iron", "iron", "mymod:thing/x", "", "A:B", 5, None, ["minecraft:iron"], {}, "minecraft:", "a b"]:
        s.append({"slot": "chest", "asset_id": aid})
        s.append({"slot": "head", "camera_overlay": aid})
    for ents in entity_sets(rng):
        s.append({"slot": "body", "allowed_entities": ents})
    for key in ("dispensable", "swappable", "damage_on_hurt", "equip_on_interact", "can_be_sheared"):
        for val in [True, False, 1, 0, "true", None, [True]]:
            s.append({"slot": "head", key: val})
    s.append({"slot": "body", "equip_sound": "minecraft:entity.horse.armor", "asset_id": "minecraft:saddle", "camera_overlay": "minecraft:misc/pumpkinblur",
              "allowed_entities": "#minecraft:can_equip_saddle", "dispensable": False, "swappable": False, "damage_on_hurt": False, "equip_on_interact": True,
              "can_be_sheared": True, "shearing_sound": "minecraft:item.shears.snip"})
    s.append({"slot": "saddle", "allowed_entities": ["minecraft:horse"], "equip_on_interact": True})
    return s


def main():
    rng = random.Random(30)
    out = Out()
    emit_component(out, rng, "tool", tool_seeds(rng), ["diamond_pickaxe", "stick", "shears"], muts=2)
    emit_component(out, rng, "weapon", weapon_seeds(rng), ["diamond_sword", "stick", "mace"])
    emit_component(out, rng, "attack_range", attack_range_seeds(rng), ["trident", "stick", "mace"])
    emit_component(out, rng, "blocks_attacks", blocks_attacks_seeds(rng), ["shield", "stick"], muts=2)
    emit_component(out, rng, "kinetic_weapon", kinetic_weapon_seeds(rng), ["iron_spear", "stick"], muts=2)
    emit_component(out, rng, "piercing_weapon", piercing_weapon_seeds(rng), ["iron_spear", "stick"])
    emit_component(out, rng, "attack_animation", swing_seeds(rng), ["iron_spear", "diamond_sword", "stick"])
    emit_component(out, rng, "interact_animation", swing_seeds(rng), ["iron_spear", "stick", "spyglass"])
    emit_component(out, rng, "damage_resistant", damage_resistant_seeds(rng), ["netherite_ingot", "diamond", "stick"])
    emit_component(out, rng, "equippable", equippable_seeds(rng), ["leather_helmet", "saddle", "stick", "pumpkin"], muts=2)
    for name, txt in [("tool", '{rules:[{blocks:"minecraft:stone",speed:2f,correct_for_drops:1b}],default_mining_speed:1,damage_per_block:2b}'),
                      ("tool", '{rules:[{blocks:"minecraft:stone",speed:2,correct_for_drops:true}]}'), ("tool", '{rules:[{blocks:["minecraft:stone"],speed:NaNf}]}'),
                      ("tool", '{rules:[],default_mining_speed:NaNf}'), ("tool", '{rules:[],default_mining_speed:-Infinityd}'), ("tool", '{rules:[],damage_per_block:1.9f}'),
                      ("weapon", '{item_damage_per_attack:2L,disable_blocking_for_seconds:5}'), ("weapon", '{disable_blocking_for_seconds:NaNf}'), ("weapon", '{disable_blocking_for_seconds:-0.0f}'),
                      ("attack_range", '{min_reach:1,max_reach:3b,hitbox_margin:0.5d}'), ("attack_range", '{mob_factor:NaNf}'), ("attack_range", '{min_reach:Infinityf}'),
                      ("blocks_attacks", '{damage_reductions:[{base:0,factor:1}],item_damage:{threshold:1,base:0,factor:1}}'),
                      ("blocks_attacks", '{damage_reductions:[{base:-0.0f,factor:1.0f,horizontal_blocking_angle:90.0f}]}'),
                      ("blocks_attacks", '{damage_reductions:[{base:NaNf,factor:1}]}'), ("blocks_attacks", '{item_damage:{threshold:1.0f,base:-0.0f,factor:1.0f}}'),
                      ("kinetic_weapon", '{forward_movement:-0.0f}'), ("kinetic_weapon", '{damage_multiplier:NaNd,delay_ticks:3b}'),
                      ("kinetic_weapon", '{damage_conditions:{max_duration_ticks:5b,min_speed:-0.0f}}'),
                      ("attack_animation", '{type:"stab",duration:6b}'), ("attack_animation", '{duration:6L}'), ("attack_animation", '{type:"whack",duration:6.5f}'),
                      ("damage_resistant", '{types:"#minecraft:is_fire"}'), ("damage_resistant", '{types:["minecraft:fall"]}'), ("damage_resistant", '{types:[]}'),
                      ("equippable", '{slot:"head",dispensable:0b}'), ("equippable", '{slot:"head",dispensable:2b}'), ("equippable", '{slot:"head",asset_id:"minecraft:iron"}'),
                      ("equippable", '{slot:"head",equip_sound:{sound_id:"a:b",range:1.5f}}'), ("equippable", '{slot:"head",equip_sound:{sound_id:"a:b",range:"x"}}')]:
        out.add("snbt", "minecraft:" + name, txt)
    for name, txt in [("tool", '{"rules":[],"rules":[{"blocks":"minecraft:stone"}]}'), ("weapon", '{"item_damage_per_attack":1,"item_damage_per_attack":2}'),
                      ("equippable", '{"slot":"head","slot":"feet"}'), ("tool", '{"rules":[{"blocks":"minecraft:stone","speed":1.0e0}]}'),
                      ("attack_range", '{"min_reach":0.0,"max_reach":+3}'), ("weapon", '{"disable_blocking_for_seconds":1E1}')]:
        out.add("json", "minecraft:" + name, txt)
    for name, hx in [("equippable", "63010000000001010100000100" + "01"), ("equippable", "00010000000001010100000100" + "01"),
                     ("attack_animation", "6306"), ("attack_animation", "0206"), ("attack_animation", "0100"), ("weapon", "0200000000"), ("weapon", "02400000000"),
                     ("attack_range", "00000000" * 6), ("tool", "00" + "3f800000" + "01" + "01")]:
        out.add("wire", "minecraft:" + name, hx)
    write_corpus("30-combat.txt", out.lines)


if __name__ == "__main__":
    main()
