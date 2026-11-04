//! `PrimaryLevelData` (the `Data` compound of `level.dat`) with `LevelSettings`, `WorldDataConfiguration`,
//! `DataPackConfig`, `LevelData.RespawnData` and `LevelVersion`.
//!
//! In 26.3 `level.dat` only carries the settings listed in [`PrimaryLevelData::to_tag_at`]; everything else
//! (game rules, weather, clocks, world border, world gen settings/seed, boss events, scheduled events, the
//! scoreboard, ...) lives in `data/<namespace>/<id>.dat` saved data (see [`super::saved_data_storage`],
//! [`super::world_gen_settings`]). Keys vanilla's `parse` does not consume (e.g. pre-26 `Player` /
//! `WorldGenSettings`) are kept in [`PrimaryLevelData::extra`] and written back unchanged.

use indexmap::IndexSet;
use mc_core::ident::Identifier;
use mc_core::pos::block_pos::BlockPos;
use mc_core::uuid_util::{uuid_from_int_array, uuid_to_int_array};
use mc_nbt::{CompoundTag, ListTag, Tag};
use uuid::Uuid;

use super::dynamic::{self, as_boolean, as_int, as_long, as_string, get, java_string_set_order, without_keys};
use super::feature_flags::FeatureFlagSet;
use super::version::{
    ANVIL_VERSION_ID, DataVersion, MAIN_SERIES, WORLD_VERSION, WORLD_VERSION_NAME, WORLD_VERSION_STABLE, add_current_data_version,
};

/// `net.minecraft.world.level.GameType`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum GameType {
    Survival = 0,
    Creative = 1,
    Adventure = 2,
    Spectator = 3,
}

impl GameType {
    pub const DEFAULT_MODE: GameType = GameType::Survival;

    pub fn get_id(self) -> i32 {
        self as i32
    }

    /// `GameType.byId`: out-of-range ids fall back to survival (`OutOfBoundsStrategy.ZERO`).
    pub fn by_id(id: i32) -> GameType {
        match id {
            1 => GameType::Creative,
            2 => GameType::Adventure,
            3 => GameType::Spectator,
            _ => GameType::Survival,
        }
    }

    pub fn get_name(self) -> &'static str {
        match self {
            GameType::Survival => "survival",
            GameType::Creative => "creative",
            GameType::Adventure => "adventure",
            GameType::Spectator => "spectator",
        }
    }
}

/// `net.minecraft.world.Difficulty`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Difficulty {
    Peaceful = 0,
    Easy = 1,
    Normal = 2,
    Hard = 3,
}

impl Difficulty {
    pub fn get_id(self) -> i32 {
        self as i32
    }

    /// `Difficulty.byId`: wraps around (`OutOfBoundsStrategy.WRAP`).
    pub fn by_id(id: i32) -> Difficulty {
        match id.rem_euclid(4) {
            0 => Difficulty::Peaceful,
            1 => Difficulty::Easy,
            2 => Difficulty::Normal,
            _ => Difficulty::Hard,
        }
    }

    pub fn get_serialized_name(self) -> &'static str {
        match self {
            Difficulty::Peaceful => "peaceful",
            Difficulty::Easy => "easy",
            Difficulty::Normal => "normal",
            Difficulty::Hard => "hard",
        }
    }

    /// `Difficulty.byName` (serialized names only).
    pub fn by_name(name: &str) -> Option<Difficulty> {
        [Difficulty::Peaceful, Difficulty::Easy, Difficulty::Normal, Difficulty::Hard].into_iter().find(|d| d.get_serialized_name() == name)
    }
}

/// `LevelSettings.DifficultySettings`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DifficultySettings {
    pub difficulty: Difficulty,
    pub hardcore: bool,
    pub locked: bool,
}

impl DifficultySettings {
    pub const DEFAULT: DifficultySettings = DifficultySettings { difficulty: Difficulty::Normal, hardcore: false, locked: false };

