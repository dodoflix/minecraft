//! `net.minecraft.world.item.slot`: `SlotSource`s (the `slot_source` registry), `SlotCollection`, `SlotSelector`, and the
//! `SlotAccess` / `SlotProvider` they read.

use std::any::Any;
use std::cell::Cell;
use std::rc::Rc;
use std::sync::{Arc, LazyLock};

use mc_dfu::{Codec, Either, MapCodec, record};
use mc_protocol::components::ItemStack;
use mc_protocol::components::values::predicates::ItemPredicate;

use crate::commands::arguments::slot_arguments::SlotRanges;
use crate::registry::keys;
use crate::world::level::storage::loot::context::{ContextKey, EntityTarget, LootContext};
use crate::world::level::storage::loot::functions::ContainerComponentManipulator;
use crate::world::level::storage::loot::holder::{
    Holder, HolderSet, TypeTable, holder_codec, holder_set_codec, lift,
};
use crate::world::level::storage::loot::validation::{
    ValidatableData, ValidationContext, validate_holder_field, validate_holder_set,
};

/// `SlotAccess`: one slot (`get` / `set`; `set` reports whether the slot accepted the stack).
pub trait SlotAccess {
    fn get(&self) -> ItemStack;
    fn set(&self, stack: ItemStack) -> bool;
}

/// `SlotProvider` (`getSlot(slot)`).
pub trait SlotProvider {
    fn get_slot(&self, slot: i32) -> Option<Rc<dyn SlotAccess + '_>>;

    /// `getSlotsFromRange(slots)`.
    fn get_slots_from_range<'a>(&'a self, slots: &[i32]) -> SlotCollection<'a> {
        SlotCollection::of(slots.iter().filter_map(|s| self.get_slot(*s)).collect())
    }
}

type Predicate<'a> = Rc<dyn Fn(&ItemStack) -> bool + 'a>;
pub type SlotRef<'a> = Rc<dyn SlotAccess + 'a>;

/// A slot holding a value of its own (a copy of a stack that is read through `itemCopies`).
pub struct ValueSlot(std::cell::RefCell<ItemStack>);

impl ValueSlot {
    pub fn new(stack: ItemStack) -> ValueSlot {
        ValueSlot(std::cell::RefCell::new(stack))
    }
}

impl SlotAccess for ValueSlot {
    fn get(&self) -> ItemStack {
        self.0.borrow().clone()
    }
    fn set(&self, stack: ItemStack) -> bool {
        *self.0.borrow_mut() = stack;
        true
    }
}

/// `SlotSelector`.
#[derive(Clone)]
pub enum SlotSelector<'a> {
    Filtering(Option<Predicate<'a>>),
    Tracking {
        selector: Box<SlotSelector<'a>>,
        limit: i32,
        selected_count: Rc<Cell<i32>>,
    },
}

