#!/usr/bin/env bash
# Records STEP samples from the real vanilla fixer.
# Usage: crates/mc-datafix/tests/fixtures/samples/record.sh NAME N [INVALID]
# Output: crates/mc-datafix/tests/fixtures/samples/NAME.tsv.gz
# Reads samples/NAME.spec (lines `FROM TO TYPE[,TYPE..]`, `#` comments), draws N schema-driven samples per (type, FROM)
# with the sample generator (INVALID: fraction damaged on purpose, default 0.1), runs them through vanilla's
# DataFixers.update and writes samples/NAME.tsv.gz: alternating lines, the batch request and vanilla's record.
# samples/NAME.snbt (optional; NAME.snbt.gz when over 1 MB, `gzip -9n`) holds hand written and real samples (`ID TYPE FROM TO SNBT`
# or `ID TYPE FROM TO BASE64`, tab separated) appended to the batch.
# Output is a pure function of the spec, N, INVALID and the vocabulary; recording twice must give identical files.
# Needs: java, javac, gzip, ref/ (scripts/setup-ref.sh).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../../../.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
[ $# -ge 2 ] || { usage >&2; exit 2; }
need java javac gzip
NAME="$1" N="$2" INVALID="${3:-0.1}"
HERE="$ROOT/crates/mc-datafix/tests/fixtures/samples"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
while read -r from to types; do
  case "$from" in '' | '#'*) continue ;; esac
  "$ROOT/scripts/datafix-oracle.sh" gen --type "$types" --from "$from" --to "$to" --n "$N" --invalid "$INVALID" >>"$TMP/in" 2>/dev/null
done <"$HERE/$NAME.spec"
# hand written samples: lines `ID TYPE FROM TO SNBT` (tab separated), `#` comments
if [ -f "$HERE/$NAME.snbt" ]; then grep -v '^#' "$HERE/$NAME.snbt" | grep -v '^$' >>"$TMP/in"; fi
if [ -f "$HERE/$NAME.snbt.gz" ]; then gzip -dc "$HERE/$NAME.snbt.gz" | grep -v '^#' | grep -v '^$' >>"$TMP/in"; fi
"$ROOT/scripts/datafix-oracle.sh" batch "$TMP/in" "$TMP/out" 2>/dev/null
paste -d '\n' "$TMP/in" "$TMP/out" | gzip -9n >"$HERE/$NAME.tsv.gz"
wc -l <"$TMP/in"
