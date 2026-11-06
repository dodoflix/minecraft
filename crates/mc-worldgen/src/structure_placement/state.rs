//! `ChunkGeneratorStructureState`: the per-level structure placement state (possible structure sets, the
//! concentric ring positions such as the strongholds').

use std::collections::{HashMap, HashSet};
use std::f64::consts::PI;
use std::sync::{Arc, OnceLock};

use mc_core::ident::Identifier;
use mc_core::math::java::round_f64;
use mc_core::pos::chunk_pos::ChunkPos;
use mc_core::pos::section_pos::SectionPos;
use mc_core::random::{LegacyRandomSource, RandomSource};
use mc_world::palette::BiomeId;

use super::placement::{ConcentricRingsStructurePlacement, StructurePlacement};
use super::structure_set::{StructureSet, StructureSetHolder, StructureSetRegistry};
use crate::biome::BiomeSource;
use crate::random_state::RandomState;

/// What vanilla computes lazily on the first `ensureStructuresGenerated()`.
struct Generated {
    /// `placementsForStructure`: structure -> the sets (in `possibleStructureSets` order) that place it.
    placements_for_structure: HashMap<Identifier, Vec<StructureSetHolder>>,
    /// `ringPositions`, keyed by the address of the `ConcentricRingsStructurePlacement` inside its (`Arc`ed) set:
    /// vanilla keys the map by placement object identity as well.
    ring_positions: HashMap<usize, Vec<ChunkPos>>,
}

/// `ChunkGeneratorStructureState`.
pub struct ChunkGeneratorStructureState {
    random_state: Arc<RandomState>,
    biome_source: Arc<dyn BiomeSource>,
    registry: Arc<StructureSetRegistry>,
    level_seed: i64,
    concentric_rings_seed: i64,
    origin: ChunkPos,
    possible_structure_sets: Vec<StructureSetHolder>,
    /// Per possible set: the structures of its entries (entry order) with a biome the biome source can produce.
    placeable_structures: Vec<Vec<Identifier>>,
    generated: OnceLock<Generated>,
}

impl ChunkGeneratorStructureState {
    /// `createForNormal(randomState, levelSeed, origin, biomeSource, allStructures)`: every set of `registry`
    /// whose structures can occur in the biome source's biomes. `structure_biomes(structure)` is
    /// `Structure.biomes()` flattened (for a registry from [`StructureSetRegistry::load`]:
    /// `|s| registry.structure_biomes(s).to_vec()`).
    pub fn create_for_normal(
        random_state: Arc<RandomState>,
        level_seed: i64,
        origin: ChunkPos,
        biome_source: Arc<dyn BiomeSource>,
        registry: Arc<StructureSetRegistry>,
        structure_biomes: &dyn Fn(&Identifier) -> Vec<BiomeId>,
    ) -> ChunkGeneratorStructureState {
        let sets = registry.iter().cloned().collect::<Vec<_>>();
        Self::new(
            random_state,
            biome_source,
            registry,
            level_seed,
            origin,
            level_seed,
            sets,
            structure_biomes,
        )
    }

