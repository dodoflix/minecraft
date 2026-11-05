//! `net.minecraft.stats.ServerStatsCounter`: the per-player statistics and their `players/stats/<uuid>.json` file.
//!
//! shortcut: files written by an older game version are not run through the DataFixer (`DataFixTypes.STATS`); the
//! current format (`{"stats": {<type>: {<stat>: <int>}}, "DataVersion": N}`) is read and written exactly. The key
//! order of a written file follows vanilla's `HashMap` iteration order, except that the stat types are written in
//! registry order (vanilla's order there is the identity hash of the `StatType` objects, i.e. random per JVM run).
use super::hash_order::{java_hash_map_order, object_open_hash_order};
use super::stat::{Stat, build_name, by_names, stat_type, stat_type_name, value_name};
use super::stats_counter::StatsCounter;
use indexmap::IndexSet;
use mc_core::ident::Identifier;
use mc_core::math::java::java_string_hash;
use mc_data::StatType;
use mc_protocol::packets::play::ClientboundAwardStatsPacket;
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

pub struct ServerStatsCounter {
    pub counter: StatsCounter,
    file: Option<PathBuf>,
    /// `dirty` (a `HashSet`): kept in insertion order, which `send_stats` turns into the Java iteration order.
    dirty: IndexSet<Stat>,
}

/// `Codec.INT` on a `JsonOps` element: numbers (`Number.intValue`) and booleans (as 0/1).
fn json_int(value: &Value) -> Result<i32, String> {
    match value {
        Value::Number(n) => Ok(n.as_i64().map(|v| v as i32).or_else(|| n.as_u64().map(|v| v as i32)).unwrap_or_else(|| n.as_f64().map_or(0, |f| f as i64 as i32))),
        Value::Bool(b) => Ok(i32::from(*b)),
        other => Err(format!("Not a number: {other}")),
    }
}

impl ServerStatsCounter {
    /// A counter that is never written to disk (players created without a player list).
    pub fn detached() -> Self {
        Self { counter: StatsCounter::default(), file: None, dirty: IndexSet::new() }
    }

    /// `new ServerStatsCounter(server, file)`: reads `file` when it is a regular file; unreadable or unparsable files
    /// are logged and ignored (the stats stay empty).
    pub fn new(file: PathBuf) -> Self {
        let mut this = Self { counter: StatsCounter::default(), file: Some(file), dirty: IndexSet::new() };
        let file = this.file.clone().expect("just set");
        if file.is_file() {
            match std::fs::read_to_string(&file) {
                Ok(text) => match serde_json::from_str::<Value>(&text) {
                    Ok(element) => this.parse(&element),
                    Err(e) => log::error!("Couldn't parse statistics file {}: {e}", file.display()),
                },
                Err(e) => log::error!("Couldn't read statistics file {}: {e}", file.display()),
            }
        }
        this
    }

    pub fn file(&self) -> Option<&Path> {
        self.file.as_deref()
    }

    /// `save()`: creates the directory; failures are logged (`Couldn't save stats to {}`).
    pub fn save(&self) {
        let Some(file) = &self.file else { return };
        if let Err(e) = crate::server::players::player_files::save_json(file, &self.to_json()) {
            log::error!("Couldn't save stats to {}: {e}", file.display());
        }
    }

    pub fn increment(&mut self, stat: Stat, count: i32) {
        let value = (i64::from(self.counter.get_value(&stat)) + i64::from(count)).min(i64::from(i32::MAX)) as i32;
        self.set_value(stat, value);
    }

    /// `setValue(player, stat, count)`: marks the stat dirty.
    pub fn set_value(&mut self, stat: Stat, count: i32) {
        self.counter.set_value(stat, count);
        self.dirty.insert(stat);
    }

    pub fn get_value(&self, stat: &Stat) -> i32 {
        self.counter.get_value(stat)
    }

