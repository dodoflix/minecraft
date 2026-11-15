//! The command verbs of the scenario replays (`cmd`, `cmd_as`, `cmd_json`, the entity replay's `command_env`) against the real vanilla
//! level: the feedback lines (`CF`) of console and player sources, permission levels and the dumps of the scoreboard, teams, boss bars,
//! command storage and selectors. Scenarios `tests/oracle/scenarios/NAME.txt`, fixtures recorded with `scripts/block-oracle.sh NAME`
//! (`scripts/entity-oracle.sh` for the entity replay).

use crate::common;

#[test]
fn console_command_feedback_matches_vanilla() {
    common::block_scenario::check("cmd_feedback_smoke");
}

#[test]
fn permission_levels_of_console_and_player_sources_match_vanilla() {
    common::block_scenario::check("cmd_perm_levels");
}

#[test]
fn entity_replay_runs_real_commands() {
    common::entity_scenario::run_scenario_test("ent_cmd_arms");
}

#[test]
fn scoreboard_bossbar_storage_and_selector_dumps_match_vanilla() {
    common::block_scenario::check("dump_smoke");
}
