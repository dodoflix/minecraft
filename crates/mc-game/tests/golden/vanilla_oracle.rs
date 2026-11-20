//! Our registry loader against the recorded vanilla server (`tests/fixtures/oracle/`: the raw configuration packets and the
//! server log of three worlds: default, custom data packs, broken data pack).
//! Regenerate: `MC_UPDATE_FIXTURES=1 cargo test -p mc-game --test oracle -- --ignored regen_oracle_fixtures --nocapture`.

use crate::common;

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use mc_core::ident::Identifier;
use mc_game::registry::keys;
use mc_game::registry::synchronization::{pack_registries, serialize_tags_to_network};
use mc_nbt::Tag;
use mc_protocol::packets::configuration::*;
use mc_world::level_storage::WorldDataConfiguration;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/oracle")
}

fn read_packets(name: &str) -> Vec<ClientboundConfigurationPacket> {
    let bin = std::fs::read(fixture_dir().join(format!("{name}.bin"))).unwrap();
    let mut out = Vec::new();
    let mut rest = &bin[..];
    while !rest.is_empty() {
        let len = u32::from_be_bytes(rest[..4].try_into().unwrap()) as usize;
        out.push(ClientboundConfigurationPacket::decode_payload(&rest[4..4 + len]).unwrap());
        rest = &rest[4 + len..];
    }
    out
}

