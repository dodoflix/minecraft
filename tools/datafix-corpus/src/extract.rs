//! Candidates from one world directory, by the load paths of vanilla: which file is updated with which
//! `DataFixTypes` and from which data version (`NbtUtils.getDataVersion(tag, default)`).

use crate::candidate::Candidate;
use mc_core::ident::Identifier;
use mc_core::pos::ChunkPos;
use mc_nbt::{CompoundTag, NbtAccounter, Tag, nbt_utils::get_data_version_or};
use mc_world::region::{RegionFileStorage, RegionStorageInfo};
use serde_json::Value;
use std::io::Read;
use std::path::{Path, PathBuf};

/// `SavedDataStorage` reads a file without `DataVersion` as this version.
const SAVED_DATA_DEFAULT: i32 = 1343;
/// `PoiManager`'s default version.
const POI_DEFAULT: i32 = 1945;

/// What one world gave: the candidates and the files that could not be used.
#[derive(Default)]
pub struct Extracted {
    pub candidates: Vec<Candidate>,
    pub skipped: Vec<String>,
}

impl Extracted {
    fn push(&mut self, id: String, ty: &str, from: i32, bytes: Vec<u8>) {
        match Candidate::from_bytes(id, ty, from, bytes) {
            Ok(c) => self.candidates.push(c),
            Err(e) => self.skipped.push(e),
        }
    }

    fn push_tag(&mut self, id: String, ty: &str, from: i32, tag: &CompoundTag) {
        self.push(id, ty, from, mc_nbt::nbt_io::to_bytes(tag));
    }
}

/// The `DataFixTypes` constant of a saved data file (`rel`: its path below a `data` folder without the extension), or
/// `None` for the files no `DataFixTypes` covers (legacy village data).
pub fn saved_data_type(rel: &str) -> Option<&'static str> {
    let mut parts = rel.rsplit('/');
    let stem = parts.next()?;
    let parent = parts.next();
    let numbered = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    Some(match stem {
        "last_id" | "idcounts" => "SAVED_DATA_MAP_INDEX",
        _ if stem.strip_prefix("map_").is_some_and(numbered)
            || (parent == Some("maps") && numbered(stem)) =>
        {
            "SAVED_DATA_MAP_DATA"
        }
        "scoreboard" => "SAVED_DATA_SCOREBOARD",
        "raids" | "raids_end" => "SAVED_DATA_RAIDS",
        "chunks" | "chunk_tickets" => "SAVED_DATA_FORCED_CHUNKS",
        _ if stem.starts_with("command_storage") => "SAVED_DATA_COMMAND_STORAGE",
        "random_sequences" => "SAVED_DATA_RANDOM_SEQUENCES",
        "stopwatches" => "SAVED_DATA_STOPWATCHES",
        "world_clocks" => "SAVED_DATA_WORLD_CLOCKS",
        "world_border" => "SAVED_DATA_WORLD_BORDER",
        "weather" => "SAVED_DATA_WEATHER",
        "wandering_trader" => "SAVED_DATA_WANDERING_TRADER",
        "scheduled_events" => "SAVED_DATA_SCHEDULED_EVENTS",
        "game_rules" => "SAVED_DATA_GAME_RULES",
        "custom_boss_events" => "SAVED_DATA_CUSTOM_BOSS_EVENTS",
        "ender_dragon_fight" => "SAVED_DATA_ENDER_DRAGON_FIGHT",
        "world_gen_settings" => "SAVED_DATA_WORLD_GEN_SETTINGS",
        // the legacy structure index files (`Monument.dat`, `Fortress.dat`, ...) sit in the data folder itself
        _ if parent.is_none() && stem.starts_with(|c: char| c.is_ascii_uppercase()) => {
            "SAVED_DATA_STRUCTURE_FEATURE_INDICES"
        }
        _ => return None,
    })
}

