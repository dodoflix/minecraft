//! The shulker against the vanilla oracle (scenarios in `oracle/scenarios`): peeking and attacking, bullets, teleporting, the lid as a push and a
//! solid, the copy a bullet hit leaves behind, the colour and the shulkers of a generated end city.
use crate::common::block_scenario;

#[test]
fn shulker_peeks_attacks_and_closes() {
    block_scenario::check("mob_shulker_peek_attack");
}

#[test]
fn shulker_bullets_home_and_levitate() {
    block_scenario::check("mob_shulker_bullet");
}

#[test]
fn shulker_teleports_when_it_loses_its_surface_or_is_hurt() {
    block_scenario::check("mob_shulker_teleport");
}

#[test]
fn shulker_lid_pushes_and_carries_entities() {
    block_scenario::check("mob_shulker_push");
}

#[test]
fn shulker_bullet_hit_teleports_and_duplicates() {
    block_scenario::check("mob_shulker_replicate");
}

#[test]
fn shulker_colour_from_spawn_egg_component_and_saved_data() {
    block_scenario::check("mob_shulker_color");
}

#[test]
fn placed_end_city_creates_its_shulkers_and_item_frame() {
    block_scenario::check("mob_shulker_end_city");
}

/// A teleporting shulker posts the teleport game event at the centre of the block it left: the sculk sensor vibration starts there.
#[test]
fn shulker_teleport_vibration() {
    block_scenario::check("mob_shulker_teleport_vibration");
}
