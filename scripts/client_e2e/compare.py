"""Compares the artifacts of two sides of one scenario run (vanilla and ours, or any two) and lists the differences.

Every difference has a key `KIND:DETAIL` that `tests/e2e/client/known-differences.txt` can list as `SCENARIO KEY`:

    error:SIDE:STEP              a step of the side failed (timeout, client gone, control command refused); STEP is `line N (op)`,
                                 for a failure before the steps (server did not start) the message up to its first `:`
    screens                      the sequence of screen classes of the client differs
    disconnect                   the disconnect screen texts differ
    snapshot:LABEL:PATH          a snapshot value differs (PATH is dotted, list indices are numbers)
    chat:LABEL                   the `[CHAT]` lines differ
    client-log:SIDE:LINE         the client log has a finding on SIDE (gate.py)
    client-log:missing:LINE      the first side logged LINE (more often) than the second
    client-log:extra:LINE        the second side logged LINE (more often) than the first
    server-log:SIDE:LINE         the server log has an error line on SIDE
    server-log:missing:LINE      session line of the first server the second did not print
    server-log:extra:LINE        session line only the second printed
    client-report:SIDE:FILE      the client wrote a crash report or debug report on SIDE
    snapshot:LABEL:missing       only one side saved the snapshot
    shot:LABEL                   the screenshots differ beyond the thresholds (shotdiff.py)
"""
import collections
import copy
import fnmatch
import json
import os
import re

from client_e2e import gate, normalise, shotdiff

# values that depend on when the snapshot was taken, not on the server
DEFAULT_IGNORE = ("level.game_time", "level.day_time")
# packet counters vary with timing (client ticks, entity traffic): only the set of packet types is compared, unless the
# scenario pins a counter with `tolerate GLOB N` (N = 0 for an exact count)
PRESENCE_ONLY = ("packets.*",)
PLAYER_TYPE = "minecraft:player"


def _rewrite_strings(value):
    """Replaces the per-run ports of loopback addresses in every string."""
    if isinstance(value, str):
        return normalise.ADDRESS.sub(r"\1:<port>", value)
    if isinstance(value, list):
        return [_rewrite_strings(v) for v in value]
    if isinstance(value, dict):
        return {k: _rewrite_strings(v) for k, v in value.items()}
    return value


def normalise_snapshot(snap):
    """A copy with entity ids renumbered in level order (ids come from a per-server counter), the random UUIDs of non-player
    entities blanked and the ports of loopback addresses replaced."""
    snap = _rewrite_strings(copy.deepcopy(snap))
    entities = snap.get("entities")
    if isinstance(entities, list):
        ids = {e["id"]: n for n, e in enumerate(entities) if isinstance(e, dict) and "id" in e}
        for n, e in enumerate(entities):
            if not isinstance(e, dict):
                continue
            e["id"] = n
            e["passengers"] = [ids.get(p, p) for p in e.get("passengers") or []]
            if e.get("type") != PLAYER_TYPE:
                e["uuid"] = "<uuid>"
    return snap


def flatten(value, prefix=""):
    """{dotted path: leaf}; empty containers are leaves so that `[]` versus `{}` differs."""
    if isinstance(value, dict) and value:
        out = {}
        for k, v in value.items():
            out.update(flatten(v, f"{prefix}.{k}" if prefix else str(k)))
        return out
    if isinstance(value, list) and value:
        out = {}
        for i, v in enumerate(value):
            out.update(flatten(v, f"{prefix}.{i}" if prefix else str(i)))
        return out
    return {prefix: value}


def _within(path, a, b, tolerances):
    if isinstance(a, bool) or isinstance(b, bool):
        return False
    if not isinstance(a, (int, float)) or not isinstance(b, (int, float)):
        return False
    return any(fnmatch.fnmatchcase(path, glob) and abs(a - b) <= tol for glob, tol in tolerances)


def _path_order(path):
    """Sort key of a dotted path: list indices numerically, names alphabetically, at every level."""
    return [(0, int(t), "") if t.isdigit() else (1, 0, t) for t in path.split(".")]


def diff_snapshots(a, b, tolerances=(), ignore=DEFAULT_IGNORE):
    """[(path, value in a, value in b)] over the normalised snapshots, minus ignored paths and values within tolerance."""
    fa, fb = flatten(normalise_snapshot(a)), flatten(normalise_snapshot(b))
    missing = object()
    out = []
    for path in sorted(set(fa) | set(fb), key=_path_order):
        if any(fnmatch.fnmatchcase(path, g) for g in ignore):
            continue
        va, vb = fa.get(path, missing), fb.get(path, missing)
        if va is missing or vb is missing or va != vb:
            if va is not missing and vb is not missing:
                pinned = any(fnmatch.fnmatchcase(path, glob) for glob, _ in tolerances)
                if _within(path, va, vb, tolerances) or (not pinned and any(fnmatch.fnmatchcase(path, g) for g in PRESENCE_ONLY)):
                    continue
            out.append((path, "<absent>" if va is missing else va, "<absent>" if vb is missing else vb))
    return out


def _read(path, default=""):
    try:
        with open(path, encoding="utf-8", errors="replace") as f:
            return f.read()
    except FileNotFoundError:
        return default


def _json(path):
    text = _read(path, None)
    return None if text is None else json.loads(text)


def client_log_text(side_dir):
    """The richest client log of a side: game/latest.log (DEBUG of the network packages) when it was saved, else the console capture."""
    return _read(os.path.join(side_dir, "latest.log")) or _read(os.path.join(side_dir, "client.log"))