/// `JsonOps.convertTo(NbtOps)` of the vanilla library: a number takes the narrowest of byte, short, int and long that
/// holds it, otherwise float or double; a boolean is a byte.
pub fn json_to_tag(value: &Value) -> Result<Tag, String> {
    Ok(match value {
        Value::Null => return Err("null".into()),
        Value::Bool(b) => Tag::Byte(i8::from(*b)),
        Value::Number(n) => {
            // the library reads a number as a BigDecimal: a whole value is an integer, 1.0 and 1e3 included
            let whole = n.as_i64().or_else(|| {
                n.as_f64()
                    .filter(|d| d.fract() == 0.0 && d.abs() < 9.2e18)
                    .map(|d| d as i64)
            });
            match whole {
                Some(l) => i8::try_from(l).map(Tag::Byte).unwrap_or_else(|_| {
                    i16::try_from(l)
                        .map(Tag::Short)
                        .unwrap_or_else(|_| i32::try_from(l).map(Tag::Int).unwrap_or(Tag::Long(l)))
                }),
                None => {
                    let d = n.as_f64().ok_or("number out of range")?;
                    if f64::from(d as f32) == d {
                        Tag::float(d as f32)
                    } else {
                        Tag::double(d)
                    }
                }
            }
        }
        Value::String(s) => Tag::String(s.clone()),
        Value::Array(a) => {
            let mut list = mc_nbt::ListTag::new();
            for v in a {
                list.push(json_to_tag(v)?);
            }
            Tag::List(list)
        }
        Value::Object(o) => {
            let mut c = CompoundTag::new();
            for (k, v) in o {
                c.put(k.clone(), json_to_tag(v)?);
            }
            Tag::Compound(c)
        }
    })
}

fn read_gzip_or_plain(path: &Path) -> Result<Vec<u8>, String> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    if data.starts_with(&[0x1f, 0x8b]) {
        mc_nbt::compress::gzip_decompress(&data, None).map_err(|e| e.to_string())
    } else {
        Ok(data)
    }
}

fn files(dir: &Path, extension: &str, recurse: bool) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut entries: Vec<PathBuf> = rd.filter_map(|e| Some(e.ok()?.path())).collect();
    entries.sort();
    let mut out = Vec::new();
    for p in entries {
        if p.is_dir() {
            if recurse {
                out.extend(files(&p, extension, true));
            }
        } else if p.extension().is_some_and(|e| e == extension) {
            out.push(p);
        }
    }
    out
}

fn region_coords(name: &str) -> Option<(i32, i32)> {
    let mut it = name.strip_prefix("r.")?.strip_suffix(".mca")?.split('.');
    let (x, z): (i32, i32) = (it.next()?.parse().ok()?, it.next()?.parse().ok()?);
    (name == format!("r.{x}.{z}.mca")).then_some((x, z))
}

/// Every chunk of a `region`, `entities` or `poi` folder as `(position, NbtIo bytes)`; a chunk that cannot be read is
/// reported in `skipped`.
fn chunk_payloads(
    folder: &Path,
    kind: &str,
    skipped: &mut Vec<String>,
) -> Vec<(ChunkPos, Vec<u8>)> {
    let Ok(rd) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut regions: Vec<(i32, i32)> = rd
        .filter_map(|e| region_coords(&e.ok()?.file_name().into_string().ok()?))
        .collect();
    regions.sort_unstable();
    let info = RegionStorageInfo::new(
        "datafix-corpus",
        Identifier::parse("minecraft:overworld").expect("a valid identifier"),
        kind,
    );
    let mut storage = RegionFileStorage::new(info, folder, false);
    let mut out = Vec::new();
    for (rx, rz) in regions {
        for lz in 0..32 {
            for lx in 0..32 {
                let pos = ChunkPos::new(rx * 32 + lx, rz * 32 + lz);
                let place = format!("{} chunk [{}, {}]", folder.display(), pos.x, pos.z);
                let region = match storage.get_region_file(&pos, false) {
                    Ok(Some(r)) => r,
                    Ok(None) => continue,
                    Err(e) => {
                        skipped.push(format!("{place}: {e}"));
                        continue;
                    }
                };
                match region.get_chunk_data_input_stream(&pos) {
                    Ok(Some(mut stream)) => {
                        let mut data = Vec::new();
                        match stream.read_to_end(&mut data) {
                            Ok(_) => out.push((pos, data)),
                            Err(e) => skipped.push(format!("{place}: {e}")),
                        }
                    }
                    Ok(None) => {}
                    Err(e) => skipped.push(format!("{place}: {e}")),
                }
            }
        }
    }
    out
}

