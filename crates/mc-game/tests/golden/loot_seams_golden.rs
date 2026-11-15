//! Loot functions that reach into the server and the world: `set_name`/`set_lore` resolving selector, score and nbt text with the
//! entity of the loot context, `/loot ... mine` running the block's `getDrops` side effects (a beehive releasing its bees) and
//! `furnace_smelt` asking the recipe manager. Scenarios `tests/oracle/scenarios/loot_*.txt`, fixtures recorded with
//! `scripts/block-oracle.sh NAME`; the feedback (`CF`) and the dumps are compared.

use crate::common;

#[test]
fn set_name_and_set_lore_resolve_text_like_vanilla() {
    common::block_scenario::check("loot_set_name_resolve");
}

#[test]
fn loot_mine_releases_bees_like_vanilla() {
    common::block_scenario::check("loot_mine_beehive");
}

#[test]
fn furnace_smelt_matches_vanilla() {
    common::block_scenario::check("loot_smelt_features");
}
