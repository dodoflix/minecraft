//! The location of `players/stats/<uuid>.json` (`crate::stats::ServerStatsCounter` reads/writes it, including the legacy
//! `<name>.json` migration) and the shared JSON writer of the per-player files (`players/advancements/<uuid>.json` is
//! `crate::advancements::PlayerAdvancements`).

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Gson `setPrettyPrinting` output (2-space indent, `"key": value`), no trailing newline, with the default `htmlSafe`
/// escapes: `< > & = '` and U+2028/U+2029 become `\uXXXX` (they can only occur inside a string).
pub fn to_gson_pretty(value: &Value) -> String {
    let mut out = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b"  ");
    let mut ser = serde_json::Serializer::with_formatter(&mut out, formatter);
    serde::Serialize::serialize(value, &mut ser).expect("serializing a JSON value cannot fail");
    let json = String::from_utf8(out).expect("serde_json writes UTF-8");
    let mut escaped = String::with_capacity(json.len());
    for c in json.chars() {
        match c {
            '<' | '>' | '&' | '=' | '\'' | '\u{2028}' | '\u{2029}' => {
                escaped.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => escaped.push(c),
        }
    }
    escaped
}

pub(crate) fn save_json(file: &Path, json: &Value) -> std::io::Result<()> {
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(file, to_gson_pretty(json))
}

/// A file whose `DataVersion` (1343 when absent, like the stats and advancements loads) is below the registered part of
/// the fix chain cannot be upgraded and is loaded as empty; the next save would overwrite it, so a copy
/// `<name>_corrupted_<time>.<ext>` is made first (the naming of `PlayerDataStorage.backup`).
pub(crate) fn keep_copy_if_unsupported(file: &Path, json: &Value) {
    let version = json
        .get("DataVersion")
        .and_then(Value::as_i64)
        .map_or(1343, |v| v as i32);
    if mc_datafix::data_fixer().check_supported(version).is_ok() {
        return;
    }
    let (stem, ext) = (
        file.file_stem().unwrap_or_default().to_string_lossy(),
        file.extension().unwrap_or_default().to_string_lossy(),
    );
    let copy = file.with_file_name(format!(
        "{stem}_corrupted_{}.{ext}",
        mc_world::level_storage::format_file_name_date_now()
    ));
    if let Err(e) = fs::copy(file, &copy) {
        log::warn!(
            "Failed to copy {} to {}: {e}",
            file.display(),
            copy.display()
        );
    }
}

/// `PlayerList.locateStatsFile`: `<uuid>.json`, migrating a legacy `<name>.json` (`FileUtil.isValidPathSegment`).
pub fn locate_stats_file(stats_dir: &Path, id: Uuid, name: &str) -> PathBuf {
    let uuid_file = stats_dir.join(format!("{id}.json"));
    if uuid_file.exists() {
        return uuid_file;
    }
    let name_file = format!("{name}.json");
    if is_valid_path_segment(&name_file) {
        let name_path = stats_dir.join(&name_file);
        if name_path.is_file() {
            return match fs::rename(&name_path, &uuid_file) {
                Ok(()) => uuid_file,
                Err(_) => {
                    log::warn!("Failed to copy file {name_file} to {}", uuid_file.display());
                    name_path
                }
            };
        }
    }
    uuid_file
}

/// `FileUtil.isValidPathSegment`: not `.`/`..`-like, no separators or reserved characters.
fn is_valid_path_segment(s: &str) -> bool {
    !s.is_empty()
        && s != "."
        && s != ".."
        && !s.chars().any(|c| {
            matches!(
                c,
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0'
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gson_escapes_html_characters_in_strings() {
        let json: Value =
            serde_json::from_str(r#"{"a<b>":"x&y=z'w","n":[1,{"k":[]}],"u":"\u2028 é"}"#).unwrap();
        assert_eq!(
            to_gson_pretty(&json),
            "{\n  \"a\\u003cb\\u003e\": \"x\\u0026y\\u003dz\\u0027w\",\n  \"n\": [\n    1,\n    {\n      \"k\": []\n    }\n  ],\n  \"u\": \"\\u2028 é\"\n}"
        );
    }

    #[test]
    fn legacy_name_file_is_migrated() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Steve.json"), "{}").unwrap();
        let id = Uuid::from_u128(7);
        let located = locate_stats_file(dir.path(), id, "Steve");
        assert_eq!(located, dir.path().join(format!("{id}.json")));
        assert!(located.exists() && !dir.path().join("Steve.json").exists());
        assert!(!is_valid_path_segment("../x") && is_valid_path_segment("Steve.json"));
    }
}
