//! `ItemCommands`: `/item replace|fill|override|modify` over entity and container block slots.
use super::arg_provider::{ArgProvider, Factory};
use super::command_response_tracker::CommandResponseTracker;
use super::item::item_accessor::{ItemAccessor, ItemTarget, resolve_resource_or_id, slot_source_value};
use super::item::item_provider::ItemProvider;
use super::item::{block_item_accessor, entity_item_accessor};
use mc_game_core::commands::arguments::item::ItemArgument;
use mc_game_core::commands::arguments::resource_arguments::{LootModifierArgument, ResourceOrId};
use mc_game_core::commands::arguments::slot_arguments::{SlotSourceArgument, SlotSourceResult};
use mc_game_core::commands::message::{dynamic_translatable, message, simple_translatable};
use mc_game_core::commands::{CommandBuildContext, CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, literal};
use mc_game_core::registry::keys;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::item::slot::SlotSelector;
use mc_game_core::world::level::storage::loot::context::{LootContext, LootParams, VisitedEntry};
use mc_game_core::world::level::storage::loot::functions::{LootItemFunctionRef, function_codec};
use mc_game_core::world::level::storage::loot::LootContextParamSets;
use mc_game_core::{register_command, translatable};
use mc_brigadier::arguments::{IntegerArgumentType, integer_range};
use mc_brigadier::{
    ArgumentBuilder, CommandSyntaxException, Dynamic3CommandExceptionType, DynamicCommandExceptionType, LiteralArgumentBuilder, SimpleCommandExceptionType,
};
use mc_protocol::components::ItemStack;
use mc_text::Component;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::LazyLock;

pub static ERROR_TARGET_NOT_A_CONTAINER: LazyLock<Dynamic3CommandExceptionType<i32, i32, i32>> = LazyLock::new(|| {
    Dynamic3CommandExceptionType::new(|x: i32, y: i32, z: i32| message(translatable!("commands.item.target.not_a_container", x, y, z)))
});
pub static ERROR_SOURCE_NOT_A_CONTAINER: LazyLock<Dynamic3CommandExceptionType<i32, i32, i32>> = LazyLock::new(|| {
    Dynamic3CommandExceptionType::new(|x: i32, y: i32, z: i32| message(translatable!("commands.item.source.not_a_container", x, y, z)))
});
pub static ERROR_TARGET_INAPPLICABLE_SLOT: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| dynamic_translatable("commands.item.target.no_such_slot"));
static ERROR_TARGET_INAPPLICABLE_UNNAMED_SLOT: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("commands.item.target.no_such_slot.unnamed"));
static ERROR_SOURCE_INAPPLICABLE_SLOT: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| dynamic_translatable("commands.item.source.no_such_slot"));
static ERROR_SOURCE_INAPPLICABLE_UNNAMED_SLOT: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("commands.item.source.no_such_slot.unnamed"));
pub static ERROR_TARGET_NO_CHANGES: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.item.target.failed"));
pub static ERROR_TARGET_NO_CHANGES_KNOWN_ITEM: LazyLock<DynamicCommandExceptionType<Component>> = LazyLock::new(|| {
    DynamicCommandExceptionType::new(|item: Component| message(translatable!("commands.item.target.failed.known_item", item)))
});

pub type Accessor = Box<dyn ItemAccessor>;

/// `ItemCommands.ALL_PROVIDERS`.
pub const ALL_PROVIDERS: [Factory<Accessor>; 2] = [entity_item_accessor::provider, block_item_accessor::provider];

/// `ItemCommands.TARGET_PROVIDERS`.
pub static TARGET_PROVIDERS: LazyLock<Vec<ArgProvider<Accessor>>> = LazyLock::new(|| ArgProvider::build_list("target", &ALL_PROVIDERS));

/// `ItemCommands.SOURCE_PROVIDERS`.
pub static SOURCE_PROVIDERS: LazyLock<Vec<ArgProvider<Accessor>>> = LazyLock::new(|| ArgProvider::build_list("source", &ALL_PROVIDERS));

/// `ItemCommands.ItemDistributor`.
type Distributor = fn(Vec<ItemStack>) -> ItemProvider;

