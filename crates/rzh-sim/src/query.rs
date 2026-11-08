//! The partition query family — the spatial queries the AI and weapon
//! targeting consume (plan §4 WP1's third slice).
//!
//! Origin: `PartitionManager.cpp` at baseline 571a7c5, the FASTER_GCO
//! build (the retail `PartitionManager.h` defines it) — `calcMinRadius`
//! (the 4×4 corner-pair minimum over the cell corners, the
//! promoted-square-root and the CEIL cell radius) and `calcRadiusVec`
//! (the `m_maxGcoRadius` bound from the map diagonal, the per-radius
//! offset rings in the y-outer/x-inner scan order); the four
//! `distCalcProc_X_Y` distance measures (center/center and
//! boundary/boundary in 2d and 3d — the boundary pair's
//! bounding-circle/sphere shrink with its clamped-to-zero branch, the 3d
//! z delta through [`GeometryInfo::z_delta_to_center_position`]) and
//! `theDistCalcProcs`; `getClosestObjects` (the ring walk with the
//! done-flag dedup, the `closestDistSqr` tightening, the
//! insert-vs-closest split, the `maxRadiusLimit = curRadius` early exit,
//! the non-retail `closestVec` prefill of `maxDist³`), the
//! `getClosestObject` / `iterateObjectsInRange` / `iteratePotentialCollisions`
//! (the bounding-sphere 1.1 slop and the would-collide filter) /
//! `iterateAllObjects` wrappers, and the `getVectorTo` /
//! `getDistanceSquared` / `getGoalDistanceSquared` / `getRelativeAngle2D`
//! measure entries — over `SimpleObjectIterator.cpp` (the clump list
//! with its prepend insert, `nextWithNumeric`, `reset`, `makeEmpty` and
//! the bottom-up linked-list mergesort of `sort()` with its five compare
//! procs), `ObjectIter.h` (`IterOrderType`), the `PartitionFilter` base
//! with `filtersAllow` and the wouldCollide / acceptByKindOf /
//! rejectByKindOf / alive / onMap / sameMapStatus filters, and
//! `Thing.cpp`'s `getUnitDirectionVector2D` (Cos/Sin of the facing
//! angle).
//!
//! Stated deviations, behaviour-preserving (plan §3.3 rule 2): the
//! original's `SimpleObjectIterator` is a `MemoryPoolObject` the caller
//! news and deletes (the `MemoryPoolObjectHolder` dance of the iterate
//! wrappers) — the port returns it by value, the pointer never being
//! observable; `getClosestObjects`' function-static `theIterFlag` (the
//! per-call generation counter the done-flag dedup keys on) is manager
//! state ([`PartitionManager::iter_flag`]); the cost-sorted iterators'
//! build cost is a plain [`Object::build_cost`] field (the template that
//! carries it is the rzh-data object model's). The players/teams/
//! locomotors filters (SamePlayer, Relationship, Team, Squad,
//! LineOfSight, StealthedAndUndetected, IsFlying, ...) read state the
//! later slices own and arrive with them; the filter *framework* and
//! every filter over state this slice owns are here.
//!
//! The float paths run on the pinned GameMath trig (`rzh_math::gmath`)
//! and the correctly-rounded sqrt (`rzh_math::scalar`) exactly as the
//! deterministic build does.
//!
//! Original: Copyright 2025 Electronic Arts Inc.
//! Rust port Copyright 2026 the RustZeroHour contributors.

use rzh_math::gmath::acosf;
use rzh_math::scalar::sqrt;

use crate::collide::{COLLIDE_TEST_PROCS, CollideInfo, CollideLocAndNormal};
use crate::fastmath::real_to_int_ceil;
use crate::logic::{Object, Objects};
use crate::partition::PartitionManager;
use crate::types::{
    Coord3, DistanceCalculationType, GeometryInfo, HUGE_DIST, HUGE_DIST_SQR, INVALID_ID,
    IterOrderType, ObjectID,
};

/// `DistCalcProc` (PartitionManager.cpp): the measure signature — the
/// pair's positions and optional objects, and the rejection bound. The
/// `(bool, f32, Coord3)` return is the original's `bool` plus its
/// `abDistSqr` / `abVec` reference outs (only read on a `true`).
pub type DistCalcProc =
    fn(&Coord3, Option<&Object>, &Coord3, Option<&Object>, f32) -> (bool, f32, Coord3);

/// `distCalcProc_CenterAndCenter_2D`: the xy center distance, z zeroed.
fn dist_calc_proc_center_and_center_2d(
    pos_a: &Coord3,
    obj_a: Option<&Object>,
    pos_b: &Coord3,
    obj_b: Option<&Object>,
    max_dist_sqr: f32,
) -> (bool, f32, Coord3) {
    let _ = (obj_a, obj_b);
    // note that object positions are defined as the bottom center of the geometry,
    // thus we must add the radius to the z coord to get the proper center of the bounding sphere.
    let mut diff = Coord3 {
        x: pos_b.x - pos_a.x,
        y: pos_b.y - pos_a.y,
        z: 0.0,
    };

    let ab_dist_sqr = diff.x * diff.x + diff.y * diff.y;
    diff.z = 0.0; // (already zero — the original writes the full vec back)
    let ab_vec = diff;

    (ab_dist_sqr < max_dist_sqr, ab_dist_sqr, ab_vec)
}

