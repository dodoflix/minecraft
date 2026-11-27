#!/usr/bin/env python3
"""Matrix of every node of vanilla's command tree (ref/generated/reports/commands.json) against the command lines the
repository replays: the recorded command corpora, the scenario `cmd` verbs and the exception corpora. One TSV row per node;
a report lists the executable nodes that lack a success or a failure case, by command family, and the argument types.

Usage: scripts/command-matrix.py [--out FILE.tsv] [--report FILE.md] [--extra DIR ...] [--list] [--anomalies]

Evidence read (all under crates/mc-game/tests/fixtures unless noted):
  corpus     commands_corpus/*.txt: one case per command line; the outcome comes from the vanilla transcript
             commands_corpus/golden/<name>.vanilla.txt (a red message, or a message matching a vanilla exception text, is a
             failure). `@bot` lines run at permission level 4, `@bot0` lines at 0, console lines at 4.
  exception  commands/*exception_corpus*.txt: command + vanilla error text per line; every case is a failure.
  scenario   oracle/scenarios/*.txt: the `cmd`, `cmd_quiet`, `cmd_json`, `cmd_as`, `cmd_as_json`, `cmd_inform` verbs and the
             `difficulty`, `gamerule`, `time`, `biome` verbs that run a command; `cmd_as -N` runs at level N, `cmd_as NAME` at 4 when
             the scenario ops NAME, else 0. The outcome (`CF` lines of entity_oracle/<name>.txt) is known only for a
             scenario whose recorded results split into exactly as many cases as it has command lines; others are `unk`.
  extra      --extra DIR: more scenarios (DIR/*.txt or DIR/scenarios/*.txt, recordings in DIR/fixtures/)
  rust       string literals of the Rust test sources that parse as a complete command (no outcome; counted apart)

Columns of the TSV:
  family, path (`<name>` = argument node), kind (literal/argument), parser, executable, redirect, need_level (permission
  level the path needs: max of the `permissions` along it), reach (lines whose parse passed the node), end_ok / end_fail /
  end_unk (lines that ended on the node, by outcome), stop (lines stuck on the node with unparsed tokens left),
  bad_arg (lines whose token this argument node rejected), denied (lines the node refused for a source below need_level),
  levels (source levels of the lines that ended, stopped or were denied here, `level:count`), src_* (ends per evidence kind),
  rust (complete-command literals of Rust tests), status.
Status of an executable node: `uncovered` (no line ends on it), `needs-ok`, `needs-fail` (a case of that outcome is
missing; unknown outcomes do not count), `unk-only` (lines end on it but none has a known outcome), `covered`.
A failure case of a node is end_fail + stop + bad_arg.
"""
import argparse
import collections
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
REF = ROOT / "ref"
TESTS = ROOT / "crates/mc-game/tests"
LEVELS = {"all": 0, "moderators": 1, "gamemasters": 2, "admins": 3, "owners": 4}

# a scenario verb that replays as a command (see the replay's verb table)
VERB_COMMANDS = {
    "difficulty": lambda a: f"difficulty {a[0]}",
    "gamerule": lambda a: f"gamerule {a[0]} {a[1]}",
    "time": lambda a: f"time set {a[0]}",
    "biome": lambda a: "fillbiome " + " ".join(a[:7]),
}
# the root nodes the dispatcher registers with a redirect to the root
ROOT_REDIRECTS = {("execute", "run"), ("return", "run")}
# commands that may run no fork (or a function body) and so print no case at all
FORKING = {"execute", "function", "return", "schedule"}


# ---------------------------------------------------------------------------------------------------- tree

