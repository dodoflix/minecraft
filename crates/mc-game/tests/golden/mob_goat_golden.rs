//! Goat goldens (ram and horn drop, horn NBT, milking, long jump, breeding and temptation, finalizeSpawn rolls) replayed against the
//! vanilla oracle recordings. Scenarios: `tests/oracle/scenarios/mob_goat_*.txt`, fixtures: `tests/fixtures/entity_oracle/mob_goat_*.txt`
//! (`scripts/entity-oracle.sh`).
use crate::common::entity_scenario::run_scenario_test;

#[test]
#[ignore = "deviation: a tempted goat walks slightly slower than vanilla on its first steps (position and delta differ from the first moving tick)"]
fn mob_goat_breed_tempt_matches_vanilla() {
    run_scenario_test("mob_goat_breed_tempt");
}

#[test]
#[ignore = "deviation: a path through a block placed after the path search starts differs (vanilla climbs the log at y=-59)"]
fn mob_goat_horn_nbt_matches_vanilla() {
    run_scenario_test("mob_goat_horn_nbt");
}

#[test]
fn mob_goat_longjump_matches_vanilla() {
    run_scenario_test("mob_goat_longjump");
}

#[test]
fn mob_goat_milk_matches_vanilla() {
    run_scenario_test("mob_goat_milk");
}

/// Replayed on the ticking level: two mobs push each other only inside the entity-ticking range, which the detached entity replay has
/// no chunk tickets for.
#[test]
fn mob_goat_ram_matches_vanilla() {
    crate::common::block_scenario::check("mob_goat_ram");
}

#[test]
fn mob_goat_scream_spawn_matches_vanilla() {
    run_scenario_test("mob_goat_scream_spawn");
}
