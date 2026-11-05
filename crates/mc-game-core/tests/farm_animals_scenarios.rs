//! Farm-animal goldens (pig, cow, sheep, chicken idle/panic; breeding) replayed against the committed vanilla fixtures.
extern crate mc_game_core as mc_game;
#[path = "../../mc-game/tests/common/mod.rs"]
mod common;
use common::entity_scenario::*;

#[test]
#[ignore = "harness: `love`/`breed` commands are not implemented in Harness::run yet"]
fn mob_breed() {
    run_scenario_test("mob_breed");
}

#[test]
#[ignore = "harness: `dump mobs` rows (mobspawn/tick dumps) are not produced by Harness::run yet: the replay starts at the HURT line"]
fn mob_animals_idle() {
    run_scenario_test("mob_animals_idle");
}
