//! `StructurePlacement` and its implementations (`RandomSpreadStructurePlacement`,
//! `ConcentricRingsStructurePlacement`, `DimensionOriginStructurePlacement`) with their JSON codecs.

use serde_json::Value;

use mc_core::ident::Identifier;
use mc_core::math::java::floor_div_i32;
use mc_core::pos::block_pos::BlockPos;
use mc_core::pos::chunk_pos::ChunkPos;
use mc_core::pos::vec3i::Vec3i;
use mc_core::random::{LegacyRandomSource, RandomSource, WorldgenRandom};
use mc_world::palette::BiomeId;

use super::state::ChunkGeneratorStructureState;
use crate::biome::BiomeRegistry;
use crate::data::LoadError;
use crate::densityfunction::codec::{
    Fields, LoadResult, parse_f32, parse_identifier, parse_int, parse_int_range, parse_str,
};

/// `AbstractSpreadingStructurePlacement.HIGHLY_ARBITRARY_RANDOM_SALT`.
const HIGHLY_ARBITRARY_RANDOM_SALT: i32 = 10387320;

/// A `WorldgenRandom(LegacyRandomSource(0))`, the random every placement reducer starts from.
fn worldgen_random() -> WorldgenRandom {
    WorldgenRandom::new(Box::new(LegacyRandomSource::new(0)))
}

/// `RandomSpreadType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RandomSpreadType {
    Linear,
    Triangular,
}

impl RandomSpreadType {
    /// `RandomSpreadType.CODEC` (`StringRepresentable.fromEnum`).
    pub fn from_name(name: &str) -> Option<RandomSpreadType> {
        match name {
            "linear" => Some(RandomSpreadType::Linear),
            "triangular" => Some(RandomSpreadType::Triangular),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            RandomSpreadType::Linear => "linear",
            RandomSpreadType::Triangular => "triangular",
        }
    }

    /// `evaluate(random, limit)`.
    pub fn evaluate(self, random: &mut dyn RandomSource, limit: i32) -> i32 {
        match self {
            RandomSpreadType::Linear => random.next_int_bounded(limit),
            RandomSpreadType::Triangular => {
                let first = random.next_int_bounded(limit);
                let second = random.next_int_bounded(limit);
                first.wrapping_add(second) / 2
            }
        }
    }
}

/// `AbstractSpreadingStructurePlacement.FrequencyReductionMethod`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FrequencyReductionMethod {
    Default,
    LegacyType1,
    LegacyType2,
    LegacyType3,
}

impl FrequencyReductionMethod {
    /// `FrequencyReductionMethod.CODEC`.
    pub fn from_name(name: &str) -> Option<FrequencyReductionMethod> {
        match name {
            "default" => Some(FrequencyReductionMethod::Default),
            "legacy_type_1" => Some(FrequencyReductionMethod::LegacyType1),
            "legacy_type_2" => Some(FrequencyReductionMethod::LegacyType2),
            "legacy_type_3" => Some(FrequencyReductionMethod::LegacyType3),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            FrequencyReductionMethod::Default => "default",
            FrequencyReductionMethod::LegacyType1 => "legacy_type_1",
            FrequencyReductionMethod::LegacyType2 => "legacy_type_2",
            FrequencyReductionMethod::LegacyType3 => "legacy_type_3",
        }
    }

    /// `shouldGenerate(seed, salt, sourceX, sourceZ, probability)`.
    pub fn should_generate(self, seed: i64, salt: i32, source_x: i32, source_z: i32, probability: f32) -> bool {
        let mut random = worldgen_random();
        match self {
            // `probabilityReducer`: vanilla passes (salt, sourceX, sourceZ) as (x, z, blend) - kept as is.
            FrequencyReductionMethod::Default => {
                random.set_large_feature_with_salt(seed, salt, source_x, source_z);
                random.next_float() < probability
            }
            // `legacyPillagerOutpostReducer`
            FrequencyReductionMethod::LegacyType1 => {
                let cx = source_x >> 4;
                let cz = source_z >> 4;
                random.set_seed(i64::from(cx ^ (cz << 4)) ^ seed);
                random.next_int();
                random.next_int_bounded((1.0f32 / probability) as i32) == 0
            }
            // `legacyArbitrarySaltProbabilityReducer`
            FrequencyReductionMethod::LegacyType2 => {
                random.set_large_feature_with_salt(seed, source_x, source_z, HIGHLY_ARBITRARY_RANDOM_SALT);
                random.next_float() < probability
            }
            // `legacyProbabilityReducerWithDouble`
            FrequencyReductionMethod::LegacyType3 => {
                random.set_large_feature_seed(seed, source_x, source_z);
                random.next_double() < f64::from(probability)
            }
        }
    }
}

