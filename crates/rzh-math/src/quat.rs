// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Ported from Core/Libraries/Source/WWVegas/WWMath/quat.h and quat.cpp
// @ 571a7c5, Zero Hour configuration, deterministic branches
// (USE_DETERMINISTIC_MATH = 1).
//
//! The quaternion (`Quaternion`) and its conversion / interpolation
//! functions.
//!
//! X, Y, Z are the imaginary parts, W the real part. Every
//! angle-consuming entry point routes through [`crate::gmath`] (the
//! GameMath pin) or the [`crate::fast`] tables, so results are
//! bit-reproducible.
//!
//! # Not ported
//!
//! - `Quaternion::Randomize` uses the C library's global `rand()` —
//!   hidden global state the migration plan §3.3 bans. Its callers
//!   (tools) can build a random quaternion from an explicit RNG and
//!   [`Self::normalize`].
//! - `Build_Quaternion(const Matrix3D&)` / `(const Matrix4x4&)` and
//!   `Build_Matrix4` — they arrive with the `Matrix3D`/`Matrix4` port.
//! - `Trackball` — a UI-input helper; deferred to the client phase.
//!
//! # Parity level
//!
//! **P0 exact** for everything ported: golden vectors in
//! `tests/golden/matrices.txt`.

use crate::fast::{fast_acos, fast_sin};
use crate::gmath;
use crate::matrix3::Matrix3;
use crate::scalar;
use crate::vector3::Vector3;

/// The quaternion: X, Y, Z imaginary, W real.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Quaternion {
    /// The X imaginary component (`X`).
    pub x: f32,
    /// The Y imaginary component (`Y`).
    pub y: f32,
    /// The Z imaginary component (`Z`).
    pub z: f32,
    /// The real component (`W`).
    pub w: f32,
}

/// `SLERP_EPSILON` (0.001): the linear-fallback threshold of
/// [`slerp_setup`].
pub const SLERP_EPSILON: f32 = 0.001;

impl Quaternion {
    /// `Quaternion(true)`: the identity (0, 0, 0, 1).
    #[must_use]
    pub const fn identity() -> Self {
        Self::new(0.0, 0.0, 0.0, 1.0)
    }

