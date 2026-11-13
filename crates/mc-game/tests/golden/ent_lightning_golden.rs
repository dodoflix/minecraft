//! Lightning replayed against the real vanilla level (`scripts/block-oracle.sh NAME`): the fire a bolt lights on hard, what it does to
//! the mobs around it, the strike trigger, and where a thunderstorm aims its bolts (lightning rods, the trap skeleton horse).

use crate::common;

#[test]
fn bolt_on_hard_lights_fires_and_hurts_what_is_near() {
    common::block_scenario::check("ent_lightning_hard");
}

#[test]
fn bolt_converts_and_charges_mobs() {
    common::block_scenario::check("ent_lightning_mobs");
}

#[test]
fn strike_trigger_needs_a_bystander_villager_and_no_fire() {
    common::block_scenario::check("ent_lightning_advancement");
}

#[test]
fn thunderstorm_strikes_the_closest_open_lightning_rod() {
    common::block_scenario::check("ent_thunder_rod");
}

#[test]
fn thunderstorm_trap_spawns_a_skeleton_horse_before_the_bolt() {
    common::block_scenario::check("ent_thunder_trap");
}

#[test]
fn channeling_trident_strikes_with_its_thrower_as_cause() {
    common::block_scenario::check("ent_lightning_channeling");
}