impl<'a> SlotSelector<'a> {
    pub fn any_slot() -> SlotSelector<'a> {
        SlotSelector::Filtering(None)
    }

    pub fn selecting(predicate: impl Fn(&ItemStack) -> bool + 'a) -> SlotSelector<'a> {
        SlotSelector::Filtering(Some(Rc::new(predicate)))
    }

    pub fn tracking(selector: SlotSelector<'a>, selected_count: Rc<Cell<i32>>) -> SlotSelector<'a> {
        SlotSelector::Tracking {
            selector: Box::new(selector),
            limit: i32::MAX,
            selected_count,
        }
    }

    /// `trySelectSlot(itemInSlot)`.
    pub fn try_select_slot(&self, item_in_slot: &ItemStack) -> bool {
        match self {
            SlotSelector::Filtering(f) => f.as_ref().is_none_or(|f| f(item_in_slot)),
            SlotSelector::Tracking {
                selector,
                limit,
                selected_count,
            } => {
                if selected_count.get() < *limit && selector.try_select_slot(item_in_slot) {
                    selected_count.set(selected_count.get() + 1);
                    true
                } else {
                    false
                }
            }
        }
    }

    pub fn filter(&self, predicate: Predicate<'a>) -> SlotSelector<'a> {
        match self {
            SlotSelector::Filtering(None) => SlotSelector::Filtering(Some(predicate)),
            SlotSelector::Filtering(Some(f)) => {
                let f = f.clone();
                SlotSelector::Filtering(Some(Rc::new(move |s| f(s) && predicate(s))))
            }
            SlotSelector::Tracking {
                selector,
                limit,
                selected_count,
            } => SlotSelector::Tracking {
                selector: Box::new(selector.filter(predicate)),
                limit: *limit,
                selected_count: selected_count.clone(),
            },
        }
    }

    pub fn limit(&self, limit: i32) -> SlotSelector<'a> {
        match self {
            SlotSelector::Filtering(_) => SlotSelector::Tracking {
                selector: Box::new(self.clone()),
                limit,
                selected_count: Rc::new(Cell::new(0)),
            },
            SlotSelector::Tracking {
                selector,
                limit: l,
                selected_count,
            } => {
                if limit < *l {
                    SlotSelector::Tracking {
                        selector: selector.clone(),
                        limit,
                        selected_count: selected_count.clone(),
                    }
                } else {
                    self.clone()
                }
            }
        }
    }
}

/// `SlotCollection`.
#[derive(Clone)]
pub enum SlotCollection<'a> {
    Empty,
    Slots(Vec<Rc<dyn SlotAccess + 'a>>),
    Concat(Vec<SlotCollection<'a>>),
    Filtered(Box<SlotCollection<'a>>, Predicate<'a>),
    FlatMapped(
        Box<SlotCollection<'a>>,
        Rc<dyn Fn(SlotRef<'a>) -> SlotCollection<'a> + 'a>,
    ),
    Limited(Box<SlotCollection<'a>>, i32),
}

impl<'a> SlotCollection<'a> {
    pub fn of(slots: Vec<Rc<dyn SlotAccess + 'a>>) -> SlotCollection<'a> {
        if slots.is_empty() {
            SlotCollection::Empty
        } else {
            SlotCollection::Slots(slots)
        }
    }

    pub fn concat(terms: Vec<SlotCollection<'a>>) -> SlotCollection<'a> {
        match terms.len() {
            0 => SlotCollection::Empty,
            1 => terms.into_iter().next().expect("one term"),
            _ => SlotCollection::Concat(terms),
        }
    }

    /// `itemCopies()`.
    pub fn item_copies(&self) -> Vec<ItemStack> {
        match self {
            SlotCollection::Empty => Vec::new(),
            SlotCollection::Slots(slots) => slots.iter().map(|s| s.get().copy()).collect(),
            SlotCollection::Concat(terms) => terms.iter().flat_map(|t| t.item_copies()).collect(),
            SlotCollection::Filtered(slots, filter) => slots
                .item_copies()
                .into_iter()
                .filter(|s| filter(s))
                .collect(),
            SlotCollection::FlatMapped(slots, mapper) => slots
                .item_copies()
                .into_iter()
                .flat_map(|s| mapper(Rc::new(ValueSlot::new(s))).item_copies())
                .collect(),
            SlotCollection::Limited(slots, limit) => slots
                .item_copies()
                .into_iter()
                .take(*limit as usize)
                .collect(),
        }
    }

    /// `size()`.
    pub fn size(&self) -> i32 {
        match self {
            SlotCollection::Empty => 0,
            SlotCollection::Slots(slots) => slots.len() as i32,
            SlotCollection::Concat(terms) => terms.iter().map(SlotCollection::size).sum(),
            SlotCollection::Filtered(..) => self.item_copies().len() as i32,
            SlotCollection::FlatMapped(slots, mapper) => slots
                .item_copies()
                .into_iter()
                .map(|s| mapper(Rc::new(ValueSlot::new(s))).size())
                .sum(),
            SlotCollection::Limited(slots, limit) => slots.size().min(*limit),
        }
    }

    /// `replaceSlotItems(items, slotSelector)`.
    pub fn replace_slot_items(
        &self,
        items: &mut dyn Iterator<Item = ItemStack>,
        selector: &SlotSelector<'a>,
    ) -> i32 {
        let mut items = items.peekable();
        self.replace_with(&mut items, selector)
    }

    fn replace_with(
        &self,
        items: &mut std::iter::Peekable<&mut dyn Iterator<Item = ItemStack>>,
        selector: &SlotSelector<'a>,
    ) -> i32 {
        match self {
            SlotCollection::Empty => 0,
            SlotCollection::Slots(slots) => {
                let mut success = 0;
                for slot in slots {
                    if items.peek().is_none() {
                        break;
                    }
                    if selector.try_select_slot(&slot.get()) {
                        let next = items.next().expect("peeked");
                        if slot.set(next) {
                            success += 1;
                        }
                    }
                }
                success
            }
            SlotCollection::Concat(terms) => {
                terms.iter().map(|t| t.replace_with(items, selector)).sum()
            }
            SlotCollection::Filtered(slots, filter) => {
                slots.replace_with(items, &selector.filter(filter.clone()))
            }
            SlotCollection::FlatMapped(slots, mapper) => {
                let mut success = 0;
                slots.modify_slots(
                    &mut |slot| {
                        let mapped = mapper(slot);
                        success += mapped.replace_with(items, selector);
                    },
                    &SlotSelector::any_slot(),
                );
                success
            }
            SlotCollection::Limited(slots, limit) => {
                slots.replace_with(items, &selector.limit(*limit))
            }
        }
    }

    /// `modifySlots(consumer, slotSelector)`.
    pub fn modify_slots(&self, consumer: &mut dyn FnMut(SlotRef<'a>), selector: &SlotSelector<'a>) {
        match self {
            SlotCollection::Empty => {}
            SlotCollection::Slots(slots) => {
                for slot in slots {
                    if selector.try_select_slot(&slot.get()) {
                        consumer(slot.clone());
                    }
                }
            }
            SlotCollection::Concat(terms) => {
                for t in terms {
                    t.modify_slots(consumer, selector);
                }
            }
            SlotCollection::Filtered(slots, filter) => {
                slots.modify_slots(consumer, &selector.filter(filter.clone()))
            }
            SlotCollection::FlatMapped(slots, mapper) => {
                slots.modify_slots(
                    &mut |slot| {
                        let mapped = mapper(slot);
                        mapped.modify_slots(consumer, selector);
                    },
                    &SlotSelector::any_slot(),
                );
            }
            SlotCollection::Limited(slots, limit) => {
                slots.modify_slots(consumer, &selector.limit(*limit))
            }
        }
    }

    pub fn filter(self, predicate: impl Fn(&ItemStack) -> bool + 'a) -> SlotCollection<'a> {
        match self {
            SlotCollection::Filtered(slots, f) => {
                SlotCollection::Filtered(slots, Rc::new(move |s| f(s) && predicate(s)))
            }
            other => SlotCollection::Filtered(Box::new(other), Rc::new(predicate)),
        }
    }

    pub fn flat_map(
        self,
        mapper: impl Fn(SlotRef<'a>) -> SlotCollection<'a> + 'a,
    ) -> SlotCollection<'a> {
        SlotCollection::FlatMapped(Box::new(self), Rc::new(mapper))
    }

    pub fn limit(self, limit: i32) -> SlotCollection<'a> {
        match self {
            SlotCollection::Limited(slots, l) => SlotCollection::Limited(slots, l.min(limit)),
            other => SlotCollection::Limited(Box::new(other), limit),
        }
    }
}

// ---- SlotSource ------------------------------------------------------------------------------------------------------------------

/// `SlotSource`.
pub trait SlotSource: Send + Sync {
    fn type_name(&self) -> &'static str;
    fn as_any(&self) -> &dyn Any;
    /// `provide(context)`.
    fn provide<'a>(&self, context: &LootContext<'a>) -> SlotCollection<'a>;
    fn referenced_params(&self) -> Vec<ContextKey> {
        Vec::new()
    }
    fn validate(&self, context: &ValidationContext) {
        context.validate_context_usage(&self.referenced_params());
    }
}

impl ValidatableData for dyn SlotSource {
    const REGISTRY: &'static str = keys::SLOT_SOURCE;
    fn validate_value(&self, context: &ValidationContext) {
        self.validate(context);
    }
}

pub type SlotSourceRef = Arc<dyn SlotSource>;
pub type SlotSourceHolder = Holder<dyn SlotSource>;

#[derive(Clone)]
pub struct EmptySlotSource;

impl SlotSource for EmptySlotSource {
    fn type_name(&self) -> &'static str {
        "empty"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn provide<'a>(&self, _context: &LootContext<'a>) -> SlotCollection<'a> {
        SlotCollection::Empty
    }
}

/// `GroupSlotSource`.
#[derive(Clone)]
pub struct GroupSlotSource {
    pub terms: HolderSet<dyn SlotSource>,
}

impl SlotSource for GroupSlotSource {
    fn type_name(&self) -> &'static str {
        "group"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn provide<'a>(&self, context: &LootContext<'a>) -> SlotCollection<'a> {
        let collections = self
            .terms
            .holders(context.get_resolver())
            .iter()
            .map(|t| t.value(context.get_resolver()).provide(context))
            .collect();
        SlotCollection::concat(collections)
    }
    fn validate(&self, context: &ValidationContext) {
        context.validate_context_usage(&[]);
        validate_holder_set(context, "terms", &self.terms);
    }
}

/// The transformed sources: `filtered`, `limit_slots`, `contents`.
#[derive(Clone)]
pub enum Transform {
    Filtered(ItemPredicate),
    Limit(i32),
    Contents(ContainerComponentManipulator),
}

#[derive(Clone)]
pub struct TransformedSlotSource {
    pub slot_source: SlotSourceHolder,
    pub transform: Transform,
}

impl SlotSource for TransformedSlotSource {
    fn type_name(&self) -> &'static str {
        match self.transform {
            Transform::Filtered(_) => "filtered",
            Transform::Limit(_) => "limit_slots",
            Transform::Contents(_) => "contents",
        }
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn provide<'a>(&self, context: &LootContext<'a>) -> SlotCollection<'a> {
        let slots = self
            .slot_source
            .value(context.get_resolver())
            .provide(context);
        match &self.transform {
            Transform::Filtered(filter) => {
                let filter = filter.clone();
                slots.filter(move |s| filter.test(s))
            }
            Transform::Limit(limit) => slots.limit(*limit),
            Transform::Contents(component) => {
                let component = *component;
                slots.flat_map(move |slot| component.get_slots(slot))
            }
        }
    }
    fn validate(&self, context: &ValidationContext) {
        validate_holder_field(context, "slot_source", &self.slot_source);
    }
}

