// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Ported from the following files at baseline commit 571a7c5, Zero Hour
// configuration, deterministic branches (ADR-000). The originals carry the
// following notices, preserved here:
//
//   colmath.h — CollisionMath: the collision-math function library.
//   Original author: Greg Hjelstrom.
//
//   colmath.cpp / colmathaabox.h / colmathplane.cpp — the Overlap_Test
//   implementations ported here.
//
//   (c) 2001-2003 Electronic Arts Inc.  Confidential — Westwood Studios.
//
//! The `CollisionMath` overlap-test subset the ported geometry types
//! need.
//!
//! [`OverlapType`] classifies the second operand with respect to the
//! first (`Overlap_Test(box, point)` asks where the point is relative to
//! the box); [`eval_overlap_mask`] reduces per-vertex classifications of
//! a whole primitive into one verdict.
//!
//! Ported here: `Overlap_Test` for AABox-vs-point, AABox-vs-AABox
//! (colmathaabox.h), AAPlane-vs-point and AAPlane-vs-AABox
//! (colmathplane.cpp). The rest of the family — line segments, triangles,
//! spheres, oriented boxes and the `Intersection_Test`/`Cast_Ray`
//! machinery — arrives with the culling/collision system that consumes
//! it (plan §4, Phase 3/4).
//!
//! # Parity level
//!
//! **P0 exact**: the golden vectors in `tests/golden/geometry.txt` pin
//! every verdict.

use crate::aabox::AABox;
use crate::aaplane::{AAPlane, Axis};
use crate::scalar;
use crate::vector3::Vector3;

/// `CollisionMath::OverlapType`: where operand B is with respect to
/// operand A.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum OverlapType {
    /// `POS` / `OUTSIDE` / `FRONT`: in the positive space of A.
    Pos = 0x01,
    /// `NEG` / `INSIDE` / `BACK`: in the negative space of A.
    Neg = 0x02,
    /// `ON`: on A (within the coincidence epsilon).
    On = 0x04,
    /// `BOTH` / `OVERLAPPED`: spans A.
    Both = 0x08,
}

impl OverlapType {
    /// `OUTSIDE` (an alias of [`OverlapType::Pos`]).
    pub const OUTSIDE: Self = Self::Pos;
    /// `INSIDE` (an alias of [`OverlapType::Neg`]).
    pub const INSIDE: Self = Self::Neg;
    /// `OVERLAPPED` (an alias of [`OverlapType::Both`]).
    pub const OVERLAPPED: Self = Self::Both;
    /// `FRONT` (an alias of [`OverlapType::Pos`]).
    pub const FRONT: Self = Self::Pos;
    /// `BACK` (an alias of [`OverlapType::Neg`]).
    pub const BACK: Self = Self::Neg;

    /// The original's numeric value (the enum's bit pattern).
    #[must_use]
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// The value with the original's numeric code, if any.
    #[must_use]
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            0x01 => Some(Self::Pos),
            0x02 => Some(Self::Neg),
            0x04 => Some(Self::On),
            0x08 => Some(Self::Both),
            _ => None,
        }
    }
}

/// `CollisionMath::COINCIDENCE_EPSILON` (colmath.cpp).
pub const COINCIDENCE_EPSILON: f32 = 0.000_001;

/// `CollisionMath::eval_overlap_mask`: reduces a mask of per-vertex
/// classifications into one verdict — all-`ON` stays `ON`, otherwise the
/// single-sided masks collapse to that side, and anything else spans.
#[must_use]
pub const fn eval_overlap_mask(mask: u8) -> OverlapType {
    // check if all verts are "ON"
    if mask == OverlapType::On as u8 {
        return OverlapType::On;
    }

    // check if all verts are either "ON" or "POS"
    if mask & !(OverlapType::Pos as u8 | OverlapType::On as u8) == 0 {
        return OverlapType::Pos;
    }

    // check if all verts are either "ON" or "BACK"
    if mask & !(OverlapType::Neg as u8 | OverlapType::On as u8) == 0 {
        return OverlapType::Neg;
    }

    // otherwise, poly spans the plane.
    OverlapType::Both
}

