//! `net.minecraft.util.filefix.access`: typed access to the files a content fix reads and writes, through the
//! [`CowTree`]: `level.dat` (`LevelDat`), a player file (`PlayerData`), a saved data file (`SavedDataNbt`) and the
//! chunks of a region folder (`ChunkNbt`). Reads run the stored data through the data fixer up to the version of
//! the fix; writes stamp that version. Tags are returned as compounds.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use mc_core::pos::chunk_pos::ChunkPos;
use mc_datafix::{DataFixTypes, References, TypeReference, data_fixer};
use mc_nbt::{CompoundTag, NbtAccounter};

use super::cow_tree::CowTree;
use crate::level_storage::{
    get_data_version, get_data_version_or, read_compressed, write_compressed,
};
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
            MissingSeverity::Important => log::error!("Missing file: {}", path.display()),
            MissingSeverity::Neutral => log::info!("Missing file: {}", path.display()),
            MissingSeverity::Minor => log::debug!("Missing file: {}", path.display()),
        }
    }
}

/// `CompressedNbt.readFile`: the file's root compound, `None` (and the missing-file log line) when it does not exist.
fn read_file(
    tree: &CowTree,
    path: &Path,
    missing_severity: MissingSeverity,
) -> io::Result<Option<CompoundTag>> {
    match tree
        .file_for_read(path)
        .and_then(|real| read_compressed(real, &mut NbtAccounter::unlimited_heap()))
    {
        Ok(tag) => Ok(Some(tag)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            missing_severity.log(path);
            Ok(None)
        }
        Err(e) => Err(e),
    }
}

/// `CompressedNbt.writeFile`: creates the folders and writes; a failure is logged and the file left as it is.
fn write_file(tree: &mut CowTree, path: &Path, data: &CompoundTag) {
    let result = (|| {
        if let Some(parent) = path.parent() {
            tree.create_dir_all(parent)?;
        }
        let real = tree.file_for_write(path, true)?;
        write_compressed(data, &real)
    })();
    if let Err(e) = result {
        log::error!("Failed to write to {}: {e}", path.display());
    }
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
    pub fn read(&self, tree: &CowTree) -> io::Result<Option<CompoundTag>> {
        Ok(
            read_file(tree, &self.path, MissingSeverity::Important)?.map(|root| {
                let content = root.get_compound_or_empty("Data");
                data_fixer().update_tag(
                    &References::LEVEL,
                    content,
                    get_data_version(content),
                    self.target_version,
                )
            }),
        )
    }

    pub fn write(&self, tree: &mut CowTree, mut data: CompoundTag) {
        data.put_int("DataVersion", self.target_version);
        let mut root = CompoundTag::new();
        root.put("Data", data);
        write_file(tree, &self.path, &root);
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

    pub fn read(&self, tree: &CowTree) -> io::Result<Option<CompoundTag>> {
        Ok(
            read_file(tree, &self.path, MissingSeverity::Neutral)?.map(|root| {
                data_fixer().update_tag(
                    &References::PLAYER,
                    &root,
                    get_data_version(&root),
                    self.target_version,
                )
            }),
        )
    }

    pub fn write(&self, tree: &mut CowTree, mut data: CompoundTag) {
        data.put_int("DataVersion", self.target_version);
        write_file(tree, &self.path, &data);
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

    pub fn read(&self, tree: &CowTree) -> io::Result<Option<CompoundTag>> {
        Ok(
            read_file(tree, &self.path, self.missing_severity)?.map(|root| {
                data_fixer()
                    .update_tag(
                        &self.ty,
                        &root,
                        get_data_version(&root),
                        self.target_version,
                    )
                    .get_compound_or_empty("data")
                    .clone()
            }),
        )
    }

    pub fn write(&self, tree: &mut CowTree, data: CompoundTag) {
        let mut root = CompoundTag::new();
        root.put("data", data);
        root.put_int("DataVersion", self.target_version);
        write_file(tree, &self.path, &root);
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
        fs::create_dir(&stage)?;
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
                    fs::copy(
                        tree.file_for_read(&self.folder.join(name))?,
                        self.stage.join(name),
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
            fs::copy(tree.file_for_read(&virtual_path)?, self.stage.join(&name))?;
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
        self.stage(tree, pos)?;
        let Some(tag) = self.storage.read(pos)? else {
            return Ok(());
        };
        let tag = fixer(self.upgrade_chunk_tag(tag, data_fix_context));
        self.storage.write(pos, Some(&tag))
    }

    /// `SimpleRegionStorage.upgradeChunkTag(tag, -1, context, targetVersion)`.
    fn upgrade_chunk_tag(
        &self,
        mut chunk_tag: CompoundTag,
        data_fix_context: Option<&CompoundTag>,
    ) -> CompoundTag {
        let version = get_data_version_or(&chunk_tag, -1);
        if version >= self.target_version {
            return chunk_tag;
        }
        if let Some(context) = data_fix_context {
            chunk_tag.put("__context", context.clone());
        }
        let mut fixed = self
            .ty
            .update_tag(data_fixer(), &chunk_tag, version, self.target_version);
        fixed.remove("__context");
        fixed.put_int("DataVersion", self.target_version);
        fixed
    }

    /// `close`: closes the region files and moves the staged files into `tree`; a region file or external chunk
    /// file that is gone from the staging folder is removed from the tree.
    pub fn close(mut self, tree: &mut CowTree) -> io::Result<()> {
        self.closed = true;
        self.storage.close()?;
        for name in &self.staged_regions {
            let staged = self.stage.join(name);
            let tmp = tree.create_temporary_file_path();
            fs::rename(&staged, &tmp)?;
            tree.adopt_file(&self.folder.join(name), tmp)?;
        }
        if self.staged_external {
            let mut kept = BTreeSet::new();
            for entry in fs::read_dir(&self.stage)? {
                let name = entry?.file_name().to_string_lossy().into_owned();
                if name.starts_with("c.") && name.ends_with(".mcc") {
                    let tmp = tree.create_temporary_file_path();
                    fs::rename(self.stage.join(&name), &tmp)?;
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
        fs::remove_dir_all(&self.stage)
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
