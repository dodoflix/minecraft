use crate::server::level::ServerLevel;
use crate::world::level::shapes::voxel_shape;
use crate::world::level::{BlockGetter, LevelReader};
use linkme::distributed_slice;
use mc_core::direction::Direction;
use mc_core::phys::shapes::VoxelShape;
use mc_core::phys::{AABB, Vec3};
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_data::fluid::{Fluid, FluidState};
use std::sync::OnceLock;

/// `Fluid` (overridable behavior). Defaults are vanilla's `EmptyFluid` / base `Fluid` behavior; static per-state data
/// (amount, source flag, own height, legacy block, random ticking, explosion resistance) is in `mc_data`.
pub trait FluidBehaviour: Send + Sync + 'static {
    /// `tick(level, pos, blockState, fluidState)` (scheduled fluid tick).
    fn tick(&self, _level: &mut ServerLevel, _pos: BlockPos, _block_state: BlockState, _fluid_state: FluidState) {}
    /// `randomTick`.
    fn random_tick(&self, _level: &mut ServerLevel, _pos: BlockPos, _fluid_state: FluidState) {}
    /// `entityInside(level, pos, entity, effectApplier)`.
    fn entity_inside(&self, _level: &mut ServerLevel, _pos: BlockPos, _entity: &mut dyn crate::world::entity::Entity, _effect_applier: &mut dyn crate::world::entity::inside_block_effects::InsideBlockEffectApplier) {}
    /// `canBeReplacedWith(state, level, pos, other, direction)`.
    fn can_be_replaced_with(&self, _state: FluidState, _level: &dyn BlockGetter, _pos: BlockPos, _other: Fluid, _direction: Direction) -> bool {
        false
    }
    /// `getFlow(level, pos, state)`.
    fn get_flow(&self, _level: &dyn BlockGetter, _pos: BlockPos, _state: FluidState) -> Vec3 {
        Vec3::ZERO
    }
    /// `getTickDelay(level)`.
    fn get_tick_delay(&self, _level: &dyn LevelReader) -> i32 {
        0
    }
    /// `getHeight(state, level, pos)`.
    fn get_height(&self, state: FluidState, _level: &dyn BlockGetter, _pos: BlockPos) -> f32 {
        state.own_height()
    }
    /// `getShape(state, level, pos)`.
    fn get_shape(&self, state: FluidState, _level: &dyn BlockGetter, _pos: BlockPos) -> VoxelShape {
        voxel_shape(state.data().shape)
    }
}

/// `EmptyFluid`.
pub struct EmptyFluid;
impl FluidBehaviour for EmptyFluid {}

/// Registers behavior for the fluid with registry name `fluid` (`minecraft:water`). (Java class names are ambiguous
/// for fluids: both `WaterFluid.Flowing` and `LavaFluid.Flowing` are `Flowing`.)
pub struct FluidRegistration {
    pub fluid: &'static str,
    pub create: fn(Fluid) -> Box<dyn FluidBehaviour>,
}

#[distributed_slice]
pub static FLUIDS: [FluidRegistration];

#[macro_export]
macro_rules! register_fluid {
    ($name:literal, $create:expr) => {
        const _: () = {
            #[::linkme::distributed_slice($crate::world::level::material::FLUIDS)]
            static REG: $crate::world::level::material::FluidRegistration =
                $crate::world::level::material::FluidRegistration { fluid: $name, create: $create };
        };
    };
}

static TABLE: OnceLock<Vec<Box<dyn FluidBehaviour>>> = OnceLock::new();

/// The behavior of `fluid` (`FluidState.getType()` + virtual dispatch).
pub fn fluid_behaviour(fluid: Fluid) -> &'static dyn FluidBehaviour {
    TABLE
        .get_or_init(|| {
            Fluid::iter()
                .map(|f| match FLUIDS.iter().find(|r| r.fluid == f.data().name) {
                    Some(r) => (r.create)(f),
                    None => match super::flowing_fluid::flowing_fluid_for(f) {
                        Some(flowing) => Box::new(flowing) as Box<dyn FluidBehaviour>,
                        None => Box::new(EmptyFluid) as Box<dyn FluidBehaviour>,
                    },
                })
                .collect()
        })[fluid.id() as usize]
        .as_ref()
}

