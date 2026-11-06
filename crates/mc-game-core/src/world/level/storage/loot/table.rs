//! `LootTable` and `LootPool`.

use std::sync::{Arc, LazyLock};

use mc_core::ident::Identifier;
use mc_core::math::mth;
use mc_core::random::RandomSource;
use mc_dfu::extra::IDENTIFIER;
use mc_dfu::{Codec, record};
use mc_protocol::components::ItemStack;

use super::context::{ContextKeySet, LootContext, LootParams, VisitedEntry};
use super::entries::{ItemSink, LootPoolEntry, LootPoolEntryContainer, entry_codec};
use super::functions::{FunctionHolder, decorate, function_holder_codec};
use super::parameters::LootContextParamSets;
use super::predicates::{ConditionHolder, condition_holder_codec};
use super::providers::number::{
    FloatHolder, IntHolder, float_exactly, float_holder_codec, int_holder_codec,
};
use super::validation::{
    ValidatableData, ValidationContext, validate_holder_field, validate_holder_opt,
};
use crate::registry::keys;
use crate::server::level::ServerLevel;
use crate::world::container::Container;
use crate::world::item::is_item_enabled;

/// `LootPool`.
#[derive(Clone)]
pub struct LootPool {
    pub entries: Vec<LootPoolEntryContainer>,
    pub condition: Option<ConditionHolder>,
    pub modifier: Option<FunctionHolder>,
    pub rolls: IntHolder,
    pub bonus_rolls: FloatHolder,
}

impl LootPool {
    /// `LootPool.CODEC`.
    pub fn codec() -> Codec<LootPool> {
        record!(LootPool {
            entries: entry_codec().list_of().field_of("entries"),
            condition: condition_holder_codec().optional_field_of("condition"),
            modifier: function_holder_codec().optional_field_of("modifier"),
            rolls: int_holder_codec().field_of("rolls"),
            bonus_rolls: float_holder_codec().optional_field_of("bonus_rolls").xmap(
                |o| o.unwrap_or_else(|| float_exactly(0.0)),
                |h: &FloatHolder| Some(h.clone()),
            ),
        })
        .codec()
    }

    fn add_random_item(&self, result: ItemSink, context: &mut LootContext) {
        let mut valid_entries: Vec<LootPoolEntry> = Vec::new();
        let luck = context.get_luck();
        let mut total_weight: i32 = 0;
        let mut expanded: Vec<LootPoolEntry> = Vec::new();
        for entry in &self.entries {
            expanded.clear();
            entry.expand(context, &mut expanded);
            for e in expanded.drain(..) {
                let weight = e.get_weight(luck);
                if weight > 0 {
                    total_weight = total_weight.wrapping_add(weight);
                    valid_entries.push(e);
                }
            }
        }
        let entry_count = valid_entries.len();
        if total_weight != 0 && entry_count != 0 {
            if entry_count == 1 {
                valid_entries[0].create_item_stack(result, context);
            } else {
                let mut index = context.get_random().next_int_bounded(total_weight);
                for entry in &valid_entries {
                    index -= entry.get_weight(luck);
                    if index < 0 {
                        entry.create_item_stack(result, context);
                        return;
                    }
                }
            }
        }
    }

    /// `addRandomItems(result, context)`.
    pub fn add_random_items(&self, result: ItemSink, context: &mut LootContext) {
        if let Some(condition) = &self.condition {
            if !condition.value(context.get_resolver()).test(context) {
                return;
            }
        }
        let mut decorated = decorate(&self.modifier, result);
        let rolls = self.rolls.value(context.get_resolver()).get_int(context);
        let bonus = self
            .bonus_rolls
            .value(context.get_resolver())
            .get_float(context);
        let count = rolls.wrapping_add(mth::floor_f32(bonus * context.get_luck()));
        for _ in 0..count {
            self.add_random_item(&mut decorated, context);
        }
    }

    pub fn validate(&self, output: &ValidationContext) {
        validate_holder_opt(output, "condition", &self.condition);
        validate_holder_opt(output, "modifier", &self.modifier);
        for (i, e) in self.entries.iter().enumerate() {
            e.validate(&output.for_indexed_field("entries", i));
        }
        validate_holder_field(output, "rolls", &self.rolls);
        validate_holder_field(output, "bonus_rolls", &self.bonus_rolls);
    }
}

/// `LootTable`.
#[derive(Clone)]
pub struct LootTable {
    param_set: ContextKeySet,
    random_sequence: Option<Identifier>,
    pools: Vec<LootPool>,
    modifier: Option<FunctionHolder>,
}

