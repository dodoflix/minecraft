//! Decoding of block-state containers (valid, partial, broken NBT) vs the real vanilla codec
//! (`fixtures/palette/PaletteDecodeGolden.java`).

use crate::chunk_common;

use mc_nbt::{Tag, snbt};
use mc_world::palette::BlockStateCodec;

#[test]
fn container_decoding_matches_vanilla() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/palette/decode_golden.txt"
    ))
    .unwrap();
    let factory = chunk_common::factory();
    let mut lines = text.lines();
    let mut n = 0;
    while let Some(input) = lines.next() {
        let rest = input.strip_prefix("IN ").unwrap();
        let (label, snbt_text) = rest.split_once(' ').unwrap();
        let expected = lines.next().unwrap();
        let tag = snbt::parse(snbt_text).unwrap();
        let got = factory.block_states_from_nbt(&tag);
        match (expected.strip_prefix("OK "), got) {
            (None, Err(_)) => {}
            (None, Ok(_)) => panic!("{label}: vanilla rejects, we accept"),
            (Some(_), Err(e)) => panic!("{label}: vanilla accepts, we reject: {e}"),
            (Some(ok), Ok(unpacked)) => {
                let mut parts = ok.splitn(3, ' ');
                let hash: i64 = parts.next().unwrap().parse().unwrap();
                let problems: usize = parts.next().unwrap().parse().unwrap();
                let reencoded = snbt::parse(parts.next().unwrap()).unwrap();
                let c = &unpacked.container;
                let mut h: i64 = 1125899906842597;
                for i in 0..4096 {
                    h = h
                        .wrapping_mul(31)
                        .wrapping_add(i64::from(c.get(i & 15, (i >> 8) & 15, (i >> 4) & 15).id()));
                }
                assert_eq!(h, hash, "{label}: contents");
                assert_eq!(
                    unpacked.problems.len().min(1),
                    problems,
                    "{label}: problems {:?}",
                    unpacked.problems
                );
                assert_eq!(
                    Tag::Compound(c.to_nbt(&BlockStateCodec).unwrap()),
                    reencoded,
                    "{label}: re-encoded"
                );
            }
        }
        n += 1;
    }
    assert_eq!(n, 31);
}
