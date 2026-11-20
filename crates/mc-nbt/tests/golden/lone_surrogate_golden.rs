//! Strings with lone UTF-16 surrogates in NBT and SNBT against vanilla: `fixtures/lone_surrogate.tsv`, recorded by
//! `crates/mc-protocol/tests/fixtures/lone_surrogate/LoneSurrogateGolden.java`.
//!
//! A Rust `String` cannot hold a lone surrogate, so every recorded result is compared after replacing each lone
//! surrogate by U+FFFD (in text) or by `ef bf bd` (in modified UTF-8). Rows whose input needs a lone surrogate in a
//! Rust string are skipped.

use mc_nbt::snbt::{parse, print_compact, print_pretty, quote_and_escape};
use mc_nbt::{CompoundTag, ListTag, NbtAccounter, NbtError, Tag, nbt_io};

const GOLDEN: &str = include_str!("../fixtures/lone_surrogate.tsv");

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(h: &str) -> Vec<u8> {
    (0..h.len() / 2)
        .map(|i| u8::from_str_radix(&h[2 * i..2 * i + 2], 16).unwrap())
        .collect()
}

/// Inverse of the recorder's `esc`, as UTF-16 units.
fn unesc_units(s: &str) -> Vec<u16> {
    let mut units: Vec<u16> = Vec::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            units.extend(c.encode_utf16(&mut [0; 2]).iter());
            continue;
        }
        units.push(match chars.next().unwrap() {
            '\\' => u16::from(b'\\'),
            't' => u16::from(b'\t'),
            'n' => u16::from(b'\n'),
            'r' => u16::from(b'\r'),
            'u' => u16::from_str_radix(&chars.by_ref().take(4).collect::<String>(), 16).unwrap(),
            other => panic!("bad escape \\{other}"),
        });
    }
    units
}

/// A recorded text result with each lone surrogate replaced by U+FFFD.
fn lossy(s: &str) -> String {
    String::from_utf16_lossy(&unesc_units(s))
}

