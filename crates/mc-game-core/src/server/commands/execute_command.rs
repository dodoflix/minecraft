//! `ExecuteCommand`: `/execute` and its `if`/`unless`/`store` families.
use super::arg_provider::ArgProvider;
use super::data::DataAccessor;
use super::data_commands::{Accessor, SOURCE_PROVIDERS, TARGET_PROVIDERS};
use super::item::item_accessor::{ItemAccessor, slot_source_value};
use crate::commands::arguments::coordinates::block_pos_argument::ERROR_NOT_LOADED;
use crate::commands::arguments::coordinates::{BlockPosArgument, RotationArgument, SwizzleArgument, Vec3Argument};
use crate::commands::arguments::entity_anchor_argument::{Anchor, EntityAnchorArgument};
use crate::commands::arguments::function_argument::FunctionArgument;
use crate::commands::arguments::resource_arguments::{LootPredicateArgument, ResourceArgument, ResourceOrId, ResourceOrTagArgument};
use crate::commands::arguments::scores::ScoreHolder;
use crate::commands::arguments::selector::EntityRef;
use crate::commands::arguments::{
    DimensionArgument, EntityArgument, FloatRangeArgument, HeightmapTypeArgument, IdentifierArgument, IntRangeArgument, NbtPath, NbtPathArgument,
    ObjectiveArgument, ScoreHolderArgument,
};
use crate::commands::arguments::blocks::BlockPredicateArgument;
use crate::commands::arguments::item::ItemPredicateArgument;
use crate::commands::arguments::slot_arguments::SlotSourceArgument;
use crate::commands::execution::tasks::build_contexts::BuildContexts;
use crate::commands::execution::tasks::call_function::CallFunction;
use crate::commands::execution::tasks::fallthrough_task;
use crate::commands::execution::tasks::isolated_call::isolated_call;
use crate::commands::execution::{ChainModifiers, CustomModifierExecutor, EntryAction, ExecutionControl, bind, custom_modifier};
use crate::commands::functions::{CommandFunction, FunctionInstantiationException, InstantiatedFunction};
use crate::commands::host::{self, EntityRelation};
use crate::commands::message::{message, simple_translatable};
use crate::commands::{CommandResultCallback, CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, literal};
use crate::server::MinecraftServer;
use crate::{register_command, translatable};
use mc_brigadier::arguments::{DoubleArgumentType, double_arg};
use mc_brigadier::{
    ArgumentBuilder, CommandContext, CommandNode, CommandSyntaxException, ContextChain, Dynamic2CommandExceptionType, DynamicCommandExceptionType,
    LiteralArgumentBuilder, SimpleCommandExceptionType,
};
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::bounding_box::BoundingBox;
use mc_core::pos::BlockPos;
use mc_data::block::Block;
use mc_nbt::{CompoundTag, Tag};
use mc_text::Component;
use std::sync::{Arc, LazyLock, Mutex};

/// `MAX_TEST_AREA`.
const MAX_TEST_AREA: i64 = 32768;

static ERROR_AREA_TOO_LARGE: LazyLock<Dynamic2CommandExceptionType<i32, i64>> = LazyLock::new(|| {
    Dynamic2CommandExceptionType::new(|max: i32, count: i64| message(translatable!("commands.execute.blocks.toobig", max, count)))
});
static ERROR_CONDITIONAL_FAILED: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.execute.conditional.fail"));
static ERROR_CONDITIONAL_FAILED_COUNT: LazyLock<DynamicCommandExceptionType<i32>> =
    LazyLock::new(|| DynamicCommandExceptionType::new(|count: i32| message(translatable!("commands.execute.conditional.fail_count", count))));
/// `ERROR_FUNCTION_CONDITION_INSTANTATION_FAILURE`.
pub static ERROR_FUNCTION_CONDITION_INSTANTATION_FAILURE: LazyLock<Dynamic2CommandExceptionType<String, Component>> = LazyLock::new(|| {
    Dynamic2CommandExceptionType::new(|id: String, reason: Component| message(translatable!("commands.execute.function.instantiationFailure", id, reason)))
});

type Sources = Result<Vec<CommandSourceStack>, CommandSyntaxException>;

