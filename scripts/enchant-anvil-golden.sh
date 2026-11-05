#!/usr/bin/env bash
# Regenerates crates/mc-game/tests/fixtures/enchant_anvil/{enchant,anvil,grindstone}.txt.gz from the REAL vanilla
# EnchantmentMenu / AnvilMenu / GrindstoneMenu (crates/mc-game/tests/oracle/EnchantAnvilOracle.java, see its header).
#   scripts/enchant-anvil-golden.sh [enchant|anvil|grindstone|all]
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
F="$ROOT/crates/mc-game/tests/fixtures/enchant_anvil"
mkdir -p "$F"
for section in $( [ "${1:-all}" = all ] && echo enchant anvil grindstone || echo "$1" ); do
  "$ROOT/scripts/entity-oracle.sh" "$ROOT/crates/mc-game/tests/oracle/EnchantAnvilOracle.java" "$section" 2>/dev/null | gzip -9 > "$F/$section.txt.gz.tmp"
  mv "$F/$section.txt.gz.tmp" "$F/$section.txt.gz"
  ls -l "$F/$section.txt.gz" >&2
done