impl ValidatableData for LootTable {
    const REGISTRY: &'static str = keys::LOOT_TABLE;
    fn validate_value(&self, context: &ValidationContext) {
        self.validate(context);
    }
}

#[derive(Clone)]
struct TableFields {
    param_set: ContextKeySet,
    random_sequence: Option<Identifier>,
    pools: Vec<LootPool>,
    modifier: Option<FunctionHolder>,
}

/// `LootTable.DIRECT_CODEC`.
pub fn loot_table_direct_codec() -> Codec<Arc<LootTable>> {
    Codec::lazy(|| {
        static C: LazyLock<Codec<Arc<LootTable>>> = LazyLock::new(|| {
            record!(TableFields {
                param_set: LootContextParamSets::codec()
                    .lenient_optional_field_of("type")
                    .xmap(
                        |o| o.unwrap_or(LootContextParamSets::ALL_PARAMS),
                        |s: &ContextKeySet| Some(*s),
                    ),
                random_sequence: IDENTIFIER.optional_field_of("random_sequence"),
                pools: LootPool::codec().list_of().optional_field_of("pools").xmap(
                    |o| o.unwrap_or_default(),
                    |p: &Vec<LootPool>| { if p.is_empty() { None } else { Some(p.clone()) } }
                ),
                modifier: function_holder_codec().optional_field_of("modifier"),
            })
            .codec()
            .xmap(
                |f| {
                    Arc::new(LootTable {
                        param_set: f.param_set,
                        random_sequence: f.random_sequence,
                        pools: f.pools,
                        modifier: f.modifier,
                    })
                },
                |t: &Arc<LootTable>| TableFields {
                    param_set: t.param_set,
                    random_sequence: t.random_sequence.clone(),
                    pools: t.pools.clone(),
                    modifier: t.modifier.clone(),
                },
            )
        });
        C.clone()
    })
}

static EMPTY: LazyLock<Arc<LootTable>> = LazyLock::new(|| {
    Arc::new(LootTable {
        param_set: LootContextParamSets::EMPTY,
        random_sequence: None,
        pools: Vec::new(),
        modifier: None,
    })
});

/// `createStackSplitter(level, output)`: disabled items are dropped, oversized stacks are split.
pub fn create_stack_splitter<'a>(
    level: &'a ServerLevel,
    output: &'a mut dyn FnMut(ItemStack),
) -> impl FnMut(ItemStack) + 'a {
    move |result: ItemStack| {
        if is_item_enabled(&result, level.tick_state.enabled_features) {
            if result.count() < result.get_max_stack_size() {
                output(result);
            } else {
                let mut count = result.count();
                while count > 0 {
                    let copy = result.copy_with_count(count.min(result.get_max_stack_size()));
                    count -= copy.count();
                    output(copy);
                }
            }
        }
    }
}

impl LootTable {
    /// `LootTable.EMPTY`.
    pub fn empty() -> Arc<LootTable> {
        EMPTY.clone()
    }

    pub fn get_param_set(&self) -> ContextKeySet {
        self.param_set
    }

    pub fn random_sequence(&self) -> Option<&Identifier> {
        self.random_sequence.as_ref()
    }

    /// `getRandomItemsRaw(context, output)`.
    pub fn get_random_items_raw_with(&self, context: &mut LootContext, output: ItemSink) {
        let breadcrumb = VisitedEntry::table(self);
        if context.push_visited_element(breadcrumb.clone()) {
            let mut decorated = decorate(&self.modifier, output);
            for pool in &self.pools {
                pool.add_random_items(&mut decorated, context);
            }
            context.pop_visited_element(&breadcrumb);
        } else {
            log::warn!("Detected infinite loop in loot tables");
        }
    }

    /// `getRandomItemsRaw(params, output)`.
    pub fn get_random_items_raw(&self, params: &LootParams, output: &mut dyn FnMut(ItemStack)) {
        let mut context = LootContext::builder(params).create(self.random_sequence.as_ref());
        self.get_random_items_raw_with(&mut context, &mut |s, _| output(s));
    }

    fn items_into(&self, mut context: LootContext, level: &ServerLevel) -> Vec<ItemStack> {
        let mut out = Vec::new();
        {
            let mut push = |s: ItemStack| out.push(s);
            let mut split = create_stack_splitter(level, &mut push);
            self.get_random_items_raw_with(&mut context, &mut |s, _| split(s));
        }
        out
    }

    /// `getRandomItems(params)`.
    pub fn get_random_items(&self, params: &LootParams) -> Vec<ItemStack> {
        let context = LootContext::builder(params).create(self.random_sequence.as_ref());
        self.items_into(context, params.level())
    }

