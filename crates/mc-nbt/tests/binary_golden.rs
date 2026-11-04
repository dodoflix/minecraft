//! Differential tests of binary IO against vanilla (`fixtures/BinaryOracle.java`).

use mc_nbt::{compound, nbt_io, snbt, CompoundTag, ListTag, NbtAccounter, NbtError, Tag};

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(h: &str) -> Vec<u8> {
    (0..h.len() / 2).map(|i| u8::from_str_radix(&h[2 * i..2 * i + 2], 16).unwrap()).collect()
}

fn named(c: &CompoundTag) -> String {
    hex(&nbt_io::to_bytes(c))
}

/// Same classification as `BinaryOracle.result`.
fn result(bytes: &[u8], mut acc: NbtAccounter) -> String {
    fn io(e: &NbtError) -> String {
        match e {
            NbtError::Eof => "EOF".into(),
            NbtError::InvalidTagId(id) => format!("IOException:Invalid tag id: {id}"),
            NbtError::Io(m) => format!("IOException:{m}"),
            NbtError::MalformedUtf(m) => format!("UTFDataFormatException:{m}"),
            other => panic!("not an io error: {other:?}"),
        }
    }
    match nbt_io::read_any_tag(&mut &*bytes, &mut acc) {
        Ok(_) => "OK".into(),
        Err(NbtError::Eof) => "EOF".into(),
        Err(NbtError::Accounter(m)) => format!("NbtAccounterException:{m}"),
        Err(NbtError::Format(m)) => format!("NbtFormatException:{m}"),
        Err(NbtError::IllegalArgument(m)) => format!("IllegalArgumentException:{m}"),
        Err(NbtError::Reported { cause, .. }) => format!("Reported:{}", io(&cause)),
        Err(e) => format!("IO:{}", io(&e)),
    }
}

fn scenario(name: &str) -> CompoundTag {
    match name {
        "order40" => (0..40).map(|i| (format!("k{i}"), i)).collect(),
        "order13" => (0..13).map(|i| (format!("key_{i}"), i)).collect(),
        "collide" | "collide_rev" => {
            let mut keys = ["Aa", "BB", "AaAa", "BBBB", "AaBB", "BBAa", "C#", "Bb", "Aa1", "BB1"].to_vec();
            if name == "collide_rev" {
                keys.reverse();
            }
            keys.into_iter().map(|k| (k, k)).collect()
        }
        "removed" | "copy_of_removed" | "copy_then_put" => {
            let mut c: CompoundTag = (0..30).map(|i| (format!("n{i}"), i)).collect();
            for i in (0..30).step_by(3).chain((0..30).step_by(4)) {
                c.remove(&format!("n{i}"));
            }
            match name {
                "removed" => c,
                "copy_of_removed" => c.copy(),
                _ => {
                    let mut d = c.copy();
                    d.put_string("Aa", "x");
                    d.put_string("BB", "y");
                    d
                }
            }
        }
        "ops_remove" => {
            let mut c = scenario("order40").shallow_copy();
            c.remove("k5");
            c
        }
        "shallow_grow" => {
            let mut c = compound! {"a" => 1};
            c = c.shallow_copy();
            c.remove("zz");
            for i in 0..10 {
                c.put_int(format!("q{i}"), i);
            }
            c
        }
        "merge" => {
            let mut s7: CompoundTag = (0..20).map(|i| (format!("x{i}"), i)).collect();
            s7.put("sub", compound! {"deep" => 1, "deeper" => 2});
            let mut o = CompoundTag::new();
            o.put_int("x3", 99);
            o.put_int("new1", 1);
            o.put_int("new2", 2);
            o.put("sub", compound! {"deep" => 5, "other" => 3});
            s7.merge(&o);
            s7
        }
        "strings" => {
            let strs = ["", "a", "\0", "a\0b", "\u{1}", "\u{7f}", "\u{80}", "\u{7ff}", "\u{800}", "\u{ffff}", "\u{1F600}", "é日本語😀", "\u{d7ff}\u{e000}"];
            let mut c = CompoundTag::new();
            for (i, s) in strs.iter().enumerate() {
                c.put_string(format!("s{i}"), *s);
                c.put_string(format!("{s}key{i}"), "v");
            }
            c
        }
        "longstring" => {
            let mut c = CompoundTag::new();
            c.put_string("long", format!("{}{}", "x".repeat(30000), "\u{800}".repeat(30000)));
            c.put_string("ok", "y".repeat(65535));
            c.put_string("x".repeat(70000), "v");
            c
        }
        "listwrap" => {
            let mut c = CompoundTag::new();
            let mut mixed = ListTag::new();
            mixed.push(1);
            mixed.push("a");
            mixed.push(compound! {"x" => 1});
            mixed.push(compound! {"" => 2});
            c.put("mixed", mixed);
            let mut comps = ListTag::new();
            comps.push(compound! {"" => 1});
            comps.push(compound! {"" => 1, "b" => 1});
            comps.push(CompoundTag::new());
            c.put("comps", comps);
            let mut lists = ListTag::new();
            lists.push(ListTag::from(vec![Tag::Byte(1)]));
            lists.push(ListTag::new());
            lists.push(ListTag::from(vec![Tag::Short(2)]));
            c.put("lists", lists);
            c.put("withEnd", ListTag::from(vec![Tag::End]));
            c.put("withEnd2", ListTag::from(vec![Tag::End, Tag::Int(1)]));
            c.put("hetNum", ListTag::from(vec![Tag::Int(1), Tag::Long(2), Tag::Double(0.5), Tag::IntArray(vec![1, 2])]));
            c
        }
        "misc" => {
            let mut c = CompoundTag::new();
            c.put_double("negzero", -0.0);
            c.put_float("negzerof", -0.0);
            c.put_double("nan", f64::NAN);
            c.put_float("nanf", f32::NAN);
            c.put_double("nan2", f64::from_bits(0x7ff8000000000123));
            c.put_float("nanf2", f32::from_bits(0x7fc00123));
            c.put_double("inf", f64::INFINITY);
            c.put_double("min", f64::from_bits(1));
            c.put_float("fmax", f32::MAX);
            c.put_byte_array("ba", vec![1, -2, 3]);
            c.put_int_array("ia", vec![1, -2, i32::MIN]);
            c.put_long_array("la", vec![1, i64::MIN]);
            c.put_byte_array("empty", vec![]);
            c.put_boolean("t", true);
            c.put_byte("b", -5);
            c.put_short("sh", -300);
            c.put_long("l", -1);
            c
        }
        "read_negzero" => {
            let mut b = vec![10, 0, 0, 6, 0, 1, b'z'];
            b.extend(0x8000000000000000u64.to_be_bytes());
            b.extend([5, 0, 1, b'f']);
            b.extend(0x80000000u32.to_be_bytes());
            b.push(0);
            nbt_io::read(&mut &b[..], &mut NbtAccounter::unlimited_heap()).unwrap()
        }
        other => panic!("unknown scenario {other}"),
    }
}

