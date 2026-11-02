// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Ported from Core/Libraries/Source/WWVegas/WWMath/vector3.h @ 571a7c5, Zero
// Hour configuration. The original carries the following notices, preserved
// here:
//
//   VECTOR3.h — Westwood 3D. Programmer: Greg Hjelstrom. 02/24/1997.
//
//   (c) 2001-2003 Electronic Arts Inc.
//
//! 3D float vector (`Vector3`).
//!
//! Ported from `vector3.h`. Bodies guarded by `ALLOW_TEMPORARIES` in the
//! original (the free `Normalize`, the value-returning `Lerp` and
//! `Cross_Product`) are **not** part of the game build and are not ported.
//! The `Rotate_X/Y/Z(angle)` variants are deferred with the trigonometric
//! entry points (ADR-001); the `(s_angle, c_angle)` forms are ported.
//!
//! The dot product is [`Vector3::dot`] (C++ `a * b`), the cross product is
//! [`Vector3::cross`] (C++ writes it through an out-parameter).

use core::ops::{Add, AddAssign, Div, Index, IndexMut, Mul, MulAssign, Neg, Sub, SubAssign};

use crate::scalar;

/// A 3D float vector (`Vector3`).
#[derive(Debug, Default, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct Vector3 {
    /// X component (`X`).
    pub x: f32,
    /// Y component (`Y`).
    pub y: f32,
    /// Z component (`Z`).
    pub z: f32,
}

impl Vector3 {
    /// Constructs a vector from its components.
    #[must_use]
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    /// `operator[]` (read): 0 = `x`, 1 = `y`, 2 = `z`.
    #[must_use]
    pub fn by_index(self, i: usize) -> f32 {
        match i {
            0 => self.x,
            1 => self.y,
            _ => self.z,
        }
    }

    /// `operator[]` (write): 0 = `x`, 1 = `y`, 2 = `z`.
    pub fn set_index(&mut self, i: usize, value: f32) {
        match i {
            0 => self.x = value,
            1 => self.y = value,
            _ => self.z = value,
        }
    }

    /// `operator[]` (mutable): 0 = `x`, 1 = `y`, 2 = `z`.
    pub fn by_index_mut(&mut self, i: usize) -> &mut f32 {
        match i {
            0 => &mut self.x,
            1 => &mut self.y,
            _ => &mut self.z,
        }
    }

    /// Sets all components (`Set`).
    pub fn set(&mut self, x: f32, y: f32, z: f32) {
        self.x = x;
        self.y = y;
        self.z = z;
    }

