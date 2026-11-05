//! `TryFindLiquidGoal` (`TryFindWaterGoal` is `new(mob, FluidTags.WATER)`).

use super::{Goal, GoalFlags};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::level::BlockGetter;
use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_data::fluid::Fluid;

pub struct TryFindLiquidGoal {
    /// `fluidTag` (a `fluid` tag name such as `"water"`).
    fluid_tag: &'static str,
}

impl TryFindLiquidGoal {
    pub fn new(fluid_tag: &'static str) -> Self {
        Self { fluid_tag }
    }

    fn is_fluid(&self, level: &ServerLevel, pos: BlockPos) -> bool {
        mc_data::tags::get::<Fluid>(self.fluid_tag).is_some_and(|t| t.contains(level.get_fluid_state(pos).fluid()))
    }
}

impl Goal for TryFindLiquidGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        mob.base().on_ground() && !self.is_fluid(level, mob.block_position())
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let b = mob.base();
        let found = BlockPos::between_closed_xyz(
            mth::floor_f64(b.x() - 2.0),
            mth::floor_f64(b.y() - 2.0),
            mth::floor_f64(b.z() - 2.0),
            mth::floor_f64(b.x() + 2.0),
            mob.block_position().y(),
            mth::floor_f64(b.z() + 2.0),
        )
        .find(|pos| self.is_fluid(level, *pos));
        if let Some(pos) = found {
            crate::world::entity::mob::set_wanted_position(mob, f64::from(pos.x()), f64::from(pos.y()), f64::from(pos.z()), 1.0);
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::NONE
    }
}
