//! Saved data files go through the data fixer of their type when they are read: every vanilla `SavedDataType` has a
//! `DataFixTypes`, and an older file of a current world loads and is re-saved at the current version.

use std::collections::BTreeSet;
use std::path::Path;

use mc_core::ident::Identifier;
use mc_datafix::{CURRENT_VERSION, DataFixTypes};
use mc_game::server::bossevents::CustomBossEvents;
use mc_game::util::timer_queue::{FunctionHost, TimerQueue};
use mc_game::world::clock::ServerClockManager;
use mc_game::world::entity::raid::raids::raids_type;
use mc_game::world::level::dimension::end::ender_dragon_fight_type;
use mc_game::world::level::gamerules::{GameRuleMap, GameRules};
use mc_game::world::level::saveddata::command_storage::Container;
use mc_game::world::level::saveddata::maps::{MapIndex, MapItemSavedData};
use mc_game::world::level::saveddata::random_sequences::RandomSequences;
use mc_game::world::level::saveddata::stopwatches::Stopwatches;
use mc_game::world::level::saveddata::wandering_trader_data::WanderingTraderData;
use mc_game::world::level::saveddata::weather_data::WeatherData;
use mc_game::world::level::saveddata::world_border::WorldBorder;
use mc_game::world::level::saveddata::{SavedData, SavedDataManager, SavedDataType};
use mc_game::world::level::ticket_storage;
use mc_game::world::scores::ScoreboardSaveData;
use mc_nbt::{CompoundTag, Tag};
use mc_protocol::components::MapId;
use mc_world::level_storage::{FeatureFlagSet, SavedDataStorage, write_compressed};

struct Ctx;
impl FunctionHost for Ctx {
    fn execute_function(&mut self, _: &Identifier) {}
    fn execute_function_tag(&mut self, _: &Identifier) {}
}

/// Every `DataFixTypes.SAVED_DATA_*` the vanilla sources name.
fn vanilla_fix_types() -> BTreeSet<String> {
    fn walk(dir: &Path, out: &mut BTreeSet<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "java")
                && path.file_name().is_some_and(|n| n != "DataFixTypes.java")
            {
                let text = std::fs::read_to_string(&path).unwrap();
                for (at, _) in text.match_indices("DataFixTypes.SAVED_DATA_") {
                    let rest = &text[at + "DataFixTypes.".len()..];
                    let end = rest
                        .find(|c: char| !(c.is_ascii_uppercase() || c == '_'))
                        .unwrap_or(rest.len());
                    out.insert(rest[..end].to_owned());
                }
            }
        }
    }
    let mut names = BTreeSet::new();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ref/src/net/minecraft");
    walk(&root, &mut names);
    names
}

fn pair<T: SavedData>(ty: SavedDataType<T>) -> (String, &'static str) {
    (ty.id.to_string(), ty.data_fix_type.name())
}

