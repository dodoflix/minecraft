//! `net.minecraft.util.worldupdate.RegionStorageUpgrader`: upgrades every chunk of one folder (`region`, `entities`,
//! `poi`) of one dimension to the current data version, optionally into recreated region files.

use mc_core::ident::Identifier;
use mc_core::java_exception::JavaException;
use mc_core::pos::chunk_pos::ChunkPos;
use mc_datafix::{CURRENT_VERSION, DataFixTypes};
use mc_nbt::CompoundTag;
use mc_nbt::nbt_utils::get_data_version;
use mc_world::filefix::{Status, UpgradeProgress};
use mc_world::region::upgrade::{ChunkUpgradeError, upgrade_chunk_tag_to};
use mc_world::region::{
    RecreatingSimpleRegionStorage, RegionFile, RegionStorageInfo, SimpleRegionStorage,
};
use regex::Regex;
use std::collections::BTreeMap;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

const NEW_DIRECTORY_PREFIX: &str = "new_";

/// `REGEX`.
static REGION_FILE_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^r\.(-?[0-9]+)\.(-?[0-9]+)\.mca$").expect("region file name"));

/// What stops the upgrade (vanilla's uncaught exception of the `World Upgrader` thread).
#[derive(Debug)]
pub enum UpgradeError {
    /// A `ReportedException` titled `Updated chunk`.
    UpdatedChunk(ChunkUpgradeError),
    /// An `IOException` (or the `CompletionException` of a failed write) outside the per-chunk handler.
    Io(io::Error),
    /// Any other exception (`NumberFormatException` of a region file name, ...).
    Other(String),
}

impl fmt::Display for UpgradeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UpgradeError::UpdatedChunk(e) => write!(
                f,
                "net.minecraft.ReportedException: Updated chunk (Data version: {}, Target version: {}): {}",
                e.data_version, e.target_version, e.cause
            ),
            UpgradeError::Io(e) => write!(f, "{e}"),
            UpgradeError::Other(message) => f.write_str(message),
        }
    }
}

/// `RegionStorageUpgrader.TagModifier`: changes a fixed chunk tag and reports whether it did.
pub type TagModifier = fn(ChunkPos, &mut CompoundTag) -> bool;

/// `FileToUpgrade`: a region file and the positions of the chunks it holds.
struct FileToUpgrade {
    path: PathBuf,
    chunks_to_upgrade: Vec<ChunkPos>,
}

/// The `SimpleRegionStorage` of an upgrader: plain, or the `RecreatingSimpleRegionStorage` of `--recreateRegionFiles`.
enum Storage {
    Plain(Box<SimpleRegionStorage>),
    Recreating(Box<RecreatingSimpleRegionStorage>),
}

impl Storage {
    fn read(&mut self, pos: &ChunkPos) -> io::Result<Option<CompoundTag>> {
        match self {
            Storage::Plain(s) => s.read(pos),
            Storage::Recreating(s) => s.read(pos),
        }
    }

    fn write(&mut self, pos: &ChunkPos, tag: CompoundTag) {
        match self {
            Storage::Plain(s) => s.write(pos, Some(tag)),
            Storage::Recreating(s) => s.write(pos, Some(tag)),
        }
    }

    fn join_writes(&mut self) -> io::Result<()> {
        match self {
            Storage::Plain(s) => s.synchronize(false),
            Storage::Recreating(s) => s.join_writes(),
        }
    }

    fn close(&mut self) -> io::Result<()> {
        match self {
            // `IOWorker.close()` logs a failed close and never throws
            Storage::Plain(s) => {
                let _ = s.close();
                Ok(())
            }
            Storage::Recreating(s) => s.close(),
        }
    }
}

/// `RegionStorageUpgrader.Builder`.
#[derive(Clone)]
pub struct Builder {
    data_fix_type: Option<DataFixTypes>,
    type_: Option<String>,
    folder_name: Option<String>,
    default_version: i32,
    recreate_region_files: bool,
    upgrade_progress: Arc<UpgradeProgress>,
    data_fix_context_tag: Option<CompoundTag>,
    tag_modifiers: BTreeMap<i32, TagModifier>,
}

impl Default for Builder {
    fn default() -> Builder {
        Builder {
            data_fix_type: None,
            type_: None,
            folder_name: None,
            default_version: -1,
            recreate_region_files: false,
            upgrade_progress: Arc::new(UpgradeProgress::noop()),
            data_fix_context_tag: None,
            tag_modifiers: BTreeMap::new(),
        }
    }
}

