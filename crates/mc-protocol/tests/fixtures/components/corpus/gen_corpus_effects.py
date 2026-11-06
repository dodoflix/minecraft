#!/usr/bin/env python3
"""Generates corpus/20-effects.txt (components: attribute_modifiers, potion_contents, suspicious_stew_effects,
consumable, death_protection, food, use_effects, use_cooldown, use_remainder).

Hand-written seeds (valid + invalid + shorthand forms) plus seeded mutations (type confusion, dropped/added keys,
number edge cases, list/scalar shape changes) emitted as JSON and as SNBT with randomly typed numbers, raw wire
hex, and `stack-snbt` / `stack-json` / `patch-json` records embedding the values.
Also imported by gen_corpus_combat.py (helpers below).

    python3 gen_corpus_effects.py > 20-effects.txt     (deterministic)
"""
import json
import random
import sys

# ---------------------------------------------------------------------------------------------------------------
# helpers shared with gen_corpus_combat.py
# ---------------------------------------------------------------------------------------------------------------

POOL = [None, True, False, 0, 1, -1, 2, 7, 255, 256, 2**31 - 1, 2**31, -2**31, 2**53, 2**63, 0.0, -0.0, 0.5, 1.5, -1.5, 0.1,
        1e-45, 3.4028235e38, 1e39, 1e300, "", "x", " ", "minecraft:stone", "minecraft:", ":", "#minecraft:logs", "Minecraft:Stone",
        "a b", "minecraft:speed", [], {}, [1], ["minecraft:stone"], {"a": 1}, [[]], [None], {"type": "minecraft:x"}]


def paths(v, p=()):
    yield p
    if isinstance(v, dict):
        for k in v:
            yield from paths(v[k], p + (k,))
    elif isinstance(v, list):
        for i, x in enumerate(v):
            yield from paths(x, p + (i,))


def get(v, p):
    for k in p:
        v = v[k]
    return v


def put(v, p, x):
    if not p:
        return x
    c = get(v, p[:-1])
    c[p[-1]] = x
    return v


def copy(v):
    return json.loads(json.dumps(v))


def mutate(rng, v):
    """One random structural / type / value mutation of a JSON-like value."""
    v = copy(v)
    for _ in range(rng.choice([1, 1, 1, 2, 3])):
        ps = list(paths(v))
        p = rng.choice(ps)
        cur = get(v, p)
        op = rng.randrange(9)
        if op == 0:
            v = put(v, p, copy(rng.choice(POOL)))
        elif op == 1 and isinstance(cur, dict) and cur:
            del cur[rng.choice(list(cur))]
        elif op == 2 and isinstance(cur, dict):
            cur[rng.choice(["unknown", "type", "id", "value", "amount", "duration", "x"])] = copy(rng.choice(POOL))
        elif op == 3 and isinstance(cur, list) and cur:
            cur.append(copy(rng.choice(cur)))
        elif op == 4 and isinstance(cur, list):
            v = put(v, p, [])
        elif op == 5:
            v = put(v, p, [cur])
        elif op == 6 and isinstance(cur, list) and len(cur) == 1:
            v = put(v, p, cur[0])
        elif op == 7 and isinstance(cur, str):
            t = rng.choice(["strip", "upper", "ns", "space", "trail", "dup"])
            if t == "strip":
                cur = cur.replace("minecraft:", "")
            elif t == "upper":
                cur = cur.upper()
            elif t == "ns":
                cur = "mymod:" + cur.replace("minecraft:", "")
            elif t == "space":
                cur = cur + " "
            elif t == "trail":
                cur = cur + "_x"
            else:
                cur = cur + cur
            v = put(v, p, cur)
        elif op == 8 and isinstance(cur, (int, float)) and not isinstance(cur, bool):
            v = put(v, p, rng.choice([0, 1, -1, 2, 127, 128, 255, 256, 32767, 65536, 2**31 - 1, 2**31, -2**31, 2**40, 0.5, -0.5, 1.0e10,
                                      3.4028235e38, 1e39, 1e-10, 100, 1000, 20.0, 64.0, 64.5, 90.0, 180.0]))
        else:
            v = put(v, p, copy(rng.choice(POOL)))
    return v


def js(v):
    return json.dumps(v, separators=(",", ":"), allow_nan=False)


def snbt_str(s, rng):
    if rng.random() < 0.15 and s and all(c.isalnum() or c in "_-.+" for c in s) and not s[0].isdigit() and s[0] not in "+-." and s not in ("true", "false"):
        return s
    q = rng.choice(['"', "'"])
    e = s.replace("\\", "\\\\").replace(q, "\\" + q)
    return q + e + q


