//! `org.joml.Matrix4f`: the operations `com.mojang.math.Transformation` and the `Matrix4f` codec use, with JOML's float
//! arithmetic and its `properties` flags (every method picks its fast path from them exactly as JOML does, which is
//! observable in the low float bits and in `-0.0`/NaN results). Elements are stored in JOML's field order
//! `m00, m01, m02, m03, m10, ...` where `m<column><row>`, so index `column * 4 + row`.
//!
//! Not implemented: the remaining JOML `Matrix4f` API (projection, look-at, transforms of vectors, ...), which no server
//! code reads.

use super::joml::{abs_equals_one, fma, is_finite};
use super::quaternionf::Quaternionf;
use std::hash::{Hash, Hasher};

/// `Matrix4fc.PROPERTY_PERSPECTIVE`.
pub const PROPERTY_PERSPECTIVE: i32 = 1;
/// `Matrix4fc.PROPERTY_AFFINE`.
pub const PROPERTY_AFFINE: i32 = 2;
/// `Matrix4fc.PROPERTY_IDENTITY`.
pub const PROPERTY_IDENTITY: i32 = 4;
/// `Matrix4fc.PROPERTY_TRANSLATION`.
pub const PROPERTY_TRANSLATION: i32 = 8;
/// `Matrix4fc.PROPERTY_ORTHONORMAL`.
pub const PROPERTY_ORTHONORMAL: i32 = 16;

const IDENTITY: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

#[derive(Clone, Debug)]
pub struct Matrix4f {
    m: [f32; 16],
    properties: i32,
}

impl Default for Matrix4f {
    /// `new Matrix4f()`: the identity (properties: affine, identity, translation, orthonormal).
    fn default() -> Self {
        Self {
            m: IDENTITY,
            properties: PROPERTY_AFFINE
                | PROPERTY_IDENTITY
                | PROPERTY_TRANSLATION
                | PROPERTY_ORTHONORMAL,
        }
    }
}

/// `equals`: the `Float.floatToIntBits` of every element (all NaNs are equal), not the properties.
fn canonical_bits(f: f32) -> u32 {
    if f.is_nan() { 0x7FC0_0000 } else { f.to_bits() }
}

impl PartialEq for Matrix4f {
    fn eq(&self, other: &Self) -> bool {
        self.m
            .iter()
            .zip(&other.m)
            .all(|(a, b)| canonical_bits(*a) == canonical_bits(*b))
    }
}

impl Eq for Matrix4f {}

impl Hash for Matrix4f {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for f in self.m {
            canonical_bits(f).hash(state);
        }
    }
}

/// The nine entries of `Quaternionf` -> rotation matrix shared by every `rotate` variant (`rm<column><row>`).
fn rotation_entries(q: &Quaternionf) -> [f32; 9] {
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
    [
        w2 + x2 - z2 - y2,
        dxy + dzw,
        dxz - dyw,
        -dzw + dxy,
        y2 - z2 + w2 - x2,
        dyz + dxw,
        dyw + dxz,
        dyz - dxw,
        z2 - y2 - x2 + w2,
    ]
}

impl Matrix4f {
    /// The 16 elements in JOML field order (`index = column * 4 + row`).
    pub fn elements(&self) -> [f32; 16] {
        self.m
    }

    /// `properties()`.
    pub fn properties(&self) -> i32 {
        self.properties
    }

    /// `assume(properties)`.
    pub fn assume(&mut self, properties: i32) -> &mut Self {
        self.properties = properties;
        self
    }

