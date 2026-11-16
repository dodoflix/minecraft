//! The world border of each level against the real vanilla level (`tests/oracle/scenarios/border_*.txt`; fixtures regenerated with
//! `scripts/block-oracle.sh NAME`).

use crate::common;

/// Size, centre, damage, warning and lerp commands act on the border of one level; its packets reach only that level's players.
#[test]
fn every_level_has_its_own_border() {
    common::block_scenario::check("border_dimensions");
}

/// Joining or changing level sends the border of that level, lerps in progress included.
#[test]
fn the_border_of_the_new_level_is_sent_on_join_and_level_change() {
    common::block_scenario::check("border_join_dimension_change");
}

/// A portal of the nether leads to the edge of a small overworld border.
#[test]
fn the_destination_of_a_portal_is_clamped_into_the_border_of_the_overworld() {
    common::block_scenario::check("border_portal_clamp");
}

/// Chunk generation spawns animals only inside the border of its level.
#[test]
fn generation_spawns_animals_only_inside_the_border() {
    common::block_scenario::check("border_gen_mobs");
}
