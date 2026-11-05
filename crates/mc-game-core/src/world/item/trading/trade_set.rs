//! `TradeSet`: a pool of trades and how many of them a merchant draws (the `trade_set` registry) and its decoder.

use std::sync::Arc;

use mc_core::ident::Identifier;
use mc_dfu::extra::IDENTIFIER;
use mc_dfu::{BOOL, Dyn, JSON, record};
use mc_protocol::components::{HolderSet, NoDirect};
use serde_json::Value;

use super::villager_trade::VillagerTrade;
use crate::registry::codec::RegistryDecodeContext;
use crate::registry::decoders::{DYNAMIC_REGISTRY_DECODERS, DecodedEntry, DynamicRegistryDecoder};
use crate::registry::{RegistryAccess, keys};
use crate::world::item::enchantment::codecs::{holder_set_codec, with_decode_context as with_scoped_context};
use crate::world::level::storage::loot::context::LootContext;
use crate::world::level::storage::loot::holder::{holder_set_ids, with_decode_context};
use crate::world::level::storage::loot::providers::number::{IntHolder, int_holder_codec};

/// `TradeSet`.
#[derive(Clone)]
pub struct TradeSet {
    pub trades: HolderSet<NoDirect>,
    pub amount: IntHolder,
    pub allow_duplicates: bool,
    pub random_sequence: Option<Identifier>,
}

impl TradeSet {
    /// `TradeSet.CODEC`.
    pub fn codec() -> mc_dfu::Codec<TradeSet> {
        record!(TradeSet {
            trades: holder_set_codec::<NoDirect>(keys::VILLAGER_TRADE).field_of("trades"),
            amount: int_holder_codec().field_of("amount"),
            allow_duplicates: BOOL.optional_field_of_default("allow_duplicates", false),
            random_sequence: IDENTIFIER.optional_field_of("random_sequence"),
        })
        .codec()
    }

    /// `trades()` in iteration order: the ids of the member trades (a tag expands in its tag order).
    pub fn trade_ids(&self, registries: &RegistryAccess) -> Vec<Identifier> {
        holder_set_ids(registries, keys::VILLAGER_TRADE, &self.trades)
    }

    /// `calculateNumberOfTrades(lootContext)`.
    pub fn calculate_number_of_trades(&self, context: &mut LootContext) -> i32 {
        self.amount.value(context.get_resolver()).get_int(context)
    }
}

fn decode(json: &Value, ctx: &RegistryDecodeContext) -> Result<DecodedEntry, String> {
    let input = Dyn::from_json_str(&json.to_string())?;
    let set = with_decode_context(ctx, || with_scoped_context(ctx, || TradeSet::codec().parse(&JSON, &input))).into_result()?;
    Ok(DecodedEntry::new(Arc::new(set)))
}

#[linkme::distributed_slice(DYNAMIC_REGISTRY_DECODERS)]
static TRADE_SET_DECODER: DynamicRegistryDecoder = DynamicRegistryDecoder { registry: keys::TRADE_SET, decode };

/// The `villager_trade` entry `id`.
pub fn get_trade(registries: &RegistryAccess, id: &Identifier) -> Option<Arc<VillagerTrade>> {
    registries.get::<Arc<VillagerTrade>>(keys::VILLAGER_TRADE, id).cloned()
}

/// The `trade_set` entry `id`.
pub fn get_trade_set(registries: &RegistryAccess, id: &Identifier) -> Option<Arc<TradeSet>> {
    registries.get::<Arc<TradeSet>>(keys::TRADE_SET, id).cloned()
}
