#!/usr/bin/env bash
# Find interesting worldgen locations with the REAL vanilla server (`/locate structure`) for the given seeds and print
# `seed dim structure x z` lines (x z = block coordinates, y is ~). Used to (re)generate scripts/worldgen-diff-areas.txt:
#
#   Usage: scripts/worldgen-locate.sh SEED... >> scripts/worldgen-diff-areas.txt
#
# Output: `seed dim structure x z` lines on stdout (appended to scripts/worldgen-diff-areas.txt by the redirect above, never truncating it).
# Env: LOCATE_PORT (default 25598), MC_ACCEPT_EULA=1 (starts the vanilla server). Takes a few minutes per seed (every search generates
# structure starts).
# Needs: java, ref/ (scripts/setup-ref.sh).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need java
VS="$ROOT/scripts/vanilla-server.sh"
PORT="${LOCATE_PORT:-25598}"
OVERWORLD='#minecraft:village minecraft:stronghold minecraft:mansion minecraft:ancient_city minecraft:trial_chambers minecraft:pillager_outpost
  minecraft:monument #minecraft:shipwreck #minecraft:ruined_portal #minecraft:mineshaft minecraft:desert_pyramid minecraft:jungle_pyramid
  minecraft:swamp_hut minecraft:igloo #minecraft:abandoned_camp minecraft:buried_treasure #minecraft:ocean_ruin'
NETHER='minecraft:fortress minecraft:bastion_remnant minecraft:nether_fossil minecraft:ruined_portal_nether'
END='minecraft:end_city'
for seed in "$@"; do
  name="locate-$seed"
  "$VS" start "$name" --port "$PORT" --seed "$seed" --reset --view-distance 3 -- generate-structures=true max-players=0 \
    pause-when-empty-seconds=-1 </dev/null >/dev/null 2>&1
  LOG="$("$VS" log "$name")"
  n=0
  ask() {   # ask DIM STRUCT
    n=$((n + 1))
    local before; before="$(wc -l < "$LOG")"
    "$VS" cmd "$name" "execute in minecraft:$1 positioned 0 64 0 run locate structure $2"
    "$VS" cmd "$name" "say LOCATE-DONE-$n"
    "$VS" wait "$name" "\[Server\] LOCATE-DONE-$n" 1800 >/dev/null
    # "The nearest minecraft:igloo is at [x, ~, z]" or, for tag searches, "The nearest #minecraft:village (minecraft:village_plains) is at [x, ~, z]"
    tail -n +"$((before + 1))" "$LOG" | sed -nE 's/.*The nearest (#?[^ ]+)( \(([^)]+)\))? is at \[(-?[0-9]+), (~|-?[0-9]+), (-?[0-9]+)\].*/\1|\3|\4|\6/p' | head -1 |
      while IFS='|' read -r tagorid id x z; do echo "$seed $1 ${id:-$tagorid} $x $z"; done
  }
  for s in $OVERWORLD; do ask overworld "$s"; done
  for s in $NETHER; do ask the_nether "$s"; done
  for s in $END; do ask the_end "$s"; done
  "$VS" stop "$name" >/dev/null </dev/null 2>&1
done
