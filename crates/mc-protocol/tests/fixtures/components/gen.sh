#!/usr/bin/env bash
# Regenerate the golden fixtures: gen.sh            -> golden.txt from corpus/*.txt, registries.golden
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../../../../.." && pwd)"
CP="$("$ROOT/scripts/vanilla-cp.sh")"
OUT="$(mktemp -d)"
javac -nowarn -cp "$CP" -d "$OUT" "$ROOT/tools/extractor/src/extractor/Setup.java" "$HERE/ComponentGolden.java" 2>&1 | grep -v "^Note:" || true
cat "$HERE"/corpus/*.txt > "$OUT/corpus.txt"
java -cp "$OUT:$CP" ComponentGolden registries "$HERE/registries.golden"
java -cp "$OUT:$CP" ComponentGolden "$OUT/corpus.txt" "$HERE/golden.txt"
echo "wrote $(wc -l < "$HERE/golden.txt") records"
