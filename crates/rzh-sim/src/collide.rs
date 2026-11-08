//! The collide-test family — the geometry collision matrix the contact
//! list's processing drives.
//!
//! Origin: `PartitionManager.cpp` at baseline 571a7c5 — `CollideInfo` /
//! `CollideLocAndNormal` (PartitionManager.h), `rectToFourPoints` and
//! `testRotatedPointsAgainstRect` (the inverse rotation with
//! `WWMath::Fabs`), the `xy_collideTest_Rect_Rect` / `Rect_Circle` /
//! `Circle_Rect` / `Circle_Circle` quartet (`Rect_Circle` promotes the
//! circle to a square — `setMinorRadius(majorRadius)` — and defers to
//! `Rect_Rect`; `Circle_Rect` is `Rect_Circle` with the flipped normal),
//! `z_collideTest_Sphere_Nonsphere` (the center-within-z case and the
//! below/above slice cases with the sphere-slice radius), and
//! `z_collideTest_Nonsphere_Nonsphere` (the `minRadius`-squared
//! `closeEnough` preflight) — the nine `collideTest_X_Y` procs and
//! `theCollideTestProcs` (indexed `(this - GEOMETRY_FIRST) *
//! GEOMETRY_NUM_TYPES + (that - GEOMETRY_FIRST)`, a `DEPENDS` on the
//! enum order the static_assert pins) — plus `Geometry.cpp`'s
//! `getMaxHeightAbovePosition` / `getMaxHeightBelowPosition` (the
//! `GeometryInfo` methods of [`crate::types`]) and
//! `PartitionManager::geomCollidesWithGeom` (the standalone test whose
//! z-gate ignores the below-extents — narrower than `collides_with`'s,
//! pinned by the goldens). `PartitionData::collidesWith` — the object
//! entry with the `KINDOF_NO_COLLIDE` refusal and the below-extent
//! z-gate — lives in [`crate::partition`] (it reads the object arena).
//!
//! Stated deviations, behaviour-preserving (plan §3.3 rule 2): the
//! original's five-argument forward declaration of
//! `xy_collideTest_Circle_Circle` (declared, never defined, never called
//! — a dead overload sibling) is not ported; `GeometryInfo`'s
//! `calcBoundingStuff` recompute is not modelled (the collide family
//! never reads the bounding radii); the original's uninitialized
//! `CollideLocAndNormal` stack locals become zero-initialized — the only
//! unwritten-read path is the `closeEnough` preflight, whose `loc.x/y`
//! and `normal` are garbage in the original, the pinned zero here.
//!
//! The float paths run on the pinned GameMath trig (`rzh_math::gmath`)
//! and the correctly-rounded sqrt / fabs (`rzh_math::scalar`) exactly as
//! the deterministic build does.
//!
//! Original: Copyright 2025 Electronic Arts Inc.
//! Rust port Copyright 2026 the RustZeroHour contributors.

use rzh_math::gmath::{cosf, sinf};
use rzh_math::scalar::{fabs, sqrt};

use crate::logic::Objects;
use crate::types::{Coord3, GeometryInfo, GeometryType, ObjectID};

/// `sqr` (Lib/Precompiled/BaseAlgorithm.h): `x * x`.
#[inline]
fn csqr(x: f32) -> f32 {
    x * x
}

/// `CollideLocAndNormal` (PartitionManager.h): the collision location
/// and the collision surface normal at `a`. Both are filled only when a
/// proc returns `true`; the `closeEnough` preflight's `true` leaves
/// `loc.x`/`loc.y` and `normal` at the zero the port pins (the original
/// hands its uninitialized stack local onward there).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CollideLocAndNormal {
    /// `loc`: an in-between spot on the collision — not guaranteed to lie
    /// precisely on either geometry.
    pub loc: Coord3,
    /// `normal`: the normal of the collision surface at `a` (exactly
    /// opposite for `b`); a unit vector when written.
    pub normal: Coord3,
}

/// `CollideInfo` (PartitionManager.cpp): the geometry pair under test —
/// a position, a geometry copy and an angle.
#[derive(Clone, Copy, Debug)]
pub struct CollideInfo {
    /// The canonical position (the geometry's bottom center).
    pub position: Coord3,
    /// The geometry (copied by value — the slice cases mutate their copy).
    pub geom: GeometryInfo,
    /// The facing angle, radians.
    pub angle: f32,
}

