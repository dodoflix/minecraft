//! `LevelResource`: relative paths inside a world directory.

use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct LevelResource(pub &'static str);

impl LevelResource {
    pub const PLAYER_ADVANCEMENTS_DIR: LevelResource = LevelResource("players/advancements");
    pub const PLAYER_STATS_DIR: LevelResource = LevelResource("players/stats");
    pub const PLAYER_DATA_DIR: LevelResource = LevelResource("players/data");
    pub const PLAYER_OLD_DATA_DIR: LevelResource = LevelResource("players");
    pub const LEVEL_DATA_FILE: LevelResource = LevelResource("level.dat");
    pub const OLD_LEVEL_DATA_FILE: LevelResource = LevelResource("level.dat_old");
    pub const ICON_FILE: LevelResource = LevelResource("icon.png");
    pub const LOCK_FILE: LevelResource = LevelResource("session.lock");
    pub const GENERATED_DIR: LevelResource = LevelResource("generated");
    pub const DATAPACK_DIR: LevelResource = LevelResource("datapacks");
    pub const MAP_RESOURCE_FILE: LevelResource = LevelResource("resourcepacks/resources.zip");
    pub const DATA: LevelResource = LevelResource("data");
    pub const ROOT: LevelResource = LevelResource(".");

    pub fn id(&self) -> &'static str {
        self.0
    }
}

impl fmt::Display for LevelResource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "/{}", self.0)
    }
}
