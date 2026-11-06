//! Chunk + light packet data encoding vs the real vanilla `ClientboundLevelChunkWithLightPacket.STREAM_CODEC`
//! (`fixtures/chunk_packet/ChunkPacketGolden.java`).

use mc_data::BlockEntityType;
use mc_data::block::BlockState;
use mc_nbt::snbt;
use mc_world::chunk_packet::*;
use mc_world::data_layer::DataLayer;
use mc_world::heightmap::HeightmapType;
use mc_world::palette::{BiomeId, PalettedContainer, Strategy};
use mc_world::section::LevelChunkSection;

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap())
        .collect()
}

#[test]
fn chunk_and_light_packet_data_match_vanilla() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/chunk_packet/golden.txt"
    ))
    .unwrap();
    let mut lines = text.lines();
    let mut cases = 0;
    while let Some(head) = lines.next() {
        assert!(head.starts_with("CASE"));
        let mut sections = Vec::new();
        let mut heightmaps: Vec<(HeightmapType, Vec<i64>)> = Vec::new();
        let mut block_entities = Vec::new();
        let (mut sky, mut blk): (Vec<Option<DataLayer>>, Vec<Option<DataLayer>>) =
            (Vec::new(), Vec::new());
        let (mut x, mut z) = (0, 0);
        let mut want = String::new();
        loop {
            let line = lines.next().unwrap();
            if line == "ENDCASE" {
                break;
            }
            let (kind, rest) = line.split_once(' ').unwrap_or((line, ""));
            match kind {
                "SEC" => {
                    let mut states = PalettedContainer::new(
                        BlockState::from_id(0).unwrap(),
                        Strategy::for_block_states(),
                    );
                    let mut biomes = PalettedContainer::new(
                        BiomeId(rest.parse().unwrap()),
                        Strategy::for_biomes(64),
                    );
                    loop {
                        let l = lines.next().unwrap();
                        if l == "ENDSEC" {
                            break;
                        }
                        let (k, r) = l.split_once(' ').unwrap();
                        let v: Vec<i32> = r.split(' ').map(|p| p.parse().unwrap()).collect();
                        match k {
                            "B" => states.set(
                                v[0],
                                v[1],
                                v[2],
                                BlockState::from_id(v[3] as u32).unwrap(),
                            ),
                            "M" => biomes.set(v[0], v[1], v[2], BiomeId(v[3] as u32)),
                            _ => panic!("bad line {l}"),
                        }
                    }
                    sections.push(LevelChunkSection::new(states, biomes));
                }
                "HM" => {
                    let (ty, longs) = rest.split_once(' ').unwrap();
                    heightmaps.push((
                        HeightmapType::from_id(ty.parse().unwrap()),
                        longs
                            .split(',')
                            .filter(|p| !p.is_empty())
                            .map(|p| p.parse().unwrap())
                            .collect(),
                    ));
                }
                "BE" => {
                    let mut it = rest.splitn(5, ' ');
                    let (bx, by, bz, ty, tag) = (
                        it.next().unwrap(),
                        it.next().unwrap(),
                        it.next().unwrap(),
                        it.next().unwrap(),
                        it.next().unwrap(),
                    );
                    let tag = if tag == "-" {
                        mc_nbt::CompoundTag::new()
                    } else {
                        snbt::parse_compound(tag).unwrap()
                    };
                    block_entities.push(BlockEntityInfo::create(
                        bx.parse().unwrap(),
                        by.parse().unwrap(),
                        bz.parse().unwrap(),
                        BlockEntityType::from_id(ty.parse().unwrap()).unwrap(),
                        tag,
                    ));
                }
                "SKY" | "BLK" => {
                    let (_, v) = rest.split_once(' ').unwrap();
                    let layer = match v {
                        "-" => None,
                        "E" => Some(DataLayer::new()),
                        h => Some(DataLayer::from_bytes(unhex(&h[1..])).unwrap()),
                    };
                    if kind == "SKY" {
                        sky.push(layer)
                    } else {
                        blk.push(layer)
                    }
                }
                "POS" => {
                    let (a, b) = rest.split_once(' ').unwrap();
                    x = a.parse().unwrap();
                    z = b.parse().unwrap();
                }
                "HEX" => want = rest.to_owned(),
                other => panic!("unknown line {other}"),
            }
        }
        let chunk = LevelChunkPacketData::new(
            &sections,
            heightmaps.iter().map(|(t, l)| (*t, l.as_slice())),
            block_entities,
        )
        .unwrap();
        let light = LightUpdatePacketData::new(
            &sky.iter().map(Option::as_ref).collect::<Vec<_>>(),
            &blk.iter().map(Option::as_ref).collect::<Vec<_>>(),
            None,
            None,
        );
        let mut got = Vec::new();
        write_level_chunk_with_light(&mut got, x, z, &chunk, &light).unwrap();
        assert_eq!(hex(&got), want, "case {cases}");
        // and back
        let mut input = &got[8..];
        let back = LevelChunkPacketData::read(&mut input).unwrap();
        assert_eq!(back, chunk);
        assert_eq!(LightUpdatePacketData::read(&mut input).unwrap(), light);
        assert!(input.is_empty());
        cases += 1;
    }
    assert_eq!(cases, 4);
}
