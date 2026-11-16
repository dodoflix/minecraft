//! Entities are stored in `entities/r.X.Z.mca` when their chunk unloads, on `save`, and when the level closes, and come
//! back (with their passengers) when the chunk loads again, also in a second level over the same folder.

use mc_core::ident::Identifier;
use mc_core::pos::ChunkPos;
use mc_game::server::level::world_gen::{ChunkGeneratorBackend, WorldGenRegion};
use mc_game::server::level::{ServerLevel, ServerLevelParams};
use mc_game::server::shared::ServerShared;
use mc_game::world::entity::factory::{self, EntitySpawnRequest};
use mc_game::world::entity::{EntityId, entity as entity_mod};
use mc_nbt::CompoundTag;
use mc_world::chunk::{LevelHeight, ProtoChunk};
use mc_world::palette::{BiomeRegistry, PalettedContainerFactory};
use mc_world::region::{EntityStorage, RegionStorageInfo, SimpleRegionStorage};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

struct VoidGenerator;

impl ChunkGeneratorBackend for VoidGenerator {
    fn type_name_for_data_fixer(&self) -> Option<Identifier> {
        None
    }
    fn create_biomes(&self, _chunk: &mut ProtoChunk, _region: &WorldGenRegion) {}
    fn build_terrain(&self, _chunk: &mut ProtoChunk, _region: &WorldGenRegion) {}
    fn get_base_height(
        &self,
        _x: i32,
        _z: i32,
        _ty: mc_world::heightmap::HeightmapType,
        height: LevelHeight,
    ) -> i32 {
        height.min_y
    }
}

fn open(dir: &Path) -> ServerLevel {
    let (shared, _rx) = ServerShared::new();
    shared.bind_game_thread();
    open_with(dir, shared)
}

pub(crate) fn open_with(dir: &Path, shared: Arc<ServerShared>) -> ServerLevel {
    let biomes = BiomeRegistry::new(vec!["minecraft:plains".to_string()]);
    ServerLevel::new(ServerLevelParams {
        shared,
        dimension: Identifier::parse("minecraft:overworld").unwrap(),
        level_id: "entities".into(),
        dimension_dir: dir.to_path_buf(),
        level_height: LevelHeight {
            min_y: -64,
            height: 384,
        },
        sea_level: 63,
        has_sky_light: true,
        factory: Arc::new(PalettedContainerFactory::create(biomes).unwrap()),
        generator: Arc::new(VoidGenerator),
        generate_structures: false,
        hooks: Arc::new(mc_game::server::level::world_gen::NoWorldGenHooks),
        seed: 0,
        is_known_structure: Arc::new(|_| false),
        view_distance: 4,
        simulation_distance: 4,
        sync_writes: false,
        is_debug: false,
        level_data: Box::new(mc_game::server::level::level_data::PrimaryServerLevelData(
            mc_game::server::level::level_data::default_world_data(),
        )),
    })
}

/// One server tick's worth of chunk and entity management.
pub(crate) fn step(level: &mut ServerLevel) {
    if !level.poll_chunk_task() {
        std::thread::sleep(Duration::from_millis(1));
    }
    level.tick_chunk_source(&|| true, false, true);
    level.tick_entity_management();
}

pub(crate) fn run_until(level: &mut ServerLevel, mut done: impl FnMut(&mut ServerLevel) -> bool) {
    let start = Instant::now();
    while !done(level) {
        step(level);
        assert!(start.elapsed() < Duration::from_secs(30), "timed out");
    }
}

pub(crate) fn load_chunk(level: &mut ServerLevel, pos: ChunkPos) {
    level.chunk_source.update_chunk_forced(pos, true);
    run_until(level, |l| {
        l.chunk_now(pos.x, pos.z).is_some() && l.entity_manager.are_entities_loaded(pos.pack())
    });
}

fn unload_chunk(level: &mut ServerLevel, pos: ChunkPos) {
    level.chunk_source.update_chunk_forced(pos, false);
    run_until(level, |l| {
        l.chunk_source.get_loaded_chunks_count() == 0 && l.entity_manager.get_all().next().is_none()
    });
}

pub(crate) const PIG: &str =
    r#"{id:"minecraft:pig",UUID:[I;1,1,1,1],Pos:[8.5d,64.0d,8.5d],CustomName:'"Wilbur"'}"#;
