//! `BiomeSource`: the base class of all biome sources, with its biome search algorithms.

use std::collections::HashSet;
use std::sync::Arc;

use mc_core::direction::Direction;
use mc_core::math::java::floor_div_i32;
use mc_core::math::mth;
use mc_core::pos::block_pos::BlockPos;
use mc_core::pos::quart_pos;
use mc_core::random::RandomSource;
use mc_world::chunk::LevelHeight;
use mc_world::palette::BiomeId;

use serde_json::Value;

use super::climate::Sampler;
use super::manager::BiomeResolver;
use super::multi_noise::MultiNoiseBiomeSource;
use super::registry::BiomeRegistry;
use super::sources::{CheckerboardColumnBiomeSource, FixedBiomeSource, TheEndBiomeSource};
use crate::data::{LoadError, WorldgenRegistries};
use crate::densityfunction::SamplerContext;
use crate::densityfunction::codec::{Fields, LoadResult, parse_identifier};
use crate::random_state::RandomState;

/// `Pair<BlockPos, Holder<Biome>>` search results.
pub type BiomeSearchResult = (BlockPos, BiomeId);

/// `BiomeSource`. Implementations are the vanilla `BiomeSource` subclasses; the possible biomes are
/// collected once (`Suppliers.memoize(... .distinct() ...)`, insertion order).
pub trait BiomeSource: Send + Sync {
    /// `possibleBiomes()`.
    fn possible_biomes(&self) -> &[BiomeId];

    /// `createResolver(sampler)`.
    fn create_resolver<'a>(&'a self, sampler: Sampler) -> Box<dyn BiomeResolver + 'a>;

    /// `createResolverForChunk(...)`: a resolver for the quart box starting at `min_quart_*` with the given
    /// sizes; defaults to [`Self::create_resolver`].
    #[allow(clippy::too_many_arguments)]
    fn create_resolver_for_chunk<'a>(
        &'a self,
        sampler: Sampler,
        _min_quart_x: i32,
        _min_quart_y: i32,
        _min_quart_z: i32,
        _quart_size_x: i32,
        _quart_size_y: i32,
        _quart_size_z: i32,
    ) -> Box<dyn BiomeResolver + 'a> {
        self.create_resolver(sampler)
    }

    /// `createUncachedResolver(randomState)`.
    fn create_uncached_resolver<'a>(
        &'a self,
        random_state: &Arc<RandomState>,
    ) -> Box<dyn BiomeResolver + 'a> {
        self.create_resolver(random_state.create_climate_sampler(SamplerContext::empty_uncached()))
    }

    /// `createCachingResolver(randomState)`.
    fn create_caching_resolver<'a>(
        &'a self,
        random_state: &Arc<RandomState>,
    ) -> Box<dyn BiomeResolver + 'a> {
        let context = SamplerContext::builder().enable_caches().build();
        self.create_resolver(random_state.create_climate_sampler(context))
    }

    /// `findBiomeHorizontal(x, y, z, searchRadius, allowed, random, randomState)`.
    #[allow(clippy::too_many_arguments)]
    fn find_biome_horizontal(
        &self,
        x: i32,
        y: i32,
        z: i32,
        search_radius: i32,
        allowed: &dyn Fn(BiomeId) -> bool,
        random: &mut dyn RandomSource,
        random_state: &Arc<RandomState>,
    ) -> Option<BiomeSearchResult> {
        self.find_biome_horizontal_stepped(
            x,
            y,
            z,
            search_radius,
            1,
            allowed,
            random,
            false,
            random_state,
        )
    }

    /// `findBiomeHorizontal(originX, originY, originZ, searchRadius, skipSteps, allowed, random, findClosest,
    /// randomState)`.
    #[allow(clippy::too_many_arguments)]
    fn find_biome_horizontal_stepped(
        &self,
        origin_x: i32,
        origin_y: i32,
        origin_z: i32,
        search_radius: i32,
        skip_steps: i32,
        allowed: &dyn Fn(BiomeId) -> bool,
        random: &mut dyn RandomSource,
        find_closest: bool,
        random_state: &Arc<RandomState>,
    ) -> Option<BiomeSearchResult> {
        let noise_center_x = quart_pos::from_block(origin_x);
        let noise_center_z = quart_pos::from_block(origin_z);
        let noise_radius = quart_pos::from_block(search_radius);
        let noise_y = quart_pos::from_block(origin_y);
        let resolver = self.create_caching_resolver(random_state);
        let mut result: Option<BiomeSearchResult> = None;
        let mut found = 0i32;
        let start_radius = if find_closest { 0 } else { noise_radius };
        let mut current_radius = start_radius;
        while current_radius <= noise_radius {
            let mut z = -current_radius;
            while z <= current_radius {
                let z_edge = z.wrapping_abs() == current_radius;
                let mut x = -current_radius;
                while x <= current_radius {
                    if find_closest {
                        let x_edge = x.wrapping_abs() == current_radius;
                        if !x_edge && !z_edge {
                            x += skip_steps;
                            continue;
                        }
                    }
                    let noise_x = noise_center_x.wrapping_add(x);
                    let noise_z = noise_center_z.wrapping_add(z);
                    let biome = resolver.get_noise_biome(noise_x, noise_y, noise_z);
                    if allowed(biome) {
                        if result.is_none() || random.next_int_bounded(found + 1) == 0 {
                            let result_pos = BlockPos::new(
                                quart_pos::to_block(noise_x),
                                origin_y,
                                quart_pos::to_block(noise_z),
                            );
                            if find_closest {
                                return Some((result_pos, biome));
                            }
                            result = Some((result_pos, biome));
                        }
                        found += 1;
                    }
                    x += skip_steps;
                }
                z += skip_steps;
            }
            current_radius += skip_steps;
        }
        result
    }

    /// `findClosestBiome3d(origin, searchRadius, sampleResolutionHorizontal, sampleResolutionVertical, allowed,
    /// randomState, level)`; `level` supplies `getMinY()`/`getMaxY()`.
    #[allow(clippy::too_many_arguments)]
    fn find_closest_biome_3d(
        &self,
        origin: BlockPos,
        search_radius: i32,
        sample_resolution_horizontal: i32,
        sample_resolution_vertical: i32,
        allowed: &dyn Fn(BiomeId) -> bool,
        random_state: &Arc<RandomState>,
        level: LevelHeight,
    ) -> Option<BiomeSearchResult> {
        let candidate_biomes: HashSet<BiomeId> = self
            .possible_biomes()
            .iter()
            .copied()
            .filter(|&b| allowed(b))
            .collect();
        if candidate_biomes.is_empty() {
            return None;
        }
        let resolver = self.create_caching_resolver(random_state);
        let sample_radius = floor_div_i32(search_radius, sample_resolution_horizontal);
        let sample_ys = mth::out_from_origin_step(
            origin.y(),
            level.min_y + 1,
            level.max_y() + 1,
            sample_resolution_vertical,
        );
        for sample_column in BlockPos::spiral_around(
            &BlockPos::ZERO,
            sample_radius,
            Direction::East,
            Direction::South,
        ) {
            let block_x = origin
                .x()
                .wrapping_add(sample_column.x().wrapping_mul(sample_resolution_horizontal));
            let block_z = origin
                .z()
                .wrapping_add(sample_column.z().wrapping_mul(sample_resolution_horizontal));
            let noise_x = quart_pos::from_block(block_x);
            let noise_z = quart_pos::from_block(block_z);
            for &block_y in &sample_ys {
                let noise_y = quart_pos::from_block(block_y);
                let biome = resolver.get_noise_biome(noise_x, noise_y, noise_z);
                if candidate_biomes.contains(&biome) {
                    return Some((BlockPos::new(block_x, block_y, block_z), biome));
                }
            }
        }
        None
    }
}

