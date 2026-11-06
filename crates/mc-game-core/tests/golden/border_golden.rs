//! `WorldBorder` against the real class (`fixtures/BorderGolden.java` replays `fixtures/border_ops.txt`).

use mc_core::phys::AABB;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_game::world::level::saveddata::SavedData;
use mc_game::world::level::saveddata::world_border::{BorderStatus, Settings, WorldBorder};

fn d(v: f64) -> String {
    format!("{:x}", v.to_bits())
}

fn line(b: &WorldBorder) -> String {
    let mut s = String::new();
    for p in [0.0f32, 0.5, 1.0] {
        s += &format!(
            "{} {} {} {} | ",
            d(b.get_min_x_at(p)),
            d(b.get_max_x_at(p)),
            d(b.get_min_z_at(p)),
            d(b.get_max_z_at(p))
        );
    }
    let status = match b.get_status() {
        BorderStatus::Growing => "GROWING",
        BorderStatus::Shrinking => "SHRINKING",
        BorderStatus::Stationary => "STATIONARY",
    };
    s += &format!(
        "{} {} {} {} {status} | ",
        d(b.get_size()),
        b.get_lerp_time(),
        d(b.get_lerp_target()),
        d(b.get_lerp_speed())
    );
    s += &format!(
        "{} {} {} {} {} {} {} | ",
        d(b.get_center_x()),
        d(b.get_center_z()),
        d(b.get_safe_zone()),
        d(b.get_damage_per_block()),
        b.get_warning_time(),
        b.get_warning_blocks(),
        b.get_absolute_max_size()
    );
    let pts = [
        (0.0, 0.0),
        (10.5, -7.25),
        (49.99, 50.0),
        (-100.0, 3.0),
        (1e9, -1e9),
        (b.get_max_x(), 0.0),
        (b.get_min_x(), b.get_min_z()),
    ];
    for (x, z) in pts {
        let c = b.clamp_vec3_to_bound(x, 64.0, z);
        s += &format!(
            "{},{},{},{},{},{};",
            b.is_within_bounds(x, z),
            b.is_within_bounds_margin(x, z, 2.5),
            d(b.get_distance_to_border(x, z)),
            d(c.x),
            d(c.z),
            b.clamp_to_bounds(x, 64.0, z).as_long()
        );
    }
    s += &format!(
        " | {},{},{},{},{} | ",
        b.is_within_bounds_aabb(&AABB::new(-1.0, 0.0, -1.0, 1.0, 2.0, 1.0)),
        b.is_within_bounds_aabb(&AABB::new(
            b.get_max_x() - 1.0,
            0.0,
            0.0,
            b.get_max_x() + 1.0,
            2.0,
            1.0
        )),
        b.is_within_bounds_chunk(&ChunkPos::new(0, 0)),
        b.is_within_bounds_chunk(&ChunkPos::new(1875000, 0)),
        b.is_within_bounds_block(&BlockPos::new(5, 5, 5)),
    );
    for bb in b.get_collision_shape().to_aabbs() {
        s += &format!(
            "{},{},{},{},{},{};",
            d(bb.min_x),
            d(bb.min_y),
            d(bb.min_z),
            d(bb.max_x),
            d(bb.max_y),
            d(bb.max_z)
        );
    }
    s
}

#[test]
fn replay_matches_vanilla() {
    let ops = include_str!("../../../mc-game/tests/fixtures/border_ops.txt");
    let golden = include_str!("../../../mc-game/tests/fixtures/border.golden");
    let mut b = WorldBorder::default();
    let mut n = 0;
    for (op, expected) in ops.lines().zip(golden.lines()) {
        let t: Vec<&str> = op.split(' ').collect();
        let f = |i: usize| t[i].parse::<f64>().unwrap();
        match t[0] {
            "new" if t.len() == 1 => b = WorldBorder::default(),
            "new" => {
                b = WorldBorder::new(Settings {
                    center_x: f(1),
                    center_z: f(2),
                    damage_per_block: f(3),
                    safe_zone: f(4),
                    warning_blocks: t[5].parse().unwrap(),
                    warning_time: t[6].parse().unwrap(),
                    size: f(7),
                    lerp_time: t[8].parse().unwrap(),
                    lerp_target: f(9),
                })
            }
            "size" => b.set_size(f(1)),
            "center" => b.set_center(f(1), f(2)),
            "lerp" => b.lerp_size_between(f(1), f(2), t[3].parse().unwrap(), t[4].parse().unwrap()),
            "tick" => (0..t[1].parse::<i32>().unwrap()).for_each(|_| b.tick()),
            "safe" => b.set_safe_zone(f(1)),
            "damage" => b.set_damage_per_block(f(1)),
            "wt" => b.set_warning_time(t[1].parse().unwrap()),
            "wb" => b.set_warning_blocks(t[1].parse().unwrap()),
            "absmax" => b.set_absolute_max_size(t[1].parse().unwrap()),
            "init" => b.apply_initial_settings(t[1].parse().unwrap()),
            other => panic!("{other}"),
        }
        let (head, snbt) = expected
            .strip_prefix(&format!("{op} => "))
            .unwrap()
            .rsplit_once(" | ")
            .unwrap();
        // the shape column may be empty, which leaves a trailing "| " in `head`
        let ours = line(&b);
        assert_eq!(ours.trim_end(), head.trim_end(), "after `{op}`");
        assert_eq!(
            b.current_settings().encode(),
            mc_nbt::snbt::parse(snbt).unwrap(),
            "codec after `{op}`"
        );
        n += 1;
    }
    assert_eq!(n, ops.lines().count());
    let _ = b.is_dirty();
}