register_command!("ExecuteCommand", |ctx| {
    let execute = ctx.dispatcher.register(literal("execute").requires(has_permission(LEVEL_GAMEMASTERS)));
    let root = ctx.dispatcher.get_root().clone();
    ctx.dispatcher.register(
        literal("execute")
            .requires(has_permission(LEVEL_GAMEMASTERS))
            .then(literal("run").redirect(root))
            .then(add_conditionals(&execute, literal("if"), true, ctx.build_context))
            .then(add_conditionals(&execute, literal("unless"), false, ctx.build_context))
            .then(literal("as").then(argument("targets", EntityArgument::entities()).fork(execute.clone(), |c| {
                map_targets(c, |server, source, entity| Some(source.with_entity(host::command_entity(server, entity)?)))
            })))
            .then(literal("at").then(argument("targets", EntityArgument::entities()).fork(execute.clone(), |c| {
                map_targets(c, |server, source, entity| {
                    let scale = host::teleportation_scale(server, source.get_level(), &entity.dimension);
                    Some(
                        source
                            .with_level(entity.dimension.clone(), scale)
                            .with_position(host::entity_position(server, entity)?)
                            .with_rotation(host::entity_rotation_vector(server, entity)?),
                    )
                })
            })))
            .then(
                literal("store")
                    .then(wrap_stores(&execute, literal("result"), true, ctx.build_context))
                    .then(wrap_stores(&execute, literal("success"), false, ctx.build_context)),
            )
            .then(
                literal("positioned")
                    .then(
                        argument("pos", Vec3Argument::vec3())
                            .redirect_with(execute.clone(), |c| Ok(c.get_source().with_position(Vec3Argument::get_vec3(c, "pos")).with_anchor(Anchor::Feet))),
                    )
                    .then(literal("as").then(argument("targets", EntityArgument::entities()).fork(execute.clone(), |c| {
                        map_targets(c, |server, source, entity| Some(source.with_position(host::entity_position(server, entity)?)))
                    })))
                    .then(literal("over").then(argument("heightmap", HeightmapTypeArgument).redirect_with(execute.clone(), |c| {
                        let source = c.get_source();
                        let position = source.get_position();
                        let (x, z) = (position.x(), position.z());
                        let height = source.with_server(|server| {
                            use crate::world::level::LevelReader;
                            let level = server.level(source.get_level())?;
                            if !level.has_chunk(mth::floor_f64(x) >> 4, mth::floor_f64(z) >> 4) {
                                return Some(None);
                            }
                            Some(Some(level.get_height_at(HeightmapTypeArgument::get(c, "heightmap"), mth::floor_f64(x), mth::floor_f64(z))))
                        });
                        match height.flatten() {
                            Some(height) => Ok(source.with_position(Vec3::new(x, f64::from(height), z))),
                            None => Err(ERROR_NOT_LOADED.create()),
                        }
                    }))),
            )
            .then(
                literal("rotated")
                    .then(argument("rot", RotationArgument::rotation()).redirect_with(execute.clone(), |c| {
                        Ok(c.get_source().with_rotation(RotationArgument::get_rotation(c, "rot").get_rotation(c.get_source())))
                    }))
                    .then(literal("as").then(argument("targets", EntityArgument::entities()).fork(execute.clone(), |c| {
                        map_targets(c, |server, source, entity| Some(source.with_rotation(host::entity_rotation_vector(server, entity)?)))
                    }))),
            )
            .then(
                literal("facing")
                    .then(literal("entity").then(argument("targets", EntityArgument::entities()).then(
                        argument("anchor", EntityAnchorArgument::anchor()).fork(execute.clone(), |c| {
                            let anchor = EntityAnchorArgument::get_anchor(c, "anchor");
                            map_targets(c, |server, source, entity| {
                                let target = host::get_entity(server, entity)?;
                                Some(source.facing(anchor.apply_entity(target.position(), target.base().eye_height)))
                            })
                        }),
                    )))
                    .then(argument("pos", Vec3Argument::vec3()).redirect_with(execute.clone(), |c| Ok(c.get_source().facing(Vec3Argument::get_vec3(c, "pos"))))),
            )
            .then(literal("align").then(argument("axes", SwizzleArgument::swizzle()).redirect_with(execute.clone(), |c| {
                let axes: Vec<_> = SwizzleArgument::get_swizzle(c, "axes").into_iter().collect();
                Ok(c.get_source().with_position(c.get_source().get_position().align(&axes)))
            })))
            .then(literal("anchored").then(
                argument("anchor", EntityAnchorArgument::anchor())
                    .redirect_with(execute.clone(), |c| Ok(c.get_source().with_anchor(EntityAnchorArgument::get_anchor(c, "anchor")))),
            ))
            .then(literal("in").then(argument("dimension", DimensionArgument::dimension()).redirect_with(execute.clone(), |c| {
                let level = DimensionArgument::get_dimension(c, "dimension")?;
                let source = c.get_source();
                let scale = source.with_server(|server| host::teleportation_scale(server, source.get_level(), &level));
                Ok(source.with_level(level, scale))
            })))
            .then(
                literal("summon").then(
                    argument("entity", ResourceArgument::resource(ctx.build_context, "entity_type"))
                        .suggests_provider(Some(crate::commands::synchronization::suggestion_providers::summonable_entities()))
                        .redirect_with(execute.clone(), |c| {
                            spawn_entity_and_redirect(c.get_source(), ResourceArgument::get_summonable_entity_type(c, "entity")?)
                        }),
                ),
            )
            .then(create_relation_operations(&execute, literal("on"))),
    );
});

/// Runs `f` for every entity of the `targets` argument (`getOptionalEntities`) under one server borrow.
fn map_targets(
    c: &CommandContext<CommandSourceStack>,
    f: impl Fn(&MinecraftServer, &CommandSourceStack, &EntityRef) -> Option<CommandSourceStack>,
) -> Sources {
    let entities = EntityArgument::get_optional_entities(c, "targets")?;
    let source = c.get_source();
    Ok(source.with_server(|server| entities.iter().filter_map(|entity| f(server, source, entity)).collect()))
}

