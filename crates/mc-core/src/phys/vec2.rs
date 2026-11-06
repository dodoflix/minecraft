//! `world.phys.Vec2` (float vector). Codec not ported.

use crate::math::mth;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const ZERO: Vec2 = Vec2::new(0.0, 0.0);
    pub const ONE: Vec2 = Vec2::new(1.0, 1.0);
    pub const UNIT_X: Vec2 = Vec2::new(1.0, 0.0);
    pub const NEG_UNIT_X: Vec2 = Vec2::new(-1.0, 0.0);
    pub const UNIT_Y: Vec2 = Vec2::new(0.0, 1.0);
    pub const NEG_UNIT_Y: Vec2 = Vec2::new(0.0, -1.0);
    pub const MAX: Vec2 = Vec2::new(f32::MAX, f32::MAX);
    /// `Float.MIN_VALUE` (smallest positive subnormal, not `f32::MIN`).
    pub const MIN: Vec2 = Vec2::new(f32::from_bits(1), f32::from_bits(1));

    pub const fn new(x: f32, y: f32) -> Vec2 {
        Vec2 { x, y }
    }

    pub fn scale(&self, s: f32) -> Vec2 {
        Vec2::new(self.x * s, self.y * s)
    }

    pub fn dot(&self, v: Vec2) -> f32 {
        self.x * v.x + self.y * v.y
    }

    #[allow(clippy::should_implement_trait)]
    pub fn add(&self, rhs: Vec2) -> Vec2 {
        Vec2::new(self.x + rhs.x, self.y + rhs.y)
    }

    pub fn add_scalar(&self, v: f32) -> Vec2 {
        Vec2::new(self.x + v, self.y + v)
    }

    pub fn normalized(&self) -> Vec2 {
        let dist = mth::sqrt(self.x * self.x + self.y * self.y);
        if dist < 1.0E-4 {
            Vec2::ZERO
        } else {
            Vec2::new(self.x / dist, self.y / dist)
        }
    }

    pub fn length(&self) -> f32 {
        mth::sqrt(self.x * self.x + self.y * self.y)
    }

    pub fn length_squared(&self) -> f32 {
        self.x * self.x + self.y * self.y
    }

    pub fn distance_to_sqr(&self, p: Vec2) -> f32 {
        let xd = p.x - self.x;
        let yd = p.y - self.y;
        xd * xd + yd * yd
    }

    pub fn negated(&self) -> Vec2 {
        Vec2::new(-self.x, -self.y)
    }

    pub fn rotate(&self, angle_radians: f64) -> Vec2 {
        let cosine = mth::cos(angle_radians);
        let sine = mth::sin(angle_radians);
        Vec2::new(
            self.x * cosine - self.y * sine,
            self.y * cosine + self.x * sine,
        )
    }
}
