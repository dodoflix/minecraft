//! Full `buildTerrain` pipeline (fill + surface + carvers) vs the vanilla `NoiseBasedChunkGenerator` for every noise
//! settings x 3 seeds x 12 chunk positions: block states of every section, heightmaps and post-processing marks
//! (fixture `tests/fixtures/terrain_golden.txt`, generator `tests/oracle/TerrainOracle.java`, `F*` lines).

use mc_worldgen::biome::BiomeManager;
use mc_worldgen::blending::Blender;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use mc_core::pos::chunk_pos::ChunkPos;
use mc_world::palette::BiomeId;
use mc_worldgen::generator::{BuildTerrainContext, CarverBiomeCache, ChunkGenerator};

use crate::common::*;

/// `ChunkAccess.carverBiome(supplier)` stand-in: computed once per source chunk, remembered afterwards.
#[derive(Default)]
struct MapCache {
    biomes: RefCell<HashMap<ChunkPos, BiomeId>>,
    computed: Cell<usize>,
}

impl CarverBiomeCache for MapCache {
    fn carver_biome(&self, chunk_pos: ChunkPos, compute: &mut dyn FnMut() -> BiomeId) -> BiomeId {
        if let Some(&b) = self.biomes.borrow().get(&chunk_pos) {
            return b;
        }
        self.computed.set(self.computed.get() + 1);
        let b = compute();
        self.biomes.borrow_mut().insert(chunk_pos, b);
        b
    }
}

#[test]
fn build_terrain_matches_vanilla() {
    run_pipeline(false);
}

/// Same fixture, with a `carverBiomeRegion`-style cache shared by neighbouring chunks of one seed.
#[test]
fn build_terrain_with_carver_biome_cache_matches_vanilla() {
    run_pipeline(true);
}

fn run_pipeline(use_cache: bool) {
    let blocks = fixture("F", 4);
    let heightmaps = fixture("FH", 4);
    let post = fixture("FP", 4);
    let (mut checked, mut bad) = (0, Vec::new());
    for setting in SETTINGS {
        let generator = generator(setting);
        for seed in SEEDS {
            let rs = random_state(&generator, seed);
            let resolver = generator.biome_source().create_uncached_resolver(&rs);
            let biome_manager = BiomeManager::new(&*resolver, BiomeManager::obfuscate_seed(seed));
            let blender = Blender::empty();
            let cache = MapCache::default();
            for (key, lines) in &blocks {
                let (s, sd, cx, cz) = parse_key(key);
                if s != setting || sd != seed {
                    continue;
                }
                let mut possible = Vec::new();
                let mut chunk = None;
                for dz in -1..=1 {
                    for dx in -1..=1 {
                        let n = biome_chunk(&generator, setting, &rs, cx + dx, cz + dz);
                        n.access.collect_biomes_in_palette(|b| {
                            if !possible.contains(&b) {
                                possible.push(b);
                            }
                        });
                        if dx == 0 && dz == 0 {
                            chunk = Some(n);
                        }
                    }
                }
                let mut chunk = chunk.unwrap();
                let ctx = BuildTerrainContext {
                    random_state: &rs,
                    blender: &blender,
                    structure_starts: &[],
                    biome_manager: &biome_manager,
                    possible_biomes: Some(&possible),
                    carver_biome_cache: if use_cache { Some(&cache) } else { None },
                };
                generator.build_terrain(&mut chunk, &ctx);
                checked += 1;
                let got = section_hashes(&chunk).trim_start().to_owned();
                if got != lines[0] {
                    let first = got
                        .split(' ')
                        .zip(lines[0].split(' '))
                        .position(|(a, b)| a != b);
                    bad.push(format!(
                        "{key}: blocks differ (first differing section {first:?})"
                    ));
                }
                let got_hm = heightmap_lines(&mut chunk);
                if got_hm != heightmaps[key] {
                    bad.push(format!(
                        "{key}: heightmaps {got_hm:?} != {:?}",
                        heightmaps[key]
                    ));
                }
                let got_post = format!("{:016x}", post_processing_hash(&chunk));
                if got_post != post[key][0] {
                    bad.push(format!(
                        "{key}: post-processing {got_post} != {}",
                        post[key][0]
                    ));
                }
            }
            if use_cache {
                assert!(
                    cache.computed.get() > 0,
                    "carver biome cache never consulted"
                );
            }
        }
    }
    report("buildTerrain", checked, &bad);
}
