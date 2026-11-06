//! `ForceLoadCommand`.
use super::command_response_tracker::{CommandResponseTracker, ElementType};
use mc_brigadier::{
    ArgumentBuilder, CommandSyntaxException, Dynamic2CommandExceptionType,
    SimpleCommandExceptionType,
};
use mc_core::pos::ChunkPos;
use mc_game_core::commands::arguments::coordinates::block_pos_argument::ERROR_OUT_OF_WORLD;
use mc_game_core::commands::arguments::coordinates::{ColumnPos, ColumnPosArgument};
use mc_game_core::commands::message::{message, simple_translatable};
use mc_game_core::commands::{
    CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, literal,
};
use mc_game_core::{register_command, translatable};
use mc_text::Component;
use std::sync::LazyLock;

static ERROR_TOO_MANY_CHUNKS: LazyLock<Dynamic2CommandExceptionType<i32, i64>> =
    LazyLock::new(|| {
        Dynamic2CommandExceptionType::new(|max: i32, amount: i64| {
            message(translatable!("commands.forceload.toobig", max, amount))
        })
    });
static ERROR_NOT_TICKING: LazyLock<Dynamic2CommandExceptionType<String, String>> =
    LazyLock::new(|| {
        Dynamic2CommandExceptionType::new(|pos: String, dimension: String| {
            message(translatable!(
                "commands.forceload.query.failure",
                pos,
                dimension
            ))
        })
    });
static ERROR_ALL_ADDED: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("commands.forceload.added.failure"));
static ERROR_NONE_REMOVED: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("commands.forceload.removed.failure"));

register_command!("ForceLoadCommand", |ctx| {
    ctx.dispatcher.register(
        literal("forceload")
            .requires(has_permission(LEVEL_GAMEMASTERS))
            .then(
                literal("add").then(
                    argument("from", ColumnPosArgument::column_pos())
                        .executes(|c| {
                            let from = ColumnPosArgument::get_column_pos(c, "from");
                            change_force_load(c.get_source(), from, from, true)
                        })
                        .then(
                            argument("to", ColumnPosArgument::column_pos()).executes(|c| {
                                change_force_load(
                                    c.get_source(),
                                    ColumnPosArgument::get_column_pos(c, "from"),
                                    ColumnPosArgument::get_column_pos(c, "to"),
                                    true,
                                )
                            }),
                        ),
                ),
            )
            .then(
                literal("remove")
                    .then(
                        argument("from", ColumnPosArgument::column_pos())
                            .executes(|c| {
                                let from = ColumnPosArgument::get_column_pos(c, "from");
                                change_force_load(c.get_source(), from, from, false)
                            })
                            .then(
                                argument("to", ColumnPosArgument::column_pos()).executes(|c| {
                                    change_force_load(
                                        c.get_source(),
                                        ColumnPosArgument::get_column_pos(c, "from"),
                                        ColumnPosArgument::get_column_pos(c, "to"),
                                        false,
                                    )
                                }),
                            ),
                    )
                    .then(literal("all").executes(|c| Ok(remove_all(c.get_source())))),
            )
            .then(
                literal("query")
                    .executes(|c| Ok(list_force_load(c.get_source())))
                    .then(
                        argument("pos", ColumnPosArgument::column_pos()).executes(|c| {
                            query_force_load(
                                c.get_source(),
                                ColumnPosArgument::get_column_pos(c, "pos"),
                            )
                        }),
                    ),
            ),
    );
});

fn query_force_load(
    source: &CommandSourceStack,
    pos: ColumnPos,
) -> Result<i32, CommandSyntaxException> {
    let chunk_pos = ChunkPos::new(pos.x >> 4, pos.z >> 4);
    let dimension = source.get_level().clone();
    let result = source.with_server(|server| {
        server.level(&dimension).is_some_and(|l| {
            l.chunk_source
                .get_force_loaded_chunks()
                .contains(chunk_pos.pack())
        })
    });
    if result {
        source.send_success(
            || {
                translatable!(
                    "commands.forceload.query.success",
                    Component::literal(chunk_pos.to_string()),
                    Component::literal(dimension.to_string())
                )
            },
            false,
        );
        Ok(1)
    } else {
        Err(ERROR_NOT_TICKING.create(chunk_pos.to_string(), dimension.to_string()))
    }
}

