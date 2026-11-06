//! `LevelStorageSource` (+ `LevelStorageAccess`, `LevelDirectory`, `LevelCandidates`, `LevelStorageException`).
//!
//! Layout of a 26.3 world directory (`LevelResource` + `DimensionType.getStorageFolder`):
//!
//! ```text
//! <world>/level.dat, level.dat_old      gzip NBT { Data: PrimaryLevelData }
//! <world>/session.lock                  DirectoryLock
//! <world>/icon.png, datapacks/, generated/, resourcepacks/resources.zip
//! <world>/data/<ns>/<id>.dat            world-wide saved data (game rules, world_gen_settings, weather, ...)
//! <world>/players/{data,advancements,stats}/<uuid>.{dat,json}
//! <world>/dimensions/<ns>/<path>/       every dimension, the overworld included (minecraft/overworld)
//!     region/ entities/ poi/ data/<ns>/<id>.dat
//! ```

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use mc_core::ident::Identifier;
use mc_nbt::{CompoundTag, NbtAccounter};

use super::directory_lock::DirectoryLock;
use super::feature_flags::FeatureFlagSet;
use super::file_util::{
    create_directories_safe, create_temp_file, format_file_name_date_now, read_compressed,
    safe_replace_file, safe_replace_or_move_file, write_compressed,
};
use super::level_data::{
    LevelSettings, LevelVersion, PrimaryLevelData, SpecialWorldProperty, WorldDataConfiguration,
};
use super::level_resource::LevelResource;
use super::level_summary::{LevelSummary, LevelSummaryKind};
use super::player_data_storage::PlayerDataStorage;
use super::saved_data_storage::resolve_against;
use super::validation::{ContentValidationException, DirectoryValidator, PathAllowList};
use super::version::{
    ANVIL_VERSION_ID, MCREGION_VERSION_ID, get_data_version, requires_file_fixing,
};
use super::world_gen_settings::WorldGenSettings;

/// `LevelStorageSource.TAG_DATA`.
pub const TAG_DATA: &str = "Data";
pub const ALLOWED_SYMLINKS_CONFIG_NAME: &str = "allowed_symlinks.txt";
const DISK_SPACE_WARNING_THRESHOLD: u64 = 67_108_864;

/// Sub-folders of a dimension folder (`ChunkMap`, `ServerLevel`, `ServerChunkCache`).
pub const REGION_DIR: &str = "region";
pub const ENTITIES_DIR: &str = "entities";
pub const POI_DIR: &str = "poi";
pub const DIMENSION_DATA_DIR: &str = "data";
/// Parent of the dimension folders (`DimensionType.getStorageFolder`).
pub const DIMENSIONS_DIR: &str = "dimensions";

/// `LevelStorageException`: the message is the English text of the translatable component (`getString()`).
#[derive(Debug)]
pub struct LevelStorageException {
    pub translation_key: &'static str,
    pub message: String,
    pub cause: Option<io::Error>,
}

impl LevelStorageException {
    fn load_folder_access(cause: Option<io::Error>) -> LevelStorageException {
        let key = "selectWorld.load_folder_access";
        LevelStorageException {
            translation_key: key,
            message: mc_data::lang::translate(key).unwrap_or(key).to_owned(),
            cause,
        }
    }
}

impl fmt::Display for LevelStorageException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for LevelStorageException {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.cause.as_ref().map(|e| e as _)
    }
}

/// Failure of [`LevelStorageSource::validate_and_create_access`].
#[derive(Debug)]
pub enum AccessError {
    Io(io::Error),
    ContentValidation(ContentValidationException),
}

impl fmt::Display for AccessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AccessError::Io(e) => e.fmt(f),
            AccessError::ContentValidation(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for AccessError {}

impl From<io::Error> for AccessError {
    fn from(e: io::Error) -> Self {
        AccessError::Io(e)
    }
}

/// `LevelDataAndDimensions` for the parts that do not need registries: the parsed level data and the parsed
/// `world_gen_settings` saved data. `world_gen_settings` is `Err(message)` when vanilla would log "Unable to
/// read or access the world gen settings file!" and fall back to `WorldOptions.defaultWithRandomSeed()` with
/// the data pack's dimensions (the caller owns that fallback: it needs the registries / a random source).
#[derive(Debug)]
pub struct LevelDataAndDimensions {
    pub world_data: PrimaryLevelData,
    pub world_gen_settings: Result<WorldGenSettings, String>,
}

/// `LevelStorageSource.LevelDirectory`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LevelDirectory {
    pub path: PathBuf,
}

