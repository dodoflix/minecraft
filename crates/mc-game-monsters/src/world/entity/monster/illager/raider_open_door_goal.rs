//! `AbstractIllager.RaiderOpenDoorGoal`: an `OpenDoorGoal` that only runs while the raider's raid is active.

use mc_game_ai::world::entity::ai::goal::OpenDoorGoal;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::entity::ai::goal::{Goal, GoalFlags};
use mc_game_core::world::entity::raid::raider;

pub struct RaiderOpenDoorGoal {
    open_door: OpenDoorGoal,
}

impl RaiderOpenDoorGoal {
    /// `RaiderOpenDoorGoal(raider)`: `OpenDoorGoal(raider, false)`.
    pub fn new(mob: &dyn Entity) -> Self {
        Self {
            open_door: OpenDoorGoal::new(mob, false),
        }
    }
}

impl Goal for RaiderOpenDoorGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.open_door.can_use(mob, level) && raider::has_active_raid(mob, level)
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.open_door.can_continue_to_use(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.open_door.start(mob, level);
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.open_door.stop(mob, level);
    }

    fn requires_update_every_tick(&self) -> bool {
        self.open_door.requires_update_every_tick()
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.open_door.tick(mob, level);
    }

    fn flags(&self) -> GoalFlags {
        self.open_door.flags()
    }
}
