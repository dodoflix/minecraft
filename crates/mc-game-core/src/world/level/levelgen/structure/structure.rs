//! `Structure` (the data-driven structure definition of `worldgen/structure`), its settings, the generation context
//! and the generation stub.
//!
//! A structure type lives in its own file and registers itself in [`STRUCTURE_TYPES`]:
//! ```ignore
//! #[linkme::distributed_slice(STRUCTURE_TYPES)]
//! static IGLOO: TypeEntry<dyn Structure> = TypeEntry { name: "igloo", decode: IglooStructure::decode };
//! ```
//! `decode` receives the whole JSON object (`type` + settings + its own fields); [`StructureSettings::decode`] reads the
//! shared `Structure.StructureSettings.CODEC` fields from it.

use super::manager::StructureManager;
use super::pieces::{PiecesContainer, StructurePiecesBuilder};
use crate::world::level::levelgen::codec::{Fields, LoadError, LoadResult, TypeEntry, parse_str};
use crate::world::level::levelgen::env::GenEnv;
use crate::world::level::levelgen::generation_step::Decoration;
use crate::world::level::levelgen::registries::DecodeCtx;
use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_core::bounding_box::BoundingBox;
use mc_core::pos::{BlockPos, ChunkPos, quart_pos};
use mc_core::random::{LegacyRandomSource, RandomSource, WorldgenRandom};
use mc_core::rotation::Rotation;
use mc_data::entity_type::MOB_CATEGORIES;
use mc_data::enums::MobCategory;
use mc_data::ids::EntityType;
use mc_world::chunk::LevelHeight;
use mc_world::heightmap::HeightmapType;
use mc_world::palette::BiomeId;
use mc_worldgen::beardifier::TerrainAdjustment;
use mc_worldgen::biome::BiomeResolver;
use mc_worldgen::densityfunction::SamplerContext;
use mc_worldgen::provider::{IntProvider, Weighted, WeightedList};
use serde_json::Value;
use std::sync::Arc;

/// `BuiltInRegistries.STRUCTURE_TYPE` (`StructureType.codec()`): the decoders of the structure classes.
#[linkme::distributed_slice]
pub static STRUCTURE_TYPES: [TypeEntry<dyn Structure>];

/// `StructureSpawnOverride.BoundingBoxType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoundingBoxType {
    /// `piece`
    Piece,
    /// `full`
    Structure,
}

/// `MobSpawnSettings.SpawnerData(type, count)`.
#[derive(Clone, Debug, PartialEq)]
pub struct SpawnerData {
    pub entity_type: EntityType,
    pub count: IntProvider,
}

impl SpawnerData {
    /// `SpawnerData.CODEC` (MapCodec: `type` + `count`; a `MISC` category type becomes a pig).
    pub fn decode(fields: &Fields<'_>) -> LoadResult<SpawnerData> {
        let mut entity_type = fields.required("type", |v| {
            let name = parse_str(v)?;
            EntityType::by_name(name)
                .ok_or_else(|| LoadError::new(format!("Unknown registry key in ResourceKey[minecraft:root / minecraft:entity_type]: {name}")))
        })?;
        let count = fields.required("count", IntProvider::from_json)?;
        if entity_type.data().category == MobCategory::Misc {
            entity_type = EntityType::by_name("minecraft:pig").expect("pig exists");
        }
        Ok(SpawnerData { entity_type, count })
    }
}

/// `StructureSpawnOverride(boundingBox, spawns)`.
#[derive(Clone, Debug, PartialEq)]
pub struct StructureSpawnOverride {
    pub bounding_box: BoundingBoxType,
    pub spawns: WeightedList<SpawnerData>,
}

