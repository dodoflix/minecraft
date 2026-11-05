#!/usr/bin/env bash
# Compares the world spawn (level.dat Data.spawn) our server selects for each seed of
# crates/mc-game/tests/fixtures/world_spawn/vanilla_spawns.txt with the vanilla server's.
#   world-spawn-check.sh [path/to/mc-server]      (default: target/debug/mc-server)
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${1:-$ROOT/target/debug/mc-server}"
fail=0
while read -r seed x y z; do
  [[ -z "$seed" || "$seed" == \#* ]] && continue
  d="$(mktemp -d)"
  echo "eula=true" > "$d/eula.txt"
  printf 'online-mode=false\nserver-port=25792\nview-distance=2\nsimulation-distance=2\nlevel-seed=%s\nenable-rcon=false\nmax-tick-time=-1\n' "$seed" > "$d/server.properties"
  mkfifo "$d/in"
  (cd "$d" && exec 3<>in && "$BIN" nogui <&3 >out.log 2>&1 & echo $! > "$d/pid")
  for _ in $(seq 1200); do grep -q 'Done (' "$d/out.log" 2>/dev/null && break; sleep 0.5; done
  printf 'stop\n' > "$d/in"
  for _ in $(seq 300); do kill -0 "$(cat "$d/pid")" 2>/dev/null || break; sleep 0.2; done
  kill -9 "$(cat "$d/pid")" 2>/dev/null
  got="$(python3 "$ROOT/scripts/level-dat-spawn.py" "$d/world/level.dat" | grep -o "\[[-0-9, ]*\]" | head -1 | tr -d '[],')"
  if [[ "$got" == "$x $y $z" ]]; then echo "ok   seed $seed: $got"; else echo "FAIL seed $seed: ours [$got] vanilla [$x $y $z]"; fail=1; fi
  rm -rf "$d"
done < "$ROOT/crates/mc-game/tests/fixtures/world_spawn/vanilla_spawns.txt"
exit $fail
