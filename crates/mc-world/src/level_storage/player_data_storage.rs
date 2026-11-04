//! `PlayerDataStorage`: `players/data/<uuid>.dat` (raw player compound, gzip NBT) with the `.dat_old`
//! backup rotation and the `<uuid>-<random>.dat` temp-file swap. The `DataFixTypes.PLAYER` upgrade of `load`
//! is not performed (the caller sees `get_data_version(&tag)` of the file).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use mc_nbt::{CompoundTag, NbtAccounter};
use uuid::Uuid;

use super::file_util::{create_temp_file, format_file_name_date_now, read_compressed, safe_replace_file, write_compressed};
use super::version::add_current_data_version;

#[derive(Clone, Debug)]
pub struct PlayerDataStorage {
    player_dir: PathBuf,
}

impl PlayerDataStorage {
    /// `new PlayerDataStorage(levelAccess, fixer)`: `player_dir` is `levelAccess.getLevelPath(PLAYER_DATA_DIR)`;
    /// like `File.mkdirs()` a failure to create it is ignored.
    pub fn new(player_dir: impl Into<PathBuf>) -> PlayerDataStorage {
        let player_dir = player_dir.into();
        let _ = fs::create_dir_all(&player_dir);
        PlayerDataStorage { player_dir }
    }

    pub fn player_dir(&self) -> &Path {
        &self.player_dir
    }

    /// `save(player)` for the already serialized player compound (`Player.saveWithoutId`, which stamps
    /// `DataVersion`): temp file → `Util.safeReplaceFile(real, tmp, old)`. Vanilla logs and swallows any failure
    /// to write; here it is returned.
    pub fn save(&self, uuid: &Uuid, mut data: CompoundTag) -> io::Result<()> {
        add_current_data_version(&mut data);
        let id = uuid.to_string();
        let tmp_file = create_temp_file(&self.player_dir, &format!("{id}-"), ".dat")?;
        write_compressed(&data, &tmp_file)?;
        let real_file = self.player_dir.join(format!("{id}.dat"));
        let old_file = self.player_dir.join(format!("{id}.dat_old"));
        safe_replace_file(&real_file, &tmp_file, &old_file);
        Ok(())
    }

    /// `backup(nameAndId, suffix)`: copy a damaged file to `<uuid>_corrupted_<time><suffix>`.
    fn backup(&self, uuid: &Uuid, name: &str, suffix: &str) {
        let id = uuid.to_string();
        let real_path = self.player_dir.join(format!("{id}{suffix}"));
        let backup_path = self.player_dir.join(format!("{id}_corrupted_{}{suffix}", format_file_name_date_now()));
        if real_path.is_file() {
            let copied = fs::copy(&real_path, &backup_path).and_then(|_| {
                // StandardCopyOption.COPY_ATTRIBUTES: keep the modification time
                let modified = fs::metadata(&real_path)?.modified()?;
                fs::File::options().write(true).open(&backup_path)?.set_modified(modified)
            });
            if let Err(e) = copied {
                log::warn!("Failed to copy the player.dat file for {name}: {e}");
            }
        }
    }

    fn load_suffix(&self, uuid: &Uuid, name: &str, suffix: &str) -> Option<CompoundTag> {
        let real_file = self.player_dir.join(format!("{uuid}{suffix}"));
        if real_file.exists() && real_file.is_file() {
            match read_compressed(&real_file, &mut NbtAccounter::unlimited_heap()) {
                Ok(tag) => return Some(tag),
                Err(_) => log::warn!("Failed to load player data for {name}"),
            }
        }
        None
    }

    /// `load(nameAndId)`: `<uuid>.dat`; if that is unreadable, back it up and fall back to `<uuid>.dat_old`.
    pub fn load(&self, uuid: &Uuid, name: &str) -> Option<CompoundTag> {
        let tag = self.load_suffix(uuid, name, ".dat");
        if tag.is_none() {
            self.backup(uuid, name, ".dat");
        }
        tag.or_else(|| self.load_suffix(uuid, name, ".dat_old"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mc_nbt::Tag;

    #[test]
    fn save_load_rotate_and_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let storage = PlayerDataStorage::new(dir.path().join("players/data"));
        let uuid = Uuid::from_u128(0x1234_5678_9abc_def0_1122_3344_5566_7788);
        assert!(storage.load(&uuid, "Bob").is_none());

        storage.save(&uuid, CompoundTag::new().with("Health", Tag::float(20.0))).unwrap();
        storage.save(&uuid, CompoundTag::new().with("Health", Tag::float(10.0))).unwrap();
        let tag = storage.load(&uuid, "Bob").unwrap();
        assert_eq!(tag.get_float("Health"), Some(10.0));
        assert_eq!(tag.get_int("DataVersion"), Some(5023));
        let old = read_compressed(&storage.player_dir().join(format!("{uuid}.dat_old")), &mut NbtAccounter::unlimited_heap()).unwrap();
        assert_eq!(old.get_float("Health"), Some(20.0));
        // no temp files left behind
        assert_eq!(fs::read_dir(storage.player_dir()).unwrap().count(), 2);

        // corrupt .dat -> backed up copy + fallback to .dat_old
        fs::write(storage.player_dir().join(format!("{uuid}.dat")), b"garbage").unwrap();
        let fallback = storage.load(&uuid, "Bob").unwrap();
        assert_eq!(fallback.get_float("Health"), Some(20.0));
        let corrupted = fs::read_dir(storage.player_dir())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains("_corrupted_"))
            .count();
        assert_eq!(corrupted, 1);
    }
}
