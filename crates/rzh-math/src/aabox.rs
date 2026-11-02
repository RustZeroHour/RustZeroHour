// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Ported from the following files at baseline commit 571a7c5, Zero Hour
// configuration, deterministic branches (ADR-000). The originals carry the
// following notices, preserved here:
//
//   aabox.h / aabox.cpp — Axis-Aligned Boxes. Author: Greg Hjelstrom
//   (by Greg_h / gth in the file headers), 1997-2001.
//
//   (c) 2001-2003 Electronic Arts Inc.  Confidential — Westwood Studios.
//
//! Axis-aligned boxes: the center/extent form and the min/max form.
//!
//! Two flavours, exactly like the WWMath originals:
//!
//! - [`AABox`]: `AABoxClass` — a world-space center plus a half-size
//!   extent per axis;
//! - [`MinMaxAABox`]: `MinMaxAABoxClass` — the min and max corners,
//!   convenient to grow incrementally and convert to a center-extent box.
//!
//! The `Init(const LineSegClass&)` and `Init_Random` members of the
//! original are **not ported yet**: the first arrives with the line
//! segment type, the second is tool/test code driving the client RNG.
//!
//! # Quirks preserved
//!
//! - `MinMaxAABox::Add_Box` silently ignores boxes whose corners
//!   coincide (and `Add_Box(AABox)` ignores zero-extent boxes, and the
//!   raw-corner variant ignores equal corners) — early returns that date
//!   from "ok to add boxes with zero extent" (gth, 9/29/2000).
//! - `AABox::Volume` computes in **double** precision (the original's
//!   `2.0` literals are doubles) and truncates once at the return.
//! - `MinMaxAABox::Init(points)` re-visits `points[0]` (its loop starts
//!   at 0 where `AABox::Init` starts at 1) — harmless, but preserved.
//!
//! # Parity level
//!
//! **P0 exact**: the golden vectors in `tests/golden/geometry.txt` pin
//! every operation's bit patterns.

use crate::colmath::{self, OverlapType};
use crate::matrix3d::Matrix3D;
use crate::vector3::Vector3;

/// `AABoxClass`: an axis-aligned box by center and half-extent.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct AABox {
    /// World-space center.
    pub center: Vector3,
    /// Half-size of the box along the three axes.
    pub extent: Vector3,
}

impl AABox {
    /// `AABoxClass(center, extent)`.
    #[must_use]
    pub const fn new(center: Vector3, extent: Vector3) -> Self {
        Self { center, extent }
    }

    /// `Init(points, num)`: the bounding box of the given points
    /// (panics on an empty slice, where the original asserts `num > 0`).
    #[must_use]
    pub fn from_points(points: &[Vector3]) -> Self {
        let mut min = points[0];
        let mut max = points[0];
        for p in &points[1..] {
            if min.x > p.x {
                min.x = p.x;
            }
            if min.y > p.y {
                min.y = p.y;
            }
            if min.z > p.z {
                min.z = p.z;
            }
            if max.x < p.x {
                max.x = p.x;
            }
            if max.y < p.y {
                max.y = p.y;
            }
            if max.z < p.z {
                max.z = p.z;
            }
        }
        Self::from_min_max(min, max)
    }

    /// `Init_Min_Max(min, max)`.
    #[must_use]
    pub fn from_min_max(min: Vector3, max: Vector3) -> Self {
        Self {
            center: (max + min) * 0.5,
            extent: (max - min) * 0.5,
        }
    }

    /// `Init(const MinMaxAABoxClass&)`.
    #[must_use]
    pub fn from_min_max_box(b: &MinMaxAABox) -> Self {
        Self {
            center: (b.max_corner + b.min_corner) * 0.5,
            extent: (b.max_corner - b.min_corner) * 0.5,
        }
    }

