#!/usr/bin/env bash
# Regenerates golden.jsonl (and the text-component dialog goldens) from the real vanilla classes:
#   crates/mc-protocol/tests/fixtures/dialog/gen.sh
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../../../../.." && pwd)"
CP="$("$ROOT/scripts/vanilla-cp.sh")"
OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT
javac -nowarn -cp "$CP" -d "$OUT" "$ROOT/tools/extractor/src/extractor/Setup.java" "$HERE/DialogGolden.java" 2>&1 | grep -v "^Note:" || true
cd "$ROOT"
java -cp "$OUT:$CP" DialogGolden "$HERE/corpus.txt" "$HERE/golden.jsonl" 2>/dev/null
echo "wrote $HERE/golden.jsonl"
