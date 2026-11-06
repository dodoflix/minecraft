#!/usr/bin/env bash
# Regenerates the dialog golden from the real vanilla classes (DialogGolden.java over corpus.txt).
# Usage: crates/mc-protocol/tests/fixtures/dialog/gen.sh
# Output: crates/mc-protocol/tests/fixtures/dialog/golden.jsonl (moved into place only when DialogGolden succeeds)
# Needs: java, javac, ref/ (scripts/setup-ref.sh).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../../../.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need java javac
HERE="$ROOT/crates/mc-protocol/tests/fixtures/dialog"
CP="$("$ROOT/scripts/vanilla-cp.sh")"
OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT
javac -nowarn -cp "$CP" -d "$OUT" "$ROOT/tools/extractor/src/extractor/Setup.java" "$HERE/DialogGolden.java"
cd "$ROOT"
java -cp "$OUT:$CP" DialogGolden "$HERE/corpus.txt" "$OUT/golden.jsonl" 2>"$OUT/stderr.log" || { cat "$OUT/stderr.log" >&2; die "DialogGolden failed"; }
mv "$OUT/golden.jsonl" "$HERE/golden.jsonl"
echo "wrote $HERE/golden.jsonl"
