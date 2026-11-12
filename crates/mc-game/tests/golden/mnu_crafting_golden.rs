//! Crafting through the player's own menu: the 2x2 grid, its result slot and the recipe unlocks it causes, clicked through the
//! packet handler. Scenarios `tests/oracle/scenarios/mnu_crafting_*.txt` replayed on the real vanilla level
//! (`scripts/block-oracle.sh NAME`) vs our level.

use crate::common;

#[test]
fn the_inventory_grid_crafts_every_recipe_kind_with_remainders_and_limited_crafting() {
    common::block_scenario::check("mnu_crafting_inventory");
}