    /// `DifficultySettings.CODEC` decode (all three fields required).
    pub fn decode(tag: &Tag) -> Result<DifficultySettings, String> {
        let c = tag.as_compound().ok_or("Not a map")?;
        let difficulty = c
            .get("difficulty")
            .ok_or("No key difficulty")?
            .as_string()
            .and_then(Difficulty::by_name)
            .ok_or("Unknown difficulty")?;
        let hardcore = c.get("hardcore").and_then(Tag::as_number).ok_or("hardcore")?.double_value() != 0.0;
        let locked = c.get("locked").and_then(Tag::as_number).ok_or("locked")?.double_value() != 0.0;
        Ok(DifficultySettings { difficulty, hardcore, locked })
    }

    pub fn encode(&self) -> Tag {
        let mut c = CompoundTag::new();
        c.put_string("difficulty", self.difficulty.get_serialized_name());
        c.put_boolean("hardcore", self.hardcore);
        c.put_boolean("locked", self.locked);
        Tag::Compound(c)
    }
}

/// `DataPackConfig` (enabled / disabled pack ids).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DataPackConfig {
    pub enabled: Vec<String>,
    pub disabled: Vec<String>,
}

impl DataPackConfig {
    /// `DataPackConfig.DEFAULT`.
    pub fn default_config() -> DataPackConfig {
        DataPackConfig { enabled: vec!["vanilla".to_owned()], disabled: Vec::new() }
    }

    /// `DataPackConfig.CODEC` decode.
    pub fn decode(tag: &Tag) -> Result<DataPackConfig, String> {
        let c = tag.as_compound().ok_or("Not a map")?;
        let list = |key: &str| {
            c.get(key).ok_or_else(|| format!("No key {key}")).and_then(|t| dynamic::string_list(t).ok_or_else(|| format!("{key}: not a list of strings")))
        };
        Ok(DataPackConfig { enabled: list("Enabled")?, disabled: list("Disabled")? })
    }

    pub fn encode(&self) -> Tag {
        let mut c = CompoundTag::new();
        c.put("Enabled", Tag::List(dynamic::string_list_tag(&self.enabled)));
        c.put("Disabled", Tag::List(dynamic::string_list_tag(&self.disabled)));
        Tag::Compound(c)
    }
}

/// `WorldDataConfiguration`.
///
/// `data_packs == None` stands for the `DataPackConfig.DEFAULT` *instance*: vanilla's codec omits `DataPacks`
/// only for that very instance (`DataPackConfig` has no `equals`), so a level.dat whose `DataPacks` merely
/// has the default contents is written back with the field, one without it is written back without.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct WorldDataConfiguration {
    pub data_packs: Option<DataPackConfig>,
    pub enabled_features: FeatureFlagSet,
}

impl WorldDataConfiguration {
    /// `WorldDataConfiguration.DEFAULT`.
    pub const DEFAULT: WorldDataConfiguration = WorldDataConfiguration { data_packs: None, enabled_features: FeatureFlagSet::DEFAULT_FLAGS };

    /// The effective pack config (`dataPacks()`).
    pub fn data_packs(&self) -> DataPackConfig {
        self.data_packs.clone().unwrap_or_else(DataPackConfig::default_config)
    }

    /// `WorldDataConfiguration.CODEC` decode of the level data compound: both fields are *lenient* optionals, a
    /// field that is absent or fails to decode silently falls back to its default.
    pub fn decode(level_data: &CompoundTag) -> WorldDataConfiguration {
        WorldDataConfiguration {
            data_packs: level_data.get("DataPacks").and_then(|t| DataPackConfig::decode(t).ok()),
            enabled_features: level_data
                .get("enabled_features")
                .and_then(|t| FeatureFlagSet::decode(t).ok())
                .unwrap_or(FeatureFlagSet::DEFAULT_FLAGS),
        }
    }

    /// `WorldDataConfiguration.MAP_CODEC` encode into `tag` (`DataPacks` / `enabled_features` omitted when default).
    pub fn encode_into(&self, tag: &mut CompoundTag) {
        if let Some(packs) = &self.data_packs {
            tag.put("DataPacks", packs.encode());
        }
        if self.enabled_features != FeatureFlagSet::DEFAULT_FLAGS {
            tag.put("enabled_features", self.enabled_features.encode());
        }
    }

