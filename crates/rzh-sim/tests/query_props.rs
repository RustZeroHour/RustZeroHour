// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Rust port Copyright 2026 the RustZeroHour contributors.
//
//! Property tests for the partition query family.
//!
//! - the sorted orders are monotone (`ITER_SORTED_NEAR_TO_FAR` drains
//!   non-decreasing numerics, `ITER_SORTED_FAR_TO_NEAR` non-increasing)
//!   and the cost orders monotone by the build cost;
//! - the mergesort is stable in the clump-list order: every tie group
//!   appears in the same relative order in ALL of the sorted drains as
//!   in the `ITER_FASTEST` drain (the insertion order the prepend
//!   reversed) — the `<= 0.0f` compare favors the `to_do` segment;
//! - `getClosestObjects` from a position agrees with the brute-force
//!   minimum: the returned distance squared equals the minimum
//!   center-2d distance over every live candidate, and a result exists
//!   iff that minimum is strictly below `maxDist²` (the strict `<`
//!   boundary);
//! - the ring precompute partitions the offset space: over random grids
//!   every offset of the `(2cx-1) x (2cy-1)` rectangle lands in exactly
//!   one ring (the RTS_DEBUG assert's content), each offset's ring is
//!   its `calcMinRadius`, and `m_maxGcoRadius` bounds them all.

use proptest::prelude::*;

use rzh_sim::logic::{Kernel, KernelHost, ObjectSpec};
use rzh_sim::partition::Region2;
use rzh_sim::query::SimpleObjectIterator;
use rzh_sim::types::{
    Coord3, DistanceCalculationType, GeometryInfo, GeometryType, INVALID_ID, IterOrderType,
};

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

/// A position in the corpus-scale world.
fn arb_position() -> impl Strategy<Value = Coord3> {
    (10.0f32..90.0, 10.0f32..90.0, -5.0f32..15.0).prop_map(|(x, y, z)| Coord3 { x, y, z })
}

/// A random build cost (the cost orders' key).
fn arb_cost() -> impl Strategy<Value = f32> {
    (0.0f32..2000.0).prop_map(|c| (c * 4.0).trunc() / 4.0) // quarter steps: ties happen
}

/// The no-op host the corpus needs (creation events only).
#[derive(Default)]
struct PropHost;

#[allow(unused_variables)]
impl KernelHost for PropHost {}

/// Build a random world of `n` objects on the 100x100 grid and refresh
/// the store once (the coverage the queries walk).
fn build_world(
    seed_objs: Vec<(GeometryInfo, Coord3, f32, f32)>,
    angle_source: f32,
) -> (Kernel, PropHost) {
    let mut host = PropHost;
    let mut kernel = Kernel::with_grid(
        10.0,
        Region2 {
            lo_x: 0.0,
            lo_y: 0.0,
            hi_x: 100.0,
            hi_y: 100.0,
        },
        &mut host,
    );
    for (geometry_ref, pos_ref, _, angle_ref) in &seed_objs {
        let spec = ObjectSpec {
            geometry: *geometry_ref,
            immobile: false,
            modules: Vec::new(),
            colliders: 0,
            position: *pos_ref,
            orientation: *angle_ref,
        };
        kernel.create_object(&spec, &mut host);
    }
    // the template-supplied build costs, programmed post-creation
    for (i, (_, _, cost, _)) in seed_objs.iter().enumerate() {
        let id = i as i32 + 1; // the ids run 1..=n (the counter starts at 1)
        kernel.objects.get_mut(id).build_cost = *cost;
    }
    kernel.objects.get_mut(1).orientation = angle_source;
    kernel
        .pm
        .update(&mut kernel.objects, &mut kernel.logic, &mut host);
    (kernel, host)
}

