//! The location of `players/stats/<uuid>.json` (`crate::stats::ServerStatsCounter` reads/writes it, including the legacy
//! `<name>.json` migration) and the shared JSON writer of the per-player files (`players/advancements/<uuid>.json` is
//! `crate::advancements::PlayerAdvancements`).

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Gson `setPrettyPrinting` output (2-space indent, `"key": value`), no trailing newline.
fn to_gson_pretty(value: &Value) -> String {
    let mut out = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b"  ");
    let mut ser = serde_json::Serializer::with_formatter(&mut out, formatter);
    serde::Serialize::serialize(value, &mut ser).expect("serializing a JSON value cannot fail");
    String::from_utf8(out).expect("serde_json writes UTF-8")
}

pub(crate) fn save_json(file: &Path, json: &Value) -> std::io::Result<()> {
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(file, to_gson_pretty(json))
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
