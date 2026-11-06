#!/usr/bin/env bash
# Regenerates the predicate fixtures from the real vanilla predicate classes (crates/mc-game/tests/oracle/PredicateOracle.java
# over scenarios/predicates/*.txt, run on the in-process vanilla level of scripts/entity-oracle.sh).
# Usage:
#   scripts/predicate-oracle.sh [NAME...]     (default: every scenario)
#
# Output: crates/mc-game/tests/fixtures/predicate_oracle/NAME.txt (each replaced only when the oracle succeeds)
# Needs: java, ref/ with decompiled sources (scripts/setup-ref.sh --decompile); MC_ACCEPT_EULA=1.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need java
T="$ROOT/crates/mc-game/tests"
names=("$@")
if [ ${#names[@]} -eq 0 ]; then
  for f in "$T"/oracle/scenarios/predicates/*.txt; do names+=("$(basename "$f" .txt)"); done
fi
for n in "${names[@]}"; do
  echo "== $n" >&2
  to_file "$T/fixtures/predicate_oracle/$n.txt" "$ROOT/scripts/entity-oracle.sh" "$T/oracle/PredicateOracle.java" "$T/oracle/scenarios/predicates/$n.txt"
  wc -l "$T/fixtures/predicate_oracle/$n.txt" >&2
done