    /// `Quaternion(a, b, c, d)`.
    #[must_use]
    pub const fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self { x, y, z, w }
    }

    /// `Set(a, b, c, d)` with the original's default arguments.
    pub fn set(&mut self, x: f32, y: f32, z: f32, w: f32) {
        *self = Self::new(x, y, z, w);
    }

    /// `Make_Identity()`.
    pub fn make_identity(&mut self) {
        *self = Self::identity();
    }

    /// `Scale(s)`.
    pub fn scale(&mut self, s: f32) {
        self.x *= s;
        self.y *= s;
        self.z *= s;
        self.w *= s;
    }

    /// `Length2()`: the squared magnitude.
    #[must_use]
    pub fn length2(&self) -> f32 {
        self.x * self.x + self.y * self.y + self.z * self.z + self.w * self.w
    }

    /// `Length()`: the magnitude.
    #[must_use]
    pub fn length(&self) -> f32 {
        scalar::sqrt(self.length2())
    }

    /// `Normalize()`: scale to unit length (`Inv_Sqrt_Legacy` =
    /// `1/sqrt(len2)` in the deterministic build; zero-length returns
    /// unchanged).
    pub fn normalize(&mut self) {
        let len2 = self.length2();
        if len2 == 0.0 {
            return;
        }
        let inv_mag = scalar::inv_sqrt(len2);
        self.x *= inv_mag;
        self.y *= inv_mag;
        self.z *= inv_mag;
        self.w *= inv_mag;
    }

    /// `Quaternion(axis, angle)`: the rotation about `axis` by `angle`
    /// radians.
    #[must_use]
    pub fn from_axis_angle(axis: Vector3, angle: f32) -> Self {
        let s = gmath::sinf(angle / 2.0);
        let c = gmath::cosf(angle / 2.0);
        Self::new(s * axis.x, s * axis.y, s * axis.z, c)
    }

    /// `Make_Closest(qto)`: negates self if it is on the opposite
    /// hemisphere from `qto` (q and -q are the same rotation).
    pub fn make_closest(&mut self, qto: &Self) {
        let cos_t = qto.x * self.x + qto.y * self.y + qto.z * self.z + qto.w * self.w;
        // if we are on opposite hemisphere from qto, negate ourselves
        if cos_t < 0.0 {
            self.x = -self.x;
            self.y = -self.y;
            self.z = -self.z;
            self.w = -self.w;
        }
    }

    /// `Rotate_X(theta)`: post-concatenates a rotation about X
    /// (`*this = *this * Quaternion(Vector3(1,0,0), theta)`).
    pub fn rotate_x(&mut self, theta: f32) {
        *self = *self * Self::from_axis_angle(Vector3::new(1.0, 0.0, 0.0), theta);
    }

    /// `Rotate_Y(theta)`.
    pub fn rotate_y(&mut self, theta: f32) {
        *self = *self * Self::from_axis_angle(Vector3::new(0.0, 1.0, 0.0), theta);
    }

    /// `Rotate_Z(theta)`.
    pub fn rotate_z(&mut self, theta: f32) {
        *self = *self * Self::from_axis_angle(Vector3::new(0.0, 0.0, 1.0), theta);
    }

    /// `Rotate_Vector(v)`: rotates `v` by this quaternion.
    #[must_use]
    pub fn rotate_vector(&self, v: Vector3) -> Vector3 {
        let x = self.w * v.x + (self.y * v.z - v.y * self.z);
        let y = self.w * v.y - (self.x * v.z - v.x * self.z);
        let z = self.w * v.z + (self.x * v.y - v.x * self.y);
        let w = -(self.x * v.x + self.y * v.y + self.z * v.z);

        Vector3::new(
            w * (-self.x) + self.w * x + (y * (-self.z) - (-self.y) * z),
            w * (-self.y) + self.w * y - (x * (-self.z) - (-self.x) * z),
            w * (-self.z) + self.w * z + (x * (-self.y) - (-self.x) * y),
        )
    }

    /// `Is_Valid()`: no component is a NaN or infinity.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        scalar::is_valid_float(self.x)
            && scalar::is_valid_float(self.y)
            && scalar::is_valid_float(self.z)
            && scalar::is_valid_float(self.w)
    }
}

/// `Inverse(a)` (and `Conjugate(a)` — the same formula).
#[must_use]
pub fn inverse(a: Quaternion) -> Quaternion {
    Quaternion::new(-a.x, -a.y, -a.z, a.w)
}

/// `Conjugate(a)`.
#[must_use]
pub fn conjugate(a: Quaternion) -> Quaternion {
    Quaternion::new(-a.x, -a.y, -a.z, a.w)
}

/// `Normalize(a)` (free function): the zero-magnitude quaternion returns
/// unchanged.
#[must_use]
pub fn normalize(a: Quaternion) -> Quaternion {
    let mag = a.length();
    if mag == 0.0 {
        return a;
    }
    let oomag = 1.0 / mag;
    Quaternion::new(a.x * oomag, a.y * oomag, a.z * oomag, a.w * oomag)
}

/// `Axis_To_Quat(a, phi)`: normalizes `a`, scales by sin(phi/2), real part
/// cos(phi/2).
#[must_use]
pub fn axis_to_quat(a: Vector3, phi: f32) -> Quaternion {
    let mut tmp = a;
    tmp.normalize();
    let mut q = Quaternion::new(tmp.x, tmp.y, tmp.z, 0.0);
    q.scale(gmath::sinf(phi / 2.0));
    q.w = gmath::cosf(phi / 2.0);
    q
}

