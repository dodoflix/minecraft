#!/usr/bin/env bash
# Regenerates ../base_fixes.tsv from the real vanilla fix classes (BaseFixGolden.java).
# Usage: crates/mc-datafix/tests/fixtures/base_fixes/gen.sh
# Output: crates/mc-datafix/tests/fixtures/base_fixes.tsv
# Needs: java, javac, ref/ (scripts/setup-ref.sh).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../../../.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need java javac
HERE="$ROOT/crates/mc-datafix/tests/fixtures/base_fixes"
CP="$("$ROOT/scripts/vanilla-cp.sh")"
OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT
javac -nowarn -cp "$CP" -d "$OUT" "$HERE/BaseFixGolden.java"
java -cp "$OUT:$CP" BaseFixGolden "$OUT/golden.tsv"
mv "$OUT/golden.tsv" "$HERE/../base_fixes.tsv"
wc -l <"$HERE/../base_fixes.tsv"