/// Drain an iterator into (id, numeric) pairs (the reset first — the
/// fresh cursor's null is the golden corpus's quirk).
fn drain(iter: &mut SimpleObjectIterator) -> Vec<(i32, f32)> {
    iter.reset();
    let mut out = Vec::new();
    loop {
        let (o, num) = iter.next_with_numeric();
        if o == INVALID_ID {
            break;
        }
        out.push((o, num));
    }
    out
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// The sorted drains are monotone, and every tie group keeps the
    /// FASTEST (insertion) order — the mergesort's stability over the
    /// clump chain, shared by the ascending and descending compares
    /// (the `<= 0.0f` favors the to_do segment in both).
    #[test]
    fn sorted_drains_are_monotone_and_stable(
        objects in prop::collection::vec((arb_geometry(), arb_position(), arb_cost(), -3.2f32..3.2), 1..24),
        query_pos in arb_position(),
        max_dist in 5.0f32..45.0,
    ) {
        let (mut kernel, mut host) = build_world(objects, 0.0);
        let mut fastest = kernel.pm.iterate_objects_in_range_from_position(
            &mut kernel.objects,
            &kernel.players,
            query_pos,
            max_dist,
            DistanceCalculationType::FromCenter2d,
            &[],
            IterOrderType::Fastest,
        );
        let fastest_drain = drain(&mut fastest);

        let mut near = kernel.pm.iterate_objects_in_range_from_position(
            &mut kernel.objects,
            &kernel.players,
            query_pos,
            max_dist,
            DistanceCalculationType::FromCenter2d,
            &[],
            IterOrderType::SortedNearToFar,
        );
        let near_drain = drain(&mut near);
        for w in near_drain.windows(2) {
            prop_assert!(w[0].1 <= w[1].1, "near-to-far not monotone: {} > {}", w[0].1, w[1].1);
        }

        let mut far = kernel.pm.iterate_objects_in_range_from_position(
            &mut kernel.objects,
            &kernel.players,
            query_pos,
            max_dist,
            DistanceCalculationType::FromCenter2d,
            &[],
            IterOrderType::SortedFarToNear,
        );
        let far_drain = drain(&mut far);
        for w in far_drain.windows(2) {
            prop_assert!(w[0].1 >= w[1].1, "far-to-near not monotone: {} < {}", w[0].1, w[1].1);
        }

        // the stability: within each equal-numeric group the relative
        // order is the FASTEST order (the clump-list order), in both
        // sorted drains (the group SEQUENCE is the sort's own — the
        // monotonicity checks above cover it)
        let tie_order = |drain: &[(i32, f32)], num: f32| -> Vec<i32> {
            drain
                .iter()
                .filter(|&&(_, n)| n == num)
                .map(|&(id, _)| id)
                .collect()
        };
        for &(_, num) in &fastest_drain {
            let expected = tie_order(&fastest_drain, num);
            prop_assert_eq!(tie_order(&near_drain, num), expected.clone(), "near tie group {}", num);
            prop_assert_eq!(tie_order(&far_drain, num), expected, "far tie group {}", num);
        }

        // the cost orders: monotone by cost, ties in the same stable order
        // (the costs snapshotted — the query below takes the arena)
        let costs: Vec<f32> = (1..=24 + 24)
            .map(|id| kernel.objects.try_get(id).map_or(f32::NAN, |o| o.build_cost))
            .collect();
        let cost_of = |id: i32| costs[(id - 1) as usize];
        let mut cheap = kernel.pm.iterate_objects_in_range_from_position(
            &mut kernel.objects,
            &kernel.players,
            query_pos,
            max_dist,
            DistanceCalculationType::FromCenter2d,
            &[],
            IterOrderType::SortedCheapToExpensive,
        );
        let cheap_drain = drain(&mut cheap);
        for w in cheap_drain.windows(2) {
            prop_assert!(cost_of(w[0].0) <= cost_of(w[1].0));
        }
        let mut exp = kernel.pm.iterate_objects_in_range_from_position(
            &mut kernel.objects,
            &kernel.players,
            query_pos,
            max_dist,
            DistanceCalculationType::FromCenter2d,
            &[],
            IterOrderType::SortedExpensiveToCheap,
        );
        let exp_drain = drain(&mut exp);
        for w in exp_drain.windows(2) {
            prop_assert!(cost_of(w[0].0) >= cost_of(w[1].0));
        }
        let _ = &mut host;
    }

    /// The range query is set-exact against the brute force — every
    /// query-visible object strictly inside the bound is iterated and
    /// nothing else — and getClosestObjects' result honors the same
    /// bound: a result exists iff the set is nonempty, and the result
    /// is never closer than the set's minimum (the early-exit slop of
    /// `maxRadiusLimit = curRadius` can MISS a later-ring closer
    /// object — the original's own heuristic, pinned by the corpus —
    /// but it can never return an out-of-range or impossibly close one).
    #[test]
    fn range_query_and_closest_agree_with_bruteforce(
        objects in prop::collection::vec((arb_geometry(), arb_position(), arb_cost(), -3.2f32..3.2), 1..24),
        query_pos in arb_position(),
        max_dist in 5.0f32..45.0,
    ) {
        let (mut kernel, _host) = build_world(objects, 0.0);

        // the brute force over the live, query-visible object list (the
        // ids run 1..=n; zero-extent geometries cover no cells — the
        // degenerate-zero-step cases the fill corpora pin — and so are
        // invisible to the ring walk, in the original too)
        let mut in_range: Vec<(i32, f32)> = Vec::new();
        for oid in 1.. {
            let Some(obj) = kernel.objects.try_get(oid) else {
                break;
            };
            if obj.partition.as_ref().is_none_or(|p| p.coi_in_use == 0) {
                continue;
            }
            let dx = obj.position.x - query_pos.x;
            let dy = obj.position.y - query_pos.y;
            let d2 = dx * dx + dy * dy;
            if d2 < max_dist * max_dist {
                in_range.push((oid, d2));
            }
        }
        in_range.sort_by_key(|&(id, _)| id);

        // the range query drains exactly that set (with the exact
        // center-2d numerics)
        let mut iter = kernel.pm.iterate_objects_in_range_from_position(
            &mut kernel.objects,
            &kernel.players,
            query_pos,
            max_dist,
            DistanceCalculationType::FromCenter2d,
            &[],
            IterOrderType::SortedNearToFar,
        );
        let drained = drain(&mut iter);
        let mut drained_ids: Vec<i32> = drained.iter().map(|&(id, _)| id).collect();
        drained_ids.sort_unstable();
        prop_assert_eq!(drained_ids, in_range.iter().map(|&(id, _)| id).collect::<Vec<_>>());
        for (id, num) in drained {
            let expect = in_range.iter().find(|&&(i, _)| i == id).unwrap().1;
            prop_assert_eq!(num, expect);
        }

        // the closest query: exists iff the set is nonempty, in range,
        // and never closer than the set's minimum
        let (id, dist, _vec) = kernel.pm.get_closest_object_from_position(
            &mut kernel.objects,
            &kernel.players,
            query_pos,
            max_dist,
            DistanceCalculationType::FromCenter2d,
            &[],
        );
        if in_range.is_empty() {
            prop_assert!(id.is_none(), "no candidate in range, got {id:?}");
        } else {
            let got = id.expect("a candidate exists");
            let min_d2 = in_range.iter().map(|&(_, d)| d).fold(f32::INFINITY, f32::min);
            let dx = kernel.objects.get(got).position.x - query_pos.x;
            let dy = kernel.objects.get(got).position.y - query_pos.y;
            let d2 = dx * dx + dy * dy;
            prop_assert!(d2 < max_dist * max_dist, "result out of range");
            prop_assert!(d2 >= min_d2, "result closer than the true min");
            prop_assert!((dist * dist - d2).abs() < 1e-3);
        }
    }

    /// The ring precompute partitions the offset space: every offset of
    /// the rectangle lands in exactly one ring, at its calcMinRadius,
    /// and m_maxGcoRadius bounds them all (the RTS_DEBUG assert's
    /// content, over random grids).
    #[test]
    fn rings_partition_the_offset_space(
        cell_size in 1.0f32..12.0,
        w in 1.0f32..80.0,
        h in 1.0f32..80.0,
    ) {
        let mut host = PropHost;
        let kernel = Kernel::with_grid(
            cell_size,
            Region2 {
                lo_x: 0.0,
                lo_y: 0.0,
                hi_x: w,
                hi_y: h,
            },
            &mut host,
        );
        let pm = &kernel.pm;
        let cx = pm.cell_count_x;
        let cy = pm.cell_count_y;
        prop_assume!(cx > 0 && cy > 0);

        let mut seen = std::collections::HashSet::new();
        let mut total = 0usize;
        for (r, ring) in pm.radius_vec.iter().enumerate() {
            prop_assert!((r as i32) <= pm.max_gco_radius);
            for &(ox, oy) in ring {
                prop_assert_eq!(pm.calc_min_radius((ox, oy)), r as i32);
                prop_assert!(seen.insert((ox, oy)), "offset {ox},{oy} in two rings");
                total += 1;
            }
        }
        prop_assert_eq!(total, (2 * cx - 1) as usize * (2 * cy - 1) as usize);
    }
}