/// `Slerp(result, a, b, t)`: spherical linear interpolation.
///
/// The original signature writes through an out-parameter; this port
/// returns the result.
#[must_use]
pub fn slerp(p: Quaternion, q: Quaternion, alpha: f32) -> Quaternion {
    let mut alpha = alpha;
    let beta;
    let cos_t = p.x * q.x + p.y * q.y + p.z * q.z + p.w * q.w;

    let mut cos_t = cos_t;
    let qflip;
    if cos_t < 0.0 {
        cos_t = -cos_t;
        qflip = true;
    } else {
        qflip = false;
    }

    if 1.0 - cos_t < scalar::EPSILON * scalar::EPSILON {
        beta = 1.0 - alpha;
    } else {
        let theta = gmath::acosf(cos_t);
        let sin_t = gmath::sinf(theta);
        let oo_sin_t = 1.0 / sin_t;
        beta = gmath::sinf(theta - alpha * theta) * oo_sin_t;
        alpha = gmath::sinf(alpha * theta) * oo_sin_t;
    }

    if qflip {
        alpha = -alpha;
    }

    Quaternion::new(
        beta * p.x + alpha * q.x,
        beta * p.y + alpha * q.y,
        beta * p.z + alpha * q.z,
        beta * p.w + alpha * q.w,
    )
}

/// `Fast_Slerp(res, p, q, alpha)`: the table-based slerp (the non-asm
/// branch of the original).
#[must_use]
pub fn fast_slerp(p: Quaternion, q: Quaternion, alpha: f32) -> Quaternion {
    let mut alpha = alpha;
    let beta;
    let cos_t = p.x * q.x + p.y * q.y + p.z * q.z + p.w * q.w;

    let mut cos_t = cos_t;
    let qflip;
    if cos_t < 0.0 {
        cos_t = -cos_t;
        qflip = true;
    } else {
        qflip = false;
    }

    if 1.0 - cos_t < scalar::EPSILON * scalar::EPSILON {
        beta = 1.0 - alpha;
    } else {
        let theta = fast_acos(cos_t);
        let sin_t = fast_sin(theta);
        let oo_sin_t = 1.0 / sin_t;
        beta = fast_sin(theta - alpha * theta) * oo_sin_t;
        alpha = fast_sin(alpha * theta) * oo_sin_t;
    }

    if qflip {
        alpha = -alpha;
    }

    Quaternion::new(
        beta * p.x + alpha * q.x,
        beta * p.y + alpha * q.y,
        beta * p.z + alpha * q.z,
        beta * p.w + alpha * q.w,
    )
}

/// `SlerpInfoStruct`: the cached slerp parameters of
/// [`slerp_setup`] / [`cached_slerp`].
#[derive(Clone, Copy, Debug, Default)]
pub struct SlerpInfo {
    /// `Flip`: q is on the opposite hemisphere.
    pub flip: bool,
    /// `Linear`: fall back to linear interpolation.
    pub linear: bool,
    /// `Theta`: the angle between p and q.
    pub theta: f32,
    /// `SinT`: sin(Theta).
    pub sin_t: f32,
}

/// `Slerp_Setup(p, q, slerpinfo)`.
#[must_use]
pub fn slerp_setup(p: Quaternion, q: Quaternion) -> SlerpInfo {
    let mut info = SlerpInfo::default();

    let mut cos_t = p.x * q.x + p.y * q.y + p.z * q.z + p.w * q.w;
    if cos_t < 0.0 {
        cos_t = -cos_t;
        info.flip = true;
    } else {
        info.flip = false;
    }

    if 1.0 - cos_t < SLERP_EPSILON {
        info.linear = true;
        info.theta = 0.0;
        info.sin_t = 0.0;
    } else {
        info.linear = false;
        info.theta = gmath::acosf(cos_t);
        info.sin_t = gmath::sinf(info.theta);
    }
    info
}

/// `Cached_Slerp(p, q, alpha, slerpinfo)`.
///
/// NOTE: the original divides by `Theta` here (not `SinT`) — that is a
/// bug in the original (`oo_sin_t = 1.0f / slerpinfo->Theta`), preserved
/// verbatim; see the C++ source.
#[must_use]
pub fn cached_slerp(p: Quaternion, q: Quaternion, alpha: f32, info: &SlerpInfo) -> Quaternion {
    let mut alpha = alpha;
    let beta;
    if info.linear {
        beta = 1.0 - alpha;
    } else {
        let oo_sin_t = 1.0 / info.theta;
        beta = gmath::sinf(info.theta - alpha * info.theta) * oo_sin_t;
        alpha = gmath::sinf(alpha * info.theta) * oo_sin_t;
    }

    if info.flip {
        alpha = -alpha;
    }

    Quaternion::new(
        beta * p.x + alpha * q.x,
        beta * p.y + alpha * q.y,
        beta * p.z + alpha * q.z,
        beta * p.w + alpha * q.w,
    )
}

