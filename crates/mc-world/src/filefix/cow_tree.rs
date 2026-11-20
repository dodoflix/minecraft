//! The copy-on-write view of a world folder the file fixes run against.
//!
//! Vanilla builds a `java.nio` file system provider (`CopyOnWriteFileSystem`) so the fixes can use `Files` on a
//! layered view. The 8 fixes only list, test, create, delete, move, read and write, so the view here is a plain tree
//! of nodes that owns that vocabulary: a directory (children by name) or a file that is either an untouched original
//! (`storage` = the real file in the world) or a copy (`storage` = a `tmp_N` file in the temporary directory,
//! created on the first write). Nothing in the world folder is touched until the caller moves files out of the
//! [`Moves`] [`CowTree::collect_move_operations`] returns. Paths are relative to the tree root; `.` and `..` are
//! resolved like `Path.normalize` (`..` stops at the root).

use std::fmt;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use mc_core::java_collections::JavaHashMap;
use mc_core::java_exception::{FILE_ALREADY_EXISTS_EXCEPTION, JavaException};
use mc_core::math::java::java_string_hash;

use super::error::FileMove;
use super::ops::shown;
use crate::level_storage::safe_move_file;

/// The `CowFS*Exception` classes: the message is the Java message; carried as the payload of the [`io::Error`]
/// (`err.get_ref().and_then(|e| e.downcast_ref::<CowFsError>())`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CowFsError {
    /// `CowFSCreationException`.
    Creation(String),
    /// `CowFSSymlinkException`.
    Symlink(String),
    /// `CowFSNoSuchFileException`.
    NoSuchFile(String),
    /// `CowFSNotDirectoryException`.
    NotDirectory(String),
    /// `CowFSFileAlreadyExistsException`.
    FileAlreadyExists(String),
    /// `CowFSDirectoryNotEmptyException`.
    DirectoryNotEmpty(String),
    /// `CowFSFileSystemException`.
    FileSystem(String),
}

impl CowFsError {
    fn message(&self) -> &str {
        match self {
            CowFsError::Creation(m)
            | CowFsError::Symlink(m)
            | CowFsError::NoSuchFile(m)
            | CowFsError::NotDirectory(m)
            | CowFsError::FileAlreadyExists(m)
            | CowFsError::DirectoryNotEmpty(m)
            | CowFsError::FileSystem(m) => m,
        }
    }

    /// The fully qualified Java class name.
    fn class_name(&self) -> &'static str {
        match self {
            CowFsError::Creation(_) => {
                "net.minecraft.util.filefix.virtualfilesystem.exception.CowFSCreationException"
            }
            CowFsError::Symlink(_) => {
                "net.minecraft.util.filefix.virtualfilesystem.exception.CowFSSymlinkException"
            }
            CowFsError::NoSuchFile(_) => {
                "net.minecraft.util.filefix.virtualfilesystem.exception.CowFSNoSuchFileException"
            }
            CowFsError::NotDirectory(_) => {
                "net.minecraft.util.filefix.virtualfilesystem.exception.CowFSNotDirectoryException"
            }
            CowFsError::FileAlreadyExists(_) => {
                "net.minecraft.util.filefix.virtualfilesystem.exception.CowFSFileAlreadyExistsException"
            }
            CowFsError::DirectoryNotEmpty(_) => {
                "net.minecraft.util.filefix.virtualfilesystem.exception.CowFSDirectoryNotEmptyException"
            }
            CowFsError::FileSystem(_) => {
                "net.minecraft.util.filefix.virtualfilesystem.exception.CowFSFileSystemException"
            }
        }
    }

    fn kind(&self) -> io::ErrorKind {
        match self {
            CowFsError::NoSuchFile(_) => io::ErrorKind::NotFound,
            CowFsError::NotDirectory(_) => io::ErrorKind::NotADirectory,
            CowFsError::FileAlreadyExists(_) => io::ErrorKind::AlreadyExists,
            CowFsError::DirectoryNotEmpty(_) => io::ErrorKind::DirectoryNotEmpty,
            CowFsError::Creation(_) | CowFsError::Symlink(_) | CowFsError::FileSystem(_) => {
                io::ErrorKind::Other
            }
        }
    }
}

