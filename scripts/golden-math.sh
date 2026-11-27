#!/usr/bin/env bash
# Regenerates the java.lang.Math golden (2^21 inputs per function, block digests) from the oracle JDK, or measures how many inputs
# each Rust spelling gets wrong.
# Usage:
#   scripts/golden-math.sh            rewrite the fixture
#   scripts/golden-math.sh --rate     run the JDK live over every input and print the exact mismatch count per spelling
#
# Output: crates/mc-core/tests/fixtures/math/math_intrinsics.txt.gz (replaced only when the generator succeeds); --rate prints a table.
# Needs: java (the oracle JDK); --rate also cargo.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need java
F="$ROOT/crates/mc-core/tests/fixtures/math"
case "${1:-}" in
  "") to_file "$F/math_intrinsics.txt.gz" java "$F/MathGolden.java"; zcat "$F/math_intrinsics.txt.gz" | wc -l ;;
  --rate) cd "$ROOT" && cargo test -p mc-core --test golden live_mismatch_rates -- --ignored --nocapture ;;
  *) usage >&2; exit 2 ;;
esac
