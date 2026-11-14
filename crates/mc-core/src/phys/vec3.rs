//! `world.phys.Vec3` (immutable double vector). Codecs, `Vector3fc` conversions (`new Vec3(Vector3fc)`,
//! `toVector3f`) and `Position` (only `Vec3` implements it) are not ported.

use crate::direction::{Axis, Direction};
use crate::math::java::java_double_to_string;
use crate::math::mth;
use crate::phys::Vec2;
use crate::pos::Vec3i;
use crate::random::RandomSource;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::{Add, Mul, Neg, Sub};

/// `(float)(Math.PI / 180.0)`
const DEG_TO_RAD_F32: f32 = (std::f64::consts::PI / 180.0) as f32;
/// `180.0F / (float)Math.PI`
const RAD_TO_DEG_F32: f32 = 180.0 / (std::f64::consts::PI as f32);

/// `Double.doubleToLongBits` (all NaNs canonicalised).
pub(crate) fn double_to_long_bits(v: f64) -> u64 {
    if v.is_nan() {
        0x7ff8_0000_0000_0000
    } else {
        v.to_bits()
    }
}

/// `(int)(temp ^ temp >>> 32)` of `Double.doubleToLongBits`.
pub(crate) fn double_hash(v: f64) -> i32 {
    let temp = double_to_long_bits(v);
    (temp ^ (temp >> 32)) as i32
}

#[derive(Clone, Copy, Debug)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

/// `Double.compare(a, b) == 0` semantics: `-0.0 != 0.0`, `NaN == NaN`.
impl PartialEq for Vec3 {
    fn eq(&self, other: &Self) -> bool {
        double_to_long_bits(self.x) == double_to_long_bits(other.x)
            && double_to_long_bits(self.y) == double_to_long_bits(other.y)
            && double_to_long_bits(self.z) == double_to_long_bits(other.z)
    }
}
impl Eq for Vec3 {}

impl Hash for Vec3 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        double_to_long_bits(self.x).hash(state);
        double_to_long_bits(self.y).hash(state);
        double_to_long_bits(self.z).hash(state);
    }
}

impl Vec3 {
    pub const ZERO: Vec3 = Vec3::new(0.0, 0.0, 0.0);
    pub const X_AXIS: Vec3 = Vec3::new(1.0, 0.0, 0.0);
    pub const Y_AXIS: Vec3 = Vec3::new(0.0, 1.0, 0.0);
    pub const Z_AXIS: Vec3 = Vec3::new(0.0, 0.0, 1.0);

    pub const fn new(x: f64, y: f64, z: f64) -> Vec3 {
        Vec3 { x, y, z }
    }

    /// `new Vec3(Vec3i)`
    pub fn from_vec3i(vec: &Vec3i) -> Vec3 {
        Vec3::new(vec.x() as f64, vec.y() as f64, vec.z() as f64)
    }

    pub const fn x(&self) -> f64 {
        self.x
    }
    pub const fn y(&self) -> f64 {
        self.y
    }
    pub const fn z(&self) -> f64 {
        self.z
    }

    pub fn at_lower_corner_of(pos: &Vec3i) -> Vec3 {
        Self::from_vec3i(pos)
    }

    pub fn at_lower_corner_with_offset(pos: &Vec3i, x: f64, y: f64, z: f64) -> Vec3 {
        Vec3::new(pos.x() as f64 + x, pos.y() as f64 + y, pos.z() as f64 + z)
    }

    pub fn at_center_of(pos: &Vec3i) -> Vec3 {
        Self::at_lower_corner_with_offset(pos, 0.5, 0.5, 0.5)
    }

    pub fn at_center_of_with_y(pos: &Vec3i, y: f64) -> Vec3 {
        Vec3::new(pos.x() as f64 + 0.5, y, pos.z() as f64 + 0.5)
    }

    pub fn at_bottom_center_of(pos: &Vec3i) -> Vec3 {
        Self::at_lower_corner_with_offset(pos, 0.5, 0.0, 0.5)
    }

    pub fn up_from_bottom_center_of(pos: &Vec3i, y_offset: f64) -> Vec3 {
        Self::at_lower_corner_with_offset(pos, 0.5, y_offset, 0.5)
    }

    pub fn vector_to(&self, vec: Vec3) -> Vec3 {
        Vec3::new(vec.x - self.x, vec.y - self.y, vec.z - self.z)
    }

    pub fn normalize(&self) -> Vec3 {
        let dist = (self.x * self.x + self.y * self.y + self.z * self.z).sqrt();
        if dist < 1.0E-5_f32 as f64 {
            Vec3::ZERO
        } else {
            Vec3::new(self.x / dist, self.y / dist, self.z / dist)
        }
    }

    pub fn dot(&self, vec: Vec3) -> f64 {
        self.x * vec.x + self.y * vec.y + self.z * vec.z
    }