impl fmt::Display for CowFsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

impl std::error::Error for CowFsError {}

impl From<CowFsError> for io::Error {
    fn from(e: CowFsError) -> io::Error {
        let kind = e.kind();
        JavaException::new(e.class_name(), e.message()).into_io(kind)
    }
}

/// `DirectoryNode.childNodes`, a `HashMap<String, Node>`: its iteration order is the order the fixes list a directory
/// in and the order the files are moved into the new world, which sets the order the world is listed in afterwards.
type Children = JavaHashMap<String, Node>;

fn new_children() -> Children {
    JavaHashMap::new(|name| java_string_hash(name))
}

enum Node {
    Dir(Children),
    File { storage: PathBuf, is_copy: bool },
}

/// `CopyOnWriteFileSystem.Moves`: where the tree's content goes under the output directory, directories parents
/// first.
#[derive(Debug, Default)]
pub struct Moves {
    pub directories: Vec<PathBuf>,
    /// Files a fix wrote: their `storage` is a temporary file.
    pub copied_files: Vec<FileMove>,
    /// Files no fix touched: their `storage` is the original in the world.
    pub preexisting_files: Vec<FileMove>,
}

pub struct CowTree {
    tmp_directory: PathBuf,
    root: Children,
    tmp_file_index: u32,
}

/// `Files.isWritable`.
fn is_writable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        std::ffi::CString::new(path.as_os_str().as_bytes()).is_ok_and(|c| {
            // SAFETY: `c` is a valid NUL-terminated path for the duration of the call.
            unsafe { libc::access(c.as_ptr(), libc::W_OK) == 0 }
        })
    }
    #[cfg(not(unix))]
    {
        fs::metadata(path).is_ok_and(|m| !m.permissions().readonly())
    }
}

/// `checkAttributes` of the tree builder: only files and directories, all writable.
fn check_attributes(real: &Path, file_type: fs::FileType) -> io::Result<()> {
    if !file_type.is_file() && !file_type.is_dir() {
        return Err(CowFsError::Symlink(format!(
            "Cannot build copy-on-write file system when symlink is present: {}",
            real.display()
        ))
        .into());
    }
    if !is_writable(real) {
        return Err(CowFsError::Creation(format!(
            "Cannot build copy-on-write file system, missing write access for file: {}",
            real.display()
        ))
        .into());
    }
    Ok(())
}

fn build(real_dir: &Path, skipped: &dyn Fn(&Path) -> bool) -> io::Result<Children> {
    let mut children = new_children();
    for entry in fs::read_dir(real_dir)? {
        let real = entry?.path();
        let file_type = fs::symlink_metadata(&real)?.file_type();
        check_attributes(&real, file_type)?;
        if skipped(&real) {
            continue;
        }
        let name = real
            .file_name()
            .expect("a directory entry has a name")
            .to_string_lossy()
            .into_owned();
        let node = if file_type.is_dir() {
            Node::Dir(build(&real, skipped)?)
        } else {
            Node::File {
                storage: real,
                is_copy: false,
            }
        };
        children.insert(name, node);
    }
    Ok(children)
}

/// The names of `path` below the root.
fn names(path: &Path) -> Vec<String> {
    let mut names = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(name) => names.push(name.to_string_lossy().into_owned()),
            Component::ParentDir => {
                names.pop();
            }
            Component::CurDir | Component::RootDir | Component::Prefix(_) => {}
        }
    }
    names
}

fn dir_of<'a>(root: &'a Children, names: &[String]) -> Option<&'a Children> {
    let mut dir = root;
    for name in names {
        match dir.get(name)? {
            Node::Dir(children) => dir = children,
            Node::File { .. } => return None,
        }
    }
    Some(dir)
}

