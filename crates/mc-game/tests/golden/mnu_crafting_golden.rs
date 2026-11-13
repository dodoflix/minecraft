//! Crafting through the player's own menu and the crafting table: the 2x2 and 3x3 grids, their result slots, the recipe unlocks they
//! cause and the recipe book placing recipes into them, clicked through the packet handler. Scenarios
//! `tests/oracle/scenarios/mnu_crafting_*.txt`, `mnu_place_recipe_*.txt` and `mnu_open_crafting_table.txt` replayed on the real
//! vanilla level (`scripts/block-oracle.sh NAME`) vs our level.

use crate::common;

#[test]
fn the_inventory_grid_crafts_every_recipe_kind_with_remainders_and_limited_crafting() {
    common::block_scenario::check("mnu_crafting_inventory");
}

#[test]
fn the_recipe_book_places_recipes_into_the_inventory_grid_and_ignores_what_vanilla_ignores() {
    common::block_scenario::check("mnu_place_recipe_inventory");
}

#[test]
fn the_crafting_table_opens_crafts_and_closes_with_its_block() {
    common::block_scenario::check("mnu_open_crafting_table");
}

#[test]
fn the_recipe_book_places_recipes_into_the_crafting_table_grid() {
    common::block_scenario::check("mnu_place_recipe_table");
}
