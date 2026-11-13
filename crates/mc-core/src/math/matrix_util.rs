//! `com.mojang.math.MatrixUtil` (the SVD of a 4x4 matrix through a Jacobi eigen decomposition) and
//! `com.mojang.math.GivensParameters`.
//!
//! Not implemented: `MatrixUtil.mulComponentWise` and the `Matrix4f` property helpers, which only client rendering calls.

use super::joml::{cos_from_sin, hypot, invsqrt, max, sin};
use super::matrix3f::Matrix3f;
use super::matrix4f::Matrix4f;
use super::quaternionf::Quaternionf;

/// `GivensParameters(sinHalf, cosHalf)`.
#[derive(Clone, Copy, Debug)]
struct GivensParameters {
    sin_half: f32,
    cos_half: f32,
}

impl GivensParameters {
    fn from_unnormalized(sin_half: f32, cos_half: f32) -> Self {
        let w = invsqrt(sin_half * sin_half + cos_half * cos_half);
        Self {
            sin_half: w * sin_half,
            cos_half: w * cos_half,
        }
    }

    fn from_positive_angle(angle: f32) -> Self {
        let sin = sin(angle / 2.0);
        let cos = cos_from_sin(sin, angle / 2.0);
        Self {
            sin_half: sin,
            cos_half: cos,
        }
    }

    fn inverse(self) -> Self {
        Self {
            sin_half: -self.sin_half,
            cos_half: self.cos_half,
        }
    }

    fn around_x(self) -> Quaternionf {
        Quaternionf::new(self.sin_half, 0.0, 0.0, self.cos_half)
    }

    fn around_y(self) -> Quaternionf {
        Quaternionf::new(0.0, self.sin_half, 0.0, self.cos_half)
    }

    fn around_z(self) -> Quaternionf {
        Quaternionf::new(0.0, 0.0, self.sin_half, self.cos_half)
    }

    fn cos(self) -> f32 {
        self.cos_half * self.cos_half - self.sin_half * self.sin_half
    }

    fn sin(self) -> f32 {
        2.0 * self.sin_half * self.cos_half
    }

    fn matrix_around_x(self) -> Matrix3f {
        let (c, s) = (self.cos(), self.sin());
        Matrix3f {
            m00: 1.0,
            m01: 0.0,
            m02: 0.0,
            m10: 0.0,
            m11: c,
            m12: s,
            m20: 0.0,
            m21: -s,
            m22: c,
        }
    }

    fn matrix_around_y(self) -> Matrix3f {
        let (c, s) = (self.cos(), self.sin());
        Matrix3f {
            m00: c,
            m01: 0.0,
            m02: -s,
            m10: 0.0,
            m11: 1.0,
            m12: 0.0,
            m20: s,
            m21: 0.0,
            m22: c,
        }
    }

    fn matrix_around_z(self) -> Matrix3f {
        let (c, s) = (self.cos(), self.sin());
        Matrix3f {
            m00: c,
            m01: s,
            m02: 0.0,
            m10: -s,
            m11: c,
            m12: 0.0,
            m20: 0.0,
            m21: 0.0,
            m22: 1.0,
        }
    }
}

/// `MatrixUtil.G`: `3.0F + 2.0F * Math.sqrt(2.0F)`.
fn g() -> f32 {
    3.0 + 2.0 * (2.0f64.sqrt() as f32)
}

/// `MatrixUtil.PI_4`.
fn pi_4() -> GivensParameters {
    GivensParameters::from_positive_angle((std::f64::consts::PI / 4.0) as f32)
}

fn approx_givens_quat(a11: f32, a12: f32, a22: f32) -> GivensParameters {
    let ch = 2.0 * (a11 - a22);
    if g() * a12 * a12 < ch * ch {
        GivensParameters::from_unnormalized(a12, ch)
    } else {
        pi_4()
    }
}

fn qr_givens_quat(a1: f32, a2: f32) -> GivensParameters {
    let p = hypot(a1, a2);
    let mut sh = if p > 1.0e-6 { a2 } else { 0.0 };
    let mut ch = a1.abs() + max(p, 1.0e-6);
    if a1 < 0.0 {
        std::mem::swap(&mut sh, &mut ch);
    }
    GivensParameters::from_unnormalized(sh, ch)
}

