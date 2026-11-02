// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Ported from the following files at baseline commit 571a7c5, Zero Hour
// configuration, deterministic branches (ADR-000). The originals carry the
// following notices, preserved here:
//
//   plane.h — PlaneClass: 3D-planes using the Normal+Distance description.
//   Programmer: Greg Hjelstrom, 03/17/97.
//
//   (c) 2001-2003 Electronic Arts Inc.  Confidential — Westwood Studios.
//
//! 3D planes in the normal-plus-distance form.
//!
//! The plane is the set of points `p` with `N·p = D`; beware that the
//! sign of `D` is **inverted** relative to the familiar
//! `Ax + By + Cz + D = 0` form (an original warning, preserved).
//!
//! The `In_Front(const SphereClass&)` and
//! `In_Front_Or_Intersecting(const SphereClass&)` members are **not
//! ported yet**: they arrive with the sphere type. The plane-vs-X
//! `CollisionMath::Overlap_Test` family is likewise deferred with the
//! rest of the collision math.
//!
//! # Quirks preserved
//!
//! - [`Plane::from_points`] returns the default plane `(N=(0,0,1), D=0)`
//!   for colinear input points, because "constructors can't fail" (the
//!   original comment).
//! - [`Plane::compute_intersection`] leaves the `t` out-parameter
//!   untouched when the segment is parallel to the plane.
//!
//! # Parity level
//!
//! **P0 exact**: the golden vectors in `tests/golden/geometry.txt` pin
//! every operation's bit patterns.

use crate::vector3::Vector3;

/// `PlaneClass`: a plane as a unit normal and a distance along it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plane {
    /// Normal of the plane.
    pub n: Vector3,
    /// Distance along the normal from the origin.
    pub d: f32,
}

/// `PlaneClass::IntersectionResType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntersectionRes {
    /// No intersection: the line is parallel to the plane.
    NoIntersection,
    /// The segment between A and B intersects the plane.
    InsideSegment,
    /// The infinite line outside A and B intersects the plane.
    OutsideLine,
}