/// `distCalcProc_BoundaryAndBoundary_2D`: the xy distance shrunk by the
/// pair's bounding-circle radii — a side without an object contributes
/// no radius — with the clamped-to-zero branch ("sorry, distances can't
/// be negative").
fn dist_calc_proc_boundary_and_boundary_2d(
    pos_a: &Coord3,
    obj_a: Option<&Object>,
    pos_b: &Coord3,
    obj_b: Option<&Object>,
    max_dist_sqr: f32,
) -> (bool, f32, Coord3) {
    let mut diff = Coord3 {
        x: pos_b.x - pos_a.x,
        y: pos_b.y - pos_a.y,
        z: 0.0,
    };

    let actual_dist_sqr = diff.x * diff.x + diff.y * diff.y;

    let mut shrink_factor = 1.0;
    let mut shrunken_dist_sqr = actual_dist_sqr;
    let total_rad = obj_a.map_or(0.0, |o| o.geometry.bounding_circle_radius())
        + obj_b.map_or(0.0, |o| o.geometry.bounding_circle_radius());

    if total_rad > 0.0 {
        let actual_dist = sqrt(actual_dist_sqr);
        let shrunken_dist = actual_dist - total_rad;
        if shrunken_dist <= 0.0 {
            shrink_factor = 0.0;
            shrunken_dist_sqr = 0.0; // sorry, distances can't be negative
        } else {
            shrink_factor = shrunken_dist / actual_dist;
            shrunken_dist_sqr = shrunken_dist * shrunken_dist;
        }
    }

    // (the original's DEBUG_ASSERTCRASH on shrinkFactor outside [0,1] is
    // debug-only — dropped)
    diff.x *= shrink_factor;
    diff.y *= shrink_factor;
    (shrunken_dist_sqr < max_dist_sqr, shrunken_dist_sqr, diff)
}

/// `distCalcProc_CenterAndCenter_3D`: the full xyz center distance.
fn dist_calc_proc_center_and_center_3d(
    pos_a: &Coord3,
    obj_a: Option<&Object>,
    pos_b: &Coord3,
    obj_b: Option<&Object>,
    max_dist_sqr: f32,
) -> (bool, f32, Coord3) {
    let _ = (obj_a, obj_b);
    // note that object positions are defined as the bottom center of the geometry,
    // thus we must add the radius to the z coord to get the proper center of the bounding sphere.
    let diff = Coord3 {
        x: pos_b.x - pos_a.x,
        y: pos_b.y - pos_a.y,
        z: pos_b.z - pos_a.z,
    };

    let ab_dist_sqr = diff.x * diff.x + diff.y * diff.y + diff.z * diff.z;

    (ab_dist_sqr < max_dist_sqr, ab_dist_sqr, diff)
}

/// `distCalcProc_BoundaryAndBoundary_3D`: the full 3d distance between
/// the geometry centers (the z through the
/// [`GeometryInfo::z_delta_to_center_position`] half-heights) shrunk by
/// the pair's bounding-sphere radii.
fn dist_calc_proc_boundary_and_boundary_3d(
    pos_a: &Coord3,
    obj_a: Option<&Object>,
    pos_b: &Coord3,
    obj_b: Option<&Object>,
    max_dist_sqr: f32,
) -> (bool, f32, Coord3) {
    let geom_a = obj_a.map(|o| &o.geometry);
    let geom_b = obj_b.map(|o| &o.geometry);

    // note that object positions are defined as the bottom center of the geometry,
    // thus we must add the radius to the z coord to get the proper center of the bounding sphere.
    let mut diff = Coord3 {
        x: pos_b.x - pos_a.x,
        y: pos_b.y - pos_a.y,
        z: (pos_b.z + geom_b.map_or(0.0, |g| g.z_delta_to_center_position()))
            - (pos_a.z + geom_a.map_or(0.0, |g| g.z_delta_to_center_position())),
    };

    let actual_dist_sqr = diff.x * diff.x + diff.y * diff.y + diff.z * diff.z;

    let mut shrink_factor = 1.0;
    let mut shrunken_dist_sqr = actual_dist_sqr;
    let total_rad = geom_a.map_or(0.0, |g| g.bounding_sphere_radius())
        + geom_b.map_or(0.0, |g| g.bounding_sphere_radius());
    if total_rad > 0.0 {
        let actual_dist = sqrt(actual_dist_sqr);
        let shrunken_dist = actual_dist - total_rad;
        if shrunken_dist <= 0.0 {
            shrink_factor = 0.0;
            shrunken_dist_sqr = 0.0; // sorry, distances can't be negative
        } else {
            shrink_factor = shrunken_dist / actual_dist;
            shrunken_dist_sqr = shrunken_dist * shrunken_dist;
        }
    }

    diff.x *= shrink_factor;
    diff.y *= shrink_factor;
    diff.z *= shrink_factor;
    (shrunken_dist_sqr < max_dist_sqr, shrunken_dist_sqr, diff)
}

/// `theDistCalcProcs` (PartitionManager.cpp), indexed by the
/// `DistanceCalculationType` ordinals.
pub const THE_DIST_CALC_PROCS: [DistCalcProc; 4] = [
    dist_calc_proc_center_and_center_2d,
    dist_calc_proc_center_and_center_3d,
    dist_calc_proc_boundary_and_boundary_2d,
    dist_calc_proc_boundary_and_boundary_3d,
];

// --- SimpleObjectIterator (SimpleObjectIterator.cpp). ------------------------

/// `SimpleObjectIterator::Clump`: one entry — the object and its numeric
/// (typically, dist-squared).
#[derive(Debug)]
struct Clump {
    /// `m_obj`.
    obj: ObjectID,
    /// `m_numeric`.
    numeric: f32,
    /// `m_nextClump` (the node index chain; the arena never frees
    /// mid-life).
    next: Option<u32>,
}

