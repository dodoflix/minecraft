//! Primitive/composite codecs vs vanilla `FriendlyByteBuf` / `ByteBufCodecs`
//! (fixture `tests/fixtures/codec.golden`, generator `tests/fixtures/CodecGolden.java`).
use std::collections::HashMap;

use mc_core::{
    ident::Identifier,
    phys::vec3::Vec3,
    pos::{block_pos::BlockPos, chunk_pos::ChunkPos, section_pos::SectionPos, vec3i::Vec3i},
};
use mc_nbt::CompoundTag;
use mc_protocol::codec::*;
use uuid::Uuid;

fn golden() -> HashMap<String, Vec<u8>> {
    include_str!("fixtures/codec.golden")
        .lines()
        .map(|l| {
            let (label, hex) = l.split_once(':').unwrap();
            (label.to_owned(), (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap()).collect())
        })
        .collect()
}

/// Encode `v`, compare with vanilla's bytes, decode vanilla's bytes back and compare with `v`.
fn check<T: Encode + Decode + PartialEq + std::fmt::Debug>(g: &HashMap<String, Vec<u8>>, label: &str, v: T) {
    let expected = &g[label];
    assert_eq!(to_bytes(&v), *expected, "encode {label}");
    assert_eq!(from_bytes::<T>(expected).unwrap_or_else(|e| panic!("decode {label}: {e}")), v, "decode {label}");
}

fn id(s: &str) -> Identifier {
    Identifier::parse(s).unwrap()
}

#[test]
fn lp_vec3() {
    let g = golden();
    let inputs: [[f64; 3]; 14] = [
        [0.0, 0.0, 0.0],
        [1e-6, 0.0, 0.0],
        [0.5, -0.25, 0.125],
        [1.0, 1.0, 1.0],
        [-1.0, 2.0, -3.0],
        [3.0, 3.0, 3.0],
        [4.0, 0.0, 0.0],
        [100.5, -2000.25, 7.0],
        [1.7179869183E10, -1.7179869183E10, 0.0],
        [1e12, 0.0, -1e12],
        [f64::NAN, 5.0, 0.0],
        [0.00003051944088384301, 0.0, 0.0],
        [123456.789, 0.001, -99999.9999],
        [-0.0, 3.0517578125E-5, 0.0],
    ];
    for (i, v) in inputs.iter().enumerate() {
        let bytes = &g[&format!("lpvec3/{i}")];
        assert_eq!(to_bytes(&LpVec3(Vec3::new(v[0], v[1], v[2]))), *bytes, "encode {i}");
        // the decoded doubles, bit for bit
        let d = from_bytes::<LpVec3>(bytes).unwrap().0;
        let want = &g[&format!("lpvec3_decoded/{i}")];
        let got: Vec<u8> = [d.x, d.y, d.z].iter().flat_map(|x| x.to_bits().to_be_bytes()).collect();
        assert_eq!(got, *want, "decode {i}");
    }
}

#[test]
fn positions() {
    let g = golden();
    check(&g, "vec3/0", Vec3::new(1.5, -2.25, 1e300));
    check(&g, "vec3i/0", Vec3i { x: 1, y: -1, z: 300000 });
    let bp = [[0, 0, 0], [1, 2, 3], [-1, -1, -1], [33554431, 2047, -33554432], [-33554432, -2048, 33554431], [123456, -64, -654321]];
    for (i, p) in bp.iter().enumerate() {
        check(&g, &format!("blockpos/{i}"), BlockPos::new(p[0], p[1], p[2]));
        check(&g, &format!("sectionpos/{i}"), SectionPos::of(p[0] >> 4, p[1] >> 4, p[2] >> 4));
        check(&g, &format!("chunkpos/{i}"), ChunkPos { x: p[0], z: p[2] });
    }
    check(&g, "globalpos/0", GlobalPos { dimension: id("minecraft:the_nether"), pos: BlockPos::new(-5, 70, 12) });
}

