#!/usr/bin/env bash
# Regenerates the variants golden with the REAL vanilla mob variant registries (DIRECT/NETWORK codec digests of every entry, Registry.getRandom, PriorityProvider.pick over biome x moon phase x structure contexts)
# through the in-process vanilla oracle (crates/mc-game/tests/oracle/VariantsOracle.java).
# Usage:
#   scripts/golden-variants.sh
#
# Output: crates/mc-game/tests/fixtures/variants_golden.txt (replaced only when the oracle succeeds)
# Needs: java, ref/ with decompiled sources (scripts/setup-ref.sh --decompile); MC_ACCEPT_EULA=1 (the oracle starts a vanilla level).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need java
T="$ROOT/crates/mc-game/tests"
to_file "$T/fixtures/variants_golden.txt" "$ROOT/scripts/entity-oracle.sh" "$T/oracle/VariantsOracle.java"
