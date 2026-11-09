//! The kernel's shared vocabulary: ids, coordinates, geometry, masks,
//! sleep times and the frame constants.
//!
//! Origin: `Core/GameEngine/Include/Common/GameType.h` (ObjectID),
//! `Core/GameEngine/Include/Common/GameCommon.h` (MAX_PLAYER_COUNT,
//! NEVER/FOREVER, the LOGICFRAMES_* constants),
//! `Core/Libraries/Source/WWVegas/WWLib/WWCommon.h` (WWSyncPerSecond),
//! `GeneralsMD/Code/GameEngine/Include/Common/DisabledTypes.h` (the 13
//! disabled types), `Core/Libraries/Source/Lib/Include/Lib/Coord3D.h`
//! (Coord3D), `GeneralsMD/Code/GameEngine/Include/Common/Geometry.h`
//! (GeometryType/GeometryInfo), `Core/GameEngine/Include/Common/KindOf.h`
//! (the IMMOBILE bit) and
//! `GeneralsMD/Code/GameEngine/Include/GameLogic/Module/UpdateModule.h`
//! (UpdateSleepTime, SleepyUpdatePhase),
//! `GeneralsMD/Code/GameEngine/Include/GameLogic/PartitionManager.h`
//! (HUGE_DIST, DistanceCalculationType),
//! `GeneralsMD/Code/GameEngine/Include/GameLogic/ObjectIter.h`
//! (IterOrderType),
//! `Core/GameEngine/Include/Common/GameCommon.h` (the `Relationship`
//! enum with `TheRelationShipNames`, `PlayerMaskType` /
//! `PLAYERMASK_ALL`, `PlayerType`),
//! `GeneralsMD/Code/GameEngine/Include/Common/PlayerList.h`
//! (`AllowPlayerRelationship`),
//! `GeneralsMD/Code/GameEngine/Include/Common/Player.h`
//! (`PLAYER_INDEX_INVALID`),
//! `GeneralsMD/Code/GameEngine/Include/Common/Team.h` (`TeamID` /
//! `TeamPrototypeID` with their invalids) and
//! `Core/GameEngine/Include/GameNetwork/NetworkDefs.h` (`MAX_SLOTS`) —
//! at baseline 571a7c5.
//!
//! Original: Copyright 2025 Electronic Arts Inc.
//! Rust port Copyright 2026 the RustZeroHour contributors.

use rzh_math::scalar::sqrt;

/// `MAX_PLAYER_COUNT` (GameCommon.h): the per-player array bound.
pub const MAX_PLAYER_COUNT: usize = 16;

/// `NEVER` (GameCommon.h).
pub const NEVER: u32 = 0;

/// `FOREVER` (GameCommon.h): `0x3fffffff`, chosen so offsets can be added
/// without overflow; at 30 fps this is ~414 days.
pub const FOREVER: u32 = 0x3fff_ffff;

/// `WWSyncPerSecond` (WWLib/WWCommon.h): the logic rate the whole engine
/// is tied to — the "30 Hz in retail, to verify" item of plan §4 WP1,
/// verified against the source.
pub const WW_SYNC_PER_SECOND: u32 = 30;

/// `WWSyncMilliseconds` (WWLib/WWCommon.h): `1000 / WWSyncPerSecond`.
pub const WW_SYNC_MILLISECONDS: u32 = 1000 / WW_SYNC_PER_SECOND;

/// `LOGICFRAMES_PER_SECOND` (GameCommon.h): `= WWSyncPerSecond` = 30.
pub const LOGICFRAMES_PER_SECOND: u32 = WW_SYNC_PER_SECOND;

/// `MSEC_PER_SECOND` (GameCommon.h).
pub const MSEC_PER_SECOND: u32 = 1000;

/// `LOGICFRAMES_PER_MSEC_REAL` (GameCommon.h).
pub const LOGICFRAMES_PER_MSEC_REAL: f32 =
    (LOGICFRAMES_PER_SECOND as f32) / (MSEC_PER_SECOND as f32);

