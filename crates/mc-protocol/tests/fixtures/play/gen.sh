#!/usr/bin/env bash
# Regenerates a play golden fixture from the real vanilla classes.
#   crates/mc-protocol/tests/fixtures/play/gen.sh Gen<Group>.java     (writes <group>.golden next to it; name = args[0] of the generator)
# Compiles tools/extractor's Setup.java + PlayHarness.java into target/play-golden-build, then runs the generator with the vanilla classpath.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../../../../.." && pwd)"
CP="$("$ROOT/scripts/vanilla-cp.sh")"
BUILD="$ROOT/target/play-golden-build"
mkdir -p "$BUILD"
javac -nowarn -d "$BUILD" -cp "$CP" "$ROOT/tools/extractor/src/extractor/Setup.java" "$HERE/PlayHarness.java"
GEN="$1"; shift
OUT="$HERE/$(basename "$GEN" .java | sed 's/^Gen//' | tr '[:upper:]' '[:lower:]').golden"
cd "$ROOT"
java -cp "$BUILD:$CP" "$HERE/$GEN" "$OUT" "$@"
echo "wrote $OUT"
