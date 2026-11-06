//! Differential test of the material system against the vanilla server: the fixture
//! (`tests/oracle/MaterialSurfaceOracle.java`) holds vanilla's `doFill` output, the biomes the
//! `BiomeManager` reads, and the result of vanilla's `MaterialSystem.buildSurface` / `topMaterial`.

use std::collections::{BTreeSet, HashMap};
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;

use mc_core::ident::Identifier;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_data::block::{Block, BlockState};
use mc_world::chunk::{LevelHeight, ProtoChunk, SectionsView};
use mc_world::heightmap::HeightmapType;
use mc_world::palette::{BiomeId, BiomeRegistry as WorldBiomeRegistry, PalettedContainerFactory};
use mc_world::upgrade_data::UpgradeData;
use mc_worldgen::biome::{BiomeManager, BiomeRegistry};
use mc_worldgen::data::{FsDataSource, WorldgenRegistries};
use mc_worldgen::densityfunction::{DensityVolume, SamplerContext};
use mc_worldgen::material::MaterialRule;
use mc_worldgen::noise_settings::NoiseGeneratorSettings;
use mc_worldgen::random_state::RandomState;
use mc_worldgen::world_generation_context::WorldGenerationContext;

fn data_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ref/src/data")
}

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut text = String::new();
    flate2::read::GzDecoder::new(&bytes[..])
        .read_to_string(&mut text)
        .expect("gzip fixture");
    text
}

fn state(id: u32) -> BlockState {
    BlockState::from_id(id).unwrap_or_else(|| panic!("state id {id}"))
}

#[derive(Default)]
struct Case {
    name: String,
    seed: i64,
    cx: i32,
    cz: i32,
    min_y: i32,
    height: i32,
    label: String,
    palette: Vec<String>,
    quarts: (i32, i32, i32, i32, i32, i32),
    quart_data: Vec<u8>,
    possible: Vec<usize>,
    sections: Vec<(usize, Vec<(u32, u32)>)>,
    pp_before: Vec<(usize, Vec<i16>)>,
    pp_after: Vec<(usize, Vec<i16>)>,
    hm_before: HashMap<String, Vec<i32>>,
    hm_after: HashMap<String, Vec<i32>>,
    tops: Vec<(i32, i32, i32, bool, i64)>,
    diff: Vec<(i32, i32, i32, u32)>,
}

fn rle(tokens: &[&str]) -> Vec<(u32, u32)> {
    tokens
        .iter()
        .map(|t| {
            let (a, b) = t.split_once(':').expect("rle token");
            (a.parse().expect("rle id"), b.parse().expect("rle count"))
        })
        .collect()
}

fn ints<T: std::str::FromStr>(tokens: &[&str]) -> Vec<T>
where
    T::Err: std::fmt::Debug,
{
    tokens
        .iter()
        .map(|t| t.parse().expect("int token"))
        .collect()
}

struct Bands {
    name: String,
    seed: i64,
    bands: Vec<u32>,
    band_samples: Vec<(i32, i32, i32, u32)>,
    depth_samples: Vec<(i32, i32, i32)>,
    secondary_samples: Vec<(i32, i32, u64)>,
}