def snbt_num(x, rng):
    if isinstance(x, int):
        opts = [""]
        if -128 <= x <= 127:
            opts += ["b", "B"]
        if -32768 <= x <= 32767:
            opts += ["s"]
        if -2**31 <= x < 2**31:
            opts += [""]
        if -2**63 <= x < 2**63:
            opts += ["L", "l"]
        opts += ["f", "d"]
        suf = rng.choice(opts)
        if suf == "" and not -2**31 <= x < 2**31:
            suf = "L"
        if suf in ("f", "d", "F", "D") and rng.random() < 0.5:
            return f"{x}.0{suf}"
        return f"{x}{suf}"
    f = float(x)
    if f != f:
        return rng.choice(["NaNf", "NaNd"])
    if f in (float("inf"), float("-inf")):
        return ("-" if f < 0 else "") + rng.choice(["Infinityf", "Infinityd"])
    r = repr(f)
    return r + rng.choice(["f", "d", "", "F", "D"])


def snbt(v, rng):
    if v is None:
        return rng.choice(['""', "{}", "[]"])
    if isinstance(v, bool):
        return ("true" if v else "false") if rng.random() < 0.5 else ("1b" if v else "0b")
    if isinstance(v, (int, float)):
        return snbt_num(v, rng)
    if isinstance(v, str):
        return snbt_str(v, rng)
    if isinstance(v, list):
        return "[" + ",".join(snbt(x, rng) for x in v) + "]"
    parts = []
    for k, x in v.items():
        parts.append(snbt_str(k, rng) + ":" + snbt(x, rng))
    return "{" + ",".join(parts) + "}"


def varint(n):
    n &= 0xFFFFFFFF
    out = bytearray()
    while True:
        if n & ~0x7F == 0:
            out.append(n)
            return bytes(out)
        out.append((n & 0x7F) | 0x80)
        n >>= 7


def random_wire(rng):
    n = rng.randrange(1, 40)
    return bytes(rng.choice([0, 0, 0, 1, 1, 2, 3, 4, 0x3f, 0x80, rng.randrange(256)]) for _ in range(n)).hex()


class Out:
    def __init__(self):
        self.lines = []

    def add(self, kind, name, payload):
        assert "\t" not in payload and "\n" not in payload, payload
        self.lines.append(f"{kind}\t{name}\t{payload}")


def emit_component(out, rng, comp, seeds, items, muts=3, wires=14):
    """`seeds`: list of python values (valid or not). `items`: item ids a stack embedding the component may use."""
    name = "minecraft:" + comp
    for s in seeds:
        out.add("json", name, js(s))
        out.add("snbt", name, snbt(s, rng))
        for _ in range(muts):
            m = mutate(rng, s)
            if rng.random() < 0.5:
                out.add("json", name, js(m))
            else:
                out.add("snbt", name, snbt(m, rng))
    for _ in range(wires):
        out.add("wire", name, random_wire(rng))
    for s in rng.sample(seeds, min(len(seeds), 8)):
        item = rng.choice(items)
        out.add("stack-snbt", "-", '{id:"minecraft:%s",count:%d,components:{"%s":%s}}' % (item, rng.choice([1, 1, 2, 16]), name, snbt(s, rng)))
    for s in rng.sample(seeds, min(len(seeds), 3)):
        out.add("stack-json", "-", js({"id": "minecraft:" + rng.choice(items), "count": 1, "components": {name: s}}))
        out.add("patch-json", "-", js({name: s}))
    out.add("stack-snbt", "-", '{id:"minecraft:%s",count:1,components:{"!%s":{}}}' % (items[0], name))
    out.add("patch-json", "-", js({"!" + name: {}}))
    out.add("patch-json", "-", js({"!" + name: 1}))


# ---------------------------------------------------------------------------------------------------------------
# seeds
# ---------------------------------------------------------------------------------------------------------------

ATTRS = ["armor", "armor_toughness", "attack_damage", "attack_knockback", "attack_speed", "block_break_speed", "block_interaction_range",
         "burning_time", "entity_interaction_range", "explosion_knockback_resistance", "fall_damage_multiplier", "flying_speed",
         "follow_range", "gravity", "jump_strength", "knockback_resistance", "luck", "max_absorption", "max_health", "mining_efficiency",
         "movement_efficiency", "movement_speed", "oxygen_bonus", "safe_fall_distance", "scale", "sneaking_speed", "step_height",
         "submerged_mining_speed", "sweeping_damage_ratio", "water_movement_efficiency", "tempt_range", "waypoint_transmit_range"]
SLOTS = ["any", "mainhand", "offhand", "hand", "feet", "legs", "chest", "head", "armor", "body", "saddle"]
OPS = ["add_value", "add_multiplied_base", "add_multiplied_total"]
EFFECTS = ["speed", "slowness", "haste", "mining_fatigue", "strength", "instant_health", "instant_damage", "jump_boost", "nausea",
           "regeneration", "resistance", "fire_resistance", "water_breathing", "invisibility", "blindness", "night_vision", "hunger",
           "weakness", "poison", "wither", "health_boost", "absorption", "saturation", "glowing", "levitation", "luck", "unluck",
           "slow_falling", "conduit_power", "dolphins_grace", "bad_omen", "hero_of_the_village", "darkness", "infested", "oozing", "weaving", "wind_charged"]
