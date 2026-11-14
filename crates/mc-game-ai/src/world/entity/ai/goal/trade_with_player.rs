//! `TradeWithPlayerGoal`: a merchant holds still while a player trades with it.

use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::entity::ai::goal::{Flag, Goal, GoalFlags};
use mc_game_core::world::entity::mob::navigation;

#[derive(Default)]
pub struct TradeWithPlayerGoal;

impl TradeWithPlayerGoal {
    /// `TradeWithPlayerGoal(mob)`.
    pub fn new() -> Self {
        Self
    }
}

impl Goal for TradeWithPlayerGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if !mob.is_alive()
            || mob.is_in_water()
            || !mob.base().on_ground()
            || mob.was_hurt_recently()
        {
            return false;
        }
        let Some(trader) = mob.as_merchant().and_then(|m| m.get_trading_player()) else {
            return false;
        };
        level
            .get_entity(trader)
            .is_some_and(|p| mob.base().distance_to_sqr_vec(p.position()) <= 16.0)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        navigation::stop(mob, level);
    }

    fn stop(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        if let Some(m) = mob.as_merchant_mut() {
            m.set_trading_player(None);
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Jump, Flag::Move])
    }
}