class Node:
    __slots__ = ("name", "kind", "parser", "props", "executable", "redirect", "perm", "children", "path", "need", "family",
                 "stats")

    def __init__(self, name, raw, parent):
        self.name = name
        self.kind = raw["type"]
        self.parser = raw.get("parser", "")
        self.props = raw.get("properties", {})
        self.executable = bool(raw.get("executable"))
        self.redirect = raw.get("redirect")
        perm = raw.get("permissions")
        self.perm = LEVELS[perm["permission"]["level"]] if perm else 0
        self.children = {}
        label = f"<{name}>" if self.kind == "argument" else name
        self.path = (parent.path + [label]) if parent else []
        self.need = max(self.perm, parent.need if parent else 0)
        self.family = self.path[0] if self.path else ""
        self.stats = collections.Counter()


def load_tree():
    raw = json.loads((REF / "generated/reports/commands.json").read_text())
    root = Node("", raw, None)
    nodes = []

    def build(node, rawnode):
        for name, r in rawnode.get("children", {}).items():
            child = Node(name, r, node)
            node.children[name] = child
            nodes.append(child)
            build(child, r)

    build(root, raw)
    return root, nodes


def redirect_target(root, node):
    """The node whose children follow `node`, or None for a node without a redirect."""
    path = tuple(p for p in node.path if not p.startswith("<"))
    if path in ROOT_REDIRECTS:
        return root
    if node.redirect is None:
        return None
    target = root
    for name in node.redirect:
        target = target.children[name]
    return target


# ---------------------------------------------------------------------------------------------------- tokens

def tokenize(line):
    """Split at blanks outside quotes, brackets and braces (selectors, SNBT, JSON text and block states hold blanks)."""
    tokens, i, n = [], 0, len(line)
    while i < n:
        if line[i] == " ":
            i += 1
            continue
        j, depth, quote = i, 0, None
        while j < n:
            c = line[j]
            if quote:
                if c == "\\":
                    j += 1
                elif c == quote:
                    quote = None
            elif c in "\"'" and (depth > 0 or j == i):
                quote = c
            elif c in "[{(":
                depth += 1
            elif c in "]})":
                depth = max(0, depth - 1)
            elif c == " " and depth == 0:
                break
            j += 1
        tokens.append(line[i:j])
        i = j
    return tokens


INT = re.compile(r"-?\d+$")
NUM = re.compile(r"-?[0-9.]+$")
TIME = re.compile(r"[-+]?[0-9.]+[a-zA-Z]*$")
MULTI = {"minecraft:block_pos": 3, "minecraft:vec3": 3, "minecraft:vec2": 2, "minecraft:column_pos": 2,
         "minecraft:rotation": 2}


def number(parser, tok, props):
    try:
        if parser in ("brigadier:integer", "brigadier:long"):
            if not INT.match(tok):
                return False
            v = int(tok)
        else:
            if not NUM.match(tok):
                return False
            v = float(tok)
    except ValueError:
        return False
    lo, hi = props.get("min"), props.get("max")
    return not ((lo is not None and v < lo) or (hi is not None and v > hi))


def take(node, tokens, i):
    """How many tokens the argument `node` takes at `i`, or None when its parser rejects them."""
    p, left = node.parser, len(tokens) - i
    if p == "brigadier:string" and node.props.get("type") == "greedy" or p == "minecraft:message":
        return left if left else None
    need = MULTI.get(p, 1)
    if left < need:
        return None
    tok = tokens[i]
    if p == "brigadier:bool":
        return 1 if tok in ("true", "false") else None
    if p in ("brigadier:integer", "brigadier:long", "brigadier:float", "brigadier:double"):
        return 1 if number(p, tok, node.props) else None
    if p == "minecraft:nbt_compound_tag":
        return 1 if tok.startswith("{") else None
    if p == "minecraft:time":
        return 1 if TIME.match(tok) else None
    return need


# ---------------------------------------------------------------------------------------------------- parse

class Parse:
    __slots__ = ("trail", "end", "i", "state", "rejected", "denied")

    def __init__(self, trail, end, i, state, rejected=(), denied=None):
        self.trail, self.end, self.i, self.state, self.rejected, self.denied = trail, end, i, state, rejected, denied

    def rank(self):
        full = 2 if self.state == "end" and self.end.executable else 1 if self.state == "end" else 0
        literals = sum(1 for n in self.trail if n.kind == "literal")
        return (full, self.i, literals)


