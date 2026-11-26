#!/usr/bin/env python3
"""Download the real 26.3 client (jar, linux libraries, asset index + assets) into a cache dir
and print the launch classpath (one line) on stdout. Idempotent; verifies sha1.

Usage: scripts/mc-client-prepare.py CACHE_DIR [--all-assets]
       scripts/mc-client-prepare.py --servers-dat OUT NAME=HOST:PORT...
Sound objects (minecraft/sounds/*.ogg) are skipped unless --all-assets: the harness runs without audio.
--servers-dat writes a server list (the client's servers.dat, plain NBT) with those entries in that order, then exits.
"""
import hashlib, json, os, struct, sys, urllib.request
from concurrent.futures import ThreadPoolExecutor

if sys.argv[1:2] in (["-h"], ["--help"]):
    print(__doc__)
    sys.exit(0)

def servers_dat(out, entries):
    def string(tag_name, value):
        n, v = tag_name.encode(), value.encode()
        return b"\x08" + struct.pack(">H", len(n)) + n + struct.pack(">H", len(v)) + v

    items = b""
    for e in entries:
        name, _, ip = e.partition("=")
        if not ip:
            sys.exit(f"--servers-dat: expected NAME=HOST:PORT, got {e!r}")
        items += string("ip", ip) + string("name", name) + b"\x00"
    nbt = b"\x0a\x00\x00" + b"\x09" + struct.pack(">H", 7) + b"servers" + b"\x0a" + struct.pack(">i", len(entries)) + items + b"\x00"
    os.makedirs(os.path.dirname(os.path.abspath(out)), exist_ok=True)
    with open(out, "wb") as f:
        f.write(nbt)


if sys.argv[1:2] == ["--servers-dat"]:
    servers_dat(sys.argv[2], sys.argv[3:])
    sys.exit(0)

root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
cache = os.path.abspath(sys.argv[1])
all_assets = "--all-assets" in sys.argv
meta = json.load(open(os.path.join(root, "ref/26.3.json")))


def log(*a):
    print(*a, file=sys.stderr, flush=True)


def sha1(path):
    h = hashlib.sha1()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def fetch(url, dest, want_sha1):
    if os.path.exists(dest) and sha1(dest) == want_sha1:
        return
    os.makedirs(os.path.dirname(dest), exist_ok=True)
    tmp = dest + ".part"
    for attempt in range(4):
        try:
            with urllib.request.urlopen(url, timeout=60) as r, open(tmp, "wb") as f:
                while chunk := r.read(1 << 20):
                    f.write(chunk)
            if sha1(tmp) != want_sha1:
                raise IOError(f"sha1 mismatch for {url}")
            os.replace(tmp, dest)
            return
        except Exception as e:
            if attempt == 3:
                raise
            log(f"retry {url}: {e}")


def allowed(lib):
    rules = lib.get("rules")
    if not rules:
        return True
    ok = False
    for r in rules:
        os_ = r.get("os")
        if os_ is None or (os_.get("name") == "linux" and "arch" not in os_):
            ok = r["action"] == "allow"
    return ok


jobs, cp = [], []
client = meta["downloads"]["client"]
jar = os.path.join(cache, "client.jar")
jobs.append((client["url"], jar, client["sha1"]))
for lib in meta["libraries"]:
    if not allowed(lib):
        continue
    a = lib["downloads"]["artifact"]
    p = os.path.join(cache, "libraries", a["path"])
    jobs.append((a["url"], p, a["sha1"]))
    cp.append(p)
cp.append(jar)

ai = meta["assetIndex"]
index_path = os.path.join(cache, "assets/indexes", ai["id"] + ".json")
fetch(ai["url"], index_path, ai["sha1"])
for name, o in json.load(open(index_path))["objects"].items():
    if not all_assets and name.startswith("minecraft/sounds/"):
        continue
    h = o["hash"]
    jobs.append((f"https://resources.download.minecraft.net/{h[:2]}/{h}", os.path.join(cache, "assets/objects", h[:2], h), h))

log(f"{len(jobs)} files to verify/download")
with ThreadPoolExecutor(16) as ex:
    list(ex.map(lambda j: fetch(*j), jobs))
print(":".join(cp))