const ITEM: &str = r#"{id:"minecraft:item",UUID:[I;2,2,2,2],Pos:[9.5d,64.0d,3.5d],Item:{id:"minecraft:diamond",count:3},Age:5s}"#;
const FRAME: &str = r#"{id:"minecraft:item_frame",UUID:[I;3,3,3,3],Pos:[4.5d,64.5d,4.5d],block_pos:[I;4,64,4],facing:2b,Item:{id:"minecraft:apple",count:1},ItemRotation:3b}"#;
const MINECART: &str = r#"{id:"minecraft:minecart",UUID:[I;4,4,4,4],Pos:[2.5d,64.0d,12.5d],Passengers:[{id:"minecraft:zombie",UUID:[I;5,5,5,5],Pos:[2.5d,64.0d,12.5d],Health:7.0f}]}"#;
const VILLAGER: &str = r#"{id:"minecraft:villager",UUID:[I;6,6,6,6],Pos:[12.5d,64.0d,12.5d],VillagerData:{type:"minecraft:plains",profession:"minecraft:farmer",level:2},Xp:11}"#;

pub(crate) fn spawn(level: &mut ServerLevel, snbt: &str) -> EntityId {
    let tag = mc_nbt::snbt::parse_compound(snbt).expect("snbt");
    let mut loaded = Vec::new();
    factory::load_entity_recursive(
        &tag,
        level,
        EntitySpawnRequest {
            reason: mc_data::enums::EntitySpawnReason::Load,
            ignore_checks: false,
        },
        &mut loaded,
    );
    let id = loaded.first().expect("entity loads").id();
    level.add_fresh_entity_with_passengers(loaded);
    id
}

/// The saved NBT of every root entity, ordered by its UUID.
fn root_tags(level: &ServerLevel) -> Vec<CompoundTag> {
    let lookup = |id: EntityId| level.entity_manager.get_any(id);
    let mut tags: Vec<CompoundTag> = level
        .entity_manager
        .get_all()
        .filter(|e| !e.base().is_passenger())
        .filter_map(|e| entity_mod::save(e, &lookup))
        .collect();
    tags.sort_by_key(|t| format!("{:?}", t.get("UUID")));
    tags
}

fn stored_tags(dir: &Path, pos: ChunkPos) -> Vec<CompoundTag> {
    let mut storage = EntityStorage::new(SimpleRegionStorage::new(
        RegionStorageInfo::new(
            "entities",
            Identifier::parse("minecraft:overworld").unwrap(),
            "entities",
        ),
        dir.join("entities"),
        false,
    ));
    let mut tags = storage
        .load_entities(&pos)
        .unwrap()
        .chunk
        .get_entities()
        .to_vec();
    tags.sort_by_key(|t| format!("{:?}", t.get("UUID")));
    tags
}

fn populate(level: &mut ServerLevel) -> Vec<CompoundTag> {
    for snbt in [PIG, ITEM, FRAME, MINECART, VILLAGER] {
        spawn(level, snbt);
    }
    let tags = root_tags(level);
    assert_eq!(
        tags.len(),
        5,
        "pig, item, frame, minecart (+zombie), villager"
    );
    tags
}

#[test]
fn entities_survive_a_chunk_unload() {
    let scratch = tempfile::tempdir().unwrap();
    let pos = ChunkPos::new(0, 0);
    let mut level = open(scratch.path());
    load_chunk(&mut level, pos);
    let before = populate(&mut level);

    unload_chunk(&mut level, pos);
    assert!(level.entity_manager.get_all().next().is_none());

    load_chunk(&mut level, pos);
    assert_eq!(root_tags(&level), before);
    let minecart = level
        .entity_manager
        .get_all()
        .find(|e| e.get_type() == mc_data::EntityType::MINECART)
        .expect("minecart");
    assert_eq!(
        minecart.base().passengers.len(),
        1,
        "the zombie rides again"
    );
    level.close();
}

#[test]
fn entities_survive_a_restart() {
    let scratch = tempfile::tempdir().unwrap();
    let pos = ChunkPos::new(0, 0);
    let before;
    {
        let mut level = open(scratch.path());
        load_chunk(&mut level, pos);
        before = populate(&mut level);
        // the forced chunk is still loaded: the stop sequence stores its entities
        level.chunk_source.deactivate_tickets_on_closing();
        level.close();
    }
    assert!(scratch.path().join("entities/r.0.0.mca").is_file());
    assert_eq!(stored_tags(scratch.path(), pos), before);
    {
        let mut level = open(scratch.path());
        level.chunk_source.activate_all_deactivated_tickets();
        run_until(&mut level, |l| {
            l.chunk_now(pos.x, pos.z).is_some() && l.entity_manager.are_entities_loaded(pos.pack())
        });
        assert_eq!(root_tags(&level), before);
        level.close();
    }
}

