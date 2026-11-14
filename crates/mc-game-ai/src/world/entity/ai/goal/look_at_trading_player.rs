//! `LookAtTradingPlayerGoal`: a `LookAtPlayerGoal` that always faces the player a merchant trades with.

use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::entity::ai::goal::look_at_player::{LookAtPlayerGoal, LookAtType};
use mc_game_core::world::entity::ai::goal::{Goal, GoalFlags};

pub struct LookAtTradingPlayerGoal {
    base: LookAtPlayerGoal,
}

impl LookAtTradingPlayerGoal {
    /// `LookAtTradingPlayerGoal(villager)`: `super(villager, Player.class, 8.0F)`.
    pub fn new(villager: &dyn Entity) -> Self {
        Self {
            base: LookAtPlayerGoal::new(villager, LookAtType::Player, 8.0),
        }
    }
}

impl Goal for LookAtTradingPlayerGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        match mob.as_merchant().and_then(|m| m.get_trading_player()) {
            Some(player) => {
                self.base.look_at = Some(player);
                true
            }
            None => false,
        }
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.base.can_continue_to_use(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.base.start(mob, level);
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.base.stop(mob, level);
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.base.tick(mob, level);
    }

    fn flags(&self) -> GoalFlags {
        self.base.flags()
    }
}
