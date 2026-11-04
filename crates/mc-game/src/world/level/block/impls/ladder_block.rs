//! `net.minecraft.world.level.block.LadderBlock`.
use super::prelude::*;

pub struct LadderBlock;

impl BlockBehaviour for LadderBlock {
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let direction = state.get(props::HORIZONTAL_FACING);
        let attach = pos.relative(direction.get_opposite());
        level.get_block_state(attach).is_face_sturdy_at(level, attach, direction)
    }
    fn update_shape(
        &self,
        state: BlockState,
        level: &mut dyn LevelAccessor,
        pos: BlockPos,
        direction_to_neighbour: Direction,
        _neighbour_pos: BlockPos,
        _neighbour_state: BlockState,
    ) -> BlockState {
        if direction_to_neighbour.get_opposite() == state.get(props::HORIZONTAL_FACING) && !state.can_survive(&*level, pos) {
            air()
        } else {
            if state.get(props::WATERLOGGED) {
                helpers::schedule_water_tick(level, pos);
            }
            state
        }
    }
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("LadderBlock", |_| Box::new(LadderBlock));
