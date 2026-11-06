//! `Croak` (the frog's croaking behavior; only needs the pose of its body).

use mc_data::enums::Pose;
use mc_game_core::entry_condition;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::ai::behavior::{Behavior, BehaviorBase};
use mc_game_core::world::entity::ai::memory::memory_module_type::WALK_TARGET;
use mc_game_core::world::entity::entity::Entity;

const CROAK_TICKS: i32 = 60;
const TIME_OUT_DURATION: i32 = 100;

pub struct Croak {
    croak_counter: i32,
}

impl Croak {
    /// `new Croak()`.
    pub fn new() -> BehaviorBase<Croak> {
        BehaviorBase::with_timeout(
            entry_condition![WALK_TARGET => ValueAbsent],
            TIME_OUT_DURATION,
            Croak { croak_counter: 0 },
        )
    }
}

impl Behavior for Croak {
    fn check_extra_start_conditions(
        &mut self,
        _level: &mut ServerLevel,
        body: &mut dyn Entity,
    ) -> bool {
        body.base().get_pose() == Pose::Standing
    }

    fn can_still_use(
        &mut self,
        _level: &mut ServerLevel,
        _body: &mut dyn Entity,
        _timestamp: i64,
    ) -> bool {
        self.croak_counter < CROAK_TICKS
    }

    fn start(&mut self, _level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        if !body.is_in_liquid() {
            body.set_pose(Pose::Croaking);
            self.croak_counter = 0;
        }
    }

    fn stop(&mut self, _level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        body.set_pose(Pose::Standing);
    }

    fn tick(&mut self, _level: &mut ServerLevel, _body: &mut dyn Entity, _timestamp: i64) {
        self.croak_counter += 1;
    }
}
