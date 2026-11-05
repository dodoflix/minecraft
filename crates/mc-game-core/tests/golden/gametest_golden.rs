//! Pure geometry of the game test framework against vanilla (`fixtures/GameTestGolden.java`).

use mc_core::pos::{BlockPos, Vec3i};
use mc_core::rotation::Rotation;
use mc_game::gametest::structure_utils::{
    get_rotation_for_rotation_steps, get_rotation_steps_for_rotation, get_structure_bounding_box, get_transformed_far_corner,
};

#[test]
fn structure_geometry_equals_vanilla() {
    let golden = include_str!("../../../mc-game/tests/fixtures/gametest_golden.txt");
    let mut checked = 0;
    for line in golden.lines() {
        let words: Vec<&str> = line.split(' ').collect();
        if words[0] == "steps" {
            let steps: i32 = words[1].parse().unwrap();
            let rotation = get_rotation_for_rotation_steps(steps);
            assert_eq!(rotation.ordinal().to_string(), words[2]);
            assert_eq!(get_rotation_steps_for_rotation(rotation).to_string(), words[3]);
            continue;
        }
        let n: Vec<i32> = words.iter().filter_map(|w| w.parse().ok()).collect();
        // origin xyz, size xyz, rotation, far xyz, box min xyz max xyz
        let (origin, size, rotation) = (BlockPos::new(n[0], n[1], n[2]), Vec3i::new(n[3], n[4], n[5]), Rotation::ALL[n[6] as usize]);
        let far = get_transformed_far_corner(origin, size, rotation);
        assert_eq!([far.x(), far.y(), far.z()], [n[7], n[8], n[9]], "far corner: {line}");
        let b = get_structure_bounding_box(origin, size, rotation);
        assert_eq!([b.min_x(), b.min_y(), b.min_z(), b.max_x(), b.max_y(), b.max_z()], [n[10], n[11], n[12], n[13], n[14], n[15]], "box: {line}");
        checked += 1;
    }
    assert_eq!(checked, 60);
}
