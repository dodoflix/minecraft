#!/usr/bin/env bash
# Join check: run the real 26.3 client against a server and print a machine-readable JSON verdict on stdout.
#
#   e2e-join-check.sh HOST PORT [--seconds N=15] [--join-timeout N=90] [--username U=E2EBot] [--out DIR]
#                     [--render-distance N=4] [--keep-client]
#
# Flow: launch client -> wait until joined (window title "Multiplayer") / disconnected / crashed / join-timeout
#       -> stay connected --seconds more (aborts early on disconnect) -> screenshot (F2) -> stop client.
# Artifacts in --out (default run/e2e/<timestamp>): verdict.json, client.log, screenshot.png.
# Verdict JSON: ok, joined, stayed_connected, connected_seconds, client_game_time_ticks, chunks_loaded, disconnect_reason, decode_errors[], errors[],
#   crashed, crash_report, screenshot, log, elapsed_seconds, host, port.
# Exit code 0 only if joined, still connected at the end, no disconnect, no decode errors, no crash.
# client_game_time_ticks / chunks_loaded come from scripts/e2e/E2eMain.java (state probe inside the stock client).
# Needs: python3 and the prerequisites of scripts/real-client.sh.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need python3
host="${1:?HOST}"; port="${2:?PORT}"; shift 2
secs=15 jt=90 user=E2EBot out="" rd=4 keep=0
# shellcheck disable=SC2034  # `keep` is read by the EXIT trap below
while (($#)); do
  case "$1" in
    --seconds) secs="$2"; shift 2;;
    --join-timeout) jt="$2"; shift 2;;
    --username) user="$2"; shift 2;;
    --out) out="$2"; shift 2;;
    --render-distance) rd="$2"; shift 2;;
    --keep-client) keep=1; shift;;
    *) echo "unknown option $1" >&2; exit 2;;
  esac
done
[[ -n "$out" ]] || out="$ROOT/run/e2e/$(date +%Y%m%d-%H%M%S)"
mkdir -p "$out"; out="$(cd "$out" && pwd)"
RC="$ROOT/scripts/real-client.sh"
name="e2e-$$"
trap '((keep)) || { "$RC" stop "$name" >/dev/null 2>&1 || true; rm -rf "$ROOT/run/clients/$name"; }' EXIT

t0=$SECONDS
"$RC" start "$name" "$user" "$host:$port" --render-distance "$rd" >&2
st() { "$RC" status "$name"; }
jq_() { python3 -c 'import json,sys;print(json.load(sys.stdin)[sys.argv[1]])' "$1"; }  # prints Python repr: True/False/None

# Phase 1: wait for join (or a terminal state)
join_at="" sj="{}"
while ((SECONDS - t0 < jt)); do
  s="$(st)"
  [[ "$(jq_ joined <<<"$s")" == True ]] && { join_at=$SECONDS; sj="$s"; break; }
  [[ "$(jq_ disconnected <<<"$s")" == True || "$(jq_ crashed <<<"$s")" == True || "$(jq_ running <<<"$s")" == False ]] && break
  sleep 0.5
done

# Phase 2: stay connected
if [[ -n "$join_at" ]]; then
  while ((SECONDS - join_at < secs)); do
    s="$(st)"
    [[ "$(jq_ joined <<<"$s")" == True ]] && sj="$s"
    [[ "$(jq_ disconnected <<<"$s")" == True || "$(jq_ crashed <<<"$s")" == True || "$(jq_ running <<<"$s")" == False ]] && break
    sleep 0.5
  done
  s="$(st)"
  # Screenshot only while still connected, so it shows the world and not the disconnect screen.
  if [[ "$(jq_ joined <<<"$s")" == True ]]; then "$RC" screenshot "$name" "$out/screenshot.png" >/dev/null 2>&1 || true; fi
fi
s="$(st)"
end=$SECONDS
cp "$ROOT/run/clients/$name/client.out" "$out/client.log" 2>/dev/null || true
crash="$(jq_ crash_report <<<"$s")"
[[ "$crash" != None && -f "$crash" ]] && cp "$crash" "$out/crash-report.txt"

S="$s" SJ="$sj" OUT="$out" HOST="$host" PORT="$port" JOIN_AT="$join_at" T0="$t0" END="$end" python3 - <<'PY'
import json, os, sys
s = json.loads(os.environ["S"]); sj = json.loads(os.environ["SJ"]); out = os.environ["OUT"]
join_at = os.environ["JOIN_AT"]
connected = (int(os.environ["END"]) - int(join_at)) if join_at else 0
shot = os.path.join(out, "screenshot.png")
v = {
    "joined": s["joined_ever"],
    "stayed_connected": s["joined"] and not s["disconnected"] and s["running"],
    "connected_seconds": connected,
    "client_game_time_ticks": sj.get("game_time"),  # client level game time at last in-world sample
    "chunks_loaded": sj.get("chunks_loaded"),       # chunks the client had received at that sample
    "disconnect_reason": s["disconnect_reason"],
    "decode_errors": s["decode_errors"],
    "errors": s["errors"],
    "crashed": s["crashed"],
    "crash_report": os.path.join(out, "crash-report.txt") if s["crash_report"] else None,
    "screenshot": shot if os.path.exists(shot) else None,
    "log": os.path.join(out, "client.log"),
    "elapsed_seconds": int(os.environ["END"]) - int(os.environ["T0"]),
    "host": os.environ["HOST"], "port": int(os.environ["PORT"]),
}
v["ok"] = bool(v["joined"] and v["stayed_connected"] and not v["disconnect_reason"]
               and not v["decode_errors"] and not v["crashed"])
json.dump({"ok": v.pop("ok"), **v}, open(os.path.join(out, "verdict.json"), "w"), indent=2)
print(open(os.path.join(out, "verdict.json")).read())
sys.exit(0 if json.load(open(os.path.join(out, "verdict.json")))["ok"] else 1)
PY
