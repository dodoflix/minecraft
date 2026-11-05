#!/usr/bin/env bash
# Regenerates crates/mc-game/tests/fixtures/variants_golden.txt with the REAL vanilla mob variant registries (DIRECT/NETWORK codec
# digests of every entry, Registry.getRandom, PriorityProvider.pick over biome x moon phase x structure contexts) through the
# in-process vanilla oracle (tests/oracle/VariantsOracle.java).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
"$ROOT/scripts/entity-oracle.sh" "$ROOT/crates/mc-game/tests/oracle/VariantsOracle.java" > "$ROOT/crates/mc-game/tests/fixtures/variants_golden.txt"