impl Builder {
    pub fn set_data_fix_type(mut self, data_fix_type: DataFixTypes) -> Builder {
        self.data_fix_type = Some(data_fix_type);
        self
    }

    pub fn set_type_and_folder_name(self, name: &str) -> Builder {
        self.set_type(name).set_folder_name(name)
    }

    pub fn set_type(mut self, type_: &str) -> Builder {
        self.type_ = Some(type_.to_owned());
        self
    }

    pub fn set_folder_name(mut self, folder_name: &str) -> Builder {
        self.folder_name = Some(folder_name.to_owned());
        self
    }

    pub fn set_default_version(mut self, default_version: i32) -> Builder {
        self.default_version = default_version;
        self
    }

    pub fn set_recreate_region_files(mut self, recreate_region_files: bool) -> Builder {
        self.recreate_region_files = recreate_region_files;
        self
    }

    pub fn track_progress(mut self, upgrade_progress: Arc<UpgradeProgress>) -> Builder {
        self.upgrade_progress = upgrade_progress;
        self
    }

    pub fn set_data_fix_context_tag(mut self, tag: Option<CompoundTag>) -> Builder {
        self.data_fix_context_tag = tag;
        self
    }

    /// `addTagModifier`: one modifier per data version.
    pub fn add_tag_modifier(mut self, version: i32, tag_modifier: TagModifier) -> Builder {
        assert!(
            !self.tag_modifiers.contains_key(&version),
            "Can't add two fixers for the same data version"
        );
        self.tag_modifiers.insert(version, tag_modifier);
        self
    }

    pub fn build(self, previous_copies_file_amounts: i32) -> RegionStorageUpgrader {
        RegionStorageUpgrader {
            data_fix_type: self.data_fix_type.expect("data fix type"),
            type_: self.type_.expect("type"),
            folder_name: self.folder_name.expect("folder name"),
            default_version: self.default_version,
            recreate_region_files: self.recreate_region_files,
            upgrade_progress: self.upgrade_progress,
            start_index: previous_copies_file_amounts,
            data_fix_context_tag: self.data_fix_context_tag,
            tag_modifiers: self.tag_modifiers,
            dimension_key: None,
            storage: None,
            files: None,
            previous_write: false,
            write_failure: None,
        }
    }
}

pub struct RegionStorageUpgrader {
    data_fix_type: DataFixTypes,
    type_: String,
    folder_name: String,
    default_version: i32,
    recreate_region_files: bool,
    upgrade_progress: Arc<UpgradeProgress>,
    start_index: i32,
    data_fix_context_tag: Option<CompoundTag>,
    tag_modifiers: BTreeMap<i32, TagModifier>,
    dimension_key: Option<Identifier>,
    storage: Option<Storage>,
    files: Option<Vec<FileToUpgrade>>,
    /// `previousWriteFuture != null`.
    previous_write: bool,
    /// The failure a `previousWriteFuture.join()` keeps throwing once a write failed.
    write_failure: Option<(io::ErrorKind, String)>,
}

impl RegionStorageUpgrader {
    /// `init(dimensionKey, levelStorage)` with the level id and the folder of the dimension.
    pub fn init(
        &mut self,
        dimension_key: &Identifier,
        level_id: &str,
        dimension_dir: &Path,
    ) -> Result<(), UpgradeError> {
        let info = RegionStorageInfo::new(level_id, dimension_key.clone(), self.type_.clone());
        let region_folder = dimension_dir.join(&self.folder_name);
        self.dimension_key = Some(dimension_key.clone());
        self.storage = Some(self.create_storage(&info, &region_folder));
        self.files = Some(self.get_files_to_process(&info, &region_folder)?);
        Ok(())
    }

    /// `upgrade()`.
    pub fn upgrade(&mut self) -> Result<(), UpgradeError> {
        let (Some(dimension_key), Some(mut storage), Some(files)) = (
            self.dimension_key.clone(),
            self.storage.take(),
            self.files.take(),
        ) else {
            panic!("RegionStorageUpgrader has not been initialized");
        };
        if files.is_empty() {
            return Ok(());
        }
        self.upgrade_progress.set_status(Status::Upgrading);
        if let Err(e) = self.upgrade_files(&mut storage, &files, &dimension_key) {
            // vanilla's `IOWorker` is not shut down by the failure and still stores the write that is queued
            let _ = storage.join_writes();
            return Err(e);
        }
        self.upgrade_progress.set_status(Status::Finished);
        if let Err(e) = storage.close() {
            log::error!("Error upgrading chunk\n{e}");
        }
        Ok(())
    }