/// `SimpleObjectIterator` (SimpleObjectIterator.cpp / ObjectIter.h): the
/// clump list the range queries fill. `insert` prepends (so the drained
/// order is the reverse of the insertion order), `next_with_numeric`
/// walks from the cursor, and `sort` re-links the chain through the
/// bottom-up mergesort — the exact splice choreography decides the order
/// of equal numerics (the tie order the corpora pin). A fresh iterator's
/// cursor is empty: the original's callers must `first()`/`reset()` before
/// draining (the sorted orders return post-sort cursors; `ITER_FASTEST`
/// does not sort and so does not).
#[derive(Debug, Default)]
pub struct SimpleObjectIterator {
    /// The node arena (`newInstance(Clump)`).
    clumps: Vec<Clump>,
    /// `m_firstClump`.
    first: Option<u32>,
    /// `m_curClump`.
    cur: Option<u32>,
    /// `m_clumpCount`.
    count: usize,
}

impl SimpleObjectIterator {
    /// A fresh, empty iterator (the `newInstance(SimpleObjectIterator)`).
    pub fn new() -> Self {
        Self::default()
    }

    /// `insert(obj, numeric)`: the prepend.
    pub fn insert(&mut self, obj: ObjectID, numeric: f32) {
        let idx = self.clumps.len() as u32;
        self.clumps.push(Clump {
            obj,
            numeric,
            next: self.first,
        });
        self.first = Some(idx);

        self.count += 1;
    }

    /// `nextWithNumeric(num)`: the cursor's entry and numeric, or
    /// `(INVALID_ID, 0.0)` at exhaustion (the original zeroes the out
    /// first).
    pub fn next_with_numeric(&mut self) -> (ObjectID, f32) {
        let mut num = 0.0;
        let mut obj = INVALID_ID;

        if let Some(c) = self.cur {
            obj = self.clumps[c as usize].obj;
            num = self.clumps[c as usize].numeric;
            self.cur = self.clumps[c as usize].next;
        }

        (obj, num)
    }

    /// `reset()`: the cursor back to the head.
    pub fn reset(&mut self) {
        self.cur = self.first;
    }

    /// `makeEmpty()`.
    pub fn make_empty(&mut self) {
        self.clumps.clear();
        self.first = None;
        self.cur = None;
        self.count = 0;
    }

    /// `m_clumpCount`.
    #[must_use]
    pub fn count(&self) -> usize {
        self.count
    }

    /// `sort(order)`: the bottom-up linked-list mergesort over the
    /// clump chain, then the reset ("always reset after sorting, to
    /// prevent weirdness"). `ITER_FASTEST` has no compare proc — "my,
    /// that was easy" — and returns without touching the cursor.
    pub fn sort(&mut self, objects: &Objects, order: IterOrderType) {
        if self.count == 0 {
            return;
        }

        let Some(cmp_proc) = the_clump_compare_procs(order) else {
            return; // my, that was easy
        };

        // do a basic mergesort, which works nicely for linked lists,
        // and is reasonably efficient (N log N).

        let mut n: usize = 1;
        loop {
            let mut to_do = self.first;
            let mut tail: Option<u32> = None;
            self.first = None;

            let mut merge_count = 0;

            while let Some(head) = to_do {
                merge_count += 1;

                let mut to_do_idx: Option<u32> = Some(head);
                let mut to_do_count: i64 = 0;

                // make two lists of length 'n' (to_do is one, sub is the other)
                let mut sub: Option<u32> = Some(head);
                for _ in 0..n {
                    to_do_count += 1;
                    sub = self.clumps[sub.unwrap() as usize].next;
                    if sub.is_none() {
                        break;
                    }
                }
                let mut sub_count: i64 = if sub.is_some() { n as i64 } else { 0 };

                // merge the two lists.
                while to_do_count + sub_count > 0 {
                    let tmp: Option<u32>;

                    // bleah, coalesce into more elegant test case
                    if sub_count == 0 {
                        tmp = to_do_idx;
                        to_do_idx = self.clumps[to_do_idx.unwrap() as usize].next;
                        to_do_count -= 1;
                    } else if to_do_count == 0 {
                        tmp = sub;
                        sub = self.clumps[sub.unwrap() as usize].next;
                        sub_count -= 1;
                    } else if cmp_proc(
                        objects,
                        &self.clumps[to_do_idx.unwrap() as usize],
                        &self.clumps[sub.unwrap() as usize],
                    ) <= 0.0
                    {
                        tmp = to_do_idx;
                        to_do_idx = self.clumps[to_do_idx.unwrap() as usize].next;
                        to_do_count -= 1;
                    } else {
                        tmp = sub;
                        sub = self.clumps[sub.unwrap() as usize].next;
                        sub_count -= 1;
                    }
                    if sub.is_none() {
                        sub_count = 0;
                    }
                    if to_do_idx.is_none() {
                        to_do_count = 0;
                    }

                    match tail {
                        Some(t) => self.clumps[t as usize].next = tmp,
                        None => self.first = tmp,
                    }
                    tail = tmp;
                }

                to_do = sub;
            }
            if let Some(t) = tail {
                self.clumps[t as usize].next = None;
            }

            if merge_count <= 1 {
                // when we have done just one (or none) swap, we're done
                break;
            }
            n *= 2;
        }

        // always reset after sorting, to prevent weirdness
        self.reset();
    }
}