/// `CollisionMath::Overlap_Test(box, point)` (colmathaabox.h): whether
/// the point is inside the box.
#[must_use]
pub fn overlap_test_aabox_point(b: &AABox, point: &Vector3) -> OverlapType {
    if scalar::fabs(point.x - b.center.x) > b.extent.x {
        return OverlapType::Pos;
    }
    if scalar::fabs(point.y - b.center.y) > b.extent.y {
        return OverlapType::Pos;
    }
    if scalar::fabs(point.z - b.center.z) > b.extent.z {
        return OverlapType::Pos;
    }

    OverlapType::Neg
}

/// `CollisionMath::Overlap_Test(box, box2)` (colmathaabox.h): the
/// separation/containment verdict of two axis-aligned boxes.
#[must_use]
pub fn overlap_test_aabox_aabox(b: &AABox, b2: &AABox) -> OverlapType {
    let dc = b2.center - b.center;

    if b.extent.x + b2.extent.x < scalar::fabs(dc.x) {
        return OverlapType::Pos;
    }
    if b.extent.y + b2.extent.y < scalar::fabs(dc.y) {
        return OverlapType::Pos;
    }
    if b.extent.z + b2.extent.z < scalar::fabs(dc.z) {
        return OverlapType::Pos;
    }

    if (dc.x + b2.extent.x <= b.extent.x)
        && (dc.y + b2.extent.y <= b.extent.y)
        && (dc.z + b2.extent.z <= b.extent.z)
        && (dc.x - b2.extent.x >= -b.extent.x)
        && (dc.y - b2.extent.y >= -b.extent.y)
        && (dc.z - b2.extent.z >= -b.extent.z)
    {
        return OverlapType::Neg; // inside;
    }

    OverlapType::Both
}

/// The axis indexing used by the AAPlane overlap tests (`point[plane.
/// Normal]` in the original).
const fn axis_value(v: &Vector3, axis: Axis) -> f32 {
    match axis {
        Axis::X => v.x,
        Axis::Y => v.y,
        Axis::Z => v.z,
    }
}

/// `CollisionMath::Overlap_Test(plane, point)` (colmathplane.cpp): the
/// side of the plane the point is on, with the coincidence epsilon.
#[must_use]
pub fn overlap_test_aaplane_point(plane: &AAPlane, point: &Vector3) -> OverlapType {
    let delta = axis_value(point, plane.normal) - plane.dist;
    if delta > COINCIDENCE_EPSILON {
        return OverlapType::Pos;
    }
    if delta < -COINCIDENCE_EPSILON {
        return OverlapType::Neg;
    }
    OverlapType::On
}

