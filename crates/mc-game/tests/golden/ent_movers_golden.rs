//! Entities against pistons, slime, honey, mud and soul sand, bubble columns, powder snow, fluids and each other, replayed against the real
//! vanilla level (scenarios `blk_pistons_entities`, `blk_pistons_entity_types`, `blk_slime_bounce`, `blk_honey_slide`,
//! `blk_bubble_column_entities`, `blk_powder_snow_entities`, `blk_fluid_entities`, `blk_mud_soul_sand_entities`, `ent_blast_entities` and `ent_push_pairs`
//! in `tests/oracle/scenarios/`; fixtures from `scripts/block-oracle.sh NAME`).
use crate::common::block_scenario::check_with;
use crate::common::entity_oracle::Options;

/// The mobs' body rotation and ambient sounds follow AI random draws that these scenarios do not test.
fn check_ignoring_mob_ai(name: &str) {
    check_with(
        name,
        &Options {
            ignore_fields: vec!["rot"],
            ignore_containing: vec![".ambient"],
            ..Options::default()
        },
    );
}

#[test]
fn pistons_push_launch_and_carry_entities() {
    check_ignoring_mob_ai("blk_pistons_entities");
}

#[test]
fn slime_blocks_bounce_what_lands_on_them() {
    check_ignoring_mob_ai("blk_slime_bounce");
}

#[test]
fn honey_blocks_slow_slides_and_falls() {
    check_ignoring_mob_ai("blk_honey_slide");
}

#[test]
fn bubble_columns_lift_and_drag_entities() {
    check_ignoring_mob_ai("blk_bubble_column_entities");
}

#[test]
fn powder_snow_sticks_freezes_and_carries_entities() {
    check_ignoring_mob_ai("blk_powder_snow_entities");
}

#[test]
fn overlapping_entities_push_each_other() {
    check_ignoring_mob_ai("ent_push_pairs");
}

#[test]
fn flowing_water_and_lava_move_and_burn_entities() {
    check_ignoring_mob_ai("blk_fluid_entities");
}

#[test]
fn pistons_move_every_kind_of_entity_in_every_direction() {
    check_ignoring_mob_ai("blk_pistons_entity_types");
}

#[test]
fn mud_and_soul_sand_lower_what_stands_on_them() {
    check_ignoring_mob_ai("blk_mud_soul_sand_entities");
}

#[test]
fn explosions_and_wind_bursts_push_every_kind_of_entity() {
    crate::common::block_scenario::check("ent_blast_entities");
}
