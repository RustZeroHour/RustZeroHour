// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Ported from Core/Libraries/Source/WWVegas/WWMath/matrix4.h and
// matrix4.cpp @ 571a7c5, Zero Hour configuration. The original carries the
// notices: (c) 2001-2003 Electronic Arts Inc.; Greg Hjelstrom, Neall
// Verheyde.
//
//! The full 4x4 matrix (`Matrix4x4`).
//!
//! Four [`Vector4`] rows. Like [`crate::matrix3d::Matrix3D`] this is a
//! column-vector convention; the fourth row makes it the type projections
//! and full homogeneous transforms live in. The simulation itself uses
//! [`Matrix3D`]; this type serves the frustum/projection math and the
//! render-side pipelines.
//!
//! # Quirks preserved (they are behaviour)
//!
//! - [`std::ops::DivAssign`] **multiplies** by the operand: the original's
//!   `operator/=` reads `float ood = d;` and scales the rows by it — the
//!   reciprocal is never taken. The free division
//!   ([`std::ops::Div`]) computes `1/d` correctly.
//! - [`std::ops::SubAssign`] subtracts through `Vector4::operator-=`,
//!   whose **W column is added, not subtracted** (the preserved original
//!   bug also visible in [`crate::matrix3d`]'s `solve_linear_system`).
//!   The free subtraction subtracts every component correctly.
//! - [`Matrix4::inverse_with_determinant`] is TheSuperHackers' 2026
//!   cofactor/adjugate implementation; singular matrices (`|det| < 1e-8`)
//!   return `None` (the C++ returns null and leaves the output
//!   unwritten).
//! - The mixed products with [`Matrix3D`] assume the implicit
//!   `0 0 0 1` row/column, hand-coded like the originals.
//!
//! # Not ported (documented, with their consumers)
//!
//! - `Init(const Matrix3x3&)` / `Matrix4x4(const Matrix3x3&)` — declared
//!   in the original header but **never defined** (dead declarations;
//!   calling them would be a link error).
//! - `To_D3DMATRIX` / `To_Matrix4x4` (DirectX 8 interop) — re-implemented
//!   on the render side per plan §1.
//!
//! # Parity level
//!
//! **P0 exact** for everything except the fov-based
//! [`Matrix4::init_perspective_fov`]: its `1/tan(fov/2)` term uses the
//! C library's *double* `tan` in the original — platform libm, not the
//! GameMath pin — so its bits are not stable across platforms and it is
//! covered by structural unit tests instead of golden vectors. Everything
//! else is pinned by `tests/golden/matrices.txt` (records `m4 …`).

use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};

use crate::Matrix3D;
use crate::Vector3;
use crate::Vector4;
use crate::scalar;

/// The 4x4 float matrix, four [`Vector4`] rows (`Matrix4x4`). Index as
/// `m.rows[i]` for row *i*.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Matrix4 {
    /// The four rows (`Row[0..4]`).
    pub rows: [Vector4; 4],
}

impl Matrix4 {
    /// `Matrix4x4(true)`: the identity matrix.
    #[must_use]
    pub const fn identity() -> Self {
        Self {
            rows: [
                Vector4::new(1.0, 0.0, 0.0, 0.0),
                Vector4::new(0.0, 1.0, 0.0, 0.0),
                Vector4::new(0.0, 0.0, 1.0, 0.0),
                Vector4::new(0.0, 0.0, 0.0, 1.0),
            ],
        }
    }

    /// Constructs from sixteen components, row-major
    /// (`Matrix4x4(m11..m44)`).
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn from_components(
        m11: f32,
        m12: f32,
        m13: f32,
        m14: f32,
        m21: f32,
        m22: f32,
        m23: f32,
        m24: f32,
        m31: f32,
        m32: f32,
        m33: f32,
        m34: f32,
        m41: f32,
        m42: f32,
        m43: f32,
        m44: f32,
    ) -> Self {
        Self {
            rows: [
                Vector4::new(m11, m12, m13, m14),
                Vector4::new(m21, m22, m23, m24),
                Vector4::new(m31, m32, m33, m34),
                Vector4::new(m41, m42, m43, m44),
            ],
        }
    }

    /// Constructs from four rows (`Matrix4x4(v0, v1, v2, v3)`).
    #[must_use]
    pub const fn from_rows(r0: Vector4, r1: Vector4, r2: Vector4, r3: Vector4) -> Self {
        Self {
            rows: [r0, r1, r2, r3],
        }
    }