impl CollideInfo {
    /// The `CollideInfo(p, g, a)` constructor.
    pub fn new(position: Coord3, geom: GeometryInfo, angle: f32) -> Self {
        Self {
            position,
            geom,
            angle,
        }
    }
}

/// `CollideTestProc` (PartitionManager.cpp): every proc of the matrix
/// takes the pair and an optional output — `collideLoc` /
/// `collideNormal` are filled only on a `true` result, and the `None`
/// form is the original's null-pointer "don't need 'em" call.
pub type CollideTestProc = fn(&CollideInfo, &CollideInfo, Option<&mut CollideLocAndNormal>) -> bool;

// --- the 2d/3d vector helpers (PartitionManager.cpp's inline functions). ---

/// `vecDiff_2D`: the xy difference `a - b`, z zeroed.
fn vec_diff_2d(pos_a: &Coord3, pos_b: &Coord3) -> Coord3 {
    Coord3 {
        x: pos_a.x - pos_b.x,
        y: pos_a.y - pos_b.y,
        z: 0.0,
    }
}

/// `vecDiff_3D`: the full difference `a - b`.
fn vec_diff_3d(pos_a: &Coord3, pos_b: &Coord3) -> Coord3 {
    Coord3 {
        x: pos_a.x - pos_b.x,
        y: pos_a.y - pos_b.y,
        z: pos_a.z - pos_b.z,
    }
}

/// `calcSqrDist_2D`.
fn calc_sqr_dist_2d(dist: &Coord3) -> f32 {
    csqr(dist.x) + csqr(dist.y)
}

/// `calcSqrDist_3D`.
fn calc_sqr_dist_3d(dist: &Coord3) -> f32 {
    csqr(dist.x) + csqr(dist.y) + csqr(dist.z)
}

/// `projectCoord3D`: `coord += unitDir * dist`.
fn project_coord3d(coord: &mut Coord3, unit_dir: &Coord3, dist: f32) {
    coord.x += unit_dir.x * dist;
    coord.y += unit_dir.y * dist;
    coord.z += unit_dir.z * dist;
}

/// `flipCoord3D`: negate all three components (the flipped normal's z
/// becomes `-0.0` — bit-visible in the goldens).
fn flip_coord3d(coord: &mut Coord3) {
    coord.x = -coord.x;
    coord.y = -coord.y;
    coord.z = -coord.z;
}

// --- rectToFourPoints / testRotatedPointsAgainstRect. ---------------------

/// `rectToFourPoints`: the rect's four corners, rotation included (z is
/// ignored).
fn rect_to_four_points(a: &CollideInfo) -> [Coord2; 4] {
    let c = cosf(a.angle);
    let s = sinf(a.angle);

    let exc = a.geom.major_radius * c;
    let eyc = a.geom.minor_radius * c;
    let exs = a.geom.major_radius * s;
    let eys = a.geom.minor_radius * s;

    // tl
    let tl = Coord2 {
        x: a.position.x - exc - eys,
        y: a.position.y + eyc - exs,
    };
    // tr
    let tr = Coord2 {
        x: a.position.x + exc - eys,
        y: a.position.y + eyc + exs,
    };
    // bl
    let bl = Coord2 {
        x: a.position.x - exc + eys,
        y: a.position.y - eyc - exs,
    };
    // br
    let br = Coord2 {
        x: a.position.x + exc + eys,
        y: a.position.y - eyc + exs,
    };
    [tl, tr, bl, br]
}

/// `Coord2D` (Lib/Coord2D.h): the rect corners' plane.
#[derive(Clone, Copy, Debug, Default)]
struct Coord2 {
    /// The x coordinate.
    x: f32,
    /// The y coordinate.
    y: f32,
}