/// `ClumpCompareProc` (ObjectIter.h): the sort's compare, returning the
/// signed difference the `<= 0.0f` test reads.
type ClumpCompareProc = fn(&Objects, &Clump, &Clump) -> f32;

/// `sortNearToFar`.
fn sort_near_to_far(_objects: &Objects, a: &Clump, b: &Clump) -> f32 {
    a.numeric - b.numeric
}

/// `sortFarToNear`.
fn sort_far_to_near(_objects: &Objects, a: &Clump, b: &Clump) -> f32 {
    b.numeric - a.numeric
}

/// `sortCheapToExpensive` — `m_obj->getTemplate()->friend_getBuildCost()`
/// (the template's cost as the object carries it).
fn sort_cheap_to_expensive(objects: &Objects, a: &Clump, b: &Clump) -> f32 {
    objects.get(a.obj).build_cost - objects.get(b.obj).build_cost
}

/// `sortExpensiveToCheap`.
fn sort_expensive_to_cheap(objects: &Objects, a: &Clump, b: &Clump) -> f32 {
    objects.get(b.obj).build_cost - objects.get(a.obj).build_cost
}

/// `theClumpCompareProcs` (SimpleObjectIterator.cpp): the compare per
/// order — `None` for `ITER_FASTEST` ("fastest" gets no proc).
fn the_clump_compare_procs(order: IterOrderType) -> Option<ClumpCompareProc> {
    match order {
        IterOrderType::Fastest => None,
        IterOrderType::SortedNearToFar => Some(sort_near_to_far),
        IterOrderType::SortedFarToNear => Some(sort_far_to_near),
        IterOrderType::SortedCheapToExpensive => Some(sort_cheap_to_expensive),
        IterOrderType::SortedExpensiveToCheap => Some(sort_expensive_to_cheap),
    }
}

// --- the partition filters (PartitionManager.h / PartitionManager.cpp). ------

/// `PartitionFilter` (PartitionManager.h): the query family's acceptance
/// test — any object a filter refuses is not iterated. Either the query
/// source object or the query position is the context; the filter sees
/// the candidate (the original's virtual hands the `Object*`; the port
/// hands the arena plus the id so the filters reading *other* objects
/// (same-map-status) can).
pub trait PartitionFilter {
    /// `allow(objOther)`.
    fn allow(&self, objects: &Objects, obj_other: ObjectID) -> bool;
}

/// `filtersAllow` (PartitionManager.cpp, the non-profiling build): every
/// filter must allow, no filter must reject.
#[must_use]
pub fn filters_allow(
    filters: &[&dyn PartitionFilter],
    objects: &Objects,
    obj_other: ObjectID,
) -> bool {
    for filter in filters {
        if !filter.allow(objects, obj_other) {
            return false;
        }
    }
    // assume true if no filters rejected it
    true
}

/// `PartitionFilterWouldCollide` (PartitionManager.cpp): the inline
/// `geomCollidesWithGeom` body (the z-gate that ignores the
/// below-extents, the `theCollideTestProcs` dispatch) compared against
/// the desired result.
#[derive(Debug)]
pub struct PartitionFilterWouldCollide {
    /// `m_position`.
    pub position: Coord3,
    /// `m_geom`.
    pub geom: GeometryInfo,
    /// `m_angle`.
    pub angle: f32,
    /// `m_desiredCollisionResult` — the collision must match this for
    /// `allow` to return true.
    pub desired_collision_result: bool,
}

impl PartitionFilterWouldCollide {
    /// The `PartitionFilterWouldCollide(pos, geom, angle, desired)`
    /// constructor.
    pub fn new(position: Coord3, geom: GeometryInfo, angle: f32, desired: bool) -> Self {
        Self {
            position,
            geom,
            angle,
            desired_collision_result: desired,
        }
    }
}

impl PartitionFilter for PartitionFilterWouldCollide {
    fn allow(&self, objects: &Objects, obj_other: ObjectID) -> bool {
        let that = objects.get(obj_other);
        let this_info = CollideInfo::new(self.position, self.geom, self.angle);
        let that_info = CollideInfo::new(that.position, that.geometry, that.orientation);

        // invariant for all geometries: first do z collision check.
        let does_collide = if this_info.position.z + this_info.geom.max_height_above_position()
            >= that_info.position.z
            && this_info.position.z
                <= that_info.position.z + that_info.geom.max_height_above_position()
        {
            // NOTE: This assumes geometry enumerations that start at GEOMETRY_FIRST AND depends on the
            // order in which they appear in the enum list
            let collide_proc = COLLIDE_TEST_PROCS[(this_info.geom.geom_type.bits() as usize) * 3
                + (that_info.geom.geom_type.bits() as usize)];
            // (the original's uninitialized CollideLocAndNormal local — the
            // pinned zero; the value is never read)
            let mut cinfo = CollideLocAndNormal::default();
            collide_proc(&this_info, &that_info, Some(&mut cinfo))
        } else {
            // no z-intersection -> no collision.
            false
        };

        does_collide == self.desired_collision_result
    }
}

/// `PartitionFilterAcceptByKindOf`: every must-be-set kindof bit set and
/// every must-be-clear bit clear.
#[derive(Debug)]
pub struct PartitionFilterAcceptByKindOf {
    /// `m_mustBeSet`.
    pub must_be_set: u32,
    /// `m_mustBeClear`.
    pub must_be_clear: u32,
}

impl PartitionFilterAcceptByKindOf {
    /// The `PartitionFilterAcceptByKindOf(mustBeSet, mustBeClear)`
    /// constructor.
    pub fn new(must_be_set: u32, must_be_clear: u32) -> Self {
        Self {
            must_be_set,
            must_be_clear,
        }
    }
}

