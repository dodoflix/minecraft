//! `net.minecraft.world.level.material.WaterFluid` (`Source` and `Flowing`; the client-only `animateTick` and
//! `getDripParticle` are not part of the server).

use super::flowing_fluid::{self, FlowingFluid};
use super::{FluidBehaviour, fluid_is_tag};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::inside_block_effects::{InsideBlockEffectApplier, InsideBlockEffectType};
use crate::world::level::block::helpers;
use crate::world::level::{BlockGetter, LevelReader};
use mc_core::direction::Direction;
use mc_core::phys::Vec3;
use mc_core::phys::shapes::VoxelShape;
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_data::fluid::{Fluid, FluidState};

pub struct WaterFluid;

impl FluidBehaviour for WaterFluid {
    fn tick(
        &self,
        level: &mut ServerLevel,
        pos: BlockPos,
        block_state: BlockState,
        fluid_state: FluidState,
    ) {
        flowing_fluid::tick(self, level, pos, block_state, fluid_state);
    }
    fn entity_inside(
        &self,
        _level: &mut ServerLevel,
        _pos: BlockPos,
        _entity: &mut dyn Entity,
        effect_applier: &mut dyn InsideBlockEffectApplier,
    ) {
        effect_applier.apply(InsideBlockEffectType::Extinguish);
    }
    fn can_be_replaced_with(
        &self,
        _state: FluidState,
        _level: &dyn BlockGetter,
        _pos: BlockPos,
        other: Fluid,
        direction: Direction,
    ) -> bool {
        direction == Direction::Down && !fluid_is_tag(other, "water")
    }
    fn get_flow(&self, level: &dyn BlockGetter, pos: BlockPos, state: FluidState) -> Vec3 {
        flowing_fluid::get_flow(self, level, pos, state)
    }
    fn get_tick_delay(&self, _level: &dyn LevelReader) -> i32 {
        5
    }
    fn get_height(&self, state: FluidState, level: &dyn BlockGetter, pos: BlockPos) -> f32 {
        flowing_fluid::get_height(state, level, pos)
    }
    fn is_same(&self, other: Fluid) -> bool {
        other == Fluid::WATER || other == Fluid::FLOWING_WATER
    }
    fn get_shape(&self, state: FluidState, level: &dyn BlockGetter, pos: BlockPos) -> VoxelShape {
        flowing_fluid::get_shape(state, level, pos)
    }
}

impl FlowingFluid for WaterFluid {
    fn get_flowing_fluid(&self) -> Fluid {
        Fluid::FLOWING_WATER
    }
    fn get_source_fluid(&self) -> Fluid {
        Fluid::WATER
    }
    fn can_convert_to_source(&self, level: &ServerLevel) -> bool {
        level.water_source_conversion()
    }
    fn before_destroying_block(&self, level: &mut ServerLevel, pos: BlockPos, state: BlockState) {
        helpers::drop_resources(level, state, pos, None);
    }
    fn get_slope_find_distance(&self, _level: &dyn LevelReader) -> i32 {
        4
    }
    fn get_drop_off(&self, _level: &dyn LevelReader) -> i32 {
        1
    }
}

crate::register_fluid!("minecraft:water", |_| Box::new(WaterFluid));
crate::register_fluid!("minecraft:flowing_water", |_| Box::new(WaterFluid));
