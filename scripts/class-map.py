#!/usr/bin/env python3
"""Maps every class of the vanilla server jar to the Rust code that implements it.

  scripts/class-map.py [--jar JAR] [--out FILE] [--summary]

Reads the jar with scripts/ClassDump.java (JDK 24+, `java.lang.classfile`), joins each class with crates/*/src and writes one
TSV row per class (stdout, or FILE):

  class  kind  nest  lines  rust_files  status  how  reason

  lines       top-level classes: lines of ref/src/<class>.java; nested classes: span of their bytecode line table
  rust_files  up to three files, best match first (nested classes: the files of their outer class)
  status      ported     the class has its own Rust unit (same-name file or type, or a registration naming it)
              named      the name occurs three or more times (or in two files) in Rust code or comments, no unit of its own
              mentioned  the name occurs once or twice in one file: weakest evidence, read before relying on it
              folded     implemented without a unit of its own; `reason` says where or why
              nested     nested/anonymous/local class of a ported, named or folded class (of a mentioned class: mentioned)
              missing    no Rust counterpart yet, though one is needed; `reason` says what is not implemented
              n/a        no counterpart needed; `reason` says why
              unclassified   no rule applies (the run exits 1 when any exists)
  how         the rule that decided: stem, item, registered, rule, block-overrides, structure, outer, named, ...

Rules without a name match live in scripts/class-map-rules.tsv: `regex<TAB>status<TAB>rust files<TAB>reason`, first match wins,
the regex is matched against the binary class name (`net/minecraft/...`).
Needs: java 24+, ref/ (scripts/setup-ref.sh), python3.
"""
import argparse
import collections
import glob
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PREFIX = "net/minecraft/"
# Vanilla package segments that carry no information when ranking candidate Rust paths.
GENERIC = {"net", "minecraft", "world", "level", "server", "com", "mojang", "util"}


def snake(name):
    return re.sub(r"(?<=[a-z0-9])(?=[A-Z])|(?<=[A-Z])(?=[A-Z][a-z])", "_", name).lower()


def package_segments(name):
    return [s for s in name[len(PREFIX) :].split("/")[:-1] if s not in GENERIC] if name.startswith(PREFIX) else []


def rust_sources():
    for d, dirs, fs in os.walk(os.path.join(ROOT, "crates")):
        crate_root = os.path.dirname(d) == os.path.join(ROOT, "crates")
        # the test client is not the server; a crate's build directory is not source
        dirs[:] = [x for x in dirs if x != "tests" and not (crate_root and x == "target") and not (d.endswith("/crates") and x == "mc-testbot")]
        for f in fs:
            if f.endswith(".rs"):
                yield os.path.join(d, f)


class RustIndex:
    def __init__(self, simple_names):
        self.simple_names = simple_names
        self.stems = collections.defaultdict(list)
        self.items = collections.defaultdict(set)
        self.words = collections.defaultdict(collections.Counter)
        self.registered = collections.defaultdict(set)
        self.all_text = ""
        for path in sorted(rust_sources()):
            rel = os.path.relpath(path, ROOT)
            text = open(path, errors="replace").read()
            self.stems[os.path.basename(path)[:-3]].append(rel)
            for m in re.finditer(r"^([ \t]*(?:(?:///[^\n]*|#\[[^\n]*\])\n[ \t]*)*)(pub(?:\([a-z]+\))?\s+)?(?:struct|enum|trait|type|union)\s+([A-Za-z_]\w*)", text, re.M):
                doc, public, item = m.groups()
                # a private type counts only when its doc comment starts with its own name in backticks
                first = re.search(r"///\s*`([^`]+)`", doc)
                named = re.split(r"[.:]+", re.sub(r"\(.*?\)", "", first.group(1))) if first else []
                if first and item not in named:
                    continue  # documented as a different vanilla type (`Outer.Inner`), not as this one
                if public or (first and named[0] == item):
                    self.items[item].add(rel)
            for m in re.finditer(r"\bregister_\w+!\(\s*\"([A-Za-z_]\w*)\"", text):
                self.registered[m.group(1)].add(rel)
            for w in re.findall(r"[A-Za-z_]\w*", text):
                self.words[w][rel] += 1

    def best(self, files, segments, name=""):
        """Candidate files ordered by how many package segments their path shares with the vanilla package.

        Files of the datafixer crates only count for vanilla datafixer classes: they define many generic type names."""
        if not name.startswith(PREFIX + "util/datafix"):
            files = [f for f in files if not f.startswith(("crates/mc-datafix/", "crates/mc-dfu/"))]
        if segments and len(self.simple_names[name.rsplit("/", 1)[1]]) > 1:
            # several vanilla classes share this simple name: a file goes to the class whose package fits it best, none on a tie
            def fit(f, cls):
                parts = set(re.split(r"[/_.]", f))
                segs = package_segments(cls)
                shared = sum(1 for s in segs if s in parts)
                return shared - 0.5 * (len(segs) - shared)  # a file fits a deeper package it also names better

            others = [c for c in self.simple_names[name.rsplit("/", 1)[1]] if c != name]
            files = [f for f in files if fit(f, name) > max(fit(f, c) for c in others)]
        def score(f):
            parts = set(re.split(r"[/_.]", f))
            return -sum(1 for s in segments if s in parts)
        return sorted(files, key=lambda f: (score(f), len(f), f))[:3]


