//! File helpers shared by the storage code: `FileUtil.createDirectoriesSafe`, `Util.safeReplaceFile`
//! (+ `safeReplaceOrMoveFile` / `safeMoveFile` with their retry loops), `Files.createTempFile`,
//! `NbtIo.{read,write}Compressed(Path)` and `FileNameDateFormatter`.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use mc_nbt::{CompoundTag, NbtAccounter, NbtError};

/// `FileUtil.createDirectoriesSafe`: `Files.createDirectories(exists ? toRealPath() : dir)`.
pub fn create_directories_safe(dir: &Path) -> io::Result<()> {
    if dir.exists() {
        fs::create_dir_all(dir.canonicalize()?)
    } else {
        fs::create_dir_all(dir)
    }
}

/// Maps an NBT failure onto the `IOException` family (the unchecked vanilla variants stay distinguishable
/// through the message; callers that vanilla wraps in `catch (NbtException | ReportedNbtException | IOException)`
/// treat every kind alike).
pub fn nbt_error_to_io(e: NbtError) -> io::Error {
    match e {
        NbtError::Eof => io::Error::new(io::ErrorKind::UnexpectedEof, e),
        other => io::Error::new(io::ErrorKind::InvalidData, other),
    }
}

/// `NbtIo.readCompressed(Path, NbtAccounter)`.
pub fn read_compressed(path: &Path, acc: &mut NbtAccounter) -> io::Result<CompoundTag> {
    let mut data = Vec::new();
    File::open(path)?.read_to_end(&mut data)?;
    mc_nbt::compress::read_compressed(&data, acc).map_err(nbt_error_to_io)
}

/// `NbtIo.writeCompressed(CompoundTag, Path)`: `CREATE, TRUNCATE_EXISTING, WRITE, SYNC`.
pub fn write_compressed(tag: &CompoundTag, path: &Path) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)?;
    file.write_all(&mc_nbt::compress::write_compressed(tag))?;
    file.sync_all()
}

/// `Files.createTempFile(dir, prefix, suffix)`: `prefix + unsigned-long + suffix`, created exclusively
/// (mode `rw-------` on POSIX like the JDK's default temp-file attributes).
pub fn create_temp_file(dir: &Path, prefix: &str, suffix: &str) -> io::Result<PathBuf> {
    use std::hash::{BuildHasher, Hasher};
    loop {
        // RandomState is seeded from the OS per instance; plenty for a collision-avoiding name.
        let n = std::collections::hash_map::RandomState::new()
            .build_hasher()
            .finish();
        let path = dir.join(format!("{prefix}{n}{suffix}"));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        match options.open(&path) {
            Ok(_) => return Ok(path),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
}

/// `Files.move(from, to)` without options: fails when the target exists.
fn move_no_replace(from: &Path, to: &Path) -> io::Result<()> {
    if fs::symlink_metadata(to).is_ok() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            to.display().to_string(),
        ));
    }
    fs::rename(from, to)
}

type Operation<'a> = (String, Box<dyn Fn() -> bool + 'a>);

fn renamer<'a>(from: &'a Path, to: &'a Path) -> Operation<'a> {
    (
        format!("rename {} to {}", from.display(), to.display()),
        Box::new(move || match move_no_replace(from, to) {
            Ok(()) => true,
            Err(e) => {
                log::error!("Failed to rename: {e}");
                false
            }
        }),
    )
}

fn deleter(target: &Path) -> Operation<'_> {
    (
        format!("delete old {}", target.display()),
        Box::new(move || match fs::remove_file(target) {
            Ok(()) => true,
            Err(e) if e.kind() == io::ErrorKind::NotFound => true,
            Err(e) => {
                log::warn!("Failed to delete: {e}");
                false
            }
        }),
    )
}

fn file_deleted_check(target: &Path) -> Operation<'_> {
    (
        format!("verify that {} is deleted", target.display()),
        Box::new(move || !target.exists()),
    )
}

fn file_created_check(target: &Path) -> Operation<'_> {
    (
        format!("verify that {} is present", target.display()),
        Box::new(move || target.is_file()),
    )
}

