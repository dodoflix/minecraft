//! `ClimbOnTopOfPowderSnowGoal`.

use super::{Flag, Goal, GoalFlags};
use crate::server::level::ServerLevel;
use crate::world::entity::mob;
use crate::world::entity::Entity;
use crate::world::level::BlockGetter;
use crate::world::level::block::state::BlockStateExt;
use mc_data::EntityType;
use mc_data::block::Block;

#[derive(Default)]
pub struct ClimbOnTopOfPowderSnowGoal;

impl ClimbOnTopOfPowderSnowGoal {
    pub fn new() -> Self {
        Self
    }
}

impl Goal for ClimbOnTopOfPowderSnowGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let in_powder_snow = mob.base().was_in_powder_snow || mob.base().is_in_powder_snow;
        if in_powder_snow && mc_data::tags::get::<EntityType>("powder_snow_walkable_mobs").is_some_and(|t| t.contains(mob.get_type())) {
            let above = mob.block_position().above();
            let above_state = level.get_block_state(above);
            above_state.block() == Block::POWDER_SNOW || above_state.get_collision_shape(level, above).is_empty()
        } else {
            false
        }
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        mob::jump_control_jump(mob);
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Jump])
    }
}
