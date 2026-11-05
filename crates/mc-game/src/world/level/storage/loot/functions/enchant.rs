//! `enchant_with_levels`, `enchant_randomly`, `set_enchantments`.

use std::sync::Arc;

use mc_core::ident::Identifier;
use mc_core::math::mth;
use mc_data::Item;
use mc_dfu::{BOOL, MapCodec, record};
use mc_protocol::components::values::basic::ADDITIONAL_TRADE_COST;
use mc_protocol::components::values::predicates::data_driven_set_codec;
use mc_protocol::components::{HolderSet, ItemStack, NoDirect};

use super::{ConditionalFunction, LootItemFunctionRef, condition_field};
use crate::registry::keys;
use crate::world::level::storage::loot::context::{ContextKey, LootContext};
use crate::world::level::storage::loot::enchant_api;
use crate::world::level::storage::loot::holder::{lift, registry_ref_codec};
use crate::world::level::storage::loot::predicates::ConditionHolder;
use crate::world::level::storage::loot::providers::number::{IntHolder, int_holder_codec};
use crate::world::level::storage::loot::validation::{ValidationContext, validate_holder_field};

type Entries = Vec<(&'static str, MapCodec<LootItemFunctionRef>)>;

fn entry<F: ConditionalFunction + Clone>(codec: MapCodec<F>) -> MapCodec<LootItemFunctionRef> {
    lift(codec, |f| Arc::new(f) as LootItemFunctionRef, |t| t.as_any().downcast_ref::<F>())
}

/// `Util.getRandomSafe(list, random)`.
fn random_safe<T: Clone>(list: &[T], random: &mut dyn mc_core::random::RandomSource) -> Option<T> {
    if list.is_empty() { None } else { Some(list[random.next_int_bounded(list.len() as i32) as usize].clone()) }
}

#[derive(Clone)]
pub struct EnchantWithLevelsFunction {
    pub condition: Option<ConditionHolder>,
    pub levels: IntHolder,
    pub options: Option<HolderSet<NoDirect>>,
    pub include_additional_cost_component: bool,
}

impl ConditionalFunction for EnchantWithLevelsFunction {
    fn type_name(&self) -> &'static str {
        "enchant_with_levels"
    }
    fn condition(&self) -> &Option<ConditionHolder> {
        &self.condition
    }
    fn referenced_params(&self) -> Vec<ContextKey> {
        if self.include_additional_cost_component { vec![ContextKey::AdditionalCostComponentAllowed] } else { vec![] }
    }
    fn validate_fields(&self, context: &ValidationContext) {
        validate_holder_field(context, "levels", &self.levels);
    }
    fn run(&self, stack: ItemStack, context: &mut LootContext) -> ItemStack {
        let enchantment_cost = self.levels.value(context.get_resolver()).get_int(context);
        let registries = context.get_resolver();
        let mut result = enchant_api::enchant_item(context.get_random(), stack, enchantment_cost, registries, self.options.as_ref());
        if self.include_additional_cost_component
            && context.has_parameter(ContextKey::AdditionalCostComponentAllowed)
            && !result.is_empty()
            && enchantment_cost > 0
        {
            result.set(ADDITIONAL_TRADE_COST, enchantment_cost);
        }
        result
    }
}

#[derive(Clone)]
pub struct EnchantRandomlyFunction {
    pub condition: Option<ConditionHolder>,
    pub options: Option<HolderSet<NoDirect>>,
    pub only_compatible: bool,
    pub include_additional_cost_component: bool,
}

impl ConditionalFunction for EnchantRandomlyFunction {
    fn type_name(&self) -> &'static str {
        "enchant_randomly"
    }
    fn condition(&self) -> &Option<ConditionHolder> {
        &self.condition
    }
    fn referenced_params(&self) -> Vec<ContextKey> {
        if self.include_additional_cost_component { vec![ContextKey::AdditionalCostComponentAllowed] } else { vec![] }
    }
    fn run(&self, stack: ItemStack, context: &mut LootContext) -> ItemStack {
        let target_is_book = stack.is(Item::Book);
        let should_check_compatibility = !target_is_book && self.only_compatible;
        let registries = context.get_resolver();
        let candidates: Vec<Identifier> = match &self.options {
            Some(set) => enchant_api::holder_set_ids(registries, keys::ENCHANTMENT, set),
            None => registries.registry(keys::ENCHANTMENT).map(|r| r.names()).unwrap_or_default(),
        };
        let compatible: Vec<Identifier> =
            candidates.into_iter().filter(|c| !should_check_compatibility || enchant_api::can_enchant(registries, c, &stack)).collect();
        let Some(enchantment) = random_safe(&compatible, context.get_random()) else {
            log::warn!("Couldn't find a compatible enchantment for {stack:?}");
            return stack;
        };
        self.enchant_item(stack, &enchantment, context)
    }
}

