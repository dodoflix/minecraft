#!/usr/bin/env bash
# Regenerates the block placement golden from the vanilla block classes on a real vanilla level.
# Usage:
#   scripts/golden-block-placement.sh                  regenerate the fixture
#   scripts/golden-block-placement.sh verbose BLOCK    print the per-context detail of BLOCK instead (to diff against the Rust
#                                                      test's `BLOCK_PLACEMENT_VERBOSE=<block>` output)
#
# Output: crates/mc-game/tests/fixtures/block_placement.txt (replaced only when the generator succeeds)
# Needs: java, ref/ with decompiled sources (scripts/setup-ref.sh --decompile) and MC_ACCEPT_EULA=1 (the oracle starts a vanilla level).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need java
T="$ROOT/crates/mc-game/tests"
if [ "${1:-}" = verbose ]; then
  "$ROOT/scripts/entity-oracle.sh" "$T/oracle/BlockPlacementOracle.java" "$T/fixtures/block_behaviour_pool.txt" verbose "${2:?block name}"
else
  "$ROOT/scripts/entity-oracle.sh" "$T/oracle/BlockPlacementOracle.java" -o "$T/fixtures/block_placement.txt" "$T/fixtures/block_behaviour_pool.txt"
fi