    /// `getRandomItems(params, optionalLootTableSeed)`.
    pub fn get_random_items_with_seed(
        &self,
        params: &LootParams,
        optional_loot_table_seed: i64,
    ) -> Vec<ItemStack> {
        let context = LootContext::builder(params)
            .with_optional_random_seed(optional_loot_table_seed)
            .create(self.random_sequence.as_ref());
        self.items_into(context, params.level())
    }

    /// `getRandomItems(params, randomSource)`.
    pub fn get_random_items_with_random(
        &self,
        params: &LootParams,
        random: Box<dyn RandomSource>,
    ) -> Vec<ItemStack> {
        let context = LootContext::builder(params)
            .with_optional_random_source(random)
            .create(self.random_sequence.as_ref());
        self.items_into(context, params.level())
    }

    /// `getRandomItems(context)`.
    pub fn get_random_items_with_context(&self, context: LootContext) -> Vec<ItemStack> {
        let level = context.get_level();
        self.items_into(context, level)
    }

    /// `getRandomItems(params, seed, output)`.
    pub fn get_random_items_to(
        &self,
        params: &LootParams,
        optional_loot_table_seed: i64,
        output: &mut dyn FnMut(ItemStack),
    ) {
        for s in self.get_random_items_with_seed(params, optional_loot_table_seed) {
            output(s);
        }
    }

    pub fn validate(&self, context: &ValidationContext) {
        for (i, pool) in self.pools.iter().enumerate() {
            pool.validate(&context.for_indexed_field("pools", i));
        }
        validate_holder_opt(context, "modifier", &self.modifier);
    }

    /// `fill(container, params, optionalRandomSeed)`: chest-style filling of the empty slots.
    pub fn fill(
        &self,
        container: &mut dyn Container,
        params: &LootParams,
        optional_random_seed: i64,
    ) {
        let mut context = LootContext::builder(params)
            .with_optional_random_seed(optional_random_seed)
            .create(self.random_sequence.as_ref());
        let level = params.level();
        let mut item_stacks = Vec::new();
        {
            let mut push = |s: ItemStack| item_stacks.push(s);
            let mut split = create_stack_splitter(level, &mut push);
            self.get_random_items_raw_with(&mut context, &mut |s, _| split(s));
        }
        let random = context.get_random();
        let mut available_slots = get_available_slots(container, random);
        shuffle_and_split_items(&mut item_stacks, available_slots.len(), random);
        for item_stack in item_stacks {
            let Some(slot) = available_slots.pop() else {
                log::warn!("Tried to over-fill a container");
                return;
            };
            if item_stack.is_empty() {
                container.set_item(slot, ItemStack::empty());
            } else {
                container.set_item(slot, item_stack);
            }
        }
    }
}

/// `Util.shuffle`.
fn shuffle<T>(list: &mut [T], random: &mut dyn RandomSource) {
    for i in (2..=list.len()).rev() {
        let swap_to = random.next_int_bounded(i as i32) as usize;
        list.swap(i - 1, swap_to);
    }
}

fn get_available_slots(container: &dyn Container, random: &mut dyn RandomSource) -> Vec<usize> {
    let mut slots: Vec<usize> = (0..container.get_container_size())
        .filter(|i| container.get_item(*i).is_empty())
        .collect();
    shuffle(&mut slots, random);
    slots
}

fn shuffle_and_split_items(
    result: &mut Vec<ItemStack>,
    available_slots: usize,
    random: &mut dyn RandomSource,
) {
    let mut splittable_items: Vec<ItemStack> = Vec::new();
    let mut kept = Vec::with_capacity(result.len());
    for item_stack in result.drain(..) {
        if item_stack.is_empty() {
            // removed
        } else if item_stack.count() > 1 {
            splittable_items.push(item_stack);
        } else {
            kept.push(item_stack);
        }
    }
    *result = kept;
    while available_slots as i64 - result.len() as i64 - splittable_items.len() as i64 > 0
        && !splittable_items.is_empty()
    {
        let index = mth::next_int(random, 0, splittable_items.len() as i32 - 1) as usize;
        let mut item_stack = splittable_items.remove(index);
        let remove = mth::next_int(random, 1, item_stack.count() / 2);
        let copy = item_stack.split(remove);
        if item_stack.count() > 1 && random.next_boolean() {
            splittable_items.push(item_stack);
        } else {
            result.push(item_stack);
        }
        if copy.count() > 1 && random.next_boolean() {
            splittable_items.push(copy);
        } else {
            result.push(copy);
        }
    }
    result.extend(splittable_items);
    shuffle(result, random);
}
