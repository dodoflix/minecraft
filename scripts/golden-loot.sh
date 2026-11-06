#!/usr/bin/env bash
# Regenerates the loot golden with the REAL vanilla LootTable.getRandomItems (block tables x tools x seeds, chest tables x seeds)
# through the in-process vanilla oracle (crates/mc-game/tests/oracle/LootOracle.java).
# Usage:
#   scripts/golden-loot.sh
#
# Output: crates/mc-game/tests/fixtures/loot_golden.txt.gz (replaced only when the oracle succeeds)
# Needs: java, ref/ with decompiled sources (scripts/setup-ref.sh --decompile); MC_ACCEPT_EULA=1 (the oracle starts a vanilla level).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need java
T="$ROOT/crates/mc-game/tests"
to_file "$T/fixtures/loot_golden.txt.gz" "$ROOT/scripts/entity-oracle.sh" "$T/oracle/LootOracle.java"
