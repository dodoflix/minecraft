//! `loot.entries`: `LootPoolEntryContainer` and its kinds (`empty`, `item`, `loot_table`, `dynamic`, `tag`, `slots`,
//! `alternatives`, `sequence`, `group`), their expansion into weighted `LootPoolEntry`s.

use std::sync::LazyLock;

use mc_core::ident::Identifier;
use mc_core::math::mth;
use mc_data::Item;
use mc_dfu::{BOOL, Codec, INT, MapCodec, record};
use mc_protocol::components::{ItemStack, RegistrySet};

use super::LootTable;
use super::context::LootContext;
use super::functions::{FunctionHolder, decorate, function_holder_codec};
use super::holder::{Holder, HolderSet, holder_set_codec};
use super::predicates::{ConditionHolder, condition_holder_codec};
use super::validation::{ValidationContext, validate_holder_field, validate_holder_opt, validate_holder_set};
use crate::registry::keys;
use crate::world::item::slot::{SlotSourceHolder, slot_source_holder_codec};

/// Where the stacks of a roll go: the consumer of `getRandomItems` (it gets the context back so that functions applied
/// on the way can use it).
pub type ItemSink<'a> = &'a mut dyn FnMut(ItemStack, &mut LootContext);

/// `weight` and `quality` of the uniform containers.
#[derive(Clone, Copy)]
pub struct Uniform {
    pub weight: i32,
    pub quality: i32,
}

#[derive(Clone)]
pub enum EntryKind {
    Empty(Uniform),
    Item(Item, Uniform),
    Tag { items: RegistrySet<Item>, expand: bool, uniform: Uniform },
    Dynamic(Identifier, Uniform),
    LootTable { value: HolderSet<LootTable>, expand: bool, uniform: Uniform },
    Slots(SlotSourceHolder, Uniform),
    Alternatives(Vec<LootPoolEntryContainer>),
    Sequence(Vec<LootPoolEntryContainer>),
    Group(Vec<LootPoolEntryContainer>),
}

/// `LootPoolEntryContainer`.
#[derive(Clone)]
pub struct LootPoolEntryContainer {
    pub condition: Option<ConditionHolder>,
    pub modifier: Option<FunctionHolder>,
    pub kind: EntryKind,
}

enum EntrySource {
    Empty,
    Item(Item),
    TagItems(Vec<Item>),
    Dynamic(Identifier),
    Table(Holder<LootTable>),
    Tables(Vec<Holder<LootTable>>),
    Slots(SlotSourceHolder),
}

/// `LootPoolEntry`.
pub struct LootPoolEntry {
    uniform: Uniform,
    source: EntrySource,
    /// The `modifier`s of the containers it passed through, innermost first.
    modifiers: Vec<FunctionHolder>,
}

impl LootPoolEntry {
    fn new(uniform: Uniform, source: EntrySource) -> LootPoolEntry {
        LootPoolEntry { uniform, source, modifiers: Vec::new() }
    }

    /// `getWeight(luck)`.
    pub fn get_weight(&self, luck: f32) -> i32 {
        mth::floor_f32(self.uniform.weight as f32 + self.uniform.quality as f32 * luck).max(0)
    }

    /// `createItemStack(output, context)`.
    pub fn create_item_stack(&self, output: ItemSink, context: &mut LootContext) {
        let modifiers = &self.modifiers;
        let mut deliver = |stack: ItemStack, context: &mut LootContext| {
            let mut stack = stack;
            for m in modifiers {
                stack = m.value(context.get_resolver()).apply(stack, context);
            }
            output(stack, context);
        };
        match &self.source {
            EntrySource::Empty => {}
            EntrySource::Item(item) => deliver(ItemStack::new(*item), context),
            EntrySource::TagItems(items) => {
                for item in items {
                    deliver(ItemStack::new(*item), context);
                }
            }
            EntrySource::Dynamic(name) => {
                let mut collected: Vec<ItemStack> = Vec::new();
                context.add_dynamic_drops(name, &mut |s| collected.push(s));
                for s in collected {
                    deliver(s, context);
                }
            }
            EntrySource::Table(table) => {
                let table = table.value(context.get_resolver());
                table.get_random_items_raw_with(context, &mut deliver);
            }
            EntrySource::Tables(tables) => {
                for table in tables {
                    let table = table.value(context.get_resolver());
                    table.get_random_items_raw_with(context, &mut deliver);
                }
            }
            EntrySource::Slots(source) => {
                let provided = source.value(context.get_resolver()).provide(context);
                for s in provided.item_copies().into_iter().filter(|s| !s.is_empty()) {
                    deliver(s, context);
                }
            }
        }
    }
}

