//! The entity verbs of the block replay (`tests/oracle/scenarios/ent_verbs.txt`; fixtures regenerated with
//! `scripts/block-oracle.sh NAME`).

use crate::common;

#[test]
fn entity_verbs_match_vanilla() {
    common::block_scenario::check("ent_verbs");
}

#[test]
fn respawn_matches_vanilla() {
    common::block_scenario::check("ent_respawn");
}

#[test]
fn spawned_player_yaw_matches_vanilla() {
    common::block_scenario::check("ent_spawn_yaw");
}

#[test]
fn sneaking_click_resets_with_the_next_tick_like_vanilla() {
    common::block_scenario::check("ent_interact_sneak");
}

#[test]
fn removed_entities_dump_like_vanilla() {
    common::block_scenario::check("ent_removed_dump");
}

#[test]
fn removed_entities_keep_their_names_in_dumps() {
    common::block_scenario::check("blk_removed_entity_names");
}

#[test]
fn chat_and_command_output_reach_the_players() {
    common::block_scenario::check("blk_chat_command_output");
}

#[test]
fn a_player_joining_in_flowing_water_is_pushed_from_the_first_tick() {
    common::block_scenario::check("blk_join_in_flowing_water");
}

#[test]
fn tp_resets_motion_dismounts_and_turns_in_the_block_replay() {
    common::block_scenario::check("blk_tp_resets_motion");
}

#[test]
fn tp_resets_motion_and_turns_in_the_entity_replay() {
    common::block_scenario::check("ent_tp_resets_motion");
    common::entity_scenario::run_scenario_test("ent_tp_resets_motion");
}

#[test]
fn a_kicked_player_leaves_the_level_and_the_player_list() {
    common::block_scenario::check("blk_disconnect_removes_player");
}
