//! `BreakDoorGoal`.

use super::door_interact::DoorInteractData;
use super::{Goal, GoalFlags};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::living::LIVING_ENTITY_FLAG_OFF_HAND;
use crate::world::entity::mob::combat;
use crate::world::item::InteractionHand;
use crate::world::level::{BlockGetter, LevelAccessor};
use mc_core::random::RandomSource;
use mc_world::level_storage::Difficulty;
use std::sync::Arc;

pub const DEFAULT_DOOR_BREAK_TIME: i32 = 240;

/// `Predicate<Difficulty>`.
pub type DifficultyPredicate = Arc<dyn Fn(Difficulty) -> bool + Send + Sync>;

pub struct BreakDoorGoal {
    pub base: DoorInteractData,
    valid_difficulties: DifficultyPredicate,
    pub break_time: i32,
    pub last_break_progress: i32,
    pub door_break_time: i32,
}

impl BreakDoorGoal {
    /// `BreakDoorGoal(mob, validDifficulties)`.
    pub fn new(mob: &dyn Entity, valid_difficulties: DifficultyPredicate) -> Self {
        Self {
            base: DoorInteractData::new(mob),
            valid_difficulties,
            break_time: 0,
            last_break_progress: -1,
            door_break_time: -1,
        }
    }

    /// `BreakDoorGoal(mob, seconds, validDifficulties)`.
    pub fn with_seconds(
        mob: &dyn Entity,
        seconds: i32,
        valid_difficulties: DifficultyPredicate,
    ) -> Self {
        let mut goal = Self::new(mob, valid_difficulties);
        goal.door_break_time = seconds;
        goal
    }

    /// `getDoorBreakTime()`.
    pub fn get_door_break_time(&self) -> i32 {
        DEFAULT_DOOR_BREAK_TIME.max(self.door_break_time)
    }

    fn is_valid_difficulty(&self, difficulty: Difficulty) -> bool {
        (self.valid_difficulties)(difficulty)
    }
}

impl Goal for BreakDoorGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if !self.base.can_use(mob, level) {
            false
        } else if !level.mob_griefing() {
            false
        } else {
            self.is_valid_difficulty(level.difficulty()) && !self.base.is_open(level)
        }
    }

    fn start(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        self.base.start(mob);
        self.break_time = 0;
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.break_time <= self.get_door_break_time()
            && !self.base.is_open(level)
            && self
                .base
                .door_pos
                .closer_to_center_than(&mob.position(), 2.0)
            && self.is_valid_difficulty(level.difficulty())
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        // `super.stop()` is `Goal.stop()` (no-op)
        level.destroy_block_progress(mob.id(), self.base.door_pos, -1);
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.base.tick(mob);
        if mob.base_mut().random.next_int_bounded(20) == 0 {
            level.level_event(None, 1019, self.base.door_pos, 0);
            if !mob.living().is_some_and(|l| l.swing_state.is_swinging()) {
                let off_hand = mob
                    .living()
                    .is_some_and(|l| l.get_living_entity_flag(LIVING_ENTITY_FLAG_OFF_HAND));
                combat::swing_for_attack(
                    mob,
                    level,
                    if off_hand {
                        InteractionHand::OffHand
                    } else {
                        InteractionHand::MainHand
                    },
                );
            }
        }
        self.break_time += 1;
        let progress = (self.break_time as f32 / self.get_door_break_time() as f32 * 10.0) as i32;
        if progress != self.last_break_progress {
            level.destroy_block_progress(mob.id(), self.base.door_pos, progress);
            self.last_break_progress = progress;
        }
        if self.break_time == self.get_door_break_time()
            && self.is_valid_difficulty(level.difficulty())
        {
            level.remove_block(self.base.door_pos, false);
            level.level_event(None, 1021, self.base.door_pos, 0);
            let state_id = level.get_block_state(self.base.door_pos).id() as i32;
            level.level_event(None, 2001, self.base.door_pos, state_id);
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::NONE
    }
}