def _labels(side_dir, sub, ext):
    d = os.path.join(side_dir, sub)
    return sorted(n[: -len(ext)] for n in os.listdir(d) if n.endswith(ext)) if os.path.isdir(d) else []


def error_diffs(label, errors):
    """[(key, detail)] of a side's failed steps. The key names the step (`line 7 (wait_screen)`), not the message: timeouts
    print the last client state and control replies carry random ids, so a message cannot be listed as a known difference."""
    return [(f"error:{label}:{(re.match(r'line \d+ \(\w+\)', e) or re.match(r'[^:\n]*', e)).group(0)}", e) for e in errors]


def compare_sides(a_dir, b_dir, tolerances=(), allow=(), shot_options=None, server_allow=()):
    """Differences between side A (the baseline, normally vanilla) and side B as [(key, detail)]."""
    diffs = []
    meta = {s: _json(os.path.join(d, "side.json")) or {} for s, d in (("a", a_dir), ("b", b_dir))}
    label = {s: meta[s].get("label", s) for s in meta}
    for s in ("a", "b"):
        diffs += error_diffs(label[s], meta[s].get("errors", []))
    if meta["a"].get("screens") != meta["b"].get("screens"):
        diffs.append(("screens", f"{label['a']}: {meta['a'].get('screens')}  {label['b']}: {meta['b'].get('screens')}"))
    if meta["a"].get("disconnects") != meta["b"].get("disconnects"):
        diffs.append(("disconnect", f"{label['a']}: {meta['a'].get('disconnects')}  {label['b']}: {meta['b'].get('disconnects')}"))
    for name in sorted(set(_labels(a_dir, "snapshots", ".json")) | set(_labels(b_dir, "snapshots", ".json"))):
        sa, sb = (_json(os.path.join(d, "snapshots", name + ".json")) for d in (a_dir, b_dir))
        if sa is None or sb is None:
            diffs.append((f"snapshot:{name}:missing", f"only {label['b' if sa is None else 'a']} has the snapshot"))
            continue
        for path, va, vb in diff_snapshots(sa, sb, tolerances):
            diffs.append((f"snapshot:{name}:{path}", f"{label['a']}={json.dumps(va)} {label['b']}={json.dumps(vb)}"))
    for name in sorted(set(_labels(a_dir, "chat", ".txt")) | set(_labels(b_dir, "chat", ".txt"))):
        ca, cb = (_read(os.path.join(d, "chat", name + ".txt"), None) for d in (a_dir, b_dir))
        if ca != cb:
            diffs.append((f"chat:{name}", f"{label['a']}: {ca!r}  {label['b']}: {cb!r}"))
    texts = {s: client_log_text(d) for s, d in (("a", a_dir), ("b", b_dir))}
    for s in ("a", "b"):
        found, _ = gate.findings(gate.parse(texts[s]), allow)
        diffs += [(f"client-log:{label[s]}:{f}", f) for f in found]
    parity = gate.parity(gate.parse(texts["b"]), gate.parse(texts["a"]))
    diffs += [(f"client-log:missing:{line}", line) for line in parity["missing"]]
    diffs += [(f"client-log:extra:{line}", line) for line in parity["extra"]]
    for s, d in (("a", a_dir), ("b", b_dir)):
        diffs += [(f"client-report:{label[s]}:{os.path.basename(f)}", f) for f in gate.extra_files([os.path.join(d, "reports")])]
    servers = {s: _read(os.path.join(d, "server.log")) for s, d in (("a", a_dir), ("b", b_dir))}
    for s in ("a", "b"):
        diffs += [(f"server-log:{label[s]}:{f}", f) for f in normalise.server_findings(servers[s]) if f not in server_allow]
    lines_a, lines_b = collections.Counter(normalise.server_player_lines(servers["a"])), collections.Counter(normalise.server_player_lines(servers["b"]))
    diffs += [(f"server-log:missing:{k}", f"{n}x {k}") for k, n in (lines_a - lines_b).items()]
    diffs += [(f"server-log:extra:{k}", f"{n}x {k}") for k, n in (lines_b - lines_a).items()]
    for name in sorted(set(_labels(a_dir, "shots", ".png")) | set(_labels(b_dir, "shots", ".png"))):
        pa, pb = (os.path.join(d, "shots", name + ".png") for d in (a_dir, b_dir))
        if not (os.path.exists(pa) and os.path.exists(pb)):
            diffs.append((f"shot:{name}", "only one side has the screenshot"))
            continue
        report = shotdiff.compare(shotdiff.read_png(pa), shotdiff.read_png(pb), **(shot_options or {}))
        if not report["ok"]:
            bad = report.get("error") or max(report["regions"], key=lambda r: r["mean_abs_diff"])
            diffs.append((f"shot:{name}", f"{bad}"))
    return diffs


def load_known(path):
    """{(scenario, key)} from a known-differences file (`SCENARIO KEY` per line, `#` comments)."""
    known = set()
    for raw in _read(path).splitlines():
        line = raw.strip()
        if line and not line.startswith("#"):
            scenario, _, key = line.partition(" ")
            known.add((scenario, key.strip()))
    return known


def apply_known(scenario, diffs, known):
    """(unexpected differences, known ones that occurred, known entries of this scenario that did not occur)."""
    keys = {k for k, _ in diffs}
    unexpected = [d for d in diffs if (scenario, d[0]) not in known]
    listed = [d for d in diffs if (scenario, d[0]) in known]
    stale = sorted(k for s, k in known if s == scenario and k not in keys)
    return unexpected, listed, stale
