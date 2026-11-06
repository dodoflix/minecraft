//! `VillagerTrade`: one data-driven trade (the `villager_trade` registry) and its decoder.

use std::sync::{Arc, LazyLock};

use mc_dfu::{Dyn, JSON, record};
use mc_protocol::components::values::basic::{ADDITIONAL_TRADE_COST, STORED_ENCHANTMENTS};
use mc_protocol::components::values::predicates::data_driven_set_codec;
use mc_protocol::components::{HolderSet, ItemStackTemplate, NoDirect};
use serde_json::Value;

use super::item_cost::ItemCost;
use super::merchant_offer::MerchantOffer;
use super::trade_cost::{TradeCost, float_holder_field, int_holder_field};
use crate::registry::codec::RegistryDecodeContext;
use crate::registry::decoders::{DYNAMIC_REGISTRY_DECODERS, DecodedEntry, DynamicRegistryDecoder};
use crate::registry::{RegistryAccess, keys};
use crate::world::level::storage::loot::LootContextParamSets;
use crate::world::level::storage::loot::context::LootContext;
use crate::world::level::storage::loot::functions::{FunctionHolder, function_holder_codec};
use crate::world::level::storage::loot::holder::{holder_set_ids, with_decode_context};
use crate::world::level::storage::loot::predicates::{ConditionHolder, condition_holder_codec};
use crate::world::level::storage::loot::providers::number::{FloatHolder, IntHolder};
use crate::world::level::storage::loot::validation::{
    ProblemCollector, Validatable, ValidationContext, validate_holder_field, validate_holder_opt,
    validate_opt,
};

/// `VillagerTrade`.
#[derive(Clone)]
pub struct VillagerTrade {
    wants: TradeCost,
    additional_wants: Option<TradeCost>,
    gives: ItemStackTemplate,
    max_uses: IntHolder,
    xp: IntHolder,
    reputation_discount: FloatHolder,
    merchant_predicate: Option<ConditionHolder>,
    given_item_modifier: Option<FunctionHolder>,
    double_trade_price_enchantments: Option<HolderSet<NoDirect>>,
}

impl VillagerTrade {
    /// `VillagerTrade.CODEC` (with the `VILLAGER_TRADE` context validation).
    pub fn codec() -> mc_dfu::Codec<VillagerTrade> {
        static C: LazyLock<mc_dfu::Codec<VillagerTrade>> = LazyLock::new(|| {
            record!(VillagerTrade {
                wants: TradeCost::codec().field_of("wants"),
                additional_wants: TradeCost::codec().optional_field_of("additional_wants"),
                gives: ItemStackTemplate::codec().field_of("gives"),
                max_uses: int_holder_field("max_uses", 4),
                xp: int_holder_field("xp", 1),
                reputation_discount: float_holder_field("reputation_discount", 0.0),
                merchant_predicate: condition_holder_codec()
                    .optional_field_of("merchant_predicate"),
                given_item_modifier: function_holder_codec()
                    .optional_field_of("given_item_modifier"),
                double_trade_price_enchantments: data_driven_set_codec(keys::ENCHANTMENT)
                    .optional_field_of("double_trade_price_enchantments"),
            })
            .codec()
            .validate(|trade: VillagerTrade| {
                // `Validatable.validatorForContext(VILLAGER_TRADE)`; holders are only entered when they resolve, and nothing is
                // resolvable while the registry is being decoded
                static NO_REGISTRIES: LazyLock<RegistryAccess> =
                    LazyLock::new(|| RegistryAccess::from_registries(std::iter::empty()));
                let problems = ProblemCollector::new();
                trade.validate(&ValidationContext::new(
                    &problems,
                    LootContextParamSets::VILLAGER_TRADE,
                    &NO_REGISTRIES,
                ));
                if problems.is_empty() {
                    mc_dfu::DataResult::success(trade)
                } else {
                    let message = format!("Validation error: {}", problems.get_report());
                    mc_dfu::DataResult::error(move || message.clone())
                }
            })
        });
        C.clone()
    }

    /// `getOffer(lootContext)`: `None` when the merchant predicate fails or the result/costs turn out empty.
    pub fn get_offer(&self, context: &mut LootContext) -> Option<MerchantOffer> {
        if let Some(predicate) = &self.merchant_predicate {
            if !predicate.value(context.get_resolver()).test(context) {
                return None;
            }
        }
        let mut result = self.gives.create();
        let mut additional_cost: i32 = 0;
        if let Some(modifier) = &self.given_item_modifier {
            result = modifier
                .value(context.get_resolver())
                .apply(result, context);
            if result.is_empty() {
                return None;
            }
        }
        if let Some(extra) = result.get(&ADDITIONAL_TRADE_COST).copied() {
            result.remove(ADDITIONAL_TRADE_COST.id());
            additional_cost = additional_cost.wrapping_add(extra);
        }
        if let Some(set) = &self.double_trade_price_enchantments {
            if let Some(stored) = result.get(&STORED_ENCHANTMENTS) {
                let ids = holder_set_ids(context.get_resolver(), keys::ENCHANTMENT, set);
                if stored
                    .entries()
                    .iter()
                    .any(|(e, _)| e.name().is_some_and(|n| ids.contains(n)))
                {
                    additional_cost = additional_cost.wrapping_mul(2);
                }
            }
        }
        let item_cost: ItemCost = self.wants.to_item_cost(context, additional_cost);
        if item_cost.count() < 1 {
            return None;
        }
        let additional_item_cost = self
            .additional_wants
            .as_ref()
            .map(|c| c.to_item_cost(context, 0));
        if additional_item_cost.as_ref().is_some_and(|c| c.count() < 1) {
            return None;
        }
        let max_uses = self
            .max_uses
            .value(context.get_resolver())
            .get_int(context)
            .max(1);
        let xp = self
            .xp
            .value(context.get_resolver())
            .get_int(context)
            .max(0);
        let discount = self
            .reputation_discount
            .value(context.get_resolver())
            .get_float(context);
        // `Math.max(discount, 0.0F)`
        let discount = if discount > 0.0 { discount } else { 0.0 };
        Some(MerchantOffer::with_cost_b(
            item_cost,
            additional_item_cost,
            result,
            max_uses,
            xp,
            discount,
        ))
    }
}

impl Validatable for VillagerTrade {
    fn validate(&self, context: &ValidationContext) {
        self.wants.validate(&context.for_field("wants"));
        validate_opt(context, "additional_wants", &self.additional_wants);
        validate_holder_field(context, "max_uses", &self.max_uses);
        validate_holder_field(context, "reputation_discount", &self.reputation_discount);
        validate_holder_field(context, "xp", &self.xp);
        validate_holder_opt(context, "merchant_predicate", &self.merchant_predicate);
        validate_holder_opt(context, "given_item_modifier", &self.given_item_modifier);
    }
}

fn decode(json: &Value, ctx: &RegistryDecodeContext) -> Result<DecodedEntry, String> {
    let input = Dyn::from_json_str(&json.to_string())?;
    let trade =
        with_decode_context(ctx, || VillagerTrade::codec().parse(&JSON, &input)).into_result()?;
    Ok(DecodedEntry::new(Arc::new(trade)))
}

#[linkme::distributed_slice(DYNAMIC_REGISTRY_DECODERS)]
static VILLAGER_TRADE_DECODER: DynamicRegistryDecoder = DynamicRegistryDecoder {
    registry: keys::VILLAGER_TRADE,
    decode,
};