impl Plane {
    /// The default plane: normal +Z through the origin.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            n: Vector3::new(0.0, 0.0, 1.0),
            d: 0.0,
        }
    }

    /// `PlaneClass(nx, ny, nz, dist)` / `Set(a, b, c, d)`: the four
    /// coefficients (note the sign of `d`!).
    #[must_use]
    pub const fn from_coeffs(nx: f32, ny: f32, nz: f32, dist: f32) -> Self {
        Self {
            n: Vector3::new(nx, ny, nz),
            d: dist,
        }
    }

    /// `PlaneClass(normal, dist)` / `Set(normal, dist)`.
    #[must_use]
    pub const fn from_normal_dist(normal: Vector3, dist: f32) -> Self {
        Self { n: normal, d: dist }
    }

    /// `PlaneClass(normal, point)` / `Set(normal, point)`: the plane
    /// with the given normal containing the point.
    #[must_use]
    pub fn from_normal_point(normal: Vector3, point: Vector3) -> Self {
        Self {
            n: normal,
            d: Vector3::dot(normal, point),
        }
    }

    /// `PlaneClass(p1, p2, p3)` / `Set(p1, p2, p3)`: the plane through
    /// three points. Colinear points yield the default plane
    /// ("constructors can't fail", original comment).
    #[must_use]
    pub fn from_points(point1: Vector3, point2: Vector3, point3: Vector3) -> Self {
        let mut n = Vector3::cross(point2 - point1, point3 - point1);
        if n != Vector3::new(0.0, 0.0, 0.0) {
            // Points are not colinear. Normalize N and calculate D.
            n.normalize();
            let d = Vector3::dot(n, point1);
            Self { n, d }
        } else {
            // They are colinear - return default plane (constructors
            // can't fail).
            Self::new()
        }
    }

    /// `Set(a, b, c, d)`.
    pub fn set(&mut self, a: f32, b: f32, c: f32, d: f32) {
        self.n.x = a;
        self.n.y = b;
        self.n.z = c;
        self.d = d;
    }

    /// `Set(normal, dist)`.
    pub fn set_normal_dist(&mut self, normal: Vector3, dist: f32) {
        self.n = normal;
        self.d = dist;
    }

    /// `Set(normal, point)`.
    pub fn set_normal_point(&mut self, normal: Vector3, point: Vector3) {
        self.n = normal;
        self.d = Vector3::dot(normal, point);
    }

    /// `Set(p1, p2, p3)` (see [`Self::from_points`]).
    pub fn set_points(&mut self, point1: Vector3, point2: Vector3, point3: Vector3) {
        *self = Self::from_points(point1, point2, point3);
    }

    /// `Compute_Intersection(p0, p1, set_t)`: the parametric hit of the
    /// p0→p1 segment with the plane. `set_t` is **not written** when the
    /// segment is parallel to the plane, exactly like the original.
    pub fn compute_intersection(
        &self,
        p0: Vector3,
        p1: Vector3,
        set_t: &mut f32,
    ) -> IntersectionRes {
        let den = Vector3::dot(self.n, p1 - p0);

        // If the denominator is zero, the ray is parallel to the plane
        if den == 0.0 {
            return IntersectionRes::NoIntersection;
        }

        let num = -(Vector3::dot(self.n, p0) - self.d);
        *set_t = num / den;

        // If t is not between 0 and 1, the line containing the segment
        // intersects the plane but the segment does not
        if (*set_t < 0.0) || (*set_t > 1.0) {
            return IntersectionRes::OutsideLine;
        }

        IntersectionRes::InsideSegment
    }

    /// `In_Front(point)`.
    #[must_use]
    pub fn in_front(&self, point: Vector3) -> bool {
        let dist = Vector3::dot(point, self.n);
        dist > self.d
    }

    /// `Intersect_Planes(a, b, line_dir, line_point)`: the intersection
    /// line of two planes (Graphics Gems III, "plane-to-plane
    /// intersection", pp. 233-235). The direction is returned normalized,
    /// exactly like the original.
    #[must_use]
    pub fn intersect_planes(a: &Self, b: &Self) -> (Vector3, Vector3) {
        // Find line of intersection. First find direction vector of line:
        let mut line_dir = Vector3::cross(a.n, b.n);

        // Now find point on line. How we do it depends on what the
        // largest coordinate of the direction vector is.
        let mut abs_dir = line_dir;
        abs_dir.update_max(-abs_dir);
        let line_point;
        if abs_dir.x > abs_dir.y {
            if abs_dir.x > abs_dir.z {
                // X largest
                let ool = 1.0 / line_dir.x;
                line_point = Vector3::new(
                    0.0,
                    (b.n.z * a.d - a.n.z * b.d) * ool,
                    (a.n.y * b.d - b.n.y * a.d) * ool,
                );
            } else {
                // Z largest
                let ool = 1.0 / line_dir.z;
                line_point = Vector3::new(
                    (b.n.y * a.d - a.n.y * b.d) * ool,
                    (a.n.x * b.d - b.n.x * a.d) * ool,
                    0.0,
                );
            }
        } else if abs_dir.y > abs_dir.z {
            // Y largest
            let ool = 1.0 / line_dir.y;
            line_point = Vector3::new(
                (a.n.z * b.d - b.n.z * a.d) * ool,
                0.0,
                (b.n.x * a.d - a.n.x * b.d) * ool,
            );
        } else {
            // Z largest
            let ool = 1.0 / line_dir.z;
            line_point = Vector3::new(
                (b.n.y * a.d - a.n.y * b.d) * ool,
                (a.n.x * b.d - b.n.x * a.d) * ool,
                0.0,
            );
        }

        // Normalize direction vector (we do it here because we needed
        // the non-normalized version to find the point).
        line_dir.normalize();
        (line_dir, line_point)
    }
}

