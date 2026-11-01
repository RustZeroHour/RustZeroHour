// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Ported from Core/Libraries/Source/WWVegas/WWMath/vector4.h @ 571a7c5, Zero
// Hour configuration. The original carries the following notices, preserved
// here:
//
//   VECTOR4.h — Westwood 3D. Programmer: Greg Hjelstrom. 02/24/1997.
//
//   (c) 2001-2003 Electronic Arts Inc.
//
//! 4D float vector (`Vector4`).
//!
//! Ported from `vector4.h`, quirks included and documented in place:
//!
//! - the compound `operator -=` **adds** the W component instead of
//!   subtracting it (see [`SubAssign`]),
//! - the out-parameter `Lerp` writes the W interpolation into X and leaves
//!   W untouched (see [`Vector4::lerp_out`]).
//!
//! Both are behaviour of the baseline and must not be "fixed" without an
//! ADR; the golden vectors pin them.

use core::ops::{Add, AddAssign, Div, Index, IndexMut, Mul, MulAssign, Neg, Sub, SubAssign};

use crate::scalar;

/// A 4D float vector (`Vector4`).
#[derive(Debug, Default, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct Vector4 {
    /// X component (`X`).
    pub x: f32,
    /// Y component (`Y`).
    pub y: f32,
    /// Z component (`Z`).
    pub z: f32,
    /// W component (`W`).
    pub w: f32,
}

impl Vector4 {
    /// Constructs a vector from its components.
    #[must_use]
    pub const fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self { x, y, z, w }
    }

    /// Sets all components (`Set`).
    pub fn set(&mut self, x: f32, y: f32, z: f32, w: f32) {
        self.x = x;
        self.y = y;
        self.z = z;
        self.w = w;
    }

    /// The squared length (`Length2`).
    #[must_use]
    #[inline]
    pub fn length2(self) -> f32 {
        self.x * self.x + self.y * self.y + self.z * self.z + self.w * self.w
    }

    /// The length (`Length`).
    #[must_use]
    #[inline]
    pub fn length(self) -> f32 {
        scalar::sqrt(self.length2())
    }

    /// Normalizes in place (`Normalize`).
    pub fn normalize(&mut self) {
        let len2 = self.length2();
        if len2 != 0.0 {
            let oolen = scalar::inv_sqrt(len2);
            self.x *= oolen;
            self.y *= oolen;
            self.z *= oolen;
            self.w *= oolen;
        }
    }

    /// The free `Normalize(vec)` of `vector4.h`: multiplies by
    /// `1/sqrt(len2)` (unlike [`Vector2::normalize_free`], this one really
    /// multiplies). Zero-length input returns `(0, 0, 0, 0)`.
    ///
    /// [`Vector2::normalize_free`]: crate::vector2::Vector2::normalize_free
    #[must_use]
    pub fn normalize_free(vec: Self) -> Self {
        let len2 = vec.length2();
        if len2 != 0.0 {
            let oolen = scalar::inv_sqrt(len2);
            return vec * oolen;
        }
        Self::new(0.0, 0.0, 0.0, 0.0)
    }

    /// `Dot_Product` (the C++ `a * b`).
    #[must_use]
    #[inline]
    pub fn dot(a: Self, b: Self) -> f32 {
        a.x * b.x + a.y * b.y + a.z * b.z + a.w * b.w
    }

    /// The static `Vector4::Lerp(a, b, alpha)`: the correct interpolation of
    /// all four components.
    #[must_use]
    #[inline]
    pub fn lerp(a: Self, b: Self, alpha: f32) -> Self {
        Self::new(
            a.x + (b.x - a.x) * alpha,
            a.y + (b.y - a.y) * alpha,
            a.z + (b.z - a.z) * alpha,
            a.w + (b.w - a.w) * alpha,
        )
    }

    /// The out-parameter `Vector4::Lerp(a, b, alpha, result)`, quirk
    /// included: the original writes the **W** interpolation into `X` and
    /// never touches W. Transcribed verbatim.
    #[allow(clippy::similar_names)]
    pub fn lerp_out(a: Self, b: Self, alpha: f32, set_result: &mut Self) {
        set_result.x = a.x + (b.x - a.x) * alpha;
        set_result.y = a.y + (b.y - a.y) * alpha;
        set_result.z = a.z + (b.z - a.z) * alpha;
        // Original quirk: `set_result->X = ...` again, not W.
        set_result.x = a.w + (b.w - a.w) * alpha;
    }

    /// `Is_Valid`: no component is NaN or infinite.
    #[must_use]
    pub fn is_valid(self) -> bool {
        scalar::is_valid_float(self.x)
            && scalar::is_valid_float(self.y)
            && scalar::is_valid_float(self.z)
            && scalar::is_valid_float(self.w)
    }
}