    /// `Matrix4x4(const Matrix3D&)`: the 4x3 transform with the implicit
    /// `0 0 0 1` last row made explicit (`Init(m)`).
    #[must_use]
    pub const fn from_matrix3d(m: Matrix3D) -> Self {
        Self {
            rows: [
                m.rows[0],
                m.rows[1],
                m.rows[2],
                Vector4::new(0.0, 0.0, 0.0, 1.0),
            ],
        }
    }

    /// `Make_Identity`.
    pub fn make_identity(&mut self) {
        *self = Self::identity();
    }

    /// `Init(m)` for a [`Matrix3D`].
    pub fn set_matrix3d(&mut self, m: Matrix3D) {
        *self = Self::from_matrix3d(m);
    }

    /// `Init_Ortho(left, right, bottom, top, znear, zfar)`: the
    /// orthographic projection (OpenGL/Surrender convention; points in
    /// the volume map to -1..=1, znear to z = -1, zfar to z = +1).
    ///
    /// `znear`/`zfar` are positive distances even though the camera
    /// looks down -z.
    pub fn init_ortho(
        &mut self,
        left: f32,
        right: f32,
        bottom: f32,
        top: f32,
        znear: f32,
        zfar: f32,
    ) {
        self.make_identity();
        self.rows[0].x = 2.0 / (right - left);
        self.rows[0].w = -(right + left) / (right - left);
        self.rows[1].y = 2.0 / (top - bottom);
        self.rows[1].w = -(top + bottom) / (top - bottom);
        self.rows[2].z = -2.0 / (zfar - znear);
        self.rows[2].w = -(zfar + znear) / (zfar - znear);
    }

    /// `Init_Perspective(hfov, vfov, znear, zfar)`: the perspective
    /// projection from horizontal/vertical fields of view (radians).
    ///
    /// **Not golden-pinned**: the original's `1.0 / tan(fov*0.5)` runs
    /// through the C library's *double* `tan` — platform libm, unlike
    /// every other transcendental in this crate (ADR-001) — so its bits
    /// are not stable across platforms. Structural behaviour is unit
    /// tested instead.
    pub fn init_perspective_fov(&mut self, hfov: f32, vfov: f32, znear: f32, zfar: f32) {
        self.make_identity();
        self.rows[0].x = (1.0 / f64::tan(f64::from(hfov) * 0.5)) as f32;
        self.rows[1].y = (1.0 / f64::tan(f64::from(vfov) * 0.5)) as f32;
        self.rows[2].z = -(zfar + znear) / (zfar - znear);
        self.rows[2].w =
            (-(2.0 * f64::from(zfar) * f64::from(znear)) / f64::from(zfar - znear)) as f32;
        self.rows[3].z = -1.0;
        self.rows[3].w = 0.0;
    }

    /// `Init_Perspective(left, right, bottom, top, znear, zfar)`: the
    /// perspective projection from a frustum (the near-plane rectangle).
    /// Pure double-literal arithmetic — golden-pinned.
    pub fn init_perspective_frustum(
        &mut self,
        left: f32,
        right: f32,
        bottom: f32,
        top: f32,
        znear: f32,
        zfar: f32,
    ) {
        self.make_identity();
        self.rows[0].x = (2.0 * f64::from(znear) / f64::from(right - left)) as f32;
        self.rows[0].z = (right + left) / (right - left);
        self.rows[1].y = (2.0 * f64::from(znear) / f64::from(top - bottom)) as f32;
        self.rows[1].z = (top + bottom) / (top - bottom);
        self.rows[2].z = -(zfar + znear) / (zfar - znear);
        self.rows[2].w =
            (-(2.0 * f64::from(zfar) * f64::from(znear)) / f64::from(zfar - znear)) as f32;
        self.rows[3].z = -1.0;
        self.rows[3].w = 0.0;
    }

    /// `Transpose()`.
    #[must_use]
    pub fn transpose(&self) -> Self {
        Self::from_rows(
            Vector4::new(
                self.rows[0].x,
                self.rows[1].x,
                self.rows[2].x,
                self.rows[3].x,
            ),
            Vector4::new(
                self.rows[0].y,
                self.rows[1].y,
                self.rows[2].y,
                self.rows[3].y,
            ),
            Vector4::new(
                self.rows[0].z,
                self.rows[1].z,
                self.rows[2].z,
                self.rows[3].z,
            ),
            Vector4::new(
                self.rows[0].w,
                self.rows[1].w,
                self.rows[2].w,
                self.rows[3].w,
            ),
        )
    }

