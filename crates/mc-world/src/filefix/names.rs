//! `FileUtil.findAvailableName` for the names of the folders the world swap uses (`mc-game-core` has the same port;
//! this crate cannot depend on it).

use std::io;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use super::jdk_files::{create_directory, delete_if_exists};

/// `SharedConstants.ILLEGAL_FILE_CHARACTERS`.
const ILLEGAL_FILE_CHARACTERS: [char; 15] = [
    '/', '\n', '\r', '\t', '\0', '\u{c}', '`', '?', '*', '\\', '<', '>', '|', '"', ':',
];

static COPY_COUNTER_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)^(?<name>.*) \((?<count>\d*)\)$").expect("static regex"));
static RESERVED_WINDOWS_FILENAMES: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)^(?:.*\.|(?:COM|CLOCK\$|CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])(?:\..*)?)$")
        .expect("static regex")
});

/// `FileUtil.sanitizeName`.
fn sanitize_name(base_name: &str) -> String {
    base_name
        .chars()
        .map(|c| {
            if ILLEGAL_FILE_CHARACTERS.contains(&c) || matches!(c, '.' | '"') {
                '_'
            } else {
                c
            }
        })
        .collect()
}

/// Whether `candidate` is a name [`find_available_name`] makes of `base_name` (`name` or `name (n)`, the name cut to
/// the 255 UTF-16 units of a file name).
pub(super) fn is_available_name_of(base_name: &str, candidate: &str) -> bool {
    let base: Vec<u16> = sanitize_name(base_name).encode_utf16().collect();
    let cut = |length: usize| String::from_utf16_lossy(&base[..base.len().min(length)]);
    if candidate == cut(255) {
        return true;
    }
    let Some((name, counter)) = candidate.rsplit_once(" (") else {
        return false;
    };
    let Some(digits) = counter.strip_suffix(')') else {
        return false;
    };
    !digits.is_empty()
        && digits.bytes().all(|b| b.is_ascii_digit())
        && 255usize
            .checked_sub(digits.len() + 3)
            .is_some_and(|length| name == cut(length))
}

/// `FileUtil.findAvailableName(baseDir, baseName, suffix)`: the first `name[ (n)]suffix` that does not exist.
pub(super) fn find_available_name(
    base_dir: &Path,
    base_name: &str,
    suffix: &str,
) -> io::Result<String> {
    let mut base_name = sanitize_name(base_name);
    if RESERVED_WINDOWS_FILENAMES.is_match(&base_name) {
        base_name = format!("_{base_name}_");
    }
    let mut count: i64 = 0;
    if let Some(captures) = COPY_COUNTER_PATTERN.captures(&base_name) {
        let parsed = captures["count"].parse::<i64>().map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("NumberFormatException: {e}"),
            )
        })?;
        let name = captures["name"].to_owned();
        base_name = name;
        count = parsed;
    }
    let units: Vec<u16> = base_name.encode_utf16().collect();
    let max = 255usize.saturating_sub(suffix.encode_utf16().count());
    let base_units: Vec<u16> = if units.len() > max {
        units[..max].to_vec()
    } else {
        units
    };
    loop {
        let mut name_to_test = base_units.clone();
        if count != 0 {
            let count_suffix: Vec<u16> = format!(" ({count})").encode_utf16().collect();
            let length = 255 - count_suffix.len();
            name_to_test.truncate(length);
            name_to_test.extend(count_suffix);
        }
        let mut name = String::from_utf16_lossy(&name_to_test);
        name.push_str(suffix);
        let candidate = base_dir.join(&name);
        match create_directory(&candidate) {
            Ok(()) => {
                delete_if_exists(&candidate)?;
                return Ok(name);
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => count += 1,
            Err(e) => return Err(e),
        }
    }
}