POTIONS = ["water", "mundane", "thick", "awkward", "night_vision", "long_night_vision", "invisibility", "leaping", "strong_leaping",
           "fire_resistance", "swiftness", "long_swiftness", "strong_swiftness", "slowness", "water_breathing", "healing", "strong_healing",
           "harming", "poison", "long_poison", "regeneration", "strength", "weakness", "luck", "turtle_master", "slow_falling",
           "wind_charged", "weaving", "oozing", "infested", "empty"]
SOUNDS = ["entity.generic.eat", "entity.generic.drink", "item.armor.equip_iron", "item.totem.use", "block.anvil.land", "entity.player.burp",
          "entity.experience_orb.pickup", "item.shield.block", "entity.chorus_fruit.teleport", "item.honey_bottle.drink", "item.trident.hit"]
ANIMS = ["none", "eat", "drink", "block", "bow", "trident", "crossbow", "spyglass", "toot_horn", "brush", "bundle", "spear"]


def mi(effect="speed", **kw):
    d = {"id": "minecraft:" + effect}
    d.update(kw)
    return d


def text_forms():
    return ["hello", {"text": "hi", "color": "red"}, ["a", {"text": "b", "bold": True}], {"translate": "item.minecraft.apple"}, 5, True,
            {"text": "x", "extra": ["y"]}, "", {"nbt": "x", "block": "0 0 0"}, {"selector": "@p"}, {}, [], None]


def attribute_modifiers_seeds(rng):
    s = [[], [{}]]
    base = {"type": "minecraft:attack_damage", "id": "minecraft:base_attack_damage", "amount": 5.0, "operation": "add_value"}
    s.append([dict(base, slot="mainhand")])
    s.append([base])
    s.append(base)  # not a list
    s.append([{"type": "attack_damage", "id": "mymod:x", "amount": -2, "operation": "add_multiplied_base", "slot": "offhand"}])
    for slot in SLOTS:
        s.append([{"type": "minecraft:armor", "id": "minecraft:armor.x", "amount": 3.0, "operation": "add_value", "slot": slot}])
    for op in OPS + ["add", "ADD_VALUE", 0, 1, ""]:
        s.append([{"type": "minecraft:luck", "id": "a:b", "amount": 1.0, "operation": op}])
    for a in rng.sample(ATTRS, 12):
        s.append([{"type": "minecraft:" + a, "id": "t:" + a, "amount": round(rng.uniform(-5, 5), 3), "operation": rng.choice(OPS), "slot": rng.choice(SLOTS)}])
    for amount in [0, 1, -1.5, 1e300, -1e300, 5e-324, 123456789012, "5", "x", True, None, 0.1, 2**63, [1]]:
        s.append([{"type": "minecraft:max_health", "id": "x:y", "amount": amount, "operation": "add_value"}])
    for disp in [{"type": "default"}, {"type": "hidden"}, {"type": "override", "value": "text"}, {"type": "override", "value": {"text": "t", "color": "gold"}},
                 {"type": "override"}, {"type": "unknown"}, {"type": "hidden", "extra": 1}, "hidden", [], {}, None, {"type": 1}, {"type": "override", "value": 5},
                 {"type": "override", "value": ["a", "b"]}, {"type": "default", "value": "ignored"}, {"type": "override", "value": {"text": ""}}]:
        s.append([dict(base, display=disp)])
    for tv in text_forms():
        s.append([dict(base, display={"type": "override", "value": tv})])
    for bad in [{"id": "a:b", "amount": 1, "operation": "add_value"}, {"type": "minecraft:luck", "amount": 1, "operation": "add_value"},
                {"type": "minecraft:luck", "id": "a:b", "operation": "add_value"}, {"type": "minecraft:luck", "id": "a:b", "amount": 1},
                {"type": "minecraft:nope", "id": "a:b", "amount": 1, "operation": "add_value"},
                {"type": "minecraft:luck", "id": "A:B", "amount": 1, "operation": "add_value"},
                {"type": "minecraft:luck", "id": "", "amount": 1, "operation": "add_value"},
                {"type": 5, "id": "a:b", "amount": 1, "operation": "add_value"},
                {"type": "minecraft:luck", "id": "a:b", "amount": 1, "operation": "add_value", "slot": "nowhere"},
                {"type": "minecraft:luck", "id": "a:b", "amount": 1, "operation": "add_value", "slot": 3},
                {"type": "minecraft:luck", "id": "a:b", "amount": 1, "operation": "add_value", "unknown": True}]:
        s.append([bad])
    e = {"type": "minecraft:luck", "id": "a:b", "amount": 1.0, "operation": "add_value"}
    s.append([e, e])
    s.append([e, dict(e, id="a:c", slot="head"), dict(e, type="minecraft:armor", display={"type": "hidden"})])
    s.append([e] * 6)
    s.append([None])
    s.append("nope")
    s.append(5)
    s.append(None)
    return s