    /// The loop of `upgrade()`: one region file per round, until a round finds no file left or the upgrade is canceled.
    fn upgrade_files(
        &mut self,
        storage: &mut Storage,
        files: &[FileToUpgrade],
        dimension_key: &Identifier,
    ) -> Result<(), UpgradeError> {
        let total_size = self
            .upgrade_progress
            .total_file_fix_stats()
            .total_operations() as f32;
        let mut next_index = 0;
        while !self.upgrade_progress.is_canceled() {
            let mut worked = false;
            let mut total_progress = 0.0f32;
            if let Some(file_to_upgrade) = files.get(next_index) {
                next_index += 1;
                let mut converted = true;
                for chunk_pos in &file_to_upgrade.chunks_to_upgrade {
                    // `converted = converted && processOnePosition(..)`: after the first chunk that was not written the
                    // rest of the file is neither processed nor counted
                    converted = converted && self.process_one_position(storage, *chunk_pos)?;
                    worked = true;
                }
                if self.recreate_region_files {
                    if converted {
                        self.on_file_finished(storage, &file_to_upgrade.path)?;
                    } else {
                        log::error!(
                            "Failed to convert region file {}",
                            file_to_upgrade.path.display()
                        );
                    }
                }
            }
            let current_dimension_progress = next_index as f32 / total_size;
            let current_total_progress = (self.start_index + next_index as i32) as f32 / total_size;
            self.upgrade_progress
                .set_dimension_progress(dimension_key.clone(), current_dimension_progress);
            total_progress += current_total_progress;
            self.upgrade_progress.set_total_progress(total_progress);
            if !worked {
                break;
            }
        }
        Ok(())
    }

    fn create_storage(&self, info: &RegionStorageInfo, region_folder: &Path) -> Storage {
        if self.recreate_region_files {
            Storage::Recreating(Box::new(RecreatingSimpleRegionStorage::new(
                info.with_type_suffix("source"),
                region_folder,
                info.with_type_suffix("target"),
                resolve_recreate_directory(region_folder),
                true,
            )))
        } else {
            Storage::Plain(Box::new(SimpleRegionStorage::new(
                info.clone(),
                region_folder,
                true,
            )))
        }
    }

    fn get_files_to_process(
        &self,
        info: &RegionStorageInfo,
        region_folder: &Path,
    ) -> Result<Vec<FileToUpgrade>, UpgradeError> {
        let files_to_upgrade = get_all_chunk_positions(info, region_folder)?;
        self.upgrade_progress
            .add_total_file_fix_operations(files_to_upgrade.len() as i32);
        self.upgrade_progress.add_total_chunks(
            files_to_upgrade
                .iter()
                .map(|f| f.chunks_to_upgrade.len() as i32)
                .sum(),
        );
        Ok(files_to_upgrade)
    }

    /// `fileAmount()`.
    pub fn file_amount(&self) -> i32 {
        self.files.as_ref().map_or(0, |files| files.len() as i32)
    }

    fn process_one_position(
        &mut self,
        storage: &mut Storage,
        pos: ChunkPos,
    ) -> Result<bool, UpgradeError> {
        let converted = match self.try_process_one_position(storage, pos) {
            Ok(converted) => converted,
            Err(UpgradeError::Io(cause)) => {
                log::error!("Error upgrading chunk {pos}\n{cause}");
                false
            }
            Err(e) => return Err(e),
        };
        if converted {
            self.upgrade_progress.increment_converted();
        } else {
            self.upgrade_progress.increment_skipped();
        }
        Ok(converted)
    }

    fn try_process_one_position(
        &mut self,
        storage: &mut Storage,
        pos: ChunkPos,
    ) -> Result<bool, UpgradeError> {
        let read = storage.read(&pos).map_err(|e| {
            if JavaException::is_unchecked(&e) {
                // not an `IOException`: `processOnePosition` does not catch it
                UpgradeError::Other(e.to_string())
            } else {
                UpgradeError::Io(e)
            }
        })?;
        let Some(mut chunk_tag) = read else {
            return Ok(false);
        };
        let version = get_data_version(&chunk_tag);
        let latest_version = CURRENT_VERSION;
        let mut changed = false;
        for (&needed_version, tag_fixer) in &self.tag_modifiers {
            chunk_tag = self.upgrade_tag(chunk_tag, needed_version)?;
            changed |= tag_fixer(pos, &mut chunk_tag);
        }
        let upgraded_tag = self.upgrade_tag(chunk_tag, latest_version)?;
        changed |= version < latest_version;
        if changed || self.recreate_region_files {
            self.join_previous_write(storage)?;
            storage.write(&pos, upgraded_tag);
            self.previous_write = true;
            return Ok(true);
        }
        Ok(false)
    }

