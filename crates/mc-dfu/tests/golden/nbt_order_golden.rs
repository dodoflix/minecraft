//! The key order of NBT compounds that `NbtOps` builds (`createMap`) and edits (`set`, `remove`) against the real vanilla
//! classes (`fixtures/nbt_order/NbtOrderGolden.java`): keys that collide in `java.util.HashMap`, enough of them to make it
//! resize small tables and turn bins into red-black trees.

use mc_dfu::{Dyn, NBT};
use mc_nbt::CompoundTag;
use std::sync::LazyLock;

static GOLDEN: LazyLock<String> =
    LazyLock::new(|| mc_testkit::include_gz!("../fixtures/nbt_order/golden.txt.gz"));

fn split(s: &str) -> Vec<&str> {
    if s.is_empty() {
        vec![]
    } else {
        s.split('\u{1f}').collect()
    }
}

fn keys(d: &Dyn) -> Vec<String> {
    match d {
        Dyn::Map(m) => m
            .iter()
            .map(|(k, _)| k.as_str().expect("string key").to_owned())
            .collect(),
        other => panic!("not a compound: {other}"),
    }
}

#[test]
fn compound_key_order_matches_vanilla() {
    let mut n = 0;
    for line in GOLDEN.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        let (initial, ops, want) = (split(f[0]), split(f[1]), split(f[2]));
        let mut tag = NBT.create_map(
            initial
                .iter()
                .enumerate()
                .map(|(i, k)| (NBT.create_string(*k), NBT.create_int(i as i32)))
                .collect(),
        );
        for (i, op) in ops.iter().enumerate() {
            tag = match op.split_at(1) {
                ("+", key) => NBT.set(&tag, key, NBT.create_int(1000 + i as i32)),
                ("-", key) => NBT.remove(&tag, key),
                _ => panic!("operation {op}"),
            };
        }
        assert_eq!(keys(&tag), want, "initial {initial:?} operations {ops:?}");
        n += 1;
    }
    assert!(n >= 900, "{n}");
}

/// The same sequences through `CompoundTag` itself: `new CompoundTag()` and `put`s, then per operation `shallowCopy()` with a
/// `put` (`NbtOps.set`) or a `remove`; the order of `iter_java_order` (what the binary writer lists) is vanilla's `keySet()`.
#[test]
fn compound_tag_key_order_matches_vanilla() {
    let mut n = 0;
    for line in GOLDEN.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        let (initial, ops, want) = (split(f[0]), split(f[1]), split(f[2]));
        let mut tag = CompoundTag::new();
        for (i, k) in initial.iter().enumerate() {
            tag.put_int(*k, i as i32);
        }
        for (i, op) in ops.iter().enumerate() {
            let mut copy = tag.shallow_copy();
            match op.split_at(1) {
                ("+", key) => copy.put_int(key, 1000 + i as i32),
                ("-", key) => {
                    copy.remove(key);
                }
                _ => panic!("operation {op}"),
            }
            let owned: Vec<String> = copy
                .share()
                .into_shallow_copy()
                .iter_java_order()
                .into_iter()
                .map(|(k, _)| k.as_lossy_str().to_owned())
                .collect();
            let borrowed: Vec<String> = copy
                .shallow_copy()
                .iter_java_order()
                .into_iter()
                .map(|(k, _)| k.as_lossy_str().to_owned())
                .collect();
            assert_eq!(owned, borrowed, "initial {initial:?} operations {ops:?}");
            tag = copy;
        }
        let got: Vec<String> = tag
            .iter_java_order()
            .into_iter()
            .map(|(k, _)| k.as_lossy_str().to_owned())
            .collect();
        assert_eq!(got, want, "initial {initial:?} operations {ops:?}");
        n += 1;
    }
    assert!(n >= 900, "{n}");
}

static IN_PLACE: LazyLock<String> =
    LazyLock::new(|| mc_testkit::include_gz!("../fixtures/nbt_order/in_place.txt.gz"));

/// Sequences of `put`, `remove` and `shallow_copy` / `copy` on one `CompoundTag`, so the table keeps its growth, its tree bins
/// and the shape earlier removes left (a remove outside a tree bin, a second remove from a tree bin, puts into a tree bin
/// after a remove, a table that grew with keys that are gone, edits after a copy).
#[test]
fn compound_tag_edited_in_place_matches_vanilla() {
    let mut n = 0;
    for line in IN_PLACE.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        let (initial, ops, want) = (split(f[0]), split(f[1]), split(f[2]));
        let mut tag = CompoundTag::new();
        for (i, k) in initial.iter().enumerate() {
            tag.put_int(*k, i as i32);
        }
        for (i, op) in ops.iter().enumerate() {
            match op.split_at(1) {
                ("+", key) => tag.put_int(key, 1000 + i as i32),
                ("-", key) => {
                    tag.remove(key);
                }
                ("~", _) => tag = tag.shallow_copy(),
                ("=", _) => tag = tag.copy(),
                _ => panic!("operation {op}"),
            }
        }
        let got: Vec<String> = tag
            .iter_java_order()
            .into_iter()
            .map(|(k, _)| k.as_lossy_str().to_owned())
            .collect();
        assert_eq!(got, want, "initial {initial:?} operations {ops:?}");
        // `share()` keeps every table: the same order, and later edits of the copy go on from the same table
        let shared: Vec<String> = tag
            .share()
            .iter_java_order()
            .into_iter()
            .map(|(k, _)| k.as_lossy_str().to_owned())
            .collect();
        assert_eq!(shared, want, "share of {initial:?} operations {ops:?}");
        n += 1;
    }
    assert!(n >= 1500, "{n}");
}
