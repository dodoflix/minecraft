//! `net.minecraft.network.protocol.game.VecDeltaCodec` (+ `VecDelta`/`PositionPath` linear part).
use mc_core::phys::Vec3;

pub fn encode(input: f64) -> i64 {
    mc_core::math::java::round_f64(input * 4096.0)
}

pub fn decode(v: i64) -> f64 {
    v as f64 / 4096.0
}

pub fn encoding_precision_loss(d: f64) -> f64 {
    decode(encode(d)) - d
}

pub fn is_delta_too_big(xa: i64, ya: i64, za: i64) -> bool {
    !(-32768..=32767).contains(&xa)
        || !(-32768..=32767).contains(&ya)
        || !(-32768..=32767).contains(&za)
}

/// `VecDelta.Linear`: a short per axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VecDeltaLinear {
    pub xa: i16,
    pub ya: i16,
    pub za: i16,
}

#[derive(Clone, Copy, Debug)]
pub struct VecDeltaCodec {
    base: Vec3,
}

impl Default for VecDeltaCodec {
    fn default() -> Self {
        Self { base: Vec3::ZERO }
    }
}

impl VecDeltaCodec {
    pub fn decode(&self, xa: i64, ya: i64, za: i64) -> Vec3 {
        if xa == 0 && ya == 0 && za == 0 {
            self.base
        } else {
            let x = if xa == 0 {
                self.base.x
            } else {
                decode(encode(self.base.x) + xa)
            };
            let y = if ya == 0 {
                self.base.y
            } else {
                decode(encode(self.base.y) + ya)
            };
            let z = if za == 0 {
                self.base.z
            } else {
                decode(encode(self.base.z) + za)
            };
            Vec3::new(x, y, z)
        }
    }

    /// `tryEncode(Vec3)`: `None` when a delta doesn't fit a short.
    pub fn try_encode(&self, pos: Vec3) -> Option<VecDeltaLinear> {
        let (xa, ya, za) = (self.encode_x(pos), self.encode_y(pos), self.encode_z(pos));
        (!is_delta_too_big(xa, ya, za)).then_some(VecDeltaLinear {
            xa: xa as i16,
            ya: ya as i16,
            za: za as i16,
        })
    }

    pub fn encode_x(&self, pos: Vec3) -> i64 {
        encode(pos.x) - encode(self.base.x)
    }

    pub fn encode_y(&self, pos: Vec3) -> i64 {
        encode(pos.y) - encode(self.base.y)
    }

    pub fn encode_z(&self, pos: Vec3) -> i64 {
        encode(pos.z) - encode(self.base.z)
    }

    pub fn delta(&self, pos: Vec3) -> Vec3 {
        pos.subtract(self.base)
    }

    pub fn set_base(&mut self, base: Vec3) {
        self.base = base;
    }

    pub fn get_base(&self) -> Vec3 {
        self.base
    }
}
