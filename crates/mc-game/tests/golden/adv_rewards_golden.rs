//! Advancement rewards and a trigger no vanilla advancement uses, against the vanilla oracle with a data pack
//! (`tests/oracle/datapacks/adv_rewards`, the scenarios' `datapack` option). Scenarios: `tests/oracle/scenarios/adv_*.txt`,
//! fixtures: `tests/fixtures/entity_oracle/` (`scripts/block-oracle.sh`).
use crate::common::block_scenario::check;

/// The reward function runs as the awarded player with game master permissions, in award order; a missing function runs nothing.
#[test]
fn reward_function_runs_as_the_player() {
    check("adv_reward_function");
}

/// Loot rewards go into the inventory, or drop for the player alone when it is full.
#[test]
fn reward_loot_drops_when_the_inventory_is_full() {
    check("adv_reward_loot_full_inventory");
}

/// Experience and recipe rewards, with a recipe of the data pack.
#[test]
fn reward_experience_and_recipes() {
    check("adv_reward_recipes_xp");
}

/// The raid omen trigger awards a pack advancement to the player who carries the omen into a new raid.
#[test]
fn raid_omen_awards_the_voluntary_exile_trigger() {
    check("adv_voluntary_exile");
}