    /// `parse(fixerUpper, element)` (without the DataFixer): `STATS_CODEC.parse(data.get("stats").orElseEmptyMap())`
    /// with `resultOrPartial`: malformed entries are skipped (and logged), the rest is loaded.
    pub fn parse(&mut self, element: &Value) {
        let empty = Value::Object(Map::new());
        let stats = element.as_object().and_then(|o| o.get("stats")).unwrap_or(&empty);
        let Some(types) = stats.as_object() else {
            log::error!("Failed to parse statistics for {}: Not a map: {stats}", self.file.as_deref().map(|f| f.display().to_string()).unwrap_or_default());
            return;
        };
        let mut errors = Vec::new();
        for (type_key, values) in types {
            let Ok(type_id) = Identifier::parse(type_key) else {
                errors.push(format!("Non [a-z0-9_.:-] character in key: {type_key}"));
                continue;
            };
            if StatType::by_name(&type_id.to_string()).is_none() {
                errors.push(format!("Unknown registry key in ResourceKey[minecraft:root / minecraft:stat_type]: {type_id}"));
                continue;
            }
            let Some(values) = values.as_object() else {
                errors.push(format!("Not a JSON object: {values}"));
                continue;
            };
            for (value_key, count) in values {
                let parsed = Identifier::parse(value_key)
                    .map_err(|_| format!("Non [a-z0-9_.:-] character in key: {value_key}"))
                    .and_then(|value_id| by_names(&type_id, &value_id))
                    .and_then(|stat| json_int(count).map(|c| (stat, c)));
                match parsed {
                    Ok((stat, c)) => {
                        self.counter.set_value(stat, c);
                    }
                    Err(e) => errors.push(e),
                }
            }
        }
        if !errors.is_empty() {
            log::error!(
                "Failed to parse statistics for {}: {}",
                self.file.as_deref().map(|f| f.display().to_string()).unwrap_or_default(),
                errors.join("; ")
            );
        }
    }

    /// `toJson()`.
    pub fn to_json(&self) -> Value {
        let ordered = self.counter.iter_fast_order();
        let mut stats = Map::new();
        for ty in StatType::iter() {
            let of_type: Vec<&(Stat, i32)> = ordered.iter().filter(|(s, _)| stat_type(s) == ty).collect();
            if of_type.is_empty() {
                continue;
            }
            // `Util.toMap()` groups: a `HashMap` keyed by the stat (hash = name hash)
            let names: Vec<String> = of_type.iter().map(|(s, _)| build_name(s)).collect();
            let mut values = Map::new();
            for i in java_hash_map_order(&names, 16) {
                let (stat, count) = of_type[i];
                values.insert(value_name(stat).to_owned(), Value::from(*count));
            }
            stats.insert(stat_type_name(&of_type[0].0).to_owned(), Value::Object(values));
        }
        let mut root = Map::new();
        root.insert("stats".into(), Value::Object(stats));
        root.insert("DataVersion".into(), Value::from(mc_world::level_storage::WORLD_VERSION));
        Value::Object(root)
    }

    /// `markAllDirty()`: every stat, in the counter's iteration order.
    pub fn mark_all_dirty(&mut self) {
        for (stat, _) in self.counter.iter_fast_order() {
            self.dirty.insert(stat);
        }
    }

    /// `getDirty()`: copies the set into a new `HashSet`, clears the old one; returns the copy in Java iteration order.
    fn take_dirty(&mut self) -> Vec<Stat> {
        let names: Vec<String> = self.dirty.iter().map(build_name).collect();
        let source: Vec<Stat> = java_hash_map_order(&names, 16).into_iter().map(|i| self.dirty[i]).collect();
        self.dirty.clear();
        // `new HashSet<>(collection)`: initial capacity `max((int)(size / .75f) + 1, 16)`
        let capacity = ((source.len() as f32 / 0.75) as usize + 1).max(16);
        let names: Vec<String> = source.iter().map(build_name).collect();
        java_hash_map_order(&names, capacity).into_iter().map(|i| source[i]).collect()
    }

    /// `sendStats(player)`: the `ClientboundAwardStatsPacket` for the dirty stats (a fastutil `Object2IntOpenHashMap`).
    pub fn stats_packet(&mut self) -> ClientboundAwardStatsPacket {
        let dirty = self.take_dirty();
        let hashes: Vec<i32> = dirty.iter().map(|s| java_string_hash(&build_name(s))).collect();
        let stats = object_open_hash_order(&hashes, 16).into_iter().map(|i| (dirty[i], mc_protocol::codec::VarInt(self.counter.get_value(&dirty[i])))).collect();
        ClientboundAwardStatsPacket { stats }
    }
}
