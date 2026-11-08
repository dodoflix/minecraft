//! `LootCommand`: `/loot <target> <source>`: roll loot (fishing, a loot table, an entity's kill drops, a mined block) and
//! replace container/entity slots, insert into a container, give to players or spawn the items.
use super::command_response_tracker::{CommandResponseTracker, ElementType};
use super::item::item_accessor::resolve_resource_or_id;
use super::item_commands::{ERROR_TARGET_INAPPLICABLE_SLOT, ERROR_TARGET_NOT_A_CONTAINER};
use mc_brigadier::arguments::{IntegerArgumentType, integer_min};
use mc_brigadier::{
    ArgumentBuilder, CommandContext, CommandSyntaxException, DynamicCommandExceptionType,
    RequiredArgumentBuilder,
};
use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_game_core::commands::arguments::EntityArgument;
use mc_game_core::commands::arguments::coordinates::Vec3Argument;
use mc_game_core::commands::arguments::coordinates::block_pos_argument::BlockPosArgument;
use mc_game_core::commands::arguments::item::ItemArgument;
use mc_game_core::commands::arguments::resource_arguments::{LootTableArgument, ResourceOrId};
use mc_game_core::commands::arguments::selector::EntityRef;
use mc_game_core::commands::arguments::slot_arguments::SlotArgument;
use mc_game_core::commands::message::message;
use mc_game_core::commands::{
    CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, host, literal,
};
use mc_game_core::registry::keys;
use mc_game_core::server::commands::item::slots::{
    EntitySlots, apply_container_writes, apply_entity_writes, container_at, entity_in,
    give_to_inventory,
};
use mc_game_core::world::container::Container;
use mc_game_core::world::entity::equipment::EquipmentSlot;
use mc_game_core::world::entity::item::ItemEntity;
use mc_game_core::world::item::ItemStackExt;
use mc_game_core::world::item::slot::SlotProvider;
use mc_game_core::world::level::BlockGetter;
use mc_game_core::world::level::storage::loot::LootContextParamSets;
use mc_game_core::world::level::storage::loot::block_drops::state_get_drops;
use mc_game_core::world::level::storage::loot::context::LootParams;
use mc_game_core::world::level::storage::loot::loading::get_loot_table;
use mc_game_core::world::level::storage::loot::table::loot_table_direct_codec;
use mc_game_core::{register_command, translatable};
use mc_protocol::components::ItemStack;
use mc_text::Component;
use std::sync::{Arc, LazyLock};

static ERROR_NO_HELD_ITEMS: LazyLock<DynamicCommandExceptionType<Component>> =
    LazyLock::new(|| {
        DynamicCommandExceptionType::new(|entity: Component| {
            message(translatable!("commands.drop.no_held_items", entity))
        })
    });
static ERROR_NO_ENTITY_LOOT_TABLE: LazyLock<DynamicCommandExceptionType<Component>> =
    LazyLock::new(|| {
        DynamicCommandExceptionType::new(|entity: Component| {
            message(translatable!("commands.drop.no_loot_table.entity", entity))
        })
    });
static ERROR_NO_BLOCK_LOOT_TABLE: LazyLock<DynamicCommandExceptionType<Component>> =
    LazyLock::new(|| {
        DynamicCommandExceptionType::new(|block: Component| {
            message(translatable!("commands.drop.no_loot_table.block", block))
        })
    });

type Tracker = CommandResponseTracker<ItemStack>;
/// The command body built for one tool choice.
type ToolRun = Box<
    dyn Fn(&CommandContext<CommandSourceStack>) -> Result<i32, CommandSyntaxException>
        + Send
        + Sync,
>;

/// `LootCommand.DropConsumer`.
type DropConsumer = Arc<
    dyn Fn(
            &CommandContext<CommandSourceStack>,
            &[ItemStack],
            &mut Tracker,
        ) -> Result<(), CommandSyntaxException>
        + Send
        + Sync,
