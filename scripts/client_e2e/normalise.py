"""Normalisation of client log lines: what differs between two runs of the same scenario without meaning anything.

`log_key` turns a parsed log entry into the string two runs are compared by, or None for lines that are pure run noise
(state heartbeats, GPU buffer growth, asset loading order).
"""
import re

# `[12:00:00] [Render thread/INFO]: text` (console) and `[12:00:00] [Render thread/INFO] [n.m.C]: text` (log file): the
# format of the client's and the server's log lines, groups thread, level, message
LINE = re.compile(r"^\[\d\d:\d\d:\d\d\] \[([^/\]]+)/([A-Z]+)\](?: \[[^\]]*\])?: (.*)$")

# messages that carry nothing for the comparison: heartbeats and control replies are compared as state, the rest depends
# on the machine's frame timing or on thread scheduling
IGNORED_PREFIXES = (
    "[STDOUT]: [E2E] ",
    "[STDOUT]: [E2E-CTL] ",
    "Resizing ",
    "Created: ",
    "Found unifont",
)

# `127.0.0.1:54321`: the port is chosen per run
ADDRESS = re.compile(r"\b(127\.0\.0\.1|localhost):\d+")

# (pattern, replacement), applied in order to the message
REWRITES = (
    (re.compile(r"^Setting user: .*"), "Setting user: <user>"),
    (re.compile(r"^Connecting to (\S+), \d+$"), r"Connecting to \1, <port>"),
    (ADDRESS, r"\1:<port>"),
    (re.compile(r"took \d+ milliseconds"), "took <n> milliseconds"),
    (re.compile(r"\b\d{4}-\d\d-\d\d_\d\d\.\d\d\.\d\d(_\d+)?\.png"), "<time>.png"),
    (re.compile(r"\S*/run/clients/[^/\s]+"), "<client dir>"),
)


def log_message(message):
    """The message with the per-run parts replaced."""
    for pattern, replacement in REWRITES:
        message = pattern.sub(replacement, message)
    return message


def log_key(level, message):
    """`LEVEL: normalised message`, or None when the line is run noise."""
    if message.startswith(IGNORED_PREFIXES):
        return None
    return f"{level}: {log_message(message)}"


# ---- server log -----------------------------------------------------------------------------------------------

# what the server log is gated on: errors, the vanilla message for a read of a chunk that is not generated yet, and the
# pointer a console command that did not parse prints (a scenario must not rely on a command that never ran)
SERVER_FINDING = re.compile(r"Exception|Detected unsafe terrain read|<--\[HERE\]")
# the lines of a player's session that both servers must print alike
SERVER_PLAYER = re.compile(r"joined the game|left the game|UUID of player|logged in with entity id|lost connection|Disconnecting|was kicked")
SERVER_REWRITES = (
    (re.compile(r"/?\d{1,3}(\.\d{1,3}){3}:\d+"), "<addr>"),
    (re.compile(r"entity id \d+"), "entity id <id>"),
    # after a kick the vanilla server logs the kick reason or `Disconnected`, depending on whether its tick or the network
    # thread gets to the connection first; the client's disconnect text is compared on its own
    (re.compile(r"(lost connection): .*"), r"\1: <reason>"),
)


def server_entries(text):
    """(level, message) of every structured line of a server log."""
    return [m.group(2, 3) for m in map(LINE.match, text.splitlines()) if m]


def server_findings(text):
    """`LEVEL: message` of every error line or line naming an exception or an unsafe terrain read."""
    return [f"{lvl}: {msg}" for lvl, msg in server_entries(text) if lvl in ("ERROR", "FATAL") or SERVER_FINDING.search(msg)]


def server_player_lines(text):
    """Normalised `LEVEL: message` of the session lines of a server log, in order."""
    out = []
    for lvl, msg in server_entries(text):
        if SERVER_PLAYER.search(msg):
            for pattern, replacement in SERVER_REWRITES:
                msg = pattern.sub(replacement, msg)
            out.append(f"{lvl}: {msg}")
    return out
