//! `FixedBiomeSource`, `CheckerboardColumnBiomeSource` and `TheEndBiomeSource`.

use std::collections::HashSet;
use std::sync::Arc;

use serde_json::Value;

use mc_core::math::java::floor_mod_i32;
use mc_core::math::mth;
use mc_core::pos::block_pos::BlockPos;
use mc_core::pos::quart_pos;
use mc_core::random::RandomSource;
use mc_world::chunk::LevelHeight;
use mc_world::palette::BiomeId;

use super::climate::Sampler;
use super::manager::BiomeResolver;
use super::registry::BiomeRegistry;
use super::source::{BiomeSearchResult, BiomeSource, distinct_biomes};
use crate::data::LoadError;
use crate::densityfunction::codec::{Fields, LoadResult, parse_int_range};
use crate::random_state::RandomState;

/// `FixedBiomeSource`: one biome everywhere.
#[derive(Clone, Debug)]
pub struct FixedBiomeSource {
    biome: BiomeId,
    possible: [BiomeId; 1],
}

impl FixedBiomeSource {
    pub fn new(biome: BiomeId) -> FixedBiomeSource {
        FixedBiomeSource {
            biome,
            possible: [biome],
        }
    }

    pub fn biome(&self) -> BiomeId {
        self.biome
    }

    /// `FixedBiomeSource.CODEC` (`{biome}`).
    pub fn from_json(value: &Value, biomes: &BiomeRegistry) -> LoadResult<FixedBiomeSource> {
        let fields = Fields::new(value)?;
        Ok(FixedBiomeSource::new(
            fields.required("biome", |v| biomes.holder_from_json(v))?,
        ))
    }
}

impl BiomeResolver for FixedBiomeSource {
    fn get_noise_biome(&self, _quart_x: i32, _quart_y: i32, _quart_z: i32) -> BiomeId {
        self.biome
    }

    fn get_biomes_within(&self, _x: i32, _y: i32, _z: i32, _radius: i32) -> HashSet<BiomeId> {
        HashSet::from([self.biome])
    }
}

impl BiomeSource for FixedBiomeSource {
    fn possible_biomes(&self) -> &[BiomeId] {
        &self.possible
    }

    fn create_resolver<'a>(&'a self, _sampler: Sampler) -> Box<dyn BiomeResolver + 'a> {
        Box::new(self.clone())
    }

    fn find_biome_horizontal_stepped(
        &self,
        origin_x: i32,
        origin_y: i32,
        origin_z: i32,
        r: i32,
        _skip_steps: i32,
        allowed: &dyn Fn(BiomeId) -> bool,
        random: &mut dyn RandomSource,
        find_closest: bool,
        _random_state: &Arc<RandomState>,
    ) -> Option<BiomeSearchResult> {
        if !allowed(self.biome) {
            return None;
        }
        let pos = if find_closest {
            BlockPos::new(origin_x, origin_y, origin_z)
        } else {
            let x = origin_x
                .wrapping_sub(r)
                .wrapping_add(random.next_int_bounded(r.wrapping_mul(2).wrapping_add(1)));
            let z = origin_z
                .wrapping_sub(r)
                .wrapping_add(random.next_int_bounded(r.wrapping_mul(2).wrapping_add(1)));
            BlockPos::new(x, origin_y, z)
        };
        Some((pos, self.biome))
    }

    fn find_closest_biome_3d(
        &self,
        origin: BlockPos,
        _search_radius: i32,
        _sample_resolution_horizontal: i32,
        _sample_resolution_vertical: i32,
        allowed: &dyn Fn(BiomeId) -> bool,
        _random_state: &Arc<RandomState>,
        level: LevelHeight,
    ) -> Option<BiomeSearchResult> {
        allowed(self.biome).then(|| {
            (
                BlockPos::new(
                    origin.x(),
                    mth::clamp_i32(origin.y(), level.min_y + 1, level.max_y() + 1),
                    origin.z(),
                ),
                self.biome,
            )
        })
    }
}

/// `CheckerboardColumnBiomeSource`: the allowed biomes in a checkerboard of `4 << scale` block squares.
#[derive(Clone, Debug)]
pub struct CheckerboardColumnBiomeSource {
    allowed_biomes: Vec<BiomeId>,
    bit_shift: i32,
    size: i32,
    possible: Vec<BiomeId>,
}

impl CheckerboardColumnBiomeSource {
    /// `new CheckerboardColumnBiomeSource(allowedBiomes, size)`. Panics on an empty set (vanilla would divide
    /// by zero on the first lookup).
    pub fn new(allowed_biomes: Vec<BiomeId>, size: i32) -> CheckerboardColumnBiomeSource {
        assert!(!allowed_biomes.is_empty(), "checkerboard needs at least one biome");
        CheckerboardColumnBiomeSource {
            possible: distinct_biomes(allowed_biomes.iter().copied()),
            allowed_biomes,
            bit_shift: size.wrapping_add(2),
            size,
        }
    }

