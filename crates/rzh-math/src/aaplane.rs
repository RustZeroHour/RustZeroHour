// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Ported from the following files at baseline commit 571a7c5, Zero Hour
// configuration, deterministic branches (ADR-000). The originals carry the
// following notices, preserved here:
//
//   aaplane.h — AAPlaneClass: an "axis-aligned" plane whose normal is one
//   of the three coordinate axes. Author: Greg Hjelstrom, 2000.
//
//   (c) 2001-2003 Electronic Arts Inc.  Confidential — Westwood Studios.
//
//! Axis-aligned planes: planes whose normal is one of the coordinate axes.
//!
//! A value is an axis ([`Axis`]) plus a distance along it; expanding it to
//! a full [`Vector3`](crate::vector3::Vector3) normal is
//! [`AAPlane::normal`].

use crate::vector3::Vector3;

/// `AAPlaneClass::AxisEnum`: which coordinate axis the normal is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// `XNORMAL`.
    X,
    /// `YNORMAL`.
    Y,
    /// `ZNORMAL`.
    Z,
}

/// `AAPlaneClass`: an axis-aligned plane (axis normal plus distance).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AAPlane {
    /// Which axis the normal points along.
    pub normal: Axis,
    /// Distance along the normal from the origin.
    pub dist: f32,
}

impl AAPlane {
    /// `AAPlaneClass(normal, dist)`.
    #[must_use]
    pub const fn new(normal: Axis, dist: f32) -> Self {
        Self { normal, dist }
    }

    /// `Set`.
    pub fn set(&mut self, normal: Axis, dist: f32) {
        self.normal = normal;
        self.dist = dist;
    }

    /// `Get_Normal`: the normal expanded to a unit [`Vector3`].
    #[must_use]
    pub fn normal(&self) -> Vector3 {
        match self.normal {
            Axis::X => Vector3::new(1.0, 0.0, 0.0),
            Axis::Y => Vector3::new(0.0, 1.0, 0.0),
            Axis::Z => Vector3::new(0.0, 0.0, 1.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normals_expand_to_unit_axes() {
        assert_eq!(
            AAPlane::new(Axis::X, 5.0).normal(),
            Vector3::new(1.0, 0.0, 0.0)
        );
        assert_eq!(
            AAPlane::new(Axis::Y, -2.0).normal(),
            Vector3::new(0.0, 1.0, 0.0)
        );
        assert_eq!(
            AAPlane::new(Axis::Z, 0.0).normal(),
            Vector3::new(0.0, 0.0, 1.0)
        );
    }
}
