//! Golden tests for Direction / AxisCycle / CompositeDirection / Rotation / Mirror / Vec2 / Vec3 / AABB against the
//! vanilla oracle `fixtures/geometry_dir/GeometryDir.java` (output: `geometry_dir.txt`).
//!
//! Fixture line format: `op in-tokens = out-tokens`; doubles are 16 hex digits of the (canonical-NaN) bit pattern,
//! floats 8 hex digits, ints decimal, booleans 0/1, `null` = absent, `ERR` = vanilla threw.

use mc_core::direction::{Axis, AxisCycle, AxisDirection, CompositeDirection, Direction, Direction8, Directional, Plane};
use mc_core::mirror::Mirror;
use mc_core::phys::aabb::Builder;
use mc_core::phys::{Vec2, Vec3, AABB};
use mc_core::pos::{BlockPos, Vec3i};
use mc_core::random::{self, RandomSource};
use mc_core::rotation::Rotation;
use std::collections::BTreeMap;
use std::panic::{catch_unwind, AssertUnwindSafe};

const FIXTURE: &str = include_str!("../fixtures/geometry_dir/geometry_dir.txt");

// ---- formatting (Rust values -> fixture tokens) ----
fn d(v: f64) -> String {
    format!("{:016x}", if v.is_nan() { 0x7ff8_0000_0000_0000 } else { v.to_bits() })
}
fn f(v: f32) -> String {
    format!("{:08x}", if v.is_nan() { 0x7fc0_0000 } else { v.to_bits() })
}
fn b(v: bool) -> String {
    (v as u8).to_string()
}
fn dir(v: Option<Direction>) -> String {
    v.map_or("null".to_string(), |d| d.get_name().to_string())
}
fn v3(v: Vec3) -> String {
    format!("{} {} {}", d(v.x), d(v.y), d(v.z))
}
fn ov3(v: Option<Vec3>) -> String {
    v.map_or("null".to_string(), v3)
}
fn v2(v: Vec2) -> String {
    format!("{} {}", f(v.x), f(v.y))
}
fn bb(a: &AABB) -> String {
    [a.min_x, a.min_y, a.min_z, a.max_x, a.max_y, a.max_z].map(d).join(" ")
}
fn guard<T>(g: impl FnOnce() -> T, fmt: impl FnOnce(T) -> String) -> String {
    catch_unwind(AssertUnwindSafe(g)).map_or("ERR".to_string(), fmt)
}
fn ax(a: Axis) -> String {
    a.get_name().to_string()
}
fn rot_name(r: Rotation) -> &'static str {
    match r {
        Rotation::None => "NONE",
        Rotation::Clockwise90 => "CLOCKWISE_90",
        Rotation::Clockwise180 => "CLOCKWISE_180",
        Rotation::Counterclockwise90 => "COUNTERCLOCKWISE_90",
    }
}
fn mirror_name(m: Mirror) -> &'static str {
    match m {
        Mirror::None => "NONE",
        Mirror::LeftRight => "LEFT_RIGHT",
        Mirror::FrontBack => "FRONT_BACK",
    }
}
fn cycle_name(c: AxisCycle) -> &'static str {
    match c {
        AxisCycle::None => "NONE",
        AxisCycle::Forward => "FORWARD",
        AxisCycle::Backward => "BACKWARD",
    }
}
fn ad_name(a: AxisDirection) -> &'static str {
    match a {
        AxisDirection::Positive => "POSITIVE",
        AxisDirection::Negative => "NEGATIVE",
    }
}
fn plane_name(p: Plane) -> &'static str {
    match p {
        Plane::Horizontal => "HORIZONTAL",
        Plane::Vertical => "VERTICAL",
    }
}
fn dirs(v: &[Direction]) -> String {
    v.iter().map(|d| format!("{},", d.get_name())).collect()
}