fn parse(text: &str) -> (Vec<Case>, Vec<Bands>) {
    let mut cases = Vec::new();
    let mut bands: Vec<Bands> = Vec::new();
    let mut current: Option<Case> = None;
    for line in text.lines() {
        let t: Vec<&str> = line.split(' ').collect();
        match t[0] {
            "CASE" => {
                current = Some(Case {
                    name: t[1].to_owned(),
                    seed: t[2].parse().unwrap(),
                    cx: t[3].parse().unwrap(),
                    cz: t[4].parse().unwrap(),
                    min_y: t[5].parse().unwrap(),
                    height: t[6].parse().unwrap(),
                    label: t[7].to_owned(),
                    ..Case::default()
                })
            }
            "PAL" => {
                current.as_mut().unwrap().palette = t[2..].iter().map(|s| s.to_string()).collect()
            }
            "Q" => {
                let c = current.as_mut().unwrap();
                let h: Vec<i32> = ints(&t[1..7]);
                c.quarts = (h[0], h[1], h[2], h[3], h[4], h[5]);
                for (idx, count) in rle(&t[7..]) {
                    c.quart_data
                        .extend(std::iter::repeat_n(idx as u8, count as usize));
                }
            }
            "POSS" => current.as_mut().unwrap().possible = ints(&t[2..]),
            "S" => current
                .as_mut()
                .unwrap()
                .sections
                .push((t[1].parse().unwrap(), rle(&t[2..]))),
            "PB" => current
                .as_mut()
                .unwrap()
                .pp_before
                .push((t[1].parse().unwrap(), ints(&t[2..]))),
            "PA" => current
                .as_mut()
                .unwrap()
                .pp_after
                .push((t[1].parse().unwrap(), ints(&t[2..]))),
            "HB" => {
                current
                    .as_mut()
                    .unwrap()
                    .hm_before
                    .insert(t[1].to_owned(), ints(&t[3..]));
            }
            "HA" => {
                current
                    .as_mut()
                    .unwrap()
                    .hm_after
                    .insert(t[1].to_owned(), ints(&t[3..]));
            }
            "T" => current.as_mut().unwrap().tops.push((
                t[1].parse().unwrap(),
                t[2].parse().unwrap(),
                t[3].parse().unwrap(),
                t[4] == "true",
                t[5].parse().unwrap(),
            )),
            "D" => {
                let c = current.as_mut().unwrap();
                for e in &t[2..] {
                    let v: Vec<&str> = e.split(',').collect();
                    c.diff.push((
                        v[0].parse().unwrap(),
                        v[1].parse().unwrap(),
                        v[2].parse().unwrap(),
                        v[3].parse().unwrap(),
                    ));
                }
            }
            "END" => cases.push(current.take().unwrap()),
            "BANDS" => bands.push(Bands {
                name: t[1].to_owned(),
                seed: t[2].parse().unwrap(),
                bands: ints(&t[4..]),
                band_samples: Vec::new(),
                depth_samples: Vec::new(),
                secondary_samples: Vec::new(),
            }),
            "B" => bands.last_mut().unwrap().band_samples.push((
                t[3].parse().unwrap(),
                t[4].parse().unwrap(),
                t[5].parse().unwrap(),
                t[6].parse().unwrap(),
            )),
            "SD" => bands.last_mut().unwrap().depth_samples.push((
                t[3].parse().unwrap(),
                t[4].parse().unwrap(),
                t[5].parse().unwrap(),
            )),
            "SS" => bands.last_mut().unwrap().secondary_samples.push((
                t[3].parse().unwrap(),
                t[4].parse().unwrap(),
                u64::from_str_radix(t[5], 16).unwrap(),
            )),
            other => panic!("unknown fixture line {other}"),
        }
    }
    (cases, bands)
}

struct World {
    registries: Arc<WorldgenRegistries>,
    biomes: BiomeRegistry,
    factory: PalettedContainerFactory,
    settings: HashMap<String, Arc<NoiseGeneratorSettings>>,
    rules: HashMap<String, Arc<MaterialRule>>,
    states: HashMap<(String, i64), Arc<RandomState>>,
}

impl World {
    fn new() -> World {
        let registries = Arc::new(WorldgenRegistries::new(Arc::new(FsDataSource::new(
            data_root(),
        ))));
        let biomes = BiomeRegistry::load(&registries).expect("biomes load");
        let factory = PalettedContainerFactory::create(WorldBiomeRegistry::new(biomes.names()))
            .expect("plains");
        World {
            registries,
            biomes,
            factory,
            settings: HashMap::new(),
            rules: HashMap::new(),
            states: HashMap::new(),
        }
    }

    fn settings(&mut self, name: &str) -> Arc<NoiseGeneratorSettings> {
        let registries = self.registries.clone();
        self.settings
            .entry(name.to_owned())
            .or_insert_with(|| {
                Arc::new(
                    registries
                        .noise_settings(&Identifier::with_default_namespace(name).unwrap())
                        .expect("settings"),
                )
            })
            .clone()
    }

    fn rule(&mut self, name: &str) -> Arc<MaterialRule> {
        let settings = self.settings(name);
        let (registries, biomes) = (self.registries.clone(), &self.biomes);
        self.rules
            .entry(name.to_owned())
            .or_insert_with(|| {
                Arc::new(
                    MaterialRule::from_json(&settings.material_rule, &registries, biomes)
                        .expect("material rule"),
                )
            })
            .clone()
    }

    fn state(&mut self, name: &str, seed: i64) -> Arc<RandomState> {
        let settings = self.settings(name);
        let registries = self.registries.clone();
        self.states
            .entry((name.to_owned(), seed))
            .or_insert_with(|| Arc::new(RandomState::create(registries, seed, &settings)))
            .clone()
    }
}