impl LevelDirectory {
    pub fn new(path: impl Into<PathBuf>) -> LevelDirectory {
        LevelDirectory { path: path.into() }
    }

    pub fn directory_name(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    pub fn data_file(&self) -> PathBuf {
        self.resource_path(LevelResource::LEVEL_DATA_FILE)
    }

    pub fn old_data_file(&self) -> PathBuf {
        self.resource_path(LevelResource::OLD_LEVEL_DATA_FILE)
    }

    /// `corruptedDataFile(time)`: `time` formatted with `FileNameDateFormatter` (see [`format_file_name_date_now`]).
    pub fn corrupted_data_file(&self, formatted_time: &str) -> PathBuf {
        self.path.join(format!(
            "{}_corrupted_{formatted_time}",
            LevelResource::LEVEL_DATA_FILE.id()
        ))
    }

    /// `rawDataFile(time)`.
    pub fn raw_data_file(&self, formatted_time: &str) -> PathBuf {
        self.path.join(format!(
            "{}_raw_{formatted_time}",
            LevelResource::LEVEL_DATA_FILE.id()
        ))
    }

    pub fn icon_file(&self) -> PathBuf {
        self.resource_path(LevelResource::ICON_FILE)
    }

    pub fn lock_file(&self) -> PathBuf {
        self.resource_path(LevelResource::LOCK_FILE)
    }

    pub fn resource_path(&self, resource: LevelResource) -> PathBuf {
        self.path.join(resource.id())
    }
}

/// `LevelStorageSource.LevelCandidates`.
#[derive(Clone, Debug, Default)]
pub struct LevelCandidates {
    pub levels: Vec<LevelDirectory>,
}

impl LevelCandidates {
    pub fn is_empty(&self) -> bool {
        self.levels.is_empty()
    }
}

impl<'a> IntoIterator for &'a LevelCandidates {
    type Item = &'a LevelDirectory;
    type IntoIter = std::slice::Iter<'a, LevelDirectory>;

    fn into_iter(self) -> Self::IntoIter {
        self.levels.iter()
    }
}

/// `LevelStorageSource`: a folder that holds world folders (`saves/` or the server's universe folder).
pub struct LevelStorageSource {
    base_dir: PathBuf,
    backup_dir: PathBuf,
    world_dir_validator: DirectoryValidator,
}

impl LevelStorageSource {
    /// `new LevelStorageSource(baseDir, backupDir, validator, fixer)` (no datafixer here).
    pub fn new(
        base_dir: impl Into<PathBuf>,
        backup_dir: impl Into<PathBuf>,
        world_dir_validator: DirectoryValidator,
    ) -> io::Result<LevelStorageSource> {
        let base_dir = base_dir.into();
        create_directories_safe(&base_dir)?;
        Ok(LevelStorageSource {
            base_dir,
            backup_dir: backup_dir.into(),
            world_dir_validator,
        })
    }

    /// `LevelStorageSource.parseValidator`: a missing file or any parse failure disallows all symlinks
    /// (a parse failure is logged).
    pub fn parse_validator(config_path: &Path) -> DirectoryValidator {
        if config_path.exists() {
            match fs::read_to_string(config_path)
                .map_err(|e| e.to_string())
                .and_then(|t| PathAllowList::read_plain(&t))
            {
                Ok(list) => return DirectoryValidator::new(list.matcher()),
                Err(e) => log::error!(
                    "Failed to parse {ALLOWED_SYMLINKS_CONFIG_NAME}, disallowing all symbolic links: {e}"
                ),
            }
        }
        DirectoryValidator::no_symlinks_allowed()
    }

    /// `LevelStorageSource.createDefault`: backups in `<path>/../backups`.
    pub fn create_default(path: impl Into<PathBuf>) -> io::Result<LevelStorageSource> {
        let path = path.into();
        let validator =
            LevelStorageSource::parse_validator(&path.join(ALLOWED_SYMLINKS_CONFIG_NAME));
        let backup_dir = path.join("../backups");
        LevelStorageSource::new(path, backup_dir, validator)
    }

