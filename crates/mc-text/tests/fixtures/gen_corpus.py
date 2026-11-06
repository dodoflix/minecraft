#!/usr/bin/env python3
"""Deterministic random component corpus generator (J/N lines).

Usage: python3 crates/mc-text/tests/fixtures/gen_corpus.py crates/mc-text/tests/fixtures/corpus_random.txt [count] [seed]
Output: crates/mc-text/tests/fixtures/corpus_random.txt (written to OUT.tmp, renamed into place on success)
"""
import json, os, random, sys

if sys.argv[1:] in (["-h"], ["--help"]):
    print(__doc__)
    sys.exit(0)
if len(sys.argv) < 2:
    sys.exit(__doc__)
OUT = sys.argv[1]
rnd = random.Random(int(sys.argv[3]) if len(sys.argv) > 3 else 26_3)
KEYS = ["gui.done", "chat.type.text", "commands.give.success.single", "death.attack.player", "nope.key",
        "multiplayer.player.joined", "commands.teleport.success.entity.single", "options.on", "block.minecraft.stone", ""]
FALLBACKS = ["%s", "x %s y", "%s %s", "%1$s %2$s", "%2$s%1$s", "100%", "%%", "%d", "%1$s%s", "%5$s", "a%sb%%c", "%", "%1$", "plain"]
COLORS = ["red", "gold", "#FF5555", "#123456", "#1", "white", "bad", "", "#GGGGGG", "#-1", "dark_aqua"]
IDS = ["minecraft:foo", "bar", "a:b", "A:b", "x y", "mod:path/sub", ""]
TEXTS = ["", "a", "hello world", "§cred", "tab\there", "ünïcode", "😀", "%s", "line\nbreak"]

def text_leaf():
    return rnd.choice(TEXTS)

def style(d, depth):
    if rnd.random() < 0.5: d["color"] = rnd.choice(COLORS)
    if rnd.random() < 0.1: d["shadow_color"] = rnd.choice([0, -1, 16711680, [1.0, 0.5, 0.25, 0.75], [1, 2, 3, 4], 1.5])
    for k in ("bold", "italic", "underlined", "strikethrough", "obfuscated"):
        if rnd.random() < 0.15: d[k] = rnd.choice([True, False])
    if rnd.random() < 0.1: d["insertion"] = text_leaf()
    if rnd.random() < 0.08: d["font"] = rnd.choice(IDS)
    if rnd.random() < 0.12:
        a = rnd.choice(["open_url", "run_command", "suggest_command", "change_page", "copy_to_clipboard", "custom", "open_file", "bogus"])
        e = {"action": a}
        if a == "open_url": e["url"] = rnd.choice(["https://example.com", "http://a.b/c?d=e#f", "ftp://x", "example.com", "https://a b", "HTTP://X"])
        elif a in ("run_command", "suggest_command"): e["command"] = rnd.choice(["/say hi", "say §", "x\ny", ""])
        elif a == "change_page": e["page"] = rnd.choice([1, 5, 0, -1, "2"])
        elif a == "copy_to_clipboard": e["value"] = text_leaf()
        elif a == "custom":
            e["id"] = rnd.choice(IDS)
            if rnd.random() < 0.5: e["payload"] = rnd.choice([1, "s", [1, 2], {"a": 1, "b": [1.5, 300, 70000, 5000000000]}, True, {}])
        else: e["path"] = "x"
        d["click_event"] = e
    if rnd.random() < 0.12:
        a = rnd.choice(["show_text", "show_item", "show_entity", "bogus"])
        e = {"action": a}
        if a == "show_text": e["value"] = component(depth + 1)
        elif a == "show_item":
            e["id"] = rnd.choice(["minecraft:stone", "stone", "minecraft:diamond_sword", "minecraft:bad_item", "x y"])
            if rnd.random() < 0.5: e["count"] = rnd.choice([1, 2, 64, 99, 0, 100])
            if rnd.random() < 0.3: e["components"] = rnd.choice([{}, {"minecraft:custom_name": "x"}, {"!minecraft:custom_name": {}}])
        elif a == "show_entity":
            e["id"] = rnd.choice(["minecraft:pig", "zombie", "minecraft:bad"])
            e["uuid"] = rnd.choice([[1, 2, 3, 4], [1, 2, 3], "123e4567-e89b-12d3-a456-426614174000", "1-1-1-1-1", "zz", 5])
            if rnd.random() < 0.5: e["name"] = component(depth + 1)
        d["hover_event"] = e