def parse(root, tokens, level):
    """The best parse of `tokens` for a source at permission `level`: Parse(trail, end node, tokens used, state)."""
    n = len(tokens)
    results = []

    def go(node, i, trail):
        if i == n:
            results.append(Parse(trail, node, i, "end"))
            return
        target = redirect_target(ROOT_NODE, node)
        kids = (target if target is not None else node).children
        literal = [c for c in kids.values() if c.kind == "literal" and c.name == tokens[i]]
        matched, rejected = False, []
        for child in literal or kids.values():  # a literal that matches wins over the argument siblings
            if child.kind == "literal":
                if tokens[i] != child.name:
                    continue
                used = 1
            else:
                used = take(child, tokens, i)
                if used is None:
                    rejected.append(child)
                    continue
            matched = True
            if child.need > level:
                results.append(Parse(trail, node, i, "denied", denied=child))
                continue
            go(child, i + used, trail + [child])
        if not matched:
            results.append(Parse(trail, node, i, "stuck", rejected))

    go(root, 0, [])
    return max(results, key=Parse.rank) if results else None


ROOT_NODE = None


# ---------------------------------------------------------------------------------------------------- outcomes

def error_patterns():
    """Regexes of the vanilla texts a failing command prints: the exception messages of the command code."""
    lang = json.loads((REF / "src/assets/minecraft/lang/en_us.json").read_text())
    keys = {"command.unknown.command", "command.unknown.argument", "command.context.parse_error", "command.failed",
            "command.context.here", "command.expected.separator", "command.exception"}
    exc = re.compile(r"CommandExceptionType\(.{0,300}?translatable(?:Escape)?\(\"([\w.]+)\"", re.S)
    fail = re.compile(r"sendFailure\(\s*Component\.translatable(?:Escape)?\(\"([\w.]+)\"")
    for f in (REF / "src/net/minecraft").rglob("*.java"):
        text = f.read_text(errors="ignore")
        keys.update(m[1] for m in exc.finditer(text))
        keys.update(m[1] for m in fail.finditer(text))
    # keys assembled from pieces in the Java code
    keys.update(k for k in lang if re.search(r"\.(failed|failure)(\.|$)", k))
    keys.add("commands.drop.no_loot_table.entity")
    out = []
    for k in keys:
        if k in lang and lang[k]:
            parts = re.split(r"%(?:\d+\$)?[sd]", lang[k])
            out.append(re.compile("^" + ".+?".join(re.escape(p) for p in parts) + "$", re.S))
    return out


ERRORS = None
PREFIX = re.compile(r"^(?:log\| System chat: |\w+\| \[system_chat\] (?:\[Server: )?)")


def is_error_text(text):
    return "<--[HERE]" in text or any(p.match(text) for p in ERRORS)


def classify_transcript(lines):
    """`ok`, `fail` or `unk` for the output lines of one command in a corpus transcript."""
    texts, red = [], False
    for ln in lines:
        m = PREFIX.match(ln)
        if m:
            texts.append(ln[m.end():].rstrip("]") if "[Server: " in ln[:m.end() + 10] else ln[m.end():])
        elif re.match(r"^\w+\|   json ", ln):
            try:
                j = json.loads(ln.split("json ", 1)[1])
                red |= isinstance(j, dict) and j.get("color") == "red"
            except ValueError:
                pass
    if red or any(is_error_text(t) for t in texts):
        return "fail"
    return "ok" if texts else "unk"


def transcript_outcomes(path):
    """{line number: outcome} of a corpus transcript."""
    out, key, buf = {}, None, []
    for ln in path.read_text(errors="replace").split("\n"):
        m = re.match(r"^### \[[^:\]]+:(\d+)\] ", ln)
        if m:
            if key is not None:
                out[key] = classify_transcript(buf)
            key, buf = int(m[1]), []
        elif key is not None:
            buf.append(ln)
    if key is not None:
        out[key] = classify_transcript(buf)
    return out


