//! Recipe unlocks and the recipe book packets of a new player, replayed against the real vanilla level
//! (`scripts/block-oracle.sh NAME`): `tests/oracle/scenarios/mnu_recipe_unlock.txt`, `mnu_join_recipes.txt`.

use crate::common;

#[test]
fn recipes_unlock_through_advancement_rewards_and_the_recipe_command() {
    common::block_scenario::check("mnu_recipe_unlock");
}

#[test]
fn a_new_player_joins_with_the_recipe_packets_and_gets_the_crafting_table_recipe() {
    common::block_scenario::check("mnu_join_recipes");
}
