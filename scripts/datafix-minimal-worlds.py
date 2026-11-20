#!/usr/bin/env python3
"""Cuts the minimal worlds of crates/mc-world/tests/fixtures/filefix/worlds out of the scripted worlds of old servers.

Usage: scripts/datafix-minimal-worlds.py [--cache DIR] [--out DIR]
Input:  DIR/worlds/<id>-normal/ of scripts/datafix-worlds.sh (default $MC_DATAFIX_CACHE or ~/.cache/mc-datafix-corpus):
        1.12.2, 1.16.5, 1.20.4, 26.1.
Output: DIR/<name>/ (default crates/mc-world/tests/fixtures/filefix/worlds): the same folders and files, every region
        file cut down to a few chunks (the chunks around the structures of the legacy index files for the 1.12.2 worlds),
        plus the worlds with shapes the scripted servers never write (a single player in level.dat, legacy structure
        index files of every kind, a hardcore world that was opened in single player). The inputs are made here; what the
        file fixer does with them is recorded from vanilla (scripts/datafix-oracle.sh filefix).
"""
import gzip
import os
import shutil
import struct
import sys

SECTOR = 4096


def usage():
    print(__doc__)
    sys.exit(0)


# ---- a typed NBT model: (tag id, value); a compound is a dict name -> typed value, a list is (element id, [values]) ----

def read_payload(f, t):
    if t == 1:
        return struct.unpack(">b", f.read(1))[0]
    if t == 2:
        return struct.unpack(">h", f.read(2))[0]
    if t == 3:
        return struct.unpack(">i", f.read(4))[0]
    if t == 4:
        return struct.unpack(">q", f.read(8))[0]
    if t == 5:
        return struct.unpack(">f", f.read(4))[0]
    if t == 6:
        return struct.unpack(">d", f.read(8))[0]
    if t == 7:
        n = struct.unpack(">i", f.read(4))[0]
        return list(struct.unpack(">%db" % n, f.read(n)))
    if t == 8:
        n = struct.unpack(">H", f.read(2))[0]
        return f.read(n).decode("utf-8")
    if t == 9:
        et = f.read(1)[0]
        n = struct.unpack(">i", f.read(4))[0]
        return (et, [read_payload(f, et) for _ in range(n)])
    if t == 10:
        d = {}
        while True:
            tt = f.read(1)[0]
            if tt == 0:
                return d
            n = struct.unpack(">H", f.read(2))[0]
            name = f.read(n).decode("utf-8")
            d[name] = (tt, read_payload(f, tt))
    if t == 11:
        n = struct.unpack(">i", f.read(4))[0]
        return list(struct.unpack(">%di" % n, f.read(4 * n)))
    if t == 12:
        n = struct.unpack(">i", f.read(4))[0]
        return list(struct.unpack(">%dq" % n, f.read(8 * n)))
    raise ValueError("tag %d" % t)


def write_payload(out, t, v):
    if t == 1:
        out += struct.pack(">b", v)
    elif t == 2:
        out += struct.pack(">h", v)
    elif t == 3:
        out += struct.pack(">i", v)
    elif t == 4:
        out += struct.pack(">q", v)
    elif t == 5:
        out += struct.pack(">f", v)
    elif t == 6:
        out += struct.pack(">d", v)
    elif t == 7:
        out += struct.pack(">i", len(v)) + struct.pack(">%db" % len(v), *v)
    elif t == 8:
        b = v.encode("utf-8")
        out += struct.pack(">H", len(b)) + b
    elif t == 9:
        et, items = v
        out += struct.pack(">bi", et, len(items))
        for item in items:
            write_payload(out, et, item)
    elif t == 10:
        for name, (tt, vv) in v.items():
            b = name.encode("utf-8")
            out += struct.pack(">bH", tt, len(b)) + b
            write_payload(out, tt, vv)
        out += b"\0"
    elif t == 11:
        out += struct.pack(">i", len(v)) + struct.pack(">%di" % len(v), *v)
    elif t == 12:
        out += struct.pack(">i", len(v)) + struct.pack(">%dq" % len(v), *v)


def load(path):
    f = gzip.open(path)
    t = f.read(1)[0]
    f.read(struct.unpack(">H", f.read(2))[0])
    return read_payload(f, t)


def save(path, root):
    out = bytearray(b"\x0a\x00\x00")
    write_payload(out, 10, root)
    with open(path, "wb") as fh:
        # mtime 0 and no name: the same bytes on every run
        fh.write(gzip.compress(bytes(out), mtime=0))


# ---- region files ----

