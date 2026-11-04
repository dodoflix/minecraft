//! End-to-end: write two synthetic worlds with the real region writer and compare them through the public API.

use std::path::Path;

use mc_core::ident::Identifier;
use mc_core::pos::ChunkPos;
use mc_nbt::{CompoundTag, Tag};
use mc_world::region::{RegionFileStorage, RegionStorageInfo};
use worldcmp::compare::{status_rank, Category};
use worldcmp::world::{compare_dimension, render_json, render_text, Filter};

fn write(world: &Path, folder: &str, x: i32, z: i32, tag: &CompoundTag) {
    let dir = world.join("dimensions/minecraft/overworld").join(folder);
    let info = RegionStorageInfo::new("t", Identifier::parse("minecraft:overworld").unwrap(), folder);
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
    Filter { chunks: None, min_status: status_rank(min).unwrap(), max_listed: 10 }
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
    write(a.path(), "region", 3, 0, &chunk("minecraft:features", 3, 0, 1)); // below full in A
    write(b.path(), "region", 3, 0, &chunk("minecraft:full", 3, 0, 1));
    write(a.path(), "region", 4, 0, &chunk("minecraft:full", 4, 0, 1));
    write(b.path(), "region", 4, 0, &chunk("minecraft:full", 4, 0, 2)); // differs (DataVersion)
    write(a.path(), "entities", 0, 0, &{
        let mut e = CompoundTag::new();
        e.put("Entities", Tag::List(vec![Tag::Compound(CompoundTag::new().with("id", Tag::String("minecraft:cow".into())))].into()));
        e
    });

    let r = compare_dimension(a.path(), b.path(), "overworld", &filter("full"));
    assert_eq!((r.present_a, r.present_b, r.compared, r.only_a, r.only_b, r.skipped_status), (5, 5, 3, 1, 1, 1));
    // chunk [0,0] differs through its entity list only, [4,0] through DataVersion
    let pos: Vec<(i32, i32)> = r.differing.iter().map(|(p, _)| (p.x, p.z)).collect();
    assert_eq!(pos, vec![(0, 0), (4, 0)]);
    assert_eq!(r.identical, 1);
    assert!(r.totals.contains_key(&Category::Entities) && r.totals.contains_key(&Category::Other));
    assert!(!r.is_clean());

    let text = render_text("A", "B", std::slice::from_ref(&r), 10);
    assert!(text.contains("compared 3 (identical 1, differing 2)") && text.contains("RESULT: DIFFERENT"), "{text}");
    assert!(render_json("A", "B", &[r]).contains("\"differing\":2"));

    // --chunks restricts, a lower status threshold compares the proto chunk too
    let f = Filter { chunks: Some((4, 0, 4, 0)), ..filter("full") };
    assert_eq!(compare_dimension(a.path(), b.path(), "minecraft:overworld", &f).compared, 1);
    assert_eq!(compare_dimension(a.path(), b.path(), "overworld", &filter("features")).compared, 4);
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
