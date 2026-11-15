#!/usr/bin/env bash
# Regenerates the synthetic loot fixture: every case of crates/mc-game/tests/oracle/scenarios/loot_synthetic/*.txt is decoded,
# validated and evaluated by the REAL vanilla loot codecs/classes (crates/mc-game/tests/oracle/LootSyntheticOracle.java) on the
# in-process vanilla level of scripts/entity-oracle.sh.
# Usage:
#   scripts/loot-synthetic-oracle.sh
#
# Output: crates/mc-game/tests/fixtures/loot_synthetic.txt.gz (replaced only when the oracle succeeds)
# Needs: java, ref/ with decompiled sources (scripts/setup-ref.sh --decompile); MC_ACCEPT_EULA=1.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need java
T="$ROOT/crates/mc-game/tests"
to_file "$T/fixtures/loot_synthetic.txt.gz" "$ROOT/scripts/entity-oracle.sh" "$T/oracle/LootSyntheticOracle.java" "$T"/oracle/scenarios/loot_synthetic/*.txt
