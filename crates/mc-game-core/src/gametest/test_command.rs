//! `TestCommand`: `/test` (run, runmultiple, runthese, runclosest, runthat, runfailed, verify, locate, reset*, clear*,
//! stop, pos, create; and in IDE builds export*).

use super::failed_test_tracker;
use super::game_test_batch::{GameTestBatch, GameTestBatchListener, from_game_test_info_with, to_game_test_batch};
use super::game_test_info::GameTestInfo;
use super::game_test_instance::all_tests;
use super::game_test_listener::GameTestListener;
use super::game_test_runner::{self, GameTestRunner, StructureSpawner};
use super::multiple_test_tracker::MultipleTestTracker;
use super::player_view::PlayerView;
use super::retry_options::RetryOptions;
use super::structure_grid_spawner::{FirstCorner, StructureGridSpawner};
use super::structure_utils;
use super::test_finder::{GlobalPos, TestFinder};
use super::{IS_RUNNING_IN_IDE, game_test_ticker};
use crate::commands::arguments::IdentifierArgument;
use crate::commands::arguments::resource_arguments::{ResourceArgument, ResourceSelectorArgument};
use crate::commands::message::{message, simple_translatable};
use crate::commands::server_access::enter_server;
use crate::commands::shared_suggestion_provider::filter_resources;
use crate::commands::{CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, literal};
use crate::server::MinecraftServer;
use crate::world::block_entity::test_instance_block_entity::{self, TestInstanceView};
use crate::world::level::{BlockGetter, DimensionKey, LevelAccessor};
use crate::{register_command, translatable};
use mc_brigadier::arguments::{BoolArgumentType, IntegerArgumentType, StringArgumentType, integer, integer_min};
use mc_brigadier::{ArgumentBuilder, CommandContext, CommandSyntaxException, Dynamic3CommandExceptionType, DynamicCommandExceptionType, SimpleCommandExceptionType};
use mc_core::ident::Identifier;
use mc_core::pos::{BlockPos, Vec3i};
use mc_core::rotation::Rotation;
use mc_data::block::Block;
use mc_protocol::packets::play::ClientboundGameTestHighlightPosPacket;
use mc_text::util::wrap_in_square_brackets;
use mc_text::{ChatFormatting, ClickEvent, Component, HoverEvent};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, LazyLock};

/// `TEST_NEARBY_SEARCH_RADIUS` / `TEST_FULL_SEARCH_RADIUS`.
pub const TEST_NEARBY_SEARCH_RADIUS: i32 = 15;
pub const TEST_FULL_SEARCH_RADIUS: i32 = 250;
/// `VERIFY_TEST_GRID_AXIS_SIZE` / `VERIFY_TEST_BATCH_SIZE`.
pub const VERIFY_TEST_GRID_AXIS_SIZE: i32 = 10;
pub const VERIFY_TEST_BATCH_SIZE: usize = 100;
const DEFAULT_CLEAR_RADIUS: i32 = 250;
const MAX_CLEAR_RADIUS: i32 = 1024;
const TEST_POS_Z_OFFSET_FROM_PLAYER: i32 = 3;
const DEFAULT_X_SIZE: i32 = 5;
const DEFAULT_Y_SIZE: i32 = 5;
const DEFAULT_Z_SIZE: i32 = 5;

static CLEAR_NO_TESTS: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.test.clear.error.no_tests"));
static TEST_INSTANCE_COULD_NOT_BE_FOUND: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.test.error.test_instance_not_found"));
static NO_STRUCTURES_TO_EXPORT: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| SimpleCommandExceptionType::new(message(Component::literal("Could not find any structures to export"))));
static NO_TEST_INSTANCES: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.test.error.no_test_instances"));
static NO_TEST_CONTAINING: LazyLock<Dynamic3CommandExceptionType<i32, i32, i32>> = LazyLock::new(|| {
    Dynamic3CommandExceptionType::new(|x: i32, y: i32, z: i32| message(translatable!("commands.test.error.no_test_containing_pos", x, y, z)))
});
static TOO_LARGE: LazyLock<DynamicCommandExceptionType<i32>> =
    LazyLock::new(|| DynamicCommandExceptionType::new(|size: i32| message(translatable!("commands.test.error.too_large", size))));

type Finder = Arc<dyn Fn(&CommandContext<CommandSourceStack>) -> TestFinder + Send + Sync>;

/// Sends `message_supplier` through `source` with the server reachable for the stack's broadcast (the caller holds the
/// server, so `source.send_success` must not borrow it again).
fn send_success(server: &mut MinecraftServer, source: &CommandSourceStack, message_supplier: impl FnOnce() -> Component, broadcast: bool) {
    enter_server(server, || source.send_success(message_supplier, broadcast));
}

// ---------------------------------------------------------------------------------------------------------------
// registration
// ---------------------------------------------------------------------------------------------------------------

fn get_int(c: &CommandContext<CommandSourceStack>, name: &str) -> i32 {
    IntegerArgumentType::get_integer(c, name)
}

