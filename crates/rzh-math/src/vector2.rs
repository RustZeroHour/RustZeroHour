// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Ported from Core/Libraries/Source/WWVegas/WWMath/vector2.h @ 571a7c5, Zero
// Hour configuration. The original carries the following notices, preserved
// here:
//
//   VECTOR2.h — Westwood 3D. Programmer: Greg Hjelstrom. 02/24/1997.
//
//   (c) 2001-2003 Electronic Arts Inc.
//
//! 2D float vector (`Vector2`).
//!
//! Ported from `vector2.h`. The arithmetic operators, `Normalize`,
//! `Quick_Distance` and the `(s, c)` rotation are transcribed verbatim,
//! including the quirks documented on each function — they are behaviour
//! the simulation may depend on. The `Rotate(theta)` / `Rotate_Towards_
//! Vector(target, max_theta, …)` angle-taking variants are deferred with
//! the trigonometric entry points (ADR-001); use the `(s, c)` forms with
//! precomputed sine/cosine.
//!
//! Notable differences from the C++ (all documented in place):
//!
//! - the C++ dot product is spelled `a * b`; here it is
//!   [`Vector2::dot`] — overloading `Mul` to return a scalar reads as a
//!   bug in Rust,
//! - the default constructor leaves C++ fields uninitialized;
//!   [`Vector2::default`] is `(0, 0)`.

use core::ops::{Add, AddAssign, Div, Index, IndexMut, Mul, MulAssign, Neg, Sub, SubAssign};

use crate::scalar;

/// A 2D float vector (`Vector2`).
#[derive(Debug, Default, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct Vector2 {
    /// X component (`X`).
    pub x: f32,
    /// Y component (`Y`).
    pub y: f32,
}

impl Vector2 {
    /// Constructs a vector from its components.
    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    /// Sets both components (`Set`).
    pub fn set(&mut self, x: f32, y: f32) {
        self.x = x;
        self.y = y;
    }

    /// The squared length (`Length2`): `x*x + y*y`.
    #[must_use]
    #[inline]
    pub fn length2(self) -> f32 {
        self.x * self.x + self.y * self.y
    }

    /// The length (`Length`): `sqrt(Length2())`.
    #[must_use]
    #[inline]
    pub fn length(self) -> f32 {
        scalar::sqrt(self.length2())
    }

    /// Normalizes in place (`Normalize`): scales by `1/sqrt(Length2())`
    /// unless the squared length is zero.
    pub fn normalize(&mut self) {
        let len2 = self.length2();
        if len2 != 0.0 {
            let oolen = scalar::inv_sqrt(len2);
            self.x *= oolen;
            self.y *= oolen;
        }
    }

    /// The free `Normalize(vec)` of `vector2.h`, quirk included.
    ///
    /// The original **divides** the vector by the *inverse* square root
    /// (`vec / oolen`), which is not bit-equivalent to multiplying by
    /// `sqrt(len2)`; this port keeps the division. Zero-length input
    /// returns `(0, 0)`.
    #[must_use]
    pub fn normalize_free(vec: Self) -> Self {
        let len2 = vec.length2();
        if len2 != 0.0 {
            let oolen = scalar::inv_sqrt(len2);
            return vec / oolen;
        }
        Self::new(0.0, 0.0)
    }

    /// `Dot_Product` (the C++ `a * b`).
    #[must_use]
    #[inline]
    pub fn dot(a: Self, b: Self) -> f32 {
        a.x * b.x + a.y * b.y
    }

    /// `Perp_Dot_Product`: the dot with `b` rotated 90°, `a.X*-b.Y + a.Y*b.X`.
    #[must_use]
    #[inline]
    pub fn perp_dot(a: Self, b: Self) -> f32 {
        a.x * -b.y + a.y * b.x
    }

    /// Sets each component to the minimum of itself and `a` (`Update_Min`).
    pub fn update_min(&mut self, a: Self) {
        if a.x < self.x {
            self.x = a.x;
        }
        if a.y < self.y {
            self.y = a.y;
        }
    }

    /// Sets each component to the maximum of itself and `a` (`Update_Max`).
    pub fn update_max(&mut self, a: Self) {
        if a.x > self.x {
            self.x = a.x;
        }
        if a.y > self.y {
            self.y = a.y;
        }
    }

    /// Scales by independent factors (`Scale(float, float)`).
    pub fn scale(&mut self, a: f32, b: f32) {
        self.x *= a;
        self.y *= b;
    }