def effect_instance_seeds(rng):
    e = []
    for eff in rng.sample(EFFECTS, 10):
        e.append({"id": "minecraft:" + eff, "amplifier": rng.choice([0, 1, 2, 4, 127, 255]), "duration": rng.choice([0, 1, 200, 3600, -1, 2**31 - 1]),
                  "ambient": rng.random() < 0.3, "show_particles": rng.random() < 0.7})
    e.append({"id": "minecraft:speed"})
    e.append({"id": "minecraft:speed", "show_icon": False})
    e.append({"id": "minecraft:speed", "show_particles": False})
    e.append({"id": "minecraft:speed", "show_particles": False, "show_icon": True})
    e.append({"id": "minecraft:speed", "hidden_effect": {"amplifier": 3, "duration": 10}})
    e.append({"id": "minecraft:speed", "amplifier": 1, "hidden_effect": {"amplifier": 3, "duration": 10, "hidden_effect": {"amplifier": 4, "show_icon": False}}})
    e.append({"id": "minecraft:speed", "hidden_effect": {"id": "minecraft:slowness", "amplifier": 3}})
    e.append({"id": "minecraft:speed", "hidden_effect": {}})
    e.append({"id": "minecraft:speed", "hidden_effect": 5})
    e.append({"id": "minecraft:speed", "hidden_effect": None})
    for a in [-1, 255, 256, 257, 300, 2**31 - 1, 1.5, "2", True, None, -256, 511, 65535]:
        e.append({"id": "minecraft:speed", "amplifier": a})
    for d in [-1, -2, 0, 1.5, "200", True, None, 2**31, 2**31 - 1, -2**31]:
        e.append({"id": "minecraft:speed", "duration": d})
    for bad in [{}, {"amplifier": 1}, {"id": "minecraft:nope"}, {"id": 5}, {"id": "speed"}, {"id": ""}, {"id": "minecraft:speed", "ambient": "yes"},
                {"id": "minecraft:speed", "show_particles": 1}, {"id": "minecraft:speed", "show_icon": None}, [], "minecraft:speed", None, 5]:
        e.append(bad)
    return e


def potion_contents_seeds(rng):
    s = ["minecraft:water", "healing", "minecraft:nope", "", 5, None, [], True, "Minecraft:Water"]
    s += ["minecraft:" + p for p in rng.sample(POTIONS, 10)]
    s += [{}, {"potion": "minecraft:healing"}, {"potion": "strong_healing", "custom_color": 16711680}, {"custom_color": -1}, {"custom_color": 0},
          {"custom_color": 2**31}, {"custom_color": 1.5}, {"custom_color": "red"}, {"custom_color": 16777215.0}, {"custom_name": "my"}, {"custom_name": ""},
          {"custom_name": 5}, {"potion": "nope"}, {"potion": 5}, {"potion": None}, {"potion": ["minecraft:water"]}, {"custom_effects": []},
          {"custom_effects": {}}, {"custom_effects": "minecraft:speed"}, {"custom_effects": [None]}, {"unknown": 1}]
    effs = effect_instance_seeds(rng)
    for e in effs:
        s.append({"custom_effects": [e]})
    s.append({"potion": "minecraft:awkward", "custom_color": 255, "custom_effects": [effs[0], effs[1], effs[2]], "custom_name": "brew"})
    s.append({"potion": "minecraft:awkward", "custom_effects": [effs[0]] * 4})
    return s


def stew_seeds(rng):
    s = [[], [{"id": "minecraft:night_vision"}], [{"id": "minecraft:blindness", "duration": 200}], [{"id": "minecraft:blindness", "duration": "x"}],
         [{"id": "minecraft:blindness", "duration": 1.5}], [{"id": "minecraft:blindness", "duration": -5}], [{"id": "minecraft:blindness", "duration": None}],
         [{"id": "minecraft:blindness", "duration": 2**31}], [{"id": "minecraft:blindness", "duration": True}], [{"id": "minecraft:blindness", "duration": [1]}],
         [{"duration": 5}], [{"id": "minecraft:nope"}], [{"id": "speed"}], [{"id": 5}], [{}], {}, "x", None, 5,
         [{"id": "minecraft:speed", "duration": 100, "amplifier": 3}], [{"id": "minecraft:speed", "unknown": 1}], [None], [[]],
         [{"id": "minecraft:speed"}, {"id": "minecraft:speed"}, {"id": "minecraft:poison", "duration": 0}]]
    for eff in rng.sample(EFFECTS, 8):
        s.append([{"id": "minecraft:" + eff, "duration": rng.choice([1, 160, 600, 1200, 0, -1])}])
    return s


