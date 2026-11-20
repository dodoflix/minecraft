#!/usr/bin/env bash
# Sample fixtures of the data fixers from the worlds of old vanilla servers: the real inputs of every DataFixTypes constant.
#
#   Usage: scripts/datafix-corpus.sh extract [ID...]    candidate samples from the worlds in the cache (no ID: every world)
#          scripts/datafix-corpus.sh synth              generated candidates for the constants no world has files of
#          scripts/datafix-corpus.sh select             keep a deduplicated set cover per constant, write the update requests
#          scripts/datafix-corpus.sh record             run the requests through vanilla's DataFixers and write the fixtures
#          scripts/datafix-corpus.sh all                extract every world, synth, select and record
#
# Input:  the worlds of scripts/datafix-worlds.sh ($MC_DATAFIX_CACHE/worlds/<id>[-normal]); `all` needs them all.
# Output: crates/mc-datafix/tests/fixtures/samples/corpus_<type>.tsv.gz, one per DataFixTypes constant (format and replay:
#         crates/mc-datafix/tests/common/corpus_samples.rs), working files in $MC_DATAFIX_CACHE/corpus/{candidates,requests,records}.
# Reruns: the fixtures are a function of the worlds in the cache and of the seed; recording twice from the same worlds gives
#         identical files. A different set of worlds is a reviewed diff of the fixtures, never an automatic update.
# Budget: the fixtures stay under 6 MiB together (docs/testing.md allows 8 MiB per file); the counts and sizes per constant are
#         in budget() below.
# Env: MC_DATAFIX_CACHE (default ${XDG_CACHE_HOME:-$HOME/.cache}/mc-datafix-corpus).
# Needs: cargo, java (JDK 25 or newer for the oracle), ref/ (scripts/setup-ref.sh), gzip.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need cargo java gzip

CACHE="${MC_DATAFIX_CACHE:-${XDG_CACHE_HOME:-$HOME/.cache}/mc-datafix-corpus}"
WORK="$CACHE/corpus"
FIXTURES="$ROOT/crates/mc-datafix/tests/fixtures/samples"
ORACLE="$ROOT/scripts/datafix-oracle.sh"

# Constants without files in a server's world folder: their samples come from the schemas (`gen`), n samples per (type,
# milestone), some of them damaged on purpose to compare the error path.
SYNTH_TYPES=(ADVANCEMENTS DEBUG_PROFILE HOTBAR OPTIONS PLAYER SAVED_DATA_COMMAND_STORAGE SAVED_DATA_WANDERING_TRADER STATS STRUCTURE)
SYNTH_FROM=-1,99,184,512,922,1343,1519,1631,1976,2230,2586,2730,2975,3337,3465,3700,3839,3955,4189,4325,4671,4772,4773,4786,4899,4903,5016
SYNTH_N=4
SYNTH_INVALID=0.05

# budget TYPE: `--count N --budget GZIP_BYTES` for select. Real constants keep up to 300 samples; the chunk types are bounded by size.
budget() {
  case "$1" in
    CHUNK) echo "--count 300 --budget 2097152" ;;
    ENTITY_CHUNK) echo "--count 300 --budget 1048576" ;;
    POI_CHUNK) echo "--count 300 --budget 524288" ;;
    LEVEL | LEVEL_SUMMARY) echo "--count 300 --budget 262144" ;;
    *) if printf '%s\n' "${SYNTH_TYPES[@]}" | grep -qx "$1"; then echo "--count 75 --budget 262144"; else echo "--count 300 --budget 262144"; fi ;;
  esac
}

corpus() { (cd "$ROOT" && cargo run -q -p datafix-corpus -- "$@"); }

