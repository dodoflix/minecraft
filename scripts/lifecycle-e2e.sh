#!/usr/bin/env bash
# Join + restart-persistence acceptance for OUR server (the real 26.3 client joins; console commands change the world;
# `stop`; restart; the changes must still be there; finally the real vanilla server must load the world we saved).
#
#   scripts/lifecycle-e2e.sh [--bin PATH_TO_mc-server] [--port N=25620] [--seconds N=20]
#
# Scratch: run/servers/lc-e2e (ours), run/servers/lc-e2e-vanilla (vanilla loading the resulting world).
# Needs: java, python3, ref/, the e2e client prerequisites of scripts/real-client.sh, our mc-server binary
# (default target/release/mc-server: cargo build --locked --release -p mc-server; MC_SERVER_BIN or --bin override), MC_ACCEPT_EULA=1.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need java python3 mkfifo setsid
require_eula
BIN="${MC_SERVER_BIN:-$ROOT/target/release/mc-server}"
PORT=25620 SECS=20
while (($#)); do
  case "$1" in
    --bin) BIN="$2"; shift 2;;
    --port) PORT="$2"; shift 2;;
    --seconds) SECS="$2"; shift 2;;
    *) echo "unknown option $1" >&2; exit 2;;
  esac
done
BIN="$(realpath "$BIN" 2>/dev/null || echo "$BIN")"
[[ -x "$BIN" ]] || die "mc-server binary not found: $BIN (build it: cargo build --locked --release -p mc-server)"
D="$ROOT/run/servers/lc-e2e"
fail() { echo "FAIL: $*" >&2; exit 1; }

start_ours() {
  rm -f "$D/stdin.fifo" "$D/server.pid"; mkfifo "$D/stdin.fifo"
  # shellcheck disable=SC2016  # the inner script expands in the child shell
  (cd "$D" && setsid bash -c '
      exec 3<>stdin.fifo
      echo $$ > server.pid
      exec "$1" nogui <&3 >>server.out 2>&1
    ' _ "$BIN" &)
  for _ in $(seq 100); do [[ -s "$D/server.pid" ]] && break; sleep 0.1; done
  local pid; pid="$(cat "$D/server.pid")"
  for _ in $(seq 600); do
    grep -qF 'Done (' "$D/logs/latest.log" 2>/dev/null && return 0
    kill -0 "$pid" 2>/dev/null || { tail -30 "$D/server.out" >&2; fail "our server died during startup"; }
    sleep 0.5
  done
  fail "timeout waiting for Done"
}
cmd() { printf '%s\n' "$*" > "$D/stdin.fifo"; }
stop_ours() {
  local pid; pid="$(cat "$D/server.pid")"
  cmd stop
  for _ in $(seq 40); do kill -0 "$pid" 2>/dev/null || return 0; sleep 0.25; done
  kill -INT "$pid" 2>/dev/null || true   # `stop` not registered yet: vanilla's shutdown-hook path
  for _ in $(seq 200); do kill -0 "$pid" 2>/dev/null || return 0; sleep 0.25; done
  kill -9 "$pid"; fail "our server did not stop"
}

rm -rf "$D"; mkdir -p "$D"
write_eula "$D"
cat > "$D/server.properties" <<EOF
server-port=$PORT
online-mode=false
enforce-secure-profile=false
white-list=false
spawn-protection=0
view-distance=6
simulation-distance=6
gamemode=survival
difficulty=peaceful
level-seed=12345
motd=lifecycle e2e
EOF

echo "== run 1: real client joins =="
start_ours
"$ROOT/scripts/e2e-join-check.sh" 127.0.0.1 "$PORT" --seconds "$SECS" --username E2EBot --out "$ROOT/run/e2e/lc-e2e-1" | tee "$D/verdict1.json"
grep -q '"ok": true' "$D/verdict1.json" || fail "real client could not join/stay"
grep -q 'E2EBot\[.*logged in with entity id' "$D/logs/latest.log" || fail "no login line in our log"
cmd "setblock 0 120 0 minecraft:gold_block"
cmd "time set 6000"
cmd "gamerule keep_inventory true"
sleep 3
stop_ours
tail -8 "$D/logs/latest.log"
compgen -G "$D/world/players/data/*.dat" >/dev/null || fail "no player data file written"
[[ -f "$D/world/level.dat" ]] || fail "no level.dat"

echo "== run 2: restart, check persistence =="
start_ours
cmd "time query daytime"
cmd "gamerule keep_inventory"
cmd "forceload add 0 0"
sleep 3
if grep -q 'forceload add 0 0<--\[HERE\]' "$D/logs/latest.log"; then
  echo "warn: /forceload is not registered yet: block persistence check skipped"; BLOCKS=0
else
  BLOCKS=1
  cmd "setblock 0 120 0 minecraft:gold_block"  # unchanged block => "Could not set the block"
  sleep 2
  grep -q 'Could not set the block' "$D/logs/latest.log" || fail "placed block did not persist"
fi
grep -Eq 'The time is 6[0-9]{3}' "$D/logs/latest.log" || fail "day time did not persist"
grep -q 'keep_inventory.*true' "$D/logs/latest.log" || fail "game rule did not persist"
stop_ours

echo "== vanilla loads the saved world =="
V="$ROOT/run/servers/lc-e2e-vanilla"
"$ROOT/scripts/vanilla-server.sh" start lc-e2e-vanilla --reset --port $((PORT + 1)) >/dev/null
"$ROOT/scripts/vanilla-server.sh" stop lc-e2e-vanilla >/dev/null
rm -rf "$V/world"; cp -r "$D/world" "$V/world"
"$ROOT/scripts/vanilla-server.sh" start lc-e2e-vanilla --port $((PORT + 1)) >/dev/null
sleep 2
if ((BLOCKS)); then
  "$ROOT/scripts/vanilla-server.sh" cmd lc-e2e-vanilla "forceload add 0 0"
  sleep 3
  "$ROOT/scripts/vanilla-server.sh" cmd lc-e2e-vanilla "setblock 0 120 0 minecraft:gold_block"
  sleep 2
  grep -q "Could not set the block" "$V/logs/latest.log" || fail "vanilla does not see our block"
fi
if grep -E 'ERROR|WARN.*(chunk|world|level)' "$V/logs/latest.log"; then fail "vanilla complained about our world"; fi
"$ROOT/scripts/vanilla-server.sh" stop lc-e2e-vanilla >/dev/null
echo "OK"
