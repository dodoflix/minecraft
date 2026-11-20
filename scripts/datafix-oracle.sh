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
#   scripts/datafix-oracle.sh gen OPTIONS                   schema-driven samples as batch lines (options and shapes: SampleGen.java)
#   scripts/datafix-oracle.sh tables DIR                    the lookup tables of the fix classes as DIR/<class>.tsv (format: TableDump.java)
#   scripts/datafix-oracle.sh table-accessors               rows of the public table accessors over every key (format: TableDump.java)
#   scripts/datafix-oracle.sh --regen                       rewrite fixes.tsv, schema_types.tsv.gz and table_accessors.tsv.gz under
#                                                           crates/mc-datafix/tests/fixtures and the tables under crates/mc-datafix/src/fixes/data
#
# Output: stdout of the verb (--regen writes crates/mc-datafix/tests/fixtures/fixes.tsv, schema_types.tsv.gz,
# table_accessors.tsv.gz and crates/mc-datafix/src/fixes/data/*.tsv.gz).
# `gen` reads the vocabulary target/datafix/vocab.txt (string literals of the datafix sources plus the registry ids of
# ref/extracted), rebuilt when a datafix source is newer.
# The Java sources in crates/mc-datafix/tests/oracle are compiled into target/datafix-oracle when they changed.
# Needs: java, javac, flock, sha256sum, grep, sort, ref/ (scripts/setup-ref.sh).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
[ $# -gt 0 ] || { usage >&2; exit 2; }
need java javac flock sha256sum grep sort
SRC="$ROOT/crates/mc-datafix/tests/oracle"
FX="$ROOT/crates/mc-datafix/tests/fixtures"
TABLES="$ROOT/crates/mc-datafix/src/fixes/data"
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

# vocab: one string per line, sorted bytewise and unique: what the fixers mention (old ids, field names) and the registry ids.
vocab() {
  local v="$ROOT/target/datafix/vocab.txt" d="$ROOT/ref/src/net/minecraft/util/datafix" tmp
  [ -d "$d" ] || die "$d missing; run scripts/setup-ref.sh --decompile"
  [ -s "$ROOT/ref/extracted/items.json" ] || die "ref/extracted is missing; run scripts/setup-ref.sh"
  if [ ! -s "$v" ] || [ "$0" -nt "$v" ] || [ -n "$(find "$d" -newer "$v" -print -quit)" ]; then
    mkdir -p "$ROOT/target/datafix"
    tmp="$(mktemp "$v.XXXXXX")"
    {
      grep -rhoE '"([^"\\]|\\.){1,64}"' "$d/fixes" "$d/schemas" "$d/DataFixers.java" | grep -v '[\\[:cntrl:]]' || true
      grep -hoE '"name":"minecraft:[a-z0-9_/.]+"' "$ROOT"/ref/extracted/{items,blocks,entity_types}.json | sed 's/^"name"://'
    } | tr -d '"' | LC_ALL=C sort -u >"$tmp"
    chmod 644 "$tmp"
    mv "$tmp" "$v"
  fi
  echo "$v"
}

compile

# regen_tables: the tables dump of every table class as TABLES/<class>.tsv.gz (gzip -9n); a class that is no longer
# dumped loses its file. Nothing is touched when the dump fails.
regen_tables() {
  local tmp f
  tmp="$(mktemp -d "$ROOT/target/datafix-tables.XXXXXX")"
  "${ORACLE[@]}" tables "$tmp" || { rm -rf "$tmp"; return 1; }
  for f in "$tmp"/*.tsv; do
    gzip -9n <"$f" >"$f.gz"
    chmod 644 "$f.gz"
  done
  mkdir -p "$TABLES"
  for f in "$TABLES"/*.tsv.gz; do
    [ ! -e "$f" ] || [ -e "$tmp/$(basename "$f")" ] || rm "$f"
  done
  mv "$tmp"/*.tsv.gz "$TABLES"/
  rm -rf "$tmp"
}

# -XX:hashCode=2 makes every identity hash 1: Dynamic.hashCode() includes ops.hashCode() (an identity hash), so the order of the
# HashMap<Dynamic, Dynamic> behind Dynamic.updateMapValues would otherwise differ from run to run (recordings could not repeat).
ORACLE=(java -XX:+UnlockExperimentalVMOptions -XX:hashCode=2 -Dlog4j2.configurationFile="$SRC/log4j2-oracle.xml" -cp "$OUT:$CP")
if [ "$1" = gen ]; then
  VOCAB="$(vocab)" # a failing assignment stops the script
  ORACLE+=("-Ddatafix.vocab=$VOCAB")
fi
ORACLE+=(DataFixerOracle)
if [ "$1" = --regen ]; then
  to_file "$FX/fixes.tsv" "${ORACLE[@]}" fixes-list
  to_file "$FX/schema_types.tsv.gz" "${ORACLE[@]}" schema-dump all
  to_file "$FX/table_accessors.tsv.gz" "${ORACLE[@]}" table-accessors
  regen_tables
  wc -l "$FX/fixes.tsv" >&2
else
  exec "${ORACLE[@]}" "$@" # the JVM replaces the script: a caller's process group and parent-death signal reach it
fi
