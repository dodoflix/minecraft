//! `net.minecraft.util.FileUtil` (`sanitizeName`, `isPathPartPortable`, `findAvailableName`).

use std::io;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

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

/// `FileUtil.sanitizeName(baseName)`.
pub fn sanitize_name(base_name: &str) -> String {
    base_name
        .chars()
        .map(|c| {
            if ILLEGAL_FILE_CHARACTERS.contains(&c) || matches!(c, '.' | '/' | '"') {
                '_'
            } else {
                c
            }
        })
        .collect()
}

/// `FileUtil.isPathPartPortable`.
pub fn is_path_part_portable(name: &str) -> bool {
    !RESERVED_WINDOWS_FILENAMES.is_match(name)
}

/// `Util.sanitizeName(name, Identifier::validPathChar)`.
pub fn sanitize_to_path_chars(name: &str) -> String {
    name.chars()
        .map(|c| {
            if matches!(c, '_' | '-' | 'a'..='z' | '0'..='9' | '/' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// `FileUtil.findAvailableName(baseDir, baseName, suffix)`: the first `name[ (n)]suffix` that does not exist.
pub fn find_available_name(base_dir: &Path, base_name: &str, suffix: &str) -> io::Result<String> {
    let mut base_name = sanitize_name(base_name);
    if !is_path_part_portable(&base_name) {
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
            if name_to_test.len() > length {
                name_to_test.truncate(length);
            }
            name_to_test.extend(count_suffix);
        }
        let mut name = String::from_utf16_lossy(&name_to_test);
        name.push_str(suffix);
        match std::fs::create_dir(base_dir.join(&name)) {
            Ok(()) => {
                let _ = std::fs::remove_dir(base_dir.join(&name));
                return Ok(name);
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => count += 1,
            Err(e) => return Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_and_numbers_names() {
        assert_eq!(sanitize_name("a.b/c:d\"e"), "a_b_c_d_e");
        assert!(!is_path_part_portable("CON"));
        assert!(!is_path_part_portable("abc."));
        assert!(is_path_part_portable("abc"));
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            find_available_name(dir.path(), "x-y", ".zip").unwrap(),
            "x-y.zip"
        );
        std::fs::create_dir(dir.path().join("x-y.zip")).unwrap();
        assert_eq!(
            find_available_name(dir.path(), "x-y", ".zip").unwrap(),
            "x-y (1).zip"
        );
        std::fs::create_dir(dir.path().join("x-y (1).zip")).unwrap();
        assert_eq!(
            find_available_name(dir.path(), "x-y", ".zip").unwrap(),
            "x-y (2).zip"
        );
    }
}