def consume_effect_seeds(rng):
    s = []
    for p in [None, 0.0, 1.0, 0.5, -0.1, 1.1, "x", 1, True, 0.3333333, 1e-10, 2]:
        d = {"type": "minecraft:apply_effects", "effects": [{"id": "minecraft:poison", "duration": 100}]}
        if p is not None:
            d["probability"] = p
        s.append(d)
    s += [{"type": "apply_effects", "effects": []}, {"type": "minecraft:apply_effects"}, {"type": "minecraft:apply_effects", "effects": {}},
          {"type": "minecraft:apply_effects", "effects": [{"id": "minecraft:poison"}, {"id": "minecraft:wither", "amplifier": 2, "hidden_effect": {"duration": 5}}]},
          {"type": "minecraft:remove_effects", "effects": "minecraft:poison"}, {"type": "minecraft:remove_effects", "effects": ["minecraft:poison", "minecraft:wither"]},
          {"type": "minecraft:remove_effects", "effects": []}, {"type": "minecraft:remove_effects", "effects": "#minecraft:nope"},
          {"type": "minecraft:remove_effects", "effects": ["minecraft:poison", "minecraft:nope"]}, {"type": "minecraft:remove_effects"},
          {"type": "minecraft:remove_effects", "effects": 5}, {"type": "minecraft:remove_effects", "effects": ["minecraft:poison"]},
          {"type": "minecraft:remove_effects", "effects": ["minecraft:poison", "minecraft:poison"]},
          {"type": "minecraft:clear_all_effects"}, {"type": "minecraft:clear_all_effects", "extra": 1}, {"type": "clear_all_effects"},
          {"type": "minecraft:teleport_randomly"}, {"type": "minecraft:teleport_randomly", "diameter": 8.0}, {"type": "minecraft:teleport_randomly", "diameter": 0},
          {"type": "minecraft:teleport_randomly", "diameter": -3}, {"type": "minecraft:teleport_randomly", "diameter": 16}, {"type": "minecraft:teleport_randomly", "diameter": 16.0, "directional_particles": True},
          {"type": "minecraft:teleport_randomly", "directional_particles": False}, {"type": "minecraft:teleport_randomly", "directional_particles": 1},
          {"type": "minecraft:teleport_randomly", "diameter": 1e30}, {"type": "minecraft:teleport_randomly", "diameter": "x"},
          {"type": "minecraft:play_sound", "sound": "minecraft:entity.generic.eat"}, {"type": "minecraft:play_sound", "sound": {"sound_id": "mymod:s"}},
          {"type": "minecraft:play_sound", "sound": {"sound_id": "mymod:s", "range": 12.5}}, {"type": "minecraft:play_sound", "sound": {"sound_id": "mymod:s", "range": "x"}},
          {"type": "minecraft:play_sound", "sound": {"range": 5}}, {"type": "minecraft:play_sound", "sound": "minecraft:nope"}, {"type": "minecraft:play_sound"},
          {"type": "minecraft:play_sound", "sound": 5}, {"type": "minecraft:play_sound", "sound": ""},
          {"type": "minecraft:nope"}, {"type": 5}, {}, {"effects": []}, "minecraft:clear_all_effects", [], None]
    for snd in rng.sample(SOUNDS, 3):
        s.append({"type": "minecraft:play_sound", "sound": "minecraft:" + snd})
    return s


def consumable_seeds(rng):
    ce = consume_effect_seeds(rng)
    s = [{}, {"consume_seconds": 0}, {"consume_seconds": 0.5}, {"consume_seconds": 1.6}, {"consume_seconds": -1}, {"consume_seconds": -0.0}, {"consume_seconds": 3.4028235e38},
         {"consume_seconds": 1e39}, {"consume_seconds": "x"}, {"consume_seconds": None}, {"consume_seconds": 32}, {"has_consume_particles": False}, {"has_consume_particles": True},
         {"has_consume_particles": 1}, {"animation": "nope"}, {"animation": 1}, {"animation": "EAT"}, {"unknown": 1}, [], None, "x", 5,
         {"sound": "minecraft:entity.generic.eat"}, {"sound": "minecraft:nope"}, {"sound": {"sound_id": "a:b"}}, {"sound": {"sound_id": "a:b", "range": 3}},
         {"sound": {"sound_id": "a:b", "range": "x"}}, {"sound": {}}, {"sound": 5}, {"sound": None}, {"sound": ""},
         {"on_consume_effects": []}, {"on_consume_effects": {}}, {"on_consume_effects": "x"}, {"on_consume_effects": [None]}]
    for a in ANIMS:
        s.append({"animation": a})
    for snd in SOUNDS[:5]:
        s.append({"sound": "minecraft:" + snd, "animation": rng.choice(ANIMS), "consume_seconds": rng.choice([0.5, 1.6, 2.0, 0.0])})
    for e in ce:
        s.append({"on_consume_effects": [e]})
    s.append({"consume_seconds": 2.0, "animation": "drink", "sound": "minecraft:item.honey_bottle.drink", "has_consume_particles": False,
              "on_consume_effects": [ce[0], ce[-1], {"type": "minecraft:clear_all_effects"}]})
    return s