>;

/// `LootCommand.TailProvider`.
type TailProvider<'a> = &'a dyn Fn(
    RequiredArgumentBuilder<CommandSourceStack>,
    DropConsumer,
) -> RequiredArgumentBuilder<CommandSourceStack>;

/// Where the tool of `fish` / `mine` comes from.
#[derive(Clone, Copy)]
enum Tool {
    Empty,
    Item,
    Hand(EquipmentSlot),
}

register_command!("LootCommand", |ctx| {
    let build = ctx.build_context;
    ctx.dispatcher.register(add_targets(
        literal("loot").requires(has_permission(LEVEL_GAMEMASTERS)),
        &|target, output| {
            let tool_nodes = |node: RequiredArgumentBuilder<CommandSourceStack>,
                              run: &dyn Fn(Tool) -> ToolRun| {
                node.then(argument("tool", ItemArgument::item(build)).executes({
                    let f = run(Tool::Item);
                    move |c| f(c)
                }))
                .then(literal("mainhand").executes({
                    let f = run(Tool::Hand(EquipmentSlot::MainHand));
                    move |c| f(c)
                }))
                .then(literal("offhand").executes({
                    let f = run(Tool::Hand(EquipmentSlot::OffHand));
                    move |c| f(c)
                }))
            };
            let fish_output = output.clone();
            let loot_output = output.clone();
            let kill_output = output.clone();
            let mine_output = output.clone();
            let mine_run = move |tool: Tool| -> ToolRun {
                let output = mine_output.clone();
                Box::new(move |c| {
                    drop_block_loot(
                        c,
                        BlockPosArgument::get_loaded_block_pos(c, "pos")?,
                        resolve_tool(c, tool)?,
                        &output,
                    )
                })
            };
            let fish_run = move |tool: Tool| -> ToolRun {
                let output = fish_output.clone();
                Box::new(move |c| {
                    drop_fishing_loot(
                        c,
                        &LootTableArgument::get_loot_table(c, "loot_table"),
                        BlockPosArgument::get_loaded_block_pos(c, "pos")?,
                        resolve_tool(c, tool)?,
                        &output,
                    )
                })
            };
            target
                .then(literal("fish").then(
                    argument("loot_table", LootTableArgument::loot_table(build)).then(tool_nodes(
                        argument("pos", BlockPosArgument::block_pos()).executes({
                            let f = fish_run(Tool::Empty);
                            move |c| f(c)
                        }),
                        &fish_run,
                    )),
                ))
                .then(literal("loot").then(
                    argument("loot_table", LootTableArgument::loot_table(build)).executes(
                        move |c| {
                            drop_chest_loot(
                                c,
                                &LootTableArgument::get_loot_table(c, "loot_table"),
                                &loot_output,
                            )
                        },
                    ),
                ))
                .then(
                    literal("kill").then(argument("target", EntityArgument::entity()).executes(
                        move |c| {
                            drop_kill_loot(
                                c,
                                EntityArgument::get_entity(c, "target")?,
                                &kill_output,
                            )
                        },
                    )),
                )
                .then(literal("mine").then(tool_nodes(
                    argument("pos", BlockPosArgument::block_pos()).executes({
                        let f = mine_run(Tool::Empty);
                        move |c| f(c)
                    }),
                    &mine_run,
                )))
        },
    ));
});

fn resolve_tool(
    c: &CommandContext<CommandSourceStack>,
    tool: Tool,
) -> Result<ItemStack, CommandSyntaxException> {
    match tool {
        Tool::Empty => Ok(ItemStack::empty()),
        Tool::Item => ItemArgument::get_item(c, "tool").create_item_stack(1),
        Tool::Hand(slot) => get_source_hand_item(c.get_source(), slot),
    }
}

