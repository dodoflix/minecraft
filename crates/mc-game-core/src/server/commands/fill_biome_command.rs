//! `FillBiomeCommand`.
use crate::commands::arguments::coordinates::BlockPosArgument;
use crate::commands::arguments::resource_arguments::{ResourceArgument, ResourceOrTag, ResourceOrTagArgument};
use crate::commands::message::{message, simple_translatable};
use crate::commands::{CommandSourceStack, LEVEL_GAMEMASTERS, argument, has_permission, literal};
use crate::server::level::ServerLevel;
use crate::world::level::gamerules::GameRules;
use crate::{register_command, translatable};
use mc_brigadier::{ArgumentBuilder, CommandSyntaxException, Dynamic2CommandExceptionType, SimpleCommandExceptionType};
use mc_core::bounding_box::BoundingBox;
use mc_core::pos::quart_pos;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_text::Component;
use mc_world::palette::BiomeId;
use std::sync::LazyLock;

pub static ERROR_NOT_LOADED: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("argument.pos.unloaded"));
static ERROR_VOLUME_TOO_LARGE: LazyLock<Dynamic2CommandExceptionType<i32, i64>> =
    LazyLock::new(|| Dynamic2CommandExceptionType::new(|max: i32, count: i64| message(translatable!("commands.fillbiome.toobig", max, count))));
pub static ERROR_NO_BIOMES_SET: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| simple_translatable("commands.fillbiome.no_changes"));

const BIOME_REGISTRY: &str = "worldgen/biome";

register_command!("FillBiomeCommand", |ctx| {
    ctx.dispatcher.register(
        literal("fillbiome").requires(has_permission(LEVEL_GAMEMASTERS)).then(
            argument("from", BlockPosArgument::block_pos()).then(
                argument("to", BlockPosArgument::block_pos()).then(
                    argument("biome", ResourceArgument::resource(ctx.build_context, BIOME_REGISTRY))
                        .executes(|c| {
                            fill_command(
                                c.get_source(),
                                BlockPosArgument::get_loaded_block_pos(c, "from")?,
                                BlockPosArgument::get_loaded_block_pos(c, "to")?,
                                &ResourceArgument::get_resource(c, "biome", BIOME_REGISTRY)?.id,
                                None,
                            )
                        })
                        .then(literal("replace").then(
                            argument("filter", ResourceOrTagArgument::resource_or_tag(ctx.build_context, BIOME_REGISTRY)).executes(|c| {
                                fill_command(
                                    c.get_source(),
                                    BlockPosArgument::get_loaded_block_pos(c, "from")?,
                                    BlockPosArgument::get_loaded_block_pos(c, "to")?,
                                    &ResourceArgument::get_resource(c, "biome", BIOME_REGISTRY)?.id,
                                    Some(ResourceOrTagArgument::get_resource_or_tag(c, "filter", BIOME_REGISTRY)?),
                                )
                            }),
                        )),
                ),
            ),
        ),
    );
});

/// `QuartPos.toBlock(QuartPos.fromBlock(blockCoord))`.
fn quantize(block_coord: i32) -> i32 {
    quart_pos::to_block(quart_pos::from_block(block_coord))
}

fn quantize_pos(block: BlockPos) -> BlockPos {
    BlockPos::new(quantize(block.x()), quantize(block.y()), quantize(block.z()))
}

/// `FillBiomeCommand.fill(level, rawFrom, rawTo, biome, filter, successMessageConsumer)`: the number of changed
/// biome cells and the success message (the caller sends it).
pub fn fill(
    level: &mut ServerLevel,
    raw_from: BlockPos,
    raw_to: BlockPos,
    to_fill: BiomeId,
    filter: &dyn Fn(BiomeId) -> bool,
    limit: i32,
) -> Result<(i32, Component), CommandSyntaxException> {
    let from = quantize_pos(raw_from);
    let to = quantize_pos(raw_to);
    let region = BoundingBox::from_corners(&*from, &*to);
    let volume = region.get_x_span() as i64 * region.get_y_span() as i64 * region.get_z_span() as i64;
    if volume > limit as i64 {
        return Err(ERROR_VOLUME_TOO_LARGE.create(limit, volume));
    }
    let mut chunks: Vec<ChunkPos> = Vec::new();
    for chunk_z in (region.min_z() >> 4)..=(region.max_z() >> 4) {
        for chunk_x in (region.min_x() >> 4)..=(region.max_x() >> 4) {
            if level.chunk_now(chunk_x, chunk_z).is_none() {
                return Err(ERROR_NOT_LOADED.create());
            }
            chunks.push(ChunkPos::new(chunk_x, chunk_z));
        }
    }
    let mut changed_count = 0;
    let mut changed_chunks: Vec<ChunkPos> = Vec::new();
    for pos in chunks {
        let previous_changed_count = changed_count;
        let chunk = level.chunk_now_mut(pos.x, pos.z).expect("checked above");
        let access = &mut chunk.data.access;
        let height = access.level_height;
        access.fill_biomes_from_noise_with_current(height, |quart_x, quart_y, quart_z, current| {
            let (block_x, block_y, block_z) = (quart_pos::to_block(quart_x), quart_pos::to_block(quart_y), quart_pos::to_block(quart_z));
            if region.is_inside_xyz(block_x, block_y, block_z) && filter(current) {
                if current != to_fill {
                    changed_count += 1;
                }
                to_fill
            } else {
                current
            }
        });
        if previous_changed_count != changed_count {
            access.mark_unsaved();
            changed_chunks.push(pos);
        }
    }
    if changed_count > 0 {
        level.resend_biomes_for_chunks(&changed_chunks);
        let message = translatable!(
            "commands.fillbiome.success.count",
            changed_count,
            region.min_x(),
            region.min_y(),
            region.min_z(),
            region.max_x(),
            region.max_y(),
            region.max_z()
        );
        Ok((changed_count, message))
    } else {
        Err(ERROR_NO_BIOMES_SET.create())
    }
}

fn fill_command(
    source: &CommandSourceStack,
    raw_from: BlockPos,
    raw_to: BlockPos,
    biome: &mc_core::ident::Identifier,
    filter: Option<ResourceOrTag>,
) -> Result<i32, CommandSyntaxException> {
    let result = source.with_server(|server| {
        let limit = server.game_rules_ref().get(GameRules::MAX_BLOCK_MODIFICATIONS);
        let level = server.level_mut(source.get_level())?;
        let registry = level.tick_state.biomes.clone().expect("the biome registry is set at level creation");
        let to_fill = registry.id_of(biome).expect("the biome argument is checked against the registry");
        let accepts = |id: BiomeId| filter.as_ref().is_none_or(|f| f.test(registry.key(id)));
        Some(fill(level, raw_from, raw_to, to_fill, &accepts, limit))
    });
    let (count, message) = result.expect("the source level exists")?;
    source.send_success(|| message.clone(), true);
    Ok(count)
}