impl StructureSpawnOverride {
    /// `StructureSpawnOverride.CODEC`.
    pub fn from_json(value: &Value) -> LoadResult<StructureSpawnOverride> {
        let fields = Fields::new(value)?;
        let bounding_box = fields.required("bounding_box", |v| match parse_str(v)? {
            "piece" => Ok(BoundingBoxType::Piece),
            "full" => Ok(BoundingBoxType::Structure),
            other => Err(LoadError::new(format!("Unknown element name:{other}"))),
        })?;
        let spawns = fields.required("spawns", |v| {
            let items = crate::world::level::levelgen::codec::parse_list(v, |entry| {
                let entry_fields = Fields::new(entry)?;
                let data = SpawnerData::decode(&entry_fields)?;
                let weight = entry_fields.required("weight", |w| {
                    let weight = crate::world::level::levelgen::codec::parse_int(w)?;
                    if weight < 0 {
                        return Err(LoadError::new(format!(
                            "Value must be non-negative: {weight}"
                        )));
                    }
                    Ok(weight)
                })?;
                Ok(Weighted {
                    value: data,
                    weight,
                })
            })?;
            WeightedList::of(items)
        })?;
        Ok(StructureSpawnOverride {
            bounding_box,
            spawns,
        })
    }
}

/// `MobCategory.CODEC` (serialized names).
pub fn mob_category_by_name(name: &str) -> Option<MobCategory> {
    MOB_CATEGORIES
        .iter()
        .find(|d| d.serialized_name == name)
        .map(|d| d.category)
}

/// `Structure.StructureSettings(biomes, spawnOverrides, step, terrainAdaptation)`.
#[derive(Clone, Debug)]
pub struct StructureSettings {
    /// `HolderSet<Biome>` flattened to the biome ids (tags are resolved when decoded).
    pub biomes: Vec<BiomeId>,
    /// `Map<MobCategory, StructureSpawnOverride>` in JSON key order.
    pub spawn_overrides: Vec<(MobCategory, StructureSpawnOverride)>,
    pub step: Decoration,
    pub terrain_adaptation: TerrainAdjustment,
}

impl StructureSettings {
    /// `StructureSettings(biomes)` (`DEFAULT`: no overrides, `SURFACE_STRUCTURES`, `NONE`).
    pub fn new(biomes: Vec<BiomeId>) -> StructureSettings {
        StructureSettings {
            biomes,
            spawn_overrides: Vec::new(),
            step: Decoration::SurfaceStructures,
            terrain_adaptation: TerrainAdjustment::None,
        }
    }

    /// `StructureSettings.CODEC` (a MapCodec: the fields live in the structure's own object).
    pub fn decode(fields: &Fields<'_>, ctx: &DecodeCtx<'_>) -> LoadResult<StructureSettings> {
        let biomes = fields.required("biomes", |v| ctx.biomes.holder_set_from_json(v))?;
        let spawn_overrides = fields.required("spawn_overrides", |v| {
            let object = v.as_object().ok_or_else(|| LoadError::new("Not a map"))?;
            let mut out = Vec::with_capacity(object.len());
            for (key, value) in object {
                let category = mob_category_by_name(key)
                    .ok_or_else(|| LoadError::new(format!("Unknown element name:{key}")).at(key))?;
                out.push((
                    category,
                    StructureSpawnOverride::from_json(value).map_err(|e| e.at(key))?,
                ));
            }
            Ok(out)
        })?;
        let step = fields.required("step", |v| {
            let name = parse_str(v)?;
            Decoration::by_name(name)
                .ok_or_else(|| LoadError::new(format!("Unknown element name:{name}")))
        })?;
        let terrain_adaptation = fields
            .optional("terrain_adaptation", |v| {
                let name = parse_str(v)?;
                TerrainAdjustment::from_name(name)
                    .ok_or_else(|| LoadError::new(format!("Unknown element name:{name}")))
            })?
            .unwrap_or(TerrainAdjustment::None);
        Ok(StructureSettings {
            biomes,
            spawn_overrides,
            step,
            terrain_adaptation,
        })
    }

    /// `spawnOverrides().get(category)`.
    pub fn spawn_override(&self, category: MobCategory) -> Option<&StructureSpawnOverride> {
        self.spawn_overrides
            .iter()
            .find(|(c, _)| *c == category)
            .map(|(_, o)| o)
    }
}

