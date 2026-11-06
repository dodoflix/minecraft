//! `FillCommand`.
use mc_brigadier::{
    ArgumentBuilder, CommandContext, CommandSyntaxException, Dynamic2CommandExceptionType,
    SimpleCommandExceptionType,
};
use mc_core::bounding_box::BoundingBox;
use mc_core::pos::BlockPos;
use mc_data::block::Block;
use mc_game_core::commands::arguments::blocks::{
    BlockInput, BlockPredicate, BlockPredicateArgument, BlockStateArgument,
};
use mc_game_core::commands::arguments::coordinates::BlockPosArgument;
use mc_game_core::commands::message::{message, simple_translatable};
use mc_game_core::commands::{
    CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, literal,
};
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::level::gamerules::GameRules;
use mc_game_core::world::level::{BlockGetter, LevelAccessor, LevelReader};
use mc_game_core::{register_command, translatable};
use std::sync::LazyLock;

static ERROR_AREA_TOO_LARGE: LazyLock<Dynamic2CommandExceptionType<i32, i64>> =
    LazyLock::new(|| {
        Dynamic2CommandExceptionType::new(|max: i32, count: i64| {
            message(translatable!("commands.fill.toobig", max, count))
        })
    });
static ERROR_FAILED: LazyLock<SimpleCommandExceptionType> =
    LazyLock::new(|| simple_translatable("commands.fill.failed"));

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Replace,
    Outline,
    Hollow,
    Destroy,
}

/// The `Predicate<BlockInWorld>` of a fill.
#[derive(Clone)]
enum Filter {
    /// `null`.
    Any,
    /// `b -> b.getLevel().isEmptyBlock(b.getPos())`.
    Keep,
    Predicate(BlockPredicate),
}

type PosGetter =
    fn(&CommandContext<CommandSourceStack>) -> Result<BlockPos, CommandSyntaxException>;
type BlockGetterFn = fn(&CommandContext<CommandSourceStack>) -> BlockInput;
type FilterGetter =
    fn(&CommandContext<CommandSourceStack>) -> Result<Filter, CommandSyntaxException>;

fn from_pos(c: &CommandContext<CommandSourceStack>) -> Result<BlockPos, CommandSyntaxException> {
    BlockPosArgument::get_loaded_block_pos(c, "from")
}

fn to_pos(c: &CommandContext<CommandSourceStack>) -> Result<BlockPos, CommandSyntaxException> {
    BlockPosArgument::get_loaded_block_pos(c, "to")
}

fn block_of(c: &CommandContext<CommandSourceStack>) -> BlockInput {
    BlockStateArgument::get_block(c, "block")
}

register_command!("FillCommand", |ctx| {
    ctx.dispatcher.register(
        literal("fill")
            .requires(has_permission(LEVEL_GAMEMASTERS))
            .then(
                argument("from", BlockPosArgument::block_pos()).then(
                    argument("to", BlockPosArgument::block_pos()).then(
                        wrap_with_mode(
                            argument("block", BlockStateArgument::block(ctx.build_context)),
                            from_pos,
                            to_pos,
                            block_of,
                            |_| Ok(Filter::Any),
                        )
                        .then(
                            literal("replace")
                                .executes(|c| fill_from_args(c, Mode::Replace, Filter::Any, false))
                                .then(wrap_with_mode(
                                    argument(
                                        "filter",
                                        BlockPredicateArgument::block_predicate(ctx.build_context),
                                    ),
                                    from_pos,
                                    to_pos,
                                    block_of,
                                    |c| {
                                        Ok(Filter::Predicate(
                                            BlockPredicateArgument::get_block_predicate(
                                                c, "filter",
                                            ),
                                        ))
                                    },
                                )),
                        )
                        .then(
                            literal("keep").executes(|c| {
                                fill_from_args(c, Mode::Replace, Filter::Keep, false)
                            }),
                        ),
                    ),
                ),
            ),
    );
});

fn fill_from_args(
    c: &CommandContext<CommandSourceStack>,
    mode: Mode,
    filter: Filter,
    strict: bool,
) -> Result<i32, CommandSyntaxException> {
    fill_blocks(
        c.get_source(),
        BoundingBox::from_corners(&*from_pos(c)?, &*to_pos(c)?),
        &block_of(c),
        mode,
        &filter,
        strict,
    )
}