/// Loads `doFill`'s output like `NoiseBasedChunkGenerator.doFill` writes it.
fn load_chunk(world: &World, case: &Case) -> ProtoChunk {
    let level_height = LevelHeight::new(case.min_y, case.height);
    let mut chunk = ProtoChunk::new(
        ChunkPos::new(case.cx, case.cz),
        UpgradeData::EMPTY,
        level_height,
        &world.factory,
        None,
    );
    let access = &mut chunk.access;
    access.get_or_create_heightmap_unprimed(HeightmapType::OceanFloorWg);
    access.get_or_create_heightmap_unprimed(HeightmapType::WorldSurfaceWg);
    let air = Block::AIR.default_state();
    let mut dense: HashMap<usize, Vec<BlockState>> = HashMap::new();
    for (section, runs) in &case.sections {
        let mut states = Vec::with_capacity(4096);
        for &(id, count) in runs {
            states.extend(std::iter::repeat_n(state(id), count as usize));
        }
        assert_eq!(states.len(), 4096);
        dense.insert(*section, states);
    }
    // doFill order: z, x, y descending; only non-air states are written.
    for z in 0..16 {
        for x in 0..16 {
            for y in (0..case.height).rev() {
                let block_y = case.min_y + y;
                let section_index = level_height.section_index(block_y) as usize;
                let Some(states) = dense.get(&section_index) else {
                    continue;
                };
                let s = states[(((block_y & 15) * 16 + z) * 16 + x) as usize];
                if s != air {
                    let access = &mut chunk.access;
                    access
                        .get_section_mut(section_index)
                        .set_block_state(x, block_y & 15, z, s);
                    let view = SectionsView {
                        level_height,
                        sections: &access.sections,
                        void_air_outside: true,
                    };
                    for ty in [HeightmapType::OceanFloorWg, HeightmapType::WorldSurfaceWg] {
                        access
                            .heightmaps
                            .get_mut(&ty)
                            .unwrap()
                            .update(x, block_y, z, s, &view);
                    }
                }
            }
        }
    }
    for (section, packed) in &case.pp_before {
        chunk.access.add_packed_post_process(packed, *section);
    }
    chunk
}

fn snapshot(chunk: &ProtoChunk) -> Vec<BlockState> {
    let h = chunk.access.level_height;
    let (min_x, min_z) = (chunk.pos().get_min_block_x(), chunk.pos().get_min_block_z());
    (0..h.height * 256)
        .map(|i| {
            chunk.get_block_state(BlockPos::new(
                min_x + (i & 15),
                h.min_y + i / 256,
                min_z + ((i / 16) & 15),
            ))
        })
        .collect()
}

fn heights(chunk: &mut ProtoChunk, ty: HeightmapType) -> Vec<i32> {
    let mut out = Vec::new();
    for z in 0..16 {
        for x in 0..16 {
            out.push(chunk.access.get_height(true, ty, x, z));
        }
    }
    out
}

fn post_process(chunk: &ProtoChunk) -> Vec<(usize, Vec<i16>)> {
    chunk
        .access
        .post_processing
        .iter()
        .enumerate()
        .filter_map(|(i, p)| p.as_ref().filter(|p| !p.is_empty()).map(|p| (i, p.clone())))
        .collect()
}

