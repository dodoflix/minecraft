//! `ContainerComponentManipulator(s)`, `set_contents`, `modify_contents`.

use std::sync::Arc;

use mc_dfu::{Codec, DataResult, MapCodec, record};
use mc_protocol::components::values::containers::{
    BUNDLE_CONTENTS, BundleContents, CHARGED_PROJECTILES, CONTAINER, ChargedProjectiles,
    ItemContainerContents,
};
use mc_protocol::components::{ItemStack, ItemStackTemplate};

use super::{
    ConditionalFunction, FunctionHolder, LootItemFunctionRef, condition_field,
    function_holder_codec,
};
use crate::world::item::BundleMutable;
use crate::world::item::slot::{SlotAccess, SlotCollection, SlotRef};
use crate::world::level::storage::loot::context::LootContext;
use crate::world::level::storage::loot::entries::{
    LootPoolEntry, LootPoolEntryContainer, entry_codec,
};
use crate::world::level::storage::loot::holder::lift;
use crate::world::level::storage::loot::predicates::ConditionHolder;
use crate::world::level::storage::loot::table::create_stack_splitter;
use crate::world::level::storage::loot::validation::{ValidationContext, validate_holder_field};

type Entries = Vec<(&'static str, MapCodec<LootItemFunctionRef>)>;

fn entry<F: ConditionalFunction + Clone>(codec: MapCodec<F>) -> MapCodec<LootItemFunctionRef> {
    lift(
        codec,
        |f| Arc::new(f) as LootItemFunctionRef,
        |t| t.as_any().downcast_ref::<F>(),
    )
}

/// `ContainerComponentManipulator<T>`: the container-like data components (`container`, `bundle_contents`,
/// `charged_projectiles`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ContainerComponentManipulator {
    Container,
    BundleContents,
    ChargedProjectiles,
}

fn template(stack: &ItemStack) -> Option<ItemStackTemplate> {
    if stack.is_empty() {
        None
    } else {
        ItemStackTemplate::from_stack(stack).ok()
    }
}

impl ContainerComponentManipulator {
    /// The component's type name (`ContainerComponentManipulators.CODEC` is the data component type id).
    fn type_name(self) -> &'static str {
        match self {
            ContainerComponentManipulator::Container => "container",
            ContainerComponentManipulator::BundleContents => "bundle_contents",
            ContainerComponentManipulator::ChargedProjectiles => "charged_projectiles",
        }
    }

    /// `ContainerComponentManipulators.CODEC`.
    pub fn codec() -> Codec<ContainerComponentManipulator> {
        mc_protocol::components::map::type_codec().comap_flat_map(
            |t| match t.name() {
                "minecraft:container" => {
                    DataResult::success(ContainerComponentManipulator::Container)
                }
                "minecraft:bundle_contents" => {
                    DataResult::success(ContainerComponentManipulator::BundleContents)
                }
                "minecraft:charged_projectiles" => {
                    DataResult::success(ContainerComponentManipulator::ChargedProjectiles)
                }
                _ => DataResult::error_str("No items in component"),
            },
            |m: &ContainerComponentManipulator| {
                mc_data::DataComponentType::by_name(&format!("minecraft:{}", m.type_name()))
                    .expect("registered component type")
            },
        )
    }

    /// `itemCopies()` of the component (`None` when the stack has no such component).
    fn contents(self, stack: &ItemStack) -> Option<Vec<ItemStack>> {
        match self {
            ContainerComponentManipulator::Container => stack.get(&CONTAINER).map(|c| {
                c.items()
                    .iter()
                    .map(|t| {
                        t.as_ref()
                            .map_or_else(ItemStack::empty, ItemStackTemplate::create)
                    })
                    .collect()
            }),
            ContainerComponentManipulator::BundleContents => stack
                .get(&BUNDLE_CONTENTS)
                .map(|c| c.items().iter().map(ItemStackTemplate::create).collect()),
            ContainerComponentManipulator::ChargedProjectiles => stack
                .get(&CHARGED_PROJECTILES)
                .map(|c| c.items.iter().map(ItemStackTemplate::create).collect()),
        }
    }

    /// `setContents(itemStack, newContents)` (`currentValue.copyWithContents(newContents)`).
    pub fn set_contents(self, stack: &mut ItemStack, new_contents: Vec<ItemStack>) {
        match self {
            ContainerComponentManipulator::Container => {
                // `ItemContainerContents.fromItems`: trailing empty slots are dropped
                let last = new_contents.iter().rposition(|s| !s.is_empty());
                let items: Vec<Option<ItemStackTemplate>> = match last {
                    None => Vec::new(),
                    Some(last) => new_contents[..=last].iter().map(template).collect(),
                };
                stack.set(CONTAINER, ItemContainerContents::from_items(items));
            }
            ContainerComponentManipulator::BundleContents => {
                let mut mutable = BundleMutable::from_contents(&BundleContents::default());
                for mut s in new_contents {
                    mutable.try_insert(&mut s);
                }
                stack.set(BUNDLE_CONTENTS, mutable.to_immutable());
            }
            ContainerComponentManipulator::ChargedProjectiles => {
                stack.set(
                    CHARGED_PROJECTILES,
                    ChargedProjectiles {
                        items: new_contents.iter().filter_map(template).collect(),
                    },
                );
            }
        }
    }

    /// `modifyItems(itemStack, modifier)`.
    pub fn modify_items(
        self,
        stack: &mut ItemStack,
        modifier: &mut dyn FnMut(ItemStack) -> ItemStack,
    ) {
        if let Some(contents) = self.contents(stack) {
            let new: Vec<ItemStack> = contents
                .into_iter()
                .map(|current| {
                    if current.is_empty() {
                        current
                    } else {
                        let mut n = modifier(current);
                        n.limit_size(n.get_max_stack_size());
                        n
                    }
                })
                .collect();
            self.set_contents(stack, new);
        }
    }

    /// `getSlots(itemStack)`: one slot per stored item.
    pub fn get_slots<'a>(self, slot: SlotRef<'a>) -> SlotCollection<'a> {
        let size = self.contents(&slot.get()).map_or(0, |c| c.len());
        let slots: Vec<SlotRef<'a>> = (0..size)
            .map(|index| {
                Rc::new(ComponentSlot {
                    owner: slot.clone(),
                    manipulator: self,
                    index,
                }) as SlotRef<'a>
            })
            .collect();
        SlotCollection::of(slots)
    }
}

