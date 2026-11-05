//! `CloneCommands`.
use crate::commands::arguments::DimensionArgument;
use crate::commands::arguments::blocks::{BlockPredicate, BlockPredicateArgument};
use crate::commands::arguments::coordinates::BlockPosArgument;
use crate::commands::arguments::coordinates::block_pos_argument::ERROR_NOT_LOADED;
use crate::commands::message::{message, simple_translatable};
use crate::commands::{CommandBuildContext, CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, literal};
use crate::server::level::ServerLevel;
use crate::server::level::level_chunk_ops::EntityCreationType;
use crate::world::level::gamerules::GameRules;
use crate::world::level::{BlockGetter, DimensionKey, LevelAccessor};
use crate::world::level::block::state::BlockStateExt;
use crate::{register_command, translatable};
use mc_brigadier::{ArgumentBuilder, CommandContext, CommandSyntaxException, Dynamic2CommandExceptionType, RequiredArgumentBuilder, SimpleCommandExceptionType};
use mc_core::bounding_box::BoundingBox;
use mc_core::pos::BlockPos;
use mc_data::block::{Block, BlockState};
use mc_nbt::CompoundTag;
use mc_protocol::components::DataComponentMap;
use std::collections::VecDeque;
use std::sync::{Arc, LazyLock};

static ERROR_OVERLAP: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.clone.overlap"));
static ERROR_AREA_TOO_LARGE: LazyLock<Dynamic2CommandExceptionType<i32, i64>> =
    LazyLock::new(|| Dynamic2CommandExceptionType::new(|max: i32, count: i64| message(translatable!("commands.clone.toobig", max, count))));
static ERROR_FAILED: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.clone.failed"));

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Force,
    Move,
    Normal,
}

impl Mode {
    fn can_overlap(self) -> bool {
        self != Mode::Normal
    }
}

/// The `Predicate<BlockInWorld>` of a clone.
#[derive(Clone)]
enum Filter {
    /// `b -> true`.
    All,
    /// `FILTER_AIR`.
    NotAir,
    Predicate(BlockPredicate),
}

struct DimensionAndPosition {
    dimension: DimensionKey,
    position: BlockPos,
}

type DimensionGetter = fn(&CommandContext<CommandSourceStack>) -> Result<DimensionKey, CommandSyntaxException>;
type PositionGetter = Arc<dyn Fn(&CommandContext<CommandSourceStack>) -> Result<DimensionAndPosition, CommandSyntaxException> + Send + Sync>;
type FilterGetter = Arc<dyn Fn(&CommandContext<CommandSourceStack>) -> Filter + Send + Sync>;

fn source_level(c: &CommandContext<CommandSourceStack>) -> Result<DimensionKey, CommandSyntaxException> {
    Ok(c.get_source().get_level().clone())
}

fn source_dimension_argument(c: &CommandContext<CommandSourceStack>) -> Result<DimensionKey, CommandSyntaxException> {
    DimensionArgument::get_dimension(c, "sourceDimension")
}

fn target_dimension_argument(c: &CommandContext<CommandSourceStack>) -> Result<DimensionKey, CommandSyntaxException> {
    DimensionArgument::get_dimension(c, "targetDimension")
}

register_command!("CloneCommands", |ctx| {
    ctx.dispatcher.register(
        literal("clone")
            .requires(has_permission(LEVEL_GAMEMASTERS))
            .then(begin_end_destination_and_mode_suffix(ctx.build_context, source_level))
            .then(literal("from").then(
                argument("sourceDimension", DimensionArgument::dimension()).then(begin_end_destination_and_mode_suffix(ctx.build_context, source_dimension_argument)),
            )),
    );
});

fn begin_end_destination_and_mode_suffix(context: &CommandBuildContext, from_dimension: DimensionGetter) -> RequiredArgumentBuilder<CommandSourceStack> {
    argument("begin", BlockPosArgument::block_pos()).then(
        argument("end", BlockPosArgument::block_pos())
            .then(destination_and_strict_suffix(context, from_dimension, source_level))
            .then(literal("to").then(
                argument("targetDimension", DimensionArgument::dimension()).then(destination_and_strict_suffix(context, from_dimension, target_dimension_argument)),
            )),
    )
}

fn loaded_dimension_and_position(
    c: &CommandContext<CommandSourceStack>,
    dimension: DimensionKey,
    position_argument: &str,
) -> Result<DimensionAndPosition, CommandSyntaxException> {
    let position = BlockPosArgument::get_loaded_block_pos_in(c, &dimension, position_argument)?;
    Ok(DimensionAndPosition { dimension, position })
}

