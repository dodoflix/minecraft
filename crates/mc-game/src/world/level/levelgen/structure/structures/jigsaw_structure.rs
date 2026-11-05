//! `JigsawStructure` (type `jigsaw`): villages, pillager outposts, bastions, ancient cities, trail ruins, trial chambers,
//! abandoned camps — everything built from a template pool by `JigsawPlacement`.

use crate::world::level::levelgen::codec::*;
use crate::world::level::levelgen::holder::Holder;
use crate::world::level::levelgen::registries::DecodeCtx;
use crate::world::level::levelgen::structure::pool_element_structure_piece::DEFAULT_LIQUID_SETTINGS;
use crate::world::level::levelgen::structure::pools::alias::{PoolAliasBinding, PoolAliasLookup};
use crate::world::level::levelgen::structure::pools::dimension_padding::DimensionPadding;
use crate::world::level::levelgen::structure::pools::jigsaw_placement::{MaxDistance, add_pieces};
use crate::world::level::levelgen::structure::pools::structure_template_pool::StructureTemplatePool;
use crate::world::level::levelgen::structure::structure::{GenerationContext, GenerationStub, STRUCTURE_TYPES, Structure, StructureSettings};
use crate::world::level::levelgen::structure::templatesystem::LiquidSettings;
use mc_core::ident::Identifier;
use mc_core::pos::BlockPos;
use mc_world::heightmap::HeightmapType;
use mc_worldgen::beardifier::TerrainAdjustment;
use mc_worldgen::provider::HeightProvider;
use serde_json::Value;
use std::sync::Arc;

/// `JigsawStructure.MAX_TOTAL_STRUCTURE_RANGE`.
const MAX_TOTAL_STRUCTURE_RANGE: i32 = 128;
/// `DimensionType.Y_SIZE`.
const Y_SIZE: i32 = 4096;

pub struct JigsawStructure {
    settings: StructureSettings,
    start_pool: Holder<StructureTemplatePool>,
    start_jigsaw_name: Option<Identifier>,
    max_depth: i32,
    start_height: HeightProvider,
    use_expansion_hack: bool,
    project_start_to_heightmap: Option<HeightmapType>,
    max_distance_from_center: MaxDistance,
    pool_aliases: Vec<PoolAliasBinding>,
    dimension_padding: DimensionPadding,
    liquid_settings: LiquidSettings,
}

/// `Heightmap.Types.CODEC`.
fn parse_heightmap(value: &Value) -> LoadResult<HeightmapType> {
    let name = parse_str(value)?;
    HeightmapType::from_serialization_key(name).ok_or_else(|| LoadError::new(format!("Unknown element name:{name}")))
}

/// `JigsawStructure.MaxDistance.CODEC`: `Codec.either(FULL_CODEC, HORIZONTAL_VALUE_CODEC)`.
fn parse_max_distance(value: &Value) -> LoadResult<MaxDistance> {
    let first = Fields::new(value).and_then(|f| {
        Ok(MaxDistance {
            horizontal: f.required("horizontal", |v| parse_int_range(v, 1, MAX_TOTAL_STRUCTURE_RANGE))?,
            vertical: f.optional("vertical", |v| parse_int_range(v, 1, Y_SIZE))?.unwrap_or(Y_SIZE),
        })
    });
    let first = match first {
        Ok(distance) => return Ok(distance),
        Err(e) => e,
    };
    match parse_int_range(value, 1, MAX_TOTAL_STRUCTURE_RANGE) {
        Ok(horizontal) => Ok(MaxDistance::new(horizontal)),
        Err(second) => Err(LoadError::new(format!("Failed to parse either. First: {first}; Second: {second}"))),
    }
}

impl JigsawStructure {
    /// `JigsawStructure.CODEC`.
    pub fn decode(fields: &Fields<'_>, ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Structure>> {
        let settings = StructureSettings::decode(fields, ctx)?;
        let start_pool = fields.required("start_pool", |v| ctx.template_pool(v))?;
        let start_jigsaw_name = fields.optional("start_jigsaw_name", parse_identifier)?;
        let max_depth = fields.required("size", |v| parse_int_range(v, 0, 20))?;
        let start_height = fields.required("start_height", HeightProvider::from_json)?;
        let use_expansion_hack = fields.required("use_expansion_hack", parse_bool)?;
        let project_start_to_heightmap = fields.optional("project_start_to_heightmap", parse_heightmap)?;
        let max_distance_from_center = fields.required("max_distance_from_center", parse_max_distance)?;
        let pool_aliases = fields.optional("pool_aliases", |v| parse_list(v, PoolAliasBinding::from_json))?.unwrap_or_default();
        let dimension_padding = fields.optional("dimension_padding", DimensionPadding::from_json)?.unwrap_or(DimensionPadding::ZERO);
        let liquid_settings = fields
            .optional("liquid_settings", |v| {
                let name = parse_str(v)?;
                LiquidSettings::by_name(name).ok_or_else(|| LoadError::new(format!("Unknown element name:{name}")))
            })?
            .unwrap_or(DEFAULT_LIQUID_SETTINGS);
        // `verifyRange`
        let edge_needed = if settings.terrain_adaptation == TerrainAdjustment::None { 0 } else { 12 };
        if max_distance_from_center.horizontal + edge_needed > MAX_TOTAL_STRUCTURE_RANGE {
            return Err(LoadError::new("Horizontal structure size including terrain adaptation must not exceed 128"));
        }
        Ok(Arc::new(JigsawStructure {
            settings,
            start_pool,
            start_jigsaw_name,
            max_depth,
            start_height,
            use_expansion_hack,
            project_start_to_heightmap,
            max_distance_from_center,
            pool_aliases,
            dimension_padding,
            liquid_settings,
        }))
    }

    /// `getStartPool()`.
    pub fn get_start_pool(&self) -> &Holder<StructureTemplatePool> {
        &self.start_pool
    }

    /// `getPoolAliases()`.
    pub fn get_pool_aliases(&self) -> &[PoolAliasBinding] {
        &self.pool_aliases
    }
}

impl Structure for JigsawStructure {
    fn settings(&self) -> &StructureSettings {
        &self.settings
    }

    fn find_generation_point<'a>(&'a self, context: &mut GenerationContext<'_>) -> Option<GenerationStub<'a>> {
        let chunk_pos = context.chunk_pos;
        let world_context = context.env.world_generation_context(context.height_accessor);
        let height = self.start_height.sample(&mut context.random, &world_context);
        let start_pos = BlockPos::new(chunk_pos.get_min_block_x(), height, chunk_pos.get_min_block_z());
        let lookup = PoolAliasLookup::create(&self.pool_aliases, start_pos, context.seed);
        add_pieces(
            context,
            &self.start_pool,
            self.start_jigsaw_name.as_ref(),
            self.max_depth,
            start_pos,
            self.use_expansion_hack,
            self.project_start_to_heightmap,
            self.max_distance_from_center,
            lookup,
            self.dimension_padding,
            self.liquid_settings,
        )
    }

    fn type_name(&self) -> &'static str {
        "jigsaw"
    }
}

#[linkme::distributed_slice(STRUCTURE_TYPES)]
static TYPE: TypeEntry<dyn Structure> = TypeEntry { name: "jigsaw", decode: JigsawStructure::decode };