fn get_bool(c: &CommandContext<CommandSourceStack>, name: &str) -> bool {
    BoolArgumentType::get_bool(c, name)
}

/// `runWithRetryOptions(runArgument, finder, then)` / `runWithRetryOptionsAndBuildInfo` (`build_info`).
fn run_with_retry_options<A: ArgumentBuilder<CommandSourceStack>>(run_argument: A, finder: Finder, build_info: bool) -> A {
    let until_failed = {
        let (f1, f2, f3) = (finder.clone(), finder.clone(), finder.clone());
        let mut until_failed = argument("untilFailed", BoolArgumentType::bool()).executes(move |c| {
            run(c.get_source(), f1(c), RetryOptions::new(get_int(c, "numberOfTimes"), get_bool(c, "untilFailed")), 0, 8)
        });
        if build_info {
            until_failed = until_failed.then(
                argument("rotationSteps", integer())
                    .executes(move |c| {
                        run(c.get_source(), f2(c), RetryOptions::new(get_int(c, "numberOfTimes"), get_bool(c, "untilFailed")), get_int(c, "rotationSteps"), 8)
                    })
                    .then(argument("testsPerRow", integer()).executes(move |c| {
                        run(
                            c.get_source(),
                            f3(c),
                            RetryOptions::new(get_int(c, "numberOfTimes"), get_bool(c, "untilFailed")),
                            get_int(c, "rotationSteps"),
                            get_int(c, "testsPerRow"),
                        )
                    })),
            );
        }
        until_failed
    };
    let (f_plain, f_times) = (finder.clone(), finder);
    run_argument
        .executes(move |c| run(c.get_source(), f_plain(c), RetryOptions::no_retries(), 0, 8))
        .then(
            argument("numberOfTimes", integer_min(0))
                .executes(move |c| run(c.get_source(), f_times(c), RetryOptions::new(get_int(c, "numberOfTimes"), false), 0, 8))
                .then(until_failed),
        )
}

/// `TestCommand::suggestTestInstanceIds`.
fn suggest_test_instance_ids(c: &CommandContext<CommandSourceStack>, builder: mc_brigadier::SuggestionsBuilder) -> Result<mc_brigadier::Suggestions, CommandSyntaxException> {
    let test_ids: Vec<Identifier> = c.get_source().with_server(|server| all_tests(server.registries.composite_access()).into_iter().map(|t| t.key).collect());
    let mut builder = builder;
    let remaining = builder.get_remaining().to_lowercase();
    let mut matched = Vec::new();
    filter_resources(test_ids, &remaining, |id| id.clone(), |id| matched.push(id));
    for id in matched {
        builder.suggest(&display_test_id(&id));
    }
    Ok(builder.build())
}

/// `displayTestId(id)`.
fn display_test_id(id: &Identifier) -> String {
    if id.namespace() == "minecraft" { id.path().to_owned() } else { id.to_string() }
}

