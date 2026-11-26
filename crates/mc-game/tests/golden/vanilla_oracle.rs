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
        keys::INSTRUMENT,
        keys::JUKEBOX_SONG,
        keys::BLOCK_TRANSFORMER,
        keys::BLOCK_STATE_PROVIDER,
        keys::TEST_ENVIRONMENT,
        keys::TEST_INSTANCE,
        keys::INSTRUMENT,
        keys::JUKEBOX_SONG,
        keys::SULFUR_CUBE_ARCHETYPE,
        keys::TIMELINE,
    ];
    // registries whose compounds are also compared in the order the server writes their keys (the hash order of the
    // compound, which depends on the order the codec inserted the keys in)
    let ordered = [
        keys::ENCHANTMENT,
        keys::BLOCK_TRANSFORMER,
        keys::BLOCK_STATE_PROVIDER,
        keys::TEST_ENVIRONMENT,
        keys::TEST_INSTANCE,
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
    let wanted = |m: &String| {
        m.contains("pack") || m.contains("Registry loading") || m.contains("Spawn data: ")
    };
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

/// Identity hash codes (`Class@1b2c3d`) differ between two vanilla runs: both sides are compared with them masked.
fn mask_identity_hashes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('@') {
        out.push_str(&rest[..=at]);
        let digits = rest[at + 1..]
            .find(|c: char| !c.is_ascii_hexdigit())
            .unwrap_or(rest.len() - at - 1);
        out.push('0');
        rest = &rest[at + 1 + digits..];
    }
    out.push_str(rest);
    out
}

