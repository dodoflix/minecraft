//! `net.minecraft.util.filefix.operations` and `FileFixUtil`: the operations a file fix is made of. Every operation
//! runs against a base path of the [`CowTree`] (the root, or a folder an [`FileFixOperation::ApplyInFolders`]
//! found); the paths of an operation are relative to it, and one starting with `/` is relative to the root.

use std::io;
use std::path::{Path, PathBuf};

use mc_datafix::{DataFixTypes, TypeReference};
use regex::{Captures, Regex, RegexBuilder};

use super::access::{ChunkNbt, LevelDat, MissingSeverity, PlayerData, SavedDataNbt};
use super::cow_tree::{CowFsError, CowTree};
use super::error::FileFixError;
use super::progress::UpgradeProgress;
use super::relation::FileRelation;
use crate::region::RegionStorageInfo;

/// `Move`: a file or folder to another name; a source that is not there is skipped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Move {
    from: String,
    to: String,
}

impl Move {
    pub fn new(from: &str, to: &str) -> Move {
        Move {
            from: from.to_owned(),
            to: to.to_owned(),
        }
    }

    /// `FileFixOperations.moveSimple`.
    pub fn simple(file: &str) -> Move {
        Move::new(file, file)
    }

    /// `relative(sourceFolder, targetFolder)`.
    pub fn relative(&self, source_folder: &str, target_folder: &str) -> Move {
        Move {
            from: format!("{source_folder}/{}", self.from),
            to: format!("{target_folder}/{}", self.to),
        }
    }
}

/// `RegexMove`: every entry of the folder whose whole name matches is renamed with the replacement (`$1` groups,
/// `\x` for a literal `x`, as `Matcher.replaceAll`).
#[derive(Clone, Debug)]
pub struct RegexMove {
    from_pattern: Regex,
    to_replacement: String,
}

impl RegexMove {
    /// # Panics
    /// On an invalid pattern, like `Pattern.compile`.
    pub fn new(from_pattern: &str, to_replacement: &str) -> RegexMove {
        RegexMove {
            from_pattern: RegexBuilder::new(&format!("^(?:{from_pattern})$"))
                .unicode(false)
                .build()
                .unwrap_or_else(|e| panic!("invalid pattern {from_pattern}: {e}")),
            to_replacement: to_replacement.to_owned(),
        }
    }
}

/// `Matcher.replaceAll(replacement)` for one match.
fn java_replacement(replacement: &str, captures: &Captures<'_>) -> String {
    let mut out = String::new();
    let mut chars = replacement.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.extend(chars.next()),
            '$' => {
                let mut group = chars
                    .next()
                    .and_then(|d| d.to_digit(10))
                    .unwrap_or_else(|| {
                        panic!("Illegal group reference in replacement {replacement}")
                    }) as usize;
                // As many digits as still name a group.
                while let Some(next) = chars.peek().and_then(|d| d.to_digit(10)) {
                    let longer = group * 10 + next as usize;
                    if longer >= captures.len() {
                        break;
                    }
                    group = longer;
                    chars.next();
                }
                assert!(
                    group < captures.len(),
                    "No group {group} in replacement {replacement}"
                );
                out.push_str(captures.get(group).map_or("", |m| m.as_str()));
            }
            c => out.push(c),
        }
    }
    out
}

/// `DeleteFileOrEmptyDirectory`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeleteFileOrEmptyDirectory {
    target: String,
}