/// `addTargets(root, tail)`.
fn add_targets<T: ArgumentBuilder<CommandSourceStack>>(root: T, tail: TailProvider<'_>) -> T {
    root.then(
        literal("replace")
            .then(
                literal("entity").then(
                    argument("entities", EntityArgument::entities()).then(
                        tail(
                            argument("slot", SlotArgument::slot()),
                            Arc::new(|c, drops, used| {
                                entity_replace(
                                    c.get_source(),
                                    &EntityArgument::get_entities(c, "entities")?,
                                    SlotArgument::get_slot(c, "slot"),
                                    drops.len() as i32,
                                    drops,
                                    used,
                                );
                                Ok(())
                            }),
                        )
                        .then(tail(
                            argument("count", integer_min(0)),
                            Arc::new(|c, drops, used| {
                                entity_replace(
                                    c.get_source(),
                                    &EntityArgument::get_entities(c, "entities")?,
                                    SlotArgument::get_slot(c, "slot"),
                                    IntegerArgumentType::get_integer(c, "count"),
                                    drops,
                                    used,
                                );
                                Ok(())
                            }),
                        )),
                    ),
                ),
            )
            .then(
                literal("block").then(
                    argument("targetPos", BlockPosArgument::block_pos()).then(
                        tail(
                            argument("slot", SlotArgument::slot()),
                            Arc::new(|c, drops, used| {
                                block_replace(
                                    c.get_source(),
                                    BlockPosArgument::get_loaded_block_pos(c, "targetPos")?,
                                    SlotArgument::get_slot(c, "slot"),
                                    drops.len() as i32,
                                    drops,
                                    used,
                                )
                            }),
                        )
                        .then(tail(
                            argument("count", integer_min(0)),
                            Arc::new(|c, drops, used| {
                                block_replace(
                                    c.get_source(),
                                    BlockPosArgument::get_loaded_block_pos(c, "targetPos")?,
                                    IntegerArgumentType::get_integer(c, "slot"),
                                    IntegerArgumentType::get_integer(c, "count"),
                                    drops,
                                    used,
                                )
                            }),
                        )),
                    ),
                ),
            ),
    )
    .then(literal("insert").then(tail(
        argument("targetPos", BlockPosArgument::block_pos()),
        Arc::new(|c, drops, used| {
            block_distribute(
                c.get_source(),
                BlockPosArgument::get_loaded_block_pos(c, "targetPos")?,
                drops,
                used,
            )
        }),
    )))
    .then(literal("give").then(tail(
        argument("players", EntityArgument::players()),
        Arc::new(|c, drops, used| {
            player_give(
                c.get_source(),
                &EntityArgument::get_players(c, "players")?,
                drops,
                used,
            );
            Ok(())
        }),
    )))
    .then(literal("spawn").then(tail(
        argument("targetPos", Vec3Argument::vec3()),
        Arc::new(|c, drops, used| {
            drop_in_world(
                c.get_source(),
                Vec3Argument::get_vec3(c, "targetPos"),
                drops,
                used,
            );
            Ok(())
        }),
    )))
}

/// `blockDistribute(source, pos, drops, usedItems)`.
fn block_distribute(
    source: &CommandSourceStack,
    pos: BlockPos,
    drops: &[ItemStack],
    used: &mut Tracker,
) -> Result<(), CommandSyntaxException> {
    source.with_server(|server| {
        let level = server
            .level_mut(source.get_level())
            .ok_or_else(|| not_a_container(pos))?;
        if container_at(level, pos).is_none() {
            return Err(not_a_container(pos));
        }
        level.with_block_entity(pos, |_, block_entity| {
            let container = block_entity.as_container_mut().expect("checked above");
            for drop in drops {
                if distribute_to_container(container, drop.copy()) {
                    container.set_changed();
                    used.track(drop, 1);
                }
            }
        });
        Ok(())
    })
}

fn not_a_container(pos: BlockPos) -> CommandSyntaxException {
    ERROR_TARGET_NOT_A_CONTAINER.create(pos.x(), pos.y(), pos.z())
}

