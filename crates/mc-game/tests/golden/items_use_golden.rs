//! Item use through the player action verbs (`use`, `select`, `give`, `release`, the dumps `use:`, `cooldowns:`, `stats:`, `inventory:`) against the
//! real vanilla level: scenarios `tests/oracle/scenarios/itm_*.txt`, fixtures from `scripts/block-oracle.sh NAME`.
use crate::common::block_scenario::check;

#[test]
fn use_basic_matches_vanilla() {
    check("itm_use_basic");
}

#[test]
fn cooldowns_match_vanilla() {
    check("itm_cooldown");
}

/// The same scenario through the entity replay: both replays share the player actions and dumps.
#[test]
fn cooldowns_match_vanilla_in_the_entity_replay() {
    crate::common::entity_scenario::run_scenario_test("itm_cooldown");
}

#[test]
fn drop_stats_match_vanilla() {
    crate::common::entity_scenario::run_scenario_test("itm_drop_stats");
}

#[test]
fn tool_break_stats_match_vanilla() {
    check("itm_tool_break");
}
