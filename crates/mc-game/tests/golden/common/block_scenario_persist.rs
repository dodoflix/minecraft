//! The save, unload and reload verbs and the file dumps of the block replay (grammar and recorded twin: `tests/oracle/PersistVerbs.java`):
//! `save [flush]`, `unload CX CZ`, `unloadall`, `load CX CZ`, `reload CX CZ`, `rejoin PLAYER`, dumps `region:CX,CZ`, `entityfile:CX,CZ`,
//! `poifile:CX,CZ`, `playerfile:PLAYER`, `datafile:ID`, `statsfile:PLAYER`, `advancementsfile:PLAYER`, `forget_structure_checks`.
use super::Scenario;
use crate::common::entity_scenario::canon;
use mc_core::ident::Identifier;
use mc_core::pos::ChunkPos;
use mc_game::server::level::ServerLevel;
use mc_game::server::players::NameAndId;
use mc_nbt::{CompoundTag, Tag};
use mc_world::level_storage::SavedDataStorage;
use std::collections::BTreeMap;
use uuid::Uuid;

/// The server ticks a verb waits for a chunk to unload or load (`PersistVerbs.MAX_TICKS`).
const MAX_TICKS: i32 = 40;

fn int(s: &str) -> i32 {
    s.parse().unwrap_or_else(|_| panic!("not an integer: {s}"))
}

/// The UUIDs inside `tag` (an int array of four) as the scenario names of their entities, `?` for any other.
fn named(tag: &mut Tag, names: &BTreeMap<Uuid, String>) {
    match tag {
        Tag::IntArray(ints) if ints.len() == 4 => {
            let pair = |a: i32, b: i32| (u64::from(a as u32) << 32) | u64::from(b as u32);
            let uuid = Uuid::from_u64_pair(pair(ints[0], ints[1]), pair(ints[2], ints[3]));
            *tag = Tag::String(
                names
                    .get(&uuid)
                    .cloned()
                    .unwrap_or_else(|| "?".into())
                    .into(),
            );
        }
        Tag::Compound(compound) => compound.values_mut().for_each(|v| named(v, names)),
        Tag::List(list) => list.iter_mut().for_each(|v| named(v, names)),
        _ => {}
    }
}

