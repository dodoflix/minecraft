//! The `java.nio.file.Files` calls of the file fixer, each failing with the exception the JDK throws for it: its class
//! and message name the path (`UnixException.translateToIOException`), not the OS error text alone.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::Path;

use mc_core::java_exception::{
    DIRECTORY_NOT_EMPTY_EXCEPTION, FILE_ALREADY_EXISTS_EXCEPTION, JavaException,
};

/// `java.nio.file.NotDirectoryException`.
const NOT_DIRECTORY_EXCEPTION: &str = "java.nio.file.NotDirectoryException";

use crate::level_storage::read_all_bytes;

fn failed(e: io::Error, path: &Path) -> io::Error {
    JavaException::from_file_error(e, path)
}

/// `Files.createDirectory(dir)`.
pub(super) fn create_directory(dir: &Path) -> io::Result<()> {
    fs::create_dir(dir).map_err(|e| failed(e, dir))
}

/// `Files.createFile(path)`.
pub(super) fn create_file(path: &Path) -> io::Result<()> {
    File::create_new(path)
        .map(drop)
        .map_err(|e| failed(e, path))
}

/// `Files.newDirectoryStream(dir)`: the entries of `dir`.
pub(super) fn list(dir: &Path) -> io::Result<Vec<fs::DirEntry>> {
    fs::read_dir(dir)
        .map_err(|e| {
            if e.raw_os_error() == Some(20) {
                JavaException::new(NOT_DIRECTORY_EXCEPTION, dir.display().to_string())
                    .into_io(e.kind())
            } else {
                failed(e, dir)
            }
        })?
        .map(|entry| entry.map_err(|e| JavaException::read_failure(&e).into_io(e.kind())))
        .collect()
}

/// `Files.readString(path)`: a file that is not UTF-8 is a `MalformedInputException`.
pub(super) fn read_string(path: &Path) -> io::Result<String> {
    String::from_utf8(read_all_bytes(path)?)
        .map_err(|e| JavaException::malformed_string_input(&e).into_io(io::ErrorKind::InvalidData))
}

/// `Files.deleteIfExists(path)`: an empty directory is removed like a file, a directory that is not empty is a
/// `DirectoryNotEmptyException`.
pub(super) fn delete_if_exists(path: &Path) -> io::Result<bool> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(failed(e, path)),
    };
    let result = if metadata.is_dir() {
        fs::remove_dir(path)
    } else {
        fs::remove_file(path)
    };
    match result {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) if metadata.is_dir() && e.kind() == io::ErrorKind::DirectoryNotEmpty => Err(
            JavaException::new(DIRECTORY_NOT_EMPTY_EXCEPTION, path.display().to_string())
                .into_io(e.kind()),
        ),
        Err(e) => Err(failed(e, path)),
    }
}

/// `PathUtils.deleteDirectory(dir)`: everything below `dir` and `dir` itself, files first.
pub(super) fn delete_tree(path: &Path) -> io::Result<()> {
    if fs::symlink_metadata(path)
        .map_err(|e| failed(e, path))?
        .is_dir()
    {
        for entry in list(path)? {
            delete_tree(&entry.path())?;
        }
    }
    delete_if_exists(path).map(drop)
}

/// `Files.copy(source, target, COPY_ATTRIBUTES)` of a regular file to a name that is free. A failure to stat, open or
/// create names the file it failed on; one while the data is transferred is a plain `IOException(strerror)` without a
/// path, and one while the attributes are copied names `source -> target`.
pub(super) fn copy_file(source: &Path, target: &Path) -> io::Result<()> {
    let metadata = fs::metadata(source).map_err(|e| failed(e, source))?;
    if fs::symlink_metadata(target).is_ok() {
        return Err(JavaException::new(
            FILE_ALREADY_EXISTS_EXCEPTION,
            target.display().to_string(),
        )
        .into_io(io::ErrorKind::AlreadyExists));
    }
    let mut input = File::open(source).map_err(|e| failed(e, source))?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(
        &mut options,
        std::os::unix::fs::PermissionsExt::mode(&metadata.permissions()),
    );
    let mut output = options.open(target).map_err(|e| failed(e, target))?;
    // the data goes through a channel transfer, whose failure is a plain `IOException(strerror)` without a path
    let result = io::copy(&mut input, &mut output)
        .map_err(|e| JavaException::read_failure(&e).into_io(e.kind()))
        // the mode is set exactly (the umask applies to the creation) and the times are copied, `COPY_ATTRIBUTES`
        .and_then(|_| {
            output
                .set_permissions(metadata.permissions())
                .and_then(|()| {
                    let mut times = fs::FileTimes::new();
                    if let Ok(accessed) = metadata.accessed() {
                        times = times.set_accessed(accessed);
                    }
                    if let Ok(modified) = metadata.modified() {
                        times = times.set_modified(modified);
                    }
                    output.set_times(times)
                })
                .map_err(|e| JavaException::from_file_pair_error(e, source, Some(target)))
        });
    result.inspect_err(|_| {
        let _ = fs::remove_file(target);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(result: io::Result<impl std::fmt::Debug>) -> String {
        JavaException::of_io(&result.unwrap_err()).to_string()
    }

    #[test]
    fn failures_print_as_the_jdk_throws_them() {
        let dir = tempfile::tempdir().unwrap();
        let (missing, file, full) = (
            dir.path().join("missing"),
            dir.path().join("file"),
            dir.path().join("full"),
        );
        fs::write(&file, b"x").unwrap();
        fs::create_dir(&full).unwrap();
        fs::write(full.join("inner"), b"y").unwrap();
        assert!(!delete_if_exists(&missing).unwrap());
        assert_eq!(
            text(delete_if_exists(&full)),
            format!(
                "java.nio.file.DirectoryNotEmptyException: {}",
                full.display()
            )
        );
        assert_eq!(
            text(create_directory(&file)),
            format!(
                "java.nio.file.FileAlreadyExistsException: {}",
                file.display()
            )
        );
        assert_eq!(
            text(copy_file(&missing, &file)),
            format!("java.nio.file.NoSuchFileException: {}", missing.display())
        );
        let other = dir.path().join("other");
        fs::write(&other, b"z").unwrap();
        assert_eq!(
            text(copy_file(&file, &other)),
            format!(
                "java.nio.file.FileAlreadyExistsException: {}",
                other.display()
            )
        );
        assert_eq!(
            text(list(&file)),
            format!("java.nio.file.NotDirectoryException: {}", file.display())
        );
        delete_tree(&full).unwrap();
        assert!(!full.exists());
    }
}
