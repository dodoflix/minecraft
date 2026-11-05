#!/usr/bin/env bash
# Regenerates wire_limits.golden from the real vanilla classes (write-side limits / EncoderExceptions).
#   crates/mc-protocol/tests/fixtures/wire_limits.sh
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../../../.." && pwd)"
CP="$("$ROOT/scripts/vanilla-cp.sh")"
BUILD="$ROOT/target/play-golden-build"
mkdir -p "$BUILD"
javac -nowarn -d "$BUILD" -cp "$CP" "$ROOT/tools/extractor/src/extractor/Setup.java" "$HERE/play/PlayHarness.java" "$HERE/WireLimitsGolden.java"
cd "$ROOT"
java -cp "$BUILD:$CP" WireLimitsGolden "$HERE/wire_limits.golden"
echo "wrote $HERE/wire_limits.golden"
