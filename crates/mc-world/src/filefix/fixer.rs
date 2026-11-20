//! `FileFixerUpper`: upgrades the file layout of a world saved before the latest file fixer version.
//!
//! The fixes run against a [`CowTree`] of the world, so the world folder is not touched while they run: they leave
//! the new files in `filefix/` (`new_world/` for the new tree, `cow/` for the files they wrote). Only then is the new
//! world swapped in: with hard links, the untouched files are linked into `new_world/` and that folder becomes
//! `<world> upgraded`; without, the untouched files are *moved* into `new_world/` after the moves were written to
//! `filefix/upgrade_in_progress.json`, so an interrupted upgrade is continued at the next start and a failing one
//! moves every file back. The old world folder is then replaced by the new one (and takes the journal along).

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use mc_core::gson::{JsonElement, JsonObject};
use mc_core::java_exception::JavaException;
use mc_core::java_string::JavaString;
use mc_datafix::{
    CURRENT_VERSION, DataFixTypes, DataFixer, FileFixRegistration, FileFixerRegistry, References,
    world_version_to_file_fixer_version,
};
use mc_nbt::{CompoundTag, NbtAccounter};

use super::cow_tree::{
    CowTree, create_directories, hard_link_files, move_files, move_files_with_retry,
    try_revert_moves,
};
use super::error::{FileFixError, FileMove, FileSystemCapabilities, java_to_string};
use super::fix::FileFix;
use super::jdk_files::{create_directory, create_file, delete_if_exists, delete_tree, read_string};
use super::names::{find_available_name, is_available_name_of};
use super::progress::{Status, Type, UpgradeProgress};
use crate::level_storage::{
    LevelStorageAccess, epoch_millis, get_data_version, read_compressed, write_compressed,
    write_version_tag,
};

const FILE_FIX_DIRECTORY_NAME: &str = "filefix";
const NEW_WORLD_TEMP_NAME: &str = "new_world";
const UPGRADE_IN_PROGRESS_NAME: &str = "upgrade_in_progress.json";

/// A step of the upgrade at which [`FixHooks::checkpoint`] can stop it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Checkpoint {
    /// The fixes ran and `filefix/new_world` holds the new tree's directories and the files the fixes wrote.
    FixesApplied,
    /// `upgrade_in_progress.json` was written (no hard links).
    JournalWritten,
    /// Before the move of the untouched file with this index into `new_world` (no hard links).
    MoveFile(usize),
    /// `new_world` became the top-level `<world> upgraded` folder.
    NewWorldAtTopLevel,
    /// The old world folder was moved out of the way.
    OldWorldMovedOut,
    /// The new world folder was moved in.
    NewWorldMovedIn,
}

/// What the upgrade asks of its environment, replaceable to try the other file system behaviours and to stop the
/// upgrade at any step.
pub trait FixHooks: Send + Sync {
    /// `detectFileSystemCapabilities(dir)`.
    fn file_system_capabilities(&self, dir: &Path) -> io::Result<FileSystemCapabilities> {
        detect_file_system_capabilities(dir)
    }

    /// Called at each [`Checkpoint`]; an error fails that step as an I/O error of the step would.
    fn checkpoint(&self, _at: Checkpoint) -> io::Result<()> {
        Ok(())
    }
}

struct RealEnvironment;

impl FixHooks for RealEnvironment {}

pub struct FileFixerUpper {
    data_fixer: &'static DataFixer,
    file_fixes: Vec<FileFix>,
    latest_file_fixer_version: i32,
    hooks: Box<dyn FixHooks>,
}

fn random_name() -> String {
    use std::hash::{BuildHasher, Hasher};
    std::collections::hash_map::RandomState::new()
        .build_hasher()
        .finish()
        .to_string()
}

/// `detectFileSystemCapabilities`: tries an atomic move and a hard link in `dir`.
pub fn detect_file_system_capabilities(dir: &Path) -> io::Result<FileSystemCapabilities> {
    Ok(FileSystemCapabilities {
        atomic_move: supports_atomic_move(dir)?,
        hard_links: supports_hard_links(dir)?,
    })
}

/// Runs `test(source, target)` on two fresh names in `dir`, removing both afterwards.
fn with_scratch_files<T>(
    dir: &Path,
    test: impl FnOnce(&Path, &Path) -> io::Result<T>,
) -> io::Result<T> {
    let source = dir.join(random_name());
    let target = dir.join(random_name());
    create_file(&source)?;
    let result = test(&source, &target);
    for path in [&source, &target] {
        delete_if_exists(path)?;
    }
    result
}

