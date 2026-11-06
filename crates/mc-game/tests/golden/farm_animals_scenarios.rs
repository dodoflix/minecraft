//! Farm-animal goldens (pig, cow, sheep, chicken idle/panic; breeding) replayed against the committed vanilla fixtures.
use crate::common::entity_scenario::*;

#[test]
#[ignore = "deviation: the scenario replay lacks the love and breed commands (#7)"]
fn mob_breed() {
    run_scenario_test("mob_breed");
}

#[test]
#[ignore = "deviation: the scenario replay lacks the dump mobs rows (#7)"]
fn mob_animals_idle() {
    run_scenario_test("mob_animals_idle");
}
