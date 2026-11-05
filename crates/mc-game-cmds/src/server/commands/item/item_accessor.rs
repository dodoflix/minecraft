//! `ItemAccessor<Target>`: what `/item` (and `execute if items`) can read and write: the slots of entities or of a container
//! block, selected by a `SlotSource`.
//!
//! Vanilla lets the setter write slots while it runs. A `SlotCollection` here borrows the level it was built from, so the
//! accessor builds the collections of every target (reads through a write overlay, see `slots`), hands them to the
//! setter in one call together with the level (loot contexts of `item modify` need it), and then applies the recorded
//! writes to the targets. The observable effect equals vanilla's per-target interleaving except that a loot function
//! evaluated for one target no longer sees the slot writes made to another target of the same command.
use mc_game_core::commands::arguments::selector::EntityRef;
use mc_game_core::commands::arguments::slot_arguments::SlotSourceResult;
use mc_game_core::commands::arguments::resource_arguments::ResourceOrId;
use mc_game_core::commands::message::message;
use mc_game_core::commands::CommandSourceStack;
use crate::server::commands::command_response_tracker::CommandResponseTracker;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::translatable;
use mc_game_core::world::item::slot::{RangeSlotSource, RangeSource, SlotCollection, SlotSourceRef, slot_source_codec};
use mc_game_core::world::level::storage::loot::context::{LootContext, LootParams, VisitedEntry};
use mc_game_core::registry::keys;
use mc_brigadier::{CommandSyntaxException, DynamicCommandExceptionType};
use mc_dfu::Codec;
use mc_core::pos::BlockPos;
use mc_protocol::components::ItemStack;
use std::sync::{Arc, LazyLock};

/// The `Target` type parameter of `ItemAccessor`: an entity or a block position.
#[derive(Clone, Debug)]
pub enum ItemTarget {
    Entity(EntityRef),
    Block(BlockPos),
}

/// `SlotCollection.size()` and `itemCopies()` of a collection, taken while its target was borrowed.
pub struct SlotSnapshot {
    size: i32,
    item_copies: Vec<ItemStack>,
}

impl SlotSnapshot {
    pub fn of(slots: &SlotCollection<'_>) -> Self {
        Self { size: slots.size(), item_copies: slots.item_copies() }
    }

    pub fn empty() -> Self {
        Self { size: 0, item_copies: Vec::new() }
    }

    pub fn append(&mut self, other: SlotSnapshot) {
        self.size += other.size;
        self.item_copies.extend(other.item_copies);
    }

    /// `size()`.
    pub fn size(&self) -> i32 {
        self.size
    }

    /// `itemCopies()`.
    pub fn item_copies(&self) -> &[ItemStack] {
        &self.item_copies
    }
}

/// `ItemAccessor.SetterFunction` for all targets at once: given the level and `(target, slots)` of every target, applies
/// the change and returns the number of updated slots per target (same order).
pub type Setter<'f> = dyn for<'l> FnMut(&'l ServerLevel, &[(ItemTarget, SlotCollection<'l>)]) -> Vec<i32> + 'f;

/// `ItemAccessor`.
pub trait ItemAccessor: Send + Sync {
    /// `setItems(source, slotSource, function)`.
    fn set_items(&self, source: &CommandSourceStack, slot_source: &SlotSourceRef, function: &mut Setter<'_>) -> Result<(), CommandSyntaxException>;

    /// `getSlots(source, slotSource)`.
    fn get_slots(&self, source: &CommandSourceStack, slot_source: &SlotSourceRef) -> Result<SlotSnapshot, CommandSyntaxException>;

    /// `getReplaceSuccess(source, tracker, knownItem)`.
    fn get_replace_success(
        &self,
        source: &CommandSourceStack,
        tracker: &CommandResponseTracker<ItemTarget>,
        known_item: Option<&ItemStack>,
    ) -> Result<i32, CommandSyntaxException>;

    /// `getModifySuccess(source, tracker)`.
    fn get_modify_success(&self, source: &CommandSourceStack, tracker: &CommandResponseTracker<ItemTarget>) -> Result<i32, CommandSyntaxException>;
}

/// `ItemCommands.getSlotsFromProvider(source, container, slotSource)` for every `params` (one per provider).
pub fn provide_all<'a>(params: &'a [LootParams<'a>], slot_source: &SlotSourceRef) -> Vec<SlotCollection<'a>> {
    params
        .iter()
        .map(|p| {
            let mut context = LootContext::builder(p).create(None);
            context.push_visited_element(VisitedEntry::slot_source(slot_source));
            slot_source.provide(&context)
        })
        .collect()
}

static ERROR_FAILED_TO_PARSE: LazyLock<DynamicCommandExceptionType<String>> = LazyLock::new(|| {
    DynamicCommandExceptionType::new(|error: String| message(translatable!("argument.resource_or_id.failed_to_parse", error)))
});

/// `ResourceOrIdArgument` result as the element: a registered element of `registry` or an inline definition decoded
/// with `codec`. Inline definitions are decoded here, not while parsing (the argument keeps the raw tag), so a malformed
/// one fails when the command runs.
pub fn resolve_resource_or_id<T: Clone + Send + Sync + 'static>(
    source: &CommandSourceStack,
    registry: &'static str,
    value: &ResourceOrId,
    codec: Codec<T>,
) -> Result<T, CommandSyntaxException> {
    match value {
        ResourceOrId::Reference(key) => Ok(source
            .with_server(|server| server.level(source.get_level()).and_then(|l| l.registries.get::<T>(registry, &key.id).cloned()))
            .unwrap_or_else(|| panic!("Trying to access unbound value 'ResourceKey[{} / {}]'", key.registry, key.id))),
        ResourceOrId::Inline(tag) => mc_dfu::parse_nbt(&codec, tag).into_result().map_err(|msg| ERROR_FAILED_TO_PARSE.create(msg)),
    }
}

/// `SlotSourceArgument.Result.value()`: the `SlotSource` of a slot range literal, a registered `slot_source` or an inline
/// definition.
pub fn slot_source_value(source: &CommandSourceStack, result: &SlotSourceResult) -> Result<SlotSourceRef, CommandSyntaxException> {
    match result {
        SlotSourceResult::Literal(range) => Ok(Arc::new(RangeSlotSource {
            source: RangeSource::Container,
            slot_range: range.slots().to_vec(),
            slot_range_name: range.get_serialized_name().to_owned(),
        })),
        SlotSourceResult::Holder(holder) => resolve_resource_or_id(source, keys::SLOT_SOURCE, holder, slot_source_codec()),
    }
}