impl Default for Plane {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructors_match_their_definitions() {
        let p = Plane::from_coeffs(0.0, 0.0, 1.0, 5.0);
        assert_eq!(p.n, Vector3::new(0.0, 0.0, 1.0));
        assert_eq!(p.d, 5.0);

        // The normal-point form computes D = N·p.
        let p = Plane::from_normal_point(Vector3::new(0.0, 1.0, 0.0), Vector3::new(3.0, 7.0, -2.0));
        assert_eq!(p.d, 7.0);

        // Three points spanning the z=1 plane.
        let p = Plane::from_points(
            Vector3::new(0.0, 0.0, 1.0),
            Vector3::new(1.0, 0.0, 1.0),
            Vector3::new(0.0, 1.0, 1.0),
        );
        assert_eq!(p.n, Vector3::new(0.0, 0.0, 1.0));
        assert_eq!(p.d, 1.0);

        // Colinear points yield the default plane.
        let p = Plane::from_points(
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 1.0, 1.0),
            Vector3::new(2.0, 2.0, 2.0),
        );
        assert_eq!(p, Plane::new());
    }

    #[test]
    fn segment_intersection_t_values() {
        let p = Plane::from_coeffs(0.0, 0.0, 1.0, 10.0);
        let mut t = f32::NAN;

        // Segment crossing the plane at t=0.25.
        assert_eq!(
            p.compute_intersection(
                Vector3::new(0.0, 0.0, 0.0),
                Vector3::new(0.0, 0.0, 40.0),
                &mut t
            ),
            IntersectionRes::InsideSegment
        );
        assert_eq!(t, 0.25);

        // Parallel segment: t untouched, no intersection.
        let sentinel = -13.0;
        t = sentinel;
        assert_eq!(
            p.compute_intersection(
                Vector3::new(0.0, 0.0, 1.0),
                Vector3::new(5.0, 0.0, 1.0),
                &mut t
            ),
            IntersectionRes::NoIntersection
        );
        assert_eq!(t, sentinel);

        // Hit behind the segment start (the segment runs away from the
        // plane): den = 20, num = -10, t = -0.5.
        assert_eq!(
            p.compute_intersection(
                Vector3::new(0.0, 0.0, 20.0),
                Vector3::new(0.0, 0.0, 40.0),
                &mut t
            ),
            IntersectionRes::OutsideLine
        );
        assert_eq!(t, -0.5);

        // Hit beyond the segment end: den = -5, num = -10, t = 2.
        assert_eq!(
            p.compute_intersection(
                Vector3::new(0.0, 0.0, 20.0),
                Vector3::new(0.0, 0.0, 15.0),
                &mut t
            ),
            IntersectionRes::OutsideLine
        );
        assert_eq!(t, 2.0);
    }

    #[test]
    fn in_front_tests_the_signed_distance() {
        let p = Plane::from_coeffs(0.0, 0.0, 1.0, 10.0);
        assert!(p.in_front(Vector3::new(0.0, 0.0, 10.5)));
        assert!(!p.in_front(Vector3::new(0.0, 0.0, 9.5)));
        assert!(!p.in_front(Vector3::new(0.0, 0.0, 10.0)));
    }

    #[test]
    fn plane_plane_intersection_lies_on_both_planes() {
        // The z=5 plane and the x=2 plane intersect along (2, y, 5).
        let a = Plane::from_coeffs(0.0, 0.0, 1.0, 5.0);
        let b = Plane::from_coeffs(1.0, 0.0, 0.0, 2.0);
        let (dir, point) = Plane::intersect_planes(&a, &b);
        assert_eq!(dir, Vector3::new(0.0, 1.0, 0.0));
        assert_eq!(point.x, 2.0);
        assert_eq!(point.z, 5.0);
        assert_eq!(point.y, 0.0);
    }
}