impl Scenario {
    /// The persistence verbs; `false` for any other command.
    pub(super) fn persist_command(&mut self, a: &[String]) -> bool {
        match a[0].as_str() {
            "save" => {
                self.world();
                self.save_everything(a.get(1).is_some_and(|f| f == "flush"));
            }
            "unload" => {
                self.world();
                self.unload(int(&a[1]), int(&a[2]));
            }
            "forget_structure_checks" => {
                self.world();
                let clear = |level: &ServerLevel| {
                    if let Some(env) = &level.gen_env {
                        env.structure_check.clear_caches();
                    }
                };
                clear(self.level());
                if self.multi() {
                    mc_game::commands::server_access::with_server(|server| {
                        server.all_levels().for_each(clear)
                    });
                }
            }
            "unloadall" => {
                self.world();
                self.remember_uuids();
                self.focus(self.current);
                let forced: Vec<i64> = self
                    .level()
                    .chunk_source()
                    .get_force_loaded_chunks()
                    .iter()
                    .collect();
                for chunk in forced {
                    self.level()
                        .set_chunk_forced(chunk as i32, (chunk >> 32) as i32, false);
                }
                let mut ticks = 0;
                while self.level().chunk_source().get_loaded_chunks_count() > 0 && ticks < MAX_TICKS
                {
                    self.tick_once();
                    ticks += 1;
                }
                let loaded = self.level().chunk_source().get_loaded_chunks_count();
                self.out
                    .push(format!("UNLOADALL ticks={ticks} loaded={loaded}"));
            }
            "load" => {
                self.world();
                self.load_chunk(int(&a[1]), int(&a[2]));
            }
            "reload" => {
                self.world();
                let (cx, cz) = (int(&a[1]), int(&a[2]));
                if self.unload(cx, cz) {
                    self.load_chunk(cx, cz);
                } else {
                    self.out.push(format!("RELOAD {cx} {cz} unloaded=false"));
                }
            }
            "rejoin" => {
                self.world();
                let profile =
                    mc_game::server::network::login_listener::create_offline_profile(&a[1]);
                let saved = mc_game::commands::server_access::with_server(|server| {
                    server
                        .player_list
                        .load_player_data(&NameAndId::new(profile.id, profile.name.clone()))
                })
                .unwrap_or_else(|| panic!("no saved data of {}", a[1]));
                let list = |key: &str, index: usize| {
                    saved
                        .get_list(key)
                        .and_then(|l| l.get_double(index).or(l.get_float(index).map(f64::from)))
                        .unwrap_or(0.0)
                };
                let mode = match saved.get_int_or("playerGameType", 0) {
                    1 => "creative",
                    2 => "adventure",
                    3 => "spectator",
                    _ => "survival",
                };
                // the login prepares the spawn in the saved dimension (`PrepareSpawnTask.start_world`), not in the level a line runs in
                let dimension = saved
                    .get_string("Dimension")
                    .map(|d| d.strip_prefix("minecraft:").unwrap_or(d))
                    .and_then(|d| self.dims.iter().copied().find(|dim| *dim == d));
                if let Some(dimension) = dimension {
                    self.focus(dimension);
                }
                let number = |v: f64| mc_core::math::java::java_double_to_string(v);
                let mut join: Vec<String> = ["spawn_player", a[1].as_str(), mode]
                    .into_iter()
                    .map(str::to_owned)
                    .collect();
                join.extend((0..3).map(|i| number(list("Pos", i))));
                join.extend((0..2).map(|i| number(list("Rotation", i))));
                join.extend(a[2..].iter().cloned());
                join.push("load".into());
                assert!(self.actor_command(&join), "spawn_player");
            }
            _ => return false,
        }
        true
    }

    /// `saveEverything(true, flush, true)`: every level of the server (the level held here is lent to it), the players, the world data.
    fn save_everything(&mut self, flush: bool) {
        if mc_game::commands::server_access::has_server() {
            let mut level = self.level.take().expect("world");
            mc_game::commands::server_access::with_server(|server| {
                server.with_lent_level(&mut level, |server| {
                    server.save_everything(true, flush, true)
                })
            });
            self.level = Some(level);
        } else {
            self.level().save(flush);
        }
    }

    /// Unforces the chunk and ticks until the chunk and its entities are gone: whether they are.
    fn unload(&mut self, cx: i32, cz: i32) -> bool {
        self.remember_uuids();
        self.focus(self.current);
        self.level().set_chunk_forced(cx, cz, false);
        let mut ticks = 0;
        while self.chunk_present(cx, cz) && ticks < MAX_TICKS {
            self.tick_once();
            ticks += 1;
        }
        let loaded = self.chunk_present(cx, cz);
        self.out
            .push(format!("UNLOAD {cx} {cz} ticks={ticks} loaded={loaded}"));
        !loaded
    }

    /// The chunk is a full chunk or its entities are loaded.
    fn chunk_present(&mut self, cx: i32, cz: i32) -> bool {
        let key = ChunkPos::new(cx, cz).pack();
        let level = self.level();
        level.chunk_source().has_chunk(cx, cz) || level.are_entities_loaded(key)
    }

    fn load_chunk(&mut self, cx: i32, cz: i32) {
        self.focus(self.current);
        let key = ChunkPos::new(cx, cz).pack();
        self.level().set_chunk_forced(cx, cz, true);
        let mut ticks = 0;
        while !self.level().is_position_ticking_with_entities_loaded(key) && ticks < MAX_TICKS {
            self.tick_once();
            ticks += 1;
        }
        let loaded = self.level().is_position_ticking_with_entities_loaded(key);
        self.out
            .push(format!("LOAD {cx} {cz} ticks={ticks} loaded={loaded}"));
    }