def cf_units_of(lines):
    """The cases (`ok`/`fail`) the `CF` lines of a scenario recording split into, in order.

    A command that ran ends with a `result` line, a failed one followed by its failure messages; a command that did not
    parse prints a message and a `<--[HERE]` context line, both as failures and without a result."""
    units, run, after_failure = [], [], False

    def flush():
        nonlocal run, after_failure
        parse_units = 0
        while run and "<--[HERE]" in run[-1]:
            run = run[:-2]
            parse_units += 1
        if run and not after_failure:
            parse_units += 1
        units.extend(["fail"] * parse_units)
        run, after_failure = [], False

    for ln in lines:
        if not ln.startswith("CF ") or ln.startswith("CF json"):
            continue
        t = ln.split(" ", 2)
        if t[1] == "fail":
            run.append(ln)
            continue
        flush()
        if t[1] == "result":
            units.append("fail" if t[2].startswith("failure") else "ok")
            after_failure = units[-1] == "fail"
    flush()
    return units


def cf_units(path):
    return cf_units_of(path.read_text(errors="replace").split("\n"))


# ---------------------------------------------------------------------------------------------------- sources

def corpus_cases():
    d = TESTS / "fixtures/commands_corpus"
    for f in sorted(d.glob("*.txt")):
        outcomes = transcript_outcomes(d / "golden" / f"{f.stem}.vanilla.txt") if (d / "golden" / f"{f.stem}.vanilla.txt").exists() else {}
        for no, raw in enumerate(f.read_text().split("\n"), 1):
            line = raw.strip()
            if not line or line.startswith(("#", "!")):
                continue
            level = 4
            if line.startswith("@bot0 "):
                level, line = 0, line[6:]
            elif line.startswith("@bot "):
                level, line = 4, line[5:]
            if line.startswith("!"):
                continue
            yield "corpus", line.lstrip("/"), level, outcomes.get(no, "unk")


def exception_cases():
    d = TESTS / "fixtures/commands"
    for f in sorted(d.glob("*exception_corpus.txt")):
        for raw in f.read_text().split("\n"):
            if "\t" in raw:
                yield "exception", raw.split("\t", 1)[0].lstrip("/"), 4, "fail"


# the verbs a scenario starts with before the world boots: `difficulty` among them only sets a boot option
BOOT_VERBS = {"level", "dimension", "seed", "features", "datapack", "structures", "difficulty", "hardcore", "login_detail",
              "end", "nether", "levels", "preload", "preload_settle", "world_spawn", "bootrule", "note", "command_env"}


def scenario_lines(path):
    """[(command, level, prints feedback)] of the command-running lines of a scenario, in order."""
    ops, out, booted = set(), [], False
    for raw in path.read_text().split("\n"):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        a = line.split()
        verb = a[0]
        booted = booted or verb not in BOOT_VERBS
        if verb == "op" and len(a) > 1:
            ops.add(a[1])
        elif verb in ("cmd", "cmd_quiet", "cmd_json"):
            out.append((line[len(verb):].strip(), 4, verb != "cmd_quiet"))
        elif verb in ("cmd_as", "cmd_as_json", "cmd_inform") and len(a) > 2:
            who = a[1]
            level = int(who[1:]) if who.startswith("-") else 4 if who in ops else 0
            out.append((line[len(verb):].strip()[len(who):].strip(), level, True))
        elif verb in VERB_COMMANDS and (booted or verb != "difficulty"):
            try:
                out.append((VERB_COMMANDS[verb](a[1:]), 4, True))
            except IndexError:
                pass
    return out


