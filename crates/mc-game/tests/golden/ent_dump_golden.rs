//! The rich entity dump (`xentities`) and the synched data dump (`edata:`) of every non-mob entity class, in both replays
//! (`tests/oracle/scenarios/ent_dump_smoke.txt`; fixtures regenerated with `scripts/block-oracle.sh NAME`).

use crate::common;

#[test]
fn every_class_dumps_like_vanilla() {
    common::block_scenario::check("ent_dump_smoke");
}

#[test]
fn entity_replay_dumps_like_vanilla() {
    common::entity_scenario::run_scenario_test("ent_dump_smoke");
}

#[test]
fn level_and_player_random_streams_stay_on_the_vanilla_sequence() {
    common::block_scenario::check("ent_rng_level");
}

#[test]
fn base_save_data_and_passengers_round_trip_like_vanilla() {
    common::block_scenario::check("ent_passengers_base_nbt");
}

/// Scenario names with one Java hashCode ("FR", "G3") share the entity seeds and so one UUID: the level refuses the second pig, and
/// both replays still dump it by its name.
#[test]
fn names_with_one_hash_code_both_dump() {
    common::block_scenario::check("ent_same_hash_names");
    common::entity_scenario::run_scenario_test("ent_same_hash_names");
}
