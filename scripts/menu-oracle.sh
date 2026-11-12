#!/usr/bin/env bash
# Records an in-process menu oracle script against the REAL vanilla menus (crates/mc-game/tests/oracle/MenuOracleBase.java, see its
# header for the ops).
# Usage:
#   scripts/menu-oracle.sh NAME     runs crates/mc-game/tests/oracle/menu_scripts/NAME.txt
#   scripts/menu-oracle.sh all      every script
#
# Output: crates/mc-game/tests/fixtures/menu_oracle/NAME.txt.gz (replaced only when the oracle succeeded)
# Needs: java, gzip, ref/ with decompiled sources (scripts/setup-ref.sh --decompile); MC_ACCEPT_EULA=1.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need java gzip
[ $# -eq 1 ] || { usage >&2; exit 2; }
T="$ROOT/crates/mc-game/tests"
if [ "$1" = all ]; then
  for script in "$T"/oracle/menu_scripts/*.txt; do
    "$0" "$(basename "$script" .txt)"
  done
  exit 0
fi
script="$T/oracle/menu_scripts/$1.txt"
[ -f "$script" ] || die "no script $script"
to_file "$T/fixtures/menu_oracle/$1.txt.gz" quiet "$ROOT/scripts/entity-oracle.sh" "$T/oracle/MenuOracleBase.java" "$script"
ls -l "$T/fixtures/menu_oracle/$1.txt.gz" >&2
