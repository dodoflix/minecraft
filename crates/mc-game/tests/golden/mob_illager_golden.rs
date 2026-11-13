//! Pillager against the vanilla oracle (entity scenarios in `oracle/scenarios`): the crossbow charge and shot at a player.
use crate::common::entity_scenario::run_scenario_test;

#[test]
fn pillager_crossbow_volleys() {
    run_scenario_test("mob_pillager_crossbow");
}
