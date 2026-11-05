//! Goldens of the mob spine through its proof mob (`Chicken`): per-tick rich `E` lines (position, delta, yaw, goals, navigation,
//! love/age state, egg timer through `EggLayTime`) against the vanilla oracle. Scenarios: `tests/oracle/scenarios/chicken_*.txt`,
//! fixtures: `tests/fixtures/entity_oracle/chicken_*.txt` (`scripts/entity-oracle.sh`).
use crate::common::entity_scenario::run_scenario_test;

#[test]
fn chicken_idle_matches_vanilla() {
    run_scenario_test("chicken_idle");
}

#[test]
fn chicken_breed_matches_vanilla() {
    run_scenario_test("chicken_breed");
}

#[test]
fn chicken_tempt_matches_vanilla() {
    run_scenario_test("chicken_tempt");
}

#[test]
fn chicken_egg_matches_vanilla() {
    run_scenario_test("chicken_egg");
}
