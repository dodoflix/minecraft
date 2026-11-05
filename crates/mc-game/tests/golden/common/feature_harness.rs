//! Shared harness of the worldgen-feature golden tests (Java mirror: `tests/oracle/FeatureOracleBase.java`).
//!
//! How a feature test uses it (see `crates/mc-game-core/tests/golden/gen_level_golden.rs`):
//! ```ignore
//! mod common; use common::feature_harness::*;
//! let mut synth = SynthRegion::new(seed, cx, cz, ChunkStatus::Features, 2);   // 5x5 synthetic chunks, FEATURES step
//! let before = snapshot(&synth);
//! {
//!     let mut level = synth.level();           // GenLevel over the lent chunks (a WorldGenLevel)
//!     /* run the Rust feature under test with `&mut level`, `&*synth.env` (GenEnv), level.random() ... */
//!     lines = dump(&mut level, &before);       // compare line by line with the Java `dump` output of the fixture
//! }
//! ```
//! The synthetic terrain ([`terrain`]) is a pure integer formula of (seed, x, y, z) shared with the Java harness and checked
//! through the `T`/`M` digest lines of [`dump`]. Chunks have status TERRAIN and all four final heightmaps primed. The
//! `GenEnv` has the real overworld `NoiseBasedChunkGenerator` and `RandomState` of the seed (as the Java oracle's
//! `generator()`/`randomState(seed)`) and the vanilla feature registries (when all of them decode; otherwise empty ones
//! so tests of the level itself still run).
#![allow(dead_code, reason = "each test target uses a subset of the shared helpers")]

use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

use mc_core::ident::Identifier;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_data::block::{Block, BlockState};
use mc_game::server::level::world_gen::{GenChunk, WorldGenRegion};
use mc_game::world::level::chunk::status::{ChunkStep, GENERATION_PYRAMID};
use mc_game::world::level::levelgen::gen_level::GenLevel;
use mc_game::world::level::levelgen::registries::GenRegistries;
use mc_game::world::level::levelgen::structure::check::{NoChunkScan, StructureCheck};
use mc_game::world::level::levelgen::structure::registries::{StartCache, StructureRegistries};
use mc_game::world::level::levelgen::structure::templatesystem::StructureTemplateManager;
use mc_game::server::packs::repository::PackSource;
use mc_game::server::packs::resource_manager::MultiPackResourceManager;
use mc_game::server::packs::resources::PathPackResources;
use mc_game::server::packs::{PackLocationInfo, PackType};
use mc_text::Component;
use mc_game::world::level::levelgen::GenEnv;
use mc_game::world::level::LevelAccessor;
use mc_world::chunk::{LevelHeight, ProtoChunk};
use mc_world::heightmap::HeightmapType;
use mc_world::palette::{BiomeId, PalettedContainerFactory};
use mc_world::status::{ChunkStatus, FINAL_HEIGHTMAPS};
use mc_world::ticks::TickPriority;
use mc_world::upgrade_data::UpgradeData;
use mc_worldgen::biome::{BiomeRegistry, biome_source_from_json};
use mc_worldgen::data::{FsDataSource, WorldgenDataSource, WorldgenRegistries};
use mc_worldgen::generator::NoiseBasedChunkGenerator;
use mc_worldgen::random_state::RandomState;

pub const MIN_Y: i32 = -64;
pub const HEIGHT: i32 = 384;
pub const SEA_LEVEL: i32 = 63;
pub const BIOME_NAMES: [&str; 8] = ["plains", "forest", "desert", "ocean", "river", "swamp", "taiga", "birch_forest"];

// ---- deterministic hash helpers (ported verbatim from FeatureOracleBase.java) ----

pub fn mix64(mut z: i64) -> i64 {
    z = (z ^ ((z as u64) >> 30) as i64).wrapping_mul(0xBF58476D1CE4E5B9u64 as i64);
    z = (z ^ ((z as u64) >> 27) as i64).wrapping_mul(0x94D049BB133111EBu64 as i64);
    z ^ ((z as u64) >> 31) as i64
}