impl CowTree {
    /// `CopyOnWriteFileSystem.create`: the tree of the files below `base_directory` (what `skipped` accepts is left
    /// out), with `tmp_directory` created for the files a fix writes.
    ///
    /// # Errors
    /// A [`CowFsError`] for an existing `tmp_directory`, a symlink or other special file, or a file without write
    /// access; otherwise the I/O error that stopped the walk.
    pub fn create(
        base_directory: &Path,
        tmp_directory: &Path,
        skipped: impl Fn(&Path) -> bool,
    ) -> io::Result<CowTree> {
        if tmp_directory.exists() {
            return Err(CowFsError::Creation(format!(
                "Temporary directory already exists: {}",
                tmp_directory.display()
            ))
            .into());
        }
        let base_type = fs::symlink_metadata(base_directory)?.file_type();
        check_attributes(base_directory, base_type)?;
        let tree = CowTree {
            tmp_directory: tmp_directory.to_path_buf(),
            root: build(base_directory, &skipped)?,
            tmp_file_index: 0,
        };
        fs::create_dir(tmp_directory)?;
        Ok(tree)
    }

    /// `close`: deletes the temporary directory.
    pub fn close(self) -> io::Result<()> {
        if self.tmp_directory.exists() {
            fs::remove_dir_all(&self.tmp_directory)?;
        }
        Ok(())
    }

    pub fn tmp_directory(&self) -> &Path {
        &self.tmp_directory
    }

    /// `createTemporaryFilePath`: the next unused `tmp_N` of the temporary directory.
    pub fn create_temporary_file_path(&mut self) -> PathBuf {
        self.tmp_file_index += 1;
        self.tmp_directory
            .join(format!("tmp_{}", self.tmp_file_index))
    }

    fn node(&self, names: &[String]) -> Option<&Node> {
        let (name, parents) = names.split_last()?;
        dir_of(&self.root, parents)?.get(name)
    }

    /// The directory at `names`: a missing path is `NoSuchFile`, a file `NotDirectory`.
    fn dir_mut(&mut self, names: &[String], shown_path: &Path) -> io::Result<&mut Children> {
        let mut dir = &mut self.root;
        for name in names {
            match dir.get_mut(name) {
                Some(Node::Dir(children)) => dir = children,
                Some(Node::File { .. }) => {
                    return Err(CowFsError::NotDirectory(format!(
                        "{} was a file, expected directory",
                        shown(shown_path)
                    ))
                    .into());
                }
                None => return Err(CowFsError::NoSuchFile(shown(shown_path)).into()),
            }
        }
        Ok(dir)
    }

    pub fn exists(&self, path: &Path) -> bool {
        let names = names(path);
        names.is_empty() || self.node(&names).is_some()
    }

    pub fn is_dir(&self, path: &Path) -> bool {
        let names = names(path);
        names.is_empty() || matches!(self.node(&names), Some(Node::Dir(_)))
    }

    pub fn is_file(&self, path: &Path) -> bool {
        matches!(self.node(&names(path)), Some(Node::File { .. }))
    }

    /// `Files.list`: the names of the directory's children in `HashMap` order.
    pub fn list(&self, path: &Path) -> io::Result<Vec<String>> {
        let names = names(path);
        if names.is_empty() {
            return Ok(self.root.keys().cloned().collect());
        }
        match self.node(&names) {
            Some(Node::Dir(children)) => Ok(children.keys().cloned().collect()),
            Some(Node::File { .. }) => Err(CowFsError::NotDirectory(format!(
                "{} was a file, expected directory",
                shown(path)
            ))
            .into()),
            None => Err(CowFsError::NoSuchFile(shown(path)).into()),
        }
    }

    /// `Files.createDirectory`.
    pub fn create_dir(&mut self, path: &Path) -> io::Result<()> {
        let mut names = names(path);
        let Some(name) = names.pop() else {
            return Err(CowFsError::FileAlreadyExists(shown(path)).into());
        };
        let parent = path.parent().unwrap_or(Path::new(""));
        let dir = self.dir_mut(&names, parent)?;
        if dir.contains_key(&name) {
            return Err(CowFsError::FileAlreadyExists(shown(path)).into());
        }
        dir.insert(name, Node::Dir(new_children()));
        Ok(())
    }