/// `distributeToContainer(container, itemStack)`.
fn distribute_to_container(container: &mut dyn Container, mut item_stack: ItemStack) -> bool {
    let mut changed = false;
    let mut slot = 0;
    while slot < container.get_container_size() && !item_stack.is_empty() {
        if container.can_place_item(slot, &item_stack) {
            let current_empty = container.get_item(slot).is_empty();
            if current_empty {
                container.set_item(slot, item_stack.clone());
                changed = true;
                break;
            }
            if can_merge_items(container.get_item(slot), &item_stack) {
                let space = item_stack.get_max_stack_size() - container.get_item(slot).count();
                let count = item_stack.count().min(space);
                item_stack.shrink(count);
                if let Some(current) = container.get_item_mut(slot) {
                    current.grow(count);
                }
                changed = true;
            }
        }
        slot += 1;
    }
    changed
}

/// `canMergeItems(a, b)`.
fn can_merge_items(a: &ItemStack, b: &ItemStack) -> bool {
    a.count() <= a.get_max_stack_size() && ItemStack::is_same_item_same_components(a, b)
}

/// `blockReplace(source, pos, startSlot, slotCount, drops, usedItems)`.
fn block_replace(
    source: &CommandSourceStack,
    pos: BlockPos,
    start_slot: i32,
    slot_count: i32,
    drops: &[ItemStack],
    used: &mut Tracker,
) -> Result<(), CommandSyntaxException> {
    source.with_server(|server| {
        let level = server
            .level_mut(source.get_level())
            .ok_or_else(|| not_a_container(pos))?;
        let max_slot = container_at(level, pos)
            .ok_or_else(|| not_a_container(pos))?
            .get_container_size() as i32;
        if start_slot < 0 || start_slot >= max_slot {
            return Err(ERROR_TARGET_INAPPLICABLE_SLOT.create(start_slot.to_string()));
        }
        let mut writes = Vec::new();
        {
            let container = container_at(level, pos).expect("checked above");
            for i in 0..slot_count {
                let slot = start_slot + i;
                let to_add = drops.get(i as usize).cloned().unwrap_or_default();
                // vanilla indexes past the container end (an `IndexOutOfBoundsException` fails the command); slots beyond it are skipped
                if slot < max_slot && container.can_place_item(slot as usize, &to_add) {
                    writes.push((slot as usize, to_add.clone()));
                    used.track(&to_add, 1);
                }
            }
        }
        apply_container_writes(level, pos, writes);
        Ok(())
    })
}

/// `playerGive(players, drops, usedItems)`.
fn player_give(
    source: &CommandSourceStack,
    players: &[EntityRef],
    drops: &[ItemStack],
    used: &mut Tracker,
) {
    for drop in drops {
        for player in players {
            let added = source.with_server(|server| {
                server
                    .level_mut(&player.dimension)
                    .is_some_and(|level| give_to_inventory(level, player.id, drop.copy()))
            });
            if added {
                used.track(drop, 1);
            }
        }
    }
}

/// `entityReplace(entities, startSlot, count, drops, usedItems)` with `setSlots`.
fn entity_replace(
    source: &CommandSourceStack,
    entities: &[EntityRef],
    start_slot: i32,
    count: i32,
    drops: &[ItemStack],
    used: &mut Tracker,
) {
    source.with_server(|server| {
        for entity in entities {
            let writes = {
                let Some(e) = entity_in(server, entity) else {
                    continue;
                };
                let provider = EntitySlots::new(e);
                for i in 0..count {
                    let item = drops.get(i as usize).cloned().unwrap_or_default();
                    if let Some(slot) = provider.get_slot(start_slot + i)
                        && slot.set(item.copy())
                    {
                        used.track(&item, 1);
                    }
                }
                provider.into_writes()
            };
            let Some(level) = server.level_mut(&entity.dimension) else {
                continue;
            };
            apply_entity_writes(level, entity.id, writes);
            // `entity instanceof ServerPlayer`: `containerMenu.broadcastChanges()`
            level.with_player(entity.id, |level, player| {
                player.broadcast_menu_changes(level)
            });
        }
    });
}

