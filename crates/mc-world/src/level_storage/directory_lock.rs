//! `net.minecraft.util.DirectoryLock`: the exclusive `session.lock` of a world directory.
//!
//! Like vanilla (26.3) the lock file is created if missing, the UTF-8 bytes of `☃` (U+2603) are written at
//! position 0 *before* locking (a failed lock still leaves them behind), the data is forced to disk, and then
//! the whole file is locked exclusively without blocking. On Linux the lock is an open-file-description
//! (`F_OFD_SETLK`) POSIX lock: it conflicts with the `fcntl` locks the JVM takes (so a vanilla server and this
//! server exclude each other) and, unlike plain `F_SETLK`, also conflicts between two handles in this process.

use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use super::file_util::create_directories_safe;

pub const LOCK_FILE: &str = "session.lock";

/// `"\u2603".getBytes(UTF_8)`.
const DUMMY: [u8; 3] = [0xE2, 0x98, 0x83];

/// `DirectoryLock.LockException`: carried as the payload of the returned [`io::Error`]
/// (`err.get_ref().and_then(|e| e.downcast_ref::<LockException>())`).
#[derive(Debug)]
pub struct LockException(String);

impl LockException {
    /// `LockException.alreadyLocked`.
    pub fn already_locked(path: &Path) -> Self {
        let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
        Self(format!(
            "{}: already locked (possibly by other Minecraft instance?)",
            absolute.display()
        ))
    }
}

impl fmt::Display for LockException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for LockException {}

/// Try to lock the whole file exclusively. `Ok(false)` when somebody else holds a conflicting lock.
fn try_lock(file: &File) -> io::Result<bool> {
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        // SAFETY: zeroed `flock` is a valid initial value; the fields below are the documented inputs.
        let mut fl: libc::flock = unsafe { std::mem::zeroed() };
        fl.l_type = libc::F_WRLCK as _;
        fl.l_whence = libc::SEEK_SET as _;
        fl.l_start = 0;
        fl.l_len = 0; // to EOF and beyond: the whole file, like `FileChannel.tryLock()`
        #[cfg(target_os = "linux")]
        let cmd = libc::F_OFD_SETLK;
        #[cfg(not(target_os = "linux"))]
        let cmd = libc::F_SETLK;
        // SAFETY: valid fd for the lifetime of `file`; `fl` outlives the call.
        let r = unsafe { libc::fcntl(file.as_raw_fd(), cmd, &fl as *const libc::flock) };
        if r == 0 {
            return Ok(true);
        }
        let err = io::Error::last_os_error();
        match err.raw_os_error() {
            Some(libc::EAGAIN) | Some(libc::EACCES) => Ok(false),
            _ => Err(err),
        }
    }
    #[cfg(not(unix))]
    {
        match file.try_lock() {
            Ok(()) => Ok(true),
            Err(std::fs::TryLockError::WouldBlock) => Ok(false),
            Err(std::fs::TryLockError::Error(e)) => Err(e),
        }
    }
}

pub struct DirectoryLock {
    /// `None` once released (`FileLock.isValid()` is then false).
    file: Option<File>,
}

impl DirectoryLock {
    /// `DirectoryLock.create`.
    pub fn create(dir: &Path) -> io::Result<DirectoryLock> {
        let lock_path: PathBuf = dir.join(LOCK_FILE);
        create_directories_safe(dir)?;
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&lock_path)?;
        // `FileChannel.write(buf)` at the channel position 0; leaves any longer existing content in place.
        file.write_all(&DUMMY)?;
        file.sync_all()?;
        if !try_lock(&file)? {
            return Err(io::Error::other(LockException::already_locked(&lock_path)));
        }
        Ok(DirectoryLock { file: Some(file) })
    }

    /// `DirectoryLock.isValid`.
    pub fn is_valid(&self) -> bool {
        self.file.is_some()
    }

    /// `DirectoryLock.close`: release the lock and close the channel (idempotent).
    pub fn close(&mut self) -> io::Result<()> {
        // Dropping the descriptor releases an OFD lock; on Windows `File` drop unlocks too.
        self.file = None;
        Ok(())
    }

    /// `DirectoryLock.isLocked`: whether somebody else holds `dir/session.lock`.
    pub fn is_locked(dir: &Path) -> io::Result<bool> {
        let lock_path = dir.join(LOCK_FILE);
        match OpenOptions::new().write(true).open(&lock_path) {
            Ok(file) => Ok(!try_lock(&file)?),
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied => Ok(true),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e),
        }
    }
}

impl Drop for DirectoryLock {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_acquire_fails_with_vanilla_message() {
        let dir = tempfile::tempdir().unwrap();
        let mut first = DirectoryLock::create(dir.path()).unwrap();
        assert!(first.is_valid());
        assert_eq!(std::fs::read(dir.path().join(LOCK_FILE)).unwrap(), DUMMY);
        assert!(DirectoryLock::is_locked(dir.path()).unwrap());

        let err = DirectoryLock::create(dir.path())
            .err()
            .expect("second lock must fail");
        let lock_err = err
            .get_ref()
            .and_then(|e| e.downcast_ref::<LockException>())
            .expect("LockException");
        assert_eq!(
            lock_err.to_string(),
            format!(
                "{}: already locked (possibly by other Minecraft instance?)",
                dir.path().join(LOCK_FILE).display()
            )
        );

        first.close().unwrap();
        assert!(!first.is_valid());
        first.close().unwrap();
        assert!(!DirectoryLock::is_locked(dir.path()).unwrap());
        let again = DirectoryLock::create(dir.path()).unwrap();
        assert!(again.is_valid());
    }

    #[test]
    fn is_locked_without_lock_file() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!DirectoryLock::is_locked(dir.path()).unwrap());
    }
}
