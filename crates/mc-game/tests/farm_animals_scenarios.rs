//! Farm-animal goldens (pig, cow, sheep, chicken idle/panic; breeding) replayed against the committed vanilla fixtures.
mod common;
use common::entity_scenario::*;

#[test]
fn mob_breed() {
    run_scenario_test("mob_breed");
}

#[test]
fn mob_animals_idle() {
    run_scenario_test("mob_animals_idle");
}
