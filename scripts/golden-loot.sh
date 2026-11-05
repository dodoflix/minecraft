#!/usr/bin/env bash
# Regenerates crates/mc-game/tests/fixtures/loot_golden.txt with the REAL vanilla LootTable.getRandomItems (block tables x tools x
# seeds, chest tables x seeds) through the in-process vanilla oracle (tests/oracle/LootOracle.java).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
"$ROOT/scripts/entity-oracle.sh" "$ROOT/crates/mc-game/tests/oracle/LootOracle.java" > "$ROOT/crates/mc-game/tests/fixtures/loot_golden.txt"