register_command!("ItemCommands", |ctx| {
    let build = ctx.build_context;
    let mut root = literal("item").requires(has_permission(LEVEL_GAMEMASTERS));
    for target_provider in TARGET_PROVIDERS.iter() {
        root = root
            .then(wrap_set_items(build, literal("replace"), target_provider, replace_item_provider))
            .then(wrap_set_items(build, literal("fill"), target_provider, fill_item_provider))
            .then(wrap_set_items(build, literal("override"), target_provider, override_item_provider))
            .then({
                let tp = target_provider.clone();
                target_provider.wrap(literal("modify"), move |p| {
                    p.then(argument("slots", SlotSourceArgument::slot_source(build)).then(
                        argument("modifier", LootModifierArgument::loot_modifier(build)).executes(move |c| {
                            modify_items(
                                c.get_source(),
                                &*tp.access(c)?,
                                &SlotSourceArgument::get_slot_source(c, "slots"),
                                &LootModifierArgument::get_loot_modifier(c, "modifier"),
                            )
                        }),
                    ))
                })
            });
    }
    ctx.dispatcher.register(root);
});

fn wrap_set_items(
    build: &CommandBuildContext,
    builder: LiteralArgumentBuilder<CommandSourceStack>,
    target_provider: &ArgProvider<Accessor>,
    distributor: Distributor,
) -> LiteralArgumentBuilder<CommandSourceStack> {
    let tp = target_provider.clone();
    target_provider.wrap(builder, move |t| {
        let mut slots_node = argument("slots", SlotSourceArgument::slot_source(build));
        for source_provider in SOURCE_PROVIDERS.iter() {
            let sp = source_provider.clone();
            let (tp1, sp1) = (tp.clone(), sp.clone());
            let (tp2, sp2) = (tp.clone(), sp.clone());
            slots_node = slots_node.then(source_provider.wrap(literal("from"), move |p| {
                p.then(
                    argument("sourceSlots", SlotSourceArgument::slot_source(build))
                        .executes(move |c| {
                            let items = get_items(c.get_source(), &*sp1.access(c)?, &SlotSourceArgument::get_slot_source(c, "sourceSlots"), None)?;
                            set_items_from(c.get_source(), &*tp1.access(c)?, &SlotSourceArgument::get_slot_source(c, "slots"), distributor(items), None)
                        })
                        .then(argument("modifier", LootModifierArgument::loot_modifier(build)).executes(move |c| {
                            let items = get_items(
                                c.get_source(),
                                &*sp2.access(c)?,
                                &SlotSourceArgument::get_slot_source(c, "sourceSlots"),
                                Some(&LootModifierArgument::get_loot_modifier(c, "modifier")),
                            )?;
                            set_items_from(c.get_source(), &*tp2.access(c)?, &SlotSourceArgument::get_slot_source(c, "slots"), distributor(items), None)
                        })),
                )
            }));
        }
        let (tp1, tp2) = (tp.clone(), tp.clone());
        slots_node = slots_node.then(
            literal("with").then(
                argument("item", ItemArgument::item(build))
                    .executes(move |c| {
                        let item = ItemArgument::get_item(c, "item").create_item_stack(1)?;
                        set_item(c.get_source(), &*tp1.access(c)?, &SlotSourceArgument::get_slot_source(c, "slots"), distributor, item)
                    })
                    .then(argument("count", integer_range(1, 99)).executes(move |c| {
                        let item = ItemArgument::get_item(c, "item").create_item_stack(IntegerArgumentType::get_integer(c, "count"))?;
                        set_item(c.get_source(), &*tp2.access(c)?, &SlotSourceArgument::get_slot_source(c, "slots"), distributor, item)
                    })),
            ),
        );
        t.then(slots_node)
    })
}

/// `ItemCommands.replaceItemProvider`.
fn replace_item_provider(items: Vec<ItemStack>) -> ItemProvider {
    ItemProvider::of(items)
}

/// `ItemCommands.fillItemProvider`.
fn fill_item_provider(items: Vec<ItemStack>) -> ItemProvider {
    ItemProvider::cycle(items)
}

/// `ItemCommands.overrideItemProvider`.
fn override_item_provider(items: Vec<ItemStack>) -> ItemProvider {
    ItemProvider::of(items).or_else_provide(ItemStack::empty())
}

fn inapplicable_target_slot(slot_source: &SlotSourceResult) -> CommandSyntaxException {
    match slot_source.name() {
        Some(name) => ERROR_TARGET_INAPPLICABLE_SLOT.create(name),
        None => ERROR_TARGET_INAPPLICABLE_UNNAMED_SLOT.create(),
    }
}

