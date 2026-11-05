//! Shared helpers: run the world-load pipeline against a world directory and capture log output.
#![allow(dead_code)]

use std::path::Path;
use std::sync::Mutex;

use mc_game::registry::RegistryAccess;
use mc_game::server::packs::ServerPacksSource;
use mc_game::server::world_loader::{self, DataLoadOutput, PackConfig, WorldStem};
use mc_world::level_storage::{DirectoryValidator, WorldDataConfiguration};

/// Tests that look at captured logs hold this for their whole body.
pub static SERIAL: Mutex<()> = Mutex::new(());
static LOGS: Mutex<Vec<String>> = Mutex::new(Vec::new());
static INIT: std::sync::Once = std::sync::Once::new();

struct TestLogger;

impl log::Log for TestLogger {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, r: &log::Record) {
        LOGS.lock().unwrap().push(format!("{}: {}", r.level(), r.args()));
    }
    fn flush(&self) {}
}

/// Install the capturing logger (once per test binary). Tests using it must run serially (`--test-threads=1` or one test).
pub fn init_logger() {
    INIT.call_once(|| {
        log::set_logger(&TestLogger).unwrap();
        log::set_max_level(log::LevelFilter::Trace);
    });
}

pub fn take_logs() -> Vec<String> {
    std::mem::take(&mut *LOGS.lock().unwrap())
}

pub fn load_world(world: &Path, config: WorldDataConfiguration, init_mode: bool) -> Result<(WorldDataConfiguration, WorldStem<()>), world_loader::WorldLoadError> {
    let mut repo = ServerPacksSource::create_pack_repository(world.join("datapacks"), DirectoryValidator::no_symlinks_allowed());
    let mut pc = PackConfig { pack_repository: &mut repo, initial_data_config: config, safe_mode: false, init_mode };
    world_loader::load(
        &mut pc,
        |_ctx| DataLoadOutput { cookie: (), final_dimensions: RegistryAccess::empty() },
        |rm, managers, layers, cookie| WorldStem::new(rm, managers, layers, cookie),
    )
}

// ---- test data packs (shared by the vanilla oracle generator and the comparison tests) -----------------------------

use std::fs;
use std::io::Write;

fn write(path: impl AsRef<Path>, content: &str) {
    let path = path.as_ref();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

const VANILLA_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../ref/src/data/minecraft");

fn vanilla_json(rel: &str) -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(format!("{VANILLA_ROOT}/{rel}")).unwrap()).unwrap()
}

/// The data packs of the "custom" oracle scenario: valid packs (directory + zip) with new entries and a tag `replace`,
/// plus packs with unusable metadata and non-pack entries.
pub fn write_custom_packs(world: &Path) {
    let dp = world.join("datapacks");
    fs::create_dir_all(&dp).unwrap();
    write(dp.join("good/pack.mcmeta"), r##"{"pack": {"description": "good pack", "min_format": 121, "max_format": 121}}"##);
    let mut dim = vanilla_json("dimension_type/overworld.json");
    dim["min_y"] = 0.into();
    dim["height"] = 256.into();
    dim["logical_height"] = 128.into();
    dim["coordinate_scale"] = 2.5.into();
    dim["ambient_light"] = 0.25.into();
    dim["monster_spawn_light_level"] = serde_json::json!({"type": "minecraft:uniform", "min_inclusive": 1, "max_inclusive": 9});
    dim["attributes"]["minecraft:visual/sky_color"] = "#112233".into();
    dim["attributes"]["minecraft:visual/fog_end_distance"] = 100.into();
    write(dp.join("good/data/test/dimension_type/custom.json"), &dim.to_string());
    let mut biome = vanilla_json("worldgen/biome/plains.json");
    biome["temperature"] = 0.3.into();
    biome["downfall"] = 1.into();
    biome["effects"]["water_color"] = "#abcdef".into();
    biome["effects"]["grass_color_modifier"] = "dark_forest".into();
    write(dp.join("good/data/test/worldgen/biome/custom.json"), &biome.to_string());
    write(
        dp.join("good/data/test/damage_type/custom.json"),
        r#"{"message_id": "custom", "scaling": "always", "exhaustion": 0.25, "effects": "freezing", "death_message_type": "fall_variants"}"#,
    );
    write(
        dp.join("good/data/test/chat_type/custom.json"),
        r##"{"chat": {"translation_key": "chat.type.custom", "parameters": ["sender", "content"], "style": {"color": "red", "italic": true}}, "narration": {"translation_key": "n", "parameters": ["content"]}}"##,
    );
    write(dp.join("good/data/minecraft/tags/block/wool.json"), r#"{"replace": true, "values": ["minecraft:stone", {"id": "test:missing", "required": false}]}"#);
    write(dp.join("good/data/minecraft/tags/block/mineable/pickaxe.json"), r##"{"values": ["#test:extra"]}"##);
    write(dp.join("good/data/test/tags/block/extra.json"), r#"{"values": ["minecraft:dirt", "minecraft:dirt"]}"#);
    write(dp.join("good/data/minecraft/tags/worldgen/biome/is_forest.json"), r#"{"values": ["test:custom"]}"#);
    // zip pack
    {
        let file = fs::File::create(dp.join("zipped.zip")).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default();
        zip.start_file("pack.mcmeta", opts).unwrap();
        zip.write_all(br#"{"pack": {"description": {"text": "zipped", "color": "gold"}, "min_format": 121, "max_format": [121, 5]}}"#).unwrap();
        zip.start_file("data/zip/damage_type/zipped.json", opts).unwrap();
        zip.write_all(br#"{"message_id": "zipped", "scaling": "never", "exhaustion": 0.0}"#).unwrap();
        zip.finish().unwrap();
    }
    write(dp.join("badmeta/pack.mcmeta"), "{ not json");
    write(dp.join("oldformat/pack.mcmeta"), r#"{"pack": {"description": "old", "pack_format": 5}}"#);
    write(dp.join("badformat/pack.mcmeta"), r#"{"pack": {"description": "bad", "pack_format": 100}}"#);
    write(dp.join("nodesc/pack.mcmeta"), r#"{"pack": {"pack_format": 48}}"#);
    write(dp.join("nopack/pack.mcmeta"), r#"{"other": {}}"#);
    write(dp.join("plain/readme.txt"), "hello");
    write(dp.join("notes.txt"), "hello");
}

/// A world whose only pack contains entries that must fail the load.
pub fn write_broken_packs(world: &Path) {
    let dp = world.join("datapacks");
    write(dp.join("broken/pack.mcmeta"), r##"{"pack": {"description": "broken", "min_format": 121, "max_format": 121}}"##);
    write(dp.join("broken/data/test/damage_type/bad_type.json"), r#"{"message_id": 5, "scaling": "sideways", "exhaustion": "x"}"#);
    write(dp.join("broken/data/test/damage_type/bad_syntax.json"), "{\"message_id\": ");
    write(dp.join("broken/data/test/chat_type/missing_field.json"), r#"{"chat": {"translation_key": "x", "parameters": []}}"#);
}

pub mod entity_oracle;
pub mod feature_harness;
pub mod structure_golden;
pub mod block_scenario;
pub mod entity_oracle;
