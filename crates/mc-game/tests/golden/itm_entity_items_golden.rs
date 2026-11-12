//! Items that place entities (armor stand, item frames, painting, cushion, boats) and a spectator opening a chest boat (`use`, `use_on`, `interact`,
//! the dumps `entities`, `nbt:`, `inventory:`, `packets:`, `events`) replayed on the real vanilla level (`scripts/block-oracle.sh NAME`) vs our ticking level.

use crate::common;

#[test]
fn armor_stands_frames_paintings_and_cushions_are_placed_where_they_fit() {
    common::block_scenario::check("itm_place_entities");
}

#[test]
fn boats_are_placed_where_the_ray_hits() {
    common::block_scenario::check("itm_boat_place");
}

#[test]
fn spectators_open_chest_boats() {
    common::block_scenario::check("itm_spectator_chest_boat");
}
