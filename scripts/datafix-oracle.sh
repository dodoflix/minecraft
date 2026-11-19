#!/usr/bin/env bash
# Runs the data fixer oracle: vanilla's DataFixers on NBT inputs, the fix chain listing and the schema type dump.
#
# Usage:
#   scripts/datafix-oracle.sh fixes-list                    INDEX VERSION SUBVERSION KIND CLASS of every registered fix
#   scripts/datafix-oracle.sh schema-dump KEY TYPE          normalised description of one type (KEY = version * 10 + sub)
#   scripts/datafix-oracle.sh schema-dump KEY all           TYPE and sha256 of every type of one schema
#   scripts/datafix-oracle.sh schema-dump all               every schema, delta encoded: KEY * - opens a schema, KEY TYPE sha256 follows
#   scripts/datafix-oracle.sh grammar-sample                descriptions of a small schema built with the real DSL
#   scripts/datafix-oracle.sh fix TYPE FROM TO FILE [OUT]   one NBT file through DataFixTypes.update (TYPE: CHUNK or ref:item_stack)
#   scripts/datafix-oracle.sh batch IN|- OUT|-              many updates, one line each (record format: DataFixerOracle.java)
#   scripts/datafix-oracle.sh --regen                       rewrite fixes.tsv and schema_types.tsv.gz under crates/mc-datafix/tests/fixtures
#
# Output: stdout of the verb (--regen writes crates/mc-datafix/tests/fixtures/fixes.tsv and schema_types.tsv.gz).
# The Java sources in crates/mc-datafix/tests/oracle are compiled into target/datafix-oracle when they changed.
# Needs: java, javac, flock, sha256sum, ref/ (scripts/setup-ref.sh).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
[ $# -gt 0 ] || { usage >&2; exit 2; }
need java javac flock sha256sum
SRC="$ROOT/crates/mc-datafix/tests/oracle"
FX="$ROOT/crates/mc-datafix/tests/fixtures"
OUT="$ROOT/target/datafix-oracle"
CP="$("$ROOT/scripts/vanilla-cp.sh")"

compile() {
  mkdir -p "$ROOT/target"
  exec 9>"$ROOT/target/datafix-oracle.lock"
  flock 9
  local stamp; stamp="$(cat "$SRC"/*.java | sha256sum | cut -d' ' -f1)"
  if [ "$(cat "$OUT/stamp" 2>/dev/null || true)" != "$stamp" ]; then
    local tmp; tmp="$(mktemp -d "$ROOT/target/datafix-oracle.XXXXXX")"
    javac -proc:none -nowarn -Xlint:none -cp "$CP" -d "$tmp" "$SRC"/*.java || { rm -rf "$tmp"; exit 1; }
    echo "$stamp" >"$tmp/stamp"
    rm -rf "$OUT"
    mv "$tmp" "$OUT"
  fi
  exec 9>&-
}

compile
ORACLE=(java -Dlog4j2.configurationFile="$SRC/log4j2-oracle.xml" -cp "$OUT:$CP" DataFixerOracle)
if [ "$1" = --regen ]; then
  to_file "$FX/fixes.tsv" "${ORACLE[@]}" fixes-list
  to_file "$FX/schema_types.tsv.gz" "${ORACLE[@]}" schema-dump all
  wc -l "$FX/fixes.tsv" >&2
else
  exec "${ORACLE[@]}" "$@" # the JVM replaces the script: a caller's process group and parent-death signal reach it
fi
