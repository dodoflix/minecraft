#!/usr/bin/env bash
# Regenerates the advancement tree fixtures from the real vanilla classes (crates/mc-game/tests/oracle/AdvTreeOracle.java over
# scenarios/adv_tree/*.txt, run on the in-process vanilla level of scripts/entity-oracle.sh).
# Usage:
#   scripts/adv-tree-oracle.sh [NAME...]     (default: every scenario)
#
# Output: crates/mc-game/tests/fixtures/adv_tree/NAME.txt (each replaced only when the oracle succeeds)
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
  for f in "$T"/oracle/scenarios/adv_tree/*.txt; do names+=("$(basename "$f" .txt)"); done
fi
for n in "${names[@]}"; do
  echo "== $n" >&2
  to_file "$T/fixtures/adv_tree/$n.txt" "$ROOT/scripts/entity-oracle.sh" "$T/oracle/AdvTreeOracle.java" "$T/oracle/scenarios/adv_tree/$n.txt"
  wc -l "$T/fixtures/adv_tree/$n.txt" >&2
done