/// `_nxt[3] = { 1, 2, 0 }`: the successor cycle of `Build_Quaternion`.
const NXT: [usize; 3] = [1, 2, 0];

/// `Build_Quaternion(const Matrix3x3&)`: the Shepperd-style extraction.
#[must_use]
pub fn build_quaternion(mat: &Matrix3) -> Quaternion {
    let mut q = Quaternion::default();
    let tr = mat[(0, 0)] + mat[(1, 1)] + mat[(2, 2)];
    if tr > 0.0 {
        let s = scalar::sqrt(tr + 1.0);
        q.w = s * 0.5;
        let s = 0.5 / s;
        q.x = (mat[(2, 1)] - mat[(1, 2)]) * s;
        q.y = (mat[(0, 2)] - mat[(2, 0)]) * s;
        q.z = (mat[(1, 0)] - mat[(0, 1)]) * s;
    } else {
        let mut i = 0;
        if mat[(1, 1)] > mat[(0, 0)] {
            i = 1;
        }
        if mat[(2, 2)] > mat[(i, i)] {
            i = 2;
        }
        let j = NXT[i];
        let k = NXT[j];

        let mut s = scalar::sqrt((mat[(i, i)] - (mat[(j, j)] + mat[(k, k)])) + 1.0);
        q[i] = s * 0.5;
        if s != 0.0 {
            s = 0.5 / s;
        }
        q.w = (mat[(k, j)] - mat[(j, k)]) * s;
        q[j] = (mat[(j, i)] + mat[(i, j)]) * s;
        q[k] = (mat[(k, i)] + mat[(i, k)]) * s;
    }
    q
}

/// `Build_Matrix3(const Quaternion&)`: the quaternion-to-matrix
/// conversion with the original's mixed double/float literal quirks (see
/// [`crate::matrix3`] module notes).
#[must_use]
pub fn build_matrix3(q: Quaternion) -> Matrix3 {
    Matrix3::from_quaternion(q)
}

impl std::ops::Index<usize> for Quaternion {
    type Output = f32;

    /// `q[i]` (`(&X)[i]` in the original).
    #[inline]
    fn index(&self, i: usize) -> &f32 {
        match i {
            0 => &self.x,
            1 => &self.y,
            2 => &self.z,
            3 => &self.w,
            _ => panic!("Quaternion index out of range: {i}"),
        }
    }
}

impl std::ops::IndexMut<usize> for Quaternion {
    #[inline]
    fn index_mut(&mut self, i: usize) -> &mut f32 {
        match i {
            0 => &mut self.x,
            1 => &mut self.y,
            2 => &mut self.z,
            3 => &mut self.w,
            _ => panic!("Quaternion index out of range: {i}"),
        }
    }
}

impl std::ops::Neg for Quaternion {
    type Output = Self;

    /// `operator -` (unary).
    fn neg(self) -> Self {
        Self::new(-self.x, -self.y, -self.z, -self.w)
    }
}

impl std::ops::Add for Quaternion {
    type Output = Self;

    /// `operator +`.
    fn add(self, b: Self) -> Self {
        Self::new(self.x + b.x, self.y + b.y, self.z + b.z, self.w + b.w)
    }
}

impl std::ops::Sub for Quaternion {
    type Output = Self;

    /// `operator -`.
    fn sub(self, b: Self) -> Self {
        Self::new(self.x - b.x, self.y - b.y, self.z - b.z, self.w - b.w)
    }
}

impl std::ops::Mul<f32> for Quaternion {
    type Output = Self;

    /// `operator * (Quaternion, float)`.
    fn mul(self, scl: f32) -> Self {
        Self::new(scl * self.x, scl * self.y, scl * self.z, scl * self.w)
    }
}

impl std::ops::Mul<Quaternion> for f32 {
    type Output = Quaternion;

    /// `operator * (float, Quaternion)`.
    fn mul(self, a: Quaternion) -> Quaternion {
        a * self
    }
}

