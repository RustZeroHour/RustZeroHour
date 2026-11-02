// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
//! Property tests (plan §6 test layers) for the `rzh-math` types: the
//! algebraic invariants that must hold for *any* input — normalization
//! produces unit vectors, rotations undo through their orthogonal
//! inverse, growing a box keeps containing the points that grew it, and
//! the frustum bound really bounds its corners.
//!
//! These complement the bit-exact golden vectors (which pin the C++
//! parity, quirks included): properties state mathematical contracts,
//! with tolerances where `f32` rounding demands them.

use proptest::prelude::*;

use rzh_math::{AABox, Frustum, Matrix3D, Vector2, Vector3};

/// A quarter-step float, exactly representable, bounded so that no
/// intermediate overflows: the same class of inputs the golden vectors
/// use (`stepFloat`).
fn quarter() -> impl Strategy<Value = f32> {
    (0i32..20_001).prop_map(|i| ((i - 10_000) as f32) * 0.25)
}

/// A finite quarter-step point.
fn point3() -> impl Strategy<Value = Vector3> {
    (quarter(), quarter(), quarter()).prop_map(|(x, y, z)| Vector3::new(x, y, z))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// `Normalize` yields a unit vector (or leaves the zero vector).
    #[test]
    fn normalize_makes_unit_vectors(v in point3()) {
        let len2 = v.length2();
        let mut normalized = v;
        normalized.normalize();
        if len2 == 0.0 {
            prop_assert_eq!(normalized, Vector3::new(0.0, 0.0, 0.0));
        } else {
            // 1/sqrt is correctly rounded and each component picks up at
            // most a few ulp, so the renormalized length² sits within
            // ~3e-6 of 1.
            prop_assert!(
                (normalized.length2() - 1.0).abs() < 1e-5,
                "length2 after normalize: {}",
                normalized.length2()
            );
        }
    }

    /// A rotation composed with its orthogonal inverse is the identity
    /// (rotations are orthogonal, so `Get_Orthogonal_Inverse` undoes
    /// them).
    #[test]
    fn rotations_undo_through_the_orthogonal_inverse(
        axis in point3(),
        eighth_turns in -40i32..=40,
    ) {
        prop_assume!(axis.length2() != 0.0);
        let mut unit_axis = axis;
        unit_axis.normalize();
        let angle = (eighth_turns as f32) * 0.125;

        let rotation = Matrix3D::from_axis_angle(unit_axis, angle);
        let inverse = rotation.get_orthogonal_inverse();

        let mut product = Matrix3D::identity();
        product.mul(rotation, inverse);

        // The 3x3 part within tolerance of the identity and the
        // translation exactly zero.
        for (i, row) in product.rows.iter().enumerate() {
            for (j, value) in [row.x, row.y, row.z].into_iter().enumerate() {
                let expected = if i == j { 1.0 } else { 0.0 };
                prop_assert!(
                    (value - expected).abs() < 1e-4,
                    "({i},{j}) = {value}, expected {expected}"
                );
            }
            prop_assert_eq!(row.w, 0.0);
        }
    }

    /// Growing a box to contain a point makes it contain that point.
    #[test]
    fn add_point_then_contains_point(
        center in point3(),
        extent_raw in point3(),
        point in point3(),
    ) {
        // Extents are half-sizes: non-negative by construction.
        let extent = Vector3::new(
            extent_raw.x.abs(),
            extent_raw.y.abs(),
            extent_raw.z.abs(),
        );
        let mut b = AABox::new(center, extent);
        b.add_point(point);
        prop_assert!(b.contains_point(point));
    }

    /// The frustum bound bounds every corner, axis by axis.
    #[test]
    fn frustum_bound_bounds_its_corners(
        tx in quarter(), ty in quarter(), tz in quarter(),
        vp in (quarter(), quarter(), quarter(), quarter()),
        near in 1i32..4_000,
        far_extra in 0i32..4_000,
    ) {
        let mut camera = Matrix3D::identity();
        camera.rows[0].w = tx;
        camera.rows[1].w = ty;
        camera.rows[2].w = tz;

        let znear = -((near as f32) * 0.25);
        let zfar = znear - (far_extra as f32) * 0.25;

        let mut f = Frustum::default();
        f.init(
            &camera,
            Vector2::new(vp.0, vp.1),
            Vector2::new(vp.2, vp.3),
            znear,
            zfar,
        );

        for corner in &f.corners {
            prop_assert!(corner.x >= f.bound_min.x && corner.x <= f.bound_max.x);
            prop_assert!(corner.y >= f.bound_min.y && corner.y <= f.bound_max.y);
            prop_assert!(corner.z >= f.bound_min.z && corner.z <= f.bound_max.z);
        }
    }
}
