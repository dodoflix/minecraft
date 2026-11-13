//! `org.joml.Matrix3f`: the operations `MatrixUtil.svdDecompose` uses. Fields are named like JOML's `m<column><row>`.

use super::joml::fma;
use super::matrix4f::Matrix4f;
use super::quaternionf::Quaternionf;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Matrix3f {
    pub m00: f32,
    pub m01: f32,
    pub m02: f32,
    pub m10: f32,
    pub m11: f32,
    pub m12: f32,
    pub m20: f32,
    pub m21: f32,
    pub m22: f32,
}

impl Default for Matrix3f {
    /// `new Matrix3f()`: the identity.
    fn default() -> Self {
        Self {
            m00: 1.0,
            m01: 0.0,
            m02: 0.0,
            m10: 0.0,
            m11: 1.0,
            m12: 0.0,
            m20: 0.0,
            m21: 0.0,
            m22: 1.0,
        }
    }
}

impl Matrix3f {
    /// `new Matrix3f(Matrix4fc)`: the upper left 3x3 block.
    pub fn from_matrix4(m: &Matrix4f) -> Self {
        let [m00, m01, m02, _, m10, m11, m12, _, m20, m21, m22, ..] = m.elements();
        Self {
            m00,
            m01,
            m02,
            m10,
            m11,
            m12,
            m20,
            m21,
            m22,
        }
    }

    /// `scale(float)`.
    pub fn scale(&mut self, f: f32) -> &mut Self {
        self.m00 *= f;
        self.m01 *= f;
        self.m02 *= f;
        self.m10 *= f;
        self.m11 *= f;
        self.m12 *= f;
        self.m20 *= f;
        self.m21 *= f;
        self.m22 *= f;
        self
    }

    /// `transpose()`.
    pub fn transpose(&mut self) -> &mut Self {
        *self = Self {
            m00: self.m00,
            m01: self.m10,
            m02: self.m20,
            m10: self.m01,
            m11: self.m11,
            m12: self.m21,
            m20: self.m02,
            m21: self.m12,
            m22: self.m22,
        };
        self
    }

    /// `mul(Matrix3fc right)`: `this = this * right`.
    pub fn mul(&mut self, r: &Matrix3f) -> &mut Self {
        let a = *self;
        *self = Self {
            m00: fma(a.m00, r.m00, fma(a.m10, r.m01, a.m20 * r.m02)),
            m01: fma(a.m01, r.m00, fma(a.m11, r.m01, a.m21 * r.m02)),
            m02: fma(a.m02, r.m00, fma(a.m12, r.m01, a.m22 * r.m02)),
            m10: fma(a.m00, r.m10, fma(a.m10, r.m11, a.m20 * r.m12)),
            m11: fma(a.m01, r.m10, fma(a.m11, r.m11, a.m21 * r.m12)),
            m12: fma(a.m02, r.m10, fma(a.m12, r.m11, a.m22 * r.m12)),
            m20: fma(a.m00, r.m20, fma(a.m10, r.m21, a.m20 * r.m22)),
            m21: fma(a.m01, r.m20, fma(a.m11, r.m21, a.m21 * r.m22)),
            m22: fma(a.m02, r.m20, fma(a.m12, r.m21, a.m22 * r.m22)),
        };
        self
    }

    /// `rotate(Quaternionfc)`: `this = this * rotation(q)`.
    pub fn rotate(&mut self, q: &Quaternionf) -> &mut Self {
        let w2 = q.w * q.w;
        let x2 = q.x * q.x;
        let y2 = q.y * q.y;
        let z2 = q.z * q.z;
        let zw = q.z * q.w;
        let dzw = zw + zw;
        let xy = q.x * q.y;
        let dxy = xy + xy;
        let xz = q.x * q.z;
        let dxz = xz + xz;
        let yw = q.y * q.w;
        let dyw = yw + yw;
        let yz = q.y * q.z;
        let dyz = yz + yz;
        let xw = q.x * q.w;
        let dxw = xw + xw;
        let rm00 = w2 + x2 - z2 - y2;
        let rm01 = dxy + dzw;
        let rm02 = dxz - dyw;
        let rm10 = dxy - dzw;
        let rm11 = y2 - z2 + w2 - x2;
        let rm12 = dyz + dxw;
        let rm20 = dyw + dxz;
        let rm21 = dyz - dxw;
        let rm22 = z2 - y2 - x2 + w2;
        let a = *self;
        *self = Self {
            m00: a.m00 * rm00 + a.m10 * rm01 + a.m20 * rm02,
            m01: a.m01 * rm00 + a.m11 * rm01 + a.m21 * rm02,
            m02: a.m02 * rm00 + a.m12 * rm01 + a.m22 * rm02,
            m10: a.m00 * rm10 + a.m10 * rm11 + a.m20 * rm12,
            m11: a.m01 * rm10 + a.m11 * rm11 + a.m21 * rm12,
            m12: a.m02 * rm10 + a.m12 * rm11 + a.m22 * rm12,
            m20: a.m00 * rm20 + a.m10 * rm21 + a.m20 * rm22,
            m21: a.m01 * rm20 + a.m11 * rm21 + a.m21 * rm22,
            m22: a.m02 * rm20 + a.m12 * rm21 + a.m22 * rm22,
        };
        self
    }
}
