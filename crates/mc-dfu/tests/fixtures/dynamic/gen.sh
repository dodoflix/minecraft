#!/usr/bin/env bash
# Regenerate golden.txt from the real vanilla/DFU classes: gen.sh
# -XX:hashCode=2 makes every identity hash 1, so `Dynamic.hashCode()` (which includes `ops.hashCode()`) and the tie order of
# `Dynamic.updateMapValues`' intermediate HashMap are deterministic.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../../../../.." && pwd)"
CP="$("$ROOT/scripts/vanilla-cp.sh")"
OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT
javac -nowarn -cp "$CP" -d "$OUT" "$HERE/DynamicGolden.java"
java -XX:+UnlockExperimentalVMOptions -XX:hashCode=2 -cp "$OUT:$CP" DynamicGolden "$HERE/golden.txt"
wc -l "$HERE/golden.txt"