    /// `previousWriteFuture.join()`: a failed write throws again on every later join.
    fn join_previous_write(&mut self, storage: &mut Storage) -> Result<(), UpgradeError> {
        if !self.previous_write {
            return Ok(());
        }
        if self.write_failure.is_none()
            && let Err(e) = storage.join_writes()
        {
            self.write_failure = Some((e.kind(), e.to_string()));
        }
        match &self.write_failure {
            Some((kind, message)) => Err(UpgradeError::Io(io::Error::new(*kind, message.clone()))),
            None => Ok(()),
        }
    }

    /// `upgradeTag(storage, chunkTag, targetVersion)`: `SimpleRegionStorage.upgradeChunkTag`.
    fn upgrade_tag(
        &self,
        chunk_tag: CompoundTag,
        target_version: i32,
    ) -> Result<CompoundTag, UpgradeError> {
        upgrade_chunk_tag_to(
            self.data_fix_type,
            chunk_tag,
            self.default_version,
            self.data_fix_context_tag.as_ref(),
            target_version,
        )
        .map_err(UpgradeError::UpdatedChunk)
    }

    /// `onFileFinished(regionFile)`: the recreated file replaces the original.
    fn on_file_finished(
        &mut self,
        storage: &mut Storage,
        file_path: &Path,
    ) -> Result<(), UpgradeError> {
        self.join_previous_write(storage)?;
        let directory_path = file_path.parent().unwrap_or(Path::new(""));
        let new_file_path = resolve_recreate_directory(directory_path)
            .join(file_path.file_name().unwrap_or_default());
        if new_file_path.exists() {
            if let Err(e) = std::fs::remove_file(file_path)
                .and_then(|()| std::fs::rename(&new_file_path, file_path))
            {
                log::error!("Failed to replace an old region file\n{e}");
            }
        } else {
            log::error!(
                "Failed to replace an old region file. New file {} does not exist.",
                new_file_path.display()
            );
        }
        Ok(())
    }
}

/// `resolveRecreateDirectory(directoryPath)`: the `new_<name>` sibling.
fn resolve_recreate_directory(directory_path: &Path) -> PathBuf {
    let name = directory_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    directory_path.with_file_name(format!("{NEW_DIRECTORY_PREFIX}{name}"))
}

/// `getAllChunkPositions(info, regionFolder)`: the region files (`r.<x>.<z>.mca`) of the folder that hold chunks, in the
/// order the directory lists them.
fn get_all_chunk_positions(
    info: &RegionStorageInfo,
    region_folder: &Path,
) -> Result<Vec<FileToUpgrade>, UpgradeError> {
    // `File.listFiles(filter)` is null for a folder that does not exist or cannot be read
    let Ok(entries) = std::fs::read_dir(region_folder) else {
        return Ok(Vec::new());
    };
    let mut region_file_chunks = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.ends_with(".mca") {
            continue;
        }
        let Some(captures) = REGION_FILE_NAME.captures(&name) else {
            continue;
        };
        let parse = |group: usize| {
            captures[group].parse::<i32>().map_err(|_| {
                UpgradeError::Other(format!(
                    "java.lang.NumberFormatException: For input string: \"{}\"",
                    &captures[group]
                ))
            })
        };
        let x_offset = parse(1)?.wrapping_shl(5);
        let z_offset = parse(2)?.wrapping_shl(5);
        let region_path = entry.path();
        match RegionFile::new(info.clone(), &region_path, region_folder, true) {
            Ok(region_source) => {
                let mut chunk_positions = Vec::new();
                for x in 0..32i32 {
                    for z in 0..32i32 {
                        let pos = ChunkPos::new(x.wrapping_add(x_offset), z.wrapping_add(z_offset));
                        if region_source.does_chunk_exist(&pos) {
                            chunk_positions.push(pos);
                        }
                    }
                }
                if !chunk_positions.is_empty() {
                    region_file_chunks.push(FileToUpgrade {
                        path: region_path.clone(),
                        chunks_to_upgrade: chunk_positions,
                    });
                }
                // the try-with-resources closes after the file was added
                if let Err(e) = region_source.close() {
                    log::error!(
                        "Failed to read chunks from region file {}\n{e}",
                        region_path.display()
                    );
                }
            }
            Err(e) => log::error!(
                "Failed to read chunks from region file {}\n{e}",
                region_path.display()
            ),
        }
    }
    Ok(region_file_chunks)
}
