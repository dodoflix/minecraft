#!/usr/bin/env bash
# Regenerates the trades golden with the REAL vanilla trade generation (villager_trade/trade_set registries, AbstractVillager.addOffersFromItemListings, Villager/WanderingTrader.getOffers, MerchantOffers codecs)
# through the in-process vanilla oracle (crates/mc-game/tests/oracle/TradesOracle.java).
# Usage:
#   scripts/golden-trades.sh
#
# Output: crates/mc-game/tests/fixtures/trades_golden.txt (replaced only when the oracle succeeds)
# Needs: java, ref/ with decompiled sources (scripts/setup-ref.sh --decompile); MC_ACCEPT_EULA=1 (the oracle starts a vanilla level).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=scripts/lib.sh
. "$ROOT/scripts/lib.sh"
help_flag "$@"
need java
T="$ROOT/crates/mc-game/tests"
to_file "$T/fixtures/trades_golden.txt" "$ROOT/scripts/entity-oracle.sh" "$T/oracle/TradesOracle.java"
