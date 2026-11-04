//! `net.minecraft.world.level.block.SmallDripleafBlock` (extends `DoublePlantBlock`).
use super::double_plant_block;
use super::prelude::*;
use super::vegetation_block::MayPlaceOn;
use mc_data::enums::DoubleBlockHalf;
use mc_data::fluid::Fluid;

pub struct SmallDripleafBlock;

/// `SmallDripleafBlock.mayPlaceOn` (`super.mayPlaceOn` is the `SUPPORTS_VEGETATION` default).
struct DefaultVegetation;
impl MayPlaceOn for DefaultVegetation {}

impl MayPlaceOn for SmallDripleafBlock {
    fn may_place_on(&self, state: BlockState, level: &dyn BlockGetter, pos: BlockPos) -> bool {
        state.is_tag("supports_small_dripleaf")
            || (level.get_fluid_state(pos.above()).is_source_of_type(Fluid::WATER) && DefaultVegetation.may_place_on(state, level, pos))
    }
}

impl BlockBehaviour for SmallDripleafBlock {
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        if state.get(props::DOUBLE_BLOCK_HALF) == DoubleBlockHalf::Upper {
            double_plant_block::can_survive(self, state, level, pos)
        } else {
            let below_pos = pos.below();
            self.may_place_on(level.get_block_state(below_pos), level, below_pos)
        }
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if state.get(props::WATERLOGGED) {
            helpers::schedule_water_tick(level, pos);
        }
        double_plant_block::update_shape(state, &*level, pos, direction, neighbour_state)
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("SmallDripleafBlock", |_| Box::new(SmallDripleafBlock));
