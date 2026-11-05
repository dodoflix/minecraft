//! `FlowingFluid` (water / lava): the read-only part entities need (`getFlow`, `getHeight`, `isSame`). The scheduled
//! tick / spreading half of the class is not ported here. `fluid_behaviour` falls back to [`FlowingFluid`] for the four
//! fluids when nothing registered them with `register_fluid!`; the slice that ports the tick half registers its own
//! behavior (and must implement `get_flow` / `get_height` there, they are shadowed by that registration).
use super::{FluidBehaviour, FluidStateExt};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::inside_block_effects::{InsideBlockEffectApplier, InsideBlockEffectType};
use crate::world::level::BlockGetter;
use crate::world::level::block::BlockStateExt;
use mc_core::direction::Direction;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::fluid::{Fluid, FluidState};

/// A `FlowingFluid` subclass: `source` = `getSource()`, `flowing` = `getFlowing()`.
pub struct FlowingFluid {
    pub source: Fluid,
    pub flowing: Fluid,
}

/// `FlowingFluid.isSame(other)` (`other == getSource() || other == getFlowing()`).
fn is_same(this: &FlowingFluid, other: Fluid) -> bool {
    other == this.source || other == this.flowing
}

/// `FluidState.getValue(FALLING)`.
fn is_falling(state: FluidState) -> bool {
    state.data().properties.iter().any(|&(name, value)| name == "falling" && value == "true")
}

impl FlowingFluid {
    /// `affectsFlow(neighbourFluid)`.
    fn affects_flow(&self, neighbour: FluidState) -> bool {
        neighbour.is_empty() || is_same(self, neighbour.get_type())
    }

    /// `isSolidFace(level, pos, direction)`.
    fn is_solid_face(&self, level: &dyn BlockGetter, pos: BlockPos, direction: Direction) -> bool {
        let state = level.get_block_state(pos);
        let fluid_state = level.get_fluid_state(pos);
        if is_same(self, fluid_state.get_type()) {
            false
        } else if direction == Direction::Up {
            true
        } else if state.block().data().classes.contains(&"IceBlock") {
            false
        } else {
            state.is_face_sturdy(level, pos, direction)
        }
    }
}

impl FluidBehaviour for FlowingFluid {
    fn get_flow(&self, level: &dyn BlockGetter, pos: BlockPos, fluid_state: FluidState) -> Vec3 {
        let mut flow_x = 0.0f64;
        let mut flow_z = 0.0f64;
        for direction in Direction::HORIZONTAL {
            let block_pos = pos.relative(direction);
            let neighbour_fluid = level.get_fluid_state(block_pos);
            if self.affects_flow(neighbour_fluid) {
                let mut neighbor_height = neighbour_fluid.own_height();
                let mut distance = 0.0f32;
                if neighbor_height == 0.0 {
                    if !level.get_block_state(block_pos).block().is_in_tag("blocks_fluid_flow") {
                        let neighbor_pos = block_pos.below();
                        let below_neighbor_state = level.get_fluid_state(neighbor_pos);
                        if self.affects_flow(below_neighbor_state) {
                            neighbor_height = below_neighbor_state.own_height();
                            if neighbor_height > 0.0 {
                                distance = fluid_state.own_height() - (neighbor_height - 0.8888889f32);
                            }
                        }
                    }
                } else if neighbor_height > 0.0 {
                    distance = fluid_state.own_height() - neighbor_height;
                }
                if distance != 0.0 {
                    flow_x += f64::from(direction.step_x() as f32 * distance);
                    flow_z += f64::from(direction.step_z() as f32 * distance);
                }
            }
        }
        let mut flow = Vec3::new(flow_x, 0.0, flow_z);
        if is_falling(fluid_state) {
            for direction in Direction::HORIZONTAL {
                let block_pos = pos.relative(direction);
                if self.is_solid_face(level, block_pos, direction) || self.is_solid_face(level, block_pos.above(), direction) {
                    flow = flow.normalize().add(Vec3::new(0.0, -6.0, 0.0));
                    break;
                }
            }
        }
        flow.normalize()
    }

    fn get_height(&self, fluid_state: FluidState, level: &dyn BlockGetter, pos: BlockPos) -> f32 {
        // `hasSameAbove`: `fluidState.getType().isSame(level.getFluidState(pos.above()).getType())`
        if is_same(self, level.get_fluid_state(pos.above()).get_type()) { 1.0 } else { fluid_state.own_height() }
    }

    fn entity_inside(&self, _level: &mut ServerLevel, _pos: BlockPos, _entity: &mut dyn Entity, effect_applier: &mut dyn InsideBlockEffectApplier) {
        if self.source == Fluid::WATER {
            // `WaterFluid.entityInside`
            effect_applier.apply(InsideBlockEffectType::Extinguish);
        } else {
            // `LavaFluid.entityInside`
            effect_applier.apply(InsideBlockEffectType::ClearFreeze);
            effect_applier.apply(InsideBlockEffectType::LavaIgnite);
            effect_applier.run_after(InsideBlockEffectType::LavaIgnite, Box::new(|entity, level| entity.lava_hurt(level)));
        }
    }
}

/// The `FlowingFluid` of water/lava, used by `fluid_behaviour` for fluids no `register_fluid!` claims.
pub fn flowing_fluid_for(fluid: Fluid) -> Option<FlowingFluid> {
    match fluid {
        Fluid::WATER | Fluid::FLOWING_WATER => Some(FlowingFluid { source: Fluid::WATER, flowing: Fluid::FLOWING_WATER }),
        Fluid::LAVA | Fluid::FLOWING_LAVA => Some(FlowingFluid { source: Fluid::LAVA, flowing: Fluid::FLOWING_LAVA }),
        _ => None,
    }
}