#[test]
fn every_saved_data_type_has_the_fix_type_of_vanilla() {
    let mut ours = vec![
        pair(CustomBossEvents::type_()),
        pair(TimerQueue::<Ctx>::type_()),
        pair(ServerClockManager::type_(&[])),
        pair(raids_type().clone()),
        pair(ender_dragon_fight_type().clone()),
        pair(GameRuleMap::type_()),
        pair(GameRules::type_(FeatureFlagSet::VANILLA)),
        pair(Container::type_("minecraft").unwrap()),
        pair(MapItemSavedData::type_(MapId(0))),
        pair(MapIndex::type_()),
        pair(RandomSequences::type_()),
        pair(Stopwatches::type_()),
        pair(WanderingTraderData::type_()),
        pair(WeatherData::type_()),
        pair(WorldBorder::type_()),
        pair(ticket_storage::saved_data_type()),
        pair(ScoreboardSaveData::type_()),
    ];
    ours.sort();
    // The pairs of the `new SavedDataType<>(id, .., DataFixTypes.X)` sites of the vanilla sources; the game rule
    // type is built twice here (the map and the typed rules share the file).
    let expected = [
        ("minecraft:chunk_tickets", "SAVED_DATA_FORCED_CHUNKS"),
        ("minecraft:command_storage", "SAVED_DATA_COMMAND_STORAGE"),
        (
            "minecraft:custom_boss_events",
            "SAVED_DATA_CUSTOM_BOSS_EVENTS",
        ),
        (
            "minecraft:ender_dragon_fight",
            "SAVED_DATA_ENDER_DRAGON_FIGHT",
        ),
        ("minecraft:game_rules", "SAVED_DATA_GAME_RULES"),
        ("minecraft:game_rules", "SAVED_DATA_GAME_RULES"),
        ("minecraft:maps/0", "SAVED_DATA_MAP_DATA"),
        ("minecraft:maps/last_id", "SAVED_DATA_MAP_INDEX"),
        ("minecraft:raids", "SAVED_DATA_RAIDS"),
        ("minecraft:random_sequences", "SAVED_DATA_RANDOM_SEQUENCES"),
        ("minecraft:scheduled_events", "SAVED_DATA_SCHEDULED_EVENTS"),
        ("minecraft:scoreboard", "SAVED_DATA_SCOREBOARD"),
        ("minecraft:stopwatches", "SAVED_DATA_STOPWATCHES"),
        ("minecraft:wandering_trader", "SAVED_DATA_WANDERING_TRADER"),
        ("minecraft:weather", "SAVED_DATA_WEATHER"),
        ("minecraft:world_border", "SAVED_DATA_WORLD_BORDER"),
        ("minecraft:world_clocks", "SAVED_DATA_WORLD_CLOCKS"),
    ];
    assert_eq!(
        ours.iter()
            .map(|(id, name)| (id.as_str(), *name))
            .collect::<Vec<_>>(),
        expected
    );
    let mut names: BTreeSet<String> = ours.iter().map(|(_, name)| (*name).to_owned()).collect();
    // `world_gen_settings.dat` is read once at startup without a fixer (`readExistingSavedData`) and not through
    // a saved data manager here, so it has no type of its own.
    names.insert(
        DataFixTypes::SAVED_DATA_WORLD_GEN_SETTINGS
            .name()
            .to_owned(),
    );
    assert_eq!(names, vanilla_fix_types());
    assert_eq!(names.len(), 17);
}

fn write_file(data_dir: &Path, name: &str, data: CompoundTag, version: Option<i32>) {
    let mut file = CompoundTag::new().with("data", Tag::Compound(data));
    if let Some(version) = version {
        file.put_int("DataVersion", version);
    }
    let path = data_dir.join(format!("minecraft/{name}.dat"));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    write_compressed(&file, &path).unwrap();
}

fn write_weather(data_dir: &Path, version: Option<i32>) {
    let data = CompoundTag::new()
        .with("raining", Tag::Byte(1))
        .with("rain_time", Tag::Int(4321))
        .with("thundering", Tag::Byte(0))
        .with("thunder_time", Tag::Int(11))
        .with("clear_weather_time", Tag::Int(0));
    write_file(data_dir, "weather", data, version);
}

#[test]
fn an_older_file_of_a_current_world_loads_and_is_saved_at_the_current_version() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("data");
    write_weather(&data_dir, Some(4899));
    let mut manager = SavedDataManager::new(SavedDataStorage::new(&data_dir));
    let ty = WeatherData::type_();
    {
        let weather = manager.get(&ty).expect("the 4899 file loads");
        assert_eq!(weather.get_rain_time(), 4321);
        assert!(!weather.is_dirty());
    }
    manager.get(&ty).unwrap().set_dirty();
    manager.save_and_join();
    let file = SavedDataStorage::new(&data_dir)
        .read_file(&ty.id)
        .unwrap()
        .unwrap();
    assert_eq!(file.get_int("DataVersion"), Some(CURRENT_VERSION));
    assert_eq!(
        file.get_compound("data").unwrap().get_int("rain_time"),
        Some(4321)
    );
}
