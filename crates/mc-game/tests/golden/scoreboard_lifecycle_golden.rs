//! Scoreboard state against the real vanilla level: scores and teams of removed mobs, the objectives a kill, a death and a
//! disconnect move, the objectives that follow a player's state, and what a team changes for its members (`sb_*` block-tier
//! scenarios, fixtures recorded with `scripts/block-oracle.sh NAME`).

use crate::common;

#[test]
fn removed_mobs_lose_their_scores_and_team() {
    common::block_scenario::check("sb_entity_removed");
}

#[test]
fn kill_death_and_disconnect_criteria_match_vanilla() {
    common::block_scenario::check("sb_kill_criteria");
}

#[test]
fn player_state_criteria_match_vanilla() {
    common::block_scenario::check("sb_player_criteria");
}

#[test]
fn team_death_message_friendly_fire_and_collision_rules_match_vanilla() {
    common::block_scenario::check("sb_team_visibility");
}

#[test]
fn objective_formats_displays_and_operations_match_vanilla() {
    common::block_scenario::check("sb_formats");
}

#[test]
fn team_subcommands_options_and_chat_match_vanilla() {
    common::block_scenario::check("cmd_team_edges");
}