register_command!("TestCommand", |ctx| {
    let by_selection: Finder = Arc::new(|c| TestFinder::builder().by_resource_selection(c.get_source(), &ResourceSelectorArgument::get_selected_resources(c, "tests")));
    let run_failed_with_required_tests_flag = run_with_retry_options(
        argument("onlyRequiredTests", BoolArgumentType::bool()),
        Arc::new(|c| TestFinder::builder().failed_tests(c.get_source(), get_bool(c, "onlyRequiredTests"))),
        true,
    );
    let tests_argument = || argument("tests", ResourceSelectorArgument::resource_selector(ctx.build_context, "test_instance")).suggests(suggest_test_instance_ids);
    let nearest: Finder = Arc::new(|c| TestFinder::builder().nearest(c.get_source()));
    let all_nearby: Finder = Arc::new(|c| TestFinder::builder().all_nearby(c.get_source()));
    let looked_at: Finder = Arc::new(|c| TestFinder::builder().looked_at(c.get_source()));
    let failed: Finder = Arc::new(|c| TestFinder::builder().failed_tests(c.get_source(), false));
    let mut test_command = literal("test")
        .requires(has_permission(LEVEL_GAMEMASTERS))
        .then(literal("run").then(run_with_retry_options(tests_argument(), by_selection.clone(), true)))
        .then(
            literal("runmultiple").then(
                tests_argument()
                    .executes({
                        let f = by_selection.clone();
                        move |c| run(c.get_source(), f(c), RetryOptions::no_retries(), 0, 8)
                    })
                    .then(argument("amount", integer()).executes(|c| {
                        run(
                            c.get_source(),
                            TestFinder::builder()
                                .create_multiple_copies(get_int(c, "amount"))
                                .by_resource_selection(c.get_source(), &ResourceSelectorArgument::get_selected_resources(c, "tests")),
                            RetryOptions::no_retries(),
                            0,
                            8,
                        )
                    })),
            ),
        )
        .then(run_with_retry_options(literal("runthese"), all_nearby, false))
        .then(run_with_retry_options(literal("runclosest"), nearest, false))
        .then(run_with_retry_options(literal("runthat"), looked_at, false))
        .then(run_with_retry_options(literal("runfailed").then(run_failed_with_required_tests_flag), failed, true))
        .then(literal("verify").then(tests_argument().executes(move |c| verify(c.get_source(), by_selection(c)))))
        .then(literal("locate").then(
            tests_argument().executes(|c| locate(c.get_source(), TestFinder::builder().by_resource_selection(c.get_source(), &ResourceSelectorArgument::get_selected_resources(c, "tests")))),
        ))
        .then(literal("resetclosest").executes(|c| reset(c.get_source(), TestFinder::builder().nearest(c.get_source()))))
        .then(literal("resetthese").executes(|c| reset(c.get_source(), TestFinder::builder().all_nearby(c.get_source()))))
        .then(literal("resetthat").executes(|c| reset(c.get_source(), TestFinder::builder().looked_at(c.get_source()))))
        .then(literal("clearthat").executes(|c| clear(c.get_source(), TestFinder::builder().looked_at(c.get_source()))))
        .then(literal("clearthese").executes(|c| clear(c.get_source(), TestFinder::builder().all_nearby(c.get_source()))))
        .then(
            literal("clearall")
                .executes(|c| clear(c.get_source(), TestFinder::builder().radius(c.get_source(), DEFAULT_CLEAR_RADIUS)))
                .then(argument("radius", integer()).executes(|c| {
                    clear(c.get_source(), TestFinder::builder().radius(c.get_source(), get_int(c, "radius").clamp(0, MAX_CLEAR_RADIUS)))
                })),
        )
        .then(literal("stop").executes(|c| Ok(c.get_source().with_server(stop_tests))))
        .then(
            literal("pos")
                .executes(|c| show_pos(c.get_source(), "pos"))
                .then(argument("var", StringArgumentType::word()).executes(|c| show_pos(c.get_source(), &StringArgumentType::get_string(c, "var")))),
        )
        .then(
            literal("create").then(
                argument("id", IdentifierArgument::id())
                    .executes(|c| create_new_structure(c.get_source(), IdentifierArgument::get_id(c, "id"), DEFAULT_X_SIZE, DEFAULT_Y_SIZE, DEFAULT_Z_SIZE))
                    .then(
                        argument("width", integer())
                            .executes(|c| {
                                create_new_structure(c.get_source(), IdentifierArgument::get_id(c, "id"), get_int(c, "width"), get_int(c, "width"), get_int(c, "width"))
                            })
                            .then(argument("height", integer()).then(argument("depth", integer()).executes(|c| {
                                create_new_structure(
                                    c.get_source(),
                                    IdentifierArgument::get_id(c, "id"),
                                    get_int(c, "width"),
                                    get_int(c, "height"),
                                    get_int(c, "depth"),
                                )
                            }))),
                    ),
            ),
        );
    if IS_RUNNING_IN_IDE {
        test_command = test_command
            .then(
                literal("export").then(
                    argument("test", ResourceArgument::resource(ctx.build_context, "test_instance"))
                        .suggests(suggest_test_instance_ids)
                        .executes(|c| export_test_structure(c.get_source(), ResourceArgument::get_resource(c, "test", "test_instance")?.id)),
                ),
            )
            .then(literal("exportclosest").executes(|c| export(c.get_source(), TestFinder::builder().nearest(c.get_source()))))
            .then(literal("exportthese").executes(|c| export(c.get_source(), TestFinder::builder().all_nearby(c.get_source()))))
            .then(literal("exportthat").executes(|c| export(c.get_source(), TestFinder::builder().looked_at(c.get_source()))));
    }
    ctx.dispatcher.register(test_command);
});

// ---------------------------------------------------------------------------------------------------------------
// subcommands
// ---------------------------------------------------------------------------------------------------------------

/// `reset(finder)`.
fn reset(source: &CommandSourceStack, finder: TestFinder) -> Result<i32, CommandSyntaxException> {
    let count = source.with_server(|server| {
        stop_tests(server);
        let mut count = 0;
        for pos in finder.find_test_pos(server) {
            if let Some(info) = create_game_test_info(server, &pos, source, RetryOptions::no_retries()) {
                reset_game_test_info(server, source, &info);
                count += 1;
            }
        }
        if count > 0 {
            send_success(server, source, || translatable!("commands.test.reset.success", count), true);
        }
        count
    });
    if count == 0 { Err(CLEAR_NO_TESTS.create()) } else { Ok(count) }
}