    /// `Inverse(out, detOut, m)` (static): the general inverse via
    /// cofactors/adjugate plus the determinant. `None` when
    /// `|det| < 1e-8` (the C++ returns null and leaves the output
    /// unwritten; the determinant is still reported).
    #[must_use]
    pub fn inverse_with_determinant(m: Matrix4) -> (f32, Option<Matrix4>) {
        // the C's transposed element naming is kept to stay comparable
        let m00 = m.rows[0].x;
        let m01 = m.rows[1].x;
        let m02 = m.rows[2].x;
        let m03 = m.rows[3].x;
        let m10 = m.rows[0].y;
        let m11 = m.rows[1].y;
        let m12 = m.rows[2].y;
        let m13 = m.rows[3].y;
        let m20 = m.rows[0].z;
        let m21 = m.rows[1].z;
        let m22 = m.rows[2].z;
        let m23 = m.rows[3].z;
        let m30 = m.rows[0].w;
        let m31 = m.rows[1].w;
        let m32 = m.rows[2].w;
        let m33 = m.rows[3].w;

        // 2x2 determinants (minors) for the cofactors
        let s0 = m00 * m11 - m10 * m01;
        let s1 = m00 * m12 - m10 * m02;
        let s2 = m00 * m13 - m10 * m03;
        let s3 = m01 * m12 - m11 * m02;
        let s4 = m01 * m13 - m11 * m03;
        let s5 = m02 * m13 - m12 * m03;

        let c5 = m22 * m33 - m32 * m23;
        let c4 = m21 * m33 - m31 * m23;
        let c3 = m21 * m32 - m31 * m22;
        let c2 = m20 * m33 - m30 * m23;
        let c1 = m20 * m32 - m30 * m22;
        let c0 = m20 * m31 - m30 * m21;

        let det = s0 * c5 - s1 * c4 + s2 * c3 + s3 * c2 - s4 * c1 + s5 * c0;

        if scalar::fabs(det) < 1e-8 {
            return (det, None);
        }

        let inv_det = 1.0 / det;

        let mut out = Self::default();
        out.rows[0].x = (m11 * c5 - m12 * c4 + m13 * c3) * inv_det;
        out.rows[1].x = (-m01 * c5 + m02 * c4 - m03 * c3) * inv_det;
        out.rows[2].x = (m31 * s5 - m32 * s4 + m33 * s3) * inv_det;
        out.rows[3].x = (-m21 * s5 + m22 * s4 - m23 * s3) * inv_det;

        out.rows[0].y = (-m10 * c5 + m12 * c2 - m13 * c1) * inv_det;
        out.rows[1].y = (m00 * c5 - m02 * c2 + m03 * c1) * inv_det;
        out.rows[2].y = (-m30 * s5 + m32 * s2 - m33 * s1) * inv_det;
        out.rows[3].y = (m20 * s5 - m22 * s2 + m23 * s1) * inv_det;

        out.rows[0].z = (m10 * c4 - m11 * c2 + m13 * c0) * inv_det;
        out.rows[1].z = (-m00 * c4 + m01 * c2 - m03 * c0) * inv_det;
        out.rows[2].z = (m30 * s4 - m31 * s2 + m33 * s0) * inv_det;
        out.rows[3].z = (-m20 * s4 + m21 * s2 - m23 * s0) * inv_det;

        out.rows[0].w = (-m10 * c3 + m11 * c1 - m12 * c0) * inv_det;
        out.rows[1].w = (m00 * c3 - m01 * c1 + m02 * c0) * inv_det;
        out.rows[2].w = (-m30 * s3 + m31 * s1 - m32 * s0) * inv_det;
        out.rows[3].w = (m20 * s3 - m21 * s1 + m22 * s0) * inv_det;

        (det, Some(out))
    }

    /// `Inverse()`: the general inverse; `None` when singular (where the
    /// C++ returns an uninitialized matrix).
    #[must_use]
    #[inline]
    pub fn get_inverse(&self) -> Option<Matrix4> {
        Self::inverse_with_determinant(*self).1
    }