// ---- parsing (fixture tokens -> Rust values) ----
struct Toks<'a> {
    t: Vec<&'a str>,
    i: usize,
}
impl<'a> Toks<'a> {
    fn s(&mut self) -> &'a str {
        self.i += 1;
        self.t[self.i - 1]
    }
    fn d(&mut self) -> f64 {
        f64::from_bits(u64::from_str_radix(self.s(), 16).unwrap())
    }
    fn f(&mut self) -> f32 {
        f32::from_bits(u32::from_str_radix(self.s(), 16).unwrap())
    }
    fn i(&mut self) -> i32 {
        self.s().parse().unwrap()
    }
    fn l(&mut self) -> i64 {
        self.s().parse().unwrap()
    }
    fn dir(&mut self) -> Direction {
        Direction::by_name(self.s()).unwrap()
    }
    fn axis(&mut self) -> Axis {
        Axis::by_name(self.s()).unwrap()
    }
    fn rot(&mut self) -> Rotation {
        let s = self.s();
        Rotation::ALL.into_iter().find(|r| rot_name(*r) == s).unwrap()
    }
    fn mirror(&mut self) -> Mirror {
        let s = self.s();
        Mirror::ALL.into_iter().find(|m| mirror_name(*m) == s).unwrap()
    }
    fn cycle(&mut self) -> AxisCycle {
        let s = self.s();
        AxisCycle::VALUES.into_iter().find(|c| cycle_name(*c) == s).unwrap()
    }
    fn v3(&mut self) -> Vec3 {
        Vec3::new(self.d(), self.d(), self.d())
    }
    fn v2(&mut self) -> Vec2 {
        Vec2::new(self.f(), self.f())
    }
    fn bb(&mut self) -> AABB {
        // fixture boxes are already sorted vanilla AABBs, so `new` is the identity here
        AABB::new(self.d(), self.d(), self.d(), self.d(), self.d(), self.d())
    }
    fn pos(&mut self) -> BlockPos {
        BlockPos::new(self.i(), self.i(), self.i())
    }
}

