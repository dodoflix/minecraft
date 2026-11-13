//! The vehicle packets of the listener and what the trackers send for driven vehicles: accepted moves, moves refused as too fast or
//! wrong, the floating kick scaled by gravity, invalid values, teleports while riding, and the pairing and movement packets of
//! boats, carts and pigs (`tests/oracle/scenarios/ent_vehicle_*.txt`; fixtures regenerated with `scripts/block-oracle.sh NAME`).

use crate::common;

#[test]
fn driving_a_boat_with_movement_packets() {
    common::block_scenario::check("ent_vehicle_move_ok");
}

#[test]
fn moves_beyond_the_speed_limit_are_refused() {
    common::block_scenario::check("ent_vehicle_fast");
}

#[test]
fn moves_the_server_does_not_reproduce_are_undone() {
    common::block_scenario::check("ent_vehicle_wrong");
}

#[test]
fn drivers_of_floating_vehicles_are_kicked() {
    common::block_scenario::check("ent_vehicle_float");
}

#[test]
fn invalid_vehicle_values_disconnect_the_driver() {
    common::block_scenario::check("ent_vehicle_invalid");
}

#[test]
fn driven_vehicles_are_tracked_with_their_own_intervals() {
    common::block_scenario::check("ent_vehicle_tracking");
}

#[test]
fn teleporting_a_boat_or_its_driver() {
    common::block_scenario::check("ent_vehicle_tp");
}
