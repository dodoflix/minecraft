//! Lazy loot-table unpack of lootable containers (chests, barrels, shulker boxes, hoppers, dispensers, droppers, crafters, decorated pots,
//! chest boats and container minecarts) through every access path, against the real vanilla level. Scenarios
//! `tests/oracle/scenarios/NAME.txt`, fixtures recorded with `scripts/block-oracle.sh NAME`.

use crate::common;
use mc_core::ident::Identifier;
use mc_core::pos::BlockPos;
use mc_data::BlockEntityType;
use mc_game::world::block_entity::new_block_entity;

/// The block entity types vanilla implements `RandomizableContainer` with: `RandomizableContainerBlockEntity` subclasses (the dropper
/// is a `DispenserBlockEntity` of its own type) and the decorated pot.
const LOOTABLE: &[BlockEntityType] = &[
    BlockEntityType::BARREL,
    BlockEntityType::CHEST,
    BlockEntityType::CRAFTER,
    BlockEntityType::DECORATED_POT,
    BlockEntityType::DISPENSER,
    BlockEntityType::DROPPER,
    BlockEntityType::HOPPER,
    BlockEntityType::SHULKER_BOX,
    BlockEntityType::TRAPPED_CHEST,
];

/// Every block entity type that takes a loot table is a vanilla lootable container and the other way round (a new lootable type needs
/// its unpack entry points), and every one but the crafter, whose `getItem` does not unpack, reports that a read unpacks.
#[test]
fn lootable_block_entity_types_are_exactly_the_vanilla_ones() {
    use mc_data::block::Block;
    let table = Identifier::parse("minecraft:chests/simple_dungeon").unwrap();
    let mut problems = Vec::new();
    for ty in BlockEntityType::iter() {
        let Some(block) = ty
            .data()
            .valid_blocks
            .iter()
            .find(|b| **b != Block::MOVING_PISTON)
        else {
            continue;
        };
        let Some(mut be) = new_block_entity(BlockPos::ZERO, block.default_state()) else {
            continue;
        };
        be.set_loot_table(table.clone(), 7);
        let takes_table = be.save_with_full_metadata().contains("LootTable");
        if takes_table != LOOTABLE.contains(&ty) {
            problems.push(format!(
                "{ty:?}: takes a loot table = {takes_table}, vanilla lootable = {}",
                LOOTABLE.contains(&ty)
            ));
        }
        if takes_table && be.has_loot_table() == (ty == BlockEntityType::CRAFTER) {
            problems.push(format!("{ty:?}: a read unpacks = {}", be.has_loot_table()));
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn block_commands_on_pending_containers_match_vanilla() {
    common::block_scenario::check("cmd_lazy_unpack_blocks");
}

#[test]
fn breaking_pending_containers_matches_vanilla() {
    common::block_scenario::check("blk_lazy_unpack_break");
}

#[test]
fn counting_and_replacing_slots_of_pending_containers_matches_vanilla() {
    common::block_scenario::check("cmd_lazy_unpack_counts");
}

#[test]
fn crafter_with_pending_table_matches_vanilla() {
    common::block_scenario::check("blk_lazy_unpack_crafter");
}

#[test]
fn hoppers_and_droppers_on_pending_containers_match_vanilla() {
    common::block_scenario::check("blk_lazy_unpack_hopper");
}

#[test]
fn loot_generated_trigger_on_opening_matches_vanilla() {
    common::block_scenario::check("blk_lazy_unpack_trigger");
}

#[test]
fn pending_vehicles_match_vanilla() {
    common::block_scenario::check("ent_lazy_unpack_vehicles");
}
