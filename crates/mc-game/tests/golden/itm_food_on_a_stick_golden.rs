//! `FoodOnAStickItem.use` against vanilla (`tests/oracle/scenarios/itm_food_on_a_stick.txt`; fixture regenerated with
//! `scripts/block-oracle.sh itm_food_on_a_stick`).

use crate::common::entity_scenario::run_scenario_test;

#[test]
fn food_on_a_stick_boosts_steered_pigs_and_striders() {
    run_scenario_test("itm_food_on_a_stick");
}