fn supports_atomic_move(dir: &Path) -> io::Result<bool> {
    with_scratch_files(dir, |source, target| match fs::rename(source, target) {
        Ok(()) => Ok(true),
        Err(e)
            if matches!(
                e.kind(),
                io::ErrorKind::CrossesDevices | io::ErrorKind::Unsupported
            ) =>
        {
            Ok(false)
        }
        Err(e) => Err(e),
    })
}

fn supports_hard_links(dir: &Path) -> io::Result<bool> {
    with_scratch_files(dir, |source, target| {
        Ok(fs::hard_link(source, target).is_ok())
    })
}

/// `deleteDirectory`: deletes whatever it can, then what is left or fails.
fn delete_directory(directory: &Path) -> io::Result<()> {
    fn delete_all(path: &Path) {
        let Ok(metadata) = fs::symlink_metadata(path) else {
            return;
        };
        if metadata.is_dir() {
            if let Ok(entries) = fs::read_dir(path) {
                for entry in entries.flatten() {
                    delete_all(&entry.path());
                }
            }
            let _ = fs::remove_dir(path);
        } else {
            let _ = fs::remove_file(path);
        }
    }
    delete_all(directory);
    if directory.exists() {
        delete_tree(directory)?;
    }
    Ok(())
}

/// `cleanup`.
fn cleanup(file_fix_directory: &Path) {
    if let Err(e) = delete_directory(file_fix_directory) {
        log::error!("Failed to clean up: {e}");
    }
}

/// `addVersionsToLevelData`: the data version, the time and the `Version` of the running game.
pub(crate) fn add_versions_to_level_data(mut fixed: CompoundTag, to_version: i32) -> CompoundTag {
    fixed.put_int("DataVersion", to_version);
    fixed.put_long("LastPlayed", epoch_millis());
    write_version_tag(&mut fixed);
    fixed
}

/// `UpgradeInProgress.codec`'s path codec: `base` plus a relative path that stays inside it.
fn guarded_path(base: &Path, text: &str) -> Result<PathBuf, FileFixError> {
    use std::path::Component;
    let path = Path::new(text);
    let invalid = |message: String| FileFixError::Unchecked(message);
    if path.is_absolute() {
        return Err(invalid(format!("Illegal absolute path: {text}")));
    }
    let mut normal = false;
    for component in path.components() {
        match component {
            Component::Normal(_) => normal = true,
            Component::CurDir | Component::ParentDir => {
                return Err(invalid(format!("Illegal path traversal: {text}")));
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(invalid(format!("Illegal absolute path: {text}")));
            }
        }
    }
    if !normal {
        return Err(invalid(format!("Path is empty: {text}")));
    }
    Ok(base.join(path))
}

/// `writeMoves`: `{"moves": [{"from", "to"}], "upgraded_folder": name}` (`from` relative to the world, `to` to the new
/// world; the folder the new world is moved to), pretty printed like Gson, and put in place whole so that an
/// interrupted write never leaves a journal that cannot be read. Vanilla's journal has no `upgraded_folder`; without
/// it a continuation cannot tell that the new world was already moved there and fails at every start.
pub(super) fn write_moves(
    moves: &[FileMove],
    from_directory: &Path,
    to_directory: &Path,
    upgraded_folder: &str,
    file_path: &Path,
) -> io::Result<()> {
    let relative = |base: &Path, path: &Path| -> io::Result<JsonElement> {
        let relative = path.strip_prefix(base).map_err(|_| {
            io::Error::other(format!(
                "Path {} does not start with base folder {}",
                path.display(),
                base.display()
            ))
        })?;
        Ok(JsonElement::String(
            relative.to_string_lossy().into_owned().into(),
        ))
    };
    let mut list = Vec::new();
    for m in moves {
        let mut entry = JsonObject::new();
        entry.add("from", relative(from_directory, &m.from)?);
        entry.add("to", relative(to_directory, &m.to)?);
        list.push(JsonElement::Object(entry));
    }
    let mut root = JsonObject::new();
    root.add("moves", JsonElement::Array(list));
    root.add(
        "upgraded_folder",
        JsonElement::String(upgraded_folder.to_owned().into()),
    );
    let json = JsonElement::Object(root).to_pretty_string();

    let mut tmp = file_path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    // `Files.writeString(file_path, ...)`: a failure names the journal, not the file it is staged in
    let named = |e| JavaException::from_file_error(e, file_path);
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&tmp)
        .map_err(named)?;
    file.write_all(json.as_bytes())?;
    file.sync_all()?;
    fs::rename(&tmp, file_path).map_err(named)?;
    // Persist the rename: the journal must exist before the first file is moved.
    #[cfg(unix)]
    if let Some(parent) = file_path.parent() {
        File::open(if parent.as_os_str().is_empty() {
            Path::new(".")
        } else {
            parent
        })?
        .sync_all()?;
    }
    Ok(())
}

