#!/usr/bin/env bash
# Regenerates the lone-surrogate fixtures from the real vanilla classes (LoneSurrogateGolden.java).
# Usage: crates/mc-protocol/tests/fixtures/lone_surrogate/gen.sh
# Output: crates/mc-nbt/tests/fixtures/lone_surrogate.tsv, crates/mc-text/tests/fixtures/lone_surrogate.tsv,
#   crates/mc-protocol/tests/fixtures/lone_surrogate.tsv, crates/mc-brigadier/tests/fixtures/lone_surrogate.tsv,
#   crates/mc-game/tests/fixtures/lone_surrogate.tsv, crates/mc-game-core/tests/fixtures/lone_surrogate.tsv
# Needs: java, javac, ref/ (scripts/setup-ref.sh).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../../../.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need java javac
HERE="$ROOT/crates/mc-protocol/tests/fixtures/lone_surrogate"
CP="$("$ROOT/scripts/vanilla-cp.sh")"
OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT
javac -nowarn -encoding UTF-8 -cp "$CP" -d "$OUT" "$ROOT/tools/extractor/src/extractor/Setup.java" "$HERE/LoneSurrogateGolden.java"
mkdir -p "$OUT/rows"
java -cp "$OUT:$CP" LoneSurrogateGolden "$OUT/rows"
wc -l "$OUT"/rows/*.tsv
for f in "nbt:mc-nbt" "text:mc-text" "wire:mc-protocol" "brigadier:mc-brigadier" "game:mc-game" "core:mc-game-core"; do
  place_fixture "$OUT/rows/${f%%:*}.tsv" "$ROOT/crates/${f##*:}/tests/fixtures/lone_surrogate.tsv"
done
