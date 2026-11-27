//! End-to-end: write two synthetic worlds with the real region writer and compare them through the public API.

use std::path::Path;

use mc_core::ident::Identifier;
use mc_core::pos::ChunkPos;
use mc_nbt::{CompoundTag, Tag};
use mc_world::region::{RegionFileStorage, RegionStorageInfo};
use worldcmp::compare::{Category, status_rank};
use worldcmp::fields::Ignores;
use worldcmp::tree::{Kind, TreeOptions, compare_trees};
use worldcmp::world::{Filter, compare_dimension, render_json, render_text};

fn write(world: &Path, folder: &str, x: i32, z: i32, tag: &CompoundTag) {
    let dir = world.join("dimensions/minecraft/overworld").join(folder);
    let info = RegionStorageInfo::new(
        "t",
        Identifier::parse("minecraft:overworld").unwrap(),
        folder,
    );
    let mut s = RegionFileStorage::new(info, dir, false);
    s.write(&ChunkPos::new(x, z), Some(tag)).unwrap();
    s.close().unwrap();
}

fn chunk(status: &str, x: i32, z: i32, tag: i32) -> CompoundTag {
    let mut c = CompoundTag::new();
    c.put_string("Status", status);
    c.put_int("xPos", x);
    c.put_int("zPos", z);
    c.put_int("DataVersion", tag);
    c
}

fn filter(min: &str) -> Filter {
    Filter {
        chunks: None,
        min_status: status_rank(min).unwrap(),
        max_listed: 10,
        ignore: vec![],
        exact: false,
        ignores: Ignores::default(),
    }
}

#[test]
fn compares_regions_and_classifies_chunks() {
    let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    for (w, dv) in [(a.path(), 1), (b.path(), 1)] {
        write(w, "region", 0, 0, &chunk("minecraft:full", 0, 0, dv));
        write(w, "region", 33, -1, &chunk("minecraft:full", 33, -1, dv)); // other region file
    }
    write(a.path(), "region", 1, 0, &chunk("minecraft:full", 1, 0, 1)); // only in A
    write(b.path(), "region", 2, 0, &chunk("minecraft:full", 2, 0, 1)); // only in B
    write(
        a.path(),
        "region",
        3,
        0,
        &chunk("minecraft:features", 3, 0, 1),
    ); // below full in A
    write(b.path(), "region", 3, 0, &chunk("minecraft:full", 3, 0, 1));
    write(a.path(), "region", 4, 0, &chunk("minecraft:full", 4, 0, 1));
    write(b.path(), "region", 4, 0, &chunk("minecraft:full", 4, 0, 2)); // differs (DataVersion)
    write(a.path(), "entities", 0, 0, &{
        let mut e = CompoundTag::new();
        e.put(
            "Entities",
            Tag::List(
                vec![Tag::Compound(
                    CompoundTag::new().with("id", Tag::String("minecraft:cow".into())),
                )]
                .into(),
            ),
        );
        e
    });

    let r = compare_dimension(a.path(), b.path(), "overworld", &filter("full"));
    assert_eq!(
        (
            r.present_a,
            r.present_b,
            r.compared,
            r.only_a,
            r.only_b,
            r.skipped_status
        ),
        (5, 5, 3, 1, 1, 1)
    );
    // chunk [0,0] differs through its entity list only, [4,0] through DataVersion
    let pos: Vec<(i32, i32)> = r.differing.iter().map(|(p, _)| (p.x, p.z)).collect();
    assert_eq!(pos, vec![(0, 0), (4, 0)]);
    assert_eq!(r.identical, 1);
    assert!(r.totals.contains_key(&Category::Entities) && r.totals.contains_key(&Category::Other));
    assert!(!r.is_clean());

    let text = render_text("A", "B", std::slice::from_ref(&r), 10);
    assert!(
        text.contains("compared 3 (identical 1, differing 2)")
            && text.contains("RESULT: DIFFERENT"),
        "{text}"
    );
    assert!(render_json("A", "B", &[r]).contains("\"differing\":2"));

    // --chunks restricts, a lower status threshold compares the proto chunk too
    let f = Filter {
        chunks: Some((4, 0, 4, 0)),
        ..filter("full")
    };
    assert_eq!(
        compare_dimension(a.path(), b.path(), "minecraft:overworld", &f).compared,
        1
    );
    assert_eq!(
        compare_dimension(a.path(), b.path(), "overworld", &filter("features")).compared,
        4
    );
}

