//! `SwellGoal` (creeper ignition).

use super::{Flag, Goal, GoalFlags};
use crate::server::level::ServerLevel;
use crate::world::entity::mob::{navigation, tick};
use crate::world::entity::{Entity, EntityId};

#[derive(Default)]
pub struct SwellGoal {
    target: Option<EntityId>,
}

impl SwellGoal {
    /// `SwellGoal(creeper)`.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Goal for SwellGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let target = tick::get_target_checked(mob, level).and_then(|t| level.get_entity(t));
        mob.get_swell_dir() > 0 || target.is_some_and(|t| !t.is_dead_or_dying() && mob.base().distance_to_sqr_vec(t.position()) < 9.0)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        navigation::stop(mob, level);
        self.target = tick::get_target_checked(mob, level);
    }

    fn stop(&mut self, _mob: &mut dyn Entity, _level: &mut ServerLevel) {
        self.target = None;
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let dir = match self.target.and_then(|t| level.get_entity(t)).filter(|t| !t.is_dead_or_dying()) {
            Some(target) => {
                if mob.base().distance_to_sqr_vec(target.position()) > 49.0 || !mob.mob().expect("mob").sensing.has_line_of_sight(level, mob, target) {
                    -1
                } else {
                    1
                }
            }
            None => -1,
        };
        mob.set_swell_dir(dir);
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move])
    }
}