/// `MSEC_PER_LOGICFRAME_REAL` (GameCommon.h): 33⅓ ms per logic frame.
pub const MSEC_PER_LOGICFRAME_REAL: f32 =
    (MSEC_PER_SECOND as f32) / (LOGICFRAMES_PER_SECOND as f32);

/// `LOGICFRAMES_PER_SECONDS_REAL` (GameCommon.h).
pub const LOGICFRAMES_PER_SECONDS_REAL: f32 = LOGICFRAMES_PER_SECOND as f32;

/// `SECONDS_PER_LOGICFRAME_REAL` (GameCommon.h).
pub const SECONDS_PER_LOGICFRAME_REAL: f32 = 1.0 / LOGICFRAMES_PER_SECONDS_REAL;

/// `ObjectID` (GameType.h): a signed 32-bit id; objects are referenced by
/// this handle everywhere in the sim (plan §3.3 rule 6).
pub type ObjectID = i32;

/// `INVALID_ID` (GameType.h): id 0 — no live object ever carries it.
pub const INVALID_ID: ObjectID = 0;

/// `UpdateSleepTime` (UpdateModule.h): the frame count a module asks to
/// sleep for. `0` is invalid, `1` means "next frame",
/// `0x3fffffff` is "forever" (~414 days at 30 fps, chosen so offsets can
/// be added without overflow).
pub type UpdateSleepTime = i32;

/// `UPDATE_SLEEP_INVALID` (UpdateModule.h).
pub const UPDATE_SLEEP_INVALID: UpdateSleepTime = 0;

/// `UPDATE_SLEEP_NONE` (UpdateModule.h): wake up as soon as possible.
pub const UPDATE_SLEEP_NONE: UpdateSleepTime = 1;

/// `UPDATE_SLEEP_FOREVER` (UpdateModule.h).
pub const UPDATE_SLEEP_FOREVER: UpdateSleepTime = 0x3fff_ffff;

/// `SleepyUpdatePhase` (UpdateModule.h): the two low bits of the packed
/// priority. All phase 0 modules are called before all phase 1 modules
/// within a given frame, and so on — "you should really never specify
/// anything other than PHASE_NORMAL without very careful deliberation".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum SleepyUpdatePhase {
    /// `PHASE_INITIAL` = 0.
    Initial = 0,
    /// `PHASE_PHYSICS` = 1.
    Physics = 1,
    /// `PHASE_NORMAL` = 2 (the default).
    #[default]
    Normal = 2,
    /// `PHASE_FINAL` = 3.
    Final = 3,
}

impl SleepyUpdatePhase {
    /// The two-bit phase value.
    #[must_use]
    pub fn bits(self) -> u32 {
        self as u32
    }
}

/// `GeometryType` (Common/Geometry.h).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GeometryType {
    /// `GEOMETRY_SPHERE`: partition/collision testing as sphere
    /// (major_radius = radius).
    #[default]
    Sphere = 0,
    /// `GEOMETRY_CYLINDER`: partition/collision testing as cylinder
    /// (major_radius = radius, height = height).
    Cylinder = 1,
    /// `GEOMETRY_BOX`: partition/collision testing as rectangular box
    /// (major_radius = half len in forward dir; minor_radius = half len
    /// in side dir; height = height).
    Box = 2,
}

impl GeometryType {
    /// The wire value (the C++ enum ordinals; out-of-range values read by
    /// the original fall to the `default:` branches — the port models the
    /// three known types only and states the rest).
    #[must_use]
    pub fn bits(self) -> i32 {
        self as i32
    }
}

/// `Coord3D` (Lib/Coord3D.h): the game's ubiquitous x/y/z triple.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Coord3 {
    /// The x coordinate.
    pub x: f32,
    /// The y coordinate.
    pub y: f32,
    /// The z coordinate.
    pub z: f32,
}

impl Coord3 {
    /// `Coord3D::normalize` (Lib/Coord3D.h): the length via the
    /// unqualified `Sqrt` (Lib/trig.h — `gm_sqrtf` in the deterministic
    /// build) and the `len != 0` guard (a zero vector stays zero — the
    /// coincident-spheres normal).
    pub fn normalize(&mut self) {
        let len = rzh_math::scalar::sqrt(self.x * self.x + self.y * self.y + self.z * self.z);
        if len != 0.0 {
            self.x /= len;
            self.y /= len;
            self.z /= len;
        }
    }
}

