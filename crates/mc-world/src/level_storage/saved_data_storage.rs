//! `SavedDataStorage` on-disk format, kept raw: `<folder>/<namespace>/<path>.dat` is a gzip (or, on read,
//! plain) NBT file `{ data: <tag>, DataVersion: int }`. No datafixing, no typed `SavedData` cache — callers
//! own the typed codecs (`SavedDataType`) and hand over / receive the raw `data` tag.

use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use mc_core::ident::Identifier;
use mc_nbt::{CompoundTag, NbtAccounter, Tag};

use super::file_util::{
    create_directories_safe, nbt_error_to_io, read_compressed, write_compressed,
};
use super::version::add_current_data_version;

/// `Path.normalize()` (lexical).
fn normalize(path: &Path) -> PathBuf {
    let mut out: Vec<Component<'_>> = Vec::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => match out.last() {
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                Some(Component::RootDir) | Some(Component::Prefix(_)) => {}
                _ => out.push(c),
            },
            other => out.push(other),
        }
    }
    out.iter().collect()
}

/// `Identifier.resolveAgainst(root)`: `root/<namespace>/<path>`, refusing results that escape `root`
/// (`IllegalStateException` in vanilla, an `InvalidInput` error here).
pub fn resolve_against(id: &Identifier, root: &Path) -> io::Result<PathBuf> {
    let resulting = root.join(id.namespace()).join(id.path());
    if !normalize(&resulting).starts_with(normalize(root)) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "Identifier \"{id}\" tried to access path \"{}\" from root \"{}\"",
                normalize(&resulting).display(),
                normalize(root).display()
            ),
        ));
    }
    Ok(resulting)
}

/// A `data/` folder of saved data files.
#[derive(Clone, Debug)]
pub struct SavedDataStorage {
    data_folder: PathBuf,
}

impl SavedDataStorage {
    pub fn new(data_folder: impl Into<PathBuf>) -> SavedDataStorage {
        SavedDataStorage {
            data_folder: data_folder.into(),
        }
    }

    pub fn data_folder(&self) -> &Path {
        &self.data_folder
    }

    /// `getDataFile`: `id` + `.dat` against the data folder; escaping it is `IllegalArgumentException`.
    pub fn get_data_file(&self, id: &Identifier) -> io::Result<PathBuf> {
        let path = resolve_against(
            &id.with_suffix(".dat").map_err(io::Error::other)?,
            &self.data_folder,
        )?;
        let absolute = std::path::absolute(&path)?;
        if !normalize(&absolute).starts_with(normalize(&std::path::absolute(&self.data_folder)?)) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "SavedDataStorage attempted file access outside of data directory: {{}}{}",
                    path.display()
                ),
            ));
        }
        Ok(path)
    }

    /// `readTagFromDisk` minus the datafixer: gzip (magic `1f 8b`) or plain NBT. The caller reads the file's
    /// `DataVersion` with `get_data_version_or(tag, 1343)`.
    pub fn read_tag_from_disk(data_file: &Path) -> io::Result<CompoundTag> {
        let mut header = [0u8; 2];
        let read = fs::File::open(data_file)?.read(&mut header)?;
        if read == 2 && u16::from_le_bytes(header) == 35615 {
            read_compressed(data_file, &mut NbtAccounter::unlimited_heap())
        } else {
            let data = fs::read(data_file)?;
            mc_nbt::nbt_io::read(&mut data.as_slice(), &mut NbtAccounter::unlimited_heap())
                .map_err(nbt_error_to_io)
        }
    }

    /// The whole file compound of `id`, `None` when the file does not exist.
    pub fn read_file(&self, id: &Identifier) -> io::Result<Option<CompoundTag>> {
        let file = self.get_data_file(id)?;
        if !file.exists() {
            return Ok(None);
        }
        SavedDataStorage::read_tag_from_disk(&file).map(Some)
    }

    /// `readSavedData` up to the codec: the `data` tag of `id`, `None` when absent. Read failures are logged
    /// and reported as absent like vanilla.
    pub fn get(&self, id: &Identifier) -> Option<Tag> {
        match self.read_file(id) {
            Ok(Some(tag)) => tag.get("data").cloned(),
            Ok(None) => None,
            Err(e) => {
                log::error!("Error loading saved data: {id}: {e}");
                None
            }
        }
    }

    /// `encodeUnchecked`: `{ data, DataVersion = current }`.
    pub fn encode_file(data: Tag) -> CompoundTag {
        let mut tag = CompoundTag::new();
        tag.put("data", data);
        add_current_data_version(&mut tag);
        tag
    }

    /// `tryWrite` for an encoded `data` tag (creates parent directories, syncs the file).
    pub fn save(&self, id: &Identifier, data: Tag) -> io::Result<()> {
        let path = self.get_data_file(id)?;
        if let Some(parent) = path.parent() {
            create_directories_safe(parent)?;
        }
        write_compressed(&SavedDataStorage::encode_file(data), &path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_wrapper_shape() {
        let dir = tempfile::tempdir().unwrap();
        let storage = SavedDataStorage::new(dir.path().join("data"));
        let id = Identifier::parse("minecraft:weather").unwrap();
        assert!(storage.get(&id).is_none());
        let data = Tag::Compound(CompoundTag::new().with("raining", Tag::Byte(1)));
        storage.save(&id, data.clone()).unwrap();
        assert!(dir.path().join("data/minecraft/weather.dat").is_file());
        assert_eq!(storage.get(&id), Some(data));
        let file = storage.read_file(&id).unwrap().unwrap();
        assert_eq!(
            file.get_int("DataVersion"),
            Some(super::super::version::WORLD_VERSION)
        );
    }

    #[test]
    fn plain_nbt_is_read_and_escape_refused() {
        let dir = tempfile::tempdir().unwrap();
        let storage = SavedDataStorage::new(dir.path());
        let id = Identifier::parse("minecraft:plain").unwrap();
        let tag = SavedDataStorage::encode_file(Tag::Int(7));
        std::fs::create_dir_all(dir.path().join("minecraft")).unwrap();
        std::fs::write(
            dir.path().join("minecraft/plain.dat"),
            mc_nbt::nbt_io::to_bytes(&tag),
        )
        .unwrap();
        assert_eq!(storage.get(&id), Some(Tag::Int(7)));
        let evil = Identifier::parse("minecraft:a/../../../b").unwrap();
        assert!(storage.get_data_file(&evil).is_err());
    }
}
