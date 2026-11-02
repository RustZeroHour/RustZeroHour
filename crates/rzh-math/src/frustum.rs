// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Ported from the following files at baseline commit 571a7c5, Zero Hour
// configuration, deterministic branches (ADR-000). The originals carry the
// following notices, preserved here:
//
//   frustum.h / frustum.cpp — FrustumClass: a view frustum initialized
//   from a camera transform and viewport. Author: Greg Hjelstrom, 2000.
//
//   (c) 2001-2003 Electronic Arts Inc.  Confidential — Westwood Studios.
//
//! The view frustum: six outward-facing planes, eight corners and a
//! bounding box, built from a camera description.
//!
//! [`Frustum::init`] is the whole type: it stores the camera transform,
//! extrapolates the viewport rectangle on the `z = -1` view plane out to
//! the near and far clip planes (each corner is scaled by the clip
//! distance as a whole vector), transforms the eight corners to world
//! space and builds the bounding planes (normals pointing outward) and
//! the world-space bound.
//!
//! # Quirks preserved
//!
//! - positive `znear`/`zfar` inputs are negated **only when both are
//!   positive** (a mixed-sign pair passes through untouched, exactly
//!   like the original's `&&`);
//! - a reflected camera matrix (one whose Z vector disagrees with
//!   X × Y) swaps the corners horizontally;
//! - the near-plane triangles are wound `0, 3, 1` (upper-left, lower-
//!   right, upper-right), which is what makes the plane normals face
//!   outward — kept verbatim, comments and all.
//!
//! # Parity level
//!
//! **P0 exact**: the golden vectors in `tests/golden/geometry.txt` pin
//! every plane, corner and bound for normal, reflected and sign-flipped
//! camera inputs.

use crate::matrix3d::Matrix3D;
use crate::plane::Plane;
use crate::vector2::Vector2;
use crate::vector3::Vector3;

/// `FrustumClass`: a view frustum.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frustum {
    /// The camera transform the frustum was built from.
    pub camera_transform: Matrix3D,
    /// The six bounding planes, normals pointing outward.
    ///
    /// Plane 0: NEAR, 1: bottom, 2: right, 3: top, 4: left, 5: FAR.
    pub planes: [Plane; 6],
    /// The eight frustum corners in world space.
    ///
    /// Corners 0-3: NEAR upper left, upper right, lower left, lower
    /// right; 4-7: the FAR corners in the same order.
    pub corners: [Vector3; 8],
    /// The world-space bound of all corners (sloppy quick rejection).
    pub bound_min: Vector3,
    /// See [`Self::bound_min`].
    pub bound_max: Vector3,
}

impl Default for Frustum {
    fn default() -> Self {
        // The C++ default-constructs the members; the planes repeat the
        // PlaneClass default (N = +Z, D = 0) and the Matrix3D is
        // uninitialised garbage. The port uses Matrix3D::identity(),
        // which every use of a default Frustum immediately overwrites
        // via init().
        Self {
            camera_transform: Matrix3D::identity(),
            planes: [Plane::new(); 6],
            corners: [Vector3::default(); 8],
            bound_min: Vector3::default(),
            bound_max: Vector3::default(),
        }
    }
}

impl Frustum {
    /// `Get_Bound_Min`.
    #[must_use]
    pub const fn bound_min(&self) -> Vector3 {
        self.bound_min
    }

    /// `Get_Bound_Max`.
    #[must_use]
    pub const fn bound_max(&self) -> Vector3 {
        self.bound_max
    }

