//! `net.minecraft.util.filefix.access`: typed access to the files a content fix reads and writes, through the
//! [`CowTree`]: `level.dat` (`LevelDat`), a player file (`PlayerData`), a saved data file (`SavedDataNbt`) and the
//! chunks of a region folder (`ChunkNbt`). Reads run the stored data through the data fixer up to the version of
//! the fix; writes stamp that version. Values are read and written as `Dynamic`, like vanilla.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use mc_core::java_exception::JavaException;
use mc_core::pos::chunk_pos::ChunkPos;
use mc_datafix::{DataFixTypes, References, TypeReference, data_fixer};
use mc_dfu::{Dyn, Dynamic, NBT};
use mc_nbt::{CompoundTag, NbtAccounter, Tag};

use super::cow_tree::CowTree;
use super::error::FileFixError;
use super::jdk_files::{copy_file, create_directory, delete_tree, list};
use super::ops::shown;
use crate::level_storage::{read_compressed, write_compressed};
use crate::region::upgrade::upgrade_chunk_tag_to;
use crate::region::{RegionFileStorage, RegionStorageInfo};

/// `CompressedNbt.MissingSeverity`: how loudly a missing file is logged.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MissingSeverity {
    Important,
    Neutral,
    Minor,
}

impl MissingSeverity {
    fn log(self, path: &Path) {
        match self {
            MissingSeverity::Important => log::error!("Missing file: {}", shown(path)),
            MissingSeverity::Neutral => log::info!("Missing file: {}", shown(path)),
            MissingSeverity::Minor => log::debug!("Missing file: {}", shown(path)),
        }
    }
}

/// `CompressedNbt.readFile`: the file's root compound, `None` (and the missing-file log line) when it does not exist.
fn read_file(
    tree: &CowTree,
    path: &Path,
    missing_severity: MissingSeverity,
) -> io::Result<Option<Dynamic>> {
    match tree
        .file_for_read(path)
        .and_then(|real| read_compressed(real, &mut NbtAccounter::unlimited_heap()))
    {
        Ok(tag) => Ok(Some(Dynamic::new(NBT, Dyn::from(&Tag::Compound(tag))))),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            missing_severity.log(path);
            Ok(None)
        }
        Err(e) => Err(e),
    }
}

/// `CompressedNbt.writeFile`: creates the folders and writes; an I/O failure is logged and the file left as it is.
/// A value that is not a compound is the `ClassCastException` of `(CompoundTag) data.cast(NbtOps.INSTANCE)`, which
/// nothing catches.
fn write_file(tree: &mut CowTree, path: &Path, data: &Dynamic) -> Result<(), FileFixError> {
    let data = match data.cast(&NBT).to_tag() {
        Ok(Tag::Compound(data)) => data,
        other => return Err(FileFixError::Unchecked(class_cast(other.ok()))),
    };
    let result = (|| {
        if let Some(parent) = path.parent() {
            tree.create_dir_all(parent)?;
        }
        let real = tree.file_for_write(path, true)?;
        write_compressed(&data, &real)
    })();
    if let Err(e) = result {
        log::error!("Failed to write to {}: {e}", shown(path));
    }
    Ok(())
}

/// The message of the `ClassCastException` of casting `tag` to `CompoundTag`.
fn class_cast(tag: Option<Tag>) -> String {
    let class = match tag {
        Some(Tag::End) | None => "EndTag",
        Some(Tag::Byte(_)) => "ByteTag",
        Some(Tag::Short(_)) => "ShortTag",
        Some(Tag::Int(_)) => "IntTag",
        Some(Tag::Long(_)) => "LongTag",
        Some(Tag::Float(_)) => "FloatTag",
        Some(Tag::Double(_)) => "DoubleTag",
        Some(Tag::ByteArray(_)) => "ByteArrayTag",
        Some(Tag::String(_)) => "StringTag",
        Some(Tag::List(_)) => "ListTag",
        Some(Tag::IntArray(_)) => "IntArrayTag",
        Some(Tag::LongArray(_)) => "LongArrayTag",
        Some(Tag::Compound(_)) => unreachable!("a compound casts"),
    };
    format!(
        "java.lang.ClassCastException: class net.minecraft.nbt.{class} cannot be cast to class net.minecraft.nbt.CompoundTag (net.minecraft.nbt.{class} and net.minecraft.nbt.CompoundTag are in unnamed module of loader 'app')"
    )
}