fn list_force_load(source: &CommandSourceStack) -> i32 {
    let dimension = source.get_level().clone();
    let mut forced: Vec<i64> = source.with_server(|server| {
        server
            .level(&dimension)
            .map(|l| l.chunk_source.get_force_loaded_chunks().iter().collect())
            .unwrap_or_default()
    });
    let chunk_count = forced.len() as i32;
    if chunk_count > 0 {
        forced.sort_unstable();
        let chunk_list = forced
            .iter()
            .map(|&key| ChunkPos::unpack(key).to_string())
            .collect::<Vec<_>>()
            .join(", ");
        if chunk_count == 1 {
            source.send_success(
                || {
                    translatable!(
                        "commands.forceload.list.single",
                        Component::literal(dimension.to_string()),
                        chunk_list.clone()
                    )
                },
                false,
            );
        } else {
            source.send_success(
                || {
                    translatable!(
                        "commands.forceload.list.multiple",
                        chunk_count,
                        Component::literal(dimension.to_string()),
                        chunk_list.clone()
                    )
                },
                false,
            );
        }
    } else {
        source.send_failure(translatable!(
            "commands.forceload.added.none",
            Component::literal(dimension.to_string())
        ));
    }
    chunk_count
}

fn remove_all(source: &CommandSourceStack) -> i32 {
    let dimension = source.get_level().clone();
    // `TicketStorage.updateForcedChunks` swaps the set for a fresh one on every removal, so vanilla's `forEach` runs
    // over the complete old set: a snapshot of the keys.
    source.with_server(|server| {
        if let Some(level) = server.level_mut(&dimension) {
            let forced: Vec<i64> = level
                .chunk_source
                .get_force_loaded_chunks()
                .iter()
                .collect();
            for chunk in forced {
                level.set_chunk_forced(ChunkPos::get_x(chunk), ChunkPos::get_z(chunk), false);
            }
        }
    });
    source.send_success(
        || {
            translatable!(
                "commands.forceload.removed.all",
                Component::literal(dimension.to_string())
            )
        },
        true,
    );
    0
}

fn change_force_load(
    source: &CommandSourceStack,
    from: ColumnPos,
    to: ColumnPos,
    add: bool,
) -> Result<i32, CommandSyntaxException> {
    let (min_x, min_z) = (from.x.min(to.x), from.z.min(to.z));
    let (max_x, max_z) = (from.x.max(to.x), from.z.max(to.z));
    if !(min_x >= -30_000_000 && min_z >= -30_000_000 && max_x < 30_000_000 && max_z < 30_000_000) {
        return Err(ERROR_OUT_OF_WORLD.create());
    }
    let (min_chunk_x, min_chunk_z, max_chunk_x, max_chunk_z) =
        (min_x >> 4, min_z >> 4, max_x >> 4, max_z >> 4);
    let chunk_count =
        (max_chunk_x - min_chunk_x + 1) as i64 * (max_chunk_z - min_chunk_z + 1) as i64;
    if chunk_count > 256 {
        return Err(ERROR_TOO_MANY_CHUNKS.create(256, chunk_count));
    }
    let dimension = source.get_level().clone();
    let mut tracker = CommandResponseTracker::<ChunkPos>::create();
    source.with_server(|server| {
        let Some(level) = server.level_mut(&dimension) else {
            return;
        };
        for x in min_chunk_x..=max_chunk_x {
            for z in min_chunk_z..=max_chunk_z {
                tracker.track_bool(&ChunkPos::new(x, z), level.set_chunk_forced(x, z, add));
            }
        }
    });
    let dimension_arg = || Component::literal(dimension.to_string());
    let (min, max) = (
        ChunkPos::new(min_chunk_x, min_chunk_z),
        ChunkPos::new(max_chunk_x, max_chunk_z),
    );
    tracker.send_feedback(
        source,
        true,
        ElementType::NonZero,
        Some(|| {
            if add {
                ERROR_ALL_ADDED.create()
            } else {
                ERROR_NONE_REMOVED.create()
            }
        }),
        |chunk_pos, _| {
            translatable!(
                if add {
                    "commands.forceload.added.single"
                } else {
                    "commands.forceload.removed.single"
                },
                Component::literal(chunk_pos.to_string()),
                dimension_arg()
            )
        },
        |chunk_count, _| {
            translatable!(
                if add {
                    "commands.forceload.added.multiple"
                } else {
                    "commands.forceload.removed.multiple"
                },
                chunk_count,
                dimension_arg(),
                Component::literal(min.to_string()),
                Component::literal(max.to_string())
            )
        },
    )
}
