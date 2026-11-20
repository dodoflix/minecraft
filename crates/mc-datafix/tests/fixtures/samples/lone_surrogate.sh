#!/usr/bin/env bash
# Records the lone-surrogate STEP samples from the real vanilla fixer.
# Usage: crates/mc-datafix/tests/fixtures/samples/lone_surrogate.sh
# Output: crates/mc-datafix/tests/fixtures/samples/step_lone_surrogate.tsv.gz
# One schema-driven sample per (fix version, type) with half of its strings and keys given an unpaired UTF-16 surrogate
# (`gen --matrix step --poison 0.5`), run through vanilla's DataFixers.update; the alternating request and record lines are
# replayed by samples_lone_surrogate.rs. Output is a pure function of the seed and the vocabulary; recording twice must give
# identical files.
# Needs: java, javac, gzip, ref/ (scripts/setup-ref.sh).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../../../.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need java javac gzip
HERE="$ROOT/crates/mc-datafix/tests/fixtures/samples"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
"$ROOT/scripts/datafix-oracle.sh" gen --matrix step --n 1 --poison 0.5 --seed 7 >"$TMP/in" 2>/dev/null
"$ROOT/scripts/datafix-oracle.sh" batch "$TMP/in" "$TMP/out" 2>/dev/null
paste -d '\n' "$TMP/in" "$TMP/out" | gzip -9n >"$HERE/step_lone_surrogate.tsv.gz"
wc -l <"$TMP/in"
