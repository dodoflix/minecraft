#!/usr/bin/env bash
# Regenerates crates/mc-game/tests/fixtures/trades_golden.txt with the REAL vanilla trade generation (villager_trade/trade_set
# registries, AbstractVillager.addOffersFromItemListings, Villager/WanderingTrader.getOffers, MerchantOffers codecs) through the
# in-process vanilla oracle (tests/oracle/TradesOracle.java).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
"$ROOT/scripts/entity-oracle.sh" "$ROOT/crates/mc-game/tests/oracle/TradesOracle.java" > "$ROOT/crates/mc-game/tests/fixtures/trades_golden.txt"