/// `wrapStores`.
fn wrap_stores(
    execute: &Arc<CommandNode<CommandSourceStack>>,
    literal_node: LiteralArgumentBuilder<CommandSourceStack>,
    store_result: bool,
    _build_context: &crate::commands::CommandBuildContext,
) -> LiteralArgumentBuilder<CommandSourceStack> {
    let mut literal_node = literal_node.then(literal("score").then(
        argument("targets", ScoreHolderArgument::score_holders()).suggests_provider(Some(crate::commands::arguments::scores::suggest_score_holders())).then(
            argument("objective", ObjectiveArgument::objective()).redirect_with(execute.clone(), move |c| {
                Ok(store_score(
                    c.get_source(),
                    ScoreHolderArgument::get_names_with_default_wildcard(c, "targets")?,
                    ObjectiveArgument::get_objective(c, "objective")?,
                    store_result,
                ))
            }),
        ),
    ));
    literal_node = literal_node.then(literal("bossbar").then(
        argument("id", IdentifierArgument::id())
            .suggests_provider(Some(super::boss_bar_commands::suggest_boss_bar()))
            .then(literal("value").redirect_with(execute.clone(), move |c| {
                Ok(store_boss_bar(c.get_source(), super::boss_bar_commands::get_boss_bar(c)?, true, store_result))
            }))
            .then(literal("max").redirect_with(execute.clone(), move |c| {
                Ok(store_boss_bar(c.get_source(), super::boss_bar_commands::get_boss_bar(c)?, false, store_result))
            })),
    ));
    for provider in TARGET_PROVIDERS.iter() {
        literal_node = provider.wrap(literal_node, |p| {
            p.then(
                argument("path", NbtPathArgument::nbt_path())
                    .then(store_data_node("int", provider, execute, store_result, |v, s| Tag::Int((f64::from(v) * s) as i32)))
                    .then(store_data_node("float", provider, execute, store_result, |v, s| Tag::Float((f64::from(v) * s) as f32)))
                    // Java `(short)(double)` narrows through `int`.
                    .then(store_data_node("short", provider, execute, store_result, |v, s| Tag::Short(((f64::from(v) * s) as i32) as i16)))
                    .then(store_data_node("long", provider, execute, store_result, |v, s| Tag::Long((f64::from(v) * s) as i64)))
                    .then(store_data_node("double", provider, execute, store_result, |v, s| Tag::Double(f64::from(v) * s)))
                    .then(store_data_node("byte", provider, execute, store_result, |v, s| Tag::Byte(((f64::from(v) * s) as i32) as i8))),
            )
        });
    }
    literal_node
}

/// `literal(<type>).then(argument("scale", doubleArg()).redirect(execute, c -> storeData(...)))`.
fn store_data_node(
    type_name: &'static str,
    provider: &ArgProvider<Accessor>,
    execute: &Arc<CommandNode<CommandSourceStack>>,
    store_result: bool,
    constructor: fn(i32, f64) -> Tag,
) -> LiteralArgumentBuilder<CommandSourceStack> {
    let provider = provider.clone();
    literal(type_name).then(argument("scale", double_arg()).redirect_with(execute.clone(), move |c| {
        let scale = DoubleArgumentType::get_double(c, "scale");
        Ok(store_data(c.get_source(), provider.access(c)?, NbtPathArgument::get_path(c, "path"), move |v| constructor(v, scale), store_result))
    }))
}

/// The value a `store` writes: `result`, or `1`/`0` for `success`.
fn stored_value(store_result: bool, success: bool, result: i32) -> i32 {
    if store_result { result } else { i32::from(success) }
}

/// `storeValue(source, names, objective, storeResult)`.
fn store_score(source: &CommandSourceStack, names: Vec<ScoreHolder>, objective: String, store_result: bool) -> CommandSourceStack {
    let callback_source = source.clone();
    source.with_callback_combined(
        CommandResultCallback::new(move |success, result| {
            let value = stored_value(store_result, success, result);
            callback_source.with_server(|server| {
                for name in &names {
                    // `getOrCreatePlayerScore` + `set`: a read-only criterion is vanilla's `IllegalStateException`.
                    if let Some(mut score) = server.scoreboard.write().get_or_create_player_score(name, &objective) {
                        score.set(value).unwrap_or_else(|e| panic!("{e}"));
                    }
                }
                server.flush_scoreboard();
            });
        }),
        CommandResultCallback::chain,
    )
}

/// `storeValue(source, bossEvent, storeIntoValue, storeResult)`.
fn store_boss_bar(source: &CommandSourceStack, event: mc_core::ident::Identifier, store_into_value: bool, store_result: bool) -> CommandSourceStack {
    let callback_source = source.clone();
    source.with_callback_combined(
        CommandResultCallback::new(move |success, result| {
            let value = stored_value(store_result, success, result);
            callback_source.with_server(|server| {
                if let Some(event) = server.custom_boss_events().get_mut(&event) {
                    if store_into_value { event.set_value(value) } else { event.set_max(value) }
                }
            });
        }),
        CommandResultCallback::chain,
    )
}

