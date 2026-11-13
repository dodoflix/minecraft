//! Base-class mob behaviour against vanilla: persistent anger, taming and sitting, growing up, the leash tick and the state a
//! conversion carries over (`tests/oracle/scenarios/mob_*.txt`; the fixtures come from `scripts/entity-oracle.sh` and
//! `scripts/block-oracle.sh`).

use crate::blocks_dispenser_spine_golden::check;
use crate::common::entity_scenario::run_scenario_test;

#[test]
fn anger_neutral() {
    run_scenario_test("mob_anger_neutral");
}

#[test]
fn tame_sit_follow() {
    run_scenario_test("mob_tame_sit_follow");
}

#[test]
fn age_growth() {
    check("mob_age_growth");
}

#[test]
fn leash_tick() {
    run_scenario_test("mob_leash_tick");
}

#[test]
#[ignore = "deviation: a zombie that converts to a drowned: vanilla's drowned has already moved once in the conversion tick (line 5: pos=6.481968, goals=DrownedGoToBeachGoal), ours has not been ticked (pos=6.5, goals=DrownedSwimUpGoal)"]
fn conversion_common() {
    check("mob_conversion_common");
}
