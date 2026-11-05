//! `Block.getDrops` / `Block.dropResources` / `BlockBehaviour.getDrops` and `Block.dropFromLootTable`: block loot tables
//! (`BlockData.loot_table`, `minecraft:blocks/<name>`) evaluated with the `block` parameter set.

use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_data::block::BlockState;
use mc_core::pos::BlockPos;
use mc_protocol::components::ItemStack;

use super::context::{LootParams, LootParamsBuilder};
use super::loading::get_loot_table;
use super::parameters::LootContextParamSets;
use crate::server::level::ServerLevel;
use crate::world::block_entity::BlockEntity;
use crate::world::entity::{Entity, EntityId};
use crate::world::level::BlockGetter;
use crate::world::level::block::{BlockStateExt, behaviour, helpers};

/// `BlockBehaviour.BlockStateBase.getDrops(params)` (`BlockBehaviour.getDrops`): the loot table of the state's block with
/// `BLOCK_STATE` added to `params`; blocks without a loot table drop nothing.
pub fn state_get_drops(state: BlockState, pos: BlockPos, builder: LootParamsBuilder) -> Vec<ItemStack> {
    let level = builder.level();
    // `MovingPistonBlock.getDrops`: the moved state's drops
    let state = behaviour(state.block()).get_drops_state(state, level, pos);
    let Some(key) = state.block().data().loot_table else { return Vec::new() };
    let params = builder.block_state(state).create(&LootContextParamSets::BLOCK);
    let Some(id) = Identifier::try_parse(key) else { return Vec::new() };
    get_loot_table(&level.registries, &id).get_random_items(&params)
}

/// `Block.getDrops(state, level, pos, blockEntity)` (empty tool).
pub fn get_drops(level: &ServerLevel, state: BlockState, pos: BlockPos, block_entity: Option<&dyn BlockEntity>) -> Vec<ItemStack> {
    let params = LootParams::builder(level).origin(Vec3::at_center_of(&pos)).tool(&ItemStack::empty()).block_entity(block_entity);
    state_get_drops(state, pos, params)
}

/// `Block.getDrops(state, level, pos, blockEntity, breaker, tool)`.
pub fn block_drops(
    level: &ServerLevel,
    state: BlockState,
    pos: BlockPos,
    block_entity: Option<&dyn BlockEntity>,
    breaker: Option<&dyn Entity>,
    tool: &ItemStack,
) -> Vec<ItemStack> {
    let params = LootParams::builder(level).origin(Vec3::at_center_of(&pos)).tool(tool).optional_this_entity(breaker).block_entity(block_entity);
    state_get_drops(state, pos, params)
}

/// The drops of a block destroyed by an explosion (`ServerExplosion.destroyBlocks`): `ORIGIN` at the block's center, an
/// empty `TOOL`, the optional block entity and explosion source, `EXPLOSION_RADIUS` only for `DESTROY_WITH_DECAY`.
pub fn block_drops_explosion(
    level: &ServerLevel,
    state: BlockState,
    pos: BlockPos,
    block_entity: Option<&dyn BlockEntity>,
    this_entity: Option<&dyn Entity>,
    explosion_radius: Option<f32>,
) -> Vec<ItemStack> {
    let params = LootParams::builder(level)
        .origin(Vec3::at_center_of(&pos))
        .tool(&ItemStack::empty())
        .block_entity(block_entity)
        .optional_this_entity(this_entity)
        .optional_explosion_radius(explosion_radius);
    state_get_drops(state, pos, params)
}

/// `Block.dropResources(state, level, pos, blockEntity, breaker, tool)`: pops every drop, then `spawnAfterBreak`.
pub fn drop_resources_with_tool(
    level: &mut ServerLevel,
    state: BlockState,
    pos: BlockPos,
    block_entity: Option<&dyn BlockEntity>,
    breaker: Option<&dyn Entity>,
    tool: &ItemStack,
) {
    let drops = block_drops(level, state, pos, block_entity, breaker, tool);
    for stack in drops {
        helpers::pop_resource(level, pos, stack);
    }
    behaviour(state.block()).spawn_after_break(state, level, pos, tool, true);
}

/// `helpers::drop_resources`: `Level.destroyBlock` →
/// `Block.dropResources(state, level, pos, blockEntity, breaker, ItemStack.EMPTY)`.
pub fn drop_resources_for_destroy(level: &mut ServerLevel, state: BlockState, pos: BlockPos, breaker: Option<EntityId>) {
    let drops = {
        let level_ref: &ServerLevel = level;
        let block_entity = if state.has_block_entity() { level_ref.get_block_entity(pos) } else { None };
        let breaker_entity = breaker.and_then(|id| level_ref.get_entity(id));
        block_drops(level_ref, state, pos, block_entity, breaker_entity, &ItemStack::empty())
    };
    for stack in drops {
        helpers::pop_resource(level, pos, stack);
    }
    behaviour(state.block()).spawn_after_break(state, level, pos, &ItemStack::empty(), true);
}

/// `Block.dropFromLootTable(level, key, paramsBuilder, consumer)`: rolls `key` (the `LootTable.EMPTY` for unknown ids) and
/// hands every drop to `consumer`; `true` when there was any.
pub fn drop_from_loot_table<'a>(
    level: &'a ServerLevel,
    key: &Identifier,
    params_builder: impl FnOnce(LootParamsBuilder<'a>) -> LootParams<'a>,
    consumer: &mut dyn FnMut(ItemStack),
) -> bool {
    let table = get_loot_table(&level.registries, key);
    let params = params_builder(LootParams::builder(level));
    let drops = table.get_random_items(&params);
    if drops.is_empty() {
        return false;
    }
    for stack in drops {
        consumer(stack);
    }
    true
}

/// `Block.dropFromBlockInteractLootTable(level, key, pos, state, blockEntity, tool, interactingEntity, consumer)`: the
/// drops of `key` rolled with the `block_interact` parameter set (the caller pops them, in order).
pub fn block_interact_drops(
    level: &ServerLevel,
    key: &str,
    pos: BlockPos,
    state: BlockState,
    tool: Option<&ItemStack>,
    interacting_entity: Option<&dyn Entity>,
) -> Vec<ItemStack> {
    let Some(id) = Identifier::try_parse(key) else { return Vec::new() };
    let mut drops = Vec::new();
    drop_from_loot_table(
        level,
        &id,
        |builder| {
            let mut builder = builder
                .block_state(state)
                .origin(Vec3::at_center_of(&pos))
                .block_entity(level.get_block_entity(pos))
                .interacting_entity(interacting_entity);
            if let Some(tool) = tool {
                builder = builder.tool(tool);
            }
            builder.create(&LootContextParamSets::BLOCK_INTERACT)
        },
        &mut |stack| drops.push(stack),
    );
    drops
}
