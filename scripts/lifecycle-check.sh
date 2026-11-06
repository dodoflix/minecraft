#!/usr/bin/env bash
# Startup/shutdown differential check: run OUR mc-server from an empty dir and the real vanilla server with the same
# server.properties, compare the normalized startup+shutdown logs, then let the real vanilla server load the world we
# saved (and nothing else touches it).
#
#   scripts/lifecycle-check.sh [--bin PATH_TO_mc-server] [--port N=25590] [--seed S=12345]
#
# Output: diff of the logs (exit 0 iff identical after normalization, vanilla-only worker-thread noise ignored),
# "vanilla loaded our world: ok" on success. Scratch dirs: run/servers/lc-ours, run/servers/lc-vanilla, run/servers/lc-reload.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${MC_SERVER_BIN:-$ROOT/target/gamelifecycle/debug/mc-server}"
PORT=25590 SEED=12345
while (($#)); do
  case "$1" in
    --bin) BIN="$2"; shift 2;;
    --port) PORT="$2"; shift 2;;
    --seed) SEED="$2"; shift 2;;
    *) echo "unknown option $1" >&2; exit 2;;
  esac
done
BIN="$(realpath "$BIN" 2>/dev/null || echo "$BIN")"
[[ -x "$BIN" ]] || { echo "mc-server binary not found: $BIN (build it: CARGO_TARGET_DIR=target/gamelifecycle cargo build -p mc-server)" >&2; exit 2; }

OURS="$ROOT/run/servers/lc-ours"
rm -rf "$OURS"; mkdir -p "$OURS"
cat > "$OURS/server.properties" <<EOF
server-port=$PORT
online-mode=false
enforce-secure-profile=false
white-list=false
spawn-protection=0
view-distance=6
simulation-distance=6
gamemode=creative
difficulty=peaceful
level-seed=$SEED
motd=lifecycle check
EOF
echo "eula=true" > "$OURS/eula.txt"
mkfifo "$OURS/stdin.fifo"
# shellcheck disable=SC2016  # the inner script expands in the child shell
(cd "$OURS" && setsid bash -c '
    exec 3<>stdin.fifo
    echo $$ > server.pid
    exec "$1" nogui <&3 >server.out 2>&1
  ' _ "$BIN" &)
for _ in $(seq 100); do [[ -s "$OURS/server.pid" ]] && break; sleep 0.1; done
pid="$(cat "$OURS/server.pid")"
for _ in $(seq 600); do
  grep -qF 'Done (' "$OURS/logs/latest.log" 2>/dev/null && break
  kill -0 "$pid" 2>/dev/null || { tail -30 "$OURS/server.out" >&2; echo "our server died during startup" >&2; exit 1; }
  sleep 0.5
done
grep -qF 'Done (' "$OURS/logs/latest.log" || { echo "timeout waiting for Done" >&2; kill "$pid"; exit 1; }
printf 'stop\n' > "$OURS/stdin.fifo"
# the `stop` command may not be registered yet in our server: fall back to SIGINT (vanilla's shutdown hook path)
for _ in $(seq 25); do kill -0 "$pid" 2>/dev/null || break; sleep 0.2; done
kill -0 "$pid" 2>/dev/null && kill -INT "$pid"
for _ in $(seq 300); do kill -0 "$pid" 2>/dev/null || break; sleep 0.2; done
kill -0 "$pid" 2>/dev/null && { echo "our server did not stop" >&2; kill -9 "$pid"; exit 1; }

"$ROOT/scripts/vanilla-server.sh" start lc-vanilla --reset --port $((PORT + 1)) --seed "$SEED" level-type=minecraft:normal \
  view-distance=6 simulation-distance=6 gamemode=creative difficulty=peaceful motd="lifecycle check" >/dev/null
"$ROOT/scripts/vanilla-server.sh" stop lc-vanilla >/dev/null

# normalize: drop timestamps, durations, ports; drop lines only vanilla's worker threads print before `Server thread`
norm() {
  sed -E 's/^\[[0-9:]+\] //; s/Done \([0-9.]+s\)/Done (X)/; s/Time elapsed: [0-9]+ ms/Time elapsed: X ms/; s/(on \*:)[0-9]+/\1PORT/' "$1" \
    | grep -vE '^\[(ServerMain|Worker-Main-[0-9]+)/' | grep -vE 'Can.t keep up'
}
norm "$OURS/logs/latest.log" > "$OURS/norm.log"
norm "$ROOT/run/servers/lc-vanilla/logs/latest.log" > "$ROOT/run/servers/lc-vanilla/norm.log"
echo "--- diff (ours vs vanilla) ---"
status=0
diff -u "$ROOT/run/servers/lc-vanilla/norm.log" "$OURS/norm.log" || status=1

# vanilla must load the world we saved
RELOAD="$ROOT/run/servers/lc-reload"
rm -rf "$RELOAD"; mkdir -p "$RELOAD"
cp -r "$OURS/world" "$RELOAD/world"
# no --reset: vanilla-server.sh keeps the existing world dir and only rewrites server.properties
"$ROOT/scripts/vanilla-server.sh" start lc-reload --port $((PORT + 2)) --seed "$SEED" >/dev/null
if grep -qE 'ERROR|WARN.*(world|chunk|level)' "$ROOT/run/servers/lc-reload/logs/latest.log"; then
  echo "vanilla complained about our world:"; grep -E 'ERROR|WARN' "$ROOT/run/servers/lc-reload/logs/latest.log"; status=1
else
  echo "vanilla loaded our world: ok"
fi
"$ROOT/scripts/vanilla-server.sh" stop lc-reload >/dev/null
exit $status
