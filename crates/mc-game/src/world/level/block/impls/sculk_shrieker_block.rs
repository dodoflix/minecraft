//! `net.minecraft.world.level.block.SculkShriekerBlock`.
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::block_entity::SculkShriekerBlockEntity;
use crate::world::entity::Entity;
use crate::world::item::BlockPlaceContext;
use mc_data::fluid::Fluid;

pub struct SculkShriekerBlock;

impl BlockBehaviour for SculkShriekerBlock {
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

    fn spawn_after_break(&self, _state: BlockState, level: &mut ServerLevel, pos: BlockPos, tool: &crate::world::item::ItemStack, drop_experience: bool) {
        if drop_experience {
            helpers::try_drop_experience(level, pos, tool, &crate::registry::typed::int_provider::IntProvider::Constant(5));
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