/// `GeometryInfo` (Common/Geometry.h): the shape and size the partition
/// store rasterizes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeometryInfo {
    /// The shape family.
    pub geom_type: GeometryType,
    /// The "small" flag: at most four cells, filled by the dedicated
    /// small-shape filler.
    pub is_small: bool,
    /// The height (unused by the coverage fills; carried for the object
    /// model's completeness).
    pub height: f32,
    /// The major radius (the radius for spheres/cylinders, the forward
    /// half-extent for boxes).
    pub major_radius: f32,
    /// The minor radius (the side half-extent for boxes).
    pub minor_radius: f32,
}

impl Default for GeometryInfo {
    fn default() -> Self {
        // the GeometryInfo(type, isSmall, height, majorRadius,
        // minorRadius) ctor defaults
        Self {
            geom_type: GeometryType::Sphere,
            is_small: false,
            height: 0.0,
            major_radius: 0.0,
            minor_radius: 0.0,
        }
    }
}

impl GeometryInfo {
    /// `setMajorRadius` (Geometry.h): the field write. (The original also
    /// recomputes the bounding circle/sphere radii via `calcBoundingStuff`;
    /// the port computes them on demand in
    /// [`Self::bounding_circle_radius`] / [`Self::bounding_sphere_radius`]
    /// — the same pure function of the same five inputs, so the values
    /// the cached original reads are identical, plan §3.3 rule 2.)
    pub fn set_major_radius(&mut self, radius: f32) {
        self.major_radius = radius;
        // (calcBoundingStuff — recomputed on demand by the getters below)
    }

    /// `setMinorRadius` (Geometry.h): the field write (ditto the
    /// `calcBoundingStuff` note of [`Self::set_major_radius`]).
    pub fn set_minor_radius(&mut self, radius: f32) {
        self.minor_radius = radius;
        // (calcBoundingStuff — recomputed on demand by the getters below)
    }

    /// `calcBoundingStuff` (Geometry.cpp), the `m_boundingCircleRadius`
    /// half: the circle radius of the shape's footprint — the major
    /// radius for spheres and cylinders, the diagonal for boxes.
    #[must_use]
    pub fn bounding_circle_radius(&self) -> f32 {
        match self.geom_type {
            GeometryType::Sphere | GeometryType::Cylinder => self.major_radius,
            GeometryType::Box => {
                sqrt(self.major_radius * self.major_radius + self.minor_radius * self.minor_radius)
            }
        }
    }

    /// `calcBoundingStuff` (Geometry.cpp), the `m_boundingSphereRadius`
    /// half: the sphere radius of the full 3d shape — the radius for
    /// spheres, the max of the half-height and the radius for cylinders,
    /// the full 3d diagonal for boxes.
    #[must_use]
    pub fn bounding_sphere_radius(&self) -> f32 {
        match self.geom_type {
            GeometryType::Sphere => self.major_radius,
            GeometryType::Cylinder => {
                let r = self.height * 0.5;
                if r < self.major_radius {
                    self.major_radius
                } else {
                    r
                }
            }
            GeometryType::Box => sqrt(
                self.major_radius * self.major_radius
                    + self.minor_radius * self.minor_radius
                    + (self.height * 0.5) * (self.height * 0.5),
            ),
        }
    }

    /// `getZDeltaToCenterPosition` (Geometry.cpp): where the "center"
    /// of the geometry is, relative to the canonical position — the
    /// half-height for the grounded shapes, nothing for spheres.
    #[must_use]
    pub fn z_delta_to_center_position(&self) -> f32 {
        if self.geom_type == GeometryType::Sphere {
            0.0
        } else {
            self.height * 0.5
        }
    }