fn run(op: &str, mut t: Toks) -> String {
    match op {
        "dir_table" => {
            let x = t.dir();
            [
                x.ordinal().to_string(),
                x.get_3d_data_value().to_string(),
                x.get_2d_data_value().to_string(),
                ax(x.get_axis()),
                x.get_axis_direction().get_step().to_string(),
                dir(Some(x.get_opposite())),
                x.get_step_x().to_string(),
                x.get_step_y().to_string(),
                x.get_step_z().to_string(),
                f(x.to_y_rot()),
                guard(|| Direction::get_y_rot(x), f),
                guard(|| x.get_clockwise(), |r| dir(Some(r))),
                guard(|| x.get_counter_clockwise(), |r| dir(Some(r))),
                v3(x.get_unit_vec3()),
            ]
            .join(" ")
        }
        "dir_cw_axis" => {
            let (x, a) = (t.dir(), t.axis());
            format!(
                "{} {}",
                guard(|| x.get_clockwise_axis(a), |r| dir(Some(r))),
                guard(|| x.get_counter_clockwise_axis(a), |r| dir(Some(r)))
            )
        }
        "axis_table" => {
            let a = t.axis();
            [
                a.ordinal().to_string(),
                b(a.is_vertical()),
                b(a.is_horizontal()),
                dir(Some(a.get_positive())),
                dir(Some(a.get_negative())),
                plane_name(a.get_plane()).to_string(),
                a.choose(1, 2, 3).to_string(),
                d(a.choose(1.5, 2.5, 3.5)),
                b(a.choose(true, false, false)),
            ]
            .join(" ")
        }
        "axis_test" => {
            let (a, x) = (t.axis(), t.dir());
            b(a.test(x))
        }
        "from_axis_dir" => {
            let a = t.axis();
            let ad = if t.s() == "POSITIVE" { AxisDirection::Positive } else { AxisDirection::Negative };
            format!("{} {}", dir(Some(Direction::from_axis_and_direction(a, ad))), dir(Some(Direction::get(ad, a))))
        }
        "axis_direction" => {
            let ad = if t.s() == "POSITIVE" { AxisDirection::Positive } else { AxisDirection::Negative };
            format!("{} {} {}", ad.get_step(), ad_name(ad.opposite()), ad.get_name().replace(' ', "_"))
        }
        "plane" => {
            let p = if t.s() == "HORIZONTAL" { Plane::Horizontal } else { Plane::Vertical };
            let tests: String = Direction::ALL.iter().map(|x| b(p.test(*x))).collect();
            let faces: Vec<Direction> = p.into_iter().collect();
            format!("{} {} {}", p.length(), dirs(&faces), tests)
        }
        "from3d" => {
            let i = t.i();
            format!(
                "{} {} {}",
                dir(Some(Direction::from_3d_data_value(i))),
                dir(Some(Direction::from_2d_data_value(i))),
                dir(Some(Direction::by_id(i)))
            )
        }
        "from_yrot" => dir(Some(Direction::from_y_rot(t.d()))),
        "nearest_int" => {
            let (x, y, z) = (t.i(), t.i(), t.i());
            format!("{} {}", dir(Direction::get_nearest(x, y, z, None)), dir(Direction::get_nearest(x, y, z, Some(Direction::Up))))
        }
        "approx_nearest" => {
            let (x, y, z) = (t.d(), t.d(), t.d());
            format!(
                "{} {} {}",
                dir(Some(Direction::get_approximate_nearest(x, y, z))),
                dir(Some(Direction::get_approximate_nearest_f32(x as f32, y as f32, z as f32))),
                dir(Some(Direction::get_approximate_nearest_vec3(Vec3::new(x, y, z))))
            )
        }
        "facing_angle" => {
            let yaw = t.f();
            Direction::ALL.iter().map(|x| b(x.is_facing_angle(yaw))).collect()
        }
        "ordered_by_nearest" => {
            let (pitch, yaw) = (t.f(), t.f());
            format!(
                "{} {} {} {}",
                dirs(&Direction::ordered_by_nearest(pitch, yaw)),
                dir(Some(Direction::get_facing_axis(Axis::X, pitch, yaw))),
                dir(Some(Direction::get_facing_axis(Axis::Y, pitch, yaw))),
                dir(Some(Direction::get_facing_axis(Axis::Z, pitch, yaw)))
            )
        }
        "axis_step_order" => Direction::axis_step_order(t.v3()).iter().map(|a| a.get_name()).collect(),
        "dir_random" => {
            let mut r = random::create(t.l());
            let mut s = String::new();
            for _ in 0..6 {
                s += &format!("{},", Direction::get_random(&mut r).get_name());
            }
            for _ in 0..3 {
                s += &format!("{},", Axis::get_random(&mut r).get_name());
            }
            for p in [Plane::Horizontal, Plane::Vertical] {
                for _ in 0..3 {
                    s += &format!("{},{},", p.get_random_direction(&mut r).get_name(), p.get_random_axis(&mut r).get_name());
                }
                s += &dirs(&p.shuffled_copy(&mut r));
            }
            s += &dirs(&Direction::all_shuffled(&mut r));
            format!("{s} {}", r.next_int())
        }
        "cycle_inverse" => cycle_name(t.cycle().inverse()).to_string(),
        "cycle" => {
            let (c, a) = (t.cycle(), t.axis());
            format!("{} {} {}", ax(c.cycle_axis(a)), c.cycle_i32(10, 20, 30, a), d(c.cycle_f64(1.5, 2.5, 3.5, a)))
        }
        "cycle_between" => {
            let (a, c) = (t.axis(), t.axis());
            cycle_name(AxisCycle::between(a, c)).to_string()
        }
        "composite" => {
            let c = match t.s() {
                "NORTH" => CompositeDirection::NORTH,
                "EAST" => CompositeDirection::EAST,
                "SOUTH" => CompositeDirection::SOUTH,
                "WEST" => CompositeDirection::WEST,
                "NORTH_EAST" => CompositeDirection::NORTH_EAST,
                "SOUTH_EAST" => CompositeDirection::SOUTH_EAST,
                "SOUTH_WEST" => CompositeDirection::SOUTH_WEST,
                "NORTH_WEST" => CompositeDirection::NORTH_WEST,
                "UP" => CompositeDirection::UP,
                "NORTH_UP" => CompositeDirection::NORTH_UP,
                "EAST_UP" => CompositeDirection::EAST_UP,
                "SOUTH_UP" => CompositeDirection::SOUTH_UP,
                "WEST_UP" => CompositeDirection::WEST_UP,
                "NORTH_EAST_UP" => CompositeDirection::NORTH_EAST_UP,
                "SOUTH_EAST_UP" => CompositeDirection::SOUTH_EAST_UP,
                "SOUTH_WEST_UP" => CompositeDirection::SOUTH_WEST_UP,
                "NORTH_WEST_UP" => CompositeDirection::NORTH_WEST_UP,
                "DOWN" => CompositeDirection::DOWN,
                "NORTH_DOWN" => CompositeDirection::NORTH_DOWN,
                "EAST_DOWN" => CompositeDirection::EAST_DOWN,
                "SOUTH_DOWN" => CompositeDirection::SOUTH_DOWN,
                "WEST_DOWN" => CompositeDirection::WEST_DOWN,
                "NORTH_EAST_DOWN" => CompositeDirection::NORTH_EAST_DOWN,
                "SOUTH_EAST_DOWN" => CompositeDirection::SOUTH_EAST_DOWN,
                "SOUTH_WEST_DOWN" => CompositeDirection::SOUTH_WEST_DOWN,
                "NORTH_WEST_DOWN" => CompositeDirection::NORTH_WEST_DOWN,
                other => panic!("unknown composite {other}"),
            };
            format!("{} {} {} {}", dirs(&c.directions()), c.get_step_x(), c.get_step_y(), c.get_step_z())
        }
        "direction8" => {
            let name = t.s();
            let d8 = Direction8::ALL.into_iter().find(|d| format!("{d:?}").to_uppercase().replace("NORTHEAST", "NORTH_EAST").replace("SOUTHEAST", "SOUTH_EAST").replace("SOUTHWEST", "SOUTH_WEST").replace("NORTHWEST", "NORTH_WEST") == name).unwrap();
            format!("{} {} {} {}", dirs(&d8.get_directions()), d8.get_step_x(), d8.get_step_y(), d8.get_step_z())
        }
        "rotation" => {
            let r = t.rot();
            format!("{} {}", r.get_serialized_name(), rot_name(Rotation::by_id(r.get_index())))
        }
        "rotation_rotated" => {
            let (r, o) = (t.rot(), t.rot());
            rot_name(r.get_rotated(o)).to_string()
        }
        "rotation_dir" => {
            let (r, x) = (t.rot(), t.dir());
            dir(Some(r.rotate(x)))
        }
        "rotation_steps" => {
            let (r, steps) = (t.rot(), t.i());
            (-2..=steps + 2).map(|i| format!("{},", r.rotate_steps(i, steps))).collect()
        }
        "rotation_by_id" => rot_name(Rotation::by_id(t.i())).to_string(),
        "rotation_random" => {
            let mut r = random::create(t.l());
            let mut s = String::new();
            for _ in 0..6 {
                s += &format!("{},", rot_name(Rotation::get_random(&mut r)));
            }
            for x in Rotation::get_shuffled(&mut r) {
                s += &format!("{},", rot_name(x));
            }
            format!("{s} {}", r.next_int())
        }
        "mirror" => t.mirror().get_serialized_name().to_string(),
        "mirror_dir" => {
            let (m, x) = (t.mirror(), t.dir());
            format!("{} {}", dir(Some(m.mirror(x))), rot_name(m.get_rotation(x)))
        }
        "mirror_steps" => {
            let (m, steps) = (t.mirror(), t.i());
            (-2..=steps + 2).map(|i| format!("{},", m.mirror_steps(i, steps))).collect()
        }
        "vec2_unary" => {
            let a = t.v2();
            [
                v2(a.scale(2.5)),
                v2(a.add_scalar(0.75)),
                v2(a.normalized()),
                f(a.length()),
                f(a.length_squared()),
                v2(a.negated()),
                v2(a.rotate(0.7)),
                v2(a.rotate(-3.0)),
                v2(a.rotate(100.5)),
            ]
            .join(" ")
        }
        "vec2_binary" => {
            let (a, c) = (t.v2(), t.v2());
            format!("{} {} {} {}", f(a.dot(c)), v2(a.add(c)), f(a.distance_to_sqr(c)), b(a == c))
        }
        "vec3_flags" => {
            let a = t.v3();
            let copy = Vec3::new(a.x, a.y, a.z);
            format!("{} {} {} {} {} {}", b(a.is_finite()), b(a == a), b(a == copy), a.hash_code(), d(a.length()), d(a.length_sqr()))
        }
        "vec3_special_unary" => {
            let a = t.v3();
            format!("{} {} {} {} {}", v3(a.normalize()), v3(a.reverse()), v3(a.horizontal()), d(a.horizontal_distance()), d(a.horizontal_distance_sqr()))
        }
        "vec3_unary" => {
            let a = t.v3();
            [
                v3(a.normalize()),
                v3(a.reverse()),
                v3(a.horizontal()),
                d(a.length()),
                d(a.length_sqr()),
                d(a.horizontal_distance()),
                d(a.horizontal_distance_sqr()),
                v3(a.rotate_clockwise_90()),
                v2(a.rotation()),
            ]
            .join(" ")
        }
        "vec3_rot" => {
            let (a, r) = (t.v3(), t.f());
            format!("{} {} {}", v3(a.x_rot(r)), v3(a.y_rot(r)), v3(a.z_rot(r)))
        }
        "vec3_scalar" => {
            let (a, s) = (t.v3(), t.d());
            [
                v3(a.scale(s)),
                v3(a.add_scalar(s)),
                v3(a.subtract_scalar(s)),
                v3(a.multiply_xyz(s, -s, 2.0 * s)),
                v3(a.add_xyz(s, -s, 2.0 * s)),
                v3(a.subtract_xyz(s, -s, 2.0 * s)),
                d(a.distance_to_sqr_xyz(s, -s, 2.0 * s)),
                b(a.closer_than(Vec3::ZERO, s)),
                v3(a.relative(Direction::Up, s)),
                v3(a.relative(Direction::West, s)),
                v3(a.relative(Direction::South, s)),
            ]
            .join(" ")
        }
        "vec3_axis" => {
            let (a, s, axis) = (t.v3(), t.d(), t.axis());
            format!("{} {}", d(a.get(axis)), v3(a.with(axis, s)))
        }
        "vec3_align" => {
            let (a, mask) = (t.v3(), t.i());
            let axes: Vec<Axis> = Axis::ALL.into_iter().filter(|x| mask >> x.ordinal() & 1 != 0).collect();
            v3(a.align(&axes))
        }
        "vec3_binary" => {
            let (a, c) = (t.v3(), t.v3());
            [
                v3(a.add(c)),
                v3(a.subtract(c)),
                v3(a.cross(c)),
                d(a.dot(c)),
                v3(a.vector_to(c)),
                d(a.distance_to(c)),
                d(a.distance_to_sqr(c)),
                v3(a.multiply(c)),
                v3(a.projected_on(c)),
                b(a == c),
                b(a.closer_than(c, 3.0)),
                b(a.closer_than_xz_y(c, 2.0, 1.0)),
                b(a.closer_than_xz_y(c, 0.0, 0.0)),
            ]
            .join(" ")
        }
        "vec3_lerp" => {
            let (a, c, s) = (t.v3(), t.v3(), t.d());
            v3(a.lerp(c, s))
        }
        "vec3_direction_from_rotation" => {
            let (rx, ry) = (t.f(), t.f());
            format!("{} {}", v3(Vec3::direction_from_rotation(rx, ry)), v3(Vec3::direction_from_rotation_vec2(Vec2::new(rx, ry))))
        }
        "vec3_offset_random" => {
            let mut r = random::create(t.l());
            let base = Vec3::new(1.5, -2.5, 3.5);
            let a = base.offset_random(&mut r, 2.5);
            let c = base.offset_random_xz(&mut r, 0.75);
            format!("{} {} {}", v3(a), v3(c), r.next_int())
        }
        "vec3_at" => {
            let p = Vec3i::new(t.i(), t.i(), t.i());
            [
                v3(Vec3::at_lower_corner_of(&p)),
                v3(Vec3::at_lower_corner_with_offset(&p, 0.1, -0.2, 0.3)),
                v3(Vec3::at_center_of(&p)),
                v3(Vec3::at_center_of_with_y(&p, 7.5)),
                v3(Vec3::at_bottom_center_of(&p)),
                v3(Vec3::up_from_bottom_center_of(&p, 0.25)),
                v3(Vec3::from_vec3i(&p)),
            ]
            .join(" ")
        }
        "aabb_new" => b(t.bb().has_nan()),
        "aabb_flags" => {
            let a = t.bb();
            let copy = AABB::new(a.min_x, a.min_y, a.min_z, a.max_x, a.max_y, a.max_z);
            format!(
                "{} {} {} {} {} {}",
                b(a.has_nan()),
                a.hash_code(),
                b(a == copy),
                bb(&a.next_deflated()),
                v3(a.get_min_position()),
                v3(a.get_max_position())
            )
        }
        "aabb_dims" => {
            let a = t.bb();
            format!(
                "{} {} {} {} {} {}",
                d(a.get_size()),
                d(a.get_xsize()),
                d(a.get_ysize()),
                d(a.get_zsize()),
                v3(a.get_center()),
                v3(a.get_bottom_center())
            )
        }
        "aabb_ctor" => {
            let (x, y) = (t.d(), t.d());
            format!("{} {}", bb(&AABB::new(x, y, x, y, x, y)), bb(&AABB::new(y, x, y, x, y, x)))
        }
        "aabb_axis" => {
            let (a, axis) = (t.bb(), t.axis());
            format!("{} {}", d(a.min(axis)), d(a.max(axis)))
        }
        "aabb_set" => {
            let (a, lo, hi) = (t.bb(), t.d(), t.d());
            [a.set_min_x(lo), a.set_min_y(hi), a.set_min_z(lo), a.set_max_x(hi), a.set_max_y(lo), a.set_max_z(hi)].iter().map(bb).collect::<Vec<_>>().join(" ")
        }
        "aabb_delta" => {
            let (a, dl) = (t.bb(), t.v3());
            [
                bb(&a.contract(dl.x, dl.y, dl.z)),
                bb(&a.expand_towards_vec3(dl)),
                bb(&a.expand_towards(dl.x, dl.y, dl.z)),
                bb(&a.inflate(dl.x, dl.y, dl.z)),
                bb(&a.deflate(dl.x, dl.y, dl.z)),
                bb(&a.inflate_scalar(dl.x)),
                bb(&a.deflate_scalar(dl.y)),
                bb(&a.move_vec(dl)),
                bb(&a.move_(dl.x, dl.y, dl.z)),
                bb(&AABB::of_size(dl, a.get_xsize(), a.get_ysize(), a.get_zsize())),
                bb(&AABB::unit_cube_from_lower_corner(dl)),
                bb(&AABB::from_corners(dl, dl.add(a.get_center()))),
                b(a.contains_vec3(dl)),
                b(a.contains(dl.x, dl.y, dl.z)),
                d(a.distance_to_sqr(dl)),
                b(a.intersects_vec3(dl, dl.add_xyz(1.0, 1.0, 1.0))),
                b(a.intersects_vec3(dl.add_xyz(1.0, 1.0, 1.0), dl)),
            ]
            .join(" ")
        }
        "aabb_binary" => {
            let (a, c) = (t.bb(), t.bb());
            [
                bb(&a.intersect(&c)),
                bb(&a.minmax(&c)),
                b(a.intersects_aabb(&c)),
                b(a.intersects_coords(c.min_x, c.min_y, c.min_z, c.max_x, c.max_y, c.max_z)),
                d(a.distance_to_sqr_aabb(&c)),
                b(a == c),
            ]
            .join(" ")
        }
        "aabb_pos" => {
            let (a, p) = (t.bb(), t.pos());
            format!("{} {} {}", bb(&a.move_block_pos(&p)), b(a.intersects_block_pos(&p)), bb(&AABB::from_block_pos(&p)))
        }
        "aabb_full_blocks" => {
            let (p, q) = (t.pos(), t.pos());
            bb(&AABB::encapsulating_full_blocks(&p, &q))
        }
        "aabb_of_box" => {
            let n: Vec<i32> = (0..6).map(|_| t.i()).collect();
            bb(&AABB::of_box(n[0], n[1], n[2], n[3], n[4], n[5]))
        }
        "aabb_clip" => {
            let (a, from, to) = (t.bb(), t.v3(), t.v3());
            format!(
                "{} {}",
                ov3(a.clip(from, to)),
                ov3(AABB::clip_box_coords(a.min_x, a.min_y, a.min_z, a.max_x, a.max_y, a.max_z, from, to))
            )
        }
        "aabb_clip_boxes" => {
            let n = t.i();
            let boxes: Vec<AABB> = (0..n).map(|_| t.bb()).collect();
            let (pos, from, to) = (t.pos(), t.v3(), t.v3());
            match AABB::clip_boxes(&boxes, from, to, pos) {
                None => "null".to_string(),
                Some(h) => format!(
                    "{} {} {} {} {} {} {} {}",
                    v3(h.location),
                    h.direction.get_name(),
                    h.block_pos.x(),
                    h.block_pos.y(),
                    h.block_pos.z(),
                    b(h.inside),
                    b(h.miss),
                    b(h.world_border_hit)
                ),
            }
        }
        "aabb_collided" => {
            let (a, dl) = (t.bb(), t.v3());
            let n = t.i();
            let boxes: Vec<AABB> = (0..n).map(|_| t.bb()).collect();
            b(a.collided_along_vector(dl, &boxes))
        }
        "aabb_builder" => b(Builder::default().is_defined()),
        "aabb_builder_step" => {
            // inputs are the cumulative point list; replay all points
            let mut bld = Builder::default();
            while t.i < t.t.len() {
                bld.include(t.f(), t.f(), t.f());
            }
            format!("{} {}", b(bld.is_defined()), bb(&bld.build()))
        }
        other => panic!("unhandled fixture op {other}"),
    }
}

#[test]
fn golden_geometry_dir() {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    let mut failures = Vec::new();
    for (n, line) in FIXTURE.lines().enumerate() {
        let (lhs, expected) = line.split_once(" = ").expect("fixture line");
        let (op, input) = lhs.split_once(' ').expect("op");
        // Rust panics inside `run` (e.g. a missing ERR guard) must fail the test, so no catch here.
        let actual = run(op, Toks { t: input.split(' ').collect(), i: 0 });
        *counts.entry(op).or_default() += 1;
        if actual != expected && failures.len() < 20 {
            failures.push(format!("line {}: {op} {input}\n  expected: {expected}\n  actual:   {actual}", n + 1));
        }
    }
    assert!(failures.is_empty(), "{} shown mismatches:\n{}", failures.len(), failures.join("\n"));
    assert!(counts.len() >= 55, "fixture lost ops: {counts:?}");
}

#[test]
fn directional_trait_matches_inherent() {
    for x in Direction::ALL {
        assert_eq!(Directional::get_step(&x), x.get_unit_vec3i());
    }
}
