//! `Entity.calculateViewVector` against the real vanilla class (generator: `tests/fixtures/EntityBaseGolden.java`,
//! `scripts/vanilla-java.sh crates/mc-game/tests/fixtures/EntityBaseGolden.java > tests/fixtures/entity_base/view_vector.txt`).

use mc_game::world::entity::entity::calculate_view_vector;

#[test]
fn view_vector_matches_vanilla() {
    let text = include_str!("../../../mc-game/tests/fixtures/entity_base/view_vector.txt");
    let mut n = 0;
    for line in text.lines() {
        let f: Vec<&str> = line.split(' ').collect();
        let x = f32::from_bits(f[0].parse::<i32>().unwrap() as u32);
        let y = f32::from_bits(f[1].parse::<i32>().unwrap() as u32);
        let expect = [f[2], f[3], f[4]].map(|s| f64::from_bits(s.parse::<i64>().unwrap() as u64));
        let v = calculate_view_vector(x, y);
        let got = [v.x, v.y, v.z];
        for k in 0..3 {
            assert_eq!(got[k].to_bits(), expect[k].to_bits(), "xRot={x} yRot={y} component {k}: {} vs {}", got[k], expect[k]);
        }
        n += 1;
    }
    assert_eq!(n, 400);
}