/// `Structure.GenerationContext`: what `findGenerationPoint` works with.
pub struct GenerationContext<'a> {
    /// `registryAccess`, `chunkGenerator`, `randomState`, `structureTemplateManager`, `biomeSource` all hang off the
    /// level's environment.
    pub env: &'a GenEnv,
    pub biome_resolver: Box<dyn BiomeResolver + 'a>,
    /// `random()`: a `WorldgenRandom(LegacyRandomSource(0))` seeded with `setLargeFeatureSeed(seed, chunkX, chunkZ)`.
    pub random: WorldgenRandom,
    pub seed: i64,
    pub chunk_pos: ChunkPos,
    pub height_accessor: LevelHeight,
    pub valid_biome: &'a dyn Fn(BiomeId) -> bool,
}

impl<'a> GenerationContext<'a> {
    /// `new GenerationContext(registryAccess, chunkGenerator, biomeSource, climateSampler, randomState, templateManager,
    /// seed, chunkPos, heightAccessor, validBiome)`; the climate sampler is created from the environment's random state
    /// (caching sampler, as `createStructures` does).
    pub fn new(
        env: &'a GenEnv,
        seed: i64,
        chunk_pos: ChunkPos,
        height_accessor: LevelHeight,
        valid_biome: &'a dyn Fn(BiomeId) -> bool,
    ) -> GenerationContext<'a> {
        let sampler = env
            .random_state
            .create_climate_sampler(SamplerContext::builder().enable_caches().build());
        let biome_resolver = env.biome_source().create_resolver(sampler);
        let mut random = WorldgenRandom::new(Box::new(LegacyRandomSource::new(0)));
        random.set_large_feature_seed(seed, chunk_pos.x, chunk_pos.z);
        GenerationContext {
            env,
            biome_resolver,
            random,
            seed,
            chunk_pos,
            height_accessor,
            valid_biome,
        }
    }

    /// `isValidBiome(stub)`.
    pub fn is_valid_biome(&self, stub: &GenerationStub<'_>) -> bool {
        let start_pos = stub.position;
        (self.valid_biome)(self.biome_resolver.get_noise_biome(
            quart_pos::from_block(start_pos.x()),
            quart_pos::from_block(start_pos.y()),
            quart_pos::from_block(start_pos.z()),
        ))
    }

    /// `couldStructureExistInColumn(blockX, blockZ, minBlockY, maxBlockY)`.
    pub fn could_structure_exist_in_column(
        &self,
        block_x: i32,
        block_z: i32,
        min_block_y: i32,
        max_block_y: i32,
    ) -> bool {
        let quart_x = quart_pos::from_block(block_x);
        let quart_z = quart_pos::from_block(block_z);
        let min_quart_y = quart_pos::from_block(min_block_y);
        let max_quart_y = quart_pos::from_block(max_block_y);
        let sampler = self
            .env
            .random_state
            .create_climate_sampler(SamplerContext::builder().enable_caches().build());
        let column_resolver = self.env.biome_source().create_resolver_for_chunk(
            sampler,
            quart_x,
            min_quart_y,
            quart_z,
            1,
            max_quart_y - min_quart_y + 1,
            1,
        );
        (min_quart_y..=max_quart_y).any(|quart_y| {
            (self.valid_biome)(column_resolver.get_noise_biome(quart_x, quart_y, quart_z))
        })
    }

    /// `couldValidBiomeExistInTerrainColumn(blockX, blockZ)`.
    pub fn could_valid_biome_exist_in_terrain_column(&self, block_x: i32, block_z: i32) -> bool {
        self.could_structure_exist_in_column(
            block_x,
            block_z,
            self.height_accessor.min_y - 1,
            self.height_accessor.max_y(),
        )
    }

    /// `couldValidBiomeExistOnTopOfChunkCenter()`.
    pub fn could_valid_biome_exist_on_top_of_chunk_center(&self) -> bool {
        self.could_valid_biome_exist_in_terrain_column(
            self.chunk_pos.get_middle_block_x(),
            self.chunk_pos.get_middle_block_z(),
        )
    }

    /// `chunkGenerator().getFirstOccupiedHeight(x, z, type, heightAccessor, randomState)`.
    pub fn get_first_occupied_height(&self, x: i32, z: i32, heightmap: HeightmapType) -> i32 {
        self.env.generator.get_first_occupied_height(
            x,
            z,
            heightmap,
            self.height_accessor,
            &self.env.random_state,
        )
    }

    /// `chunkGenerator().getBaseColumn(x, z, heightAccessor, randomState)`.
    pub fn get_base_column(&self, x: i32, z: i32) -> mc_worldgen::generator::NoiseColumn {
        self.env.get_base_column(x, z, self.height_accessor)
    }
}

