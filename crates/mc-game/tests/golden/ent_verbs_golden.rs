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
