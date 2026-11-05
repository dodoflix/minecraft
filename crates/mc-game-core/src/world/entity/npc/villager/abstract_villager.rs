//! `AbstractVillager`: what `Villager` and `WanderingTrader` share (trading player, offers, inventory, unhappy counter).
//!
//! The concrete entities embed [`AbstractVillagerData`] and call these functions for the non-overridden parts, like the
//! other mob layers do (`mob::nbt::add_additional_save_data`, ...).

use crate::define_accessor;
use crate::server::level::ServerLevel;
use crate::world::container::SimpleContainer;
use crate::world::entity::mob::{self, MobData};
use crate::world::entity::npc::inventory_carrier;
use crate::world::entity::{Entity, EntityId};
use crate::world::item::trading::{self, MerchantOffers};
use crate::world::level::pathfinder::PathType;
use mc_core::ident::Identifier;
use mc_nbt::CompoundTag;

define_accessor!(pub DATA_UNHAPPY_COUNTER: Int = AbstractVillager);

/// The fields `AbstractVillager` adds to `AgeableMob`.
pub struct AbstractVillagerData {
    /// `tradingPlayer`.
    pub trading_player: Option<EntityId>,
    /// `offers` (`None` until first `getOffers()` / load).
    pub offers: Option<MerchantOffers>,
    /// `inventory` (`new SimpleContainer(8)`).
    pub inventory: SimpleContainer,
}

impl AbstractVillagerData {
    pub fn new() -> Self {
        Self { trading_player: None, offers: None, inventory: SimpleContainer::new(8) }
    }
}

impl Default for AbstractVillagerData {
    fn default() -> Self {
        Self::new()
    }
}

/// The `AbstractVillager` constructor body (after `AgeableMob`).
pub fn init(mob: &mut MobData) {
    mob.set_pathfinding_malus(PathType::FireInNeighbor, 16.0);
    mob.set_pathfinding_malus(PathType::Fire, -1.0);
}

/// `getUnhappyCounter()`.
pub fn get_unhappy_counter(e: &dyn Entity) -> i32 {
    e.base().entity_data.get(DATA_UNHAPPY_COUNTER)
}

/// `setUnhappyCounter(value)`.
pub fn set_unhappy_counter(e: &mut dyn Entity, value: i32) {
    e.base_mut().entity_data.set(DATA_UNHAPPY_COUNTER, value);
}

/// `AbstractVillager.addAdditionalSaveData` (after `super`).
pub fn add_additional_save_data(data: &AbstractVillagerData, out: &mut CompoundTag) {
    if let Some(offers) = &data.offers {
        if let Some(tag) = mc_dfu::encode_nbt(&MerchantOffers::codec(), offers).into_result().ok() {
            out.put("Offers", tag);
        }
    }
    inventory_carrier::write_inventory_to_tag(&data.inventory, out);
}

/// `AbstractVillager.readAdditionalSaveData` (after `super`).
pub fn read_additional_save_data(data: &mut AbstractVillagerData, input: &CompoundTag) {
    data.offers = input.get("Offers").and_then(|t| mc_dfu::parse_nbt(&MerchantOffers::codec(), t).into_result().ok());
    inventory_carrier::read_inventory_from_tag(&mut data.inventory, input);
}

/// `addOffersFromTradeSet(level, offers, key)`.
pub fn add_offers_from_trade_set(level: &ServerLevel, merchant: &dyn Entity, offers: &mut MerchantOffers, key: &Identifier) {
    trading::add_offers_from_trade_set(level, merchant, offers, key);
}

/// `ambientSoundTime = -getAmbientSoundInterval()`.
pub fn reset_ambient_sound_time(e: &mut dyn Entity) {
    mob::tick::reset_ambient_sound_time(e);
}