impl PartitionFilter for PartitionFilterAcceptByKindOf {
    fn allow(&self, objects: &Objects, obj_other: ObjectID) -> bool {
        objects
            .get(obj_other)
            .is_kind_of_multi(self.must_be_set, self.must_be_clear)
    }
}

/// `PartitionFilterRejectByKindOf`: the inverse of
/// [`PartitionFilterAcceptByKindOf`].
#[derive(Debug)]
pub struct PartitionFilterRejectByKindOf {
    /// `m_mustBeSet`.
    pub must_be_set: u32,
    /// `m_mustBeClear`.
    pub must_be_clear: u32,
}

impl PartitionFilterRejectByKindOf {
    /// The `PartitionFilterRejectByKindOf(mustBeSet, mustBeClear)`
    /// constructor.
    pub fn new(must_be_set: u32, must_be_clear: u32) -> Self {
        Self {
            must_be_set,
            must_be_clear,
        }
    }
}

impl PartitionFilter for PartitionFilterRejectByKindOf {
    fn allow(&self, objects: &Objects, obj_other: ObjectID) -> bool {
        !objects
            .get(obj_other)
            .is_kind_of_multi(self.must_be_set, self.must_be_clear)
    }
}

/// `PartitionFilterAlive`: reject the effectively dead.
#[derive(Debug, Default)]
pub struct PartitionFilterAlive;

impl PartitionFilter for PartitionFilterAlive {
    fn allow(&self, objects: &Objects, obj_other: ObjectID) -> bool {
        !objects.get(obj_other).is_effectively_dead()
    }
}

/// `PartitionFilterOnMap`: reject the off-map.
#[derive(Debug, Default)]
pub struct PartitionFilterOnMap;

impl PartitionFilter for PartitionFilterOnMap {
    fn allow(&self, objects: &Objects, obj_other: ObjectID) -> bool {
        !objects.get(obj_other).is_off_map()
    }
}

/// `PartitionFilterSameMapStatus`: only objects sharing `m_obj`'s map
/// status (the source read live, as the original's pointer does).
#[derive(Debug)]
pub struct PartitionFilterSameMapStatus {
    /// `m_obj` (the id the original's pointer carries).
    pub source: ObjectID,
}

impl PartitionFilterSameMapStatus {
    /// The `PartitionFilterSameMapStatus(obj)` constructor.
    pub fn new(source: ObjectID) -> Self {
        Self { source }
    }
}

impl PartitionFilter for PartitionFilterSameMapStatus {
    fn allow(&self, objects: &Objects, obj_other: ObjectID) -> bool {
        // objOther is guaranteed to be non-null, so we don't need to check (srj)
        objects.get(obj_other).is_off_map() == objects.get(self.source).is_off_map()
    }
}

// --- the FASTER_GCO ring precompute and the query family. ---------------------

impl PartitionManager {
    /// `calcMinRadius` (PartitionManager.cpp): the smallest distance two
    /// objects — one anywhere in cell (0,0), one anywhere in cell
    /// `cur` — could be apart, in integral multiples of the cell size.
    /// The corner-pair minimum over both cells' four corners (so even
    /// objects in the same cell can still be `cellSize * 1.414` apart,
    /// and objects in diagonally adjacent cells can be zero apart — the
    /// corner touch), the square root and the CEIL. `cur` is in cell
    /// units.
    #[must_use]
    pub fn calc_min_radius(&self, cur: (i32, i32)) -> i32 {
        let half_cell = self.cell_size * 0.5;
        let center_pos = [
            (-half_cell, -half_cell),
            (half_cell, -half_cell),
            (-half_cell, half_cell),
            (half_cell, half_cell),
        ];

        let x = cur.0 as f32 * self.cell_size;
        let y = cur.1 as f32 * self.cell_size;
        let other_pos = [
            (x - half_cell, y - half_cell),
            (x + half_cell, y - half_cell),
            (x - half_cell, y + half_cell),
            (x + half_cell, y + half_cell),
        ];

        // this is ugly and there's probably a faster way, but we only do this once per PM reset,
        // so it really shouldn't matter... (I hope)
        //
        // (RETAIL_COMPATIBLE_CRC is off in this build: Real, not double)
        let mut min_dist_sqr = 1e12_f32; // double, not real
        for &(cx, cy) in &center_pos {
            for &(ox, oy) in &other_pos {
                // double, not real
                let dx = cx - ox;
                let dy = cy - oy;
                let cur_dist_sqr = dx * dx + dy * dy;
                if min_dist_sqr > cur_dist_sqr {
                    min_dist_sqr = cur_dist_sqr;
                }
            }
        }

        // double, not real
        let dist = sqrt(min_dist_sqr);
        real_to_int_ceil(dist / self.cell_size)
    }