/// `similarityTransform(a, q)`: `a = q^T * (a * q)`.
fn similarity_transform(a: &mut Matrix3f, q: &mut Matrix3f) {
    a.mul(q);
    q.transpose();
    q.mul(a);
    *a = *q;
}

fn step_jacobi(m: &mut Matrix3f, output: &mut Quaternionf) {
    if m.m01 * m.m01 + m.m10 * m.m10 > 1.0e-6 {
        let p = approx_givens_quat(m.m00, 0.5 * (m.m01 + m.m10), m.m11);
        output.mul(&p.around_z());
        similarity_transform(m, &mut p.matrix_around_z());
    }
    if m.m02 * m.m02 + m.m20 * m.m20 > 1.0e-6 {
        let p = approx_givens_quat(m.m00, 0.5 * (m.m02 + m.m20), m.m22).inverse();
        output.mul(&p.around_y());
        similarity_transform(m, &mut p.matrix_around_y());
    }
    if m.m12 * m.m12 + m.m21 * m.m21 > 1.0e-6 {
        let p = approx_givens_quat(m.m11, 0.5 * (m.m12 + m.m21), m.m22);
        output.mul(&p.around_x());
        similarity_transform(m, &mut p.matrix_around_x());
    }
}

/// `MatrixUtil.eigenvalueJacobi(inOut, steps, result)`.
fn eigenvalue_jacobi(in_out: &mut Matrix3f, steps: usize) -> Quaternionf {
    let mut result = Quaternionf::identity();
    for _ in 0..steps {
        step_jacobi(in_out, &mut result);
    }
    result.normalize();
    result
}

/// `MatrixUtil.svdDecompose(input, t, u, s, v)` (`input = T * u * diag(s) * v`): returns `(t, u, s, v)`.
pub fn svd_decompose(input: &Matrix4f) -> ([f32; 3], Quaternionf, [f32; 3], Quaternionf) {
    let scale_factor = 1.0 / input.elements()[15];
    let mut m3 = Matrix3f::from_matrix4(input);
    m3.scale(scale_factor);
    let (u, s, v) = svd_decompose3(m3);
    let [tx, ty, tz] = input.get_translation();
    (
        [tx * scale_factor, ty * scale_factor, tz * scale_factor],
        u,
        s,
        v,
    )
}

fn svd_decompose3(mut input: Matrix3f) -> (Quaternionf, [f32; 3], Quaternionf) {
    let mut scratch = input;
    scratch.transpose();
    scratch.mul(&input);
    let mut v = eigenvalue_jacobi(&mut scratch, 5);
    let column_scale_square0 = scratch.m00;
    let column_scale_square1 = scratch.m11;
    let zero_column0 = f64::from(column_scale_square0) < 1.0e-6;
    let zero_column1 = f64::from(column_scale_square1) < 1.0e-6;
    // `u012s` is `input` itself (`input.rotate(v)` returns `this`)
    input.rotate(&v);
    let mut u = Quaternionf::identity();
    let mut p = if zero_column0 {
        qr_givens_quat(input.m11, -input.m10)
    } else {
        qr_givens_quat(input.m00, input.m01)
    };
    let qt0 = p.around_z();
    // `u12s` is `scratch`
    scratch = p.matrix_around_z();
    u.mul(&qt0);
    scratch.transpose().mul(&input);
    p = if zero_column0 {
        qr_givens_quat(scratch.m22, -scratch.m20)
    } else {
        qr_givens_quat(scratch.m00, scratch.m02)
    };
    p = p.inverse();
    let qt1 = p.around_y();
    // `u2s` is `input`
    input = p.matrix_around_y();
    u.mul(&qt1);
    input.transpose().mul(&scratch);
    p = if zero_column1 {
        qr_givens_quat(input.m22, -input.m21)
    } else {
        qr_givens_quat(input.m11, input.m12)
    };
    let qt2 = p.around_x();
    // `sm` is `scratch`
    scratch = p.matrix_around_x();
    u.mul(&qt2);
    scratch.transpose().mul(&input);
    v.conjugate();
    (u, [scratch.m00, scratch.m11, scratch.m22], v)
}