    /// `CheckerboardColumnBiomeSource.CODEC` (`{biomes, scale?}`).
    pub fn from_json(
        value: &Value,
        biomes: &BiomeRegistry,
    ) -> LoadResult<CheckerboardColumnBiomeSource> {
        let fields = Fields::new(value)?;
        let allowed = fields.required("biomes", |v| biomes.holder_set_from_json(v))?;
        let size = fields
            .optional("scale", |v| parse_int_range(v, 0, 62))?
            .unwrap_or(2);
        if allowed.is_empty() {
            return Err(LoadError::new("checkerboard needs at least one biome").at("biomes"));
        }
        Ok(Self::new(allowed, size))
    }

    /// The `scale` (`size`) of the checkerboard.
    pub fn size(&self) -> i32 {
        self.size
    }
}

impl BiomeResolver for CheckerboardColumnBiomeSource {
    fn get_noise_biome(&self, quart_x: i32, _quart_y: i32, quart_z: i32) -> BiomeId {
        let index = floor_mod_i32(
            (quart_x.wrapping_shr(self.bit_shift as u32)).wrapping_add(quart_z.wrapping_shr(self.bit_shift as u32)),
            self.allowed_biomes.len() as i32,
        );
        self.allowed_biomes[index as usize]
    }
}

impl BiomeSource for CheckerboardColumnBiomeSource {
    fn possible_biomes(&self) -> &[BiomeId] {
        &self.possible
    }

    fn create_resolver<'a>(&'a self, _sampler: Sampler) -> Box<dyn BiomeResolver + 'a> {
        Box::new(self.clone())
    }
}

/// `TheEndBiomeSource`.
#[derive(Clone, Debug)]
pub struct TheEndBiomeSource {
    end: BiomeId,
    highlands: BiomeId,
    midlands: BiomeId,
    islands: BiomeId,
    barrens: BiomeId,
    possible: Vec<BiomeId>,
}

impl TheEndBiomeSource {
    pub fn new(
        end: BiomeId,
        highlands: BiomeId,
        midlands: BiomeId,
        islands: BiomeId,
        barrens: BiomeId,
    ) -> TheEndBiomeSource {
        TheEndBiomeSource {
            end,
            highlands,
            midlands,
            islands,
            barrens,
            possible: distinct_biomes([end, highlands, midlands, islands, barrens]),
        }
    }

    /// `TheEndBiomeSource.create(biomes)`: looks the five end biomes up in the registry.
    pub fn create(biomes: &BiomeRegistry) -> LoadResult<TheEndBiomeSource> {
        let get = |name: &str| {
            biomes
                .id_of_name(name)
                .ok_or_else(|| LoadError::new(format!("Failed to get element minecraft:{name}")))
        };
        Ok(TheEndBiomeSource::new(
            get("the_end")?,
            get("end_highlands")?,
            get("end_midlands")?,
            get("small_end_islands")?,
            get("end_barrens")?,
        ))
    }

    fn get_noise_biome(&self, quart_x: i32, quart_y: i32, quart_z: i32, sampler: &Sampler) -> BiomeId {
        let block_x = quart_pos::to_block(quart_x);
        let block_y = quart_pos::to_block(quart_y);
        let block_z = quart_pos::to_block(quart_z);
        let chunk_x = block_x >> 4;
        let chunk_z = block_z >> 4;
        if i64::from(chunk_x) * i64::from(chunk_x) + i64::from(chunk_z) * i64::from(chunk_z) <= 4096 {
            self.end
        } else {
            let weird_block_x = (chunk_x.wrapping_mul(2).wrapping_add(1)).wrapping_mul(8);
            let weird_block_z = (chunk_z.wrapping_mul(2).wrapping_add(1)).wrapping_mul(8);
            let height_value =
                f64::from(sampler.erosion().sample_value(weird_block_x, block_y, weird_block_z));
            if height_value > 0.25 {
                self.highlands
            } else if height_value >= -0.0625 {
                self.midlands
            } else if height_value < -0.21875 {
                self.islands
            } else {
                self.barrens
            }
        }
    }
}

impl BiomeSource for TheEndBiomeSource {
    fn possible_biomes(&self) -> &[BiomeId] {
        &self.possible
    }

    fn create_resolver<'a>(&'a self, sampler: Sampler) -> Box<dyn BiomeResolver + 'a> {
        Box::new(move |quart_x: i32, quart_y: i32, quart_z: i32| {
            self.get_noise_biome(quart_x, quart_y, quart_z, &sampler)
        })
    }
}
