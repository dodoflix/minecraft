//! `net.minecraft.util.filefix`: the file fixer, which upgrades the file layout of a world (moves, renames and
//! rewrites of whole files) before the data of the files is fixed lazily as it is read.
//!
//! The fixes ([`FileFix`]) are lists of operations ([`FileFixOperation`]) run against a copy-on-write view of the
//! world ([`CowTree`]), so a failure leaves the world as it was; [`FileFixerUpper`] runs them and swaps the result in.

mod access;
mod cow_tree;
mod error;
mod fix;
mod fixer;
pub mod fixes;
mod jdk_files;
mod names;
mod ops;
mod progress;
mod relation;

pub use access::{ChunkNbt, LevelDat, MissingSeverity, PlayerData, SavedDataNbt};
pub use cow_tree::{CowFsError, CowTree, Moves};
pub use error::{
    CrashReportCategory, FileFixCrashReport, FileFixError, FileMove, FileSystemCapabilities,
};
pub use fix::FileFix;
pub use fixer::{Checkpoint, FileFixerUpper, FixHooks, detect_file_system_capabilities};
pub use fixes::{file_fixer, new_file_fixer};
pub use ops::{
    ContentContext, DeleteFileOrEmptyDirectory, FileFixOperation, ModifyContent, Move, RegexMove,
    delete_file_or_empty_directory, move_file, only_file,
};
pub use progress::{
    FileFixStats, NoNotifications, Status, Type, UpgradeNotifications, UpgradeProgress,
};
pub use relation::FileRelation;

#[cfg(test)]
mod tests;