/// `clear(finder)`.
fn clear(source: &CommandSourceStack, finder: TestFinder) -> Result<i32, CommandSyntaxException> {
    let count = source.with_server(|server| {
        stop_tests(server);
        let tests: Vec<(DimensionKey, TestInstanceView)> = finder
            .find_test_pos(server)
            .into_iter()
            .filter_map(|p| TestInstanceView::read(server, &p.dimension, p.pos).map(|view| (p.dimension, view)))
            .collect();
        for (dimension, view) in &tests {
            if let Some(level) = server.level_mut(dimension) {
                structure_utils::clear_space_for_structure(level, &view.get_test_bounding_box());
            }
            test_instance_block_entity::remove_barriers(server, dimension, view.pos);
            if let Some(level) = server.level_mut(dimension) {
                level.destroy_block(view.pos, false, None, 512);
            }
        }
        if !tests.is_empty() {
            send_success(server, source, || translatable!("commands.test.clear.success", tests.len() as i32), true);
        }
        tests.len() as i32
    });
    if count == 0 { Err(CLEAR_NO_TESTS.create()) } else { Ok(count) }
}

/// `export(finder)`.
fn export(source: &CommandSourceStack, finder: TestFinder) -> Result<i32, CommandSyntaxException> {
    let result = source.with_server(|server| {
        let mut count = 0;
        let mut all_good = true;
        for pos in finder.find_test_pos(server) {
            if TestInstanceView::read(server, &pos.dimension, pos.pos).is_none() {
                return Err(TEST_INSTANCE_COULD_NOT_BE_FOUND.create());
            }
            if !test_instance_block_entity::export_test(server, &pos.dimension, pos.pos, &mut |c| source.send_system_message(&c)) {
                all_good = false;
            }
            count += 1;
        }
        if count == 0 {
            return Err(NO_STRUCTURES_TO_EXPORT.create());
        }
        let message = format!("Exported {count} structures");
        send_success(server, source, || Component::literal(message), true);
        Ok(if all_good { 0 } else { 1 })
    });
    result
}

/// `verify(finder)`.
fn verify(source: &CommandSourceStack, finder: TestFinder) -> Result<i32, CommandSyntaxException> {
    source.with_server(|server| {
        stop_tests(server);
        let test_pos: FirstCorner = {
            let source = source.clone();
            Rc::new(move |server, dimension| create_test_position_around(server, &source, dimension))
        };
        let mut infos = to_game_test_infos(server, source, RetryOptions::no_retries(), &finder);
        infos.extend(to_game_test_info(server, source, RetryOptions::no_retries(), &finder, 0));
        failed_test_tracker::forget_failed_tests();
        let mut batches: Vec<GameTestBatch> = Vec::new();
        for info in &infos {
            for rotation in Rotation::ALL {
                let mut transformed_infos = Vec::new();
                for _ in 0..VERIFY_TEST_BATCH_SIZE {
                    let copy_info = GameTestInfo::new(info.get_test_holder().clone(), rotation, info.level().clone(), RetryOptions::new(1, true));
                    copy_info.set_test_block_pos(info.get_test_block_pos());
                    transformed_infos.push(copy_info);
                }
                batches.push(to_game_test_batch(transformed_infos, info.get_test().batch().clone(), rotation.ordinal() as i32, info.level().clone()));
            }
        }
        let spawner: Rc<StructureGridSpawner> = StructureGridSpawner::new(test_pos, VERIFY_TEST_GRID_AXIS_SIZE, true);
        let runner = game_test_runner::Builder::from_batches(batches)
            .batcher(from_game_test_info_with(VERIFY_TEST_BATCH_SIZE))
            .new_structure_spawner(spawner.clone() as Rc<dyn StructureSpawner>)
            .existing_structure_spawner(spawner as Rc<dyn StructureSpawner>)
            .halt_on_error()
            .clear_between_batches()
            .build();
        Ok(track_and_start_runner(server, source.clone(), &runner))
    })
}

/// `run(finder, retryOptions, extraRotationSteps, testsPerRow)`.
fn run(source: &CommandSourceStack, finder: TestFinder, retry_options: RetryOptions, extra_rotation_steps: i32, tests_per_row: i32) -> Result<i32, CommandSyntaxException> {
    source.with_server(|server| {
        stop_tests(server);
        let mut infos = to_game_test_infos(server, source, retry_options, &finder);
        infos.extend(to_game_test_info(server, source, retry_options, &finder, extra_rotation_steps));
        if infos.is_empty() {
            send_success(server, source, || translatable!("commands.test.no_tests"), false);
            return Ok(0);
        }
        failed_test_tracker::forget_failed_tests();
        let count = infos.len() as i32;
        send_success(server, source, || translatable!("commands.test.run.running", count), false);
        let test_pos: FirstCorner = {
            let source = source.clone();
            Rc::new(move |server, dimension| create_test_position_around(server, &source, dimension))
        };
        let runner = game_test_runner::Builder::from_info(infos).new_structure_spawner(StructureGridSpawner::new(test_pos, tests_per_row, false)).build();
        Ok(track_and_start_runner(server, source.clone(), &runner))
    })
}

