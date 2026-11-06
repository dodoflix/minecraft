//! `TradeWithPlayerGoal`.

use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::entity::ai::goal::{Flag, Goal, GoalFlags};
use mc_game_core::world::entity::mob::navigation;

#[derive(Default)]
pub struct TradeWithPlayerGoal;

impl TradeWithPlayerGoal {
    pub fn new() -> Self {
        Self
    }
}

impl Goal for TradeWithPlayerGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if !mob.is_alive()
            || mob.is_in_water()
            || !mob.base().on_ground()
            // `wasHurtRecently()`
            || mob.living().is_some_and(|l| l.hurt_time > 0)
        {
            false
        } else {
            match mob
                .as_abstract_villager()
                .and_then(|v| v.get_trading_player())
                .and_then(|p| level.get_entity(p))
            {
                None => false,
                Some(trader) => !(mob.base().distance_to_sqr_vec(trader.position()) > 16.0),
            }
        }
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        navigation::stop(mob, level);
    }

    fn stop(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        if let Some(v) = mob.as_abstract_villager_mut() {
            v.set_trading_player(None);
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Jump, Flag::Move])
    }
}