/// `testRotatedPointsAgainstRect`: inverse-rotate each of the four
/// points into `a`'s frame and count the ones inside `a`'s
/// half-extents — the sphere branch of the minor-radius selection is
/// the promoted-square path (`Rect_Circle`'s circle-as-square keeps its
/// `GEOMETRY_SPHERE` type).
fn test_rotated_points_against_rect(
    pts: &[Coord2; 4],
    a: &CollideInfo,
    avg: &mut Coord2,
    avg_tot: &mut i32,
) {
    //DEBUG_ASSERTCRASH(a->geom.getGeomType() == GEOMETRY_BOX, ("only boxes are ok here"));
    let major = a.geom.major_radius;
    let minor = if a.geom.geom_type == GeometryType::Sphere {
        a.geom.major_radius
    } else {
        a.geom.minor_radius
    };

    let c = cosf(-a.angle);
    let s = sinf(-a.angle);

    for pt in pts {
        // convert to a delta relative to rect ctr
        let ptx = pt.x - a.position.x;
        let pty = pt.y - a.position.y;

        // inverse-rotate it to the right coord system
        let ptx_new = fabs(ptx * c - pty * s);
        let pty_new = fabs(ptx * s + pty * c);

        if ptx_new <= major && pty_new <= minor {
            avg.x += pt.x;
            avg.y += pt.y;
            *avg_tot += 1;
        }
    }
}

// --- the xy quartet. -------------------------------------------------------

/// `xy_collideTest_Rect_Rect`: the corner-average containment test — a
/// point-count over both rects' corners, each inverse-rotated into the
/// other's frame.
fn xy_collide_test_rect_rect(
    a: &CollideInfo,
    b: &CollideInfo,
    cinfo: Option<&mut CollideLocAndNormal>,
) -> bool {
    let mut avg = Coord2::default();
    let mut avg_tot: i32 = 0;

    let pts_a = rect_to_four_points(a);
    test_rotated_points_against_rect(&pts_a, b, &mut avg, &mut avg_tot);

    let pts_b = rect_to_four_points(b);
    test_rotated_points_against_rect(&pts_b, a, &mut avg, &mut avg_tot);

    if avg_tot > 0 {
        if let Some(cinfo) = cinfo {
            // this is a little hokey, but generally adequate...
            cinfo.loc.x = avg.x / avg_tot as f32;
            cinfo.loc.y = avg.y / avg_tot as f32;
            cinfo.loc.z = (a.position.z + b.position.z) * 0.5;

            // yow, what exactly does it mean to find the normal to two *intersecting*
            // rects? ... we provide a (poor) guess by just doing the center-to-center
            // normal. (srj)
            cinfo.normal = vec_diff_2d(&b.position, &a.position);
            cinfo.normal.normalize();
        }
        true
    } else {
        false
    }
}

/// `xy_collideTest_Rect_Circle`: the circle is promoted to a square —
/// `setMinorRadius(majorRadius)` — and the pair defers to
/// [`xy_collide_test_rect_rect`] ("this is better than the other one,
/// since it actually handles rotated rects, but still not as accurate
/// as it could be", srj).
fn xy_collide_test_rect_circle(
    a: &CollideInfo,
    b: &CollideInfo,
    cinfo: Option<&mut CollideLocAndNormal>,
) -> bool {
    let mut btmp = *b;
    btmp.geom.set_minor_radius(btmp.geom.major_radius);
    xy_collide_test_rect_rect(a, &btmp, cinfo)
}

/// `xy_collideTest_Circle_Rect`: [`xy_collide_test_rect_circle`] with
/// the pair swapped and the normal flipped.
fn xy_collide_test_circle_rect(
    a: &CollideInfo,
    b: &CollideInfo,
    mut cinfo: Option<&mut CollideLocAndNormal>,
) -> bool {
    let result = xy_collide_test_rect_circle(b, a, cinfo.as_deref_mut());
    if let Some(cinfo) = cinfo {
        flip_coord3d(&mut cinfo.normal);
    }
    result
}

/// `xy_collideTest_Circle_Circle` (the `CollideInfo` overload the z
/// composites pass around; the five-argument forward declaration in the
/// original is dead — declared, never defined, never called).
fn xy_collide_test_circle_circle(
    a: &CollideInfo,
    b: &CollideInfo,
    cinfo: Option<&mut CollideLocAndNormal>,
) -> bool {
    let diff = vec_diff_2d(&b.position, &a.position);
    let dist_sqr = calc_sqr_dist_2d(&diff);
    let touching_dist_sqr = csqr(a.geom.major_radius + b.geom.major_radius);
    if dist_sqr <= touching_dist_sqr {
        // calc the approximate location and normal.
        if let Some(cinfo) = cinfo {
            cinfo.normal = diff;
            cinfo.normal.normalize();
            cinfo.loc = a.position;
            project_coord3d(&mut cinfo.loc, &cinfo.normal, a.geom.major_radius);
        }

        true
    } else {
        false
    }
}

// --- the z composites. -----------------------------------------------------

