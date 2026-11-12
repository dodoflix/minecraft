//! Items used on entities against vanilla (`tests/oracle/scenarios/*.txt`, fixtures from `scripts/entity-oracle.sh`): leashing,
//! taming, name tags, equippable items on mobs and shears on entities.

use crate::common::entity_scenario::run_scenario_test;

#[test]
fn leashed_cow_snaps_and_drops_the_lead() {
    run_scenario_test("mob_leash");
}

#[test]
fn bones_tame_a_wolf() {
    run_scenario_test("mob_tame_wolf");
}

#[test]
fn name_tags_match_vanilla() {
    run_scenario_test("itm_name_tag");
}

#[test]
fn equippable_items_on_mobs_match_vanilla() {
    run_scenario_test("itm_equippable_interact");
}

#[test]
fn shears_on_entities_match_vanilla() {
    run_scenario_test("itm_shears_entities");
}
