#!/usr/bin/env bash
# Regenerates the argument-type goldens (crates/mc-game/tests/fixtures/args/<type>_golden.txt) from the real vanilla classes.
#   scripts/args-golden.sh [type ...]
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
EXT="${MC_EXTRACTOR_BUILD:-$ROOT/tools/extractor/build}"
[ -d "$EXT" ] || EXT=./tools/extractor/build
D="$ROOT/crates/mc-game/tests/fixtures/args"
exec java -cp "$EXT:$("$ROOT/scripts/vanilla-cp.sh")" "$D/ArgsGolden.java" "$D" "$@"
