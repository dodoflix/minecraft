#!/usr/bin/env bash
# Regenerates ../json_compat.tsv from the real Gson stack and vanilla's LegacyComponentDataFixUtils (JsonCompatGolden.java).
# Usage: crates/mc-datafix/tests/fixtures/json_compat/gen.sh
# Output: crates/mc-datafix/tests/fixtures/json_compat.tsv
# Needs: java, javac, ref/ (scripts/setup-ref.sh).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../../../.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need java javac
HERE="$ROOT/crates/mc-datafix/tests/fixtures/json_compat"
CP="$("$ROOT/scripts/vanilla-cp.sh")"
OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT
javac -nowarn -cp "$CP" -d "$OUT" "$HERE/JsonCompatGolden.java"
java -cp "$OUT:$CP" JsonCompatGolden "$OUT/golden.tsv"
mv "$OUT/golden.tsv" "$HERE/../json_compat.tsv"
wc -l <"$HERE/../json_compat.tsv"