    /// Scales component-wise (`Scale(const Vector2&)`).
    pub fn scale_v(&mut self, a: Self) {
        self.x *= a.x;
        self.y *= a.y;
    }

    /// Rotates by a precomputed sine and cosine (`Rotate(float s, float c)`).
    pub fn rotate(&mut self, s: f32, c: f32) {
        let new_x = self.x * c + self.y * -s;
        let new_y = self.x * s + self.y * c;
        self.x = new_x;
        self.y = new_y;
    }

    /// `Rotate_Towards_Vector`, precomputed-sine variant.
    ///
    /// Rotates `self` towards `target` by at most the angle whose sine is
    /// `max_s` and cosine is `max_c`. Returns `true` when the target was
    /// reached (in which case `self` is set to it); `positive_turn` reports
    /// the turn direction. Both vectors are assumed normalized, as in the
    /// original.
    pub fn rotate_towards_vector(
        &mut self,
        target: &Self,
        max_s: f32,
        max_c: f32,
        positive_turn: &mut bool,
    ) -> bool {
        *positive_turn = Self::perp_dot(*target, *self) > 0.0;

        if Self::dot(*self, *target) >= max_c {
            self.set(target.x, target.y);
            true
        } else if *positive_turn {
            self.rotate(max_s, max_c);
            false
        } else {
            self.rotate(-max_s, max_c);
            false
        }
    }

    /// `Distance`: the length of `a - b`.
    #[must_use]
    #[inline]
    pub fn distance(a: Self, b: Self) -> f32 {
        (a - b).length()
    }

    /// `Quick_Distance`: the Graphics-Gems-I approximation.
    ///
    /// Ported from `Quick_Distance(x1, y1, x2, y2)` with its quirk: the
    /// original computes `Fabsf_Legacy` of the differences and **discards
    /// the results**, so negative differences stay negative.
    #[must_use]
    #[inline]
    pub fn quick_distance(a: Self, b: Self) -> f32 {
        let x_diff = a.x - b.x;
        let y_diff = a.y - b.y;

        // The original's discarded absolute-value calls (kept as a comment
        // so the omission is not "fixed" by accident):
        // scalar::fabs(x_diff); scalar::fabs(y_diff);

        if x_diff > y_diff {
            (y_diff / 2.0) + x_diff
        } else {
            (x_diff / 2.0) + y_diff
        }
    }

    /// `Lerp(a, b, t, result)`; the port returns the value instead of
    /// writing through an out-parameter.
    #[must_use]
    #[inline]
    pub fn lerp(a: Self, b: Self, t: f32) -> Self {
        Self::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
    }

    /// `Is_Valid`: no component is NaN or infinite.
    #[must_use]
    pub fn is_valid(self) -> bool {
        scalar::is_valid_float(self.x) && scalar::is_valid_float(self.y)
    }
}

/// `Equal_Within_Epsilon`: both components within `epsilon` of each other.
#[must_use]
pub fn equal_within_epsilon(a: Vector2, b: Vector2, epsilon: f32) -> bool {
    scalar::fabs(a.x - b.x) < epsilon && scalar::fabs(a.y - b.y) < epsilon
}

impl Neg for Vector2 {
    type Output = Self;

    #[inline]
    fn neg(self) -> Self {
        Self::new(-self.x, -self.y)
    }
}

impl Add for Vector2 {
    type Output = Self;

    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub for Vector2 {
    type Output = Self;

    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl AddAssign for Vector2 {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        self.x += rhs.x;
        self.y += rhs.y;
    }
}

impl SubAssign for Vector2 {
    #[inline]
    fn sub_assign(&mut self, rhs: Self) {
        self.x -= rhs.x;
        self.y -= rhs.y;
    }
}

impl Mul<f32> for Vector2 {
    type Output = Self;

    /// `operator * (Vector2, float)`; also covers `float * Vector2`
    /// (the original bodies are identical).
    #[inline]
    fn mul(self, k: f32) -> Self {
        Self::new(self.x * k, self.y * k)
    }
}

impl Mul<Vector2> for f32 {
    type Output = Vector2;

    #[inline]
    fn mul(self, rhs: Vector2) -> Vector2 {
        rhs * self
    }
}

impl MulAssign<f32> for Vector2 {
    /// `operator *= (float)` (the original's `(float)` casts are no-ops).
    #[inline]
    fn mul_assign(&mut self, k: f32) {
        self.x *= k;
        self.y *= k;
    }
}

impl Div<f32> for Vector2 {
    type Output = Self;

