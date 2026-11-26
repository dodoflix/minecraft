#!/usr/bin/env bash
# Runs the vanilla-side oracle drivers one JVM at a time under the execution-coverage agent and merges what they executed.
#
# Usage:
#   scripts/vanilla-coverage-campaign.sh [GROUP...]     GROUP: entity menu predicates goldens rust (default: all, in that order)
#
# Env CAMPAIGN_DIR: working directory (default target/vanilla-coverage-campaign); a rerun skips the jobs already in runs.tsv.
# Env CAMPAIGN_RUST_CRATES: crates whose `oracle` test target runs under the agent (default: every crate that has one).
# Output: CAMPAIGN_DIR/merged.tsv.gz (union of all runs, see scripts/vanilla-coverage.py), CAMPAIGN_DIR/runs.tsv (one row per
#   job: label, group, exit status, seconds, fixture comparison), CAMPAIGN_DIR/campaign.log. Fixtures are never changed: scripts that
#   rewrite them run in a copy, entity scenarios write to a scratch file that is compared byte for byte with the committed one.
# Needs: java, python3, ref/ with decompiled sources, MC_ACCEPT_EULA=1.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need java python3 gzip
require_eula
T="$ROOT/crates/mc-game/tests"
D="$(realpath -m "${CAMPAIGN_DIR:-$ROOT/target/vanilla-coverage-campaign}")"
mkdir -p "$D/raw" "$D/scratch"
export MC_VANILLA_AGENT="$D/raw"
RUNS="$D/runs.tsv"
touch "$RUNS"
COV="$ROOT/scripts/vanilla-coverage.py"
COPY="$D/scratch/tree"
pending=0

log() { echo "$(date +%T) $*" | tee -a "$D/campaign.log" >&2; }

flush() {
  shopt -s nullglob
  local files=("$D"/raw/*.cov.gz)
  [ ${#files[@]} -gt 0 ] || return 0
  local in=("${files[@]}")
  [ ! -f "$D/merged.tsv.gz" ] || in=("$D/merged.tsv.gz" "${files[@]}")
  python3 "$COV" merge "$D/merged.tsv.gz" "${in[@]}"
  rm -f "${files[@]}"
  pending=0
}

done_before() { awk -F'\t' -v l="$1" '$1 == l { f = 1 } END { exit !f }' "$RUNS"; }

# job LABEL GROUP CMD...: runs CMD under the agent (coverage file labelled LABEL); CMD's fixture check is `verify` below.
job() {
  local label=$1 group=$2; shift 2
  done_before "$label" && return 0
  local t0 rc=0 cmp=-
  t0=$(date +%s)
  rm -f "$D/scratch/fixture"
  MC_VANILLA_AGENT_LABEL="${label//[^A-Za-z0-9._-]/_}" "$@" >"$D/scratch/out.txt" 2>"$D/scratch/err.txt" || rc=$?
  if [ -n "${VERIFY:-}" ] && [ $rc = 0 ]; then
    if [ -e "$VERIFY" ]; then
      if [[ $VERIFY == *.gz ]]; then
        if zcat "$VERIFY" | cmp -s - "$D/scratch/fixture"; then cmp=same; else cmp=DIFFERENT; fi
      elif cmp -s "$VERIFY" "$D/scratch/fixture"; then cmp=same; else cmp=DIFFERENT; fi
    else
      cmp=no-fixture
    fi
  fi
  printf '%s\t%s\t%s\t%s\t%s\n' "$label" "$group" "$rc" "$(($(date +%s) - t0))" "$cmp" >>"$RUNS"
  [ "$rc" = 0 ] || log "FAILED $label rc=$rc: $(tail -2 "$D/scratch/err.txt" | tr '\n' ' ' | cut -c1-200)"
  [ "$cmp" != DIFFERENT ] || log "DIFFERENT fixture $label"
  pending=$((pending + 1))
  [ $pending -lt 25 ] || flush
}

group_entity() {
  local sc n fixture
  for sc in "$T"/oracle/scenarios/*.txt "$T"/oracle/scenarios/*.txt.gz; do
    [ -e "$sc" ] || continue
    n="$(basename "$sc" .gz)"
    fixture="$T/fixtures/entity_oracle/$n"
    [ -e "$fixture" ] || fixture="$fixture.gz"
    VERIFY="$fixture" job "eo:$n" entity "$ROOT/scripts/entity-oracle.sh" "$sc" -o "$D/scratch/fixture"
  done
}

# Scripts that write committed fixtures run in a throwaway copy of the tree (they find their paths through their own location;
# ref/ and target/ are shared), and the copy's fixtures are compared with the committed ones afterwards.
tree_copy() {
  rm -rf "$COPY"; mkdir -p "$COPY"
  git -C "$ROOT" archive HEAD scripts tools crates/mc-game/tests crates/mc-datafix/tests crates/mc-world/tests Cargo.toml | tar -x -C "$COPY"
  ln -sfn "$ROOT/ref" "$COPY/ref"
  ln -sfn "$ROOT/target" "$COPY/target"
}

tree_compare() {
  local t d
  for t in mc-game mc-datafix mc-world; do
    d="$(diff -rq "$ROOT/crates/$t/tests/fixtures" "$COPY/crates/$t/tests/fixtures" 2>&1 | grep -v '^Only in' || true)"
    [ -z "$d" ] || { log "DIFFERENT fixtures after $1: $d"; echo "$d" >>"$D/fixture-diffs.txt"; }
  done
}

group_scripts() {
  local group=$1 s; shift
  tree_copy
  for s in "$@"; do
    job "$group:$s" "$group" bash "$COPY/scripts/$s"
    tree_compare "$group:$s"
  done
  rm -rf "$COPY"
}

group_menu() {
  local sc n
  tree_copy
  for sc in "$T"/oracle/menu_scripts/*.txt; do
    n="$(basename "$sc" .txt)"
    job "menu:$n" menu bash "$COPY/scripts/menu-oracle.sh" "$n"
  done
  tree_compare menu
  rm -rf "$COPY"
}

group_rust() {
  local crates c
  if [ -n "${CAMPAIGN_RUST_CRATES:-}" ]; then read -ra crates <<<"$CAMPAIGN_RUST_CRATES"
  else mapfile -t crates < <(cd "$ROOT/crates" && find . -path '*/tests/oracle/main.rs' | cut -d/ -f2 | sort); fi
  for c in "${crates[@]}"; do
    job "rust:$c" rust cargo test -p "$c" --test oracle -- --ignored --test-threads=1
  done
}

want=("$@")
[ ${#want[@]} -gt 0 ] || want=(entity menu predicates goldens rust)
for g in "${want[@]}"; do
  log "group $g"
  case "$g" in
    entity) group_entity ;;
    menu) group_menu ;;
    predicates) group_scripts predicates predicate-oracle.sh adv-tree-oracle.sh loot-synthetic-oracle.sh ;;
    goldens)
      group_scripts goldens args-golden.sh enchant-anvil-golden.sh golden-block-behaviour.sh golden-block-placement.sh golden-effects.sh \
        golden-loot-all.sh golden-loot.sh golden-menu.sh golden-recipes.sh golden-stats.sh golden-trades.sh golden-variants.sh ;;
    rust) group_rust ;;
    *) die "unknown group $g" ;;
  esac
  flush
done
log "done: $(wc -l <"$RUNS") jobs, merged into $D/merged.tsv.gz"
