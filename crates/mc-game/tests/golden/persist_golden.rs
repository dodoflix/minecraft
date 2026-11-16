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
fn player_rejoins_on_the_vehicle_he_left_on() {
    check("persist_rejoin_mounted");
}

/// A player who leaves after the death screen has been up for a while is still saved, and joins again from the saved dead state.
#[test]
fn player_leaves_and_rejoins_while_dead() {
    check("persist_leave_dead");
}

/// A populated player (inventory with components, effects, experience, ender chest, abilities, respawn point, recipe book, shoulder entity) keeps all of it through leave, join and leave.
#[test]
fn populated_player_data_round_trips() {
    check("persist_player_populated");
}

/// The statistics and progress files are written by a world save and when the player leaves, not while the player plays.
#[test]
fn statistics_and_progress_files_are_written_on_save_and_leave() {
    check("persist_stats_files");
}
