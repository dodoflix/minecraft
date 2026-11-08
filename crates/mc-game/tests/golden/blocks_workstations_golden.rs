//! Campfire cooking, enchanting table and the anvil/grindstone menu hooks: scenarios
//! `tests/oracle/scenarios/blk_{fbc_campfire,campfire_cook,enchanting_table_nbt,workstation_menus}.txt` replayed on the
//! real vanilla level (`scripts/block-oracle.sh NAME`) vs our ticking level (fixtures `tests/fixtures/entity_oracle/`).

use crate::common;

#[test]
fn campfire_block_states_match_vanilla() {
    common::block_scenario::check("blk_fbc_campfire");
}

#[test]
fn campfire_cooking_matches_vanilla() {
    common::block_scenario::check("blk_campfire_cook");
}

#[test]
fn enchanting_table_block_entity_matches_vanilla() {
    common::block_scenario::check("blk_enchanting_table_nbt");
}

#[test]
fn workstation_menus_open_like_vanilla() {
    common::block_scenario::check("blk_workstation_menus");
}