def load_rules():
    rules = []
    path = os.path.join(ROOT, "scripts/class-map-rules.tsv")
    for n, line in enumerate(open(path), 1):
        if not line.strip() or line.startswith("#"):
            continue
        cols = line.rstrip("\n").split("\t")
        if len(cols) != 4 or cols[1] not in ("n/a", "folded", "missing"):
            sys.exit(f"{path}:{n}: expected `regex<TAB>n/a|folded|missing<TAB>rust files<TAB>reason`")
        for f in filter(None, cols[2].split(",")):
            if not os.path.exists(os.path.join(ROOT, f)):
                sys.exit(f"{path}:{n}: no such Rust file: {f}")
        rules.append((re.compile(cols[0]), cols[1], cols[2], cols[3]))
    return rules


def block_overrides():
    """Server-side overrides of every vanilla block class (fixture of the block binding guard) and its masked-class table."""
    tests = os.path.join(ROOT, "crates/mc-game/tests")
    guard = open(tests + "/golden/block_class_binding.rs").read()
    client = set(re.findall(r'"(\w+)"', guard.split("const DATA_OR_CLIENT")[1].split("];")[0]))
    masked = dict(re.findall(r'\(\s*"(\w+)",\s*"([^"]+)",?\s*\)', guard.split("const MASKED")[1].split("];")[0]))
    over = {}
    for line in open(tests + "/fixtures/block_class_overrides.txt"):
        cls, _, methods = line.rstrip("\n").partition(" ")
        over[cls] = {m for m in methods.split(",") if m}
    return client, masked, over


def dump(jar):
    out = subprocess.run(["java", os.path.join(ROOT, "scripts/ClassDump.java"), jar], check=True, capture_output=True, text=True).stdout
    rows = {}
    for line in out.splitlines():
        c = line.split("\t")
        rows[c[0]] = dict(zip(("kind", "nest", "outer", "sup", "ifaces", "lo", "hi", "methods", "own", "fields"), c[1:]))
    return rows


def source_lines(name):
    path = os.path.join(ROOT, "ref/src", name + ".java")
    return sum(1 for _ in open(path, errors="replace")) if os.path.exists(path) else 0


