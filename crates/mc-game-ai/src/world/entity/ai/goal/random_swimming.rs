//! `RandomSwimmingGoal` (`RandomStrollGoal` with `BehaviorUtils.getRandomSwimmablePos`).

use mc_game_core::world::entity::ai::goal::{Goal, GoalFlags, RandomStrollGoal};
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::ai::util::default_random_pos;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::level::BlockGetter;
use mc_game_core::world::level::block::state::BlockStateExt;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::block::PathComputationType;

/// `BehaviorUtils.getRandomSwimmablePos(body, maxHorizontalDistance, maxVerticalDistance)`.
pub fn get_random_swimmable_pos(body: &mut dyn Entity, level: &mut ServerLevel, max_horizontal_distance: i32, max_vertical_distance: i32) -> Option<Vec3> {
    let mut target_pos = default_random_pos::get_pos(body, level, max_horizontal_distance, max_vertical_distance);
    let mut count = 0;
    while let Some(p) = target_pos {
        if level.get_block_state(BlockPos::containing(p.x, p.y, p.z)).is_pathfindable_by(PathComputationType::Water) {
            break;
        }
        let again = count < 10;
        count += 1;
        if !again {
            break;
        }
        target_pos = default_random_pos::get_pos(body, level, max_horizontal_distance, max_vertical_distance);
    }
    target_pos
}

pub struct RandomSwimmingGoal {
    pub base: RandomStrollGoal,
}

impl RandomSwimmingGoal {
    /// `RandomSwimmingGoal(mob, speedModifier, interval)`.
    pub fn new(speed_modifier: f64, interval: i32) -> Self {
        Self { base: RandomStrollGoal::with_interval(speed_modifier, interval) }
    }

    /// `getPosition()`.
    pub fn get_position(mob: &mut dyn Entity, level: &mut ServerLevel) -> Option<Vec3> {
        get_random_swimmable_pos(mob, level, 10, 7)
    }
}

impl Goal for RandomSwimmingGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.base.can_use_with(mob, level, Self::get_position)
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.base.can_continue_to_use_impl(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.base.start_impl(mob, level);
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.base.stop_impl(mob, level);
    }

    fn flags(&self) -> GoalFlags {
        self.base.flags()
    }
}