/// The journal, and what a staged upgrade holds: the moves and the folder of the saves directory the new world is
/// moved to (`None` before the journal was written).
#[derive(Debug)]
pub(super) struct Journal {
    pub(super) moves: Vec<FileMove>,
    pub(super) upgraded_folder: Option<String>,
}

/// `readMoves`.
pub(super) fn read_moves(
    from_directory: &Path,
    to_directory: &Path,
    file_path: &Path,
) -> Result<Journal, FileFixError> {
    // A journal that cannot be parsed is a `RuntimeException` of Gson or the codec, not an `IOException`.
    let invalid = |message: String| FileFixError::Unchecked(message);
    let json = mc_core::gson::parse_object_strict(&read_string(file_path)?)
        .map_err(|e| invalid(e.to_string()))?;
    let Some(JsonElement::Array(entries)) = json.get("moves") else {
        return Err(invalid("No key moves in MapLike".to_owned()));
    };
    let field = |entry: &JsonElement, name: &str| -> Result<String, FileFixError> {
        match entry.as_object().and_then(|o| o.get(name)) {
            Some(value) => value
                .as_string()
                .map(JavaString::into_string_lossy)
                .map_err(|e| invalid(e.to_string())),
            None => Err(invalid(format!("No key {name} in MapLike"))),
        }
    };
    let moves = entries
        .iter()
        .map(|entry| {
            Ok(FileMove {
                from: guarded_path(from_directory, &field(entry, "from")?)?,
                to: guarded_path(to_directory, &field(entry, "to")?)?,
            })
        })
        .collect::<Result<Vec<_>, FileFixError>>()?;
    let upgraded_folder = match json.get("upgraded_folder") {
        Some(name) => {
            let name = name
                .as_string()
                .map(JavaString::into_string_lossy)
                .map_err(|e| invalid(e.to_string()))?;
            if Path::new(&name).components().count() != 1
                || guarded_path(Path::new(""), &name).is_err()
            {
                return Err(invalid(format!("Illegal folder name: {name}")));
            }
            Some(name)
        }
        None => None,
    };
    Ok(Journal {
        moves,
        upgraded_folder,
    })
}

impl FileFixerUpper {
    /// `new FileFixerUpper(dataFixer, fileFixes, latestFileFixerVersion)`.
    pub fn new(
        data_fixer: &'static DataFixer,
        file_fixes: Vec<FileFix>,
        latest_file_fixer_version: i32,
    ) -> FileFixerUpper {
        FileFixerUpper {
            data_fixer,
            file_fixes,
            latest_file_fixer_version,
            hooks: Box::new(RealEnvironment),
        }
    }

    /// `FileFixerUpper.Builder.build`: the file fixes registered in `registry` (`DataFixers.getFileFixer()`), each
    /// built by `make_fix` from its registration; a fix must be of the version it is registered for.
    ///
    /// # Panics
    /// When a built fix is of another version than its registration.
    pub fn from_registry(
        data_fixer: &'static DataFixer,
        registry: &FileFixerRegistry,
        make_fix: impl Fn(&FileFixRegistration) -> FileFix,
    ) -> FileFixerUpper {
        let file_fixes = registry
            .fixes()
            .iter()
            .map(|registration| {
                let fix = make_fix(registration);
                assert_eq!(
                    fix.version(),
                    registration.version,
                    "file fix {} is registered for another version",
                    registration.name
                );
                fix
            })
            .collect();
        FileFixerUpper::new(data_fixer, file_fixes, registry.latest_file_fixer_version())
    }

