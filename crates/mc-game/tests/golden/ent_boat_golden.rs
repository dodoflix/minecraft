//! Boats against vanilla: boarding and ejection under water, passengers, hits and breaking, chest boats, dismounting, bubble
//! columns, the inventory key, lily pads, every boat type, leads, and the `container.N` slots of chest vehicles
//! (`tests/oracle/scenarios/ent_boat_*.txt`, `ent_chest_*.txt`; fixtures regenerated with `scripts/block-oracle.sh NAME`).

use crate::common;

#[test]
fn boats_under_water_throw_their_passengers_off() {
    common::block_scenario::check("ent_boat_board");
}

#[test]
fn boats_take_overlapping_animals_as_passengers() {
    common::block_scenario::check("ent_boat_passengers");
}

#[test]
fn hitting_and_breaking_boats() {
    common::block_scenario::check("ent_boat_hurt");
}

#[test]
fn breaking_chest_boats_drops_the_contents_and_angers_piglins() {
    common::block_scenario::check("ent_chest_boat_destroy");
}

#[test]
fn leaving_a_boat_finds_the_side_to_step_on() {
    common::block_scenario::check("ent_boat_dismount");
}

#[test]
fn bubble_columns_bump_boats_and_throw_riders_off() {
    common::block_scenario::check("ent_boat_bubble_ride");
}

#[test]
fn the_inventory_key_opens_the_chest_boat_of_a_rider() {
    common::block_scenario::check("ent_boat_open_inventory");
}

#[test]
fn boats_destroy_lily_pads() {
    common::block_scenario::check("ent_boat_lily");
}

#[test]
fn every_boat_type_spawns_saves_and_drops_its_item() {
    common::block_scenario::check("ent_boat_types");
}

#[test]
fn boats_take_leads() {
    common::block_scenario::check("ent_boat_leash");
}

#[test]
fn chest_vehicle_slots_through_item_commands() {
    common::block_scenario::check("ent_chest_vehicle_slots");
}