/// The dimension folders of a world: the legacy ones (`.`, `DIM-1`, `DIM1`) and `dimensions/<namespace>/<path>`, each
/// with a short label for the ids and the dimension's identifier.
fn dimension_dirs(world: &Path) -> Vec<(String, String, PathBuf)> {
    let mut dirs = vec![
        (
            "ow".to_string(),
            "minecraft:overworld".to_string(),
            world.to_path_buf(),
        ),
        (
            "nether".into(),
            "minecraft:the_nether".into(),
            world.join("DIM-1"),
        ),
        ("end".into(), "minecraft:the_end".into(), world.join("DIM1")),
    ];
    for ns in files_dirs(&world.join("dimensions")) {
        for path in files_dirs(&ns) {
            let label = |p: &Path| {
                p.file_name()
                    .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
            };
            dirs.push((
                format!("{}.{}", label(&ns), label(&path)),
                format!("{}:{}", label(&ns), label(&path)),
                path,
            ));
        }
    }
    dirs
}

fn files_dirs(dir: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = rd
        .filter_map(|e| Some(e.ok()?.path()))
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    dirs
}

/// Reads the candidates of the world in `world`; `id` names it in the candidate ids. `flat` tells that the overworld
/// is a flat world (the other dimensions of a server world always have their noise generators).
pub fn extract_world(world: &Path, id: &str, flat: bool) -> Extracted {
    let mut out = Extracted::default();
    level_dat(world, id, &mut out);
    for (label, dimension, dir) in dimension_dirs(world) {
        let generator = if flat && dimension == "minecraft:overworld" {
            "minecraft:flat"
        } else {
            "minecraft:noise"
        };
        for (sub, ty, default) in [
            ("region", "CHUNK", -1),
            ("entities", "ENTITY_CHUNK", -1),
            ("poi", "POI_CHUNK", POI_DEFAULT),
        ] {
            for (pos, bytes) in chunk_payloads(&dir.join(sub), sub, &mut out.skipped) {
                let Ok(mut tag) =
                    mc_nbt::read_named(&mut bytes.as_slice(), &mut NbtAccounter::unlimited_heap())
                else {
                    out.skipped
                        .push(format!("{id}-{label}-{sub}-c{}.{}: not NBT", pos.x, pos.z));
                    continue;
                };
                let from = get_data_version_or(&tag, default);
                // `ChunkMap.upgradeChunkTag` hands the dimension and the generator to the fixes of a chunk
                let bytes = if ty == "CHUNK" {
                    let mut context = CompoundTag::new();
                    context.put_string("dimension", dimension.as_str());
                    context.put_string("generator", generator);
                    tag.put("__context", Tag::Compound(context));
                    mc_nbt::nbt_io::to_bytes(&tag)
                } else {
                    bytes
                };
                out.push(
                    format!("{id}-{label}-{sub}-c{}.{}", pos.x, pos.z),
                    ty,
                    from,
                    bytes,
                );
            }
        }
    }
    let mut data_dirs = vec![world.join("data")];
    data_dirs.extend(
        dimension_dirs(world)
            .into_iter()
            .map(|(_, _, d)| d.join("data"))
            .filter(|d| d != &world.join("data")),
    );
    for dir in data_dirs {
        for file in files(&dir, "dat", true) {
            let rel = file.strip_prefix(&dir).map_or_else(
                |_| String::new(),
                |r| r.with_extension("").to_string_lossy().replace('\\', "/"),
            );
            let Some(ty) = saved_data_type(&rel) else {
                continue;
            };
            // `LegacyStructureFileFix` reads the structure index files itself, from -1; `SavedDataStorage` reads the rest
            let default = if ty == "SAVED_DATA_STRUCTURE_FEATURE_INDICES" {
                -1
            } else {
                SAVED_DATA_DEFAULT
            };
            match read_gzip_or_plain(&file) {
                Ok(bytes) => {
                    let from = mc_nbt::read_named(
                        &mut bytes.as_slice(),
                        &mut NbtAccounter::unlimited_heap(),
                    )
                    .map_or(default, |t| get_data_version_or(&t, default));
                    let place = dir
                        .strip_prefix(world)
                        .map_or_else(|_| String::new(), |p| p.to_string_lossy().replace('/', "_"));
                    out.push(
                        format!("{id}-{place}-{}", rel.replace('/', "_")),
                        ty,
                        from,
                        bytes,
                    );
                }
                Err(e) => out.skipped.push(format!("{}: {e}", file.display())),
            }
        }
    }
    players(world, id, &mut out);
    out
}