def contents(d, depth):
    k = rnd.choice(["text", "text", "text", "translatable", "keybind", "score", "selector", "nbt", "object"])
    if rnd.random() < 0.1: d["type"] = rnd.choice(["text", "translatable", "keybind", "score", "selector", "nbt", "object", "bogus", "minecraft:text"])
    if k == "text": d["text"] = text_leaf()
    elif k == "translatable":
        d["translate"] = rnd.choice(KEYS)
        if rnd.random() < 0.5: d["fallback"] = rnd.choice(FALLBACKS)
        if rnd.random() < 0.7:
            d["with"] = [rnd.choice([component(depth + 1), text_leaf(), rnd.randint(-5, 300), 1.5, 1.0, 1e10, True, 123456789012, [text_leaf(), text_leaf()]]) for _ in range(rnd.randint(0, 4))]
    elif k == "keybind": d["keybind"] = rnd.choice(["key.jump", "key.inventory", ""])
    elif k == "score": d["score"] = {"name": rnd.choice(["@s", "Steve", "*", "@a[tag=x]"]), "objective": rnd.choice(["obj", "kills"])}
    elif k == "selector":
        d["selector"] = rnd.choice(["@a", "@s", "Steve"])
        if rnd.random() < 0.4: d["separator"] = component(depth + 1)
    elif k == "nbt":
        d["nbt"] = rnd.choice(["a.b", "Items[0]", "x"])
        src = rnd.choice(["entity", "block", "storage"])
        d[src] = {"entity": "@s", "block": "~ ~ ~", "storage": rnd.choice(["a:b", "minecraft:foo", "A:b"])}[src]
        if rnd.random() < 0.3: d["interpret"] = rnd.choice([True, False, 1])
        if rnd.random() < 0.3: d["plain"] = rnd.choice([True, False])
        if rnd.random() < 0.3: d["separator"] = component(depth + 1)
        if rnd.random() < 0.1: d["source"] = rnd.choice(["entity", "block", "storage", "bad"])
    else:
        if rnd.random() < 0.5:
            d["sprite"] = rnd.choice(["stone", "minecraft:item/diamond", "x:y"])
            if rnd.random() < 0.4: d["atlas"] = rnd.choice(["minecraft:blocks", "minecraft:items", "a:b", "A"])
            if rnd.random() < 0.1: d["object"] = "atlas"
        else:
            d["player"] = rnd.choice(["Steve", "Alex", "a b", "x" * 20, {"name": "Notch"}, {"name": "N", "properties": [{"name": "textures", "value": "v"}]}])
            if rnd.random() < 0.4: d["hat"] = rnd.choice([True, False])
            if rnd.random() < 0.1: d["object"] = "player"
        if rnd.random() < 0.3: d["fallback"] = component(depth + 1)

def component(depth=0):
    r = rnd.random()
    if depth > 3 or r < 0.2: return text_leaf()
    if r < 0.3: return [component(depth + 1) for _ in range(rnd.randint(0, 3))]
    d = {}
    contents(d, depth)
    if rnd.random() < 0.35: d["extra"] = [component(depth + 1) for _ in range(rnd.randint(0, 3))]
    style(d, depth)
    return d

def mutate(v):
    """Randomly corrupt a JSON tree."""
    if isinstance(v, dict):
        v = dict(v)
        for k in list(v):
            x = rnd.random()
            if x < 0.04: del v[k]
            elif x < 0.08: v[k] = rnd.choice([5, "s", True, None, [], {}, 1.5, [1]])
            else: v[k] = mutate(v[k])
        if rnd.random() < 0.03: v["unknown"] = 1
        return v
    if isinstance(v, list): return [mutate(x) for x in v]
    return v

def snbt_str(s): return json.dumps(s, ensure_ascii=False)

def to_snbt(v, ctx=""):
    if v is None: return '""'
    if isinstance(v, bool): return rnd.choice(["1b", "true"]) if v else rnd.choice(["0b", "false"])
    if isinstance(v, int):
        if -128 <= v <= 127 and rnd.random() < 0.3: return f"{v}b"
        if -32768 <= v <= 32767 and rnd.random() < 0.2: return f"{v}s"
        if -2**31 <= v < 2**31:
            return str(v) if rnd.random() < 0.7 else f"{v}L"
        return f"{v}L"
    if isinstance(v, float): return rnd.choice([f"{v}f", f"{v}d", f"{v}"])
    if isinstance(v, str): return snbt_str(v)
    if isinstance(v, list):
        if v and all(isinstance(x, int) and not isinstance(x, bool) and -2**31 <= x < 2**31 for x in v) and rnd.random() < 0.3:
            return "[I;" + ",".join(str(x) for x in v) + "]"
        return "[" + ",".join(to_snbt(x) for x in v) + "]"
    if isinstance(v, dict):
        return "{" + ",".join(f"{snbt_str(k)}:{to_snbt(x)}" for k, x in v.items()) + "}"
    raise TypeError(v)

N = int(sys.argv[2]) if len(sys.argv) > 2 else 700
lines = []
for _ in range(N):
    c = component()
    if rnd.random() < 0.4: c = mutate(c)
    lines.append("J" + json.dumps(c, ensure_ascii=False))
    if isinstance(c, (dict, list, str)):
        lines.append("N" + to_snbt(c))
with open(OUT + ".tmp", "w", encoding="utf-8", newline="\n") as f:
    f.write("".join(line + "\n" for line in lines))
os.replace(OUT + ".tmp", OUT)
