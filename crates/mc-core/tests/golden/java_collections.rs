//! `mc_core::java_collections` against the real `java.util.HashMap` / fastutil iteration orders
//! (`fixtures/java_collections/HashOrderGolden.java`).

use mc_core::java_collections::*;
use mc_core::math::java::java_string_hash;
use mc_core::pos::BlockPos;
use std::sync::LazyLock;

static GOLDEN: LazyLock<String> =
    LazyLock::new(|| mc_testkit::include_gz!("../fixtures/java_collections/golden.txt.gz"));

fn split(s: &str) -> Vec<&str> {
    if s.is_empty() {
        vec![]
    } else {
        s.split('\u{1f}').collect()
    }
}

fn parse_pos(s: &str) -> BlockPos {
    let c: Vec<i32> = s.split(',').map(|n| n.parse().unwrap()).collect();
    BlockPos::new(c[0], c[1], c[2])
}

fn pos_string(p: &BlockPos) -> String {
    format!("{},{},{}", p.x(), p.y(), p.z())
}

/// `pending.putAll(source)` (iterating the source map), once.
fn flush_source(source: &mut JavaHashMap<BlockPos, ()>, pending: &mut JavaHashMap<BlockPos, ()>) {
    if !source.is_empty() {
        let entries: Vec<_> = source.keys().map(|p| (*p, ())).collect();
        source.clear();
        pending.put_all(entries.into_iter());
    }
}

/// Replays an operation list on the pending `HashMap` (`P`, filled from the `S` source map with `putAll`) and the live
/// fastutil map (`L`); see `HashOrderGolden.java`.
fn block_pos_order(kind: &str, expected: usize, ops: &[&str]) -> Vec<String> {
    let mut source = JavaHashMap::new(block_pos_hash);
    let mut pending = JavaHashMap::new(block_pos_hash);
    let mut live = FastOpenHashMap::new(expected, block_pos_hash);
    for op in ops {
        let (map, verb, arg) = (&op[..1], &op[1..2], &op[2..]);
        match (map, verb) {
            ("S", "+") => {
                source.insert(parse_pos(arg), ());
                continue;
            }
            ("P", _) => flush_source(&mut source, &mut pending),
            _ => {}
        }
        match (map, verb) {
            ("P", "+") => {
                pending.insert(parse_pos(arg), ());
            }
            ("P", "-") => {
                pending.remove(&parse_pos(arg));
            }
            ("P", "C") => pending.clear(),
            ("L", "+") => {
                live.put(parse_pos(arg), ());
            }
            ("L", "-") => {
                live.remove(&parse_pos(arg));
            }
            ("L", "C") => live.clear(),
            _ => panic!("op {op}"),
        }
    }
    flush_source(&mut source, &mut pending);
    let order: Vec<BlockPos> = match kind {
        "posmap" => pending.keys().copied().collect(),
        "posfast" => live.iter().map(|e| *e.0).collect(),
        _ => hash_set_union_order(
            pending.keys().copied().collect::<Vec<_>>().into_iter(),
            live.iter().map(|e| *e.0),
            block_pos_hash,
        ),
    };
    order.iter().map(pos_string).collect()
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
                keys.iter().for_each(|k| {
                    m.put(k.to_string(), ());
                });
                m.into_entries().into_iter().map(|e| e.0).collect()
            }
            "fastint" => {
                let mut m = FastOpenHashMap::with_null_key(cap, |k: &i32| *k, |k| *k == 0);
                keys.iter().for_each(|k| {
                    m.put(k.parse::<i32>().unwrap(), ());
                });
                m.into_entries()
                    .into_iter()
                    .map(|e| e.0.to_string())
                    .collect()
            }
            "posmap" | "posfast" | "posunion" => block_pos_order(kind, cap, &keys),
            _ => panic!("kind {kind}"),
        };
        assert_eq!(got, want, "{kind} cap {cap} keys {keys:?}");
        n += 1;
    }
    assert!(n >= 3000, "{n}");
}