/// `NbtUtils.getDataVersion(Dynamic)`.
fn data_version(data: &Dynamic) -> i32 {
    data.get("DataVersion").as_int_or(-1)
}

/// `NbtUtils.addDataVersion(Dynamic, version)`.
fn add_data_version(data: &Dynamic, version: i32) -> Dynamic {
    data.set("DataVersion", data.create_int(version))
}

/// `LevelDat`: `level.dat`, the `Data` compound at the data version it was last fixed to.
pub struct LevelDat {
    path: PathBuf,
    target_version: i32,
}

impl LevelDat {
    pub fn new(path: PathBuf, target_version: i32) -> LevelDat {
        LevelDat {
            path,
            target_version,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The `Data` compound fixed to the target version, `None` for a missing file.
    pub fn read(&self, tree: &CowTree) -> io::Result<Option<Dynamic>> {
        Ok(
            read_file(tree, &self.path, MissingSeverity::Important)?.map(|read_data| {
                let content = read_data.get("Data").or_else_empty_map();
                let version = data_version(&content);
                data_fixer().update(&References::LEVEL, content, version, self.target_version)
            }),
        )
    }

    pub fn write(&self, tree: &mut CowTree, data: &Dynamic) -> Result<(), FileFixError> {
        let versioned = add_data_version(data, self.target_version);
        write_file(tree, &self.path, &data.empty_map().set("Data", versioned))
    }
}

/// `PlayerData`: a `players/data/<uuid>.dat` file.
pub struct PlayerData {
    path: PathBuf,
    target_version: i32,
}

impl PlayerData {
    pub fn new(path: PathBuf, target_version: i32) -> PlayerData {
        PlayerData {
            path,
            target_version,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn read(&self, tree: &CowTree) -> io::Result<Option<Dynamic>> {
        Ok(
            read_file(tree, &self.path, MissingSeverity::Neutral)?.map(|read_data| {
                let version = data_version(&read_data);
                data_fixer().update(&References::PLAYER, read_data, version, self.target_version)
            }),
        )
    }

    pub fn write(&self, tree: &mut CowTree, data: &Dynamic) -> Result<(), FileFixError> {
        write_file(
            tree,
            &self.path,
            &add_data_version(data, self.target_version),
        )
    }
}

/// `SavedDataNbt`: a saved data file, `{data, DataVersion}`; reading gives the `data` compound.
pub struct SavedDataNbt {
    ty: TypeReference,
    path: PathBuf,
    target_version: i32,
    missing_severity: MissingSeverity,
}

impl SavedDataNbt {
    pub fn new(
        ty: TypeReference,
        path: PathBuf,
        target_version: i32,
        missing_severity: MissingSeverity,
    ) -> SavedDataNbt {
        SavedDataNbt {
            ty,
            path,
            target_version,
            missing_severity,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn read(&self, tree: &CowTree) -> io::Result<Option<Dynamic>> {
        Ok(
            read_file(tree, &self.path, self.missing_severity)?.map(|read_data| {
                let version = data_version(&read_data);
                data_fixer()
                    .update(&self.ty, read_data, version, self.target_version)
                    .get("data")
                    .or_else_empty_map()
            }),
        )
    }

    pub fn write(&self, tree: &mut CowTree, data: &Dynamic) -> Result<(), FileFixError> {
        let data_tag = data.empty_map().set("data", data.clone());
        write_file(
            tree,
            &self.path,
            &add_data_version(&data_tag, self.target_version),
        )
    }
}

/// `ChunkNbt`: the chunks of one region folder. A region file is edited in a staging folder (the region storage
/// opens real files next to its `c.<x>.<z>.mcc` files) and goes back into the tree on [`ChunkNbt::close`].
pub struct ChunkNbt {
    folder: PathBuf,
    ty: DataFixTypes,
    target_version: i32,
    stage: PathBuf,
    storage: RegionFileStorage,
    staged_regions: BTreeSet<String>,
    staged_external: bool,
    closed: bool,
}

/// Moves a finished file out of the staging folder (`Files.move`).
fn move_staged(from: &Path, to: &Path) -> io::Result<()> {
    fs::rename(from, to).map_err(|e| JavaException::from_file_pair_error(e, from, Some(to)))
}

fn region_file_name(pos: &ChunkPos) -> String {
    format!("r.{}.{}.mca", pos.get_region_x(), pos.get_region_z())
}

impl ChunkNbt {
    /// A `ChunkNbt` over the virtual `folder`, whose staging folder is made in the temporary directory of `tree`.
    pub fn new(
        tree: &mut CowTree,
        info: RegionStorageInfo,
        folder: PathBuf,
        ty: DataFixTypes,
        target_version: i32,
    ) -> io::Result<ChunkNbt> {
        let stage = tree.create_temporary_file_path();
        create_directory(&stage)?;
        Ok(ChunkNbt {
            folder,
            ty,
            target_version,
            storage: RegionFileStorage::new(info, &stage, false),
            stage,
            staged_regions: BTreeSet::new(),
            staged_external: false,
            closed: false,
        })
    }

    /// Copies the region file of `pos` (and, once, the external chunk files) into the staging folder.
    fn stage(&mut self, tree: &CowTree, pos: &ChunkPos) -> io::Result<()> {
        if !self.staged_external {
            self.staged_external = true;
            if let Ok(names) = tree.list(&self.folder) {
                for name in names
                    .iter()
                    .filter(|n| n.starts_with("c.") && n.ends_with(".mcc"))
                {
                    copy_file(
                        tree.file_for_read(&self.folder.join(name))?,
                        &self.stage.join(name),
                    )?;
                }
            }
        }
        let name = region_file_name(pos);
        if self.staged_regions.contains(&name) {
            return Ok(());
        }
        let virtual_path = self.folder.join(&name);
        if tree.is_file(&virtual_path) {
            copy_file(tree.file_for_read(&virtual_path)?, &self.stage.join(&name))?;
            self.staged_regions.insert(name);
        }
        Ok(())
    }

    /// `updateChunk`: the chunk at `pos` is data fixed to the target version (with `data_fix_context` as its
    /// `__context`), passed through `fixer` and written back; a chunk that is not stored is left alone.
    pub fn update_chunk(
        &mut self,
        tree: &CowTree,
        pos: &ChunkPos,
        data_fix_context: Option<&CompoundTag>,
        fixer: impl FnOnce(CompoundTag) -> CompoundTag,
    ) -> io::Result<()> {
        // the region file is opened (copied) by the read, whose failure the `IOWorker` logs
        let stored = self
            .stage(tree, pos)
            .and_then(|()| self.storage.read(pos))
            .inspect_err(|e| log::warn!("Failed to read chunk {pos}\n{e}"))?;
        let Some(tag) = stored else {
            return Ok(());
        };
        let tag = upgrade_chunk_tag_to(self.ty, tag, -1, data_fix_context, self.target_version)
            .map_err(io::Error::other)?;
        let tag = fixer(tag);
        self.storage.write(pos, Some(&tag))
    }

    /// `close`: closes the region files and moves the staged files into `tree`; a region file or external chunk
    /// file that is gone from the staging folder is removed from the tree.
    pub fn close(mut self, tree: &mut CowTree) -> io::Result<()> {
        self.closed = true;
        self.storage.close()?;
        for name in &self.staged_regions {
            let staged = self.stage.join(name);
            let tmp = tree.create_temporary_file_path();
            move_staged(&staged, &tmp)?;
            tree.adopt_file(&self.folder.join(name), tmp)?;
        }
        if self.staged_external {
            let mut kept = BTreeSet::new();
            for entry in list(&self.stage)? {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.starts_with("c.") && name.ends_with(".mcc") {
                    let tmp = tree.create_temporary_file_path();
                    move_staged(&self.stage.join(&name), &tmp)?;
                    tree.adopt_file(&self.folder.join(&name), tmp)?;
                    kept.insert(name);
                }
            }
            if let Ok(names) = tree.list(&self.folder) {
                for name in names
                    .iter()
                    .filter(|n| n.starts_with("c.") && n.ends_with(".mcc") && !kept.contains(*n))
                {
                    tree.delete(&self.folder.join(name))?;
                }
            }
        }
        delete_tree(&self.stage)
    }
}

impl Drop for ChunkNbt {
    fn drop(&mut self) {
        if !self.closed && !self.staged_regions.is_empty() {
            log::error!(
                "Chunks of {} were changed but the changes were not closed into the world",
                self.folder.display()
            );
        }
    }
}
