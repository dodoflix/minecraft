//! `ChunkStatus` static data and string decode vs. the real vanilla class (`tests/fixtures/status/StatusGolden.java`).

use mc_world::heightmap::HeightmapType;
use mc_world::status::{ChunkStatus, ChunkType, FINAL_HEIGHTMAPS, MAX_STRUCTURE_DISTANCE};

fn status(name: &str) -> ChunkStatus {
    ChunkStatus::decode_str(name).unwrap_or_else(|| panic!("{name}"))
}

fn keys(types: &[HeightmapType]) -> String {
    types.iter().map(|t| t.serialization_key()).collect::<Vec<_>>().join(",")
}

#[test]
fn matches_vanilla_chunk_status() {
    let golden = include_str!("../fixtures/status/golden.txt");
    let (mut statuses, mut cmps, mut ins) = (0, 0, 0);
    for line in golden.lines() {
        let (kind, rest) = line.split_once([' ', '\t']).unwrap_or((line, ""));
        match kind {
            "LIST" => {
                let names: Vec<String> = ChunkStatus::get_status_list().iter().map(|s| s.name()).collect();
                assert_eq!(names.join(","), rest);
            }
            "STATUS" => {
                statuses += 1;
                let f: Vec<&str> = rest.split(' ').collect();
                let s = status(f[0]);
                assert_eq!(s.name(), f[0]);
                assert_eq!(s.index(), f[1].parse::<i32>().unwrap());
                assert_eq!(s.parent(), status(f[2]));
                assert_eq!(
                    s.chunk_type(),
                    match f[3] {
                        "PROTOCHUNK" => ChunkType::ProtoChunk,
                        "LEVELCHUNK" => ChunkType::LevelChunk,
                        other => panic!("{other}"),
                    }
                );
                assert_eq!(s.id(), f[4].parse::<u32>().unwrap());
                assert_eq!(ChunkStatus::from_id(s.id()), Some(s));
                assert_eq!(keys(s.heightmaps_after()), f[5]);
            }
            "CMP" => {
                cmps += 1;
                let f: Vec<&str> = rest.split(' ').collect();
                let a = status(f[0]);
                for (b, cell) in ChunkStatus::ALL.iter().zip(&f[1..]) {
                    let c: Vec<char> = cell.chars().collect();
                    let bit = |i: usize| c[i] == '1';
                    assert_eq!(a.is_or_after(*b), bit(0), "{a:?} {b:?}");
                    assert_eq!(a.is_after(*b), bit(1));
                    assert_eq!(a.is_or_before(*b), bit(2));
                    assert_eq!(a.is_before(*b), bit(3));
                    assert_eq!(ChunkStatus::max(a, *b).index(), c[4].to_digit(10).unwrap() as i32);
                }
            }
            "FINAL" => assert_eq!(keys(FINAL_HEIGHTMAPS), rest),
            "MAXDIST" => assert_eq!(MAX_STRUCTURE_DISTANCE, rest.parse::<i32>().unwrap()),
            "IN" => {
                ins += 1;
                let f: Vec<&str> = rest.split('\t').collect();
                let tag = mc_nbt::snbt::parse(f[0]).unwrap();
                if let Some(s) = tag.as_string() {
                    assert_eq!(ChunkStatus::by_name(s).name(), f[1], "byName({s:?})");
                } else {
                    assert_eq!(f[1], "n/a");
                }
                assert_eq!(ChunkStatus::decode(&tag).map_or("ERR".to_owned(), |s| s.name()), f[2], "CODEC {}", f[0]);
            }
            _ => panic!("bad line {line}"),
        }
    }
    assert_eq!((statuses, cmps), (10, 10));
    assert_eq!(ins, 50);
}

#[test]
fn registry_order_matches_report() {
    // ref/generated/reports/registries.json minecraft:chunk_status protocol ids
    let expected = [
        ("empty", 0),
        ("structure_starts", 1),
        ("structure_references", 2),
        ("biomes", 3),
        ("terrain", 4),
        ("features", 5),
        ("initialize_light", 6),
        ("light", 7),
        ("spawn", 8),
        ("full", 9),
    ];
    for (path, id) in expected {
        assert_eq!(ChunkStatus::from_id(id).unwrap().path(), path);
        assert_eq!(mc_data::registry::registries::CHUNK_STATUS.id_of(&format!("minecraft:{path}")), Some(id));
    }
    assert_eq!(ChunkStatus::from_id(10), None);
    assert_eq!(ChunkStatus::Empty.parent(), ChunkStatus::Empty);
    assert_eq!(ChunkStatus::Full.to_string(), "minecraft:full");
}