/// `AbstractSpreadingStructurePlacement.ExclusionZone`: `other_set` is the id of a registered structure set
/// (`RegistryCodecs.holder(..., allowInline = false)`), resolved through the
/// [`ChunkGeneratorStructureState`]'s registry when evaluated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExclusionZone {
    pub other_set: Identifier,
    pub chunk_count: i32,
}

impl ExclusionZone {
    fn from_json(value: &Value) -> LoadResult<ExclusionZone> {
        let fields = Fields::new(value)?;
        Ok(ExclusionZone {
            other_set: fields.required("other_set", |v| match v {
                Value::String(_) => parse_identifier(v),
                _ => Err(LoadError::new("Inline definitions not allowed here")),
            })?,
            chunk_count: fields.required("chunk_count", |v| parse_int_range(v, 1, 16))?,
        })
    }
}

/// The state shared by the two spreading placements (`AbstractSpreadingStructurePlacement`).
#[derive(Clone, Debug, PartialEq)]
pub struct SpreadingPlacement {
    pub locate_offset: Vec3i,
    pub frequency_reduction_method: FrequencyReductionMethod,
    pub frequency: f32,
    pub salt: i32,
    pub exclusion_zone: Option<ExclusionZone>,
}

impl SpreadingPlacement {
    /// The defaults of the convenience constructors: no offset, `default` reduction, frequency 1, no exclusion.
    pub fn new(salt: i32) -> SpreadingPlacement {
        SpreadingPlacement {
            locate_offset: Vec3i::ZERO,
            frequency_reduction_method: FrequencyReductionMethod::Default,
            frequency: 1.0,
            salt,
            exclusion_zone: None,
        }
    }

    /// `placementCodec(i)`.
    fn from_json(fields: &Fields<'_>) -> LoadResult<SpreadingPlacement> {
        Ok(SpreadingPlacement {
            locate_offset: fields
                .optional("locate_offset", parse_offset_vec3i)?
                .unwrap_or(Vec3i::ZERO),
            frequency_reduction_method: fields
                .optional("frequency_reduction_method", |v| {
                    let name = parse_str(v)?;
                    FrequencyReductionMethod::from_name(name)
                        .ok_or_else(|| LoadError::new(format!("Unknown element name:{name}")))
                })?
                .unwrap_or(FrequencyReductionMethod::Default),
            frequency: fields.optional("frequency", parse_frequency)?.unwrap_or(1.0),
            salt: fields.required("salt", |v| {
                let n = parse_int(v)?;
                if n >= 0 {
                    Ok(n)
                } else {
                    Err(LoadError::new(format!("Value must be non-negative: {n}")))
                }
            })?,
            exclusion_zone: fields.optional("exclusion_zone", ExclusionZone::from_json)?,
        })
    }

    /// `applyAdditionalChunkRestrictions(sourceX, sourceZ, levelSeed)`.
    pub fn apply_additional_chunk_restrictions(&self, source_x: i32, source_z: i32, level_seed: i64) -> bool {
        !(self.frequency < 1.0)
            || self
                .frequency_reduction_method
                .should_generate(level_seed, self.salt, source_x, source_z, self.frequency)
    }

    /// `applyInteractionsWithOtherStructures(state, sourceX, sourceZ)`. An `other_set` that the state's registry
    /// does not contain forbids nothing (vanilla cannot decode such a placement).
    pub fn apply_interactions_with_other_structures(
        &self,
        state: &ChunkGeneratorStructureState,
        source_x: i32,
        source_z: i32,
    ) -> bool {
        match &self.exclusion_zone {
            None => true,
            Some(zone) => !state
                .registry()
                .get(&zone.other_set)
                .is_some_and(|other| state.has_structure_chunk_in_range(other.value(), source_x, source_z, zone.chunk_count)),
        }
    }

    /// `isStructureChunk` given the subclass' `isPlacementChunk` result.
    fn is_structure_chunk(
        &self,
        is_placement_chunk: bool,
        state: &ChunkGeneratorStructureState,
        source_x: i32,
        source_z: i32,
    ) -> bool {
        is_placement_chunk
            && self.apply_additional_chunk_restrictions(source_x, source_z, state.level_seed())
            && self.apply_interactions_with_other_structures(state, source_x, source_z)
    }
}

/// `Vec3i.offsetCodec(16)`: a list of exactly three ints, each at most 16 from zero (`Math.abs`, so `Integer.MIN_VALUE` passes).
fn parse_offset_vec3i(value: &Value) -> LoadResult<Vec3i> {
    let list = value.as_array().ok_or_else(|| LoadError::new("Not a JSON array"))?;
    let ints = list.iter().map(parse_int).collect::<LoadResult<Vec<i32>>>()?;
    let [x, y, z] = ints[..] else {
        return Err(LoadError::new("Input is not a list of 3 ints"));
    };
    if x.wrapping_abs() <= 16 && y.wrapping_abs() <= 16 && z.wrapping_abs() <= 16 {
        Ok(Vec3i::new(x, y, z))
    } else {
        Err(LoadError::new(format!(
            "Position out of range, expected at most 16: Vec3i{{x={x}, y={y}, z={z}}}"
        )))
    }
}