/// Modified UTF-8 bytes with each lone surrogate (`ed a0..bf xx` not forming a pair) replaced by U+FFFD.
fn lossy_hex(h: &str) -> String {
    let b = unhex(h);
    let triple = |i: usize| b.len() >= i + 3 && b[i] == 0xED && b[i + 1] >= 0xA0;
    let (mut out, mut i) = (Vec::new(), 0);
    while i < b.len() {
        if triple(i) {
            if b[i + 1] < 0xB0 && triple(i + 3) && b[i + 4] >= 0xB0 {
                out.extend_from_slice(&b[i..i + 6]);
                i += 6;
            } else {
                out.extend_from_slice(&[0xEF, 0xBF, 0xBD]);
                i += 3;
            }
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    hex(&out)
}

fn rows(op: &str) -> impl Iterator<Item = Vec<&'static str>> + '_ {
    GOLDEN
        .lines()
        .map(|l| l.split('\t').collect::<Vec<_>>())
        .filter(move |c| c[0] == op)
}

/// The `UTFDataFormatException` message under a read error, in the recorder's `ERR:` form.
fn read_err(e: &NbtError) -> String {
    assert!(matches!(e, NbtError::Reported { .. }), "{e:?}");
    let mut e = e;
    while let NbtError::Reported { cause, .. } = e {
        e = cause;
    }
    match e {
        NbtError::MalformedUtf(m) => {
            format!("ERR:ReportedNbtException:Loading NBT data<-UTFDataFormatException:{m}")
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn strings_written_without_lone_surrogates() {
    let mut n = 0;
    for c in rows("nbt_string_write") {
        let Ok(s) = String::from_utf16(&unesc_units(c[1])) else {
            continue;
        };
        let tag = Tag::String(s.clone());
        let mut out = Vec::new();
        nbt_io::write_any_tag(&tag, &mut out).unwrap();
        assert_eq!(hex(&out), c[2], "bytes of {}", c[1]);
        let back =
            nbt_io::read_any_tag(&mut &out[..], &mut NbtAccounter::unlimited_heap()).unwrap();
        assert_eq!(back, tag);
        assert_eq!(print_compact(&tag), lossy(c[4]), "toString of {}", c[1]);
        assert_eq!(quote_and_escape(&s), lossy(c[5]));
        assert_eq!(tag == Tag::String("\u{fffd}".into()), c[7] == "true");
        n += 1;
    }
    for c in rows("nbt_key_write") {
        let Ok(s) = String::from_utf16(&unesc_units(c[1])) else {
            continue;
        };
        let mut tag = CompoundTag::new();
        tag.put_byte(s, 1);
        tag.put_byte("a", 2);
        assert_eq!(hex(&nbt_io::to_bytes(&tag)), c[2], "key {}", c[1]);
        assert_eq!(print_compact(&Tag::Compound(tag)), lossy(c[4]));
        n += 1;
    }
    for c in rows("nbt_list_write") {
        let Ok(s) = String::from_utf16(&unesc_units(c[1])) else {
            continue;
        };
        let mut list = ListTag::new();
        list.push(Tag::String(s.clone()));
        list.push(Tag::String(format!("{s}{s}")));
        let tag = Tag::List(list);
        let mut out = Vec::new();
        nbt_io::write_any_tag(&tag, &mut out).unwrap();
        assert_eq!(hex(&out), c[2], "list {}", c[1]);
        assert_eq!(print_compact(&tag), lossy(c[3]));
        n += 1;
    }
    assert!(n >= 25, "{n} rows");
}

#[test]
fn modified_utf8_reads_like_vanilla() {
    let mut n = 0;
    for c in rows("nbt_read") {
        n += 1;
        let payload = unhex(c[1]);
        let mut full = vec![8, (payload.len() >> 8) as u8, payload.len() as u8];
        full.extend_from_slice(&payload);
        match nbt_io::read_any_tag(&mut &full[..], &mut NbtAccounter::unlimited_heap()) {
            Ok(tag) => {
                let Tag::String(s) = &tag else { panic!() };
                assert_eq!(*s, lossy(c[2]), "value of {}", c[1]);
                assert_eq!(print_compact(&tag), lossy(c[3]));
                let mut out = Vec::new();
                nbt_io::write_any_tag(&tag, &mut out).unwrap();
                assert_eq!(hex(&out), lossy_hex(c[4]), "rewrite of {}", c[1]);
            }
            Err(e) => assert_eq!(read_err(&e), c[2], "error of {}", c[1]),
        }
    }
    for c in rows("nbt_read_key") {
        n += 1;
        let payload = unhex(c[1]);
        let mut full = vec![10, 0, 0, 1, (payload.len() >> 8) as u8, payload.len() as u8];
        full.extend_from_slice(&payload);
        full.extend_from_slice(&[5, 0, 0]);
        match nbt_io::read(&mut &full[..], &mut NbtAccounter::unlimited_heap()) {
            Ok(tag) => {
                assert_eq!(
                    tag.keys().cloned().collect::<Vec<_>>().join(","),
                    lossy(c[2])
                );
                assert_eq!(hex(&nbt_io::to_bytes(&tag)), lossy_hex(c[3]), "{}", c[1]);
            }
            Err(e) => assert_eq!(read_err(&e), c[2], "error of key {}", c[1]),
        }
    }
    assert_eq!(n, 72, "rows");
}

#[test]
fn snbt_escapes_parse_and_print_like_vanilla() {
    let mut n = 0;
    for c in rows("snbt_parse") {
        let input = String::from_utf16(&unesc_units(c[1])).unwrap();
        // two keys that only differ by a lone surrogate collapse into one here
        if input.contains("\\ufffd") {
            continue;
        }
        match parse(&input) {
            Ok(tag) => {
                assert_eq!(print_compact(&tag), lossy(c[2]), "toString of {input}");
                let mut out = Vec::new();
                nbt_io::write_any_tag(&tag, &mut out).unwrap();
                assert_eq!(hex(&out), lossy_hex(c[3]), "bytes of {input}");
                assert_eq!(print_pretty(&tag), lossy(c[4]), "pretty of {input}");
            }
            Err(e) => assert_eq!(
                format!("ERR:CommandSyntaxException:{e}"),
                lossy(c[2]),
                "error of {input}"
            ),
        }
        n += 1;
    }
    assert!(n > 15, "{n} rows");
}