    /// The upgrade asks `hooks` for what the file system decides and reports its steps to it.
    pub fn with_hooks(mut self, hooks: Box<dyn FixHooks>) -> FileFixerUpper {
        self.hooks = hooks;
        self
    }

    /// `requiresFileFixing`.
    pub fn requires_file_fixing(&self, level_data_version: i32) -> bool {
        world_version_to_file_fixer_version(level_data_version) < self.latest_file_fixer_version
    }

    /// `fix(worldAccess, levelDataTag, upgradeProgress)`: upgrades the layout of the world when `level_data_tag`
    /// (the `Data` of level.dat as read) is of a version that needs it, and returns the level data fixed to the
    /// current version.
    pub fn fix(
        &self,
        world_access: &mut LevelStorageAccess,
        level_data_tag: CompoundTag,
        upgrade_progress: &UpgradeProgress,
    ) -> Result<CompoundTag, FileFixError> {
        self.fix_to(
            world_access,
            level_data_tag,
            upgrade_progress,
            CURRENT_VERSION,
        )
    }

    /// `fix(.., toVersion)`.
    pub fn fix_to(
        &self,
        world_access: &mut LevelStorageAccess,
        mut level_data_tag: CompoundTag,
        upgrade_progress: &UpgradeProgress,
        to_version: i32,
    ) -> Result<CompoundTag, FileFixError> {
        let mut loaded_version = get_data_version(&level_data_tag);
        if self.requires_file_fixing(loaded_version) {
            log::info!(
                "Starting upgrade for world \"{}\"",
                world_access.get_level_id()
            );
            upgrade_progress.notifications().world_upgrade_started();
            let world_folder = world_access.get_level_directory().path.clone();
            let file_fix_directory = world_folder.join(FILE_FIX_DIRECTORY_NAME);
            let temp_world = file_fix_directory.join(NEW_WORLD_TEMP_NAME);

            let staged = match self.start_or_continue_file_fixing(
                upgrade_progress,
                to_version,
                &world_folder,
                &temp_world,
                &file_fix_directory,
                loaded_version,
            ) {
                Ok(staged) => staged,
                // only an `IOException` is wrapped: an unchecked exception (`ReportedNbtException`, ...) passes
                Err(FileFixError::Io(e)) if JavaException::is_unchecked(&e) => {
                    return Err(FileFixError::Unchecked(
                        JavaException::of_io(&e).to_string(),
                    ));
                }
                Err(FileFixError::Io(e)) => {
                    upgrade_progress
                        .notifications()
                        .world_upgrade_failed("The world upgrade failed during file fixing");
                    return Err(FileFixError::aborted(e));
                }
                Err(other) => return Err(other),
            };

            match self.swap_in_fixed_world(
                world_access,
                &staged,
                &file_fix_directory,
                &temp_world,
                upgrade_progress,
            ) {
                Ok(()) => {}
                Err(error) => {
                    if let FileFixError::Aborted {
                        not_reverted_moves, ..
                    } = &error
                    {
                        if not_reverted_moves.is_empty() {
                            // The folder of a continued upgrade holds the only copy of the files that an earlier
                            // run moved: its journal says so, and the next start continues from it.
                            if !file_fix_directory.join(UPGRADE_IN_PROGRESS_NAME).exists() {
                                cleanup(&file_fix_directory);
                            }
                            upgrade_progress.notifications().world_upgrade_failed(
                                "The world upgrade failed while swapping in the file-fixed world",
                            );
                        } else {
                            upgrade_progress.notifications().world_upgrade_failed(
                                "The world upgrade failed while swapping in the file-fixed world and some moves were not reverted",
                            );
                        }
                    }
                    return Err(error);
                }
            }

            level_data_tag = match world_access.get_unfixed_data_tag(false) {
                Ok(tag) => tag,
                Err(e) if JavaException::is_unchecked(&e) => {
                    return Err(FileFixError::Unchecked(
                        JavaException::of_io(&e).to_string(),
                    ));
                }
                Err(e) => {
                    upgrade_progress
                        .notifications()
                        .world_upgrade_failed("The world upgrade failed while reading level.dat");
                    let kind = e.kind();
                    return Err(FileFixError::Io(
                        JavaException::unchecked(
                            "java.io.UncheckedIOException",
                            java_to_string(&e),
                        )
                        .into_io(kind),
                    ));
                }
            };
            loaded_version = get_data_version(&level_data_tag);
        }
        let fixed = DataFixTypes::LEVEL.update_tag_to_current_version(
            self.data_fixer,
            &level_data_tag,
            loaded_version,
        );
        Ok(add_versions_to_level_data(fixed, to_version))
    }

