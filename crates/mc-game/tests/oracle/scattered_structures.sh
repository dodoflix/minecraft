#!/usr/bin/env bash
# Regenerates the fixtures of crates/mc-game/tests/golden/scattered_structures_golden.rs with ScatteredStructuresOracle.java.
#
# Usage: crates/mc-game/tests/oracle/scattered_structures.sh [structure ...]   (default: all)
# Output: crates/mc-game/tests/fixtures/scattered/<structure>.txt, or <structure>.txt.gz for shipwreck, shipwreck_beached,
#         nether_fossil, ocean_ruin_cold and ocean_ruin_warm (each replaced only when its run succeeds)
# Needs: java, ref/ (scripts/setup-ref.sh).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../../.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need java
declare -A COUNT=(
  [desert_pyramid]=120 [jungle_pyramid]=120 [swamp_hut]=60 [igloo]=90 [buried_treasure]=45
  [shipwreck]=300 [shipwreck_beached]=90 [ocean_ruin_cold]=150 [ocean_ruin_warm]=150
  [ruined_portal]=60 [ruined_portal_desert]=45 [ruined_portal_jungle]=45 [ruined_portal_swamp]=45 [ruined_portal_mountain]=45
  [ruined_portal_ocean]=45 [ruined_portal_nether]=60 [nether_fossil]=300
)
declare -A DIM=([ruined_portal_nether]=nether [nether_fossil]=nether)
ORACLE="$ROOT/crates/mc-game/tests/oracle/ScatteredStructuresOracle.java"
F="$ROOT/crates/mc-game/tests/fixtures/scattered"
ids=("$@"); [ ${#ids[@]} -gt 0 ] || ids=("${!COUNT[@]}")
for id in "${ids[@]}"; do
  [ -n "${COUNT[$id]:-}" ] || die "unknown structure '$id' (known: ${!COUNT[*]})"
  case "$id" in
    shipwreck | shipwreck_beached | nether_fossil | ocean_ruin_cold | ocean_ruin_warm) out="$F/$id.txt.gz" ;;
    *) out="$F/$id.txt" ;;
  esac
  "$ROOT/scripts/vanilla-java.sh" -o "$out" "$ORACLE" "$out" "$id" "${COUNT[$id]}" "${DIM[$id]:-overworld}"
done
