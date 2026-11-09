#!/usr/bin/env bash
# Regenerates the block behaviour golden from the vanilla block classes.
# Usage:
#   scripts/golden-block-behaviour.sh                  regenerate the fixture
#   scripts/golden-block-behaviour.sh overrides        regenerate tests/fixtures/block_class_overrides.txt (the methods every block class overrides)
#   scripts/golden-block-behaviour.sh verbose BLOCK    print the per-state detail of BLOCK instead (to diff against the Rust
#                                                      test's `BLOCK_BEHAVIOUR_VERBOSE=<block>` output)
#
# Output: crates/mc-game/tests/fixtures/block_behaviour.txt (replaced only when the generator succeeds)
# Needs: java, ref/ (scripts/setup-ref.sh) and the extractor build (scripts/extract.sh; MC_EXTRACTOR_BUILD overrides its directory).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need java
export MC_EXTRA_CP="${MC_EXTRACTOR_BUILD:-$ROOT/tools/extractor/build}"
T="$ROOT/crates/mc-game/tests"
if [ "${1:-}" = overrides ]; then
  "$ROOT/scripts/vanilla-java.sh" -o "$T/fixtures/block_class_overrides.txt" "$T/oracle/BlockClassOverridesOracle.java"
  wc -l "$T/fixtures/block_class_overrides.txt"
elif [ "${1:-}" = verbose ]; then
  "$ROOT/scripts/vanilla-java.sh" "$T/oracle/BlockBehaviourOracle.java" "$T/fixtures/block_behaviour_pool.txt" verbose "${2:?block name}"
else
  "$ROOT/scripts/vanilla-java.sh" -o "$T/fixtures/block_behaviour.txt" "$T/oracle/BlockBehaviourOracle.java" "$T/fixtures/block_behaviour_pool.txt"
  wc -l "$T/fixtures/block_behaviour.txt"
fi
