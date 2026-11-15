//! Goat goldens (ram and horn drop, horn NBT, milking, long jump, breeding and temptation, finalizeSpawn rolls) replayed against the
//! vanilla oracle recordings. Scenarios: `tests/oracle/scenarios/mob_goat_*.txt`, fixtures: `tests/fixtures/entity_oracle/mob_goat_*.txt`
//! (`scripts/entity-oracle.sh`).
use crate::common::block_scenario::check;
use crate::common::entity_scenario::run_scenario_test;

#[test]
fn mob_goat_breed_tempt_matches_vanilla() {
    crate::common::block_scenario::check("mob_goat_breed_tempt");
}

/// Goats spawned in one group: a calf reaches the baby boundary during `finalizeSpawn` and has the attack damage of a baby.
#[test]
fn mob_goat_baby_group_matches_vanilla() {
    check("mob_goat_baby_group");
}

/// A goat or hoglin loaded with a negative Age (summon, data merge) gets the attack damage of a calf while the data loads.
#[test]
fn mob_summon_age_stats_matches_vanilla() {
    check("mob_summon_age_stats");
}

/// Replayed by the block replay: the vanilla level shuffles its ticking chunks with `Level.random` every tick, which the goats' ram
/// cooldown draws from.
#[test]
fn mob_goat_horn_nbt_matches_vanilla() {
    check("mob_goat_horn_nbt");
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
