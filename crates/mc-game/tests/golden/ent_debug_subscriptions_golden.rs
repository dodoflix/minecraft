//! Debug subscriptions: the `debug/*` packets an operator gets for the brains, goals and paths of the mobs it tracks, the
//! points of interest and village sections, raids and structures of the chunks it tracks, bees and hives, neighbor updates, game
//! events and their listeners, redstone wire orientations and the blocks entities move through
//! (`tests/oracle/scenarios/ent_debug_*.txt`; fixtures regenerated with `scripts/block-oracle.sh NAME`).

use crate::common;

#[test]
fn bee_and_hive_values_match_vanilla() {
    common::block_scenario::check("ent_debug_bee");
}

#[test]
fn brain_values_match_vanilla() {
    common::block_scenario::check("ent_debug_brain");
}

#[test]
fn game_events_and_listeners_match_vanilla() {
    common::block_scenario::check("ent_debug_events");
}

#[test]
fn goal_values_match_vanilla() {
    common::block_scenario::check("ent_debug_goals");
}

#[test]
fn entity_block_intersections_match_vanilla() {
    common::block_scenario::check("ent_debug_intersections");
}

#[test]
fn neighbor_updates_and_wire_orientations_match_vanilla() {
    common::block_scenario::check("ent_debug_neighbors");
}

#[test]
fn path_values_match_vanilla() {
    common::block_scenario::check("ent_debug_path");
}

#[test]
fn poi_and_village_values_match_vanilla() {
    common::block_scenario::check("ent_debug_poi");
}

#[test]
fn worldgen_points_of_interest_match_vanilla() {
    common::block_scenario::check("ent_debug_worldgen_poi");
}

#[test]
fn raid_and_structure_values_match_vanilla() {
    common::block_scenario::check("ent_debug_raids_structures");
}
