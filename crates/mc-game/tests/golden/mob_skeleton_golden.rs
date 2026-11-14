//! Skeleton family behaviour (bow goal, weapon reassessment, arrow effects, freezing) replayed against the vanilla oracle recordings
//! (`tests/oracle/scenarios/`, fixtures `tests/fixtures/entity_oracle/`, recorded with `scripts/block-oracle.sh`).
use crate::common::block_scenario::check;

#[test]
fn skeleton_draws_and_shoots_its_bow_at_a_player() {
    check("mob_skeleton_shoot");
}

#[test]
fn skeleton_swaps_bow_and_melee_goals_when_its_equipment_changes() {
    check("mob_skeleton_weapon_swap");
}

#[test]
fn bow_interval_follows_difficulty_and_skeleton_class() {
    check("mob_skeleton_hard_interval");
}

#[test]
fn stray_arrows_slow_the_target() {
    check("mob_stray_arrow");
}

#[test]
fn bogged_parched_and_wither_skeleton_arrows_carry_their_effect() {
    check("mob_bogged_parched_wither_arrow");
}

#[test]
fn skeleton_in_powder_snow_freezes_into_a_stray() {
    check("mob_skeleton_freeze");
}

#[test]
fn enchanted_bow_sets_arrows_on_fire_and_raises_their_damage() {
    check("mob_skeleton_flame_bow");
}

#[test]
fn sunlight_burns_skeletons_and_wears_their_helmets() {
    check("mob_skeleton_sun_helmet");
}
