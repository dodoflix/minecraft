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
fn conversion_common() {
    check("mob_conversion_common");
}

/// A baby grows up squeezed under a ceiling, beside a wall and in a corner: `Entity.refreshDimensions` pushes it out of the blocks.
#[test]
fn baby_grows_against_blocks() {
    check("mob_baby_grow_wall");
}
