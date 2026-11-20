//! `DataVersion`, the running game's world version constants (`SharedConstants.getCurrentVersion()`) and the
//! `NbtUtils` data-version helpers.

use mc_nbt::CompoundTag;

pub use mc_datafix::{
    latest_file_fixer_version, requires_file_fixing, world_version_to_file_fixer_version,
};

/// `SharedConstants.WORLD_VERSION` (`getCurrentVersion().dataVersion().version()`).
pub const WORLD_VERSION: i32 = mc_datafix::CURRENT_VERSION;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_fixing_is_required_below_the_latest_file_fixer_version() {
        for (version, required) in [
            (-1, true),
            (0, true),
            (4771, true),
            (4772, true),
            (4898, true),
            (4899, false),
            (5022, false),
            (WORLD_VERSION, false),
            (WORLD_VERSION + 1, false),
        ] {
            assert_eq!(requires_file_fixing(version), required, "{version}");
        }
        assert_eq!(world_version_to_file_fixer_version(4771), 0);
        assert_eq!(world_version_to_file_fixer_version(4772), 4772);
    }
}
