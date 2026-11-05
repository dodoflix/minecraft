//! `net.minecraft.world.level.block.BasePressurePlateBlock` (`PressurePlateBlock`, `WeightedPressurePlateBlock`).
use super::prelude::*;

pub struct BasePressurePlateBlock;

impl BlockBehaviour for BasePressurePlateBlock {
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let below = pos.below();
        helpers::can_support_rigid_block(level, below) || helpers::can_support_center(level, below, Direction::Up)
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if direction == Direction::Down && !state.can_survive(&*level, pos) { air() } else { state }
    });
}

register_block_class!("BasePressurePlateBlock", |_| Box::new(BasePressurePlateBlock));
