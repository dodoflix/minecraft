#!/usr/bin/env bash
# Regenerates wire_limits.golden from the real vanilla classes (write-side limits / EncoderExceptions).
# Usage: crates/mc-protocol/tests/fixtures/wire_limits.sh
# Output: crates/mc-protocol/tests/fixtures/wire_limits.golden (moved into place only when WireLimitsGolden succeeds)
# Compiles tools/extractor Setup + play/PlayHarness into target/play-golden-build, then runs WireLimitsGolden.java.
# Needs: java, javac, ref/ (scripts/setup-ref.sh).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../../.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need java javac
HERE="$ROOT/crates/mc-protocol/tests/fixtures"
CP="$("$ROOT/scripts/vanilla-cp.sh")"
BUILD="$ROOT/target/play-golden-build"
mkdir -p "$BUILD"
javac -nowarn -d "$BUILD" -cp "$CP" "$ROOT/tools/extractor/src/extractor/Setup.java" "$HERE/play/PlayHarness.java" "$HERE/WireLimitsGolden.java"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
cd "$ROOT"
java -cp "$BUILD:$CP" WireLimitsGolden "$TMP/wire_limits.golden"
mv "$TMP/wire_limits.golden" "$HERE/wire_limits.golden"
echo "wrote $HERE/wire_limits.golden"