def death_protection_seeds(rng):
    ce = consume_effect_seeds(rng)
    s = [{}, {"death_effects": []}, None, [], "x", 5, {"death_effects": {}}, {"death_effects": "x"}, {"death_effects": [None]}, {"death_effects": [{}]},
         {"unknown": []}, {"death_effects": [{"type": "minecraft:clear_all_effects"}, {"type": "minecraft:apply_effects", "effects": [
             {"id": "minecraft:regeneration", "duration": 900, "amplifier": 1}, {"id": "minecraft:absorption", "duration": 100, "amplifier": 1},
             {"id": "minecraft:fire_resistance", "duration": 800}]}]}]
    for e in ce:
        s.append({"death_effects": [e]})
    return s


def food_seeds(rng):
    s = [{"nutrition": 4, "saturation": 2.4}, {"nutrition": 0, "saturation": 0}, {"nutrition": 20, "saturation": 12.0, "can_always_eat": True},
         {"nutrition": 1, "saturation": -1.5}, {"nutrition": -1, "saturation": 1}, {"nutrition": 2**31 - 1, "saturation": 3.4028235e38}, {"nutrition": 2**31, "saturation": 1},
         {"nutrition": 1.5, "saturation": 1}, {"nutrition": "4", "saturation": 1}, {"nutrition": 4, "saturation": "x"}, {"nutrition": 4}, {"saturation": 1}, {},
         {"nutrition": 4, "saturation": 1, "can_always_eat": 1}, {"nutrition": 4, "saturation": 1, "can_always_eat": "true"}, {"nutrition": 4, "saturation": 1, "can_always_eat": None},
         {"nutrition": 4, "saturation": 1e39}, {"nutrition": 4, "saturation": -0.0}, {"nutrition": 4, "saturation": 0.1}, {"nutrition": 4, "saturation": 1, "extra": 5},
         [], None, 5, "x", [4, 2.4], {"nutrition": [4], "saturation": 1}, {"nutrition": 4, "saturation": 1, "can_always_eat": False}]
    for _ in range(8):
        s.append({"nutrition": rng.randrange(0, 20), "saturation": round(rng.uniform(0, 14), 2), "can_always_eat": rng.random() < 0.3})
    return s


def use_effects_seeds(rng):
    s = [{}, {"can_sprint": True}, {"interact_vibrations": False}, {"speed_multiplier": 0.5}, {"speed_multiplier": 0}, {"speed_multiplier": 1}, {"speed_multiplier": 1.0},
         {"speed_multiplier": 0.2}, {"speed_multiplier": 0.2000000029802322}, {"speed_multiplier": -0.0}, {"speed_multiplier": -0.1}, {"speed_multiplier": 1.1},
         {"speed_multiplier": "x"}, {"speed_multiplier": None}, {"speed_multiplier": 1e39}, {"can_sprint": 1}, {"can_sprint": "true"}, {"can_sprint": None},
         {"can_sprint": False, "interact_vibrations": True, "speed_multiplier": 0.2}, {"can_sprint": True, "interact_vibrations": False, "speed_multiplier": 1.0},
         [], None, 5, "x", {"unknown": 1}, {"can_sprint": [True]}]
    return s


def use_cooldown_seeds(rng):
    s = [{"seconds": 1.5}, {"seconds": 1}, {"seconds": 0}, {"seconds": -1}, {"seconds": 1e-45}, {"seconds": 3.4028235e38}, {"seconds": 1e39}, {"seconds": "x"}, {"seconds": None},
         {"seconds": 1.5, "cooldown_group": "minecraft:ender_pearl"}, {"seconds": 1.5, "cooldown_group": "mymod:group/a"}, {"seconds": 1.5, "cooldown_group": ""},
         {"seconds": 1.5, "cooldown_group": "A:B"}, {"seconds": 1.5, "cooldown_group": 5}, {"seconds": 1.5, "cooldown_group": None}, {"seconds": 1.5, "cooldown_group": ["a:b"]},
         {"cooldown_group": "a:b"}, {}, [], None, 5, "x", {"seconds": 2, "extra": 1}, {"seconds": [2]}, {"seconds": 20.0, "cooldown_group": "foo"}]
    return s


