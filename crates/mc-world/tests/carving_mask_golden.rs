//! `CarvingMask` vs. the real vanilla class (`tests/fixtures/carving_mask/CarvingMaskGolden.java`).

use mc_world::carving_mask::{CarverOutput, CarvingMask};

#[test]
fn matches_vanilla_carving_mask() {
    let golden = include_str!("fixtures/carving_mask/golden.txt");
    let mut mask = CarvingMask::new(0, 0);
    let mut visited: Vec<String> = Vec::new();
    let (mut cfgs, mut visits) = (0, 0);
    let mut expected: Vec<String> = Vec::new();
    for line in golden.lines() {
        let (kind, rest) = line.split_once(' ').unwrap_or((line, ""));
        let f: Vec<&str> = rest.split(' ').collect();
        let n = |i: usize| f[i].parse::<i32>().unwrap();
        match kind {
            "CFG" => {
                cfgs += 1;
                mask = CarvingMask::new(n(0), n(1));
                assert_eq!((mask.min_y(), mask.max_y()), (n(2), n(3)));
                assert_eq!(mask.is_empty(), f[4] == "empty=true");
                expected.clear();
            }
            "C" => mask.carve(n(0), n(1), n(2)),
            "R" => (n(2)..=n(3)).for_each(|y| mask.carve(n(0), y, n(1))),
            "EMPTY" => assert_eq!(mask.is_empty(), f[0] == "true"),
            "V" => expected.push(rest.to_owned()),
            "END" => {
                visited.clear();
                mask.visit(|x, z, bottom, top| visited.push(format!("{x} {z} {bottom} {top}")));
                visits += visited.len();
                assert_eq!(visited, expected);
            }
            other => panic!("bad line {other}"),
        }
    }
    assert_eq!(cfgs, 28);
    assert_eq!(visits, 4307);
}

#[test]
fn grows_like_bitset_and_rejects_below_min() {
    let mut mask = CarvingMask::new(-64, 319);
    mask.carve(15, 319, 15);
    let mut seen = Vec::new();
    mask.visit(|x, z, b, t| seen.push((x, z, b, t)));
    assert_eq!(seen, [(15, 15, 319, 319)]);
    let result = std::panic::catch_unwind(|| CarvingMask::new(0, 15).carve(0, -1, 0));
    assert!(result.is_err(), "carve below minY must not silently wrap");
}
