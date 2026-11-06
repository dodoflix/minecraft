#!/usr/bin/env bash
# Compares the world spawn (level.dat Data.spawn) our server selects for each seed of
# crates/mc-game/tests/fixtures/world_spawn/vanilla_spawns.txt with the vanilla server's.
#
#   scripts/world-spawn-check.sh [PATH_TO_mc-server]     (default: target/debug/mc-server)
#
# Output: one `ok` / `FAIL` line per seed; exit 1 when any seed differs.
# Needs: python3, mkfifo, our mc-server binary, MC_ACCEPT_EULA=1 (the server starts in a scratch directory).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need python3 mkfifo
BIN="${1:-$ROOT/target/debug/mc-server}"
[ -x "$BIN" ] || die "$BIN not found (cargo build -p mc-server)"
fail=0
while read -r seed x y z; do
  [[ -z "$seed" || "$seed" == \#* ]] && continue
  d="$(mktemp -d)"
  write_eula "$d"
  printf 'online-mode=false\nserver-port=25792\nview-distance=2\nsimulation-distance=2\nlevel-seed=%s\nenable-rcon=false\nmax-tick-time=-1\n' "$seed" > "$d/server.properties"
  mkfifo "$d/in"
  (cd "$d" && exec 3<>in && "$BIN" nogui <&3 >out.log 2>&1 & echo $! > "$d/pid")
  for _ in $(seq 1200); do grep -q 'Done (' "$d/out.log" 2>/dev/null && break; sleep 0.5; done
  printf 'stop\n' > "$d/in"
  for _ in $(seq 300); do kill -0 "$(cat "$d/pid")" 2>/dev/null || break; sleep 0.2; done
  kill -9 "$(cat "$d/pid")" 2>/dev/null || true
  got="$(python3 "$ROOT/scripts/level-dat-spawn.py" "$d/world/level.dat" | grep -o "\[[-0-9, ]*\]" | head -1 | tr -d '[],' || true)"
  if [[ "$got" == "$x $y $z" ]]; then echo "ok   seed $seed: $got"; else echo "FAIL seed $seed: ours [$got] vanilla [$x $y $z]"; fail=1; fi
  rm -rf "$d"
done < "$ROOT/crates/mc-game/tests/fixtures/world_spawn/vanilla_spawns.txt"
exit $fail
