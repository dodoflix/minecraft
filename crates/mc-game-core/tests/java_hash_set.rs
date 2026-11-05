//! `tree::java_hash_set::BlockPosHashSet` iterates in the order of `java.util.HashSet<BlockPos>` (golden:
//! `tests/oracle/JavaHashSetOracle.java`).

extern crate mc_game_core as mc_game;
use mc_core::pos::BlockPos;
use mc_core::random::{LegacyRandomSource, RandomSource};
use mc_game::world::level::levelgen::feature::tree::java_hash_set::BlockPosHashSet;

#[test]
fn iteration_order_matches_java() {
    let golden = include_str!("../../mc-game/tests/fixtures/java_hash_set.txt");
    let mut r = LegacyRandomSource::new(12345);
    let sizes = [1, 5, 12, 13, 14, 25, 50, 97, 100, 300, 800, 2000];
    let mut lines = golden.lines();
    let mut mismatches = Vec::new();
    for _round in 0..12 {
        for n in sizes {
            let ox = r.next_int_bounded(8001) - 4000;
            let oy = r.next_int_bounded(300) - 64;
            let oz = r.next_int_bounded(8001) - 4000;
            let rx = 1 + r.next_int_bounded(9);
            let ry = 1 + r.next_int_bounded(40);
            let rz = 1 + r.next_int_bounded(9);
            let mut set = BlockPosHashSet::new();
            for _ in 0..n {
                let x = ox + r.next_int_bounded(2 * rx + 1) - rx;
                let y = oy + r.next_int_bounded(ry);
                let z = oz + r.next_int_bounded(2 * rz + 1) - rz;
                set.insert(BlockPos::new(x, y, z));
            }
            let mut h: i64 = 0xcbf29ce484222325u64 as i64;
            for p in set.iter() {
                for v in [p.x(), p.y(), p.z()] {
                    h = (h ^ i64::from(v)).wrapping_mul(0x100000001b3);
                }
            }
            let expected = format!("{n} {ox} {oy} {oz} {rx} {ry} {rz} {} {:x}", set.len(), h);
            let line = lines.next().expect("golden line");
            if line != expected {
                mismatches.push(format!("java: {line}\nrust: {expected}"));
            }
        }
    }
    assert!(mismatches.is_empty(), "{} of 144 sets differ:\n{}", mismatches.len(), mismatches.join("\n"));
}