/// The free `Lerp(a, b, alpha)` of `vector4.h`; identical to
/// [`Vector4::lerp`].
#[must_use]
#[inline]
pub fn lerp(a: Vector4, b: Vector4, alpha: f32) -> Vector4 {
    Vector4::lerp(a, b, alpha)
}

impl Neg for Vector4 {
    type Output = Self;

    #[inline]
    fn neg(self) -> Self {
        Self::new(-self.x, -self.y, -self.z, -self.w)
    }
}

impl Add for Vector4 {
    type Output = Self;

    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self::new(
            self.x + rhs.x,
            self.y + rhs.y,
            self.z + rhs.z,
            self.w + rhs.w,
        )
    }
}

impl Sub for Vector4 {
    type Output = Self;

    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Self::new(
            self.x - rhs.x,
            self.y - rhs.y,
            self.z - rhs.z,
            self.w - rhs.w,
        )
    }
}

/// The original `operator -=`, quirk included: **W is added, not
/// subtracted**. Use [`Sub`] for the correct difference.
impl SubAssign for Vector4 {
    #[inline]
    fn sub_assign(&mut self, rhs: Self) {
        self.x -= rhs.x;
        self.y -= rhs.y;
        self.z -= rhs.z;
        self.w += rhs.w;
    }
}

impl AddAssign for Vector4 {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        self.x += rhs.x;
        self.y += rhs.y;
        self.z += rhs.z;
        self.w += rhs.w;
    }
}

impl Mul<f32> for Vector4 {
    type Output = Self;

    #[inline]
    fn mul(self, k: f32) -> Self {
        Self::new(self.x * k, self.y * k, self.z * k, self.w * k)
    }
}

impl Mul<Vector4> for f32 {
    type Output = Vector4;

    /// `operator * (float, Vector4)`; the original forwards to
    /// `Vector4 * float`.
    #[inline]
    fn mul(self, rhs: Vector4) -> Vector4 {
        rhs * self
    }
}

impl MulAssign<f32> for Vector4 {
    #[inline]
    fn mul_assign(&mut self, k: f32) {
        self.x *= k;
        self.y *= k;
        self.z *= k;
        self.w *= k;
    }
}

impl Div<f32> for Vector4 {
    type Output = Self;

    /// `operator / (Vector4, float)`: multiplies by the reciprocal `1/k`.
    #[inline]
    fn div(self, k: f32) -> Self {
        let ook = 1.0 / k;
        Self::new(self.x * ook, self.y * ook, self.z * ook, self.w * ook)
    }
}

/// `operator /= (float)`: reciprocal multiply, like [`Div`].
impl core::ops::DivAssign<f32> for Vector4 {
    #[inline]
    fn div_assign(&mut self, k: f32) {
        let ook = 1.0 / k;
        self.x *= ook;
        self.y *= ook;
        self.z *= ook;
        self.w *= ook;
    }
}

impl Index<usize> for Vector4 {
    type Output = f32;

    #[inline]
    fn index(&self, i: usize) -> &f32 {
        match i {
            0 => &self.x,
            1 => &self.y,
            2 => &self.z,
            3 => &self.w,
            _ => panic!("Vector4 index out of range: {i}"),
        }
    }
}