/// `ModifyContent`'s `FixFunction` with the file access it needs.
type FixFunction = Box<dyn Fn(&mut ContentContext<'_>) -> Result<(), FileFixError> + Send + Sync>;

/// `ModifyContent`: code that reads and writes file contents, at the data version of its fix.
pub struct ModifyContent {
    data_version: i32,
    fix_function: FixFunction,
}

impl ModifyContent {
    pub fn new(
        data_version: i32,
        fix_function: impl Fn(&mut ContentContext<'_>) -> Result<(), FileFixError>
        + Send
        + Sync
        + 'static,
    ) -> ModifyContent {
        ModifyContent {
            data_version,
            fix_function: Box::new(fix_function),
        }
    }
}

/// What a [`ModifyContent`] function works with: the files of the tree (the `FileAccessProvider`).
pub struct ContentContext<'a> {
    pub tree: &'a mut CowTree,
    /// `FileAccessProvider.baseDirectory`.
    pub base_directory: &'a Path,
    pub progress: &'a UpgradeProgress,
    /// `FileAccessProvider.dataVersion`: the version of the fix, to which the files are read and written.
    pub data_version: i32,
}

impl ContentContext<'_> {
    /// `FileAccess.get()` paths: where `relation` leads from the base directory.
    pub fn paths(&self, relation: &FileRelation) -> Vec<PathBuf> {
        relation.paths(self.tree, self.base_directory)
    }

    pub fn level_dat(&self, relation: &FileRelation) -> Vec<LevelDat> {
        self.paths(relation)
            .into_iter()
            .map(|path| LevelDat::new(path, self.data_version))
            .collect()
    }

    pub fn player_data(&self, relation: &FileRelation) -> Vec<PlayerData> {
        self.paths(relation)
            .into_iter()
            .map(|path| PlayerData::new(path, self.data_version))
            .collect()
    }

    pub fn saved_data(
        &self,
        ty: TypeReference,
        relation: &FileRelation,
        missing_severity: MissingSeverity,
    ) -> Vec<SavedDataNbt> {
        self.paths(relation)
            .into_iter()
            .map(|path| SavedDataNbt::new(ty, path, self.data_version, missing_severity))
            .collect()
    }

    /// The `ChunkNbt`s must be closed (`ChunkNbt::close`) for their changes to reach the tree.
    pub fn chunk_nbt(
        &mut self,
        relation: &FileRelation,
        info: &RegionStorageInfo,
        ty: DataFixTypes,
    ) -> io::Result<Vec<ChunkNbt>> {
        self.paths(relation)
            .into_iter()
            .map(|path| ChunkNbt::new(self.tree, info.clone(), path, ty, self.data_version))
            .collect()
    }
}

/// `FileAccess.getOnlyFile`: an `IllegalStateException` for any other number of files.
pub fn only_file<T>(mut files: Vec<T>) -> Result<T, FileFixError> {
    if files.len() == 1 {
        Ok(files.remove(0))
    } else {
        Err(FileFixError::Unchecked(format!(
            "Trying to get only file, but there are {} files",
            files.len()
        )))
    }
}

/// `FileFixOperation`.
pub enum FileFixOperation {
    Move(Move),
    RegexMove(RegexMove),
    /// `GroupMove`: the moves, each relative to each (source folder, target folder) pair.
    GroupMove {
        from_to: Vec<(String, String)>,
        moves: Vec<Move>,
    },
    /// `ApplyInFolders`.
    ApplyInFolders {
        folders: FileRelation,
        operations: Vec<FileFixOperation>,
    },
    Delete(DeleteFileOrEmptyDirectory),
    ModifyContent(ModifyContent),
}

impl FileFixOperation {
    /// `FileFixOperations.delete`.
    pub fn delete(target: &str) -> FileFixOperation {
        FileFixOperation::Delete(DeleteFileOrEmptyDirectory {
            target: target.to_owned(),
        })
    }