    /// `operator * (Matrix4x4, Matrix4x4)` / `Multiply(A, B)`: the full
    /// 4x4 product (the C++ `ROWCOL` expansion).
    #[must_use]
    #[inline]
    pub fn multiply(a: Matrix4, b: Matrix4) -> Matrix4 {
        let mut out = Self::default();
        for (i, row) in out.rows.iter_mut().enumerate() {
            *row = Vector4::new(
                a.rows[i].x * b.rows[0].x
                    + a.rows[i].y * b.rows[1].x
                    + a.rows[i].z * b.rows[2].x
                    + a.rows[i].w * b.rows[3].x,
                a.rows[i].x * b.rows[0].y
                    + a.rows[i].y * b.rows[1].y
                    + a.rows[i].z * b.rows[2].y
                    + a.rows[i].w * b.rows[3].y,
                a.rows[i].x * b.rows[0].z
                    + a.rows[i].y * b.rows[1].z
                    + a.rows[i].z * b.rows[2].z
                    + a.rows[i].w * b.rows[3].z,
                a.rows[i].x * b.rows[0].w
                    + a.rows[i].y * b.rows[1].w
                    + a.rows[i].z * b.rows[2].w
                    + a.rows[i].w * b.rows[3].w,
            );
        }
        out
    }

    /// `operator * (Matrix4x4, Matrix3D)`: the mixed product, hand-coded
    /// for `b`'s implicit `0 0 0 1` last row (the last row of the result
    /// adds `a`'s fourth column).
    #[must_use]
    pub fn multiply_4_3d(a: Matrix4, b: Matrix3D) -> Matrix4 {
        let mut out = Self::default();
        for i in 0..4 {
            out.rows[i] = Vector4::new(
                a.rows[i].x * b.rows[0].x + a.rows[i].y * b.rows[1].x + a.rows[i].z * b.rows[2].x,
                a.rows[i].x * b.rows[0].y + a.rows[i].y * b.rows[1].y + a.rows[i].z * b.rows[2].y,
                a.rows[i].x * b.rows[0].z + a.rows[i].y * b.rows[1].z + a.rows[i].z * b.rows[2].z,
                a.rows[i].x * b.rows[0].w
                    + a.rows[i].y * b.rows[1].w
                    + a.rows[i].z * b.rows[2].w
                    + a.rows[i].w,
            );
        }
        out
    }

    /// `operator * (Matrix3D, Matrix4x4)`: the mixed product, hand-coded
    /// for `a`'s implicit `0 0 0 1` last row (the result's last row is
    /// `b`'s last row).
    #[must_use]
    pub fn multiply_3d_4(a: Matrix3D, b: Matrix4) -> Matrix4 {
        let mut out = Self::default();
        for i in 0..3 {
            out.rows[i] = Vector4::new(
                a.rows[i].x * b.rows[0].x
                    + a.rows[i].y * b.rows[1].x
                    + a.rows[i].z * b.rows[2].x
                    + a.rows[i].w * b.rows[3].x,
                a.rows[i].x * b.rows[0].y
                    + a.rows[i].y * b.rows[1].y
                    + a.rows[i].z * b.rows[2].y
                    + a.rows[i].w * b.rows[3].y,
                a.rows[i].x * b.rows[0].z
                    + a.rows[i].y * b.rows[1].z
                    + a.rows[i].z * b.rows[2].z
                    + a.rows[i].w * b.rows[3].z,
                a.rows[i].x * b.rows[0].w
                    + a.rows[i].y * b.rows[1].w
                    + a.rows[i].z * b.rows[2].w
                    + a.rows[i].w * b.rows[3].w,
            );
        }
        out.rows[3] = b.rows[3];
        out
    }

    /// `Transform_Vector(tm, in, out)` (Vector3 in, Vector3 out): the
    /// transform of a point.
    #[must_use]
    pub fn transform_vector3(&self, v: Vector3) -> Vector3 {
        Vector3::new(
            self.rows[0].x * v.x + self.rows[0].y * v.y + self.rows[0].z * v.z + self.rows[0].w,
            self.rows[1].x * v.x + self.rows[1].y * v.y + self.rows[1].z * v.z + self.rows[1].w,
            self.rows[2].x * v.x + self.rows[2].y * v.y + self.rows[2].z * v.z + self.rows[2].w,
        )
    }

