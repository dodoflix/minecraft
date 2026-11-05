//! `net.minecraft.world.level.block.LightBlock`.
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::entity::ServerPlayer;
use crate::world::item::{InteractionResult, ItemStack};
use crate::world::level::collision::CollisionContext;
use mc_core::phys::shapes::{Shapes, VoxelShape};
use mc_core::phys::BlockHitResult;
use mc_data::Item;

pub struct LightBlock;

impl BlockBehaviour for LightBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if state.get(props::WATERLOGGED) {
            helpers::schedule_water_tick(level, pos);
        }
        state
    });
    fn get_shape(&self, _state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos, context: &CollisionContext) -> VoxelShape {
        if context.is_holding_item(Item::LIGHT) { Shapes::block() } else { Shapes::empty() }
    }
    fn use_without_item(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, player: &mut ServerPlayer, _hit: &BlockHitResult) -> InteractionResult {
        if player.can_use_game_master_blocks() {
            level.set_block_flags(pos, state.cycle(props::LEVEL), 2);
            InteractionResult::SUCCESS_SERVER
        } else {
            InteractionResult::CONSUME
        }
    }
    fn get_clone_item_stack(&self, state: BlockState, _level: &ServerLevel, _pos: BlockPos, _include_data: bool) -> ItemStack {
        set_light_on_stack(ItemStack::new(Item::LIGHT), state.get(props::LEVEL))
    }
}

/// `LightBlock.setLightOnStack`.
pub fn set_light_on_stack(mut stack: ItemStack, light_level: i32) -> ItemStack {
    helpers::set_block_state_property(&mut stack, "level", &light_level.to_string());
    stack
}

register_block_class!("LightBlock", |_| Box::new(LightBlock));
