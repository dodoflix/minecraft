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