    /// The reference of every entity the replay can still find, by UUID (an unloaded entity is gone from every level).
    fn remember_uuids(&mut self) {
        let (_, uuids) = self.names_and_uuids();
        for (uuid, id) in uuids {
            self.actors.persist_uuids.insert(id, uuid);
        }
    }

    /// A reference to an entity that is gone names the entity with its UUID that is loaded now (an unloaded entity comes back as a new one).
    pub(super) fn rebind_refs(&mut self) {
        let refs = self.actors.refs.clone();
        for (name, id) in refs {
            if self.actors.players.contains(&name) {
                continue;
            }
            let Some(&uuid) = self.actors.persist_uuids.get(&id) else {
                continue;
            };
            let live = |level: &mc_game::server::level::ServerLevel| level.get_entity(id).is_some();
            let held = self.level.as_ref().is_some_and(live);
            let elsewhere = mc_game::commands::server_access::has_server()
                && mc_game::commands::server_access::with_server(|server| {
                    server.all_levels().any(live)
                });
            if held || elsewhere {
                continue;
            }
            let found = |level: &mc_game::server::level::ServerLevel| {
                level.get_entity_by_uuid(uuid).map(|e| e.id())
            };
            let mut now = self.level.as_ref().and_then(found);
            if now.is_none() && mc_game::commands::server_access::has_server() {
                now = mc_game::commands::server_access::with_server(|server| {
                    server.all_levels().find_map(found)
                });
            }
            if let Some(now) = now {
                for entry in &mut self.actors.refs {
                    if entry.1 == id && entry.0 == name {
                        entry.1 = now;
                    }
                }
                self.actors.known.insert(now);
                self.actors.persist_uuids.insert(now, uuid);
            }
        }
    }

    /// The scenario names of the UUIDs of the entities the replay knows.
    fn uuid_names(&self) -> BTreeMap<Uuid, String> {
        let (names, uuids) = self.names_and_uuids();
        uuids
            .into_iter()
            .filter_map(|(uuid, id)| names.get(&id).map(|n| (uuid, n.clone())))
            .collect()
    }

    fn file_text(&self, tag: Option<CompoundTag>, strip_light: bool) -> String {
        let Some(mut tag) = tag else {
            return "none".into();
        };
        if strip_light && let Some(Tag::List(sections)) = tag.get_mut("sections") {
            for section in sections.iter_mut() {
                if let Tag::Compound(section) = section {
                    section.remove("BlockLight");
                    section.remove("SkyLight");
                }
            }
        }
        let mut tag = Tag::Compound(tag);
        named(&mut tag, &self.uuid_names());
        canon(&tag)
    }

