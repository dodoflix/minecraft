#!/usr/bin/env bash
# Regenerates crates/mc-game/tests/fixtures/menu_golden.txt from the vanilla classes (needs tools/extractor/build for Setup.bootstrap).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CP="$("$ROOT/scripts/vanilla-cp.sh")"
EXTRACTOR="${EXTRACTOR_BUILD:-$ROOT/tools/extractor/build}"
F="$ROOT/crates/mc-game/tests/fixtures"
java -cp "$EXTRACTOR:$CP" "$F/MenuGolden.java" > "$F/menu_golden.txt"
wc -l "$F/menu_golden.txt"
java -cp "$EXTRACTOR:$CP" "$F/ItemGolden.java" 2>/dev/null | grep -E '^(tool|use) ' > "$F/item_golden.txt"
wc -l "$F/item_golden.txt"
