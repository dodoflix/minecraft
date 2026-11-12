//! `DyeItem.interactLivingEntity` on sheep against vanilla (`tests/oracle/scenarios/itm_dye_sheep.txt`; fixture regenerated with
//! `scripts/entity-oracle.sh`).

use crate::common;

#[test]
fn dyes_apply_to_live_unsheared_sheep_of_another_colour() {
    common::entity_scenario::run_scenario_test("itm_dye_sheep");
}
