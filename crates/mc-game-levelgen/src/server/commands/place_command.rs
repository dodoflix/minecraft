//! `PlaceCommand`: `/place feature|jigsaw|structure|template`.
use mc_game_core::commands::arguments::coordinates::BlockPosArgument;
use mc_game_core::commands::arguments::resource_arguments::{ERROR_FAILED_TO_PARSE, FeatureArgument, ResourceKey, ResourceKeyArgument, ResourceOrId};
use mc_game_core::server::world_dimensions::nbt_to_json;
use mc_game_core::world::level::levelgen::registries::GenRegistries;
use mc_game_core::world::level::BlockGetter;
use mc_game_core::commands::arguments::{IdentifierArgument, TemplateMirrorArgument, TemplateRotationArgument};
use mc_game_core::commands::message::{message, simple_translatable};
use mc_game_core::commands::shared_suggestion_provider::suggest_resource;
use mc_game_core::commands::{CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, literal};
use mc_game_core::server::level::ServerLevel;
use mc_game_core::server::level::server_level_worldgen::with_worldgen_level;
use mc_game_core::world::level::levelgen::structure::generation::generate_structure;
use mc_game_core::world::level::levelgen::structure::manager::StructureManager;
use crate::world::level::levelgen::structure::pools::jigsaw_placement::generate_jigsaw;
use mc_game_core::world::level::levelgen::structure::templatesystem::block_rot_processor::BlockRotProcessor;
use mc_game_core::world::level::levelgen::structure::templatesystem::structure_place_settings::{SharedRandom, StructurePlaceSettings};
use mc_game_core::world::level::levelgen::{GenEnv, WorldGenLevel};
use mc_game_core::world::level::{LevelAccessor, LevelReader};
use mc_game_core::{register_command, translatable};
use mc_brigadier::arguments::{FloatArgumentType, IntegerArgumentType, float_arg_range, integer, integer_range};
use mc_brigadier::{ArgumentBuilder, CommandContext, CommandSyntaxException, DynamicCommandExceptionType, SimpleCommandExceptionType};
use mc_core::bounding_box::BoundingBox;
use mc_core::ident::Identifier;
use mc_core::mirror::Mirror;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_core::random::{LegacyRandomSource, RandomSource};
use mc_core::rotation::Rotation;
use mc_text::Component;
use std::sync::{Arc, LazyLock};

static ERROR_FEATURE_FAILED: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.place.feature.failed"));
static ERROR_JIGSAW_FAILED: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.place.jigsaw.failed"));
static ERROR_STRUCTURE_FAILED: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.place.structure.failed"));
static ERROR_TEMPLATE_INVALID: LazyLock<DynamicCommandExceptionType<String>> =
    LazyLock::new(|| DynamicCommandExceptionType::new(|value: String| message(translatable!("commands.place.template.invalid", value))));
static ERROR_TEMPLATE_FAILED: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.place.template.failed"));

/// `BlockPos.containing(source.getPosition())`.
fn source_pos(source: &CommandSourceStack) -> BlockPos {
    BlockPos::containing_position(&source.get_position())
}

