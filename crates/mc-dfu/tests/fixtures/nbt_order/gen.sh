#!/usr/bin/env bash
# Regenerates golden.txt.gz from the real vanilla classes (NbtOrderGolden.java): the key order of NbtOps compounds.
# Usage: crates/mc-dfu/tests/fixtures/nbt_order/gen.sh
# Output: crates/mc-dfu/tests/fixtures/nbt_order/golden.txt.gz
# Needs: java, javac, ref/ (scripts/setup-ref.sh).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../../../.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need java javac
HERE="$ROOT/crates/mc-dfu/tests/fixtures/nbt_order"
CP="$("$ROOT/scripts/vanilla-cp.sh")"
OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT
javac -nowarn -cp "$CP" -d "$OUT" "$HERE/NbtOrderGolden.java"
java -cp "$OUT:$CP" NbtOrderGolden "$OUT/golden.txt"
gzip -9n <"$OUT/golden.txt" >"$OUT/golden.txt.gz"
mv "$OUT/golden.txt.gz" "$HERE/golden.txt.gz"
wc -l <"$OUT/golden.txt"
