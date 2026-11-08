// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Rust port Copyright 2026 the RustZeroHour contributors.
//
//! Property and unit tests for the collide-test family.
//!
//! - the result symmetry over random geometry pairs: swapping the pair
//!   keeps the boolean (`collideTest_X_Y(a, b)` and the swapped
//!   `collideTest_Y_X(b, a)` are the mirrored dispatches — the
//!   sphere/nonsphere composites swap and flip, the rect composites swap
//!   and flip, the symmetric procs are direct);
//! - the normal's unit length when written and the positions differ
//!   ("collideNormal is guaranteed to be a unit vector" — the
//!   `closeEnough` preflight's unwritten normal is the exception the
//!   corpora pin, and coincident positions take the normalize zero-guard);
//! - the geomCollidesWithGeom z-gate's below-extent refusal (the narrower
//!   gate the golden corpora pin against `collidesWith`'s).

use proptest::prelude::*;

use rzh_sim::collide::{
    COLLIDE_TEST_PROCS, CollideInfo, CollideLocAndNormal, geom_collides_with_geom,
};
use rzh_sim::types::{Coord3, GeometryInfo, GeometryType};

/// A random geometry: the three flavors with smallish extents.
fn arb_geometry() -> impl Strategy<Value = GeometryInfo> {
    (
        prop_oneof![
            Just(GeometryType::Sphere),
            Just(GeometryType::Cylinder),
            Just(GeometryType::Box),
        ],
        0.0f32..20.0,
        0.0f32..12.0,
        0.0f32..10.0,
        any::<bool>(),
    )
        .prop_map(|(geom_type, major, minor, height, is_small)| GeometryInfo {
            geom_type,
            is_small,
            height,
            major_radius: major,
            minor_radius: minor,
        })
}

/// A position in the corpus-scale world, z kept in the gate's interesting
/// band.
fn arb_position() -> impl Strategy<Value = Coord3> {
    (-100.0f32..100.0, -100.0f32..100.0, -25.0f32..25.0).prop_map(|(x, y, z)| Coord3 { x, y, z })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// The boolean result is symmetric over the pair swap: the dispatch
    /// `(this, that)` and `(that, this)` pick mirrored table entries that
    /// agree — the swapped composites re-run the same xy/z logic.
    #[test]
    fn collide_result_is_symmetric(
        geom_a in arb_geometry(),
        geom_b in arb_geometry(),
        pos_a in arb_position(),
        pos_b in arb_position(),
        angle_a in -3.2f32..3.2,
        angle_b in -3.2f32..3.2,
    ) {
        let a = CollideInfo::new(pos_a, geom_a, angle_a);
        let b = CollideInfo::new(pos_b, geom_b, angle_b);

        let ia = geom_a.geom_type.bits() as usize;
        let ib = geom_b.geom_type.bits() as usize;
        let ab = COLLIDE_TEST_PROCS[ia * 3 + ib](&a, &b, None);
        let ba = COLLIDE_TEST_PROCS[ib * 3 + ia](&b, &a, None);
        prop_assert_eq!(ab, ba);
    }

    /// When a proc writes the cinfo and the positions differ, the normal
    /// is a unit vector ("collideNormal is guaranteed to be a unit
    /// vector" — the `closeEnough` preflight's unwritten normal is the
    /// exception the golden corpora pin, so this checks only procs whose
    /// true result went through the xy path — the preflight's loc/normal
    /// is exactly zero, distinguishable by the zero loc.x/y).
    #[test]
    fn written_normal_is_unit_length(
        geom_a in arb_geometry(),
        geom_b in arb_geometry(),
        pos_a in arb_position(),
        pos_b in arb_position(),
        angle_a in -3.2f32..3.2,
        angle_b in -3.2f32..3.2,
    ) {
        // distinct positions (the normalize zero-guard's domain)
        prop_assume!(pos_a.x != pos_b.x || pos_a.y != pos_b.y || pos_a.z != pos_b.z);

        let a = CollideInfo::new(pos_a, geom_a, angle_a);
        let b = CollideInfo::new(pos_b, geom_b, angle_b);
        let mut cinfo = CollideLocAndNormal::default();
        let ia = geom_a.geom_type.bits() as usize;
        let ib = geom_b.geom_type.bits() as usize;
        let res = COLLIDE_TEST_PROCS[ia * 3 + ib](&a, &b, Some(&mut cinfo));
        if !res {
            return Ok(());
        }
        // the closeEnough preflight leaves loc.x/y and the normal unwritten
        // (all zero) — its signature; skip it
        if cinfo.loc.x == 0.0 && cinfo.loc.y == 0.0 && cinfo.normal.x == 0.0 && cinfo.normal.y == 0.0 {
            return Ok(());
        }
        let len_sq = cinfo.normal.x * cinfo.normal.x
            + cinfo.normal.y * cinfo.normal.y
            + cinfo.normal.z * cinfo.normal.z;
        prop_assert!((len_sq - 1.0).abs() < 1e-5, "normal not unit: {}", len_sq);
    }
}

#[test]
fn geom_gate_ignores_the_below_extents() {
    // two spheres r=10 at z=5 and z=-6, xy 3 apart: `collidesWith`'s
    // gate passes (thisBot=-5 <= thatTop=4) — the golden corpus pins the
    // asymmetry — while geomCollidesWithGeom's raw-z gate refuses
    // (this.z=5 > thatTop=4), in both orders
    let sph = GeometryInfo {
        geom_type: GeometryType::Sphere,
        is_small: false,
        height: 10.0,
        major_radius: 10.0,
        minor_radius: 10.0,
    };
    let p1 = Coord3 {
        x: 10.0,
        y: 10.0,
        z: 5.0,
    };
    let p2 = Coord3 {
        x: 13.0,
        y: 10.0,
        z: -6.0,
    };
    assert!(!geom_collides_with_geom(&p1, &sph, 0.0, &p2, &sph, 0.0));
    assert!(!geom_collides_with_geom(&p2, &sph, 0.0, &p1, &sph, 0.0));
    // one step closer in z and the raw-z gate passes again (the xy test
    // decides): z=-4 gives thatTop=6 >= this.z=5
    let p3 = Coord3 {
        x: 13.0,
        y: 10.0,
        z: -4.0,
    };
    assert!(geom_collides_with_geom(&p1, &sph, 0.0, &p3, &sph, 0.0));
}

#[test]
fn the_height_helpers_match_the_shapes() {
    // the sphere's extents ride the radius; the grounded shapes extend
    // up by the height and not below
    let sph = GeometryInfo {
        geom_type: GeometryType::Sphere,
        is_small: false,
        height: 3.0,
        major_radius: 7.0,
        minor_radius: 7.0,
    };
    let cyl = GeometryInfo {
        geom_type: GeometryType::Cylinder,
        is_small: false,
        height: 3.0,
        major_radius: 7.0,
        minor_radius: 7.0,
    };
    let bx = GeometryInfo {
        geom_type: GeometryType::Box,
        is_small: false,
        height: 3.0,
        major_radius: 7.0,
        minor_radius: 5.0,
    };
    assert_eq!(sph.max_height_above_position(), 7.0);
    assert_eq!(sph.max_height_below_position(), 7.0);
    assert_eq!(cyl.max_height_above_position(), 3.0);
    assert_eq!(cyl.max_height_below_position(), 0.0);
    assert_eq!(bx.max_height_above_position(), 3.0);
    assert_eq!(bx.max_height_below_position(), 0.0);
}