    /// `determineProperties()`.
    pub fn determine_properties(&mut self) -> &mut Self {
        let [
            m00,
            m01,
            m02,
            m03,
            m10,
            m11,
            m12,
            m13,
            m20,
            m21,
            m22,
            m23,
            m30,
            m31,
            m32,
            m33,
        ] = self.m;
        let mut properties = 0;
        if m03 == 0.0 && m13 == 0.0 {
            if m23 == 0.0 && m33 == 1.0 {
                properties |= PROPERTY_AFFINE;
                if m00 == 1.0
                    && m01 == 0.0
                    && m02 == 0.0
                    && m10 == 0.0
                    && m11 == 1.0
                    && m12 == 0.0
                    && m20 == 0.0
                    && m21 == 0.0
                    && m22 == 1.0
                {
                    properties |= PROPERTY_TRANSLATION | PROPERTY_ORTHONORMAL;
                    if m30 == 0.0 && m31 == 0.0 && m32 == 0.0 {
                        properties |= PROPERTY_IDENTITY;
                    }
                }
            } else if m01 == 0.0
                && m02 == 0.0
                && m10 == 0.0
                && m12 == 0.0
                && m20 == 0.0
                && m21 == 0.0
                && m30 == 0.0
                && m31 == 0.0
                && m33 == 0.0
            {
                properties |= PROPERTY_PERSPECTIVE;
            }
        }
        self.properties = properties;
        self
    }

    /// `getRowColumn(row, column)`.
    pub fn get_row_column(&self, row: usize, column: usize) -> f32 {
        self.m[column * 4 + row]
    }

    /// `setRowColumn(row, column, value)`: clears the properties.
    pub fn set_row_column(&mut self, row: usize, column: usize, value: f32) -> &mut Self {
        self.m[column * 4 + row] = value;
        self.properties = 0;
        self
    }

    /// `set(Matrix4fc)`: elements and properties (`new Matrix4f(m)` is `default().set(m)`).
    pub fn set(&mut self, other: &Matrix4f) -> &mut Self {
        self.m = other.m;
        self.properties = other.properties;
        self
    }

    /// `getTranslation(dest)`.
    pub fn get_translation(&self) -> [f32; 3] {
        [self.m[12], self.m[13], self.m[14]]
    }

    /// `isFinite()`.
    pub fn is_finite(&self) -> bool {
        self.m.iter().all(|f| is_finite(*f))
    }

    /// `MemUtil.identity(this)` (elements only).
    fn reset_elements(&mut self) {
        self.m = IDENTITY;
    }

    /// `translation(x, y, z)`.
    pub fn translation(&mut self, x: f32, y: f32, z: f32) -> &mut Self {
        if self.properties & PROPERTY_IDENTITY == 0 {
            self.reset_elements();
        }
        self.m[12] = x;
        self.m[13] = y;
        self.m[14] = z;
        self.properties = PROPERTY_AFFINE | PROPERTY_TRANSLATION | PROPERTY_ORTHONORMAL;
        self
    }

    /// `scaling(x, y, z)`.
    pub fn scaling(&mut self, x: f32, y: f32, z: f32) -> &mut Self {
        if self.properties & PROPERTY_IDENTITY == 0 {
            self.reset_elements();
        }
        let one = abs_equals_one(x) && abs_equals_one(y) && abs_equals_one(z);
        self.m[0] = x;
        self.m[5] = y;
        self.m[10] = z;
        self.properties = PROPERTY_AFFINE | if one { PROPERTY_ORTHONORMAL } else { 0 };
        self
    }

    /// `scale(x, y, z)`: `this = this * scaling(x, y, z)`.
    pub fn scale(&mut self, x: f32, y: f32, z: f32) -> &mut Self {
        if self.properties & PROPERTY_IDENTITY != 0 {
            return self.scaling(x, y, z);
        }
        let one = abs_equals_one(x) && abs_equals_one(y) && abs_equals_one(z);
        for (column, factor) in [x, y, z].into_iter().enumerate() {
            for row in 0..4 {
                self.m[column * 4 + row] *= factor;
            }
        }
        let cleared = 13 | if one { 0 } else { PROPERTY_ORTHONORMAL };
        self.properties &= !cleared;
        self
    }

    /// `rotation(Quaternionfc)`.
    pub fn rotation(&mut self, q: &Quaternionf) -> &mut Self {
        let rm = rotation_entries(q);
        if self.properties & PROPERTY_IDENTITY == 0 {
            self.reset_elements();
        }
        for column in 0..3 {
            for row in 0..3 {
                self.m[column * 4 + row] = rm[column * 3 + row];
            }
        }
        self.properties = PROPERTY_AFFINE | PROPERTY_ORTHONORMAL;
        self
    }