/// `FluidState` methods that dispatch to the fluid's behavior.
pub trait FluidStateExt: Copy {
    fn get_type(self) -> Fluid;
    fn is_tag(self, tag: &str) -> bool;
    fn is_source_of_type(self, fluid: Fluid) -> bool;
    fn is_full(self) -> bool;
    fn is_randomly_ticking(self) -> bool;
    fn tick(self, level: &mut ServerLevel, pos: BlockPos, block_state: BlockState);
    fn random_tick(self, level: &mut ServerLevel, pos: BlockPos);
    fn get_flow(self, level: &dyn BlockGetter, pos: BlockPos) -> Vec3;
    fn get_height(self, level: &dyn BlockGetter, pos: BlockPos) -> f32;
    fn can_be_replaced_with(self, level: &dyn BlockGetter, pos: BlockPos, other: Fluid, direction: Direction) -> bool;
    fn get_shape(self, level: &dyn BlockGetter, pos: BlockPos) -> VoxelShape;
    /// `getAABB(level, pos)`.
    fn get_aabb(self, level: &dyn BlockGetter, pos: BlockPos) -> Option<AABB>;
    fn get_explosion_resistance(self) -> f32;
}

impl FluidStateExt for FluidState {
    fn get_type(self) -> Fluid {
        self.fluid()
    }
    fn is_tag(self, tag: &str) -> bool {
        mc_data::tags::get::<Fluid>(tag).is_some_and(|t| t.contains(self.fluid()))
    }
    fn is_source_of_type(self, fluid: Fluid) -> bool {
        self.fluid() == fluid && self.is_source()
    }
    fn is_full(self) -> bool {
        self.amount() == 8
    }
    fn is_randomly_ticking(self) -> bool {
        self.data().random_ticking
    }
    fn tick(self, level: &mut ServerLevel, pos: BlockPos, block_state: BlockState) {
        fluid_behaviour(self.fluid()).tick(level, pos, block_state, self);
    }
    fn random_tick(self, level: &mut ServerLevel, pos: BlockPos) {
        fluid_behaviour(self.fluid()).random_tick(level, pos, self);
    }
    fn get_flow(self, level: &dyn BlockGetter, pos: BlockPos) -> Vec3 {
        fluid_behaviour(self.fluid()).get_flow(level, pos, self)
    }
    fn get_height(self, level: &dyn BlockGetter, pos: BlockPos) -> f32 {
        fluid_behaviour(self.fluid()).get_height(self, level, pos)
    }
    fn can_be_replaced_with(self, level: &dyn BlockGetter, pos: BlockPos, other: Fluid, direction: Direction) -> bool {
        fluid_behaviour(self.fluid()).can_be_replaced_with(self, level, pos, other, direction)
    }
    fn get_shape(self, level: &dyn BlockGetter, pos: BlockPos) -> VoxelShape {
        fluid_behaviour(self.fluid()).get_shape(self, level, pos)
    }
    fn get_aabb(self, level: &dyn BlockGetter, pos: BlockPos) -> Option<AABB> {
        if self.is_empty() {
            return None;
        }
        let height = f64::from(self.get_height(level, pos));
        Some(AABB::new(
            f64::from(pos.x()),
            f64::from(pos.y()),
            f64::from(pos.z()),
            f64::from(pos.x()) + 1.0,
            f64::from(pos.y()) + height,
            f64::from(pos.z()) + 1.0,
        ))
    }
    fn get_explosion_resistance(self) -> f32 {
        self.fluid().data().explosion_resistance
    }
}