register_command!("PlaceCommand", |ctx| {
    fn feature(c: &CommandContext<CommandSourceStack>, pos: BlockPos) -> Result<i32, CommandSyntaxException> {
        place_feature_command(c.get_source(), FeatureArgument::get_feature(c, "feature"), pos)
    }
    fn jigsaw(c: &CommandContext<CommandSourceStack>, pos: BlockPos) -> Result<i32, CommandSyntaxException> {
        place_jigsaw(
            c.get_source(),
            ResourceKeyArgument::get_structure_template_pool(c, "pool")?,
            IdentifierArgument::get_id(c, "target"),
            IntegerArgumentType::get_integer(c, "max_depth"),
            pos,
        )
    }
    fn structure(c: &CommandContext<CommandSourceStack>, pos: BlockPos) -> Result<i32, CommandSyntaxException> {
        place_structure(c.get_source(), ResourceKeyArgument::get_structure(c, "structure")?, pos)
    }
    fn template(
        c: &CommandContext<CommandSourceStack>,
        rotation: Rotation,
        mirror: Mirror,
        integrity: f32,
        seed: i32,
        strict: bool,
    ) -> Result<i32, CommandSyntaxException> {
        place_template(
            c.get_source(),
            &IdentifierArgument::get_id(c, "template"),
            BlockPosArgument::get_loaded_block_pos(c, "pos")?,
            rotation,
            mirror,
            integrity,
            seed,
            strict,
        )
    }
    fn template_at_source(c: &CommandContext<CommandSourceStack>) -> Result<i32, CommandSyntaxException> {
        place_template(
            c.get_source(),
            &IdentifierArgument::get_id(c, "template"),
            source_pos(c.get_source()),
            Rotation::None,
            Mirror::None,
            1.0,
            0,
            false,
        )
    }
    fn rotation(c: &CommandContext<CommandSourceStack>) -> Rotation {
        TemplateRotationArgument::get(c, "rotation")
    }
    fn mirror(c: &CommandContext<CommandSourceStack>) -> Mirror {
        TemplateMirrorArgument::get(c, "mirror")
    }
    fn integrity(c: &CommandContext<CommandSourceStack>) -> f32 {
        FloatArgumentType::get_float(c, "integrity")
    }
    fn seed(c: &CommandContext<CommandSourceStack>) -> i32 {
        IntegerArgumentType::get_integer(c, "seed")
    }

    ctx.dispatcher.register(
        literal("place")
            .requires(has_permission(LEVEL_GAMEMASTERS))
            .then(
                literal("feature").then(
                    argument("feature", FeatureArgument::feature(ctx.build_context))
                        .executes(|c| feature(c, source_pos(c.get_source())))
                        .then(argument("pos", BlockPosArgument::block_pos()).executes(|c| feature(c, BlockPosArgument::get_loaded_block_pos(c, "pos")?))),
                ),
            )
            .then(
                literal("jigsaw").then(
                    argument("pool", ResourceKeyArgument::key("worldgen/template_pool")).then(
                        argument("target", IdentifierArgument::id()).then(
                            argument("max_depth", integer_range(1, 20))
                                .executes(|c| jigsaw(c, source_pos(c.get_source())))
                                .then(argument("position", BlockPosArgument::block_pos()).executes(|c| jigsaw(c, BlockPosArgument::get_loaded_block_pos(c, "position")?))),
                        ),
                    ),
                ),
            )
            .then(
                literal("structure").then(
                    argument("structure", ResourceKeyArgument::key("worldgen/structure"))
                        .executes(|c| structure(c, source_pos(c.get_source())))
                        .then(argument("pos", BlockPosArgument::block_pos()).executes(|c| structure(c, BlockPosArgument::get_loaded_block_pos(c, "pos")?))),
                ),
            )
            .then(
                literal("template").then(
                    argument("template", IdentifierArgument::id())
                        .suggests(|c, builder| {
                            let templates = c.get_source().with_server(|server| {
                                let level = c.get_source().get_level();
                                server.level(level).map(|l| l.structure_template_manager().list_templates()).unwrap_or_default()
                            });
                            Ok(suggest_resource(templates, builder))
                        })
                        .executes(template_at_source)
                        .then(
                            argument("pos", BlockPosArgument::block_pos())
                                .executes(|c| template(c, Rotation::None, Mirror::None, 1.0, 0, false))
                                .then(
                                    argument("rotation", TemplateRotationArgument)
                                        .executes(|c| template(c, rotation(c), Mirror::None, 1.0, 0, false))
                                        .then(
                                            argument("mirror", TemplateMirrorArgument)
                                                .executes(|c| template(c, rotation(c), mirror(c), 1.0, 0, false))
                                                .then(
                                                    argument("integrity", float_arg_range(0.0, 1.0))
                                                        .executes(|c| template(c, rotation(c), mirror(c), integrity(c), 0, false))
                                                        .then(
                                                            argument("seed", integer())
                                                                .executes(|c| template(c, rotation(c), mirror(c), integrity(c), seed(c), false))
                                                                .then(literal("strict").executes(|c| template(c, rotation(c), mirror(c), integrity(c), seed(c), true))),
                                                        ),
                                                ),
                                        ),
                                ),
                        ),
                ),
            ),
    );
});

