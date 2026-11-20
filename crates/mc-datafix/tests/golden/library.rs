//! The small DataFixerUpper library pieces the fixes use against the real vanilla/DFU classes
//! (`fixtures/library/LibraryGolden.java`, regenerate with `fixtures/library/gen.sh`): `DataFixUtils.ceillog2`,
//! `PackedBitStorage`, `CrudeIncrementalIntIdentityHashBiMap` and the `Dynamic` helpers of `ExtraDataFixUtils`.

use mc_core::crude_incremental_int_identity_hash_bimap::CrudeIncrementalIntIdentityHashBiMap as BiMap;
use mc_core::math::java::{java_double_to_string, java_float_to_string};
use mc_core::math::mth;
use mc_datafix::extra_data_fix_utils as extra;
use mc_datafix::packed_bit_storage::PackedBitStorage;
use mc_datafix::{DSL, Type};
use mc_dfu::{Dyn, Dynamic, NBT};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

const GOLDEN: &str = include_str!("../fixtures/library.tsv");

pub(crate) fn dump(d: &Dyn) -> String {
    let join = |it: &mut dyn Iterator<Item = String>| it.collect::<Vec<_>>().join(",");
    match d {
        Dyn::Byte(v) => format!("{v}b"),
        Dyn::Short(v) => format!("{v}s"),
        Dyn::Int(v) => v.to_string(),
        Dyn::Long(v) => format!("{v}L"),
        Dyn::Float(v) => format!("{}f", java_float_to_string(*v)),
        Dyn::Double(v) => format!("{}d", java_double_to_string(*v)),
        Dyn::Str(s) => format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")),
        Dyn::List(l) => format!("[{}]", join(&mut l.iter().map(dump))),
        Dyn::Map(m) => format!(
            "{{{}}}",
            join(
                &mut m
                    .iter()
                    .map(|(k, v)| format!("{}:{}", k.as_str().unwrap(), dump(v)))
            )
        ),
        Dyn::Ints(v) => format!("[I;{}]", join(&mut v.iter().map(|x| x.to_string()))),
        other => panic!("not in the recording: {other}"),
    }
}

pub(crate) fn dyn_(snbt: &str) -> Dynamic {
    Dynamic::new(NBT, Dyn::from(&mc_nbt::snbt::parse(snbt).unwrap()))
}

pub(crate) fn dump_dynamic(d: &Dynamic) -> String {
    dump(d.value())
}

fn digest(a: &[i64]) -> i64 {
    a.iter().fold(1125899906842597i64, |h, x| {
        h.wrapping_mul(31).wrapping_add(*x)
    })
}

fn read_all(s: &PackedBitStorage, size: i32) -> i64 {
    (0..size).fold(7i64, |h, i| {
        h.wrapping_mul(31).wrapping_add(i64::from(s.get(i)))
    })
}

fn packed(input: &str) -> String {
    let (bits, size) = input.split_once(',').unwrap();
    let (bits, size): (i32, i32) = (bits.parse().unwrap(), size.parse().unwrap());
    let mask = (1i64 << bits) - 1;
    let mut s = PackedBitStorage::new(bits, size);
    for i in 0..size {
        let v = (i64::from(i) * 2654435761 + i64::from(bits) * 40503 + 12345) & mask & 0x7fffffff;
        s.set(i, v as i32);
    }
    let raw = s.raw();
    let first: Vec<String> = raw.iter().take(4).map(i64::to_string).collect();
    let a = format!(
        "{}:{}/{}/{}",
        raw.len(),
        first.join(","),
        digest(raw),
        read_all(&s, size)
    );
    for i in (0..size).step_by(3) {
        let v = ((i64::from(i) * 40503) ^ 0x5bd1e995 ^ (i64::from(bits) * 977)) & mask & 0x7fffffff;
        s.set(i, v as i32);
    }
    let b = format!("{}/{}", digest(s.raw()), read_all(&s, size));
    let copy = PackedBitStorage::from_data(bits, size, s.raw().to_vec());
    format!("{a}|{b}|{}/{}", read_all(&copy, size), copy.bits())
}

fn packed_case(name: &str) -> String {
    let run = || match name {
        "bits0" => drop(PackedBitStorage::new(0, 16)),
        "bits33" => drop(PackedBitStorage::new(33, 16)),
        "bits_neg" => drop(PackedBitStorage::new(-1, 16)),
        "short_data" => drop(PackedBitStorage::from_data(4, 16, vec![])),
        "long_data" => drop(PackedBitStorage::from_data(4, 16, vec![0; 5])),
        "ok_len" => drop(PackedBitStorage::from_data(5, 13, vec![0; 2])),
        "set_neg" => PackedBitStorage::new(4, 16).set(-1, 0),
        "set_oob" => PackedBitStorage::new(4, 16).set(16, 0),
        "set_value_big" => PackedBitStorage::new(4, 16).set(0, 16),
        "set_value_neg" => PackedBitStorage::new(4, 16).set(0, -1),
        "get_oob" => drop(PackedBitStorage::new(4, 16).get(16)),
        "get_neg" => drop(PackedBitStorage::new(4, 16).get(-1)),
        "bits32_big_value" => PackedBitStorage::new(32, 4).set(0, -1),
        other => panic!("unknown case {other}"),
    };
    match catch_unwind(AssertUnwindSafe(run)) {
        Ok(()) => "OK".into(),
        Err(p) => format!(
            "EX:IllegalArgumentException:{}",
            p.downcast_ref::<String>()
                .unwrap()
                .strip_prefix("java.lang.IllegalArgumentException: ")
                .expect("an IllegalArgumentException payload")
        ),
    }
}