fn wrap_with_mode<B: ArgumentBuilder<CommandSourceStack>>(
    builder: B,
    from: PosGetter,
    to: PosGetter,
    block: BlockGetterFn,
    filter: FilterGetter,
) -> B {
    let run = move |mode: Mode, strict: bool| {
        move |c: &CommandContext<CommandSourceStack>| {
            fill_blocks(
                c.get_source(),
                BoundingBox::from_corners(&*from(c)?, &*to(c)?),
                &block(c),
                mode,
                &filter(c)?,
                strict,
            )
        }
    };
    builder
        .executes(run(Mode::Replace, false))
        .then(literal("outline").executes(run(Mode::Outline, false)))
        .then(literal("hollow").executes(run(Mode::Hollow, false)))
        .then(literal("destroy").executes(run(Mode::Destroy, false)))
        .then(literal("strict").executes(run(Mode::Replace, true)))
}

/// Block access in vanilla loads the chunk of every position it touches (`Level.getChunk`); the region's chunks are
/// loaded up front, in the order the iteration would first touch them.
pub(super) fn load_chunks_covering(
    level: &mut ServerLevel,
    min_x: i32,
    min_y: i32,
    min_z: i32,
    max_x: i32,
    max_y: i32,
    max_z: i32,
) {
    let height = level.level_height();
    if max_y < height.min_y || min_y > height.max_y() {
        return;
    }
    for chunk_z in (min_z >> 4)..=(max_z >> 4) {
        for chunk_x in (min_x >> 4)..=(max_x >> 4) {
            if level.chunk_now(chunk_x, chunk_z).is_none() {
                level.chunk_blocking_mut(chunk_x, chunk_z);
            }
        }
    }
}

fn fill_blocks(
    source: &CommandSourceStack,
    region: BoundingBox,
    target: &BlockInput,
    mode: Mode,
    filter: &Filter,
    strict: bool,
) -> Result<i32, CommandSyntaxException> {
    let area = region.get_x_span() as i64 * region.get_y_span() as i64 * region.get_z_span() as i64;
    let limit = source.with_server(|server| {
        server
            .game_rules_ref()
            .get(GameRules::MAX_BLOCK_MODIFICATIONS)
    });
    if area > limit as i64 {
        return Err(ERROR_AREA_TOO_LARGE.create(limit, area));
    }
    let hollow_core = BlockInput::new(Block::AIR.default_state(), Vec::new(), None);
    let count = source.with_server(|server| {
        let level = server.level_mut(source.get_level())?;
        if level.is_debug {
            return None;
        }
        load_chunks_covering(
            level,
            region.min_x(),
            region.min_y(),
            region.min_z(),
            region.max_x(),
            region.max_y(),
            region.max_z(),
        );
        let mut update_positions: Vec<(BlockPos, mc_data::block::BlockState)> = Vec::new();
        let mut count = 0;
        for pos in BlockPos::between_closed_xyz(
            region.min_x(),
            region.min_y(),
            region.min_z(),
            region.max_x(),
            region.max_y(),
            region.max_z(),
        ) {
            let matches = match filter {
                Filter::Any => true,
                Filter::Keep => level.is_empty_block(pos),
                Filter::Predicate(predicate) => predicate.test(level, pos),
            };
            if !matches {
                continue;
            }
            let old_state = level.get_block_state(pos);
            let mut affected = false;
            if mode == Mode::Destroy && level.destroy_block(pos, true, None, 512) {
                affected = true;
            }
            let on_edge = pos.x() == region.min_x()
                || pos.x() == region.max_x()
                || pos.y() == region.min_y()
                || pos.y() == region.max_y()
                || pos.z() == region.min_z()
                || pos.z() == region.max_z();
            let block = match mode {
                Mode::Outline => on_edge.then_some(target),
                Mode::Hollow => Some(if on_edge { target } else { &hollow_core }),
                Mode::Replace | Mode::Destroy => Some(target),
            };
            match block {
                None => {
                    if affected {
                        count += 1;
                    }
                }
                Some(block) => {
                    if !block.place(level, pos, 2 | if strict { 816 } else { 256 }) {
                        if affected {
                            count += 1;
                        }
                    } else {
                        if !strict {
                            update_positions.push((pos, old_state));
                        }
                        count += 1;
                    }
                }
            }
        }
        for (pos, old_state) in update_positions {
            level.update_neighbours_on_block_set(pos, old_state);
        }
        Some(count)
    });
    match count {
        Some(count) if count != 0 => {
            source.send_success(|| translatable!("commands.fill.success", count), true);
            Ok(count)
        }
        _ => Err(ERROR_FAILED.create()),
    }
}