fn destination_and_strict_suffix(context: &CommandBuildContext, from_dimension: DimensionGetter, to_dimension: DimensionGetter) -> RequiredArgumentBuilder<CommandSourceStack> {
    let begin_pos: PositionGetter = Arc::new(move |c| loaded_dimension_and_position(c, from_dimension(c)?, "begin"));
    let end_pos: PositionGetter = Arc::new(move |c| loaded_dimension_and_position(c, from_dimension(c)?, "end"));
    let destination_pos: PositionGetter = Arc::new(move |c| loaded_dimension_and_position(c, to_dimension(c)?, "destination"));
    mode_suffix(context, &begin_pos, &end_pos, &destination_pos, false, argument("destination", BlockPosArgument::block_pos()))
        .then(mode_suffix(context, &begin_pos, &end_pos, &destination_pos, true, literal("strict")))
}

fn mode_suffix<B: ArgumentBuilder<CommandSourceStack>>(
    context: &CommandBuildContext,
    begin_pos: &PositionGetter,
    end_pos: &PositionGetter,
    destination_pos: &PositionGetter,
    strict: bool,
    builder: B,
) -> B {
    let all: FilterGetter = Arc::new(|_| Filter::All);
    let not_air: FilterGetter = Arc::new(|_| Filter::NotAir);
    let predicate: FilterGetter = Arc::new(|c| Filter::Predicate(BlockPredicateArgument::get_block_predicate(c, "filter")));
    let (b, e, d) = (begin_pos.clone(), end_pos.clone(), destination_pos.clone());
    builder
        .executes(move |c| clone(c.get_source(), b(c)?, e(c)?, d(c)?, &Filter::All, Mode::Normal, strict))
        .then(wrap_with_clone_mode(begin_pos, end_pos, destination_pos, &all, strict, literal("replace")))
        .then(wrap_with_clone_mode(begin_pos, end_pos, destination_pos, &not_air, strict, literal("masked")))
        .then(literal("filtered").then(wrap_with_clone_mode(
            begin_pos,
            end_pos,
            destination_pos,
            &predicate,
            strict,
            argument("filter", BlockPredicateArgument::block_predicate(context)),
        )))
}

fn wrap_with_clone_mode<B: ArgumentBuilder<CommandSourceStack>>(
    begin_pos: &PositionGetter,
    end_pos: &PositionGetter,
    destination_pos: &PositionGetter,
    filter: &FilterGetter,
    strict: bool,
    builder: B,
) -> B {
    let run = |mode: Mode| {
        let (b, e, d, f) = (begin_pos.clone(), end_pos.clone(), destination_pos.clone(), filter.clone());
        move |c: &CommandContext<CommandSourceStack>| clone(c.get_source(), b(c)?, e(c)?, d(c)?, &f(c), mode, strict)
    };
    builder
        .executes(run(Mode::Normal))
        .then(literal("force").executes(run(Mode::Force)))
        .then(literal("move").executes(run(Mode::Move)))
        .then(literal("normal").executes(run(Mode::Normal)))
}

/// `LevelReader.hasChunksAt(pos0, pos1)` (the corners are used as given, not sorted).
fn has_chunks_at(level: &ServerLevel, p0: BlockPos, p1: BlockPos) -> bool {
    use crate::world::level::LevelReader;
    let height = level.level_height();
    if !(p1.y() >= height.min_y && p0.y() <= height.max_y()) {
        return false;
    }
    ((p0.x() >> 4)..=(p1.x() >> 4)).all(|cx| ((p0.z() >> 4)..=(p1.z() >> 4)).all(|cz| level.has_chunk(cx, cz)))
}

struct CloneBlockInfo {
    pos: BlockPos,
    state: BlockState,
    block_entity_info: Option<(CompoundTag, DataComponentMap)>,
    previous_state_at_destination: BlockState,
}