    /// The file dumps; `false` for any other kind.
    pub(super) fn dump_persist(&mut self, kind: &str, arg: Option<&str>) -> bool {
        let chunk = || {
            let c: Vec<i32> = arg
                .expect("CX,CZ")
                .split(',')
                .map(|n| n.parse().expect("chunk coordinate"))
                .collect();
            ChunkPos::new(c[0], c[1])
        };
        let line = match kind {
            "region" => {
                let stored = self
                    .level()
                    .chunk_source()
                    .chunk_map
                    .read_stored_chunk(chunk());
                format!(
                    "REGION {} {}",
                    arg.expect("CX,CZ"),
                    self.file_text(stored, true)
                )
            }
            "entityfile" => {
                let stored = self.level().entity_manager.read_stored_entities(chunk());
                format!(
                    "ENTITYFILE {} {}",
                    arg.expect("CX,CZ"),
                    self.file_text(stored, false)
                )
            }
            "poifile" => {
                let stored = self
                    .level()
                    .chunk_source()
                    .chunk_map
                    .poi_manager
                    .read_stored_chunk(chunk());
                format!(
                    "POIFILE {} {}",
                    arg.expect("CX,CZ"),
                    self.file_text(stored, false)
                )
            }
            "playerfile" => {
                let name = arg.expect("playerfile:PLAYER");
                let profile =
                    mc_game::server::network::login_listener::create_offline_profile(name);
                let stored = mc_game::commands::server_access::with_server(|server| {
                    server
                        .player_list
                        .load_player_data(&NameAndId::new(profile.id, profile.name.clone()))
                });
                let stored = stored.map(|mut tag| {
                    super::super::mob_verbs::sort_recipe_book(&mut tag);
                    tag
                });
                format!("PLAYERFILE {name} {}", self.file_text(stored, false))
            }
            "statsfile" | "advancementsfile" => {
                let name = arg.expect("PLAYER");
                let id = mc_game::server::network::login_listener::create_offline_profile(name).id;
                let directory = if kind == "statsfile" {
                    mc_world::level_storage::LevelResource::PLAYER_STATS_DIR
                } else {
                    mc_world::level_storage::LevelResource::PLAYER_ADVANCEMENTS_DIR
                };
                let file = mc_game::commands::server_access::with_server(|server| {
                    server.storage_source.get_level_path(directory)
                })
                .join(format!("{id}.json"));
                let mut leaves = Vec::new();
                if let Ok(text) = std::fs::read_to_string(&file) {
                    json_leaves("", &serde_json::from_str(&text).expect("json"), &mut leaves);
                    leaves.sort();
                }
                let text = if leaves.is_empty() {
                    "none".to_owned()
                } else {
                    leaves.join(";")
                };
                format!("{} {name} {text}", kind.to_uppercase())
            }
            "datafile" => {
                let id = arg.expect("datafile:ID");
                let identifier = Identifier::parse(id).expect("saved data id");
                let file = self
                    .level()
                    .get_data_storage()
                    .storage()
                    .get_data_file(&identifier)
                    .expect("data file path");
                if file.exists() {
                    let root =
                        SavedDataStorage::read_tag_from_disk(&file).expect("the data file reads");
                    let version = root.get_int_or("DataVersion", -1);
                    // the chunk tickets come out of a hash map: sorted
                    let mut data = root.get_compound("data").cloned().unwrap_or_default();
                    if let Some(Tag::List(list)) = data.get_mut("tickets") {
                        let mut items: Vec<Tag> = list.iter().cloned().collect();
                        items.sort_by_key(canon);
                        *list = items.into_iter().fold(mc_nbt::ListTag::new(), |mut l, i| {
                            l.push(i);
                            l
                        });
                    }
                    format!(
                        "DATAFILE {id} DataVersion={version} {}",
                        self.file_text(Some(data), false)
                    )
                } else {
                    format!("DATAFILE {id} none")
                }
            }
            _ => return false,
        };
        self.out.push(line);
        true
    }
}

/// `path=value` for every leaf of `json`; an obtained time (`2026-11-16 10:41:34 +0300`) prints as `T`.
fn json_leaves(path: &str, json: &serde_json::Value, out: &mut Vec<String>) {
    use serde_json::Value;
    match json {
        Value::Object(map) => map
            .iter()
            .for_each(|(k, v)| json_leaves(&format!("{path}/{k}"), v, out)),
        Value::Array(items) => items
            .iter()
            .enumerate()
            .for_each(|(i, v)| json_leaves(&format!("{path}[{i}]"), v, out)),
        leaf => {
            let value = leaf
                .as_str()
                .map_or_else(|| leaf.to_string(), str::to_owned);
            let bytes = value.as_bytes();
            let time = bytes.len() == 25
                && bytes[4] == b'-'
                && bytes[10] == b' '
                && bytes[13] == b':'
                && bytes[19] == b' ';
            out.push(format!("{path}={}", if time { "T" } else { &value }));
        }
    }
}
