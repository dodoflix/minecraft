//! `net.minecraft.world.item.trading`: offers, costs and the data-driven villager trades.
//!
//! # Trades are data
//! In 26.x the trades live in the `villager_trade` and `trade_set` data-pack registries (decoded here by
//! [`villager_trade`] / [`trade_set`] through `DYNAMIC_REGISTRY_DECODERS`, so `RegistryAccess::get::<Arc<VillagerTrade>>` and
//! `get::<Arc<TradeSet>>` work after loading). `VillagerTrades`/`TradeRebalanceVillagerTrades`/`TradeSets.bootstrap` only
//! generate that data (`data/minecraft/villager_trade`, `trade_set`, `tags/villager_trade`, the `trade_rebalance` pack) and
//! have no runtime counterpart.
//!
//! # How a villager / wandering trader calls generation
//! Vanilla's `AbstractVillager.addOffersFromTradeSet(level, offers, key)` is
//! [`add_offers_from_trade_set`]`(&level, &*self, &mut offers, &key)`:
//!
//! * `key` is the trade set id: for a `Villager`, `profession.getTrades(villagerData.level())` =
//!   [`trade_sets::profession_trade_set`]`(profession_id, level)` (`None` = no trades, call nothing); for a
//!   `WanderingTrader`, the three sets [`trade_sets::WANDERING_TRADER_BUYING`], `..._UNCOMMON`, `..._COMMON` (in that order,
//!   through [`trade_sets::trade_set_key`]) added to the same `offers`.
//! * `merchant` is the villager itself as `&dyn Entity` (position = `ORIGIN`, entity = `THIS_ENTITY`). The `merchant_predicate`
//!   of trades (explorer maps by biome type: `minecraft:villager/variant`) reads the villager's component view, so the
//!   villager MUST return `VILLAGER_VARIANT` from `Entity::entity_components()`.
//! * The random is the trade set's `random_sequence` of the level (`level.server_state.loot.random_sequences`), exactly like
//!   vanilla's `LootContext.Builder.create(tradeSet.randomSequence())`; the caller passes none.
//! * `level` is `&ServerLevel` (shared): reborrow `&*level` from the `&mut ServerLevel` of goals/behaviors. Explorer maps go
//!   through `loot::map_api::explore` (`exploration_map` loot function).
//!
//! The villager then applies what is *not* generation: `Villager::updateTrades` ordering with `increaseMerchantCareer`, special
//! prices from gossip, restock/demand (`MerchantOffer::update_demand`, `reset_uses`, ...) and the `Offers` NBT
//! ([`MerchantOffers::codec`], saved form) / `ClientboundMerchantOffersPacket` ([`MerchantOffers::to_wire`]).

pub mod generation;
pub mod item_cost;
pub mod merchant;
pub mod merchant_offer;
pub mod trade_cost;
pub mod trade_set;
pub mod trade_sets;
pub mod villager_trade;

pub use generation::{add_offers_from_item_listings, add_offers_from_trade_set_in, add_offers_from_item_listings_without_duplicates, add_offers_from_trade_set, generate_offers};
pub use item_cost::ItemCost;
pub use merchant::{Merchant, open_trading_screen, with_merchant};
pub use merchant_offer::{MerchantOffer, MerchantOffers};
pub use trade_cost::TradeCost;
pub use trade_set::{TradeSet, get_trade, get_trade_set};
pub use villager_trade::VillagerTrade;