    /// `Transform_Vector(tm, in, out)` (Vector3 in, Vector4 out): the
    /// point transform with the result's `w` forced to 1.
    #[must_use]
    pub fn transform_vector3_w1(&self, v: Vector3) -> Vector4 {
        Vector4::new(
            self.rows[0].x * v.x + self.rows[0].y * v.y + self.rows[0].z * v.z + self.rows[0].w,
            self.rows[1].x * v.x + self.rows[1].y * v.y + self.rows[1].z * v.z + self.rows[1].w,
            self.rows[2].x * v.x + self.rows[2].y * v.y + self.rows[2].z * v.z + self.rows[2].w,
            1.0,
        )
    }

    /// `Transform_Vector(tm, in, out)` (Vector4 in, Vector4 out) /
    /// `operator * (Matrix4x4, Vector4)`: the full homogeneous
    /// transform.
    #[must_use]
    pub fn transform_vector4(&self, v: Vector4) -> Vector4 {
        Vector4::new(
            self.rows[0].x * v.x
                + self.rows[0].y * v.y
                + self.rows[0].z * v.z
                + self.rows[0].w * v.w,
            self.rows[1].x * v.x
                + self.rows[1].y * v.y
                + self.rows[1].z * v.z
                + self.rows[1].w * v.w,
            self.rows[2].x * v.x
                + self.rows[2].y * v.y
                + self.rows[2].z * v.z
                + self.rows[2].w * v.w,
            self.rows[3].x * v.x
                + self.rows[3].y * v.y
                + self.rows[3].z * v.z
                + self.rows[3].w * v.w,
        )
    }

    /// `operator * (Matrix4x4, Vector3)`: the point transform with
    /// `w = 1` (the result keeps the projected fourth component).
    #[must_use]
    pub fn transform_point(&self, v: Vector3) -> Vector4 {
        Vector4::new(
            self.rows[0].x * v.x + self.rows[0].y * v.y + self.rows[0].z * v.z + self.rows[0].w,
            self.rows[1].x * v.x + self.rows[1].y * v.y + self.rows[1].z * v.z + self.rows[1].w,
            self.rows[2].x * v.x + self.rows[2].y * v.y + self.rows[2].z * v.z + self.rows[2].w,
            self.rows[3].x * v.x + self.rows[3].y * v.y + self.rows[3].z * v.z + self.rows[3].w,
        )
    }
}

impl Mul for Matrix4 {
    type Output = Self;

    /// `operator * (Matrix4x4, Matrix4x4)`.
    fn mul(self, rhs: Self) -> Self {
        Self::multiply(self, rhs)
    }
}

impl Mul<Matrix3D> for Matrix4 {
    type Output = Matrix4;

    /// `operator * (Matrix4x4, Matrix3D)`.
    fn mul(self, rhs: Matrix3D) -> Matrix4 {
        Matrix4::multiply_4_3d(self, rhs)
    }
}

impl Mul<Matrix4> for Matrix3D {
    type Output = Matrix4;

    /// `operator * (Matrix3D, Matrix4x4)`.
    fn mul(self, rhs: Matrix4) -> Matrix4 {
        Matrix4::multiply_3d_4(self, rhs)
    }
}

impl Mul<f32> for Matrix4 {
    type Output = Self;

    /// `operator * (Matrix4x4, float)`.
    fn mul(self, rhs: f32) -> Self {
        Self::from_rows(
            self.rows[0] * rhs,
            self.rows[1] * rhs,
            self.rows[2] * rhs,
            self.rows[3] * rhs,
        )
    }
}

impl Mul<Matrix4> for f32 {
    type Output = Matrix4;

    /// `operator * (float, Matrix4x4)`.
    fn mul(self, rhs: Matrix4) -> Matrix4 {
        rhs * self
    }
}

impl Div<f32> for Matrix4 {
    type Output = Self;

    /// `operator / (Matrix4x4, float)`: scales by the **reciprocal**
    /// (correctly, unlike [`DivAssign`]).
    fn div(self, rhs: f32) -> Self {
        let ood = 1.0 / rhs;
        Self::from_rows(
            self.rows[0] * ood,
            self.rows[1] * ood,
            self.rows[2] * ood,
            self.rows[3] * ood,
        )
    }
}

impl MulAssign<f32> for Matrix4 {
    /// `operator *= (float)`.
    fn mul_assign(&mut self, rhs: f32) {
        for row in &mut self.rows {
            *row = *row * rhs;
        }
    }
}

