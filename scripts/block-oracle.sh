#!/usr/bin/env bash
# Regenerates block-oracle fixtures from the REAL vanilla level (wrapper of scripts/entity-oracle.sh).
# Usage:
#   scripts/block-oracle.sh NAME...     crates/mc-game/tests/oracle/scenarios/NAME.txt -> crates/mc-game/tests/fixtures/entity_oracle/NAME.txt
#
# Output: crates/mc-game/tests/fixtures/entity_oracle/NAME.txt (each replaced only when the oracle succeeds)
# The Rust replay is crates/mc-game/tests/common/block_scenario.rs (`common::block_scenario::check("NAME")`).
# Needs: java, ref/ with decompiled sources (scripts/setup-ref.sh --decompile); MC_ACCEPT_EULA=1.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need java
T="$ROOT/crates/mc-game/tests"
[ $# -ge 1 ] || { usage >&2; exit 2; }
for n in "$@"; do
  n="${n%.txt}"
  echo "== $n" >&2
  "$ROOT/scripts/entity-oracle.sh" "$T/oracle/scenarios/$n.txt" -o "$T/fixtures/entity_oracle/$n.txt"
done
