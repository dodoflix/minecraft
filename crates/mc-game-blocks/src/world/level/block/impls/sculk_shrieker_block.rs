//! `net.minecraft.world.level.block.SculkShriekerBlock`.
use mc_game_core::{register_block_class, update_shape};
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::block_entity::SculkShriekerBlockEntity;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::item::BlockPlaceContext;
use mc_data::fluid::Fluid;

pub struct SculkShriekerBlock;

impl BlockBehaviour for SculkShriekerBlock {
    mc_game_core::simple_waterlogged!();
    fn get_state_for_placement(&self, block: Block, level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        let replaced_fluid = level.get_fluid_state(context.get_clicked_pos());
        Some(block.default_state().set(props::WATERLOGGED, replaced_fluid.fluid() == Fluid::WATER))
    }

    fn step_on(&self, level: &mut ServerLevel, pos: BlockPos, _on_state: BlockState, entity: &mut dyn Entity) {
        if let Some(player) = SculkShriekerBlockEntity::try_get_player(level, entity) {
            SculkShriekerBlockEntity::try_shriek(level, pos, Some(player));
        }
    }

    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        if state.get(props::SHRIEKING) {
            level.set_block_and_update(pos, state.set(props::SHRIEKING, false));
            SculkShriekerBlockEntity::try_respond(level, pos);
        }
    }

    fn spawn_after_break(&self, _state: BlockState, level: &mut ServerLevel, pos: BlockPos, tool: &mc_game_core::world::item::ItemStack, drop_experience: bool) {
        if drop_experience {
            helpers::try_drop_experience(level, pos, tool, &mc_game_core::registry::typed::int_provider::IntProvider::Constant(5));
        }
    }

    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if state.get(props::WATERLOGGED) {
            helpers::schedule_water_tick(level, pos);
        }
        state
    });
}

register_block_class!("SculkShriekerBlock", |_| Box::new(SculkShriekerBlock));
