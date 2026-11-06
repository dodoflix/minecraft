//! `LookAtTradingPlayerGoal` (`LookAtPlayerGoal` whose target is the trading player).

use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::entity::ai::goal::look_at_player::LookAtPlayerGoal;
use mc_game_core::world::entity::ai::goal::{Goal, GoalFlags};

pub struct LookAtTradingPlayerGoal {
    pub base: LookAtPlayerGoal,
}

impl LookAtTradingPlayerGoal {
    /// `LookAtTradingPlayerGoal(villager)`: `super(villager, Player.class, 8.0F)`.
    pub fn new(villager: &dyn Entity) -> Self {
        Self {
            base: LookAtPlayerGoal::new_player(villager, 8.0),
        }
    }
}

impl Goal for LookAtTradingPlayerGoal {
    fn can_use(&mut self, villager: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        match villager.as_abstract_villager() {
            Some(v) if v.is_trading() => {
                self.base.look_at = v.get_trading_player();
                true
            }
            _ => false,
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
