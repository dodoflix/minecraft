//! `net.minecraft.world.level.block.SlabBlock`.
use super::prelude::*;

pub struct SlabBlock;

impl BlockBehaviour for SlabBlock {
    fn update_shape(
        &self,
        state: BlockState,
        level: &mut dyn LevelAccessor,
        pos: BlockPos,
        _direction_to_neighbour: Direction,
        _neighbour_pos: BlockPos,
        _neighbour_state: BlockState,
    ) -> BlockState {
        if state.get(props::WATERLOGGED) {
            helpers::schedule_water_tick(level, pos);
        }
        state
    }
}

register_block_class!("SlabBlock", |_| Box::new(SlabBlock));