    /// `LevelStorageSource.readDataConfig` over the `Data` compound.
    pub fn read_data_config(level_data: &CompoundTag) -> WorldDataConfiguration {
        WorldDataConfiguration::decode(level_data)
    }

    /// `LevelStorageSource.getLevelDataAndDimensions`: `Err` is the `IllegalStateException` for data that still
    /// needs file fixing.
    pub fn get_level_data_and_dimensions(
        world_access: &LevelStorageAccess,
        level_data_tag: &CompoundTag,
        data_configuration: WorldDataConfiguration,
    ) -> Result<LevelDataAndDimensions, String> {
        if requires_file_fixing(get_data_version(level_data_tag)) {
            return Err("Cannot get level data without file fixing first".to_owned());
        }
        let world_gen_settings =
            WorldGenSettings::read_existing(&world_access.get_level_path(LevelResource::ROOT));
        let special = world_gen_settings
            .as_ref()
            .map(WorldGenSettings::special_world_property)
            .unwrap_or(SpecialWorldProperty::None);
        let settings = LevelSettings::parse(level_data_tag, data_configuration);
        Ok(LevelDataAndDimensions {
            world_data: PrimaryLevelData::parse_with(level_data_tag, settings, special),
            world_gen_settings,
        })
    }

    pub fn get_name(&self) -> &'static str {
        "Anvil"
    }

    /// `findLevelCandidates`.
    pub fn find_level_candidates(&self) -> Result<LevelCandidates, LevelStorageException> {
        if !self.base_dir.exists() {
            fs::create_dir(&self.base_dir)
                .map_err(|e| LevelStorageException::load_folder_access(Some(e)))?;
        } else if !self.base_dir.is_dir() {
            return Err(LevelStorageException::load_folder_access(None));
        }
        let entries = fs::read_dir(&self.base_dir)
            .map_err(|_| LevelStorageException::load_folder_access(None))?;
        let mut levels = Vec::new();
        for entry in entries {
            let path = entry
                .map_err(|_| LevelStorageException::load_folder_access(None))?
                .path();
            if !path.is_dir() {
                continue;
            }
            let directory = LevelDirectory::new(path);
            if directory.data_file().is_file() || directory.old_data_file().is_file() {
                levels.push(directory);
            }
        }
        Ok(LevelCandidates { levels })
    }

    /// `loadLevelSummaries` (sequential): unreadable lock state skips the world, summaries are sorted
    /// (`LevelSummary.compareTo`).
    pub fn load_level_summaries(&self, candidates: &LevelCandidates) -> Vec<LevelSummary> {
        let mut summaries: Vec<LevelSummary> = candidates
            .into_iter()
            .filter_map(|level| match DirectoryLock::is_locked(&level.path) {
                Ok(locked) => Some(self.read_level_summary(level, locked)),
                Err(e) => {
                    log::warn!("Failed to read {} lock: {e}", level.path.display());
                    None
                }
            })
            .collect();
        summaries.sort_by(|a, b| a.compare_to(b));
        summaries
    }

    /// `readLevelDataTagRaw`: `NbtAccounter.uncompressedQuota()`.
    pub fn read_level_data_tag_raw(data_file: &Path) -> io::Result<CompoundTag> {
        read_compressed(data_file, &mut NbtAccounter::uncompressed_quota())
    }

    fn read_level_summary(&self, level: &LevelDirectory, locked: bool) -> LevelSummary {
        let data_file = level.data_file();
        if data_file.exists() {
            let result: Result<Option<LevelSummary>, String> = (|| {
                if fs::symlink_metadata(&data_file)
                    .map(|m| m.file_type().is_symlink())
                    .unwrap_or(false)
                {
                    let issues = self
                        .world_dir_validator
                        .validate_symlink(&data_file)
                        .map_err(|e| e.to_string())?;
                    if !issues.is_empty() {
                        log::warn!(
                            "{}",
                            ContentValidationException::message(&data_file, &issues)
                        );
                        return Ok(Some(LevelSummary {
                            level_id: level.directory_name(),
                            icon: level.icon_file(),
                            requires_manual_conversion: false,
                            requires_file_fixing: false,
                            locked: false,
                            experimental: false,
                            kind: LevelSummaryKind::Symlink,
                        }));
                    }
                }
                let root = LevelStorageSource::read_level_data_tag_raw(&data_file)
                    .map_err(|e| e.to_string())?;
                let tag = root.get_compound_or_empty(TAG_DATA);
                let data_version = get_data_version(tag);
                LevelStorageSource::make_level_summary(tag, level, locked, data_version).map(Some)
            })();
            match result {
                Ok(Some(summary)) => return summary,
                Ok(None) => {}
                Err(e) => log::error!("Exception reading {}: {e}", data_file.display()),
            }
        }
        LevelSummary {
            level_id: level.directory_name(),
            icon: level.icon_file(),
            requires_manual_conversion: false,
            requires_file_fixing: false,
            locked: false,
            experimental: false,
            kind: LevelSummaryKind::Corrupted {
                last_played: LevelStorageSource::level_file_modification_millis(level),
            },
        }
    }

