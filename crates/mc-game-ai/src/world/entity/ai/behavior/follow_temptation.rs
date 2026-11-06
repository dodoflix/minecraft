//! `FollowTemptation`.

use mc_game_core::entry_condition;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::ai::behavior::position_tracker::{EntityTracker, PositionTracker};
use mc_game_core::world::entity::ai::behavior::{Behavior, BehaviorBase};
use mc_game_core::world::entity::ai::memory::WalkTarget;
use mc_game_core::world::entity::ai::memory::memory_module_type::{
    BREED_TARGET, IS_PANICKING, LOOK_TARGET, TEMPTATION_COOLDOWN_TICKS, TEMPTING_PLAYER,
    WALK_TARGET,
};
use mc_game_core::world::entity::ai::sensing::distance_to_sqr;
use mc_game_core::world::entity::entity::Entity;

pub const TEMPTATION_COOLDOWN: i32 = 100;
pub const DEFAULT_CLOSE_ENOUGH_DIST: f64 = 2.5;
pub const BACKED_UP_CLOSE_ENOUGH_DIST: f64 = 3.5;

pub struct FollowTemptation {
    speed_modifier: Box<dyn Fn(&dyn Entity) -> f32>,
    close_enough_distance: Box<dyn Fn(&dyn Entity) -> f64>,
    look_in_the_eyes: bool,
}

impl FollowTemptation {
    /// `new FollowTemptation(speedModifier)`.
    pub fn new(
        speed_modifier: impl Fn(&dyn Entity) -> f32 + 'static,
    ) -> BehaviorBase<FollowTemptation> {
        Self::with_distance(speed_modifier, |_| DEFAULT_CLOSE_ENOUGH_DIST)
    }

    /// `new FollowTemptation(speedModifier, closeEnoughDistance)`.
    pub fn with_distance(
        speed_modifier: impl Fn(&dyn Entity) -> f32 + 'static,
        close_enough_distance: impl Fn(&dyn Entity) -> f64 + 'static,
    ) -> BehaviorBase<FollowTemptation> {
        Self::with_look(speed_modifier, close_enough_distance, false)
    }

    /// `new FollowTemptation(speedModifier, closeEnoughDistance, lookInTheEyes)`.
    pub fn with_look(
        speed_modifier: impl Fn(&dyn Entity) -> f32 + 'static,
        close_enough_distance: impl Fn(&dyn Entity) -> f64 + 'static,
        look_in_the_eyes: bool,
    ) -> BehaviorBase<FollowTemptation> {
        BehaviorBase::new(
            entry_condition![
                LOOK_TARGET => Registered,
                WALK_TARGET => Registered,
                TEMPTATION_COOLDOWN_TICKS => ValueAbsent,
                TEMPTING_PLAYER => ValuePresent,
                BREED_TARGET => ValueAbsent,
                IS_PANICKING => ValueAbsent,
            ],
            FollowTemptation {
                speed_modifier: Box::new(speed_modifier),
                close_enough_distance: Box::new(close_enough_distance),
                look_in_the_eyes,
            },
        )
    }
}

impl Behavior for FollowTemptation {
    fn timed_out(&self, _timestamp: i64, _end_timestamp: i64) -> bool {
        false
    }

    fn can_still_use(
        &mut self,
        _level: &mut ServerLevel,
        body: &mut dyn Entity,
        _timestamp: i64,
    ) -> bool {
        body.brain().has_memory_value(TEMPTING_PLAYER)
            && !body.brain().has_memory_value(BREED_TARGET)
            && !body.brain().has_memory_value(IS_PANICKING)
    }

    fn stop(&mut self, _level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        let brain = body.brain_mut();
        brain.set_memory(&TEMPTATION_COOLDOWN_TICKS, TEMPTATION_COOLDOWN);
        brain.erase_memory(&WALK_TARGET);
        brain.erase_memory(&LOOK_TARGET);
    }

    fn tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        let Some(player) = body
            .brain()
            .get_memory(&TEMPTING_PLAYER)
            .and_then(|p| level.get_entity(p.0))
        else {
            return;
        };
        let look = EntityTracker::new(player, true);
        let walk = EntityTracker::new_full(player, self.look_in_the_eyes, self.look_in_the_eyes);
        let close_enough = (self.close_enough_distance)(&*body);
        let too_close = distance_to_sqr(body, player) < close_enough * close_enough;
        body.brain_mut()
            .set_memory(&LOOK_TARGET, PositionTracker::Entity(look));
        if too_close {
            body.brain_mut().erase_memory(&WALK_TARGET);
        } else {
            let speed = (self.speed_modifier)(&*body);
            body.brain_mut().set_memory(
                &WALK_TARGET,
                WalkTarget::new(PositionTracker::Entity(walk), speed, 2),
            );
        }
    }
}
