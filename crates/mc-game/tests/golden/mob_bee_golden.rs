//! Goldens of the bee: flower search and pollination, anger and stinging (entity replay), entering and leaving a hive, hive
//! search over the POI manager, crop growth, and the bees' reactions to a hive that is broken, harvested or on fire (block
//! replay: the hives tick their occupants, the crops are dumped). Scenarios: `tests/oracle/scenarios/mob_bee_*.txt`, fixtures:
//! `tests/fixtures/entity_oracle/mob_bee_*.txt` (`scripts/entity-oracle.sh`, `scripts/block-oracle.sh`).
use crate::common::block_scenario;
use crate::common::entity_scenario::run_scenario_test;

#[test]
fn bee_anger_matches_vanilla() {
    run_scenario_test("mob_bee_anger");
}

#[test]
fn bee_grow_crop_matches_vanilla() {
    block_scenario::check("mob_bee_grow_crop");
}

#[test]
fn bee_hive_break_matches_vanilla() {
    block_scenario::check("mob_bee_hive_break");
}

#[test]
fn bee_hive_fire_matches_vanilla() {
    block_scenario::check("mob_bee_hive_fire");
}

#[test]
fn bee_hive_harvest_matches_vanilla() {
    block_scenario::check("mob_bee_hive_harvest");
}

#[test]
fn bee_hive_release_matches_vanilla() {
    block_scenario::check("mob_bee_hive_release");
}

#[test]
fn bee_hive_search_matches_vanilla() {
    block_scenario::check("mob_bee_hive_search");
}

#[test]
fn bee_nectar_hive_matches_vanilla() {
    block_scenario::check("mob_bee_nectar_hive");
}

#[test]
fn bee_pollinate_flower_matches_vanilla() {
    run_scenario_test("mob_bee_pollinate_flower");
}

#[test]
fn bee_sting_die_matches_vanilla() {
    run_scenario_test("mob_bee_sting_die");
}
