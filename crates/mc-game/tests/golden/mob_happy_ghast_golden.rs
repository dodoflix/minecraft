//! Happy ghast behaviour (the harness and growth of a ghastling, riding with up to four riders, leashing and the quad leash, the hatch of a
//! dried ghast block) replayed against the vanilla oracle recordings (`tests/oracle/scenarios/`, fixtures `tests/fixtures/entity_oracle/`,
//! recorded with `scripts/block-oracle.sh`).
use crate::common::block_scenario::check;

#[test]
fn dried_ghast_hatches_a_ghastling_facing_like_the_block() {
    check("blk_dried_ghast_hatch");
}

#[test]
fn happy_ghast_takes_a_harness_and_ghastling_grows_up() {
    check("mob_happy_ghast_harness");
}

#[test]
fn harnessed_happy_ghast_carries_four_riders_and_stays_still_under_a_player() {
    check("mob_happy_ghast_ride");
}

#[test]
fn happy_ghast_leash_and_quad_leash() {
    check("mob_happy_ghast_leash");
}

#[test]
fn happy_ghast_data_merge_crossing_age_zero_and_loaded_passengers() {
    check("mob_happy_ghast_load");
}

#[test]
fn scale_attribute_is_capped_by_happy_ghast_and_shulker_and_ignored_by_dragon() {
    check("mob_scale_sanitize");
}