/// `z_collideTest_Sphere_Nonsphere`: case 1 — the sphere's center
/// within the nonsphere's z-space (a side-to-side collision, the xy
/// result with the z overwritten); case 2a/2b — the sphere's center
/// below/above the nonsphere but within its major radius of the
/// bottom/top (the slice of the sphere at that plane, its radius via
/// `WWMath::Sqrtf`).
fn z_collide_test_sphere_nonsphere(
    xyproc: CollideTestProc,
    a: &CollideInfo,
    b: &CollideInfo,
    mut cinfo: Option<&mut CollideLocAndNormal>,
) -> bool {
    // case 1: center of sphere is within the nonsphere z-space.
    if a.position.z >= b.position.z
        && a.position.z <= b.position.z + b.geom.max_height_above_position()
    {
        if xyproc(a, b, cinfo.as_deref_mut()) {
            // a side-to-side collision; just need to adjust the z-coord of collideLoc
            if let Some(cinfo) = cinfo.as_deref_mut() {
                cinfo.loc.z = a.position.z;
            }
            return true;
        }
        return false;
    }

    // case 2: center of sphere is above or below nonsphere, but within
    // major-radius of top or bottom.

    // 2a: sphere is below the nonsphere
    let b_bot = b.position.z;
    if a.position.z < b_bot && a.position.z + a.geom.major_radius >= b_bot {
        // find the radius of the slice of the sphere that is at b_bot
        let mut amod = *a;
        amod.position.z = b_bot;
        amod.geom
            .set_major_radius(sqrt(csqr(a.geom.major_radius) - csqr(b_bot - a.position.z)));
        if xyproc(&amod, b, cinfo.as_deref_mut()) {
            // (the 'end' collision normal would point straight up; never
            // possible, never computed)
            return true;
        }
        return false;
    }

    // 2b: sphere is above the nonsphere
    let b_top = b.position.z + b.geom.max_height_above_position();
    if a.position.z > b_top && a.position.z - a.geom.major_radius <= b_top {
        let mut amod = *a;
        amod.position.z = b_top;
        amod.geom
            .set_major_radius(sqrt(csqr(a.geom.major_radius) - csqr(a.position.z - b_top)));
        if xyproc(&amod, b, cinfo) {
            // (ditto, straight down)
            return true;
        }
        return false;
    }

    false
}

/// `z_collideTest_Nonsphere_Nonsphere`: the `closeEnough` preflight —
/// centers closer than the smallest half-extent collide regardless of
/// the rect test ("cylindrical shapes currently never collide
/// top-to-bottom, so we just declare everything to be side-to-side").
/// The preflight's `true` leaves `loc.x`/`loc.y` and `normal` unwritten
/// (the pinned zero).
fn z_collide_test_nonsphere_nonsphere(
    xyproc: CollideTestProc,
    a: &CollideInfo,
    b: &CollideInfo,
    mut cinfo: Option<&mut CollideLocAndNormal>,
) -> bool {
    // note: we already know that there is a z-intersection (our caller filters that out)
    // so we need not recheck that here.

    // Preflight for close objects.
    let d_sqr = csqr(a.position.x - b.position.x) + csqr(a.position.y - b.position.y);
    let mut min_radius = a.geom.major_radius;
    let r = a.geom.minor_radius;
    if min_radius > r {
        min_radius = r;
    }
    let r = b.geom.major_radius;
    if min_radius > r {
        min_radius = r;
    }
    let r = b.geom.minor_radius;
    if min_radius > r {
        min_radius = r;
    }

    let close_enough = csqr(min_radius) > d_sqr;

    if close_enough || xyproc(a, b, cinfo.as_deref_mut()) {
        // since we allow objects to overlap at times, how do we distinguish
        // between a "top-to-bottom" vs "side-to-side" collision?
        // currently, we don't: cylindrical shapes currently never collide
        // top-to-bottom, so we just declare everything to be side-to-side.
        // if this changes, need to smarten this.

        // case 1: side-to-side collision
        // just need to adjust the z-coord of collideLoc
        if let Some(cinfo) = cinfo {
            if b.position.z > a.position.z {
                cinfo.loc.z =
                    (b.position.z + a.position.z + a.geom.max_height_above_position()) * 0.5;
            } else {
                cinfo.loc.z =
                    (a.position.z + b.position.z + b.geom.max_height_above_position()) * 0.5;
            }
        }
        true
    } else {
        false
    }
}

