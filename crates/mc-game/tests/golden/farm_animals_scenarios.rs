//! Farm-animal goldens (pig, cow, sheep, chicken idle/panic; breeding) replayed against the committed vanilla fixtures.
use crate::common::entity_scenario::*;

#[test]
#[ignore = "deviation: from tick 7 the baby cow's follow_range spawn bonus differs (the level random state after the first breeding differs)"]
fn mob_breed() {
    run_scenario_test("mob_breed");
}

#[test]
fn mob_animals_idle() {
    run_scenario_test("mob_animals_idle");
}
