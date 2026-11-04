//! `net.minecraft.world.level.block.LanternBlock`.
use super::prelude::*;

pub struct LanternBlock;

/// `LanternBlock.getConnectedDirection`.
fn get_connected_direction(state: BlockState) -> Direction {
    if state.get(props::HANGING) { Direction::Down } else { Direction::Up }
}

impl BlockBehaviour for LanternBlock {
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let direction = get_connected_direction(state).get_opposite();
        helpers::can_support_center(level, pos.relative(direction), direction.get_opposite())
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
        if state.get(props::WATERLOGGED) {
            helpers::schedule_water_tick(level, pos);
        }
        if get_connected_direction(state).get_opposite() == direction_to_neighbour && !state.can_survive(&*level, pos) { air() } else { state }
    }
}

register_block_class!("LanternBlock", |_| Box::new(LanternBlock));