def align(incomplete, drop, units):
    """The recorded case of each voiced command, or None when the optimal alignments disagree.

    A command takes one case; a case without a command (a command block, a fork of an `execute`) costs 1, a command without
    a case `drop[i]` (cheap for the commands that may run no fork); a command the grammar cannot complete only takes a
    failing case. The assignment of a command is kept when every optimal alignment makes it."""
    m, k = len(incomplete), len(units)
    inf = float("inf")

    def fits(i, j):
        return not (incomplete[i] and units[j] == "ok")

    def sweep(rev):
        cost = [[inf] * (k + 1) for _ in range(m + 1)]
        ways = [[0] * (k + 1) for _ in range(m + 1)]
        cost[0][0], ways[0][0] = 0, 1
        for i in range(m + 1):
            for j in range(k + 1):
                if i == j == 0:
                    continue
                opts = []
                if i and j and fits(*((m - i, k - j) if rev else (i - 1, j - 1))):
                    opts.append((cost[i - 1][j - 1], ways[i - 1][j - 1]))
                if j:
                    opts.append((cost[i][j - 1] + 1, ways[i][j - 1]))
                if i:
                    opts.append((cost[i - 1][j] + drop[m - i if rev else i - 1], ways[i - 1][j]))
                best = min(c for c, _ in opts)
                cost[i][j] = best
                ways[i][j] = sum(w for c, w in opts if c == best)
        return cost, ways

    fc, fw = sweep(False)
    bc, bw = sweep(True)
    best, total = fc[m][k], fw[m][k]
    out = [None] * m
    for i in range(m):
        hits = [j for j in range(k) if fits(i, j) and fc[i][j] + bc[m - i - 1][k - j - 1] == best
                and fw[i][j] * bw[m - i - 1][k - j - 1] == total]
        if hits:
            out[i] = hits[0]
    return out


def scenario_cases(scenarios, fixtures, kind):
    for f in sorted(scenarios.glob("*.txt")):
        lines = scenario_lines(f)
        if not lines:
            continue
        fx = fixtures / f.name
        units = cf_units(fx) if fx.exists() else None
        voiced = [(c, l) for (c, l, v) in lines if v]
        given = [None] * len(voiced)
        if units:
            incomplete, drop = [], []
            for cmd, level in voiced:
                toks = tokenize(cmd)
                r = parse(ROOT_NODE, toks, level)
                incomplete.append(r is None or r.state != "end" or not r.end.executable)
                drop.append(1 if toks and toks[0] in FORKING else 3)
            given = align(incomplete, drop, units)
        k = 0
        for cmd, level, v in lines:
            if v:
                yield kind, cmd, level, units[given[k]] if given[k] is not None else "unk"
                k += 1
            else:
                yield kind, cmd, level, "unk"


def rust_cases():
    lit = re.compile(r'"((?:[^"\\\n]|\\.)*)"')
    for f in sorted((ROOT / "crates").glob("*/tests/**/*.rs")):
        for m in lit.finditer(f.read_text(errors="ignore")):
            s = m[1].replace('\\"', '"').replace("\\\\", "\\").lstrip("/")
            if 3 < len(s) < 300 and s.split(" ", 1)[0] in ROOT_NODE.children and "\\n" not in s:
                yield "rust", s, 4, "unk"


# ---------------------------------------------------------------------------------------------------- main

def credit(case):
    kind, line, level, outcome = case
    tokens = tokenize(line)
    if not tokens:
        return None
    res = parse(ROOT_NODE, tokens, level)
    if res is None:
        return None
    if kind == "rust":
        if res.state == "end" and res.end.executable:
            res.end.stats["src_rust"] += 1
        return res
    for n in res.trail:
        n.stats["reach"] += 1
    if res.state == "end":
        node = res.end
        # the outcome of `execute ... run X` can come from a modifier as well as from X: it says nothing about X
        if any(tuple(p for p in n.path if not p.startswith("<")) in ROOT_REDIRECTS for n in res.trail):
            outcome = "unk"
        node.stats[f"end_{outcome}"] += 1
        node.stats[f"src_{kind}"] += 1
        node.stats[f"lv{level}"] += 1
    elif res.state == "stuck":
        if outcome != "ok":
            res.end.stats["stop"] += 1
            res.end.stats[f"lv{level}"] += 1
            for r in res.rejected:
                r.stats["bad_arg"] += 1
        else:
            res.end.stats["anomaly"] += 1
    else:
        res.denied.stats["denied"] += 1
        res.denied.stats[f"lv{level}"] += 1
    return res


