//! `fixtures/model_golden.tsv` (from `ModelOracle.java`): numeric narrowing, `sizeInBytes`, `compareNbt`.

use mc_nbt::{NbtAccounter, Tag, nbt_io, nbt_utils, snbt};

fn unhex(h: &str) -> Vec<u8> {
    (0..h.len() / 2)
        .map(|i| u8::from_str_radix(&h[2 * i..2 * i + 2], 16).unwrap())
        .collect()
}

fn unesc(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next().unwrap() {
            'n' => out.push('\n'),
            't' => out.push('\t'),
            'r' => out.push('\r'),
            '\\' => out.push('\\'),
            'x' => {
                let h: String = it.by_ref().take(2).collect();
                out.push(char::from_u32(u32::from_str_radix(&h, 16).unwrap()).unwrap());
            }
            o => panic!("bad escape {o}"),
        }
    }
    out
}

fn fbits(v: f32) -> u32 {
    if v.is_nan() { 0x7fc0_0000 } else { v.to_bits() }
}

fn dbits(v: f64) -> u64 {
    if v.is_nan() {
        0x7ff8_0000_0000_0000
    } else {
        v.to_bits()
    }
}

#[test]
fn model_matches_vanilla() {
    let data = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/model_golden.tsv"
    ))
    .unwrap();
    let (mut n, mut z, mut c) = (0, 0, 0);
    for line in data.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        match f[0] {
            "N" => {
                n += 1;
                let bits = u64::from_str_radix(f[2], 16).unwrap();
                let tag = match f[1] {
                    "byte" => Tag::Byte(bits as u8 as i8),
                    "short" => Tag::Short(bits as u16 as i16),
                    "int" => Tag::Int(bits as u32 as i32),
                    "long" => Tag::Long(bits as i64),
                    "float" => Tag::float(f32::from_bits(bits as u32)),
                    "double" => Tag::double(f64::from_bits(bits)),
                    k => panic!("kind {k}"),
                };
                let got = format!(
                    "{}\t{}\t{}\t{}\t{:x}\t{:x}\t{}",
                    tag.as_byte().unwrap(),
                    tag.as_short().unwrap(),
                    tag.as_int().unwrap(),
                    tag.as_long().unwrap(),
                    fbits(tag.as_float().unwrap()),
                    dbits(tag.as_double().unwrap()),
                    tag.as_boolean().unwrap()
                );
                assert_eq!(got, f[3..].join("\t"), "{line}");
            }
            "Z" => {
                z += 1;
                let bytes = unhex(f[1]);
                let tag =
                    nbt_io::read_any_tag(&mut &bytes[..], &mut NbtAccounter::unlimited_heap())
                        .unwrap();
                assert_eq!(
                    tag.size_in_bytes().to_string(),
                    f[2],
                    "sizeInBytes of {tag}"
                );
            }
            "C" => {
                c += 1;
                let (e, a) = (
                    snbt::parse(&unesc(f[1])).unwrap(),
                    snbt::parse(&unesc(f[2])).unwrap(),
                );
                let got = nbt_utils::compare_nbt(Some(&e), Some(&a), f[3] == "1");
                assert_eq!(
                    got.to_string(),
                    f[4],
                    "compareNbt({e}, {a}, partial={})",
                    f[3]
                );
            }
            k => panic!("row {k}"),
        }
    }
    assert!(n > 400 && z > 2000 && c > 900);
}
