//! Full world generation through the server's chunk pipeline (`ServerLevel` + chunk map + worldgen thread, the same
//! `GenEnv`/hooks wiring `level_lifecycle` builds) for the three vanilla dimensions: features and structures show up
//! in the generated chunks, and the structure starts/references written to the region files read back identically.
//!
//! Needs the vanilla data pack to decode completely (every feature/placement/structure type ported).

use mc_core::ident::Identifier;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_game::server::level::level_data::{PrimaryServerLevelData, default_world_data};
use mc_game::server::level::worldgen_adapter::NoiseWorldGenerator;
use mc_game::server::level::{ServerLevel, ServerLevelParams};
use mc_game::server::packs::repository::PackSource;
use mc_game::server::packs::resource_manager::MultiPackResourceManager;
use mc_game::server::packs::resources::PathPackResources;
use mc_game::server::packs::{PackLocationInfo, PackType};
use mc_game::server::shared::ServerShared;
use mc_game::world::level::levelgen::GenEnv;
use mc_game::world::level::levelgen::hooks::VanillaWorldGenHooks;
use mc_game::world::level::levelgen::registries::GenRegistries;
use mc_game::world::level::levelgen::structure::check::{NoChunkScan, StructureCheck};
use mc_game::world::level::levelgen::structure::generation::create_structure_state;
use mc_game::world::level::levelgen::structure::registries::{StartCache, StructureRegistries};
use mc_game::world::level::levelgen::structure::templatesystem::StructureTemplateManager;
use mc_text::Component;
use mc_world::chunk::{LevelHeight, StructureData};
use mc_world::palette::PalettedContainerFactory;
use mc_world::status::ChunkStatus;
use mc_worldgen::biome::{BiomeRegistry, biome_source_from_json};
use mc_worldgen::data::{FsDataSource, WorldgenDataSource, WorldgenRegistries};
use mc_worldgen::generator::{ChunkGenerator, NoiseBasedChunkGenerator};
use mc_worldgen::random_state::RandomState;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

struct Dimension {
    id: &'static str,
    min_y: i32,
    height: i32,
    has_sky_light: bool,
}

const OVERWORLD: Dimension = Dimension { id: "overworld", min_y: -64, height: 384, has_sky_light: true };
const NETHER: Dimension = Dimension { id: "the_nether", min_y: 0, height: 256, has_sky_light: false };
const END: Dimension = Dimension { id: "the_end", min_y: 0, height: 256, has_sky_light: false };

struct World {
    worldgen: Arc<WorldgenRegistries>,
    biomes: Arc<BiomeRegistry>,
    registries: Arc<GenRegistries>,
    structures: Arc<StructureRegistries>,
    templates: Arc<StructureTemplateManager>,
}

fn load_world() -> World {
    let data = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ref/src/data");
    let source: Arc<dyn WorldgenDataSource> = Arc::new(FsDataSource::new(data));
    let worldgen = Arc::new(WorldgenRegistries::new(source.clone()));
    let biomes = Arc::new(BiomeRegistry::load(&worldgen).unwrap());
    let registries = Arc::new(GenRegistries::load(source.clone(), &worldgen, &biomes).unwrap_or_else(|e| panic!("feature registries: {e}")));
    let structures = Arc::new(StructureRegistries::load(source, &worldgen, &biomes).unwrap_or_else(|e| panic!("structure registries: {e}")));
    let pack = PathPackResources::new(
        PackLocationInfo { id: "vanilla".into(), title: Component::literal("vanilla"), source: PackSource::BuiltIn, known_pack_info: None },
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ref/src"),
    );
    let resources = Arc::new(MultiPackResourceManager::new(PackType::ServerData, vec![Arc::new(pack)]));
    let templates = Arc::new(StructureTemplateManager::new(resources, std::env::temp_dir().join(format!("mc-worldgen-features-generated-{}", std::process::id()))));
    World { worldgen, biomes, registries, structures, templates }
}