    /// `getFileModificationTime(LevelDirectory)`: level.dat, else level.dat_old, else -1 (epoch millis).
    fn level_file_modification_millis(level: &LevelDirectory) -> i64 {
        LevelStorageSource::get_file_modification_time(&level.data_file())
            .or_else(|| LevelStorageSource::get_file_modification_time(&level.old_data_file()))
            .map(|t| {
                t.duration_since(SystemTime::UNIX_EPOCH)
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or_else(|e| -(e.duration().as_millis() as i64))
            })
            .unwrap_or(-1)
    }

    /// `getFileModificationTime(Path)`.
    pub fn get_file_modification_time(path: &Path) -> Option<SystemTime> {
        fs::metadata(path).and_then(|m| m.modified()).ok()
    }

    /// `makeLevelSummary` (over the datafixed `Data` tag — the datafixer step is not implemented). `Err` is the
    /// `NbtFormatException("Unknown data version: ...")`.
    pub fn make_level_summary(
        data_tag: &CompoundTag,
        level_directory: &LevelDirectory,
        locked: bool,
        data_version: i32,
    ) -> Result<LevelSummary, String> {
        let level_version = LevelVersion::parse(data_tag);
        let level_data_version = level_version.level_data_version;
        if level_data_version != MCREGION_VERSION_ID && level_data_version != ANVIL_VERSION_ID {
            return Err(format!(
                "Unknown data version: {:x}",
                level_data_version as u32
            ));
        }
        let requires_manual_conversion = level_data_version != ANVIL_VERSION_ID;
        let data_configuration = LevelStorageSource::read_data_config(data_tag);
        let settings = LevelSettings::parse(data_tag, data_configuration);
        let enabled_feature_flags = LevelStorageSource::parse_feature_flags_from_summary(data_tag);
        Ok(LevelSummary {
            level_id: level_directory.directory_name(),
            icon: level_directory.icon_file(),
            requires_manual_conversion,
            requires_file_fixing: requires_file_fixing(data_version),
            locked,
            experimental: enabled_feature_flags.is_experimental(),
            kind: LevelSummaryKind::Normal {
                settings,
                level_version,
            },
        })
    }

    /// `parseFeatureFlagsFromSummary`: unknown / malformed ids are ignored.
    fn parse_feature_flags_from_summary(tag: &CompoundTag) -> FeatureFlagSet {
        let ids: Vec<Identifier> = super::dynamic::lenient_strings(tag.get("enabled_features"))
            .iter()
            .filter_map(|s| Identifier::try_parse(s))
            .collect();
        FeatureFlagSet::from_names(&ids, |_| {})
    }

    /// `isNewLevelIdAcceptable`: the folder can be created (and is removed again).
    pub fn is_new_level_id_acceptable(&self, level_id: &str) -> bool {
        let full_path = self.get_level_path(level_id);
        fs::create_dir(&full_path).is_ok()
            && match fs::remove_dir(&full_path) {
                Ok(()) => true,
                Err(e) => e.kind() == io::ErrorKind::NotFound,
            }
    }

    pub fn level_exists(&self, level_id: &str) -> bool {
        self.get_level_path(level_id).is_dir()
    }

    pub fn get_level_path(&self, level_id: &str) -> PathBuf {
        self.base_dir.join(level_id)
    }

    pub fn get_base_dir(&self) -> &Path {
        &self.base_dir
    }

