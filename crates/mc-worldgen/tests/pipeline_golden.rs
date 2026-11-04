//! Full `buildTerrain` pipeline (fill + surface + carvers) vs the vanilla `NoiseBasedChunkGenerator` for every noise
//! settings x 3 seeds x 12 chunk positions: block states of every section, heightmaps and post-processing marks
//! (fixture `tests/fixtures/terrain_golden.txt`, generator `tests/oracle/TerrainOracle.java`, `F*` lines).

mod common;

use mc_worldgen::biome::BiomeManager;
use mc_worldgen::blending::Blender;
use mc_worldgen::generator::{BuildTerrainContext, ChunkGenerator};

use common::*;

#[test]
fn build_terrain_matches_vanilla() {
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
                    carver_biome_cache: None,
                };
                generator.build_terrain(&mut chunk, &ctx);
                checked += 1;
                let got = section_hashes(&chunk).trim_start().to_owned();
                if got != lines[0] {
                    let first = got.split(' ').zip(lines[0].split(' ')).position(|(a, b)| a != b);
                    bad.push(format!("{key}: blocks differ (first differing section {first:?})"));
                }
                let got_hm = heightmap_lines(&mut chunk);
                if got_hm != heightmaps[key] {
                    bad.push(format!("{key}: heightmaps {got_hm:?} != {:?}", heightmaps[key]));
                }
                let got_post = format!("{:016x}", post_processing_hash(&chunk));
                if got_post != post[key][0] {
                    bad.push(format!("{key}: post-processing {got_post} != {}", post[key][0]));
                }
            }
        }
    }
    report("buildTerrain", checked, &bad);
}
