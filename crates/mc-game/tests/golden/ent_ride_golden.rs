//! Riding through the block replay: the rider's own tick (leaving by sneaking, the fall distance, the riding statistics), the
//! start-riding trigger, the authority over controlled vehicles and the head of a mob passenger
//! (`tests/oracle/scenarios/ent_ride_*.txt`; fixtures regenerated with
//! `scripts/block-oracle.sh NAME`).

use crate::common;

// `og` of a player that only sends packets is left out: vanilla keeps the flag the ride set, the replay derives it again on the first
// tick after the dismount (the position, pose and packets are compared)
fn ignore_on_ground() -> common::entity_oracle::Options {
    common::entity_oracle::Options {
        ignore_fields: vec!["og"],
        ..Default::default()
    }
}

#[test]
fn sneaking_rider_leaves_the_boat() {
    common::block_scenario::check_with("ent_ride_boat_sneak", &ignore_on_ground());
}

#[test]
fn sneaking_rider_leaves_the_pig_beside_it() {
    common::block_scenario::check_with("ent_ride_pig_sneak", &ignore_on_ground());
}

#[test]
fn riders_do_not_keep_their_fall_distance() {
    common::block_scenario::check("ent_ride_fall_reset");
}

#[test]
fn riding_distance_counts_per_vehicle_kind() {
    common::block_scenario::check("ent_ride_stats");
}

#[test]
fn starting_to_ride_triggers_the_advancement() {
    common::block_scenario::check("ent_ride_advancement");
}

#[test]
fn controlled_vehicles_are_left_to_the_client() {
    common::block_scenario::check("ent_ride_authority");
}

#[test]
fn boat_passengers_keep_their_head_within_their_own_limit() {
    common::block_scenario::check("ent_ride_boat_mob_head");
}
