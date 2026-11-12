#!/usr/bin/env bash
# Regenerates the recipe, place-recipe and recipe-book goldens from the real vanilla RecipeManager.
# Usage:
#   scripts/golden-recipes.sh
#
# Output: crates/mc-game/tests/fixtures/{recipe_golden.txt.gz,place_recipe_golden.txt,recipe_book_golden.txt}
# (each replaced only when its generator succeeds; recipe_golden is over 1 MB, so it stays gzip'd)
# Inputs: the data packs crates/mc-game/tests/fixtures/{recipes,recipes_invalid,recipes_duplicate} (also copied into worlds by the recipe golden).
# Needs: java, gzip, ref/ (scripts/setup-ref.sh). Self-contained: no extractor build.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need java gzip
F="$ROOT/crates/mc-game/tests/fixtures"
VJ="$ROOT/scripts/vanilla-java.sh"
ops() { quiet "$VJ" "$F/$1.java" | grep -E "^(OP|OUT) "; }
"$VJ" -o "$F/recipe_golden.txt.gz" "$F/RecipeGolden.java" "$F" "$F/recipe_golden.txt.gz"
zcat "$F/recipe_golden.txt.gz" | wc -l
to_file "$F/place_recipe_golden.txt" ops PlaceRecipeGolden
wc -l < "$F/place_recipe_golden.txt"
to_file "$F/recipe_book_golden.txt" ops RecipeBookGolden
wc -l < "$F/recipe_book_golden.txt"
