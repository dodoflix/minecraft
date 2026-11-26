#!/usr/bin/env python3
"""Print a JSON verdict for a real-client run directory (run/clients/NAME), parsed from client.out.

Usage: scripts/mc-client-status.py RUN_DIR

Fields: running, connecting, in_level, joined, joined_ever, screen, screen_title, game_time, chunks_loaded, packets_in,
packets_out, disconnected, disconnect_reason, decode_errors[], errors[], ctl_replies, ctl_failed[], egress[], crashed,
crash_report, screenshots[], log.
`in_level` = client has a level + player right now; `joined` = in_level and no screen is open, i.e. the world is visible
(the level exists already while the loading screen is up); `joined_ever` = `joined` held at any "[E2E]" sample.
game_time = client level game time (ticks); chunks_loaded = chunks received; packets_in/out = packets seen by the packet tap
(every protocol phase). All come from the "[E2E]" lines printed by E2eProbe. `ctl_replies` counts the control channel replies
("[E2E-CTL]"), `ctl_failed` lists the ones with ok=false. `decode_errors` also holds the DEBUG lines of game/latest.log where
the client reports a packet it could not decode or send. `egress[]` = names the client tried to resolve (E2eResolver refused
them) and non-loopback sockets (net-watch.py); any entry means the run was not isolated.
"""
import glob, json, os, re, sys

if sys.argv[1:2] in (["-h"], ["--help"]):
    print(__doc__)
    sys.exit(0)

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

# Last state line printed by scripts/e2e/E2eProbe.java (the stock client logs neither join nor login-phase kicks).
e2e, joined_ever, egress, ctl_replies, ctl_failed = {}, False, [], 0, []
for ln in lines:
    # the client redirects System.out through log4j, so the line has a "[time] [thread/INFO]: [STDOUT]: " prefix
    if "[E2E] {" in ln:
        try:
            e2e = json.loads(ln.split("[E2E] ", 1)[1])
        except ValueError:
            continue
        joined_ever |= bool(e2e.get("in_level")) and e2e.get("screen") is None
    elif "[E2E-CTL] {" in ln:
        try:
            reply = json.loads(ln.split("[E2E-CTL] ", 1)[1])
        except ValueError:
            continue
        ctl_replies += 1
        if not reply.get("ok"):
            ctl_failed.append(reply)
    elif "[E2E-NET] {" in ln:
        try:
            host = json.loads(ln.split("[E2E-NET] ", 1)[1])["blocked"]
        except (ValueError, KeyError):
            host = ln
        if f"resolve {host}" not in egress:
            egress.append(f"resolve {host}")
in_level = bool(e2e.get("in_level"))
joined = in_level and e2e.get("screen") is None
net_log = os.path.join(d, "net-egress.log")
if os.path.exists(net_log):
    egress += [f"connect {ln.strip()}" for ln in open(net_log) if ln.strip()]

# Known harmless noise of an isolated, offline, headless client: exactly these messages (the first line of the log
# entry, then the line that follows it), nothing else is hidden.
SERVICES = r"(PLAYER/getAttributes|PLAYER/getBlocklist|PLAYER/getCertificates|SESSION/getProfileById|AUTHENTICATION/getPublicKeys)"
OFFLINE = r"com\.mojang\.authlib\.exceptions\.MinecraftClientException: Services are unavailable"
IGNORE = re.compile(
    r"^\[[\d:]+\] \[[^\]]+/(WARN|ERROR)\]: (?:"
    r"Failed to connect \| com\.mojang\.authlib\.exceptions\.MinecraftClientException: "
    r"(Failed to read from http://127\.0\.0\.1:9/minecraft/client due to Connection refused \(connect failed\)|Services are unavailable)"
    rf"|Services are unavailable, unable to fetch uri for {SERVICES}"
    r"|Failed to fetch user properties \| com\.mojang\.authlib\.exceptions\.AuthenticationUnavailableException"
    rf"|Couldn't look up profile properties for [0-9a-f-]+ \| {OFFLINE}"
    r"|(?:Failed to fetch Realms feature flags|Couldn't connect to realms) \| com\.mojang\.realmsclient\.exception\.RealmsServiceException: "
    r"Realms service error \(\d+\) with message 'Could not connect to Realms: (localhost:8080|Connection refused \(connect failed\))'"
    r"|Ignoring chat session from \w+ due to missing Services public key"
    r"|Error while loading the narrator \| com\.mojang\.text2speech\.Narrator\$InitializeException: Failed to load library flite"
    r"|Requested post effect does not exist: minecraft:end_of_frame"
    r")$"
)
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
    if re.search(r"/(ERROR|WARN)\]", ln):
        nxt = lines[i + 1] if i + 1 < len(lines) and not lines[i + 1].startswith("[") else ""
        entry = (ln + (" | " + nxt if nxt else "")).strip()
        if not IGNORE.search(entry):
            (decode if DECODE.search(entry) else errors).append(entry)

# The client reports a packet it failed to decode or send only at DEBUG; game/latest.log (scripts/client-log4j2.xml) keeps it.
DEBUG_DECODE = re.compile(r"/DEBUG\] \[[^\]]*\]: (Skipping packet due to errors|Failed to sent packet)")
latest = os.path.join(d, "game/latest.log")
if os.path.exists(latest):
    dl = open(latest, errors="replace").read().splitlines()
    for i, ln in enumerate(dl):
        if DEBUG_DECODE.search(ln):
            cause = next((x.strip() for x in dl[i + 1:i + 4] if not x.startswith("[")), "")
            decode.append((ln + (" | " + cause if cause else "")).strip())
if disc and PROTO_REASON.search(disc):
    decode.insert(0, "disconnect screen: " + disc)
crash = glob.glob(os.path.join(d, "game/crash-reports/*.txt"))
print(json.dumps({
    "running": pid is not None,
    "connecting": any("Connecting to " in l for l in lines),
    "in_level": in_level,
    "joined": joined,
    "joined_ever": joined_ever,
    "screen": e2e.get("screen"),
    "screen_title": e2e.get("screen_title"),
    "game_time": e2e.get("game_time"),
    "chunks_loaded": e2e.get("chunks"),
    "packets_in": e2e.get("packets_in"),
    "packets_out": e2e.get("packets_out"),
    "disconnected": disc is not None,
    "disconnect_reason": disc,
    "decode_errors": decode,
    "errors": [e for e in errors if "disconnected with reason" not in e],
    "ctl_replies": ctl_replies,
    "ctl_failed": ctl_failed,
    "egress": egress,
    "crashed": "Game crashed!" in text or bool(crash),
    "crash_report": crash[0] if crash else None,
    "screenshots": sorted(glob.glob(os.path.join(d, "game/screenshots/*.png"))),
    "log": log_path,
}, indent=2))