    /// `getMaxHeightAbovePosition` (Geometry.cpp): how far above the
    /// canonical position the max z extends — the radius for spheres,
    /// the height for boxes and cylinders, 0 beyond the known types.
    #[must_use]
    pub fn max_height_above_position(&self) -> f32 {
        match self.geom_type {
            GeometryType::Sphere => self.major_radius,
            GeometryType::Box | GeometryType::Cylinder => self.height,
        }
    }

    /// `getMaxHeightBelowPosition` (Geometry.cpp): how far below the
    /// canonical position the min z extends — the radius for spheres,
    /// nothing for the grounded shapes.
    #[must_use]
    pub fn max_height_below_position(&self) -> f32 {
        match self.geom_type {
            GeometryType::Sphere => self.major_radius,
            GeometryType::Box | GeometryType::Cylinder => 0.0,
        }
    }
}

/// The kindof bitset's `KINDOF_IMMOBILE` (KindOf.h: `KINDOF_OBSTACLE` = 0,
/// `KINDOF_SELECTABLE` = 1, `KINDOF_IMMOBILE` = 2). The kernel consults
/// exactly this bit (the partition refresh's collision gate); the full
/// 122-entry kindof table arrives with the rzh-data object model.
pub const KINDOF_IMMOBILE_BIT: u32 = 2;

/// The kindof bitset's `KINDOF_NO_COLLIDE` (KindOf.h, the Zero Hour build
/// with `ALLOW_SURRENDER` and `RTS_GENERALS` both off — the same
/// configuration the script-engine slice's generated `KINDOF_NAMES` table
/// pinned: ordinal 27, "Never collide with or be collided with"). The
/// collide family refuses pairs where either side carries it.
pub const KINDOF_NO_COLLIDE_BIT: u32 = 27;

/// `DisabledType` (Common/DisabledTypes.h): the 13 disable reasons. The
/// ordinals are the bit indices of `DisabledMask`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(i32)]
pub enum DisabledType {
    /// `DISABLED_DEFAULT`: typical disable — like systems, things that
    /// don't need to run.
    Default = 0,
    /// `DISABLED_HACKED`: this unit has been hacked.
    Hacked = 1,
    /// `DISABLED_EMP`: this unit has been disabled via
    /// electro-magnetic-pulse.
    Emp = 2,
    /// `DISABLED_HELD`: special case — held means it can fire and
    /// `isHeld` checks to make sure ONLY held is set!
    Held = 3,
    /// `DISABLED_PARALYZED`: battle plans have changed, and unit is
    /// confused/paralyzed.
    Paralyzed = 4,
    /// `DISABLED_UNMANNED`: vehicle is unmanned.
    Unmanned = 5,
    /// `DISABLED_UNDERPOWERED`: separate from ScriptUnderpowered, the
    /// owning player has insufficient power.
    Underpowered = 6,
    /// `DISABLED_FREEFALL`: this unit has been disabled via being in
    /// free fall.
    Freefall = 7,
    /// `DISABLED_AWESTRUCK`.
    Awestruck = 8,
    /// `DISABLED_BRAINWASHED`.
    Brainwashed = 9,
    /// `DISABLED_SUBDUED`: temporarily shut down by subdual damage.
    Subdued = 10,
    /// `DISABLED_SCRIPT_DISABLED`: specifically for scripts to
    /// enable/reenable.
    ScriptDisabled = 11,
    /// `DISABLED_SCRIPT_UNDERPOWERED`: specifically for scripts to
    /// enable/reenable.
    ScriptUnderpowered = 12,
}

/// `DISABLED_COUNT` (Common/DisabledTypes.h).
pub const DISABLED_COUNT: usize = 13;

/// The thirteen disabled types in enum order (the sweep's walk order).
pub const DISABLED_TYPES: [DisabledType; DISABLED_COUNT] = [
    DisabledType::Default,
    DisabledType::Hacked,
    DisabledType::Emp,
    DisabledType::Held,
    DisabledType::Paralyzed,
    DisabledType::Unmanned,
    DisabledType::Underpowered,
    DisabledType::Freefall,
    DisabledType::Awestruck,
    DisabledType::Brainwashed,
    DisabledType::Subdued,
    DisabledType::ScriptDisabled,
    DisabledType::ScriptUnderpowered,
];

