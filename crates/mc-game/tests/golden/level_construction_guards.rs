//! Guards of what a level and its saved data are built with: a dimension type with `has_ender_dragon_fight` always gets a
//! fight, and every `SavedDataType` of the vanilla sources has a Rust counterpart (the table only ever grows with vanilla).
use crate::common::block_scenario::load_dimension_type;
use crate::common::feature_harness::gen_env;
use mc_core::ident::Identifier;
use mc_game::server::level::ServerLevel;
use std::path::{Path, PathBuf};

/// The file names (without `.json`) of the vanilla dimension types.
fn dimension_types() -> Vec<String> {
    let dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ref/src/data/minecraft/dimension_type");
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("read the vanilla dimension types")
        .map(|e| {
            let path = e.expect("directory entry").path();
            path.file_stem().unwrap().to_string_lossy().into_owned()
        })
        .collect();
    names.sort();
    names
}

#[test]
fn a_dimension_type_with_a_fight_gets_a_fight() {
    let mut with_fight = Vec::new();
    for name in dimension_types() {
        let dimension_type = load_dimension_type(&name);
        let expected = dimension_type.has_ender_dragon_fight;
        let mut level = ServerLevel::detached(Identifier::parse("minecraft:overworld").unwrap());
        level.gen_env = Some(gen_env(0));
        level.dimension_type = Some(std::sync::Arc::new(dimension_type));
        level.install_constructor_saved_data();
        assert_eq!(
            level.dragon_fight().is_some(),
            expected,
            "{name}: has_ender_dragon_fight is {expected}"
        );
        if expected {
            with_fight.push(name);
        }
    }
    assert_eq!(with_fight, ["the_end"]);
    // the server builds every level's saved data through that one call (`MinecraftServer.createLevels`)
    let glue = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../mc-game-core/src/server/state_glue.rs"),
    )
    .expect("read state_glue.rs");
    assert!(glue.contains("level.install_constructor_saved_data()"));
}

/// (vanilla file under `net/minecraft/`, Rust file under `crates/`, text the Rust file holds for the type).
const SAVED_DATA_TYPES: &[(&str, &str, &str)] = &[
    (
        "server/bossevents/CustomBossEvents.java",
        "mc-game-core/src/server/bossevents.rs",
        "SavedDataType<CustomBossEvents>",
    ),
    (
        "world/RandomSequences.java",
        "mc-game-core/src/world/level/saveddata/random_sequences.rs",
        "SavedDataType<RandomSequences>",
    ),
    (
        "world/Stopwatches.java",
        "mc-game-core/src/world/level/saveddata/stopwatches.rs",
        "SavedDataType<Stopwatches>",
    ),
    (
        "world/clock/ServerClockManager.java",
        "mc-game-core/src/world/clock/server_clock_manager.rs",
        "SavedDataType<ServerClockManager>",
    ),
    (
        "world/entity/raid/Raids.java",
        "mc-game-core/src/world/entity/raid/raids.rs",
        "SavedDataType<Raids>",
    ),
    (
        "world/level/TicketStorage.java",
        "mc-game-core/src/world/level/ticket_storage.rs",
        "SavedDataType<TicketStorage>",
    ),
    (
        "world/level/border/WorldBorder.java",
        "mc-game-core/src/world/level/saveddata/world_border.rs",
        "SavedDataType<WorldBorder>",
    ),
    (
        "world/level/dimension/end/EnderDragonFight.java",
        "mc-game-core/src/world/level/dimension/end/ender_dragon_fight.rs",
        "SavedDataType<EnderDragonFight>",
    ),
    (
        "world/level/gamerules/GameRuleMap.java",
        "mc-game-core/src/world/level/gamerules/game_rule_map.rs",
        "SavedDataType<GameRuleMap>",
    ),
    (
        // read as a compound from `world_gen_settings.dat` by the level storage, without the saved data manager
        "world/level/levelgen/WorldGenSettings.java",
        "mc-world/src/level_storage/world_gen_settings.rs",
        "fn world_gen_settings_id",
    ),
    (
        "world/level/saveddata/WanderingTraderData.java",
        "mc-game-core/src/world/level/saveddata/wandering_trader_data.rs",
        "SavedDataType<WanderingTraderData>",
    ),
    (
        "world/level/saveddata/WeatherData.java",
        "mc-game-core/src/world/level/saveddata/weather_data.rs",
        "SavedDataType<WeatherData>",
    ),
    (
        "world/level/saveddata/maps/MapIndex.java",
        "mc-game-core/src/world/level/saveddata/maps/map_markers.rs",
        "SavedDataType<MapIndex>",
    ),
    (
        "world/level/saveddata/maps/MapItemSavedData.java",
        "mc-game-core/src/world/level/saveddata/maps/map_item_saved_data.rs",
        "SavedDataType<MapItemSavedData>",
    ),
    (
        "world/level/storage/CommandStorage.java",
        "mc-game-core/src/world/level/saveddata/command_storage.rs",
        "SavedDataType<Container>",
    ),
    (
        "world/level/timers/TimerQueue.java",
        "mc-game-core/src/util/timer_queue.rs",
        "SavedDataType<TimerQueue<C>>",
    ),
    (
        "world/scores/ScoreboardSaveData.java",
        "mc-game-core/src/world/scores/scoreboard_save_data.rs",
        "SavedDataType<ScoreboardSaveData>",
    ),
];

fn java_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read the vanilla source directory") {
        let path = entry.expect("directory entry").path();
        if path.is_dir() {
            java_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "java") {
            out.push(path);
        }
    }
}

#[test]
fn every_vanilla_saved_data_type_has_a_rust_type() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let vanilla = crates.join("../ref/src/net/minecraft");
    let mut files = Vec::new();
    java_files(&vanilla, &mut files);
    // (file under `net/minecraft/`, number of `new SavedDataType<` in it)
    let mut declared: Vec<(String, usize)> = files
        .iter()
        .filter_map(|f| {
            let count = std::fs::read_to_string(f)
                .expect("read a vanilla source")
                .matches("new SavedDataType<")
                .count();
            let name = f.strip_prefix(&vanilla).unwrap().to_string_lossy();
            (count > 0).then(|| (name.into_owned(), count))
        })
        .collect();
    declared.sort();
    let mut listed: Vec<(String, usize)> = SAVED_DATA_TYPES
        .iter()
        .map(|t| (t.0.to_string(), 1))
        .collect();
    listed.sort();
    assert!(declared.len() >= 17, "{} declarations", declared.len());
    assert_eq!(
        declared, listed,
        "SAVED_DATA_TYPES must list exactly the vanilla files that declare one SavedDataType each"
    );
    for (java, rust, needle) in SAVED_DATA_TYPES {
        let source =
            std::fs::read_to_string(crates.join(rust)).unwrap_or_else(|e| panic!("{rust}: {e}"));
        assert!(source.contains(needle), "{java}: {rust} lacks `{needle}`");
    }
}