impl DivAssign<f32> for Matrix4 {
    /// `operator /= (float)` — the original's bug preserved: it reads
    /// `float ood = d;` and **multiplies** by `d` (no reciprocal).
    // Multiplying inside a DivAssign is exactly the preserved bug.
    #[allow(clippy::suspicious_op_assign_impl)]
    fn div_assign(&mut self, rhs: f32) {
        let ood = rhs;
        for row in &mut self.rows {
            *row *= ood;
        }
    }
}

impl Add for Matrix4 {
    type Output = Self;

    /// `operator +`.
    fn add(self, rhs: Self) -> Self {
        Self::from_rows(
            self.rows[0] + rhs.rows[0],
            self.rows[1] + rhs.rows[1],
            self.rows[2] + rhs.rows[2],
            self.rows[3] + rhs.rows[3],
        )
    }
}

impl Sub for Matrix4 {
    type Output = Self;

    /// `operator -` (component-wise, correct in every column).
    fn sub(self, rhs: Self) -> Self {
        Self::from_rows(
            self.rows[0] - rhs.rows[0],
            self.rows[1] - rhs.rows[1],
            self.rows[2] - rhs.rows[2],
            self.rows[3] - rhs.rows[3],
        )
    }
}

impl AddAssign for Matrix4 {
    /// `operator +=`.
    fn add_assign(&mut self, rhs: Self) {
        for (row, other) in self.rows.iter_mut().zip(rhs.rows.iter()) {
            *row = *row + *other;
        }
    }
}

impl SubAssign for Matrix4 {
    /// `operator -=` — through `Vector4::operator-=`, whose **W column
    /// is added, not subtracted** (the preserved original bug).
    fn sub_assign(&mut self, rhs: Self) {
        for (row, other) in self.rows.iter_mut().zip(rhs.rows.iter()) {
            *row -= *other;
        }
    }
}

impl Neg for Matrix4 {
    type Output = Self;

