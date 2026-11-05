#!/usr/bin/env bash
# Regenerates crates/mc-game/tests/fixtures/recipe_golden.txt from the real vanilla RecipeManager (needs tools/extractor/build? no: self-contained).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CP="$("$ROOT/scripts/vanilla-cp.sh")"
F="$ROOT/crates/mc-game/tests/fixtures"
java -cp "$CP" "$F/RecipeGolden.java" | gzip -9n > "$F/recipe_golden.txt.gz"
zcat "$F/recipe_golden.txt.gz" | wc -l
java -cp "$CP" "$F/PlaceRecipeGolden.java" 2>/dev/null | grep -E "^(OP|OUT) " | gzip -9n > "$F/place_recipe_golden.txt.gz"
zcat "$F/place_recipe_golden.txt.gz" | wc -l
java -cp "$CP" "$F/RecipeBookGolden.java" 2>/dev/null | grep -E "^(OP|OUT) " | gzip -9n > "$F/recipe_book_golden.txt.gz"
zcat "$F/recipe_book_golden.txt.gz" | wc -l
