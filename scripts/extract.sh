#!/usr/bin/env bash
# Compiles tools/extractor against the unbundled vanilla server and dumps JSON to ref/extracted/.
#
#   scripts/extract.sh [PASS ...]     (no arguments = all passes)
#
# Output: ref/extracted/*.json; tools/extractor/build (the compiled extractor, also the classpath of the golden generators).
# Stale classes of deleted extractor sources stay in tools/extractor/build until you delete that directory by hand.
# Needs: java, javac, ref/ (scripts/setup-ref.sh).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need java javac
CP="$("$ROOT/scripts/vanilla-cp.sh")"
BUILD="$ROOT/tools/extractor/build"
mkdir -p "$BUILD"
javac -nowarn -d "$BUILD" -cp "$CP" "$ROOT"/tools/extractor/src/extractor/*.java
cd "$ROOT"
exec java -cp "$BUILD:$CP" extractor.Main "$ROOT/ref/extracted" "$@"