def use_remainder_seeds(rng):
    s = ["minecraft:bucket", "bucket", "minecraft:air", "minecraft:nope", "", 5, None, [], True,
         {"id": "minecraft:bucket"}, {"id": "minecraft:bowl", "count": 2}, {"id": "minecraft:bowl", "count": 99}, {"id": "minecraft:bowl", "count": 100},
         {"id": "minecraft:bowl", "count": 0}, {"id": "minecraft:bowl", "count": -1}, {"id": "minecraft:bowl", "count": 1.5}, {"id": "minecraft:bowl", "count": "2"},
         {"id": "minecraft:air"}, {"id": "minecraft:nope"}, {"count": 1}, {}, {"id": 5}, {"id": "minecraft:bowl", "extra": 1},
         {"id": "minecraft:glass_bottle", "components": {}}, {"id": "minecraft:glass_bottle", "components": {"minecraft:damage": 3}},
         {"id": "minecraft:glass_bottle", "components": {"minecraft:damage": -3}}, {"id": "minecraft:glass_bottle", "components": {"minecraft:nope": 3}},
         {"id": "minecraft:glass_bottle", "components": {"!minecraft:food": {}}}, {"id": "minecraft:glass_bottle", "components": []},
         {"id": "minecraft:glass_bottle", "components": {"minecraft:max_damage": 7}},
         {"id": "minecraft:glass_bottle", "components": {"minecraft:use_remainder": "minecraft:stick"}},
         {"id": "minecraft:glass_bottle", "components": {"minecraft:max_stack_size": 16}},
         {"id": "minecraft:diamond_sword", "components": {"minecraft:max_stack_size": 16}},
         {"id": "minecraft:diamond_sword", "components": {"minecraft:damage": 5, "minecraft:max_damage": 100}},
         {"id": "minecraft:stick", "count": 3, "components": {"minecraft:unbreakable": {}}}]
    return s


# ---------------------------------------------------------------------------------------------------------------

