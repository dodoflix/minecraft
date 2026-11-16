//! Golden tests of the save, unload and reload verbs: `tests/oracle/scenarios/persist_*.txt`, fixtures from `scripts/block-oracle.sh NAME` (the real vanilla server).
use crate::common::block_scenario::check;

/// Entities of a chunk (armor stand, item, leashed cow, minecart, a boat with passengers) are written when the chunk unloads and come back when it loads.
#[test]
fn entities_survive_the_unload_of_their_chunk() {
    check("persist_entities_unload");
}

/// Item frames, a painting and a lead knot keep their item, rotation and attachment through an unload.
#[test]
fn hanging_entities_survive_the_unload_of_their_chunk() {
    check("persist_item_frame");
}

/// Block entities and pending block and fluid ticks are stored with the chunk and run again after the load.
#[test]
fn block_entities_and_scheduled_ticks_survive_the_unload_of_their_chunk() {
    check("persist_block_entities");
}

/// A claimed point of interest stays claimed and a villager keeps its job site and home memories through an unload.
#[test]
fn points_of_interest_and_villager_memories_survive_the_unload_of_their_chunk() {
    check("persist_villager_poi");
}

/// A warden keeps its suspects and their anger through an unload.
#[test]
fn warden_anger_survives_the_unload_of_its_chunk() {
    check("persist_warden_anger");
}

/// A pearl in flight leaves with its owner and is spawned again when the owner joins with the saved data.
#[test]
fn ender_pearl_returns_with_its_owner() {
    check("persist_pearl_owner");
}

/// Forced chunks are saved with their level and unload when the last ticket goes.
#[test]
fn forced_chunks_are_saved_data_of_their_level() {
    check("persist_forceload");
}

/// A player who leaves while riding comes back on the vehicle.
#[test]
#[ignore = "Not implemented: the vehicle and the passengers a rejoining player left on are not spawned again"]
fn player_rejoins_on_the_vehicle_he_left_on() {
    check("persist_rejoin_mounted");
}