    /// `rotate(Quaternionfc)`: `this = this * rotation(q)`.
    pub fn rotate(&mut self, q: &Quaternionf) -> &mut Self {
        if self.properties & PROPERTY_IDENTITY != 0 {
            return self.rotation(q);
        }
        let rm = rotation_entries(q);
        let a = self.m;
        let affine = self.properties & PROPERTY_AFFINE != 0;
        let translation = self.properties & PROPERTY_TRANSLATION != 0;
        // columns 0..=2 of the product; the translation column keeps its elements
        for column in 0..3 {
            let (r0, r1, r2) = (rm[column * 3], rm[column * 3 + 1], rm[column * 3 + 2]);
            for row in 0..4 {
                let value = if translation {
                    if row == 3 { 0.0 } else { rm[column * 3 + row] }
                } else if affine {
                    if row == 3 {
                        0.0
                    } else {
                        a[row] * r0 + a[4 + row] * r1 + a[8 + row] * r2
                    }
                } else {
                    fma(a[row], r0, fma(a[4 + row], r1, a[8 + row] * r2))
                };
                self.m[column * 4 + row] = value;
            }
        }
        self.properties &= -14;
        self
    }

    /// `mul(Matrix4fc right)`: `this = this * right`, choosing the multiplication by the operands' properties.
    pub fn mul(&mut self, right: &Matrix4f) -> &mut Self {
        let (lp, rp) = (self.properties, right.properties);
        if lp & PROPERTY_IDENTITY != 0 {
            return self.set(right);
        }
        if rp & PROPERTY_IDENTITY != 0 {
            return self;
        }
        if lp & PROPERTY_TRANSLATION != 0 && rp & PROPERTY_AFFINE != 0 {
            self.mul_translation_affine(right)
        } else if lp & PROPERTY_AFFINE != 0 && rp & PROPERTY_AFFINE != 0 {
            self.mul_affine(right)
        } else if lp & PROPERTY_PERSPECTIVE != 0 && rp & PROPERTY_AFFINE != 0 {
            self.mul_perspective_affine(right)
        } else if rp & PROPERTY_AFFINE != 0 {
            self.mul_affine_r(right)
        } else {
            self.mul0(right)
        }
    }

    /// `mul0`: the general product.
    fn mul0(&mut self, right: &Matrix4f) -> &mut Self {
        let (a, b) = (self.m, right.m);
        for column in 0..4 {
            for row in 0..4 {
                self.m[column * 4 + row] = fma(
                    a[row],
                    b[column * 4],
                    fma(
                        a[4 + row],
                        b[column * 4 + 1],
                        fma(
                            a[8 + row],
                            b[column * 4 + 2],
                            a[12 + row] * b[column * 4 + 3],
                        ),
                    ),
                );
            }
        }
        self.properties = 0;
        self
    }

    /// `mulAffineR`: `right` is affine.
    fn mul_affine_r(&mut self, right: &Matrix4f) -> &mut Self {
        let (a, b) = (self.m, right.m);
        for column in 0..4 {
            for row in 0..4 {
                let (b0, b1, b2) = (b[column * 4], b[column * 4 + 1], b[column * 4 + 2]);
                self.m[column * 4 + row] = if column < 3 {
                    fma(a[row], b0, fma(a[4 + row], b1, a[8 + row] * b2))
                } else {
                    fma(
                        a[row],
                        b0,
                        fma(a[4 + row], b1, fma(a[8 + row], b2, a[12 + row])),
                    )
                };
            }
        }
        self.properties &= -30;
        self
    }

    /// `mulAffine`: both operands are affine.
    fn mul_affine(&mut self, right: &Matrix4f) -> &mut Self {
        let (a, b) = (self.m, right.m);
        for column in 0..4 {
            for row in 0..3 {
                let (b0, b1, b2) = (b[column * 4], b[column * 4 + 1], b[column * 4 + 2]);
                self.m[column * 4 + row] = if column < 3 {
                    fma(a[row], b0, fma(a[4 + row], b1, a[8 + row] * b2))
                } else {
                    fma(
                        a[row],
                        b0,
                        fma(a[4 + row], b1, fma(a[8 + row], b2, a[12 + row])),
                    )
                };
            }
        }
        self.properties =
            PROPERTY_AFFINE | (self.properties & right.properties & PROPERTY_ORTHONORMAL);
        self
    }