/// `checkLoaded(level, chunkMin, chunkMax)`.
fn check_loaded(level: &ServerLevel, chunk_min: ChunkPos, chunk_max: ChunkPos) -> Result<(), CommandSyntaxException> {
    if ChunkPos::range_closed_between(&chunk_min, &chunk_max).any(|c| !level.is_loaded(c.get_world_position())) {
        return Err(mc_game_core::commands::arguments::coordinates::block_pos_argument::ERROR_NOT_LOADED.create());
    }
    Ok(())
}

/// Runs `f` on the source's level with the worldgen clock inputs; the level is always there when a command runs.
fn with_level<R>(source: &CommandSourceStack, f: impl FnOnce(&mut ServerLevel) -> R) -> R {
    source.with_server(|server| with_worldgen_level(server, source.get_level(), f)).expect("the level of a running command is loaded")
}

/// `PlaceCommand.placeFeature`.
pub fn place_feature_command(source: &CommandSourceStack, feature: ResourceOrId, pos: BlockPos) -> Result<i32, CommandSyntaxException> {
    let name = with_level(source, |level| {
        let env = level.gen_env().clone();
        let holder = match &feature {
            ResourceOrId::Reference(key) => env.registries.feature(&key.id).expect("the argument checked that the feature exists").clone(),
            ResourceOrId::Inline(tag) => GenRegistries::decode_inline_feature(level.worldgen_data.as_ref().expect("level has worldgen data"), &env.biomes, &nbt_to_json(tag))
                .map_err(|e| ERROR_FAILED_TO_PARSE.create(e.to_string()))?,
        };
        let chunk = ChunkPos::containing(&pos);
        check_loaded(level, ChunkPos::new(chunk.x - 1, chunk.z - 1), ChunkPos::new(chunk.x + 1, chunk.z + 1))?;
        if !level.place_feature_value(holder.value().as_ref(), pos) {
            return Err(ERROR_FEATURE_FAILED.create());
        }
        Ok(holder.id().map(ToString::to_string))
    })?;
    match name {
        Some(id) => source.send_success(|| translatable!("commands.place.feature.success", id, pos.x(), pos.y(), pos.z()), true),
        None => source.send_success(|| translatable!("commands.place.feature.success.inline", pos.x(), pos.y(), pos.z()), true),
    }
    Ok(1)
}

/// `PlaceCommand.placeJigsaw`.
pub fn place_jigsaw(source: &CommandSourceStack, pool: ResourceKey, target: Identifier, max_depth: i32, pos: BlockPos) -> Result<i32, CommandSyntaxException> {
    let placed = with_level(source, |level| {
        let chunk = ChunkPos::containing(&pos);
        check_loaded(level, chunk, chunk)?;
        let env = level.gen_env().clone();
        let pool = env.structures.template_pools.get(&pool.id).expect("the argument checked that the pool exists").clone();
        let mut random = level.tick_state.random.clone();
        Ok(generate_jigsaw(level, &mut random, &pool, &target, max_depth, pos, false))
    })?;
    if !placed {
        return Err(ERROR_JIGSAW_FAILED.create());
    }
    source.send_success(|| translatable!("commands.place.jigsaw.success", pos.x(), pos.y(), pos.z()), true);
    Ok(1)
}