/// `storeData(source, accessor, path, constructor, storeResult)`: failures are swallowed like vanilla's empty catch.
fn store_data(
    source: &CommandSourceStack,
    accessor: Box<dyn DataAccessor>,
    path: NbtPath,
    constructor: impl Fn(i32) -> Tag + Send + Sync + 'static,
    store_result: bool,
) -> CommandSourceStack {
    let callback_source = source.clone();
    source.with_callback_combined(
        CommandResultCallback::new(move |success, result| {
            let run = || -> Result<(), CommandSyntaxException> {
                let data = accessor.get_data(&callback_source)?;
                let mut root = Tag::Compound(data);
                path.set(&mut root, &constructor(stored_value(store_result, success, result)))?;
                let Tag::Compound(data) = root else { unreachable!("set keeps the root a compound") };
                accessor.set_data(&callback_source, data)
            };
            let _ = run();
        }),
        CommandResultCallback::chain,
    )
}

/// `addConditionals`.
fn add_conditionals(
    execute: &Arc<CommandNode<CommandSourceStack>>,
    parent: LiteralArgumentBuilder<CommandSourceStack>,
    expected: bool,
    context: &crate::commands::CommandBuildContext,
) -> LiteralArgumentBuilder<CommandSourceStack> {
    let mut parent = parent
        .then(literal("block").then(argument("pos", BlockPosArgument::block_pos()).then(add_conditional(
            execute,
            argument("block", BlockPredicateArgument::block_predicate(context)),
            expected,
            |c| {
                let source = c.get_source();
                let pos = BlockPosArgument::get_loaded_block_pos(c, "pos")?;
                let predicate = BlockPredicateArgument::get_block_predicate(c, "block");
                Ok(source.with_server(|server| server.level(source.get_level()).is_some_and(|level| predicate.test(level, pos))))
            },
        ))))
        .then(literal("biome").then(argument("pos", BlockPosArgument::block_pos()).then(add_conditional(
            execute,
            argument("biome", ResourceOrTagArgument::resource_or_tag(context, "worldgen/biome")),
            expected,
            |c| {
                let source = c.get_source();
                let pos = BlockPosArgument::get_loaded_block_pos(c, "pos")?;
                let biome = ResourceOrTagArgument::get_resource_or_tag(c, "biome", "worldgen/biome")?;
                Ok(source
                    .with_server(|server| host::biome_key_at(server, source.get_level(), pos))
                    .is_some_and(|key| biome.test(&key)))
            },
        ))))
        .then(literal("loaded").then(add_conditional(execute, argument("pos", BlockPosArgument::block_pos()), expected, |c| {
            let source = c.get_source();
            let pos = BlockPosArgument::get_block_pos(c, "pos");
            Ok(source.with_server(|server| host::is_chunk_entity_ticking(server, source.get_level(), pos)))
        })))
        .then(literal("dimension").then(add_conditional(execute, argument("dimension", DimensionArgument::dimension()), expected, |c| {
            Ok(&DimensionArgument::get_dimension(c, "dimension")? == c.get_source().get_level())
        })))
        .then(literal("score").then(
            argument("target", ScoreHolderArgument::score_holder())
                .suggests_provider(Some(crate::commands::arguments::scores::suggest_score_holders()))
                .then(
                    argument("targetObjective", ObjectiveArgument::objective())
                        .then(score_comparison("=", execute, expected, |a, b| a == b))
                        .then(score_comparison("<", execute, expected, |a, b| a < b))
                        .then(score_comparison("<=", execute, expected, |a, b| a <= b))
                        .then(score_comparison(">", execute, expected, |a, b| a > b))
                        .then(score_comparison(">=", execute, expected, |a, b| a >= b))
                        .then(literal("matches").then(add_conditional(execute, argument("range", IntRangeArgument::int_range()), expected, |c| {
                            check_score_range(c)
                        }))),
                ),
        ))
        .then(literal("blocks").then(argument("start", BlockPosArgument::block_pos()).then(
            argument("end", BlockPosArgument::block_pos()).then(
                argument("destination", BlockPosArgument::block_pos())
                    .then(add_if_blocks_conditional(execute, literal("all"), expected, false))
                    .then(add_if_blocks_conditional(execute, literal("masked"), expected, true)),
            ),
        )))
        .then(literal("entity").then(
            argument("entities", EntityArgument::entities())
                .fork(execute.clone(), move |c| expect(c, expected, !EntityArgument::get_optional_entities(c, "entities")?.is_empty()))
                .executes(numeric_conditional_handler(expected, |c| Ok(EntityArgument::get_optional_entities(c, "entities")?.len() as i32))),
        ))
        .then(literal("predicate").then(add_conditional(
            execute,
            argument("predicate", LootPredicateArgument::loot_predicate(context)),
            expected,
            |c| Ok(check_custom_predicate(c.get_source(), &LootPredicateArgument::get_loot_predicate(c, "predicate"))),
        )))
        .then(literal("function").then(
            argument("name", FunctionArgument::functions())
                .suggests(super::function_command::suggest_function)
                .forward(execute.clone(), Some(custom_modifier(IfFunctionModifier { expected })), true),
        ))
        .then(literal("stopwatch").then(
            argument("id", IdentifierArgument::id()).suggests_provider(Some(super::stopwatch_command::suggest_stopwatches())).then(add_conditional(
                execute,
                argument("range", FloatRangeArgument::float_range()),
                expected,
                check_stopwatch,
            )),
        ));
    for provider in SOURCE_PROVIDERS.iter() {
        let (fork_provider, run_provider) = (provider.clone(), provider.clone());
        parent = parent.then(provider.wrap(literal("data"), |p| {
            p.then(
                argument("path", NbtPathArgument::nbt_path())
                    .fork(execute.clone(), move |c| {
                        expect(c, expected, check_matching_data(c.get_source(), &*fork_provider.access(c)?, &NbtPathArgument::get_path(c, "path"))? > 0)
                    })
                    .executes(numeric_conditional_handler(expected, move |c| {
                        check_matching_data(c.get_source(), &*run_provider.access(c)?, &NbtPathArgument::get_path(c, "path"))
                    })),
            )
        }));
    }
    for provider in super::item_commands::SOURCE_PROVIDERS.iter() {
        let (items_fork, items_run, slots_fork, slots_run) = (provider.clone(), provider.clone(), provider.clone(), provider.clone());
        let context = context.clone();
        parent = parent
            .then(provider.wrap(literal("items"), |p| {
                p.then(argument("slots", SlotSourceArgument::slot_source(&context)).then(
                    argument("item_predicate", ItemPredicateArgument::item_predicate(&context))
                        .fork(execute.clone(), move |c| {
                            expect(c, expected, count_items(c, &*items_fork.access(c)?)? > 0)
                        })
                        .executes(numeric_conditional_handler(expected, move |c| count_items(c, &*items_run.access(c)?))),
                ))
            }))
            .then(provider.wrap(literal("slots"), |p| {
                p.then(
                    argument("slots", SlotSourceArgument::slot_source(&context))
                        .fork(execute.clone(), move |c| expect(c, expected, count_slots(c, &*slots_fork.access(c)?)? > 0))
                        .executes(numeric_conditional_handler(expected, move |c| count_slots(c, &*slots_run.access(c)?))),
                )
            }));
    }
    parent
}