    /// `createForFlat(randomState, levelSeed, origin, biomeSource, structureOverrides)`: `structure_overrides` are
    /// holders of `registry` (the ring seed is 0, not the level seed).
    pub fn create_for_flat(
        random_state: Arc<RandomState>,
        level_seed: i64,
        origin: ChunkPos,
        biome_source: Arc<dyn BiomeSource>,
        registry: Arc<StructureSetRegistry>,
        structure_overrides: impl IntoIterator<Item = StructureSetHolder>,
        structure_biomes: &dyn Fn(&Identifier) -> Vec<BiomeId>,
    ) -> ChunkGeneratorStructureState {
        let sets = structure_overrides.into_iter().collect::<Vec<_>>();
        Self::new(
            random_state,
            biome_source,
            registry,
            level_seed,
            origin,
            0,
            sets,
            structure_biomes,
        )
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "mirrors the vanilla constructor parameter list"
    )]
    fn new(
        random_state: Arc<RandomState>,
        biome_source: Arc<dyn BiomeSource>,
        registry: Arc<StructureSetRegistry>,
        level_seed: i64,
        origin: ChunkPos,
        concentric_rings_seed: i64,
        candidate_sets: Vec<StructureSetHolder>,
        structure_biomes: &dyn Fn(&Identifier) -> Vec<BiomeId>,
    ) -> ChunkGeneratorStructureState {
        let possible_biomes: HashSet<BiomeId> =
            biome_source.possible_biomes().iter().copied().collect();
        let mut possible_structure_sets = Vec::new();
        let mut placeable_structures = Vec::new();
        for set in candidate_sets {
            // `hasBiomesForStructureSet` (filter) and `generatePositions`' per-structure check coincide.
            let placeable: Vec<Identifier> = set
                .structures
                .iter()
                .filter(|entry| {
                    structure_biomes(&entry.structure)
                        .iter()
                        .any(|b| possible_biomes.contains(b))
                })
                .map(|entry| entry.structure.clone())
                .collect();
            if !placeable.is_empty() {
                possible_structure_sets.push(set);
                placeable_structures.push(placeable);
            }
        }
        ChunkGeneratorStructureState {
            random_state,
            biome_source,
            registry,
            level_seed,
            concentric_rings_seed,
            origin,
            possible_structure_sets,
            placeable_structures,
            generated: OnceLock::new(),
        }
    }

    /// `possibleStructureSets()`.
    pub fn possible_structure_sets(&self) -> &[StructureSetHolder] {
        &self.possible_structure_sets
    }

    /// `getDimensionOrigin()`.
    pub fn get_dimension_origin(&self) -> ChunkPos {
        self.origin
    }

    /// `randomState()`.
    pub fn random_state(&self) -> &Arc<RandomState> {
        &self.random_state
    }

    /// The biome source the state was created for.
    pub fn biome_source(&self) -> &Arc<dyn BiomeSource> {
        &self.biome_source
    }

    /// The structure set registry `exclusion_zone.other_set` ids are resolved in.
    pub fn registry(&self) -> &StructureSetRegistry {
        &self.registry
    }

    /// `getLevelSeed()`.
    pub fn level_seed(&self) -> i64 {
        self.level_seed
    }

    /// `ensureStructuresGenerated()`: computes `placementsForStructure` and the concentric ring positions (once;
    /// vanilla does this on the first query, the ring searches on a background executor). Every query calls it.
    pub fn ensure_structures_generated(&self) {
        self.generated();
    }

    fn generated(&self) -> &Generated {
        self.generated.get_or_init(|| {
            let mut placements_for_structure: HashMap<Identifier, Vec<StructureSetHolder>> =
                HashMap::new();
            let mut ring_positions = HashMap::new();
            for (set, placeable) in self
                .possible_structure_sets
                .iter()
                .zip(&self.placeable_structures)
            {
                for structure in placeable {
                    placements_for_structure
                        .entry(structure.clone())
                        .or_default()
                        .push(set.clone());
                }
                if let StructurePlacement::ConcentricRings(rings) = &set.placement {
                    ring_positions.insert(ring_key(rings), self.generate_ring_positions(rings));
                }
            }
            Generated {
                placements_for_structure,
                ring_positions,
            }
        })
    }

    /// `generateRingPositions(structureSet, placement)`: the exact RNG sequence, then one biome search per ring
    /// position (each has its own forked random, so concurrent searches give the same results).
    fn generate_ring_positions(
        &self,
        placement: &ConcentricRingsStructurePlacement,
    ) -> Vec<ChunkPos> {
        if placement.count == 0 {
            return Vec::new();
        }
        let distance = placement.distance;
        let count = placement.count;
        let mut spread = placement.spread;
        let mut random = LegacyRandomSource::new(self.concentric_rings_seed);
        let mut angle = random.next_double() * PI * 2.0;
        let mut position_in_circle = 0i32;
        let mut circle = 0i32;

        let mut searches = Vec::with_capacity(count as usize);
        for i in 0..count {
            let ring_distance = 4i32
                .wrapping_mul(distance)
                .wrapping_add(distance.wrapping_mul(circle).wrapping_mul(6));
            let dist = f64::from(ring_distance)
                + (random.next_double() - 0.5) * (f64::from(distance) * 2.5);
            let initial_x = round_f64(angle.cos() * dist) as i32;
            let initial_z = round_f64(angle.sin() * dist) as i32;
            // `random.fork()` of a `LegacyRandomSource`
            searches.push((initial_x, initial_z, random.next_long()));
            angle += (PI * 2.0) / f64::from(spread);
            position_in_circle += 1;
            if position_in_circle == spread {
                circle += 1;
                position_in_circle = 0;
                spread += 2 * spread / (circle + 1);
                spread = spread.min(count - i);
                angle += random.next_double() * PI * 2.0;
            }
        }

        let search = |&(initial_x, initial_z, generator_seed): &(i32, i32, i64)| {
            let closest = self.biome_source.find_biome_horizontal(
                SectionPos::section_to_block_coord_offset(initial_x, 8),
                0,
                SectionPos::section_to_block_coord_offset(initial_z, 8),
                112,
                &|biome| placement.preferred_biomes.contains(&biome),
                &mut LegacyRandomSource::new(generator_seed),
                &self.random_state,
            );
            match closest {
                Some((position, _)) => ChunkPos::new(
                    SectionPos::block_to_section_coord_i32(position.x()),
                    SectionPos::block_to_section_coord_i32(position.z()),
                ),
                None => ChunkPos::new(initial_x, initial_z),
            }
        };
        // Vanilla runs the searches on its background executor; so do we, in order-preserving chunks.
        let threads = std::thread::available_parallelism()
            .map_or(1, |n| n.get())
            .min(searches.len());
        let chunk_size = searches.len().div_ceil(threads);
        std::thread::scope(|scope| {
            let workers: Vec<_> = searches
                .chunks(chunk_size)
                .map(|part| scope.spawn(|| part.iter().map(search).collect::<Vec<_>>()))
                .collect();
            workers
                .into_iter()
                .flat_map(|worker| {
                    worker
                        .join()
                        .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
                })
                .collect()
        })
    }

    /// `getRingPositionsFor(placement)`: `None` unless `placement` is (by identity) the placement of a possible set
    /// of this state.
    pub fn get_ring_positions_for(
        &self,
        placement: &ConcentricRingsStructurePlacement,
    ) -> Option<&[ChunkPos]> {
        self.generated()
            .ring_positions
            .get(&ring_key(placement))
            .map(Vec::as_slice)
    }

    /// `getPlacementsForStructure(structure)`: the possible sets whose placement places the structure (a set is
    /// listed once per entry naming the structure).
    pub fn get_placements_for_structure(&self, structure: &Identifier) -> &[StructureSetHolder] {
        self.generated()
            .placements_for_structure
            .get(structure)
            .map_or(&[], Vec::as_slice)
    }

    /// `hasStructureChunkInRange(structureSet, sourceX, sourceZ, range)`.
    pub fn has_structure_chunk_in_range(
        &self,
        structure_set: &StructureSet,
        source_x: i32,
        source_z: i32,
        range: i32,
    ) -> bool {
        let placement = &structure_set.placement;
        for test_x in source_x.wrapping_sub(range)..=source_x.wrapping_add(range) {
            for test_z in source_z.wrapping_sub(range)..=source_z.wrapping_add(range) {
                if placement.is_structure_chunk(self, test_x, test_z) {
                    return true;
                }
            }
        }
        false
    }

    /// `structureSet.placement().isStructureChunk(state, x, z)`.
    pub fn is_structure_chunk(
        &self,
        structure_set: &StructureSet,
        source_x: i32,
        source_z: i32,
    ) -> bool {
        structure_set
            .placement
            .is_structure_chunk(self, source_x, source_z)
    }
}

fn ring_key(placement: &ConcentricRingsStructurePlacement) -> usize {
    std::ptr::from_ref(placement) as usize
}