// --- the nine proc entries. ------------------------------------------------

/// `collideTest_Sphere_Sphere`: the 3d distance against the touching
/// distance — the `<=` boundary included; the loc is `a`'s surface
/// point along the unit normal (the zero vector's normalize guard
/// leaves the normal zero for coincident spheres).
pub fn collide_test_sphere_sphere(
    a: &CollideInfo,
    b: &CollideInfo,
    cinfo: Option<&mut CollideLocAndNormal>,
) -> bool {
    let diff = vec_diff_3d(&b.position, &a.position);
    let dist_sqr = calc_sqr_dist_3d(&diff);
    let touching_dist_sqr = csqr(a.geom.major_radius + b.geom.major_radius);
    if dist_sqr <= touching_dist_sqr {
        if let Some(cinfo) = cinfo {
            cinfo.normal = diff;
            cinfo.normal.normalize();
            cinfo.loc = a.position;
            project_coord3d(&mut cinfo.loc, &cinfo.normal, a.geom.major_radius);
        }

        true
    } else {
        false
    }
}

/// `collideTest_Sphere_Cylinder`.
pub fn collide_test_sphere_cylinder(
    a: &CollideInfo,
    b: &CollideInfo,
    cinfo: Option<&mut CollideLocAndNormal>,
) -> bool {
    z_collide_test_sphere_nonsphere(xy_collide_test_circle_circle, a, b, cinfo)
}

/// `collideTest_Sphere_Box`.
pub fn collide_test_sphere_box(
    a: &CollideInfo,
    b: &CollideInfo,
    cinfo: Option<&mut CollideLocAndNormal>,
) -> bool {
    z_collide_test_sphere_nonsphere(xy_collide_test_circle_rect, a, b, cinfo)
}

/// `collideTest_Cylinder_Sphere`: the sphere composite with the pair
/// swapped and the normal flipped.
pub fn collide_test_cylinder_sphere(
    a: &CollideInfo,
    b: &CollideInfo,
    mut cinfo: Option<&mut CollideLocAndNormal>,
) -> bool {
    let result =
        z_collide_test_sphere_nonsphere(xy_collide_test_circle_circle, b, a, cinfo.as_deref_mut());
    if let Some(cinfo) = cinfo {
        flip_coord3d(&mut cinfo.normal);
    }
    result
}

/// `collideTest_Cylinder_Cylinder`.
pub fn collide_test_cylinder_cylinder(
    a: &CollideInfo,
    b: &CollideInfo,
    cinfo: Option<&mut CollideLocAndNormal>,
) -> bool {
    z_collide_test_nonsphere_nonsphere(xy_collide_test_circle_circle, a, b, cinfo)
}

/// `collideTest_Cylinder_Box`.
pub fn collide_test_cylinder_box(
    a: &CollideInfo,
    b: &CollideInfo,
    cinfo: Option<&mut CollideLocAndNormal>,
) -> bool {
    z_collide_test_nonsphere_nonsphere(xy_collide_test_circle_rect, a, b, cinfo)
}

/// `collideTest_Box_Sphere`: the sphere composite with the pair swapped
/// and the normal flipped.
pub fn collide_test_box_sphere(
    a: &CollideInfo,
    b: &CollideInfo,
    mut cinfo: Option<&mut CollideLocAndNormal>,
) -> bool {
    let result =
        z_collide_test_sphere_nonsphere(xy_collide_test_circle_rect, b, a, cinfo.as_deref_mut());
    if let Some(cinfo) = cinfo {
        flip_coord3d(&mut cinfo.normal);
    }
    result
}

/// `collideTest_Box_Cylinder`.
pub fn collide_test_box_cylinder(
    a: &CollideInfo,
    b: &CollideInfo,
    cinfo: Option<&mut CollideLocAndNormal>,
) -> bool {
    z_collide_test_nonsphere_nonsphere(xy_collide_test_rect_circle, a, b, cinfo)
}

/// `collideTest_Box_Box`.
pub fn collide_test_box_box(
    a: &CollideInfo,
    b: &CollideInfo,
    cinfo: Option<&mut CollideLocAndNormal>,
) -> bool {
    z_collide_test_nonsphere_nonsphere(xy_collide_test_rect_rect, a, b, cinfo)
}

