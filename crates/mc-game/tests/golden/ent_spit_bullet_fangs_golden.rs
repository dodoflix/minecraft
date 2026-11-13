//! Llama spit, shulker bullets, evoker fangs and firework rockets replayed against the real vanilla level
//! (`scripts/block-oracle.sh NAME`): their entity data and save data, the packets that pair and move them, and what they do to
//! the entities and blocks they meet.

use crate::common;

#[test]
fn llama_spit_flight_hits_and_ends() {
    common::block_scenario::check("ent_llama_spit");
}

#[test]
fn evoker_fangs_warmup_damage_and_life() {
    common::block_scenario::check("ent_evoker_fangs");
}

#[test]
fn shulker_bullet_homing_hits_and_removal() {
    common::block_scenario::check("ent_shulker_bullet");
}

#[test]
fn firework_explosion_damage_by_distance_and_sight() {
    common::block_scenario::check("ent_firework_aoe");
}

#[test]
fn crossbow_rocket_explodes_on_entities_and_blocks() {
    common::block_scenario::check("ent_firework_crossbow_hit");
}
