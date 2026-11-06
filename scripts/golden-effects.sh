#!/usr/bin/env bash
# Regenerates the effect/attribute golden from the real vanilla effect and attribute classes.
# Usage:
#   scripts/golden-effects.sh
#
# Output: crates/mc-game/tests/fixtures/effects_golden.txt.gz (replaced only when the generator succeeds)
# Needs: java, ref/ (scripts/setup-ref.sh).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need java
F="$ROOT/crates/mc-game/tests/fixtures"
"$ROOT/scripts/vanilla-java.sh" -o "$F/effects_golden.txt.gz" "$F/EffectsGolden.java" "$F/effects_golden.txt.gz"
zcat "$F/effects_golden.txt.gz" | wc -l
