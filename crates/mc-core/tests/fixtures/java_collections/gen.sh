#!/usr/bin/env bash
# Regenerates golden.txt from the real vanilla/JDK/fastutil classes (HashOrderGolden.java).
# Usage: crates/mc-core/tests/fixtures/java_collections/gen.sh
# Output: crates/mc-core/tests/fixtures/java_collections/golden.txt
# Needs: java, javac, ref/ (scripts/setup-ref.sh).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../../../.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need java javac
HERE="$ROOT/crates/mc-core/tests/fixtures/java_collections"
CP="$("$ROOT/scripts/vanilla-cp.sh")"
OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT
javac -nowarn -cp "$CP" -d "$OUT" "$HERE/HashOrderGolden.java"
java -cp "$OUT:$CP" HashOrderGolden "$OUT/golden.txt"
mv "$OUT/golden.txt" "$HERE/golden.txt"
wc -l "$HERE/golden.txt"
