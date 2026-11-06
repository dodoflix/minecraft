//! Terrain stages vs the vanilla `NoiseBasedChunkGenerator` (fixture `tests/fixtures/terrain_golden.txt`,
//! generator `tests/oracle/TerrainOracle.java`): `createBiomes`, `doFill` (blocks, heightmaps, post-processing marks),
//! `Aquifer.computeSubstance`, `getBaseHeight`/`getBaseColumn` and `getOrigin` for every noise settings x 3 seeds.

use std::collections::BTreeMap;

use mc_core::pos::block_pos::BlockPos;
use mc_core::pos::chunk_pos::ChunkPos;
use mc_world::heightmap::HeightmapType;
use mc_worldgen::biome::BiomeManager;
use mc_worldgen::blending::Blender;
use mc_worldgen::generator::{
    BlockColumn, BuildTerrainContext, ChunkGenerator, NoiseBasedChunkGenerator,
};
use mc_worldgen::random_state::RandomState;
use std::sync::Arc;

use crate::common::*;

/// Runs `check(setting, seed, generator, random_state)` for every (setting, seed) in fixture order.
fn for_each_state(mut check: impl FnMut(&str, i64, &NoiseBasedChunkGenerator, &Arc<RandomState>)) {
    for setting in SETTINGS {
        let generator = generator(setting);
        for seed in SEEDS {
            let rs = random_state(&generator, seed);
            check(setting, seed, &generator, &rs);
        }
    }
}

fn with_noise_chunk<R>(
    generator: &NoiseBasedChunkGenerator,
    _setting: &str,
    rs: &Arc<RandomState>,
    seed: i64,
    chunk: &mc_world::chunk::ProtoChunk,
    f: impl FnOnce(&mut mc_worldgen::noise_chunk::NoiseChunk) -> R,
) -> R {
    let resolver = generator.biome_source().create_uncached_resolver(rs);
    let biome_manager = BiomeManager::new(&*resolver, BiomeManager::obfuscate_seed(seed));
    let blender = Blender::empty();
    let ctx = BuildTerrainContext {
        random_state: rs,
        blender: &blender,
        structure_starts: &[],
        biome_manager: &biome_manager,
        possible_biomes: None,
        carver_biome_cache: None,
    };
    let noise_settings = generator
        .settings()
        .noise
        .clamp_to_height_accessor(chunk.get_height_accessor_for_generation());
    let mut noise_chunk = generator.create_noise_chunk(chunk.pos(), &ctx, noise_settings);
    f(&mut noise_chunk)
}

#[test]
fn create_biomes_matches_vanilla() {
    let expected = fixture("B", 4);
    let (mut checked, mut bad) = (0, Vec::new());
    for_each_state(|setting, seed, generator, rs| {
        for (key, lines) in &expected {
            let (s, sd, cx, cz) = parse_key(key);
            if s != setting || sd != seed {
                continue;
            }
            let chunk = biome_chunk(generator, setting, rs, cx, cz);
            let got = format!("{:016x}", biome_hash(&chunk));
            checked += 1;
            if got != lines[0] {
                bad.push(format!("{key}: {got} != {}", lines[0]));
            }
        }
    });
    report("createBiomes", checked, &bad);
}

