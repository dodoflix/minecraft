//! `net.minecraft.stats.ServerStatsCounter`: the per-player statistics and their `players/stats/<uuid>.json` file.
//!
//! A file is upgraded from its `DataVersion` (1343 when absent) through `DataFixTypes.STATS`, then read in the current
//! format (`{"stats": {<type>: {<stat>: <int>}}, "DataVersion": N}`). The key order of a written file follows
//! vanilla's `HashMap` iteration order, except that the stat types are written in registry order (vanilla's order
//! there is the identity hash of the `StatType` objects, i.e. random per JVM run).
use super::hash_order::{
    DEFAULT_TABLE, JavaHashMapSim, java_hash_map_order, object_open_hash_order, presized_table,
};
use super::stat::{Stat, build_name, by_names, stat_type, stat_type_name, value_name};
use super::stats_counter::StatsCounter;
use crate::server::players::player_files::{JsonFileError, read_strict_json};
use indexmap::IndexSet;
use mc_core::gson::JsonElement;
use mc_core::ident::Identifier;
use mc_core::math::java::java_string_hash;
use mc_data::StatType;
use mc_datafix::{DataFixTypes, data_fixer};
use mc_dfu::extra::{IDENTIFIER, dispatched_map};
use mc_dfu::{Codec, DataResult, Dyn, Dynamic, INT, JSON};
use mc_protocol::packets::play::ClientboundAwardStatsPacket;
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

pub struct ServerStatsCounter {
    pub counter: StatsCounter,
    file: Option<PathBuf>,
    /// `dirty` (a `HashSet`): kept in insertion order, which `send_stats` turns into the Java iteration order.
    dirty: IndexSet<Stat>,
}

/// The entries of a stats file per stat type.
type StatsByType = Vec<(StatType, Vec<(Stat, i32)>)>;

/// `STATS_CODEC` up to its `xmap`: `Codec.dispatchedMap(STAT_TYPE.byNameCodec(), type -> Codec.unboundedMap(statCodec, Codec.INT))`
/// as the entries per stat type. Only decoding is needed (the file is written by [`ServerStatsCounter::to_json`], in the
/// order of vanilla's hash maps), so the encoding halves of the registry codecs refuse.
fn stats_codec() -> &'static Codec<StatsByType> {
    static CODEC: LazyLock<Codec<StatsByType>> = LazyLock::new(|| {
        let unsupported = || DataResult::error(|| "Not encoded by this codec".to_owned());
        let type_codec = IDENTIFIER.flat_xmap(
            |id| match StatType::by_name(&id.to_string()) {
                Some(ty) => DataResult::success(ty),
                None => DataResult::error(move || {
                    format!(
                        "Unknown registry key in ResourceKey[minecraft:root / minecraft:stat_type]: {id}"
                    )
                }),
            },
            move |_: &StatType| unsupported(),
        );
        dispatched_map(&type_codec, |ty: &StatType| {
            let type_id = Identifier::parse(ty.name()).expect("a vanilla stat type id");
            let stat_codec = IDENTIFIER.flat_xmap(
                move |value_id| match by_names(&type_id, &value_id) {
                    Ok(stat) => DataResult::success(stat),
                    Err(message) => DataResult::error(move || message.clone()),
                },
                |_: &Stat| DataResult::error(|| "Not encoded by this codec".to_owned()),
            );
            DataResult::success(stat_codec.unbounded_map(&INT))
        })
    });
    &CODEC
}

impl ServerStatsCounter {
    /// A counter that is never written to disk (players created without a player list).
    pub fn detached() -> Self {
        Self {
            counter: StatsCounter::default(),
            file: None,
            dirty: IndexSet::new(),
        }
    }

