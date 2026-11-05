#!/usr/bin/env bash
# Regenerates crates/mc-game/tests/fixtures/predicate_oracle/*.txt from the real vanilla predicate classes
# (crates/mc-game/tests/oracle/PredicateOracle.java over scenarios/predicates/*.txt, run on the in-process vanilla level of
# scripts/entity-oracle.sh).  Usage: scripts/predicate-oracle.sh [NAME...]   (default: every scenario)
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
T="$ROOT/crates/mc-game/tests"
names=("$@")
if [ ${#names[@]} -eq 0 ]; then
  for f in "$T"/oracle/scenarios/predicates/*.txt; do names+=("$(basename "$f" .txt)"); done
fi
mkdir -p "$T/fixtures/predicate_oracle"
for n in "${names[@]}"; do
  echo "== $n" >&2
  tmp="$(mktemp)"
  "$ROOT/scripts/entity-oracle.sh" "$T/oracle/PredicateOracle.java" "$T/oracle/scenarios/predicates/$n.txt" > "$tmp"
  mv "$tmp" "$T/fixtures/predicate_oracle/$n.txt"
  wc -l "$T/fixtures/predicate_oracle/$n.txt" >&2
done