    fn start_or_continue_file_fixing(
        &self,
        upgrade_progress: &UpgradeProgress,
        to_version: i32,
        world_folder: &Path,
        temp_world: &Path,
        file_fix_directory: &Path,
        loaded_version: i32,
    ) -> Result<Journal, FileFixError> {
        let upgrade_in_progress_file = file_fix_directory.join(UPGRADE_IN_PROGRESS_NAME);
        if upgrade_in_progress_file.exists() {
            log::warn!("Found previously interrupted world upgrade, attempting to continue it");
            return read_moves(world_folder, temp_world, &upgrade_in_progress_file);
        }
        if file_fix_directory.exists() {
            delete_directory(file_fix_directory)?;
        }
        let result = create_directory(file_fix_directory)
            .map_err(FileFixError::from)
            .and_then(|()| {
                self.apply_file_fixers_on_cow(
                    upgrade_progress,
                    loaded_version,
                    to_version,
                    world_folder,
                    file_fix_directory,
                    temp_world,
                )
            });
        if result.is_err() {
            cleanup(file_fix_directory);
        }
        result
    }

    fn apply_file_fixers_on_cow(
        &self,
        upgrade_progress: &UpgradeProgress,
        loaded_version: i32,
        to_version: i32,
        world_folder: &Path,
        file_fix_directory: &Path,
        temp_world: &Path,
    ) -> Result<Journal, FileFixError> {
        let mut tree = CowTree::create(world_folder, &file_fix_directory.join("cow"), |path| {
            path == file_fix_directory
        })?;
        self.apply_file_fixers(
            &mut tree,
            upgrade_progress,
            loaded_version,
            to_version,
            Path::new(""),
        )?;
        let moves = tree.collect_move_operations(temp_world);
        create_directory(temp_world)?;
        create_directories(&moves.directories)?;
        move_files(&moves.copied_files)?;
        tree.close()?;
        self.hooks.checkpoint(Checkpoint::FixesApplied)?;
        Ok(Journal {
            moves: moves.preexisting_files,
            upgraded_folder: None,
        })
    }

    /// `applyFileFixers`: the fixes of the versions after the world's, then the new level.dat; `level.dat_old` and
    /// `session.lock` are left out of the new world.
    pub fn apply_file_fixers(
        &self,
        tree: &mut CowTree,
        upgrade_progress: &UpgradeProgress,
        loaded_version: i32,
        to_version: i32,
        base_path: &Path,
    ) -> Result<(), FileFixError> {
        let applicable = self.get_applicable_fixers(loaded_version, to_version);
        upgrade_progress.set_type(Type::Files);
        upgrade_progress.set_status(Status::Counting);
        upgrade_progress.add_total_file_fix_operations(
            applicable
                .iter()
                .map(|fix| fix.count_file_operations() as i32)
                .sum(),
        );
        upgrade_progress.set_status(Status::Upgrading);
        upgrade_progress.set_applicable_fixer_amount(applicable.len() as i32);
        for file_fix in applicable {
            upgrade_progress.increment_running_file_fixer();
            file_fix.run_fix_operations(tree, base_path, upgrade_progress)?;
        }
        self.write_updated_level_data(tree, base_path, to_version)?;
        tree.delete_if_exists(&base_path.join("level.dat_old"))?;
        tree.delete_if_exists(&base_path.join("session.lock"))?;
        Ok(())
    }

    fn get_applicable_fixers(&self, from_version: i32, to_version: i32) -> Vec<&FileFix> {
        let file_fixer_from_version = world_version_to_file_fixer_version(from_version);
        self.file_fixes
            .iter()
            .filter(|f| f.version() > file_fixer_from_version && f.version() <= to_version)
            .collect()
    }

    /// `writeUpdatedLevelData`.
    fn write_updated_level_data(
        &self,
        tree: &mut CowTree,
        base_path: &Path,
        to_version: i32,
    ) -> io::Result<()> {
        let level_dat_path = base_path.join("level.dat");
        let unfixed_level_dat = read_compressed(
            tree.file_for_read(&level_dat_path)?,
            &mut NbtAccounter::default_quota(),
        )?;
        let unfixed_data_tag = unfixed_level_dat.get_compound_or_empty("Data");
        let data_version = get_data_version(unfixed_data_tag);
        let fixed = self.data_fixer.update_tag(
            &References::LEVEL,
            unfixed_data_tag,
            data_version,
            to_version,
        );
        let mut root = CompoundTag::new();
        root.put("Data", add_versions_to_level_data(fixed, to_version));
        write_compressed(&root, &tree.file_for_write(&level_dat_path, true)?)
    }

