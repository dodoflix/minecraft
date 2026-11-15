//! Advancement criteria fired by items and inventories against the vanilla oracle (ticking-level replay, data pack
//! `tests/oracle/datapacks/adv_items`): consuming, using, durability, buckets, inventory changes, crafting and thrown items picked
//! up. Scenarios: `tests/oracle/scenarios/adv_*.txt`, fixtures: `tests/fixtures/entity_oracle/` (`scripts/block-oracle.sh`).
use crate::common::block_scenario::check;

/// Eating and drinking: the stack the trigger sees (before one item is used up), the food level at that moment, refused meals,
/// creative players, and the per-tick using trigger.
#[test]
fn consuming_items_awards_the_consumer() {
    check("adv_consume_items");
}

/// Wearing a tool, hoe, flint and steel, shears and armor down, the last points of a tool, a boosted strider, creative players.
#[test]
fn damaging_items_awards_the_holder() {
    check("adv_durability");
}

/// Filling a bucket from a fluid and from a mob in the water.
#[test]
fn filling_buckets_awards_the_player() {
    check("adv_buckets");
}

/// Items dropped by a player and picked up by mobs, other players and the thrower itself.
#[test]
fn thrown_items_picked_up_award_thrower_and_taker() {
    check("adv_thrown_pickup");
}

/// An allay throws the item it fetched to its liked player.
#[test]
fn allay_delivery_awards_the_taker() {
    check("adv_allay_delivery");
}

/// Slot counts, single and multiple item predicates, merges by pickup, commands, armor slots.
#[test]
fn inventory_changes_award_the_owner() {
    check("adv_inventory_changed");
}

/// Recipe and ingredient conditions of crafting results in the grid, at a crafting table and at a smithing table.
#[test]
fn taking_crafting_results_awards_the_crafter() {
    check("adv_recipe_crafted");
}

/// Using a bow, shield and spyglass every tick, the sneaking and looked-at-entity conditions, a refused use.
#[test]
fn using_items_awards_the_user() {
    check("adv_using_item");
}