/// `findSelectedTestPositionsAcrossDimensions(finder)`.
fn find_selected_test_positions_across_dimensions(server: &mut MinecraftServer, source: &CommandSourceStack, finder: &TestFinder) -> Vec<GlobalPos> {
    let selected_tests: Vec<Identifier> = finder.find_tests().into_iter().map(|t| t.key).collect();
    if selected_tests.is_empty() {
        return Vec::new();
    }
    let source_pos = BlockPos::containing_position(&source.get_position());
    let mut matches = Vec::new();
    for dimension in server.level_keys() {
        for pos in structure_utils::find_test_blocks(server, &dimension, source_pos, TEST_FULL_SEARCH_RADIUS) {
            let selected = TestInstanceView::read(server, &dimension, pos).is_some_and(|view| view.test_key().is_some_and(|key| selected_tests.contains(key)));
            if selected {
                matches.push(GlobalPos { dimension: dimension.clone(), pos });
            }
        }
    }
    matches
}

/// `locate(finder)`.
fn locate(source: &CommandSourceStack, finder: TestFinder) -> Result<i32, CommandSyntaxException> {
    let structures = source.with_server(|server| {
        source.send_system_message(&translatable!("commands.test.locate.started"));
        let mut structures_found = 0;
        let source_pos = BlockPos::containing_position(&source.get_position());
        let source_dimension = source.get_level().clone();
        let mut found_test_dimensions: Vec<DimensionKey> = Vec::new();
        let mut positions = finder.find_test_pos(server);
        positions.extend(find_selected_test_positions_across_dimensions(server, source, &finder));
        let mut seen: Vec<GlobalPos> = Vec::new();
        for structure_pos in positions {
            if seen.contains(&structure_pos) {
                continue;
            }
            seen.push(structure_pos.clone());
            let Some(test_block) = TestInstanceView::read(server, &structure_pos.dimension, structure_pos.pos) else { continue };
            if !found_test_dimensions.contains(&structure_pos.dimension) {
                found_test_dimensions.push(structure_pos.dimension.clone());
            }
            let direction = test_block.get_rotation().rotate(mc_core::direction::Direction::North);
            let teleport_position = test_block.pos.relative_n(direction, 3);
            let teleport_y_rot = direction.get_opposite().to_y_rot() as i32;
            let dimension_id = structure_pos.dimension.to_string();
            let tp_command = format!("/execute in {} run tp @s {} {} {} {} 0", dimension_id, teleport_position.x(), teleport_position.y(), teleport_position.z(), teleport_y_rot);
            let dx = source_pos.x().wrapping_sub(structure_pos.pos.x());
            let dz = source_pos.z().wrapping_sub(structure_pos.pos.z());
            let distance = if structure_pos.dimension == source_dimension {
                let squared = dx.wrapping_mul(dx).wrapping_add(dz.wrapping_mul(dz));
                mc_core::math::mth::floor_f32((f64::from(squared as f32).sqrt()) as f32).to_string()
            } else {
                "N/A".to_owned()
            };
            let coordinates = wrap_in_square_brackets(
                translatable!("chat.coordinates", structure_pos.pos.x(), structure_pos.pos.y(), structure_pos.pos.z())
                    .append(Component::literal(format!(" ->[{}]", structure_pos.dimension.path()))),
            )
            .with_style_fn(|s| {
                s.with_color_formatting(Some(ChatFormatting::Green))
                    .with_click_event(Some(ClickEvent::SuggestCommand(tp_command.clone())))
                    .with_hover_event(Some(HoverEvent::show_text(translatable!("chat.coordinates.tooltip"))))
            });
            send_success(server, source, || translatable!("commands.test.locate.found", coordinates, distance), false);
            structures_found += 1;
        }
        if structures_found > 0 {
            output_player_coordinates(server, source, &found_test_dimensions);
            send_success(server, source, || translatable!("commands.test.locate.done", structures_found), true);
        }
        structures_found
    });
    if structures == 0 { Err(NO_TEST_INSTANCES.create()) } else { Ok(structures) }
}

/// `resetGameTestInfo(source, testInfo)`.
fn reset_game_test_info(server: &mut MinecraftServer, source: &CommandSourceStack, test_info: &Rc<GameTestInfo>) {
    if let Some(pos) = test_info.get_test_block_pos() {
        test_instance_block_entity::reset_test(server, test_info.level(), pos, &mut |c| source.send_system_message(&c));
    }
}

/// `toGameTestInfos(source, retryOptions, finder)`.
fn to_game_test_infos(server: &mut MinecraftServer, source: &CommandSourceStack, retry_options: RetryOptions, finder: &TestFinder) -> Vec<Rc<GameTestInfo>> {
    finder.find_test_pos(server).iter().filter_map(|pos| create_game_test_info(server, pos, source, retry_options)).collect()
}

