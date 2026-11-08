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