pub fn h(seed: i64, a: i64, b: i64, c: i64) -> i64 {
    mix64(
        seed.wrapping_mul(0x9E3779B97F4A7C15u64 as i64)
            ^ mix64(a.wrapping_mul(0xD6E8FEB86659FD93u64 as i64) ^ mix64(b.wrapping_mul(0xCA5A826395121157u64 as i64) ^ mix64(c.wrapping_add(0x1234567)))),
    )
}

pub fn hmod(v: i64, m: i32) -> i32 {
    (((v as u64) >> 1) % m as u64) as i32
}

/// FNV-1a 64 (`FeatureOracleBase.Fnv`).
pub struct Fnv {
    pub v: u64,
    pub count: u64,
}

impl Default for Fnv {
    fn default() -> Self {
        Fnv { v: 0xcbf29ce484222325, count: 0 }
    }
}

impl Fnv {
    fn b(&mut self, x: i32) {
        self.v ^= (x & 0xFF) as u64;
        self.v = self.v.wrapping_mul(0x100000001b3);
    }

    pub fn i(&mut self, x: i32) {
        for shift in [0, 8, 16, 24] {
            self.b(((x as u32) >> shift) as i32);
        }
        self.count += 1;
    }

    pub fn l(&mut self, x: i64) {
        self.i(x as i32);
        self.i((x >> 32) as i32);
    }

    pub fn s(&mut self, s: &str) {
        for c in s.bytes() {
            self.b(i32::from(c as i8));
        }
        self.b(0xFF);
        self.count += 1;
    }

    pub fn hex(&self) -> String {
        format!("{:x}", self.v)
    }
}

// ---- synthetic terrain (mirror of FeatureOracleBase.terrain) ----

fn lattice(seed: i64, i: i32, j: i32) -> i32 {
    hmod(h(seed, i64::from(i), i64::from(j), 1), 24)
}

/// First y above the solid surface of column (x, z): 50..75.
pub fn surface_top(seed: i64, x: i32, z: i32) -> i32 {
    let (gx, gz, fx, fz) = (x.div_euclid(8), z.div_euclid(8), x.rem_euclid(8), z.rem_euclid(8));
    let (a, b, c, d) = (lattice(seed, gx, gz), lattice(seed, gx + 1, gz), lattice(seed, gx, gz + 1), lattice(seed, gx + 1, gz + 1));
    let v = ((a * (8 - fx) + b * fx) * (8 - fz) + (c * (8 - fx) + d * fx) * fz) / 64;
    50 + v + i32::from(hmod(h(seed, i64::from(x), i64::from(z), 2), 3) == 0)
}

pub fn terrain(seed: i64, x: i32, y: i32, z: i32) -> BlockState {
    let at = |a: i32, b: i32, c: i32| h(seed, i64::from(a), i64::from(b), i64::from(c));
    if y == MIN_Y {
        return Block::BEDROCK.default_state();
    }
    if y < MIN_Y + 5 && hmod(at(x, y, z), 5) >= y - MIN_Y {
        return Block::BEDROCK.default_state();
    }
    let top = surface_top(seed, x, z);
    if y >= top {
        return if y < SEA_LEVEL { Block::WATER } else { Block::AIR }.default_state();
    }
    let bed = if hmod(at(x >> 3, z >> 3, 3), 2) == 0 { Block::SAND } else { Block::GRAVEL };
    if y == top - 1 {
        return if top <= 64 { bed } else { Block::GRASS_BLOCK }.default_state();
    }
    if y >= top - 3 {
        return if top <= 64 { bed } else { Block::DIRT }.default_state();
    }
    let cave = y > MIN_Y + 5 && y < top - 8 && hmod(at(x >> 2, y >> 2, z >> 2), 9) == 0;
    if cave {
        return if y < -50 { Block::LAVA } else { Block::AIR }.default_state();
    }
    if y < hmod(at(x >> 2, 0, z >> 2), 8) { Block::DEEPSLATE } else { Block::STONE }.default_state()
}

