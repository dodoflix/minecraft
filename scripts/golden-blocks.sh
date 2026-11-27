#!/usr/bin/env bash
# Regenerates the block goldens from the vanilla block classes.
# Usage:
#   scripts/golden-blocks.sh
#
# Output: crates/mc-data/tests/fixtures/block_golden.json.gz and block_state_strings.txt.gz (each replaced only when its
# generator succeeds)
# Needs: java, ref/ (scripts/setup-ref.sh) and, for block_golden, the extractor build (scripts/extract.sh; MC_EXTRACTOR_BUILD overrides its directory).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need java
F="$ROOT/crates/mc-data/tests/fixtures"
cd "$ROOT"
quiet env MC_EXTRA_CP="${MC_EXTRACTOR_BUILD:-$ROOT/tools/extractor/build}" "$ROOT/scripts/vanilla-java.sh" -o "$F/block_golden.json.gz" "$F/GoldenBlocks.java" "$F/block_golden.json.gz" >/dev/null
quiet "$ROOT/scripts/vanilla-java.sh" -o "$F/block_state_strings.txt.gz" "$F/BlockStateToString.java" "$F/block_state_strings.txt.gz" >/dev/null
ls -la "$F/block_golden.json.gz" "$F/block_state_strings.txt.gz"
