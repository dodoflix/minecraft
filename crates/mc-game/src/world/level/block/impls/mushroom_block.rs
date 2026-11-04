//! `net.minecraft.world.level.block.MushroomBlock` (extends `VegetationBlock`).
use super::prelude::*;
use super::vegetation_block::MayPlaceOn;

pub struct MushroomBlock;

impl MayPlaceOn for MushroomBlock {
    fn may_place_on(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> bool {
        state.is_solid_render()
    }
}

impl BlockBehaviour for MushroomBlock {
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let below_pos = pos.below();
        let below = level.get_block_state(below_pos);
        if below.is_tag("overrides_mushroom_light_requirement") {
            true
        } else {
            level.get_raw_brightness(pos, 0) < 13 && self.may_place_on(below, level, below_pos)
        }
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        super::vegetation_block::update_shape(state, &*level, pos)
    });
}

register_block_class!("MushroomBlock", |_| Box::new(MushroomBlock));