#[test]
fn binary_matches_vanilla() {
    let data = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/binary_golden.tsv")).unwrap();
    let mut failures = Vec::new();
    let (mut scenarios, mut reads, mut hands) = (0, 0, 0);
    for line in data.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        match f[0] {
            "S" => {
                scenarios += 1;
                let got = named(&scenario(f[1]));
                if got != f[2] {
                    failures.push(format!("scenario {}:\n  got  {}\n  want {}", f[1], &got[..got.len().min(400)], &f[2][..f[2].len().min(400)]));
                }
            }
            "R" => {
                reads += 1;
                let bytes = unhex(f[1]);
                let mut acc = NbtAccounter::unlimited_heap();
                nbt_io::read_any_tag(&mut &bytes[..], &mut acc).unwrap();
                let usage = acc.usage();
                let got = [
                    usage.to_string(),
                    result(&bytes, NbtAccounter::new((usage - 1).max(0), 512)),
                    result(&bytes, NbtAccounter::new(usage / 2, 512)),
                    result(&bytes[..bytes.len().saturating_sub(1)], NbtAccounter::unlimited_heap()),
                    result(&bytes[..bytes.len() / 2], NbtAccounter::unlimited_heap()),
                ];
                if got.iter().map(String::as_str).ne(f[2..7].iter().copied()) {
                    failures.push(format!("read {}:\n  got  {:?}\n  want {:?}", f[1], got, &f[2..7]));
                }
            }
            "H" => {
                hands += 1;
                let got = result(&unhex(f[1]), NbtAccounter::default_quota());
                if got != f[2] {
                    failures.push(format!("hand {}: got {got:?} want {:?}", &f[1][..f[1].len().min(60)], f[2]));
                }
            }
            other => panic!("bad row {other}"),
        }
    }
    assert!(scenarios == 15 && reads > 100 && hands > 30, "fixture too small: {scenarios} {reads} {hands}");
    assert!(failures.is_empty(), "{} mismatches:\n{}", failures.len(), failures.iter().take(15).cloned().collect::<Vec<_>>().join("\n"));
}

/// Every vanilla-parseable SNBT sample survives `write_any_tag` -> `read_any_tag` unchanged
/// (heterogeneous lists are wrapped on write and unwrapped on read).
#[test]
fn roundtrip_snbt_corpus() {
    let data = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/snbt_golden.tsv")).unwrap();
    let mut n = 0;
    for line in data.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        if f[1] != "O" {
            continue;
        }
        let bytes = unhex(f[4]);
        let tag = nbt_io::read_any_tag(&mut &bytes[..], &mut NbtAccounter::unlimited_heap()).unwrap();
        let mut again = Vec::new();
        nbt_io::write_any_tag(&tag, &mut again).unwrap();
        assert_eq!(hex(&again), f[4], "re-encode differs for {}", f[0]);
        n += 1;
    }
    assert!(n > 5000);
}

#[test]
fn gzip_and_zlib_roundtrip() {
    let c = snbt::parse_compound("{a:1,b:[1,2,3],c:\"x\",d:{e:1.5d}}").unwrap();
    for bytes in [mc_nbt::compress::write_compressed(&c), mc_nbt::compress::write_compressed_zlib(&c)] {
        assert_ne!(bytes, nbt_io::to_bytes(&c));
    }
    let gz = mc_nbt::compress::write_compressed(&c);
    assert_eq!(&gz[..2], &[0x1f, 0x8b]);
    assert_eq!(mc_nbt::compress::read_compressed(&gz, &mut NbtAccounter::default_quota()).unwrap(), c);
    let z = mc_nbt::compress::write_compressed_zlib(&c);
    assert_eq!(mc_nbt::compress::read_compressed_zlib(&z, &mut NbtAccounter::default_quota()).unwrap(), c);
    // a decompressed size beyond the quota is rejected
    assert!(mc_nbt::compress::read_compressed(&gz, &mut NbtAccounter::create(10)).is_err());
}