    pub fn get_backup_path(&self) -> &Path {
        &self.backup_dir
    }

    /// `validateAndCreateAccess`: refuses worlds containing forbidden symlinks, then locks the world.
    pub fn validate_and_create_access(
        &self,
        level_id: &str,
    ) -> Result<LevelStorageAccess, AccessError> {
        let level_path = self.get_level_path(level_id);
        let validation_results = self
            .world_dir_validator
            .validate_directory(&level_path, true)?;
        if !validation_results.is_empty() {
            return Err(AccessError::ContentValidation(ContentValidationException {
                directory: level_path,
                entries: validation_results,
            }));
        }
        Ok(LevelStorageAccess::new(level_id, level_path)?)
    }

    /// `createAccess`: locks the world (creating the folder when needed).
    pub fn create_access(&self, level_id: &str) -> io::Result<LevelStorageAccess> {
        LevelStorageAccess::new(level_id, self.get_level_path(level_id))
    }

    pub fn get_world_dir_validator(&self) -> &DirectoryValidator {
        &self.world_dir_validator
    }
}

/// `LevelStorageSource.LevelStorageAccess`: an exclusive (`session.lock`) handle on one world folder.
pub struct LevelStorageAccess {
    lock: DirectoryLock,
    level_directory: LevelDirectory,
    level_id: String,
}

fn lock_no_longer_valid() -> io::Error {
    io::Error::other("Lock is no longer valid")
}

impl LevelStorageAccess {
    fn new(level_id: &str, path: PathBuf) -> io::Result<LevelStorageAccess> {
        let lock = DirectoryLock::create(&path)?;
        Ok(LevelStorageAccess {
            lock,
            level_directory: LevelDirectory::new(path),
            level_id: level_id.to_owned(),
        })
    }

    fn create_lock(&mut self) -> io::Result<()> {
        self.lock = DirectoryLock::create(&self.level_directory.path)?;
        Ok(())
    }

    /// `releaseTemporarilyAndRun`: unlock, run, lock again (even when `f` failed).
    pub fn release_temporarily_and_run(
        &mut self,
        f: impl FnOnce() -> io::Result<()>,
    ) -> io::Result<()> {
        self.close()?;
        let result = f();
        let relock = self.create_lock();
        result.and(relock)
    }