/// The deferred piece generator of a [`GenerationStub`] (`Consumer<StructurePiecesBuilder>`): vanilla's lambdas capture
/// the context, here it is handed in again when the pieces are generated.
pub type PiecesGenerator<'a> =
    Box<dyn FnOnce(&mut StructurePiecesBuilder, &mut GenerationContext<'_>) + 'a>;

/// `Structure.GenerationStub(position, generator)`.
pub struct GenerationStub<'a> {
    pub position: BlockPos,
    generator: StubGenerator<'a>,
}

enum StubGenerator<'a> {
    Lazy(PiecesGenerator<'a>),
    Ready(StructurePiecesBuilder),
}

impl<'a> GenerationStub<'a> {
    /// `new GenerationStub(position, Consumer)`.
    pub fn new(
        position: BlockPos,
        generator: impl FnOnce(&mut StructurePiecesBuilder, &mut GenerationContext<'_>) + 'a,
    ) -> GenerationStub<'a> {
        GenerationStub {
            position,
            generator: StubGenerator::Lazy(Box::new(generator)),
        }
    }

    /// `new GenerationStub(position, Either.right(builder))`.
    pub fn with_builder(position: BlockPos, builder: StructurePiecesBuilder) -> GenerationStub<'a> {
        GenerationStub {
            position,
            generator: StubGenerator::Ready(builder),
        }
    }

    /// `getPiecesBuilder()`.
    pub fn get_pieces_builder(self, context: &mut GenerationContext<'_>) -> StructurePiecesBuilder {
        match self.generator {
            StubGenerator::Lazy(generator) => {
                let mut builder = StructurePiecesBuilder::new();
                generator(&mut builder, context);
                builder
            }
            StubGenerator::Ready(builder) => builder,
        }
    }
}

/// `Structure`.
pub trait Structure: Send + Sync {
    fn settings(&self) -> &StructureSettings;

    /// `findGenerationPoint(context)`.
    fn find_generation_point<'a>(
        &'a self,
        context: &mut GenerationContext<'_>,
    ) -> Option<GenerationStub<'a>>;

    /// `type()`: the `StructureType` id (path in `minecraft:`).
    fn type_name(&self) -> &'static str;

    /// `afterPlace(level, structureManager, generator, random, chunkBB, chunkPos, pieces)`.
    fn after_place(
        &self,
        _level: &mut dyn WorldGenLevel,
        _structure_manager: &StructureManager,
        _generator: &GenEnv,
        _random: &mut dyn RandomSource,
        _chunk_bb: &BoundingBox,
        _chunk_pos: ChunkPos,
        _pieces: &PiecesContainer,
    ) {
    }

    /// `OceanMonumentStructure.regeneratePiecesAfterLoad(chunkPos, seed, pieces)` (identity for every other structure).
    fn regenerate_pieces_after_load(
        &self,
        _chunk_pos: ChunkPos,
        _seed: i64,
        pieces: PiecesContainer,
    ) -> PiecesContainer {
        pieces
    }

    /// `biomes()`.
    fn biomes(&self) -> &[BiomeId] {
        &self.settings().biomes
    }

    /// `step()`.
    fn step(&self) -> Decoration {
        self.settings().step
    }

    /// `terrainAdaptation()`.
    fn terrain_adaptation(&self) -> TerrainAdjustment {
        self.settings().terrain_adaptation
    }

    /// `adjustBoundingBox(boundingBox)`.
    fn adjust_bounding_box(&self, bounding_box: BoundingBox) -> BoundingBox {
        if self.terrain_adaptation() != TerrainAdjustment::None {
            bounding_box.inflated_by(12)
        } else {
            bounding_box
        }
    }

    /// `findValidGenerationPoint(context)`.
    fn find_valid_generation_point<'a>(
        &'a self,
        context: &mut GenerationContext<'_>,
    ) -> Option<GenerationStub<'a>> {
        self.find_generation_point(context)
            .filter(|stub| context.is_valid_biome(stub))
    }
}

