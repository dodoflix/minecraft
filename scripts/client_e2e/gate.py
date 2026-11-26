#!/usr/bin/env python3
"""Client log gate: fail on anything the real client logged that is not known noise, and on log differences between sides.

    gate.py LOG [--baseline LOG] [--allow FILE] [--extra-dir DIR]... [--json]

LOG is the client's log (the console capture `client.out` or `game/latest.log`; both layouts are read, lines that are
not log lines, such as the compositor's, are skipped). A line is a finding when its level is WARN, ERROR or FATAL, or
when its text (with the stack trace under it) mentions an exception, `Failed to`, `Skipping packet`, or the isolation
check `[E2E-NET]`, unless its exact form `LEVEL: message`, followed by ` | ` and the exception line under it when there is one,
is in the allow list (--allow FILE, one per line, `#` comments). DEBUG and TRACE lines only count when they are findings: the client reports a packet it failed to
decode or send at DEBUG.

Every file in an --extra-dir (the client's `game/crash-reports` and `game/debug`) is a finding as well.

With --baseline (the log of the other side) the multisets of normalised lines are compared, DEBUG/TRACE and run noise
left out (client_e2e.normalise): a line only one side logged is a difference, for example `Loaded 2 advancements`.

Prints a JSON report; exit 0 iff there are no findings and no differences, 1 otherwise, 2 on usage or read errors.
"""
import collections
import json
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from client_e2e import normalise  # noqa: E402

# `[12:00:00] [Render thread/INFO]: text` (console) and `[12:00:00] [Render thread/INFO] [n.m.C]: text` (latest.log)
LINE = normalise.LINE
# lines that belong to the entry above: stack frames, causes, a bare exception class name
CONTINUATION = re.compile(r"^(\s+at |\s*Caused by: |\s*\.\.\. \d+ more|[\w.$]+(Exception|Error)\b)")
SUSPECT = re.compile(r"Exception|Failed to|Skipping packet|\[E2E-NET\]")
QUIET = ("DEBUG", "TRACE")


class Entry:
    def __init__(self, thread, level, message):
        self.thread, self.level, self.message = thread, level, message
        self.extra = []

    @property
    def text(self):
        return "\n".join([self.message, *self.extra])

    @property
    def line(self):
        """`LEVEL: message`, plus ` | ` and the exception line under it when the entry has one (the allow-list form)."""
        cause = self.extra[0].strip() if self.extra and not self.extra[0].lstrip().startswith("at ") else ""
        return f"{self.level}: {self.message}" + (f" | {cause}" if cause else "")

    def is_finding(self):
        return self.level in ("WARN", "ERROR", "FATAL") or bool(SUSPECT.search(self.text))


def parse(text):
    """The log entries of `text`, in order; unstructured lines that are not continuations are dropped."""
    entries = []
    for raw in text.splitlines():
        m = LINE.match(raw)
        if m:
            entries.append(Entry(*m.groups()))
        elif entries and CONTINUATION.match(raw):
            entries[-1].extra.append(raw)
    return entries


def load_allow(path):
    allowed = []
    with open(path, encoding="utf-8") as f:
        for raw in f:
            line = raw.rstrip("\n")
            if line.strip() and not line.lstrip().startswith("#"):
                allowed.append(line)
    return allowed


def findings(entries, allow=()):
    """(findings as `LEVEL: message [| cause]`, unused allow-list lines)."""
    allowed = set(allow)
    used, found = set(), []
    for e in entries:
        if not e.is_finding():
            continue
        if e.line in allowed:
            used.add(e.line)
            continue
        found.append(e.line)
    return found, sorted(allowed - used)


def multiset(entries):
    keys = collections.Counter()
    for e in entries:
        if e.level in QUIET and not SUSPECT.search(e.text):
            continue
        key = normalise.log_key(e.level, e.message)
        if key is not None:
            keys[key] += 1
    return keys


def parity(entries, baseline_entries):
    """Lines the baseline logged more often than `entries` (`missing`) and the other way round (`extra`)."""
    ours, theirs = multiset(entries), multiset(baseline_entries)
    return {
        "missing": sorted(f"{n}x {k}" for k, n in (theirs - ours).items()),
        "extra": sorted(f"{n}x {k}" for k, n in (ours - theirs).items()),
    }


def extra_files(directories):
    """One finding per file under the directories (crash reports, client debug reports)."""
    found = []
    for d in directories:
        for root, _, names in os.walk(d):
            found += [f"report file: {os.path.join(root, n)}" for n in sorted(names)]
    return sorted(found)


def gate(log_text, baseline_text=None, allow=(), extra_dirs=()):
    """The report: findings, unused allow-list lines, differences to the baseline, and the verdict."""
    entries = parse(log_text)
    found, unused = findings(entries, allow)
    found += extra_files(extra_dirs)
    report = {"ok": not found, "findings": found, "unused_allow": unused}
    if baseline_text is not None:
        diff = parity(entries, parse(baseline_text))
        report["parity"] = diff
        report["ok"] = report["ok"] and not diff["missing"] and not diff["extra"]
    return report


def _read(path):
    with open(path, encoding="utf-8", errors="replace") as f:
        return f.read()


def main(argv):
    if not argv or argv[0] in ("-h", "--help"):
        sys.stdout.write(__doc__)
        return 0 if argv else 2
    log, baseline, allow, extra, i = None, None, [], [], 0
    try:
        while i < len(argv):
            arg = argv[i]
            if arg == "--json":  # the report is always JSON; accepted for symmetry with the other tools
                i += 1
                continue
            if arg.startswith("--"):
                value = argv[i + 1]
                i += 2
                if arg == "--baseline":
                    baseline = value
                elif arg == "--allow":
                    allow += load_allow(value)
                elif arg == "--extra-dir":
                    extra.append(value)
                else:
                    raise ValueError(f"unknown option {arg}")
            elif log is not None:
                raise ValueError("needs exactly one log file (the other side's log goes after --baseline)")
            else:
                log, i = arg, i + 1
        if log is None:
            raise ValueError("needs the log file")
        report = gate(_read(log), None if baseline is None else _read(baseline), allow, extra)
    except (ValueError, IndexError, OSError) as e:
        sys.stderr.write(f"gate: {e}\n")
        return 2
    sys.stdout.write(json.dumps(report, indent=2) + "\n")
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
