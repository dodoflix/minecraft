//! `LevelSummary` (+ `CorruptedLevelSummary` / `SymlinkLevelSummary`): what the world list / server start shows
//! about a world. UI-only parts (`getInfo` components, button messages) are not ported; the predicates that
//! drive them are.

use std::cmp::Ordering;
use std::path::PathBuf;

use super::level_data::{GameType, LevelSettings, LevelVersion};
use super::version::{DataVersion, WORLD_VERSION, WORLD_VERSION_STABLE, requires_file_fixing};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BackupStatus {
    None,
    Downgrade,
    UpgradeToSnapshot,
    FileFixingRequired,
}

impl BackupStatus {
    pub fn should_backup(self) -> bool {
        self != BackupStatus::None
    }

    pub fn is_severe(self) -> bool {
        self == BackupStatus::Downgrade
    }

    pub fn translation_key(self) -> &'static str {
        match self {
            BackupStatus::None => "",
            BackupStatus::Downgrade => "downgrade",
            BackupStatus::UpgradeToSnapshot => "snapshot",
            BackupStatus::FileFixingRequired => "file_fixing_required",
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum LevelSummaryKind {
    Normal { settings: LevelSettings, level_version: LevelVersion },
    /// `CorruptedLevelSummary`: unreadable level.dat (`last_played` = file modification time or -1).
    Corrupted { last_played: i64 },
    /// `SymlinkLevelSummary`: level.dat is a forbidden symlink.
    Symlink,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LevelSummary {
    pub level_id: String,
    pub icon: PathBuf,
    pub requires_manual_conversion: bool,
    pub requires_file_fixing: bool,
    pub locked: bool,
    pub experimental: bool,
    pub kind: LevelSummaryKind,
}

impl LevelSummary {
    pub fn settings(&self) -> Option<&LevelSettings> {
        match &self.kind {
            LevelSummaryKind::Normal { settings, .. } => Some(settings),
            _ => None,
        }
    }

    pub fn level_version(&self) -> Option<&LevelVersion> {
        match &self.kind {
            LevelSummaryKind::Normal { level_version, .. } => Some(level_version),
            _ => None,
        }
    }

    pub fn level_id(&self) -> &str {
        &self.level_id
    }

    /// `getLevelName`: the folder name when the level name is empty (and for the special summaries).
    pub fn level_name(&self) -> &str {
        match self.settings() {
            Some(s) if !s.level_name.is_empty() => &s.level_name,
            _ => &self.level_id,
        }
    }

    pub fn last_played(&self) -> i64 {
        match &self.kind {
            LevelSummaryKind::Normal { level_version, .. } => level_version.last_played,
            LevelSummaryKind::Corrupted { last_played } => *last_played,
            LevelSummaryKind::Symlink => -1,
        }
    }

    pub fn game_mode(&self) -> Option<GameType> {
        self.settings().map(|s| s.game_type)
    }

    pub fn is_hardcore(&self) -> Option<bool> {
        self.settings().map(|s| s.difficulty_settings.hardcore)
    }

    pub fn has_commands(&self) -> Option<bool> {
        self.settings().map(|s| s.allow_commands)
    }

    /// `getWorldVersionName` (empty = `selectWorld.versionUnknown`).
    pub fn world_version_name(&self) -> Option<&str> {
        self.level_version().map(|v| v.minecraft_version_name.as_str())
    }

    pub fn backup_status(&self) -> BackupStatus {
        let Some(level_version) = self.level_version() else { return BackupStatus::None };
        let level_version_number = level_version.minecraft_version.version;
        if requires_file_fixing(level_version_number) {
            BackupStatus::FileFixingRequired
        } else if !WORLD_VERSION_STABLE && level_version_number < WORLD_VERSION {
            BackupStatus::UpgradeToSnapshot
        } else if level_version_number > WORLD_VERSION {
            BackupStatus::Downgrade
        } else {
            BackupStatus::None
        }
    }

    pub fn should_backup(&self) -> bool {
        self.backup_status().should_backup()
    }

    pub fn is_downgrade(&self) -> bool {
        self.backup_status() == BackupStatus::Downgrade
    }

    /// `isCompatible`: same data version series as the running game.
    pub fn is_compatible(&self) -> bool {
        match self.level_version() {
            Some(v) => DataVersion::current().is_compatible(&v.minecraft_version),
            None => true,
        }
    }

    /// `isDisabled` (the corrupted / symlink summaries are never disabled).
    pub fn is_disabled(&self) -> bool {
        match self.kind {
            LevelSummaryKind::Normal { .. } => self.locked || self.requires_manual_conversion || !self.is_compatible(),
            _ => false,
        }
    }

    pub fn primary_action_active(&self) -> bool {
        match self.kind {
            LevelSummaryKind::Normal { .. } => !self.is_disabled(),
            _ => true,
        }
    }

    pub fn can_upload(&self) -> bool {
        matches!(self.kind, LevelSummaryKind::Normal { .. }) && !self.requires_manual_conversion && !self.locked
    }

    pub fn can_edit(&self) -> bool {
        matches!(self.kind, LevelSummaryKind::Normal { .. }) && !self.is_disabled() && !self.requires_file_fixing
    }

    pub fn can_recreate(&self) -> bool {
        self.can_edit()
    }

    pub fn can_delete(&self) -> bool {
        true
    }

    /// `compareTo`: most recently played first, then by level id (`String.compareTo`, UTF-16 order).
    pub fn compare_to(&self, rhs: &LevelSummary) -> Ordering {
        rhs.last_played()
            .cmp(&self.last_played())
            .then_with(|| self.level_id.encode_utf16().cmp(rhs.level_id.encode_utf16()))
    }
}