    /// The squared length (`Length2`): `x*x + y*y + z*z`.
    #[must_use]
    #[inline]
    pub fn length2(self) -> f32 {
        self.x * self.x + self.y * self.y + self.z * self.z
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
        }
    }

    /// `Quick_Length`: the Graphics-Gems-I ±8% approximation.
    ///
    /// Quirk preserved: the original's third sort line assigns `min = mid`
    /// instead of `min = tmp`, so the old `mid` is lost whenever the third
    /// comparison fires. The result is the original's, not the textbook's.
    #[must_use]
    #[allow(clippy::manual_swap, clippy::almost_swapped)] // the almost-swap below is the point
    pub fn quick_length(self) -> f32 {
        let mut max = scalar::fabs(self.x);
        let mut mid = scalar::fabs(self.y);
        let mut min = scalar::fabs(self.z);
        let mut tmp;

        if max < mid {
            tmp = max;
            max = mid;
            mid = tmp;
        }
        if max < min {
            tmp = max;
            max = min;
            min = tmp;
        }
        // Original quirk: `min = mid` (not `min = tmp`), so both end up
        // holding the old min and the old mid is lost. Kept verbatim
        // (including the dead store to `tmp`).
        if mid < min {
            tmp = mid;
            mid = min;
            min = mid;
            let _ = tmp;
        }

        max + (11.0 / 32.0) * mid + (1.0 / 4.0) * min
    }

    /// `Dot_Product` (the C++ `a * b`).
    #[must_use]
    #[inline]
    pub fn dot(a: Self, b: Self) -> f32 {
        a.x * b.x + a.y * b.y + a.z * b.z
    }

    /// `Cross_Product(a, b, result)`; the port returns the value.
    #[must_use]
    #[inline]
    pub fn cross(a: Self, b: Self) -> Self {
        Self::new(
            a.y * b.z - a.z * b.y,
            a.z * b.x - a.x * b.z,
            a.x * b.y - a.y * b.x,
        )
    }

    /// `Cross_Product_X`: the X component of the cross product.
    #[must_use]
    #[inline]
    pub fn cross_x(a: Self, b: Self) -> f32 {
        a.y * b.z - a.z * b.y
    }

    /// `Cross_Product_Y`: the Y component of the cross product.
    #[must_use]
    #[inline]
    pub fn cross_y(a: Self, b: Self) -> f32 {
        a.z * b.x - a.x * b.z
    }

    /// `Cross_Product_Z`: the Z component of the cross product.
    #[must_use]
    #[inline]
    pub fn cross_z(a: Self, b: Self) -> f32 {
        a.x * b.y - a.y * b.x
    }

    /// `Normalized_Cross_Product(a, b, result)`: the cross product,
    /// normalized in place (zero-length results stay zero).
    #[must_use]
    pub fn normalized_cross(a: Self, b: Self) -> Self {
        let mut result = Self::cross(a, b);
        result.normalize();
        result
    }

    /// `Lerp(a, b, alpha, result)`; the port returns the value.
    #[must_use]
    #[inline]
    pub fn lerp(a: Self, b: Self, alpha: f32) -> Self {
        Self::new(
            a.x + (b.x - a.x) * alpha,
            a.y + (b.y - a.y) * alpha,
            a.z + (b.z - a.z) * alpha,
        )
    }

    /// Sets each component to the minimum of itself and `a` (`Update_Min`).
    pub fn update_min(&mut self, a: Self) {
        if a.x < self.x {
            self.x = a.x;
        }
        if a.y < self.y {
            self.y = a.y;
        }
        if a.z < self.z {
            self.z = a.z;
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
        if a.z > self.z {
            self.z = a.z;
        }
    }

    /// `Cap_Absolute_To`: caps each component at `±a`'s component,
    /// respecting the original's sign logic (`-a` for negative components).
    pub fn cap_absolute_to(&mut self, a: Self) {
        if self.x > 0.0 {
            if a.x < self.x {
                self.x = a.x;
            }
        } else if -a.x > self.x {
            self.x = -a.x;
        }

        if self.y > 0.0 {
            if a.y < self.y {
                self.y = a.y;
            }
        } else if -a.y > self.y {
            self.y = -a.y;
        }

        if self.z > 0.0 {
            if a.z < self.z {
                self.z = a.z;
            }
        } else if -a.z > self.z {
            self.z = -a.z;
        }
    }

    /// Scales component-wise (`Scale(const Vector3&)`).
    pub fn scale_v(&mut self, scale: Self) {
        self.x *= scale.x;
        self.y *= scale.y;
        self.z *= scale.z;
    }

    /// `Rotate_X(s, c)`: rotates around the X axis by a precomputed sine
    /// and cosine.
    pub fn rotate_x(&mut self, s_angle: f32, c_angle: f32) {
        let tmp_y = self.y;
        let tmp_z = self.z;
        self.y = c_angle * tmp_y - s_angle * tmp_z;
        self.z = s_angle * tmp_y + c_angle * tmp_z;
    }

    /// `Rotate_Y(s, c)`: rotates around the Y axis by a precomputed sine
    /// and cosine.
    pub fn rotate_y(&mut self, s_angle: f32, c_angle: f32) {
        let tmp_x = self.x;
        let tmp_z = self.z;
        self.x = c_angle * tmp_x + s_angle * tmp_z;
        self.z = -s_angle * tmp_x + c_angle * tmp_z;
    }

    /// `Rotate_Z(s, c)`: rotates around the Z axis by a precomputed sine
    /// and cosine.
    pub fn rotate_z(&mut self, s_angle: f32, c_angle: f32) {
        let tmp_x = self.x;
        let tmp_y = self.y;
        self.x = c_angle * tmp_x - s_angle * tmp_y;
        self.y = s_angle * tmp_x + c_angle * tmp_y;
    }

    /// `Find_X_At_Y(y, p1, p2)`: the X where the p1→p2 line crosses the
    /// given Y.
    #[must_use]
    #[inline]
    pub fn find_x_at_y(y: f32, p1: Self, p2: Self) -> f32 {
        p1.x + ((y - p1.y) * ((p2.x - p1.x) / (p2.y - p1.y)))
    }

    /// `Find_Y_At_X(x, p1, p2)`.
    #[must_use]
    #[inline]
    pub fn find_y_at_x(x: f32, p1: Self, p2: Self) -> f32 {
        p1.y + ((x - p1.x) * ((p2.y - p1.y) / (p2.x - p1.x)))
    }

    /// `Find_Z_At_X(x, p1, p2)`.
    #[must_use]
    #[inline]
    pub fn find_z_at_x(x: f32, p1: Self, p2: Self) -> f32 {
        p1.z + ((x - p1.x) * ((p2.z - p1.z) / (p2.x - p1.x)))
    }

    /// `Distance`: the length of `p1 - p2`.
    #[must_use]
    #[inline]
    pub fn distance(p1: Self, p2: Self) -> f32 {
        (p1 - p2).length()
    }

    /// `Quick_Distance`: [`quick_length`](Self::quick_length) of the
    /// difference.
    #[must_use]
    #[inline]
    pub fn quick_distance(p1: Self, p2: Self) -> f32 {
        (p1 - p2).quick_length()
    }

    /// `Convert_To_ABGR`: packs a `[0, 1]` colour as `0xFF<<24 | B<<16 |
    /// G<<8 | R`.
    #[must_use]
    pub fn convert_to_abgr(self) -> u32 {
        (255u32 << 24)
            | ((scalar::unit_float_to_byte(self.z) as u32) << 16)
            | ((scalar::unit_float_to_byte(self.y) as u32) << 8)
            | scalar::unit_float_to_byte(self.x) as u32
    }

    /// `Convert_To_ARGB`: packs a `[0, 1]` colour as `0xFF<<24 | R<<16 |
    /// G<<8 | B`.
    #[must_use]
    pub fn convert_to_argb(self) -> u32 {
        (255u32 << 24)
            | ((scalar::unit_float_to_byte(self.x) as u32) << 16)
            | ((scalar::unit_float_to_byte(self.y) as u32) << 8)
            | scalar::unit_float_to_byte(self.z) as u32
    }

    /// `Is_Valid`: no component is NaN or infinite.
    #[must_use]
    pub fn is_valid(self) -> bool {
        scalar::is_valid_float(self.x)
            && scalar::is_valid_float(self.y)
            && scalar::is_valid_float(self.z)
    }
}