    /// `calcRadiusVec` (PartitionManager.cpp): the FASTER_GCO
    /// precompute — the `m_maxGcoRadius` bound from the map diagonal,
    /// then every offset of the `(2cx-1) x (2cy-1)` offset space dropped
    /// into its `calcMinRadius` ring, in the y-outer/x-inner scan order.
    /// `m_radiusVec[curRadius]` contains a list of the cells (foo) that
    /// could contain objects that are `<= (curRadius * cellSize)`
    /// distance away from cell (0,0).
    pub fn calc_radius_vec(&mut self) {
        let cell_size = self.cell_size;
        let cx = self.cell_count_x;
        let cy = self.cell_count_y;

        // (RETAIL_COMPATIBLE_CRC is off in this build: Real, not double)
        let dx = cx as f32 * cell_size;
        let dy = cy as f32 * cell_size;
        let max_possible_dist = sqrt(dx * dx + dy * dy);

        self.max_gco_radius = real_to_int_ceil(max_possible_dist / cell_size);

        self.radius_vec.clear();
        self.radius_vec
            .resize((self.max_gco_radius + 1) as usize, Vec::new());

        let mut cur_y = -cy + 1;
        while cur_y < cy {
            let mut cur_x = -cx + 1;
            while cur_x < cx {
                let cur_radius = self.calc_min_radius((cur_x, cur_y));
                // (the DEBUG_ASSERTCRASH on curRadius > m_maxGcoRadius
                // is debug-only — dropped)
                if cur_radius <= self.max_gco_radius {
                    self.radius_vec[cur_radius as usize].push((cur_x, cur_y));
                }
                cur_x += 1;
            }
            cur_y += 1;
        }
    }

    /// `getClosestObjects` (the FASTER_GCO branch): the ring walk. One
    /// of `obj` / `pos` must be given (the original's assert; the
    /// Options carry the original's null-or-value pointer args). With
    /// `iter_arg` every object inside `max_dist` is inserted (numerics
    /// attached); without it the closest is tracked — the walk stops
    /// with the ring that found it (`maxRadiusLimit = curRadius`) — and
    /// the first candidate at the minimal distance wins (the strict `<`
    /// refuses the later equals). Returns the closest id (or `None`).
    ///
    /// The pinned quirks: the rejection bound tightens with each closer
    /// find (but never when inserting); an exactly-`max_dist` candidate
    /// is refused; the no-match outputs carry `sqrt(maxDist²)` and the
    /// non-retail `closestVec` prefill of `maxDist³`.
    #[allow(clippy::too_many_arguments)]
    pub fn get_closest_objects(
        &mut self,
        objects: &mut Objects,
        obj: Option<ObjectID>,
        pos: Option<Coord3>,
        max_dist: f32,
        dc: DistanceCalculationType,
        filters: &[&dyn PartitionFilter],
        mut iter_arg: Option<&mut SimpleObjectIterator>,
        closest_dist: Option<&mut f32>,
        closest_vec: Option<&mut Coord3>,
    ) -> Option<ObjectID> {
        // (the entrancy assert is RTS_DEBUG-only — dropped)
        debug_assert!(
            (obj.is_some()) != (pos.is_some()),
            "either obj or pos must be null"
        );

        let dist_proc = THE_DIST_CALC_PROCS[dc.bits() as usize];

        let (obj_pos, obj_to_use) = if let Some(pos) = pos {
            (pos, None)
        } else {
            let obj = obj.expect("either obj or pos");
            (objects.get(obj).position, Some(obj))
        };
        let (cell_center_x, cell_center_y) = self.world_to_cell(obj_pos.x, obj_pos.y);

        let mut closest_obj: Option<ObjectID> = None;
        let mut closest_dist_sqr = max_dist * max_dist; // if it's not closer than this, we shouldn't consider it anyway...
        // TheSuperHackers @info This should be safe to initialize because it is
        // unused, but let us be extra safe for now. (The non-retail build
        // initializes; the corpora pin the no-match output.)
        let mut closest_vec_val = Coord3 {
            x: max_dist,
            y: max_dist,
            z: max_dist,
        };

        let mut max_radius = self.max_gco_radius;
        if max_dist < HUGE_DIST {
            // don't go outwards any farther than necessary.
            max_radius = self.max_gco_radius.min(self.world_to_cell_dist(max_dist));
        }
        // (INTENSE_DEBUG's maxRadiusLimit = maxRadius + 3 clamp is not
        // defined in the build — the plain limit)
        let mut max_radius_limit = max_radius;

        let mut found_any = false;

        self.iter_flag += 1; // the function-static theIterFlag, hoisted to manager state

        let mut cur_radius = 0;
        while cur_radius <= max_radius_limit {
            let offsets = &self.radius_vec[cur_radius as usize];
            if offsets.is_empty() {
                cur_radius += 1;
                continue;
            }
            for &(ox, oy) in offsets {
                let Some(cell) = self.cell_at(cell_center_x + ox, cell_center_y + oy) else {
                    continue;
                };

                for k in 0..self.cells[cell].cois.len() {
                    // (the COI walk holds an immutable borrow of the
                    // store; the candidate processing below works the
                    // object arena — distinct structures)
                    let this_id = self.cells[cell].cois[k];

                    // never compare against ourself. (the dead-id guard
                    // is the original's null-object case)
                    let Some(this_obj) = objects.try_get(this_id) else {
                        continue;
                    };
                    if Some(this_id) == obj {
                        continue;
                    }

                    // since an object can exist in multiple COIs, we use this to avoid processing
                    // the same one more than once.
                    if objects.partition(this_id).done_flag == self.iter_flag {
                        continue;
                    }
                    let this_pos = this_obj.position;
                    objects.partition_mut(this_id).done_flag = self.iter_flag;

                    let (allowed, this_dist_sqr, dist_vec) = dist_proc(
                        &obj_pos,
                        obj_to_use.map(|id| objects.get(id)),
                        &this_pos,
                        Some(objects.get(this_id)),
                        closest_dist_sqr,
                    );
                    if !allowed {
                        continue;
                    }

                    // check the filters now
                    if !filters_allow(filters, objects, this_id) {
                        continue;
                    }

                    // ok, guess this is a winner!
                    if let Some(iter) = iter_arg.as_deref_mut() {
                        iter.insert(this_id, this_dist_sqr);
                    } else {
                        closest_obj = Some(this_id);
                        closest_dist_sqr = this_dist_sqr;
                        closest_vec_val = dist_vec;

                        if !found_any {
                            // if not adding to iterArg, we want to stop once we have the closest object.
                            max_radius_limit = cur_radius;
                        }
                        found_any = true;
                    }
                }
            }
            cur_radius += 1;
        }

        if let Some(vec) = closest_vec {
            *vec = closest_vec_val;
        }
        if let Some(dist) = closest_dist {
            *dist = sqrt(closest_dist_sqr);
        }

        closest_obj // might be null...
    }