/// `CollisionMath::Overlap_Test(plane, box)` (colmathplane.cpp): where
/// the box is with respect to the plane, by classifying the box's min
/// and max sides along the plane's axis and reducing the mask.
#[must_use]
pub fn overlap_test_aaplane_aabox(plane: &AAPlane, b: &AABox) -> OverlapType {
    let mut mask = 0u8;

    // check the 'min' side of the box
    let delta =
        (axis_value(&b.center, plane.normal) - axis_value(&b.extent, plane.normal)) - plane.dist;
    if delta > crate::scalar::EPSILON {
        mask |= OverlapType::Pos as u8;
    } else if delta < -crate::scalar::EPSILON {
        mask |= OverlapType::Neg as u8;
    } else {
        mask |= OverlapType::On as u8;
    }

    // check the 'max' side of the box
    let delta =
        (axis_value(&b.center, plane.normal) + axis_value(&b.extent, plane.normal)) - plane.dist;
    if delta > crate::scalar::EPSILON {
        mask |= OverlapType::Pos as u8;
    } else if delta < -crate::scalar::EPSILON {
        mask |= OverlapType::Neg as u8;
    } else {
        mask |= OverlapType::On as u8;
    }

    eval_overlap_mask(mask)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aabox::MinMaxAABox;

    #[test]
    fn aabox_point_classification() {
        let b = AABox::new(Vector3::new(0.0, 0.0, 0.0), Vector3::new(1.0, 2.0, 3.0));
        assert_eq!(
            overlap_test_aabox_point(&b, &Vector3::new(0.0, 0.0, 0.0)),
            OverlapType::Neg
        );
        assert_eq!(
            overlap_test_aabox_point(&b, &Vector3::new(1.0, 2.0, 3.0)),
            OverlapType::Neg
        );
        assert_eq!(
            overlap_test_aabox_point(&b, &Vector3::new(1.5, 0.0, 0.0)),
            OverlapType::Pos
        );
    }

    #[test]
    fn aabox_aabox_classification() {
        let b = AABox::new(Vector3::new(0.0, 0.0, 0.0), Vector3::new(1.0, 1.0, 1.0));
        // Disjoint along x.
        let other = AABox::new(Vector3::new(3.0, 0.0, 0.0), Vector3::new(1.0, 1.0, 1.0));
        assert_eq!(overlap_test_aabox_aabox(&b, &other), OverlapType::Pos);
        // Contained.
        let other = AABox::new(Vector3::new(0.0, 0.0, 0.0), Vector3::new(0.5, 0.5, 0.5));
        assert_eq!(overlap_test_aabox_aabox(&b, &other), OverlapType::Neg);
        // Overlapping.
        let other = AABox::new(Vector3::new(1.0, 0.0, 0.0), Vector3::new(1.0, 1.0, 1.0));
        assert_eq!(overlap_test_aabox_aabox(&b, &other), OverlapType::Both);
    }

    #[test]
    fn aaplane_classifications_use_their_epsilons() {
        let plane = AAPlane::new(Axis::X, 5.0);
        // Beyond the coincidence epsilon: positive.
        assert_eq!(
            overlap_test_aaplane_point(&plane, &Vector3::new(5.0 + 2e-6, 0.0, 0.0)),
            OverlapType::Pos
        );
        assert_eq!(
            overlap_test_aaplane_point(&plane, &Vector3::new(4.0, 0.0, 0.0)),
            OverlapType::Neg
        );
        // Inside the coincidence epsilon: ON.
        assert_eq!(
            overlap_test_aaplane_point(&plane, &Vector3::new(5.0 + 1e-7, 0.0, 0.0)),
            OverlapType::On
        );

        // A box straddling the plane.
        let b = AABox::new(Vector3::new(5.0, 0.0, 0.0), Vector3::new(2.0, 1.0, 1.0));
        assert_eq!(overlap_test_aaplane_aabox(&plane, &b), OverlapType::Both);
        // A box entirely on the far side.
        let b = AABox::new(Vector3::new(9.0, 0.0, 0.0), Vector3::new(2.0, 1.0, 1.0));
        assert_eq!(overlap_test_aaplane_aabox(&plane, &b), OverlapType::Pos);
        // A box entirely on the near side.
        let b = AABox::new(Vector3::new(1.0, 0.0, 0.0), Vector3::new(2.0, 1.0, 1.0));
        assert_eq!(overlap_test_aaplane_aabox(&plane, &b), OverlapType::Neg);
    }

    #[test]
    fn overlap_mask_reduction() {
        assert_eq!(eval_overlap_mask(OverlapType::On as u8), OverlapType::On);
        assert_eq!(
            eval_overlap_mask(OverlapType::Pos as u8 | OverlapType::On as u8),
            OverlapType::Pos
        );
        assert_eq!(
            eval_overlap_mask(OverlapType::Neg as u8 | OverlapType::On as u8),
            OverlapType::Neg
        );
        assert_eq!(
            eval_overlap_mask(OverlapType::Pos as u8 | OverlapType::Neg as u8),
            OverlapType::Both
        );
    }

    #[test]
    fn contains_wires_through_the_overlap_tests() {
        let b = AABox::new(Vector3::new(0.0, 0.0, 0.0), Vector3::new(1.0, 1.0, 1.0));
        let inner = MinMaxAABox::new(Vector3::new(-0.5, -0.5, -0.5), Vector3::new(0.5, 0.5, 0.5));
        assert!(b.contains_min_max_box(&inner));
        let poke = MinMaxAABox::new(Vector3::new(-0.5, -0.5, -0.5), Vector3::new(1.5, 0.5, 0.5));
        assert!(!b.contains_min_max_box(&poke));
    }
}