/// Biome of a 4x4x4 cell: 8 surface biomes in 8x8 columns, cave biomes below y=-16.
pub fn biome_name(seed: i64, qx: i32, qy: i32, qz: i32) -> &'static str {
    let at = |a: i32, b: i32, c: i32| h(seed ^ 0xB10, i64::from(a), i64::from(b), i64::from(c));
    if qy < -4 {
        return if hmod(at(qx >> 1, qy >> 1, qz >> 1), 2) == 0 { "lush_caves" } else { "dripstone_caves" };
    }
    BIOME_NAMES[hmod(at(qx >> 1, 0, qz >> 1), BIOME_NAMES.len() as i32) as usize]
}

// ---- world: registries, generator, factory ----

pub struct FeatureWorld {
    pub worldgen: Arc<WorldgenRegistries>,
    pub biomes: Arc<BiomeRegistry>,
    pub registries: Arc<GenRegistries>,
    pub structures: Arc<StructureRegistries>,
    pub templates: Arc<StructureTemplateManager>,
    pub generator: Arc<NoiseBasedChunkGenerator>,
    pub factory: PalettedContainerFactory,
    /// Whether the vanilla feature registries decoded completely (`false`: empty registries are used).
    pub registries_loaded: bool,
    /// Whether `GenRegistries` alone (features, placed features, block state providers) decoded completely.
    pub features_loaded: bool,
}

pub static WORLD: LazyLock<FeatureWorld> = LazyLock::new(|| {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ref/src/data");
    let source: Arc<dyn WorldgenDataSource> = Arc::new(FsDataSource::new(root.clone()));
    let empty: Arc<dyn WorldgenDataSource> = Arc::new(FsDataSource::new(root.join("no_such_data_dir")));
    let worldgen = Arc::new(WorldgenRegistries::new(source.clone()));
    let biomes = Arc::new(BiomeRegistry::load(&worldgen).expect("vanilla biomes load"));
    // Registries of types that are not ported yet fail to decode: those tests then run with empty ones.
    let mut registries_loaded = true;
    let mut features_loaded = true;
    let registries = GenRegistries::load(source.clone(), &worldgen, &biomes).unwrap_or_else(|e| {
        eprintln!("feature registries do not decode yet ({e}): using empty ones");
        registries_loaded = false;
        features_loaded = false;
        GenRegistries::load(empty.clone(), &worldgen, &biomes).expect("empty registries")
    });
    let structures = StructureRegistries::load(source, &worldgen, &biomes).unwrap_or_else(|e| {
        eprintln!("structure registries do not decode yet ({e}): using empty ones");
        registries_loaded = false;
        StructureRegistries::load(empty, &worldgen, &biomes).expect("empty structure registries")
    });
    let settings = worldgen.noise_settings(&Identifier::with_default_namespace("overworld").unwrap()).expect("overworld noise settings");
    let generator_source = biome_source_from_json(&serde_json::json!({"type": "minecraft:multi_noise", "preset": "minecraft:overworld"}), &worldgen, &biomes)
        .expect("overworld biome source");
    let generator = NoiseBasedChunkGenerator::new(generator_source, Arc::new(settings), worldgen.clone(), biomes.clone()).expect("overworld generator");
    let factory = PalettedContainerFactory::create(mc_world::palette::BiomeRegistry::new(biomes.names())).expect("plains");
    // The vanilla data pack (`ref/src/data`), e.g. for the structure templates; generated structures go to a scratch dir.
    let pack = PathPackResources::new(
        PackLocationInfo { id: "vanilla".into(), title: Component::literal("vanilla"), source: PackSource::BuiltIn, known_pack_info: None },
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ref/src"),
    );
    let resources = Arc::new(MultiPackResourceManager::new(PackType::ServerData, vec![Arc::new(pack)]));
    let templates = Arc::new(StructureTemplateManager::new(resources, std::env::temp_dir().join("mc-feature-harness-generated")));
    FeatureWorld { worldgen, biomes, registries: Arc::new(registries), structures: Arc::new(structures), templates, generator: Arc::new(generator), factory, registries_loaded, features_loaded }
});

