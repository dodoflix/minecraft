//! `LongOpenHashSet` iteration order against fastutil (`fixtures/SetGolden.java`).

use mc_light::long_set::LongOpenHashSet;
use std::path::Path;

#[test]
fn iteration_order_matches_fastutil() {
    let text = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/set_golden.txt")).unwrap();
    let mut set = LongOpenHashSet::default();
    let mut light = false;
    let mut orders = 0;
    for line in text.lines() {
        let f: Vec<&str> = line.split(' ').collect();
        match f[0] {
            "NEW" => {
                if f[2] == "0" {
                    light = f[1] == "512";
                    set = if light { LongOpenHashSet::with_capacity(512, 0.5) } else { LongOpenHashSet::default() };
                }
            }
            "ADD" => assert_eq!(set.add(f[1].parse().unwrap()), f[2] == "true", "{line}"),
            "ORDER" => {
                let want: Vec<i64> = f[2..].iter().map(|n| n.parse().unwrap()).collect();
                assert_eq!(set.len(), f[1].parse::<usize>().unwrap());
                assert_eq!(set.iter().collect::<Vec<_>>(), want);
                orders += 1;
                set.clear();
                if light {
                    set.trim(512);
                }
            }
            _ => panic!("bad line {line}"),
        }
    }
    assert_eq!(orders, 24);
}
