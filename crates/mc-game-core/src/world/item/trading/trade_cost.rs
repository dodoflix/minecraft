//! `TradeCost`: the data-driven form of an `ItemCost` (the count is a context int provider).

use std::sync::Arc;

use mc_core::math::mth;
use mc_data::Item;
use mc_dfu::{Codec, MapCodec, record};
use mc_protocol::components::holder::registry_by_name_codec;
use mc_protocol::components::{DataComponentExactPredicate, ItemStack};

use super::item_cost::ItemCost;
use crate::world::level::storage::loot::context::LootContext;
use crate::world::level::storage::loot::holder::Holder;
use crate::world::level::storage::loot::providers::number::{
    ContextIntProvider, FloatConstant, FloatHolder, IntConstant, IntHolder, float_exactly, float_holder_codec, int_exactly,
    int_holder_codec,
};
use crate::world::level::storage::loot::validation::{Validatable, ValidationContext, validate_holder_field};

/// `TradeCost`.
#[derive(Clone)]
pub struct TradeCost {
    pub item: Item,
    pub count: IntHolder,
    pub components: DataComponentExactPredicate,
}

/// `ContextIntProviders.CODEC.optionalFieldOf(name, ContextIntProviders.exactly(default))`.
pub(super) fn int_holder_field(name: &str, default: i32) -> MapCodec<IntHolder> {
    int_holder_codec().optional_field_of(name).xmap(
        move |o| o.unwrap_or_else(|| int_exactly(default)),
        move |h: &IntHolder| match h {
            Holder::Direct(v) if v.as_any().downcast_ref::<IntConstant>().is_some_and(|c| c.value == default) => None,
            _ => Some(h.clone()),
        },
    )
}

/// `ContextFloatProviders.CODEC.optionalFieldOf(name, ContextFloatProviders.exactly(default))`.
pub(super) fn float_holder_field(name: &str, default: f32) -> MapCodec<FloatHolder> {
    float_holder_codec().optional_field_of(name).xmap(
        move |o| o.unwrap_or_else(|| float_exactly(default)),
        move |h: &FloatHolder| match h {
            Holder::Direct(v) if v.as_any().downcast_ref::<FloatConstant>().is_some_and(|c| c.value.to_bits() == default.to_bits()) => None,
            _ => Some(h.clone()),
        },
    )
}

impl TradeCost {
    /// `new TradeCost(item, count)`.
    pub fn of(item: Item, count: i32) -> TradeCost {
        TradeCost { item, count: int_exactly(count), components: DataComponentExactPredicate::empty() }
    }

    /// `TradeCost.CODEC`.
    pub fn codec() -> Codec<TradeCost> {
        record!(TradeCost {
            item: registry_by_name_codec::<Item>().field_of("id"),
            count: int_holder_field("count", 1),
            components: DataComponentExactPredicate::codec().optional_field_of_default("components", DataComponentExactPredicate::empty()),
        })
        .codec()
    }

    /// `toItemCost(lootContext, additionalCost)`.
    pub fn to_item_cost(&self, context: &mut LootContext, additional_cost: i32) -> ItemCost {
        let provider: Arc<dyn ContextIntProvider> = self.count.value(context.get_resolver());
        let count = mth::clamp_i32(provider.get_int(context).wrapping_add(additional_cost), 0, ItemStack::new(self.item).get_max_stack_size());
        ItemCost::new(self.item, count, self.components.clone())
    }
}

impl Validatable for TradeCost {
    fn validate(&self, context: &ValidationContext) {
        validate_holder_field(context, "count", &self.count);
    }
}