    /// `estimateDiskSpace`: usable bytes on the world's file system (`u64::MAX` when unknown).
    pub fn estimate_disk_space(&self) -> u64 {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            if let Ok(path) =
                std::ffi::CString::new(self.level_directory.path.as_os_str().as_bytes())
            {
                // SAFETY: `statvfs` is plain data and `path` is a valid NUL-terminated string.
                let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
                if unsafe { libc::statvfs(path.as_ptr(), &mut stat) } == 0 {
                    return (stat.f_bavail as u64).saturating_mul(stat.f_frsize as u64);
                }
            }
        }
        u64::MAX
    }

    pub fn check_for_low_disk_space(&self) -> bool {
        self.estimate_disk_space() < DISK_SPACE_WARNING_THRESHOLD
    }

    /// `safeClose`: closes, logging a failure.
    pub fn safe_close(&mut self) {
        if let Err(e) = self.close() {
            log::warn!("Failed to unlock access to level {}: {e}", self.level_id);
        }
    }

    pub fn get_level_directory(&self) -> &LevelDirectory {
        &self.level_directory
    }

    pub fn get_level_id(&self) -> &str {
        &self.level_id
    }

    pub fn get_level_path(&self, resource: LevelResource) -> PathBuf {
        self.level_directory.resource_path(resource)
    }

    /// `getDimensionPath(dimension)`: `<world>/dimensions/<namespace>/<path>`.
    pub fn get_dimension_path(&self, dimension: &Identifier) -> io::Result<PathBuf> {
        get_dimension_storage_folder(dimension, &self.level_directory.path)
    }

    fn check_lock(&self) -> io::Result<()> {
        if self.lock.is_valid() {
            Ok(())
        } else {
            Err(lock_no_longer_valid())
        }
    }

    /// `createPlayerStorage`.
    pub fn create_player_storage(&self) -> io::Result<PlayerDataStorage> {
        self.check_lock()?;
        Ok(PlayerDataStorage::new(
            self.get_level_path(LevelResource::PLAYER_DATA_DIR),
        ))
    }

    /// `collectIssues`: parse the level data as the world list would (datafixer step omitted).
    pub fn collect_issues(&self, use_fallback: bool) -> io::Result<()> {
        self.check_lock()?;
        let unfixed = self.get_unfixed_data_tag(use_fallback)?;
        let data_version = get_data_version(&unfixed);
        self.make_level_summary_checked(&unfixed, data_version)
            .map(|_| ())
    }

    fn make_level_summary_checked(
        &self,
        data_tag: &CompoundTag,
        data_version: i32,
    ) -> io::Result<LevelSummary> {
        LevelStorageSource::make_level_summary(data_tag, &self.level_directory, false, data_version)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    /// `fixAndGetSummary` (no datafixer: the tag is summarized as read).
    pub fn fix_and_get_summary(&self) -> io::Result<LevelSummary> {
        self.check_lock()?;
        let tag = self.get_unfixed_data_tag(false)?;
        self.fix_and_get_summary_from_tag(&tag)
    }

    /// `fixAndGetSummaryFromTag`.
    pub fn fix_and_get_summary_from_tag(&self, data_tag: &CompoundTag) -> io::Result<LevelSummary> {
        self.check_lock()?;
        self.make_level_summary_checked(data_tag, get_data_version(data_tag))
    }

    /// `getUnfixedDataTagWithFallback`: level.dat, else (any read failure) level.dat_old, which is then restored
    /// over level.dat (a corrupted level.dat is kept as `level.dat_corrupted_<time>`).
    pub fn get_unfixed_data_tag_with_fallback(&self) -> io::Result<CompoundTag> {
        match self.get_unfixed_data_tag(false) {
            Ok(tag) => Ok(tag),
            Err(e) => {
                log::warn!(
                    "Failed to load world data from {}: {e}",
                    self.level_directory.data_file().display()
                );
                log::info!(
                    "Attempting to use fallback {}",
                    self.level_directory.old_data_file().display()
                );
                let tag = self.get_unfixed_data_tag(true)?;
                self.restore_level_data_from_old();
                Ok(tag)
            }
        }
    }

    /// `getUnfixedDataTag`: the `Data` compound (empty when absent) of level.dat / level.dat_old.
    pub fn get_unfixed_data_tag(&self, use_fallback: bool) -> io::Result<CompoundTag> {
        self.check_lock()?;
        let root = LevelStorageSource::read_level_data_tag_raw(&self.get_data_file(use_fallback))?;
        Ok(root.get_compound_or_empty(TAG_DATA).clone())
    }

    fn get_data_file(&self, use_fallback: bool) -> PathBuf {
        if use_fallback {
            self.level_directory.old_data_file()
        } else {
            self.level_directory.data_file()
        }
    }

    /// `saveDataTag(levelData)` / `saveDataTag(levelData, singleplayerUUID)`.
    pub fn save_data_tag(
        &self,
        level_data: &PrimaryLevelData,
        singleplayer_uuid: Option<uuid::Uuid>,
    ) -> io::Result<()> {
        self.save_level_data_tag(level_data.to_tag_with_player(singleplayer_uuid))
    }

    /// `saveLevelData(Dynamic)`: wraps `data` into `{ Data: data }` and saves.
    pub fn save_level_data_tag(&self, data: CompoundTag) -> io::Result<()> {
        let mut root = CompoundTag::new();
        root.put(TAG_DATA, mc_nbt::Tag::Compound(data));
        self.save_level_data(&root)
    }

    /// `saveLevelData(CompoundTag root)`: temp file `level<random>.dat` in the world folder, written, then
    /// `Util.safeReplaceFile(level.dat, tmp, level.dat_old)`. Vanilla logs "Failed to save level" and swallows
    /// the error; it is logged here and returned.
    pub fn save_level_data(&self, root: &CompoundTag) -> io::Result<()> {
        let world_dir = &self.level_directory.path;
        let result = (|| {
            let data_file = create_temp_file(world_dir, "level", ".dat")?;
            write_compressed(root, &data_file)?;
            safe_replace_file(
                &self.level_directory.data_file(),
                &data_file,
                &self.level_directory.old_data_file(),
            );
            Ok(())
        })();
        if let Err(e) = &result {
            log::error!("Failed to save level {}: {e}", world_dir.display());
        }
        result
    }

    /// `getIconFile`.
    pub fn get_icon_file(&self) -> Option<PathBuf> {
        self.lock
            .is_valid()
            .then(|| self.level_directory.icon_file())
    }

    /// `deleteLevel`: up to 5 attempts (500 ms apart); the lock file is removed last.
    pub fn delete_level(&mut self) -> io::Result<()> {
        self.check_lock()?;
        let lock_path = self.level_directory.lock_file();
        log::info!("Deleting level {}", self.level_id);
        for attempt in 1..=5 {
            log::info!("Attempt {attempt}...");
            let root = self.level_directory.path.clone();
            match self.delete_tree(&root, &root, &lock_path) {
                Ok(()) => break,
                Err(e) => {
                    if attempt >= 5 {
                        return Err(e);
                    }
                    log::warn!("Failed to delete {}: {e}", root.display());
                    std::thread::sleep(Duration::from_millis(500));
                }
            }
        }
        Ok(())
    }

    /// `Files.walkFileTree` + the visitor of `deleteLevel` (links are not followed).
    fn delete_tree(&mut self, dir: &Path, root: &Path, lock_path: &Path) -> io::Result<()> {
        for entry in fs::read_dir(dir)? {
            let path = entry?.path();
            if fs::symlink_metadata(&path)?.is_dir() {
                self.delete_tree(&path, root, lock_path)?;
            } else if path != lock_path {
                log::debug!("Deleting {}", path.display());
                match fs::remove_file(&path) {
                    Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
                    _ => {}
                }
            }
        }
        if dir == root {
            self.lock.close()?;
            match fs::remove_file(lock_path) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
                _ => {}
            }
        }
        match fs::remove_dir(dir) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }

    /// `renameLevel`: rewrites `Data.LevelName` of level.dat (`String.trim()`), without touching anything else.
    pub fn rename_level(&self, new_name: &str) -> io::Result<()> {
        self.modify_level_data_without_datafix(|tag| {
            tag.put_string("LevelName", java_trim(new_name))
        })
    }

    /// `renameAndDropPlayer`.
    pub fn rename_and_drop_player(&self, new_name: &str) -> io::Result<()> {
        self.modify_level_data_without_datafix(|tag| {
            tag.put_string("LevelName", java_trim(new_name));
            tag.remove("singleplayer_uuid");
        })
    }

    fn modify_level_data_without_datafix(
        &self,
        updater: impl FnOnce(&mut CompoundTag),
    ) -> io::Result<()> {
        self.check_lock()?;
        let mut root =
            LevelStorageSource::read_level_data_tag_raw(&self.level_directory.data_file())?;
        if let Some(data) = root.get_compound_mut(TAG_DATA) {
            updater(data);
        }
        self.save_level_data(&root)
    }

    /// `hasWorldData`: level.dat or level.dat_old exists.
    pub fn has_world_data(&self) -> bool {
        self.level_directory.data_file().exists() || self.level_directory.old_data_file().exists()
    }

    /// `close`: release the lock.
    pub fn close(&mut self) -> io::Result<()> {
        self.lock.close()
    }

    /// `restoreLevelDataFromOld`: level.dat_old → level.dat, keeping the old level.dat as `level.dat_corrupted_<time>`.
    pub fn restore_level_data_from_old(&self) -> bool {
        safe_replace_or_move_file(
            &self.level_directory.data_file(),
            &self.level_directory.old_data_file(),
            &self
                .level_directory
                .corrupted_data_file(&format_file_name_date_now()),
            true,
        )
    }

    /// `getFileModificationTime(fallback)`.
    pub fn get_file_modification_time(&self, fallback: bool) -> Option<SystemTime> {
        LevelStorageSource::get_file_modification_time(&self.get_data_file(fallback))
    }
}

/// `DimensionType.getStorageFolder`.
pub fn get_dimension_storage_folder(
    dimension: &Identifier,
    base_folder: &Path,
) -> io::Result<PathBuf> {
    resolve_against(dimension, &base_folder.join(DIMENSIONS_DIR))
}

/// `String.trim()`: strips leading/trailing chars `<= U+0020`.
fn java_trim(s: &str) -> String {
    s.trim_matches(|c: char| c <= ' ').to_owned()
}