/// `literal(op).then(argument("source", scoreHolder()).suggests(..).then(addConditional(.., argument("sourceObjective", ..), ..)))`.
fn score_comparison(
    operator: &'static str,
    execute: &Arc<CommandNode<CommandSourceStack>>,
    expected: bool,
    operation: fn(i32, i32) -> bool,
) -> LiteralArgumentBuilder<CommandSourceStack> {
    literal(operator).then(
        argument("source", ScoreHolderArgument::score_holder())
            .suggests_provider(Some(crate::commands::arguments::scores::suggest_score_holders()))
            .then(add_conditional(execute, argument("sourceObjective", ObjectiveArgument::objective()), expected, move |c| check_score(c, operation))),
    )
}

/// `countItems(source, accessor, slotSource, predicate)`.
fn count_items(c: &CommandContext<CommandSourceStack>, accessor: &dyn ItemAccessor) -> Result<i32, CommandSyntaxException> {
    let slot_source = slot_source_value(c.get_source(), &SlotSourceArgument::get_slot_source(c, "slots"))?;
    let predicate = ItemPredicateArgument::get_item_predicate(c, "item_predicate");
    let slots = accessor.get_slots(c.get_source(), &slot_source)?;
    Ok(slots.item_copies().iter().filter(|stack| predicate.test(stack)).map(|stack| stack.count()).fold(0i32, i32::wrapping_add))
}

/// `countSlots(source, accessor, slotSource)`.
fn count_slots(c: &CommandContext<CommandSourceStack>, accessor: &dyn ItemAccessor) -> Result<i32, CommandSyntaxException> {
    let slot_source = slot_source_value(c.get_source(), &SlotSourceArgument::get_slot_source(c, "slots"))?;
    Ok(accessor.get_slots(c.get_source(), &slot_source)?.size())
}

/// `createNumericConditionalHandler(expected, condition)`.
fn numeric_conditional_handler(
    expected: bool,
    condition: impl Fn(&CommandContext<CommandSourceStack>) -> Result<i32, CommandSyntaxException> + Send + Sync + 'static,
) -> impl Fn(&CommandContext<CommandSourceStack>) -> Result<i32, CommandSyntaxException> + Send + Sync + 'static {
    move |c| {
        let count = condition(c)?;
        if expected {
            if count > 0 {
                c.get_source().send_success(|| translatable!("commands.execute.conditional.pass_count", count), false);
                Ok(count)
            } else {
                Err(ERROR_CONDITIONAL_FAILED.create())
            }
        } else if count == 0 {
            c.get_source().send_success(|| translatable!("commands.execute.conditional.pass"), false);
            Ok(1)
        } else {
            Err(ERROR_CONDITIONAL_FAILED_COUNT.create(count))
        }
    }
}

/// `checkMatchingData(accessor, path)`.
fn check_matching_data(source: &CommandSourceStack, accessor: &dyn DataAccessor, path: &NbtPath) -> Result<i32, CommandSyntaxException> {
    Ok(path.count_matching(&Tag::Compound(accessor.get_data(source)?)) as i32)
}

