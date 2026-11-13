//! `org.joml.Quaternionf` and `org.joml.AxisAngle4f`: the parts vanilla's display transformations use, with JOML's float
//! arithmetic (every `Math.fma` is a plain multiply-add, see `joml`).

use super::joml::{cos_from_sin, fma, invsqrt, sin};

/// `AxisAngle4f` (`angle` is reduced to `[0, 2*PI)` like the JOML constructor does).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AxisAngle4f {
    pub angle: f32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl AxisAngle4f {
    /// `new AxisAngle4f(angle, x, y, z)`.
    pub fn new(angle: f32, x: f32, y: f32, z: f32) -> Self {
        const TAU: f32 = (std::f64::consts::PI * 2.0) as f32;
        let wrapped = if angle < 0.0 {
            TAU + angle % TAU
        } else {
            angle
        };
        Self {
            angle: wrapped % TAU,
            x,
            y,
            z,
        }
    }
}

/// `Quaternionf` (`x`, `y`, `z`, `w`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quaternionf {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Default for Quaternionf {
    fn default() -> Self {
        Self::identity()
    }
}

impl Quaternionf {
    pub const fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self { x, y, z, w }
    }

    /// `new Quaternionf()`.
    pub const fn identity() -> Self {
        Self::new(0.0, 0.0, 0.0, 1.0)
    }

    /// `new Quaternionf(AxisAngle4f)`.
    pub fn from_axis_angle(a: &AxisAngle4f) -> Self {
        let half = a.angle * 0.5;
        let s = sin(half);
        Self::new(a.x * s, a.y * s, a.z * s, cos_from_sin(s, half))
    }

    /// `normalize()`.
    pub fn normalize(&mut self) -> &mut Self {
        let inv_norm = invsqrt(fma(
            self.x,
            self.x,
            fma(self.y, self.y, fma(self.z, self.z, self.w * self.w)),
        ));
        self.x *= inv_norm;
        self.y *= inv_norm;
        self.z *= inv_norm;
        self.w *= inv_norm;
        self
    }

    /// `mul(Quaternionfc q)`: `this = this * q`.
    pub fn mul(&mut self, q: &Quaternionf) -> &mut Self {
        let (x, y, z, w) = (self.x, self.y, self.z, self.w);
        *self = Self::new(
            fma(w, q.x, fma(x, q.w, fma(y, q.z, -z * q.y))),
            fma(w, q.y, fma(-x, q.z, fma(y, q.w, z * q.x))),
            fma(w, q.z, fma(x, q.y, fma(-y, q.x, z * q.w))),
            fma(w, q.w, fma(-x, q.x, fma(-y, q.y, -z * q.z))),
        );
        self
    }

    /// `conjugate()`.
    pub fn conjugate(&mut self) -> &mut Self {
        self.x = -self.x;
        self.y = -self.y;
        self.z = -self.z;
        self
    }
}