/// `toGameTestInfo(source, retryOptions, finder, rotationSteps)`.
fn to_game_test_info(server: &mut MinecraftServer, source: &CommandSourceStack, retry_options: RetryOptions, finder: &TestFinder, rotation_steps: i32) -> Vec<Rc<GameTestInfo>> {
    let mut infos = Vec::new();
    for test in finder.find_tests() {
        if !verify_structure_exists(server, source, test.value.structure()) {
            continue;
        }
        let dimension = test.value.info().dimension.clone();
        if server.level(&dimension).is_none() {
            source.send_failure(Component::literal(format!("Could not resolve level for dimension: {dimension}")));
            continue;
        }
        infos.push(GameTestInfo::new(test, structure_utils::get_rotation_for_rotation_steps(rotation_steps), dimension, retry_options));
    }
    infos
}

/// `createGameTestInfo(testBlockPos, source, retryOptions)`.
fn create_game_test_info(server: &mut MinecraftServer, test_block_pos: &GlobalPos, source: &CommandSourceStack, retry_options: RetryOptions) -> Option<Rc<GameTestInfo>> {
    let Some(view) = TestInstanceView::read(server, &test_block_pos.dimension, test_block_pos.pos) else {
        source.send_failure(translatable!(
            "commands.test.error.test_instance_not_found.position",
            test_block_pos.pos.x(),
            test_block_pos.pos.y(),
            test_block_pos.pos.z()
        ));
        return None;
    };
    let Some(test) = view.test.clone() else {
        source.send_failure(translatable!("commands.test.error.non_existant_test", view.get_test_name()));
        return None;
    };
    let test_info = GameTestInfo::new(test, view.get_rotation(), test_block_pos.dimension.clone(), retry_options);
    test_info.set_test_block_pos(Some(test_block_pos.pos));
    if verify_structure_exists(server, source, test_info.get_structure()) { Some(test_info) } else { None }
}

/// `createNewStructure(source, id, xSize, ySize, zSize)`.
fn create_new_structure(source: &CommandSourceStack, id: Identifier, x_size: i32, y_size: i32, z_size: i32) -> Result<i32, CommandSyntaxException> {
    if x_size > 48 || y_size > 48 || z_size > 48 {
        return Err(TOO_LARGE.create(48));
    }
    source.with_server(|server| {
        let level = source.get_level().clone();
        let test_pos = create_test_position_around(server, source, &level);
        structure_utils::create_new_empty_test(server, &level, &id, test_pos, Vec3i::new(x_size, y_size, z_size), Rotation::None);
        let Some(test) = TestInstanceView::read(server, &level, test_pos) else { return };
        let low = test.get_structure_pos();
        let high = low.offset(x_size - 1, 0, z_size - 1);
        if let Some(lvl) = server.level_mut(&level) {
            for block_pos in BlockPos::between_closed(&low, &high) {
                lvl.set_block_and_update(block_pos, Block::BEDROCK.default_state());
            }
        }
        send_success(server, source, || translatable!("commands.test.create.success", test.get_test_name()), true);
    });
    Ok(1)
}

/// `showPos(source, varName)`.
fn show_pos(source: &CommandSourceStack, var_name: &str) -> Result<i32, CommandSyntaxException> {
    let player = source.get_player_or_exception()?.clone();
    source.with_server(|server| {
        let level = source.get_level().clone();
        let view = PlayerView::of(server, source);
        let Some(pick) = view.as_ref().and_then(|v| v.pick(server, 10.0)) else { return Err(TEST_INSTANCE_COULD_NOT_BE_FOUND.create()) };
        let target_pos_absolute = pick.block_pos;
        let mut test_block_pos = structure_utils::find_test_containing_pos(server, &level, target_pos_absolute, TEST_NEARBY_SEARCH_RADIUS);
        if test_block_pos.is_none() {
            test_block_pos = structure_utils::find_test_containing_pos(server, &level, target_pos_absolute, TEST_FULL_SEARCH_RADIUS);
        }
        let Some(test_block_pos) = test_block_pos else {
            return Err(NO_TEST_CONTAINING.create(target_pos_absolute.x(), target_pos_absolute.y(), target_pos_absolute.z()));
        };
        let Some(test_block_entity) = TestInstanceView::read(server, &level, test_block_pos) else { return Err(TEST_INSTANCE_COULD_NOT_BE_FOUND.create()) };
        let structure_pos = test_block_entity.get_structure_pos();
        let target_pos_relative = target_pos_absolute.subtract(&structure_pos);
        let target_pos_description = format!("{}, {}, {}", target_pos_relative.x(), target_pos_relative.y(), target_pos_relative.z());
        let test_name = test_block_entity.get_test_name().get_string();
        let coords = translatable!("commands.test.coordinates", target_pos_relative.x(), target_pos_relative.y(), target_pos_relative.z()).with_style_fn(|_| {
            mc_text::Style::EMPTY
                .with_bold(Some(true))
                .with_color_formatting(Some(ChatFormatting::Green))
                .with_hover_event(Some(HoverEvent::show_text(translatable!("commands.test.coordinates.copy"))))
                .with_click_event(Some(ClickEvent::CopyToClipboard(format!("final BlockPos {var_name} = new BlockPos({target_pos_description});"))))
        });
        send_success(server, source, || translatable!("commands.test.relative_position", test_name, coords), false);
        if let Some(handle) = &player.player {
            handle.connection.send(&ClientboundGameTestHighlightPosPacket { absolute_pos: target_pos_absolute, relative_pos: target_pos_relative });
        }
        Ok(1)
    })
}