#[test]
fn do_fill_matches_vanilla() {
    let blocks = fixture("D", 4);
    let heightmaps = fixture("DH", 4);
    let post = fixture("DP", 4);
    let (mut checked, mut bad) = (0, Vec::new());
    for_each_state(|setting, seed, generator, rs| {
        for (key, lines) in &blocks {
            let (s, sd, cx, cz) = parse_key(key);
            if s != setting || sd != seed {
                continue;
            }
            let mut chunk = new_chunk(setting, cx, cz);
            chunk.set_persisted_status(mc_world::status::ChunkStatus::Biomes);
            with_noise_chunk(generator, setting, rs, seed, &chunk.clone(), |nc| {
                generator.do_fill(nc, &mut chunk)
            });
            let got = section_hashes(&chunk).trim_start().to_owned();
            checked += 1;
            if got != lines[0] {
                let first = got
                    .split(' ')
                    .zip(lines[0].split(' '))
                    .position(|(a, b)| a != b)
                    .unwrap_or(0);
                bad.push(format!(
                    "{key}: blocks differ (first section string index {first})"
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
    });
    report("doFill", checked, &bad);
}

#[test]
fn aquifer_compute_substance_matches_vanilla() {
    let expected = fixture("A", 4);
    let (mut checked, mut bad) = (0, Vec::new());
    for_each_state(|setting, seed, generator, rs| {
        for (key, lines) in &expected {
            let (s, sd, cx, cz) = parse_key(key);
            if s != setting || sd != seed {
                continue;
            }
            let chunk = new_chunk(setting, cx, cz);
            let noise_settings = generator
                .settings()
                .noise
                .clamp_to_height_accessor(chunk.get_height_accessor_for_generation());
            let (count, hash) = with_noise_chunk(generator, setting, rs, seed, &chunk, |nc| {
                let aquifer = nc.aquifer();
                let mut h = FNV_INIT;
                let mut n = 0usize;
                let densities = [0.0, -0.1, 0.05, -2.0];
                for dx in [0, 5, 11, 15] {
                    for dz in [0, 6, 15] {
                        let mut y = noise_settings.min_y + noise_settings.height - 1;
                        while y >= noise_settings.min_y {
                            let d = densities[n % densities.len()];
                            let state = aquifer.compute_substance(cx * 16 + dx, y, cz * 16 + dz, d);
                            h = fnv_word(h, state.map_or(-1, |s| s.id() as i32) as u32);
                            h = fnv_word(h, u32::from(aquifer.should_schedule_fluid_update()));
                            n += 1;
                            y -= 5;
                        }
                    }
                }
                (n, h)
            });
            let got = format!("{count} {hash:016x}");
            checked += 1;
            if got != lines[0] {
                bad.push(format!("{key}: {got} != {}", lines[0]));
            }
        }
    });
    report("Aquifer.computeSubstance", checked, &bad);
}

#[test]
fn base_height_and_column_match_vanilla() {
    let heights = fixture("H", 4);
    let columns = fixture("K", 4);
    let (mut checked, mut bad) = (0, Vec::new());
    for_each_state(|setting, seed, generator, rs| {
        let height = level_height(setting);
        for (key, lines) in &heights {
            let (s, sd, x, z) = parse_key(key);
            if s != setting || sd != seed {
                continue;
            }
            let got: Vec<String> = HeightmapType::ALL
                .iter()
                .map(|&ty| {
                    format!(
                        "{} {}",
                        heightmap_name(ty),
                        generator.get_base_height(x, z, ty, height, rs)
                    )
                })
                .collect();
            checked += 1;
            if &got != lines {
                bad.push(format!("{key}: {got:?} != {lines:?}"));
            }
            let column = generator.get_base_column(x, z, height, rs);
            let mut h = FNV_INIT;
            for y in height.min_y..height.min_y + height.height {
                h = fnv_word(h, column.get_block(y).id());
            }
            checked += 1;
            if format!("{h:016x}") != columns[key][0] {
                bad.push(format!(
                    "{key}: column hash {h:016x} != {}",
                    columns[key][0]
                ));
            }
        }
    });
    report("getBaseHeight/getBaseColumn", checked, &bad);
}

#[test]
fn origin_matches_vanilla() {
    let expected: BTreeMap<String, Vec<String>> = fixture("O", 4);
    let (mut checked, mut bad) = (0, Vec::new());
    for_each_state(|setting, seed, generator, rs| {
        let key = format!("{setting} {seed} 0 0");
        let origin: ChunkPos = generator.get_origin(rs);
        let got = format!("{} {}", origin.x, origin.z);
        checked += 1;
        if got != expected[&key][0] {
            bad.push(format!("{key}: {got} != {}", expected[&key][0]));
        }
    });
    report("getOrigin", checked, &bad);
}

#[test]
fn do_fill_with_beardifier_matches_vanilla() {
    use mc_core::bounding_box::BoundingBox;
    use mc_worldgen::aquifer::FluidPicker;
    use mc_worldgen::beardifier::{Beardifier, JigsawJunction, Rigid, TerrainAdjustment};
    use mc_worldgen::densityfunction::DensityVolume;
    use mc_worldgen::generator::noise_based_fluid_picker;
    use mc_worldgen::noise_chunk::NoiseChunk;

    let expected = fixture("E", 4);
    let adjustments = [
        TerrainAdjustment::None,
        TerrainAdjustment::Bury,
        TerrainAdjustment::BeardThin,
        TerrainAdjustment::BeardBox,
        TerrainAdjustment::Encapsulate,
    ];
    let (mut checked, mut bad) = (0, Vec::new());
    for_each_state(|setting, seed, generator, rs| {
        for (key, lines) in &expected {
            let (s, sd, cx, cz) = parse_key(key);
            if s != setting || sd != seed {
                continue;
            }
            for line in lines {
                let (variant, hashes) = line.split_once(' ').unwrap();
                let variant: usize = variant.parse().unwrap();
                let (bx, bz) = (cx * 16, cz * 16);
                let bounding_box = BoundingBox::new(bx + 2, 60, bz + 3, bx + 11, 75, bz + 18);
                let junction = JigsawJunction {
                    source_x: bx + 8,
                    source_ground_y: 66,
                    source_z: bz + 20,
                };
                let affected = BoundingBox::encapsulating(
                    &bounding_box,
                    &BoundingBox::from_block_pos(&BlockPos::new(bx + 8, 66, bz + 20)),
                )
                .inflated_by(24);
                let beardifier = Beardifier::new(
                    vec![Rigid {
                        bounding_box,
                        terrain_adjustment: adjustments[variant],
                        ground_level_delta: variant as i32 - 2,
                    }],
                    vec![junction],
                    Some(affected),
                );
                let mut chunk = new_chunk(setting, cx, cz);
                chunk.set_persisted_status(mc_world::status::ChunkStatus::Biomes);
                let ns = generator
                    .settings()
                    .noise
                    .clamp_to_height_accessor(chunk.get_height_accessor_for_generation());
                let volume = DensityVolume::with_unit_step(16, ns.height, 16, bx, ns.min_y, bz);
                let picker: FluidPicker = noise_based_fluid_picker(generator.settings()).unwrap();
                let mut nc = NoiseChunk::new(
                    rs,
                    Some(beardifier),
                    generator.settings(),
                    picker,
                    &Blender::empty(),
                    volume,
                );
                generator.do_fill(&mut nc, &mut chunk);
                checked += 1;
                if section_hashes(&chunk).trim_start() != hashes {
                    bad.push(format!("{key} variant {variant}"));
                }
            }
        }
    });
    report("doFill with beardifier", checked, &bad);
}

#[test]
fn debug_screen_info_matches_vanilla() {
    let expected = fixture("X", 2);
    let (mut checked, mut bad) = (0, Vec::new());
    for_each_state(|setting, seed, generator, rs| {
        for line in &expected[&format!("{setting} {seed}")] {
            let p: Vec<&str> = line.splitn(4, ' ').collect();
            let pos = BlockPos::new(
                p[0].parse().unwrap(),
                p[1].parse().unwrap(),
                p[2].parse().unwrap(),
            );
            let mut lines = Vec::new();
            generator.add_debug_screen_info(
                &mut lines,
                rs,
                pos,
                mc_worldgen::densityfunction::SamplerContext::builder()
                    .enable_caches()
                    .build(),
            );
            checked += 1;
            if lines != [p[3].to_owned()] {
                bad.push(format!("{setting} {seed} {pos:?}: {lines:?} != {}", p[3]));
            }
        }
    });
    report("addDebugScreenInfo", checked, &bad);
}
