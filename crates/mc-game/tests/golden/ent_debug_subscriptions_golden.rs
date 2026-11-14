//! Debug subscriptions: the `debug/*` packets an operator gets for the brains, goals and paths of the mobs it tracks, the
//! points of interest and village sections of the chunks it tracks, and bees and hives
//! (`tests/oracle/scenarios/ent_debug_*.txt`; fixtures regenerated with `scripts/block-oracle.sh NAME`).

use crate::common;

#[test]
fn bee_and_hive_values_match_vanilla() {
    common::block_scenario::check("ent_debug_bee");
}

#[test]
fn brain_values_match_vanilla() {
    common::block_scenario::check("ent_debug_brain");
}

#[test]
fn goal_values_match_vanilla() {
    common::block_scenario::check("ent_debug_goals");
}

#[test]
fn path_values_match_vanilla() {
    common::block_scenario::check("ent_debug_path");
}

#[test]
fn poi_and_village_values_match_vanilla() {
    common::block_scenario::check("ent_debug_poi");
}
