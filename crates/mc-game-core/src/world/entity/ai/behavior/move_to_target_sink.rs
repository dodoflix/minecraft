//! `MoveToTargetSink`: walks the navigation towards the `WALK_TARGET` memory.

use super::position_tracker::PositionTracker;
use super::{Behavior, BehaviorBase};
use crate::entry_condition;
use crate::server::level::ServerLevel;
use crate::world::entity::ai::memory::memory_module_type::{CANT_REACH_WALK_TARGET_SINCE, PATH, WALK_TARGET};
use crate::world::entity::ai::memory::WalkTarget;
use crate::world::entity::ai::util::default_random_pos;
use crate::world::entity::entity::Entity;
use crate::world::entity::mob::navigation;
use crate::world::level::pathfinder::Path;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;

/// `MAX_COOLDOWN_BEFORE_RETRYING`.
const MAX_COOLDOWN_BEFORE_RETRYING: i32 = 40;

pub struct MoveToTargetSink {
    remaining_cooldown: i32,
    path: Option<Path>,
    last_target_pos: Option<BlockPos>,
    speed_modifier: f32,
}

impl MoveToTargetSink {
    /// `new MoveToTargetSink()`.
    pub fn new() -> BehaviorBase<MoveToTargetSink> {
        Self::with_timeout(150, 250)
    }

    /// `new MoveToTargetSink(minTimeout, maxTimeout)`.
    pub fn with_timeout(min_timeout: i32, max_timeout: i32) -> BehaviorBase<MoveToTargetSink> {
        BehaviorBase::with_duration(
            entry_condition![CANT_REACH_WALK_TARGET_SINCE => Registered, PATH => ValueAbsent, WALK_TARGET => ValuePresent],
            min_timeout,
            max_timeout,
            MoveToTargetSink { remaining_cooldown: 0, path: None, last_target_pos: None, speed_modifier: 0.0 },
        )
    }

    fn try_compute_path(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, walk_target: &WalkTarget, timestamp: i64) -> bool {
        let target_pos = walk_target.get_target().current_block_position(level);
        self.path = navigation::create_path_pos(body, level, target_pos, 0);
        self.speed_modifier = walk_target.get_speed_modifier();
        if self.reached_target(level, body, walk_target) {
            body.brain_mut().erase_memory(&CANT_REACH_WALK_TARGET_SINCE);
        } else {
            let can_reach = self.path.as_ref().is_some_and(|p| p.can_reach());
            if can_reach {
                body.brain_mut().erase_memory(&CANT_REACH_WALK_TARGET_SINCE);
            } else if !body.brain().has_memory_value(CANT_REACH_WALK_TARGET_SINCE) {
                body.brain_mut().set_memory(&CANT_REACH_WALK_TARGET_SINCE, timestamp);
            }
            if self.path.is_some() {
                return true;
            }
            let partial_step = default_random_pos::get_pos_towards(body, level, 10, 7, Vec3::at_bottom_center_of(&target_pos), f64::from(std::f32::consts::FRAC_PI_2));
            if let Some(partial_step) = partial_step {
                self.path = navigation::create_path_xyz(body, level, partial_step.x, partial_step.y, partial_step.z, 0);
                return self.path.is_some();
            }
        }
        false
    }

    fn reached_target(&self, level: &ServerLevel, body: &dyn Entity, walk_target: &WalkTarget) -> bool {
        walk_target.get_target().current_block_position(level).dist_manhattan(&body.block_position()) <= walk_target.get_close_enough_dist()
    }

    fn is_walk_target_spectator(level: &ServerLevel, walk_target: &WalkTarget) -> bool {
        match walk_target.get_target() {
            PositionTracker::Entity(t) => level.get_entity(t.get_entity()).is_some_and(|e| e.is_spectator()),
            PositionTracker::Block(_) => false,
        }
    }
}

impl Behavior for MoveToTargetSink {
    fn check_extra_start_conditions(&mut self, level: &mut ServerLevel, body: &mut dyn Entity) -> bool {
        if self.remaining_cooldown > 0 {
            self.remaining_cooldown -= 1;
            return false;
        }
        let walk_target = body.brain().get_memory(&WALK_TARGET).cloned().expect("walk target present");
        let reached_target = self.reached_target(level, body, &walk_target);
        if !reached_target && self.try_compute_path(level, body, &walk_target, level.game_time()) {
            self.last_target_pos = Some(walk_target.get_target().current_block_position(level));
            true
        } else {
            body.brain_mut().erase_memory(&WALK_TARGET);
            if reached_target {
                body.brain_mut().erase_memory(&CANT_REACH_WALK_TARGET_SINCE);
            }
            false
        }
    }

    fn can_still_use(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) -> bool {
        if self.path.is_some() && self.last_target_pos.is_some() {
            let walk_target = body.brain().get_memory(&WALK_TARGET).cloned();
            let is_spectator = walk_target.as_ref().is_some_and(|w| Self::is_walk_target_spectator(level, w));
            !navigation::is_done(body, level) && walk_target.is_some_and(|w| !self.reached_target(level, body, &w)) && !is_spectator
        } else {
            false
        }
    }

    fn stop(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        if body.brain().has_memory_value(WALK_TARGET)
            && body.brain().get_memory(&WALK_TARGET).is_some_and(|w| !self.reached_target(level, body, w))
            && navigation::with_navigation_ref(body, level, |n| n.is_stuck()).unwrap_or(false)
        {
            self.remaining_cooldown = level.tick_state.random.next_int_bounded(MAX_COOLDOWN_BEFORE_RETRYING);
        }
        navigation::stop(body, level);
        body.brain_mut().erase_memory(&WALK_TARGET);
        body.brain_mut().erase_memory(&PATH);
        self.path = None;
    }

    fn start(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        body.brain_mut().set_memory_opt(&PATH, self.path.clone());
        navigation::move_to_path(body, level, self.path.clone(), f64::from(self.speed_modifier));
    }

    fn tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64) {
        let new_path = navigation::with_navigation_ref(body, level, |n| n.get_path()).flatten();
        let same = match (&self.path, &new_path) {
            (None, None) => true,
            (Some(a), Some(b)) => a.ptr_eq(b),
            _ => false,
        };
        if !same {
            self.path = new_path.clone();
            body.brain_mut().set_memory_opt(&PATH, new_path.clone());
        }
        if new_path.is_some()
            && let Some(last) = self.last_target_pos
        {
            let walk_target = body.brain().get_memory(&WALK_TARGET).cloned().expect("walk target present");
            if walk_target.get_target().current_block_position(level).dist_sqr(&last) > 4.0 && self.try_compute_path(level, body, &walk_target, level.game_time()) {
                self.last_target_pos = Some(walk_target.get_target().current_block_position(level));
                self.start(level, body, timestamp);
            }
        }
    }
}