/// `checkScore(context, operation)`: both scores must exist.
fn check_score(c: &CommandContext<CommandSourceStack>, operation: fn(i32, i32) -> bool) -> Result<bool, CommandSyntaxException> {
    let target = ScoreHolderArgument::get_name(c, "target")?;
    let target_objective = ObjectiveArgument::get_objective(c, "targetObjective")?;
    let source = ScoreHolderArgument::get_name(c, "source")?;
    let source_objective = ObjectiveArgument::get_objective(c, "sourceObjective")?;
    Ok(c.get_source().with_server(|server| {
        let scoreboard = server.scoreboard.read();
        let a = scoreboard.get_player_score_info(&target, &target_objective).map(|s| s.value());
        let b = scoreboard.get_player_score_info(&source, &source_objective).map(|s| s.value());
        matches!((a, b), (Some(a), Some(b)) if operation(a, b))
    }))
}

/// `checkScore(context, range)`.
fn check_score_range(c: &CommandContext<CommandSourceStack>) -> Result<bool, CommandSyntaxException> {
    let target = ScoreHolderArgument::get_name(c, "target")?;
    let target_objective = ObjectiveArgument::get_objective(c, "targetObjective")?;
    let range = IntRangeArgument::get_range(c, "range");
    Ok(c.get_source().with_server(|server| {
        server.scoreboard.read().get_player_score_info(&target, &target_objective).is_some_and(|score| range.matches(score.value()))
    }))
}

/// `checkStopwatch(context, range)`.
fn check_stopwatch(c: &CommandContext<CommandSourceStack>) -> Result<bool, CommandSyntaxException> {
    use crate::world::level::saveddata::stopwatches::Stopwatches;
    let id = IdentifierArgument::get_id(c, "id");
    let range = FloatRangeArgument::get_range(c, "range");
    let elapsed_seconds = c.get_source().with_server(|server| {
        let stopwatches = server.saved_data.compute_if_absent(&server.saved_types.stopwatches);
        stopwatches.get(&id).map(|stopwatch| stopwatch.elapsed_seconds(Stopwatches::current_time()))
    });
    match elapsed_seconds {
        Some(seconds) => Ok(range.matches(seconds)),
        None => Err(super::stopwatch_command::ERROR_DOES_NOT_EXIST.create(id.to_string())),
    }
}

/// `checkCustomPredicate(source, predicate)`: the loot predicate evaluated with `ORIGIN`/`THIS_ENTITY` of the source.
fn check_custom_predicate(source: &CommandSourceStack, predicate: &ResourceOrId) -> bool {
    source.with_server(|server| {
        let entity = source.get_entity().and_then(|e| host::entity_ref_of(server, e, source.get_level()));
        host::loot_predicate_test(server, source.get_level(), source.get_position(), entity.as_ref(), predicate)
    })
}

/// `expect(context, expected, result)`.
fn expect(c: &CommandContext<CommandSourceStack>, expected: bool, result: bool) -> Sources {
    Ok(if result == expected { vec![c.get_source().clone()] } else { Vec::new() })
}

/// `addConditional(root, argument, expected, predicate)`.
fn add_conditional<B: ArgumentBuilder<CommandSourceStack>>(
    root: &Arc<CommandNode<CommandSourceStack>>,
    argument: B,
    expected: bool,
    predicate: impl Fn(&CommandContext<CommandSourceStack>) -> Result<bool, CommandSyntaxException> + Send + Sync + 'static,
) -> B {
    let predicate = Arc::new(predicate);
    let fork_predicate = predicate.clone();
    argument
        .fork(root.clone(), move |c| expect(c, expected, fork_predicate(c)?))
        .executes(move |c| {
            if expected == predicate(c)? {
                c.get_source().send_success(|| translatable!("commands.execute.conditional.pass"), false);
                Ok(1)
            } else {
                Err(ERROR_CONDITIONAL_FAILED.create())
            }
        })
}

/// `addIfBlocksConditional(root, argument, expected, skipAir)`.
fn add_if_blocks_conditional(
    root: &Arc<CommandNode<CommandSourceStack>>,
    argument: LiteralArgumentBuilder<CommandSourceStack>,
    expected: bool,
    skip_air: bool,
) -> LiteralArgumentBuilder<CommandSourceStack> {
    let command: Arc<dyn Fn(&CommandContext<CommandSourceStack>) -> Result<i32, CommandSyntaxException> + Send + Sync> =
        if expected { Arc::new(move |c| check_if_regions(c, skip_air)) } else { Arc::new(move |c| check_unless_regions(c, skip_air)) };
    argument.fork(root.clone(), move |c| expect(c, expected, check_regions_in_context(c, skip_air)?.is_some())).executes(move |c| command(c))
}

fn check_if_regions(c: &CommandContext<CommandSourceStack>, skip_air: bool) -> Result<i32, CommandSyntaxException> {
    match check_regions_in_context(c, skip_air)? {
        Some(count) => {
            c.get_source().send_success(|| translatable!("commands.execute.conditional.pass_count", count), false);
            Ok(count)
        }
        None => Err(ERROR_CONDITIONAL_FAILED.create()),
    }
}

