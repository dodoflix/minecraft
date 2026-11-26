#!/usr/bin/env python3
"""Matrix of every vanilla packet (state x flow x id, from ref/generated/reports/packets.json) against the evidence the
repository holds for it. One TSV row per packet; the empty cells of each column are listed on stdout.

Usage: scripts/packet-matrix.py [--out FILE.tsv] [--client LABEL=SNAPSHOT.json ...] [--list]

Columns (a cell is a count; 0 = no evidence):
  codec_test    mentions in the codec golden tests (crates/mc-protocol/tests/golden)
  vanilla_gen   mentions of the vanilla class in the Java programs that record the codec fixtures
  replay_pkt    `PKT <player> minecraft:<name>` lines of the recorded replay fixtures (clientbound play only)
  dual_fixture  `<state>/<name>` observation lines of the dual-run fixtures (crates/mc-server/tests/fixtures)
  dual_test     mentions in the dual/oracle tests (crates/mc-server/tests, crates/mc-protocol/tests/oracle)
  replay_test   mentions in the replay and golden drivers (crates/mc-game/tests)
  testbot       mentions in the scripted client (crates/mc-testbot/src): decode arm (clientbound) or send (serverbound)
  impl          mentions in the server code outside the protocol crate: construction (clientbound) or handler (serverbound)
  client_LABEL  packets of that type a real client received (clientbound) or sent (serverbound) in the snapshot given
                with --client LABEL=FILE (the `packets` section of the client state snapshot, scripts/client-ctl.sh
                NAME snapshot FILE, taken after a join against the server under test)
A mention is the Rust packet type, or `<Enum>::<Variant>` (also through `use <Enum> as P;`). A class stamped into several
states (`shared` = yes) shares its type name, so only the enum-qualified mentions tell its states apart.
"""
import argparse
import collections
import gzip
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
REF = ROOT / "ref"
CRATES = ROOT / "crates"
STATES = ["handshake", "status", "login", "configuration", "play"]
NOT_IMPL = {"mc-protocol", "mc-protocol-macros", "mc-testbot", "mc-testkit"}


def files(base, pattern):
    return sorted(p for p in base.glob(pattern) if p.is_file())


def rust_packets():
    """{(state, flow): [(variant, struct)]} in enum (= packet id) order."""
    out = {}
    text = "\n".join(p.read_text() for p in files(CRATES / "mc-protocol/src/packets", "**/*.rs"))
    for m in re.finditer(r"pub enum (Clientbound|Serverbound)(\w+?)Packet \{(.*?)\n\s*\}", text, re.S):
        flow, state = m[1].lower(), m[2].lower()
        out[(state, flow)] = re.findall(r"(\w+)\((\w+)\)", m[3])
    return out


def rust_names():
    """{struct: (state, flow, name)} from the `#[packet(..)]` attributes (packets stamped by a macro are absent)."""
    text = "\n".join(p.read_text() for p in files(CRATES / "mc-protocol/src/packets", "**/*.rs"))
    rx = r'#\[packet\(state = (\w+), flow = (\w+), name = "([^"]+)"[^)]*\)\]\s*pub struct (\w+)'
    return {m[4]: (m[1], m[2], m[3]) for m in re.finditer(rx, text)}


def vanilla_classes():
    """{(flow, name): class} from the PacketType constants of the protocol packages."""
    out = {}
    for p in files(REF / "src/net/minecraft/network/protocol", "**/*PacketTypes.java"):
        for m in re.finditer(r'PacketType<([\w.]+)>\s+\w+\s*=\s*create(Clientbound|Serverbound)?\w*\(\s*"([\w/]+)"\s*\)', p.read_text()):
            out[(m[2].lower() if m[2] else "serverbound", m[3])] = m[1]
    return out


def mentions(paths, enums):
    """Counter of type names and of (enum, variant) pairs over the sources, aliases of the enums resolved."""
    types, variants = collections.Counter(), collections.Counter()
    for p in paths:
        text = p.read_text(errors="replace")
        for m in re.finditer(r"\b(\w+Packet)\s+as\s+(\w+)\b", text):
            if m[1] in enums:
                text = re.sub(rf"\b{m[2]}::", f"{m[1]}::", text)
        types.update(re.findall(r"\b(?:Clientbound|Serverbound|ClientIntention)\w*", text))
        variants.update(re.findall(r"\b(\w+Packet)::(\w+)\b", text))
    return types, variants


def fixture_lines(paths, pattern):
    """Counter of what `pattern` captures (a name, or a (state, name) tuple) over the text fixtures."""
    c = collections.Counter()
    rx = re.compile(pattern, re.M)
    for p in paths:
        data = gzip.decompress(p.read_bytes()) if p.suffix == ".gz" else p.read_bytes()
        c.update(rx.findall(data.decode(errors="replace")))
    return c


