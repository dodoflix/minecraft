#!/usr/bin/env bash
# Regenerates a play golden fixture from the real vanilla classes.
# Usage: crates/mc-protocol/tests/fixtures/play/gen.sh Gen<Group>.java
# Output: crates/mc-protocol/tests/fixtures/play/<group>.golden (group = the name after Gen, lower-cased; moved into place only when the generator succeeds)
# Compiles tools/extractor's Setup.java + PlayHarness.java into target/play-golden-build, then runs the generator with the vanilla classpath.
# Needs: java, javac, ref/ (scripts/setup-ref.sh).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../../../.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need java javac
[ $# -ge 1 ] || { usage >&2; exit 2; }
HERE="$ROOT/crates/mc-protocol/tests/fixtures/play"
CP="$("$ROOT/scripts/vanilla-cp.sh")"
BUILD="$ROOT/target/play-golden-build"
mkdir -p "$BUILD"
javac -nowarn -d "$BUILD" -cp "$CP" "$ROOT/tools/extractor/src/extractor/Setup.java" "$HERE/PlayHarness.java"
GEN="$1"; shift
name="$(basename "$GEN" .java)"; name="${name#Gen}"
DEST="$HERE/$(tr '[:upper:]' '[:lower:]' <<<"$name").golden"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
cd "$ROOT"
java -cp "$BUILD:$CP" "$HERE/$GEN" "$TMP/out.golden" "$@"
mv "$TMP/out.golden" "$DEST"
echo "wrote $DEST"
