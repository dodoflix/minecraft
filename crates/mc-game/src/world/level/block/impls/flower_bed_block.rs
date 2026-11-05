//! `net.minecraft.world.level.block.FlowerBedBlock` (pink petals, wildflowers; extends `VegetationBlock`).
use super::prelude::*;
use super::vegetation_block::MayPlaceOn;

pub struct FlowerBedBlock;
impl MayPlaceOn for FlowerBedBlock {}

impl BlockBehaviour for FlowerBedBlock {
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        super::vegetation_block::can_survive(self, level, pos)
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

register_block_class!("FlowerBedBlock", |_| Box::new(FlowerBedBlock));