pub fn level_height() -> LevelHeight {
    LevelHeight::new(MIN_Y, HEIGHT)
}

/// The `GenEnv` of the overworld with `seed` (real generator + random state).
pub fn gen_env(seed: i64) -> Arc<GenEnv> {
    let w = &*WORLD;
    Arc::new(GenEnv {
        dimension: Identifier::with_default_namespace("overworld").unwrap(),
        seed,
        generator: w.generator.clone(),
        random_state: Arc::new(RandomState::create(w.worldgen.clone(), seed, w.generator.settings())),
        biomes: w.biomes.clone(),
        registries: w.registries.clone(),
        structures: w.structures.clone(),
        start_cache: StartCache::default(),
        structure_state: None,
        structure_check: StructureCheck::new(Box::new(NoChunkScan)),
        structure_template_manager: w.templates.clone(),
        generate_structures: true,
        level_height: level_height(),
    })
}

fn biome_id(name: &str) -> BiomeId {
    WORLD.biomes.id_of_name(name).unwrap_or_else(|| panic!("biome {name}"))
}

/// A synthetic chunk with the final heightmaps (and, with `prime_wg`, the two worldgen ones) primed and the given persisted
/// status.
pub fn make_chunk(seed: i64, cx: i32, cz: i32, status: ChunkStatus, prime_wg: bool) -> ProtoChunk {
    let mut chunk = ProtoChunk::new(ChunkPos::new(cx, cz), UpgradeData::EMPTY, level_height(), &WORLD.factory, None);
    for si in 0..(HEIGHT / 16) {
        let base_y = MIN_Y + si * 16;
        let section = chunk.access.get_section_mut(si as usize);
        for y in 0..16 {
            for z in 0..16 {
                for x in 0..16 {
                    let state = terrain(seed, cx * 16 + x, base_y + y, cz * 16 + z);
                    if !state.is_air() {
                        section.set_block_state(x, y, z, state);
                    }
                }
            }
        }
    }
    chunk.access.fill_biomes_from_noise(level_height(), |qx, qy, qz| biome_id(biome_name(seed, qx, qy, qz)));
    chunk.set_persisted_status(status);
    chunk.access.prime_heightmaps(true, FINAL_HEIGHTMAPS);
    if prime_wg {
        chunk.access.prime_heightmaps(true, &[HeightmapType::WorldSurfaceWg, HeightmapType::OceanFloorWg]);
    }
    chunk
}

/// A region over synthetic chunks, set up for the generation step that produces `step_status`
/// (`ChunkPyramid.GENERATION_PYRAMID`), plus the `GenEnv` of its seed.
pub struct SynthRegion {
    pub region: WorldGenRegion,
    pub env: Arc<GenEnv>,
    pub step: &'static ChunkStep,
    pub seed: i64,
    pub cx: i32,
    pub cz: i32,
    pub radius: i32,
}

impl SynthRegion {
    /// `radius`: 1 = 3x3 chunks, 2 = 5x5.
    pub fn new(seed: i64, cx: i32, cz: i32, step_status: ChunkStatus, radius: i32) -> SynthRegion {
        Self::with_wg_heightmaps(seed, cx, cz, step_status, radius, true)
    }

    /// `prime_wg = false`: the chunks lack the worldgen heightmaps (vanilla primes them on first read).
    pub fn with_wg_heightmaps(seed: i64, cx: i32, cz: i32, step_status: ChunkStatus, radius: i32, prime_wg: bool) -> SynthRegion {
        let step = GENERATION_PYRAMID.get_step_to(step_status);
        let mut chunks = HashMap::new();
        for x in cx - radius..=cx + radius {
            for z in cz - radius..=cz + radius {
                chunks.insert(ChunkPos::pack_xz(x, z), GenChunk::Loaned(Box::new(make_chunk(seed, x, z, ChunkStatus::Terrain, prime_wg))));
            }
        }
        let region = WorldGenRegion::new(
            ChunkPos::new(cx, cz),
            step.direct_dependencies.get_radius(),
            step.block_state_write_radius,
            chunks,
            HashMap::new(),
            level_height(),
        );
        SynthRegion { region, env: gen_env(seed), step, seed, cx, cz, radius }
    }