impl DisabledType {
    /// The bit index (the C++ enum ordinal).
    #[must_use]
    pub fn index(self) -> usize {
        self as usize
    }
}

/// `BitFlags<DISABLED_COUNT, DisabledMaskTypeTag>` (Common/BitFlags.h) —
/// the operator subset the kernel drives: `set`, `test`, `any` and
/// `testForAll`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisabledMask {
    /// The raw bits (13 of them fit a `u32`).
    pub bits: u32,
}

impl DisabledMask {
    /// A mask with no bits set (`DISABLEDMASK_NONE`).
    #[must_use]
    pub const fn none() -> Self {
        Self { bits: 0 }
    }

    /// Sets or clears one bit; out-of-range indices are refused (the
    /// original's setDisabledUntil bounds check).
    pub fn set(&mut self, ty: DisabledType, value: bool) {
        let bit = 1u32 << ty.index();
        if value {
            self.bits |= bit;
        } else {
            self.bits &= !bit;
        }
    }

    /// Tests one bit.
    #[must_use]
    pub fn test(&self, ty: DisabledType) -> bool {
        (self.bits & (1u32 << ty.index())) != 0
    }

    /// `any()`: is any bit set.
    #[must_use]
    pub fn any(&self) -> bool {
        self.bits != 0
    }

    /// `testForAll(that)` (Common/BitFlags.h):
    /// `(m_bits & that.m_bits) == that.m_bits` — every set bit of the
    /// argument must be set in this mask. The sleepy dispatch's disabled
    /// gate (TheSuperHackers @bugfix Stubbjax 15/03/2026: the
    /// disabled-types-to-process mask is exclusive).
    #[must_use]
    pub fn test_for_all(&self, that: DisabledMask) -> bool {
        (self.bits & that.bits) == that.bits
    }
}

/// `ObjectShroudStatus` (GameCommon.h) — the whole-object shroud state
/// the store invalidates on movement (the recomputation itself is the
/// shroud slice).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(i32)]
pub enum ObjectShroudStatus {
    /// `OBJECTSHROUD_INVALID`: indeterminate state, will recompute.
    #[default]
    Invalid = 0,
    /// `OBJECTSHROUD_CLEAR`: completely visible.
    Clear = 1,
    /// `OBJECTSHROUD_PARTIAL_CLEAR`: partly clear (rest is shroud or fog).
    PartialClear = 2,
    /// `OBJECTSHROUD_FOGGED`: completely fogged.
    Fogged = 3,
    /// `OBJECTSHROUD_SHROUDED`: completely shrouded.
    Shrouded = 4,
    /// `OBJECTSHROUD_INVALID_BUT_PREVIOUS_VALID`: indeterminate, BUT
    /// previous status is valid, don't reset (used for save/load).
    InvalidButPreviousValid = 5,
}

/// The `OBJECT_STATUS_DESTROYED` bit position (the model-internal
/// numbering of the transcription this port mirrors — see
/// `tools/differential/gen_kernel_golden.cpp`; the real
/// ObjectStatusTypes.h ordinal is 1, but the number is pinned by the
/// golden dump only, the semantic is "destroyed").
pub const OBJECT_STATUS_DESTROYED_BIT: u32 = 24;

/// The `OBJECT_STATUS_NO_COLLISIONS` bit position (ObjectStatusTypes.h:
/// "object should be ignored for object-object collisions" — the real
/// ordinal is 5; the model-internal numbering continues past the model's
/// DESTROYED bit, pinned by the golden dump only). The collide family's
/// gates consult it: the processContactList pre-gate and the mid-walk
/// break of `Object::onCollide`.
pub const OBJECT_STATUS_NO_COLLISIONS_BIT: u32 = 25;

/// `hash2ints` (PartitionManager.cpp): the symmetric pair hash the
/// contact list's dedup buckets key on — `[a, b]` always hashes to the
/// same value as `[b, a]`.
#[must_use]
pub fn hash2ints(a: i32, b: i32) -> u32 {
    // this is unsophisticated but reasonable, since all ObjectIDs will
    // quite likely be well below 65536...
    if a < b {
        ((a as u32) << 16) + (b as u32)
    } else {
        ((b as u32) << 16) + (a as u32)
    }
}