/// `Equal_Within_Epsilon`: all components within `epsilon` of each other.
#[must_use]
pub fn equal_within_epsilon(a: Vector3, b: Vector3, epsilon: f32) -> bool {
    scalar::fabs(a.x - b.x) < epsilon
        && scalar::fabs(a.y - b.y) < epsilon
        && scalar::fabs(a.z - b.z) < epsilon
}

impl Neg for Vector3 {
    type Output = Self;

    #[inline]
    fn neg(self) -> Self {
        Self::new(-self.x, -self.y, -self.z)
    }
}

impl Add for Vector3 {
    type Output = Self;

    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}

impl Sub for Vector3 {
    type Output = Self;

    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}

impl AddAssign for Vector3 {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        self.x += rhs.x;
        self.y += rhs.y;
        self.z += rhs.z;
    }
}

impl SubAssign for Vector3 {
    #[inline]
    fn sub_assign(&mut self, rhs: Self) {
        self.x -= rhs.x;
        self.y -= rhs.y;
        self.z -= rhs.z;
    }
}

impl Mul<f32> for Vector3 {
    type Output = Self;

    /// `operator * (Vector3, float)`; identical to `float * Vector3`.
    #[inline]
    fn mul(self, k: f32) -> Self {
        Self::new(self.x * k, self.y * k, self.z * k)
    }
}

impl Mul<Vector3> for f32 {
    type Output = Vector3;

    #[inline]
    fn mul(self, rhs: Vector3) -> Vector3 {
        rhs * self
    }
}

impl MulAssign<f32> for Vector3 {
    #[inline]
    fn mul_assign(&mut self, k: f32) {
        self.x *= k;
        self.y *= k;
        self.z *= k;
    }
}

