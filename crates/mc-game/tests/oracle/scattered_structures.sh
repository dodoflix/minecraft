#!/usr/bin/env bash
# Regenerates crates/mc-game/tests/fixtures/scattered/*.txt.gz (fixtures of tests/scattered_structures_golden.rs).
# Usage (repo root): crates/mc-game/tests/oracle/scattered_structures.sh [structure ...]   (default: all)
set -euo pipefail
cd "$(dirname "$0")/../../../.."
declare -A COUNT=(
  [desert_pyramid]=120 [jungle_pyramid]=120 [swamp_hut]=60 [igloo]=90 [buried_treasure]=45
  [shipwreck]=300 [shipwreck_beached]=90 [ocean_ruin_cold]=150 [ocean_ruin_warm]=150
  [ruined_portal]=60 [ruined_portal_desert]=45 [ruined_portal_jungle]=45 [ruined_portal_swamp]=45 [ruined_portal_mountain]=45
  [ruined_portal_ocean]=45 [ruined_portal_nether]=60 [nether_fossil]=300
)
declare -A DIM=([ruined_portal_nether]=nether [nether_fossil]=nether)
ids=("$@"); [ ${#ids[@]} -gt 0 ] || ids=("${!COUNT[@]}")
for id in "${ids[@]}"; do
  out="$(mktemp)"
  scripts/vanilla-java.sh crates/mc-game/tests/oracle/ScatteredStructuresOracle.java "$out" "$id" "${COUNT[$id]}" "${DIM[$id]:-overworld}"
  gzip -9n -c "$out" > "crates/mc-game/tests/fixtures/scattered/$id.txt.gz"
  rm -f "$out"
done