/// Runs one case; returns the set of block states `buildSurface` produced (for coverage accounting).
fn run_case(world: &mut World, case: &Case, possible_mode: bool) -> BTreeSet<u32> {
    let desc = format!(
        "{} seed {} chunk ({}, {}) [{}]",
        case.name, case.seed, case.cx, case.cz, case.label
    );
    let settings = world.settings(&case.name);
    let rule = world.rule(&case.name);
    let rs = world.state(&case.name, case.seed);
    let mut chunk = load_chunk(world, case);

    assert_eq!(
        heights(&mut chunk, HeightmapType::WorldSurfaceWg),
        case.hm_before["ws"],
        "{desc}: loaded WORLD_SURFACE_WG"
    );
    assert_eq!(
        heights(&mut chunk, HeightmapType::OceanFloorWg),
        case.hm_before["ofl"],
        "{desc}: loaded OCEAN_FLOOR_WG"
    );

    // biome table -> resolver
    let (qx0, qy0, qz0, sx, sy, sz) = case.quarts;
    let biome_ids: Vec<BiomeId> = case
        .palette
        .iter()
        .map(|n| {
            world
                .biomes
                .id_of(&Identifier::parse(n).unwrap())
                .unwrap_or_else(|| panic!("biome {n}"))
        })
        .collect();
    let resolver = |qx: i32, qy: i32, qz: i32| -> BiomeId {
        let qy = qy.clamp(qy0, qy0 + sy - 1);
        assert!(
            (qx0..qx0 + sx).contains(&qx) && (qz0..qz0 + sz).contains(&qz),
            "{desc}: biome read outside table {qx} {qz}"
        );
        let i = (((qy - qy0) * sz + (qz - qz0)) * sx + (qx - qx0)) as usize;
        biome_ids[case.quart_data[i] as usize]
    };
    let manager = BiomeManager::new(&resolver, BiomeManager::obfuscate_seed(case.seed));
    let possible: Vec<BiomeId> = case.possible.iter().map(|&i| biome_ids[i]).collect();

    let noise = settings.noise;
    let level = chunk.access.level_height;
    let ctx = WorldGenerationContext::new(noise.min_y, noise.height, settings.sea_level, level);
    let clamped_min = noise.min_y.max(level.min_y);
    let clamped_height = (noise.min_y + noise.height).min(level.max_y() + 1) - clamped_min;
    let (min_x, min_z) = (chunk.pos().get_min_block_x(), chunk.pos().get_min_block_z());
    let volume = DensityVolume::with_unit_step(16, clamped_height, 16, min_x, clamped_min, min_z);
    let samplers = rs.samplers_with_context(SamplerContext::builder().enable_caches().build());
    let biome_getter = |pos: BlockPos| manager.get_biome(pos);

    // topMaterial on the doFill chunk
    if possible_mode {
        for &(x, y, z, fluid, expected) in &case.tops {
            let got = rs
                .surface_system()
                .top_material(
                    &rule,
                    &rs,
                    &ctx,
                    &world.biomes,
                    &biome_getter,
                    &mut chunk,
                    &samplers,
                    BlockPos::new(x, y, z),
                    fluid,
                )
                .map_or(-1, |s| i64::from(s.id()));
            assert_eq!(got, expected, "{desc}: topMaterial({x}, {y}, {z}, {fluid})");
        }
    }

    let before = snapshot(&chunk);
    let possible_arg = possible_mode.then_some(&possible[..]);
    rs.surface_system().build_surface(
        &rs,
        &manager,
        &ctx,
        &world.biomes,
        &mut chunk,
        &volume,
        &samplers,
        &rule,
        possible_arg,
    );
    let after = snapshot(&chunk);

    let mut expected = before.clone();
    let mut produced = BTreeSet::new();
    for &(x, y, z, id) in &case.diff {
        expected[(((y - case.min_y) * 16 + z) * 16 + x) as usize] = state(id);
        produced.insert(id);
    }
    if expected != after {
        let bad: Vec<String> = (0..after.len())
            .filter(|&i| expected[i] != after[i])
            .take(10)
            .map(|i| {
                format!(
                    "({}, {}, {}) expected {} got {}",
                    i & 15,
                    case.min_y + (i / 256) as i32,
                    (i / 16) & 15,
                    expected[i],
                    after[i]
                )
            })
            .collect();
        panic!(
            "{desc}: block mismatch (possible_biomes={possible_mode}): {}",
            bad.join("; ")
        );
    }
    assert_eq!(
        post_process(&chunk),
        case.pp_after,
        "{desc}: post-process marks"
    );
    assert_eq!(
        heights(&mut chunk, HeightmapType::WorldSurfaceWg),
        case.hm_after["ws"],
        "{desc}: WORLD_SURFACE_WG after"
    );
    assert_eq!(
        heights(&mut chunk, HeightmapType::OceanFloorWg),
        case.hm_after["ofl"],
        "{desc}: OCEAN_FLOOR_WG after"
    );
    produced
}

