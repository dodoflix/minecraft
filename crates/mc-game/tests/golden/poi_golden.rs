//! `PoiManager` against the vanilla oracle (`oracle/PoiOracle.java` -> `fixtures/poi_oracle.txt`): scripted
//! add/remove/release/take/find/getInRange/... sequences with exact result order, village distances, the NBT of every written
//! POI chunk (byte equal) and loading the vanilla-written chunks.

use mc_core::ident::Identifier;
use mc_core::pos::{BlockPos, ChunkPos, SectionPos};
use mc_game::world::entity::ai::village::poi::{Occupancy, PoiManager, PoiRecord, PoiTypeHolder};
use mc_world::chunk::LevelHeight;
use mc_world::region::{RegionStorageInfo, SimpleRegionStorage};
use std::path::Path;

fn info() -> RegionStorageInfo {
    RegionStorageInfo::new(
        "test",
        Identifier::parse("minecraft:overworld").unwrap(),
        "poi",
    )
}

fn open(dir: &Path) -> PoiManager {
    PoiManager::new(info(), dir, true, LevelHeight::new(-64, 384))
}

fn ints(a: &[&str], k: usize) -> i32 {
    a[k].parse().unwrap()
}

fn pos_at(a: &[&str], k: usize) -> BlockPos {
    BlockPos::new(ints(a, k), ints(a, k + 1), ints(a, k + 2))
}

fn rec(r: &PoiRecord) -> String {
    let p = r.get_pos();
    format!(
        "{},{},{},{},{}",
        p.x(),
        p.y(),
        p.z(),
        r.get_poi_type().registered_name(),
        r.get_free_tickets()
    )
}

fn fmt_pos(p: Option<BlockPos>) -> String {
    p.map_or("none".to_owned(), |p| {
        format!("{},{},{}", p.x(), p.y(), p.z())
    })
}

/// Stable sort by type name (the cross-type order inside a section is an identity-hash HashMap in vanilla).
fn list(mut records: Vec<PoiRecord>) -> String {
    records.sort_by(|a, b| {
        a.get_poi_type()
            .registered_name()
            .cmp(b.get_poi_type().registered_name())
    });
    format!(
        "[{}]",
        records.iter().map(rec).collect::<Vec<_>>().join(";")
    )
}

fn occupancy(s: &str) -> Occupancy {
    match s {
        "HAS_SPACE" => Occupancy::HasSpace,
        "IS_OCCUPIED" => Occupancy::IsOccupied,
        "ANY" => Occupancy::Any,
        _ => panic!("occupancy {s}"),
    }
}

fn run(m: &PoiManager, line: &str) -> String {
    let a: Vec<&str> = line.split(' ').collect();
    let pred_of = |name: &str| {
        let name = name.to_owned();
        move |t: PoiTypeHolder| name == "*" || t.registered_name() == name
    };
    match a[0] {
        "add" => m
            .add(pos_at(&a, 1), PoiTypeHolder::by_name(a[4]).unwrap())
            .map_or("null".to_owned(), |r| rec(&r)),
        "remove" => {
            m.remove(pos_at(&a, 1));
            "void".to_owned()
        }
        "release" => m
            .release(pos_at(&a, 1))
            .map_or("ISE".to_owned(), |b| b.to_string()),
        "take" => fmt_pos(m.take(&pred_of(a[5]), &|_, _| true, pos_at(&a, 1), ints(&a, 4))),
        "range" => {
            list(m.get_in_range(&pred_of(a[6]), pos_at(&a, 1), ints(&a, 4), occupancy(a[5])))
        }
        "square" => {
            list(m.get_in_square(&pred_of(a[6]), pos_at(&a, 1), ints(&a, 4), occupancy(a[5])))
        }
        "count" => m
            .get_count_in_range(&pred_of(a[6]), pos_at(&a, 1), ints(&a, 4), occupancy(a[5]))
            .to_string(),
        "closest" => {
            fmt_pos(m.find_closest(&pred_of(a[6]), pos_at(&a, 1), ints(&a, 4), occupancy(a[5])))
        }
        "find" => fmt_pos(m.find(
            &pred_of(a[6]),
            &|_| true,
            pos_at(&a, 1),
            ints(&a, 4),
            occupancy(a[5]),
        )),
        "exists" => m.exists(pos_at(&a, 1), &pred_of(a[4])).to_string(),
        "village" => m
            .sections_to_village(SectionPos::of(ints(&a, 1), ints(&a, 2), ints(&a, 3)))
            .to_string(),
        "tick" => {
            m.tick(&|| true);
            "void".to_owned()
        }
        other => panic!("op {other}"),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

#[test]
fn poi_manager_matches_vanilla() {
    let golden = include_str!("../fixtures/poi_oracle.txt");
    let mut lines = golden.lines().peekable();
    let mut scenarios = 0;
    let mut checked = 0;
    while let Some(head) = lines.next() {
        let seed = head.strip_prefix("scenario ").expect("scenario header");
        scenarios += 1;
        let dir = tempfile::tempdir().unwrap();
        let m = open(dir.path());
        let mut chunks: Vec<(i32, i32, String)> = Vec::new();
        while let Some(&line) = lines.peek() {
            if line.starts_with("chunk ") || line == "post" {
                break;
            }
            lines.next();
            let (op, expected) = line.split_once(" => ").unwrap();
            assert_eq!(run(&m, op), expected, "scenario {seed}: `{op}`");
            checked += 1;
        }
        m.flush_all();
        m.close();
        while let Some(&line) = lines.peek() {
            let Some(rest) = line.strip_prefix("chunk ") else {
                break;
            };
            lines.next();
            let mut it = rest.splitn(3, ' ');
            chunks.push((
                it.next().unwrap().parse().unwrap(),
                it.next().unwrap().parse().unwrap(),
                it.next().unwrap().to_owned(),
            ));
        }
        // the NBT we wrote equals the NBT vanilla wrote
        let mut ours = SimpleRegionStorage::new(info(), dir.path(), true);
        let mut vanilla_dir = tempfile::tempdir().unwrap();
        let mut theirs = SimpleRegionStorage::new(info(), vanilla_dir.path(), true);
        for (x, z, expected) in &chunks {
            let tag = ours
                .read(&ChunkPos::new(*x, *z))
                .unwrap()
                .expect("chunk written");
            assert_eq!(
                hex(&mc_nbt::nbt_io::to_bytes(&tag)),
                *expected,
                "scenario {seed}: poi chunk {x},{z}"
            );
            let vanilla_tag = mc_nbt::nbt_io::read(
                &mut &unhex(expected)[..],
                &mut mc_nbt::NbtAccounter::unlimited_heap(),
            )
            .unwrap();
            theirs.write(&ChunkPos::new(*x, *z), Some(vanilla_tag));
        }
        ours.close().unwrap();
        theirs.synchronize(false).unwrap();
        theirs.close().unwrap();
        assert_eq!(lines.next(), Some("post"));
        let m2 = open(vanilla_dir.path());
        while let Some(&line) = lines.peek() {
            if line.starts_with("scenario ") {
                break;
            }
            lines.next();
            let (op, expected) = line.split_once(" => ").unwrap();
            assert_eq!(
                run(&m2, op),
                expected,
                "scenario {seed} (loaded vanilla files): `{op}`"
            );
            checked += 1;
        }
        m2.close();
        let _ = &mut vanilla_dir;
    }
    assert!(
        scenarios == 3 && checked > 3000,
        "{scenarios} scenarios, {checked} ops"
    );
}