    /// `expandFeatures`.
    pub fn expand_features(&self, new_enabled_features: FeatureFlagSet) -> WorldDataConfiguration {
        WorldDataConfiguration { data_packs: self.data_packs.clone(), enabled_features: self.enabled_features.join(new_enabled_features) }
    }
}

/// `LevelSettings`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LevelSettings {
    pub level_name: String,
    pub game_type: GameType,
    pub difficulty_settings: DifficultySettings,
    pub allow_commands: bool,
    pub data_configuration: WorldDataConfiguration,
}

impl LevelSettings {
    /// `LevelSettings.parse(Dynamic, WorldDataConfiguration)`.
    pub fn parse(input: &CompoundTag, load_config: WorldDataConfiguration) -> LevelSettings {
        let game_type = GameType::by_id(as_int(input.get("GameType"), 0));
        LevelSettings {
            level_name: as_string(input.get("LevelName"), ""),
            game_type,
            difficulty_settings: input
                .get("difficulty_settings")
                .and_then(|t| DifficultySettings::decode(t).ok())
                .unwrap_or(DifficultySettings::DEFAULT),
            allow_commands: as_boolean(input.get("allowCommands"), game_type == GameType::Creative),
            data_configuration: load_config,
        }
    }
}

/// `LevelData.RespawnData` (world spawn: `GlobalPos` + yaw/pitch).
#[derive(Clone, PartialEq, Debug)]
pub struct RespawnData {
    pub dimension: Identifier,
    pub pos: BlockPos,
    pub yaw: f32,
    pub pitch: f32,
}

impl RespawnData {
    /// `RespawnData.DEFAULT`.
    pub fn default_data() -> RespawnData {
        RespawnData { dimension: Identifier::with_default_namespace("overworld").expect("valid"), pos: BlockPos::ZERO, yaw: 0.0, pitch: 0.0 }
    }

    /// `RespawnData.CODEC` decode.
    pub fn decode(tag: &Tag) -> Result<RespawnData, String> {
        let c = tag.as_compound().ok_or("Not a map")?;
        let dimension = Identifier::read(c.get("dimension").ok_or("No key dimension")?.as_string().ok_or("Not a string")?)?;
        let p = dynamic::fixed_int_stream(c.get("pos").ok_or("No key pos")?, 3)?;
        let ranged = |key: &str, min: f32, max: f32| {
            let v = c.get(key).ok_or_else(|| format!("No key {key}"))?.as_float().ok_or("Not a number")?;
            if min <= v && v <= max { Ok(v) } else { Err(format!("Value {v} outside of range [{min}:{max}]")) }
        };
        let yaw = ranged("yaw", -180.0, 180.0)?;
        let pitch = ranged("pitch", -90.0, 90.0)?;
        Ok(RespawnData { dimension, pos: BlockPos::new(p[0], p[1], p[2]), yaw, pitch })
    }

    pub fn encode(&self) -> Tag {
        let mut c = CompoundTag::new();
        c.put_string("dimension", self.dimension.to_string());
        c.put_int_array("pos", vec![self.pos.x(), self.pos.y(), self.pos.z()]);
        c.put_float("yaw", self.yaw);
        c.put_float("pitch", self.pitch);
        Tag::Compound(c)
    }
}

/// `LevelVersion`: the version info `LevelStorageSource` reads for the world list / compatibility checks.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LevelVersion {
    pub level_data_version: i32,
    pub last_played: i64,
    pub minecraft_version_name: String,
    pub minecraft_version: DataVersion,
    pub snapshot: bool,
}

impl LevelVersion {
    /// `LevelVersion.parse(Dynamic)` over the `Data` compound.
    pub fn parse(input: &CompoundTag) -> LevelVersion {
        let level_data_version = as_int(input.get("version"), 0);
        let last_played = as_long(input.get("LastPlayed"), 0);
        match input.get("Version") {
            Some(version) => {
                let version = Some(version);
                LevelVersion {
                    level_data_version,
                    last_played,
                    minecraft_version_name: as_string(get(version, "Name"), WORLD_VERSION_NAME),
                    minecraft_version: DataVersion::new(as_int(get(version, "Id"), WORLD_VERSION), as_string(get(version, "Series"), MAIN_SERIES)),
                    snapshot: as_boolean(get(version, "Snapshot"), !WORLD_VERSION_STABLE),
                }
            }
            None => LevelVersion {
                level_data_version,
                last_played,
                minecraft_version_name: String::new(),
                minecraft_version: DataVersion::new(0, MAIN_SERIES),
                snapshot: false,
            },
        }
    }
}

