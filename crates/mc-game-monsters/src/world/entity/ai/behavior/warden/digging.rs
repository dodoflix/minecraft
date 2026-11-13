//! `Digging`.

use mc_data::SoundEvent;
use mc_data::enums::Pose;
use mc_game_core::entry_condition;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::entity::RemovalReason;
use mc_game_core::world::entity::ai::behavior::{Behavior, BehaviorBase};
use mc_game_core::world::entity::ai::memory::memory_module_type::{ATTACK_TARGET, WALK_TARGET};

/// `Digging<E extends Warden>`.
pub struct Digging;

impl Digging {
    /// `new Digging(ticks)`.
    pub fn new(ticks: i32) -> BehaviorBase<Digging> {
        BehaviorBase::with_timeout(
            entry_condition![ATTACK_TARGET => ValueAbsent, WALK_TARGET => ValueAbsent],
            ticks,
            Digging,
        )
    }
}

impl Behavior for Digging {
    fn can_still_use(
        &mut self,
        _level: &mut ServerLevel,
        body: &mut dyn Entity,
        _timestamp: i64,
    ) -> bool {
        body.base().removal_reason().is_none()
    }

    fn check_extra_start_conditions(
        &mut self,
        _level: &mut ServerLevel,
        body: &mut dyn Entity,
    ) -> bool {
        body.base().on_ground() || body.is_in_liquid()
    }

    fn start(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64) {
        if body.base().on_ground() {
            body.set_pose(Pose::Digging);
            body.play_sound(level, SoundEvent::ENTITY_WARDEN_DIG, 5.0, 1.0);
        } else {
            body.play_sound(level, SoundEvent::ENTITY_WARDEN_AGITATED, 5.0, 1.0);
            self.stop(level, body, timestamp);
        }
    }

    fn stop(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        if body.base().removal_reason().is_none() {
            level.remove_held_entity(body, RemovalReason::Discarded);
        }
    }
}
