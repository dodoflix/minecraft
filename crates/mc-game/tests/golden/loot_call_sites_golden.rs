//! The production consumers of loot tables replayed through the world they run in (scenarios `tests/oracle/scenarios/`, fixtures
//! regenerated with `scripts/block-oracle.sh NAME`): the entity components `copy_components` reads from an entity and the ones a
//! spawn egg applies, the death loot override of a mob, a charged creeper's victims, shulker boxes with a pending loot table
//! destroyed by something other than a player, vaults with luck, archaeology without a loot seed, a broken piston head's moving
//! block, and probe tables of the `loot_sites` data pack that drop one item per loot parameter a call site passes (victim,
//! attacker, interacting entity, tool, origin, block state, luck): a table that finds a parameter the call site left out drops
//! a different item than vanilla's.
use crate::common::block_scenario::check;

macro_rules! golden {
    ($($test:ident => $scenario:literal),+ $(,)?) => {$(
        #[test]
        fn $test() {
            check($scenario);
        }
    )+};
}

golden!(
    copy_components_reads_the_entity_components_of_every_class => "ent_copy_components",
    spawn_eggs_apply_the_implicit_components_of_every_class => "blk_disp_spawn_egg_components",
    charged_creeper_victims_drop_their_heads => "mob_charged_creeper_victims",
    death_loot_table_override_rolls_and_clears => "mob_death_loot_table_override",
    shulker_with_a_loot_table_unpacks_when_it_is_destroyed => "blk_shulker_loot_destroy",
    vault_rewards_follow_luck_and_the_display_override => "blk_vault_ominous_luck",
    archaeology_rolls_without_a_seed => "blk_brushable_loot_sequence",
    moving_piston_block_drops_the_moved_block => "blk_piston_moving_drops",
    hoe_pops_rooted_dirt_loot_from_the_side_faces => "itm_hoe_rooted_dirt_faces",
    probe_entity_sites => "loot_probe_entities",
    probe_piglin_barter => "loot_probe_barter",
    probe_fishing => "loot_probe_fishing",
    probe_block_sites => "loot_probe_blocks",
    probe_vault => "loot_probe_vault",
    probe_archaeology => "loot_probe_archaeology",
    probe_containers => "loot_probe_containers",
);