/// `PlaceCommand.placeStructure`.
pub fn place_structure(source: &CommandSourceStack, structure: ResourceKey, pos: BlockPos) -> Result<i32, CommandSyntaxException> {
    let placed = with_level(source, |level| {
        let env: Arc<GenEnv> = level.gen_env().clone();
        let holder = env.structures.structures.get(&structure.id).expect("the argument checked that the structure exists").clone();
        let mut start = generate_structure(&env, &holder, level.get_seed(), ChunkPos::containing(&pos), 0, level.level_height(), &|_| true);
        if !start.is_valid() {
            return Err(ERROR_STRUCTURE_FAILED.create());
        }
        let bounding_box = start.get_bounding_box();
        let chunk_min = ChunkPos::new(bounding_box.min_x() >> 4, bounding_box.min_z() >> 4);
        let chunk_max = ChunkPos::new(bounding_box.max_x() >> 4, bounding_box.max_z() >> 4);
        check_loaded(level, chunk_min, chunk_max)?;
        level.ensure_chunks_loaded(ChunkPos::new(chunk_min.x - 1, chunk_min.z - 1), ChunkPos::new(chunk_max.x + 1, chunk_max.z + 1));
        let manager = StructureManager::new(&env);
        let (min_y, max_y) = (level.get_min_y(), level.get_max_y());
        let mut random = level.tick_state.random.clone();
        for c in ChunkPos::range_closed_between(&chunk_min, &chunk_max) {
            let chunk_bb = BoundingBox::new(c.get_min_block_x(), min_y, c.get_min_block_z(), c.get_max_block_x(), max_y + 1, c.get_max_block_z());
            start.place_in_chunk(level, &manager, &env, &mut random, &chunk_bb, c);
        }
        Ok(())
    });
    placed?;
    source.send_success(|| translatable!("commands.place.structure.success", structure.id.to_string(), pos.x(), pos.y(), pos.z()), true);
    Ok(1)
}

/// `StructureBlockEntity.createRandom(seed)`.
fn create_random(seed: i64) -> Box<dyn RandomSource> {
    Box::new(LegacyRandomSource::new(if seed == 0 { mc_game_core::util::util::get_millis() } else { seed }))
}

/// `PlaceCommand.placeTemplate`.
#[allow(clippy::too_many_arguments)]
pub fn place_template(
    source: &CommandSourceStack,
    template: &Identifier,
    pos: BlockPos,
    rotation: Rotation,
    mirror: Mirror,
    integrity: f32,
    seed: i32,
    strict: bool,
) -> Result<i32, CommandSyntaxException> {
    let placed = with_level(source, |level| {
        let Some(structure_template) = level.structure_template_manager().get(template) else {
            return Err(ERROR_TEMPLATE_INVALID.create(template.to_string()));
        };
        let (chunk_min, chunk_max) = (ChunkPos::containing(&pos), ChunkPos::containing(&pos.offset_vec(&structure_template.get_size())));
        check_loaded(level, chunk_min, chunk_max)?;
        level.ensure_chunks_loaded(ChunkPos::new(chunk_min.x - 1, chunk_min.z - 1), ChunkPos::new(chunk_max.x + 1, chunk_max.z + 1));
        let mut settings = StructurePlaceSettings::new();
        settings.set_mirror(mirror).set_rotation(rotation).set_known_shape(strict);
        if integrity < 1.0 {
            settings.clear_processors().add_processor(Arc::new(BlockRotProcessor::new(None, integrity))).set_random(Some(SharedRandom::new(create_random(i64::from(seed)))));
        }
        let mut random = create_random(i64::from(seed));
        Ok(structure_template.place_in_world(level, pos, pos, &settings, &mut *random, 2 | if strict { 816 } else { 0 }))
    })?;
    if !placed {
        return Err(ERROR_TEMPLATE_FAILED.create());
    }
    source.send_success(|| translatable!("commands.place.template.success", Component::literal(template.to_string()), pos.x(), pos.y(), pos.z()), true);
    Ok(1)
}
