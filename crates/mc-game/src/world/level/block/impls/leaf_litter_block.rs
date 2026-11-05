//! `net.minecraft.world.level.block.LeafLitterBlock` (extends `VegetationBlock`).
use super::prelude::*;

pub struct LeafLitterBlock;

impl BlockBehaviour for LeafLitterBlock {
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let below_pos = pos.below();
        level.get_block_state(below_pos).is_face_sturdy_at(level, below_pos, Direction::Up)
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        super::vegetation_block::update_shape(state, &*level, pos)
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("LeafLitterBlock", |_| Box::new(LeafLitterBlock));
