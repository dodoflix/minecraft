//! `OpenDoorGoal`.

use mc_game_core::world::entity::ai::goal::door_interact::DoorInteractData;
use mc_game_core::world::entity::ai::goal::{Goal, GoalFlags};
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::Entity;

pub struct OpenDoorGoal {
    pub base: DoorInteractData,
    close_door: bool,
    forget_time: i32,
}

impl OpenDoorGoal {
    /// `OpenDoorGoal(mob, closeDoorAfter)`.
    pub fn new(mob: &dyn Entity, close_door_after: bool) -> Self {
        Self { base: DoorInteractData::new(mob), close_door: close_door_after, forget_time: 0 }
    }
}

impl Goal for OpenDoorGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.base.can_use(mob, level)
    }

    fn can_continue_to_use(&mut self, _mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        self.close_door && self.forget_time > 0 && self.base.can_continue_to_use()
    }

    // Vanilla's `start()` does not call `super.start()`.
    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.forget_time = 20;
        self.base.set_open(mob, level, true);
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.base.set_open(mob, level, false);
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        self.forget_time -= 1;
        self.base.tick(mob);
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::NONE
    }
}