    /// `new ServerStatsCounter(server, file)`: reads `file` when it is a regular file; unreadable or unparsable files
    /// are logged and ignored (the stats stay empty).
    pub fn new(file: PathBuf) -> Self {
        let mut this = Self {
            counter: StatsCounter::default(),
            file: Some(file),
            dirty: IndexSet::new(),
        };
        let file = this.file.clone().expect("just set");
        if file.is_file() {
            match read_strict_json(&file) {
                Ok(element) => this.parse(&element),
                Err(JsonFileError::Open(e)) => {
                    log::error!("Couldn't read statistics file {}\n{e}", file.display());
                }
                Err(JsonFileError::Reader(e)) => log::error!(
                    "Couldn't parse statistics file {}\ncom.google.gson.JsonIOException: {e}",
                    file.display()
                ),
                Err(JsonFileError::Parse(e)) => log::error!(
                    "Couldn't parse statistics file {}\ncom.google.gson.JsonSyntaxException: {e}",
                    file.display()
                ),
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
        let value = (i64::from(self.counter.get_value(&stat)) + i64::from(count))
            .min(i64::from(i32::MAX)) as i32;
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

    /// `DataFixTypes.STATS.updateToCurrentVersion(fixerUpper, data, NbtUtils.getDataVersion(data, 1343))` over the
    /// JSON element.
    fn upgrade(element: &JsonElement) -> JsonElement {
        let data = Dynamic::new(JSON, Dyn::from_json_element(element));
        let version = data.get("DataVersion").as_int_or(1343);
        let fixed = DataFixTypes::STATS.update_to_current_version(data_fixer(), data, version);
        fixed.value().to_json_element()
    }

    /// `parse(fixerUpper, element)`: the upgrade, then `STATS_CODEC.parse(data.get("stats").orElseEmptyMap())`
    /// with `resultOrPartial`: malformed entries are skipped (and logged), the rest is loaded.
    pub fn parse(&mut self, element: &JsonElement) {
        let element = Self::upgrade(element);
        let stats = element
            .as_object()
            .and_then(|o| o.get("stats"))
            .map_or_else(|| Dyn::Map(Vec::new()), Dyn::from_json_element);
        // the codec's result: the valid entries per stat type, in file order
        let grouped: Vec<Vec<(Stat, i32)>> = match stats_codec().parse(&JSON, &stats) {
            DataResult::Success(grouped) => grouped,
            DataResult::Error { message, partial } => {
                log::error!(
                    "Failed to parse statistics for {}: {}",
                    self.file
                        .as_deref()
                        .map(|f| f.display().to_string())
                        .unwrap_or_default(),
                    message.get()
                );
                partial.unwrap_or_default()
            }
        }
        .into_iter()
        .map(|(_, of_type)| of_type)
        .collect();
        // `xmap`: `HashMap<Stat, Integer> stats = new HashMap<>(); groupedStats.forEach((type, values) -> stats.putAll(values))`
        let flat: Vec<(Stat, i32)> = grouped.iter().flatten().copied().collect();
        let mut hash_map = JavaHashMapSim::new();
        let mut next = 0;
        for of_type in &grouped {
            hash_map.put_all_start(of_type.len());
            for (stat, _) in of_type {
                hash_map.put(next, java_string_hash(&build_name(stat)));
                next += 1;
            }
        }
        // `this.stats.putAll(map)`: presizes the fastutil table, then puts in the HashMap's iteration order
        self.counter.table =
            presized_table(self.counter.table, self.counter.stats.len() + flat.len());
        for i in hash_map.order() {
            self.counter.set_value(flat[i].0, flat[i].1);
        }
    }

    /// `toJson()`.
    pub fn to_json(&self) -> Value {
        // `entrySet().stream()` walks the table upwards (fastutil's spliterator), unlike the iterator `markAllDirty` uses
        let mut ordered = self.counter.iter_fast_order();
        ordered.reverse();
        let mut stats = Map::new();
        for ty in StatType::iter() {
            let of_type: Vec<&(Stat, i32)> =
                ordered.iter().filter(|(s, _)| stat_type(s) == ty).collect();
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
            stats.insert(
                stat_type_name(&of_type[0].0).to_owned(),
                Value::Object(values),
            );
        }
        let mut root = Map::new();
        root.insert("stats".into(), Value::Object(stats));
        root.insert(
            "DataVersion".into(),
            Value::from(mc_world::level_storage::WORLD_VERSION),
        );
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
        let source: Vec<Stat> = java_hash_map_order(&names, 16)
            .into_iter()
            .map(|i| self.dirty[i])
            .collect();
        self.dirty.clear();
        // `new HashSet<>(collection)`: initial capacity `max((int)(size / .75f) + 1, 16)`
        let capacity = ((source.len() as f32 / 0.75) as usize + 1).max(16);
        let names: Vec<String> = source.iter().map(build_name).collect();
        java_hash_map_order(&names, capacity)
            .into_iter()
            .map(|i| source[i])
            .collect()
    }

    /// `sendStats(player)`: the `ClientboundAwardStatsPacket` for the dirty stats (a fastutil `Object2IntOpenHashMap`).
    pub fn stats_packet(&mut self) -> ClientboundAwardStatsPacket {
        let dirty = self.take_dirty();
        let hashes: Vec<i32> = dirty
            .iter()
            .map(|s| java_string_hash(&build_name(s)))
            .collect();
        let stats = object_open_hash_order(&hashes, DEFAULT_TABLE)
            .into_iter()
            .map(|i| {
                (
                    dirty[i],
                    mc_protocol::codec::VarInt(self.counter.get_value(&dirty[i])),
                )
            })
            .collect();
        ClientboundAwardStatsPacket { stats }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stat(type_id: &str, value_id: &str) -> Stat {
        by_names(
            &Identifier::parse(type_id).unwrap(),
            &Identifier::parse(value_id).unwrap(),
        )
        .unwrap()
    }

    fn read(json: &str) -> ServerStatsCounter {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("stats.json");
        std::fs::write(&file, json).unwrap();
        ServerStatsCounter::new(file)
    }

    #[test]
    fn a_file_of_an_older_version_goes_through_the_fixer() {
        // the explorer map items were renamed by data version 5012
        let counter = read(
            r#"{"stats":{"minecraft:used":{"minecraft:ocean_explorer_map":3}},"DataVersion":5011}"#,
        );
        assert_eq!(
            counter.get_value(&stat("minecraft:used", "minecraft:ocean_monument_map")),
            3
        );
    }

    /// `StrictJsonParser`: a BOM is skipped and an empty file is `JsonNull`; numbers are `LazilyParsedNumber`s.
    #[test]
    fn the_strict_parser_reads_a_bom_an_empty_file_and_number_texts() {
        let jump = stat("minecraft:custom", "minecraft:jump");
        let bom = read(
            "\u{feff}{\"stats\":{\"minecraft:custom\":{\"minecraft:jump\":5}},\"DataVersion\":5023}",
        );
        assert_eq!(bom.get_value(&jump), 5);
        assert_eq!(read("").get_value(&jump), 0);
        assert_eq!(read("  \n").get_value(&jump), 0);
        let numbers = read(
            r#"{"stats":{"minecraft:custom":{"minecraft:jump":3000000000,"minecraft:walk_one_cm":1e2,"minecraft:fly_one_cm":1.5}},"DataVersion":5023}"#,
        );
        assert_eq!(numbers.get_value(&jump), 3_000_000_000_i64 as i32);
        assert_eq!(
            numbers.get_value(&stat("minecraft:custom", "minecraft:walk_one_cm")),
            100
        );
        assert_eq!(
            numbers.get_value(&stat("minecraft:custom", "minecraft:fly_one_cm")),
            1
        );
    }
}