def status(n):
    if not n.executable:
        return ""
    s = n.stats
    ends = s["end_ok"] + s["end_fail"] + s["end_unk"]
    fails = s["end_fail"] + s["stop"] + s["bad_arg"]
    if ends == 0:
        return "uncovered"
    if s["end_ok"] == 0 and fails == 0:
        return "unk-only"
    if s["end_ok"] == 0:
        return "needs-ok"
    return "needs-fail" if fails == 0 else "covered"


def selftest():
    global ERRORS
    ERRORS = error_patterns()
    assert tokenize('data merge entity @e[name="a b",limit=1] {a:"x y",b:[1, 2]}') == [
        "data", "merge", "entity", '@e[name="a b",limit=1]', '{a:"x y",b:[1, 2]}']
    assert tokenize("say it's a  test") == ["say", "it's", "a", "test"]

    def ends(line, level=4):
        r = parse(ROOT_NODE, tokenize(line), level)
        return r.state, " ".join(n.name for n in r.trail)

    assert ends("give @a minecraft:stick 5") == ("end", "give targets item count")
    assert ends("give @a minecraft:stick x")[0] == "stuck"
    assert ends("gamemode creative", 0)[0] == "denied"
    assert ends("execute as @a at @s run say hi") == ("end", "execute as targets at targets run say message")
    assert ends("tp @s 1 2 3") == ("end", "tp targets location")
    assert ends("gamemode")[0] == "end" and not parse(ROOT_NODE, ["gamemode"], 4).end.executable
    assert classify_transcript(["log| System chat: Unknown item 'x'", "log| System chat: ...<--[HERE]"]) == "fail"
    assert cf_units_of(["CF msg a", "CF result success 1", "CF result failure 0", "CF fail f", "CF fail g",
                        "CF fail Unknown", "CF fail ...<--[HERE]"]) == ["ok", "fail", "fail"]
    print("selftest ok")


def main():
    global ROOT_NODE, ERRORS
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--out", help="write the TSV here")
    ap.add_argument("--report", help="write the Markdown report here")
    ap.add_argument("--extra", action="append", default=[], metavar="DIR", help="more scenario directories")
    ap.add_argument("--list", action="store_true", help="print the report (also the default without --out/--report)")
    ap.add_argument("--anomalies", action="store_true", help="print lines the parser could not follow")
    ap.add_argument("--selftest", action="store_true", help="check the tokenizer and the tree parser, then exit")
    args = ap.parse_args()

    ROOT_NODE, nodes = load_tree()
    if args.selftest:
        return selftest()
    ERRORS = error_patterns()
    cases = list(corpus_cases()) + list(exception_cases())
    cases += scenario_cases(TESTS / "oracle/scenarios", TESTS / "fixtures/entity_oracle", "scenario")
    for d in args.extra:
        d = Path(d)
        sc = d / "scenarios" if (d / "scenarios").is_dir() else d
        cases += scenario_cases(sc, d / "fixtures", "extra")
    cases += list(rust_cases())

    anomalies, totals = [], collections.Counter()
    for case in cases:
        res = credit(case)
        totals[case[0]] += 1
        if res is None:
            anomalies.append(("empty", case))
        elif res.state == "stuck" and case[3] == "ok":
            anomalies.append(("stuck-but-ok", case))
    rows = []
    for n in nodes:
        s = n.stats
        levels = ",".join(f"{l}:{s[f'lv{l}']}" for l in range(5) if s[f"lv{l}"])
        target = "/".join(n.redirect) if n.redirect else ("(root)" if tuple(p for p in n.path if not p.startswith('<')) in ROOT_REDIRECTS else "")
        rows.append([n.family, " ".join(n.path), n.kind, n.parser, "yes" if n.executable else "", target, n.need,
                     s["reach"], s["end_ok"], s["end_fail"], s["end_unk"], s["stop"], s["bad_arg"], s["denied"], levels,
                     s["src_corpus"], s["src_exception"], s["src_scenario"], s["src_extra"], s["src_rust"], status(n)])
    header = ["family", "path", "kind", "parser", "executable", "redirect", "need_level", "reach", "end_ok", "end_fail",
              "end_unk", "stop", "bad_arg", "denied", "levels", "src_corpus", "src_exception", "src_scenario",
              "src_extra", "src_rust", "status"]
    if args.out:
        Path(args.out).write_text("\n".join("\t".join(str(c) for c in r) for r in [header] + rows) + "\n")
    report = build_report(nodes, rows, totals, anomalies)
    if args.report:
        Path(args.report).write_text(report)
    if args.list or not (args.out or args.report):
        print(report)
    if args.anomalies:
        for why, c in anomalies:
            print(f"{why}\t{c[0]}\tL{c[2]}\t{c[1][:160]}")