    /// `Files.createDirectories`: missing directories are created, an existing file in the way is an error.
    pub fn create_dir_all(&mut self, path: &Path) -> io::Result<()> {
        let names = names(path);
        let mut dir = &mut self.root;
        for name in &names {
            if !dir.contains_key(name) {
                dir.insert(name.clone(), Node::Dir(new_children()));
            }
            match dir.get_mut(name).expect("inserted above") {
                Node::Dir(children) => dir = children,
                Node::File { .. } => {
                    return Err(CowFsError::FileAlreadyExists(shown(path)).into());
                }
            }
        }
        Ok(())
    }

    /// `Files.delete`: a file, or a directory that is empty.
    pub fn delete(&mut self, path: &Path) -> io::Result<()> {
        let mut names = names(path);
        let Some(name) = names.pop() else {
            return Err(CowFsError::FileSystem("Can't remove root".to_owned()).into());
        };
        let parent = path.parent().unwrap_or(Path::new(""));
        let dir = self.dir_mut(&names, parent).map_err(|e| {
            // A path below a missing or non-directory parent does not exist.
            if e.kind() == io::ErrorKind::NotFound || e.kind() == io::ErrorKind::NotADirectory {
                CowFsError::NoSuchFile(shown(path)).into()
            } else {
                e
            }
        })?;
        match dir.get(&name) {
            None => return Err(CowFsError::NoSuchFile(shown(path)).into()),
            Some(Node::Dir(children)) if !children.is_empty() => {
                return Err(CowFsError::DirectoryNotEmpty(shown(path)).into());
            }
            Some(Node::File {
                storage,
                is_copy: true,
            }) => {
                if let Err(e) = fs::remove_file(storage)
                    && e.kind() != io::ErrorKind::NotFound
                {
                    return Err(e);
                }
            }
            Some(_) => {}
        }
        dir.remove(&name);
        Ok(())
    }

    /// `Files.deleteIfExists`.
    pub fn delete_if_exists(&mut self, path: &Path) -> io::Result<bool> {
        if !self.exists(path) {
            return Ok(false);
        }
        self.delete(path)?;
        Ok(true)
    }

    /// `Files.move(from, to)` without options: the whole subtree moves; an existing target is an error.
    pub fn rename(&mut self, from: &Path, to: &Path) -> io::Result<()> {
        let from_names = names(from);
        if from_names.is_empty() {
            return Err(CowFsError::FileSystem(format!(
                "{}: can't move root directory",
                shown(from)
            ))
            .into());
        }
        if self.node(&from_names).is_none() {
            return Err(CowFsError::NoSuchFile(shown(from)).into());
        }
        let mut to_names = names(to);
        let Some(to_name) = to_names.pop() else {
            return Err(CowFsError::FileAlreadyExists(shown(to)).into());
        };
        if to_names == from_names[..from_names.len() - 1]
            && to_name == from_names[from_names.len() - 1]
        {
            return Ok(());
        }
        if to_names.starts_with(&from_names) {
            return Err(CowFsError::FileSystem(format!(
                "{}: can't move a directory into itself",
                shown(from)
            ))
            .into());
        }
        let target =
            dir_of(&self.root, &to_names).ok_or_else(|| CowFsError::NoSuchFile(shown(to)))?;
        if target.contains_key(&to_name) {
            return Err(CowFsError::FileAlreadyExists(shown(to)).into());
        }
        let (from_name, from_parents) = from_names.split_last().expect("not the root");
        let node = self
            .dir_mut(from_parents, from)?
            .remove(from_name)
            .expect("checked above");
        self.dir_mut(&to_names, to)?.insert(to_name, node);
        Ok(())
    }

