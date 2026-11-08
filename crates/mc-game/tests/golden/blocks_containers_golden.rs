//! Goldens of the container block entities: removal drops, the jukebox as a `Container`, and the criterion triggers fired by
//! beacons and loot containers. Scenarios `tests/oracle/scenarios/blk_*.txt` replayed on the real vanilla level
//! (`scripts/block-oracle.sh NAME`) vs our ticking level (fixtures `tests/fixtures/entity_oracle/blk_*.txt`).

use crate::common;

#[test]
fn container_block_entities_drop_their_contents_when_broken() {
    common::block_scenario::check("blk_container_break_drops");
}

#[test]
fn hoppers_and_droppers_move_discs_in_and_out_of_a_jukebox() {
    common::block_scenario::check("blk_jukebox_hopper");
}

#[test]
fn an_activated_beacon_fires_construct_beacon_for_players_in_range() {
    common::block_scenario::check("blk_beacon_trigger");
}

#[test]
fn a_player_opening_a_loot_container_fires_generate_loot() {
    common::block_scenario::check("blk_loot_container_trigger");
}

/// Crafting, inserting and dropping the results, the remaining items and the level events 1049/1050/2010.
#[test]
fn a_powered_crafter_crafts_and_dispenses_like_vanilla() {
    common::block_scenario::check("blk_crafter_craft");
}

#[test]
fn crafter_slots_and_saved_state_match_vanilla() {
    common::block_scenario::check("blk_crafter_slots_nbt");
}

#[test]
fn crafter_placement_orientation_matches_vanilla() {
    common::block_scenario::check("blk_crafter_place");
}

#[test]
fn a_dropped_crafter_result_fires_crafter_recipe_crafted_for_players_in_range() {
    common::block_scenario::check("blk_crafter_trigger");
}
