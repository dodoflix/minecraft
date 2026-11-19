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

/// A player who leaves is disconnected first: passengers are ejected and a sleeper is awake in the saved data.
#[test]
fn leaving_ejects_passengers_and_wakes_sleepers() {
    check("persist_leave_disconnect");
}

/// The shape update of a block next to an unloaded chunk loads it at once: the fence saved there connects to the new fence.
#[test]
fn shape_update_across_a_chunk_border_loads_the_neighbour_chunk() {
    check("persist_shape_update_chunk_load");
}

/// A player who left in the nether or in the End logs in again into that level, with the packets of a join.
#[test]
fn rejoin_logs_into_the_saved_level() {
    check("persist_rejoin_other_levels");
}

/// `/setblock` computes the state of the placed block from its neighbours: a neighbour saved in an unloaded chunk is read from disk and connects.
#[test]
fn setblock_reads_the_neighbour_across_a_chunk_border_from_disk() {
    check("persist_setblock_neighbour_load");
}

/// `/fill` places its blocks like `/setblock`: the last one connects to the neighbour saved in an unloaded chunk.
#[test]
fn fill_reads_the_neighbour_across_a_chunk_border_from_disk() {
    check("persist_fill_neighbour_load");
}

/// A redstone wire whose shape is updated by a new wire reads its other neighbours, and connects to the wire saved in an unloaded chunk.
#[test]
fn wire_shape_update_loads_the_chunk_across_the_border() {
    check("persist_wire_border_load");
}

/// A new redstone wire takes its power through a conductor from a lever saved in an unloaded chunk two blocks away.
#[test]
fn wire_power_reads_two_blocks_across_a_chunk_border() {
    check("persist_wire_power_load");
}

/// A flushed save in front of the unload of a chunk with entities, in the nether and the End.
#[test]
fn unload_after_a_flushed_save_with_entities_in_other_levels() {
    check("persist_save_flush_levels");
}

/// Forced chunks of all three levels unload in one tick, with and without a flushed save in front.
#[test]
fn unload_takes_one_tick_in_every_level() {
    check("persist_unload_ticks_levels");
}

/// A fire in a portal frame that crosses a chunk border scans the half saved in an unloaded chunk and lights the portal.
#[test]
fn portal_lit_across_an_unloaded_chunk_border() {
    check("persist_portal_border_load");
}

/// A game event posts to the chunks around it with `getChunkNow`, which caches them: a block read of such a chunk in the same tick adds no ticket, so the chunk unloads a tick sooner.
#[test]
fn game_event_posting_leaves_the_chunks_it_visited_in_the_chunk_cache() {
    check("persist_chunk_now_cache");
}

/// A vibration that arrives at a sculk sensor looks at the chunks around it with `getChunkNow`, which caches them: the ticks until its chunk unloads depend on it.
#[test]
fn vibration_arrival_leaves_the_chunks_it_visited_in_the_chunk_cache() {
    check("persist_vibration_chunk_cache");
}