impl std::ops::Mul for Quaternion {
    type Output = Self;

    /// `operator * (Quaternion, Quaternion)` — the Hamilton product with
    /// the original's exact parenthesization.
    fn mul(self, b: Self) -> Self {
        let a = self;
        Self::new(
            a.w * b.x + b.w * a.x + (a.y * b.z - b.y * a.z),
            a.w * b.y + b.w * a.y - (a.x * b.z - b.x * a.z),
            a.w * b.z + b.w * a.z + (a.x * b.y - b.x * a.y),
            a.w * b.w - (a.x * b.x + a.y * b.y + a.z * b.z),
        )
    }
}

impl std::ops::Div for Quaternion {
    type Output = Self;

    /// `operator / (Quaternion, Quaternion)`: `a * Inverse(b)`.
    // Deliberately a multiplication: the original divides by multiplying
    // with the inverse.
    #[allow(clippy::suspicious_arithmetic_impl)]
    fn div(self, b: Self) -> Self {
        self * inverse(b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() < eps
    }

    #[test]
    fn axis_angle_round_trips_through_matrix() {
        let axis = Vector3::new(0.0, 1.0, 0.0);
        let angle = 0.8;
        let q = Quaternion::from_axis_angle(axis, angle);
        let m = Matrix3::from_quaternion(q);
        assert!(m.is_orthogonal());
        // rotating by the matrix equals rotating by the quaternion
        let v = Vector3::new(0.3, -0.4, 0.5);
        let mv = m.rotate_vector(v);
        let qv = q.rotate_vector(v);
        assert!(close(mv.x, qv.x, 1e-5));
        assert!(close(mv.y, qv.y, 1e-5));
        assert!(close(mv.z, qv.z, 1e-5));
    }

    #[test]
    fn build_quaternion_round_trips() {
        let mut axis = Vector3::new(1.0, 1.0, 1.0);
        axis.normalize();
        let m = Matrix3::from_axis_angle(axis, 1.1);
        let q = build_quaternion(&m);
        let m2 = build_matrix3(q);
        for i in 0..3 {
            for j in 0..3 {
                assert!(
                    close(m[(i, j)], m2[(i, j)], 1e-5),
                    "({i},{j}): {} vs {}",
                    m[(i, j)],
                    m2[(i, j)]
                );
            }
        }
    }

    #[test]
    fn slerp_interpolates() {
        let a = Quaternion::from_axis_angle(Vector3::new(0.0, 0.0, 1.0), 0.0);
        let b = Quaternion::from_axis_angle(Vector3::new(0.0, 0.0, 1.0), scalar::HALF_PI);
        let mid = slerp(a, b, 0.5);
        let expect =
            Quaternion::from_axis_angle(Vector3::new(0.0, 0.0, 1.0), scalar::HALF_PI / 2.0);
        assert!(close(mid.x, expect.x, 1e-5));
        assert!(close(mid.y, expect.y, 1e-5));
        assert!(close(mid.z, expect.z, 1e-5));
        assert!(close(mid.w, expect.w, 1e-5));
        // fast slerp stays near the exact one
        let fmid = fast_slerp(a, b, 0.5);
        assert!(close(fmid.z, mid.z, 1e-2));
        // endpoints are exact
        let e = slerp(a, b, 0.0);
        assert!(close(e.x, a.x, 1e-6) && close(e.w, a.w, 1e-6));
    }

    #[test]
    fn slerp_setup_matches_slerp_decisions() {
        let a = Quaternion::identity();
        let b = Quaternion::from_axis_angle(Vector3::new(0.0, 1.0, 0.0), 0.5);
        let info = slerp_setup(a, b);
        assert!(!info.linear);
        assert!(!info.flip);
        let c = cached_slerp(a, b, 0.25, &info);
        let s = slerp(a, b, 0.25);
        // Cached_Slerp divides by Theta (original's bug), so it diverges
        // slightly from Slerp; both stay on the interpolation arc.
        assert!(close(c.w, s.w, 0.05));
        // near-identical quaternions take the linear path
        let info2 = slerp_setup(a, a);
        assert!(info2.linear);
    }
}