/// `PartitionContactList::SOCKET_COUNT` (PartitionManager.cpp): 5381 —
/// "prime, and not too close to a power of 2".
pub const CONTACT_LIST_SOCKET_COUNT: usize = 5381;

/// `HUGE_DIST` (PartitionManager.h): the query family's "everything"
/// range — a `maxDist` at or above it skips the ring clamp entirely.
pub const HUGE_DIST: f32 = 1_000_000.0;

/// `HUGE_DIST_SQR` (PartitionManager.cpp): `HUGE_DIST * HUGE_DIST`, the
/// no-bound ceiling the measure helpers pass the dist procs.
pub const HUGE_DIST_SQR: f32 = HUGE_DIST * HUGE_DIST;

/// `DistanceCalculationType` (PartitionManager.h): the four measures,
/// in `theDistCalcProcs` order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(i32)]
pub enum DistanceCalculationType {
    /// `FROM_CENTER_2D`: measure from Object center in 2d.
    #[default]
    FromCenter2d = 0,
    /// `FROM_CENTER_3D`: measure from Object center in 3d.
    FromCenter3d = 1,
    /// `FROM_BOUNDINGSPHERE_2D`: measure from Object bounding sphere in
    /// 2d.
    FromBoundingsphere2d = 2,
    /// `FROM_BOUNDINGSPHERE_3D`: measure from Object bounding sphere in
    /// 3d.
    FromBoundingsphere3d = 3,
}

impl DistanceCalculationType {
    /// The wire value (the C++ enum ordinals; the table is indexed by
    /// it).
    #[must_use]
    pub fn bits(self) -> i32 {
        self as i32
    }

    /// The value from the wire ordinal (out-of-range values saturate to
    /// the last known type — the original's unchecked array read can't
    /// be reached through the typed port).
    #[must_use]
    pub fn from_bits(bits: i32) -> Self {
        match bits {
            0 => Self::FromCenter2d,
            1 => Self::FromCenter3d,
            2 => Self::FromBoundingsphere2d,
            _ => Self::FromBoundingsphere3d,
        }
    }
}

/// `IterOrderType` (ObjectIter.h): the iterator orders, in
/// `theClumpCompareProcs` order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(i32)]
pub enum IterOrderType {
    /// `ITER_FASTEST`: iterate in arbitrary order (no sort proc — the
    /// insertion order, which the prepend reverses).
    #[default]
    Fastest = 0,
    /// `ITER_SORTED_NEAR_TO_FAR`: nearest-to-farthest.
    SortedNearToFar = 1,
    /// `ITER_SORTED_FAR_TO_NEAR`: farthest-to-nearest.
    SortedFarToNear = 2,
    /// `ITER_SORTED_CHEAP_TO_EXPENSIVE`: by the template build cost,
    /// ascending.
    SortedCheapToExpensive = 3,
    /// `ITER_SORTED_EXPENSIVE_TO_CHEAP`: by the template build cost,
    /// descending.
    SortedExpensiveToCheap = 4,
}

/// `ObjectPrivateStatusBits` (Object.h): the per-object private bits the
/// query family's alive / on-map / same-map-status filters read (set by
/// the body modules' death paths and the ghost-object/script off-map
/// paths — those slices' setters; the corpora program the bits
/// directly).
pub const OBJECT_PRIVATE_EFFECTIVELY_DEAD_BIT: u32 = 0;

/// `OFF_MAP` (Object.h: "set to true if I am known to be OFF the current
/// map").
pub const OBJECT_PRIVATE_OFF_MAP_BIT: u32 = 3;

/// `UNDETECTED_DEFECTOR` (Object.h: `1 << 1` of `m_privateStatus` — "set
/// to true when I defect from my team; set to false when I attack
/// anything or when time runs out"). The relationship chain's defector
/// gates consult it; the setters are the defect/capture paths' (a later
/// slice; the corpora program the bit).
pub const OBJECT_PRIVATE_UNDETECTED_DEFECTOR_BIT: u32 = 1;