    /// `operator / (Vector2, float)`: multiplies by the reciprocal
    /// `1/k`, like the original.
    #[inline]
    fn div(self, k: f32) -> Self {
        let ook = 1.0 / k;
        Self::new(self.x * ook, self.y * ook)
    }
}

/// `operator /= (float)`: reciprocal multiply, like [`Div`].
impl core::ops::DivAssign<f32> for Vector2 {
    #[inline]
    fn div_assign(&mut self, k: f32) {
        let ook = 1.0 / k;
        self.x *= ook;
        self.y *= ook;
    }
}

impl Index<usize> for Vector2 {
    type Output = f32;

    /// `operator []`: panics outside `0..2` (the original indexes off `&X`,
    /// which is undefined for other indices).
    #[inline]
    fn index(&self, i: usize) -> &f32 {
        match i {
            0 => &self.x,
            1 => &self.y,
            _ => panic!("Vector2 index out of range: {i}"),
        }
    }
}

impl IndexMut<usize> for Vector2 {
    #[inline]
    fn index_mut(&mut self, i: usize) -> &mut f32 {
        match i {
            0 => &mut self.x,
            1 => &mut self.y,
            _ => panic!("Vector2 index out of range: {i}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Vector2, equal_within_epsilon};
    use crate::scalar;

    #[test]
    fn arithmetic_matches_componentwise_semantics() {
        let a = Vector2::new(1.0, 2.0);
        let b = Vector2::new(3.0, 5.0);
        assert_eq!(a + b, Vector2::new(4.0, 7.0));
        assert_eq!(a - b, Vector2::new(-2.0, -3.0));
        assert_eq!(a * 2.0, Vector2::new(2.0, 4.0));
        assert_eq!(2.0 * a, Vector2::new(2.0, 4.0));
        assert_eq!(a / 2.0, Vector2::new(0.5, 1.0));
        assert_eq!(-a, Vector2::new(-1.0, -2.0));
    }

    #[test]
    fn division_multiplies_by_the_reciprocal() {
        // 1/3.0 is inexact in binary, and for this numerator the
        // reciprocal shortcut rounds differently from a true division —
        // pinning the original's semantics.
        let a = Vector2::new(1.0e7, -1.0e7);
        let by_reciprocal = a / 3.0;
        let ook = 1.0 / 3.0;
        assert_eq!(by_reciprocal, Vector2::new(1.0e7 * ook, -1.0e7 * ook));
        assert_eq!(a.x / 3.0, 1.0e7 / 3.0);
        assert_ne!(by_reciprocal.x, a.x / 3.0);
    }

    #[test]
    fn dot_and_perp_dot() {
        let a = Vector2::new(2.0, 3.0);
        let b = Vector2::new(4.0, 5.0);
        assert_eq!(Vector2::dot(a, b), 2.0 * 4.0 + 3.0 * 5.0);
        // Perp dot of a with b = a.X*-b.Y + a.Y*b.X.
        assert_eq!(Vector2::perp_dot(a, b), 2.0 * -5.0 + 3.0 * 4.0);
        // The perp dot of a with b = a.X*-b.Y + a.Y*b.X: for the basis
        // vectors this is NEGATIVE one (the original's sign convention).
        let unit_x = Vector2::new(1.0, 0.0);
        let unit_y = Vector2::new(0.0, 1.0);
        assert_eq!(Vector2::perp_dot(unit_x, unit_x), 0.0);
        assert_eq!(Vector2::perp_dot(unit_x, unit_y), -1.0);
        assert_eq!(Vector2::perp_dot(unit_y, unit_x), 1.0);
    }

    #[test]
    fn normalize_scales_by_inverse_sqrt() {
        let mut v = Vector2::new(3.0, 4.0);
        v.normalize();
        assert_eq!(v, Vector2::new(0.6, 0.8));

        // Zero-length stays zero instead of dividing by zero.
        let mut z = Vector2::new(0.0, 0.0);
        z.normalize();
        assert_eq!(z, Vector2::new(0.0, 0.0));
    }

    #[test]
    fn normalize_free_divides_by_the_reciprocal() {
        // The quirk: normalize_free(v) = v / (1/sqrt(len2)), which scales
        // the vector UP by ~sqrt(len2) instead of normalizing it. For
        // (1, 1) that is a factor of sqrt(2), i.e. a length of 2 — the
        // original free function is simply wrong, and we keep its
        // behaviour.
        let v = Vector2::new(1.0, 1.0);
        let mut inplace = v;
        inplace.normalize();
        let free = Vector2::normalize_free(v);
        assert_ne!(free, inplace);
        assert!((free.x - scalar::SQRT2).abs() < 1e-6);
        assert!((free.y - scalar::SQRT2).abs() < 1e-6);
        assert!((free.length() - 2.0).abs() < 1e-6);
        // Zero input stays zero.
        assert_eq!(
            Vector2::normalize_free(Vector2::new(0.0, 0.0)),
            Vector2::new(0.0, 0.0)
        );
    }

    #[test]
    fn quick_distance_keeps_negative_differences() {
        // The discarded fabs quirk: with both differences negative the
        // "approximation" returns a negative value.
        let a = Vector2::new(0.0, 0.0);
        let b = Vector2::new(4.0, 3.0);
        let d = Vector2::quick_distance(a, b);
        // x_diff = -4, y_diff = -3; -4 > -3 is false, so the original
        // returns (x_diff / 2) + y_diff = -2 + -3.
        assert_eq!(d, -5.0);

        // With non-negative differences the formula is a normal
        // approximation: x_diff = 4, y_diff = 3 → (y_diff/2) + x_diff.
        assert_eq!(Vector2::quick_distance(b, a), (3.0 / 2.0) + 4.0);
    }

    #[test]
    fn rotate_uses_the_original_sign_convention() {
        // rotate(s=1, c=0) is a quarter turn: (x, y) -> (-y, x).
        let mut v = Vector2::new(1.0, 0.0);
        v.rotate(1.0, 0.0);
        assert_eq!(v, Vector2::new(0.0, 1.0));
    }

    #[test]
    fn rotate_towards_vector_reaches_and_turns() {
        let mut v = Vector2::new(1.0, 0.0);
        let target = Vector2::new(0.0, 1.0);
        let mut positive = false;
        // Turn by 45 degrees at a time.
        let (s, c) = (scalar::SQRT2 / 2.0, scalar::SQRT2 / 2.0);
        let mut reached = false;
        for _ in 0..4 {
            reached = v.rotate_towards_vector(&target, s, c, &mut positive);
            if reached {
                break;
            }
        }
        assert!(reached);
        assert_eq!(v, target);
        assert!(positive); // quarter turn counter-clockwise
    }

    #[test]
    fn lerp_and_distance() {
        let a = Vector2::new(0.0, 0.0);
        let b = Vector2::new(10.0, 0.0);
        assert_eq!(Vector2::lerp(a, b, 0.25), Vector2::new(2.5, 0.0));
        assert_eq!(Vector2::distance(a, b), 10.0);
        assert_eq!(Vector2::distance(b, a), 10.0);
    }

    #[test]
    fn update_min_max_and_scaling() {
        let mut v = Vector2::new(5.0, -1.0);
        v.update_min(Vector2::new(2.0, 3.0));
        assert_eq!(v, Vector2::new(2.0, -1.0));
        v.update_max(Vector2::new(2.0, 3.0));
        assert_eq!(v, Vector2::new(2.0, 3.0));

        let mut w = Vector2::new(2.0, 3.0);
        w.scale(10.0, 100.0);
        assert_eq!(w, Vector2::new(20.0, 300.0));
        w.scale_v(Vector2::new(0.5, 2.0));
        assert_eq!(w, Vector2::new(10.0, 600.0));
    }

    #[test]
    fn equality_within_epsilon() {
        let a = Vector2::new(1.0, 1.0);
        let b = Vector2::new(1.0 + 1e-5, 1.0 - 1e-5);
        assert!(equal_within_epsilon(a, b, 0.0001));
        assert!(!equal_within_epsilon(a, b, 1e-6));
    }

    #[test]
    fn indexing_and_validity() {
        let mut v = Vector2::new(1.0, 2.0);
        assert_eq!(v[0], 1.0);
        assert_eq!(v[1], 2.0);
        v[1] = 5.0;
        assert_eq!(v.y, 5.0);
        assert!(v.is_valid());
        assert!(!Vector2::new(f32::NAN, 0.0).is_valid());
    }
}
