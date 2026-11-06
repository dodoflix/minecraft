//! Trade generation: `AbstractVillager.addOffersFromTradeSet` / `addOffersFromItemListings(WithoutDuplicates)`.

use mc_core::ident::Identifier;

use super::merchant_offer::MerchantOffers;
use super::trade_set::{TradeSet, get_trade, get_trade_set};
use crate::server::level::ServerLevel;
use crate::world::entity::entity::Entity;
use crate::world::level::storage::loot::context::LootContext;
use crate::world::level::storage::loot::{LootContextParamSets, LootParams};

/// `AbstractVillager.addOffersFromTradeSet(level, offers, resourceKey)`: draws `amount` trades of the trade set `trade_set`
/// into `offers`. `merchant` is the villager / wandering trader (`THIS_ENTITY` of the trades' merchant predicates and
/// `ORIGIN`'s position). A missing trade set adds nothing (vanilla only logs it).
pub fn add_offers_from_trade_set(
    level: &ServerLevel,
    merchant: &dyn Entity,
    offers: &mut MerchantOffers,
    trade_set: &Identifier,
) {
    let Some(set) = get_trade_set(&level.registries, trade_set) else {
        log::debug!("Missing expected trade set {trade_set}");
        return;
    };
    let params = LootParams::builder(level)
        .origin(merchant.position())
        .this_entity(merchant)
        .additional_cost_component_allowed()
        .create(&LootContextParamSets::VILLAGER_TRADE);
    let mut context = LootContext::builder(&params).create(set.random_sequence.as_ref());
    add_offers_from_trade_set_in(&mut context, offers, &set);
}

/// The body of `addOffersFromTradeSet` for an existing loot context (callers that need a specific random source build the
/// context themselves: `LootContext::builder(&params).with_optional_random_seed(..).create(None)`).
pub fn add_offers_from_trade_set_in(
    context: &mut LootContext,
    offers: &mut MerchantOffers,
    set: &TradeSet,
) {
    let number_of_offers = set.calculate_number_of_trades(context);
    let ids = set.trade_ids(context.get_resolver());
    if set.allow_duplicates {
        add_offers_from_item_listings(context, offers, ids, number_of_offers);
    } else {
        add_offers_from_item_listings_without_duplicates(context, offers, ids, number_of_offers);
    }
}

/// [`add_offers_from_trade_set`] into a fresh list.
pub fn generate_offers(
    level: &ServerLevel,
    merchant: &dyn Entity,
    trade_set: &Identifier,
) -> MerchantOffers {
    let mut offers = MerchantOffers::new();
    add_offers_from_trade_set(level, merchant, &mut offers, trade_set);
    offers
}

/// `addOffersFromItemListings`: random picks with replacement; a trade without an offer is dropped from the candidates.
pub fn add_offers_from_item_listings(
    context: &mut LootContext,
    offers: &mut MerchantOffers,
    mut potential: Vec<Identifier>,
    number_of_offers: i32,
) {
    let registries = context.get_resolver();
    let mut found = 0;
    while found < number_of_offers && !potential.is_empty() {
        let roll = context
            .get_random()
            .next_int_bounded(potential.len() as i32) as usize;
        match get_trade(registries, &potential[roll]).and_then(|t| t.get_offer(context)) {
            None => {
                potential.remove(roll);
            }
            Some(offer) => {
                offers.push(offer);
                found += 1;
            }
        }
    }
}

/// `addOffersFromItemListingsWithoutDuplicates`: every candidate is drawn at most once.
pub fn add_offers_from_item_listings_without_duplicates(
    context: &mut LootContext,
    offers: &mut MerchantOffers,
    mut leftover: Vec<Identifier>,
    number_of_offers: i32,
) {
    let registries = context.get_resolver();
    let mut found = 0;
    while found < number_of_offers && !leftover.is_empty() {
        let id =
            leftover.remove(context.get_random().next_int_bounded(leftover.len() as i32) as usize);
        if let Some(offer) = get_trade(registries, &id).and_then(|t| t.get_offer(context)) {
            offers.push(offer);
            found += 1;
        }
    }
}
