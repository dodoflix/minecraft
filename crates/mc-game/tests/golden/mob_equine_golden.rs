//! Horse, donkey, mule, llama, skeleton horse and zombie horse behaviour, the owner reference of tameable animals and horses, and the
//! leash behaviour of mounts replayed against the vanilla oracle recordings (`tests/oracle/scenarios/`, fixtures
//! `tests/fixtures/entity_oracle/`, recorded with `scripts/block-oracle.sh`).
use crate::common::block_scenario::check;

#[test]
fn execute_on_owner_finds_the_owner_of_pets_and_horses() {
    check("ent_execute_owner");
}

#[test]
fn leashed_horse_stops_eating_and_hangs_on_a_quad_leash() {
    check("mob_leash_horse");
}

#[test]
fn horse_breeding_blends_the_parents_attributes() {
    check("mob_horse_breed_stats");
}

#[test]
fn free_horses_idle_like_vanilla() {
    check("mob_horse_idle");
}

#[test]
fn ridden_horse_jumps_on_the_riding_jump_command() {
    check("mob_horse_jump");
}

#[test]
fn riding_an_untamed_horse_tames_it() {
    check("mob_horse_tame_ride");
}

#[test]
fn llama_takes_carpet_and_chest_by_hand() {
    check("mob_llama_decor");
}

#[test]
fn llamas_follow_the_head_of_a_caravan() {
    check("mob_llama_caravan");
}

#[test]
fn llama_spits_at_its_target() {
    check("mob_llama_spit_target");
}

#[test]
fn skeleton_trap_horse_springs_the_trap() {
    check("mob_skeleton_trap");
}

#[test]
fn zombie_horse_matches_vanilla() {
    check("mob_zombie_horse");
}
