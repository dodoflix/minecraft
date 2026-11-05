//! `TradeSets`: the keys of the vanilla `trade_set` entries (their contents are data: `data/minecraft/trade_set`).

use mc_core::ident::Identifier;

/// `TradeSets.WANDERING_TRADER_BUYING`.
pub const WANDERING_TRADER_BUYING: &str = "wandering_trader/buying";
/// `TradeSets.WANDERING_TRADER_COMMON`.
pub const WANDERING_TRADER_COMMON: &str = "wandering_trader/common";
/// `TradeSets.WANDERING_TRADER_UNCOMMON`.
pub const WANDERING_TRADER_UNCOMMON: &str = "wandering_trader/uncommon";

/// `TradeSets.resourceKey(path)`: the id of the vanilla trade set at `path`.
pub fn trade_set_key(path: &str) -> Identifier {
    Identifier::new("minecraft", path).expect("valid trade set path")
}

/// The professions that trade (`VillagerProfession.tradeSetsByLevel`).
const TRADING_PROFESSIONS: [&str; 13] =
    ["armorer", "butcher", "cartographer", "cleric", "farmer", "fisherman", "fletcher", "leatherworker", "librarian", "mason", "shepherd", "toolsmith", "weaponsmith"];

/// `VillagerProfession.getTrades(level)` for the vanilla professions: `<profession>/level_<level>` for levels 1..=5, `None` for
/// professions without trades (`none`, `nitwit`) and other levels.
pub fn profession_trade_set(profession: &Identifier, level: i32) -> Option<Identifier> {
    if profession.namespace() != "minecraft" || !(1..=5).contains(&level) || !TRADING_PROFESSIONS.contains(&profession.path()) {
        return None;
    }
    Some(trade_set_key(&format!("{}/level_{level}", profession.path())))
}