fn clone(
    source: &CommandSourceStack,
    start: DimensionAndPosition,
    end: DimensionAndPosition,
    dest: DimensionAndPosition,
    filter: &Filter,
    mode: Mode,
    strict: bool,
) -> Result<i32, CommandSyntaxException> {
    let (start_pos, end_pos) = (start.position, end.position);
    let from = BoundingBox::from_corners(&start_pos, &end_pos);
    let dest_pos = dest.position;
    let dest_end_pos = dest_pos.offset_vec(&from.get_length());
    let destination = BoundingBox::from_corners(&dest_pos, &dest_end_pos);
    let (from_dimension, to_dimension) = (start.dimension, dest.dimension);
    let same_level = from_dimension == to_dimension;
    if !mode.can_overlap() && same_level && destination.intersects(&from) {
        return Err(ERROR_OVERLAP.create());
    }
    let area = from.get_x_span() as i64 * from.get_y_span() as i64 * from.get_z_span() as i64;
    let limit = source.with_server(|server| server.game_rules_ref().get(GameRules::MAX_BLOCK_MODIFICATIONS));
    if area > limit as i64 {
        return Err(ERROR_AREA_TOO_LARGE.create(limit, area));
    }
    let outcome = source.with_server(|server| {
        let from_loaded = server.level(&from_dimension).is_some_and(|l| has_chunks_at(l, start_pos, end_pos));
        let to_loaded = server.level(&to_dimension).is_some_and(|l| has_chunks_at(l, dest_pos, dest_end_pos));
        if !from_loaded || !to_loaded {
            return Err(ERROR_NOT_LOADED.create());
        }
        if server.level(&to_dimension).is_some_and(|l| l.is_debug) {
            return Err(ERROR_FAILED.create());
        }
        macro_rules! level {
            ($key:expr) => {
                server.level_mut(&$key).expect("dimension checked by the argument")
            };
        }
        let mut solid_list: Vec<CloneBlockInfo> = Vec::new();
        let mut block_entities_list: Vec<CloneBlockInfo> = Vec::new();
        let mut other_blocks_list: Vec<CloneBlockInfo> = Vec::new();
        let mut clear_blocks_list: VecDeque<BlockPos> = VecDeque::new();
        let mut count = 0;
        let offset = BlockPos::new(destination.min_x() - from.min_x(), destination.min_y() - from.min_y(), destination.min_z() - from.min_z());

        for z in from.min_z()..=from.max_z() {
            for y in from.min_y()..=from.max_y() {
                for x in from.min_x()..=from.max_x() {
                    let source_pos = BlockPos::new(x, y, z);
                    let destination_pos = source_pos.offset_vec(&offset);
                    let state;
                    let matches;
                    let block_entity_info;
                    let solid;
                    {
                        let from_level = level!(from_dimension);
                        state = from_level.get_block_state(source_pos);
                        matches = match filter {
                            Filter::All => true,
                            Filter::NotAir => !state.is_air(),
                            Filter::Predicate(predicate) => predicate.test(from_level, source_pos),
                        };
                        if !matches {
                            continue;
                        }
                        from_level.get_block_entity_creating(source_pos, EntityCreationType::Check);
                        block_entity_info = from_level.get_block_entity(source_pos).map(|entity| {
                            let mut tag = CompoundTag::new();
                            entity.save_additional(&mut tag);
                            (tag, entity.base().components.clone())
                        });
                        solid = state.is_solid_render() || state.is_collision_shape_full_block_at(&*from_level, source_pos);
                    }
                    let previous = level!(to_dimension).get_block_state(destination_pos);
                    let info = |block_entity_info| CloneBlockInfo { pos: destination_pos, state, block_entity_info, previous_state_at_destination: previous };
                    if block_entity_info.is_some() {
                        block_entities_list.push(info(block_entity_info));
                        clear_blocks_list.push_back(source_pos);
                    } else if !solid {
                        other_blocks_list.push(info(None));
                        clear_blocks_list.push_front(source_pos);
                    } else {
                        solid_list.push(info(None));
                        clear_blocks_list.push_back(source_pos);
                    }
                }
            }
        }

        let default_update_flags: u32 = 2 | if strict { 816 } else { 0 };
        if mode == Mode::Move {
            let from_level = level!(from_dimension);
            for pos in &clear_blocks_list {
                from_level.set_block(*pos, Block::BARRIER.default_state(), default_update_flags | 816, 512);
            }
            let standard_update_flags = if strict { default_update_flags } else { 3 };
            for pos in &clear_blocks_list {
                from_level.set_block(*pos, Block::AIR.default_state(), standard_update_flags, 512);
            }
        }

        let block_infos: Vec<&CloneBlockInfo> = solid_list.iter().chain(&block_entities_list).chain(&other_blocks_list).collect();
        let to_level = level!(to_dimension);
        for info in block_infos.iter().rev() {
            to_level.set_block(info.pos, Block::BARRIER.default_state(), default_update_flags | 816, 512);
        }
        for info in &block_infos {
            if to_level.set_block(info.pos, info.state, default_update_flags, 512) {
                count += 1;
            }
        }
        for info in &block_entities_list {
            if let Some((tag, components)) = &info.block_entity_info
                && to_level
                    .with_block_entity(info.pos, |_, entity| {
                        entity.load_additional(tag);
                        entity.base_mut().components = components.clone();
                    })
                    .is_some()
            {
                to_level.block_entity_changed(info.pos);
            }
            to_level.set_block(info.pos, info.state, default_update_flags, 512);
        }
        if !strict {
            for info in block_infos.iter().rev() {
                to_level.update_neighbours_on_block_set(info.pos, info.previous_state_at_destination);
            }
        }

        if same_level {
            to_level.tick_state.ticks.lock().block.copy_area(&from, &offset);
        } else {
            let source_ticks = level!(from_dimension).tick_state.ticks.clone();
            let source_ticks = source_ticks.lock();
            level!(to_dimension).tick_state.ticks.lock().block.copy_area_from(&source_ticks.block, &from, &offset);
        }
        Ok(count)
    })?;
    if outcome == 0 {
        return Err(ERROR_FAILED.create());
    }
    source.send_success(|| translatable!("commands.clone.success", outcome), true);
    Ok(outcome)
}