#[test]
fn identical_worlds_are_clean_and_missing_dimension_is_empty() {
    let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    for w in [a.path(), b.path()] {
        write(w, "region", -1, -1, &chunk("minecraft:full", -1, -1, 1));
    }
    let r = compare_dimension(a.path(), b.path(), "overworld", &filter("full"));
    assert!(r.is_clean() && r.compared == 1 && r.identical == 1);
    let r = compare_dimension(a.path(), b.path(), "end", &filter("full"));
    assert!(r.is_clean() && r.compared == 0 && r.dim == "minecraft:the_end");
}

#[test]
fn ignored_categories_are_dropped_and_reported() {
    let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    write(a.path(), "region", 4, 0, &chunk("minecraft:full", 4, 0, 1));
    write(b.path(), "region", 4, 0, &chunk("minecraft:full", 4, 0, 2)); // DataVersion -> Other
    let f = Filter {
        ignore: vec![Category::from_name("other").unwrap()],
        ..filter("full")
    };
    let r = compare_dimension(a.path(), b.path(), "overworld", &f);
    assert!(r.is_clean() && r.identical == 1);
    assert!(
        render_text("A", "B", std::slice::from_ref(&r), 10).contains("ignored categories: other")
    );
    assert!(render_json("A", "B", &[r]).contains("\"ignored\":[\"other\"]"));
    assert!(Category::from_name("nope").is_none());
}

// ---------------------------------------------------------------------------------------------------------------------
// --exact and --tree

fn exact_filter() -> Filter {
    Filter {
        exact: true,
        min_status: 0,
        ..filter("empty")
    }
}

/// Two keys that share a `HashMap` bucket (16-slot table), so their insertion order decides the written key order.
fn colliding_keys() -> (String, String) {
    let bucket = |k: &str| {
        let h = k
            .chars()
            .fold(0i32, |h, c| h.wrapping_mul(31).wrapping_add(c as i32));
        (h ^ ((h as u32) >> 16) as i32) & 15
    };
    let keys: Vec<String> = (0..64).map(|i| format!("k{i}")).collect();
    for a in &keys {
        for b in &keys {
            if a != b && bucket(a) == bucket(b) {
                return (a.clone(), b.clone());
            }
        }
    }
    unreachable!("64 keys cannot all fall into different buckets of 16")
}

fn opts(ignores: &[&str]) -> TreeOptions {
    TreeOptions {
        ignores: Ignores(ignores.iter().map(|s| s.parse().unwrap()).collect()),
        chunks: None,
        max_listed: 10,
    }
}

