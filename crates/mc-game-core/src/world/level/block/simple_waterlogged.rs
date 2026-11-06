//! `SimpleWaterloggedBlock` (`BucketPickup` + `LiquidBlockContainer` defaults) for every block with a `waterlogged`
//! property. Implementors splice the six trait hooks in with [`simple_waterlogged!`](crate::simple_waterlogged);
//! classes overriding `canPlaceLiquid`/`placeLiquid` (`SlabBlock`, `CampfireBlock`, `CandleBlock`, ...) call the free
//! functions below for the `super` behavior.

use super::BlockStateExt;
use crate::world::entity::Entity;
use crate::world::item::ItemStack;
use crate::world::level::{BlockGetter, LevelAccessor};
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_data::fluid::{Fluid, FluidState};
use mc_data::{Item, SoundEvent, props};

/// `SimpleWaterloggedBlock.canPlaceLiquid`.
pub fn can_place_liquid(
    _user: Option<&dyn Entity>,
    _level: &dyn BlockGetter,
    _pos: BlockPos,
    _state: BlockState,
    fluid: Fluid,
) -> bool {
    fluid == Fluid::WATER
}

/// `SimpleWaterloggedBlock.placeLiquid`.
pub fn place_liquid(
    level: &mut dyn LevelAccessor,
    pos: BlockPos,
    state: BlockState,
    fluid_state: FluidState,
) -> bool {
    if !state.get(props::WATERLOGGED) && fluid_state.fluid() == Fluid::WATER {
        level.set_block_and_update(pos, state.set(props::WATERLOGGED, true));
        super::helpers::schedule_fluid_tick(level, pos, fluid_state.fluid());
        true
    } else {
        false
    }
}

/// `SimpleWaterloggedBlock.pickupBlock`.
pub fn pickup_block(
    _user: Option<&dyn Entity>,
    level: &mut dyn LevelAccessor,
    pos: BlockPos,
    state: BlockState,
) -> ItemStack {
    if state.get(props::WATERLOGGED) {
        level.set_block_and_update(pos, state.set(props::WATERLOGGED, false));
        if !state.can_survive(&*level, pos) {
            level.destroy_block(pos, true, None, 512);
        }
        ItemStack::new(Item::WATER_BUCKET)
    } else {
        ItemStack::empty()
    }
}

/// `SimpleWaterloggedBlock.getPickupSound` (`Fluids.WATER.getPickupSound()`).
pub fn get_pickup_sound() -> Option<SoundEvent> {
    Fluid::WATER.data().pickup_sound
}

/// Splices the `SimpleWaterloggedBlock` hooks (`LiquidBlockContainer` + `BucketPickup`) into a `BlockBehaviour` impl.
#[macro_export]
macro_rules! simple_waterlogged {
    () => {
        fn is_liquid_block_container(&self) -> bool {
            true
        }
        fn can_place_liquid(
            &self,
            user: Option<&dyn $crate::world::entity::Entity>,
            level: &dyn $crate::world::level::BlockGetter,
            pos: ::mc_core::pos::BlockPos,
            state: ::mc_data::block::BlockState,
            fluid: ::mc_data::fluid::Fluid,
        ) -> bool {
            $crate::world::level::block::simple_waterlogged::can_place_liquid(
                user, level, pos, state, fluid,
            )
        }
        fn place_liquid(
            &self,
            level: &mut dyn $crate::world::level::LevelAccessor,
            pos: ::mc_core::pos::BlockPos,
            state: ::mc_data::block::BlockState,
            fluid_state: ::mc_data::fluid::FluidState,
        ) -> bool {
            $crate::world::level::block::simple_waterlogged::place_liquid(
                level,
                pos,
                state,
                fluid_state,
            )
        }
        fn is_bucket_pickup(&self) -> bool {
            true
        }
        fn pickup_block(
            &self,
            user: Option<&dyn $crate::world::entity::Entity>,
            level: &mut dyn $crate::world::level::LevelAccessor,
            pos: ::mc_core::pos::BlockPos,
            state: ::mc_data::block::BlockState,
        ) -> $crate::world::item::ItemStack {
            $crate::world::level::block::simple_waterlogged::pickup_block(user, level, pos, state)
        }
        fn get_pickup_sound(&self) -> Option<::mc_data::SoundEvent> {
            $crate::world::level::block::simple_waterlogged::get_pickup_sound()
        }
    };
}