    /// The real file behind `path` to read from (a copy once a fix wrote it, else the original).
    pub fn file_for_read(&self, path: &Path) -> io::Result<&Path> {
        match self.node(&names(path)) {
            Some(Node::File { storage, .. }) => Ok(storage),
            Some(Node::Dir(_)) => {
                Err(CowFsError::FileSystem(format!("{}: not a regular file", shown(path))).into())
            }
            None if names(path).is_empty() => {
                Err(CowFsError::FileSystem(format!("{}: not a regular file", shown(path))).into())
            }
            None => Err(CowFsError::NoSuchFile(shown(path)).into()),
        }
    }

    /// The real file behind `path` to write to: the first call copies an original into the temporary directory
    /// (`FileNode.ensureCopy`); `create` makes an empty file where there is none (`StandardOpenOption.CREATE`).
    pub fn file_for_write(&mut self, path: &Path, create: bool) -> io::Result<PathBuf> {
        enum Found {
            Missing,
            Original(PathBuf),
            Copy(PathBuf),
            Directory,
        }
        let names = names(path);
        let found = match self.node(&names) {
            None if names.is_empty() => Found::Directory,
            None => Found::Missing,
            Some(Node::File {
                storage,
                is_copy: false,
            }) => Found::Original(storage.clone()),
            Some(Node::File { storage, .. }) => Found::Copy(storage.clone()),
            Some(Node::Dir(_)) => Found::Directory,
        };
        match found {
            Found::Directory => {
                Err(CowFsError::FileSystem(format!("{}: not a regular file", shown(path))).into())
            }
            Found::Copy(storage) => Ok(storage),
            Found::Missing => {
                if !create {
                    return Err(CowFsError::NoSuchFile(shown(path)).into());
                }
                let (name, parents) = names.split_last().expect("not the root");
                let parent = path.parent().unwrap_or(Path::new(""));
                self.dir_mut(parents, parent)?;
                let tmp = self.create_temporary_file_path();
                fs::File::create(&tmp)?;
                self.dir_mut(parents, parent)?.insert(
                    name.clone(),
                    Node::File {
                        storage: tmp.clone(),
                        is_copy: true,
                    },
                );
                Ok(tmp)
            }
            Found::Original(original) => {
                let tmp = self.create_temporary_file_path();
                fs::copy(&original, &tmp)?;
                let (name, parents) = names.split_last().expect("not the root");
                let parent = path.parent().unwrap_or(Path::new(""));
                self.dir_mut(parents, parent)?.insert(
                    name.clone(),
                    Node::File {
                        storage: tmp.clone(),
                        is_copy: true,
                    },
                );
                Ok(tmp)
            }
        }
    }

    /// Makes `path` a file stored in `real_file` (which becomes the tree's to delete), replacing what was there.
    pub fn adopt_file(&mut self, path: &Path, real_file: PathBuf) -> io::Result<()> {
        let mut names = names(path);
        let Some(name) = names.pop() else {
            return Err(
                CowFsError::FileSystem(format!("{}: not a regular file", shown(path))).into(),
            );
        };
        let parent = path.parent().unwrap_or(Path::new(""));
        let dir = self.dir_mut(&names, parent)?;
        match dir.get(&name) {
            Some(Node::Dir(_)) => {
                return Err(
                    CowFsError::FileSystem(format!("{}: not a regular file", shown(path))).into(),
                );
            }
            Some(Node::File {
                storage,
                is_copy: true,
            }) => {
                let _ = fs::remove_file(storage);
            }
            _ => {}
        }
        dir.insert(
            name,
            Node::File {
                storage: real_file,
                is_copy: true,
            },
        );
        Ok(())
    }