fn read_log(name: &str) -> Vec<String> {
    std::fs::read_to_string(fixture_dir().join(format!("{name}.server-log.txt")))
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

// ---- comparisons ------------------------------------------------------------------------------------------------

/// Vanilla's `registry_data` + `update_tags` of a scenario as `registry -> [(entry, nbt)]` and `registry -> tag -> ids`.
type VanillaView = (
    Vec<(String, Vec<(Identifier, Option<Tag>)>)>,
    BTreeMap<String, BTreeMap<Identifier, Vec<i32>>>,
);

fn vanilla_view(name: &str) -> VanillaView {
    let mut registries = Vec::new();
    let mut tags = BTreeMap::new();
    for p in read_packets(name) {
        match p {
            ClientboundConfigurationPacket::RegistryData(r) => registries.push((
                r.registry.to_string(),
                r.entries.into_iter().map(|e| (e.id, e.data)).collect(),
            )),
            ClientboundConfigurationPacket::UpdateTags(t) => {
                for (reg, payload) in t.tags {
                    tags.insert(
                        reg.to_string(),
                        payload.tags.into_iter().collect::<BTreeMap<_, _>>(),
                    );
                }
            }
            _ => {}
        }
    }
    (registries, tags)
}

fn compare_scenario(name: &str, setup: impl FnOnce(&Path)) -> Vec<String> {
    let _serial = common::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    common::init_logger();
    let _ = common::take_logs();
    let dir = tempfile::tempdir().unwrap();
    let world = dir.path().join("world");
    setup(&world);
    let (_, stem) =
        common::load_world(&world, WorldDataConfiguration::DEFAULT, true).expect("loads");
    let ours = common::take_logs();
    let (vanilla_registries, vanilla_tags) = vanilla_view(name);

    // registry_data: same registries in the same order, same entries with the same ids
    let access = stem.registries.composite_access();
    let packed = pack_registries(access, &HashSet::new());
    assert_eq!(
        packed.iter().map(|p| &p.0).collect::<Vec<_>>(),
        vanilla_registries.iter().map(|p| &p.0).collect::<Vec<_>>(),
        "registry order"
    );
    let typed = [
        keys::DIMENSION_TYPE,
        keys::BIOME,
        keys::DAMAGE_TYPE,
        keys::CHAT_TYPE,
        keys::ENCHANTMENT,
        keys::BLOCK_TRANSFORMER,
        keys::BLOCK_STATE_PROVIDER,
    ];
    // registries whose compounds are also compared in the order the server writes their keys (the hash order of the
    // compound, which depends on the order the codec inserted the keys in)
    let ordered = [
        keys::ENCHANTMENT,
        keys::BLOCK_TRANSFORMER,
        keys::BLOCK_STATE_PROVIDER,
    ];
    let mut approx: Vec<String> = Vec::new();
    for ((key, entries), (vkey, ventries)) in packed.iter().zip(&vanilla_registries) {
        assert_eq!(key, vkey);
        assert_eq!(
            entries.iter().map(|e| &e.id).collect::<Vec<_>>(),
            ventries.iter().map(|e| &e.0).collect::<Vec<_>>(),
            "{key}: entry names/ids"
        );
        for (e, (id, vdata)) in entries.iter().zip(ventries) {
            let same_order = !ordered.contains(&key.as_str())
                || matches!((&e.data, vdata), (Some(a), Some(b)) if same_key_order(a, b));
            if e.data == *vdata && same_order {
                continue;
            }
            if typed.contains(&key.as_str()) {
                panic!(
                    "{key} {id}: network NBT differs at {}\nours:    {:?}\nvanilla: {vdata:?}",
                    if e.data == *vdata {
                        "the key order".to_owned()
                    } else {
                        first_diff(e.data.as_ref(), vdata.as_ref(), "")
                    },
                    e.data
                );
            }
            if std::env::var("DUMP_DIFFS").is_ok_and(|v| key.contains(&v)) {
                eprintln!("{key} {id}\n ours:    {:?}\n vanilla: {vdata:?}", e.data);
            }
            approx.push(format!("{key} {id}"));
        }
    }
    let mut per_registry: BTreeMap<&str, usize> = BTreeMap::new();
    for a in &approx {
        *per_registry
            .entry(a.split(' ').next().unwrap())
            .or_default() += 1;
    }
    eprintln!(
        "[{name}] untyped registry entries whose NBT differs from vanilla's codec output: {} {per_registry:?}",
        approx.len()
    );

    // update_tags: every tag of every registry vanilla sent, same ids in the same order
    let ours_tags: BTreeMap<String, BTreeMap<Identifier, Vec<i32>>> =
        serialize_tags_to_network(&stem.registries)
            .into_iter()
            .map(|(k, p)| {
                (
                    k,
                    p.tags
                        .into_iter()
                        .map(|(t, ids)| (t, ids.into_iter().map(|i| i as i32).collect()))
                        .collect(),
                )
            })
            .collect();
    assert_eq!(
        ours_tags.keys().collect::<Vec<_>>(),
        vanilla_tags.keys().collect::<Vec<_>>(),
        "registries with tags"
    );
    for (k, v) in &vanilla_tags {
        assert_eq!(&ours_tags[k], v, "tags of {k}");
    }
    ours
}

/// `[HH:MM:SS] [Server thread/INFO]: message` -> `INFO: message` (continuation lines are dropped).
fn vanilla_messages(log: &[String]) -> Vec<String> {
    log.iter()
        .filter_map(|l| {
            let rest = l.split_once("] [")?.1;
            let (thread_level, msg) = rest.split_once("]: ")?;
            let level = thread_level.rsplit_once('/')?.1;
            Some(format!("{level}: {msg}"))
        })
        .collect()
}

fn normalize(s: &str, world: &str) -> String {
    s.replace(world, "world")
}

#[test]
fn default_world_matches_vanilla() {
    compare_scenario("default", |_| {});
    // vanilla only logs the pack selection when something is found; a default world is quiet
    let v = vanilla_messages(&read_log("default"));
    assert!(
        !v.iter()
            .any(|m| m.contains("data pack") || m.contains("Registry loading")),
        "{v:?}"
    );
}

#[test]
fn custom_packs_world_matches_vanilla() {
    let mut world_path = String::new();
    let ours = compare_scenario("custom", |w| {
        world_path = w.display().to_string();
        common::test_packs::write_custom_packs(w);
    });
    let ours: Vec<String> = ours
        .iter()
        .map(|l| normalize(l.lines().next().unwrap(), &world_path))
        .collect();
    let ours: Vec<String> = ours
        .iter()
        .map(|l| {
            l.replace("world/datapacks", "./world/datapacks")
                .replace("././", "./")
        })
        .collect();
    let wanted = |m: &String| m.contains("pack") || m.contains("Registry loading");
    let mut vanilla: Vec<String> = vanilla_messages(&read_log("custom"))
        .into_iter()
        .filter(wanted)
        .collect();
    let mut ours: Vec<String> = ours.into_iter().filter(wanted).collect();
    // Vanilla and this server list `world/datapacks` in directory order, which differs between file systems.
    ours.sort();
    vanilla.sort();
    assert_eq!(ours, vanilla);
}

#[test]
fn broken_pack_fails_like_vanilla() {
    let _serial = common::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    common::init_logger();
    let dir = tempfile::tempdir().unwrap();
    let world = dir.path().join("world");
    common::test_packs::write_broken_packs(&world);
    let err = match common::load_world(&world, WorldDataConfiguration::DEFAULT, true) {
        Err(e) => e,
        Ok(_) => panic!("vanilla refuses this world, so must we"),
    };
    let _ = common::take_logs();
    let mc_game::server::world_loader::WorldLoadError::Registries(failure) = err else {
        panic!("{err}")
    };

    // what vanilla reported: (registry, element, "Caused by" lines)
    let log = read_log("broken");
    let mut expected: Vec<(String, String, Vec<String>)> = Vec::new();
    let mut registry = String::new();
    let mut in_errors = false;
    for line in &log {
        if line.contains("Registry loading errors:") {
            in_errors = true;
        } else if in_errors {
            if let Some(r) = line.strip_prefix("> Errors in registry ") {
                registry = r.trim_end_matches(':').to_owned();
            } else if let Some(e) = line.strip_prefix(">> Errors in element ") {
                expected.push((
                    registry.clone(),
                    e.trim_end_matches(':').to_owned(),
                    Vec::new(),
                ));
            } else if let Some(c) = line.strip_prefix("Caused by: ") {
                expected.last_mut().unwrap().2.push(c.to_owned());
            } else if line.starts_with('[') {
                in_errors = false;
            }
        }
    }
    assert_eq!(expected.len(), 54);
    let ours: Vec<(String, String, Vec<String>)> = failure
        .errors
        .iter()
        .map(|((r, id), e)| {
            (
                r.clone(),
                id.to_string(),
                e.cause.iter().map(|(c, m)| format!("{c}: {m}")).collect(),
            )
        })
        .collect();
    assert_eq!(
        ours.iter().map(|e| (&e.0, &e.1)).collect::<Vec<_>>(),
        expected.iter().map(|e| (&e.0, &e.1)).collect::<Vec<_>>()
    );
    for (o, v) in ours.iter().zip(&expected) {
        if o.1.ends_with("bad_syntax") {
            assert!(
                o.2[0].starts_with("com.google.gson.JsonSyntaxException: ")
                    && v.2[0].starts_with("com.google.gson.JsonSyntaxException: ")
            );
        } else {
            // the entry of `minecraft:root` (an unbound reference) has no cause
            assert_eq!(o.2.first(), v.2.first(), "{}", o.1);
        }
    }
    // the first lines of the full report are the same shape as vanilla's
    let details = failure.full_details();
    let (registry, element, _) = &expected[0];
    assert!(details.starts_with(&format!("> Errors in registry {registry}:\n>> Errors in element {element}:\njava.lang.IllegalStateException: Failed to parse {element} from pack file/broken\nCaused by: ")));
}

/// Whether `ours` writes every compound's keys in the order they arrived in from vanilla (`vanilla` was read from the
/// wire, so its entries are in the order the server wrote them).
fn same_key_order(ours: &Tag, vanilla: &Tag) -> bool {
    match (ours, vanilla) {
        (Tag::Compound(a), Tag::Compound(b)) => {
            let a = a.iter_java_order();
            a.len() == b.len()
                && a.into_iter()
                    .zip(b.iter())
                    .all(|((ka, va), (kb, vb))| ka == kb && same_key_order(va, vb))
        }
        (Tag::List(a), Tag::List(b)) => {
            a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| same_key_order(x, y))
        }
        (a, b) => a == b,
    }
}