/// `stopTests()`.
fn stop_tests(server: &mut MinecraftServer) -> i32 {
    game_test_ticker::clear(server);
    1
}

/// `trackAndStartRunner(source, runner)`.
pub fn track_and_start_runner(server: &mut MinecraftServer, source: CommandSourceStack, runner: &Rc<GameTestRunner>) -> i32 {
    runner.add_listener(Rc::new(TestBatchSummaryDisplayer { source: source.clone() }));
    let tracker = MultipleTestTracker::with_tests(runner.get_test_infos());
    tracker.add_listener(Rc::new(TestSummaryDisplayer { source, tracker: tracker.clone(), dimensions: RefCell::new(Vec::new()) }));
    tracker.add_failure_listener(|test_info| failed_test_tracker::remember_failed_test(test_info.get_test_holder().clone()));
    runner.start(server);
    1
}

/// `exportTestStructure(source, test)`.
fn export_test_structure(source: &CommandSourceStack, test: Identifier) -> Result<i32, CommandSyntaxException> {
    source.with_server(|server| {
        let registries = server.registries.composite_access().clone();
        let Some(holder) = super::game_test_instance::lookup_test(&registries, &test) else { return Ok(0) };
        let level = source.get_level().clone();
        let failed = test_instance_block_entity::export(server, &level, holder.value.structure(), &mut |c| source.send_system_message(&c));
        Ok(if failed { 0 } else { 1 })
    })
}

/// `verifyStructureExists(source, structure)`.
fn verify_structure_exists(server: &mut MinecraftServer, source: &CommandSourceStack, structure: &Identifier) -> bool {
    let exists = server.level(source.get_level()).is_some_and(|level| level.structure_template_manager().get(structure).is_some());
    if !exists {
        source.send_failure(translatable!("commands.test.error.structure_not_found", Component::literal(structure.to_string())));
    }
    exists
}

/// `createTestPositionAround(source, dimension)`.
fn create_test_position_around(server: &mut MinecraftServer, source: &CommandSourceStack, dimension: &DimensionKey) -> BlockPos {
    let info = player_and_test_info(server, source, dimension);
    BlockPos::new(info.player_pos.x(), info.surface_y, info.player_pos.z() + TEST_POS_Z_OFFSET_FROM_PLAYER)
}

/// `TestCommand.Info`.
struct Info {
    player: Option<PlayerView>,
    player_pos: BlockPos,
    surface_y: i32,
}

/// `playerAndTestInfo(source, dimension)`.
fn player_and_test_info(server: &mut MinecraftServer, source: &CommandSourceStack, dimension: &DimensionKey) -> Info {
    let player = PlayerView::of(server, source);
    let player_pos = BlockPos::containing_position(&player.as_ref().map(|p| p.position).unwrap_or_else(|| source.get_position()));
    let test_pos = BlockPos::new(player_pos.x(), 384, player_pos.z());
    let level = server.level_mut(dimension).expect("the dimension of a test exists");
    level.chunk_blocking_mut(test_pos.x() >> 4, test_pos.z() >> 4);
    let surface_y = level.get_heightmap_pos(mc_world::heightmap::HeightmapType::WorldSurface, test_pos).y();
    Info { player, player_pos, surface_y }
}

/// `outputTestCoordinates(source, testDimensions)`.
fn output_test_coordinates(server: &mut MinecraftServer, source: &CommandSourceStack, test_dimensions: &[DimensionKey]) {
    for test_dimension_key in test_dimensions {
        let test_info = player_and_test_info(server, source, test_dimension_key);
        let player_info = player_and_test_info(server, source, &source.get_level().clone());
        if is_player_in_current_test_dimension(&player_info, test_dimensions) {
            return;
        }
        let test_dimension = test_dimension_key.to_string();
        let path = test_dimension_key.path().to_owned();
        let suggestion = format!("/execute in {} run tp @s {} {} {} 0 0", test_dimension, test_info.player_pos.x(), test_info.surface_y, test_info.player_pos.z());
        let test_coordinates = wrap_in_square_brackets(translatable!("test.run.coordinates", test_info.player_pos.x(), test_info.surface_y, test_info.player_pos.z(), path)).with_style_fn(|s| {
            s.with_color_formatting(Some(ChatFormatting::Yellow))
                .with_click_event(Some(ClickEvent::SuggestCommand(suggestion.clone())))
                .with_hover_event(Some(HoverEvent::show_text(translatable!("chat.coordinates.tooltip"))))
        });
        send_success(server, source, || test_coordinates, false);
    }
}

