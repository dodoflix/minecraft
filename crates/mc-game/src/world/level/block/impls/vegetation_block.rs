//! `net.minecraft.world.level.block.VegetationBlock` and the shared plumbing of its subclasses.
//!
//! The Java subclasses override `mayPlaceOn`; here a subclass implements [`MayPlaceOn`] and calls [`can_survive`] /
//! [`update_shape`] (its `super` behavior), or uses [`vegetation_behaviour!`] when it only overrides `mayPlaceOn`.
use super::prelude::*;

/// `VegetationBlock.mayPlaceOn(state, level, pos)`; the default is `BlockTags.SUPPORTS_VEGETATION`.
pub trait MayPlaceOn {
    fn may_place_on(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> bool {
        state.is_tag("supports_vegetation")
    }
}

/// `VegetationBlock.canSurvive`.
pub fn can_survive(this: &dyn MayPlaceOn, level: &dyn LevelReader, pos: BlockPos) -> bool {
    let below = pos.below();
    this.may_place_on(level.get_block_state(below), level, below)
}

/// `VegetationBlock.updateShape`.
pub fn update_shape(state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> BlockState {
    if !state.can_survive(level, pos) { air() } else { state }
}

/// `BlockBehaviour` for a `VegetationBlock` subclass that only overrides `mayPlaceOn`.
macro_rules! vegetation_behaviour {
    ($ty:ty) => {
        impl BlockBehaviour for $ty {
            fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
                super::vegetation_block::can_survive(self, level, pos)
            }
            fn update_shape(
                &self,
                state: BlockState,
                level: &mut dyn LevelAccessor,
                pos: BlockPos,
                _direction_to_neighbour: Direction,
                _neighbour_pos: BlockPos,
                _neighbour_state: BlockState,
            ) -> BlockState {
                super::vegetation_block::update_shape(state, &*level, pos)
            }
        }
    };
}
pub(crate) use vegetation_behaviour;

pub struct VegetationBlock;
impl MayPlaceOn for VegetationBlock {}
vegetation_behaviour!(VegetationBlock);

register_block_class!("VegetationBlock", |_| Box::new(VegetationBlock));
