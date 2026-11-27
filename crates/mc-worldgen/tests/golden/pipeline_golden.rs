//! Full `buildTerrain` pipeline (fill + surface + carvers) vs the vanilla `NoiseBasedChunkGenerator` for every noise
//! settings x 3 seeds x 12 chunk positions: block states of every section, heightmaps and post-processing marks
//! (fixture `tests/fixtures/terrain_golden.txt`, generator `tests/oracle/TerrainOracle.java`, `F*` lines).

use mc_core::ident::Identifier;
use mc_worldgen::biome::{BiomeManager, biome_source_from_json};
use mc_worldgen::blending::Blender;
use mc_worldgen::generator::NoiseBasedChunkGenerator;
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

/// `buildTerrain` of chunk (0, 0) of the overworld with a fixed biome (plains with the given `carvers`).
fn carved_terrain(carvers: serde_json::Value) -> String {
    use std::sync::Arc;
    let plains = Identifier::with_default_namespace("plains").unwrap();
    let mut biome = WORLD.registries.raw("worldgen/biome", &plains).unwrap();
    biome["carvers"] = carvers;
    let biome_source = biome_source_from_json(
        &serde_json::json!({"type": "minecraft:fixed", "biome": biome}),
        &WORLD.registries,
        &WORLD.biomes,
    )
    .unwrap();
    let settings = WORLD
        .registries
        .noise_settings(&Identifier::with_default_namespace("overworld").unwrap())
        .unwrap();
    let generator = NoiseBasedChunkGenerator::new(
        biome_source,
        Arc::new(settings),
        WORLD.registries.clone(),
        WORLD.biomes.clone(),
    )
    .unwrap();
    let rs = random_state(&generator, 0);
    let resolver = generator.biome_source().create_uncached_resolver(&rs);
    let biome_manager = BiomeManager::new(&*resolver, BiomeManager::obfuscate_seed(0));
    let blender = Blender::empty();
    let mut possible = Vec::new();
    let mut chunk = None;
    for dz in -1..=1 {
        for dx in -1..=1 {
            let n = biome_chunk(&generator, "overworld", &rs, dx, dz);
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
        carver_biome_cache: None,
    };
    generator.build_terrain(&mut chunk, &ctx);
    section_hashes(&chunk)
}

/// A biome carving with the inline definition of `minecraft:cave` shapes the terrain exactly like one naming it, and both
/// differ from the biome without carvers.
#[test]
fn inline_carver_carves_like_the_named_one() {
    let cave = WORLD
        .registries
        .raw(
            "worldgen/carver",
            &Identifier::with_default_namespace("cave").unwrap(),
        )
        .unwrap();
    let none = carved_terrain(serde_json::json!([]));
    let named = carved_terrain(serde_json::json!(["minecraft:cave"]));
    let inline = carved_terrain(serde_json::json!([cave]));
    assert_ne!(none, named, "the cave carver changes chunk (0, 0)");
    assert_eq!(named, inline);
    let alone = carved_terrain(cave);
    assert_eq!(named, alone);
}