/// `Structure.decode`: `Structure.DIRECT_CODEC` (`type` dispatch).
pub fn decode_direct(value: &Value, ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Structure>> {
    crate::world::level::levelgen::codec::decode_dispatch(value, ctx, &STRUCTURE_TYPES)
}

/// `Structure.onTopOfChunkCenter(context, heightmap, generator)`.
pub fn on_top_of_chunk_center<'a>(
    context: &GenerationContext<'_>,
    heightmap: HeightmapType,
    generator: impl FnOnce(&mut StructurePiecesBuilder, &mut GenerationContext<'_>) + 'a,
) -> Option<GenerationStub<'a>> {
    if !context.could_valid_biome_exist_on_top_of_chunk_center() {
        return None;
    }
    on_top_of_chunk_center_without_biome_check(context, heightmap, generator)
}

/// `Structure.onTopOfChunkCenterWithoutBiomeCheck(context, heightmap, generator)`.
pub fn on_top_of_chunk_center_without_biome_check<'a>(
    context: &GenerationContext<'_>,
    heightmap: HeightmapType,
    generator: impl FnOnce(&mut StructurePiecesBuilder, &mut GenerationContext<'_>) + 'a,
) -> Option<GenerationStub<'a>> {
    let chunk_pos = context.chunk_pos;
    let block_x = chunk_pos.get_middle_block_x();
    let block_z = chunk_pos.get_middle_block_z();
    let block_y = context.get_first_occupied_height(block_x, block_z, heightmap);
    Some(GenerationStub::new(
        BlockPos::new(block_x, block_y, block_z),
        generator,
    ))
}

/// `Structure.getCornerHeights(context, minX, sizeX, minZ, sizeZ)`.
fn get_corner_heights(
    context: &GenerationContext<'_>,
    min_x: i32,
    size_x: i32,
    min_z: i32,
    size_z: i32,
) -> [i32; 4] {
    let h = |x, z| context.get_first_occupied_height(x, z, HeightmapType::WorldSurfaceWg);
    [
        h(min_x, min_z),
        h(min_x, min_z + size_z),
        h(min_x + size_x, min_z),
        h(min_x + size_x, min_z + size_z),
    ]
}

/// `Structure.getMeanFirstOccupiedHeight(context, minX, sizeX, minZ, sizeZ)`.
pub fn get_mean_first_occupied_height(
    context: &GenerationContext<'_>,
    min_x: i32,
    size_x: i32,
    min_z: i32,
    size_z: i32,
) -> i32 {
    let h = get_corner_heights(context, min_x, size_x, min_z, size_z);
    (h[0] + h[1] + h[2] + h[3]) / 4
}

/// `Structure.getLowestY(context, sizeX, sizeZ)`.
pub fn get_lowest_y(context: &GenerationContext<'_>, size_x: i32, size_z: i32) -> i32 {
    let chunk_pos = context.chunk_pos;
    get_lowest_y_at(
        context,
        chunk_pos.get_min_block_x(),
        chunk_pos.get_min_block_z(),
        size_x,
        size_z,
    )
}

/// `Structure.getLowestY(context, minX, minZ, sizeX, sizeZ)`.
pub fn get_lowest_y_at(
    context: &GenerationContext<'_>,
    min_x: i32,
    min_z: i32,
    size_x: i32,
    size_z: i32,
) -> i32 {
    let h = get_corner_heights(context, min_x, size_x, min_z, size_z);
    h[0].min(h[1]).min(h[2].min(h[3]))
}

/// `Structure.getLowestYIn5by5Box(context, blockX, blockZ, rotation)`.
pub fn get_lowest_y_in_5_by_5_box(
    context: &GenerationContext<'_>,
    block_x: i32,
    block_z: i32,
    rotation: Rotation,
) -> BlockPos {
    let (mut offset_x, mut offset_z) = (5, 5);
    match rotation {
        Rotation::Clockwise90 => offset_x = -5,
        Rotation::Clockwise180 => {
            offset_x = -5;
            offset_z = -5;
        }
        Rotation::Counterclockwise90 => offset_z = -5,
        Rotation::None => {}
    }
    BlockPos::new(
        block_x,
        get_lowest_y_at(context, block_x, block_z, offset_x, offset_z),
        block_z,
    )
}
