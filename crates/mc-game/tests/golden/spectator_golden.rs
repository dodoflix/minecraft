//! Spectators and riders against the real vanilla level (`tests/oracle/scenarios/spectat*.txt`, `ride_vehicle_effects_packets.txt`;
//! fixtures regenerated with `scripts/block-oracle.sh NAME`).

use crate::common;

/// `ServerPlayer.teleportTo(level, ..)` wakes a sleeper before it moves.
#[test]
fn spectator_teleport_to_entity_wakes_a_sleeper() {
    common::block_scenario::check("spectator_tp_to_entity_sleeping");
}

/// An entity of another level as the camera moves the spectator into that level.
#[test]
fn spectating_an_entity_of_another_level() {
    common::block_scenario::check("spectate_other_dimension");
}

/// A spectator follows its camera entity through a portal.
#[test]
fn spectators_follow_a_travelling_entity() {
    common::block_scenario::check("spectator_follow_travelling_entity");
}

/// Mounting sends the effects of a living vehicle, dismounting removes them.
#[test]
fn riding_sends_the_effects_of_the_vehicle() {
    common::block_scenario::check("ride_vehicle_effects_packets");
}

/// A spectator right-clicking a nether portal block is teleported through it.
#[test]
fn spectators_teleport_through_a_portal_block() {
    common::block_scenario::check("spectator_portal_teleport");
}

/// Teleporting to an entity of another level resets the camera, wakes a sleeper and changes level.
#[test]
fn spectators_teleport_to_an_entity_of_another_level() {
    common::block_scenario::check("spectator_tp_to_entity_dimension");
}

/// The rider of a vehicle that travels through a portal is told to drop the old effects and gets the new vehicle's.
#[test]
fn a_rider_hears_of_the_effects_when_its_vehicle_travels() {
    common::block_scenario::check("ride_vehicle_travel_effects");
}
