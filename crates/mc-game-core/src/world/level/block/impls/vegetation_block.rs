//! `net.minecraft.world.level.block.VegetationBlock` and the shared plumbing of its subclasses.
//!
//! The Java subclasses override `mayPlaceOn`; here a subclass implements [`MayPlaceOn`] and calls [`can_survive`] /
//! [`update_shape()`] (its `super` behavior), or uses [`vegetation_behaviour!`] when it only overrides `mayPlaceOn`.
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
    if !state.can_survive(level, pos) {
        air()
    } else {
        state
    }
}

/// The `canSurvive` / `updateShape` methods of `VegetationBlock` for a subclass that only overrides `mayPlaceOn`
/// (invoke inside the `impl BlockBehaviour`).
#[macro_export]
macro_rules! vegetation_behaviour_methods {
    () => {
        fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
            $crate::world::level::block::impls::vegetation_block::can_survive(self, level, pos)
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
            $crate::world::level::block::impls::vegetation_block::update_shape(state, &*level, pos)
        }
    };
}
pub use vegetation_behaviour_methods;

/// `BlockBehaviour` for a `VegetationBlock` subclass that only overrides `mayPlaceOn`.
#[macro_export]
macro_rules! vegetation_behaviour {
    ($ty:ty) => {
        impl BlockBehaviour for $ty {
            $crate::world::level::block::impls::vegetation_block::vegetation_behaviour_methods!();
        }
    };
}
pub use vegetation_behaviour;

/// The bonemeal of `BushBlock` / `FireflyBushBlock`: spread to a neighbouring free position that can hold the block
/// (invoke inside the `impl BlockBehaviour`).
#[macro_export]
macro_rules! spreading_bonemeal_methods {
    () => {
        fn is_bonemealable(&self) -> bool {
            true
        }
        fn is_valid_bonemeal_target(
            &self,
            level: &dyn LevelReader,
            pos: BlockPos,
            state: BlockState,
            _source: $crate::world::level::block::bonemeal::BonemealSource,
        ) -> bool {
            $crate::world::level::block::bonemeal::has_spreadable_neighbour_pos(level, pos, state)
        }
        fn perform_bonemeal(
            &self,
            level: &mut $crate::server::level::ServerLevel,
            pos: BlockPos,
            state: BlockState,
            _source: $crate::world::level::block::bonemeal::BonemealSource,
        ) {
            if let Some(target) =
                $crate::world::level::block::bonemeal::find_spreadable_neighbour_pos(
                    level, pos, state,
                )
            {
                level.set_block_and_update(target, state.block().default_state());
            }
        }
    };
}
pub use spreading_bonemeal_methods;

pub struct VegetationBlock;
impl MayPlaceOn for VegetationBlock {}
vegetation_behaviour!(VegetationBlock);

register_block_class!("VegetationBlock", |_| Box::new(VegetationBlock));

/// The `entityInside` of `CropBlock` / `PitcherCropBlock`: a ravager destroys the crop (`mobGriefing`).
pub fn ravager_destroys_crop(
    level: &mut crate::server::level::ServerLevel,
    pos: BlockPos,
    entity: &dyn crate::world::entity::Entity,
) {
    if entity.get_type() == mc_data::EntityType::RAVAGER && level.mob_griefing() {
        level.expose(entity, |level| {
            level.destroy_block(pos, true, Some(entity.id()), 512);
        });
    }
}
