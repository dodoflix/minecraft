#!/usr/bin/env bash
# Regenerates crates/mc-game/tests/fixtures/effects_golden.txt from the real vanilla effect/attribute classes.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
F="$ROOT/crates/mc-game/tests/fixtures"
"$ROOT/scripts/vanilla-java.sh" "$F/EffectsGolden.java" "$F/effects_golden.txt" 2>/dev/null
wc -l "$F/effects_golden.txt"