    /// `mulTranslationAffine`: `this` is a pure translation, `right` affine.
    fn mul_translation_affine(&mut self, right: &Matrix4f) -> &mut Self {
        let (a, b) = (self.m, right.m);
        for column in 0..4 {
            for row in 0..3 {
                self.m[column * 4 + row] = if column < 3 {
                    b[column * 4 + row]
                } else {
                    b[12 + row] + a[12 + row]
                };
            }
        }
        self.properties = PROPERTY_AFFINE | (right.properties & PROPERTY_ORTHONORMAL);
        self
    }

    /// `mulPerspectiveAffine`: `this` is a perspective projection, `right` affine.
    fn mul_perspective_affine(&mut self, right: &Matrix4f) -> &mut Self {
        let (a, b) = (self.m, right.m);
        let (m00, m11, m22, m23, m32) = (a[0], a[5], a[10], a[11], a[14]);
        for column in 0..4 {
            let (b0, b1, b2) = (b[column * 4], b[column * 4 + 1], b[column * 4 + 2]);
            self.m[column * 4] = m00 * b0;
            self.m[column * 4 + 1] = m11 * b1;
            self.m[column * 4 + 2] = if column == 3 {
                m22 * b2 + m32
            } else {
                m22 * b2
            };
            self.m[column * 4 + 3] = m23 * b2;
        }
        self.properties = 0;
        self
    }

    /// `invertAffine()`.
    pub fn invert_affine(&mut self) -> &mut Self {
        let [
            m00,
            m01,
            m02,
            _,
            m10,
            m11,
            m12,
            _,
            m20,
            m21,
            m22,
            _,
            m30,
            m31,
            m32,
            _,
        ] = self.m;
        let m11m00 = m00 * m11;
        let m10m01 = m01 * m10;
        let m10m02 = m02 * m10;
        let m12m00 = m00 * m12;
        let m12m01 = m01 * m12;
        let m11m02 = m02 * m11;
        let det = (m11m00 - m10m01) * m22 + (m10m02 - m12m00) * m21 + (m12m01 - m11m02) * m20;
        let s = 1.0 / det;
        let m10m22 = m10 * m22;
        let m10m21 = m10 * m21;
        let m11m22 = m11 * m22;
        let m11m20 = m11 * m20;
        let m12m21 = m12 * m21;
        let m12m20 = m12 * m20;
        let m20m02 = m20 * m02;
        let m20m01 = m20 * m01;
        let m21m02 = m21 * m02;
        let m21m00 = m21 * m00;
        let m22m01 = m22 * m01;
        let m22m00 = m22 * m00;
        let nm31 = (m20m02 * m31 - m20m01 * m32 + m21m00 * m32 - m21m02 * m30 + m22m01 * m30
            - m22m00 * m31)
            * s;
        let nm32 = (m11m02 * m30 - m12m01 * m30 + m12m00 * m31 - m10m02 * m31 + m10m01 * m32
            - m11m00 * m32)
            * s;
        self.m = [
            (m11m22 - m12m21) * s,
            (m21m02 - m22m01) * s,
            (m12m01 - m11m02) * s,
            0.0,
            (m12m20 - m10m22) * s,
            (m22m00 - m20m02) * s,
            (m10m02 - m12m00) * s,
            0.0,
            (m10m21 - m11m20) * s,
            (m20m01 - m21m00) * s,
            (m11m00 - m10m01) * s,
            0.0,
            (m10m22 * m31 - m10m21 * m32 + m11m20 * m32 - m11m22 * m30 + m12m21 * m30
                - m12m20 * m31)
                * s,
            nm31,
            nm32,
            1.0,
        ];
        self.properties = PROPERTY_AFFINE;
        self
    }
}