    /// `collectMoveOperations(outPath)`.
    pub fn collect_move_operations(&self, out_path: &Path) -> Moves {
        fn collect(out: &Path, folder: &Children, result: &mut Moves) {
            for (name, node) in folder {
                let target = out.join(name);
                match node {
                    Node::File { storage, is_copy } => {
                        let file_move = FileMove {
                            from: storage.clone(),
                            to: target,
                        };
                        if *is_copy {
                            result.copied_files.push(file_move);
                        } else {
                            result.preexisting_files.push(file_move);
                        }
                    }
                    Node::Dir(children) => {
                        result.directories.push(target.clone());
                        collect(&target, children, result);
                    }
                }
            }
        }
        let mut result = Moves::default();
        collect(out_path, &self.root, &mut result);
        result
    }
}

/// `CopyOnWriteFileSystem.createDirectories`.
pub fn create_directories(directories: &[PathBuf]) -> io::Result<()> {
    directories
        .iter()
        .try_for_each(|d| fs::create_dir(d).map_err(|e| JavaException::from_file_error(e, d)))
}

/// `CopyOnWriteFileSystem.hardLinkFiles`: links the originals into the new world, leaving what is there.
pub fn hard_link_files(moves: &[FileMove]) -> io::Result<()> {
    for m in moves {
        if !m.to.exists() {
            if !m.from.is_file() {
                return Err(io::Error::other(format!(
                    "Not a regular file: {}",
                    m.from.display()
                )));
            }
            // `Files.createLink(to, from)`
            fs::hard_link(&m.from, &m.to)
                .map_err(|e| JavaException::from_file_pair_error(e, &m.to, Some(&m.from)))?;
        }
    }
    Ok(())
}

/// `Files.move(from, to)`: moves a file, failing when the target exists.
pub(super) fn move_new(from: &Path, to: &Path) -> io::Result<()> {
    if fs::symlink_metadata(to).is_ok() {
        return Err(
            JavaException::new(FILE_ALREADY_EXISTS_EXCEPTION, to.display().to_string())
                .into_io(io::ErrorKind::AlreadyExists),
        );
    }
    fs::rename(from, to).map_err(|e| JavaException::from_file_pair_error(e, from, Some(to)))
}

/// `CopyOnWriteFileSystem.moveFiles`.
pub fn move_files(moves: &[FileMove]) -> io::Result<()> {
    moves.iter().try_for_each(|m| move_new(&m.from, &m.to))
}

/// `CopyOnWriteFileSystem.moveFilesWithRetry`: a move whose source is gone and target is there was done by an
/// earlier run and is skipped. `before_each(index)` runs before each move that is not skipped.
pub fn move_files_with_retry(
    moves: &[FileMove],
    before_each: &dyn Fn(usize) -> io::Result<()>,
) -> io::Result<()> {
    for (index, m) in moves.iter().enumerate() {
        if m.from.exists() || !m.to.exists() {
            before_each(index)?;
            if !m.from.is_file() {
                return Err(io::Error::other(format!(
                    "Not a regular file: {}",
                    m.from.display()
                )));
            }
            fs::rename(&m.from, &m.to)
                .map_err(|e| JavaException::from_file_pair_error(e, &m.from, Some(&m.to)))?;
        }
    }
    Ok(())
}

/// `CopyOnWriteFileSystem.tryRevertMoves`: moves every moved file back, returning the moves that could not be
/// reverted.
pub fn try_revert_moves(moves: &[FileMove]) -> Vec<FileMove> {
    let mut failed = Vec::new();
    for m in moves {
        if m.to.exists() || !m.from.exists() {
            if m.to.is_file() {
                if safe_move_file(&m.to, &m.from) {
                    log::info!(
                        "Reverted move from {} to {}",
                        m.from.display(),
                        m.to.display()
                    );
                } else {
                    log::error!(
                        "Failed to revert move from {} to {}",
                        m.from.display(),
                        m.to.display()
                    );
                    failed.push(m.clone());
                }
            } else {
                log::error!(
                    "Skipping reverting move from {} to {} as it's not a file",
                    m.from.display(),
                    m.to.display()
                );
                failed.push(m.clone());
            }
        }
    }
    if failed.is_empty() {
        log::info!("Successfully reverted back to previous world state");
    } else {
        log::error!("Completed reverting with errors");
    }
    failed
}