/// The level `level_lifecycle::create_level` builds for the vanilla `normal` world preset's dimension.
fn open_level(world: &World, dimension: &Dimension, seed: i64, dir: &Path) -> ServerLevel {
    let preset = world.worldgen.raw("worldgen/world_preset", &Identifier::parse("minecraft:normal").unwrap()).unwrap();
    let generator_json = &preset["dimensions"][format!("minecraft:{}", dimension.id)]["generator"];
    let biome_source = biome_source_from_json(&generator_json["biome_source"], &world.worldgen, &world.biomes).unwrap();
    let settings_id = Identifier::parse(generator_json["settings"].as_str().unwrap()).unwrap();
    let settings = Arc::new(world.worldgen.noise_settings(&settings_id).unwrap());
    let generator = Arc::new(NoiseBasedChunkGenerator::new(biome_source.clone(), settings.clone(), world.worldgen.clone(), world.biomes.clone()).unwrap());
    let random_state = Arc::new(RandomState::create(world.worldgen.clone(), seed, &settings));
    let structure_state = create_structure_state(generator.as_ref(), biome_source, None, random_state.clone(), seed, &world.worldgen, &world.biomes).unwrap();
    let level_height = LevelHeight { min_y: dimension.min_y, height: dimension.height };
    let env = Arc::new(GenEnv {
        dimension: Identifier::parse(&format!("minecraft:{}", dimension.id)).unwrap(),
        seed,
        generator: generator.clone(),
        random_state: random_state.clone(),
        biomes: world.biomes.clone(),
        registries: world.registries.clone(),
        structure_template_manager: world.templates.clone(),
        structures: world.structures.clone(),
        start_cache: StartCache::default(),
        structure_state: Some(structure_state),
        structure_check: StructureCheck::new(Box::new(NoChunkScan)),
        generate_structures: true,
        level_height,
    });
    let backend = Arc::new(NoiseWorldGenerator::new(generator.clone(), random_state, seed, Identifier::parse("minecraft:noise").ok()));
    backend.set_gen_env(env.clone());
    let (shared, _rx) = ServerShared::new();
    let structures = world.structures.clone();
    ServerLevel::new(ServerLevelParams {
        shared,
        dimension: Identifier::parse(&format!("minecraft:{}", dimension.id)).unwrap(),
        level_id: dimension.id.into(),
        dimension_dir: dir.to_path_buf(),
        level_height,
        sea_level: generator.get_sea_level(),
        has_sky_light: dimension.has_sky_light,
        factory: Arc::new(PalettedContainerFactory::create(mc_world::palette::BiomeRegistry::new(world.biomes.names())).unwrap()),
        generator: backend,
        generate_structures: true,
        hooks: Arc::new(VanillaWorldGenHooks::new(env)),
        seed,
        is_known_structure: Arc::new(move |id| Identifier::try_parse(id).is_some_and(|id| structures.get(&id).is_some())),
        view_distance: 10,
        simulation_distance: 10,
        sync_writes: true,
        is_debug: false,
        level_data: Box::new(PrimaryServerLevelData(default_world_data())),
    })
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mc-worldgen-features-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// Names of the blocks in the chunk (`minecraft:` path only).
fn block_names(level: &mut ServerLevel, pos: ChunkPos, height: LevelHeight) -> BTreeSet<String> {
    let chunk = level.chunk_blocking_mut(pos.x, pos.z);
    let mut names = BTreeSet::new();
    for y in height.min_y..height.min_y + height.height {
        for z in 0..16 {
            for x in 0..16 {
                names.insert(chunk.data.get_block_state(BlockPos::new(pos.x * 16 + x, y, pos.z * 16 + z)).block().data().name.to_string());
            }
        }
    }
    names
}

/// `structures` of the chunk (starts + references) as the chunk map holds it.
fn structure_data(level: &mut ServerLevel, pos: ChunkPos) -> StructureData {
    level.chunk_blocking_mut(pos.x, pos.z).data.access.structures.clone()
}

#[test]
fn overworld_has_trees_ores_flowers_and_structure_data_round_trips() {
    let world = load_world();
    let dir = scratch("overworld");
    let seed = 2;
    let height = LevelHeight { min_y: OVERWORLD.min_y, height: OVERWORLD.height };
    let mut level = open_level(&world, &OVERWORLD, seed, &dir);
    let mut names = BTreeSet::new();
    let area: Vec<ChunkPos> = (-4..4).flat_map(|z| (-4..4).map(move |x| ChunkPos::new(x, z))).collect();
    for pos in &area {
        names.extend(block_names(&mut level, *pos, height));
    }
    let has = |needle: &str| names.iter().any(|n| n.contains(needle));
    assert!(has("_log"), "no trees: {names:?}");
    assert!(has("_leaves"), "no trees: {names:?}");
    assert!(has("coal_ore") && has("iron_ore"), "no ores: {names:?}");
    assert!(has("granite") || has("diorite") || has("andesite"), "no stone blobs: {names:?}");
    assert!(has("grass") || has("fern") || has("poppy") || has("dandelion") || has("tulip"), "no ground vegetation: {names:?}");

    // structure starts/references: generated (FULL chunks around the origin, STRUCTURE_REFERENCES further out), saved, and
    // read back from the region files by a second level
    let outside: Vec<ChunkPos> = (-24..24).flat_map(|z| (-24..24).map(move |x| ChunkPos::new(x, z))).filter(|p| !area.contains(p)).collect();
    let capture = |level: &mut ServerLevel| -> Vec<(ChunkPos, StructureData)> {
        let mut all: Vec<_> = area.iter().map(|pos| (*pos, structure_data(level, *pos))).collect();
        let proto: Vec<_> = outside
            .iter()
            .map(|pos| (*pos, level.proto_chunk_blocking(pos.x, pos.z, ChunkStatus::StructureReferences, true).expect("structure references").access.structures.clone()))
            .collect();
        all.extend(proto);
        all
    };
    let before = capture(&mut level);
    assert!(before.iter().any(|(_, data)| !data.starts.is_empty()), "no structure start in the area");
    assert!(before.iter().any(|(_, data)| !data.references.is_empty()), "no structure reference in the area");
    level.close();
    let mut reloaded = open_level(&world, &OVERWORLD, seed, &dir);
    for (pos, expected) in &capture(&mut reloaded) {
        let (_, before) = before.iter().find(|(p, _)| p == pos).unwrap();
        assert_eq!(expected, before, "structure data of {pos} after reload");
    }
    reloaded.close();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Starts of the chunks in `[x0, x0+n) x [z0, z0+n)` (`STRUCTURE_STARTS` needs no terrain).
fn start_ids(level: &mut ServerLevel, x0: i32, z0: i32, n: i32) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for z in z0..z0 + n {
        for x in x0..x0 + n {
            let chunk = level.proto_chunk_blocking(x, z, ChunkStatus::StructureStarts, true).expect("structure starts generated");
            ids.extend(chunk.access.structures.starts.keys().cloned());
        }
    }
    ids
}

#[test]
fn nether_records_fortress_and_bastion_starts_and_has_features() {
    let world = load_world();
    let dir = scratch("nether");
    let seed = 2;
    let mut level = open_level(&world, &NETHER, seed, &dir);
    let starts = start_ids(&mut level, -40, -40, 80);
    assert!(starts.contains("minecraft:fortress"), "{starts:?}");
    assert!(starts.contains("minecraft:bastion_remnant"), "{starts:?}");
    let height = LevelHeight { min_y: NETHER.min_y, height: NETHER.height };
    let mut names = BTreeSet::new();
    for pos in (-2..2).flat_map(|z| (-2..2).map(move |x| ChunkPos::new(x, z))) {
        names.extend(block_names(&mut level, pos, height));
    }
    assert!(names.iter().any(|n| n.contains("quartz_ore") || n.contains("nether_gold_ore")), "{names:?}");
    assert!(names.iter().any(|n| n.contains("glowstone") || n.contains("magma") || n.contains("fire")), "{names:?}");
    level.close();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn end_has_spikes_islands_and_end_cities() {
    let world = load_world();
    let dir = scratch("end");
    let seed = 2;
    let mut level = open_level(&world, &END, seed, &dir);
    let height = LevelHeight { min_y: END.min_y, height: END.height };
    let mut names = BTreeSet::new();
    for pos in (-3..3).flat_map(|z| (-3..3).map(move |x| ChunkPos::new(x, z))) {
        names.extend(block_names(&mut level, pos, height));
    }
    assert!(names.contains("minecraft:end_stone"), "{names:?}");
    assert!(names.contains("minecraft:obsidian"), "end spikes: {names:?}");
    // outer islands start 1000+ blocks from the center; their cities are structures of the outer end
    let starts = start_ids(&mut level, 64, 0, 160);
    assert!(starts.contains("minecraft:end_city"), "{starts:?}");
    level.close();
    let _ = std::fs::remove_dir_all(&dir);
}