    /// `Add_Point`: expands the box to contain the point.
    pub fn add_point(&mut self, point: Vector3) {
        let mut min = self.center - self.extent;
        let mut max = self.center + self.extent;

        if min.x > point.x {
            min.x = point.x;
        }
        if min.y > point.y {
            min.y = point.y;
        }
        if min.z > point.z {
            min.z = point.z;
        }
        if max.x < point.x {
            max.x = point.x;
        }
        if max.y < point.y {
            max.y = point.y;
        }
        if max.z < point.z {
            max.z = point.z;
        }

        self.center = (max + min) / 2.0;
        self.extent = (max - min) / 2.0;
    }

    /// `Add_Box(const AABoxClass&)`: expands this box to enclose `b`.
    pub fn add_box(&mut self, b: &Self) {
        let mut new_min = self.center - self.extent;
        let mut new_max = self.center + self.extent;
        new_min.update_min(b.center - b.extent);
        new_max.update_max(b.center + b.extent);

        self.center = (new_max + new_min) * 0.5;
        self.extent = (new_max - new_min) * 0.5;
    }

    /// `Add_Box(const MinMaxAABoxClass&)`.
    pub fn add_min_max_box(&mut self, b: &MinMaxAABox) {
        let mut new_min = self.center - self.extent;
        let mut new_max = self.center + self.extent;
        new_min.update_min(b.min_corner);
        new_max.update_max(b.max_corner);

        self.center = (new_max + new_min) * 0.5;
        self.extent = (new_max - new_min) * 0.5;
    }

    /// `Project_To_Axis`: the sum of the absolute projections of the
    /// three extents onto `axis`.
    #[must_use]
    pub fn project_to_axis(&self, axis: Vector3) -> f32 {
        let x = self.extent[0] * axis[0];
        let y = self.extent[1] * axis[1];
        let z = self.extent[2] * axis[2];
        crate::scalar::fabs(x) + crate::scalar::fabs(y) + crate::scalar::fabs(z)
    }

    /// `Volume`: `2*Ex * 2*Ey * 2*Ez`, computed in double precision like
    /// the original's double literals and truncated at the return.
    #[must_use]
    pub fn volume(&self) -> f32 {
        (2.0 * f64::from(self.extent.x)
            * 2.0
            * f64::from(self.extent.y)
            * 2.0
            * f64::from(self.extent.z)) as f32
    }

    /// `Contains(point)`: the point is inside the box
    /// (`Overlap_Test(box, point) == INSIDE`).
    #[must_use]
    pub fn contains_point(&self, point: Vector3) -> bool {
        colmath::overlap_test_aabox_point(self, &point) == OverlapType::INSIDE
    }

    /// `Contains(other_box)`: the other box is inside this one
    /// (`Overlap_Test(box, box2) == INSIDE`).
    #[must_use]
    pub fn contains_box(&self, other_box: &Self) -> bool {
        colmath::overlap_test_aabox_aabox(self, other_box) == OverlapType::INSIDE
    }

    /// `Contains(const MinMaxAABoxClass&)`.
    #[must_use]
    pub fn contains_min_max_box(&self, other_box: &MinMaxAABox) -> bool {
        let bmin = self.center - self.extent;
        let bmax = self.center + self.extent;

        if other_box.min_corner.x < bmin.x {
            return false;
        }
        if other_box.min_corner.y < bmin.y {
            return false;
        }
        if other_box.min_corner.z < bmin.z {
            return false;
        }
        if other_box.max_corner.x > bmax.x {
            return false;
        }
        if other_box.max_corner.y > bmax.y {
            return false;
        }
        if other_box.max_corner.z > bmax.z {
            return false;
        }
        true
    }

    /// `Transform(tm)`: expands the box to enclose its transformed form
    /// (the transform of a rotating box grows it).
    pub fn transform(&mut self, tm: &Matrix3D) {
        let (center, extent) = tm.transform_center_extent_aabox(self.center, self.extent);
        self.center = center;
        self.extent = extent;
    }

    /// `Transform(tm, in, out)` (static): the transformed box.
    #[must_use]
    pub fn transformed(&self, tm: &Matrix3D) -> Self {
        let (center, extent) = tm.transform_center_extent_aabox(self.center, self.extent);
        Self { center, extent }
    }

    /// `Translate`.
    pub fn translate(&mut self, trans: Vector3) {
        self.center += trans;
    }
}