#[test]
fn save_without_close_writes_the_entities() {
    let scratch = tempfile::tempdir().unwrap();
    let pos = ChunkPos::new(0, 0);
    let mut level = open(scratch.path());
    load_chunk(&mut level, pos);
    let before = populate(&mut level);
    level.save(true);
    assert_eq!(stored_tags(scratch.path(), pos), before);
    level.close();
}

#[test]
fn autosave_queues_the_entities_and_the_ticks_write_them() {
    let scratch = tempfile::tempdir().unwrap();
    let pos = ChunkPos::new(0, 0);
    let mut level = open(scratch.path());
    load_chunk(&mut level, pos);
    let before = populate(&mut level);
    level.save(false);
    // the entity storage's IO drains between the ticks, no flush involved
    for _ in 0..3 {
        step(&mut level);
    }
    assert_eq!(stored_tags(scratch.path(), pos), before);
    level.close();
}

#[test]
fn an_unloaded_chunk_reaches_the_file_without_a_save() {
    let scratch = tempfile::tempdir().unwrap();
    let pos = ChunkPos::new(0, 0);
    let mut level = open(scratch.path());
    load_chunk(&mut level, pos);
    let before = populate(&mut level);
    unload_chunk(&mut level, pos);
    for _ in 0..3 {
        step(&mut level);
    }
    assert_eq!(stored_tags(scratch.path(), pos), before);
    level.close();
}

#[test]
fn entities_of_a_chunk_that_never_loaded_are_stored_by_a_save() {
    let scratch = tempfile::tempdir().unwrap();
    let pos = ChunkPos::new(0, 0);
    let mut level = open(scratch.path());
    let pig = spawn(&mut level, PIG);
    let lookup = |id: EntityId| level.entity_manager.get_any(id);
    let before = vec![entity_mod::save(lookup(pig).expect("pig"), &lookup).expect("saves")];
    level.save(true);
    assert_eq!(stored_tags(scratch.path(), pos), before);
    level.close();
}

#[test]
fn a_save_while_an_entity_runs_keeps_it_in_the_file() {
    let scratch = tempfile::tempdir().unwrap();
    let pos = ChunkPos::new(0, 0);
    let mut level = open(scratch.path());
    load_chunk(&mut level, pos);
    let before = populate(&mut level);
    level.save(true);
    let pig = level
        .entity_manager
        .get_all()
        .find(|e| e.get_type() == mc_data::EntityType::PIG)
        .map(|e| e.id())
        .unwrap();
    // `/save-all` from a function or command block that runs inside the pig's own tick
    level.with_entity(pig, |level, _| level.save(true));
    assert_eq!(stored_tags(scratch.path(), pos), before);
    level.close();
}

#[test]
fn dimension_saved_data_is_written_by_a_flushed_save() {
    let scratch = tempfile::tempdir().unwrap();
    let pos = ChunkPos::new(3, 4);
    let mut level = open(scratch.path());
    level.chunk_source.update_chunk_forced(pos, true);
    run_until(&mut level, |l| l.chunk_now(3, 4).is_some());
    level.save(true);
    assert!(
        scratch
            .path()
            .join("data/minecraft/chunk_tickets.dat")
            .is_file(),
        "a flushed save writes the dimension saved data without closing the level"
    );
    level.close();
}

#[test]
fn a_changed_chunk_is_written_without_a_save() {
    let scratch = tempfile::tempdir().unwrap();
    let pos = ChunkPos::new(0, 0);
    let mut level = open(scratch.path());
    load_chunk(&mut level, pos);
    // eager saving (20 chunks a tick, 10 s cooldown per chunk) writes the generated chunk
    let start = Instant::now();
    while !scratch.path().join("region/r.0.0.mca").is_file()
        || std::fs::metadata(scratch.path().join("region/r.0.0.mca"))
            .unwrap()
            .len()
            == 0
    {
        step(&mut level);
        level.chunk_source.chunk_map.synchronize(true);
        assert!(start.elapsed() < Duration::from_secs(30), "never written");
    }
    level.close();
}
