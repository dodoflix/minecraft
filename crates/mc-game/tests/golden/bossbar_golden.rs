//! Custom boss bars against the real vanilla level: the `/bossbar` subcommands and their feedback, the `boss_event` packets of the players
//! a bar tracks, joining, leaving and respawning, and the saved form (`sb_bossbar_*` block-tier scenarios, fixtures recorded with
//! `scripts/block-oracle.sh NAME`).

use crate::common;

#[test]
fn bossbar_commands_and_feedback_match_vanilla() {
    common::block_scenario::check("sb_bossbar_commands");
}

#[test]
fn bossbar_players_and_packets_match_vanilla() {
    common::block_scenario::check("sb_bossbar_players");
}

#[test]
fn bossbar_joining_leaving_and_respawning_match_vanilla() {
    common::block_scenario::check("sb_bossbar_session");
}

#[test]
fn bossbar_saved_form_and_loaded_bars_match_vanilla() {
    common::block_scenario::check("sb_bossbar_saved");
}
