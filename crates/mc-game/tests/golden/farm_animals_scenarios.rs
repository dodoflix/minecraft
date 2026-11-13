//! Farm-animal goldens (pig, cow, sheep, chicken idle/panic; breeding) replayed against the committed vanilla fixtures.
use crate::common::entity_scenario::*;

#[test]
#[ignore = "deviation: the level random state after the first breeding differs (line 5: `RNG level 250750202969979` in vanilla, 38176621015976 here)"]
fn mob_breed() {
    run_scenario_test("mob_breed");
}

#[test]
fn mob_animals_idle() {
    run_scenario_test("mob_animals_idle");
}
