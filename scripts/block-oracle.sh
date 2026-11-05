#!/usr/bin/env bash
# Regenerate block-oracle fixtures from the REAL vanilla level (wrapper of scripts/entity-oracle.sh):
#   scripts/block-oracle.sh blk_redstone_lever_lamp [more names]   tests/oracle/scenarios/NAME.txt -> tests/fixtures/entity_oracle/NAME.txt
# The Rust replay is tests/common/block_scenario.rs (`common::block_scenario::check("NAME")`); see the usage section of STATUS.md.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
T="$ROOT/crates/mc-game/tests"
[ $# -ge 1 ] || { sed -n 2,4p "$0" >&2; exit 2; }
for n in "$@"; do
  n="${n%.txt}"
  echo "== $n" >&2
  "$ROOT/scripts/entity-oracle.sh" "$T/oracle/scenarios/$n.txt" -o "$T/fixtures/entity_oracle/$n.txt"
done