def trim_region(src, dst, keep):
    """Copies the chunks of the region file `src` that `keep(chunk_x, chunk_z)` accepts into a new file `dst`."""
    parts = os.path.basename(src).split(".")
    region_x, region_z = int(parts[1]), int(parts[2])
    with open(src, "rb") as f:
        data = f.read()
    if not data:
        open(dst, "wb").close()
        return
    offsets = struct.unpack(">1024i", data[:SECTOR])
    stamps = data[SECTOR : 2 * SECTOR]
    kept = [(i, o) for i, o in enumerate(offsets) if o and keep(region_x * 32 + i % 32, region_z * 32 + i // 32)]
    header = [0] * 1024
    new_stamps = bytearray(SECTOR)
    body = bytearray()
    sector = 2
    for i, o in kept:
        start, count = (o >> 8) & 0xFFFFFF, o & 0xFF
        blob = data[start * SECTOR : (start + count) * SECTOR]
        assert blob[4] < 128, "external chunk in " + src
        header[i] = (sector << 8) | count
        new_stamps[i * 4 : i * 4 + 4] = stamps[i * 4 : i * 4 + 4]
        body += blob
        sector += count
    with open(dst, "wb") as f:
        f.write(struct.pack(">1024i", *header) + bytes(new_stamps) + bytes(body))


def first_chunks(n):
    seen = {}

    def keep(cx, cz):
        # the first n chunks asked about per region (the callers go through a file in index order)
        key = (cx >> 5, cz >> 5)
        seen[key] = seen.get(key, 0) + 1
        return seen[key] <= n

    return keep


def structure_starts(world):
    starts = []
    for name in ("Monument", "Stronghold", "Mineshaft", "Temple", "Mansion", "Fortress", "EndCity"):
        path = os.path.join(world, "data", name + ".dat")
        if os.path.exists(path):
            for _, feature in load(path)["data"][1]["Features"][1].items():
                starts.append((feature[1]["ChunkX"][1], feature[1]["ChunkZ"][1]))
    return starts


def cut(src, dst, keep_for):
    """Copies the world `src` to `dst` with every region file cut down by `keep_for(world_folder)`."""
    if not os.path.isdir(src):
        sys.exit("no world at " + src + " (scripts/datafix-worlds.sh makes it)")
    shutil.rmtree(dst, ignore_errors=True)
    for folder, _, files in os.walk(src):
        target = os.path.join(dst, os.path.relpath(folder, src))
        os.makedirs(target, exist_ok=True)
        for name in sorted(files):
            if name.endswith(".mca"):
                trim_region(os.path.join(folder, name), os.path.join(target, name), keep_for(folder))
            else:
                shutil.copy(os.path.join(folder, name), os.path.join(target, name))


# ---- the shapes the scripted servers never write ----

def edit_level(world, edit):
    path = os.path.join(world, "level.dat")
    root = load(path)
    edit(root["Data"][1])
    save(path, root)


def single_player(data, with_uuid):
    player = {
        "Health": (5, 20.0),
        "Pos": (9, (6, [8.5, 70.0, 8.5])),
        "Dimension": (3, 0),
        "Inventory": (9, (10, [])),
        "foodLevel": (3, 20),
    }
    if with_uuid:
        player["UUIDMost"] = (4, 0x0123456789ABCDEF)
        player["UUIDLeast"] = (4, -0x0123456789ABCDEF)
    data["Player"] = (10, player)


def legacy_structures(world):
    """Index files of the kinds the scripted servers never write, with their starts in the chunks the world keeps (around the
    mineshafts at (-22,-4) and (-24,4)): temples whose first child has a legacy id, a monument in the chunk of a mineshaft
    (two starts in one chunk), a fortress and an end city with a chunk of their own dimension."""

    def feature(id_, x, z, child):
        children = [] if child is None else [{"id": (8, child)}]
        return {
            "id": (8, id_),
            "ChunkX": (3, x),
            "ChunkZ": (3, z),
            "Children": (9, (10, [dict(c) for c in children])),
        }

    def index(features):
        return {"data": (10, {"Features": (10, {"[%d,%d]" % (f["ChunkX"][1], f["ChunkZ"][1]): (10, f) for f in features})})}

    files = {
        "Temple": [feature("Temple", -22, -3, "TeDP"), feature("Temple", -23, -5, "Iglu"), feature("Temple", -25, 5, "TeSH"),
                   feature("Temple", -24, 3, "TeXX")],
        "Monument": [feature("Monument", -22, -4, None)],
        "Fortress": [feature("Fortress", 2, 2, None)],
        "EndCity": [feature("EndCity", 0, 0, None)],
    }
    for name, features in files.items():
        save(os.path.join(world, "data", name + ".dat"), index(features))
    # chunks of the nether and the end around the starts: copies of overworld chunks (the file fix reads them by position)
    overworld = os.path.join(world, "region/r.-1.-1.mca")
    for folder, (cx, cz) in (("DIM-1", (2, 2)), ("DIM1", (0, 0))):
        os.makedirs(os.path.join(world, folder, "region"), exist_ok=True)
        copy_chunks(overworld, os.path.join(world, folder, "region/r.0.0.mca"),
                    [(cx + dx, cz + dz) for dx in (-1, 0, 1) for dz in (0, 1) if cx + dx >= 0 and cz + dz >= 0])


def copy_chunks(src, dst, positions):
    """Writes the region file `dst` with the chunks at `positions` (chunk coordinates, region 0,0), each a copy of the next
    chunk the region file `src` holds."""
    with open(src, "rb") as f:
        data = f.read()
    offsets = [o for o in struct.unpack(">1024i", data[:SECTOR]) if o]
    header = [0] * 1024
    body = bytearray()
    sector = 2
    for n, (cx, cz) in enumerate(positions):
        o = offsets[n % len(offsets)]
        start, count = (o >> 8) & 0xFFFFFF, o & 0xFF
        body += data[start * SECTOR : (start + count) * SECTOR]
        header[cz * 32 + cx] = (sector << 8) | count
        sector += count
    with open(dst, "wb") as f:
        f.write(struct.pack(">1024i", *header) + bytes(SECTOR) + bytes(body))


def border_away_from_origin(data):
    data["BorderCenterX"] = (6, 800.0)
    data["BorderCenterZ"] = (6, -1600.0)


def generated(world, namespaces):
    """Structures saved by structure blocks: `generated/<namespace>/structures/<name>.nbt`."""
    for namespace in namespaces:
        folder = os.path.join(world, "generated", namespace, "structures")
        os.makedirs(folder, exist_ok=True)
        save(os.path.join(folder, "house.nbt"), {"size": (9, (3, [1, 2, 3])), "DataVersion": (3, 1)})


def misc_files(world):
    """Files of the kinds the scripted servers leave out: a resource pack, player files, saved data of every moved kind and
    the leftovers the file fix deletes. What the file fixer does with them depends on their names only."""
    uuid = "00000000-0000-0000-0000-000000000001"

    def put(name, content=None):
        path = os.path.join(world, name)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        if content is None:
            save(path, {"data": (10, {"n": (3, len(name))})})
        else:
            with open(path, "wb") as f:
                f.write(content)

    put("resources.zip.fixture", b"PK\x05\x06" + bytes(18))
    for name in ("playerdata/%s.dat" % uuid,):
        put(name)
    put("stats/%s.json" % uuid, b'{"DataVersion":2586,"stats":{}}')
    put("advancements/%s.json" % uuid, b'{"DataVersion":2586}')
    for name in (
        "data/scoreboard.dat",
        "data/idcounts.dat",
        "data/map_0.dat",
        "data/map_12.dat",
        "data/command_storage_mymod.dat",
        "data/command_storage_a.b-c_d.dat",
        "data/random_sequences.dat",
        "data/villages.dat",
        "data/villages_nether.dat",
        "data/villages_end.dat",
        "data/Village.dat",
        "data/Village_index.dat",
        "data/Temple_index.dat",
        "data/EndCity.dat",
        "DIM-1/data/world_border.dat",
        "DIM1/data/world_border.dat",
        "data/world_border.dat",
    ):
        put(name)
    put("data/advancements/a.json", b"{}")
    put("data/functions/f.mcfunction", b"")


def interrupted_states(worlds, out):
    """Saves folders an earlier start left behind, which the next start finds next to the world: the staging folder of a
    run that stopped after the fixes, and the folder of a new world that was moved to the top level but not moved in."""
    staging = os.path.join(out, "staging-left-behind")
    upgraded = os.path.join(out, "upgraded-left-behind")
    for state in (staging, upgraded):
        shutil.rmtree(state, ignore_errors=True)
    shutil.copytree(os.path.join(worlds, "1.20.4"), os.path.join(staging, "world"))
    for name in ("filefix/new_world/level.dat", "filefix/cow/x"):
        os.makedirs(os.path.dirname(os.path.join(staging, "world", name)), exist_ok=True)
        shutil.copy(os.path.join(worlds, "1.20.4/level.dat"), os.path.join(staging, "world", name))
    shutil.copytree(os.path.join(worlds, "1.20.4"), os.path.join(upgraded, "world"))
    os.makedirs(os.path.join(upgraded, "world upgraded"))
    shutil.copy(os.path.join(worlds, "1.20.4/level.dat"), os.path.join(upgraded, "world upgraded/level.dat"))


def hardcore_single_player(data):
    data["singleplayer_uuid"] = (11, [1, 2, 3, 4])
    settings = data["difficulty_settings"][1]
    settings["hardcore"] = (1, 1)


def main():
    args = sys.argv[1:]
    if "-h" in args or "--help" in args:
        usage()
    xdg_cache = os.environ.get("XDG_CACHE_HOME") or os.path.expanduser("~/.cache")
    cache = os.environ.get("MC_DATAFIX_CACHE") or os.path.join(xdg_cache, "mc-datafix-corpus")
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    out = os.path.join(root, "crates/mc-world/tests/fixtures/filefix/worlds")
    while args:
        flag = args.pop(0)
        if flag == "--cache":
            cache = args.pop(0)
        elif flag == "--out":
            out = args.pop(0)
        else:
            usage()
    worlds = os.path.join(cache, "worlds")

    def window(world):
        starts = structure_starts(world)
        return lambda cx, cz: any(abs(cx - sx) <= 2 and abs(cz - sz) <= 2 for sx, sz in starts)

    # 1.12.2: the chunks around the starts of the legacy structures (the file fix writes their `Level.Structures`)
    src = os.path.join(worlds, "1.12.2-normal")
    cut(src, os.path.join(out, "1.12.2"), lambda _: window(src))
    for name, with_uuid in (("1.12.2-player", True), ("1.12.2-player-no-uuid", False)):
        shutil.rmtree(os.path.join(out, name), ignore_errors=True)
        shutil.copytree(os.path.join(out, "1.12.2"), os.path.join(out, name))
        edit_level(os.path.join(out, name), lambda d, w=with_uuid: single_player(d, w))
    shutil.rmtree(os.path.join(out, "1.12.2-structures"), ignore_errors=True)
    shutil.copytree(os.path.join(out, "1.12.2"), os.path.join(out, "1.12.2-structures"))
    legacy_structures(os.path.join(out, "1.12.2-structures"))
    edit_level(os.path.join(out, "1.12.2-structures"), border_away_from_origin)
    # 1.16.5, 1.20.4: the moves only; two chunks per region file are enough
    for version in ("1.16.5", "1.20.4"):
        cut(os.path.join(worlds, version + "-normal"), os.path.join(out, version), lambda _: first_chunks(2))
    generated(os.path.join(out, "1.20.4"), ("minecraft", "mymod"))
    shutil.rmtree(os.path.join(out, "1.16.5-files"), ignore_errors=True)
    shutil.copytree(os.path.join(out, "1.16.5"), os.path.join(out, "1.16.5-files"))
    misc_files(os.path.join(out, "1.16.5-files"))
    # a resource pack where the file fix moves it to: the fix throws (`*.zip.fixture` becomes `*.zip` when a world is copied:
    # the repository tracks no zip files)
    shutil.rmtree(os.path.join(out, "1.16.5-conflict"), ignore_errors=True)
    shutil.copytree(os.path.join(out, "1.16.5"), os.path.join(out, "1.16.5-conflict"))
    for folder in ("", "resourcepacks"):
        path = os.path.join(out, "1.16.5-conflict", folder, "resources.zip.fixture")
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "wb") as f:
            f.write(b"PK\x05\x06" + bytes(18))
    # 26.1 (data version 4786): the layout of 4772, the fixes of 4773 and 4899 apply
    cut(os.path.join(worlds, "26.1-normal"), os.path.join(out, "26.1"), lambda _: first_chunks(2))
    generated(os.path.join(out, "26.1"), ("minecraft",))
    shutil.rmtree(os.path.join(out, "26.1-hardcore"), ignore_errors=True)
    shutil.copytree(os.path.join(out, "26.1"), os.path.join(out, "26.1-hardcore"))
    edit_level(os.path.join(out, "26.1-hardcore"), hardcore_single_player)
    interrupted_states(out, os.path.join(os.path.dirname(out), "interrupted"))
    # git keeps no empty folders: the committed worlds have none (the real worlds of the corpus do)
    for root in (out, os.path.join(os.path.dirname(out), "interrupted")):
        for folder, _, _ in sorted(os.walk(root), reverse=True):
            if not os.listdir(folder):
                os.rmdir(folder)


if __name__ == "__main__":
    main()