    pub fn cross(&self, vec: Vec3) -> Vec3 {
        Vec3::new(
            self.y * vec.z - self.z * vec.y,
            self.z * vec.x - self.x * vec.z,
            self.x * vec.y - self.y * vec.x,
        )
    }

    pub fn subtract(&self, vec: Vec3) -> Vec3 {
        self.subtract_xyz(vec.x, vec.y, vec.z)
    }

    pub fn subtract_scalar(&self, value: f64) -> Vec3 {
        self.subtract_xyz(value, value, value)
    }

    pub fn subtract_xyz(&self, x: f64, y: f64, z: f64) -> Vec3 {
        self.add_xyz(-x, -y, -z)
    }

    pub fn add_scalar(&self, value: f64) -> Vec3 {
        self.add_xyz(value, value, value)
    }

    pub fn add(&self, vec: Vec3) -> Vec3 {
        self.add_xyz(vec.x, vec.y, vec.z)
    }

    pub fn add_xyz(&self, x: f64, y: f64, z: f64) -> Vec3 {
        Vec3::new(self.x + x, self.y + y, self.z + z)
    }

    /// `closerThan(Position, double)`
    pub fn closer_than(&self, pos: Vec3, distance: f64) -> bool {
        self.distance_to_sqr_xyz(pos.x, pos.y, pos.z) < distance * distance
    }

    /// `closerThan(Vec3, double distanceXZ, double distanceY)`
    pub fn closer_than_xz_y(&self, vec: Vec3, distance_xz: f64, distance_y: f64) -> bool {
        let dx = vec.x - self.x;
        let dy = vec.y - self.y;
        let dz = vec.z - self.z;
        mth::length_squared_f64(dx, dz) < mth::square_f64(distance_xz) && dy.abs() < distance_y
    }

    pub fn distance_to(&self, vec: Vec3) -> f64 {
        let xd = vec.x - self.x;
        let yd = vec.y - self.y;
        let zd = vec.z - self.z;
        (xd * xd + yd * yd + zd * zd).sqrt()
    }

    pub fn distance_to_sqr(&self, vec: Vec3) -> f64 {
        self.distance_to_sqr_xyz(vec.x, vec.y, vec.z)
    }

    pub fn distance_to_sqr_xyz(&self, x: f64, y: f64, z: f64) -> f64 {
        let xd = x - self.x;
        let yd = y - self.y;
        let zd = z - self.z;
        xd * xd + yd * yd + zd * zd
    }

    pub fn scale(&self, scale: f64) -> Vec3 {
        self.multiply_xyz(scale, scale, scale)
    }

    pub fn reverse(&self) -> Vec3 {
        self.scale(-1.0)
    }

    pub fn multiply(&self, scale: Vec3) -> Vec3 {
        self.multiply_xyz(scale.x, scale.y, scale.z)
    }

    pub fn multiply_xyz(&self, x_scale: f64, y_scale: f64, z_scale: f64) -> Vec3 {
        Vec3::new(self.x * x_scale, self.y * y_scale, self.z * z_scale)
    }

    pub fn horizontal(&self) -> Vec3 {
        Vec3::new(self.x, 0.0, self.z)
    }

    pub fn offset_random(&self, random: &mut dyn RandomSource, offset: f32) -> Vec3 {
        let x = (random.next_float() - 0.5) * offset;
        let y = (random.next_float() - 0.5) * offset;
        let z = (random.next_float() - 0.5) * offset;
        self.add_xyz(x as f64, y as f64, z as f64)
    }

    pub fn offset_random_xz(&self, random: &mut dyn RandomSource, offset: f32) -> Vec3 {
        let x = (random.next_float() - 0.5) * offset;
        let z = (random.next_float() - 0.5) * offset;
        self.add_xyz(x as f64, 0.0, z as f64)
    }

