#!/usr/bin/env bash
# Run the real vanilla 26.3 server (ref/server.jar) as an oracle, in a scratch dir under run/servers/NAME.
#
#   vanilla-server.sh start NAME [opts] [-- key=value ...]   start in background, block until "Done (" (or fail)
#   vanilla-server.sh cmd NAME "say hi"                      send a console command
#   vanilla-server.sh wait NAME REGEX [TIMEOUT_S=30]         wait for a (extended) regex in the server log
#   vanilla-server.sh log NAME                               print latest.log path
#   vanilla-server.sh stop NAME                              clean "stop" (kills after 60 s)
#   vanilla-server.sh status NAME                            "running pid=.. port=.." or "stopped"; exit 0/1
#
# start opts: --port N (25565)  --seed S  --level-type T (minecraft:normal|minecraft:flat|minecraft:large_biomes|...)
#             --view-distance N (6)  --simulation-distance N (6)  --gamemode M (creative)  --difficulty D (peaceful)
#             --reset (wipe the whole scratch dir first)  --xmx 2G
# Trailing `key=value` args override/add any server.properties entry.
# Defaults: online-mode=false, enforce-secure-profile=false, white-list=false, eula accepted, spawn-protection=0.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
die() { echo "vanilla-server: $*" >&2; exit 1; }
dir_of() { echo "$ROOT/run/servers/$1"; }
pid_of() { cat "$(dir_of "$1")/server.pid" 2>/dev/null || true; }
alive() { local p; p="$(pid_of "$1")"; [[ -n "$p" ]] && kill -0 "$p" 2>/dev/null; }

cmd="${1:-}"; shift || true
name="${1:-}"; shift || true
[[ -n "$cmd" && -n "$name" ]] || die "usage: $0 start|cmd|wait|log|stop|status NAME ..."
D="$(dir_of "$name")"

case "$cmd" in
start)
  port=25565 seed="" level_type="minecraft:normal" vd=6 sd=6 gm=creative diff=peaceful reset=0 xmx=2G
  declare -A extra=()
  while (($#)); do
    case "$1" in
      --port) port="$2"; shift 2;;
      --seed) seed="$2"; shift 2;;
      --level-type) level_type="$2"; shift 2;;
      --view-distance) vd="$2"; shift 2;;
      --simulation-distance) sd="$2"; shift 2;;
      --gamemode) gm="$2"; shift 2;;
      --difficulty) diff="$2"; shift 2;;
      --xmx) xmx="$2"; shift 2;;
      --reset) reset=1; shift;;
      --) shift;;
      *=*) extra["${1%%=*}"]="${1#*=}"; shift;;
      *) die "unknown option $1";;
    esac
  done
  ! alive "$name" || die "$name already running (pid $(pid_of "$name"))"
  ((reset)) && rm -rf "$D"
  mkdir -p "$D"
  echo "eula=true" > "$D/eula.txt"
  {
    echo "server-port=$port"
    echo "online-mode=false"
    echo "enforce-secure-profile=false"
    echo "white-list=false"
    echo "spawn-protection=0"
    echo "view-distance=$vd"
    echo "simulation-distance=$sd"
    echo "gamemode=$gm"
    echo "difficulty=$diff"
    echo "level-type=$level_type"
    echo "level-seed=$seed"
    echo "motd=vanilla oracle $name"
    for k in "${!extra[@]}"; do echo "$k=${extra[$k]}"; done
  } > "$D/server.properties.new"
  # later keys win in java.util.Properties, so appended extras override the defaults above
  mv "$D/server.properties.new" "$D/server.properties"
  rm -f "$D/stdin.fifo" "$D/logs/latest.log"; mkfifo "$D/stdin.fifo"
  # The child opens the fifo read-write so stdin never sees EOF between writers.
  (cd "$D" && setsid bash -c '
      exec 3<>stdin.fifo
      echo $$ > server.pid
      exec java -Xms512M -Xmx'"$xmx"' -jar "$1" nogui <&3 >server.out 2>&1
    ' _ "$ROOT/ref/server.jar" &) 
  for _ in $(seq 100); do [[ -s "$D/server.pid" ]] && break; sleep 0.1; done
  # readiness: "Done (1.2s)! For help, type "help""; also fail fast if the process dies
  for _ in $(seq 240); do
    if grep -qF 'Done (' "$D/logs/latest.log" 2>/dev/null; then echo "ready pid=$(pid_of "$name") port=$port dir=$D"; exit 0; fi
    alive "$name" || { tail -30 "$D/server.out" >&2; die "server exited during startup"; }
    sleep 0.5
  done
  tail -30 "$D/server.out" >&2; die "timeout waiting for Done"
  ;;
cmd)
  alive "$name" || die "$name not running"
  # fifo is open read-write by java, so this never blocks
  printf '%s\n' "$*" > "$D/stdin.fifo"
  ;;
wait)
  re="${1:?regex}"; t="${2:-30}"
  for _ in $(seq $((t * 5))); do
    grep -qE -- "$re" "$D/logs/latest.log" 2>/dev/null && { grep -E -- "$re" "$D/logs/latest.log" | tail -1; exit 0; }
    sleep 0.2
  done
  die "timeout waiting for /$re/"
  ;;
log) echo "$D/logs/latest.log";;
status)
  if alive "$name"; then echo "running pid=$(pid_of "$name") port=$(grep -m1 '^server-port=' "$D/server.properties" | cut -d= -f2)"; else echo stopped; exit 1; fi
  ;;
stop)
  alive "$name" || { echo "already stopped"; rm -f "$D/stdin.fifo" "$D/server.pid"; exit 0; }
  p="$(pid_of "$name")"
  printf 'stop\n' > "$D/stdin.fifo"
  for _ in $(seq 600); do kill -0 "$p" 2>/dev/null || break; sleep 0.1; done
  if kill -0 "$p" 2>/dev/null; then echo "stop timed out, killing" >&2; kill -9 "$p"; fi
  rm -f "$D/stdin.fifo" "$D/server.pid"
  echo stopped
  ;;
*) die "unknown command $cmd";;
esac
