//! The per-player JSON files `players/stats/<uuid>.json` and `players/advancements/<uuid>.json`
//! (`ServerStatsCounter`, `PlayerAdvancements` persistence). The statistics and advancement *logic* belongs to later
//! waves; what the player list owns is the lifecycle: locate the file (`PlayerList.getPlayerStats`, including the
//! legacy `<name>.json` migration), load whatever is there, and write it back on save/leave exactly where and in the
//! shape vanilla does (`{"stats": {<type>: {<stat>: <int>}}, "DataVersion": N}`; advancements: `<id>: {...}` entries
//! then `DataVersion`), so the real server reads our files and vice versa.

use indexmap::IndexMap;
use serde_json::{Map, Value};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// `SharedConstants.getCurrentVersion().dataVersion().version()`.
fn current_data_version() -> i32 {
    mc_world::level_storage::WORLD_VERSION
}

/// Gson `setPrettyPrinting` output (2-space indent, `"key": value`), no trailing newline.
fn to_gson_pretty(value: &Value) -> String {
    let mut out = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b"  ");
    let mut ser = serde_json::Serializer::with_formatter(&mut out, formatter);
    serde::Serialize::serialize(value, &mut ser).expect("serializing a JSON value cannot fail");
    String::from_utf8(out).expect("serde_json writes UTF-8")
}

fn save_json(file: &Path, json: &Value) -> std::io::Result<()> {
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(file, to_gson_pretty(json))
}

/// `ServerStatsCounter`: `stat type -> stat value -> count`, keys as in the file.
pub struct ServerStatsCounter {
    file: PathBuf,
    pub stats: IndexMap<String, IndexMap<String, i32>>,
}

impl ServerStatsCounter {
    /// `new ServerStatsCounter(server, file)`: loads the file when it is a regular file; a corrupt file is logged
    /// (`Couldn't parse statistics file {}`) and ignored.
    pub fn new(file: PathBuf) -> Self {
        let mut counter = Self { file, stats: IndexMap::new() };
        if counter.file.is_file() {
            match fs::read_to_string(&counter.file).map_err(|e| e.to_string()).and_then(|s| serde_json::from_str::<Value>(&s).map_err(|e| e.to_string())) {
                Ok(json) => counter.parse_local_stats(&json),
                Err(e) => log::error!("Couldn't parse statistics file {}: {e}", counter.file.display()),
            }
        }
        counter
    }

    fn parse_local_stats(&mut self, json: &Value) {
        let Some(root) = json.as_object() else { return };
        if let Some(Value::Object(types)) = root.get("stats") {
            for (ty, values) in types {
                let Some(values) = values.as_object() else { continue };
                for (name, value) in values {
                    if let Some(n) = value.as_i64() {
                        // Gson `getAsInt` wraps like `Number.intValue()`
                        self.stats.entry(ty.clone()).or_default().insert(name.clone(), n as i32);
                    }
                }
            }
        }
    }

    /// `StatsCounter.increment`.
    pub fn increment(&mut self, ty: &str, stat: &str, amount: i32) {
        let entry = self.stats.entry(ty.to_owned()).or_default().entry(stat.to_owned()).or_insert(0);
        *entry = entry.wrapping_add(amount);
    }

    pub fn get_value(&self, ty: &str, stat: &str) -> i32 {
        self.stats.get(ty).and_then(|m| m.get(stat)).copied().unwrap_or(0)
    }

    /// `toJson`.
    pub fn to_json(&self) -> Value {
        let mut stats = Map::new();
        for (ty, values) in &self.stats {
            stats.insert(ty.clone(), Value::Object(values.iter().map(|(k, v)| (k.clone(), Value::from(*v))).collect()));
        }
        let mut root = Map::new();
        root.insert("stats".into(), Value::Object(stats));
        root.insert("DataVersion".into(), Value::from(current_data_version()));
        Value::Object(root)
    }

    /// `save()`: creates the directory, logs `Couldn't save stats to {}` on failure.
    pub fn save(&self) {
        if let Err(e) = save_json(&self.file, &self.to_json()) {
            log::error!("Couldn't save stats to {}: {e}", self.file.display());
        }
    }

    pub fn file(&self) -> &Path {
        &self.file
    }
}

/// `PlayerAdvancements` persistence: the progress entries (kept as the JSON they were read as) plus `DataVersion`.
pub struct PlayerAdvancementsFile {
    file: PathBuf,
    /// `advancement id -> {"criteria": {...}, "done": bool}` as loaded/recorded.
    pub progress: Map<String, Value>,
}

impl PlayerAdvancementsFile {
    pub fn new(file: PathBuf) -> Self {
        let mut progress = Map::new();
        if file.is_file() {
            match fs::read_to_string(&file).map_err(|e| e.to_string()).and_then(|s| serde_json::from_str::<Value>(&s).map_err(|e| e.to_string())) {
                Ok(Value::Object(root)) => {
                    progress = root.into_iter().filter(|(k, _)| k != "DataVersion").collect();
                }
                Ok(_) => {}
                Err(e) => log::error!("Couldn't parse player advancements in {}: {e}", file.display()),
            }
        }
        Self { file, progress }
    }

    pub fn to_json(&self) -> Value {
        let mut root = self.progress.clone();
        root.insert("DataVersion".into(), Value::from(current_data_version()));
        Value::Object(root)
    }

    /// `save()`: logs `Couldn't save player advancements to {}` on failure.
    pub fn save(&self) {
        if let Err(e) = save_json(&self.file, &self.to_json()) {
            log::error!("Couldn't save player advancements to {}: {e}", self.file.display());
        }
    }

    pub fn file(&self) -> &Path {
        &self.file
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
    !s.is_empty() && s != "." && s != ".." && !s.chars().any(|c| matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_file_roundtrip_matches_vanilla_shape() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("players/stats/x.json");
        let mut stats = ServerStatsCounter::new(file.clone());
        stats.increment("minecraft:custom", "minecraft:leave_game", 1);
        stats.save();
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            "{\n  \"stats\": {\n    \"minecraft:custom\": {\n      \"minecraft:leave_game\": 1\n    }\n  },\n  \"DataVersion\": 5023\n}"
        );
        assert_eq!(ServerStatsCounter::new(file).get_value("minecraft:custom", "minecraft:leave_game"), 1);
    }

    #[test]
    fn advancements_keep_unknown_progress() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.json");
        fs::write(&file, r#"{"minecraft:story/root":{"criteria":{"c":"2026-01-01 00:00:00 +0000"},"done":true},"DataVersion":4000}"#).unwrap();
        let adv = PlayerAdvancementsFile::new(file.clone());
        adv.save();
        let json: Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(json["DataVersion"], 5023);
        assert_eq!(json["minecraft:story/root"]["done"], true);
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