/// `PrimaryLevelData.SpecialWorldProperty` (derived from the world gen settings, never serialized here).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SpecialWorldProperty {
    None,
    Flat,
    Debug,
}

/// Keys `PrimaryLevelData` consumes/produces; everything else survives in `extra`.
const KNOWN_KEYS: [&str; 18] = [
    "ServerBrands",
    "WasModded",
    "removed_features",
    "Version",
    "version_history",
    "DataVersion",
    "GameType",
    "spawn",
    "Time",
    "LastPlayed",
    "LevelName",
    "version",
    "allowCommands",
    "initialized",
    "difficulty_settings",
    "singleplayer_uuid",
    "DataPacks",
    "enabled_features",
];

#[derive(Clone, PartialEq, Debug)]
pub struct PrimaryLevelData {
    pub settings: LevelSettings,
    pub special_world_property: SpecialWorldProperty,
    pub respawn_data: RespawnData,
    pub game_time: i64,
    pub single_player_uuid: Option<Uuid>,
    /// The storage version read from the file (`version`); always written as 19133.
    pub version: i32,
    pub initialized: bool,
    pub known_server_brands: IndexSet<String>,
    pub was_modded: bool,
    /// A `HashSet` in vanilla; held in file order here and re-ordered like `HashSet` iteration on write.
    pub removed_feature_flags: IndexSet<String>,
    pub version_history: Vec<i32>,
    /// Top-level keys vanilla's parse ignores, preserved verbatim.
    pub extra: CompoundTag,
}

impl PrimaryLevelData {
    /// `new PrimaryLevelData(LevelSettings, SpecialWorldProperty, Lifecycle)` (a brand-new world).
    pub fn new(settings: LevelSettings, special_world_property: SpecialWorldProperty) -> PrimaryLevelData {
        PrimaryLevelData {
            settings,
            special_world_property,
            respawn_data: RespawnData::default_data(),
            game_time: 0,
            single_player_uuid: None,
            version: ANVIL_VERSION_ID,
            initialized: false,
            known_server_brands: IndexSet::new(),
            was_modded: false,
            removed_feature_flags: IndexSet::new(),
            version_history: Vec::new(),
            extra: CompoundTag::new(),
        }
    }

    /// `LevelStorageSource.readDataConfig` + `LevelSettings.parse` + `PrimaryLevelData.parse` over the `Data` compound.
    pub fn parse(data: &CompoundTag, special_world_property: SpecialWorldProperty) -> PrimaryLevelData {
        let settings = LevelSettings::parse(data, WorldDataConfiguration::decode(data));
        PrimaryLevelData::parse_with(data, settings, special_world_property)
    }

    /// `PrimaryLevelData.parse(Dynamic, LevelSettings, SpecialWorldProperty, Lifecycle)`.
    pub fn parse_with(input: &CompoundTag, settings: LevelSettings, special_world_property: SpecialWorldProperty) -> PrimaryLevelData {
        let level_version = LevelVersion::parse(input);
        PrimaryLevelData {
            settings,
            special_world_property,
            respawn_data: input.get("spawn").and_then(|t| RespawnData::decode(t).ok()).unwrap_or_else(RespawnData::default_data),
            game_time: as_long(input.get("Time"), 0),
            single_player_uuid: input
                .get("singleplayer_uuid")
                .and_then(|t| dynamic::fixed_int_stream(t, 4).ok())
                .map(|v| uuid_from_int_array([v[0], v[1], v[2], v[3]])),
            version: level_version.level_data_version,
            initialized: as_boolean(input.get("initialized"), true),
            known_server_brands: dynamic::lenient_strings(input.get("ServerBrands")).into_iter().collect(),
            was_modded: as_boolean(input.get("WasModded"), false),
            removed_feature_flags: dynamic::lenient_strings(input.get("removed_features")).into_iter().collect(),
            version_history: input.get("version_history").and_then(dynamic::stream).map_or_else(Vec::new, |v| {
                v.iter().map(|t| as_int(Some(t), -1)).collect()
            }),
            extra: without_keys(input, &KNOWN_KEYS),
        }
    }

