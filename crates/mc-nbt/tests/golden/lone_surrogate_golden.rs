//! Strings with lone UTF-16 surrogates in NBT and SNBT against vanilla: `fixtures/lone_surrogate.tsv`, recorded by
//! `crates/mc-protocol/tests/fixtures/lone_surrogate/LoneSurrogateGolden.java`.

use mc_core::java_string::JavaString;
use mc_nbt::snbt::{parse, print_compact, print_pretty, quote_and_escape};
use mc_nbt::{CompoundTag, ListTag, NbtAccounter, NbtError, Tag, nbt_io};
use mc_testkit::{hex, rows, unesc_units, unhex};

const GOLDEN: &str = include_str!("../fixtures/lone_surrogate.tsv");

/// A recorded string (or input), exactly.
fn java(cell: &str) -> JavaString {
    JavaString::from_utf16_units(&unesc_units(cell))
}

/// The `ERR:` cell of a failed read.
fn read_err(e: &NbtError) -> JavaString {
    assert!(matches!(e, NbtError::Reported { .. }), "{e:?}");
    let mut e = e;
    while let NbtError::Reported { cause, .. } = e {
        e = cause;
    }
    match e {
        NbtError::MalformedUtf(m) => {
            let mut out = JavaString::from(
                "ERR:ReportedNbtException:Loading NBT data<-UTFDataFormatException:",
            );
            out.push_java(m);
            out
        }
        other => panic!("{other:?}"),
    }
}

fn written(tag: &Tag) -> String {
    let mut out = Vec::new();
    nbt_io::write_any_tag(tag, &mut out).unwrap();
    hex(&out)
}

#[test]
fn strings_are_written_read_and_printed_like_vanilla() {
    let mut n = 0;
    for c in rows(GOLDEN, "nbt_string_write") {
        let s = java(c[1]);
        let tag = Tag::String(s.clone());
        assert_eq!(written(&tag), c[2], "bytes of {}", c[1]);
        let mut bytes = &unhex(c[2])[..];
        let back = nbt_io::read_any_tag(&mut bytes, &mut NbtAccounter::unlimited_heap()).unwrap();
        assert_eq!(back, tag);
        assert_eq!(back.as_java_string().unwrap(), &java(c[3]));
        assert_eq!(print_compact(&tag), java(c[4]), "toString of {}", c[1]);
        assert_eq!(quote_and_escape(&s), java(c[5]));
        assert_eq!(s.java_hash_code().to_string(), c[6]);
        assert_eq!(tag == Tag::String("\u{fffd}".into()), c[7] == "true");
        n += 1;
    }
    for c in rows(GOLDEN, "nbt_key_write") {
        let s = java(c[1]);
        let mut tag = CompoundTag::new();
        tag.put_byte(s, 1);
        tag.put_byte("a", 2);
        assert_eq!(hex(&nbt_io::to_bytes(&tag)), c[2], "key {}", c[1]);
        let back = nbt_io::read(
            &mut &nbt_io::to_bytes(&tag)[..],
            &mut NbtAccounter::unlimited_heap(),
        )
        .unwrap();
        let mut keys = JavaString::new();
        for (i, (k, _)) in back.iter_java_order().into_iter().enumerate() {
            if i > 0 {
                keys.push(',');
            }
            keys.push_java(k);
        }
        assert_eq!(keys, java(c[3]));
        assert_eq!(print_compact(&Tag::Compound(tag)), java(c[4]));
        n += 1;
    }
    for c in rows(GOLDEN, "nbt_list_write") {
        let s = java(c[1]);
        let mut doubled = s.clone();
        doubled.push_java(&s);
        let mut list = ListTag::new();
        list.push(Tag::String(s));
        list.push(Tag::String(doubled));
        let tag = Tag::List(list);
        assert_eq!(written(&tag), c[2], "list {}", c[1]);
        assert_eq!(print_compact(&tag), java(c[3]));
        n += 1;
    }
    assert_eq!(n, 60);
}

#[test]
fn a_string_too_long_for_modified_utf8_reports_its_ends() {
    let mut n = 0;
    for c in
        rows(GOLDEN, "nbt_string_write_rep:21845").chain(rows(GOLDEN, "nbt_string_write_rep:21846"))
    {
        let units = vec![0xD800u16; if c[0].ends_with("5") { 21845 } else { 21846 }];
        let tag = Tag::String(JavaString::from_utf16_units(&units));
        let mut out = Vec::new();
        match nbt_io::write_any_tag(&tag, &mut out) {
            Ok(()) => {
                let h = hex(&out);
                assert_eq!(format!("{}:{}", h.len(), &h[..20]), c[2]);
            }
            Err(NbtError::MalformedUtf(m)) => {
                let mut got = JavaString::from("ERR:UTFDataFormatException:");
                got.push_java(&m);
                assert_eq!(got, java(c[2]));
            }
            Err(other) => panic!("{other:?}"),
        }
        n += 1;
    }
    assert_eq!(n, 2);
}

#[test]
fn modified_utf8_reads_like_vanilla() {
    let mut n = 0;
    for c in rows(GOLDEN, "nbt_read") {
        n += 1;
        let payload = unhex(c[1]);
        let mut full = vec![8, (payload.len() >> 8) as u8, payload.len() as u8];
        full.extend_from_slice(&payload);
        match nbt_io::read_any_tag(&mut &full[..], &mut NbtAccounter::unlimited_heap()) {
            Ok(tag) => {
                assert_eq!(
                    tag.as_java_string().unwrap(),
                    &java(c[2]),
                    "value of {}",
                    c[1]
                );
                assert_eq!(print_compact(&tag), java(c[3]));
                assert_eq!(written(&tag), c[4], "rewrite of {}", c[1]);
            }
            Err(e) => assert_eq!(read_err(&e), java(c[2]), "error of {}", c[1]),
        }
    }
    for c in rows(GOLDEN, "nbt_read_key") {
        n += 1;
        let payload = unhex(c[1]);
        let mut full = vec![10, 0, 0, 1, (payload.len() >> 8) as u8, payload.len() as u8];
        full.extend_from_slice(&payload);
        full.extend_from_slice(&[5, 0, 0]);
        match nbt_io::read(&mut &full[..], &mut NbtAccounter::unlimited_heap()) {
            Ok(tag) => {
                let key = tag.keys().next().unwrap();
                assert_eq!(*key, java(c[2]));
                assert_eq!(hex(&nbt_io::to_bytes(&tag)), c[3], "{}", c[1]);
            }
            Err(e) => assert_eq!(read_err(&e), java(c[2]), "error of key {}", c[1]),
        }
    }
    assert_eq!(n, 72, "rows");
}

#[test]
fn snbt_escapes_parse_and_print_like_vanilla() {
    let mut n = 0;
    for c in rows(GOLDEN, "snbt_parse") {
        let input = String::from_utf16(&unesc_units(c[1])).unwrap();
        match parse(&input) {
            Ok(tag) => {
                assert_eq!(print_compact(&tag), java(c[2]), "toString of {input}");
                assert_eq!(written(&tag), c[3], "bytes of {input}");
                assert_eq!(print_pretty(&tag), java(c[4]), "pretty of {input}");
            }
            Err(e) => {
                let mut want = JavaString::from("ERR:CommandSyntaxException:");
                want.push_java(&e.message_with_context());
                assert_eq!(want, java(c[2]), "error of {input}");
            }
        }
        n += 1;
    }
    assert_eq!(n, 20, "rows");
}