    /// The `GenLevel` (vanilla `WorldGenRegion`) over the lent chunks.
    pub fn level(&mut self) -> GenLevel<'_> {
        GenLevel::new(&mut self.region, self.env.clone()).with_step(self.step)
    }
}

// ---- state strings / dump ----

/// `minecraft:oak_stairs[facing=north,half=top]` -> state (properties not given keep their default).
pub fn parse_state(s: &str) -> BlockState {
    let (name, props) = match s.split_once('[') {
        Some((name, props)) => (name, props.trim_end_matches(']')),
        None => (s, ""),
    };
    let mut state = Block::by_name(name).unwrap_or_else(|| panic!("block {name}")).default_state();
    for kv in props.split(',').filter(|p| !p.is_empty()) {
        let (k, v) = kv.split_once('=').expect("property=value");
        state = state.with_property_str(k, v).unwrap_or_else(|| panic!("property {kv} of {name}"));
    }
    state
}

/// `BlockStateParser.serialize(state)`.
pub fn state_string(state: BlockState) -> String {
    mc_game::commands::arguments::blocks::block_state_parser::serialize(state)
}

pub fn heightmap_name(ty: HeightmapType) -> &'static str {
    match ty {
        HeightmapType::WorldSurfaceWg => "WORLD_SURFACE_WG",
        HeightmapType::WorldSurface => "WORLD_SURFACE",
        HeightmapType::OceanFloorWg => "OCEAN_FLOOR_WG",
        HeightmapType::OceanFloor => "OCEAN_FLOOR",
        HeightmapType::MotionBlocking => "MOTION_BLOCKING",
        HeightmapType::MotionBlockingNoLeaves => "MOTION_BLOCKING_NO_LEAVES",
    }
}

pub fn heightmap_by_name(name: &str) -> HeightmapType {
    [
        HeightmapType::WorldSurfaceWg,
        HeightmapType::WorldSurface,
        HeightmapType::OceanFloorWg,
        HeightmapType::OceanFloor,
        HeightmapType::MotionBlocking,
        HeightmapType::MotionBlockingNoLeaves,
    ]
    .into_iter()
    .find(|t| heightmap_name(*t) == name)
    .unwrap_or_else(|| panic!("heightmap {name}"))
}

pub fn priority_name(p: TickPriority) -> &'static str {
    match p {
        TickPriority::ExtremelyHigh => "EXTREMELY_HIGH",
        TickPriority::VeryHigh => "VERY_HIGH",
        TickPriority::High => "HIGH",
        TickPriority::Normal => "NORMAL",
        TickPriority::Low => "LOW",
        TickPriority::VeryLow => "VERY_LOW",
        TickPriority::ExtremelyLow => "EXTREMELY_LOW",
    }
}

fn blocks_of(chunk: &ProtoChunk) -> Vec<BlockState> {
    let min = chunk.pos();
    let mut out = Vec::with_capacity((16 * HEIGHT * 16) as usize);
    for y in MIN_Y..MIN_Y + HEIGHT {
        for z in 0..16 {
            for x in 0..16 {
                out.push(chunk.get_block_state(BlockPos::new(min.get_min_block_x() + x, y, min.get_min_block_z() + z)));
            }
        }
    }
    out
}

/// The block states of every chunk of the region before the test body ran (the baseline of [`dump`]'s `B` lines).
pub struct Snapshot {
    cx: i32,
    cz: i32,
    radius: i32,
    blocks: HashMap<i64, Vec<BlockState>>,
}

