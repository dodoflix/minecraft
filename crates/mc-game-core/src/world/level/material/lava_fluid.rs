//! `net.minecraft.world.level.material.LavaFluid` (`Source` and `Flowing`; `animateTick`/`getDripParticle` are
//! client-only).

use super::flowing_fluid::{self, FlowingFluid, is_falling};
use super::{FluidBehaviour, FluidStateExt, fluid_is_tag};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::inside_block_effects::{InsideBlockEffectApplier, InsideBlockEffectType};
use crate::world::level::block::impls::base_fire_block;
use crate::world::level::block::{BlockStateExt, helpers};
use crate::world::level::{BlockGetter, LevelAccessor, LevelReader};
use mc_core::direction::Direction;
use mc_core::phys::Vec3;
use mc_core::phys::shapes::VoxelShape;
use mc_core::pos::BlockPos;
use mc_data::block::{Block, BlockState};
use mc_data::fluid::{Fluid, FluidState};

pub struct LavaFluid;

/// `fizz(level, pos)`.
pub fn fizz(level: &mut dyn LevelAccessor, pos: BlockPos) {
    level.level_event(None, 1501, pos, 0);
}

/// `isFlammable(level, pos)`.
fn is_flammable(level: &dyn LevelReader, pos: BlockPos) -> bool {
    if !level.is_outside_build_height(pos.y()) && !level.has_chunk(pos.x() >> 4, pos.z() >> 4) {
        false
    } else {
        level.get_block_state(pos).ignited_by_lava()
    }
}

/// `hasFlammableNeighbours(level, pos)`.
fn has_flammable_neighbours(level: &dyn LevelReader, pos: BlockPos) -> bool {
    Direction::ALL
        .iter()
        .any(|&d| is_flammable(level, pos.relative(d)))
}

impl FluidBehaviour for LavaFluid {
    fn tick(
        &self,
        level: &mut ServerLevel,
        pos: BlockPos,
        block_state: BlockState,
        fluid_state: FluidState,
    ) {
        flowing_fluid::tick(self, level, pos, block_state, fluid_state);
    }

    fn random_tick(&self, level: &mut ServerLevel, pos: BlockPos, _fluid_state: FluidState) {
        if !level.can_spread_fire_around(pos) {
            return;
        }
        let passes = LevelAccessor::random(level).next_int_bounded(3);
        if passes > 0 {
            let mut test_pos = pos;
            for _ in 0..passes {
                let (dx, dz) = {
                    let r = LevelAccessor::random(level);
                    (r.next_int_bounded(3) - 1, r.next_int_bounded(3) - 1)
                };
                test_pos = test_pos.offset(dx, 1, dz);
                if !level.is_loaded(test_pos) {
                    return;
                }
                let block_state = BlockGetter::get_block_state(level, test_pos);
                if block_state.is_air() {
                    if has_flammable_neighbours(level, test_pos) {
                        let fire = base_fire_block::get_state(level, test_pos);
                        LevelAccessor::set_block_and_update(level, test_pos, fire);
                        return;
                    }
                } else if block_state.is_tag("blocks_lava_fire_spread") {
                    return;
                }
            }
        } else {
            for _ in 0..3 {
                let (dx, dz) = {
                    let r = LevelAccessor::random(level);
                    (r.next_int_bounded(3) - 1, r.next_int_bounded(3) - 1)
                };
                let test_pos = pos.offset(dx, 0, dz);
                if !level.is_loaded(test_pos) {
                    return;
                }
                if level.is_empty_block(test_pos.above()) && is_flammable(level, test_pos) {
                    let fire = base_fire_block::get_state(level, test_pos);
                    LevelAccessor::set_block_and_update(level, test_pos.above(), fire);
                }
            }
        }
    }

    fn entity_inside(
        &self,
        _level: &mut ServerLevel,
        _pos: BlockPos,
        _entity: &mut dyn Entity,
        effect_applier: &mut dyn InsideBlockEffectApplier,
    ) {
        effect_applier.apply(InsideBlockEffectType::ClearFreeze);
        effect_applier.apply(InsideBlockEffectType::LavaIgnite);
        effect_applier.run_after(
            InsideBlockEffectType::LavaIgnite,
            Box::new(|entity, level| entity.lava_hurt(level)),
        );
    }

    fn can_be_replaced_with(
        &self,
        state: FluidState,
        level: &dyn BlockGetter,
        pos: BlockPos,
        other: Fluid,
        _direction: Direction,
    ) -> bool {
        state.get_height(level, pos) >= 0.44444445 && fluid_is_tag(other, "water")
    }
    fn get_flow(&self, level: &dyn BlockGetter, pos: BlockPos, state: FluidState) -> Vec3 {
        flowing_fluid::get_flow(self, level, pos, state)
    }
    fn get_tick_delay(&self, level: &dyn LevelReader) -> i32 {
        if level.is_fast_lava() { 10 } else { 30 }
    }
    fn get_height(&self, state: FluidState, level: &dyn BlockGetter, pos: BlockPos) -> f32 {
        flowing_fluid::get_height(state, level, pos)
    }
    fn is_same(&self, other: Fluid) -> bool {
        other == Fluid::LAVA || other == Fluid::FLOWING_LAVA
    }
    fn get_shape(&self, state: FluidState, level: &dyn BlockGetter, pos: BlockPos) -> VoxelShape {
        flowing_fluid::get_shape(state, level, pos)
    }
}

impl FlowingFluid for LavaFluid {
    fn get_flowing_fluid(&self) -> Fluid {
        Fluid::FLOWING_LAVA
    }
    fn get_source_fluid(&self) -> Fluid {
        Fluid::LAVA
    }
    fn can_convert_to_source(&self, level: &ServerLevel) -> bool {
        level.lava_source_conversion()
    }
    fn before_destroying_block(&self, level: &mut ServerLevel, pos: BlockPos, _state: BlockState) {
        fizz(level, pos);
    }
    fn get_slope_find_distance(&self, level: &dyn LevelReader) -> i32 {
        if level.is_fast_lava() { 4 } else { 2 }
    }
    fn get_drop_off(&self, level: &dyn LevelReader) -> i32 {
        if level.is_fast_lava() { 1 } else { 2 }
    }

    fn get_spread_delay(
        &self,
        level: &mut ServerLevel,
        pos: BlockPos,
        old: FluidState,
        new: FluidState,
    ) -> i32 {
        let mut result = self.get_tick_delay(&*level);
        if !old.is_empty()
            && !new.is_empty()
            && !is_falling(old)
            && !is_falling(new)
            && new.get_height(level, pos) > old.get_height(level, pos)
            && LevelAccessor::random(level).next_int_bounded(4) != 0
        {
            result *= 4;
        }
        result
    }

    fn spread_to(
        &self,
        level: &mut ServerLevel,
        pos: BlockPos,
        state: BlockState,
        direction: Direction,
        target: FluidState,
    ) {
        if direction == Direction::Down {
            let fluid_state = BlockGetter::get_fluid_state(level, pos);
            if fluid_is_tag(Fluid::LAVA, "lava") && fluid_state.is_tag("water") {
                if helpers::is_instance_of(state, "LiquidBlock") {
                    LevelAccessor::set_block_and_update(level, pos, Block::STONE.default_state());
                }
                fizz(level, pos);
                return;
            }
        }
        flowing_fluid::default_spread_to(self, level, pos, state, target);
    }
}

crate::register_fluid!("minecraft:lava", |_| Box::new(LavaFluid));
crate::register_fluid!("minecraft:flowing_lava", |_| Box::new(LavaFluid));