impl LootPoolEntryContainer {
    fn can_run(&self, context: &mut LootContext) -> bool {
        match &self.condition {
            None => true,
            Some(c) => c.value(context.get_resolver()).test(context),
        }
    }

    /// `expand(context, output)`.
    pub fn expand(&self, context: &mut LootContext, output: &mut Vec<LootPoolEntry>) -> bool {
        if !self.can_run(context) {
            return false;
        }
        let start = output.len();
        let result = self.expand_raw(context, output);
        if let Some(modifier) = &self.modifier {
            for entry in &mut output[start..] {
                entry.modifiers.push(modifier.clone());
            }
        }
        result
    }

    fn expand_raw(&self, context: &mut LootContext, output: &mut Vec<LootPoolEntry>) -> bool {
        match &self.kind {
            EntryKind::Empty(u) => {
                output.push(LootPoolEntry::new(*u, EntrySource::Empty));
                true
            }
            EntryKind::Item(item, u) => {
                output.push(LootPoolEntry::new(*u, EntrySource::Item(*item)));
                true
            }
            EntryKind::Dynamic(name, u) => {
                output.push(LootPoolEntry::new(*u, EntrySource::Dynamic(name.clone())));
                true
            }
            EntryKind::Slots(source, u) => {
                output.push(LootPoolEntry::new(*u, EntrySource::Slots(source.clone())));
                true
            }
            EntryKind::Tag { items, expand, uniform } => {
                let list = tag_items(context, items);
                if *expand {
                    for item in list {
                        output.push(LootPoolEntry::new(*uniform, EntrySource::Item(item)));
                    }
                } else {
                    output.push(LootPoolEntry::new(*uniform, EntrySource::TagItems(list)));
                }
                true
            }
            EntryKind::LootTable { value, expand, uniform } => {
                let holders = value.holders(context.get_resolver());
                if *expand {
                    for table in holders {
                        output.push(LootPoolEntry::new(*uniform, EntrySource::Table(table)));
                    }
                } else {
                    output.push(LootPoolEntry::new(*uniform, EntrySource::Tables(holders)));
                }
                true
            }
            EntryKind::Alternatives(children) => {
                for child in children {
                    if child.expand(context, output) {
                        return true;
                    }
                }
                false
            }
            EntryKind::Sequence(children) => {
                for child in children {
                    if !child.expand(context, output) {
                        return false;
                    }
                }
                true
            }
            EntryKind::Group(children) => {
                for child in children {
                    child.expand(context, output);
                }
                true
            }
        }
    }

    pub fn validate(&self, context: &ValidationContext) {
        validate_holder_opt(context, "condition", &self.condition);
        validate_holder_opt(context, "modifier", &self.modifier);
        match &self.kind {
            EntryKind::Slots(source, _) => validate_holder_field(context, "slot_source", source),
            EntryKind::LootTable { value, .. } => validate_holder_set(context, "value", value),
            EntryKind::Alternatives(children) => {
                composite_validate(context, children);
                // `UNREACHABLE_PROBLEM`: a child without condition that is not the last one
                for child in children.iter().take(children.len().saturating_sub(1)) {
                    if child.condition.is_none() {
                        context.report("Unreachable entry!");
                    }
                }
            }
            EntryKind::Sequence(children) | EntryKind::Group(children) => composite_validate(context, children),
            _ => {}
        }
    }
}

fn composite_validate(context: &ValidationContext, children: &[LootPoolEntryContainer]) {
    if children.is_empty() {
        context.report("Empty children list");
    }
    for (i, child) in children.iter().enumerate() {
        child.validate(&context.for_indexed_field("children", i));
    }
}