    pub fn fix(
        &self,
        tree: &mut CowTree,
        base_directory: &Path,
        progress: &UpgradeProgress,
    ) -> Result<(), FileFixError> {
        match self {
            FileFixOperation::Move(m) => Ok(move_file(tree, base_directory, &m.from, &m.to)?),
            FileFixOperation::RegexMove(m) => Ok(regex_move(m, tree, base_directory)?),
            FileFixOperation::GroupMove { from_to, moves } => {
                for (from, to) in from_to {
                    for m in moves {
                        let relative = m.relative(from, to);
                        move_file(tree, base_directory, &relative.from, &relative.to)?;
                    }
                }
                Ok(())
            }
            FileFixOperation::ApplyInFolders {
                folders,
                operations,
            } => {
                for path in folders.paths(tree, base_directory) {
                    for operation in operations {
                        operation.fix(tree, &path, progress)?;
                    }
                }
                Ok(())
            }
            FileFixOperation::Delete(d) => Ok(delete_file_or_empty_directory(
                tree,
                base_directory,
                &d.target,
            )?),
            FileFixOperation::ModifyContent(m) => (m.fix_function)(&mut ContentContext {
                tree,
                base_directory,
                progress,
                data_version: m.data_version,
            }),
        }
    }
}

/// How the Java `Path` of the virtual file system prints: absolute.
fn shown(path: &Path) -> String {
    format!("/{}", path.display()).replace("//", "/")
}

/// `FileFixUtil.moveFile`.
pub fn move_file(
    tree: &mut CowTree,
    base_directory: &Path,
    from: &str,
    to: &str,
) -> io::Result<()> {
    let from_absolute = base_directory.join(from);
    if tree.exists(&from_absolute) {
        let to_absolute = base_directory.join(to);
        if tree.exists(&to_absolute) {
            return Err(CowFsError::FileAlreadyExists(format!(
                "Cannot move file from {from} to {to}: Target already exists"
            ))
            .into());
        }
        if let Some(parent) = to_absolute.parent() {
            tree.create_dir_all(parent)?;
        }
        tree.rename(&from_absolute, &to_absolute)?;
    }
    Ok(())
}

fn regex_move(m: &RegexMove, tree: &mut CowTree, base_directory: &Path) -> io::Result<()> {
    if tree.exists(base_directory) && tree.is_dir(base_directory) {
        for file_name in tree.list(base_directory)? {
            if let Some(captures) = m.from_pattern.captures(&file_name) {
                let new_name = java_replacement(&m.to_replacement, &captures);
                move_file(tree, base_directory, &file_name, &new_name)?;
            }
        }
    }
    Ok(())
}

/// `FileFixUtil.deleteFileOrEmptyDirectory`: a folder that is not empty (a lone `.DS_Store` aside) stays, with a
/// warning.
pub fn delete_file_or_empty_directory(
    tree: &mut CowTree,
    base_directory: &Path,
    file: &str,
) -> io::Result<()> {
    let to_delete = base_directory.join(file);
    if tree.exists(&to_delete) {
        if tree.is_dir(&to_delete) {
            let result = (|| {
                let files = tree.list(&to_delete)?;
                if files.len() == 1 && files[0] == ".DS_Store" {
                    let ds_store = to_delete.join(&files[0]);
                    log::debug!("Attempting to delete DS_Store at '{}'", shown(&to_delete));
                    if !tree.delete_if_exists(&ds_store)? {
                        log::warn!(
                            "Failed to delete file '{}' at '{}'",
                            shown(&ds_store),
                            shown(&to_delete)
                        );
                    }
                }
                tree.list(&to_delete)
            })();
            match result {
                Ok(files) if !files.is_empty() => {
                    let content: Vec<String> =
                        files.iter().map(|n| shown(&to_delete.join(n))).collect();
                    log::warn!(
                        "Failed to delete directory '{}', as it's not empty. Content: [{}]",
                        shown(&to_delete),
                        content.join(", ")
                    );
                    return Ok(());
                }
                Ok(_) => {}
                Err(e) => {
                    log::warn!(
                        "Failed to delete directory '{}' because {e}",
                        shown(&to_delete)
                    );
                    return Ok(());
                }
            }
        }
        tree.delete_if_exists(&to_delete)?;
    }
    Ok(())
}
