//! `mc_core::java_collections` against the real `java.util.HashMap` / fastutil iteration orders
//! (`fixtures/java_collections/HashOrderGolden.java`).

use mc_core::java_collections::*;
use mc_core::math::java::java_string_hash;

const GOLDEN: &str = include_str!("../fixtures/java_collections/golden.txt");

fn split(s: &str) -> Vec<&str> {
    if s.is_empty() {
        vec![]
    } else {
        s.split('\u{1f}').collect()
    }
}

#[test]
fn matches_java_iteration_order() {
    let mut n = 0;
    for line in GOLDEN.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        let (kind, cap, keys, want) = (
            f[0],
            f[1].parse::<usize>().unwrap(),
            split(f[2]),
            split(f[3]),
        );
        let got: Vec<String> = match kind {
            "hashmap" | "hashmap-copy" | "hashset-copy" => {
                let h = |k: &String| java_string_hash(k);
                let mut items: Vec<String> = keys.iter().map(|k| k.to_string()).collect();
                let cap = match kind {
                    "hashmap" => cap,
                    "hashmap-copy" => {
                        // the copy is filled by iterating the source map (capacity `cap`)
                        items = hash_map_ordered(items, h, cap);
                        hash_map_copy_capacity(keys.len())
                    }
                    _ => hash_set_copy_capacity(keys.len()),
                };
                hash_map_ordered(items, h, cap)
            }
            "fast" => {
                let mut m = FastOpenHashMap::new(cap, |k: &String| java_string_hash(k));
                keys.iter().for_each(|k| m.put(k.to_string(), ()));
                m.into_entries().into_iter().map(|e| e.0).collect()
            }
            "fastint" => {
                let mut m = FastOpenHashMap::with_null_key(cap, |k: &i32| *k, |k| *k == 0);
                keys.iter()
                    .for_each(|k| m.put(k.parse::<i32>().unwrap(), ()));
                m.into_entries()
                    .into_iter()
                    .map(|e| e.0.to_string())
                    .collect()
            }
            _ => panic!("kind {kind}"),
        };
        assert_eq!(got, want, "{kind} cap {cap} keys {keys:?}");
        n += 1;
    }
    assert!(n >= 3000, "{n}");
}
