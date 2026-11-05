//! The `AbstractVillager` trading surface used by the generic goals (`LookAtTradingPlayerGoal`, `TradeWithPlayerGoal`).
//! Villagers and wandering traders implement it and expose it through `Entity::as_abstract_villager{,_mut}`.

use crate::world::entity::EntityId;

pub trait AbstractVillagerAccess {
    /// `isTrading()`.
    fn is_trading(&self) -> bool {
        self.get_trading_player().is_some()
    }
    /// `getTradingPlayer()`.
    fn get_trading_player(&self) -> Option<EntityId>;
    /// `setTradingPlayer(player)`.
    fn set_trading_player(&mut self, player: Option<EntityId>);
}
