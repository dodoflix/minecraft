//! `RandomStandGoal`.

use super::{Goal, GoalFlags};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use mc_core::random::RandomSource;

pub struct RandomStandGoal {
    next_stand: i32,
}

impl RandomStandGoal {
    /// `RandomStandGoal(horse)`: `resetStandInterval(horse)`.
    pub fn new(horse: &dyn Entity) -> Self {
        let mut goal = Self { next_stand: 0 };
        goal.reset_stand_interval(horse);
        goal
    }

    fn reset_stand_interval(&mut self, horse: &dyn Entity) {
        self.next_stand = -horse
            .as_abstract_horse()
            .map_or(0, |h| h.get_ambient_stand_interval());
    }
}

impl Goal for RandomStandGoal {
    fn can_use(&mut self, horse: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        self.next_stand += 1;
        if self.next_stand > 0 && horse.base_mut().random.next_int_bounded(1000) < self.next_stand {
            self.reset_stand_interval(horse);
            !horse.is_immobile() && horse.base_mut().random.next_int_bounded(10) == 0
        } else {
            false
        }
    }

    fn can_continue_to_use(&mut self, _horse: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        false
    }

    fn start(&mut self, horse: &mut dyn Entity, level: &mut ServerLevel) {
        if let Some(h) = horse.as_abstract_horse_mut() {
            h.stand_if_possible(level);
        }
        if let Some(sound) = horse
            .as_abstract_horse()
            .and_then(|h| h.get_ambient_stand_sound())
        {
            horse.play_sound_default(level, sound);
        }
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::NONE
    }
}
