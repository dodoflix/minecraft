//! `net.minecraft.world.level.block.MangroveRootsBlock`.
use super::prelude::*;

pub struct MangroveRootsBlock;

impl BlockBehaviour for MangroveRootsBlock {
    crate::simple_waterlogged!();
    fn get_state_for_placement(&self, block: Block, level: &mut crate::server::level::ServerLevel, context: &crate::world::item::BlockPlaceContext) -> Option<BlockState> {
        let replaced_fluid_state = level.get_fluid_state(context.get_clicked_pos());
        Some(block.default_state().set(props::WATERLOGGED, replaced_fluid_state.is_source_of_type(mc_data::fluid::Fluid::WATER)))
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if state.get(props::WATERLOGGED) {
            helpers::schedule_water_tick(level, pos);
        }
        state
    });
}

register_block_class!("MangroveRootsBlock", |_| Box::new(MangroveRootsBlock));
