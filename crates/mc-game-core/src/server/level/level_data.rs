//! `LevelData` / `WritableLevelData` / `ServerLevelData` views over mc-world's `PrimaryLevelData`
//! (`world.level.storage.{ServerLevelData, DerivedLevelData}`). The overworld's data is the primary data itself; the other
//! dimensions use [`DerivedLevelData`] (spawn and game time of the overworld, everything else from the world data).
//! Weather, clocks and the world border are saved data of their own (GameServerState), not part of these views.

use mc_world::level_storage::{Difficulty, GameType, PrimaryLevelData, RespawnData};
use parking_lot::Mutex;
use std::sync::Arc;

/// `LevelData` + `WritableLevelData` + `ServerLevelData`.
pub trait ServerLevelData: Send {
    fn get_respawn_data(&self) -> RespawnData;
    fn get_game_time(&self) -> i64;
    fn get_level_name(&self) -> String;
    fn get_game_type(&self) -> GameType;
    fn is_hardcore(&self) -> bool;
    fn is_allow_commands(&self) -> bool;
    fn is_initialized(&self) -> bool;
    fn get_difficulty(&self) -> Difficulty;
    fn is_difficulty_locked(&self) -> bool;
    fn set_spawn(&mut self, respawn_data: RespawnData);
    fn set_game_time(&mut self, time: i64);
    fn set_game_type(&mut self, game_type: GameType);
    fn set_allow_commands(&mut self, allow_commands: bool);
    fn set_initialized(&mut self, initialized: bool);
}

/// The world data (`WorldData`/`PrimaryLevelData`) shared by the server and the levels.
pub type SharedWorldData = Arc<Mutex<PrimaryLevelData>>;

/// Fresh world data of an unnamed creative-less survival world (for tests and detached levels).
pub fn default_world_data() -> SharedWorldData {
    use mc_world::level_storage::{DifficultySettings, LevelSettings, SpecialWorldProperty};
    Arc::new(Mutex::new(PrimaryLevelData::new(
        LevelSettings {
            level_name: "world".to_string(),
            game_type: GameType::Survival,
            difficulty_settings: DifficultySettings::DEFAULT,
            allow_commands: false,
            data_configuration: mc_world::level_storage::WorldDataConfiguration::DEFAULT,
        },
        SpecialWorldProperty::None,
    )))
}

/// `PrimaryLevelData` as `ServerLevelData` (the overworld).
pub struct PrimaryServerLevelData(pub SharedWorldData);

impl ServerLevelData for PrimaryServerLevelData {
    fn get_respawn_data(&self) -> RespawnData {
        self.0.lock().respawn_data.clone()
    }
    fn get_game_time(&self) -> i64 {
        self.0.lock().game_time
    }
    fn get_level_name(&self) -> String {
        self.0.lock().settings.level_name.clone()
    }
    fn get_game_type(&self) -> GameType {
        self.0.lock().settings.game_type
    }
    fn is_hardcore(&self) -> bool {
        self.0.lock().settings.difficulty_settings.hardcore
    }
    fn is_allow_commands(&self) -> bool {
        self.0.lock().settings.allow_commands
    }
    fn is_initialized(&self) -> bool {
        self.0.lock().initialized
    }
    fn get_difficulty(&self) -> Difficulty {
        self.0.lock().settings.difficulty_settings.difficulty
    }
    fn is_difficulty_locked(&self) -> bool {
        self.0.lock().settings.difficulty_settings.locked
    }
    fn set_spawn(&mut self, respawn_data: RespawnData) {
        self.0.lock().respawn_data = respawn_data;
    }
    fn set_game_time(&mut self, time: i64) {
        self.0.lock().game_time = time;
    }
    fn set_game_type(&mut self, game_type: GameType) {
        self.0.lock().settings.game_type = game_type;
    }
    fn set_allow_commands(&mut self, allow_commands: bool) {
        self.0.lock().settings.allow_commands = allow_commands;
    }
    fn set_initialized(&mut self, initialized: bool) {
        self.0.lock().initialized = initialized;
    }
}

/// `DerivedLevelData`: spawn, game time and initialized state come from the wrapped (overworld) data, the rest from the
/// world data; the setters that vanilla leaves empty are no-ops here as well.
pub struct DerivedLevelData {
    world_data: SharedWorldData,
    wrapped: Box<dyn ServerLevelData>,
}

impl DerivedLevelData {
    pub fn new(world_data: SharedWorldData, wrapped: Box<dyn ServerLevelData>) -> Self {
        Self {
            world_data,
            wrapped,
        }
    }
}

impl ServerLevelData for DerivedLevelData {
    fn get_respawn_data(&self) -> RespawnData {
        self.wrapped.get_respawn_data()
    }
    fn get_game_time(&self) -> i64 {
        self.wrapped.get_game_time()
    }
    fn get_level_name(&self) -> String {
        self.world_data.lock().settings.level_name.clone()
    }
    fn get_game_type(&self) -> GameType {
        self.world_data.lock().settings.game_type
    }
    fn is_hardcore(&self) -> bool {
        self.world_data.lock().settings.difficulty_settings.hardcore
    }
    fn is_allow_commands(&self) -> bool {
        self.world_data.lock().settings.allow_commands
    }
    fn is_initialized(&self) -> bool {
        self.wrapped.is_initialized()
    }
    fn get_difficulty(&self) -> Difficulty {
        self.world_data
            .lock()
            .settings
            .difficulty_settings
            .difficulty
    }
    fn is_difficulty_locked(&self) -> bool {
        self.world_data.lock().settings.difficulty_settings.locked
    }
    fn set_spawn(&mut self, respawn_data: RespawnData) {
        self.wrapped.set_spawn(respawn_data);
    }
    fn set_game_time(&mut self, _time: i64) {}
    fn set_game_type(&mut self, _game_type: GameType) {}
    fn set_allow_commands(&mut self, _allow_commands: bool) {}
    fn set_initialized(&mut self, _initialized: bool) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use mc_world::level_storage::{LevelSettings, SpecialWorldProperty};

    fn primary() -> SharedWorldData {
        let mut data = PrimaryLevelData::new(
            LevelSettings {
                level_name: "w".into(),
                game_type: GameType::Creative,
                difficulty_settings: mc_world::level_storage::DifficultySettings::DEFAULT,
                allow_commands: true,
                data_configuration: mc_world::level_storage::WorldDataConfiguration::DEFAULT,
            },
            SpecialWorldProperty::None,
        );
        data.game_time = 77;
        Arc::new(Mutex::new(data))
    }

    #[test]
    fn derived_data_follows_the_wrapped_data_and_ignores_most_setters() {
        let world = primary();
        let mut derived = DerivedLevelData::new(
            world.clone(),
            Box::new(PrimaryServerLevelData(world.clone())),
        );
        assert_eq!(derived.get_game_time(), 77);
        assert_eq!(derived.get_level_name(), "w");
        assert_eq!(derived.get_game_type(), GameType::Creative);
        derived.set_game_time(5);
        derived.set_game_type(GameType::Survival);
        assert_eq!(
            world.lock().game_time,
            77,
            "DerivedLevelData.setGameTime is a no-op"
        );
        assert_eq!(world.lock().settings.game_type, GameType::Creative);
        let mut spawn = derived.get_respawn_data();
        spawn.yaw = 90.0;
        derived.set_spawn(spawn);
        assert_eq!(
            world.lock().respawn_data.yaw,
            90.0,
            "setSpawn goes to the wrapped data"
        );
    }
}