# world_ids [ID...]: the ids to extract: the arguments, else every world directory of the cache.
world_ids() {
  [ -d "$CACHE/worlds" ] || die "no worlds in $CACHE/worlds: run scripts/datafix-worlds.sh first"
  if [ $# -gt 0 ]; then printf '%s\n' "$@"; else (cd "$CACHE/worlds" && for d in */; do echo "${d%/}"; done | sort -V); fi
}

cmd_extract() {
  local id ids flat
  ids="$(world_ids "$@")"
  mkdir -p "$WORK/candidates"
  for id in $ids; do
    [ -d "$CACHE/worlds/$id" ] || die "no world $id in $CACHE/worlds"
    rm -rf "${WORK:?}/candidates/$id"
    # the scripted worlds are flat, the -normal ones are generated from the seed
    flat=--flat
    [[ $id == *-normal ]] && flat=
    # shellcheck disable=SC2086  # the flag is empty or one word
    corpus extract "$CACHE/worlds/$id" --id "$id" --out "$WORK/candidates/$id" $flat
  done
}

cmd_synth() {
  local ty tmp
  rm -rf "$WORK/candidates/synth"
  mkdir -p "$WORK/candidates/synth"
  tmp="$(mktemp)"
  for ty in "${SYNTH_TYPES[@]}"; do
    quiet "$ORACLE" gen --type "$ty" --from "$SYNTH_FROM" --n "$SYNTH_N" --invalid "$SYNTH_INVALID" --seed 1 >"$tmp"
    corpus import "$tmp" "$WORK/candidates/synth/$ty.cand"
  done
  rm -f "$tmp"
}

# types: the constants that have candidates.
types() { find "$WORK/candidates" -name '*.cand' -printf '%f\n' | sed 's#\.cand$##' | sort -u; }

cmd_select() {
  local ty files
  [ -d "$WORK/candidates" ] || die "no candidates: run extract and synth first"
  rm -rf "$WORK/requests"
  mkdir -p "$WORK/requests"
  for ty in $(types); do
    files=("$WORK"/candidates/*/"$ty.cand")
    # shellcheck disable=SC2046  # the budget is two words
    corpus select $(budget "$ty") --seed 1 --out "$WORK/requests/$ty.req" "${files[@]}"
  done
}

cmd_record() {
  local req ty out part parts=() kept=()
  [ -d "$WORK/requests" ] || die "no requests: run select first"
  # a failed or interrupted run leaves no half-written fixture behind
  trap 'rm -f "$FIXTURES"/corpus_*.tsv.gz.part' EXIT
  rm -f "$FIXTURES"/corpus_*.tsv.gz.part
  rm -rf "$WORK/records"
  mkdir -p "$WORK/records"
  for req in "$WORK"/requests/*.req; do
    [ -s "$req" ] || continue
    ty="$(basename "$req" .req)"
    out="$FIXTURES/corpus_$(echo "$ty" | tr '[:upper:]' '[:lower:]').tsv.gz"
    quiet "$ORACLE" batch "$req" "$WORK/records/$ty.rec"
    corpus pack "$req" "$WORK/records/$ty.rec" "$out.part"
    parts+=("$out.part")
    kept+=("$out")
    echo "datafix-corpus: $out: $(wc -l <"$req") updates"
  done
  [ ${#kept[@]} -gt 0 ] || die "no requests in $WORK/requests"
  # every type is recorded: replace the fixtures, and drop those of a type that has no samples any more
  for out in "$FIXTURES"/corpus_*.tsv.gz; do
    printf '%s\n' "${kept[@]}" | grep -qxF "$out" || rm -f "$out"
  done
  for part in "${parts[@]}"; do mv "$part" "${part%.part}"; done
  trap - EXIT
}

[ $# -gt 0 ] || { usage >&2; exit 2; }
sub="$1"
shift
case "$sub" in
  extract) cmd_extract "$@" ;;
  synth) cmd_synth ;;
  select) cmd_select ;;
  record) cmd_record ;;
  all) [ $# -eq 0 ] || die "all takes no worlds: it records every world of the cache"; rm -rf "$WORK/candidates"; cmd_extract; cmd_synth; cmd_select; cmd_record ;;
  *) usage >&2; exit 2 ;;
esac