/// Path of the first difference between two NBT trees (debugging aid for the typed registries).
fn first_diff(a: Option<&Tag>, b: Option<&Tag>, path: &str) -> String {
    match (a, b) {
        (Some(Tag::Compound(x)), Some(Tag::Compound(y))) => {
            for (k, v) in x.iter() {
                match y.get(k) {
                    None => return format!("{path}/{k} (only ours)"),
                    Some(w) if w != v => {
                        return first_diff(Some(v), Some(w), &format!("{path}/{k}"));
                    }
                    _ => {}
                }
            }
            for (k, _) in y.iter() {
                if x.get(k).is_none() {
                    return format!("{path}/{k} (only vanilla)");
                }
            }
            path.to_owned()
        }
        (Some(Tag::List(x)), Some(Tag::List(y))) => {
            for (i, (v, w)) in x.iter().zip(y.iter()).enumerate() {
                if v != w {
                    return first_diff(Some(v), Some(w), &format!("{path}[{i}]"));
                }
            }
            format!("{path} (list length)")
        }
        (a, b) => format!("{path}: ours {a:?} vanilla {b:?}"),
    }
}

/// The `dimensions` a server stores in `world_gen_settings.dat` for a world created from a world preset whose dimensions
/// hold inline noise settings (the density function trees, material rules and every number's tag type) are the NBT
/// vanilla saved for the same preset, key for key and in the order the server writes them.
#[test]
fn stored_dimensions_match_vanilla() {
    use mc_game::server::world_dimensions::{DimensionDecoder, WorldDimensions};

    let _serial = common::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    common::init_logger();
    let dir = tempfile::tempdir().unwrap();
    let world = dir.path().join("world");
    common::test_packs::write_dimension_pack(&world);
    let (_, stem) =
        common::load_world(&world, WorldDataConfiguration::DEFAULT, true).expect("loads");
    let access = stem.registries.composite_access();
    let decoder = DimensionDecoder::new(access).unwrap();
    let preset = access
        .registry(keys::WORLD_PRESET)
        .and_then(|r| r.entry_by_key(&Identifier::read("test:custom").unwrap()))
        .expect("the world preset of the data pack");
    let ours = WorldDimensions::from_preset_json(&decoder, &preset.json)
        .unwrap_or_else(|e| panic!("{e}"))
        .to_stored();

    let saved = mc_nbt::nbt_io::read_compressed_file(
        &fixture_dir().join("world_gen_settings.dat"),
        &mut mc_nbt::NbtAccounter::unlimited_heap(),
    )
    .unwrap();
    let vanilla = saved
        .get_compound("data")
        .and_then(|data| data.get_compound("dimensions"))
        .expect("stored dimensions");
    assert_eq!(
        ours.iter_java_order()
            .into_iter()
            .map(|(k, _)| k.as_str())
            .collect::<Vec<_>>(),
        vanilla.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
        "dimensions in the order of the stored compound"
    );
    for (id, stem) in vanilla.iter() {
        let ours = ours.get(id).expect("every dimension is stored");
        assert!(
            ours == stem && same_key_order(ours, stem),
            "dimension {id} differs at {}",
            if ours == stem {
                "the key order".to_owned()
            } else {
                first_diff(Some(ours), Some(stem), "")
            }
        );
    }
    // the settings really are inline and typed like the codecs write them (`63.0` is an int, not a byte)
    assert_eq!(vanilla.len(), 9);
    let synthetic = vanilla
        .get_compound("test:synthetic")
        .and_then(|stem| stem.get_compound("generator"))
        .and_then(|generator| generator.get_compound("settings"))
        .expect("inline settings");
    assert_eq!(synthetic.get("sea_level"), Some(&Tag::Int(63)));
    assert_eq!(
        synthetic
            .get_compound("noise_router")
            .and_then(|r| r.get("vegetation")),
        Some(&Tag::Float(1.0))
    );

    // a world loaded from the stored dimensions stores the very same ones
    let reloaded = WorldDimensions::from_stored(&decoder, &ours)
        .unwrap_or_else(|e| panic!("{e}"))
        .to_stored();
    assert!(
        reloaded == ours,
        "the stored dimensions change when they are loaded"
    );
}
