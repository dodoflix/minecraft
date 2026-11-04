#!/usr/bin/env bash
# Compiles tools/extractor against the unbundled vanilla server and dumps JSON to ref/extracted/.
# usage: scripts/extract.sh [pass ...]   (no args = all passes)
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CP="$("$ROOT/scripts/vanilla-cp.sh")"
BUILD="$ROOT/tools/extractor/build"
rm -rf "$BUILD" && mkdir -p "$BUILD"
javac -nowarn -d "$BUILD" -cp "$CP" "$ROOT"/tools/extractor/src/extractor/*.java
cd "$ROOT"
exec java -cp "$BUILD:$CP" extractor.Main "$ROOT/ref/extracted" "$@"