/// `Codec.floatRange(0.0F, 1.0F)`: DFU compares with `Float.compareTo`, which orders `-0.0` below `0.0` and NaN
/// above everything.
fn parse_frequency(value: &Value) -> LoadResult<f32> {
    let v = parse_f32(value)?;
    if v.total_cmp(&0.0).is_ge() && v.total_cmp(&1.0).is_le() {
        Ok(v)
    } else {
        Err(LoadError::new(format!(
            "Value {} outside of range [0.0:1.0]",
            mc_core::math::java::java_float_to_string(v)
        )))
    }
}

/// `RandomSpreadStructurePlacement`.
#[derive(Clone, Debug, PartialEq)]
pub struct RandomSpreadStructurePlacement {
    pub spreading: SpreadingPlacement,
    pub spacing: i32,
    pub separation: i32,
    pub spread_type: RandomSpreadType,
}

impl RandomSpreadStructurePlacement {
    /// `RandomSpreadStructurePlacement(spacing, separation, spreadType, salt)`.
    pub fn new(spacing: i32, separation: i32, spread_type: RandomSpreadType, salt: i32) -> Self {
        RandomSpreadStructurePlacement {
            spreading: SpreadingPlacement::new(salt),
            spacing,
            separation,
            spread_type,
        }
    }

    pub fn salt(&self) -> i32 {
        self.spreading.salt
    }

    fn from_json(fields: &Fields<'_>) -> LoadResult<Self> {
        let spreading = SpreadingPlacement::from_json(fields)?;
        let spacing = fields.required("spacing", |v| parse_int_range(v, 0, 4096))?;
        let separation = fields.required("separation", |v| parse_int_range(v, 0, 4096))?;
        let spread_type = fields
            .optional("spread_type", |v| {
                let name = parse_str(v)?;
                RandomSpreadType::from_name(name)
                    .ok_or_else(|| LoadError::new(format!("Unknown element name:{name}")))
            })?
            .unwrap_or(RandomSpreadType::Linear);
        // `.validate(RandomSpreadStructurePlacement::validate)`
        if spacing <= separation {
            return Err(LoadError::new("Spacing has to be larger than separation"));
        }
        Ok(RandomSpreadStructurePlacement { spreading, spacing, separation, spread_type })
    }

    /// `getPotentialStructureChunk(seed, sourceX, sourceZ)`.
    pub fn get_potential_structure_chunk(&self, seed: i64, source_x: i32, source_z: i32) -> ChunkPos {
        let spaced_grid_x = floor_div_i32(source_x, self.spacing);
        let spaced_grid_z = floor_div_i32(source_z, self.spacing);
        let mut random = worldgen_random();
        random.set_large_feature_with_salt(seed, spaced_grid_x, spaced_grid_z, self.spreading.salt);
        let limit = self.spacing - self.separation;
        let spread_x = self.spread_type.evaluate(&mut random, limit);
        let spread_z = self.spread_type.evaluate(&mut random, limit);
        ChunkPos::new(
            spaced_grid_x.wrapping_mul(self.spacing).wrapping_add(spread_x),
            spaced_grid_z.wrapping_mul(self.spacing).wrapping_add(spread_z),
        )
    }

    /// `isPlacementChunk(state, sourceX, sourceZ)`.
    pub fn is_placement_chunk(&self, state: &ChunkGeneratorStructureState, source_x: i32, source_z: i32) -> bool {
        let chunk = self.get_potential_structure_chunk(state.level_seed(), source_x, source_z);
        chunk.x == source_x && chunk.z == source_z
    }
}

/// `ConcentricRingsStructurePlacement`.
#[derive(Clone, Debug, PartialEq)]
pub struct ConcentricRingsStructurePlacement {
    pub spreading: SpreadingPlacement,
    pub distance: i32,
    pub spread: i32,
    pub count: i32,
    /// `preferredBiomes` (the `HolderSet<Biome>` flattened in order).
    pub preferred_biomes: Vec<BiomeId>,
}

impl ConcentricRingsStructurePlacement {
    /// `ConcentricRingsStructurePlacement(distance, spread, count, preferredBiomes)`.
    pub fn new(distance: i32, spread: i32, count: i32, preferred_biomes: Vec<BiomeId>) -> Self {
        ConcentricRingsStructurePlacement {
            spreading: SpreadingPlacement::new(0),
            distance,
            spread,
            count,
            preferred_biomes,
        }
    }