    /// `getClosestObject` (the object form): the closest and the
    /// measure outputs (the original's optional out-pointers, always
    /// computed here — the null-arg paths are unobservable).
    #[allow(clippy::too_many_arguments)]
    pub fn get_closest_object_from_object(
        &mut self,
        objects: &mut Objects,
        obj: ObjectID,
        max_dist: f32,
        dc: DistanceCalculationType,
        filters: &[&dyn PartitionFilter],
    ) -> (Option<ObjectID>, f32, Coord3) {
        let mut dist = 0.0;
        let mut vec = Coord3::default();
        let id = self.get_closest_objects(
            objects,
            Some(obj),
            None,
            max_dist,
            dc,
            filters,
            None,
            Some(&mut dist),
            Some(&mut vec),
        );
        (id, dist, vec)
    }

    /// `getClosestObject` (the position form).
    #[allow(clippy::too_many_arguments)]
    pub fn get_closest_object_from_position(
        &mut self,
        objects: &mut Objects,
        pos: Coord3,
        max_dist: f32,
        dc: DistanceCalculationType,
        filters: &[&dyn PartitionFilter],
    ) -> (Option<ObjectID>, f32, Coord3) {
        let mut dist = 0.0;
        let mut vec = Coord3::default();
        let id = self.get_closest_objects(
            objects,
            None,
            Some(pos),
            max_dist,
            dc,
            filters,
            None,
            Some(&mut dist),
            Some(&mut vec),
        );
        (id, dist, vec)
    }

    /// `getVectorTo(obj, otherObj, dc, vec)` — the object-to-object form.
    pub fn get_vector_to(
        &self,
        objects: &Objects,
        obj: ObjectID,
        other_obj: ObjectID,
        dc: DistanceCalculationType,
    ) -> Coord3 {
        let dist_proc = THE_DIST_CALC_PROCS[dc.bits() as usize];
        let a = objects.get(obj);
        let b = objects.get(other_obj);
        let (_, _, vec) = dist_proc(&a.position, Some(a), &b.position, Some(b), HUGE_DIST_SQR);
        vec
    }

    /// `getVectorTo(obj, pos, dc, vec)` — the object-to-position form.
    pub fn get_vector_to_pos(
        &self,
        objects: &Objects,
        obj: ObjectID,
        pos: &Coord3,
        dc: DistanceCalculationType,
    ) -> Coord3 {
        let dist_proc = THE_DIST_CALC_PROCS[dc.bits() as usize];
        let a = objects.get(obj);
        let (_, _, vec) = dist_proc(&a.position, Some(a), pos, None, HUGE_DIST_SQR);
        vec
    }

    /// `getDistanceSquared(obj, otherObj, dc, vec)` — the
    /// object-to-object form.
    pub fn get_distance_squared(
        &self,
        objects: &Objects,
        obj: ObjectID,
        other_obj: ObjectID,
        dc: DistanceCalculationType,
    ) -> (f32, Coord3) {
        let dist_proc = THE_DIST_CALC_PROCS[dc.bits() as usize];
        let a = objects.get(obj);
        let b = objects.get(other_obj);
        let (_, dist, vec) = dist_proc(&a.position, Some(a), &b.position, Some(b), HUGE_DIST_SQR);
        (dist, vec)
    }

    /// `getDistanceSquared(obj, pos, dc, vec)` — the object-to-position
    /// form.
    pub fn get_distance_squared_to_pos(
        &self,
        objects: &Objects,
        obj: ObjectID,
        pos: &Coord3,
        dc: DistanceCalculationType,
    ) -> (f32, Coord3) {
        let dist_proc = THE_DIST_CALC_PROCS[dc.bits() as usize];
        let a = objects.get(obj);
        let (_, dist, vec) = dist_proc(&a.position, Some(a), pos, None, HUGE_DIST_SQR);
        (dist, vec)
    }

    /// `getGoalDistanceSquared(obj, goalPos, otherObj, dc, vec)` — the
    /// distance if `obj` were at `goalPos` (the attack-position path).
    pub fn get_goal_distance_squared(
        &self,
        objects: &Objects,
        obj: ObjectID,
        goal_pos: &Coord3,
        other_obj: ObjectID,
        dc: DistanceCalculationType,
    ) -> (f32, Coord3) {
        let dist_proc = THE_DIST_CALC_PROCS[dc.bits() as usize];
        let a = objects.get(obj);
        let b = objects.get(other_obj);
        let (_, dist, vec) = dist_proc(goal_pos, Some(a), &b.position, Some(b), HUGE_DIST_SQR);
        (dist, vec)
    }

