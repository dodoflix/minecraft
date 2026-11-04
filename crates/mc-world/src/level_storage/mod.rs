//! `net.minecraft.world.level.storage`: `level.dat` ([`PrimaryLevelData`]), world folder layout
//! ([`LevelStorageSource`] / [`LevelStorageAccess`]), `session.lock` ([`DirectoryLock`]), player data
//! ([`PlayerDataStorage`]) and raw saved data ([`SavedDataStorage`]); plus the `world.level` types that shape
//! the serialized files ([`LevelSettings`], [`WorldDataConfiguration`], [`WorldGenSettings`], ...).
//!
//! Not ported (belong to the runtime / need registries, datafixers or zip IO): `DerivedLevelData` and the
//! `ServerLevelData`/`WritableLevelData` view traits (weather, border, clocks live in `data/*.dat` in 26.3 and are
//! owned by the game crate), the datafixer upgrade steps, `makeWorldBackup`, typed `GameRules`.

mod directory_lock;
mod dynamic;
mod feature_flags;
mod file_util;
mod level_data;
mod level_resource;
mod level_storage_source;
mod level_summary;
mod player_data_storage;
mod saved_data_storage;
mod validation;
mod version;
mod world_gen_settings;

pub use directory_lock::{DirectoryLock, LockException};
pub use feature_flags::FeatureFlagSet;
pub use file_util::{
    create_directories_safe, create_temp_file, format_file_name_date, format_file_name_date_at, format_file_name_date_now, read_compressed,
    safe_move_file, safe_replace_file, safe_replace_or_move_file, write_compressed,
};
pub use level_data::{
    DataPackConfig, Difficulty, DifficultySettings, GameType, LevelSettings, LevelVersion, PrimaryLevelData, RespawnData, SpecialWorldProperty,
    WorldDataConfiguration, epoch_millis, write_version_tag,
};
pub use level_resource::LevelResource;
pub use level_storage_source::{
    ALLOWED_SYMLINKS_CONFIG_NAME, AccessError, DIMENSION_DATA_DIR, DIMENSIONS_DIR, ENTITIES_DIR, LevelCandidates, LevelDataAndDimensions,
    LevelDirectory, LevelStorageAccess, LevelStorageException, LevelStorageSource, POI_DIR, REGION_DIR, TAG_DATA, get_dimension_storage_folder,
};
pub use level_summary::{BackupStatus, LevelSummary, LevelSummaryKind};
pub use player_data_storage::PlayerDataStorage;
pub use saved_data_storage::{SavedDataStorage, resolve_against};
pub use validation::{ConfigEntry, ContentValidationException, DirectoryValidator, EntryType, ForbiddenSymlinkInfo, PathAllowList, PathMatcher};
pub use version::{
    ANVIL_VERSION_ID, DATA_VERSION_TAG, DataVersion, LATEST_FILE_FIXER_VERSION, MAIN_SERIES, MCREGION_VERSION_ID, WORLD_VERSION, WORLD_VERSION_NAME,
    WORLD_VERSION_STABLE, add_current_data_version, get_data_version, get_data_version_or, requires_file_fixing, world_version_to_file_fixer_version,
};
pub use world_gen_settings::{WorldGenSettings, WorldOptions, world_gen_settings_id};