/// `MinMaxAABoxClass`: an axis-aligned box by its min and max corners.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct MinMaxAABox {
    /// The minimum corner.
    pub min_corner: Vector3,
    /// The maximum corner.
    pub max_corner: Vector3,
}

impl MinMaxAABox {
    /// `MinMaxAABoxClass(min_corner, max_corner)`.
    #[must_use]
    pub const fn new(min_corner: Vector3, max_corner: Vector3) -> Self {
        Self {
            min_corner,
            max_corner,
        }
    }

    /// `Init(points, num)`: the bounding box of the points. The loop
    /// starts at index 0 (the original re-visits `points[0]`).
    #[must_use]
    pub fn from_points(points: &[Vector3]) -> Self {
        let mut b = Self {
            min_corner: points[0],
            max_corner: points[0],
        };
        for p in points {
            b.min_corner.update_min(*p);
            b.max_corner.update_max(*p);
        }
        b
    }

    /// `Init(const AABoxClass&)`.
    #[must_use]
    pub fn from_aabox(b: &AABox) -> Self {
        Self {
            min_corner: b.center - b.extent,
            max_corner: b.center + b.extent,
        }
    }

    /// `Init_Empty`: the box that contains nothing (addition identity).
    pub fn init_empty(&mut self) {
        self.min_corner = Vector3::new(f32::MAX, f32::MAX, f32::MAX);
        self.max_corner = Vector3::new(-f32::MAX, -f32::MAX, -f32::MAX);
    }

    /// `Init_Empty` as a value.
    #[must_use]
    pub fn empty() -> Self {
        let mut b = Self::default();
        b.init_empty();
        b
    }

    /// `Add_Point`.
    pub fn add_point(&mut self, point: Vector3) {
        self.min_corner.update_min(point);
        self.max_corner.update_max(point);
    }

    /// `Add_Box(const MinMaxAABoxClass&)`: ignored entirely when the
    /// other box's corners coincide (preserved early return).
    pub fn add_box(&mut self, b: &Self) {
        if b.min_corner == b.max_corner {
            return;
        }
        self.min_corner.update_min(b.min_corner);
        self.max_corner.update_max(b.max_corner);
    }

    /// `Add_Box(const AABoxClass&)`: ignored entirely when the other box
    /// has zero extent (preserved early return).
    pub fn add_aabox(&mut self, b: &AABox) {
        if b.extent == Vector3::new(0.0, 0.0, 0.0) {
            return;
        }
        self.min_corner.update_min(b.center - b.extent);
        self.max_corner.update_max(b.center + b.extent);
    }

    /// `Add_Box(min_corner, max_corner)`: ignored entirely when the two
    /// corners are equal (preserved early return).
    pub fn add_min_max(&mut self, min_corner: Vector3, max_corner: Vector3) {
        if min_corner == max_corner {
            return;
        }
        self.min_corner.update_min(min_corner);
        self.max_corner.update_max(max_corner);
    }

    /// `Transform(tm)`: encloses the transformed box.
    pub fn transform(&mut self, tm: &Matrix3D) {
        let (min_corner, max_corner) = tm.transform_min_max_aabox(self.min_corner, self.max_corner);
        self.min_corner = min_corner;
        self.max_corner = max_corner;
    }

    /// `Translate`.
    pub fn translate(&mut self, pos: Vector3) {
        self.min_corner += pos;
        self.max_corner += pos;
    }