pub fn snapshot(synth: &SynthRegion) -> Snapshot {
    let mut blocks = HashMap::new();
    for x in synth.cx - synth.radius..=synth.cx + synth.radius {
        for z in synth.cz - synth.radius..=synth.cz + synth.radius {
            blocks.insert(ChunkPos::pack_xz(x, z), blocks_of(synth.region.get_chunk(x, z).expect("chunk of the region")));
        }
    }
    Snapshot { cx: synth.cx, cz: synth.cz, radius: synth.radius, blocks }
}

/// The compact dump documented on `FeatureOracleBase.dump` (same lines, same order) of every chunk of the region.
pub fn dump(level: &mut GenLevel<'_>, before: &Snapshot) -> Vec<String> {
    let mut out = Vec::new();
    for cx in before.cx - before.radius..=before.cx + before.radius {
        for cz in before.cz - before.radius..=before.cz + before.radius {
            let chunk = level.region().get_chunk(cx, cz).expect("chunk of the region");
            let was = &before.blocks[&ChunkPos::pack_xz(cx, cz)];
            out.push(format!("C {cx} {cz} {}", chunk.get_persisted_status().name()));
            let mut t = Fnv::default();
            for s in was {
                t.i(s.id() as i32);
            }
            out.push(format!("T {cx} {cz} {} {}", t.hex(), t.count));
            let mut m = Fnv::default();
            let (qx0, qz0) = (chunk.pos().get_min_block_x() >> 2, chunk.pos().get_min_block_z() >> 2);
            for qy in (MIN_Y >> 2)..((MIN_Y + HEIGHT) >> 2) {
                for qz in 0..4 {
                    for qx in 0..4 {
                        m.s(WORLD.biomes.key(chunk.access.get_noise_biome(qx0 + qx, qy, qz0 + qz)).path());
                    }
                }
            }
            out.push(format!("M {cx} {cz} {} {}", m.hex(), m.count));
            for (i, (now, was)) in blocks_of(chunk).into_iter().zip(was).enumerate() {
                if now != *was {
                    let (x, z, y) = (cx * 16 + (i & 15) as i32, cz * 16 + ((i >> 4) & 15) as i32, MIN_Y + (i >> 8) as i32);
                    out.push(format!("B {x} {y} {z} {}", state_string(now)));
                }
            }
            let mut bes: Vec<_> = chunk.access.block_entities.iter().collect();
            bes.sort_by_key(|(p, _)| (p.x(), p.y(), p.z()));
            for (p, tag) in bes {
                out.push(format!("E {} {} {} {}", p.x(), p.y(), p.z(), mc_nbt::Tag::Compound(tag.clone())));
            }
            let mut maps: Vec<_> = chunk.access.heightmaps.iter().map(|(ty, map)| (heightmap_name(*ty), map)).collect();
            maps.sort_by_key(|(name, _)| *name);
            for (name, map) in maps {
                let mut f = Fnv::default();
                for l in map.get_raw_data() {
                    f.l(*l);
                }
                out.push(format!("H {cx} {cz} {name} {}", f.hex()));
            }
            for (si, list) in chunk.access.post_processing.iter().enumerate() {
                if let Some(list) = list
                    && !list.is_empty()
                {
                    let joined: Vec<String> = list.iter().map(i16::to_string).collect();
                    out.push(format!("P {cx} {cz} {si} {}", joined.join(",")));
                }
            }
            for t in chunk.block_ticks.scheduled_ticks() {
                out.push(format!("K b {} {} {} {} {} {}", t.pos.x(), t.pos.y(), t.pos.z(), t.ty.name(), t.delay, priority_name(t.priority)));
            }
            for t in chunk.fluid_ticks.scheduled_ticks() {
                out.push(format!("K f {} {} {} {} {} {}", t.pos.x(), t.pos.y(), t.pos.z(), t.ty.name(), t.delay, priority_name(t.priority)));
            }
            for e in &chunk.entities {
                out.push(format!("N {cx} {cz} {}", mc_nbt::Tag::Compound(e.clone())));
            }
        }
    }
    out.push(format!("G {}", level.random().next_long()));
    out
}