/// The items of an item holder set in tag order.
fn tag_items(context: &LootContext, items: &RegistrySet<Item>) -> Vec<Item> {
    match items {
        RegistrySet::Entries(v) => v.clone(),
        RegistrySet::Tag(tag) => {
            let registries = context.get_resolver();
            let Some(registry) = registries.registry("minecraft:item") else { return Vec::new() };
            registry.tag(tag).unwrap_or(&[]).iter().filter_map(|id| Item::from_id(*id)).collect()
        }
    }
}

// ---- codecs ---------------------------------------------------------------------------------------------------------------------

struct Common {
    condition: Option<ConditionHolder>,
    modifier: Option<FunctionHolder>,
}

fn weight() -> MapCodec<i32> {
    INT.optional_field_of_default("weight", 1)
}
fn quality() -> MapCodec<i32> {
    INT.optional_field_of_default("quality", 0)
}

/// A map codec of one container kind: `fields` (kind specific) + weight/quality + condition + modifier.
fn uniform_entry<T: Clone + Send + Sync + 'static>(
    fields: MapCodec<T>,
    to_kind: fn(T, Uniform) -> EntryKind,
    from_kind: fn(&EntryKind) -> Option<(T, Uniform)>,
) -> MapCodec<LootPoolEntryContainer> {
    let rest = record!(UniformFields { weight: weight(), quality: quality(), condition: common_condition(), modifier: common_modifier() });
    fields.pair(&rest).xmap(
        move |(t, r)| LootPoolEntryContainer { condition: r.condition, modifier: r.modifier, kind: to_kind(t, Uniform { weight: r.weight, quality: r.quality }) },
        move |c: &LootPoolEntryContainer| {
            let (t, u) = from_kind(&c.kind).expect("entry kind matches its codec");
            (t, UniformFields { weight: u.weight, quality: u.quality, condition: c.condition.clone(), modifier: c.modifier.clone() })
        },
    )
}

#[derive(Clone)]
struct UniformFields {
    weight: i32,
    quality: i32,
    condition: Option<ConditionHolder>,
    modifier: Option<FunctionHolder>,
}

fn common_condition() -> MapCodec<Option<ConditionHolder>> {
    condition_holder_codec().optional_field_of("condition")
}

fn common_modifier() -> MapCodec<Option<FunctionHolder>> {
    function_holder_codec().optional_field_of("modifier")
}

fn composite_entry(children_of: fn(&EntryKind) -> Option<&Vec<LootPoolEntryContainer>>, make: fn(Vec<LootPoolEntryContainer>) -> EntryKind) -> MapCodec<LootPoolEntryContainer> {
    let children = entry_codec().list_of().optional_field_of_default_vec("children");
    let common = record!(Common { condition: common_condition(), modifier: common_modifier() });
    children.pair(&common).xmap(
        move |(children, c)| LootPoolEntryContainer { condition: c.condition, modifier: c.modifier, kind: make(children) },
        move |e: &LootPoolEntryContainer| {
            (children_of(&e.kind).expect("composite").clone(), Common { condition: e.condition.clone(), modifier: e.modifier.clone() })
        },
    )
}

trait OptionalDefaultVec<T> {
    fn optional_field_of_default_vec(&self, name: &str) -> MapCodec<Vec<T>>;
}

impl<T: Clone + Send + Sync + 'static> OptionalDefaultVec<T> for Codec<Vec<T>> {
    fn optional_field_of_default_vec(&self, name: &str) -> MapCodec<Vec<T>> {
        self.optional_field_of(name).xmap(|o| o.unwrap_or_default(), |v: &Vec<T>| if v.is_empty() { None } else { Some(v.clone()) })
    }
}

fn expandable_fields<T: Clone + Send + Sync + 'static>(fields: MapCodec<T>) -> MapCodec<(T, bool)> {
    fields.pair(&BOOL.optional_field_of_default("expand", false))
}

