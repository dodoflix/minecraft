//! `DataVersion`, the running game's world version constants (`SharedConstants.getCurrentVersion()`) and the
//! `NbtUtils` data-version helpers.

use mc_nbt::CompoundTag;

/// `SharedConstants.WORLD_VERSION` (`getCurrentVersion().dataVersion().version()`).
pub const WORLD_VERSION: i32 = 5023;
/// `getCurrentVersion().name()`.
pub const WORLD_VERSION_NAME: &str = "26.3";
/// `getCurrentVersion().stable()`.
pub const WORLD_VERSION_STABLE: bool = true;
/// `DataVersion.MAIN_SERIES`.
pub const MAIN_SERIES: &str = "main";
/// `SharedConstants.DATA_VERSION_TAG`.
pub const DATA_VERSION_TAG: &str = "DataVersion";
/// `WorldData.ANVIL_VERSION_ID` (`level.dat` `version`).
pub const ANVIL_VERSION_ID: i32 = 19133;
/// `WorldData.MCREGION_VERSION_ID`.
pub const MCREGION_VERSION_ID: i32 = 19132;
/// `DataFixers.getFileFixer()`'s `latestFileFixerVersion` (highest file-fix schema version of 26.3).
pub const LATEST_FILE_FIXER_VERSION: i32 = 4899;

/// `DataVersion` record.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DataVersion {
    pub version: i32,
    pub series: String,
}

impl DataVersion {
    pub fn new(version: i32, series: impl Into<String>) -> DataVersion {
        DataVersion {
            version,
            series: series.into(),
        }
    }

    /// `SharedConstants.getCurrentVersion().dataVersion()`.
    pub fn current() -> DataVersion {
        DataVersion::new(WORLD_VERSION, MAIN_SERIES)
    }

    pub fn is_side_series(&self) -> bool {
        self.series != MAIN_SERIES
    }

    /// `DataVersion.isCompatible` (`DEBUG_OPEN_INCOMPATIBLE_WORLDS` is off): same series.
    pub fn is_compatible(&self, other: &DataVersion) -> bool {
        self.series == other.series
    }
}

/// `FileFixerUpper.worldVersionToFileFixerVersion`.
pub fn world_version_to_file_fixer_version(level_data_version: i32) -> i32 {
    if level_data_version < 4772 {
        0
    } else {
        level_data_version
    }
}

/// `FileFixerUpper.requiresFileFixing`.
pub fn requires_file_fixing(level_data_version: i32) -> bool {
    world_version_to_file_fixer_version(level_data_version) < LATEST_FILE_FIXER_VERSION
}

/// `NbtUtils.addCurrentDataVersion`.
pub fn add_current_data_version(tag: &mut CompoundTag) {
    tag.put_int(DATA_VERSION_TAG, WORLD_VERSION);
}

/// `NbtUtils.getDataVersion(tag)` (`-1` when absent).
pub fn get_data_version(tag: &CompoundTag) -> i32 {
    get_data_version_or(tag, -1)
}

/// `NbtUtils.getDataVersion(tag, default)`.
pub fn get_data_version_or(tag: &CompoundTag, default: i32) -> i32 {
    tag.get_int_or(DATA_VERSION_TAG, default)
}