#[test]
fn angles() {
    let g = golden();
    let ang = [0.0f32, 90.0, -90.0, 180.0, 359.9, 360.0, -0.1, 1000.0, 1.4, -720.5];
    for (i, a) in ang.iter().enumerate() {
        let bytes = &g[&format!("angle/{i}")];
        assert_eq!(to_bytes(&Angle(*a)), *bytes, "encode {a}");
        let unpacked = from_bytes::<Angle>(bytes).unwrap().0;
        assert_eq!(unpacked, Angle::unpack(bytes[0] as i8));
    }
}

#[test]
fn varints_and_strings() {
    let g = golden();
    for (i, v) in [0, 1, 127, 128, 255, 25565, 2097151, i32::MAX, -1, i32::MIN].iter().enumerate() {
        check(&g, &format!("varint/{i}"), VarInt(*v));
    }
    for (i, v) in [0, 1, 127, 128, i64::MAX, -1, i64::MIN, 1 << 35].iter().enumerate() {
        check(&g, &format!("varlong/{i}"), VarLong(*v));
    }
    for (i, s) in ["", "abc", "h\u{e9}llo \u{20ac}", "\u{1F600} emoji", "\u{0}nul"].iter().enumerate() {
        check(&g, &format!("string/{i}"), s.to_string());
    }
    check(&g, "optvarint/none", OptionalVarInt(None));
    check(&g, "optvarint/some", OptionalVarInt(Some(41)));
    check(&g, "optvarint/max", OptionalVarInt(Some(i32::MAX)));
    check(&g, "rgb/0", RgbColor(0xFF336699u32 as i32));
    check(&g, "instant/0", Instant(1700000000123));
    check(&g, "uuid/0", Uuid::parse_str("069a79f4-44e9-4726-a5be-fca90e38aaf5").unwrap());
}

#[test]
fn collections() {
    let g = golden();
    let mut bs = BitSet::new();
    [1, 70, 130].iter().for_each(|&i| bs.set(i));
    check(&g, "bitset/long", bs.clone());
    check(&g, "bitset/bytes", BitSetBytes(bs));
    check(&g, "bitset/empty_long", BitSet::new());
    check(&g, "bitset/empty_bytes", BitSetBytes(BitSet::new()));
    let mut fb = BitSet::new();
    fb.set(0);
    fb.set(17);
    check(&g, "bitset/fixed20", FixedBitSet::<20>(fb));

    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    struct Link(usize);
    impl EnumValues for Link {
        const COUNT: usize = 10;
        fn ordinal(self) -> usize {
            self.0
        }
        fn from_ordinal(i: usize) -> Self {
            Link(i)
        }
    }
    // SUPPORT = 2, ANNOUNCEMENTS = 9
    check(&g, "enumset/0", [Link(2), Link(9)].into_iter().collect::<EnumSet<Link>>());
    check(&g, "longarray/0", LongArray(vec![1, -2, i64::MAX]));
    check(&g, "bytearray/0", vec![1u8, 2, 3]);
    check(&g, "list/0", vec![VarInt(1), VarInt(300), VarInt(-1)]);
    check(&g, "optional/some", Some(7i32));
    check(&g, "optional/none", None::<i32>);
    check(&g, "either/left", Either::<i32, String>::Left(9));
    check(&g, "either/right", Either::<i32, String>::Right("x".into()));
    let mut c = CompoundTag::new();
    c.put_byte("a", 1);
    let mut l = mc_nbt::ListTag::new();
    for i in 1..=3 {
        l.push(mc_nbt::Tag::Int(i));
    }
    c.put("b", mc_nbt::Tag::List(l));
    let mut inner = CompoundTag::new();
    inner.put_string("d", "e");
    c.put("c", mc_nbt::Tag::Compound(inner));
    c.put("f", mc_nbt::Tag::IntArray(vec![1, 2]));
    c.put_double("g", 1.5);
    check(&g, "nbt/compound", c);
}