#[test]
fn build_surface_matches_vanilla() {
    let (cases, _) = parse(&fixture("material_surface.txt.gz"));
    assert!(cases.len() >= 40, "{} cases", cases.len());
    let mut world = World::new();
    let mut produced = BTreeSet::new();
    let mut total_diff = 0;
    for case in &cases {
        produced.extend(run_case(&mut world, case, true));
        total_diff += case.diff.len();
    }
    // the optimisation by possibleBiomes must not change anything; a no-possible-biomes run too
    for case in cases.iter().step_by(3) {
        run_case(&mut world, case, false);
    }
    eprintln!(
        "{} chunks, {} changed blocks, {} distinct produced states",
        cases.len(),
        total_diff,
        produced.len()
    );
    assert!(
        total_diff > 10_000,
        "surface rules must have changed blocks"
    );

    // Coverage accounting: every block state a vanilla surface rule can place must appear in some chunk diff.
    let mut reachable = BTreeSet::new();
    for name in [
        "overworld",
        "large_biomes",
        "amplified",
        "nether",
        "end",
        "caves",
        "floating_islands",
    ] {
        let rule = world.settings(name).material_rule.clone();
        collect_result_states(&rule, &world.registries, &mut reachable);
    }
    let missing: Vec<String> = reachable
        .iter()
        .filter(|id| !produced.contains(id))
        .map(|&id| state(id).to_string())
        .collect();
    eprintln!("rule result states never produced by the fixture chunks: {missing:?}");
    let unexpected: Vec<&String> = missing
        .iter()
        .filter(|m| !UNCOVERED.contains(&m.as_str()))
        .collect();
    assert!(
        unexpected.is_empty(),
        "rule results not covered by any fixture chunk: {unexpected:?}"
    );
}

/// Rule result states the fixture chunks do not show in a diff: placements onto an identical state (netherrack /
/// end_stone / lava floors) change nothing, the others need rare spots (frozen-ocean holes, badlands ceilings,
/// nether forest wart ceilings, 3d calcite bands).
const UNCOVERED: [&str; 8] = [
    "minecraft:air",
    "minecraft:lava[level=0]",
    "minecraft:netherrack",
    "minecraft:end_stone",
    "minecraft:red_sandstone",
    "minecraft:nether_wart_block",
    "minecraft:warped_wart_block",
    "minecraft:calcite",
];

fn collect_result_states(
    rule: &serde_json::Value,
    registries: &WorldgenRegistries,
    out: &mut BTreeSet<u32>,
) {
    use serde_json::Value;
    match rule {
        Value::String(name) => {
            if let Some(json) =
                registries.raw("worldgen/material_rule", &Identifier::parse(name).unwrap())
            {
                collect_result_states(&json, registries, out);
            }
        }
        Value::Object(map) => match map.get("type").and_then(Value::as_str) {
            Some("minecraft:block") => {
                let spec =
                    mc_worldgen::noise_settings::BlockStateSpec::from_json(&map["result_state"])
                        .unwrap();
                out.insert(spec.resolve().unwrap().id());
            }
            Some("minecraft:ore_vein") => {
                for key in ["ore_block", "raw_ore_block", "filler_block"] {
                    let spec =
                        mc_worldgen::noise_settings::BlockStateSpec::from_json(&map[key]).unwrap();
                    out.insert(spec.resolve().unwrap().id());
                }
            }
            Some("minecraft:sequence") => {
                for r in map["sequence"].as_array().unwrap() {
                    collect_result_states(r, registries, out);
                }
            }
            Some("minecraft:condition") => collect_result_states(&map["then_run"], registries, out),
            _ => {}
        },
        _ => {}
    }
}

#[test]
fn bands_surface_depth_and_secondary_match_vanilla() {
    let (_, bands) = parse(&fixture("material_surface.txt.gz"));
    assert!(!bands.is_empty());
    let mut world = World::new();
    for b in &bands {
        let rs = world.state(&b.name, b.seed);
        let system = rs.surface_system();
        let got: Vec<u32> = system.clay_bands().iter().map(|s| s.id()).collect();
        assert_eq!(got, b.bands, "{} {} clay bands", b.name, b.seed);
        for &(x, y, z, id) in &b.band_samples {
            assert_eq!(
                system.get_band(x, y, z).id(),
                id,
                "{} {} band({x},{y},{z})",
                b.name,
                b.seed
            );
        }
        for &(x, z, depth) in &b.depth_samples {
            assert_eq!(
                system.get_surface_depth(x, z),
                depth,
                "{} {} surface depth({x},{z})",
                b.name,
                b.seed
            );
        }
        for &(x, z, bits) in &b.secondary_samples {
            assert_eq!(
                system.get_surface_secondary(x, z).to_bits(),
                bits,
                "{} {} secondary({x},{z})",
                b.name,
                b.seed
            );
        }
    }
}