    /// `swapInFixedWorld`.
    fn swap_in_fixed_world(
        &self,
        world_access: &mut LevelStorageAccess,
        staged: &Journal,
        file_fix_directory: &Path,
        temp_world: &Path,
        upgrade_progress: &UpgradeProgress,
    ) -> Result<(), FileFixError> {
        let moves = &staged.moves[..];
        let world_folder = world_access.get_level_directory().path.clone();
        let saves_directory = world_folder.parent().unwrap_or(Path::new("."));
        let world_name = world_folder
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();

        // The folder a continued upgrade moved the new world to; a journal is not trusted to name any other folder.
        let upgraded_name = format!("{world_name} upgraded");
        let announced_folder = staged
            .upgraded_folder
            .as_deref()
            .filter(|name| is_available_name_of(&upgraded_name, name));
        let (capabilities, temp_world_top_level, old_world_folder) = (|| -> io::Result<_> {
            let capabilities = self.hooks.file_system_capabilities(file_fix_directory)?;
            let top_level = saves_directory.join(match announced_folder {
                Some(name) => name.to_owned(),
                None => find_available_name(saves_directory, &upgraded_name, "")?,
            });
            let old = saves_directory.join(find_available_name(
                saves_directory,
                &format!("{world_name} OUTDATED"),
                "",
            )?);
            Ok((capabilities, top_level, old))
        })()
        .map_err(FileFixError::aborted)?;
        let file_name_of = |path: &Path| {
            path.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        };

        if !capabilities.atomic_move {
            return Err(FileFixError::AtomicMoveNotSupported { capabilities });
        }
        log::info!("File system capabilities: {capabilities}");
        let moves_file = file_fix_directory.join(UPGRADE_IN_PROGRESS_NAME);
        let aborted = |cause: io::Error, not_reverted_moves: Vec<FileMove>| FileFixError::Aborted {
            cause,
            not_reverted_moves,
            capabilities: Some(capabilities),
        };

        // A continued upgrade whose new world is already at the top level was interrupted after the moves.
        let already_moved =
            announced_folder.is_some() && !temp_world.exists() && temp_world_top_level.is_dir();

        (|| -> io::Result<()> {
            if already_moved {
                Ok(())
            } else if capabilities.hard_links {
                hard_link_files(moves)
            } else {
                write_moves(
                    moves,
                    &world_folder,
                    temp_world,
                    &file_name_of(&temp_world_top_level),
                    &moves_file,
                )?;
                self.hooks.checkpoint(Checkpoint::JournalWritten)
            }
        })()
        .map_err(|e| aborted(e, Vec::new()))?;

        log::info!(
            "Applying file structure changes for world \"{}\"",
            world_access.get_level_id()
        );
        if already_moved {
            log::info!("Complete move");
        } else if capabilities.hard_links {
            log::info!("Moving new hardlinked world to top level");
            move_dir(temp_world, &temp_world_top_level)
                .and_then(|()| self.hooks.checkpoint(Checkpoint::NewWorldAtTopLevel))
                .map_err(|e| {
                    log::error!("Encountered error trying to move world folder: {e}");
                    aborted(e, Vec::new())
                })?;
        } else {
            let result = (|| -> io::Result<()> {
                log::info!("Moving files into new file structure");
                move_files_with_retry(moves, &|index| {
                    self.hooks.checkpoint(Checkpoint::MoveFile(index))
                })?;
                log::info!("Moving new world to top level");
                move_dir(temp_world, &temp_world_top_level)?;
                self.hooks.checkpoint(Checkpoint::NewWorldAtTopLevel)
            })();
            if let Err(e) = result {
                log::error!("Encountered error while trying to create new world folder: {e}");
                let failed_moves = try_revert_moves(moves);
                if failed_moves.is_empty()
                    && let Err(delete_error) = remove_if_exists(&moves_file)
                {
                    log::warn!("Failed to delete {}: {delete_error}", moves_file.display());
                }
                return Err(aborted(e, failed_moves));
            }
            log::info!("Complete move");
        }

        // Vanilla deletes the journal here and the old level.dat files next, which leaves a window in which a killed
        // process leaves a folder that is no longer a world (or one with no journal to continue from). The journal
        // and level.dat stay in the old world folder until it is moved out; the level.dat files go as soon as it is,
        // the journal with the folder.
        log::info!("Start cleanup");

        let mut succeeded = false;
        let released = world_access.release_temporarily_and_run(|| {
            let failed = |cause: io::Error| {
                io::Error::other(FailedCleanup(FileFixError::FailedCleanup {
                    cause,
                    new_world_folder_name: file_name_of(&temp_world_top_level),
                    capabilities,
                }))
            };
            log::info!("Moving out old world folder");
            match move_dir(&world_folder, &old_world_folder) {
                Ok(()) => delete_outdated_level_dat(&old_world_folder),
                Err(e) => {
                    log::warn!(
                        "Failed to move outdated world folder out of the way; will try to delete instead: {e}"
                    );
                    delete_outdated_level_dat(&world_folder);
                    if delete_directory(&world_folder).is_err() {
                        log::warn!("Failed to delete outdated world folder: {e}");
                        return Err(failed(e));
                    }
                }
            }
            self.hooks
                .checkpoint(Checkpoint::OldWorldMovedOut)
                .map_err(failed)?;
            log::info!("Moving in new world folder");
            if let Err(e) = move_dir(&temp_world_top_level, &world_folder)
                .and_then(|()| self.hooks.checkpoint(Checkpoint::NewWorldMovedIn))
            {
                log::warn!("Failed to move in new world folder: {e}");
                return Err(failed(e));
            }
            succeeded = true;
            Ok(())
        });
        if let Err(e) = released {
            // The closure's failure carries its `FileFixError`; an error of the lock is a cleanup failure as well.
            return Err(match cleanup_failure(e) {
                Ok(error) => error,
                Err(cause) => FileFixError::FailedCleanup {
                    cause,
                    new_world_folder_name: file_name_of(if succeeded {
                        &world_folder
                    } else {
                        &temp_world_top_level
                    }),
                    capabilities,
                },
            });
        }

        log::info!(
            "Done applying file structure changes for world \"{}\". Cleaning up outdated data...",
            world_access.get_level_id()
        );
        if old_world_folder.exists()
            && let Err(e) = delete_directory(&old_world_folder)
        {
            log::warn!("Failed to clean up old world folder: {e}");
        }
        log::info!("Upgrade done for world \"{}\"", world_access.get_level_id());
        upgrade_progress.notifications().world_upgrade_finished();
        Ok(())
    }
}

