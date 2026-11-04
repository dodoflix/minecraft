#!/usr/bin/env python3
"""Print a JSON verdict for a real-client run directory (run/clients/NAME), parsed from client.out.

Fields: running, connecting, joined, joined_ever, screen, screen_title, game_time, chunks_loaded, disconnected,
disconnect_reason, decode_errors[], errors[], crashed, crash_report, screenshots[], log.
`joined` = client has a level + player right now; game_time = client level game time (ticks); chunks_loaded =
chunks received. Both come from the last "[E2E]" line printed by E2eMain (latched `joined_ever`).
"""
import glob, json, os, re, sys

d = os.path.abspath(sys.argv[1])
log_path = os.path.join(d, "client.out")
text = open(log_path, errors="replace").read() if os.path.exists(log_path) else ""
lines = text.splitlines()

pid = None
try:
    pid = int(open(os.path.join(d, "pid")).read())
    os.kill(pid, 0)
except (OSError, ValueError):
    pid = None

# Last state line printed by scripts/e2e/E2eMain.java (the stock client logs neither join nor login-phase kicks).
e2e = {}
for ln in lines:
    # the client redirects System.out through log4j, so the line has a "[time] [thread/INFO]: [STDOUT]: " prefix
    if "[E2E] {" in ln:
        try:
            e2e = json.loads(ln.split("[E2E] ", 1)[1])
        except ValueError:
            pass
joined = bool(e2e.get("in_level"))

# Known harmless noise on an offline, headless client.
IGNORE = re.compile(r"Failed to fetch user properties|Error while loading the narrator|flite|Realms|"
                    r"Requested post effect|Resizing Chunk|Could not authorize|AuthenticationUnavailable|"
                    r"gamescope|xwayland|SignedJWT|Failed to fetch.*feature flags|sessionserver|Couldn't look up profile|"
                    r"127\.0\.0\.1:9\b")
DECODE = re.compile(r"Error decoding packet|Failed to decode|DecoderException|Network Protocol Error|"
                    r"Skipping packet due to errors|Packet .* was larger than I expected|"
                    r"Received invalid|IndexOutOfBounds|Badly encoded|Bad packet", re.I)
disc = e2e.get("disconnect_reason")
# The disconnect screen text is the only place a decode failure shows up at default log level.
PROTO_REASON = re.compile(r"Internal Exception|DecoderException|Failed to decode|Network Protocol Error|"
                          r"Invalid packet|invalid_packet|EncoderException|Packet handling error", re.I)
errors, decode = [], []
for i, ln in enumerate(lines):
    m = re.search(r"disconnected with reason: (.*)", ln) or re.search(r"Disconnected[^:]*: (.*)", ln)
    if m and ("/INFO]" in ln or "/WARN]" in ln or "/ERROR]" in ln):
        disc = m.group(1)
    if re.search(r"/(ERROR|WARN)\]", ln) and not IGNORE.search(ln):
        nxt = lines[i + 1] if i + 1 < len(lines) and not lines[i + 1].startswith("[") else ""
        entry = (ln + (" | " + nxt if nxt else "")).strip()
        (decode if DECODE.search(entry) else errors).append(entry)

if disc and PROTO_REASON.search(disc):
    decode.insert(0, "disconnect screen: " + disc)
crash = glob.glob(os.path.join(d, "game/crash-reports/*.txt"))
print(json.dumps({
    "running": pid is not None,
    "connecting": any("Connecting to " in l for l in lines),
    "joined": joined,
    "joined_ever": bool(e2e.get("joined_ever")),
    "screen": e2e.get("screen"),
    "screen_title": e2e.get("screen_title"),
    "game_time": e2e.get("game_time"),
    "chunks_loaded": e2e.get("chunks"),
    "disconnected": disc is not None,
    "disconnect_reason": disc,
    "decode_errors": decode,
    "errors": [e for e in errors if "disconnected with reason" not in e],
    "crashed": "Game crashed!" in text or bool(crash),
    "crash_report": crash[0] if crash else None,
    "screenshots": sorted(glob.glob(os.path.join(d, "game/screenshots/*.png"))),
    "log": log_path,
}, indent=2))