def client_counts(path, flow):
    """{(state, name): count} of the snapshot's received (clientbound) or sent (serverbound) packets."""
    packets = json.loads(Path(path).read_text())["packets"]
    out = {}
    for key, n in packets["in" if flow == "clientbound" else "out"].items():
        m = re.fullmatch(r"(\w+):(clientbound|serverbound)/(?:minecraft:)?([\w/]+)", key)
        if m and m[2] == flow:
            state = {"handshaking": "handshake"}.get(m[1].lower(), m[1].lower())
            out[(state, m[3])] = n
    return out


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out", help="write the TSV here (default: stdout of the empty-cell summary only)")
    ap.add_argument("--client", action="append", default=[], metavar="LABEL=SNAPSHOT.json")
    ap.add_argument("--list", action="store_true", help="name every empty cell, not only count them")
    args = ap.parse_args()

    report = json.loads((REF / "generated/reports/packets.json").read_text())
    rust = rust_packets()
    classes, meta = vanilla_classes(), rust_names()
    enums = {f"{f.capitalize()}{s.capitalize()}Packet" for (s, f) in rust}
    srcs = {
        "codec_test": files(CRATES / "mc-protocol/tests/golden", "*.rs"),
        "dual_test": files(CRATES / "mc-server/tests", "**/*.rs") + files(CRATES / "mc-protocol/tests/oracle", "*.rs"),
        "replay_test": files(CRATES / "mc-game/tests", "**/*.rs"),
        "testbot": files(CRATES / "mc-testbot/src", "**/*.rs") + files(CRATES / "mc-testkit/src", "**/*.rs"),
        "impl": [p for c in sorted(CRATES.iterdir()) if c.name not in NOT_IMPL for p in files(c, "src/**/*.rs")],
    }
    counts = {col: mentions(paths, enums) for col, paths in srcs.items()}
    # the Java programs name the vanilla class (nested ones as Outer.Inner), not the Rust type
    java = "\n".join(p.read_text(errors="replace") for p in files(CRATES / "mc-protocol/tests/fixtures", "**/*.java"))
    replay = fixture_lines(files(CRATES / "mc-game/tests/fixtures", "**/*.txt*"), r"^PKT (?:\S+ )?minecraft:([\w/]+)")
    dual = fixture_lines(
        files(CRATES / "mc-server/tests/fixtures", "**/*.txt*"), r"\b(handshake|status|login|configuration|config|play)/([\w/]+)"
    )
    clients = {}
    for spec in args.client:
        label, _, path = spec.partition("=")
        if not label or not Path(path).is_file():
            ap.error(f"--client {spec}: expected LABEL=SNAPSHOT.json of an existing file")
        clients[label] = {f: client_counts(path, f) for f in ("clientbound", "serverbound")}

    cols = ["state", "flow", "id", "name", "class", "rust_type", "shared", "codec_test", "vanilla_gen", "replay_pkt",
            "dual_fixture", "dual_test", "replay_test", "testbot", "impl"] + [f"client_{k}" for k in clients]
    shared = collections.Counter(
        (flow, name.removeprefix("minecraft:"))
        for state in STATES
        for flow in ("clientbound", "serverbound")
        for name in report.get(state, {}).get(flow, {})
    )
    rows, problems = [], []
    for state in STATES:
        for flow in ("clientbound", "serverbound"):
            entries = {k.removeprefix("minecraft:"): v["protocol_id"] for k, v in report.get(state, {}).get(flow, {}).items()}
            enum = f"{flow.capitalize()}{state.capitalize()}Packet"
            variants = rust.get((state, flow), [])
            for name, pid in sorted(entries.items(), key=lambda kv: kv[1]):
                variant, struct = variants[pid] if pid < len(variants) else ("", "")
                if meta.get(struct, (state, flow, name))[1:] != (flow, name):
                    problems.append(f"{state}/{flow}/{pid} {name}: rust type {struct} is {meta[struct][1:]}")
                cls = classes.get((flow, name), "")
                row = {"state": state, "flow": flow, "id": pid, "name": name, "class": cls, "rust_type": struct,
                       "shared": "yes" if shared[(flow, name)] > 1 else "no"}
                for col, (types, vars_) in counts.items():
                    row[col] = types[struct] + vars_[(enum, variant)]
                row["vanilla_gen"] = len(re.findall(rf"\b{re.escape(cls)}\b", java)) if cls else 0
                row["replay_pkt"] = replay[name] if (state, flow) == ("play", "clientbound") else 0
                row["dual_fixture"] = sum(
                    n for (s, nm), n in dual.items() if nm == name and {"config": "configuration"}.get(s, s) == state
                )
                for label, c in clients.items():
                    row[f"client_{label}"] = c[flow].get((state, name), 0)
                rows.append(row)
    if args.out:
        Path(args.out).write_text("\n".join("\t".join(str(r[c]) for c in cols) for r in [dict(zip(cols, cols))] + rows) + "\n")
    print(f"{len(rows)} packets ({sum(r['flow'] == 'clientbound' for r in rows)} clientbound, "
          f"{sum(r['flow'] == 'serverbound' for r in rows)} serverbound)")
    for p in problems:
        print("MISMATCH", p)
    for col in cols[7:]:
        empty = [f"{r['state']}/{r['name']}" for r in rows if not r[col] and not (
            col == "replay_pkt" and (r["state"], r["flow"]) != ("play", "clientbound"))]
        print(f"{col}: {len(empty)} empty" + (": " + " ".join(empty) if args.list else ""))


if __name__ == "__main__":
    sys.exit(main())