/// `level.dat`: the `Data` compound is the input of `LEVEL` and `LEVEL_SUMMARY`; its `Player` and `WorldGenSettings`
/// are inputs of `PLAYER` and `WORLD_GEN_SETTINGS` at the version of the level.
fn level_dat(world: &Path, id: &str, out: &mut Extracted) {
    let path = world.join("level.dat");
    let root = match read_gzip_or_plain(&path).and_then(|b| {
        mc_nbt::read_named(&mut b.as_slice(), &mut NbtAccounter::unlimited_heap())
            .map_err(|e| e.to_string())
    }) {
        Ok(root) => root,
        Err(e) => {
            out.skipped.push(format!("{}: {e}", path.display()));
            return;
        }
    };
    let Some(data) = root.get_compound("Data") else {
        out.skipped
            .push(format!("{}: no Data compound", path.display()));
        return;
    };
    let from = get_data_version_or(data, -1);
    out.push_tag(format!("{id}-level"), "LEVEL", from, data);
    out.push_tag(format!("{id}-level-summary"), "LEVEL_SUMMARY", from, data);
    if let Some(player) = data.get_compound("Player") {
        out.push_tag(
            format!("{id}-level-player"),
            "PLAYER",
            get_data_version_or(player, -1),
            player,
        );
    }
    if let Some(settings) = data.get_compound("WorldGenSettings") {
        out.push_tag(
            format!("{id}-level-gen-settings"),
            "WORLD_GEN_SETTINGS",
            from,
            settings,
        );
    }
}

