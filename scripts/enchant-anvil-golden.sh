#!/usr/bin/env bash
# Regenerates the enchanting table, anvil and grindstone goldens from the REAL vanilla EnchantmentMenu / AnvilMenu /
# GrindstoneMenu (crates/mc-game/tests/oracle/EnchantAnvilOracle.java, see its header).
# Usage:
#   scripts/enchant-anvil-golden.sh [enchant|anvil|grindstone|all]
#
# Output: crates/mc-game/tests/fixtures/enchant_anvil/{enchant,anvil,grindstone}.txt.gz (each replaced only when the oracle
# succeeds; over 1 MB uncompressed, so they stay gzip'd)
# Needs: java, gzip, ref/ with decompiled sources (scripts/setup-ref.sh --decompile); MC_ACCEPT_EULA=1.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need java gzip
F="$ROOT/crates/mc-game/tests/fixtures/enchant_anvil"
case "${1:-all}" in
  all) sections=(enchant anvil grindstone) ;;
  enchant | anvil | grindstone) sections=("$1") ;;
  *) die "unknown section: $1 (enchant|anvil|grindstone|all)" ;;
esac
for section in "${sections[@]}"; do
  to_file "$F/$section.txt.gz" quiet "$ROOT/scripts/entity-oracle.sh" "$ROOT/crates/mc-game/tests/oracle/EnchantAnvilOracle.java" "$section"
  ls -l "$F/$section.txt.gz" >&2
done
