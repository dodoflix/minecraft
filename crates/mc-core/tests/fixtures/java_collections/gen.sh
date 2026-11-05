#!/usr/bin/env bash
# Regenerate golden.txt from the real vanilla/JDK/fastutil classes: gen.sh
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../../../../.." && pwd)"
CP="$("$ROOT/scripts/vanilla-cp.sh")"
OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT
javac -nowarn -cp "$CP" -d "$OUT" "$HERE/HashOrderGolden.java"
java -cp "$OUT:$CP" HashOrderGolden "$HERE/golden.txt"
wc -l "$HERE/golden.txt"