/// Per-player files: `playerdata/*.dat`, `stats/*.json`, `advancements/*.json`, and the same below `players/`. A JSON
/// file becomes NBT the way the data fixers see it; advancements lose their `DataVersion` (`DataFixTypes.wrapCodec`).
fn players(world: &Path, id: &str, out: &mut Extracted) {
    for (dir, ty) in [
        ("playerdata", "PLAYER"),
        ("players/data", "PLAYER"),
        ("stats", "STATS"),
        ("players/stats", "STATS"),
        ("advancements", "ADVANCEMENTS"),
        ("players/advancements", "ADVANCEMENTS"),
    ] {
        let json = ty != "PLAYER";
        for file in files(&world.join(dir), if json { "json" } else { "dat" }, false) {
            let name = format!(
                "{id}-{}-{}",
                dir.replace('/', "_"),
                file.file_stem()
                    .map_or_else(String::new, |s| s.to_string_lossy().into_owned())
            );
            if !json {
                match read_gzip_or_plain(&file) {
                    Ok(bytes) => {
                        let from = mc_nbt::read_named(
                            &mut bytes.as_slice(),
                            &mut NbtAccounter::unlimited_heap(),
                        )
                        .map_or(-1, |t| get_data_version_or(&t, -1));
                        out.push(name, ty, from, bytes);
                    }
                    Err(e) => out.skipped.push(format!("{}: {e}", file.display())),
                }
                continue;
            }
            let tag = std::fs::read_to_string(&file)
                .map_err(|e| e.to_string())
                .and_then(|text| serde_json::from_str::<Value>(&text).map_err(|e| e.to_string()))
                .and_then(|v| json_to_tag(&v));
            match tag {
                Ok(Tag::Compound(mut c)) => {
                    let from = get_data_version_or(&c, SAVED_DATA_DEFAULT);
                    if ty == "ADVANCEMENTS" {
                        c.remove("DataVersion");
                    }
                    out.push_tag(name, ty, from, &c);
                }
                Ok(_) => out
                    .skipped
                    .push(format!("{}: the root is not an object", file.display())),
                Err(e) => out.skipped.push(format!("{}: {e}", file.display())),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mc_datafix::DataFixTypes;

    #[test]
    fn saved_data_files_map_to_their_constants() {
        for (rel, ty) in [
            ("scoreboard", "SAVED_DATA_SCOREBOARD"),
            ("minecraft/scoreboard", "SAVED_DATA_SCOREBOARD"),
            ("map_12", "SAVED_DATA_MAP_DATA"),
            ("minecraft/maps/3", "SAVED_DATA_MAP_DATA"),
            ("idcounts", "SAVED_DATA_MAP_INDEX"),
            ("minecraft/maps/last_id", "SAVED_DATA_MAP_INDEX"),
            ("chunks", "SAVED_DATA_FORCED_CHUNKS"),
            ("Fortress", "SAVED_DATA_STRUCTURE_FEATURE_INDICES"),
            (
                "minecraft/command_storage_minecraft",
                "SAVED_DATA_COMMAND_STORAGE",
            ),
        ] {
            assert_eq!(saved_data_type(rel), Some(ty), "{rel}");
            assert!(DataFixTypes::by_name(ty).is_some(), "{ty}");
        }
        for rel in ["villages", "villages_end", "minecraft/Thing", "map_x"] {
            assert_eq!(saved_data_type(rel), None, "{rel}");
        }
    }

    #[test]
    fn json_numbers_take_the_narrowest_nbt_type() {
        let v: Value = serde_json::from_str(
            r#"{"a":1,"b":300,"c":70000,"d":5000000000,"e":0.5,"f":true,"g":[1,"x"]}"#,
        )
        .unwrap();
        let Tag::Compound(c) = json_to_tag(&v).unwrap() else {
            panic!("a compound")
        };
        assert_eq!(c.get("a"), Some(&Tag::Byte(1)));
        assert_eq!(c.get("b"), Some(&Tag::Short(300)));
        assert_eq!(c.get("c"), Some(&Tag::Int(70000)));
        assert_eq!(c.get("d"), Some(&Tag::Long(5_000_000_000)));
        assert_eq!(c.get("e"), Some(&Tag::float(0.5)));
        assert_eq!(c.get("f"), Some(&Tag::Byte(1)));
        assert_eq!(c.get_list("g").map(|l| l.len()), Some(2));
    }

    #[test]
    fn a_world_gives_its_level_saved_data_and_player_files() {
        let dir = tempfile::tempdir().unwrap();
        let world = dir.path();
        let mut data = CompoundTag::new();
        data.put_int("DataVersion", 1343);
        data.put("Player", Tag::Compound(CompoundTag::new()));
        let mut root = CompoundTag::new();
        root.put("Data", Tag::Compound(data));
        std::fs::write(
            world.join("level.dat"),
            mc_nbt::nbt_io::write_compressed(&root),
        )
        .unwrap();
        std::fs::create_dir_all(world.join("data")).unwrap();
        let mut score = CompoundTag::new();
        score.put("data", Tag::Compound(CompoundTag::new()));
        std::fs::write(
            world.join("data/scoreboard.dat"),
            mc_nbt::nbt_io::write_compressed(&score),
        )
        .unwrap();
        std::fs::write(
            world.join("data/villages.dat"),
            mc_nbt::nbt_io::write_compressed(&score),
        )
        .unwrap();
        std::fs::create_dir_all(world.join("stats")).unwrap();
        std::fs::write(
            world.join("stats/u.json"),
            r#"{"stats":{},"DataVersion":2230}"#,
        )
        .unwrap();
        std::fs::create_dir_all(world.join("advancements")).unwrap();
        std::fs::write(
            world.join("advancements/u.json"),
            r#"{"minecraft:story/root":{"done":true},"DataVersion":2230}"#,
        )
        .unwrap();

        let got = extract_world(world, "w", false);
        assert!(got.skipped.is_empty(), "{:?}", got.skipped);
        let rows: Vec<(&str, &str, i32)> = got
            .candidates
            .iter()
            .map(|c| (c.id.as_str(), c.ty.as_str(), c.from))
            .collect();
        assert_eq!(
            rows,
            [
                ("w-level", "LEVEL", 1343),
                ("w-level-summary", "LEVEL_SUMMARY", 1343),
                ("w-level-player", "PLAYER", -1),
                ("w-data-scoreboard", "SAVED_DATA_SCOREBOARD", 1343),
                ("w-stats-u", "STATS", 2230),
                ("w-advancements-u", "ADVANCEMENTS", 2230),
            ]
        );
        let adv = &got.candidates[5];
        let tag = mc_nbt::read_named(
            &mut adv.bytes.as_slice(),
            &mut NbtAccounter::unlimited_heap(),
        )
        .unwrap();
        assert!(!tag.contains("DataVersion") && tag.contains("minecraft:story/root"));
    }

    #[test]
    fn a_chunk_gets_the_dimension_and_generator_vanilla_hands_to_its_fixes() {
        let dir = tempfile::tempdir().unwrap();
        let world = dir.path();
        for (folder, dimension) in [("region", "overworld"), ("DIM-1/region", "the_nether")] {
            std::fs::create_dir_all(world.join(folder)).unwrap();
            let info = RegionStorageInfo::new(
                "test",
                Identifier::parse(&format!("minecraft:{dimension}")).unwrap(),
                "chunk",
            );
            let mut storage = RegionFileStorage::new(info, world.join(folder), false);
            let mut chunk = CompoundTag::new();
            chunk.put_int("DataVersion", 1343);
            storage.write(&ChunkPos::new(1, 2), Some(&chunk)).unwrap();
            storage.close().unwrap();
        }
        let context = |flat| {
            let got = extract_world(world, "w", flat);
            got.candidates
                .iter()
                .filter(|c| c.ty == "CHUNK")
                .map(|c| {
                    let tag = mc_nbt::read_named(
                        &mut c.bytes.as_slice(),
                        &mut NbtAccounter::unlimited_heap(),
                    )
                    .unwrap();
                    let ctx = tag.get_compound("__context").unwrap().clone();
                    (
                        c.from,
                        ctx.get_string("dimension").unwrap().to_string(),
                        ctx.get_string("generator").unwrap().to_string(),
                    )
                })
                .collect::<Vec<_>>()
        };
        let flat = |d: &str, g: &str| (1343, d.to_string(), g.to_string());
        assert_eq!(
            context(true),
            [
                flat("minecraft:overworld", "minecraft:flat"),
                flat("minecraft:the_nether", "minecraft:noise")
            ]
        );
        assert_eq!(
            context(false)[0],
            flat("minecraft:overworld", "minecraft:noise")
        );
    }
}
