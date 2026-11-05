//! `TradeWithPlayerGoal`.

use super::{Flag, Goal, GoalFlags};
use crate::server::level::ServerLevel;
use crate::world::entity::mob::navigation;
use crate::world::entity::Entity;

#[derive(Default)]
pub struct TradeWithPlayerGoal;

impl TradeWithPlayerGoal {
    pub fn new() -> Self {
        Self
    }
}

impl Goal for TradeWithPlayerGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if !mob.is_alive() {
            false
        } else if mob.is_in_water() {
            false
        } else if !mob.base().on_ground() {
            false
        } else if mob.living().is_some_and(|l| l.hurt_time > 0) {
            // `wasHurtRecently()`
            false
        } else {
            match mob.as_abstract_villager().and_then(|v| v.get_trading_player()).and_then(|p| level.get_entity(p)) {
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
