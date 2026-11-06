//! `LookAndFollowTradingPlayerSink`: stay with the player a villager trades with.

use super::position_tracker::{EntityTracker, PositionTracker};
use super::{Behavior, BehaviorBase};
use crate::entry_condition;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::ai::memory::WalkTarget;
use crate::world::entity::ai::memory::memory_module_type::{LOOK_TARGET, WALK_TARGET};
use crate::world::entity::npc::villager::villager_mut;

pub struct LookAndFollowTradingPlayerSink {
    speed_modifier: f32,
}

impl LookAndFollowTradingPlayerSink {
    pub fn new(speed_modifier: f32) -> BehaviorBase<LookAndFollowTradingPlayerSink> {
        BehaviorBase::with_timeout(
            entry_condition![WALK_TARGET => Registered, LOOK_TARGET => Registered],
            i32::MAX,
            LookAndFollowTradingPlayerSink { speed_modifier },
        )
    }

    fn follow_player(&self, level: &ServerLevel, body: &mut dyn Entity) {
        let Some(player) = villager_mut(body)
            .av
            .trading_player
            .and_then(|id| level.get_entity(id))
        else {
            return;
        };
        let walk = WalkTarget::new(
            PositionTracker::Entity(EntityTracker::new(player, false)),
            self.speed_modifier,
            2,
        );
        let look = PositionTracker::Entity(EntityTracker::new(player, true));
        let brain = body.brain_mut();
        brain.set_memory(&WALK_TARGET, walk);
        brain.set_memory(&LOOK_TARGET, look);
    }
}

impl Behavior for LookAndFollowTradingPlayerSink {
    fn check_extra_start_conditions(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
    ) -> bool {
        let Some(player) = villager_mut(body).av.trading_player else {
            return false;
        };
        let Some(player) = level.get_entity(player) else {
            return false;
        };
        body.is_alive()
            && !body.is_in_water()
            && !body.was_hurt_recently()
            && body.base().distance_to_sqr_vec(player.position()) <= 16.0
    }

    fn can_still_use(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
        _timestamp: i64,
    ) -> bool {
        self.check_extra_start_conditions(level, body)
    }

    fn start(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        self.follow_player(level, body);
    }

    fn stop(&mut self, _level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        let brain = body.brain_mut();
        brain.erase_memory(&WALK_TARGET);
        brain.erase_memory(&LOOK_TARGET);
    }

    fn tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        self.follow_player(level, body);
    }

    fn timed_out(&self, _timestamp: i64, _end_timestamp: i64) -> bool {
        false
    }
}