/// The registered names of the container kinds.
fn type_name(kind: &EntryKind) -> &'static str {
    match kind {
        EntryKind::Empty(_) => "empty",
        EntryKind::Item(..) => "item",
        EntryKind::LootTable { .. } => "loot_table",
        EntryKind::Dynamic(..) => "dynamic",
        EntryKind::Tag { .. } => "tag",
        EntryKind::Slots(..) => "slots",
        EntryKind::Alternatives(_) => "alternatives",
        EntryKind::Sequence(_) => "sequence",
        EntryKind::Group(_) => "group",
    }
}

/// `LootPoolEntries.CODEC` (`LOOT_POOL_ENTRY_TYPE.byNameCodec().dispatch(...)`).
pub fn entry_codec() -> Codec<LootPoolEntryContainer> {
    Codec::lazy(|| {
        static C: LazyLock<Codec<LootPoolEntryContainer>> = LazyLock::new(build_entry_codec);
        C.clone()
    })
}

fn build_entry_codec() -> Codec<LootPoolEntryContainer> {
    use mc_dfu::extra::IDENTIFIER;
    let names = [
        "empty", "item", "loot_table", "dynamic", "tag", "slots", "alternatives", "sequence", "group",
    ];
    let type_codec: Codec<&'static str> = IDENTIFIER.flat_xmap(
        move |id: Identifier| match (id.namespace() == "minecraft").then(|| names.iter().find(|n| **n == id.path())).flatten() {
            Some(n) => mc_dfu::DataResult::success(*n),
            None => mc_dfu::DataResult::error(move || format!("Unknown registry key in ResourceKey[minecraft:root / minecraft:loot_pool_entry_type]: {id}")),
        },
        |n: &&'static str| mc_dfu::DataResult::success(Identifier::new("minecraft", n).expect("valid")),
    );
    type_codec.dispatch(
        "type",
        |c: &LootPoolEntryContainer| type_name(&c.kind),
        |name: &&'static str| match *name {
            "empty" => uniform_entry(
                MapCodec::unit(()),
                |_, u| EntryKind::Empty(u),
                |k| if let EntryKind::Empty(u) = k { Some(((), *u)) } else { None },
            ),
            "item" => uniform_entry(
                mc_protocol::components::holder::registry_by_name_codec::<Item>().field_of("name"),
                |i, u| EntryKind::Item(i, u),
                |k| if let EntryKind::Item(i, u) = k { Some((*i, *u)) } else { None },
            ),
            "loot_table" => {
                let codec = expandable_fields(table_list_codec().field_of("value"));
                uniform_entry(
                    codec,
                    |(value, expand), uniform| EntryKind::LootTable { value, expand, uniform },
                    |k| if let EntryKind::LootTable { value, expand, uniform } = k { Some(((value.clone(), *expand), *uniform)) } else { None },
                )
            }
            "dynamic" => uniform_entry(
                IDENTIFIER.field_of("name"),
                |n, u| EntryKind::Dynamic(n, u),
                |k| if let EntryKind::Dynamic(n, u) = k { Some((n.clone(), *u)) } else { None },
            ),
            "tag" => {
                let codec = expandable_fields(RegistrySet::<Item>::codec().field_of("items"));
                uniform_entry(
                    codec,
                    |(items, expand), uniform| EntryKind::Tag { items, expand, uniform },
                    |k| if let EntryKind::Tag { items, expand, uniform } = k { Some(((items.clone(), *expand), *uniform)) } else { None },
                )
            }
            "slots" => uniform_entry(
                slot_source_holder_codec().field_of("slot_source"),
                |s, u| EntryKind::Slots(s, u),
                |k| if let EntryKind::Slots(s, u) = k { Some((s.clone(), *u)) } else { None },
            ),
            "alternatives" => composite_entry(|k| if let EntryKind::Alternatives(c) = k { Some(c) } else { None }, EntryKind::Alternatives),
            "sequence" => composite_entry(|k| if let EntryKind::Sequence(c) = k { Some(c) } else { None }, EntryKind::Sequence),
            _ => composite_entry(|k| if let EntryKind::Group(c) = k { Some(c) } else { None }, EntryKind::Group),
        },
    )
}

/// `LootTable.LIST_CODEC`.
fn table_list_codec() -> Codec<HolderSet<LootTable>> {
    holder_set_codec(keys::LOOT_TABLE, super::table::loot_table_direct_codec())
}
