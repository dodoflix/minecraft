//! The key order of NBT compounds that `NbtOps` builds (`createMap`) and edits (`set`, `remove`) against the real vanilla
//! classes (`fixtures/nbt_order/NbtOrderGolden.java`): keys that collide in `java.util.HashMap`, enough of them to make it
//! resize small tables and turn bins into red-black trees.

use mc_dfu::{Dyn, NBT};
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