impl IndexMut<usize> for Vector4 {
    #[inline]
    fn index_mut(&mut self, i: usize) -> &mut f32 {
        match i {
            0 => &mut self.x,
            1 => &mut self.y,
            2 => &mut self.z,
            3 => &mut self.w,
            _ => panic!("Vector4 index out of range: {i}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Vector4, lerp};

    #[test]
    fn arithmetic_is_componentwise() {
        let a = Vector4::new(1.0, 2.0, 3.0, 4.0);
        let b = Vector4::new(5.0, 6.0, 7.0, 8.0);
        assert_eq!(a + b, Vector4::new(6.0, 8.0, 10.0, 12.0));
        assert_eq!(a - b, Vector4::new(-4.0, -4.0, -4.0, -4.0));
        assert_eq!(a * 2.0, Vector4::new(2.0, 4.0, 6.0, 8.0));
        assert_eq!(2.0 * a, Vector4::new(2.0, 4.0, 6.0, 8.0));
        assert_eq!(a / 2.0, Vector4::new(0.5, 1.0, 1.5, 2.0));
        assert_eq!(-a, Vector4::new(-1.0, -2.0, -3.0, -4.0));
    }

    #[test]
    fn sub_assign_keeps_the_w_addition_quirk() {
        let a = Vector4::new(1.0, 2.0, 3.0, 4.0);
        let b = Vector4::new(5.0, 6.0, 7.0, 8.0);
        let mut via_assign = a;
        via_assign -= b;
        // W is ADDED (4 + 8), unlike the `-` operator (4 - 8).
        assert_eq!(via_assign, Vector4::new(-4.0, -4.0, -4.0, 12.0));
        assert_eq!(a - b, Vector4::new(-4.0, -4.0, -4.0, -4.0));
    }

    #[test]
    fn lerp_forms_disagree_on_w_by_design() {
        let a = Vector4::new(0.0, 0.0, 0.0, 0.0);
        let b = Vector4::new(10.0, 20.0, 30.0, 40.0);
        let correct = Vector4::lerp(a, b, 0.5);
        assert_eq!(correct, Vector4::new(5.0, 10.0, 15.0, 20.0));
        assert_eq!(lerp(a, b, 0.5), correct);

        let mut out = Vector4::default();
        Vector4::lerp_out(a, b, 0.5, &mut out);
        // Quirk: X gets the W interpolation, W stays untouched.
        assert_eq!(out, Vector4::new(20.0, 10.0, 15.0, 0.0));
    }

    #[test]
    fn normalize_and_dot() {
        let mut v = Vector4::new(0.0, 2.0, 0.0, 0.0);
        v.normalize();
        assert_eq!(v, Vector4::new(0.0, 1.0, 0.0, 0.0));
        assert_eq!(
            Vector4::normalize_free(Vector4::new(0.0, 0.0, 4.0, 0.0)),
            Vector4::new(0.0, 0.0, 1.0, 0.0)
        );
        assert_eq!(
            Vector4::dot(
                Vector4::new(1.0, 0.0, 0.0, 0.0),
                Vector4::new(0.0, 1.0, 0.0, 0.0)
            ),
            0.0
        );
        let mut z = Vector4::new(0.0, 0.0, 0.0, 0.0);
        z.normalize();
        assert_eq!(z, Vector4::new(0.0, 0.0, 0.0, 0.0));
    }

    #[test]
    fn validity_and_indexing() {
        assert!(Vector4::new(1.0, 2.0, 3.0, 4.0).is_valid());
        assert!(!Vector4::new(0.0, 0.0, 0.0, f32::NAN).is_valid());
        let mut v = Vector4::new(1.0, 2.0, 3.0, 4.0);
        assert_eq!(v[3], 4.0);
        v[0] = 9.0;
        assert_eq!(v.x, 9.0);
    }
}