    /// `PrimaryLevelData.createTag(null)`: stamps `LastPlayed` with the current time.
    pub fn to_tag(&self) -> CompoundTag {
        self.to_tag_with_player(None)
    }

    /// `PrimaryLevelData.createTag(singlePlayerUUID)`.
    pub fn to_tag_with_player(&self, single_player_uuid: Option<Uuid>) -> CompoundTag {
        self.to_tag_at(single_player_uuid, epoch_millis())
    }

    /// `createTag` with an explicit `LastPlayed` (`Util.getEpochMillis()` in vanilla).
    pub fn to_tag_at(&self, single_player_uuid: Option<Uuid>, last_played: i64) -> CompoundTag {
        let single_player_uuid = single_player_uuid.or(self.single_player_uuid);
        let mut tag = self.extra.clone();
        tag.put("ServerBrands", Tag::List(dynamic::string_list_tag(&self.known_server_brands)));
        tag.put_boolean("WasModded", self.was_modded);
        if !self.removed_feature_flags.is_empty() {
            let ordered = java_string_set_order(&self.removed_feature_flags.iter().cloned().collect::<Vec<_>>());
            tag.put("removed_features", Tag::List(dynamic::string_list_tag(&ordered)));
        }
        write_version_tag(&mut tag);
        self.write_version_history(&mut tag);
        add_current_data_version(&mut tag);
        tag.put_int("GameType", self.settings.game_type.get_id());
        tag.put("spawn", self.respawn_data.encode());
        tag.put_long("Time", self.game_time);
        tag.put_long("LastPlayed", last_played);
        tag.put_string("LevelName", self.settings.level_name.clone());
        tag.put_int("version", ANVIL_VERSION_ID);
        tag.put_boolean("allowCommands", self.settings.allow_commands);
        tag.put_boolean("initialized", self.initialized);
        tag.put("difficulty_settings", self.settings.difficulty_settings.encode());
        if let Some(uuid) = single_player_uuid {
            tag.put_int_array("singleplayer_uuid", uuid_to_int_array(&uuid).to_vec());
        }
        self.settings.data_configuration.encode_into(&mut tag);
        tag
    }

    /// `writeVersionHistory`: the history plus the current version unless it already ends with it.
    fn write_version_history(&self, tag: &mut CompoundTag) {
        let mut list = ListTag::new();
        for v in &self.version_history {
            list.push(Tag::Int(*v));
        }
        if self.version_history.last() != Some(&WORLD_VERSION) {
            list.push(Tag::Int(WORLD_VERSION));
        }
        tag.put("version_history", Tag::List(list));
    }

    /// `WorldData.setModdedInfo`.
    pub fn set_modded_info(&mut self, server_brand: &str, is_modded: bool) {
        self.known_server_brands.insert(server_brand.to_owned());
        self.was_modded |= is_modded;
    }

    pub fn is_hardcore(&self) -> bool {
        self.settings.difficulty_settings.hardcore
    }

    pub fn is_flat_world(&self) -> bool {
        self.special_world_property == SpecialWorldProperty::Flat
    }

    pub fn is_debug_world(&self) -> bool {
        self.special_world_property == SpecialWorldProperty::Debug
    }

    /// `WorldData.getStorageVersionName`.
    pub fn storage_version_name(version: i32) -> &'static str {
        match version {
            19132 => "McRegion",
            19133 => "Anvil",
            _ => "Unknown?",
        }
    }
}

/// `Util.getEpochMillis()`.
pub fn epoch_millis() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// `PrimaryLevelData.writeVersionTag`: the `Version` compound of the running game.
pub fn write_version_tag(tag: &mut CompoundTag) {
    let mut version = CompoundTag::new();
    version.put_string("Name", WORLD_VERSION_NAME);
    version.put_int("Id", WORLD_VERSION);
    version.put_boolean("Snapshot", !WORLD_VERSION_STABLE);
    version.put_string("Series", MAIN_SERIES);
    tag.put("Version", Tag::Compound(version));
}
