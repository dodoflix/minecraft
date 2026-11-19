#!/usr/bin/env bash
# Regenerates ../library.tsv from the real vanilla/DFU classes (LibraryGolden.java).
# Usage: crates/mc-datafix/tests/fixtures/library/gen.sh
# Output: crates/mc-datafix/tests/fixtures/library.tsv
# Needs: java, javac, ref/ (scripts/setup-ref.sh).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../../../.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need java javac
HERE="$ROOT/crates/mc-datafix/tests/fixtures/library"
CP="$("$ROOT/scripts/vanilla-cp.sh")"
OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT
javac -nowarn -cp "$CP" -d "$OUT" "$HERE/LibraryGolden.java"
java -cp "$OUT:$CP" LibraryGolden "$OUT/golden.tsv"
mv "$OUT/golden.tsv" "$HERE/../library.tsv"
wc -l <"$HERE/../library.tsv"
