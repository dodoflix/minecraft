//! `net.minecraft.core.Vec3i`: immutable integer vector (Java `int` wrapping arithmetic).
use crate::direction::{Axis, Direction};
use crate::math::mth;
use crate::phys::Vec3;
use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub struct Vec3i {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

/// Guava `MoreObjects.toStringHelper(name)` format shared by Vec3i, BlockPos and SectionPos.
pub(crate) fn fmt_named(
    f: &mut fmt::Formatter<'_>,
    name: &str,
    x: i32,
    y: i32,
    z: i32,
) -> fmt::Result {
    write!(f, "{name}{{x={x}, y={y}, z={z}}}")
}

impl Vec3i {
    pub const ZERO: Vec3i = Vec3i::new(0, 0, 0);

    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Vec3i { x, y, z }
    }

    pub const fn x(&self) -> i32 {
        self.x
    }

    pub const fn y(&self) -> i32 {
        self.y
    }

    pub const fn z(&self) -> i32 {
        self.z
    }

    /// Java `hashCode()`.
    pub const fn hash_code(&self) -> i32 {
        self.y
            .wrapping_add(self.z.wrapping_mul(31))
            .wrapping_mul(31)
            .wrapping_add(self.x)
    }

    /// Java `compareTo` (wrapping subtraction, so only the sign is meaningful).
    pub const fn compare_to(&self, pos: &Vec3i) -> i32 {
        if self.y == pos.y {
            if self.z == pos.z {
                self.x.wrapping_sub(pos.x)
            } else {
                self.z.wrapping_sub(pos.z)
            }
        } else {
            self.y.wrapping_sub(pos.y)
        }
    }

    pub const fn offset(&self, x: i32, y: i32, z: i32) -> Vec3i {
        Vec3i::new(
            self.x.wrapping_add(x),
            self.y.wrapping_add(y),
            self.z.wrapping_add(z),
        )
    }

    pub const fn offset_vec(&self, vec: &Vec3i) -> Vec3i {
        self.offset(vec.x, vec.y, vec.z)
    }

    pub const fn subtract(&self, vec: &Vec3i) -> Vec3i {
        self.offset(
            vec.x.wrapping_neg(),
            vec.y.wrapping_neg(),
            vec.z.wrapping_neg(),
        )
    }

    pub const fn multiply(&self, scale: i32) -> Vec3i {
        Vec3i::new(
            self.x.wrapping_mul(scale),
            self.y.wrapping_mul(scale),
            self.z.wrapping_mul(scale),
        )
    }

    pub const fn multiply_xyz(&self, x_scale: i32, y_scale: i32, z_scale: i32) -> Vec3i {
        Vec3i::new(
            self.x.wrapping_mul(x_scale),
            self.y.wrapping_mul(y_scale),
            self.z.wrapping_mul(z_scale),
        )
    }

    pub fn above(&self) -> Vec3i {
        self.relative_n(Direction::Up, 1)
    }

    pub fn above_n(&self, steps: i32) -> Vec3i {
        self.relative_n(Direction::Up, steps)
    }

    pub fn below(&self) -> Vec3i {
        self.relative_n(Direction::Down, 1)
    }

    pub fn below_n(&self, steps: i32) -> Vec3i {
        self.relative_n(Direction::Down, steps)
    }

    pub fn north(&self) -> Vec3i {
        self.relative_n(Direction::North, 1)
    }

    pub fn north_n(&self, steps: i32) -> Vec3i {
        self.relative_n(Direction::North, steps)
    }

    pub fn south(&self) -> Vec3i {
        self.relative_n(Direction::South, 1)
    }

    pub fn south_n(&self, steps: i32) -> Vec3i {
        self.relative_n(Direction::South, steps)
    }

    pub fn west(&self) -> Vec3i {
        self.relative_n(Direction::West, 1)
    }

    pub fn west_n(&self, steps: i32) -> Vec3i {
        self.relative_n(Direction::West, steps)
    }

    pub fn east(&self) -> Vec3i {
        self.relative_n(Direction::East, 1)
    }

    pub fn east_n(&self, steps: i32) -> Vec3i {
        self.relative_n(Direction::East, steps)
    }

    pub fn relative(&self, direction: Direction) -> Vec3i {
        self.relative_n(direction, 1)
    }

    pub fn relative_n(&self, direction: Direction, steps: i32) -> Vec3i {
        Vec3i::new(
            self.x
                .wrapping_add(direction.get_step_x().wrapping_mul(steps)),
            self.y
                .wrapping_add(direction.get_step_y().wrapping_mul(steps)),
            self.z
                .wrapping_add(direction.get_step_z().wrapping_mul(steps)),
        )
    }

    pub fn relative_axis(&self, axis: Axis, steps: i32) -> Vec3i {
        let x_step = if axis == Axis::X { steps } else { 0 };
        let y_step = if axis == Axis::Y { steps } else { 0 };
        let z_step = if axis == Axis::Z { steps } else { 0 };
        self.offset(x_step, y_step, z_step)
    }

    pub const fn cross(&self, up_vector: &Vec3i) -> Vec3i {
        Vec3i::new(
            self.y
                .wrapping_mul(up_vector.z)
                .wrapping_sub(self.z.wrapping_mul(up_vector.y)),
            self.z
                .wrapping_mul(up_vector.x)
                .wrapping_sub(self.x.wrapping_mul(up_vector.z)),
            self.x
                .wrapping_mul(up_vector.y)
                .wrapping_sub(self.y.wrapping_mul(up_vector.x)),
        )
    }

    pub fn closer_than(&self, pos: &Vec3i, distance: f64) -> bool {
        self.dist_sqr(pos) < mth::square_f64(distance)
    }

    pub fn closer_to_center_than(&self, pos: &Vec3, distance: f64) -> bool {
        self.dist_to_center_sqr(pos) < mth::square_f64(distance)
    }

    pub fn dist_sqr(&self, pos: &Vec3i) -> f64 {
        self.dist_to_low_corner_sqr(pos.x as f64, pos.y as f64, pos.z as f64)
    }

    pub fn dist_to_center_sqr(&self, pos: &Vec3) -> f64 {
        self.dist_to_center_sqr_xyz(pos.x, pos.y, pos.z)
    }

    pub fn dist_to_center_sqr_xyz(&self, x: f64, y: f64, z: f64) -> f64 {
        let dx = self.x as f64 + 0.5 - x;
        let dy = self.y as f64 + 0.5 - y;
        let dz = self.z as f64 + 0.5 - z;
        dx * dx + dy * dy + dz * dz
    }

    pub fn dist_to_low_corner_sqr(&self, x: f64, y: f64, z: f64) -> f64 {
        let dx = self.x as f64 - x;
        let dy = self.y as f64 - y;
        let dz = self.z as f64 - z;
        dx * dx + dy * dy + dz * dz
    }

    /// Java computes the three `abs` differences as `int` then widens each to `float`.
    pub fn dist_manhattan(&self, pos: &Vec3i) -> i32 {
        let xd = pos.x.wrapping_sub(self.x).wrapping_abs() as f32;
        let yd = pos.y.wrapping_sub(self.y).wrapping_abs() as f32;
        let zd = pos.z.wrapping_sub(self.z).wrapping_abs() as f32;
        (xd + yd + zd) as i32
    }

    pub fn dist_chessboard(&self, pos: &Vec3i) -> i32 {
        let xd = self.x.wrapping_sub(pos.x).wrapping_abs();
        let yd = self.y.wrapping_sub(pos.y).wrapping_abs();
        let zd = self.z.wrapping_sub(pos.z).wrapping_abs();
        xd.max(yd).max(zd)
    }

    pub const fn differs_horizontally(&self, pos: &Vec3i) -> bool {
        self.x != pos.x || self.z != pos.z
    }

    pub fn get(&self, axis: Axis) -> i32 {
        axis.choose(self.x, self.y, self.z)
    }

    pub fn to_short_string(&self) -> String {
        format!("{}, {}, {}", self.x, self.y, self.z)
    }
}

impl Hash for Vec3i {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_i32(self.hash_code());
    }
}

impl PartialOrd for Vec3i {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Vanilla order (y, then z, then x). Exact except where Java's wrapping subtraction would overflow
/// (coordinate differences beyond 2^31); use [`Vec3i::compare_to`] for that.
impl Ord for Vec3i {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.y, self.z, self.x).cmp(&(other.y, other.z, other.x))
    }
}

impl fmt::Display for Vec3i {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt_named(f, "Vec3i", self.x, self.y, self.z)
    }
}

impl fmt::Debug for Vec3i {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