/// `dropInWorld(source, pos, drops, usedItems)`.
fn drop_in_world(source: &CommandSourceStack, pos: Vec3, drops: &[ItemStack], used: &mut Tracker) {
    source.with_server(|server| {
        let Some(level) = server.level_mut(source.get_level()) else {
            return;
        };
        for drop in drops {
            let mut entity = ItemEntity::with_stack(level, pos.x, pos.y, pos.z, drop.copy());
            entity.set_default_pick_up_delay();
            level.add_fresh_entity(Box::new(entity));
            used.track(drop, 1);
        }
    });
}

/// `getSourceHandItem(source, slot)`.
fn get_source_hand_item(
    source: &CommandSourceStack,
    slot: EquipmentSlot,
) -> Result<ItemStack, CommandSyntaxException> {
    let entity = source.get_entity_or_exception()?;
    source.with_server(|server| {
        let live = server
            .level(source.get_level())
            .and_then(|l| l.get_entity(entity.id));
        match live {
            Some(e) if e.living().is_some() => Ok(e.get_item_by_slot(slot)),
            _ => Err(ERROR_NO_HELD_ITEMS.create(entity.display_name.clone())),
        }
    })
}

/// `RESPONSE_WITH_LOOT_TABLE` feedback (not broadcast).
fn send_with_table(
    source: &CommandSourceStack,
    used: &Tracker,
    key: &Identifier,
) -> Result<i32, CommandSyntaxException> {
    used.send_feedback(
        source,
        false,
        ElementType::NonZero,
        None::<fn() -> CommandSyntaxException>,
        |drop, _| {
            translatable!(
                "commands.drop.success.single_with_table",
                drop.count(),
                drop.get_display_name(),
                Component::literal(key.to_string())
            )
        },
        |drop_count, _| {
            translatable!(
                "commands.drop.success.multiple_with_table",
                drop_count,
                Component::literal(key.to_string())
            )
        },
    )
}

/// `RESPONSE_WITHOUT_LOOT_TABLE` feedback (not broadcast).
fn send_without_table(
    source: &CommandSourceStack,
    used: &Tracker,
) -> Result<i32, CommandSyntaxException> {
    used.send_feedback(
        source,
        false,
        ElementType::NonZero,
        None::<fn() -> CommandSyntaxException>,
        |drop, _| {
            translatable!(
                "commands.drop.success.single",
                drop.count(),
                drop.get_display_name()
            )
        },
        |drop_count, _| translatable!("commands.drop.success.multiple", drop_count),
    )
}

/// `dropBlockLoot(context, pos, tool, output)`.
fn drop_block_loot(
    c: &CommandContext<CommandSourceStack>,
    pos: BlockPos,
    tool: ItemStack,
    output: &DropConsumer,
) -> Result<i32, CommandSyntaxException> {
    let source = c.get_source();
    let (key, drops) = source.with_server(|server| {
        let level = server.level(source.get_level()).expect("source level");
        let state = level.get_block_state(pos);
        let block_entity = level.get_block_entity(pos);
        let Some(key) = state
            .block()
            .data()
            .loot_table
            .and_then(Identifier::try_parse)
        else {
            return Err(ERROR_NO_BLOCK_LOOT_TABLE.create(Component::translatable(
                state.block().data().description_id,
                vec![],
            )));
        };
        let this_entity = source.get_entity().and_then(|e| level.get_entity(e.id));
        let builder = LootParams::builder(level)
            .origin(Vec3::at_center_of(&pos))
            .block_entity(block_entity)
            .optional_this_entity(this_entity)
            .tool(&tool);
        Ok((key, state_get_drops(state, builder)))
    })?;
    let mut used = Tracker::create();
    output(c, &drops, &mut used)?;
    send_with_table(source, &used, &key)
}

