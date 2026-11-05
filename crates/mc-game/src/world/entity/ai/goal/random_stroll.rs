//! `RandomStrollGoal`.

use mc_core::random::RandomSource;
use super::{Flag, Goal, GoalFlags, reduced_tick_delay};
use crate::server::level::ServerLevel;
use crate::world::entity::ai::util::default_random_pos;
use crate::world::entity::living_entity::util::l;
use crate::world::entity::mob::navigation;
use crate::world::entity::Entity;
use mc_core::phys::Vec3;

pub const DEFAULT_INTERVAL: i32 = 120;

/// `RandomStrollGoal`; subclasses embed it and substitute `get_position` (a closure here).
pub struct RandomStrollGoal {
    pub wanted_x: f64,
    pub wanted_y: f64,
    pub wanted_z: f64,
    pub speed_modifier: f64,
    pub interval: i32,
    pub force_trigger: bool,
    check_no_action_time: bool,
}

impl RandomStrollGoal {
    pub fn new(speed_modifier: f64) -> Self {
        Self::with_interval(speed_modifier, DEFAULT_INTERVAL)
    }

    pub fn with_interval(speed_modifier: f64, interval: i32) -> Self {
        Self::with_check(speed_modifier, interval, true)
    }

    pub fn with_check(speed_modifier: f64, interval: i32, check_no_action_time: bool) -> Self {
        Self { wanted_x: 0.0, wanted_y: 0.0, wanted_z: 0.0, speed_modifier, interval, force_trigger: false, check_no_action_time }
    }

    /// `getPosition()` of the base class.
    pub fn default_position(mob: &mut dyn Entity, level: &mut ServerLevel) -> Option<Vec3> {
        default_random_pos::get_pos(mob, level, 10, 7)
    }

    /// `canUse()` with `getPosition` supplied by the (sub)class.
    pub fn can_use_with(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel, get_position: impl FnOnce(&mut dyn Entity, &mut ServerLevel) -> Option<Vec3>) -> bool {
        if mob.has_controlling_passenger() {
            return false;
        }
        if !self.force_trigger {
            if self.check_no_action_time && l(mob).no_action_time >= 100 {
                return false;
            }
            if mob.base_mut().random.next_int_bounded(reduced_tick_delay(self.interval)) != 0 {
                return false;
            }
        }
        let Some(pos) = get_position(mob, level) else { return false };
        self.wanted_x = pos.x;
        self.wanted_y = pos.y;
        self.wanted_z = pos.z;
        self.force_trigger = false;
        true
    }

    pub fn can_continue_to_use_impl(&self, mob: &dyn Entity, level: &ServerLevel) -> bool {
        !navigation::is_done(mob, level) && !mob.has_controlling_passenger()
    }

    pub fn start_impl(&self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        navigation::move_to(mob, level, self.wanted_x, self.wanted_y, self.wanted_z, self.speed_modifier);
    }

    pub fn stop_impl(&self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        navigation::stop(mob, level);
    }

    /// `trigger()`.
    pub fn trigger(&mut self) {
        self.force_trigger = true;
    }

    /// `setInterval(interval)`.
    pub fn set_interval(&mut self, interval: i32) {
        self.interval = interval;
    }
}

impl Goal for RandomStrollGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.can_use_with(mob, level, Self::default_position)
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.can_continue_to_use_impl(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.start_impl(mob, level);
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.stop_impl(mob, level);
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move])
    }
}
