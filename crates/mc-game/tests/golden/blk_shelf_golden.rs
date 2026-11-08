//! Goldens of `ShelfBlock`, `SideChainPartBlock` and `ShelfBlockEntity`: scenarios `tests/oracle/scenarios/blk_shelf_*.txt`
//! replayed on the real vanilla level (`scripts/block-oracle.sh NAME`) vs our ticking level (fixtures
//! `tests/fixtures/entity_oracle/blk_shelf_*.txt`).

use crate::common;

#[test]
fn shelf_power_chains_match_vanilla() {
    common::block_scenario::check("blk_shelf_power_chain");
}

#[test]
fn shelf_block_entity_and_comparator_signal_match_vanilla() {
    common::block_scenario::check("blk_shelf_nbt_signal");
}

#[test]
fn shelf_player_interaction_matches_vanilla() {
    common::block_scenario::check("blk_shelf_player");
}