fn bimap(input: &str) -> String {
    let (capacity, ops) = input.split_once(' ').unwrap();
    let pool: Vec<Arc<i32>> = (0..64).map(Arc::new).collect();
    let mut m = BiMap::create(capacity.parse().unwrap());
    let mut orig: Option<BiMap<i32>> = None;
    let mut results = Vec::new();
    for op in ops.split(' ') {
        let arg = &op[1..];
        let key = |s: &str| pool[s.parse::<usize>().unwrap()].clone();
        let r = catch_unwind(AssertUnwindSafe(|| match &op[..1] {
            "a" => m.add(key(arg)).to_string(),
            "g" => m.get_id(&key(arg)).to_string(),
            "b" => m
                .by_id(arg.parse().unwrap())
                .map_or("-".into(), |k| k.to_string()),
            "c" => m.contains(&key(arg)).to_string(),
            "i" => m.contains_id(arg.parse().unwrap()).to_string(),
            "m" => {
                let (k, id) = arg.split_once(':').unwrap();
                m.add_mapping(key(k), id.parse().unwrap());
                "ok".into()
            }
            "x" => {
                m.clear();
                "ok".into()
            }
            "y" => {
                let copy = m.clone();
                let old = std::mem::replace(&mut m, copy);
                orig.get_or_insert(old);
                "ok".into()
            }
            "s" => m.size().to_string(),
            "n" => m
                .iter()
                .map(|k| k.to_string())
                .collect::<Vec<_>>()
                .join(","),
            other => panic!("unknown op {other}"),
        }));
        results.push(r.unwrap_or_else(|_| "EX".into()));
    }
    let ids = |m: &BiMap<i32>| {
        m.iter()
            .map(|k| k.to_string())
            .collect::<Vec<_>>()
            .join(",")
    };
    format!(
        "{} | {} | {}",
        results.iter().map(|r| format!("{r};")).collect::<String>(),
        ids(&m),
        ids(orig.as_ref().unwrap_or(&m))
    )
}

fn extra_op(op: &str, input: &str) -> String {
    let tail = |n: usize| -> Vec<&str> {
        let mut parts: Vec<&str> = input.rsplitn(n + 1, ' ').collect();
        parts.reverse();
        parts
    };
    match op {
        "fix_block_pos" => dump_dynamic(&extra::fix_block_pos(&dyn_(input))),
        "fix_inline_block_pos" => {
            let p = tail(4);
            dump_dynamic(&extra::fix_inline_block_pos(
                &dyn_(p[0]),
                p[1],
                p[2],
                p[3],
                p[4],
            ))
        }
        "create_block_pos" => {
            let p = tail(3);
            let n = |s: &str| s.parse::<i32>().unwrap();
            dump_dynamic(&extra::create_block_pos(
                &dyn_(p[0]),
                n(p[1]),
                n(p[2]),
                n(p[3]),
            ))
        }
        "fix_string_field" => {
            let p = tail(2);
            let fix: fn(&str) -> String = match p[2] {
                "upper" => |v| v.to_uppercase(),
                "trim" => |v| v.trim_matches(|c| c <= ' ').to_owned(),
                "ell" => |v| v.replace('l', "L"),
                "const" => |_| "X".into(),
                _ => |_| String::new(),
            };
            dump_dynamic(&extra::fix_string_field(&dyn_(p[0]), p[1], fix))
        }
        "plain_translation_key" => extra::get_plain_translation_key(&dyn_(input))
            .map_or("none".into(), |v| format!("some:{v}")),
        "read_and_set" => {
            let p = tail(2);
            let ty = DSL::and_type(
                &DSL::field_type("a", &DSL::int_type()),
                &DSL::and_type(
                    &DSL::field_type("b", &DSL::string()),
                    &DSL::remainder_type(),
                ),
            );
            let run = || {
                let typed = Type::read_typed(&ty, &dyn_(p[0])).result().unwrap().0;
                let finder = if p[1] == "a" {
                    DSL::field_finder("a", &DSL::int_type())
                } else {
                    DSL::field_finder("b", &DSL::string())
                };
                extra::read_and_set(typed, &finder, &dyn_(p[2])).write()
            };
            match catch_unwind(AssertUnwindSafe(run)) {
                Ok(d) => d
                    .result()
                    .map_or("write failed".into(), |d| dump_dynamic(&d)),
                Err(_) => "EX".into(),
            }
        }
        other => panic!("unknown op {other}"),
    }
}

fn run(op: &str, input: &str) -> String {
    match op {
        "ceillog2" => mth::ceillog2(input.parse().unwrap()).to_string(),
        "packed" => packed(input),
        "packed_case" => packed_case(input),
        "bimap" => bimap(input),
        _ => extra_op(op, input),
    }
}

#[test]
fn library_matches_vanilla() {
    std::panic::set_hook(Box::new(|_| {}));
    let (mut bad, mut n) = (Vec::new(), 0);
    for line in GOLDEN.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        let (op, input, want) = (f[0], f[1], f[2]);
        let got =
            catch_unwind(AssertUnwindSafe(|| run(op, input))).unwrap_or_else(|_| "THROW".into());
        if got != want {
            bad.push(format!("{op} {input}\n   java: {want}\n   rust: {got}"));
        }
        n += 1;
    }
    let _ = std::panic::take_hook();
    assert!(n > 1100, "{n} cases");
    assert!(
        bad.is_empty(),
        "{} of {n} cases differ; first:\n{}",
        bad.len(),
        bad.iter().take(25).cloned().collect::<Vec<_>>().join("\n")
    );
}