/// `theCollideTestProcs` (PartitionManager.cpp): the 3x3 matrix, indexed
/// `(this - GEOMETRY_FIRST) * GEOMETRY_NUM_TYPES + (that - GEOMETRY_FIRST)`
/// — "This *DEPENDS* on the order of the geometry enum defines".
pub const COLLIDE_TEST_PROCS: [CollideTestProc; 9] = [
    collide_test_sphere_sphere,
    collide_test_sphere_cylinder,
    collide_test_sphere_box,
    collide_test_cylinder_sphere,
    collide_test_cylinder_cylinder,
    collide_test_cylinder_box,
    collide_test_box_sphere,
    collide_test_box_cylinder,
    collide_test_box_box,
];

/// `PartitionManager::geomCollidesWithGeom` — see if this geometry
/// collides with that one. (A `const` member on the manager in the
/// original, but pure geometry: no manager state is read.) Note the
/// z-gate: unlike [`crate::partition::collides_with`] this one ignores
/// the below-position extents entirely — `thisTop` is compared against
/// that's raw z and this's raw z against `thatTop` — a narrower gate,
/// pinned by the goldens.
#[must_use]
pub fn geom_collides_with_geom(
    pos1: &Coord3,
    geom1: &GeometryInfo,
    angle1: f32,
    pos2: &Coord3,
    geom2: &GeometryInfo,
    angle2: f32,
) -> bool {
    let this_info = CollideInfo::new(*pos1, *geom1, angle1);
    let that_info = CollideInfo::new(*pos2, *geom2, angle2);

    // invariant for all geometries: first do z collision check.
    if this_info.position.z + this_info.geom.max_height_above_position() >= that_info.position.z
        && this_info.position.z <= that_info.position.z + that_info.geom.max_height_above_position()
    {
        // NOTE: This assumes geometry enumerations that start at GEOMETRY_FIRST AND depends on the
        // order in which they appear in the enum list
        let collide_proc = COLLIDE_TEST_PROCS[(this_info.geom.geom_type.bits() as usize) * 3
            + (that_info.geom.geom_type.bits() as usize)];
        // (the original's uninitialized CollideLocAndNormal local — the
        // pinned zero; the value is never read)
        let mut cloc = CollideLocAndNormal::default();
        collide_proc(&this_info, &that_info, Some(&mut cloc))
    } else {
        // no z-intersection -> no collision.
        false
    }
}

/// `PartitionData::collidesWith` (the `friend_collidesWith` entry the
/// contact list's processing calls): the `KINDOF_NO_COLLIDE` refusal
/// ("a collision extent of zero size is still a point and can collide,
/// but we don't always want to"), the z-intersection gate with the
/// below-position extents, and the table dispatch. Returns the filled
/// [`CollideLocAndNormal`] on a collision.
#[must_use]
pub fn collides_with(
    objects: &Objects,
    this_id: ObjectID,
    that_id: ObjectID,
) -> Option<CollideLocAndNormal> {
    let this_obj = objects.get(this_id);
    let that_obj = objects.get(that_id);

    if this_obj.is_kind_of_no_collide() || that_obj.is_kind_of_no_collide() {
        // A collision extent of zero size is still a point and can collide, but we don't always want to.
        return None;
    }

    let this_info = CollideInfo::new(this_obj.position, this_obj.geometry, this_obj.orientation);
    let that_info = CollideInfo::new(that_obj.position, that_obj.geometry, that_obj.orientation);

    // invariant for all geometries: first do z collision check.
    let this_top = this_info.position.z + this_info.geom.max_height_above_position();
    let this_bot = this_info.position.z - this_info.geom.max_height_below_position();
    let that_top = that_info.position.z + that_info.geom.max_height_above_position();
    let that_bot = that_info.position.z - that_info.geom.max_height_below_position();
    if this_top >= that_bot && this_bot <= that_top {
        // NOTE: This assumes geometry enumerations that start at GEOMETRY_FIRST AND depends on the
        // order in which they appear in the enum list
        let collide_proc = COLLIDE_TEST_PROCS[(this_obj.geometry.geom_type.bits() as usize) * 3
            + (that_obj.geometry.geom_type.bits() as usize)];
        let mut cinfo = CollideLocAndNormal::default();
        if collide_proc(&this_info, &that_info, Some(&mut cinfo)) {
            Some(cinfo)
        } else {
            None
        }
    } else {
        // no z-intersection -> no collision.
        None
    }
}
