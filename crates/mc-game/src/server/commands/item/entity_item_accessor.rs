//! `EntityItemAccessor`: `/item ... entity <targets>`.
use super::item_accessor::{ItemAccessor, ItemTarget, Setter, SlotSnapshot, provide_all};
use super::slots::{EntitySlots, apply_entity_writes, entity_in, slot_params};
use crate::commands::arguments::selector::EntityRef;
use crate::commands::arguments::EntityArgument;
use crate::commands::{CommandSourceStack, argument, host};
use crate::server::commands::arg_provider::ArgProvider;
use crate::server::commands::command_response_tracker::CommandResponseTracker;
use crate::server::commands::item_commands::{ERROR_TARGET_NO_CHANGES, ERROR_TARGET_NO_CHANGES_KNOWN_ITEM};
use crate::translatable;
use crate::world::entity::Entity;
use crate::world::item::slot::{SlotCollection, SlotSourceRef};
use crate::world::item::ItemStackExt;
use crate::world::level::storage::loot::context::LootParams;
use mc_brigadier::CommandSyntaxException;
use mc_protocol::components::ItemStack;
use mc_text::Component;

pub struct EntityItemAccessor {
    entities: Vec<EntityRef>,
}

/// `EntityItemAccessor.PROVIDER`.
pub fn provider(arg: &'static str) -> ArgProvider<Box<dyn ItemAccessor>> {
    ArgProvider::create(
        "entity",
        move || argument(arg, EntityArgument::entities()),
        move |c| Ok(Box::new(EntityItemAccessor { entities: EntityArgument::get_entities(c, arg)? }) as Box<dyn ItemAccessor>),
    )
}

fn display_name(source: &CommandSourceStack, target: &ItemTarget) -> Component {
    let ItemTarget::Entity(entity) = target else { return Component::empty() };
    source.with_server(|server| entity_in(server, entity).map_or_else(Component::empty, |e: &dyn Entity| host::entity_display_name(server, e)))
}

impl ItemAccessor for EntityItemAccessor {
    fn set_items(&self, source: &CommandSourceStack, slot_source: &SlotSourceRef, function: &mut Setter<'_>) -> Result<(), CommandSyntaxException> {
        source.with_server(|server| {
            let (counts, writes) = {
                let server = &*server;
                let Some(level) = server.level(source.get_level()) else { return Ok(()) };
                let targets: Vec<(&EntityRef, &dyn Entity)> = self.entities.iter().filter_map(|r| entity_in(server, r).map(|e| (r, e))).collect();
                let providers: Vec<EntitySlots<'_>> = targets.iter().map(|(_, e)| EntitySlots::new(*e)).collect();
                let params: Vec<LootParams<'_>> = providers.iter().map(|p| slot_params(level, source, p)).collect();
                let collections = provide_all(&params, slot_source);
                let pairs: Vec<(ItemTarget, SlotCollection<'_>)> =
                    targets.iter().zip(collections).map(|((r, _), c)| (ItemTarget::Entity((*r).clone()), c)).collect();
                let counts = function(level, &pairs);
                drop(pairs);
                drop(params);
                let refs: Vec<EntityRef> = targets.iter().map(|(r, _)| (*r).clone()).collect();
                (counts, refs.into_iter().zip(providers.into_iter().map(EntitySlots::into_writes)).collect::<Vec<_>>())
            };
            for (count, (entity, writes)) in counts.into_iter().zip(writes) {
                if let Some(level) = server.level_mut(&entity.dimension) {
                    apply_entity_writes(level, entity.id, writes);
                    if count > 0 {
                        // `entity instanceof ServerPlayer`: `containerMenu.broadcastChanges()`
                        level.with_player(entity.id, |level, player| player.broadcast_menu_changes(level));
                    }
                }
            }
            Ok(())
        })
    }

    fn get_slots(&self, source: &CommandSourceStack, slot_source: &SlotSourceRef) -> Result<SlotSnapshot, CommandSyntaxException> {
        Ok(source.with_server(|server| {
            let Some(level) = server.level(source.get_level()) else { return SlotSnapshot::empty() };
            let entities: Vec<&dyn Entity> = self.entities.iter().filter_map(|r| entity_in(server, r)).collect();
            let providers: Vec<EntitySlots<'_>> = entities.iter().map(|e| EntitySlots::new(*e)).collect();
            let params: Vec<LootParams<'_>> = providers.iter().map(|p| slot_params(level, source, p)).collect();
            let mut snapshot = SlotSnapshot::empty();
            for collection in provide_all(&params, slot_source) {
                snapshot.append(SlotSnapshot::of(&collection));
            }
            snapshot
        }))
    }

    fn get_replace_success(
        &self,
        source: &CommandSourceStack,
        tracker: &CommandResponseTracker<ItemTarget>,
        known_item: Option<&ItemStack>,
    ) -> Result<i32, CommandSyntaxException> {
        match known_item {
            Some(item) => tracker.send_feedback(
                source,
                true,
                Some(ERROR_TARGET_NO_CHANGES_KNOWN_ITEM.create(item.get_display_name())),
                |entity, slot_count| {
                    translatable!("commands.item.entity.replace.success.single.known_item", slot_count, display_name(source, entity), item.get_display_name())
                },
                |entity_count, _| translatable!("commands.item.entity.replace.success.multiple.known_item", entity_count, item.get_display_name()),
            ),
            None => tracker.send_feedback(
                source,
                true,
                Some(ERROR_TARGET_NO_CHANGES.create()),
                |entity, slot_count| translatable!("commands.item.entity.replace.success.single", slot_count, display_name(source, entity)),
                |entity_count, _| translatable!("commands.item.entity.replace.success.multiple", entity_count),
            ),
        }
    }

    fn get_modify_success(&self, source: &CommandSourceStack, tracker: &CommandResponseTracker<ItemTarget>) -> Result<i32, CommandSyntaxException> {
        tracker.send_feedback(
            source,
            true,
            Some(ERROR_TARGET_NO_CHANGES.create()),
            |entity, slot_count| translatable!("commands.item.entity.modify.success.single", slot_count, display_name(source, entity)),
            |entity_count, _| translatable!("commands.item.entity.modify.success.multiple", entity_count),
        )
    }
}