/// `RangeSlotSource`: `source` is an entity/block-entity target or the `container`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RangeSource {
    Entity(EntityTarget),
    BlockEntity,
    Container,
}

#[derive(Clone)]
pub struct RangeSlotSource {
    pub source: RangeSource,
    pub slot_range: Vec<i32>,
    pub slot_range_name: String,
}

impl SlotSource for RangeSlotSource {
    fn type_name(&self) -> &'static str {
        "slot_range"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn referenced_params(&self) -> Vec<ContextKey> {
        vec![match self.source {
            RangeSource::Entity(t) => t.context_param(),
            RangeSource::BlockEntity => ContextKey::BlockEntity,
            RangeSource::Container => ContextKey::Container,
        }]
    }
    fn provide<'a>(&self, context: &LootContext<'a>) -> SlotCollection<'a> {
        let params = context.params();
        let provider: Option<&'a dyn SlotProvider> = match self.source {
            RangeSource::Container => params.container,
            RangeSource::Entity(t) => t.get(context).and_then(|e| e.as_slot_provider()),
            RangeSource::BlockEntity => params.block_entity.and_then(|b| b.as_slot_provider()),
        };
        match provider {
            Some(p) => p.get_slots_from_range(&self.slot_range),
            None => SlotCollection::Empty,
        }
    }
}