    pub fn length(&self) -> f64 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }

    pub fn length_sqr(&self) -> f64 {
        self.x * self.x + self.y * self.y + self.z * self.z
    }

    pub fn horizontal_distance(&self) -> f64 {
        (self.x * self.x + self.z * self.z).sqrt()
    }

    pub fn horizontal_distance_sqr(&self) -> f64 {
        self.x * self.x + self.z * self.z
    }

    /// Java `hashCode()`
    pub fn hash_code(&self) -> i32 {
        let mut result = double_hash(self.x);
        result = result.wrapping_mul(31).wrapping_add(double_hash(self.y));
        result.wrapping_mul(31).wrapping_add(double_hash(self.z))
    }

    pub fn lerp(&self, vec: Vec3, a: f64) -> Vec3 {
        Vec3::new(
            mth::lerp_f64(a, self.x, vec.x),
            mth::lerp_f64(a, self.y, vec.y),
            mth::lerp_f64(a, self.z, vec.z),
        )
    }

    pub fn x_rot(&self, radians: f32) -> Vec3 {
        let cos = mth::cos(radians as f64);
        let sin = mth::sin(radians as f64);
        let xx = self.x;
        let yy = self.y * cos as f64 + self.z * sin as f64;
        let zz = self.z * cos as f64 - self.y * sin as f64;
        Vec3::new(xx, yy, zz)
    }

    pub fn y_rot(&self, radians: f32) -> Vec3 {
        let cos = mth::cos(radians as f64);
        let sin = mth::sin(radians as f64);
        let xx = self.x * cos as f64 + self.z * sin as f64;
        let yy = self.y;
        let zz = self.z * cos as f64 - self.x * sin as f64;
        Vec3::new(xx, yy, zz)
    }

    pub fn z_rot(&self, radians: f32) -> Vec3 {
        let cos = mth::cos(radians as f64);
        let sin = mth::sin(radians as f64);
        let xx = self.x * cos as f64 + self.y * sin as f64;
        let yy = self.y * cos as f64 - self.x * sin as f64;
        let zz = self.z;
        Vec3::new(xx, yy, zz)
    }

    pub fn rotate_clockwise_90(&self) -> Vec3 {
        Vec3::new(-self.z, self.y, self.x)
    }

    /// `directionFromRotation(Vec2)`
    pub fn direction_from_rotation_vec2(rotation: Vec2) -> Vec3 {
        Self::direction_from_rotation(rotation.x, rotation.y)
    }

    pub fn direction_from_rotation(rot_x: f32, rot_y: f32) -> Vec3 {
        let pi = std::f64::consts::PI as f32;
        let y_cos = mth::cos((-rot_y * DEG_TO_RAD_F32 - pi) as f64);
        let y_sin = mth::sin((-rot_y * DEG_TO_RAD_F32 - pi) as f64);
        let x_cos = -mth::cos((-rot_x * DEG_TO_RAD_F32) as f64);
        let x_sin = mth::sin((-rot_x * DEG_TO_RAD_F32) as f64);
        Vec3::new((y_sin * x_cos) as f64, x_sin as f64, (y_cos * x_cos) as f64)
    }

    /// `rotation()`: `(pitch, yaw)` in degrees.
    pub fn rotation(&self) -> Vec2 {
        let yaw = (mth::java_atan2(-self.x, self.z) as f32) * RAD_TO_DEG_F32;
        let pitch =
            (mth::java_asin(-self.y / (self.x * self.x + self.y * self.y + self.z * self.z).sqrt())
                as f32)
                * RAD_TO_DEG_F32;
        Vec2::new(pitch, yaw)
    }

    /// `align(EnumSet<Axis>)`
    pub fn align(&self, axes: &[Axis]) -> Vec3 {
        let x = if axes.contains(&Axis::X) {
            mth::floor_f64(self.x) as f64
        } else {
            self.x
        };
        let y = if axes.contains(&Axis::Y) {
            mth::floor_f64(self.y) as f64
        } else {
            self.y
        };
        let z = if axes.contains(&Axis::Z) {
            mth::floor_f64(self.z) as f64
        } else {
            self.z
        };
        Vec3::new(x, y, z)
    }

    /// `get(Axis)`
    pub fn get(&self, axis: Axis) -> f64 {
        axis.choose(self.x, self.y, self.z)
    }

    /// `with(Axis, double)`
    pub fn with(&self, axis: Axis, value: f64) -> Vec3 {
        let x = if axis == Axis::X { value } else { self.x };
        let y = if axis == Axis::Y { value } else { self.y };
        let z = if axis == Axis::Z { value } else { self.z };
        Vec3::new(x, y, z)
    }

    pub fn relative(&self, direction: Direction, distance: f64) -> Vec3 {
        Vec3::new(
            self.x + distance * direction.get_step_x() as f64,
            self.y + distance * direction.get_step_y() as f64,
            self.z + distance * direction.get_step_z() as f64,
        )
    }

    pub fn projected_on(&self, onto: Vec3) -> Vec3 {
        if onto.length_sqr() == 0.0 {
            onto
        } else {
            onto.scale(self.dot(onto)).scale(1.0 / onto.length_sqr())
        }
    }

    pub fn is_finite(&self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }
}

impl Add for Vec3 {
    type Output = Vec3;
    fn add(self, rhs: Vec3) -> Vec3 {
        self.add_xyz(rhs.x, rhs.y, rhs.z)
    }
}

impl Sub for Vec3 {
    type Output = Vec3;
    fn sub(self, rhs: Vec3) -> Vec3 {
        self.subtract(rhs)
    }
}

impl Mul<f64> for Vec3 {
    type Output = Vec3;
    fn mul(self, rhs: f64) -> Vec3 {
        self.scale(rhs)
    }
}

impl Neg for Vec3 {
    type Output = Vec3;
    fn neg(self) -> Vec3 {
        self.reverse()
    }
}

/// Vanilla `Vec3.toString`: `(x, y, z)` with Java `Double.toString` number formatting.
impl fmt::Display for Vec3 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [x, y, z] = [self.x, self.y, self.z].map(java_double_to_string);
        write!(f, "({x}, {y}, {z})")
    }
}
