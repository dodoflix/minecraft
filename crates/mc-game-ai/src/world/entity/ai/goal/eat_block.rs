//! `EatBlockGoal`.

use mc_core::random::RandomSource;
use mc_data::block::Block;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::entity::ai::goal::{Flag, Goal, GoalFlags};
use mc_game_core::world::entity::mob::{ageable, navigation};
use mc_game_core::world::level::block::state::BlockStateExt;
use mc_game_core::world::level::level_access::{BlockGetter, LevelAccessor};

const EAT_ANIMATION_TICKS: i32 = 40;

pub struct EatBlockGoal {
    eat_animation_tick: i32,
}

impl EatBlockGoal {
    /// `EatBlockGoal(mob)`.
    pub fn new() -> Self {
        Self {
            eat_animation_tick: 0,
        }
    }

    /// `getEatAnimationTick()`.
    pub fn get_eat_animation_tick(&self) -> i32 {
        self.eat_animation_tick
    }
}

impl Default for EatBlockGoal {
    fn default() -> Self {
        Self::new()
    }
}

impl Goal for EatBlockGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let bound = self.adjusted_tick_delay(if ageable::is_baby(mob) { 50 } else { 1000 });
        if mob.base_mut().random.next_int_bounded(bound) != 0 {
            return false;
        }
        let pos = mob.block_position();
        level.get_block_state(pos).is_tag("edible_for_sheep")
            || level.get_block_state(pos.below()).block() == Block::GRASS_BLOCK
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.eat_animation_tick = self.adjusted_tick_delay(EAT_ANIMATION_TICKS);
        level.broadcast_entity_event(mob.id(), 10);
        navigation::stop(mob, level);
    }

    fn stop(&mut self, _mob: &mut dyn Entity, _level: &mut ServerLevel) {
        self.eat_animation_tick = 0;
    }

    fn can_continue_to_use(&mut self, _mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        self.eat_animation_tick > 0
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.eat_animation_tick = (self.eat_animation_tick - 1).max(0);
        if self.eat_animation_tick == self.adjusted_tick_delay(4) {
            let pos = mob.block_position();
            if level.get_block_state(pos).is_tag("edible_for_sheep") {
                if level.mob_griefing() {
                    level.destroy_block_impl(pos, false, None, 512);
                }
                mob.ate(level);
            } else {
                let below = pos.below();
                let state = level.get_block_state(below);
                if state.block() == Block::GRASS_BLOCK {
                    if level.mob_griefing() {
                        level.level_event_impl(None, 2001, below, state.id() as i32);
                        level.set_block_flags(below, Block::DIRT.default_state(), 2);
                    }
                    mob.ate(level);
                }
            }
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move, Flag::Look, Flag::Jump])
    }
}