    /// `FrustumClass::Init`: builds the frustum from a camera.
    ///
    /// - `camera`: camera transform; the camera looks down the -Z axis;
    /// - `viewport_min` / `viewport_max`: the min and max corner of the
    ///   view plane at `z = -1` (not necessarily the near clip plane);
    /// - `znear` / `zfar`: the clip planes (should be negative; positive
    ///   values are negated — see the module docs on the both-positive
    ///   quirk).
    pub fn init(
        &mut self,
        camera: &Matrix3D,
        vpmin: Vector2,
        vpmax: Vector2,
        mut znear: f32,
        mut zfar: f32,
    ) {
        // Store the camera transform
        self.camera_transform = *camera;

        // Forward is negative Z in our viewspace coordinate system.
        // Just flip the sign if the user passed in positive values.
        if (znear > 0.0) && (zfar > 0.0) {
            znear = -znear;
            zfar = -zfar;
        }

        // Calculate the corners of the camera frustum. Generate the
        // camera-space frustum corners by linearly extrapolating the
        // viewplane to the near and far z clipping planes.

        // The camera frustum corners are defined in the following order:
        // When looking at the frustum from the position of the camera,
        // the near four corners are numbered: upper left 0, upper right
        // 1, lower left 2, lower right 3. The far plane's frustum
        // corners are numbered from 4 to 7 in an analogous fashion.
        // (remember: the camera space has x going to the right, y up and
        // z backwards)

        // calculate a proper z-vector assuming our right-handed
        // coordinate system
        let zv = Vector3::cross(
            self.camera_transform.x_vector(),
            self.camera_transform.y_vector(),
        );

        // compare correct z-vector with the one in the camera matrix. If
        // they point in opposite directions, we have a reflected camera
        // matrix.
        if Vector3::dot(self.camera_transform.z_vector(), zv) < 0.0 {
            // flip the frustum corners horizontally for a reflected
            // matrix
            self.corners[1].set(vpmin.x, vpmax.y, 1.0);
            self.corners[5] = self.corners[1];
            self.corners[1] *= znear;
            self.corners[5] *= zfar;
            self.corners[0].set(vpmax.x, vpmax.y, 1.0);
            self.corners[4] = self.corners[0];
            self.corners[0] *= znear;
            self.corners[4] *= zfar;
            self.corners[3].set(vpmin.x, vpmin.y, 1.0);
            self.corners[7] = self.corners[3];
            self.corners[3] *= znear;
            self.corners[7] *= zfar;
            self.corners[2].set(vpmax.x, vpmin.y, 1.0);
            self.corners[6] = self.corners[2];
            self.corners[2] *= znear;
            self.corners[6] *= zfar;
        } else {
            // normal camera
            self.corners[0].set(vpmin.x, vpmax.y, 1.0);
            self.corners[4] = self.corners[0];
            self.corners[0] *= znear;
            self.corners[4] *= zfar;
            self.corners[1].set(vpmax.x, vpmax.y, 1.0);
            self.corners[5] = self.corners[1];
            self.corners[1] *= znear;
            self.corners[5] *= zfar;
            self.corners[2].set(vpmin.x, vpmin.y, 1.0);
            self.corners[6] = self.corners[2];
            self.corners[2] *= znear;
            self.corners[6] *= zfar;
            self.corners[3].set(vpmax.x, vpmin.y, 1.0);
            self.corners[7] = self.corners[3];
            self.corners[3] *= znear;
            self.corners[7] *= zfar;
        }

        // Transform the eight corners of the view frustum from camera
        // space to world space.
        for corner in &mut self.corners {
            *corner = self.camera_transform.transform_vector(*corner);
        }

        // Create the six frustum bounding planes from the eight corner
        // corners. The bounding planes are oriented so that their
        // normals point outward
        self.planes[0].set_points(self.corners[0], self.corners[3], self.corners[1]); // near
        self.planes[1].set_points(self.corners[0], self.corners[5], self.corners[4]); // bottom
        self.planes[2].set_points(self.corners[0], self.corners[6], self.corners[2]); // right
        self.planes[3].set_points(self.corners[2], self.corners[7], self.corners[3]); // top
        self.planes[4].set_points(self.corners[1], self.corners[7], self.corners[5]); // left
        self.planes[5].set_points(self.corners[4], self.corners[7], self.corners[6]); // far

        // find the bounding box of the entire frustum (may be used for
        // sloppy quick rejection)
        self.bound_min = self.corners[0];
        self.bound_max = self.corners[0];

        for corner in &self.corners[1..] {
            if corner.x < self.bound_min.x {
                self.bound_min.x = corner.x;
            }
            if corner.x > self.bound_max.x {
                self.bound_max.x = corner.x;
            }

            if corner.y < self.bound_min.y {
                self.bound_min.y = corner.y;
            }
            if corner.y > self.bound_max.y {
                self.bound_max.y = corner.y;
            }

            if corner.z < self.bound_min.z {
                self.bound_min.z = corner.z;
            }
            if corner.z > self.bound_max.z {
                self.bound_max.z = corner.z;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_camera_builds_a_backward_frustum() {
        // A camera at the origin looking down -Z with a viewport
        // [-1,1]x[-1,1] on the z=-1 plane.
        let mut f = Frustum::default();
        f.init(
            &Matrix3D::identity(),
            Vector2::new(-1.0, -1.0),
            Vector2::new(1.0, 1.0),
            -0.5,
            -10.0,
        );

        // Corner 0 (near, upper left) is (vmin.X, vmax.Y, 1) * znear —
        // the *whole* vector scales, so (-1, 1, 1) * (-0.5) is
        // (0.5, -0.5, -0.5), not (-1, 1, -0.5).
        assert_eq!(f.corners[0], Vector3::new(0.5, -0.5, -0.5));
        // Corner 5 (far, upper right): (1, 1, 1) * (-10) = (-10, -10, -10).
        assert_eq!(f.corners[5], Vector3::new(-10.0, -10.0, -10.0));
        // The world-space bound of the corners.
        assert_eq!(f.bound_min, Vector3::new(-10.0, -10.0, -10.0));
        assert_eq!(f.bound_max, Vector3::new(10.0, 10.0, -0.5));
        // The near plane is z = -0.5 with its normal pointing outward,
        // i.e. back towards the camera: N = (0, 0, 1), D = N . p = -0.5.
        assert_eq!(f.planes[0].n, Vector3::new(0.0, 0.0, 1.0));
        assert_eq!(f.planes[0].d, -0.5);
    }

    #[test]
    fn positive_clip_distances_are_negated_together() {
        let mut f = Frustum::default();
        f.init(
            &Matrix3D::identity(),
            Vector2::new(-1.0, -1.0),
            Vector2::new(1.0, 1.0),
            0.5,
            10.0,
        );
        // Both positive: negated together, so the corners match the
        // negative-input frustum exactly (whole-vector scaling).
        assert_eq!(f.corners[0], Vector3::new(0.5, -0.5, -0.5));
        assert_eq!(f.corners[4], Vector3::new(10.0, -10.0, -10.0));
    }

    #[test]
    fn reflected_camera_swaps_corners_horizontally() {
        // Mirror the camera about the YZ plane (X vector negated): a
        // reflected matrix. The viewport is asymmetric so the horizontal
        // corner swap is observable — a symmetric viewport would cancel
        // the swap against the mirror and hide it.
        let mut camera = Matrix3D::identity();
        camera.rows[0].x = -1.0;

        let mut f = Frustum::default();
        f.init(
            &camera,
            Vector2::new(-1.0, -1.0),
            Vector2::new(0.5, 1.0),
            -0.5,
            -10.0,
        );
        // The reflected branch numbers the corners from vpmax first:
        // corner 0 is (vmax.X, vmax.Y, 1) * znear = (-0.25, -0.5, -0.5)
        // in camera space, mirrored through the reflected transform to
        // (0.25, -0.5, -0.5) in world space. Corner 1 comes from vpmin:
        // (0.5, -0.5, -0.5) camera-space, (-0.5, -0.5, -0.5) world.
        assert_eq!(f.corners[0], Vector3::new(0.25, -0.5, -0.5));
        assert_eq!(f.corners[1], Vector3::new(-0.5, -0.5, -0.5));
        // The near-plane normal still points outward (towards the
        // camera) despite the reflection.
        assert_eq!(f.planes[0].n, Vector3::new(0.0, 0.0, 1.0));
        assert_eq!(f.planes[0].d, -0.5);
    }

    #[test]
    fn translated_camera_moves_the_frustum() {
        let mut camera = Matrix3D::identity();
        camera.rows[0].w = 10.0;
        camera.rows[1].w = 20.0;
        camera.rows[2].w = 30.0;

        let mut f = Frustum::default();
        f.init(
            &camera,
            Vector2::new(-1.0, -1.0),
            Vector2::new(1.0, 1.0),
            -0.5,
            -10.0,
        );
        // Corner 0 is (0.5, -0.5, -0.5) in camera space (whole-vector
        // scaling), translated to (10.5, 19.5, 29.5).
        assert_eq!(f.corners[0], Vector3::new(10.5, 19.5, 29.5));
        // Corner 4 (far, upper left): (10, -10, -10) + (10, 20, 30).
        assert_eq!(f.corners[4], Vector3::new(20.0, 10.0, 20.0));
        // The bound spans the near square at z = 29.5 and the far square
        // at z = 20.
        assert_eq!(f.bound_min, Vector3::new(0.0, 10.0, 20.0));
        assert_eq!(f.bound_max, Vector3::new(20.0, 30.0, 29.5));
    }
}