    /// `getGoalDistanceSquared(obj, goalPos, otherPos, dc, vec)` — the
    /// position-goal form.
    pub fn get_goal_distance_squared_to_pos(
        &self,
        objects: &Objects,
        obj: ObjectID,
        goal_pos: &Coord3,
        other_pos: &Coord3,
        dc: DistanceCalculationType,
    ) -> (f32, Coord3) {
        let dist_proc = THE_DIST_CALC_PROCS[dc.bits() as usize];
        let a = objects.get(obj);
        let (_, dist, vec) = dist_proc(goal_pos, Some(a), other_pos, None, HUGE_DIST_SQR);
        (dist, vec)
    }

    /// `getRelativeAngle2D(obj, otherObj)`: the signed angle from `obj`'s
    /// facing to the direction of `otherObj`.
    pub fn get_relative_angle_2d(
        &self,
        objects: &Objects,
        obj: ObjectID,
        other_obj: ObjectID,
    ) -> f32 {
        self.get_relative_angle_2d_to_pos(objects, obj, &objects.get(other_obj).position)
    }

    /// `getRelativeAngle2D(obj, pos)`: the signed angle from `obj`'s
    /// facing to the direction of `pos` — the normalized direction dot
    /// the facing (clamped to [-1, 1]), `ACos`, and the perp-Z sign flip
    /// (a zero-length direction short-circuits to 0).
    pub fn get_relative_angle_2d_to_pos(
        &self,
        objects: &Objects,
        obj: ObjectID,
        pos: &Coord3,
    ) -> f32 {
        let mut v;

        // compute vector to given position
        let obj_pos = objects.get(obj).position;
        v = Coord3 {
            x: pos.x - obj_pos.x,
            y: pos.y - obj_pos.y,
            z: 0.0,
        };

        let dist = sqrt(v.x * v.x + v.y * v.y);

        // normalize
        if dist == 0.0 {
            return 0.0;
        }

        let dir = objects.get(obj).unit_direction_vector_2d();

        let dist_inv = 1.0 / dist;
        v.x *= dist_inv;
        v.y *= dist_inv;
        v.z *= dist_inv;

        // dot of two unit vectors is cos of angle
        let mut c = dir.x * v.x + dir.y * v.y; // + dir->z*v.z;

        // bound it in case of numerical error
        c = c.clamp(-1.0, 1.0);

        let mut value = acosf(c);

        // Determine sign by checking Z component of dir cross v
        // Note this is assumes 2D, and is identical to dotting the perpendicular of v with dir
        let perp_z = dir.x * v.y - dir.y * v.x;
        if perp_z < 0.0 {
            value = -value;
        }

        value
    }

    /// `iterateObjectsInRange(obj, ...)` — the object form: the fill,
    /// then the sort.
    pub fn iterate_objects_in_range_from_object(
        &mut self,
        objects: &mut Objects,
        obj: ObjectID,
        max_dist: f32,
        dc: DistanceCalculationType,
        filters: &[&dyn PartitionFilter],
        order: IterOrderType,
    ) -> SimpleObjectIterator {
        let mut iter = SimpleObjectIterator::new();

        self.get_closest_objects(
            objects,
            Some(obj),
            None,
            max_dist,
            dc,
            filters,
            Some(&mut iter),
            None,
            None,
        );

        iter.sort(objects, order);
        iter
    }

    /// `iterateObjectsInRange(pos, ...)` — the position form.
    pub fn iterate_objects_in_range_from_position(
        &mut self,
        objects: &mut Objects,
        pos: Coord3,
        max_dist: f32,
        dc: DistanceCalculationType,
        filters: &[&dyn PartitionFilter],
        order: IterOrderType,
    ) -> SimpleObjectIterator {
        let mut iter = SimpleObjectIterator::new();

        self.get_closest_objects(
            objects,
            None,
            Some(pos),
            max_dist,
            dc,
            filters,
            Some(&mut iter),
            None,
            None,
        );

        iter.sort(objects, order);
        iter
    }

    /// `iteratePotentialCollisions`: the bounding-sphere range with the
    /// 1.1 slop ("just a little slop") and the would-collide filter —
    /// no sort (the insertion order is the caller's).
    pub fn iterate_potential_collisions(
        &mut self,
        objects: &mut Objects,
        pos: Coord3,
        geom: GeometryInfo,
        angle: f32,
        use_2d: bool,
    ) -> SimpleObjectIterator {
        let mut max_dist = geom.bounding_sphere_radius();
        max_dist *= 1.1; // just a little slop

        let mut iter = SimpleObjectIterator::new();

        let filter = PartitionFilterWouldCollide::new(pos, geom, angle, true);
        let filters: [&dyn PartitionFilter; 1] = [&filter];

        self.get_closest_objects(
            objects,
            None,
            Some(pos),
            max_dist,
            if use_2d {
                DistanceCalculationType::FromBoundingsphere2d
            } else {
                DistanceCalculationType::FromBoundingsphere3d
            },
            &filters,
            Some(&mut iter),
            None,
            None,
        );

        iter
    }

    /// `iterateAllObjects`: no distance constraints; we're gonna have
    /// to process 'em all anyway, so go thru in the fastest way we know
    /// how — the master-list walk (the insert prepend reverses it).
    pub fn iterate_all_objects(
        &mut self,
        objects: &mut Objects,
        filters: &[&dyn PartitionFilter],
    ) -> SimpleObjectIterator {
        let mut iter = SimpleObjectIterator::new();

        for &id in &self.module_list {
            // (the null-object guard is the original's dead-module case)
            if objects.try_get(id).is_some() && filters_allow(filters, objects, id) {
                iter.insert(id, 0.0);
            }
        }

        iter
    }
}
