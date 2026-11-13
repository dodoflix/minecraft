//! Crafting through the player's own menu: the 2x2 grid, its result slot, the recipe unlocks it causes and the recipe book
//! placing recipes into it, clicked through the packet handler. Scenarios `tests/oracle/scenarios/mnu_crafting_*.txt` and
//! `mnu_place_recipe_inventory.txt` replayed on the real vanilla level (`scripts/block-oracle.sh NAME`) vs our level.

use crate::common;

#[test]
fn the_inventory_grid_crafts_every_recipe_kind_with_remainders_and_limited_crafting() {
    common::block_scenario::check("mnu_crafting_inventory");
}

#[test]
fn the_recipe_book_places_recipes_into_the_inventory_grid_and_ignores_what_vanilla_ignores() {
    common::block_scenario::check("mnu_place_recipe_inventory");
}