/// A small world: `level.dat`, saved data, JSON, a region file with entities and poi, plus run-dependent files.
fn build_world(root: &Path) {
    let level = CompoundTag::new().with(
        "Data",
        CompoundTag::new()
            .with("LastPlayed", 5i64)
            .with("Time", 7i64)
            .with("Version", 1i32),
    );
    std::fs::create_dir_all(root.join("data")).unwrap();
    std::fs::create_dir_all(root.join("datapacks")).unwrap();
    std::fs::create_dir_all(root.join("logs")).unwrap();
    mc_nbt::nbt_io::write_compressed_file(&level, &root.join("level.dat")).unwrap();
    mc_nbt::nbt_io::write_compressed_file(
        &CompoundTag::new().with("data", CompoundTag::new().with("ticks", 9i64)),
        &root.join("data/clock.dat"),
    )
    .unwrap();
    std::fs::write(root.join("usercache.json"), r#"[{"name":"a","uuid":"1"}]"#).unwrap();
    std::fs::write(root.join("notes.txt"), "same").unwrap();
    std::fs::write(root.join("session.lock"), "x").unwrap();
    std::fs::write(root.join("logs/latest.log"), "start").unwrap();
    write(root, "region", 1, 2, &chunk("minecraft:empty", 1, 2, 5));
    write(root, "entities", 1, 2, &chunk("minecraft:full", 1, 2, 5));
    write(root, "poi", 1, 2, &chunk("minecraft:full", 1, 2, 5));
}

fn two_worlds() -> (tempfile::TempDir, tempfile::TempDir) {
    let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    build_world(a.path());
    build_world(b.path());
    (a, b)
}

fn run(args: &[&str]) -> (i32, String) {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_worldcmp"))
        .args(args)
        .output()
        .unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

#[test]
fn identical_trees_exit_zero_in_every_mode() {
    let (a, b) = two_worlds();
    let r = compare_trees(a.path(), b.path(), &opts(&[]));
    assert!(r.is_clean() && r.identical == 10, "{r:?}");
    let (pa, pb) = (a.path().to_str().unwrap(), b.path().to_str().unwrap());
    assert_eq!(run(&[pa, pb, "--tree"]).0, 0);
    let (code, text) = run(&[pa, pb, "--exact", "--dim", "overworld"]);
    assert_eq!(code, 0, "{text}");
    // the region chunk is below `full`: exact mode still compares it
    assert!(text.contains("compared 1 (identical 1"), "{text}");
}

#[test]
fn flipped_byte_in_a_dat_file_is_reported_with_its_path() {
    let (a, b) = two_worlds();
    let p = b.path().join("data/clock.dat");
    let mut bytes = std::fs::read(&p).unwrap();
    let mid = bytes.len() / 2;
    bytes[mid] ^= 0x01;
    std::fs::write(&p, bytes).unwrap();
    let r = compare_trees(a.path(), b.path(), &opts(&[]));
    assert_eq!(r.entries.len(), 1, "{r:?}");
    assert_eq!(
        (r.entries[0].path.as_str(), r.entries[0].kind),
        ("data/clock.dat", Kind::Differs)
    );
    let (code, text) = run(&[
        a.path().to_str().unwrap(),
        b.path().to_str().unwrap(),
        "--tree",
    ]);
    assert!(code == 1 && text.contains("data/clock.dat"), "{text}");
}

#[test]
fn a_decoded_nbt_difference_names_the_field() {
    let (a, b) = two_worlds();
    let tag = CompoundTag::new().with(
        "Data",
        CompoundTag::new()
            .with("LastPlayed", 6i64)
            .with("Time", 7i64)
            .with("Version", 1i32),
    );
    mc_nbt::nbt_io::write_compressed_file(&tag, &b.path().join("level.dat")).unwrap();
    let r = compare_trees(a.path(), b.path(), &opts(&[]));
    assert_eq!(r.entries.len(), 1);
    assert!(
        r.entries[0].samples[0].starts_with("Data.LastPlayed: A 5L B 6L"),
        "{r:?}"
    );
    // the same difference vanishes when the field is ignored, and only that field
    assert!(compare_trees(a.path(), b.path(), &opts(&["level.dat#Data.LastPlayed"])).is_clean());
    let r = compare_trees(a.path(), b.path(), &opts(&["level.dat#Data.Time"]));
    assert_eq!(r.entries.len(), 1);
}

#[test]
fn extra_missing_and_empty_directory_entries_are_reported() {
    let (a, b) = two_worlds();
    std::fs::write(b.path().join("extra.bin"), [1u8]).unwrap();
    std::fs::remove_file(a.path().join("notes.txt")).unwrap();
    std::fs::remove_dir(b.path().join("datapacks")).unwrap();
    std::fs::write(a.path().join("data/clock.txt"), "1").unwrap();
    std::fs::write(b.path().join("data/clock.txt"), "2").unwrap();
    let r = compare_trees(a.path(), b.path(), &opts(&[]));
    let got: Vec<(&str, Kind)> = r
        .entries
        .iter()
        .map(|e| (e.path.as_str(), e.kind))
        .collect();
    assert_eq!(
        got,
        vec![
            ("data/clock.txt", Kind::Differs),
            ("datapacks/", Kind::OnlyA),
            ("extra.bin", Kind::OnlyB),
            ("notes.txt", Kind::OnlyB),
        ]
    );
    assert!(r.entries[0].samples[0].starts_with("content differs: A 1 bytes sha256"));
}

#[test]
fn run_dependent_files_are_ignored_by_flags_only() {
    let (a, b) = two_worlds();
    std::fs::write(b.path().join("session.lock"), "y").unwrap();
    std::fs::write(b.path().join("logs/latest.log"), "other").unwrap();
    std::fs::write(b.path().join("logs/old.log"), "other").unwrap();
    assert_eq!(
        compare_trees(a.path(), b.path(), &opts(&[])).entries.len(),
        3
    );
    let r = compare_trees(a.path(), b.path(), &opts(&["session.lock", "logs/"]));
    assert!(r.is_clean(), "{r:?}");
    let (pa, pb) = (a.path().to_str().unwrap(), b.path().to_str().unwrap());
    assert_eq!(
        run(&[
            pa,
            pb,
            "--tree",
            "--ignore-field",
            "session.lock",
            "--ignore-field",
            "logs/"
        ])
        .0,
        0
    );
    assert_eq!(run(&[pa, pb, "--tree"]).0, 1);
}

#[test]
fn json_is_compared_structurally() {
    let (a, b) = two_worlds();
    std::fs::write(a.path().join("ops.json"), r#"{"x":1,"y":[1,2]}"#).unwrap();
    std::fs::write(
        b.path().join("ops.json"),
        "{\n  \"y\": [1, 2],\n  \"x\": 1\n}",
    )
    .unwrap();
    assert!(compare_trees(a.path(), b.path(), &opts(&[])).is_clean());
    std::fs::write(b.path().join("ops.json"), r#"{"y":[1,3],"x":1}"#).unwrap();
    let r = compare_trees(a.path(), b.path(), &opts(&[]));
    assert_eq!(r.entries[0].samples, vec!["y[1]: A 2 B 3".to_owned()]);
    std::fs::write(b.path().join("ops.json"), "{not json").unwrap();
    let r = compare_trees(a.path(), b.path(), &opts(&[]));
    assert!(
        r.entries[0].samples[0].starts_with("B does not decode"),
        "{r:?}"
    );
}

#[test]
fn changed_chunk_key_order_is_reported_by_tree_and_exact() {
    let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (k1, k2) = colliding_keys();
    for (w, order) in [(a.path(), [&k1, &k2]), (b.path(), [&k2, &k1])] {
        let mut c = chunk("minecraft:full", 3, 4, 1);
        let mut inner = CompoundTag::new();
        for k in order {
            inner.put_int(k.as_str(), 1);
        }
        c.put("nested", inner);
        write(w, "region", 3, 4, &c);
    }
    let r = compare_trees(a.path(), b.path(), &opts(&[]));
    let e = &r.entries[0];
    assert_eq!(e.path, "dimensions/minecraft/overworld/region/r.0.0.mca");
    assert!(
        e.samples[0].starts_with("chunk [3,4]: 1 differences, first nested: key order differs"),
        "{e:?}"
    );

    let r = compare_dimension(a.path(), b.path(), "overworld", &exact_filter());
    assert_eq!((r.compared, r.identical, r.differing.len()), (1, 0, 1));
    let msg = &r.differing[0].1.cats[&Category::Other].samples[0];
    assert!(
        msg.starts_with("region: nested: key order differs"),
        "{msg}"
    );
    // the semantic default mode cannot see it
    assert!(compare_dimension(a.path(), b.path(), "overworld", &filter("full")).is_clean());
}

#[test]
fn exact_compares_entities_and_poi_and_applies_field_ignores() {
    let (a, b) = two_worlds();
    write(b.path(), "poi", 1, 2, &chunk("minecraft:full", 1, 2, 6));
    write(
        b.path(),
        "entities",
        8,
        8,
        &chunk("minecraft:full", 8, 8, 5),
    );
    let r = compare_dimension(a.path(), b.path(), "overworld", &exact_filter());
    let mut got: Vec<String> = r
        .differing
        .iter()
        .flat_map(|(p, d)| {
            d.cats[&Category::Other]
                .samples
                .iter()
                .map(move |m| format!("[{},{}] {m}", p.x, p.z))
        })
        .collect();
    got.sort();
    assert_eq!(
        got,
        vec![
            "[1,2] poi: DataVersion: A 5 B 6",
            "[8,8] entities: only in B"
        ]
    );
    assert_eq!((r.compared, r.identical), (1, 0));

    let f = Filter {
        ignores: Ignores(vec![
            "dimensions/minecraft/overworld/poi/*#DataVersion"
                .parse()
                .unwrap(),
            "dimensions/minecraft/overworld/entities/r.0.0.mca"
                .parse()
                .unwrap(),
        ]),
        ..exact_filter()
    };
    let r = compare_dimension(a.path(), b.path(), "overworld", &f);
    assert!(r.is_clean() && r.identical == 1, "{r:?}");
}

#[test]
fn usage_errors_exit_two() {
    let (a, b) = two_worlds();
    let (pa, pb) = (a.path().to_str().unwrap(), b.path().to_str().unwrap());
    for extra in [
        &["--exact", "--tree"][..],
        &["--tree", "--dim", "end"],
        &["--exact", "--ignore", "blocks"],
        &["--exact", "--ignore-status-below", "full"],
        &["--ignore-field", "a#b..c"],
        &["--exact", "--ignore-field", "a#b..c"],
    ] {
        assert_eq!(run(&[&[pa, pb][..], extra].concat()).0, 2, "{extra:?}");
    }
}

#[test]
fn only_canonical_region_names_are_read_as_regions() {
    let (a, b) = two_worlds();
    std::fs::write(a.path().join("r.0.0.old.mca"), [1u8]).unwrap();
    std::fs::write(b.path().join("r.0.0.old.mca"), [2u8]).unwrap();
    let r = compare_trees(a.path(), b.path(), &opts(&[]));
    assert_eq!(r.entries.len(), 1);
    assert!(
        r.entries[0].samples[0].starts_with("content differs"),
        "{r:?}"
    );
}

#[test]
fn nbt_files_must_end_after_the_root_and_have_an_empty_root_name() {
    let (a, b) = two_worlds();
    let level = mc_nbt::nbt_io::read_compressed_file(
        &a.path().join("level.dat"),
        &mut mc_nbt::NbtAccounter::unlimited_heap(),
    )
    .unwrap();
    let mut raw = mc_nbt::nbt_io::to_bytes(&level);
    raw.push(0);
    std::fs::write(
        b.path().join("level.dat"),
        mc_nbt::compress::gzip_compress(&raw),
    )
    .unwrap();
    let r = compare_trees(a.path(), b.path(), &opts(&[]));
    assert!(
        r.entries[0].samples[0].contains("B does not decode: 1 trailing bytes"),
        "{r:?}"
    );
    let mut named = mc_nbt::nbt_io::to_bytes(&level);
    named.splice(1..3, [0, 1, b'x']);
    std::fs::write(
        b.path().join("level.dat"),
        mc_nbt::compress::gzip_compress(&named),
    )
    .unwrap();
    let r = compare_trees(a.path(), b.path(), &opts(&[]));
    assert!(
        r.entries[0].samples[0].contains("root name is not empty"),
        "{r:?}"
    );
}

#[test]
fn an_unreadable_chunk_record_is_an_error_not_an_absent_chunk() {
    let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    write(a.path(), "region", 0, 0, &chunk("minecraft:full", 0, 0, 1));
    let rel = "dimensions/minecraft/overworld/region/r.0.0.mca";
    std::fs::create_dir_all(b.path().join(rel).parent().unwrap()).unwrap();
    std::fs::copy(a.path().join(rel), b.path().join(rel)).unwrap();
    let mut bytes = std::fs::read(b.path().join(rel)).unwrap();
    bytes[8192..8196].fill(0); // the record's length field
    std::fs::write(b.path().join(rel), bytes).unwrap();
    let r = compare_dimension(a.path(), b.path(), "overworld", &exact_filter());
    assert!(
        !r.is_clean() && r.errors[0].contains("unreadable in B"),
        "{r:?}"
    );
    let t = compare_trees(a.path(), b.path(), &opts(&[]));
    assert!(!t.is_clean() && t.identical == 0, "{t:?}");
}

#[test]
fn a_directory_left_empty_by_the_ignores_counts_as_empty() {
    let (a, b) = two_worlds();
    std::fs::create_dir(a.path().join("foo")).unwrap();
    std::fs::write(a.path().join("foo/x.lock"), "1").unwrap();
    std::fs::create_dir(b.path().join("foo")).unwrap();
    assert!(compare_trees(a.path(), b.path(), &opts(&["**/*.lock"])).is_clean());
    std::fs::remove_dir(b.path().join("foo")).unwrap();
    let r = compare_trees(a.path(), b.path(), &opts(&["**/*.lock"]));
    assert_eq!(
        (r.entries[0].path.as_str(), r.entries[0].kind),
        ("foo/", Kind::OnlyA)
    );
}

#[test]
fn damaged_region_headers_are_not_taken_for_empty_regions() {
    let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let rel = "dimensions/minecraft/overworld/region/r.0.0.mca";
    for (w, byte) in [(a.path(), 1u8), (b.path(), 2u8)] {
        std::fs::create_dir_all(w.join(rel).parent().unwrap()).unwrap();
        std::fs::write(w.join(rel), [byte]).unwrap(); // a header entry the region reader zeroes
    }
    let r = compare_dimension(a.path(), b.path(), "overworld", &exact_filter());
    assert!(
        !r.is_clean() && r.errors[0].contains("unreadable in A and B"),
        "{r:?}"
    );
    assert!(!compare_trees(a.path(), b.path(), &opts(&[])).is_clean());
}

#[test]
fn equal_tags_with_different_payload_bytes_differ_unless_a_field_was_removed() {
    let (a, b) = two_worlds();
    let level = |v: f32| CompoundTag::new().with("f", Tag::Float(v)).with("n", 1i32);
    let (neg, pos) = (level(-0.0), level(0.0));
    mc_nbt::nbt_io::write_compressed_file(&neg, &a.path().join("level.dat")).unwrap();
    mc_nbt::nbt_io::write_compressed_file(&pos, &b.path().join("level.dat")).unwrap();
    let r = compare_trees(a.path(), b.path(), &opts(&[]));
    assert!(
        r.entries[0].samples[0].contains("equal tags, different payload bytes"),
        "{r:?}"
    );
    // a removed field makes the payloads incomparable, the tags decide
    assert!(compare_trees(a.path(), b.path(), &opts(&["level.dat#n"])).is_clean());
    for (w, t) in [(a.path(), &neg), (b.path(), &pos)] {
        let mut c = chunk("minecraft:full", 0, 0, 1);
        c.put("f", t.get("f").unwrap().clone());
        write(w, "region", 0, 0, &c);
    }
    let r = compare_dimension(a.path(), b.path(), "overworld", &exact_filter());
    assert_eq!(r.differing.len(), 1, "{r:?}");
}

#[test]
fn gzip_against_raw_nbt_and_out_of_range_region_names_differ() {
    let (a, b) = two_worlds();
    let level = mc_nbt::nbt_io::read_compressed_file(
        &a.path().join("level.dat"),
        &mut mc_nbt::NbtAccounter::unlimited_heap(),
    )
    .unwrap();
    std::fs::write(b.path().join("level.dat"), mc_nbt::nbt_io::to_bytes(&level)).unwrap();
    let r = compare_trees(a.path(), b.path(), &opts(&[]));
    assert_eq!(
        r.entries[0].samples,
        vec!["gzip on one side only".to_owned()]
    );
    std::fs::write(a.path().join("r.134217728.0.mca"), [1u8]).unwrap();
    std::fs::write(b.path().join("r.134217728.0.mca"), [2u8]).unwrap();
    let r = compare_trees(a.path(), b.path(), &opts(&[]));
    assert!(
        r.entries.iter().any(|e| e.path == "r.134217728.0.mca"),
        "{r:?}"
    );
}

#[test]
fn chunk_digest_follows_the_exact_comparison() {
    let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let with = |light: i32, blocks: i32| {
        let mut c = chunk("minecraft:features", 3, 0, 1);
        c.put_int("light", light);
        c.put_int("blocks", blocks);
        c
    };
    write(a.path(), "region", 3, 0, &with(1, 5));
    write(b.path(), "region", 3, 0, &with(2, 5));
    let digest = |world: &Path, ignores: &[&str]| {
        let f = Filter {
            chunks: None,
            min_status: 0,
            max_listed: 10,
            ignore: vec![],
            exact: true,
            ignores: Ignores(ignores.iter().map(|s| s.parse().unwrap()).collect()),
        };
        worldcmp::world::chunk_digest(world, "overworld", ChunkPos::new(3, 0), &f).unwrap()
    };
    let skip = ["dimensions/minecraft/overworld/region/*.mca#light"];
    assert_ne!(digest(a.path(), &[]), digest(b.path(), &[]));
    assert_eq!(digest(a.path(), &skip), digest(b.path(), &skip));
    write(b.path(), "region", 3, 0, &with(2, 6));
    assert_ne!(digest(a.path(), &skip), digest(b.path(), &skip));
}