/// `outputPlayerCoordinates(source, testDimensions)`.
fn output_player_coordinates(server: &mut MinecraftServer, source: &CommandSourceStack, test_dimensions: &[DimensionKey]) {
    let player_info = player_and_test_info(server, source, &source.get_level().clone());
    if is_player_in_current_test_dimension(&player_info, test_dimensions) {
        return;
    }
    let Some(player) = &player_info.player else { return };
    let player_dimension = player.dimension.to_string();
    let suggestion = format!(
        "/execute in {} run tp @s {} {} {} {} {}",
        player_dimension,
        player_info.player_pos.x(),
        player_info.player_pos.y(),
        player_info.player_pos.z(),
        mc_core::math::mth::floor_f32(player.y_rot),
        mc_core::math::mth::floor_f32(player.x_rot)
    );
    let player_coordinates = wrap_in_square_brackets(translatable!(
        "test.player.coordinates",
        player_info.player_pos.x(),
        player_info.player_pos.y(),
        player_info.player_pos.z(),
        player.dimension.path().to_owned()
    ))
    .with_style_fn(|s| {
        s.with_color_formatting(Some(ChatFormatting::Gold))
            .with_click_event(Some(ClickEvent::SuggestCommand(suggestion.clone())))
            .with_hover_event(Some(HoverEvent::show_text(translatable!("chat.coordinates.tooltip"))))
    });
    send_success(server, source, || player_coordinates, false);
}

/// `isPlayerInCurrentTestDimension(playerInfo, testDimensions)`.
fn is_player_in_current_test_dimension(player_info: &Info, test_dimensions: &[DimensionKey]) -> bool {
    match &player_info.player {
        None => false,
        Some(player) => {
            let overworld = Identifier::parse("minecraft:overworld").expect("valid id");
            test_dimensions.len() <= 1 && test_dimensions.contains(&player.dimension) && test_dimensions.contains(&overworld)
        }
    }
}

/// `TestBatchSummaryDisplayer`.
struct TestBatchSummaryDisplayer {
    source: CommandSourceStack,
}

impl GameTestBatchListener for TestBatchSummaryDisplayer {
    fn test_batch_starting(&self, server: &mut MinecraftServer, batch: &GameTestBatch) {
        let (name, index) = (batch.environment.registered_name(), batch.index);
        send_success(server, &self.source, || translatable!("commands.test.batch.starting", name, index), true);
    }

    fn test_batch_finished(&self, _server: &mut MinecraftServer, _batch: &GameTestBatch) {}
}

/// `TestSummaryDisplayer`.
struct TestSummaryDisplayer {
    source: CommandSourceStack,
    tracker: Rc<MultipleTestTracker>,
    dimensions: RefCell<Vec<DimensionKey>>,
}

impl TestSummaryDisplayer {
    fn add_dimension(&self, test_info: &Rc<GameTestInfo>) {
        let dimension = test_info.get_test().dimension().clone();
        let mut dimensions = self.dimensions.borrow_mut();
        if !dimensions.contains(&dimension) {
            dimensions.push(dimension);
        }
    }

    /// `showTestSummaryIfAllDone()`.
    fn show_test_summary_if_all_done(&self, server: &mut MinecraftServer) {
        if !self.tracker.is_done() {
            return;
        }
        let total = self.tracker.get_total_count();
        send_success(server, &self.source, || translatable!("commands.test.summary", total).with_format(ChatFormatting::White), true);
        if self.tracker.has_failed_required() {
            self.source.send_failure(translatable!("commands.test.summary.failed", self.tracker.get_failed_required_count()));
        } else {
            send_success(server, &self.source, || translatable!("commands.test.summary.all_required_passed").with_format(ChatFormatting::Green), true);
        }
        if self.tracker.has_failed_optional() {
            self.source.send_system_message(&translatable!("commands.test.summary.optional_failed", self.tracker.get_failed_optional_count()));
        }
        let dimensions = self.dimensions.borrow().clone();
        output_player_coordinates(server, &self.source, &dimensions);
        output_test_coordinates(server, &self.source, &dimensions);
        // the tracker and this listener reference each other through the tests
        self.tracker.release();
    }
}

impl GameTestListener for TestSummaryDisplayer {
    fn test_structure_loaded(&self, _server: &mut MinecraftServer, _test_info: &Rc<GameTestInfo>) {}

    fn test_passed(&self, server: &mut MinecraftServer, test_info: &Rc<GameTestInfo>, _runner: &Rc<GameTestRunner>) {
        self.add_dimension(test_info);
        self.show_test_summary_if_all_done(server);
    }

    fn test_failed(&self, server: &mut MinecraftServer, test_info: &Rc<GameTestInfo>, _runner: &Rc<GameTestRunner>) {
        self.add_dimension(test_info);
        self.show_test_summary_if_all_done(server);
    }

    fn test_added_for_rerun(
        &self,
        _server: &mut MinecraftServer,
        _listener: &Rc<dyn GameTestListener>,
        _original: &Rc<GameTestInfo>,
        copy: &Rc<GameTestInfo>,
        _runner: &Rc<GameTestRunner>,
    ) {
        self.tracker.add_test_to_track(copy.clone());
    }
}
