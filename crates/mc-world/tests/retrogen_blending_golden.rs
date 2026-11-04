//! `BelowZeroRetrogen.CODEC` and `BlendingData.Packed.CODEC` (+ `pack`/`unpack`) vs. the real vanilla codecs on ~110 NBT shapes
//! (`tests/fixtures/{below_zero_retrogen,blending_data}/*Golden.java`).

use mc_nbt::snbt;
use mc_world::below_zero_retrogen::BelowZeroRetrogen;
use mc_world::blending_data::{BlendingData, BlendingDataPacked};
use mc_world::status::ChunkStatus;

#[test]
fn below_zero_retrogen_codec_matches_vanilla() {
    let golden = include_str!("fixtures/below_zero_retrogen/golden.txt");
    let (mut ok, mut none) = (0, 0);
    for line in golden.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        assert_eq!(f[0], "IN");
        let parsed = snbt::parse(f[1]);
        if f[2] == "PARSEERR" {
            assert!(parsed.is_err(), "{}", f[1]);
            continue;
        }
        let tag = parsed.unwrap_or_else(|e| panic!("{}: {}", f[1], e.message()));
        let got = BelowZeroRetrogen::parse(&tag);
        match f[2] {
            "NONE" => {
                none += 1;
                assert!(got.is_none(), "{} should be rejected", f[1]);
            }
            "OK" => {
                ok += 1;
                let b = got.unwrap_or_else(|| panic!("{} should decode", f[1]));
                assert_eq!(b.target_status().name(), f[3], "{}", f[1]);
                assert_eq!(b.has_bedrock_holes().to_string(), f[4], "{}", f[1]);
                // x/z are masked with `& 15`
                assert_eq!(b.has_bedrock_hole(19, -11) == b.has_bedrock_hole(3, 5), f[5] == "true");
                let holes: String = (0..16).flat_map(|z| (0..16).map(move |x| (x, z))).map(|(x, z)| if b.has_bedrock_hole(x, z) { '1' } else { '0' }).collect();
                assert_eq!(holes, f[6], "{}", f[1]);
                assert_eq!(snbt::print_compact(&b.to_tag()), f[7], "{}", f[1]);
                // the encoded form decodes back to an equal value
                assert_eq!(BelowZeroRetrogen::parse(&b.to_tag()), Some(b));
            }
            other => panic!("bad result {other}"),
        }
    }
    assert_eq!((ok, none), (42, 15));
}

#[test]
fn blending_data_codec_and_pack_unpack_match_vanilla() {
    let golden = include_str!("fixtures/blending_data/golden.txt");
    let (mut ok, mut none) = (0, 0);
    for line in golden.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        assert_eq!(f[0], "IN");
        let parsed = snbt::parse(f[1]);
        if f[2] == "PARSEERR" {
            assert!(parsed.is_err(), "{}", f[1]);
            continue;
        }
        let tag = parsed.unwrap_or_else(|e| panic!("{}: {}", f[1], e.message()));
        let got = BlendingDataPacked::parse(&tag);
        match f[2] {
            "NONE" => {
                none += 1;
                assert!(got.is_none(), "{} should be rejected", f[1]);
            }
            "OK" => {
                ok += 1;
                let p = got.unwrap_or_else(|| panic!("{} should decode", f[1]));
                assert_eq!(snbt::print_compact(&p.to_tag().unwrap()), f[3], "{}", f[1]);
                let bd = BlendingData::unpack(Some(&p)).unwrap();
                assert_eq!(format!("{} {}", bd.min_y(), bd.height()), f[4], "{}", f[1]);
                assert_eq!(snbt::print_compact(&bd.pack().to_tag().unwrap()), f[5], "{}", f[1]);
            }
            other => panic!("bad result {other}"),
        }
    }
    assert_eq!((ok, none), (42, 12));
}

#[test]
fn blending_data_validation_and_null_unpack() {
    assert!(BlendingData::unpack(None).is_none());
    let bad = BlendingDataPacked { min_section: 0, max_section: 1, heights: Some(vec![0.0; 15]) };
    assert_eq!(bad.to_tag().unwrap_err(), "heights has to be of length 16");
    let all_no_value = BlendingDataPacked { min_section: -4, max_section: 20, heights: Some(vec![f32::MAX; 16]) };
    // pack() drops the heights array when every value is NO_VALUE
    assert_eq!(BlendingData::unpack(Some(&all_no_value)).unwrap().pack().heights, None);
}

#[test]
fn retrogen_target_status_is_never_empty() {
    for s in ChunkStatus::ALL {
        let tag = snbt::parse(&format!("{{target_status:\"{}\"}}", s.name())).unwrap();
        assert_eq!(BelowZeroRetrogen::parse(&tag).is_some(), s != ChunkStatus::Empty);
    }
}