fn check_unless_regions(c: &CommandContext<CommandSourceStack>, skip_air: bool) -> Result<i32, CommandSyntaxException> {
    match check_regions_in_context(c, skip_air)? {
        Some(count) => Err(ERROR_CONDITIONAL_FAILED_COUNT.create(count)),
        None => {
            c.get_source().send_success(|| translatable!("commands.execute.conditional.pass"), false);
            Ok(1)
        }
    }
}

/// `checkRegions(context, skipAir)`: the number of compared blocks when the two regions are equal.
fn check_regions_in_context(c: &CommandContext<CommandSourceStack>, skip_air: bool) -> Result<Option<i32>, CommandSyntaxException> {
    let start = BlockPosArgument::get_loaded_block_pos(c, "start")?;
    let end = BlockPosArgument::get_loaded_block_pos(c, "end")?;
    let destination = BlockPosArgument::get_loaded_block_pos(c, "destination")?;
    let source = c.get_source();
    source.with_server(|server| check_regions(server, source.get_level(), start, end, destination, skip_air))
}

/// `checkRegions(level, start, end, destination, skipAir)`.
pub fn check_regions(
    server: &MinecraftServer,
    dimension: &crate::world::level::DimensionKey,
    start: BlockPos,
    end: BlockPos,
    destination: BlockPos,
    skip_air: bool,
) -> Result<Option<i32>, CommandSyntaxException> {
    use crate::world::block_entity::BlockEntity;
    use crate::world::level::BlockGetter;
    let from = BoundingBox::from_corners(&start, &end);
    let destination_box = BoundingBox::from_corners(&destination, &destination.offset_vec(&from.get_length()));
    let offset = BlockPos::new(
        destination_box.min_x().wrapping_sub(from.min_x()),
        destination_box.min_y().wrapping_sub(from.min_y()),
        destination_box.min_z().wrapping_sub(from.min_z()),
    );
    let area = i64::from(from.get_x_span()) * i64::from(from.get_y_span()) * i64::from(from.get_z_span());
    if area > MAX_TEST_AREA {
        return Err(ERROR_AREA_TOO_LARGE.create(MAX_TEST_AREA as i32, area));
    }
    let Some(level) = server.level(dimension) else { return Ok(None) };
    let mut count = 0;
    for z in from.min_z()..=from.max_z() {
        for y in from.min_y()..=from.max_y() {
            for x in from.min_x()..=from.max_x() {
                let source_pos = BlockPos::new(x, y, z);
                let destination_pos = source_pos.offset_vec(&offset);
                let source_block = level.get_block_state(source_pos);
                if skip_air && source_block.block() == Block::AIR {
                    continue;
                }
                if source_block != level.get_block_state(destination_pos) {
                    return Ok(None);
                }
                if let Some(source_entity) = level.get_block_entity(source_pos) {
                    let Some(destination_entity) = level.get_block_entity(destination_pos) else { return Ok(None) };
                    if destination_entity.get_type() != source_entity.get_type()
                        || source_entity.base().components != destination_entity.base().components
                    {
                        return Ok(None);
                    }
                    let (mut source_tag, mut destination_tag) = (CompoundTag::new(), CompoundTag::new());
                    source_entity.save_additional(&mut source_tag);
                    destination_entity.save_additional(&mut destination_tag);
                    if source_tag != destination_tag {
                        return Ok(None);
                    }
                }
                count += 1;
            }
        }
    }
    Ok(Some(count))
}

/// `expandOneToOneEntityRelation` / `expandOneToManyEntityRelation`: the relation's entities for the source's entity.
fn expand_entity_relation(relation: EntityRelation) -> impl Fn(&CommandContext<CommandSourceStack>) -> Sources + Send + Sync + 'static {
    move |c| {
        let source = c.get_source();
        let Some(entity) = source.get_entity() else { return Ok(Vec::new()) };
        Ok(source.with_server(|server| {
            let Some(entity) = host::entity_ref_of(server, entity, source.get_level()) else { return Vec::new() };
            host::entity_relation(server, &entity, relation)
                .iter()
                .filter_map(|related| host::command_entity(server, related))
                .map(|related| source.with_entity(related))
                .collect()
        }))
    }
}

/// `createRelationOperations(execute, on)`.
fn create_relation_operations(
    execute: &Arc<CommandNode<CommandSourceStack>>,
    on: LiteralArgumentBuilder<CommandSourceStack>,
) -> LiteralArgumentBuilder<CommandSourceStack> {
    let relation = |name: &'static str, relation: EntityRelation| literal(name).fork(execute.clone(), expand_entity_relation(relation));
    on.then(relation("owner", EntityRelation::Owner))
        .then(relation("leasher", EntityRelation::Leasher))
        .then(relation("target", EntityRelation::Target))
        .then(relation("attacker", EntityRelation::Attacker))
        .then(relation("vehicle", EntityRelation::Vehicle))
        .then(relation("controller", EntityRelation::Controller))
        .then(relation("origin", EntityRelation::Origin))
        .then(relation("passengers", EntityRelation::Passengers))
}