/// `setItems(source, accessor, slotSource, items, knownItem)`.
fn set_items_from(
    source: &CommandSourceStack,
    accessor: &dyn ItemAccessor,
    slot_source: &SlotSourceResult,
    mut items: ItemProvider,
    known_item: Option<&ItemStack>,
) -> Result<i32, CommandSyntaxException> {
    let selected_count = Rc::new(Cell::new(0));
    let selector = SlotSelector::tracking(SlotSelector::any_slot(), selected_count.clone());
    let tracker = RefCell::new(CommandResponseTracker::<ItemTarget>::create());
    let value = slot_source_value(source, slot_source)?;
    accessor.set_items(source, &value, &mut |_, targets| {
        targets
            .iter()
            .map(|(target, slots)| {
                items.restart();
                let count = slots.replace_slot_items(&mut items, &selector);
                tracker.borrow_mut().track(target, count);
                count
            })
            .collect()
    })?;
    if selected_count.get() == 0 {
        return Err(inapplicable_target_slot(slot_source));
    }
    let tracker = tracker.into_inner();
    accessor.get_replace_success(source, &tracker, known_item)
}

/// `setItem(source, accessor, slotSource, distributor, item)`.
fn set_item(
    source: &CommandSourceStack,
    accessor: &dyn ItemAccessor,
    slot_source: &SlotSourceResult,
    distributor: Distributor,
    item: ItemStack,
) -> Result<i32, CommandSyntaxException> {
    set_items_from(source, accessor, slot_source, distributor(vec![item.clone()]), Some(&item))
}

/// `ItemCommands.applyModifier(source, modifier)` for the duration of `f`: the unary operator running the loot function
/// in a `COMMAND` context at the source position (`limitSize` on the result).
fn with_modifier<R>(
    level: &ServerLevel,
    source: &CommandSourceStack,
    modifier: &LootItemFunctionRef,
    f: impl FnOnce(&mut dyn FnMut(ItemStack) -> ItemStack) -> R,
) -> R {
    let this_entity = source.get_entity().and_then(|e| level.get_entity(e.id));
    let params = LootParams::builder(level).origin(source.get_position()).optional_this_entity(this_entity).create(&LootContextParamSets::COMMAND);
    let mut context = LootContext::builder(&params).create(None);
    context.push_visited_element(VisitedEntry::modifier(modifier));
    let mut apply = |item: ItemStack| {
        let mut new_item = modifier.apply(item, &mut context);
        let max = new_item.get_max_stack_size();
        new_item.limit_size(max);
        new_item
    };
    f(&mut apply)
}

/// `modifyItems(source, accessor, slotSource, modifier)`.
fn modify_items(
    source: &CommandSourceStack,
    accessor: &dyn ItemAccessor,
    slot_source: &SlotSourceResult,
    modifier: &ResourceOrId,
) -> Result<i32, CommandSyntaxException> {
    let function = resolve_resource_or_id(source, keys::ITEM_MODIFIER, modifier, function_codec())?;
    let selected_count = Rc::new(Cell::new(0));
    let selector = SlotSelector::tracking(SlotSelector::selecting(|s: &ItemStack| !s.is_empty()), selected_count.clone());
    let tracker = RefCell::new(CommandResponseTracker::<ItemTarget>::create());
    let value = slot_source_value(source, slot_source)?;
    accessor.set_items(source, &value, &mut |level, targets| {
        with_modifier(level, source, &function, |apply| {
            targets
                .iter()
                .map(|(target, slots)| {
                    let mut count = 0;
                    // `CountingModifier.accept(slot)`
                    slots.modify_slots(
                        &mut |slot| {
                            if slot.set(apply(slot.get().copy())) {
                                count += 1;
                            }
                        },
                        &selector,
                    );
                    tracker.borrow_mut().track(target, count);
                    count
                })
                .collect()
        })
    })?;
    if selected_count.get() == 0 {
        return Err(inapplicable_target_slot(slot_source));
    }
    let tracker = tracker.into_inner();
    accessor.get_modify_success(source, &tracker)
}

/// `getItems(source, accessor, slotSource, modifier)`.
fn get_items(
    source: &CommandSourceStack,
    accessor: &dyn ItemAccessor,
    slot_source: &SlotSourceResult,
    modifier: Option<&ResourceOrId>,
) -> Result<Vec<ItemStack>, CommandSyntaxException> {
    let value = slot_source_value(source, slot_source)?;
    let slots = accessor.get_slots(source, &value)?;
    let items: Vec<ItemStack> = match modifier {
        Some(modifier) => {
            let function = resolve_resource_or_id(source, keys::ITEM_MODIFIER, modifier, function_codec())?;
            source.with_server(|server| {
                let level = server.level(source.get_level()).expect("source level");
                with_modifier(level, source, &function, |apply| slots.item_copies().iter().map(|item| apply(item.copy())).collect())
            })
        }
        None => slots.item_copies().to_vec(),
    };
    if items.is_empty() {
        return Err(match slot_source.name() {
            Some(name) => ERROR_SOURCE_INAPPLICABLE_SLOT.create(name),
            None => ERROR_SOURCE_INAPPLICABLE_UNNAMED_SLOT.create(),
        });
    }
    Ok(items)
}