use std::rc::Rc;

/// One stored item of a container component of the stack in `owner`.
struct ComponentSlot<'a> {
    owner: SlotRef<'a>,
    manipulator: ContainerComponentManipulator,
    index: usize,
}

impl SlotAccess for ComponentSlot<'_> {
    fn get(&self) -> ItemStack {
        self.manipulator
            .contents(&self.owner.get())
            .and_then(|c| c.into_iter().nth(self.index))
            .unwrap_or_default()
    }
    fn set(&self, stack: ItemStack) -> bool {
        let mut owner = self.owner.get();
        let mut contents = self.manipulator.contents(&owner).unwrap_or_default();
        if self.index >= contents.len() {
            return false;
        }
        contents[self.index] = stack;
        self.manipulator.set_contents(&mut owner, contents);
        self.owner.set(owner)
    }
}

// ---- functions ----------------------------------------------------------------------------------------------------------------------

#[derive(Clone)]
pub struct SetContainerContents {
    pub condition: Option<ConditionHolder>,
    pub component: ContainerComponentManipulator,
    pub entries: Vec<LootPoolEntryContainer>,
}

impl ConditionalFunction for SetContainerContents {
    fn type_name(&self) -> &'static str {
        "set_contents"
    }
    fn condition(&self) -> &Option<ConditionHolder> {
        &self.condition
    }
    fn validate_fields(&self, context: &ValidationContext) {
        for (i, e) in self.entries.iter().enumerate() {
            e.validate(&context.for_indexed_field("entries", i));
        }
    }
    fn run(&self, mut stack: ItemStack, context: &mut LootContext) -> ItemStack {
        if stack.is_empty() {
            return stack;
        }
        let level = context.get_level();
        let mut contents: Vec<ItemStack> = Vec::new();
        for e in &self.entries {
            let mut expanded: Vec<LootPoolEntry> = Vec::new();
            e.expand(context, &mut expanded);
            for entry in expanded {
                let mut push = |s: ItemStack| contents.push(s);
                let mut split = create_stack_splitter(level, &mut push);
                entry.create_item_stack(&mut |s, _| split(s), context);
            }
        }
        self.component.set_contents(&mut stack, contents);
        stack
    }
}

#[derive(Clone)]
pub struct ModifyContainerContents {
    pub condition: Option<ConditionHolder>,
    pub component: ContainerComponentManipulator,
    pub modifier: FunctionHolder,
}

impl ConditionalFunction for ModifyContainerContents {
    fn type_name(&self) -> &'static str {
        "modify_contents"
    }
    fn condition(&self) -> &Option<ConditionHolder> {
        &self.condition
    }
    fn validate_fields(&self, context: &ValidationContext) {
        validate_holder_field(context, "modifier", &self.modifier);
    }
    fn run(&self, mut stack: ItemStack, context: &mut LootContext) -> ItemStack {
        if stack.is_empty() {
            return stack;
        }
        let function = self.modifier.value(context.get_resolver());
        self.component
            .modify_items(&mut stack, &mut |c| function.apply(c, context));
        stack
    }
}

pub(super) fn register(entries: &mut Entries) {
    let cond = condition_field;
    entries.push((
        "set_contents",
        entry(record!(SetContainerContents {
            condition: cond(),
            component: ContainerComponentManipulator::codec().field_of("component"),
            entries: entry_codec().list_of().field_of("entries"),
        })),
    ));
    entries.push((
        "modify_contents",
        entry(record!(ModifyContainerContents {
            condition: cond(),
            component: ContainerComponentManipulator::codec().field_of("component"),
            modifier: function_holder_codec().field_of("modifier"),
        })),
    ));
}