impl EnchantRandomlyFunction {
    fn enchant_item(&self, mut stack: ItemStack, enchantment: &Identifier, context: &mut LootContext) -> ItemStack {
        let (min, max) = enchant_api::min_max_level(context.get_resolver(), enchantment);
        let level = mth::next_int(context.get_random(), min, max);
        if stack.is(Item::Book) {
            stack = ItemStack::new(Item::EnchantedBook);
        }
        enchant_api::enchant(&mut stack, enchantment, level);
        if self.include_additional_cost_component && context.has_parameter(ContextKey::AdditionalCostComponentAllowed) {
            let random = context.get_random();
            let cost = 2 + random.next_int_bounded(5 + level * 10) + 3 * level;
            stack.set(ADDITIONAL_TRADE_COST, cost);
        }
        stack
    }
}

#[derive(Clone)]
pub struct SetEnchantmentsFunction {
    pub condition: Option<ConditionHolder>,
    pub enchantments: Vec<(Identifier, IntHolder)>,
    pub add: bool,
}

impl ConditionalFunction for SetEnchantmentsFunction {
    fn type_name(&self) -> &'static str {
        "set_enchantments"
    }
    fn condition(&self) -> &Option<ConditionHolder> {
        &self.condition
    }
    fn validate_fields(&self, context: &ValidationContext) {
        for (enchantment, holder) in &self.enchantments {
            let c = context.for_map_field("enchantments", &enchantment.to_string());
            if let Some(p) = holder.try_value(context.resolver()) {
                p.validate(&c);
            }
        }
    }
    fn run(&self, mut stack: ItemStack, context: &mut LootContext) -> ItemStack {
        if stack.is(Item::Book) {
            stack = if stack.is_empty() { stack } else { ItemStack::with_patch(Item::EnchantedBook, stack.count(), &stack.get_components_patch()) };
        }
        let mut levels: Vec<(Identifier, i32)> = Vec::new();
        for (enchantment, provider) in &self.enchantments {
            let value = provider.value(context.get_resolver()).get_int(context);
            let current = enchant_api::get_stack_enchantments(&stack).iter().find(|(e, _)| e == enchantment).map_or(0, |(_, l)| *l);
            let level = if self.add { mth::clamp_i32(current + value, 0, 255) } else { mth::clamp_i32(value, 0, 255) };
            levels.push((enchantment.clone(), level));
        }
        enchant_api::update_enchantments(&mut stack, |e| {
            for (enchantment, level) in &levels {
                e.set(enchantment, *level);
            }
        });
        stack
    }
}

pub(super) fn register(entries: &mut Entries) {
    let cond = condition_field;
    entries.push((
        "enchant_with_levels",
        entry(record!(EnchantWithLevelsFunction {
            condition: cond(),
            levels: int_holder_codec().field_of("levels"),
            options: data_driven_set_codec(keys::ENCHANTMENT).optional_field_of("options"),
            include_additional_cost_component: BOOL.optional_field_of_default("include_additional_cost_component", false),
        })),
    ));
    entries.push((
        "enchant_randomly",
        entry(record!(EnchantRandomlyFunction {
            condition: cond(),
            options: data_driven_set_codec(keys::ENCHANTMENT).optional_field_of("options"),
            only_compatible: BOOL.optional_field_of_default("only_compatible", true),
            include_additional_cost_component: BOOL.optional_field_of_default("include_additional_cost_component", false),
        })),
    ));
    entries.push((
        "set_enchantments",
        entry(record!(SetEnchantmentsFunction {
            condition: cond(),
            enchantments: registry_ref_codec(keys::ENCHANTMENT).unbounded_map(&int_holder_codec()).optional_field_of_default("enchantments", Vec::new()),
            add: BOOL.optional_field_of_default("add", false),
        })),
    ));
}