fn execute_in_sequence(operations: &[Operation<'_>]) -> bool {
    for (description, operation) in operations {
        if !operation() {
            log::warn!("Failed to execute {description}");
            return false;
        }
    }
    true
}

fn run_with_retries(
    number_of_retries: i32,
    description: &str,
    operations: &[Operation<'_>],
) -> bool {
    for retry in 0..number_of_retries {
        if execute_in_sequence(operations) {
            return true;
        }
        log::error!("Failed to {description}, retrying {retry}/{number_of_retries}");
    }
    log::error!("Failed to {description}, aborting, progress might be lost");
    false
}

/// `Util.safeMoveFile` (no copy options).
pub fn safe_move_file(from: &Path, to: &Path) -> bool {
    run_with_retries(
        10,
        &format!("move from  {} to {}", from.display(), to.display()),
        &[renamer(from, to), file_created_check(to)],
    )
}

/// `Util.safeReplaceFile`: `target` → `backup`, then `new` → `target` (rolled back when the last step fails).
pub fn safe_replace_file(target: &Path, new: &Path, backup: &Path) {
    safe_replace_or_move_file(target, new, backup, false);
}

/// `Util.safeReplaceOrMoveFile`.
pub fn safe_replace_or_move_file(
    target: &Path,
    new: &Path,
    backup: &Path,
    no_rollback: bool,
) -> bool {
    if target.exists()
        && !run_with_retries(
            10,
            &format!("create backup {}", backup.display()),
            &[
                deleter(backup),
                renamer(target, backup),
                file_created_check(backup),
            ],
        )
    {
        return false;
    }
    if !run_with_retries(
        10,
        &format!("remove old {}", target.display()),
        &[deleter(target), file_deleted_check(target)],
    ) {
        return false;
    }
    if !run_with_retries(
        10,
        &format!("replace {} with {}", target.display(), new.display()),
        &[renamer(new, target), file_created_check(target)],
    ) && !no_rollback
    {
        run_with_retries(
            10,
            &format!("restore {} from {}", target.display(), backup.display()),
            &[renamer(backup, target), file_created_check(target)],
        );
        return false;
    }
    true
}

/// `FileNameDateFormatter.FORMATTER`: `yyyy-MM-dd_HH-mm-ss` (year padded to 4 digits, a `+` sign when it
/// needs more than 4, `-` when negative).
pub fn format_file_name_date(
    year: i64,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
) -> String {
    let year = match year {
        y if y < 0 => format!("-{:04}", -y),
        y if y > 9999 => format!("+{y}"),
        y => format!("{y:04}"),
    };
    format!("{year}-{month:02}-{day:02}_{hour:02}-{minute:02}-{second:02}")
}

/// `ZonedDateTime.now().format(FileNameDateFormatter.FORMATTER)` in the system time zone.
pub fn format_file_name_date_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    format_file_name_date_at(secs)
}

/// [`format_file_name_date_now`] for an explicit epoch second.
pub fn format_file_name_date_at(epoch_secs: i64) -> String {
    #[cfg(unix)]
    {
        // SAFETY: `tm` is plain data; localtime_r only writes into it.
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        let t = epoch_secs as libc::time_t;
        if !unsafe { libc::localtime_r(&t, &mut tm) }.is_null() {
            return format_file_name_date(
                tm.tm_year as i64 + 1900,
                (tm.tm_mon + 1) as u32,
                tm.tm_mday as u32,
                tm.tm_hour as u32,
                tm.tm_min as u32,
                tm.tm_sec as u32,
            );
        }
    }
    let (y, m, d) = civil_from_days(epoch_secs.div_euclid(86_400));
    let rem = epoch_secs.rem_euclid(86_400) as u32;
    format_file_name_date(y, m, d, rem / 3600, rem / 60 % 60, rem % 60)
}

/// Days since 1970-01-01 to proleptic Gregorian (Howard Hinnant's algorithm).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_format() {
        assert_eq!(
            format_file_name_date(2026, 11, 4, 13, 8, 9),
            "2026-11-04_13-08-09"
        );
        assert_eq!(
            format_file_name_date(12345, 1, 2, 3, 4, 5),
            "+12345-01-02_03-04-05"
        );
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_396), (2025, 11, 4));
    }

    #[test]
    fn replace_dance_keeps_backup() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("level.dat");
        let backup = dir.path().join("level.dat_old");
        fs::write(&target, b"v1").unwrap();
        let tmp = create_temp_file(dir.path(), "level", ".dat").unwrap();
        fs::write(&tmp, b"v2").unwrap();
        safe_replace_file(&target, &tmp, &backup);
        assert_eq!(fs::read(&target).unwrap(), b"v2");
        assert_eq!(fs::read(&backup).unwrap(), b"v1");
        assert!(!tmp.exists());
        // second round replaces an existing backup
        let tmp = create_temp_file(dir.path(), "level", ".dat").unwrap();
        fs::write(&tmp, b"v3").unwrap();
        safe_replace_file(&target, &tmp, &backup);
        assert_eq!(fs::read(&target).unwrap(), b"v3");
        assert_eq!(fs::read(&backup).unwrap(), b"v2");
    }
}
