#!/usr/bin/env bash
# Regenerates crates/mc-game/tests/fixtures/block_behaviour.txt from the vanilla block classes (needs tools/extractor/build).
# `scripts/golden-block-behaviour.sh verbose <block>` prints the per-state detail instead (to diff against the Rust test's
# `BLOCK_BEHAVIOUR_VERBOSE=<block>` output).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CP="$("$ROOT/scripts/vanilla-cp.sh")"
EXTRACTOR="${EXTRACTOR_BUILD:-$ROOT/tools/extractor/build}"
[ -d "$EXTRACTOR" ] || EXTRACTOR=./tools/extractor/build
T="$ROOT/crates/mc-game/tests"
if [ "${1:-}" = verbose ]; then
  java -cp "$EXTRACTOR:$CP" "$T/oracle/BlockBehaviourOracle.java" "$T/fixtures/block_behaviour_pool.txt" verbose "$2"
else
  java -cp "$EXTRACTOR:$CP" "$T/oracle/BlockBehaviourOracle.java" "$T/fixtures/block_behaviour_pool.txt" > "$T/fixtures/block_behaviour.txt"
  wc -l "$T/fixtures/block_behaviour.txt"
fi
