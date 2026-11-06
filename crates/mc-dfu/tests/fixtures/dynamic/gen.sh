#!/usr/bin/env bash
# Regenerates golden.txt.gz from the real vanilla/DFU classes (DynamicGolden.java).
# -XX:hashCode=2 makes every identity hash 1, so `Dynamic.hashCode()` (which includes `ops.hashCode()`) and the tie order of
# `Dynamic.updateMapValues`' intermediate HashMap are deterministic.
# Usage: crates/mc-dfu/tests/fixtures/dynamic/gen.sh
# Output: crates/mc-dfu/tests/fixtures/dynamic/golden.txt.gz
# Needs: java, javac, ref/ (scripts/setup-ref.sh).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../../../.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need java javac
HERE="$ROOT/crates/mc-dfu/tests/fixtures/dynamic"
CP="$("$ROOT/scripts/vanilla-cp.sh")"
OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT
javac -nowarn -cp "$CP" -d "$OUT" "$HERE/DynamicGolden.java"
java -XX:+UnlockExperimentalVMOptions -XX:hashCode=2 -cp "$OUT:$CP" DynamicGolden "$OUT/golden.txt"
gzip -9n <"$OUT/golden.txt" >"$OUT/golden.txt.gz"
mv "$OUT/golden.txt.gz" "$HERE/golden.txt.gz"
wc -l <"$OUT/golden.txt"