/// `Relationship` (GameCommon.h): the alliance state between teams and
/// players. Keep `TheRelationShipNames` in sync —
/// [`RELATIONSHIP_NAMES`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
#[repr(i32)]
pub enum Relationship {
    /// `ENEMIES` = 0.
    Enemies = 0,
    /// `NEUTRAL` = 1 (the default verdict of every unresolved chain).
    #[default]
    Neutral = 1,
    /// `ALLIES` = 2.
    Allies = 2,
}

impl Relationship {
    /// The wire value (the C++ enum ordinals).
    #[must_use]
    pub fn bits(self) -> i32 {
        self as i32
    }

    /// `TheRelationShipNames` (GameCommon.cpp), in enum order.
    #[must_use]
    pub fn name(self) -> &'static str {
        RELATIONSHIP_NAMES[self as usize]
    }
}

/// `TheRelationShipNames` (GameCommon.cpp).
pub const RELATIONSHIP_NAMES: [&str; 3] = ["Enemies", "Neutral", "Allies"];

/// `PlayerMaskType` (GameCommon.h): the 16-player mask word
/// (`MAX_PLAYER_COUNT <= 16` fits an `UnsignedShort`).
pub type PlayerMaskType = u16;

/// `PLAYERMASK_ALL` (GameCommon.h).
pub const PLAYERMASK_ALL: PlayerMaskType = 0xffff;

/// `PLAYERMASK_NONE` (GameCommon.h).
pub const PLAYERMASK_NONE: PlayerMaskType = 0x0000;

/// `PLAYER_INDEX_INVALID` (Player.h): `-1` — the relation maps' clear-all
/// key and the invalid-player sentinel.
pub const PLAYER_INDEX_INVALID: i32 = -1;

/// `AllowPlayerRelationship` (PlayerList.h): the
/// `getPlayersWithRelationship` selector bits — a DIFFERENT flag set from
/// the partition filter's relationship allows (which key on
/// `1 << Relationship`).
pub const ALLOW_SAME_PLAYER: u32 = 0x01;

/// `ALLOW_ALLIES` (PlayerList.h).
pub const ALLOW_ALLIES: u32 = 0x02;

/// `ALLOW_ENEMIES` (PlayerList.h).
pub const ALLOW_ENEMIES: u32 = 0x04;

/// `ALLOW_NEUTRAL` (PlayerList.h).
pub const ALLOW_NEUTRAL: u32 = 0x08;

/// `TeamID` (Team.h): the team-instance word; `0` is
/// [`TEAM_ID_INVALID`] — the factories' `++m_uniqueTeamID`
/// pre-increments hand out ids from 1, so no live team ever carries 0.
pub type TeamID = u32;

/// `TEAM_ID_INVALID` (Team.h): `#define TEAM_ID_INVALID 0`.
pub const TEAM_ID_INVALID: TeamID = 0;

/// `TeamPrototypeID` (Team.h): the prototype word; `0` is invalid, ids
/// from 1.
pub type TeamProtoID = u32;

/// `TEAM_PROTOTYPE_ID_INVALID` (Team.h): `#define
/// TEAM_PROTOTYPE_ID_INVALID 0`.
pub const TEAM_PROTO_ID_INVALID: TeamProtoID = 0;

/// `PlayerType` (GameCommon.h): `PLAYER_HUMAN`, `PLAYER_COMPUTER`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(i32)]
pub enum PlayerType {
    /// `PLAYER_HUMAN` = 0: human-controlled.
    Human = 0,
    /// `PLAYER_COMPUTER` = 1: computer-controlled (the birth default).
    #[default]
    Computer = 1,
}

impl PlayerType {
    /// The wire value (the C++ enum ordinals).
    #[must_use]
    pub fn bits(self) -> i32 {
        self as i32
    }
}

/// `MAX_SLOTS` (NetworkDefs.h): `MAX_PLAYER + 1` = 8 — the slot
/// bookkeeping bound.
pub const MAX_SLOTS: usize = 8;