// ---- codecs -------------------------------------------------------------------------------------------------------------------------------

fn entry<S: SlotSource + Clone + 'static>(codec: MapCodec<S>) -> MapCodec<SlotSourceRef> {
    lift(
        codec,
        |s| Arc::new(s) as SlotSourceRef,
        |t| t.as_any().downcast_ref::<S>(),
    )
}

static TYPES: LazyLock<Arc<TypeTable<dyn SlotSource>>> = LazyLock::new(|| {
    let holder = slot_source_holder_codec;
    let transformed = |codec: MapCodec<Transform>| {
        entry(record!(TransformedSlotSource {
            slot_source: holder().field_of("slot_source"),
            transform: codec
        }))
    };
    TypeTable::new(
        "minecraft:slot_source_type",
        vec![
            (
                "group",
                entry(slot_source_list_codec().field_of("terms").xmap(
                    |terms| GroupSlotSource { terms },
                    |g: &GroupSlotSource| g.terms.clone(),
                )),
            ),
            (
                "filtered",
                transformed(ItemPredicate::codec().field_of("item_filter").xmap(
                    Transform::Filtered,
                    |t: &Transform| match t {
                        Transform::Filtered(p) => p.clone(),
                        _ => unreachable!(),
                    },
                )),
            ),
            (
                "limit_slots",
                transformed(mc_dfu::extra::POSITIVE_INT.field_of("limit").xmap(
                    Transform::Limit,
                    |t: &Transform| match t {
                        Transform::Limit(l) => *l,
                        _ => unreachable!(),
                    },
                )),
            ),
            (
                "slot_range",
                entry(
                    range_source_codec()
                        .optional_field_of("source")
                        .pair(&slot_range_codec().field_of("slots"))
                        .xmap(
                            |(source, (name, slots))| RangeSlotSource {
                                source: source.unwrap_or(RangeSource::Container),
                                slot_range: slots,
                                slot_range_name: name,
                            },
                            |r: &RangeSlotSource| {
                                (
                                    if r.source == RangeSource::Container {
                                        None
                                    } else {
                                        Some(r.source)
                                    },
                                    (r.slot_range_name.clone(), r.slot_range.clone()),
                                )
                            },
                        ),
                ),
            ),
            (
                "contents",
                transformed(
                    ContainerComponentManipulator::codec()
                        .field_of("component")
                        .xmap(Transform::Contents, |t: &Transform| match t {
                            Transform::Contents(c) => *c,
                            _ => unreachable!(),
                        }),
                ),
            ),
            ("empty", entry(MapCodec::unit(EmptySlotSource))),
        ],
    )
});