/// `dropKillLoot(context, target, output)`.
fn drop_kill_loot(
    c: &CommandContext<CommandSourceStack>,
    target: EntityRef,
    output: &DropConsumer,
) -> Result<i32, CommandSyntaxException> {
    let source = c.get_source();
    let (key, drops) = source.with_server(|server| {
        let Some(level) = server.level(&target.dimension) else {
            return Err(ERROR_NO_ENTITY_LOOT_TABLE.create(Component::empty()));
        };
        let Some(entity) = level.get_entity(target.id) else {
            return Err(ERROR_NO_ENTITY_LOOT_TABLE.create(Component::empty()));
        };
        let Some(key) = entity.get_loot_table() else {
            return Err(
                ERROR_NO_ENTITY_LOOT_TABLE.create(host::entity_display_name(server, entity))
            );
        };
        let killer = source.get_entity().and_then(|e| level.get_entity(e.id));
        let magic = level.damage_sources().magic();
        let params = LootParams::builder(level)
            .last_damage_player(killer.filter(|k| k.is_player()))
            .damage_source(&magic)
            .direct_attacking_entity(killer)
            .attacking_entity(killer)
            .this_entity(entity)
            .origin(source.get_position())
            .create(&LootContextParamSets::ENTITY);
        let drops = get_loot_table(&level.registries, &key).get_random_items(&params);
        Ok((key, drops))
    })?;
    let mut used = Tracker::create();
    output(c, &drops, &mut used)?;
    send_with_table(source, &used, &key)
}

/// `dropChestLoot(context, lootTable, output)`.
fn drop_chest_loot(
    c: &CommandContext<CommandSourceStack>,
    loot_table: &ResourceOrId,
    output: &DropConsumer,
) -> Result<i32, CommandSyntaxException> {
    let source = c.get_source();
    let table = resolve_resource_or_id(
        source,
        keys::LOOT_TABLE,
        loot_table,
        loot_table_direct_codec(),
    )?;
    let drops = source.with_server(|server| {
        let level = server.level(source.get_level()).expect("source level");
        let this_entity = source.get_entity().and_then(|e| level.get_entity(e.id));
        let params = LootParams::builder(level)
            .optional_this_entity(this_entity)
            .origin(source.get_position())
            .create(&LootContextParamSets::CHEST);
        table.get_random_items(&params)
    });
    drop_items(c, drops, output)
}

/// `dropFishingLoot(context, lootTable, pos, tool, output)`.
fn drop_fishing_loot(
    c: &CommandContext<CommandSourceStack>,
    loot_table: &ResourceOrId,
    pos: BlockPos,
    tool: ItemStack,
    output: &DropConsumer,
) -> Result<i32, CommandSyntaxException> {
    let source = c.get_source();
    let table = resolve_resource_or_id(
        source,
        keys::LOOT_TABLE,
        loot_table,
        loot_table_direct_codec(),
    )?;
    let drops = source.with_server(|server| {
        let level = server.level(source.get_level()).expect("source level");
        let this_entity = source.get_entity().and_then(|e| level.get_entity(e.id));
        let params = LootParams::builder(level)
            .origin(Vec3::at_center_of(&pos))
            .tool(&tool)
            .optional_this_entity(this_entity)
            .create(&LootContextParamSets::FISHING);
        table.get_random_items(&params)
    });
    drop_items(c, drops, output)
}

/// The tail of `drop(context, lootTable, lootParams, output)`.
fn drop_items(
    c: &CommandContext<CommandSourceStack>,
    drops: Vec<ItemStack>,
    output: &DropConsumer,
) -> Result<i32, CommandSyntaxException> {
    let mut used = Tracker::create();
    output(c, &drops, &mut used)?;
    send_without_table(c.get_source(), &used)
}