impl Div<f32> for Vector3 {
    type Output = Self;

    /// `operator / (Vector3, float)`: multiplies by the reciprocal `1/k`.
    #[inline]
    fn div(self, k: f32) -> Self {
        let ook = 1.0 / k;
        Self::new(self.x * ook, self.y * ook, self.z * ook)
    }
}

/// `operator /= (float)`: reciprocal multiply, like [`Div`].
impl core::ops::DivAssign<f32> for Vector3 {
    #[inline]
    fn div_assign(&mut self, k: f32) {
        let ook = 1.0 / k;
        self.x *= ook;
        self.y *= ook;
        self.z *= ook;
    }
}

impl Index<usize> for Vector3 {
    type Output = f32;

    #[inline]
    fn index(&self, i: usize) -> &f32 {
        match i {
            0 => &self.x,
            1 => &self.y,
            2 => &self.z,
            _ => panic!("Vector3 index out of range: {i}"),
        }
    }
}

impl IndexMut<usize> for Vector3 {
    #[inline]
    fn index_mut(&mut self, i: usize) -> &mut f32 {
        match i {
            0 => &mut self.x,
            1 => &mut self.y,
            2 => &mut self.z,
            _ => panic!("Vector3 index out of range: {i}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Vector3, equal_within_epsilon};

    #[test]
    fn arithmetic_is_componentwise() {
        let a = Vector3::new(1.0, 2.0, 3.0);
        let b = Vector3::new(4.0, 5.0, 6.0);
        assert_eq!(a + b, Vector3::new(5.0, 7.0, 9.0));
        assert_eq!(a - b, Vector3::new(-3.0, -3.0, -3.0));
        assert_eq!(a * 2.0, Vector3::new(2.0, 4.0, 6.0));
        assert_eq!(2.0 * a, Vector3::new(2.0, 4.0, 6.0));
        assert_eq!(a / 2.0, Vector3::new(0.5, 1.0, 1.5));
        assert_eq!(-a, Vector3::new(-1.0, -2.0, -3.0));
    }

    #[test]
    fn dot_and_cross_agree_with_algebra() {
        let a = Vector3::new(1.0, 0.0, 0.0);
        let b = Vector3::new(0.0, 1.0, 0.0);
        assert_eq!(Vector3::dot(a, b), 0.0);
        assert_eq!(Vector3::cross(a, b), Vector3::new(0.0, 0.0, 1.0));
        assert_eq!(Vector3::cross_x(a, b), 0.0);
        assert_eq!(Vector3::cross_y(a, b), 0.0);
        assert_eq!(Vector3::cross_z(a, b), 1.0);
        assert_eq!(Vector3::cross(a, a), Vector3::new(0.0, 0.0, 0.0));

        let c = Vector3::new(2.0, 3.0, 4.0);
        let d = Vector3::new(5.0, 6.0, 7.0);
        assert_eq!(Vector3::dot(c, d), 2.0 * 5.0 + 3.0 * 6.0 + 4.0 * 7.0);
        // cross(c, d) is orthogonal to both.
        let cd = Vector3::cross(c, d);
        assert_eq!(Vector3::dot(cd, c), 0.0);
        assert_eq!(Vector3::dot(cd, d), 0.0);

        let nc = Vector3::normalized_cross(a, b);
        assert_eq!(nc, Vector3::new(0.0, 0.0, 1.0));
    }

    #[test]
    fn normalize_handles_zero_and_unit() {
        let mut v = Vector3::new(0.0, 3.0, 4.0);
        v.normalize();
        assert_eq!(v, Vector3::new(0.0, 0.6, 0.8));
        let mut z = Vector3::new(0.0, 0.0, 0.0);
        z.normalize();
        assert_eq!(z, Vector3::new(0.0, 0.0, 0.0));
    }

    #[test]
    fn quick_length_matches_the_buggy_sort() {
        // (1, 2, 3): after the first two sorts max=3, mid=2, min=1; the
        // third comparison does not fire, so the quirk is invisible.
        assert_eq!(
            Vector3::new(3.0, 2.0, 1.0).quick_length(),
            3.0 + (11.0 / 32.0) * 2.0 + 0.25
        );
        // (-3, -2, -1): fabs sorts to (3, 2, 1) as above.
        assert_eq!(
            Vector3::new(-3.0, -2.0, -1.0).quick_length(),
            3.0 + (11.0 / 32.0) * 2.0 + 0.25
        );
    }

    #[test]
    fn rotations_use_the_original_conventions() {
        let mut v = Vector3::new(0.0, 1.0, 0.0);
        v.rotate_x(1.0, 0.0); // quarter turn around X: (y, z) -> (-z, y)
        assert_eq!(v, Vector3::new(0.0, 0.0, 1.0));

        let mut w = Vector3::new(1.0, 0.0, 0.0);
        w.rotate_y(1.0, 0.0); // quarter turn around Y: (x, z) -> (z, -x)
        assert_eq!(w, Vector3::new(0.0, 0.0, -1.0));

        let mut u = Vector3::new(1.0, 0.0, 0.0);
        u.rotate_z(1.0, 0.0); // quarter turn around Z: (x, y) -> (-y, x)
        assert_eq!(u, Vector3::new(0.0, 1.0, 0.0));
    }

    #[test]
    fn cap_absolute_and_min_max() {
        let mut v = Vector3::new(5.0, -5.0, 2.0);
        v.cap_absolute_to(Vector3::new(3.0, 3.0, 3.0));
        assert_eq!(v, Vector3::new(3.0, -3.0, 2.0));

        let mut w = Vector3::new(1.0, 1.0, 1.0);
        w.update_min(Vector3::new(0.0, 2.0, 1.0));
        assert_eq!(w, Vector3::new(0.0, 1.0, 1.0));
        w.update_max(Vector3::new(-1.0, 9.0, 1.0));
        assert_eq!(w, Vector3::new(0.0, 9.0, 1.0));
    }

    #[test]
    fn find_axis_interpolants() {
        let p1 = Vector3::new(0.0, 0.0, 0.0);
        let p2 = Vector3::new(10.0, 20.0, 30.0);
        assert_eq!(Vector3::find_x_at_y(10.0, p1, p2), 5.0);
        assert_eq!(Vector3::find_y_at_x(5.0, p1, p2), 10.0);
        assert_eq!(Vector3::find_z_at_x(5.0, p1, p2), 15.0);
    }

    #[test]
    fn color_packing() {
        let white = Vector3::new(1.0, 1.0, 1.0);
        assert_eq!(white.convert_to_abgr(), 0xFFFF_FFFF);
        assert_eq!(white.convert_to_argb(), 0xFFFF_FFFF);
        let red = Vector3::new(1.0, 0.0, 0.0);
        assert_eq!(red.convert_to_argb(), 0xFFFF_0000);
        assert_eq!(red.convert_to_abgr(), 0xFF00_00FF);
        let black = Vector3::new(0.0, 0.0, 0.0);
        assert_eq!(black.convert_to_argb(), 0xFF00_0000);
    }

    #[test]
    fn equality_within_epsilon_and_validity() {
        let a = Vector3::new(1.0, 1.0, 1.0);
        let b = Vector3::new(1.0 + 1e-5, 1.0, 1.0 - 1e-5);
        assert!(equal_within_epsilon(a, b, 0.0001));
        assert!(!equal_within_epsilon(a, b, 1e-6));
        assert!(!Vector3::new(0.0, f32::INFINITY, 0.0).is_valid());
    }

    #[test]
    fn lerp_distance_quick_distance() {
        let a = Vector3::new(0.0, 0.0, 0.0);
        let b = Vector3::new(0.0, 3.0, 4.0);
        assert_eq!(Vector3::distance(a, b), 5.0);
        assert_eq!(Vector3::lerp(a, b, 0.5), Vector3::new(0.0, 1.5, 2.0));
        // Walk the quirky sort by hand: |diff| = (0, 3, 4) → after the
        // first two swaps (max, mid, min) = (4, 0, 3); the third fires
        // (0 < 3) and the `min = mid` quirk leaves (4, 3, 3).
        assert_eq!(
            Vector3::quick_distance(a, b),
            4.0 + (11.0 / 32.0) * 3.0 + 0.25 * 3.0
        );
    }
}