fn range_source_codec() -> Codec<RangeSource> {
    static ALL: LazyLock<Vec<(RangeSource, &'static str)>> = LazyLock::new(|| {
        let mut all: Vec<(RangeSource, &'static str)> = EntityTarget::ALL
            .iter()
            .map(|t| (RangeSource::Entity(*t), t.name()))
            .collect();
        all.push((RangeSource::BlockEntity, "block_entity"));
        all.push((RangeSource::Container, "container"));
        all
    });
    mc_dfu::string_enum_codec(LazyLock::force(&ALL).as_slice())
}

/// `SlotRanges.CODEC`: the name and its slot ids.
fn slot_range_codec() -> Codec<(String, Vec<i32>)> {
    mc_dfu::STRING.flat_xmap(
        |name: String| match SlotRanges::name_to_ids(&name) {
            Some(r) => mc_dfu::DataResult::success((
                r.get_serialized_name().to_owned(),
                r.slots().to_vec(),
            )),
            None => mc_dfu::DataResult::error(move || format!("Unknown element name:{name}")),
        },
        |(name, _): &(String, Vec<i32>)| mc_dfu::DataResult::success(name.clone()),
    )
}

fn typed_slot_source_codec() -> Codec<SlotSourceRef> {
    Codec::lazy(|| {
        static C: LazyLock<Codec<SlotSourceRef>> = LazyLock::new(|| TYPES.codec(|s| s.type_name()));
        C.clone()
    })
}

/// `SlotSources.DIRECT_CODEC`: a typed source or a bare list (`GroupSlotSource.INLINE_CODEC`).
pub fn slot_source_codec() -> Codec<SlotSourceRef> {
    Codec::lazy(|| {
        static C: LazyLock<Codec<SlotSourceRef>> = LazyLock::new(|| {
            let inline = slot_source_list_codec().xmap(
                |terms| GroupSlotSource { terms },
                |g: &GroupSlotSource| g.terms.clone(),
            );
            typed_slot_source_codec().either(&inline).xmap(
                |e| match e {
                    Either::Left(s) => s,
                    Either::Right(g) => Arc::new(g) as SlotSourceRef,
                },
                |s: &SlotSourceRef| match s.as_any().downcast_ref::<GroupSlotSource>() {
                    Some(g) => Either::Right(g.clone()),
                    None => Either::Left(s.clone()),
                },
            )
        });
        C.clone()
    })
}

/// `SlotSources.CODEC`.
pub fn slot_source_holder_codec() -> Codec<SlotSourceHolder> {
    holder_codec(keys::SLOT_SOURCE, slot_source_codec())
}

/// `SlotSources.LIST_CODEC` (`holderSet(SLOT_SOURCE, TYPED_CODEC)`).
pub fn slot_source_list_codec() -> Codec<HolderSet<dyn SlotSource>> {
    holder_set_codec(keys::SLOT_SOURCE, typed_slot_source_codec())
}