/// `collectPossibleBiomes().distinct()` keeping first occurrences (`ImmutableSet` insertion order).
pub fn distinct_biomes(biomes: impl IntoIterator<Item = BiomeId>) -> Vec<BiomeId> {
    let mut seen = HashSet::new();
    biomes.into_iter().filter(|b| seen.insert(*b)).collect()
}

/// `BiomeSource.CODEC`: `type`-dispatched (`fixed`, `multi_noise`, `checkerboard`, `the_end`).
pub fn biome_source_from_json(
    value: &Value,
    registries: &WorldgenRegistries,
    biomes: &BiomeRegistry,
) -> LoadResult<Arc<dyn BiomeSource>> {
    let fields = Fields::new(value)?;
    let kind = fields.required("type", parse_identifier)?;
    if kind.namespace() != "minecraft" {
        return Err(
            LoadError::new(format!("Unknown registry key in biome_source: {kind}")).at("type"),
        );
    }
    Ok(match kind.path() {
        "fixed" => Arc::new(FixedBiomeSource::from_json(value, biomes)?),
        "multi_noise" => Arc::new(MultiNoiseBiomeSource::from_json(value, registries, biomes)?),
        "checkerboard" => Arc::new(CheckerboardColumnBiomeSource::from_json(value, biomes)?),
        "the_end" => Arc::new(TheEndBiomeSource::create(biomes)?),
        _ => {
            return Err(
                LoadError::new(format!("Unknown registry key in biome_source: {kind}")).at("type"),
            );
        }
    })
}

impl dyn BiomeSource {
    /// [`biome_source_from_json`].
    pub fn from_json(
        value: &Value,
        registries: &WorldgenRegistries,
        biomes: &BiomeRegistry,
    ) -> LoadResult<Arc<dyn BiomeSource>> {
        biome_source_from_json(value, registries, biomes)
    }
}
