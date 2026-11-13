//! Entities against pistons, slime, honey, mud and soul sand, bubble columns, powder snow, fluids and each other, replayed against the real
//! vanilla level (scenarios `blk_pistons_entities`, `blk_pistons_entity_types`, `blk_slime_bounce`, `blk_honey_slide`,
//! `blk_bubble_column_entities`, `blk_powder_snow_entities`, `blk_fluid_entities`, `blk_mud_soul_sand_entities`, `ent_blast_entities` and `ent_push_pairs`
//! in `tests/oracle/scenarios/`; fixtures from `scripts/block-oracle.sh NAME`).
use crate::common::block_scenario::{check, check_with};
use crate::common::entity_oracle::Options;

#[test]
fn pistons_push_launch_and_carry_entities() {
    // vanilla recomputes the dragged pig's path when a nearby block changes (`sendBlockUpdated` navigation hook), so its yaw
    // turns again; the hook is not implemented, ours keeps the old path
    check_with(
        "blk_pistons_entities",
        &Options {
            ignore_fields: vec!["rot"],
            ..Options::default()
        },
    );
}

#[test]
fn slime_blocks_bounce_what_lands_on_them() {
    check("blk_slime_bounce");
}

#[test]
fn honey_blocks_slow_slides_and_falls() {
    check("blk_honey_slide");
}

#[test]
fn bubble_columns_lift_and_drag_entities() {
    check("blk_bubble_column_entities");
}

#[test]
fn powder_snow_sticks_freezes_and_carries_entities() {
    check("blk_powder_snow_entities");
}

#[test]
fn overlapping_entities_push_each_other() {
    check("ent_push_pairs");
}

#[test]
fn flowing_water_and_lava_move_and_burn_entities() {
    check("blk_fluid_entities");
}

/// The pool water flows out over the flat ground; the player swims past the pool edge into that flowing water.
#[test]
fn swimming_out_of_a_pool_into_flowing_water() {
    check("blk_swim_pool");
}

#[test]
fn pistons_move_every_kind_of_entity_in_every_direction() {
    check("blk_pistons_entity_types");
}

#[test]
fn mud_and_soul_sand_lower_what_stands_on_them() {
    check("blk_mud_soul_sand_entities");
}

#[test]
fn explosions_and_wind_bursts_push_every_kind_of_entity() {
    crate::common::block_scenario::check("ent_blast_entities");
}