/// `Files.move(from, to)` of a folder: a failure carries the JDK's exception, naming both paths.
fn move_dir(from: &Path, to: &Path) -> io::Result<()> {
    fs::rename(from, to).map_err(|e| JavaException::from_file_pair_error(e, from, Some(to)))
}

/// The `FailedCleanupFileFixException` thrown inside the lambda of `releaseTemporarilyAndRun`, carried through the
/// `io::Error` it returns.
#[derive(Debug)]
struct FailedCleanup(FileFixError);

impl std::fmt::Display for FailedCleanup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for FailedCleanup {}

/// The `FileFixError` a failing closure of `releaseTemporarilyAndRun` put into its `io::Error`.
fn cleanup_failure(e: io::Error) -> Result<FileFixError, io::Error> {
    if e.get_ref().is_some_and(|inner| inner.is::<FailedCleanup>()) {
        let inner = e.into_inner().expect("checked");
        Ok(inner.downcast::<FailedCleanup>().expect("checked").0)
    } else {
        Err(e)
    }
}

/// Vanilla deletes the level.dat files of the old world before the swap. They stay until the folder is on its way out
/// (see `swap_in_fixed_world`), so that a folder which is no longer the world is never one a world list shows.
fn delete_outdated_level_dat(folder: &Path) {
    if let Err(e) = remove_if_exists(&folder.join("level.dat"))
        .and_then(|()| remove_if_exists(&folder.join("level.dat_old")))
    {
        log::warn!("Failed to delete outdated level.dat files: {e}");
    }
}

/// `Files.deleteIfExists`.
fn remove_if_exists(path: &Path) -> io::Result<()> {
    delete_if_exists(path).map(drop)
}