    /// `operator -` (unary).
    fn neg(self) -> Self {
        Self::from_rows(-self.rows[0], -self.rows[1], -self.rows[2], -self.rows[3])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    #[test]
    fn identity_and_from_matrix3d() {
        let m = Matrix4::identity();
        assert_eq!(m.rows[3], Vector4::new(0.0, 0.0, 0.0, 1.0));

        let t = Matrix3D::from_position(Vector3::new(1.0, 2.0, 3.0));
        let m = Matrix4::from_matrix3d(t);
        assert_eq!(m.rows[0], Vector4::new(1.0, 0.0, 0.0, 1.0));
        assert_eq!(m.rows[1], Vector4::new(0.0, 1.0, 0.0, 2.0));
        assert_eq!(m.rows[2], Vector4::new(0.0, 0.0, 1.0, 3.0));
        assert_eq!(m.rows[3], Vector4::new(0.0, 0.0, 0.0, 1.0));
    }

    #[test]
    fn multiply_matches_rowcol() {
        let a = Matrix4::from_components(
            1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0,
        );
        let b = Matrix4::from_components(
            16.0, 15.0, 14.0, 13.0, 12.0, 11.0, 10.0, 9.0, 8.0, 7.0, 6.0, 5.0, 4.0, 3.0, 2.0, 1.0,
        );
        let c = a * b;
        assert_eq!(c.rows[0].x, 1.0 * 16.0 + 2.0 * 12.0 + 3.0 * 8.0 + 4.0 * 4.0);
        assert_eq!(
            c.rows[3].w,
            13.0 * 13.0 + 14.0 * 9.0 + 15.0 * 5.0 + 16.0 * 1.0
        );

        // mixed products agree with the full product for a Matrix3D
        // embedded in a Matrix4
        let t = Matrix3D::from_components(
            1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0,
        );
        let full = Matrix4::from_matrix3d(t);
        let m4 = a * t; // Matrix4 * Matrix3D
        assert_eq!(m4, a * full);

        let m3 = t * a; // Matrix3D * Matrix4
        assert_eq!(m3.rows[3], a.rows[3]);
        let full_product = full * a;
        for i in 0..3 {
            assert_eq!(m3.rows[i], full_product.rows[i]);
        }
    }

    #[test]
    fn inverse_round_trips() {
        let mut m = Matrix4::identity();
        m.rows[0].w = 3.0;
        m.rows[1].w = -4.0;
        m.rows[2].w = 5.0;
        m.rows[0].y = 0.5;

        let inv = m.get_inverse().unwrap();
        let product = m * inv;
        for i in 0..4 {
            for j in 0..4 {
                let expected = if i == j { 1.0 } else { 0.0 };
                assert!(
                    approx(product.rows[i].by_index(j), expected),
                    "({i},{j}) = {}",
                    product.rows[i].by_index(j)
                );
            }
        }

        // singular: refuse with the determinant still observable
        let singular = Matrix4::from_components(
            1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0,
        );
        let (det, inv) = Matrix4::inverse_with_determinant(singular);
        assert!(inv.is_none());
        assert!(det.abs() < 1e-6);
    }

    #[test]
    fn projections_map_the_volume() {
        let mut p = Matrix4::identity();
        p.init_ortho(-1.0, 1.0, -1.0, 1.0, 1.0, 3.0);
        // znear (z = -1) maps to -1, zfar (z = -3) maps to +1
        let near = p.transform_point(Vector3::new(0.0, 0.0, -1.0));
        let far = p.transform_point(Vector3::new(0.0, 0.0, -3.0));
        assert!(approx(near.z, -1.0));
        assert!(approx(far.z, 1.0));
        // x/y inside the volume stay within -1..=1
        let corner = p.transform_point(Vector3::new(1.0, 1.0, -2.0));
        assert!(approx(corner.x, 1.0) && approx(corner.y, 1.0));

        // the frustum perspective: the near-plane rectangle maps to
        // the full -1..=1 range
        let mut persp = Matrix4::identity();
        persp.init_perspective_frustum(-1.0, 1.0, -1.0, 1.0, 1.0, 3.0);
        let near_corner = persp.transform_point(Vector3::new(1.0, 1.0, -1.0));
        assert!(approx(near_corner.x, 1.0) && approx(near_corner.y, 1.0));
        let near_center = persp.transform_point(Vector3::new(0.0, 0.0, -1.0));
        assert!(approx(near_center.z, -1.0));

        // the fov perspective is structurally sane (signs and identity
        // rows); its tan term is platform libm so bits are not pinned
        let mut fov = Matrix4::identity();
        fov.init_perspective_fov(1.2, 0.9, 1.0, 100.0);
        assert!(fov.rows[0].x > 0.0 && fov.rows[1].y > 0.0);
        assert_eq!(fov.rows[3].z, -1.0);
        assert_eq!(fov.rows[3].w, 0.0);
        assert!(fov.rows[2].w < 0.0);
    }

    #[test]
    fn compound_operators_preserve_the_quirks() {
        let a = Matrix4::from_components(
            1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0,
        );
        let b = Matrix4::from_components(
            1.0, 1.0, 1.0, 1.0, 2.0, 2.0, 2.0, 2.0, 3.0, 3.0, 3.0, 3.0, 4.0, 4.0, 4.0, 4.0,
        );

        // /= multiplies by the operand (the original's `float ood = d;`)
        let mut d = a;
        d /= 2.0;
        assert_eq!(d.rows[0].x, 2.0); // 1.0 * 2.0, not 0.5

        // the free division takes the reciprocal correctly
        let q = a / 2.0;
        assert_eq!(q.rows[0].x, 0.5);

        // -= adds the W column (the Vector4 operator-= quirk)
        let mut s = a;
        s -= b;
        assert_eq!(s.rows[0].x, 0.0); // 1 - 1
        assert_eq!(s.rows[0].w, 5.0); // 4 + 1, NOT 4 - 1 = 3

        // the free subtraction is correct in every column
        let f = a - b;
        assert_eq!(f.rows[0].w, 3.0); // 4 - 1
    }

    #[test]
    fn transforms() {
        let mut m = Matrix4::identity();
        m.rows[0].w = 10.0;
        m.rows[1].w = 20.0;
        m.rows[2].w = 30.0;

        let p = m.transform_vector3(Vector3::new(1.0, 2.0, 3.0));
        assert_eq!(p, Vector3::new(11.0, 22.0, 33.0));

        let p4 = m.transform_vector3_w1(Vector3::new(1.0, 2.0, 3.0));
        assert_eq!(p4.w, 1.0);

        let v4 = m.transform_vector4(Vector4::new(1.0, 2.0, 3.0, 2.0));
        // the translation column scales with w: 1 + 10*2, 2 + 20*2, ...
        assert_eq!(v4, Vector4::new(21.0, 42.0, 63.0, 2.0));

        let pt = m.transform_point(Vector3::new(1.0, 2.0, 3.0));
        assert_eq!(pt, Vector4::new(11.0, 22.0, 33.0, 1.0));
    }
}