    fn from_json(fields: &Fields<'_>, biomes: &BiomeRegistry) -> LoadResult<Self> {
        let spreading = SpreadingPlacement::from_json(fields)?;
        Ok(ConcentricRingsStructurePlacement {
            spreading,
            distance: fields.required("distance", |v| parse_int_range(v, 0, 1023))?,
            spread: fields.required("spread", |v| parse_int_range(v, 0, 1023))?,
            count: fields.required("count", |v| parse_int_range(v, 1, 4095))?,
            preferred_biomes: fields.required("preferred_biomes", |v| biomes.holder_set_from_json(v))?,
        })
    }

    /// `isPlacementChunk(state, sourceX, sourceZ)`.
    pub fn is_placement_chunk(&self, state: &ChunkGeneratorStructureState, source_x: i32, source_z: i32) -> bool {
        state
            .get_ring_positions_for(self)
            .is_some_and(|positions| positions.contains(&ChunkPos::new(source_x, source_z)))
    }
}

/// `StructurePlacement`, the registered implementations (`StructurePlacements.bootstrap`).
#[derive(Clone, Debug, PartialEq)]
pub enum StructurePlacement {
    RandomSpread(RandomSpreadStructurePlacement),
    ConcentricRings(ConcentricRingsStructurePlacement),
    /// `DimensionOriginStructurePlacement.INSTANCE`.
    DimensionOrigin,
}

impl StructurePlacement {
    /// `StructurePlacement.CODEC`: `type`-dispatched (`concentric_rings`, `dimension_origin`, `random_spread`).
    pub fn from_json(value: &Value, biomes: &BiomeRegistry) -> LoadResult<StructurePlacement> {
        let fields = Fields::new(value)?;
        let kind = fields.required("type", parse_identifier)?;
        if kind.namespace() != "minecraft" {
            return Err(LoadError::new(format!("Unknown registry key in minecraft:worldgen/structure_placement: {kind}")).at("type"));
        }
        match kind.path() {
            "random_spread" => RandomSpreadStructurePlacement::from_json(&fields).map(StructurePlacement::RandomSpread),
            "concentric_rings" => {
                ConcentricRingsStructurePlacement::from_json(&fields, biomes).map(StructurePlacement::ConcentricRings)
            }
            "dimension_origin" => Ok(StructurePlacement::DimensionOrigin),
            _ => Err(LoadError::new(format!("Unknown registry key in minecraft:worldgen/structure_placement: {kind}")).at("type")),
        }
    }

    /// `isStructureChunk(state, sourceX, sourceZ)`.
    pub fn is_structure_chunk(&self, state: &ChunkGeneratorStructureState, source_x: i32, source_z: i32) -> bool {
        match self {
            StructurePlacement::RandomSpread(p) => {
                let placement_chunk = p.is_placement_chunk(state, source_x, source_z);
                p.spreading.is_structure_chunk(placement_chunk, state, source_x, source_z)
            }
            StructurePlacement::ConcentricRings(p) => {
                let placement_chunk = p.is_placement_chunk(state, source_x, source_z);
                p.spreading.is_structure_chunk(placement_chunk, state, source_x, source_z)
            }
            StructurePlacement::DimensionOrigin => {
                let origin = state.get_dimension_origin();
                origin.x == source_x && origin.z == source_z
            }
        }
    }

    /// `applyAdditionalChunkRestrictions(sourceX, sourceZ, levelSeed)`.
    pub fn apply_additional_chunk_restrictions(&self, source_x: i32, source_z: i32, level_seed: i64) -> bool {
        match self {
            StructurePlacement::RandomSpread(RandomSpreadStructurePlacement { spreading, .. })
            | StructurePlacement::ConcentricRings(ConcentricRingsStructurePlacement { spreading, .. }) => {
                spreading.apply_additional_chunk_restrictions(source_x, source_z, level_seed)
            }
            StructurePlacement::DimensionOrigin => true,
        }
    }

    /// `locateOffset()`.
    pub fn locate_offset(&self) -> Vec3i {
        match self {
            StructurePlacement::RandomSpread(RandomSpreadStructurePlacement { spreading, .. })
            | StructurePlacement::ConcentricRings(ConcentricRingsStructurePlacement { spreading, .. }) => {
                spreading.locate_offset
            }
            StructurePlacement::DimensionOrigin => Vec3i::ZERO,
        }
    }

    /// `getLocatePos(chunkPos)`.
    pub fn get_locate_pos(&self, chunk_pos: ChunkPos) -> BlockPos {
        BlockPos::new(chunk_pos.get_min_block_x(), 0, chunk_pos.get_min_block_z()).offset_vec(&self.locate_offset())
    }
}