def main():
    rng = random.Random(20)
    out = Out()
    emit_component(out, rng, "attribute_modifiers", attribute_modifiers_seeds(rng), ["diamond_sword", "stick", "iron_helmet", "apple"], muts=2)
    emit_component(out, rng, "potion_contents", potion_contents_seeds(rng), ["potion", "splash_potion", "lingering_potion", "tipped_arrow", "stick"], muts=1)
    emit_component(out, rng, "suspicious_stew_effects", stew_seeds(rng), ["suspicious_stew", "stick"])
    emit_component(out, rng, "consumable", consumable_seeds(rng), ["apple", "potion", "stick", "milk_bucket"], muts=1)
    emit_component(out, rng, "death_protection", death_protection_seeds(rng), ["totem_of_undying", "stick"], muts=2)
    emit_component(out, rng, "food", food_seeds(rng), ["apple", "bread", "stick"])
    emit_component(out, rng, "use_effects", use_effects_seeds(rng), ["apple", "stick", "bow"])
    emit_component(out, rng, "use_cooldown", use_cooldown_seeds(rng), ["ender_pearl", "stick"])
    emit_component(out, rng, "use_remainder", use_remainder_seeds(rng), ["honey_bottle", "stick", "mushroom_stew"])
    # textual oddities: duplicate keys, whitespace, escapes, leading zeros, big exponents
    for name, txt in [("food", '{"nutrition":1,"nutrition":2,"saturation":1}'), ("food", ' { "nutrition" : 1 , "saturation" : 1.0e0 } '),
                      ("food", '{"nutrition":1e0,"saturation":1}'), ("food", '{"nutrition":01,"saturation":1}'), ("food", '{"nutrition":1,"saturation":1,}'),
                      ("food", '{"nutrition":1,"saturation":NaN}'), ("food", '{"nutrition":1,"saturation":Infinity}'), ("food", '{"nutrition":-0,"saturation":-0}'),
                      ("food", '{"nutrition":4,"saturation":1.0E+2}'), ("food", ''), ("food", 'null'), ("food", '{"nutrition":4,"saturation":1} x'),
                      ("potion_contents", '"minecraft:\\u0077ater"'), ("potion_contents", "'minecraft:water'"), ("potion_contents", '{"potion":"minecraft:water","potion":"minecraft:awkward"}'),
                      ("use_effects", '{"speed_multiplier":0.2,"speed_multiplier":0.4}'), ("use_effects", '{"can_sprint":true,"can_sprint":false}'),
                      ("consumable", '{"animation":"eat","animation":"drink"}'), ("use_cooldown", '{"seconds":1,"seconds":0}')]:
        out.add("json", "minecraft:" + name, txt)
    for name, txt in [("food", '{nutrition:4,saturation:2.4f}'), ("food", '{nutrition:4b,saturation:2.4}'), ("food", '{nutrition:4L,saturation:2.4}'),
                      ("food", '{nutrition:4,saturation:2.4,can_always_eat:true}'), ("food", '{nutrition:4,saturation:NaNf}'), ("food", '{nutrition:4,saturation:Infinityd}'),
                      ("food", '{nutrition:4,saturation:-Infinityf}'), ("food", '{nutrition:4,saturation:2.4,can_always_eat:2b}'), ("food", '{nutrition:4,saturation:2.4,can_always_eat:0.5f}'),
                      ("food", '{nutrition:4s,saturation:2s}'), ("food", '{nutrition:0x10,saturation:1}'), ("food", '{nutrition:1_0,saturation:1}'), ("food", "{nutrition:4,saturation:'2.4'}"),
                      ("use_effects", '{speed_multiplier:0.5f,can_sprint:1b}'), ("use_effects", '{speed_multiplier:1,can_sprint:true}'),
                      ("potion_contents", '"minecraft:water"'), ("potion_contents", '{potion:"minecraft:water",custom_color:255}'), ("potion_contents", '{custom_color:1.9f}'),
                      ("potion_contents", '{custom_color:255L}'), ("potion_contents", '{custom_color:-1b}'),
                      ("potion_contents", '{custom_effects:[{id:"minecraft:speed",amplifier:300b}]}'), ("potion_contents", '{custom_effects:[{id:"minecraft:speed",amplifier:256}]}'),
                      ("potion_contents", '{custom_effects:[{id:"minecraft:speed",amplifier:-1b}]}'), ("potion_contents", '{custom_effects:[{id:"minecraft:speed",amplifier:200b}]}'),
                      ("potion_contents", '{custom_effects:[{id:"minecraft:speed",amplifier:255s}]}'), ("potion_contents", '{custom_effects:[{id:"minecraft:speed",amplifier:128b}]}'),
                      ("potion_contents", '{custom_effects:[{id:"minecraft:speed",ambient:1b,show_particles:0b}]}'), ("potion_contents", '{custom_effects:[{id:"minecraft:speed",ambient:2b}]}'),
                      ("potion_contents", '{custom_effects:[{id:"minecraft:speed",duration:1.5f}]}'), ("potion_contents", '{custom_effects:[{id:"minecraft:speed",duration:5L}]}'),
                      ("suspicious_stew_effects", '[{id:"minecraft:poison",duration:5b}]'), ("suspicious_stew_effects", '[{id:"minecraft:poison",duration:"5"}]'),
                      ("suspicious_stew_effects", '[{id:"minecraft:poison",duration:5.9d}]'), ("suspicious_stew_effects", '[{id:"minecraft:poison",duration:1e3}]'),
                      ("consumable", '{consume_seconds:2,animation:"drink"}'), ("consumable", '{consume_seconds:2b}'), ("consumable", '{consume_seconds:-0.0f}'),
                      ("use_cooldown", '{seconds:5}'), ("use_cooldown", '{seconds:5L,cooldown_group:"a:b"}'),
                      ("attribute_modifiers", '[{type:"minecraft:luck",id:"a:b",amount:1b,operation:"add_value"}]'),
                      ("attribute_modifiers", '[{type:"minecraft:luck",id:"a:b",amount:1L,operation:"add_value",slot:"any"}]'),
                      ("attribute_modifiers", '[{type:"minecraft:luck",id:"a:b",amount:NaNd,operation:"add_value"}]'),
                      ("attribute_modifiers", '[{type:"minecraft:luck",id:"a:b",amount:Infinityd,operation:"add_value"}]'),
                      ("attribute_modifiers", '[{type:"minecraft:luck",id:"a:b",amount:-0.0,operation:"add_value"}]'),
                      ("attribute_modifiers", '[{type:"minecraft:luck",id:"a:b",amount:1.0f,operation:"add_value",display:{type:"override",value:{text:"x"}}}]'),
                      ("attribute_modifiers", '[{type:"minecraft:luck",id:"a:b",amount:1.0f,operation:"add_value",display:{type:"override",value:[1b,2b]}}]'),
                      ("attribute_modifiers", '[{type:"minecraft:luck",id:"a:b",amount:1.0f,operation:"add_value",display:{type:"override",value:{text:1b}}}]'),
                      ("use_remainder", '"minecraft:bucket"'), ("use_remainder", '{id:"minecraft:bowl",count:2b}'), ("use_remainder", '{id:"minecraft:bowl",count:2L}'),
                      ("use_remainder", '{id:"minecraft:bowl",components:{"minecraft:damage":1b}}'),
                      ("death_protection", '{death_effects:[{type:"minecraft:clear_all_effects"}]}'), ("death_protection", '{death_effects:[{type:"minecraft:teleport_randomly",diameter:3}]}'),
                      ("death_protection", '{death_effects:[{type:"minecraft:apply_effects",effects:[{id:"minecraft:speed"}],probability:0.5d}]}')]:
        out.add("snbt", "minecraft:" + name, txt)
    # out-of-bounds enum ids (ByIdMap ZERO strategy) on the wire
    for name, hx in [("attribute_modifiers", "010003613a623ff00000000000000563" + "4d"), ("consumable", "3fcccccd630101" + "00"),
                     ("consumable", "3fcccccd0001010101"), ("attribute_modifiers", "010003613a623ff0000000000000ff0000"),
                     ("food", "0400000000"), ("use_effects", "0001" + "3e4ccccd"), ("use_cooldown", "3f80000000"), ("use_cooldown", "3f800000" + "0103613a62")]:
        out.add("wire", "minecraft:" + name, hx)
    sys.stdout.write("\n".join(out.lines) + "\n")


if __name__ == "__main__":
    main()