def main():
    ap = argparse.ArgumentParser(description="Map vanilla server classes to Rust code")
    ap.add_argument("--jar")
    ap.add_argument("--out")
    ap.add_argument("--summary", action="store_true", help="print status counts to stderr")
    args = ap.parse_args()
    jar = args.jar or sorted(glob.glob(os.path.join(ROOT, "ref/bundle/META-INF/versions/*/server-*.jar")))[-1]

    classes = dump(jar)
    names = collections.defaultdict(list)
    for n in classes:
        names[n.rsplit("/", 1)[1]].append(n)
    rs = RustIndex(names)
    rules = load_rules()
    client, masked, over = block_overrides()
    result = {}

    def decide(name, c):
        simple = name.rsplit("/", 1)[1]
        segs = [s for s in name[len(PREFIX):].split("/")[:-1] if s not in GENERIC] if name.startswith(PREFIX) else []
        if c["kind"] in ("package-info", "module-info"):
            return "n/a", [], "structure", "package marker, no code"
        if c["kind"] == "annotation":
            return "n/a", [], "structure", "annotation without runtime behaviour"
        for rx, status, files, reason in rules:
            if status == "n/a" and rx.fullmatch(name):
                return status, [], "rule", reason
        for how, index, key in (("stem", rs.stems, snake(simple)), ("item", rs.items, simple), ("registered", rs.registered, simple)):
            found = rs.best(index.get(key, ()), segs, name)
            if found:
                return "ported", found, how, ""
        for rx, status, files, reason in rules:
            if rx.fullmatch(name):
                return status, files.split(",") if files else [], "rule", reason
        if name.startswith(PREFIX + "world/level/block/") and (simple in over or simple in masked):
            server_side = sorted(over.get(simple, set()) - client)
            if simple in masked:
                return "folded", [], "block-overrides", "masked subclass: " + masked[simple]
            if not server_side:
                return "folded", [], "block-overrides", "overrides only data or client-side methods: " + (",".join(sorted(over[simple])) or "none")
        seen = {f: n for f, n in rs.words.get(simple, {}).items() if rs.best([f], segs, name)}
        if seen:
            strong = sum(seen.values()) >= 3 or len(seen) >= 2
            top = sorted(seen, key=lambda f: (-seen[f], f))[:3]
            return ("named" if strong else "mentioned"), top, "named", ""
        if c["kind"] == "interface" and c["own"] == "0":
            return "folded", [], "structure", "marker interface without methods"
        if c["own"] == "0" and c["fields"] == "0" and c["kind"] in ("class", "record") and c["nest"] == "top":
            return "folded", [], "structure", f"no methods or state of its own besides constructors (extends {c['sup'].rsplit('/', 1)[-1]})"
        return "unclassified", [], "", ""

    def resolve(name):
        if name in result:
            return result[name]
        c = classes[name]
        if c["nest"] != "top" and c["outer"] in classes:
            status, files, how, reason = decide(name, c)
            if status in ("unclassified", "named", "mentioned", "folded") or how == "structure":
                ostatus, ofiles, _, oreason = resolve(c["outer"])
                if ostatus != "unclassified":
                    status = ostatus if ostatus in ("n/a", "mentioned") else "nested"
                    files, how = ofiles, "outer"
                    reason = f"{c['nest']} class of {c['outer'].rsplit('/', 1)[-1]}" + (f": {oreason}" if ostatus == "n/a" else "")
            result[name] = (status, files, how, reason)
        else:
            result[name] = decide(name, c)
        return result[name]

    rows = []
    for name in sorted(classes):
        c = classes[name]
        status, files, how, reason = resolve(name)
        if c["nest"] == "top":
            lines = source_lines(name)
        else:
            lines = int(c["hi"]) - int(c["lo"]) + 1 if c["hi"] != "0" else 0
        rows.append((name.replace("/", "."), c["kind"], c["nest"], str(lines), ",".join(files), status, how, reason))

    out = open(args.out, "w") if args.out else sys.stdout
    out.write("class\tkind\tnest\tlines\trust_files\tstatus\thow\treason\n")
    for r in rows:
        out.write("\t".join(r) + "\n")
    if args.out:
        out.close()
    counts = collections.Counter(r[5] for r in rows)
    if args.summary or counts["unclassified"]:
        print(f"{len(rows)} classes: " + ", ".join(f"{n} {s}" for s, n in sorted(counts.items())), file=sys.stderr)
    bad = [r[0] for r in rows if r[5] == "n/a" and not r[7]]
    if bad:
        print(f"n/a without a reason: {bad[:5]}", file=sys.stderr)
    sys.exit(1 if counts["unclassified"] or bad else 0)


main()