/// What vanilla reported when a world's registries failed to load: (registry, element, "Caused by" lines).
fn vanilla_load_errors(log: &[String]) -> Vec<(String, String, Vec<String>)> {
    let mut expected: Vec<(String, String, Vec<String>)> = Vec::new();
    let mut registry = String::new();
    let mut in_errors = false;
    for line in log {
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
    expected
}

/// Loads the world `write` creates (which vanilla refused: `scenario` is its recorded log with `count` failed entries) and
/// compares the entries that fail, their order and the first cause of each with vanilla's.
fn compare_load_failure(scenario: &str, write: fn(&Path), pack: &str, count: usize) {
    let _serial = common::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    common::init_logger();
    let dir = tempfile::tempdir().unwrap();
    let world = dir.path().join("world");
    write(&world);
    let err = match common::load_world(&world, WorldDataConfiguration::DEFAULT, true) {
        Err(e) => e,
        Ok(_) => panic!("vanilla refuses this world, so must we"),
    };
    let _ = common::take_logs();
    let mc_game::server::world_loader::WorldLoadError::Registries(failure) = err else {
        panic!("{err}")
    };

    let expected = vanilla_load_errors(&read_log(scenario));
    assert_eq!(expected.len(), count);
    let ours: Vec<(String, String, Vec<String>)> = failure
        .errors
        .iter()
        .map(|((r, id), e)| {
            (
                r.clone(),
                id.to_string(),
                e.causes.iter().map(|(c, m)| format!("{c}: {m}")).collect(),
            )
        })
        .collect();
    assert_eq!(
        ours.iter().map(|e| (&e.0, &e.1)).collect::<Vec<_>>(),
        expected.iter().map(|e| (&e.0, &e.1)).collect::<Vec<_>>()
    );
    let mut differing = Vec::new();
    for (o, v) in ours.iter().zip(&expected) {
        // the entry of `minecraft:root` (an unbound reference) has no cause; a multi-line message is compared by its
        // first line (the log prints the rest as plain lines)
        let first = |e: &(String, String, Vec<String>)| {
            e.2.iter()
                .map(|cause| mask_identity_hashes(cause.lines().next().unwrap_or("")))
                .collect::<Vec<_>>()
        };
        if first(o) != first(v) {
            differing.push(format!(
                "{}\n  ours:    {:?}\n  vanilla: {:?}",
                o.1,
                first(o),
                first(v)
            ));
        }
    }
    assert!(
        differing.is_empty(),
        "{} of {count} entries differ:\n{}",
        differing.len(),
        differing.join("\n")
    );
    // the first lines of the full report are the same shape as vanilla's
    let details = failure.full_details();
    let (registry, element, _) = &expected[0];
    assert!(details.starts_with(&format!("> Errors in registry {registry}:\n>> Errors in element {element}:\njava.lang.IllegalStateException: Failed to parse {element} from pack file/{pack}\nCaused by: ")));
}

#[test]
fn broken_pack_fails_like_vanilla() {
    compare_load_failure(
        "broken",
        common::test_packs::write_broken_packs,
        "broken",
        138,
    );
}

/// Inline noise settings with errors in every codec of the tree (density functions, noise holders, anchors, splines,
/// material rules, spawn targets, settings): the dimension stage reports vanilla's messages.
#[test]
fn broken_dimensions_fail_like_vanilla() {
    compare_load_failure(
        "broken_dimensions",
        common::test_packs::write_broken_dimension_packs,
        "broken_dimensions",
        98,
    );
}

/// Number spellings beyond `f64` in an int field (`1e10000`, `1e400`, a 10001-digit literal): the entries that fail, with
/// every cause line, are vanilla's.
#[test]
fn broken_numbers_fail_like_vanilla() {
    compare_load_failure(
        "broken_numbers",
        common::test_packs::write_broken_number_packs,
        "broken_numbers",
        7,
    );
}

/// The same spellings in float fields and fractional ones in int fields load, with the values vanilla sends.
#[test]
fn number_edges_load_like_vanilla() {
    compare_scenario("numbers", common::test_packs::write_number_edge_packs);
}

/// Timelines whose whole-number fields are fractions load with the truncated values vanilla sends.
#[test]
fn custom_timelines_load_like_vanilla() {
    compare_scenario("timelines", common::test_packs::write_timeline_packs);
}

/// The period bounds of a timeline are checked on the truncated ints.
#[test]
fn broken_timelines_fail_like_vanilla() {
    compare_load_failure(
        "broken_timelines",
        common::test_packs::write_broken_timeline_packs,
        "broken_timelines",
        3,
    );
}

/// References that are no plain identifier string (`RegistryFixedCodec`: `Not a string: 5`) and flags that are no JSON boolean
/// (`Not a boolean: 3`) fail like vanilla's codecs; a missing element is an unbound value of the root registry.
#[test]
fn broken_clock_references_fail_like_vanilla() {
    compare_load_failure(
        "broken_clocks",
        common::test_packs::write_broken_clock_packs,
        "broken_clocks",
        11,
    );
}

/// The lines of the WARN entries of a recorded vanilla log: an entry starts at a `[time] [thread/LEVEL]: text` line and
/// runs over the lines that follow; `Caused by` lines are not compared (ours prints no stack traces).
fn vanilla_warnings(log: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut warn = false;
    for line in log {
        if let Some((thread_level, text)) = line
            .split_once("] [")
            .and_then(|(_, rest)| rest.split_once("]: "))
        {
            warn = thread_level.ends_with("/WARN");
            if warn {
                out.push(format!("WARN: {text}"));
            }
        } else if warn && !line.starts_with("Caused by: ") {
            out.push(line.clone());
        }
    }
    out
}

/// A loot table that references itself: the registries load, then the loot table validation throws. Every problem is
/// logged, then the start fails with the `IllegalStateException` of the validation.
#[test]
fn recursive_loot_table_fails_the_start_like_vanilla() {
    let _serial = common::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    common::init_logger();
    let _ = common::take_logs();
    let dir = tempfile::tempdir().unwrap();
    let world = dir.path().join("world");
    common::test_packs::write_recursive_loot_packs(&world);
    let Err(err) = common::load_world(&world, WorldDataConfiguration::DEFAULT, true) else {
        panic!("vanilla refuses this world, so must we");
    };
    err.log();
    let ours: Vec<String> = common::take_logs()
        .iter()
        .filter(|entry| entry.starts_with("WARN: "))
        .flat_map(|entry| entry.lines().map(str::to_owned))
        .collect();
    assert_eq!(ours, vanilla_warnings(&read_log("recursive_loot")));
}

/// Numbers beyond `f64` and exponent spellings of ints in int providers, attributes, the dimension type, biome climate,
/// jukebox songs and instruments (hand-written readers and custom number codecs): the entries that load.
#[test]
fn number_fields_load_like_vanilla() {
    compare_scenario(
        "number_fields",
        common::test_packs::write_number_field_packs,
    );
}

/// The same fields with numbers their codecs refuse: the entries that fail, with every cause line, are vanilla's.
#[test]
fn broken_number_fields_fail_like_vanilla() {
    compare_load_failure(
        "broken_number_fields",
        common::test_packs::write_broken_number_field_packs,
        "broken_number_fields",
        21,
    );
}

/// Numbers beyond `f64` and exponent spellings of ints in density functions, noise parameters, vertical anchors, material
/// rules and spawn targets of inline noise settings.
#[test]
fn broken_dimension_numbers_fail_like_vanilla() {
    compare_load_failure(
        "broken_dimension_numbers",
        common::test_packs::write_broken_dimension_number_packs,
        "broken_dimension_numbers",
        14,
    );
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
                match y.get(k.as_lossy_str()) {
                    None => return format!("{path}/{0} (only ours)", k.as_lossy_str()),
                    Some(w) if w != v => {
                        return first_diff(
                            Some(v),
                            Some(w),
                            &format!("{path}/{0}", k.as_lossy_str()),
                        );
                    }
                    _ => {}
                }
            }
            for (k, _) in y.iter() {
                if x.get(k.as_lossy_str()).is_none() {
                    return format!("{path}/{0} (only vanilla)", k.as_lossy_str());
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
            .map(|(k, _)| k.as_lossy_str())
            .collect::<Vec<_>>(),
        vanilla
            .iter()
            .map(|(k, _)| k.as_lossy_str())
            .collect::<Vec<_>>(),
        "dimensions in the order of the stored compound"
    );
    for (id, stem) in vanilla.iter() {
        let ours = ours
            .get(id.as_lossy_str())
            .expect("every dimension is stored");
        assert!(
            ours == stem && same_key_order(ours, stem),
            "dimension {} differs at {}",
            id.as_lossy_str(),
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
