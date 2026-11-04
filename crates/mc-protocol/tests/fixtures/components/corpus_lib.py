"""Shared helpers of the gen_corpus_*.py scripts: JSON -> SNBT with random numeric types, seeded structural mutators.

A corpus line is `kind<TAB>component<TAB>payload`; payloads never contain tabs or newlines.
"""
import copy
import json
import random

INTERESTING_NUMBERS = [0, 1, -1, 2, 3, 4, 15, 16, 99, 100, 127, 128, 255, 256, 257, 1023, 1024, 1025, 32767, 32768, 65535,
                       2**31 - 1, 2**31, -2**31, -2**31 - 1, 2**53, 2**63 - 1, 2**63, -2**63, 0.5, -0.5, 1.5, 1e10, 1e300, 3.4e38, 1e-10, -0.0]
JUNK = [None, True, False, "", "x", "minecraft:stone", "#minecraft:logs", "minecraft:does_not_exist", [], {}, [None], [[]], [{}], {"": ""},
        {"id": "minecraft:stone"}, ["minecraft:stone"], "§", "\u00e9\u4e2d\U0001F600"]


def snbt_str(s):
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def snbt_key(k):
    return snbt_str(k)


def to_snbt(v, rng, style=None):
    """JSON value -> SNBT text. style: None = random numeric suffixes, 'plain' = ints as int / floats as double."""
    if v is None:
        return "null" if rng.random() < 0.5 else '""'
    if isinstance(v, bool):
        if style == "plain":
            return "true" if v else "false"
        return rng.choice(["true" if v else "false", "1b" if v else "0b"])
    if isinstance(v, int):
        if style == "plain":
            return str(v)
        opts = [str(v)]
        if -128 <= v <= 127:
            opts += [f"{v}b", f"{v}B"]
        if -32768 <= v <= 32767:
            opts += [f"{v}s"]
        if -2**31 <= v < 2**31:
            opts += [f"{v}f", f"{v}d", f"{v}.0"]
        if -2**63 <= v < 2**63:
            opts += [f"{v}L", f"{v}l"]
        return rng.choice(opts)
    if isinstance(v, float):
        if style == "plain":
            return repr(v)
        return rng.choice([repr(v), repr(v) + "d", repr(v) + "f"])
    if isinstance(v, str):
        return snbt_str(v)
    if isinstance(v, list):
        return "[" + ",".join(to_snbt(x, rng, style) for x in v) + "]"
    if isinstance(v, dict):
        return "{" + ",".join(snbt_key(k) + ":" + to_snbt(x, rng, style) for k, x in v.items()) + "}"
    raise TypeError(v)


def paths(v, prefix=()):
    yield prefix
    if isinstance(v, dict):
        for k, x in v.items():
            yield from paths(x, prefix + (k,))
    elif isinstance(v, list):
        for i, x in enumerate(v):
            yield from paths(x, prefix + (i,))


def get(v, path):
    for p in path:
        v = v[p]
    return v


def setp(v, path, new):
    if not path:
        return new
    parent = get(v, path[:-1])
    parent[path[-1]] = new
    return v


def mutate(value, rng, extra_junk=()):
    """One random structural mutation of a deep copy of `value`."""
    v = copy.deepcopy(value)
    ps = list(paths(v))
    path = rng.choice(ps)
    cur = get(v, path)
    op = rng.randrange(11)
    junk = list(JUNK) + list(extra_junk)
    if op == 0:
        return setp(v, path, rng.choice(junk))
    if op == 1:
        return setp(v, path, rng.choice(INTERESTING_NUMBERS))
    if op == 2 and isinstance(cur, dict) and cur:
        del cur[rng.choice(list(cur))]
        return v
    if op == 3 and isinstance(cur, list) and cur:
        del cur[rng.randrange(len(cur))]
        return v
    if op == 4 and isinstance(cur, list) and cur:
        cur.insert(rng.randrange(len(cur) + 1), copy.deepcopy(rng.choice(cur)))
        return v
    if op == 5 and isinstance(cur, dict):
        cur[rng.choice(["extra", "id", "count", "type", "raw", "filtered", "x", "slot"])] = rng.choice(junk)
        return v
    if op == 6:
        return setp(v, path, [cur])
    if op == 7 and isinstance(cur, list) and len(cur) == 1:
        return setp(v, path, cur[0])
    if op == 8 and isinstance(cur, dict) and len(cur) >= 2:
        a, b = rng.sample(list(cur), 2)
        cur[a], cur[b] = cur[b], cur[a]
        return v
    if op == 9 and isinstance(cur, (int, float)) and not isinstance(cur, bool):
        return setp(v, path, cur + rng.choice([-1, 1, 2**31, -2**31, 0.5]))
    if op == 10 and isinstance(cur, str):
        return setp(v, path, cur + rng.choice(["_", " ", ":", "x" * 40, "\u00e9"]))
    return setp(v, path, rng.choice(junk))


def emit(lines, kind_prefix, component, value, rng):
    """Append the json and snbt lines for a python value (both forms, so the two ops see the same data)."""
    lines.append(f"json\t{component}\t{json.dumps(value, ensure_ascii=False, separators=(',', ':'))}")
    lines.append(f"snbt\t{component}\t{to_snbt(value, rng)}")


def build(out_path, seed, components, handwritten_raw, mutations_per_base=4, stack_lines=()):
    """components: {name: (list of valid base values, extra junk)}; handwritten_raw: list of (kind, name, payload)."""
    rng = random.Random(seed)
    lines = []
    for kind, name, payload in handwritten_raw:
        lines.append(f"{kind}\t{name}\t{payload}")
    for name, (bases, extra) in components.items():
        for base in bases:
            emit(lines, "", name, base, rng)
            for _ in range(mutations_per_base):
                emit(lines, "", name, mutate(base, rng, extra), rng)
            # stacked mutations
            m = base
            for _ in range(rng.randrange(2, 4)):
                m = mutate(m, rng, extra)
            emit(lines, "", name, m, rng)
    for sl in stack_lines:
        lines.append(sl)
    # dedupe keeping order
    seen, uniq = set(), []
    for l in lines:
        if l not in seen:
            seen.add(l)
            uniq.append(l)
    with open(out_path, "w", encoding="utf-8") as f:
        f.write("\n".join(uniq) + "\n")
    print(f"{out_path}: {len(uniq)} lines")


# ---- wire (hex) building blocks ----------------------------------------------------------------------------------

def vi(n):
    """VarInt as hex."""
    n &= 0xFFFFFFFF
    out = b""
    while True:
        if n & ~0x7F == 0:
            out += bytes([n])
            break
        out += bytes([(n & 0x7F) | 0x80])
        n >>= 7
    return out.hex()


def utf(s):
    b = s.encode("utf-8")
    return vi(len(b)) + b.hex()


def ident(s):
    return utf(s)


def nbt_str(s):
    """Network NBT string tag (a text component of plain text)."""
    b = s.encode("utf-8")
    return "08" + len(b).to_bytes(2, "big").hex() + b.hex()


def be32(n):
    return (n & 0xFFFFFFFF).to_bytes(4, "big").hex()


def wire_lines(component, hexes):
    return [("wire", component, h) for h in hexes]
