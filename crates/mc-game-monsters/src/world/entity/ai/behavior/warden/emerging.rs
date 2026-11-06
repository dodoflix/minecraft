//! `Emerging`.

use mc_data::SoundEvent;
use mc_data::enums::Pose;
use mc_game_core::entry_condition;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::entity::ai::behavior::{Behavior, BehaviorBase};
use mc_game_core::world::entity::ai::memory::memory_module_type::{
    IS_EMERGING, LOOK_TARGET, WALK_TARGET,
};

/// `Emerging<E extends Warden>`.
pub struct Emerging;

impl Emerging {
    /// `new Emerging(ticks)`.
    pub fn new(ticks: i32) -> BehaviorBase<Emerging> {
        BehaviorBase::with_timeout(
            entry_condition![IS_EMERGING => ValuePresent, WALK_TARGET => ValueAbsent, LOOK_TARGET => Registered],
            ticks,
            Emerging,
        )
    }
}

impl Behavior for Emerging {
    fn can_still_use(
        &mut self,
        _level: &mut ServerLevel,
        _body: &mut dyn Entity,
        _timestamp: i64,
    ) -> bool {
        true
    }

    fn start(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        body.set_pose(Pose::Emerging);
        body.play_sound(level, SoundEvent::ENTITY_WARDEN_EMERGE, 5.0, 1.0);
    }

    fn stop(&mut self, _level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        if body.base().get_pose() == Pose::Emerging {
            body.set_pose(Pose::Standing);
        }
    }
}
