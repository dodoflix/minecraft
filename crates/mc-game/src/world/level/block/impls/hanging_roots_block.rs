//! `net.minecraft.world.level.block.HangingRootsBlock`.
use super::prelude::*;

pub struct HangingRootsBlock;

impl BlockBehaviour for HangingRootsBlock {
    fn get_state_for_placement(&self, block: Block, level: &mut crate::server::level::ServerLevel, context: &crate::world::item::BlockPlaceContext) -> Option<BlockState> {
        let fluid_state = level.get_fluid_state(context.get_clicked_pos());
        Some(block.default_state().set(props::WATERLOGGED, fluid_state.is_source_of_type(mc_data::fluid::Fluid::WATER)))
    }
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let attached_to_pos = pos.above();
        level.get_block_state(attached_to_pos).is_face_sturdy_at(level, attached_to_pos, Direction::Down)
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if direction == Direction::Up && !self.can_survive(state, &*level, pos) {
            air()
        } else {
            if state.get(props::WATERLOGGED) {
                helpers::schedule_water_tick(level, pos);
            }
            state
        }
    });
}

register_block_class!("HangingRootsBlock", |_| Box::new(HangingRootsBlock));