def build_report(nodes, rows, totals, anomalies):
    exe = [n for n in nodes if n.executable]
    by = collections.Counter(status(n) for n in exe)
    out = ["# Command matrix", "",
           f"{len(nodes)} nodes, {len(exe)} executable; {sum(totals.values())} command lines read "
           f"({', '.join(f'{k} {v}' for k, v in sorted(totals.items()))}); {len(anomalies)} lines the parser could not follow.", "",
           "Executable nodes by status: " + ", ".join(f"{k} {v}" for k, v in sorted(by.items())), ""]
    out += ["## Source levels", ""]
    lv = collections.Counter()
    for n in nodes:
        for l in range(5):
            lv[l] += n.stats[f"lv{l}"]
    out += ["Lines ending, stuck or denied at a node, by source level: " + ", ".join(f"level {l}: {lv[l]}" for l in range(5)), ""]
    roots = [n for n in nodes if len(n.path) == 1]
    out += ["## Commands needing a permission level without a denied line", ""]
    for n in roots:
        if n.need > 0 and not any(m.stats["denied"] for m in nodes if m.path[:1] == n.path[:1]):
            out.append(f"- {n.name} (level {n.need})")
    out += ["", "## Executable nodes not covered, by family", ""]
    fam = collections.defaultdict(list)
    for n in exe:
        st = status(n)
        if st != "covered":
            fam[n.family].append((st, " ".join(n.path)))
    for f in sorted(fam):
        total = sum(1 for n in exe if n.family == f)
        out.append(f"### {f} ({len(fam[f])} of {total} executable nodes)")
        out += [f"- {st}: `{p}`" for st, p in fam[f]]
        out.append("")
    out += ["## Argument types", "", "| parser | nodes | lines ending on one | rejected |", "|---|---|---|---|"]
    ap = collections.defaultdict(lambda: [0, 0, 0])
    for n in nodes:
        if n.kind == "argument":
            r = ap[n.parser]
            r[0] += 1
            r[1] += n.stats["end_ok"] + n.stats["end_fail"] + n.stats["end_unk"]
            r[2] += n.stats["bad_arg"]
    golden = {p.name[:-len("_golden.txt")] for p in (TESTS / "fixtures/args").glob("*_golden.txt")}
    for p, (cnt, ends, rej) in sorted(ap.items()):
        short = p.split(":")[-1]
        mark = " (parser corpus)" if short in golden else ""
        out.append(f"| {p}{mark} | {cnt} | {ends} | {rej} |")
    return "\n".join(out) + "\n"


if __name__ == "__main__":
    sys.exit(main())