/// `spawnEntityAndRedirect(source, type)`.
fn spawn_entity_and_redirect(source: &CommandSourceStack, entity_type: mc_data::EntityType) -> Result<CommandSourceStack, CommandSyntaxException> {
    let entity = super::summon_command::create_entity(source, entity_type, source.get_position(), &CompoundTag::new(), true)?;
    let command_entity = source.with_server(|server| host::command_entity(server, &entity));
    Ok(source.with_entity(command_entity.expect("a freshly summoned entity is in its level")))
}

/// `ExecuteIfFunctionCustomModifier`.
struct IfFunctionModifier {
    expected: bool,
}

impl CustomModifierExecutor for IfFunctionModifier {
    fn apply(
        &self,
        original_source: &CommandSourceStack,
        current_sources: Vec<CommandSourceStack>,
        current_step: &ContextChain<CommandSourceStack>,
        modifiers: ChainModifiers,
        output: &mut ExecutionControl<'_>,
    ) {
        let expected = self.expected;
        schedule_function_conditions_and_test(
            original_source,
            current_sources,
            super::function_command::modify_sender_for_execution,
            move |value| (value != 0) == expected,
            current_step,
            None,
            output,
            |c| FunctionArgument::get_functions(c, "name"),
            modifiers,
        );
    }
}

/// `ExecuteCommand.scheduleFunctionConditionsAndTest`: runs the functions once per source in an isolated frame and
/// continues the chain with the sources whose return value passes `check`.
#[allow(clippy::too_many_arguments)]
pub fn schedule_function_conditions_and_test(
    original_source: &CommandSourceStack,
    current_sources: Vec<CommandSourceStack>,
    function_context_modifier: impl Fn(&CommandSourceStack) -> CommandSourceStack,
    check: impl Fn(i32) -> bool + Send + Sync + 'static,
    current_step: &ContextChain<CommandSourceStack>,
    parameters: Option<&CompoundTag>,
    output: &mut ExecutionControl<'_>,
    function_getter: impl Fn(&CommandContext<CommandSourceStack>) -> Result<Vec<Arc<dyn CommandFunction>>, CommandSyntaxException>,
    modifiers: ChainModifiers,
) {
    let functions_to_run = match function_getter(&current_step.get_top_context().copy_for(original_source.clone())) {
        Ok(functions) => functions,
        Err(e) => {
            original_source.handle_error(&e, modifiers.is_forked(), output.tracer());
            return;
        }
    };
    if functions_to_run.is_empty() {
        return;
    }
    let dispatcher_owner = original_source.with_server(|server| server.commands.clone());
    let mut instantiated_functions: Vec<Arc<InstantiatedFunction>> = Vec::with_capacity(functions_to_run.len());
    let instantiate_all = |instantiated: &mut Vec<Arc<InstantiatedFunction>>| -> Result<(), CommandSyntaxException> {
        for function in &functions_to_run {
            match function.instantiate(parameters, dispatcher_owner.get_dispatcher()) {
                Ok(function) => instantiated.push(function),
                Err(e) => return Err(instantiation_failure(function.id(), &e)),
            }
        }
        Ok(())
    };
    if let Err(e) = instantiate_all(&mut instantiated_functions) {
        original_source.handle_error(&e, modifiers.is_forked(), output.tracer());
    }
    let instantiated_functions = Arc::new(instantiated_functions);
    let filtered_sources: Arc<Mutex<Vec<CommandSourceStack>>> = Arc::new(Mutex::new(Vec::with_capacity(current_sources.len())));
    let check = Arc::new(check);
    for source in current_sources {
        let new_function_context = function_context_modifier(&source.clear_callbacks());
        let function_callback = {
            let (filtered_sources, check, source) = (filtered_sources.clone(), check.clone(), source.clone());
            CommandResultCallback::new(move |_success, result| {
                if check(result) {
                    filtered_sources.lock().unwrap_or_else(|e| e.into_inner()).push(source.clone());
                }
            })
        };
        let functions = instantiated_functions.clone();
        output.queue_next(isolated_call(
            move |o: &mut ExecutionControl<'_>| {
                for function in functions.iter() {
                    let return_consumer = o.current_frame().return_value_consumer.clone();
                    o.queue_next(bind(Arc::new(CallFunction::new(function.clone(), return_consumer, true)), new_function_context.clone()));
                }
                o.queue_next(fallthrough_task::instance());
            },
            function_callback,
        ));
    }
    let next_stage = current_step.next_stage().expect("a function modifier is followed by another stage");
    let input = current_step.get_top_context().get_input().to_owned();
    let original_source = original_source.clone();
    let continuation: EntryAction = Box::new(move |context, frame| {
        let sources = std::mem::take(&mut *filtered_sources.lock().unwrap_or_else(|e| e.into_inner()));
        BuildContexts::continuation(&input, next_stage, modifiers, original_source, sources)(context, frame);
    });
    output.queue_next(continuation);
}

/// `ERROR_FUNCTION_CONDITION_INSTANTATION_FAILURE.create(function.id(), e.messageComponent())`.
fn instantiation_failure(id: &mc_core::ident::Identifier, e: &FunctionInstantiationException) -> CommandSyntaxException {
    ERROR_FUNCTION_CONDITION_INSTANTATION_FAILURE.create(id.to_string(), e.message_component.clone())
}