    /// `Volume`: the product of the side lengths.
    #[must_use]
    pub fn volume(&self) -> f32 {
        let size = self.max_corner - self.min_corner;
        size.x * size.y * size.z
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_from_points_and_min_max() {
        let pts = [
            Vector3::new(-1.0, 2.0, 0.0),
            Vector3::new(3.0, -4.0, 5.0),
            Vector3::new(1.0, 1.0, -6.0),
        ];
        let b = AABox::from_points(&pts);
        assert_eq!(b.center, Vector3::new(1.0, -1.0, -0.5));
        assert_eq!(b.extent, Vector3::new(2.0, 3.0, 5.5));

        let mm = MinMaxAABox::from_points(&pts);
        assert_eq!(mm.min_corner, Vector3::new(-1.0, -4.0, -6.0));
        assert_eq!(mm.max_corner, Vector3::new(3.0, 2.0, 5.0));

        assert_eq!(AABox::from_min_max(mm.min_corner, mm.max_corner), b);
        assert_eq!(MinMaxAABox::from_aabox(&b), mm);
    }

    #[test]
    fn add_point_and_boxes() {
        let mut b = AABox::new(Vector3::new(0.0, 0.0, 0.0), Vector3::new(1.0, 1.0, 1.0));
        b.add_point(Vector3::new(3.0, 0.0, 0.0));
        assert_eq!(b.extent, Vector3::new(2.0, 1.0, 1.0));

        b.add_box(&AABox::new(
            Vector3::new(0.0, 5.0, 0.0),
            Vector3::new(1.0, 1.0, 1.0),
        ));
        // Union of x:[-1,3], y:[-1,6], z:[-1,1]: center (1, 2.5, 0),
        // extent (2, 3.5, 1).
        assert_eq!(b.center, Vector3::new(1.0, 2.5, 0.0));
        assert_eq!(b.extent, Vector3::new(2.0, 3.5, 1.0));

        let mut mm = MinMaxAABox::new(Vector3::new(-1.0, -1.0, -1.0), Vector3::new(1.0, 1.0, 1.0));
        mm.add_point(Vector3::new(0.0, 0.0, 7.0));
        assert_eq!(mm.max_corner, Vector3::new(1.0, 1.0, 7.0));
        // The coincident-corner early return: adding a point box is a
        // no-op.
        mm.add_box(&MinMaxAABox::new(
            Vector3::new(2.0, 2.0, 2.0),
            Vector3::new(2.0, 2.0, 2.0),
        ));
        assert_eq!(mm.max_corner, Vector3::new(1.0, 1.0, 7.0));
        // The zero-extent early return.
        mm.add_aabox(&AABox::new(
            Vector3::new(9.0, 9.0, 9.0),
            Vector3::new(0.0, 0.0, 0.0),
        ));
        assert_eq!(mm.max_corner, Vector3::new(1.0, 1.0, 7.0));
    }

    #[test]
    fn volumes_use_the_originals_precision() {
        let b = AABox::new(Vector3::new(0.0, 0.0, 0.0), Vector3::new(0.5, 1.5, 2.5));
        // double-precision product: 2*.5 * 2*1.5 * 2*2.5 = 15 exactly.
        assert_eq!(b.volume(), 15.0);
        let mm = MinMaxAABox::new(Vector3::new(0.0, 0.0, 0.0), Vector3::new(1.0, 3.0, 5.0));
        assert_eq!(mm.volume(), 15.0);
    }

    #[test]
    fn contains_and_projection() {
        let b = AABox::new(Vector3::new(0.0, 0.0, 0.0), Vector3::new(1.0, 2.0, 3.0));
        assert!(b.contains_point(Vector3::new(0.5, 1.5, 2.5)));
        assert!(!b.contains_point(Vector3::new(1.5, 0.0, 0.0)));
        assert!(b.contains_box(&AABox::new(
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(0.5, 0.5, 0.5)
        )));
        assert!(!b.contains_box(&AABox::new(
            Vector3::new(2.0, 0.0, 0.0),
            Vector3::new(0.5, 0.5, 0.5)
        )));
        assert!(b.contains_min_max_box(&MinMaxAABox::new(
            Vector3::new(-1.0, -2.0, -3.0),
            Vector3::new(1.0, 2.0, 3.0)
        )));
        assert_eq!(b.project_to_axis(Vector3::new(0.0, 0.0, 1.0)), 3.0);
        // The diagonal axis takes the absolute projections of all three.
        let inv_sqrt3 = 1.0 / 3.0f32.sqrt();
        let d = b.project_to_axis(Vector3::new(inv_sqrt3, inv_sqrt3, inv_sqrt3));
        assert!((d - 6.0 * inv_sqrt3).abs() < 1e-6);
    }
}
