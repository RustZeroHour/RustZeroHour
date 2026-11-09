// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Golden-vector driver for `rzh-sim` (parity level P0 for the
// discrete kernel state, P2 for the float-touched coverage geometry).
// Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Verbatim transcription of the Zero Hour GAME-LOGIC KERNEL — the first
// Phase 3 slice (plan §4 WP1: frame loop, object model, partition
// manager) — the deterministic machinery every later simulation slice
// builds on:
//
//   - GameLogic.cpp's frame loop: GameLogic::update (the ordered subsystem
//     walk — the client frame hand-off, the script engine, the time-freeze
//     gate, the terrain, the recorder/CRC position, the command list, the
//     sleepy updates, the AI, the build assistant, the partition manager,
//     the destroy list, the command-list reset, the weapon/locomotor/
//     victory stores, the disabled-status sweep and the frame increment)
//     and GameLogic::preUpdate (the pause-frame transition);
//   - the object model's registry half: allocateObjectID (the counter
//     scheme), registerObject (the object-list prepend, the ID lookup
//     table and the initial sleepy wake — getFrame() == 0 becomes 1),
//     destroyObject (the destroyed re-refusal, the DESTROYED status, the
//     destroy-list append) and processDestroyList (the two-pass sleepy
//     erase with its MAX_SUO = 256 cap, the destroy-during-destroy
//     same-pass append, the list removal and the lookup erase), plus the
//     loadPostProcess ID compaction (m_nextObjID = max(id) + 1 walked
//     from INVALID_ID) and heap rebuild (the flat push and the bottom-up
//     remakeSleepyUpdate heapify);
//   - the sleepy-update scheduler: the packed m_nextCallFrameAndPhase
//     priority (frame << 2 | phase), the setNextCallFrame saturation at
//     UPDATE_SLEEP_FOREVER with the phase re-derived from the module's
//     getUpdatePhase() override, isLowerPriority, pushSleepyUpdate,
//     peekSleepyUpdate, popSleepyUpdate, eraseSleepyUpdate,
//     rebalanceParentSleepyUpdate, rebalanceChildSleepyUpdate (the
//     pristine non-unrolled variant; the hand-unrolled original is
//     behaviourally identical), rebalanceSleepyUpdate, remakeSleepyUpdate,
//     friend_awakenUpdateModule (setWakeFrame's path with the
//     "already awake, wake at now + 1" refusal and the not-in-list
//     initialization case), UpdateModule::frameToSleepTime / setWakeFrame
//     and the update() dispatch loop (the > now break, the disabled gate,
//     the return-code clamp and the rebalance-from-root requeue);
//   - Object.cpp's kernel state: the geometry (GeometryInfo), the
//     position/orientation, the kindof IMMOBILE bit, the disabled mask
//     and till-frames (setDisabledUntil's frame > now mask write,
//     checkDisabledStatus's now >= tillFrame re-enable), the position and
//     angle change thresholds of reactToTransformChange (0.01) and the
//     setGeometryInfo unregister/re-register refresh;
//   - PartitionManager.cpp's spatial store: init (the cell-size clamp,
//     the double-division cellSizeInv, the zero-extent patch, the CEIL
//     cell counts), shutdown, update (the dirty-module refresh with its
//     LIFO processing, the updateCells / collision-check gates, the
//     KINDOF_IMMOBILE collision refusal, the contact-list pair
//     collection and processContactList — the collide-test family and
//     the CollideModule dispatch), registerObject / unRegisterObject (the
//     master-list prepend, the double-register refusal), PartitionData
//     (the COI array with calcMaxCoiForShape, allocCoiArray /
//     freeCoiArray, makeDirty's status upgrades, removeAllTouchedCells,
//     addSubPixToCoverage's rescan, updateCellsTouched with doSmallFill
//     (the half-cell radius truncation), doCircleFillPrecise (the
//     non-retail bugfix branch — Stubbjax 29/01/2026 — with
//     doesCircleOverlapCell's strict inequality) and doRectFill (the
//     half-step fudge, the CEIL step counts and the rotated walk), the
//     last-cell change hook and the shroudedness invalidation), the
//     CellAndObjectIntersection coverage (the cell-list prepend and the
//     ordered unlink) and PartitionContactList::addToContactList (the
//     hash2ints symmetric hash, the 5381 sockets, the either-order dedup
//     and the LIFO pair list);
//   - PartitionManager.cpp's collide-test family: CollideInfo /
//     CollideLocAndNormal, rectToFourPoints and
//     testRotatedPointsAgainstRect (the inverse rotation with
//     WWMath::Fabs), the xy_collideTest_Rect_Rect / Rect_Circle /
//     Circle_Rect / Circle_Circle quartet (Rect_Circle promotes the
//     circle to a square — setMinorRadius(majorRadius) — and defers to
//     Rect_Rect; Circle_Rect is Rect_Circle with the flipped normal),
//     z_collideTest_Sphere_Nonsphere (the center-within-z case, the
//     below/above slice cases with the sphere-slice radius via
//     WWMath::Sqrtf) and z_collideTest_Nonsphere_Nonsphere (the
//     minRadius-squared closeEnough preflight whose true path leaves
//     loc.x/y and normal unwritten — the original's uninitialized stack
//     local, pinned as the zero here), the nine collideTest_X_Y procs
//     and theCollideTestProcs table (indexed (this - GEOMETRY_FIRST) *
//     GEOMETRY_NUM_TYPES + (that - GEOMETRY_FIRST)); PartitionData::
//     collidesWith (the KINDOF_NO_COLLIDE refusal, the z-intersection
//     gate with the getMaxHeightBelowPosition extents and the table
//     dispatch) and PartitionManager::geomCollidesWithGeom (the
//     standalone test whose z-gate ignores the below-extents — a
//     narrower gate than collidesWith's, pinned by the corpora);
//   - PartitionContactList::processContactList: the null-node skip, the
//     friend_collidesWith call, the OBJECT_STATUS_NO_COLLISIONS gates,
//     the node nulling before the dispatch, Object::onCollide's
//     behavior walk (the per-module NO_COLLISIONS re-check with its
//     mid-walk break) with the flipped-normal second call gated by both
//     sides' isDestroyed, and the makeDirty(false) re-dirty family with
//     its destroyed / unregistered / IMMOBILE refusals — over
//     Object.cpp's onCollide and Geometry.cpp's getMaxHeightAbovePosition
//     / getMaxHeightBelowPosition / setMajorRadius / setMinorRadius;
//   - PartitionManager.cpp's query family (the FASTER_GCO build):
//     calcMinRadius (the 4x4 corner-pair minimum over the cell corners,
//     the promoted square-root and the CEIL cell-radius) and calcRadiusVec
//     (the m_maxGcoRadius bound from the diagonal, the per-radius offset
//     rings in the y-outer/x-inner scan order), the four distCalcProc_X_Y
//     distance measures (center/center and boundary/boundary in 2d and
//     3d — the boundary pair's bounding-circle/sphere shrink with its
//     clamped-to-zero branch, the 3d z delta through
//     getZDeltaToCenterPosition) and theDistCalcProcs, getClosestObjects
//     (the ring walk with the done-flag dedup, the closestDistSqr
//     tightening, the iterArg insert-vs-closest split, the
//     maxRadiusLimit = curRadius early exit, the non-retail closestVec
//     prefill of maxDist^3), getClosestObject / iterateObjectsInRange /
//     iteratePotentialCollisions (the bounding-sphere 1.1 slop and the
//     PartitionFilterWouldCollide filter) / iterateAllObjects, the
//     getVectorTo / getDistanceSquared / getGoalDistanceSquared measure
//     entries and getRelativeAngle2D (the unit-direction dot, the [-1,1]
//     clamp, ACos and the perp-Z sign flip) — over SimpleObjectIterator.cpp
//     (the clump list with its prepend insert, nextWithNumeric, reset,
//     makeEmpty and the bottom-up linked-list mergesort of sort() with
//     the five compare procs), ObjectIter.h (IterOrderType), the
//     PartitionFilter base and the wouldCollide / acceptByKindOf /
//     rejectByKindOf / alive / onMap / sameMapStatus filters over
//     Object's isKindOfMulti / isEffectivelyDead / isOffMap, and Thing.cpp's
//     getUnitDirectionVector2D (Cos/Sin of the facing angle).
//
// The frame constants are pinned from the sources: WWSyncPerSecond = 30
// (WWLib/WWCommon.h), LOGICFRAMES_PER_SECOND = WWSyncPerSecond,
// MSEC_PER_LOGICFRAME_REAL = 1000 / 30 (GameCommon.h) — the "30 Hz in
// retail, to verify" item of plan §4 WP1.
//
// Deliberate deviations, pinned identically on both sides:
//
// - The RETAIL_COMPATIBLE branches are dropped per plan §3.3 rule 1 (the
//   oracle builds with the switches off): doCircleFillPrecise is THE
//   circle fill (the retail midpoint-circle and its radii >= 240
//   divergence is not transcribed), calcMaxCoiForShape takes its break
//   (the retail fallthrough into the BOX diagonal is not), and the
//   disabled gate is the exclusive mask test
//   getDisabledTypesToProcess().testForAll(disabled) (Stubbjax
//   15/03/2026), not the any-intersection test.
// - The subsystem UPDATE() calls the kernel orders but does not own
//   (ScriptEngine, TerrainLogic, the recorder/CRC block, the command
//   list, AI, BuildAssistant, WeaponStore, LocomotorStore,
//   VictoryConditions, the client frame hand-off and the startNewGame
//   kick) are counted no-op hooks: the ORDER is pinned by the dumps, the
//   bodies are later slices (WP7, movement, net/replay, WP1's GameMessage
//   processing, WP6, the economy, the stores). The CRC generation gates
//   need TheRecorder/TheGameInfo and are stated, not modelled — the
//   recorder hook covers the position.
// - The FramePacer's full pause machinery (music/input gates, render
//   pacing) is client-side and a later phase; the kernel sees one
//   observable: the time-freeze gate after the script-engine step. The
//   corpus controls it directly. preUpdate's pause-frame transition sets
//   the dumped m_gamePaused flag.
// - Pointers become stable handles (object ids, module indices) so the
//   dumps are machine-stable; the object store never reuses a slot, so
//   the original's freed-memory reads (the > 256 sleepy-erase cap) stay
//   out of the corpora — the cap itself is pinned at its boundary
//   (exactly 256 modules all erase) and the beyond-cap dangling is
//   stated as a contract, not replayed.
// - The ID lookup table is a linear array in the transcription (the
//   original's hash_map is only ever probed by key here, never iterated;
//   no observable depends on its bucket order).
// - lroundf is the generator platform's (glibc/musl agree on the
//   in-range domain the corpora use: post-floor/ceil integral values).
//   The Rust port transcribes round-half-away-from-zero semantics for
//   the same domain; the out-of-domain behaviour (|x| >= 2^31, NaN) is a
//   stated contract.
// - The contact list's processContactList is transcribed with the
//   collide-test family (this slice): the pair collection the store walk
//   produces is captured as the dump observable BEFORE the processing
//   mutates the nodes. The original's CollideLocAndNormal stack locals
//   are uninitialized; the closeEnough preflight of
//   z_collideTest_Nonsphere_Nonsphere returns true without writing
//   loc.x/y or the normal, so the original hands uninitialized memory
//   into Object::onCollide on that path — the transcription and the port
//   zero-initialize instead (the garbage becomes the pinned zero; the
//   corpora exercise the path).
// - GeometryInfo's bounding-radius recalculation (calcBoundingStuff) is
//   transcribed as on-demand getters (getBoundingCircleRadius /
//   getBoundingSphereRadius): the original caches the two radii at
//   construction and recomputes in every setter (the fields are private,
//   so the cache can never go stale); the transcription and the port
//   recompute from the same five inputs, which yields the identical
//   values the cached original reads. The dead five-argument forward
//   declaration of xy_collideTest_Circle_Circle (declared, never defined,
//   never called) is not transcribed.
// - The query family's per-object inputs that later slices own are plain
//   ObjectT fields the corpora program post-creation: the template's
//   build cost (a float — the cost-sorted iterators read it through
//   getTemplate()->friend_getBuildCost(); the template itself is the
//   rzh-data object model's) and the private-status bits
//   EFFECTIVELY_DEAD / OFF_MAP (set by the body modules and the
//   ghost-object paths — their setters are those slices'). The
//   players/teams/locomotors filters (SamePlayer, Relationship, Team,
//   Squad, LineOfSight, StealthedAndUndetected, IsFlying, ...) read
//   state those slices own and are not transcribed; the filter FRAMEWORK
//   (the PartitionFilter base and filtersAllow) and every filter over
//   state this slice owns are.
// - SimpleObjectIterator is a plain by-value struct: the original is a
//   MemoryPoolObject the caller news and deletes (the holder dance of
//   iterateObjectsInRange et al.); the pointer is never observable, the
//   list order is. getClosestObjects' function-static theIterFlag (the
//   per-call generation counter the done-flag dedup keys on) becomes
//   PartitionManager state — each corpus's fresh World starts it at 1
//   exactly as the original's first call would see it.
// - The collide-module bodies are scripted corpus effects (the absorb /
//   destroy / unregister triple, applied in that order inside the
//   dispatch) exactly as the update-module bodies are — the real module
//   bodies are the WP2 module framework's. The collide dispatches are a
//   separate event stream (nothing else emits while processContactList
//   runs, so the order against the main stream is fixed).
// - Ghost objects (the fog-memory half of unRegisterObject) need draw
//   -module knowledge and are not in this slice; the branch is
//   transcribed with its condition and never taken by the corpora.
// - The MapObject/Team/Player layers do not exist yet; the corpora
//   construct objects directly (the ThingFactory::newObject shape: the
//   ID allocation, the module construction, the GameLogic registration,
//   the partition registration).
//
// Temporary differential-testing infrastructure: removed at cut-over (M5).
//
// This program is free software; you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the
// Free Software Foundation, either version 3 of the License, or (at your
// option) any later version.

#include <algorithm>
#include <array>
#include <bit>
#include <cmath>
#include <cstdarg>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <memory>
#include <string>
#include <vector>

// The pinned GameMath sources (TheSuperHackers/GameMath @ 59f7ccd4)
// supply the deterministic trig (USE_DETERMINISTIC_MATH = 1 in
// Lib/BaseDefines.h, always on for this code base).
extern "C" {
float gm_sinf(float);
float gm_cosf(float);
float gm_sqrtf(float);
float gm_fabsf(float);
float gm_acosf(float);
}

// ============================================================================
// Lib/Trig.cpp — the global entry points PartitionManager.cpp calls
// (unqualified Cos/Sin resolve here through Lib/trig.h).
// ============================================================================
static inline float Cos(float x) { return gm_cosf(x); }
static inline float Sin(float x) { return gm_sinf(x); }
// WWMath::Sqrtf, deterministic branch.
static inline float Sqrtf(float x) { return gm_sqrtf(x); }
// WWMath::Fabs, deterministic branch (wwmath.h: gm_fabsf).
static inline float Fabs(float x) { return gm_fabsf(x); }
// ACos (Lib/trig.h → Lib/Trig.cpp), deterministic branch.
static inline float ACos(float x) { return gm_acosf(x); }

// ============================================================================
// Lib/BaseFunctions.h — the Hoffesommer conversions, non-VC6 branch, as
// the Zero Hour non-retail build defines them (RTS_GENERALS && RETAIL_
// COMPATIBLE_CRC is false, so REAL_TO_INT_* use the fast floors).
// ============================================================================

// The bit-cast helpers (C++17-compatible; the original's std::bit_cast is C++20).
static inline unsigned bits_of(float f)
{
    unsigned u;
    memcpy(&u, &f, 4);
    return u;
}
static inline float float_of(unsigned u)
{
    float f;
    memcpy(&f, &u, 4);
    return f;
}

static inline float fast_float_trunc(float f)
{
    unsigned x = bits_of(f);
    unsigned char exp = x >> 23;
    int mask = exp < 127 ? 0 : 0xff800000;
    exp -= 127; // unsigned char: wraps mod 256
    mask >>= exp & 31;
    x &= mask;
    return float_of(x);
}

static inline float fast_float_floor(float f)
{
    static unsigned almost1 = (126 << 23) | 0x7fffff;
    if (bits_of(f) & 0x80000000)
        f -= float_of(almost1);
    return fast_float_trunc(f);
}

static inline float fast_float_ceil(float f)
{
    static unsigned almost1 = (126 << 23) | 0x7fffff;
    if ((bits_of(f) & 0x80000000) == 0)
        f += float_of(almost1);
    return fast_float_trunc(f);
}

static inline long fast_float2long_round(float f) { return lroundf(f); }

static inline long REAL_TO_INT_CEIL(float x) { return fast_float2long_round(fast_float_ceil(x)); }
static inline long REAL_TO_INT_FLOOR(float x) { return fast_float2long_round(fast_float_floor(x)); }

// ============================================================================
// The kernel's constants (GameCommon.h, WWCommon.h, GameType.h,
// UpdateModule.h, DisabledTypes.h, Geometry.h, KindOf.h).
// ============================================================================
enum {
    MAX_PLAYER_COUNT = 16,
};
// Team.h: the team-id word and its invalid (both factories' ++m_uniqueXXX
// pre-increments hand out ids from 1, so no live team ever carries 0).
typedef unsigned TeamIDT;
static const TeamIDT TEAM_ID_T_INVALID = 0;
// Object.h: UNDETECTED_DEFECTOR = (1 << 1) of m_privateStatus (set by the
// defector paths — a later slice; the corpora program the bit).
static const unsigned OBJECT_PRIVATE_UNDETECTED_DEFECTOR_BIT = 1;
enum {
    NEVER = 0,
    FOREVER = 0x3fffffff, // (we use 0x3fffffff so that we can add offsets and not overflow...)
};
enum ObjectIDEnum {
    INVALID_ID = 0,
    FORCE_OBJECTID_TO_LONG_SIZE = 0x7ffffff,
};
enum {
    WWSyncPerSecond = 30, // WWLib/WWCommon.h
    WWSyncMilliseconds = 1000 / WWSyncPerSecond,
    LOGICFRAMES_PER_SECOND = WWSyncPerSecond, // GameCommon.h
    MSEC_PER_SECOND = 1000,
};
// GameCommon.h's derived reals (const Real at namespace scope):
static const float LOGICFRAMES_PER_MSEC_REAL = (((float)LOGICFRAMES_PER_SECOND) / ((float)MSEC_PER_SECOND));
static const float MSEC_PER_LOGICFRAME_REAL = (((float)MSEC_PER_SECOND) / ((float)LOGICFRAMES_PER_SECOND));
static const float LOGICFRAMES_PER_SECONDS_REAL = (float)LOGICFRAMES_PER_SECOND;
static const float SECONDS_PER_LOGICFRAME_REAL = 1.0f / LOGICFRAMES_PER_SECONDS_REAL;

enum UpdateSleepTime {
    UPDATE_SLEEP_INVALID = 0,
    UPDATE_SLEEP_NONE = 1,
    UPDATE_SLEEP_FOREVER = 0x3fffffff, // ~414 days at 30fps
};

enum SleepyUpdatePhase {
    PHASE_INITIAL = 0,
    PHASE_PHYSICS = 1,
    PHASE_NORMAL = 2,
    PHASE_FINAL = 3,
};

enum GeometryType {
    GEOMETRY_SPHERE,   // partition/collision testing as sphere. (majorRadius = radius)
    GEOMETRY_CYLINDER, // partition/collision testing as cylinder. (majorRadius = radius, height = height)
    GEOMETRY_BOX,      // partition/collision testing as rectangular box (majorRadius = half len in forward dir; minorRadius = half len in side dir)
    GEOMETRY_NUM_TYPES,
    GEOMETRY_FIRST = 0,
};

enum DisabledType {
    DISABLED_DEFAULT,      // Typical disable -- like systems, things that don't need to run.
    DISABLED_HACKED,       // This unit has been hacked
    DISABLED_EMP,          // This unit has been disabled via electro-magnetic-pulse.
    DISABLED_HELD,         // Special case -- held means it can fire and isHeld checks to make sure ONLY held is set!
    DISABLED_PARALYZED,    // Battle plans have changed, and unit is confused/paralyzed
    DISABLED_UNMANNED,     // Vehicle is unmanned
    DISABLED_UNDERPOWERED, // Separate from ScriptUnderpowered, the owning player has insufficient power.  Energy status controls this
    DISABLED_FREEFALL,     // This unit has been disabled via being in free fall
    DISABLED_AWESTRUCK,
    DISABLED_BRAINWASHED,
    DISABLED_SUBDUED,      ///< Temporarily shut down by Subdual damage
    // These ones are specifically for scripts to enable/reenable!
    DISABLED_SCRIPT_DISABLED,
    DISABLED_SCRIPT_UNDERPOWERED,
    DISABLED_COUNT,
    DISABLED_ANY = 65535, ///< Do not use this value for setting disabled types (read-only)
};

enum ObjectShroudStatus {
    OBJECTSHROUD_INVALID,                     ///< indeterminate state, will recompute
    OBJECTSHROUD_CLEAR,                       ///< object is not shrouded at all (ie, completely visible)
    OBJECTSHROUD_PARTIAL_CLEAR,               ///< object is partly clear (rest is shroud or fog)
    OBJECTSHROUD_FOGGED,                      ///< object is completely fogged
    OBJECTSHROUD_SHROUDED,                    ///< object is completely shrouded
    OBJECTSHROUD_INVALID_BUT_PREVIOUS_VALID,  ///< indeterminate, BUT previous status is valid (save/load)
};

// The kindof bit the kernel consults (the full 122-entry table is the
// rzh-data layer's; the store needs exactly this one).
enum {
    KINDOF_IMMOBILE_BIT = 2, // KindOf.h: KINDOF_OBSTACLE=0, SELECTABLE=1, IMMOBILE=2
};

// The ObjectStatus bit destroyObject sets (ObjectStatusTypes.h:
// OBJECT_STATUS_DESTROYED — bit 24 in the 32-bit mask; the number is
// pinned by the transcription's dump only, the semantic is "destroyed").
enum {
    OBJECT_STATUS_DESTROYED_BIT = 24,
};

// The ObjectStatus bit the collide family consults (ObjectStatusTypes.h:
// OBJECT_STATUS_NO_COLLISIONS — "object should be ignored for object-
// object collisions"; the real ordinal is 5, but the transcription keeps
// its model-internal numbering past the model's DESTROYED bit — the
// number is pinned by the transcription's dump only, the semantic is
// "no collisions").
enum {
    OBJECT_STATUS_NO_COLLISIONS_BIT = 25,
};

// The second kindof bit the collide family consults (KindOf.h, the Zero
// Hour build with ALLOW_SURRENDER and RTS_GENERALS both off — the same
// configuration the generated KINDOF_NAMES table of the script-engine
// slice pinned: KINDOF_NO_COLLIDE = 27, "Never collide with or be
// collided with").
enum {
    KINDOF_NO_COLLIDE_BIT = 27, // KindOf.h (ALLOW_SURRENDER off)
};

// PartitionManager.h's HUGE_DIST and its square (the "everything" range
// and the no-bound measure ceiling of the query family).
static const float HUGE_DIST = 1000000.0f;
static const float HUGE_DIST_SQR = HUGE_DIST * HUGE_DIST;

// DistanceCalculationType (PartitionManager.h): the four measures, in
// theDistCalcProcs order.
enum DistanceCalculationType {
    FROM_CENTER_2D,           ///< measure from Object center in 2d.
    FROM_CENTER_3D,           ///< measure from Object center in 3d.
    FROM_BOUNDINGSPHERE_2D,   ///< measure from Object bounding sphere in 2d.
    FROM_BOUNDINGSPHERE_3D,   ///< measure from Object bounding sphere in 3d.
    DISTANCE_CALCULATION_TYPE_COUNT,
};

// IterOrderType (ObjectIter.h): the iterator orders, in
// theClumpCompareProcs order.
enum IterOrderType {
    ITER_FASTEST,                    ///< iterate in arbitrary order
    ITER_SORTED_NEAR_TO_FAR,         ///< iterate in nearest-to-farthest order (may be slower)
    ITER_SORTED_FAR_TO_NEAR,         ///< iterate in farthest-to-nearest order (may be slower)
    ITER_SORTED_CHEAP_TO_EXPENSIVE,  ///< iterate in cheapest-to-most-expensive order (slower)
    ITER_SORTED_EXPENSIVE_TO_CHEAP,  ///< iterate in most-expensive-to-cheapest order (slower)
    ITER_ORDER_TYPE_COUNT,
};

// ObjectPrivateStatusBits (Object.h): the per-object private bits the
// query family's alive / onMap / sameMapStatus filters read (set by the
// body modules' death paths and the ghost-object/script off-map paths —
// those slices' setters; the corpora program the bits directly).
enum {
    OBJECT_PRIVATE_EFFECTIVELY_DEAD_BIT = 0, // "Object is effectively dead"
    OBJECT_PRIVATE_OFF_MAP_BIT = 3,          // "set to true if I am known to be OFF the current map"
};

// ============================================================================
// The transcription's model objects. Handles are stable ids: objects by
// their ObjectID (never reused), update modules by (object id, module
// index), partition data by object id.
// ============================================================================
struct PartitionManagerT;
struct PartitionCell;
struct World;

// --- BitFlags<DISABLED_COUNT> (Common/BitFlags.h subset: set/test/any/
// testForAll — the operators the kernel drives). --------------------------
struct DisabledMask {
    unsigned bits = 0; // DISABLED_COUNT = 13 <= 32
    void set(int bit, bool value)
    {
        if (bit < 0 || bit >= DISABLED_COUNT)
            return;
        if (value)
            bits |= (1u << bit);
        else
            bits &= ~(1u << bit);
    }
    bool test(int bit) const { return bit >= 0 && bit < DISABLED_COUNT && (bits & (1u << bit)) != 0; }
    bool any() const { return bits != 0; }
    // testForAll: (m_bits & that.m_bits) == that.m_bits — every set bit of
    // the argument must be set in this mask.
    bool testForAll(const DisabledMask &that) const { return (bits & that.bits) == that.bits; }
};

// --- UpdateModule (GameLogic/Module/UpdateModule.h) ------------------------
struct UpdateModuleT {
    // actually, it's not a real frame at all, it has phase info in the lower bits...
    // (UnsignedInt in the original — the saturated FOREVER packed value
    // 0xfffffffe must compare as the LOWEST priority, which it only does
    // unsigned)
    unsigned m_nextCallFrameAndPhase = 0; // the ctor zero (the "never set" legacy value)
    int m_indexInLogic = -1;
    int m_phase = PHASE_NORMAL; // getUpdatePhase() override point
    DisabledMask m_disabledTypesToProcess; // getDisabledTypesToProcess() override point

    // --- the corpus scripting surface (the module bodies are later slices):
    int m_scriptedSleep = UPDATE_SLEEP_NONE; // update()'s return code
    // optional side effects the corpus programs, run where the original's
    // module body would run them (inside update()):
    int m_onUpdateWakeObj = INVALID_ID; // setWakeFrame on another module
    int m_onUpdateWakeMod = -1;
    int m_onUpdateWakeWhen = -1; // absolute frame
    int m_onUpdateDestroyObj = INVALID_ID; // GameLogic::destroyObject request
};

static inline unsigned friend_getPriority(const UpdateModuleT *u) { return u->m_nextCallFrameAndPhase; }
static inline int friend_getNextCallFrame(const UpdateModuleT *u) { return (int)(u->m_nextCallFrameAndPhase >> 2); }
static inline void friend_setNextCallFrame(UpdateModuleT *u, unsigned frame)
{
    // anything greater than "forever" is still "forever"
    // (this makes setWakeFrame() comparisons simpler and more efficient)
    if (frame > (unsigned)UPDATE_SLEEP_FOREVER)
        frame = (unsigned)UPDATE_SLEEP_FOREVER;
    u->m_nextCallFrameAndPhase = (frame << 2) | (unsigned)u->m_phase; // (frame << 2) | getUpdatePhase()
}

// --- Coord3D / GeometryInfo (Lib/Coord3D.h, Common/Geometry.h) -------------
struct Coord3D {
    float x = 0, y = 0, z = 0;

    // Coord3D::normalize (Lib/Coord3D.h): the length via the unqualified
    // Sqrt (Lib/trig.h — gm_sqrtf in the deterministic build) and the
    // len != 0 guard.
    void normalize()
    {
        float len = Sqrtf(x * x + y * y + z * z);
        if (len != 0) {
            x /= len;
            y /= len;
            z /= len;
        }
    }
};

// Coord2D (Lib/Coord2D.h) — the rect corners.
struct Coord2DT {
    float x = 0, y = 0;
};

// GeometryInfo's collide-and-query-relevant surface (Geometry.h /
// Geometry.cpp). The bounding-circle/sphere radii are calcBoundingStuff's
// formulas computed on demand (the original caches them at construction
// and recomputes in every setter — the private fields can never go stale,
// so the on-demand form reads the identical values).
struct GeometryInfoT {
    int geomType = GEOMETRY_SPHERE;
    bool isSmall = false;
    float height = 0, majorRadius = 0, minorRadius = 0;

    int getGeomType() const { return geomType; }
    float getMajorRadius() const { return majorRadius; }
    float getMinorRadius() const { return minorRadius; }
    void setMajorRadius(float r)
    {
        majorRadius = r;
        // (calcBoundingStuff — recomputed on demand by the getters below)
    }
    void setMinorRadius(float r)
    {
        minorRadius = r;
        // (calcBoundingStuff — recomputed on demand by the getters below)
    }

    // GeometryInfo::calcBoundingStuff (Geometry.cpp), the
    // m_boundingCircleRadius half: the circle radius of the shape's
    // footprint — the major radius for spheres and cylinders, the
    // diagonal for boxes.
    float getBoundingCircleRadius() const
    {
        switch (geomType) {
        case GEOMETRY_SPHERE:
        case GEOMETRY_CYLINDER:
            return majorRadius;
        case GEOMETRY_BOX:
            return Sqrtf(majorRadius * majorRadius + minorRadius * minorRadius);
        }
        return 0.0f;
    }

    // calcBoundingStuff, the m_boundingSphereRadius half: the sphere
    // radius of the full 3d shape — the radius for spheres, the
    // max of the half-height and the radius for cylinders, the full
    // 3d diagonal for boxes.
    float getBoundingSphereRadius() const
    {
        switch (geomType) {
        case GEOMETRY_SPHERE:
            return majorRadius;
        case GEOMETRY_CYLINDER: {
            float r = height * 0.5f;
            if (r < majorRadius)
                r = majorRadius;
            return r;
        }
        case GEOMETRY_BOX:
            return Sqrtf(majorRadius * majorRadius + minorRadius * minorRadius +
                         (height * 0.5f) * (height * 0.5f));
        }
        return 0.0f;
    }

    // GeometryInfo::getZDeltaToCenterPosition (Geometry.cpp): where the
    // "center" of the geometry is, relative to the canonical position —
    // the half-height for the grounded shapes, nothing for spheres.
    float getZDeltaToCenterPosition() const
    {
        return (geomType == GEOMETRY_SPHERE) ? 0.0f : (height * 0.5f);
    }

    // Geometry.cpp: given an object with this geom, how far above the
    // object's canonical position does its max z extend?
    float getMaxHeightAbovePosition() const
    {
        switch (geomType) {
        case GEOMETRY_SPHERE:
            return majorRadius;
        case GEOMETRY_BOX:
        case GEOMETRY_CYLINDER:
            return height;
        };
        return 0.0f;
    }

    // Geometry.cpp: ... and how far below.
    float getMaxHeightBelowPosition() const
    {
        switch (geomType) {
        case GEOMETRY_SPHERE:
            return majorRadius;
        case GEOMETRY_BOX:
        case GEOMETRY_CYLINDER:
            return 0.0f;
        };
        return 0.0f;
    }
};

// --- the collide module interface (Object.cpp's onCollide walk) -------------
// The behavior modules whose getCollide() returns non-null. The corpus
// scripts each module's body with the same field shape the update
// modules use; the effects run where the original's module body would
// run them (inside CollideModule::onCollide).
struct CollideModuleT {
    bool m_onCollideAbsorb = false;         // sets OBJECT_STATUS_NO_COLLISIONS on the owner
    int m_onCollideDestroyObj = INVALID_ID;  // GameLogic::destroyObject request
    int m_onCollideUnregisterObj = INVALID_ID; // PartitionManager::unRegisterObject request
};

// --- Object (GameLogic/Object.h — the kernel's state half) -----------------
struct ObjectT {
    int m_id = INVALID_ID;
    GeometryInfoT m_geometryInfo;
    Coord3D m_pos;
    float m_angle = 0;
    unsigned m_kindof = 0; // the bitset; IMMOBILE is what the store consults
    unsigned m_status = 0; // ObjectStatusMaskType
    unsigned m_privateStatus = 0; // ObjectPrivateStatusBits (EFFECTIVELY_DEAD, OFF_MAP)
    float m_buildCost = 0.0f;     // the template's friend_getBuildCost (the template is rzh-data's)
    DisabledMask m_disabledMask;
    int m_disabledTillFrame[DISABLED_COUNT] = {0};
    std::vector<UpdateModuleT> m_updates;
    std::vector<CollideModuleT> m_colliders; // the behavior list's collide modules (onCollide's walk)
    int m_partitionData = INVALID_ID; // the object-id handle of the PartitionData
    int m_cellChangeCount = 0;        // onPartitionCellChange() invocations
    TeamIDT m_team = TEAM_ID_T_INVALID; // m_team (the Team* handle; 0 = no team)
    float m_visionRange = 0.0f;       // getVisionRange() (the template's vision; the corpus programs it)
    bool m_hasAi = false;             // getAIUpdateInterface() != null (the AI update's existence)
    bool m_aiIdle = false;            // AIUpdateInterface::isIdle()

    bool isKindOfImmobile() const { return (m_kindof & (1u << KINDOF_IMMOBILE_BIT)) != 0; }
    bool isKindOfNoCollide() const { return (m_kindof & (1u << KINDOF_NO_COLLIDE_BIT)) != 0; }
    bool isDestroyed() const { return (m_status & (1u << OBJECT_STATUS_DESTROYED_BIT)) != 0; }
    bool isNoCollisions() const { return (m_status & (1u << OBJECT_STATUS_NO_COLLISIONS_BIT)) != 0; }
    bool isEffectivelyDead() const { return (m_privateStatus & (1u << OBJECT_PRIVATE_EFFECTIVELY_DEAD_BIT)) != 0; }
    bool isOffMap() const { return (m_privateStatus & (1u << OBJECT_PRIVATE_OFF_MAP_BIT)) != 0; }
    // Object.h: getIsUndetectedDefector — BitIsSet(m_privateStatus, UNDETECTED_DEFECTOR).
    bool getIsUndetectedDefector() const { return (m_privateStatus & (1u << OBJECT_PRIVATE_UNDETECTED_DEFECTOR_BIT)) != 0; }

    // Thing::isKindOfMulti (Thing.cpp → the template's kindof bits; the
    // transcription keeps the bits on the object): every must-be-set bit
    // set and every must-be-clear bit clear.
    bool isKindOfMulti(unsigned mustBeSet, unsigned mustBeClear) const
    {
        return (m_kindof & mustBeSet) == mustBeSet && (m_kindof & mustBeClear) == 0;
    }

    // Thing::getUnitDirectionVector2D (Thing.cpp): the facing angle's
    // unit vector in the xy plane (the original caches it behind
    // VALID_DIRVECTOR; the on-demand form computes the identical value).
    Coord3D getUnitDirectionVector2D() const
    {
        Coord3D dir;
        dir.x = Cos(m_angle);
        dir.y = Sin(m_angle);
        dir.z = 0;
        return dir;
    }
};

// ============================================================================
// GameLogic.cpp — the kernel transcription.
// ============================================================================
struct GameLogicHooks; // the subsystem no-op surface (see below)

struct GameLogicT {
    // ---- the object registry
    std::vector<ObjectT *> m_objList;                 // prepend order (getFirstObject/getNextObject)
    std::vector<std::pair<int, ObjectT *>> m_objHash; // ID lookup (probed, never iterated)
    int m_nextObjID = INVALID_ID;

    // ---- the sleepy scheduler
    std::vector<UpdateModuleT *> m_sleepyUpdates; // the heap array (a min-heap on the packed priority)
    UpdateModuleT *m_curUpdateModule = nullptr;

    // ---- the destroy list
    std::vector<ObjectT *> m_objectsToDestroy;

    // ---- the frame loop state
    unsigned m_frame = 0;
    bool m_startNewGame = false;
    bool m_hasUpdated = false;
    bool m_isInUpdate = false;
    unsigned m_pauseFrame = 0;
    bool m_gamePaused = false;

    // --- allocateObjectID: the counter scheme (no reuse; the @todo in the
    // original is still there).
    int allocateObjectID()
    {
        int ret = m_nextObjID;
        m_nextObjID = (int)((unsigned)m_nextObjID + 1u);
        return ret;
    }
    void setObjectIDCounter(int nextObjID) { m_nextObjID = nextObjID; }

    ObjectT *findObjectByID(int id)
    {
        for (auto &kv : m_objHash)
            if (kv.first == id)
                return kv.second;
        return nullptr;
    }

    void addObjectToLookupTable(ObjectT *obj)
    {
        if (obj == nullptr)
            return;
        m_objHash.emplace_back(obj->m_id, obj);
    }
    void removeObjectFromLookupTable(ObjectT *obj)
    {
        if (obj == nullptr)
            return;
        for (size_t i = 0; i < m_objHash.size(); ++i)
            if (m_objHash[i].first == obj->m_id) {
                m_objHash.erase(m_objHash.begin() + (long)i);
                return;
            }
    }

    // --- registerObject: the list prepend, the lookup insert and the
    // initial sleepy wake.
    void registerObject(ObjectT *obj)
    {
        // add the object to the global list (prepend)
        m_objList.insert(m_objList.begin(), obj);

        // add object to lookup table
        addObjectToLookupTable(obj);

        unsigned now = getFrame();
        if (now == 0)
            now = 1;
        for (size_t b = 0; b < obj->m_updates.size(); ++b) {
            UpdateModuleT *u = &obj->m_updates[b];
            unsigned when = (unsigned)friend_getNextCallFrame(u);
            // note that 'when' can be zero here for any update module
            // that didn't bother to call setWakeFrame() in its ctor.
            // this is legal.
            if (when == 0)
                friend_setNextCallFrame(u, now);

            // (the DEBUG_ASSERTCRASH on the zero initial sleep is debug-only)
            pushSleepyUpdate(u);
        }
    }

    // --- destroyObject: the only interface to destroy objects.
    void destroyObject(ObjectT *obj, GameLogicHooks *hooks);

    // --- processDestroyList: the two-pass sleepy erase.
    void processDestroyList(PartitionManagerT *pm, GameLogicHooks *hooks);

    // --- the sleepy heap (GameLogic.cpp verbatim; the pristine
    // non-unrolled rebalanceChild).
    static inline bool isLowerPriority(const UpdateModuleT *a, const UpdateModuleT *b)
    {
        // return true iff a is lower pri than b.
        // remember: lower ordinal value means higher priority.
        // therefore, higher ordinal value means lower priority.
        // (UnsignedInt in the original — the comparison is unsigned)
        unsigned f1 = friend_getPriority(a);
        unsigned f2 = friend_getPriority(b);
        return f1 > f2;
    }

    int rebalanceParentSleepyUpdate(int i)
    {
        int parent = ((i + 1) >> 1) - 1;
        while (parent >= 0 && isLowerPriority(m_sleepyUpdates[(size_t)parent], m_sleepyUpdates[(size_t)i])) {
            UpdateModuleT *a = m_sleepyUpdates[(size_t)parent];
            UpdateModuleT *b = m_sleepyUpdates[(size_t)i];

            m_sleepyUpdates[(size_t)i] = a;
            m_sleepyUpdates[(size_t)parent] = b;

            a->m_indexInLogic = i;
            b->m_indexInLogic = parent;

            i = parent;
            parent = ((parent + 1) >> 1) - 1;
        }
        return i;
    }

    int rebalanceChildSleepyUpdate(int i)
    {
        // our children are i*2 and i*2+1 (the non-unrolled variant the
        // original carries next to its hand-unrolled twin; identical
        // behaviour, stated in the header)
        int sz = (int)m_sleepyUpdates.size();
        int child = ((i) << 1) + 1;
        while (child < sz) {
            // choose the higher-priority of the two children; we must be higher-pri than that.
            if (child < sz - 1 && isLowerPriority(m_sleepyUpdates[(size_t)child], m_sleepyUpdates[(size_t)child + 1]))
                ++child;

            // if we're higher-pri than our children, we're done.
            if (!isLowerPriority(m_sleepyUpdates[(size_t)i], m_sleepyUpdates[(size_t)child])) {
                break;
            }

            // doh. swap with the highest-pri child we have.
            UpdateModuleT *a = m_sleepyUpdates[(size_t)child];
            UpdateModuleT *b = m_sleepyUpdates[(size_t)i];

            m_sleepyUpdates[(size_t)i] = a;
            m_sleepyUpdates[(size_t)child] = b;

            a->m_indexInLogic = i;
            b->m_indexInLogic = child;
            i = child;
            child = ((i) << 1) + 1;
        }
        return i;
    }

    void rebalanceSleepyUpdate(int i)
    {
        rebalanceParentSleepyUpdate(i);
        rebalanceChildSleepyUpdate(i);
    }

    void remakeSleepyUpdate()
    {
        int parent = (int)(m_sleepyUpdates.size() / 2);
        while (true) {
            rebalanceChildSleepyUpdate(parent);
            if (parent == 0)
                break;
            --parent;
        }
    }

    void pushSleepyUpdate(UpdateModuleT *u)
    {
        m_sleepyUpdates.push_back(u);
        u->m_indexInLogic = (int)(m_sleepyUpdates.size() - 1);

        rebalanceParentSleepyUpdate((int)(m_sleepyUpdates.size() - 1));
    }

    UpdateModuleT *peekSleepyUpdate() const
    {
        UpdateModuleT *u = m_sleepyUpdates.front();
        return u;
    }

    void popSleepyUpdate()
    {
        int sz = (int)m_sleepyUpdates.size();
        if (sz == 0) {
            return; // DEBUG_CRASH in the original; the corpora never reach it
        }

        m_sleepyUpdates[0]->m_indexInLogic = -1;
        if (sz > 1) {
            m_sleepyUpdates[0] = m_sleepyUpdates[(size_t)sz - 1];
            m_sleepyUpdates[0]->m_indexInLogic = 0;
            m_sleepyUpdates.pop_back();
            rebalanceChildSleepyUpdate(0);
        } else {
            m_sleepyUpdates.pop_back();
        }
    }

    void eraseSleepyUpdate(int i)
    {
        // swap with the final item, toss the final item, then rebalance
        m_sleepyUpdates[(size_t)i]->m_indexInLogic = -1;

        int last = (int)m_sleepyUpdates.size() - 1;
        if (i < last) {
            m_sleepyUpdates[(size_t)i] = m_sleepyUpdates[(size_t)last];
            m_sleepyUpdates[(size_t)i]->m_indexInLogic = i;
            m_sleepyUpdates.pop_back();
            rebalanceSleepyUpdate(i);
        } else {
            m_sleepyUpdates.pop_back();
        }
    }

    // --- friend_awakenUpdateModule: setWakeFrame's path.
    void friend_awakenUpdateModule(ObjectT *obj, UpdateModuleT *u, unsigned whenToWakeUp);

    // --- UpdateModule::frameToSleepTime (the min-of-four).
    int frameToSleepTime(unsigned frame1, unsigned frame2 = (unsigned)FOREVER,
                         unsigned frame3 = (unsigned)FOREVER, unsigned frame4 = (unsigned)FOREVER)
    {
        if (frame1 > frame2)
            frame1 = frame2;
        if (frame1 > frame3)
            frame1 = frame3;
        if (frame1 > frame4)
            frame1 = frame4;
        unsigned now = m_frame;
        if (frame1 > now) {
            return (int)(frame1 - now); // UPDATE_SLEEP(numFrames)
        } else if (frame1 == now) {
            return UPDATE_SLEEP_NONE;
        } else {
            // frameToSleepTime: frame is in the past. oops. (DEBUG_CRASH in
            // the original; release returns NONE) — the corpora stay in-domain
            return UPDATE_SLEEP_NONE;
        }
    }

    unsigned getFrame() const { return m_frame; }

    // --- the transcription's handle plumbing (stable module identity).
    int moduleOwner(const UpdateModuleT *u) const;
    int moduleIndexOf(const UpdateModuleT *u) const;
};

// ============================================================================
// The subsystem hooks: the ordered no-op surface of GameLogic::update.
// The kernel orders these calls; the bodies are later slices.
// ============================================================================
struct GameLogicHooks {
    // The corpus-controlled state:
    bool m_engineTimeFrozen = false; // TheGameEngine->isTimeFrozen() (scripts/menu)
    // Counters per hook (the order is pinned by the event stream):
    int m_hookClientFrame = 0, m_hookScripts = 0, m_hookTerrain = 0, m_hookRecorder = 0;
    int m_hookCommands = 0, m_hookAI = 0, m_hookBuildAssistant = 0;
    int m_hookWeaponStore = 0, m_hookLocomotorStore = 0, m_hookVictory = 0;
    int m_hookStartNewGame = 0, m_hookObjectCreatedMsgs = 0, m_hookObjectDestroyedMsgs = 0;

    // the event stream the corpora read (each hook appends an entry)
    std::vector<std::array<int, 3>> m_events;
    enum Ev {
        EV_CLIENT_FRAME, EV_SCRIPTS, EV_FROZEN, EV_TERRAIN, EV_RECORDER, EV_COMMANDS,
        EV_UPDATE, EV_AI, EV_BUILD, EV_PARTITION, EV_DESTROY, EV_COMMANDS_RESET,
        EV_WEAPONSTORE, EV_LOCOMOTORSTORE, EV_VICTORY, EV_SWEEP, EV_FRAME_INC,
        EV_STARTNEWGAME, EV_OBJ_CREATED, EV_OBJ_DESTROYED, EV_CELL_CHANGE,
    };
    void ev(int e, int a = -1, int b = -1) { m_events.push_back({e, a, b}); }

    // The collide-module dispatch stream: one entry per onCollide module
    // call, with the loc and normal the collide test produced (the float
    // observables of the family). A separate stream: nothing else emits
    // while processContactList runs (the dispatches are the only
    // host-visible activity inside the partition update), so the order
    // against the main stream is fixed.
    struct CollideEvent {
        int m_obj, m_other, m_module;
        Coord3D m_loc, m_normal;
    };
    std::vector<CollideEvent> m_collideEvents;
    void collideEv(int obj, int other, int module, const Coord3D &loc, const Coord3D &normal)
    {
        m_collideEvents.push_back({obj, other, module, loc, normal});
    }

    // The players/teams event stream: one 4-int entry per hook the block's
    // choreography fires (the bodies themselves are later slices — the
// positions and arguments of the CALLS are the pinned observables).
    std::vector<std::array<int, 4>> m_teamEvents;
    enum TeamEv {
        TEV_PLAYER_UPDATE,  // Player::update's body position (a = player index)
        TEV_LOCAL,          // becomingLocalPlayer (a = player, b = 0/1)
        TEV_TEAM_MEMBER,    // becomingTeamMember (a = object, b = player, c = 0/1)
        TEV_CAPTURE,        // Object::onCapture (a = object, b = old player, c = new player)
        TEV_TEAM_SCRIPT,    // a script run (a = team id, b = the ScriptKind)
        TEV_TEAM_DESTROYED, // notifyOfTeamDestruction (a = team id)
    };
    enum ScriptKind {
        SCRIPT_ON_CREATE = 0,
        SCRIPT_ON_DESTROYED = 1,
        SCRIPT_ON_IDLE = 2,
        SCRIPT_ON_ENEMY_SIGHTED = 3,
        SCRIPT_ON_ALL_CLEAR = 4,
        SCRIPT_ON_UNIT_DESTROYED = 5,
        SCRIPT_PRODUCTION_CONDITION = 6, // createInactiveTeam's executeActions
    };
    void tev(int e, int a = -1, int b = -1, int c = -1) { m_teamEvents.push_back({e, a, b, c}); }
};

// ============================================================================
// The collide-test family (PartitionManager.cpp, PartitionManager.h's
// CollideLocAndNormal, Geometry.cpp's height helpers) — verbatim.
// ============================================================================

// PartitionManager.h: the collision location and the collision surface
// normal at 'a'. (The original leaves both undefined on a false result and
// never reads them then; the closeEnough preflight of
// z_collideTest_Nonsphere_Nonsphere returns true WITHOUT writing loc.x/y or
// the normal — the original hands its uninitialized stack local onward.
// The transcription zero-initializes every CollideLocAndNormal so the
// garbage becomes the pinned zero; stated deviation, both sides.)
struct CollideLocAndNormal {
    Coord3D loc;
    Coord3D normal;
};

// PartitionManager.cpp: the geometry pair under test.
struct CollideInfoT {
    Coord3D position;
    GeometryInfoT geom;
    float angle;

    CollideInfoT(const Coord3D &p, const GeometryInfoT &g, float a) : position(p), geom(g), angle(a) {}
};

// sqr (Lib/Precompiled/BaseAlgorithm.h): x*x.
static inline float csqr(float x) { return x * x; }

// --- the 2d/3d vector helpers (PartitionManager.cpp's inline functions). ---
static inline void vecDiff_2D(const Coord3D *posA, const Coord3D *posB, Coord3D *resultVec)
{
    resultVec->x = posA->x - posB->x;
    resultVec->y = posA->y - posB->y;
    resultVec->z = 0.0f;
}

static inline void vecDiff_3D(const Coord3D *posA, const Coord3D *posB, Coord3D *resultVec)
{
    resultVec->x = posA->x - posB->x;
    resultVec->y = posA->y - posB->y;
    resultVec->z = posA->z - posB->z;
}

static inline float calcSqrDist_2D(const Coord3D *dist) { return csqr(dist->x) + csqr(dist->y); }

static inline float calcSqrDist_3D(const Coord3D *dist)
{
    return csqr(dist->x) + csqr(dist->y) + csqr(dist->z);
}

static inline void projectCoord3D(Coord3D *coord, const Coord3D *unitDir, float dist)
{
    coord->x += unitDir->x * dist;
    coord->y += unitDir->y * dist;
    coord->z += unitDir->z * dist;
}

static inline void flipCoord3D(Coord3D *coord)
{
    coord->x = -coord->x;
    coord->y = -coord->y;
    coord->z = -coord->z;
}

// --- rectToFourPoints / testRotatedPointsAgainstRect. ---------------------
static void rectToFourPoints(
    const CollideInfoT *a, // z is ignored
    Coord2DT pts[])
{
    float c = Cos(a->angle);
    float s = Sin(a->angle);

    float exc = a->geom.getMajorRadius() * c;
    float eyc = a->geom.getMinorRadius() * c;
    float exs = a->geom.getMajorRadius() * s;
    float eys = a->geom.getMinorRadius() * s;

    // tl
    pts[0].x = a->position.x - exc - eys;
    pts[0].y = a->position.y + eyc - exs;

    // tr
    pts[1].x = a->position.x + exc - eys;
    pts[1].y = a->position.y + eyc + exs;

    // bl
    pts[2].x = a->position.x - exc + eys;
    pts[2].y = a->position.y - eyc - exs;

    // br
    pts[3].x = a->position.x + exc + eys;
    pts[3].y = a->position.y - eyc + exs;
}

static void testRotatedPointsAgainstRect(
    const Coord2DT *pts, // an array of 4
    const CollideInfoT *a,
    Coord2DT *avg,
    int *avgTot)
{
    //DEBUG_ASSERTCRASH(a->geom.getGeomType() == GEOMETRY_BOX, ("only boxes are ok here"));
    float major = a->geom.getMajorRadius();
    float minor = (a->geom.getGeomType() == GEOMETRY_SPHERE) ? a->geom.getMajorRadius() : a->geom.getMinorRadius();

    float c = Cos(-a->angle);
    float s = Sin(-a->angle);

    for (int i = 0; i < 4; ++i, ++pts) {
        // convert to a delta relative to rect ctr
        float ptx = pts->x - a->position.x;
        float pty = pts->y - a->position.y;

        // inverse-rotate it to the right coord system
        float ptx_new = Fabs(ptx * c - pty * s);
        float pty_new = Fabs(ptx * s + pty * c);

        if (ptx_new <= major && pty_new <= minor) {
            avg->x += pts->x;
            avg->y += pts->y;
            *avgTot += 1;
        }
    }
}

// The CollideTestProc: collideLoc/collideNormal are filled only if the
// result is true; either may be null if the caller doesn't need it.
typedef bool (*CollideTestProc)(const CollideInfoT *a, const CollideInfoT *b, CollideLocAndNormal *cinfo);

// --- the xy quartet. -------------------------------------------------------
// (The original forward-declares a five-argument xy_collideTest_Circle_Circle
// (a_pos, b_pos, a_radius, b_radius, cinfo) that it never defines and never
// calls — dead declaration, an overload sibling of the CollideInfo form the
// z_ helpers pass around. Not transcribed.)

static bool xy_collideTest_Rect_Rect(const CollideInfoT *a, const CollideInfoT *b, CollideLocAndNormal *cinfo)
{
    Coord2DT pts[4];
    Coord2DT avg;
    avg.x = avg.y = 0.0f;
    int avgTot = 0;

    rectToFourPoints(a, pts);
    testRotatedPointsAgainstRect(pts, b, &avg, &avgTot);

    rectToFourPoints(b, pts);
    testRotatedPointsAgainstRect(pts, a, &avg, &avgTot);

    if (avgTot > 0) {
        if (cinfo) {
            // this is a little hokey, but generally adequate...
            cinfo->loc.x = avg.x / avgTot;
            cinfo->loc.y = avg.y / avgTot;
            cinfo->loc.z = (a->position.z + b->position.z) * 0.5f;

            // yow, what exactly does it mean to find the normal to two *intersecting*
            // rects? ... we provide a (poor) guess by just doing the center-to-center
            // normal. (srj)
            vecDiff_2D(&b->position, &a->position, &cinfo->normal);
            cinfo->normal.normalize();
        }
        return true;
    }
    return false;
}

// 2d utility routine for rect/cyl & rect/sphere collisions testing.
static bool xy_collideTest_Rect_Circle(const CollideInfoT *a, const CollideInfoT *b, CollideLocAndNormal *cinfo)
{
    // srj -- this is better than the other one, since it actually handles rotated rects,
    // but still not as accurate as is could be.
    CollideInfoT btmp = *b;
    btmp.geom.setMinorRadius(btmp.geom.getMajorRadius());
    return xy_collideTest_Rect_Rect(a, &btmp, cinfo);
}

// 2d utility routine for rect/cyl & rect/sphere collisions testing.
static bool xy_collideTest_Circle_Rect(const CollideInfoT *a, const CollideInfoT *b, CollideLocAndNormal *cinfo)
{
    bool result = xy_collideTest_Rect_Circle(b, a, cinfo);
    if (cinfo)
        flipCoord3D(&cinfo->normal);
    return result;
}

// 2d utility routine for cyl & sphere collisions testing.
static bool xy_collideTest_Circle_Circle(const CollideInfoT *a, const CollideInfoT *b, CollideLocAndNormal *cinfo)
{
    Coord3D diff;
    vecDiff_2D(&b->position, &a->position, &diff);
    float distSqr = calcSqrDist_2D(&diff);
    float touchingDistSqr = csqr(a->geom.getMajorRadius() + b->geom.getMajorRadius());
    if (distSqr <= touchingDistSqr) {
        // calc the approximate location and normal.
        if (cinfo) {
            cinfo->normal = diff;
            cinfo->normal.normalize();
            cinfo->loc = a->position;
            projectCoord3D(&cinfo->loc, &cinfo->normal, a->geom.getMajorRadius());
        }

        return true;
    }
    return false;
}

// --- the z composites. -----------------------------------------------------
inline bool z_collideTest_Sphere_Nonsphere(CollideTestProc xyproc, const CollideInfoT *a, const CollideInfoT *b,
    CollideLocAndNormal *cinfo)
{
    // case 1: center of sphere is within the nonsphere z-space.
    if (a->position.z >= b->position.z && a->position.z <= b->position.z + b->geom.getMaxHeightAbovePosition()) {
        if (xyproc(a, b, cinfo)) {
            // a side-to-side collision; just need to adjust the z-coord of collideLoc
            if (cinfo) {
                cinfo->loc.z = a->position.z;
            }
            return true;
        }
        return false;
    }

    // case 2: center of sphere is above or below nonsphere, but within
    // major-radius of top or bottom.

    // 2a: sphere is below the nonsphere
    float b_bot = b->position.z;
    if (a->position.z < b_bot && a->position.z + a->geom.getMajorRadius() >= b_bot) {
        // find the radius of the slice of the sphere that is at b_bot
        CollideInfoT amod = *a;
        amod.position.z = b_bot;
        amod.geom.setMajorRadius(Sqrtf(csqr(a->geom.getMajorRadius()) - csqr(b_bot - a->position.z)));
        if (xyproc(&amod, b, cinfo)) {
            // (the 'end' collision normal would point straight up; never
            // possible, never computed)
            return true;
        }
        return false;
    }

    // 2b: sphere is above the nonsphere
    float b_top = b->position.z + b->geom.getMaxHeightAbovePosition();
    if (a->position.z > b_top && a->position.z - a->geom.getMajorRadius() <= b_top) {
        CollideInfoT amod = *a;
        amod.position.z = b_top;
        amod.geom.setMajorRadius(Sqrtf(csqr(a->geom.getMajorRadius()) - csqr(a->position.z - b_top)));
        if (xyproc(&amod, b, cinfo)) {
            // (ditto, straight down)
            return true;
        }
        return false;
    }

    return false;
}

inline bool z_collideTest_Nonsphere_Nonsphere(CollideTestProc xyproc, const CollideInfoT *a, const CollideInfoT *b,
    CollideLocAndNormal *cinfo)
{
    // note: we already know that there is a z-intersection (our caller filters that out)
    // so we need not recheck that here.

    // Preflight for close objects.
    float dSqr = csqr(a->position.x - b->position.x) + csqr(a->position.y - b->position.y);
    float minRadius = a->geom.getMajorRadius();
    float r = a->geom.getMinorRadius();
    if (minRadius > r)
        minRadius = r;
    r = b->geom.getMajorRadius();
    if (minRadius > r)
        minRadius = r;
    r = b->geom.getMinorRadius();
    if (minRadius > r)
        minRadius = r;

    bool closeEnough = csqr(minRadius) > dSqr;

    if (closeEnough || xyproc(a, b, cinfo)) {
        // since we allow objects to overlap at times, how do we distinguish
        // between a "top-to-bottom" vs "side-to-side" collision?
        // currently, we don't: cylindrical shapes currently never collide
        // top-to-bottom, so we just declare everything to be side-to-side.
        // if this changes, need to smarten this.

        // case 1: side-to-side collision
        // just need to adjust the z-coord of collideLoc
        if (cinfo) {
            if (b->position.z > a->position.z)
                cinfo->loc.z = (b->position.z + a->position.z + a->geom.getMaxHeightAbovePosition()) * 0.5f;
            else
                cinfo->loc.z = (a->position.z + b->position.z + b->geom.getMaxHeightAbovePosition()) * 0.5f;
        }
        return true;
    }

    return false;
}

// --- the nine proc entries. ------------------------------------------------
static bool collideTest_Sphere_Sphere(const CollideInfoT *a, const CollideInfoT *b, CollideLocAndNormal *cinfo)
{
    Coord3D diff;
    vecDiff_3D(&b->position, &a->position, &diff);
    float distSqr = calcSqrDist_3D(&diff);
    float touchingDistSqr = csqr(a->geom.getMajorRadius() + b->geom.getMajorRadius());
    if (distSqr <= touchingDistSqr) {
        if (cinfo) {
            cinfo->normal = diff;
            cinfo->normal.normalize();
            cinfo->loc = a->position;
            projectCoord3D(&cinfo->loc, &cinfo->normal, a->geom.getMajorRadius());
        }

        return true;
    }
    return false;
}

static bool collideTest_Sphere_Cylinder(const CollideInfoT *a, const CollideInfoT *b, CollideLocAndNormal *cinfo)
{
    return z_collideTest_Sphere_Nonsphere(xy_collideTest_Circle_Circle, a, b, cinfo);
}

static bool collideTest_Sphere_Box(const CollideInfoT *a, const CollideInfoT *b, CollideLocAndNormal *cinfo)
{
    return z_collideTest_Sphere_Nonsphere(xy_collideTest_Circle_Rect, a, b, cinfo);
}

static bool collideTest_Cylinder_Sphere(const CollideInfoT *a, const CollideInfoT *b, CollideLocAndNormal *cinfo)
{
    bool result = z_collideTest_Sphere_Nonsphere(xy_collideTest_Circle_Circle, b, a, cinfo);
    if (cinfo)
        flipCoord3D(&cinfo->normal);
    return result;
}

static bool collideTest_Cylinder_Cylinder(const CollideInfoT *a, const CollideInfoT *b, CollideLocAndNormal *cinfo)
{
    return z_collideTest_Nonsphere_Nonsphere(xy_collideTest_Circle_Circle, a, b, cinfo);
}

static bool collideTest_Cylinder_Box(const CollideInfoT *a, const CollideInfoT *b, CollideLocAndNormal *cinfo)
{
    return z_collideTest_Nonsphere_Nonsphere(xy_collideTest_Circle_Rect, a, b, cinfo);
}

static bool collideTest_Box_Sphere(const CollideInfoT *a, const CollideInfoT *b, CollideLocAndNormal *cinfo)
{
    bool result = z_collideTest_Sphere_Nonsphere(xy_collideTest_Circle_Rect, b, a, cinfo);
    if (cinfo)
        flipCoord3D(&cinfo->normal);
    return result;
}

static bool collideTest_Box_Cylinder(const CollideInfoT *a, const CollideInfoT *b, CollideLocAndNormal *cinfo)
{
    return z_collideTest_Nonsphere_Nonsphere(xy_collideTest_Rect_Circle, a, b, cinfo);
}

static bool collideTest_Box_Box(const CollideInfoT *a, const CollideInfoT *b, CollideLocAndNormal *cinfo)
{
    return z_collideTest_Nonsphere_Nonsphere(xy_collideTest_Rect_Rect, a, b, cinfo);
}

// NOTE: This *DEPENDS* on the order of the geometry enum defines
static CollideTestProc theCollideTestProcs[] = {
    collideTest_Sphere_Sphere,
    collideTest_Sphere_Cylinder,
    collideTest_Sphere_Box,
    collideTest_Cylinder_Sphere,
    collideTest_Cylinder_Cylinder,
    collideTest_Cylinder_Box,
    collideTest_Box_Sphere,
    collideTest_Box_Cylinder,
    collideTest_Box_Box
};
static_assert(sizeof(theCollideTestProcs) / sizeof(theCollideTestProcs[0]) == GEOMETRY_NUM_TYPES * GEOMETRY_NUM_TYPES,
    "Incorrect array size");

// PartitionManager::geomCollidesWithGeom — see if thisObj collides with
// geom at pos & angle. (A const member on the manager in the original,
// but pure geometry: no manager state is read, so it is a free function
// here.) Note the z-gate: unlike PartitionData::collidesWith this one
// ignores the getMaxHeightBelowPosition extents entirely — thisTop is
// compared against that's raw z and this's raw z against thatTop — a
// NARROWER gate than collidesWith's, pinned by the corpora.
static bool geomCollidesWithGeom(const Coord3D *pos1,
    const GeometryInfoT &geom1,
    float angle1,
    const Coord3D *pos2,
    const GeometryInfoT &geom2,
    float angle2)
{
    CollideInfoT thisInfo(*pos1, geom1, angle1);
    CollideInfoT thatInfo(*pos2, geom2, angle2);

    // invariant for all geometries: first do z collision check.
    if (thisInfo.position.z + thisInfo.geom.getMaxHeightAbovePosition() >= thatInfo.position.z &&
        thisInfo.position.z <= thatInfo.position.z + thatInfo.geom.getMaxHeightAbovePosition()) {
        int thisGeom = geom1.getGeomType();
        int thatGeom = geom2.getGeomType();

        //
        // NOTE: This assumes geometry enumerations that start at GEOMETRY_FIRST AND depends on the
        // order in which they appear in the enum list
        //
        CollideTestProc collideProc =
            theCollideTestProcs[(size_t)(thisGeom - GEOMETRY_FIRST) * GEOMETRY_NUM_TYPES + (thatGeom - GEOMETRY_FIRST)];
        CollideLocAndNormal cloc = {}; // (the original's uninitialized local — the pinned zero)
        return (*collideProc)(&thisInfo, &thatInfo, &cloc);
    } else {
        // no z-intersection -> no collision.
        return false;
    }
}

// ============================================================================
// The PartitionManager transcription.
// ============================================================================
struct CellAndObjectIntersection { // not MPO: we allocate these in arrays
    int m_cell = -1;               // cell index (or -1: not in use)
    int m_module = INVALID_ID;      // the PartitionData owner's object id (or INVALID_ID)
    CellAndObjectIntersection *m_prevCoi = nullptr, *m_nextCoi = nullptr; // if in use, next/prev in this cell. if not in use, next/prev free in this module.

    void addCoverage(PartitionManagerT &pm, int cellIndex, int moduleId);
    void removeAllCoverage(PartitionManagerT &pm);
    void friend_addToCellList(PartitionCell *cell);
    void friend_removeFromCellList(PartitionCell *cell);
};

struct ShroudLevel {
    int m_currentShroud = 1; // Default is "passive shroud".  1,0.
    int m_activeShroudLevel = 0;
};

struct PartitionCell { // not MPO: allocated in an array
    CellAndObjectIntersection *m_firstCoiInCell = nullptr; ///< list of COIs in this cell (may be null).
    ShroudLevel m_shroudLevel[MAX_PLAYER_COUNT];
    int m_threatValue[MAX_PLAYER_COUNT] = {0};
    int m_cashValue[MAX_PLAYER_COUNT] = {0};
    short m_coiCount = 0; ///< number of COIs in this cell.
    short m_cellX = 0;    ///< x-coord of this cell within the Partition Mgr coords (NOT in world coords)
    short m_cellY = 0;    ///< y-coord of this cell within the Partition Mgr coords (NOT in world coords)

    void init(int x, int y)
    {
        m_cellX = (short)x;
        m_cellY = (short)y;
    }
};

enum DirtyStatus {
    NOT_DIRTY,
    NEED_COLLISION_CHECK,
    NEED_CELL_UPDATE_AND_COLLISION_CHECK
};

struct PartitionData {
    int m_object = INVALID_ID;     ///< Object this module is for (object id handle)
    int m_ghostObject = INVALID_ID; ///< Temporary object used when real object is gone (never in this slice)
    PartitionData *m_next = nullptr;   ///< next module in master list
    PartitionData *m_prev = nullptr;   ///< prev module in master list
    PartitionData *m_nextDirty = nullptr;
    PartitionData *m_prevDirty = nullptr;

    int m_coiArrayCount = 0; ///< number of COIs allocated (may be more than are in use)
    int m_coiInUseCount = 0; ///< number of COIs that are actually in use
    std::vector<CellAndObjectIntersection> m_coiArray; ///< The array of COIs
    int m_doneFlag = 0;
    DirtyStatus m_dirtyStatus = NOT_DIRTY;
    int m_shroudedness[MAX_PLAYER_COUNT];
    int m_shroudednessPrevious[MAX_PLAYER_COUNT]; ///< previous frames value of m_shroudedness
    bool m_everSeenByPlayer[MAX_PLAYER_COUNT];    ///< whether this object has ever been seen by a given player.
    int m_lastCell = -1; ///< The last cell I thought my center was in (cell index; -1 = null)

    PartitionData()
    {
        for (int i = 0; i < MAX_PLAYER_COUNT; ++i) {
            m_everSeenByPlayer[i] = false;
            m_shroudedness[i] = OBJECTSHROUD_INVALID;
            m_shroudednessPrevious[i] = OBJECTSHROUD_INVALID;
        }
    }

    bool isInListDirtyModules(PartitionData *const *pListHead) const
    {
        bool result = (*pListHead == this || m_prevDirty || m_nextDirty);
        return result;
    }
    void prependToDirtyModules(PartitionData **pListHead)
    {
        m_nextDirty = *pListHead;
        if (*pListHead)
            (*pListHead)->m_prevDirty = this;
        *pListHead = this;
    }
    void removeFromDirtyModules(PartitionData **pListHead)
    {
        m_dirtyStatus = NOT_DIRTY;
        if (m_nextDirty)
            m_nextDirty->m_prevDirty = m_prevDirty;
        if (m_prevDirty)
            m_prevDirty->m_nextDirty = m_nextDirty;
        else
            *pListHead = m_nextDirty;
        m_prevDirty = nullptr;
        m_nextDirty = nullptr;
    }
};

// The contact-list pair collection (PartitionContactList::addToContactList
// verbatim; the hash table is exact — dedup rescans the bucket).
inline unsigned hash2ints(int a, int b)
{
    // do it this way so that [a,b] always hashes to the same value as [b,a].
    // this is unsophisticated but reasonable, since all ObjectIDs will
    // quite likely be well below 65536...
    if (a < b) {
        return ((unsigned)a << 16) + (unsigned)b;
    } else {
        return ((unsigned)b << 16) + (unsigned)a;
    }
}

struct PartitionContactListNode {
    PartitionContactListNode *m_nextHash = nullptr; ///< next node with same hash value
    PartitionContactListNode *m_next = nullptr;     ///< next node
    int m_obj = INVALID_ID;   ///< one object that is possibly colliding (object id)
    int m_other = INVALID_ID; ///< the other object (or null for collisions with the terrain)
    int m_hashValue = 0;      ///< index into hash table
};

struct PartitionContactList {
    enum { PartitionContactList_SOCKET_COUNT = 5381 };
    std::vector<PartitionContactListNode *> m_contactHash;
    PartitionContactListNode *m_contactList = nullptr;
    std::vector<std::unique_ptr<PartitionContactListNode>> m_nodeStore; // transcription storage

    PartitionContactList() : m_contactHash(PartitionContactList_SOCKET_COUNT, nullptr) {}

    void addToContactList(int objId, int otherId)
    {
        if (objId == otherId)
            return;

        // compute hash index based on object's ids.
        unsigned hashValue = hash2ints(objId, otherId);
        hashValue %= PartitionContactList_SOCKET_COUNT;

        // make sure given hit has not already been recorded
        for (PartitionContactListNode *cd = m_contactHash[hashValue]; cd; cd = cd->m_nextHash) {
            if ((cd->m_obj == objId && cd->m_other == otherId) ||
                (cd->m_obj == otherId && cd->m_other == objId)) {
                // already noted
                return;
            }
        }

        // new hit
        PartitionContactListNode *ncd = new PartitionContactListNode();
        m_nodeStore.emplace_back(ncd);
        ncd->m_obj = objId;
        ncd->m_other = otherId;
        ncd->m_hashValue = (int)hashValue;

        // add to hash table
        ncd->m_nextHash = m_contactHash[hashValue];
        m_contactHash[hashValue] = ncd;

        // add to list of contacts for this frame
        ncd->m_next = m_contactList;
        m_contactList = ncd;
    }

    // removeSpecificPartitionData: null every contact that refers to the
    // given object (the detachFromObject mid-processing path). The
    // original's verbatim shape — the nodes stay in the list and the hash
    // with nulled ids, and the processing walk skips them. (This replaces
    // the transcription-time unlink-and-erase variant, which had never
    // been reachable: same observables — the skipped nodes are skipped
    // either way, and the pair dump is captured before processing.)
    void removeSpecificPartitionData(int objId)
    {
        for (PartitionContactListNode *cd = m_contactList; cd; cd = cd->m_next)
        {
            if (cd->m_obj == objId || cd->m_other == objId)
            {
                cd->m_obj = INVALID_ID;
                cd->m_other = INVALID_ID;
            }
        }
    }

    // process all pairs in the contact list: first, determine if they
    // truly collide, based on their geometry. if so, call the collide
    // actions for each object in the pair. (Defined after the World —
    // the dispatch needs the driver's singletons.)
    void processContactList(World &w);
};

struct Region2DT {
    float loX = 0, loY = 0, hiX = 0, hiY = 0;
    float width() const { return hiX - loX; }
    float height() const { return hiY - loY; }
};

// ICoord2D (Lib/ICoord2D.h).
struct ICoord2DT {
    int x = 0, y = 0;
};

// OffsetVec / RadiusVec (PartitionManager.h): the FASTER_GCO rings.
typedef std::vector<ICoord2DT> OffsetVecT;
typedef std::vector<OffsetVecT> RadiusVecT;

// (the query family's collaborator types, defined with the family after
// the World — the manager's declarations only carry pointers/references)
struct PartitionFilterT;
struct SimpleObjectIteratorT;

struct PartitionManagerT {
    float m_cellSize = 0.0f;
    float m_cellSizeInv = 0.0f;
    int m_cellCountX = 0, m_cellCountY = 0, m_totalCellCount = 0;
    std::vector<PartitionCell> m_cells;
    Region2DT m_worldExtents;
    PartitionData *m_moduleList = nullptr; // the master list (prepend order)
    PartitionData *m_dirtyModules = nullptr;
    bool m_updatedSinceLastReset = false;
    PartitionContactList *m_contactList = nullptr; // TheContactList while update() runs

    // FASTER_GCO (defined in the retail PartitionManager.h): the
    // precomputed per-radius cell-offset rings getClosestObjects walks,
    // and the done-flag generation counter (the original's function-static
    // theIterFlag, hoisted to manager state).
    int m_maxGcoRadius = 0;
    RadiusVecT m_radiusVec;
    int m_iterFlag = 1; // nonzero, thanks

    // transcription storage for the PartitionData modules (ids are object
    // ids; the store never frees mid-corpus)
    std::vector<std::unique_ptr<PartitionData>> m_moduleStore;

    // the pairs the last update() collected, in contact-list order (the
    // dump observable; processContactList is the collision slice)
    std::vector<std::pair<int, int>> m_lastContacts;

    World *m_world = nullptr; // the driver's object table

    PartitionData *findModule(int objectId)
    {
        for (auto &m : m_moduleStore)
            if (m->m_object == objectId)
                return m.get();
        return nullptr;
    }
    ObjectT *resolveObject(int id);

    // --- init (the grid build).
    void init(float partitionCellSize, const Region2DT &worldExtents)
    {
        m_cellSize = partitionCellSize;
        if (m_cellSize < 1.0f)
            m_cellSize = 1.0f;

        m_cellSizeInv = (float)(1.0 / m_cellSize);

        m_worldExtents = worldExtents;
        // you'd think it wouldn't be legal for terrainlogic to have zero area, but apparently
        // that's what it resets itself to. let's just make the world a simple place and pretend
        // it's nonzero in area, so that we can proceed without crashing.
        if (m_worldExtents.width() < 1.0f)
            m_worldExtents.hiX = m_worldExtents.loX + 1.0f;
        if (m_worldExtents.height() < 1.0f)
            m_worldExtents.hiY = m_worldExtents.loY + 1.0f;
        m_cellCountX = (int)REAL_TO_INT_CEIL(m_worldExtents.width() * m_cellSizeInv);
        m_cellCountY = (int)REAL_TO_INT_CEIL(m_worldExtents.height() * m_cellSizeInv);
        m_totalCellCount = m_cellCountX * m_cellCountY;
        m_cells.assign((size_t)(m_totalCellCount > 0 ? m_totalCellCount : 0), PartitionCell());
        for (int x = 0; x < m_cellCountX; x++) {
            for (int y = 0; y < m_cellCountY; y++) {
                getCellAt(x, y)->init(x, y);
            }
        }

        // (the transcription's init always has extents — the store takes
        // them as the parameter; the original's null-TerrainLogic branch
        // zeroing the grid is the caller's domain, stated)
        calcRadiusVec();
    }

    void shutdown()
    {
        m_updatedSinceLastReset = false;
        removeAllDirtyModules();

        m_radiusVec.clear();
        m_radiusVec.shrink_to_fit();

        m_cells.clear();
        m_cells.shrink_to_fit();

        m_cellSize = m_cellSizeInv = 0.0f;
        m_cellCountX = 0;
        m_cellCountY = 0;
        m_totalCellCount = 0;
        m_worldExtents = Region2DT();
    }

    // --- the coordinate mapping.
    void worldToCell(float wx, float wy, int *cx, int *cy) const
    {
        *cx = (int)REAL_TO_INT_FLOOR((wx - m_worldExtents.loX) * m_cellSizeInv);
        *cy = (int)REAL_TO_INT_FLOOR((wy - m_worldExtents.loY) * m_cellSizeInv);
    }
    int worldToCellDist(float w) const { return (int)REAL_TO_INT_CEIL(w * m_cellSizeInv); }
    PartitionCell *getCellAt(int x, int y)
    {
        return (x < 0 || y < 0 || x >= m_cellCountX || y >= m_cellCountY)
            ? nullptr
            : &m_cells[(size_t)y * (size_t)m_cellCountX + (size_t)x];
    }

    // --- the FASTER_GCO rings (calcMinRadius / calcRadiusVec).
    int calcMinRadius(const ICoord2DT &cur);
    void calcRadiusVec();

    // --- the query family (the FASTER_GCO getClosestObjects and its
    // wrappers over the distCalcProcs, the filters and
    // SimpleObjectIteratorT — defined with the family below).
    ObjectT *getClosestObjects(ObjectT *obj, const Coord3D *pos, float maxDist, DistanceCalculationType dc,
                                PartitionFilterT **filters, SimpleObjectIteratorT *iterArg, float *closestDistArg,
                                Coord3D *closestVecArg);
    ObjectT *getClosestObject(ObjectT *obj, float maxDist, DistanceCalculationType dc, PartitionFilterT **filters,
                               float *closestDist, Coord3D *closestDistVec);
    ObjectT *getClosestObject(const Coord3D *pos, float maxDist, DistanceCalculationType dc,
                               PartitionFilterT **filters, float *closestDist, Coord3D *closestDistVec);
    void getVectorTo(const ObjectT *obj, const ObjectT *otherObj, DistanceCalculationType dc, Coord3D &vec);
    void getVectorTo(const ObjectT *obj, const Coord3D *pos, DistanceCalculationType dc, Coord3D &vec);
    float getDistanceSquared(const ObjectT *obj, const ObjectT *otherObj, DistanceCalculationType dc, Coord3D *vec);
    float getDistanceSquared(const ObjectT *obj, const Coord3D *pos, DistanceCalculationType dc, Coord3D *vec);
    float getGoalDistanceSquared(const ObjectT *obj, const Coord3D *goalPos, const ObjectT *otherObj,
                                  DistanceCalculationType dc, Coord3D *vec);
    float getGoalDistanceSquared(const ObjectT *obj, const Coord3D *goalPos, const Coord3D *otherPos,
                                  DistanceCalculationType dc, Coord3D *vec);
    float getRelativeAngle2D(const ObjectT *obj, const ObjectT *otherObj);
    float getRelativeAngle2D(const ObjectT *obj, const Coord3D *pos);
    SimpleObjectIteratorT iterateObjectsInRange(ObjectT *obj, float maxDist, DistanceCalculationType dc,
                                                 PartitionFilterT **filters, IterOrderType order);
    SimpleObjectIteratorT iterateObjectsInRange(const Coord3D *pos, float maxDist, DistanceCalculationType dc,
                                                 PartitionFilterT **filters, IterOrderType order);
    SimpleObjectIteratorT iteratePotentialCollisions(const Coord3D *pos, const GeometryInfoT &geom, float angle,
                                                      bool use2D);
    SimpleObjectIteratorT iterateAllObjects(PartitionFilterT **filters);

    // --- the dirty list.
    void removeAllDirtyModules()
    {
        while (m_dirtyModules)
            removeFromDirtyModules(m_dirtyModules);
    }
    void removeFromDirtyModules(PartitionData *mod)
    {
        mod->removeFromDirtyModules(&m_dirtyModules);
    }

    // --- registerObject / unRegisterObject.
    void registerObject(ObjectT *object)
    {
        // sanity
        if (object == nullptr)
            return;

        // if object is already part of this system get out of here
        if (object->m_partitionData != INVALID_ID)
            return;

        // allocate a new module of partition data
        PartitionData *mod = new PartitionData();
        m_moduleStore.emplace_back(mod);

        // link the module to the list in the partition manager
        mod->m_prev = nullptr;
        mod->m_next = m_moduleList;
        if (m_moduleList)
            m_moduleList->m_prev = mod;
        m_moduleList = mod;

        // add module to object
        attachToObject(mod, object);
    }

    void unRegisterObject(ObjectT *object)
    {
        // sanity
        if (object == nullptr)
            return;

        // get the partition module
        PartitionData *mod = object->m_partitionData != INVALID_ID ? findModule(object->m_partitionData) : nullptr;
        if (mod == nullptr)
            return;

        // need to figure out if any players have a fogged memory of this object.
        // if so, we can't remove it from the shroud system just yet.
        // (the ghost branch: never taken in this slice — no ghost objects
        // exist; the wasSeenByAnyPlayers ghost check is unreachable)

        // detach the module from the object
        detachFromObject(mod, object);

        // remove module from master list
        PartitionData *next = mod->m_next;
        PartitionData *prev = mod->m_prev;
        if (next)
            next->m_prev = prev;
        if (prev)
            prev->m_next = next;
        else
            m_moduleList = next;

        // deleteInstance(mod) → ~PartitionData's dirty-list cleanup (the
        // freeCoiArray inside detachFromObject re-dirtied the module)
        if (mod->isInListDirtyModules(&m_dirtyModules))
            removeFromDirtyModules(mod);
    }

    // --- attach / detach.
    void attachToObject(PartitionData *mod, ObjectT *object)
    {
        // remember who contains us
        mod->m_object = object->m_id;

        // (the ghost-object creation for IMMOBILE drawables is the
        // fog-memory slice; not taken here)

        // (re)calc maxCoi and (re)alloc cois
        freeCoiArray(mod);
        allocCoiArray(mod, object); // may throw!

        object->m_partitionData = mod->m_object;
        makeDirty(mod, true);
    }

    void detachFromObject(PartitionData *mod, ObjectT *object)
    {
        // this is a little hokey... if we are in the midst of processing the contact
        // list, we have to ensure that any potential collisions get removed from that
        // list, since we are about to go bye-bye. ugly, but effective. (srj)
        if (m_contactList)
            m_contactList->removeSpecificPartitionData(mod->m_object);

        object->m_partitionData = INVALID_ID;
        removeAllTouchedCells(mod);

        freeCoiArray(mod);

        // no longer attached to object
        mod->m_object = INVALID_ID;
        mod->m_ghostObject = INVALID_ID;
    }

    // --- the COI array.
    int calcMaxCoiForShape(int geom, float majorRadius, float minorRadius, bool isSmall)
    {
        (void)isSmall; // the small shortcut is commented out in the original (M Lorenzen 8/26/03)
        int result;

        switch (geom) {
        case GEOMETRY_SPHERE:
        case GEOMETRY_CYLINDER: {
            // note that majorRadius is a radius, not a diameter.
            // this actually allocates a few too many, but that's ok.
            int cells = worldToCellDist(majorRadius * 2) + 1;
            result = cells * cells;
            break; // (the RETAIL_COMPATIBLE_CRC fallthrough into BOX is dropped — plan §3.3 rule 1)
        }
        case GEOMETRY_BOX: {
            float diagonal = (float)(Sqrtf(majorRadius * majorRadius + minorRadius * minorRadius));
            int cells = worldToCellDist(diagonal * 2) + 1;
            result = cells * cells;
            break;
        }
        default:
            return 4;
        }
        if (result < 4)
            result = 4;
        return result;
    }

    void allocCoiArray(PartitionData *mod, ObjectT *obj)
    {
        mod->m_coiArrayCount = calcMaxCoiForShape(obj->m_geometryInfo.geomType, obj->m_geometryInfo.majorRadius,
                                                  obj->m_geometryInfo.minorRadius, obj->m_geometryInfo.isSmall);
        mod->m_coiArray.assign((size_t)mod->m_coiArrayCount, CellAndObjectIntersection());
        mod->m_coiInUseCount = 0;
        makeDirty(mod, true);
    }

    void freeCoiArray(PartitionData *mod)
    {
        mod->m_coiArray.clear();
        mod->m_coiArray.shrink_to_fit();
        mod->m_coiArrayCount = 0;
        mod->m_coiInUseCount = 0;
        makeDirty(mod, true);
    }

    // --- the coverage maintenance.
    void removeAllTouchedCells(PartitionData *mod)
    {
        for (int i = 0; i < mod->m_coiArrayCount; ++i) {
            CellAndObjectIntersection &coi = mod->m_coiArray[(size_t)i];
            if (coi.m_module != INVALID_ID) {
                coi.removeAllCoverage(*this);
                --mod->m_coiInUseCount;
            }
        }
    }

    void addSubPixToCoverage(PartitionData *mod, PartitionCell *cell)
    {
        int cellIndex = cellIndexOf(cell);
        if (cell) {
            // see if we already have a coi for this cell.
            CellAndObjectIntersection *coiToUse = nullptr;
            int scan = mod->m_coiInUseCount < mod->m_coiArrayCount ? mod->m_coiInUseCount : mod->m_coiArrayCount;
            for (int i = 0; i < scan; ++i) {
                CellAndObjectIntersection &c = mod->m_coiArray[(size_t)i];
                if (c.m_cell == cellIndex) {
                    coiToUse = &c;
                    break;
                }
            }
            if (coiToUse == nullptr && mod->m_coiInUseCount < mod->m_coiArrayCount) {
                // nope, no coi for this cell, allocate a new one
                coiToUse = &mod->m_coiArray[(size_t)mod->m_coiInUseCount++];
            }
            if (coiToUse) {
                coiToUse->addCoverage(*this, cellIndex, mod->m_object);
            }
        }
    }

    // --- the fills.
    void doRectFill(PartitionData *mod, float centerX, float centerY, float halfsizeX, float halfsizeY, float angle)
    {
        float c = (float)Cos(angle);
        float s = (float)Sin(angle);

        float actualCellSize = m_cellSize;
        float stepSize = actualCellSize * 0.5f; // in theory, should be getCellSize() exactly, but needs to be smaller to avoid aliasing problems
        float ydx = s * stepSize;
        float ydy = -c * stepSize;
        float xdx = c * stepSize;
        float xdy = s * stepSize;

        // GS 12-26-02  Oh my.  The step size has been fudged down to ensure that a cell is not jumped over,
        // but then the original step size was used to determine the number of steps.  This error gets
        // more pronounced on bigger buildings.  ie a 100 X 160 registers cells for a 80 x 100.  If you
        // fudge one, you have to fudge the other.
        float stepSizeInvTimes2 = 2.0f * (1.0f / stepSize);

        // srj fixes subtle bug: need +1 to get all of misaligned geometries accted for.
        // (the +1 fixup is commented out in the original — the GS fix)
        int numStepsX = (int)REAL_TO_INT_CEIL(halfsizeX * stepSizeInvTimes2);
        int numStepsY = (int)REAL_TO_INT_CEIL(halfsizeY * stepSizeInvTimes2);

        float tl_x = centerX - halfsizeX * c - halfsizeY * s;
        float tl_y = centerY + halfsizeY * c - halfsizeX * s;

        for (int iy = 0; iy < numStepsY; ++iy, tl_x += ydx, tl_y += ydy) {
            float x = tl_x;
            float y = tl_y;
            for (int ix = 0; ix < numStepsX; ++ix, x += xdx, y += xdy) {
                int cellx, celly;
                worldToCell(x, y, &cellx, &celly);
                PartitionCell *cell = getCellAt(cellx, celly); // might be null if off the edge
                if (cell) {
                    addSubPixToCoverage(mod, cell);
                }
            }
        }
    }

    static bool doesCircleOverlapCell(float centerX, float centerY, float radius, float cellX, float cellY, float cellSize)
    {
        float closestX = std::max(cellX, std::min(centerX, cellX + cellSize));
        float closestY = std::max(cellY, std::min(centerY, cellY + cellSize));

        float distX = centerX - closestX;
        float distY = centerY - closestY;

        return (distX * distX + distY * distY) < (radius * radius);
    }

    void doCircleFillPrecise(PartitionData *mod, float centerX, float centerY, float radius)
    {
        int minCellX, minCellY, maxCellX, maxCellY;
        worldToCell(centerX - radius, centerY - radius, &minCellX, &minCellY);
        worldToCell(centerX + radius, centerY + radius, &maxCellX, &maxCellY);

        float cellSize = m_cellSize;

        for (int x = minCellX; x <= maxCellX; ++x) {
            for (int y = minCellY; y <= maxCellY; ++y) {
                float cellWorldX = x * cellSize;
                float cellWorldY = y * cellSize;

                if (doesCircleOverlapCell(centerX, centerY, radius, cellWorldX, cellWorldY, cellSize)) {
                    PartitionCell *cell = getCellAt(x, y);
                    if (cell) {
                        addSubPixToCoverage(mod, cell);
                    }
                }
            }
        }
    }

    void doSmallFill(PartitionData *mod, float centerX, float centerY, float radius)
    {
        float halfCellSize = m_cellSize * 0.5f;
        if (radius > halfCellSize) {
            // object is too large to use a 'small' geometry, truncating size to cellsize
            radius = halfCellSize;
        }

        int cx1, cy1, cx2, cy2;
        worldToCell(centerX - radius, centerY - radius, &cx1, &cy1);
        worldToCell(centerX + radius, centerY + radius, &cx2, &cy2);

        for (int x = cx1; x <= cx2; x++) {
            for (int y = cy1; y <= cy2; y++) {
                PartitionCell *cell = getCellAt(x, y);
                if (cell) {
                    mod->m_coiArray[(size_t)mod->m_coiInUseCount++].addCoverage(*this, cellIndexOf(cell), mod->m_object);
                }
            }
        }
    }

    // --- addPossibleCollisions (the pair collection the store walk yields).
    void addPossibleCollisions(PartitionData *mod, PartitionContactList *ctList)
    {
        for (int i = 0; i < mod->m_coiInUseCount; ++i) {
            PartitionCell *cell = cellByIndex(mod->m_coiArray[(size_t)i].m_cell);
            if (cell->m_coiCount < 2)
                continue;

            for (CellAndObjectIntersection *coi = cell->m_firstCoiInCell; coi; coi = coi->m_nextCoi) {
                int that = coi->m_module;
                if (mod->m_object != that) {
                    ctList->addToContactList(mod->m_object, that);
                }
            }
        }
    }

    // --- the dirty marking.
    void makeDirty(PartitionData *mod, bool needToUpdateCells)
    {
        if (!mod->isInListDirtyModules(&m_dirtyModules)) {
            if (needToUpdateCells)
                mod->m_dirtyStatus = NEED_CELL_UPDATE_AND_COLLISION_CHECK;
            else if (mod->m_dirtyStatus == NOT_DIRTY)
                mod->m_dirtyStatus = NEED_COLLISION_CHECK;
            mod->prependToDirtyModules(&m_dirtyModules);
        } else if (needToUpdateCells) {
            // If I am in the list already, I may be in there for a half update, so if my
            // needs have increased I'll upgrade what I want.
            mod->m_dirtyStatus = NEED_CELL_UPDATE_AND_COLLISION_CHECK;
        }
    }

    // --- updateCellsTouched (the refresh body).
    void updateCellsTouched(PartitionData *mod, GameLogicHooks *hooks)
    {
        ObjectT *object = resolveObject(mod->m_object);

        GeometryInfoT geom = object->m_geometryInfo;
        bool isSmall = geom.isSmall;
        Coord3D pos = object->m_pos;
        float angle = object->m_angle;

        removeAllTouchedCells(mod);
        if (isSmall) {
            doSmallFill(mod, pos.x, pos.y, geom.majorRadius);
        } else {
            switch (geom.geomType) {
            case GEOMETRY_SPHERE:
            case GEOMETRY_CYLINDER: {
                // TheSuperHackers @bugfix Stubbjax 29/01/2026 Use precise circle fill to improve
                // collision accuracy, most notably for objects with geometry radii >= 20 and < 40.
                doCircleFillPrecise(mod, pos.x, pos.y, geom.majorRadius);
                break;
            }

            case GEOMETRY_BOX: {
                doRectFill(mod, pos.x, pos.y, geom.majorRadius, geom.minorRadius, angle);
                break;
            }
            default:
                break;
            };
        }

        int currentCellIndexX, currentCellIndexY;
        worldToCell(pos.x, pos.y, &currentCellIndexX, &currentCellIndexY);
        int currentCell = cellIndexOf(getCellAt(currentCellIndexX, currentCellIndexY)); // -1 if off the grid
        if (currentCell != mod->m_lastCell) {
            // To not expose PartitionCells, he will think in terms of points.  He will
            // unlook at a point and look at the new point.  We do the rounding and the
            // changing into PartitionCells
            object->m_cellChangeCount++;
            if (hooks)
                hooks->ev(GameLogicHooks::EV_CELL_CHANGE, mod->m_object, currentCell);
            mod->m_lastCell = currentCell;
        }

        // if we have moved, our shroudedness status might be different for all players,
        // so it must all be invalidated.
        invalidateShroudedStatusForAllPlayers(mod);
    }

    void invalidateShroudedStatusForPlayer(PartitionData *mod, int playerIndex)
    {
        // (DISABLE_INVALID_PREVENTION is defined in the build — the guard applies)
        if (mod->m_shroudedness[playerIndex] != OBJECTSHROUD_INVALID &&
            mod->m_shroudedness[playerIndex] != OBJECTSHROUD_INVALID_BUT_PREVIOUS_VALID)
            mod->m_shroudedness[playerIndex] = OBJECTSHROUD_INVALID;
    }

    void invalidateShroudedStatusForAllPlayers(PartitionData *mod)
    {
        for (int i = 0; i < MAX_PLAYER_COUNT; i++) {
            invalidateShroudedStatusForPlayer(mod, i);
        }
    }

    // --- update (the refresh loop).
    void update(GameLogicHooks *hooks)
    {
        {
            if (!m_updatedSinceLastReset) {
                m_updatedSinceLastReset = true;
            }

            PartitionContactList ctList;
            m_contactList = &ctList;
            while (m_dirtyModules) {
                // save it.
                PartitionData *dirty = m_dirtyModules;

                // get this BEFORE removing from dirty list, since that clears the
                // flag in question.
                bool updateEm = dirty->m_dirtyStatus == NEED_CELL_UPDATE_AND_COLLISION_CHECK;
                bool collideEm = dirty->m_dirtyStatus != NOT_DIRTY && dirty->m_object != INVALID_ID; // isInNeedOfCollisionCheck() && getObject()

                // detach it from the dirty list.
                removeFromDirtyModules(dirty);

                if (updateEm) {
                    updateCellsTouched(dirty, hooks);
                }

                ObjectT *obj = resolveObject(dirty->m_object);
                if (collideEm && obj && !obj->isKindOfImmobile()) {
                    addPossibleCollisions(dirty, &ctList);
                }
            }

            // the pair collection, in contact-list order (captured before
            // the processing nulls the nodes — the dump observable)
            m_lastContacts.clear();
            for (PartitionContactListNode *n = ctList.m_contactList; n; n = n->m_next)
                m_lastContacts.emplace_back(n->m_obj, n->m_other);

            // the contact list's processing: the collide-test family and
            // the CollideModule dispatch (the collision slice)
            ctList.processContactList(*m_world);

            m_contactList = nullptr;
            // (processPendingUndoShroudRevealQueue is the shroud slice)
        }
    }

    // helpers for the transcription's index-based cells
    int cellIndexOf(const PartitionCell *cell) const
    {
        if (!cell || m_cells.empty())
            return -1;
        return (int)((size_t)cell - (size_t)m_cells.data()) / (int)sizeof(PartitionCell);
    }
    PartitionCell *cellByIndex(int index)
    {
        if (index < 0 || index >= (int)m_cells.size())
            return nullptr;
        return &m_cells[(size_t)index];
    }
};

// --- CellAndObjectIntersection coverage (the cell-list prepend/unlink). ---
void CellAndObjectIntersection::friend_addToCellList(PartitionCell *cell)
{
    this->m_nextCoi = cell->m_firstCoiInCell;
    if (cell->m_firstCoiInCell)
        cell->m_firstCoiInCell->m_prevCoi = this;
    cell->m_firstCoiInCell = this;
}

void CellAndObjectIntersection::friend_removeFromCellList(PartitionCell *cell)
{
    if (this->m_prevCoi) {
        this->m_prevCoi->m_nextCoi = this->m_nextCoi;
    } else {
        cell->m_firstCoiInCell = this->m_nextCoi;
    }

    if (this->m_nextCoi) {
        this->m_nextCoi->m_prevCoi = this->m_prevCoi;
    }

    this->m_prevCoi = nullptr;
    this->m_nextCoi = nullptr;
}

void CellAndObjectIntersection::addCoverage(PartitionManagerT &pm, int cellIndex, int moduleId)
{
    if (m_module != INVALID_ID && m_module != moduleId) {
        // COI already in use by another module! (the refusal the original
        // DEBUG_CRASHes on — release returns)
        return;
    }

    PartitionCell *cell = pm.cellByIndex(cellIndex);
    if (m_cell == -1) {
        friend_addToCellList(cell);
        cell->m_coiCount++;
    }
    // (re-covering the same cell: the COI is already in the cell's list)

    m_cell = cellIndex;
    m_module = moduleId;
}

void CellAndObjectIntersection::removeAllCoverage(PartitionManagerT &pm)
{
    if (m_module == INVALID_ID) {
        // COI not in use (the original DEBUG_CRASHes; release returns)
        return;
    }

    PartitionCell *cell = pm.cellByIndex(m_cell);
    friend_removeFromCellList(cell);
    cell->m_coiCount--;
    m_cell = -1;
    m_module = INVALID_ID;
}

// ============================================================================
// GameLogic's destroy paths (need the PartitionManager for the ~Object
// unregister and the World for the module-owner resolution).
// ============================================================================
void GameLogicT::destroyObject(ObjectT *obj, GameLogicHooks *hooks)
{
    // if already flagged for destruction, ignore
    if (!obj || obj->isDestroyed())
        return;

    // run the object onDestroy event if provided
    // (the DestroyModule bodies are later slices; the scripted effects
    // cover the destroy-during-destroy reentrancy)

    // mark object as destroyed
    obj->m_status |= (1u << OBJECT_STATUS_DESTROYED_BIT);

    // add to end of destruction list, in case something is being destroyed and trying to destroy subobjects
    m_objectsToDestroy.push_back(obj);

    // run any on destroy logic internal to the object
    // (obj->onDestroy() — later slices)

    hooks->ev(GameLogicHooks::EV_OBJ_DESTROYED, obj->m_id, -1);
    hooks->m_hookObjectDestroyedMsgs++;
}

void GameLogicT::processDestroyList(PartitionManagerT *pm, GameLogicHooks *hooks)
{
    (void)hooks;
    // The destroy list exists to ensure that all objects have a chance to
    // see each other at each simulation frame - the object list is the
    // same at the start of the update as it is at the end of the update.

    // the events: the destroy-list pass itself (after the partition step,
    // before the command-list reset), carrying the processed count
    if (!m_objectsToDestroy.empty())
        hooks->ev(GameLogicHooks::EV_DESTROY, (int)m_objectsToDestroy.size(), -1);

    for (size_t it = 0; it < m_objectsToDestroy.size(); ++it) { // index walk: appended entries process in the same pass
        ObjectT *currentObject = m_objectsToDestroy[it];

        /*
                this looks odd, but is necessary; since erasing a single entry can shuffle others in the list
                (in order to maintain its heap-ness), we must do two passes: one to find the updates for this
                object, another to actually erase 'em.

                (in case you are wondering: yes, this is still more efficient than just deleting them
                and rebalancing the entire heap afterwards, at least for real-world maps, since an individual
                rebalance is O(log N) and a full rebalance is O(N)... so unless you are deleting the majority
                of the objects in the world every frame, we come out well ahead this way.)
        */

        const int MAX_SUO = 256;
        UpdateModuleT *sleepyUpdatesForThisObject[MAX_SUO];
        int numSUO = 0;

        for (size_t i2 = 0; i2 < m_sleepyUpdates.size(); ++i2) {
            UpdateModuleT *u = m_sleepyUpdates[i2];
            if (moduleOwner(u) == currentObject->m_id && numSUO < MAX_SUO) {
                sleepyUpdatesForThisObject[numSUO++] = u;
            }
        }

        for (--numSUO; numSUO >= 0; --numSUO) {
            // have to re-get idx each time since each call to erase might change others.
            int idx = sleepyUpdatesForThisObject[numSUO]->m_indexInLogic;
            eraseSleepyUpdate(idx);
        }

        // remove from the object list
        for (size_t i = 0; i < m_objList.size(); ++i)
            if (m_objList[i] == currentObject) {
                m_objList.erase(m_objList.begin() + (long)i);
                break;
            }

        // remove object from lookup table
        removeObjectFromLookupTable(currentObject);

        // the delete instance (Object::~Object: the partition unregister)
        if (currentObject->m_partitionData != INVALID_ID) {
            pm->unRegisterObject(currentObject);
        }
        currentObject->m_id = INVALID_ID; // "reset id to zero so we never mistaken grab dead objects"
    }

    m_objectsToDestroy.clear(); // list full of bad pointers now, clear it.
}

void GameLogicT::friend_awakenUpdateModule(ObjectT *obj, UpdateModuleT *u, unsigned whenToWakeUp)
{
    unsigned now = getFrame();
    // (setWakeFrame frame is in the past... the assert is debug-only; the
    // corpora stay in-domain)

    if (u == m_curUpdateModule) {
        // You should not call setWakeFrame() from inside your update(), because
        // it will be ignored, in favor of the return code from update.
        return;
    }

    if (whenToWakeUp == (unsigned)friend_getNextCallFrame(u))
        return; // my, that was easy

    if ((now > 0) && ((unsigned)friend_getNextCallFrame(u) == now) && (whenToWakeUp == now + 1)) {
        // subtle but important case: if we already awake, and someone calls
        // setWakeFrame(self, UPDATE_SLEEP_NONE), we don't want to reset our wake frame,
        // since that would prevent us from getting called THIS frame. since UPDATE_SLEEP_NONE
        // really means "wake up as soon as possible", we don't want to change our status
        // if we are already awake. (srj)
        return;
    }

    int idx = u->m_indexInLogic;
    bool inList = false;
    for (ObjectT *o : m_objList)
        if (o == obj) {
            inList = true;
            break;
        }
    if (inList) {
        if (idx < 0 || idx >= (int)m_sleepyUpdates.size()) {
            // fatal error! sleepy update module illegal index. (RELEASE_CRASH —
            // the corpora stay in-domain)
            return;
        }

        if (m_sleepyUpdates[(size_t)idx] != u) {
            // fatal error! sleepy update module index mismatch. (RELEASE_CRASH)
            return;
        }

        // update the value.
        friend_setNextCallFrame(u, whenToWakeUp);

        // rebalance.
        rebalanceSleepyUpdate(idx);

        return;
    } else {
        if (idx != -1) {
            // fatal error! sleepy update module index mismatch. (RELEASE_CRASH)
            return;
        }

        // this can happen if stuff happens during object initialization. fortunately,
        // it's easy to deal with:
        friend_setNextCallFrame(u, whenToWakeUp);
        return;
    }
}

// ============================================================================
// The players/teams/relationships block (WP1's remainder, the fourth slice) —
// PlayerList.cpp, Player.cpp's kernel half, Team.cpp's TeamFactory/Team/
// TeamPrototype state machine, Object.cpp's setTeam family, and the
// PartitionManager.cpp filters over the state (SamePlayer / Relationship /
// AcceptOnTeam) — verbatim, distilled inputs.
//
// The input distillation (the map's SidesList dicts become plain specs; the
// real dict parse is rzh-formats' sides-list slice, already ported):
//   SideSpec  = playerName, playerIsHuman, multiplayerIsLocal, the
//               PlayerTemplate's observer bit, the enemies/allies token
//               lists.
//   TeamSpec  = teamName, teamOwner, teamIsSingleton, the TeamTemplateInfo
//               fields updateState consults (the six script names and the
//               destroyedThreshold real), and the production-condition pair
//               (the executeActions flag, the condition name and whether the
//               script engine's catalog holds it).
//   GameInfo  = the occupied slot indices (assignSlotIndices walks
//               "player%d" names against them).
// Everything else the dicts carry (factions, build lists, skirmish script
// duplication, waypoints, attitudes, max instances, ...) is the data/AI/
// script-engine slices' and is stated in the header, not modelled.
//
// The name-key interning (NameKeyGenerator.cpp) is transcribed because the
// prototype map's std::map<NameKeyType,...> ITERATION ORDER — the
// findTeamByID / teamAboutToBeDeleted / clear() walks — is keyed on the
// interning ordinals (first-interned order among the map's names; the real
// global generator's catalog is shared with data loading, but the relative
// order of the map's own names is what any closed corpus observes).
// ============================================================================

// GameCommon.h: ENEMIES, NEUTRAL, ALLIES (keep TheRelationShipNames in sync).
enum RelationshipT {
    R_ENEMIES = 0,
    R_NEUTRAL = 1,
    R_ALLIES = 2,
};
static const char *relationshipName(RelationshipT r)
{
    // TheRelationShipNames (GameCommon.cpp), in enum order.
    static const char *names[3] = { "Enemies", "Neutral", "Allies" };
    return names[r];
}

// GameCommon.h: the player-mask word (16 players fit an UnsignedShort).
typedef unsigned short PlayerMaskT;
static const PlayerMaskT PLAYERMASK_ALL_T = 0xffff;
static const PlayerMaskT PLAYERMASK_NONE_T = 0x0000;

// Player.h: PLAYER_INDEX_INVALID (-1; the relation maps' clear-all key).
static const int PLAYER_INDEX_INVALID = -1;

// PlayerList.h: AllowPlayerRelationship — the getPlayersWithRelationship
// selector bits (a DIFFERENT flag set from PartitionFilterRelationship's).
enum AllowPlayerRelationshipT {
    ALLOW_SAME_PLAYER_T = 0x01,
    ALLOW_ALLIES_T = 0x02,
    ALLOW_ENEMIES_T = 0x04,
    ALLOW_NEUTRAL_T = 0x08,
};

// Team.h: the prototype-id word and its invalid.
typedef unsigned TeamProtoIDT;
static const TeamProtoIDT TEAM_PROTO_ID_T_INVALID = 0;

// GameCommon.h: PLAYER_HUMAN, PLAYER_COMPUTER.
enum PlayerTypeT { PLAYER_HUMAN_T = 0, PLAYER_COMPUTER_T = 1 };

// NetworkDefs.h: MAX_PLAYER = 7, MAX_SLOTS = MAX_PLAYER + 1 = 8.
static const int MAX_SLOTS_T = 8;

// NameKeyGenerator.cpp: calcHashForString — hash * 33 + byte over the raw
// bytes (NUL-terminated in the original; the transcription takes a length).
static inline unsigned calcHashForStringT(const char *p, size_t len)
{
    unsigned result = 0;
    for (size_t i = 0; i < len; ++i)
        result = (result << 5) + result + (unsigned char)p[i];
    return result;
}

// NameKeyGenerator.cpp: SOCKET_COUNT = 45007 ("prime, and not close to a
// power of 2").
static const unsigned NAMEKEY_SOCKET_COUNT = 45007;

// NameKeyGenerator: nameToKey — the socket walk (strcmp equality) and
// createNameKey (the prepend into the socket chain, the sequential key from
// m_nextID — init() sets 1). Only nameToKey is transcribed: the players
// block never interns case-insensitively.
struct NameKeyGenT {
    unsigned m_nextID = 1; // init()'s reset (the ctor's NAMEKEY_INVALID state never serves)
    // socket -> the bucket indices in that socket's chain (the prepend
    // puts the newest first; the equality walk's result never depends on it).
    std::vector<std::vector<unsigned>> m_sockets;
    // The buckets: the name and its key (index + 1 = the key — the original
    // news Bucket and stores m_key; the arena index carries it here).
    std::vector<std::pair<std::string, unsigned>> m_buckets;

    NameKeyGenT() { m_sockets.resize(NAMEKEY_SOCKET_COUNT); }

    // NameKeyType (the 24-bit id; 0 = NAMEKEY_INVALID).
    unsigned nameToKey(const std::string &name)
    {
        const unsigned hash = calcHashForStringT(name.data(), name.size()) % NAMEKEY_SOCKET_COUNT;
        // do we have it already?
        for (unsigned b : m_sockets[hash]) {
            if (name.compare(m_buckets[b].first) == 0)
                return m_buckets[b].second;
        }
        // nope, guess not. let's allocate it.
        m_buckets.emplace_back(name, m_nextID++);
        m_sockets[hash].push_back((unsigned)(m_buckets.size() - 1));
        return m_buckets.back().second;
    }
};

// The distilled SidesList (see the block header).
struct SideSpecT {
    std::string playerName; // empty = the neutral side ("it's neutral, which we've already done")
    bool isHuman = false;   // TheKey_playerIsHuman
    bool multiplayerIsLocal = false; // TheKey_multiplayerIsLocal
    bool observer = false;  // the PlayerTemplate's isObserver() (distilled)
    std::vector<std::string> enemies; // TheKey_playerEnemies' token list
    std::vector<std::string> allies;  // TheKey_playerAllies' token list
};

// The distilled team dict (TeamTemplateInfo's updateState-relevant half).
struct TeamSpecT {
    std::string name;   // TheKey_teamName
    std::string owner;  // TheKey_teamOwner
    bool singleton = false; // TheKey_teamIsSingleton
    std::string scriptOnCreate;       // TheKey_teamOnCreateScript
    std::string scriptOnDestroyed;    // TheKey_teamOnDestroyedScript
    std::string scriptOnIdle;         // TheKey_teamOnIdleScript
    std::string scriptOnEnemySighted; // TheKey_teamEnemySightedScript
    std::string scriptOnAllClear;     // TheKey_teamAllClearScript
    std::string scriptOnUnitDestroyed; // TheKey_teamOnUnitDestroyedScript
    float destroyedThreshold = 0.0f;  // TheKey_teamDestroyedThreshold
    bool executeActions = false;      // TheKey_teamExecutesActionsOnCreate
    std::string productionCondition;  // TheKey_teamProductionCondition
    bool productionScriptExists = false; // findScriptByName's verdict (the catalog is the script slice's)
};

// The distilled GameInfo (assignSlotIndices' occupied-slot walk).
struct GameInfoSpecT {
    std::vector<int> occupiedSlots; // the occupied slot indices (isOccupied())
};

// The corpora's spec builders (full-field initializers; the aggregate
// brace-elision would trip -Werror=missing-field-initializers).
static TeamSpecT teamSpec(const std::string &name, const std::string &owner, bool singleton)
{
    TeamSpecT t;
    t.name = name;
    t.owner = owner;
    t.singleton = singleton;
    return t;
}
static SideSpecT sideSpec(const std::string &name, bool isHuman, bool mpLocal, bool observer,
                          const std::vector<std::string> &enemies = {},
                          const std::vector<std::string> &allies = {})
{
    SideSpecT s;
    s.playerName = name;
    s.isHuman = isHuman;
    s.multiplayerIsLocal = mpLocal;
    s.observer = observer;
    s.enemies = enemies;
    s.allies = allies;
    return s;
}

struct SidesSpecT {
    std::vector<SideSpecT> sides;
    std::vector<TeamSpecT> teams;
    bool hasGameInfo = false; // TheGameInfo != nullptr
    GameInfoSpecT gameInfo;
};

struct TeamProtoT;
struct TeamT;

// Player.cpp's kernel half (the relation maps, the identity, the team list).
// The maps are std::hash_map in the original but are only ever PROBED by key
// here (find / operator[] / erase) — never iterated — so the transcription
// uses sorted vectors and dumps them in key order (a stated, order-free
// model: no observable depends on the hash_map's bucket order).
struct PlayerT {
    int m_playerIndex = 0;
    std::string m_playerName;
    unsigned m_playerNameKey = 0; // NAMEKEY_INVALID
    PlayerTypeT m_playerType = PLAYER_COMPUTER_T;
    bool m_observer = false;
    bool m_isPlayerDead = false;

    // the relation maps (PlayerRelationMapType / TeamRelationMapType),
    // sorted by key
    std::vector<std::pair<int, RelationshipT>> m_playerRelations;
    std::vector<std::pair<TeamIDT, RelationshipT>> m_teamRelations;

    TeamIDT m_defaultTeam = TEAM_ID_T_INVALID; // m_defaultTeam (Team* -> id handle)
    std::vector<TeamProtoT *> m_playerTeamPrototypes; // push_back order (addTeamToList)

    // ---- the relation-map primitives (probed, sorted by key) ----
    const RelationshipT *findPlayerRelation(int playerIndex) const
    {
        for (const auto &kv : m_playerRelations)
            if (kv.first == playerIndex)
                return &kv.second;
        return nullptr;
    }
    const RelationshipT *findTeamRelation(TeamIDT teamID) const
    {
        for (const auto &kv : m_teamRelations)
            if (kv.first == teamID)
                return &kv.second;
        return nullptr;
    }
    void setPlayerRelationEntry(int playerIndex, RelationshipT r) // m_map[k] = r
    {
        for (auto &kv : m_playerRelations)
            if (kv.first == playerIndex) { kv.second = r; return; }
        m_playerRelations.push_back({playerIndex, r});
        std::sort(m_playerRelations.begin(), m_playerRelations.end(),
                  [](const std::pair<int, RelationshipT> &a, const std::pair<int, RelationshipT> &b) { return a.first < b.first; });
    }
    void setTeamRelationEntry(TeamIDT teamID, RelationshipT r)
    {
        for (auto &kv : m_teamRelations)
            if (kv.first == teamID) { kv.second = r; return; }
        m_teamRelations.push_back({teamID, r});
        std::sort(m_teamRelations.begin(), m_teamRelations.end(),
                  [](const std::pair<TeamIDT, RelationshipT> &a, const std::pair<TeamIDT, RelationshipT> &b) { return a.first < b.first; });
    }
    bool erasePlayerRelation(int playerIndex)
    {
        for (size_t i = 0; i < m_playerRelations.size(); ++i)
            if (m_playerRelations[i].first == playerIndex) { m_playerRelations.erase(m_playerRelations.begin() + (long)i); return true; }
        return false;
    }
    bool eraseTeamRelation(TeamIDT teamID)
    {
        for (size_t i = 0; i < m_teamRelations.size(); ++i)
            if (m_teamRelations[i].first == teamID) { m_teamRelations.erase(m_teamRelations.begin() + (long)i); return true; }
        return false;
    }

    // Player::isPlayerActive: alive and not observer.
    bool isPlayerActive() const { return !m_observer && !m_isPlayerDead; }

    // ---- the declared-later surface (defined after the World) ----
    void addTeamToList(TeamProtoT *team);
    void removeTeamFromList(TeamProtoT *team);
    void preTeamDestroy(const TeamT *team);
    void setDefaultTeam(World *w);
    void updateTeamStates(World *w);
    void update(World *w);
    void newMap(World *w);
    void setPlayerType(PlayerTypeT t, bool skirmish);
    void init(bool hasTemplate, World *w);
};

// Team.cpp's TeamPrototype: the owning player, the singleton flag, the
// template info and the team instances (the DLINK prepend list — pushed to
// the back and iterated reverse: newest first).
struct TeamProtoT {
    TeamProtoIDT m_id = TEAM_PROTO_ID_T_INVALID;
    struct TeamFactoryT *m_factory = nullptr;
    std::string m_name;
    unsigned m_nameKey = 0;
    PlayerT *m_owningPlayer = nullptr;
    enum { TEAM_SINGLETON = 1 };
    unsigned m_flags = 0;
    const TeamSpecT *m_template = nullptr; // the distilled TeamTemplateInfo

    std::vector<struct TeamT *> m_instances; // prepend order: back = head

    TeamProtoT(TeamFactoryT *tf, const std::string &name, PlayerT *ownerPlayer,
               bool isSingleton, const TeamSpecT *d, TeamProtoIDT id, unsigned nameKey);

    bool getIsSingleton() const { return (m_flags & TEAM_SINGLETON) != 0; }
    TeamT *getFirstItemIn_TeamInstanceList() const
    {
        return m_instances.empty() ? nullptr : m_instances.back();
    }
    bool isInList_TeamInstanceList(const TeamT *t) const
    {
        for (const TeamT *x : m_instances)
            if (x == t)
                return true;
        return false;
    }
    void prependTo_TeamInstanceList(TeamT *t) { m_instances.push_back(t); }
    void removeFrom_TeamInstanceList(TeamT *t)
    {
        for (size_t i = 0; i < m_instances.size(); ++i)
            if (m_instances[i] == t) { m_instances.erase(m_instances.begin() + (long)i); return; }
    }
    // the DLINK iterator's order: head first = reverse of the push_backs
    const std::vector<TeamT *> &instancesInListOrder() const
    {
        // (the dumpers and walks read this reversed; the vector itself keeps
        // push order so the back is the head)
        return m_instances;
    }
};

// Team.cpp's Team: the runtime state, the override relation maps and the
// member list (the DLINK prepend — back = head, iterated reverse).
struct TeamT {
    TeamIDT m_id = TEAM_ID_T_INVALID;
    TeamProtoT *m_proto = nullptr;
    bool m_enteredOrExited = false;
    bool m_active = false;
    bool m_seeEnemy = false;
    bool m_prevSeeEnemy = false;
    bool m_checkEnemySighted = false;
    bool m_isRecruitablitySet = false;
    bool m_isRecruitable = false;
    int m_destroyThreshold = 0;
    int m_curUnits = 0;
    bool m_wasIdle = false;
    bool m_created = false;

    // the override relation maps (TeamRelationMapType / PlayerRelationMapType),
    // sorted by key (probed only — see PlayerT's note)
    std::vector<std::pair<TeamIDT, RelationshipT>> m_teamRelations;
    std::vector<std::pair<int, RelationshipT>> m_playerRelations;

    std::vector<ObjectT *> m_members; // prepend order: back = head

    // ---- the same probed-map primitives ----
    const RelationshipT *findTeamRelation(TeamIDT teamID) const
    {
        for (const auto &kv : m_teamRelations)
            if (kv.first == teamID)
                return &kv.second;
        return nullptr;
    }
    const RelationshipT *findPlayerRelation(int playerIndex) const
    {
        for (const auto &kv : m_playerRelations)
            if (kv.first == playerIndex)
                return &kv.second;
        return nullptr;
    }
    void setTeamRelationEntry(TeamIDT teamID, RelationshipT r)
    {
        for (auto &kv : m_teamRelations)
            if (kv.first == teamID) { kv.second = r; return; }
        m_teamRelations.push_back({teamID, r});
        std::sort(m_teamRelations.begin(), m_teamRelations.end(),
                  [](const std::pair<TeamIDT, RelationshipT> &a, const std::pair<TeamIDT, RelationshipT> &b) { return a.first < b.first; });
    }
    void setPlayerRelationEntry(int playerIndex, RelationshipT r)
    {
        for (auto &kv : m_playerRelations)
            if (kv.first == playerIndex) { kv.second = r; return; }
        m_playerRelations.push_back({playerIndex, r});
        std::sort(m_playerRelations.begin(), m_playerRelations.end(),
                  [](const std::pair<int, RelationshipT> &a, const std::pair<int, RelationshipT> &b) { return a.first < b.first; });
    }
    bool eraseTeamRelation(TeamIDT teamID)
    {
        for (size_t i = 0; i < m_teamRelations.size(); ++i)
            if (m_teamRelations[i].first == teamID) { m_teamRelations.erase(m_teamRelations.begin() + (long)i); return true; }
        return false;
    }
    bool erasePlayerRelation(int playerIndex)
    {
        for (size_t i = 0; i < m_playerRelations.size(); ++i)
            if (m_playerRelations[i].first == playerIndex) { m_playerRelations.erase(m_playerRelations.begin() + (long)i); return true; }
        return false;
    }

    // ---- the member-list primitives (the DLINK prepend) ----
    ObjectT *getFirstItemIn_TeamMemberList() const { return m_members.empty() ? nullptr : m_members.back(); }
    bool isInList_TeamMemberList(const ObjectT *o) const
    {
        for (const ObjectT *x : m_members)
            if (x == o)
                return true;
        return false;
    }
    void prependTo_TeamMemberList(ObjectT *o) { m_members.push_back(o); }
    void removeFrom_TeamMemberList(ObjectT *o)
    {
        for (size_t i = 0; i < m_members.size(); ++i)
            if (m_members[i] == o) { m_members.erase(m_members.begin() + (long)i); return; }
    }

    TeamIDT getID() const { return m_id; }
    TeamProtoT *getPrototype() const { return m_proto; }

    // ---- the declared-later surface ----
    void setActive();
    void setOverrideTeamRelationship(TeamIDT teamID, RelationshipT r);
    bool removeOverrideTeamRelationship(TeamIDT teamID);
    void setOverridePlayerRelationship(int playerIndex, RelationshipT r);
    bool removeOverridePlayerRelationship(int playerIndex);
    void setControllingPlayer(PlayerT *newController, World *w);
    void notifyTeamOfObjectDeath(World *w);
    bool isActive() const;
};

// Team.cpp's TeamFactory: the std::map<NameKeyType, TeamPrototype*> (ordered
// by key — the interning ordinals), the unique-id counters.
struct TeamFactoryT {
    // std::map's iteration order is the KEY order (the interning ordinals):
    // a sorted vector of (key, proto) keeps the walk faithful.
    std::vector<std::pair<unsigned, TeamProtoT *>> m_prototypes;
    TeamProtoIDT m_uniqueTeamPrototypeID = TEAM_PROTO_ID_T_INVALID;
    TeamIDT m_uniqueTeamID = TEAM_ID_T_INVALID;
    std::vector<TeamT *> m_arena; // every TeamT ever created (the destructor's delete choreography owns the lifetime)

    TeamFactoryT() = default;
    // (no ~TeamFactory: the Worlds leak by design — see World's note; the
    // original's dtor is the clear() choreography the corpora drive directly)

    // ---- the declared-later surface ----
    void addTeamPrototypeToList(TeamProtoT *team);
    void removeTeamPrototypeFromList(TeamProtoT *team);
    TeamProtoT *findTeamPrototype(const std::string &name, World *w);
    TeamProtoT *findTeamPrototypeByID(TeamProtoIDT id);
    TeamT *findTeamByID(TeamIDT teamID);
    TeamT *createInactiveTeam(const std::string &name, World *w);
    TeamT *createTeam(const std::string &name, World *w);
    TeamT *createTeamOnPrototype(TeamProtoT *prototype, World *w);
    TeamT *findTeam(const std::string &name, World *w);
    void teamAboutToBeDeleted(TeamT *team, World *w);
    void clear(World *w);
    void init(World *w);
    void reset(World *w);
    void initFromSides(const SidesSpecT &sides, World *w);
    void initTeam(const std::string &name, const std::string &owner, bool isSingleton,
                  const TeamSpecT *d, World *w);

    TeamProtoT *findTeamProtoByKey(unsigned key) const
    {
        for (const auto &kv : m_prototypes)
            if (kv.first == key)
                return kv.second;
        return nullptr;
    }
    // the std::map iteration order (ascending key)
    const std::vector<std::pair<unsigned, TeamProtoT *>> &prototypesInMapOrder() const { return m_prototypes; }
};

// PlayerList.cpp's PlayerList: the 16 pre-created players, the count, the
// local player and the slot bookkeeping.
struct PlayerListT {
    PlayerT *m_local = nullptr;
    int m_playerCount = 0;
    PlayerT *m_players[MAX_PLAYER_COUNT];
    int m_slotIndices[MAX_PLAYER_COUNT];
    int m_slotToPlayerIndices[MAX_SLOTS_T];

    PlayerT *getNeutralPlayer() { return m_players[0]; }
    PlayerT *getNthPlayer(int i)
    {
        if (i < 0 || i >= MAX_PLAYER_COUNT)
            return nullptr;
        return m_players[i];
    }
    PlayerT *findPlayerWithNameKey(unsigned key, World *w) const;
};

// ============================================================================
// The World: the driver's singletons (TheGameLogic / ThePartitionManager /
// the object table), the scripted module bodies and the corpus commands.
// ============================================================================
struct World {
    GameLogicT logic;
    PartitionManagerT pm;
    PlayerListT playerList;
    TeamFactoryT teamFactory;
    NameKeyGenT nameKeys;
    bool m_networkPresent = false; // TheNetwork != null (the newGame local-selection gate)
    GameLogicHooks hooks;
    std::vector<std::unique_ptr<ObjectT>> objects; // the arena (ids never reused)

    World()
    {
        pm.m_world = this;
        // PlayerList's ctor: "we only allocate a few of these, so don't
        // bother pooling 'em" — the 16 players are pre-created, then init().
        for (int i = 0; i < MAX_PLAYER_COUNT; ++i) {
            playerList.m_players[i] = new PlayerT();
            playerList.m_players[i]->m_playerIndex = i;
        }
        playerListInit();
    }
    // (no ~World: the corpora leak each World by design — freshWorld news
    // one and never deletes, matching the generator's existing lifetime)

    // ---- the players/teams block's World surface (defined later) ----
    void playerListInit();
    void playerListReset();
    void playerListSetLocalPlayer(PlayerT *player);
    void playerListTeamAboutToBeDeleted(TeamT *team);
    void playerListUpdate();
    void playerListNewMap();
    void playerListUpdateTeamStates();
    TeamT *playerListValidateTeam(const std::string &owner);
    PlayerT *playerListGetPlayerFromMask(PlayerMaskT mask);
    PlayerT *playerListGetEachPlayerFromMask(PlayerMaskT &maskToAdjust);
    PlayerMaskT playerListGetPlayersWithRelationship(int srcPlayerIndex, unsigned allowedRelationships);
    void playerListNewGame(const SidesSpecT &sides);
    void playerListSetSlotIndex(int playerIndex, int slotIndex);
    int playerListGetSlotIndex(int playerIndex) const;
    int playerListGetPlayerIndexFromSlotIndex(int slotIndex) const;
    PlayerT *playerListGetPlayerFromSlotIndex(int slotIndex) const;
    void playerListAssignSlotIndices(const GameInfoSpecT &gameInfo);

    ObjectT *object(int id)
    {
        for (auto &o : objects)
            if (o->m_id == id)
                return o.get();
        return nullptr;
    }

    // PartitionData::collidesWith / friend_collidesWith (defined after the
    // World — the collide family's object entry).
    bool collidesWith(int thisId, int thatId, CollideLocAndNormal *cinfo);

    // The ThingFactory::newObject shape: ID allocation, module birth (with
    // the phases and the optional constructor wakes — the not-in-list
    // awaken path — and the collide modules the behavior list carries),
    // the GameLogic registration, then the partition registration.
    ObjectT *createObject(const GeometryInfoT &geom, bool immobile, int numUpdates,
                          const std::vector<int> &ctorWakes, const Coord3D &pos = {}, float angle = 0.0f,
                          const std::vector<int> &phases = {}, int numColliders = 0)
    {
        ObjectT *obj = new ObjectT();
        objects.emplace_back(obj);
        obj->m_geometryInfo = geom;
        obj->m_pos = pos;
        obj->m_angle = angle;
        obj->m_kindof = immobile ? (1u << KINDOF_IMMOBILE_BIT) : 0u;
        for (int i = 0; i < DISABLED_COUNT; ++i)
            obj->m_disabledTillFrame[i] = NEVER;
        obj->m_updates.resize((size_t)numUpdates);
        for (int i = 0; i < numUpdates && i < (int)phases.size(); ++i)
            obj->m_updates[(size_t)i].m_phase = phases[(size_t)i];
        obj->m_colliders.resize((size_t)numColliders);
        obj->m_id = logic.allocateObjectID();

        // module constructors may setWakeFrame (the not-in-list path)
        for (int i = 0; i < numUpdates; ++i) {
            int wake = (i < (int)ctorWakes.size()) ? ctorWakes[(size_t)i] : -1;
            if (wake >= 0) {
                unsigned now = logic.getFrame();
                logic.friend_awakenUpdateModule(obj, &obj->m_updates[(size_t)i],
                                                (unsigned)wake > now ? (unsigned)wake : now);
            }
        }

        logic.registerObject(obj);
        pm.registerObject(obj);
        hooks.ev(GameLogicHooks::EV_OBJ_CREATED, obj->m_id, -1);
        hooks.m_hookObjectCreatedMsgs++;
        return obj;
    }

    // UpdateModule::setWakeFrame(obj, wakeDelay) — the delay form.
    void setWakeFrame(ObjectT *obj, int modIndex, int wakeDelay)
    {
        unsigned now = logic.getFrame();
        logic.friend_awakenUpdateModule(obj, &obj->m_updates[(size_t)modIndex], now + (unsigned)wakeDelay);
    }

    // The scripted update body: the return code plus the programmed side
    // effects (run where the original's module body would run them).
    int runUpdate(ObjectT *obj, int modIndex)
    {
        UpdateModuleT *u = &obj->m_updates[(size_t)modIndex];
        if (u->m_onUpdateDestroyObj != INVALID_ID) {
            ObjectT *victim = object(u->m_onUpdateDestroyObj);
            if (victim)
                logic.destroyObject(victim, &hooks);
        }
        if (u->m_onUpdateWakeObj != INVALID_ID && u->m_onUpdateWakeMod >= 0) {
            ObjectT *other = object(u->m_onUpdateWakeObj);
            if (other)
                logic.friend_awakenUpdateModule(other, &other->m_updates[(size_t)u->m_onUpdateWakeMod],
                                                (unsigned)u->m_onUpdateWakeWhen);
        }
        return u->m_scriptedSleep;
    }

    // Object::reactToTransformChange's dirty gate (the 0.01 thresholds).
    static bool isPosDifferent(const Coord3D &a, const Coord3D &b)
    {
        // this is necessary because PhysicsBehavior may generate tiny changes even when
        // "standing still", due to roundoff errors.
        const float THRESH = 0.01f;
        if (std::fabs(a.x - b.x) > THRESH)
            return true;
        if (std::fabs(a.y - b.y) > THRESH)
            return true;
        if (std::fabs(a.z - b.z) > THRESH)
            return true;
        return false;
    }
    static bool isAngleDifferent(float a, float b)
    {
        const float THRESH = 0.01f; // in radians, this is approx 1/2 degree.
        if (std::fabs(a - b) > THRESH)
            return true;
        return false;
    }

    void reactToTransformChange(ObjectT *obj, const Coord3D &oldPos, float oldAngle)
    {
        bool posDiff = isPosDifferent(oldPos, obj->m_pos);
        bool angDiff = isAngleDifferent(oldAngle, obj->m_angle);

        if (posDiff || angDiff) {
            PartitionData *mod = pm.findModule(obj->m_id);
            if (mod)
                pm.makeDirty(mod, true);
            // (the contain/trigger-area reactions are their own slices)
        }
    }

    // Object::setDisabledUntil's kernel half (mask + till-frame).
    void setDisabledUntil(ObjectT *obj, int type, unsigned frame)
    {
        if (type < 0 || type >= DISABLED_COUNT)
            return;

        if ((unsigned)obj->m_disabledTillFrame[type] != frame) {
            obj->m_disabledTillFrame[type] = (int)frame;
            obj->m_disabledMask.set(type, frame > logic.getFrame());
            // (the rider/spawn/carbomb side effects are their own slices)
        }
    }
    void setDisabled(ObjectT *obj, int type) { setDisabledUntil(obj, type, (unsigned)FOREVER); }

    // Object::checkDisabledStatus (the sweep update() runs).
    void checkDisabledStatus(ObjectT *obj)
    {
        unsigned now = logic.getFrame();
        for (int i = 0; i < DISABLED_COUNT; i++) {
            if (obj->m_disabledMask.test(i)) {
                if (now >= (unsigned)obj->m_disabledTillFrame[i]) {
                    // clearDisabled(type) — the mask clear is the kernel half
                    obj->m_disabledMask.set(i, false);
                }
            }
        }
    }

    // GameLogic::update — the ordered walk, transcribed.
    void update()
    {
        // (LatchRestore<Bool> inUpdateLatch(m_isInUpdate, TRUE))
        bool prevInUpdate = logic.m_isInUpdate;
        logic.m_isInUpdate = true;

        // the startNewGame gate
        if (logic.m_startNewGame) {
            hooks.ev(GameLogicHooks::EV_STARTNEWGAME, -1, -1);
            hooks.m_hookStartNewGame++;
            logic.m_startNewGame = false;
        }

        // send the current time to the GameClient
        unsigned now = logic.getFrame();
        hooks.ev(GameLogicHooks::EV_CLIENT_FRAME, (int)now, -1);
        hooks.m_hookClientFrame++;

        // update (execute) scripts
        {
            hooks.ev(GameLogicHooks::EV_SCRIPTS, -1, -1);
            hooks.m_hookScripts++;
        }

        // Updates the frozen time status because it may have changed after the script engine update.
        // (TheFramePacer->setTimeFrozen(TheGameEngine->isTimeFrozen()) — the corpus
        // controls the engine flag; the pacer mirrors it)
        bool timeFrozen = hooks.m_engineTimeFrozen;
        hooks.ev(GameLogicHooks::EV_FROZEN, timeFrozen ? 1 : 0, -1);
        if (timeFrozen) {
            logic.m_isInUpdate = prevInUpdate;
            return;
        }

        // Note - TerrainLogic update needs to happen after ScriptEngine update, but before object updates.  jba.
        {
            hooks.ev(GameLogicHooks::EV_TERRAIN, -1, -1);
            hooks.m_hookTerrain++;
        }

        // force CRC calculation, so we can keep a cache of the last N CRCs...
        // (the recorder block: stated, not modelled — the hook pins the position)
        {
            hooks.ev(GameLogicHooks::EV_RECORDER, -1, -1);
            hooks.m_hookRecorder++;
        }

        // process client commands
        {
            hooks.ev(GameLogicHooks::EV_COMMANDS, -1, -1);
            hooks.m_hookCommands++;
        }

        // (ALLOW_NONSLEEPY_UPDATES is not defined in this build — no normal
        // updates list exists)

        {
            while (!logic.m_sleepyUpdates.empty()) {
                UpdateModuleT *u = logic.peekSleepyUpdate();

                if (!u) {
                    continue;
                }

                // we're done, everyone else is sleeping.
                // break from the loop BEFORE we pop this item off.
                if ((unsigned)friend_getNextCallFrame(u) > now) {
                    break;
                }

                int sleepLen = UPDATE_SLEEP_NONE; // default, if it is disabled.

                ObjectT *uobj = object(logic.moduleOwner(u));
                DisabledMask dis = uobj->m_disabledMask;
                // TheSuperHackers @bugfix Stubbjax 15/03/2026 The disabled-types-to-process mask is now exclusive.
                // Previously, if the disabled mask had any bits in common with the disabled-types-to-process mask,
                // the update would be processed. Now, if any *other* bits are set in the disabled mask, the update
                // is no longer processed.
                if (u->m_disabledTypesToProcess.testForAll(dis)) {
                    hooks.ev(GameLogicHooks::EV_UPDATE, logic.moduleOwner(u), logic.moduleIndexOf(u));
                    logic.m_curUpdateModule = u;

                    int modIndex = logic.moduleIndexOf(u);
                    sleepLen = runUpdate(uobj, modIndex);
                    if (sleepLen < 1)
                        sleepLen = UPDATE_SLEEP_NONE;

                    logic.m_curUpdateModule = nullptr;
                }

                // else defer it till next frame and re-push it
                friend_setNextCallFrame(u, now + (unsigned)sleepLen);
                logic.rebalanceSleepyUpdate(0);
            }
        }

        // update the Artificial Intelligence system
        {
            hooks.ev(GameLogicHooks::EV_AI, -1, -1);
            hooks.m_hookAI++;
        }

        // production updates
        {
            hooks.ev(GameLogicHooks::EV_BUILD, -1, -1);
            hooks.m_hookBuildAssistant++;
        }

        // update partition info
        {
            hooks.ev(GameLogicHooks::EV_PARTITION, -1, -1);
            pm.update(&hooks);
        }

        //
        // End of frame clean-up
        //

        // destroy all pending objects
        logic.processDestroyList(&pm, &hooks);

        // reset the command list, destroying all messages
        {
            hooks.ev(GameLogicHooks::EV_COMMANDS_RESET, -1, -1);
        }

        {
            hooks.ev(GameLogicHooks::EV_WEAPONSTORE, -1, -1);
            hooks.m_hookWeaponStore++;
        }
        {
            hooks.ev(GameLogicHooks::EV_LOCOMOTORSTORE, -1, -1);
            hooks.m_hookLocomotorStore++;
        }
        {
            hooks.ev(GameLogicHooks::EV_VICTORY, -1, -1);
            hooks.m_hookVictory++;
        }

        {
            // Handle disabled statii (and re-enable objects once frame matches)
            for (ObjectT *obj : logic.m_objList) {
                if (obj->m_disabledMask.any()) {
                    checkDisabledStatus(obj);
                }
            }
            hooks.ev(GameLogicHooks::EV_SWEEP, -1, -1);
        }

        // increment world time
        if (!logic.m_startNewGame) {
            logic.m_frame++;
            logic.m_hasUpdated = true;
            hooks.ev(GameLogicHooks::EV_FRAME_INC, -1, -1);
        }

        logic.m_isInUpdate = prevInUpdate;
    }

    // GameLogic::preUpdate.
    void preUpdate()
    {
        logic.m_hasUpdated = false;

        if (logic.m_pauseFrame == logic.m_frame && logic.m_pauseFrame != 0) {
            logic.m_pauseFrame = 0;
            logic.m_gamePaused = true; // (setGamePaused's kernel half — the dumped flag)
        }
    }

    // GameLogic::loadPostProcess (the ID compaction + heap rebuild).
    void loadPostProcess()
    {
        logic.m_nextObjID = INVALID_ID;
        for (ObjectT *obj : logic.m_objList)
            if (obj->m_id >= logic.m_nextObjID)
                logic.m_nextObjID = (int)((unsigned)obj->m_id + 1u);

        // blow away the sleepy update and normal update module lists
        for (UpdateModuleT *u : logic.m_sleepyUpdates) {
            u->m_indexInLogic = -1;
        }
        logic.m_sleepyUpdates.clear();

        unsigned now = logic.getFrame();
        if (now == 0)
            now = 1;

        // go through all objects, examine each update module and put it on the appropriate update list
        for (ObjectT *obj : logic.m_objList) {
            for (size_t b = 0; b < obj->m_updates.size(); ++b) {
                UpdateModuleT *u = &obj->m_updates[b];

                // check each update module
                unsigned when = (unsigned)friend_getNextCallFrame(u);
                // note that 'when' will only be zero for legacy save files.
                if (when == 0)
                    friend_setNextCallFrame(u, now);

                logic.m_sleepyUpdates.push_back(u);
                u->m_indexInLogic = (int)(logic.m_sleepyUpdates.size() - 1);
            }
        }

        // re-sort the priority queue all at once now that all modules are on it
        logic.remakeSleepyUpdate();
    }
};

// --- the handle plumbing -----------------------------------------------------
int GameLogicT::moduleOwner(const UpdateModuleT *u) const
{
    for (auto &pair : m_objHash) {
        ObjectT *o = pair.second;
        for (size_t i = 0; i < o->m_updates.size(); ++i)
            if (&o->m_updates[i] == u)
                return o->m_id;
    }
    return INVALID_ID;
}
int GameLogicT::moduleIndexOf(const UpdateModuleT *u) const
{
    for (auto &pair : m_objHash) {
        ObjectT *o = pair.second;
        for (size_t i = 0; i < o->m_updates.size(); ++i)
            if (&o->m_updates[i] == u)
                return (int)i;
    }
    return -1;
}
ObjectT *PartitionManagerT::resolveObject(int id) { return m_world ? m_world->object(id) : nullptr; }

// --- the collide-test family's object half (needs the World). ----------------
// PartitionData::collidesWith (the friend_collidesWith the contact list
// calls): the KINDOF_NO_COLLIDE refusal, the z-intersection gate with the
// getMaxHeightBelowPosition extents, and theCollideTestProcs dispatch.
bool World::collidesWith(int thisId, int thatId, CollideLocAndNormal *cinfo)
{
    ObjectT *thisObj = object(thisId);
    ObjectT *thatObj = object(thatId);
    if (!thisObj || !thatObj)
        return false; // (the transcription's dead-id guard; unreachable in the corpora)

    if (thisObj->isKindOfNoCollide() || thatObj->isKindOfNoCollide())
        return false; // A collision extent of zero size is still a point and can collide, but we don't always want to.

    CollideInfoT thisInfo(thisObj->m_pos, thisObj->m_geometryInfo, thisObj->m_angle);
    CollideInfoT thatInfo(thatObj->m_pos, thatObj->m_geometryInfo, thatObj->m_angle);

    // invariant for all geometries: first do z collision check.
    float thisTop = thisInfo.position.z + thisInfo.geom.getMaxHeightAbovePosition();
    float thisBot = thisInfo.position.z - thisInfo.geom.getMaxHeightBelowPosition();
    float thatTop = thatInfo.position.z + thatInfo.geom.getMaxHeightAbovePosition();
    float thatBot = thatInfo.position.z - thatInfo.geom.getMaxHeightBelowPosition();
    if (thisTop >= thatBot && thisBot <= thatTop) {
        int thisGeom = thisObj->m_geometryInfo.getGeomType();
        int thatGeom = thatObj->m_geometryInfo.getGeomType();

        //
        // NOTE: This assumes geometry enumerations that start at GEOMETRY_FIRST AND depends on the
        // order in which they appear in the enum list
        //
        CollideTestProc collideProc =
            theCollideTestProcs[(size_t)(thisGeom - GEOMETRY_FIRST) * GEOMETRY_NUM_TYPES + (thatGeom - GEOMETRY_FIRST)];
        return (*collideProc)(&thisInfo, &thatInfo, cinfo);
    } else {
        // no z-intersection -> no collision.
        return false;
    }
}

// Object::onCollide (Object.cpp): walk the behavior list, dispatch every
// module whose getCollide() returns non-null, re-checking
// OBJECT_STATUS_NO_COLLISIONS each time thru the loop (a collide module
// may set it mid-walk — the break). The non-collide behaviors contribute
// nothing observable, so the walk is over the collide slots only.
static void objectOnCollide(World &w, ObjectT *obj, ObjectT *other, const Coord3D *loc, const Coord3D *normal)
{
    for (size_t i = 0; i < obj->m_colliders.size(); ++i) {
        // check each time thru the loop, in case a collide module sets it
        if (obj->isNoCollisions()) {
            break;
        }

        w.hooks.collideEv(obj->m_id, other->m_id, (int)i, *loc, *normal);

        // the scripted module body (the corpus's three effects, applied
        // in this order: the absorb, the destroy, the unregister)
        CollideModuleT &cm = obj->m_colliders[i];
        if (cm.m_onCollideAbsorb)
            obj->m_status |= (1u << OBJECT_STATUS_NO_COLLISIONS_BIT);
        if (cm.m_onCollideDestroyObj != INVALID_ID) {
            ObjectT *victim = w.object(cm.m_onCollideDestroyObj);
            if (victim)
                w.logic.destroyObject(victim, &w.hooks);
        }
        if (cm.m_onCollideUnregisterObj != INVALID_ID) {
            ObjectT *victim = w.object(cm.m_onCollideUnregisterObj);
            if (victim)
                w.pm.unRegisterObject(victim); // (detachFromObject consults m_contactList)
        }
    }
}

// PartitionContactList::processContactList (PartitionManager.cpp,
// verbatim structure): the null-node skip, the friend_collidesWith call,
// the OBJECT_STATUS_NO_COLLISIONS gates, the node nulling before the
// dispatch, the flipped-normal second call gated by both sides'
// isDestroyed, and the re-dirty family with its refusals.
void PartitionContactList::processContactList(World &w)
{
    for (PartitionContactListNode *cd = m_contactList; cd; cd = cd->m_next)
    {
        if (cd->m_obj == INVALID_ID || cd->m_other == INVALID_ID)
            continue;

        // we know that their partitions overlap; determine if they REALLY collide
        // before proceeding...
        CollideLocAndNormal cinfo = {}; // (the original's uninitialized local — the pinned zero)
        if (!w.collidesWith(cd->m_obj, cd->m_other, &cinfo))
            continue;

        ObjectT *obj = w.object(cd->m_obj);
        ObjectT *other = w.object(cd->m_other);

        if (obj->isNoCollisions() || other->isNoCollisions())
            continue;

        // (the DEBUG_ASSERTCRASH about two immobile things is debug-only — dropped)

        // the onCollide() calls can remove the object(s) from the partition mgr,
        // thus destroying the partitiondata for 'em. go ahead and null these out here
        // so we won't be tempted to use 'em (since they might be bogus).
        cd->m_obj = INVALID_ID;
        cd->m_other = INVALID_ID;

        objectOnCollide(w, obj, other, &cinfo.loc, &cinfo.normal);
        flipCoord3D(&cinfo.normal);

        //Before checking the "other" case, make sure that the previous collision didn't
        //absorb him. This becomes a conflict for pilots giving veterancy to transports
        //and pilots entering transports. Both would occur if these isDestroyed checks
        //were missing.
        if (!obj->isDestroyed() && !other->isDestroyed()) {
            objectOnCollide(w, other, obj, &cinfo.loc, &cinfo.normal);
        }

        //
        // NOTE: it is VERY IMPORTANT (for performance reasons) to not re-dirty immobile things.
        //
        // NOTE also that we re-get partitiondata from the object, since it might have been
        // removed from the partition system by the onCollide call...
        //
        if (!obj->isDestroyed() && obj->m_partitionData != INVALID_ID && !obj->isKindOfImmobile()) {
            w.pm.makeDirty(w.pm.findModule(obj->m_partitionData), false);
        }
        if (!other->isDestroyed() && other->m_partitionData != INVALID_ID && !other->isKindOfImmobile()) {
            w.pm.makeDirty(w.pm.findModule(other->m_partitionData), false);
        }
    }
}

// ============================================================================
// The query family (PartitionManager.cpp's FASTER_GCO build, over
// SimpleObjectIterator.cpp / ObjectIter.h and the filter set).
// ============================================================================

// --- the distance measures (the four distCalcProc_X_Y procs). ---------------

// if the dist is greater than maxDist, return false, and the output stuff is undefined.
typedef bool (*DistCalcProcT)(const Coord3D *posA, const ObjectT *objA, const Coord3D *posB,
                              const ObjectT *objB, float &abDistSqr, Coord3D &abVec, float maxDistSqr);

static bool distCalcProc_CenterAndCenter_2D(const Coord3D *posA, const ObjectT *objA,
                                            const Coord3D *posB, const ObjectT *objB,
                                            float &abDistSqr, Coord3D &abVec, float maxDistSqr)
{
    (void)objA;
    (void)objB;
    // note that object positions are defined as the bottom center of the geometry,
    // thus we must add the radius to the z coord to get the proper center of the bounding sphere.
    Coord3D diff;
    diff.x = posB->x - posA->x;
    diff.y = posB->y - posA->y;
    diff.z = 0.0f;

    abDistSqr = diff.x * diff.x + diff.y * diff.y;
    abVec = diff;

    return abDistSqr < maxDistSqr;
}

static bool distCalcProc_BoundaryAndBoundary_2D(const Coord3D *posA, const ObjectT *objA,
                                                const Coord3D *posB, const ObjectT *objB,
                                                float &abDistSqr, Coord3D &abVec, float maxDistSqr)
{
    Coord3D diff;
    diff.x = posB->x - posA->x;
    diff.y = posB->y - posA->y;
    diff.z = 0.0f;

    float actualDistSqr = diff.x * diff.x + diff.y * diff.y;

    float shrinkFactor = 1.0f;
    float shrunkenDistSqr = actualDistSqr;
    float totalRad = (objA ? objA->m_geometryInfo.getBoundingCircleRadius() : 0.0f) +
                     (objB ? objB->m_geometryInfo.getBoundingCircleRadius() : 0.0f);

    if (totalRad > 0.0f) {
        float actualDist = Sqrtf(actualDistSqr);
        float shrunkenDist = actualDist - totalRad;
        if (shrunkenDist <= 0.0f) {
            shrinkFactor = 0.0f;
            shrunkenDistSqr = 0.0f; // sorry, distances can't be negative
        } else {
            shrinkFactor = shrunkenDist / actualDist;
            shrunkenDistSqr = shrunkenDist * shrunkenDist;
        }
    }

    abDistSqr = shrunkenDistSqr;

    // (the original's DEBUG_ASSERTCRASH on shrinkFactor outside [0,1] is
    // debug-only — dropped)
    diff.x *= shrinkFactor;
    diff.y *= shrinkFactor;
    abVec = diff;
    return abDistSqr < maxDistSqr;
}

static bool distCalcProc_CenterAndCenter_3D(const Coord3D *posA, const ObjectT *objA,
                                            const Coord3D *posB, const ObjectT *objB,
                                            float &abDistSqr, Coord3D &abVec, float maxDistSqr)
{
    (void)objA;
    (void)objB;
    // note that object positions are defined as the bottom center of the geometry,
    // thus we must add the radius to the z coord to get the proper center of the bounding sphere.
    Coord3D diff;
    diff.x = posB->x - posA->x;
    diff.y = posB->y - posA->y;
    diff.z = posB->z - posA->z;

    abDistSqr = diff.x * diff.x + diff.y * diff.y + diff.z * diff.z;
    abVec = diff;

    return abDistSqr < maxDistSqr;
}

static bool distCalcProc_BoundaryAndBoundary_3D(const Coord3D *posA, const ObjectT *objA,
                                                const Coord3D *posB, const ObjectT *objB,
                                                float &abDistSqr, Coord3D &abVec, float maxDistSqr)
{
    const GeometryInfoT *geomA = objA ? &objA->m_geometryInfo : nullptr;
    const GeometryInfoT *geomB = objB ? &objB->m_geometryInfo : nullptr;

    // note that object positions are defined as the bottom center of the geometry,
    // thus we must add the radius to the z coord to get the proper center of the bounding sphere.
    Coord3D diff;
    diff.x = posB->x - posA->x;
    diff.y = posB->y - posA->y;
    diff.z = ((posB->z + (geomB ? geomB->getZDeltaToCenterPosition() : 0.0f)) -
              (posA->z + (geomA ? geomA->getZDeltaToCenterPosition() : 0.0f)));

    float actualDistSqr = diff.x * diff.x + diff.y * diff.y + diff.z * diff.z;

    float shrinkFactor = 1.0f;
    float shrunkenDistSqr = actualDistSqr;
    float totalRad = (geomA ? geomA->getBoundingSphereRadius() : 0) + (geomB ? geomB->getBoundingSphereRadius() : 0);
    if (totalRad > 0.0f) {
        float actualDist = Sqrtf(actualDistSqr);
        float shrunkenDist = actualDist - totalRad;
        if (shrunkenDist <= 0.0f) {
            shrinkFactor = 0.0f;
            shrunkenDistSqr = 0.0f; // sorry, distances can't be negative
        } else {
            shrinkFactor = shrunkenDist / actualDist;
            shrunkenDistSqr = shrunkenDist * shrunkenDist;
        }
    }

    abDistSqr = shrunkenDistSqr;

    diff.x *= shrinkFactor;
    diff.y *= shrinkFactor;
    diff.z *= shrinkFactor;
    abVec = diff;
    return abDistSqr < maxDistSqr;
}

static DistCalcProcT theDistCalcProcs[] = {
    distCalcProc_CenterAndCenter_2D,
    distCalcProc_CenterAndCenter_3D,
    distCalcProc_BoundaryAndBoundary_2D,
    distCalcProc_BoundaryAndBoundary_3D,
};

// --- SimpleObjectIterator (SimpleObjectIterator.cpp). ------------------------

// (the cost compares need the World's object table; the original reads the
// template pointer carried by the clump's object. The transcription's
// statics get the World through a file-scope pointer the corpora set —
// each corpus owns exactly one World at a time. The World is complete
// here — the query family sits after it — so the out-of-line cost
// compares below can dereference it.)
static World *g_queryWorld = nullptr;

struct SimpleObjectIteratorT {
    class ClumpT
    {
    public:
        ClumpT *m_nextClump = nullptr;
        int m_obj = INVALID_ID;
        float m_numeric = 0.0f; // typically, dist-squared
    };

    typedef float (*ClumpCompareProc)(ClumpT *, ClumpT *);

    ClumpT *m_firstClump = nullptr;
    ClumpT *m_curClump = nullptr;
    int m_clumpCount = 0;

    SimpleObjectIteratorT() {}
    ~SimpleObjectIteratorT() { makeEmpty(); }
    // (the original is a MemoryPoolObject — noncopyable; the pointer is
    // never observable. The transcription deletes the copy ops so a
    // by-value return moves.)
    SimpleObjectIteratorT(const SimpleObjectIteratorT &) = delete;
    SimpleObjectIteratorT &operator=(const SimpleObjectIteratorT &) = delete;
    SimpleObjectIteratorT(SimpleObjectIteratorT &&other)
        : m_firstClump(other.m_firstClump), m_curClump(other.m_curClump), m_clumpCount(other.m_clumpCount)
    {
        other.m_firstClump = nullptr;
        other.m_curClump = nullptr;
        other.m_clumpCount = 0;
    }
    SimpleObjectIteratorT &operator=(SimpleObjectIteratorT &&other)
    {
        if (this != &other) {
            makeEmpty();
            m_firstClump = other.m_firstClump;
            m_curClump = other.m_curClump;
            m_clumpCount = other.m_clumpCount;
            other.m_firstClump = nullptr;
            other.m_curClump = nullptr;
            other.m_clumpCount = 0;
        }
        return *this;
    }

    void insert(int obj, float numeric = 0.0f)
    {
        ClumpT *clump = new ClumpT();

        clump->m_nextClump = m_firstClump;
        m_firstClump = clump;

        clump->m_obj = obj;
        clump->m_numeric = numeric;

        ++m_clumpCount;
    }

    int nextWithNumeric(float *num)
    {
        int obj = INVALID_ID;
        if (num)
            *num = 0.0f;

        if (m_curClump) {
            obj = m_curClump->m_obj;
            if (num)
                *num = m_curClump->m_numeric;
            m_curClump = m_curClump->m_nextClump;
        }

        return obj;
    }

    void reset() { m_curClump = m_firstClump; }

    void makeEmpty()
    {
        while (m_firstClump) {
            ClumpT *next = m_firstClump->m_nextClump;
            delete m_firstClump;
            m_firstClump = next;
            --m_clumpCount;
        }

        m_firstClump = nullptr;
        m_curClump = nullptr;
        m_clumpCount = 0;
    }

    void sort(IterOrderType order);

    static float sortNearToFar(ClumpT *a, ClumpT *b) { return a->m_numeric - b->m_numeric; }
    static float sortFarToNear(ClumpT *a, ClumpT *b) { return b->m_numeric - a->m_numeric; }
    static float sortCheapToExpensive(ClumpT *a, ClumpT *b);
    static float sortExpensiveToCheap(ClumpT *a, ClumpT *b);
};

// the cost compares, out-of-line (they read the build costs through the
// corpus's object table): m_obj->getTemplate()->friend_getBuildCost().
float SimpleObjectIteratorT::sortCheapToExpensive(ClumpT *a, ClumpT *b)
{
    return g_queryWorld->object(a->m_obj)->m_buildCost - g_queryWorld->object(b->m_obj)->m_buildCost;
}

float SimpleObjectIteratorT::sortExpensiveToCheap(ClumpT *a, ClumpT *b)
{
    return g_queryWorld->object(b->m_obj)->m_buildCost - g_queryWorld->object(a->m_obj)->m_buildCost;
}

// SimpleObjectIterator::sort (SimpleObjectIterator.cpp): the bottom-up
// linked-list mergesort, verbatim — the exact splice choreography decides
// the order of equal numerics (the tie order the corpora pin).
void SimpleObjectIteratorT::sort(IterOrderType order)
{
    static ClumpCompareProc theClumpCompareProcs[] = {
        nullptr, // "fastest" gets no proc
        &SimpleObjectIteratorT::sortNearToFar,
        &SimpleObjectIteratorT::sortFarToNear,
        &SimpleObjectIteratorT::sortCheapToExpensive,
        &SimpleObjectIteratorT::sortExpensiveToCheap,
    };

    if (m_clumpCount == 0)
        return;

    ClumpCompareProc cmpProc = theClumpCompareProcs[order];

    if (!cmpProc)
        return; // my, that was easy

    // do a basic mergesort, which works nicely for linked lists,
    // and is reasonably efficient (N log N).

    for (int n = 1;; n *= 2) {
        ClumpT *to_do = m_firstClump;
        ClumpT *tail = nullptr;
        m_firstClump = nullptr;

        int mergeCount = 0;

        while (to_do) {
            ++mergeCount;

            int to_do_count = 0;

            // make two lists of length 'n' (to_do is one, sub is the other)
            ClumpT *sub = to_do;
            for (int i = 0; i < n; i++) {
                ++to_do_count;
                sub = sub->m_nextClump;
                if (!sub)
                    break;
            }
            int subCount = sub ? n : 0;

            // merge the two lists.
            while (to_do_count + subCount > 0) {
                ClumpT *tmp;

                // bleah, coalesce into more elegant test case
                if (subCount == 0) {
                    tmp = to_do;
                    to_do = to_do->m_nextClump;
                    --to_do_count;
                } else if (to_do_count == 0) {
                    tmp = sub;
                    sub = sub->m_nextClump;
                    --subCount;
                } else if ((*cmpProc)(to_do, sub) <= 0.0f) {
                    tmp = to_do;
                    to_do = to_do->m_nextClump;
                    --to_do_count;
                } else {
                    tmp = sub;
                    sub = sub->m_nextClump;
                    --subCount;
                }
                if (!sub)
                    subCount = 0;
                if (!to_do)
                    to_do_count = 0;

                if (tail)
                    tail->m_nextClump = tmp;
                else
                    m_firstClump = tmp;
                tail = tmp;
            }

            to_do = sub;
        }
        if (tail)
            tail->m_nextClump = nullptr;

        if (mergeCount <= 1) // when we have done just one (or none) swap, we're done
            break;
    }

    // always reset after sorting, to prevent weirdness
    reset();
}

// --- the partition filters (PartitionManager.h / PartitionManager.cpp). ------

// this is an ABC. getClosestObjects allows you to pass multiple filters to
// filter out objects you don't want; any that get false returned are not
// iterated.
struct PartitionFilterT {
    virtual ~PartitionFilterT() {}
    virtual bool allow(ObjectT *objOther) = 0;
};

// filtersAllow (PartitionManager.cpp), the non-profiling build.
static inline bool filtersAllow(PartitionFilterT **filters, ObjectT *objOther)
{
    for (PartitionFilterT **fp = filters; fp && *fp; fp++) {
        if (!(*fp)->allow(objOther)) {
            return false;
        }
    }
    // assume true if no filters rejected it
    return true;
}

// PartitionFilterWouldCollide: the inline geomCollidesWithGeom body
// (the z-gate that ignores the below-extents, theCollideTestProcs dispatch)
// compared against the desired result.
struct PartitionFilterWouldCollideT : public PartitionFilterT {
    Coord3D m_position;
    GeometryInfoT m_geom;
    float m_angle;
    bool m_desiredCollisionResult; // collision must match this for allow to return true

    PartitionFilterWouldCollideT(const Coord3D &pos, const GeometryInfoT &geom, float angle, bool desired)
        : m_position(pos), m_geom(geom), m_angle(angle), m_desiredCollisionResult(desired)
    {
    }

    bool allow(ObjectT *objOther) override
    {
        CollideInfoT thisInfo(m_position, m_geom, m_angle);
        CollideInfoT thatInfo(objOther->m_pos, objOther->m_geometryInfo, objOther->m_angle);

        bool doesCollide;

        // invariant for all geometries: first do z collision check.
        if (thisInfo.position.z + thisInfo.geom.getMaxHeightAbovePosition() >= thatInfo.position.z &&
            thisInfo.position.z <= thatInfo.position.z + thatInfo.geom.getMaxHeightAbovePosition()) {
            int thisGeom = m_geom.getGeomType();
            int thatGeom = objOther->m_geometryInfo.getGeomType();

            //
            // NOTE: This assumes geometry enumerations that start at GEOMETRY_FIRST AND depends on the
            // order in which they appear in the enum list
            //
            CollideTestProc collideProc =
                theCollideTestProcs[(size_t)(thisGeom - GEOMETRY_FIRST) * GEOMETRY_NUM_TYPES + (thatGeom - GEOMETRY_FIRST)];
            CollideLocAndNormal cinfo = {}; // (the original's uninitialized local — the pinned zero)
            doesCollide = (*collideProc)(&thisInfo, &thatInfo, &cinfo);
        } else {
            // no z-intersection -> no collision.
            doesCollide = false;
        }

        return doesCollide == m_desiredCollisionResult;
    }
};

// PartitionFilterAcceptByKindOf / RejectByKindOf: the isKindOfMulti test
// over the must-be-set / must-be-clear masks.
struct PartitionFilterAcceptByKindOfT : public PartitionFilterT {
    unsigned m_mustBeSet, m_mustBeClear;
    PartitionFilterAcceptByKindOfT(unsigned mustBeSet, unsigned mustBeClear)
        : m_mustBeSet(mustBeSet), m_mustBeClear(mustBeClear)
    {
    }
    bool allow(ObjectT *objOther) override { return objOther->isKindOfMulti(m_mustBeSet, m_mustBeClear); }
};

struct PartitionFilterRejectByKindOfT : public PartitionFilterT {
    unsigned m_mustBeSet, m_mustBeClear;
    PartitionFilterRejectByKindOfT(unsigned mustBeSet, unsigned mustBeClear)
        : m_mustBeSet(mustBeSet), m_mustBeClear(mustBeClear)
    {
    }
    bool allow(ObjectT *objOther) override { return !objOther->isKindOfMulti(m_mustBeSet, m_mustBeClear); }
};

// PartitionFilterAlive: reject the effectively dead.
struct PartitionFilterAliveT : public PartitionFilterT {
    bool allow(ObjectT *objOther) override { return !objOther->isEffectivelyDead(); }
};

// PartitionFilterOnMap: reject the off-map.
struct PartitionFilterOnMapT : public PartitionFilterT {
    bool allow(ObjectT *objOther) override { return !objOther->isOffMap(); }
};

// PartitionFilterSameMapStatus: only objects sharing m_obj's map status.
struct PartitionFilterSameMapStatusT : public PartitionFilterT {
    ObjectT *m_obj;
    explicit PartitionFilterSameMapStatusT(ObjectT *obj) : m_obj(obj) {}
    bool allow(ObjectT *objOther) override { return objOther->isOffMap() == m_obj->isOffMap(); }
};

// --- calcMinRadius / calcRadiusVec (the FASTER_GCO ring precompute). ---------

// PartitionManager::calcMinRadius (PartitionManager.cpp, verbatim):
int PartitionManagerT::calcMinRadius(const ICoord2DT &cur)
{
    /*
        ok, so if obj "A" is in cell (0,0), and obj "B"
        is in cell (cur.x, cur.y), calc the smallest distance they could be
        from each other, in integral multiples of cellSize. Note that
        each object can be anywhere within a given cell (not just in the center!)
        so even objects in the same cell can still be cellSize*1.414 apart,
        and objects in adjacent cells can be nearly-zero distance apart!
    */
    float halfCell = m_cellSize * 0.5f;
    Coord3D centerPos[4] = {
        {-halfCell, -halfCell, 0.0f},
        {halfCell, -halfCell, 0.0f},
        {-halfCell, halfCell, 0.0f},
        {halfCell, halfCell, 0.0f},
    };

    float x = (float)cur.x * m_cellSize;
    float y = (float)cur.y * m_cellSize;
    Coord3D otherPos[4] = {
        {x - halfCell, y - halfCell, 0.0f},
        {x + halfCell, y - halfCell, 0.0f},
        {x - halfCell, y + halfCell, 0.0f},
        {x + halfCell, y + halfCell, 0.0f},
    };

    /*
        this is ugly and there's probably a faster way, but we only do this once per PM reset,
        so it really shouldn't matter... (I hope)
    */

    // (RETAIL_COMPATIBLE_CRC is off in this build: Real, not double)
    typedef float RealOrDouble;

    RealOrDouble minDistSqr = (RealOrDouble)1e12; // double, not real
    for (int i = 0; i < 4; ++i) {
        for (int j = 0; j < 4; ++j) {
            // double, not real
            RealOrDouble dx = centerPos[i].x - otherPos[j].x;
            RealOrDouble dy = centerPos[i].y - otherPos[j].y;
            RealOrDouble curDistSqr = dx * dx + dy * dy;
            if (minDistSqr > curDistSqr)
                minDistSqr = curDistSqr;
        }
    }

    // double, not real
    RealOrDouble dist = Sqrtf(minDistSqr);
    int minRadius = (int)REAL_TO_INT_CEIL(dist / m_cellSize);

    return minRadius;
}

// PartitionManager::calcRadiusVec (PartitionManager.cpp, verbatim):
void PartitionManagerT::calcRadiusVec()
{
    float cellSize = m_cellSize;
    int cx = m_cellCountX;
    int cy = m_cellCountY;

    // (RETAIL_COMPATIBLE_CRC is off in this build: Real, not double)
    typedef float RealOrDouble;

    // double, not real
    RealOrDouble dx = (RealOrDouble)cx * (RealOrDouble)cellSize;
    RealOrDouble dy = (RealOrDouble)cy * (RealOrDouble)cellSize;
    RealOrDouble maxPossibleDist = Sqrtf(dx * dx + dy * dy);

    m_maxGcoRadius = (int)REAL_TO_INT_CEIL(maxPossibleDist / cellSize);

    m_radiusVec.clear();
    m_radiusVec.resize((size_t)m_maxGcoRadius + 1, OffsetVecT());

    ICoord2DT cur;
    for (cur.y = -cy + 1; cur.y < cy; ++cur.y) {
        for (cur.x = -cx + 1; cur.x < cx; ++cur.x) {
            /*
                m_radiusVec[curRadius] contains a list of the cells (foo) that could
                contain objects that are <= (curRadius * cellSize) distance away from cell (0,0).
            */
            int curRadius = calcMinRadius(cur);
            // (the DEBUG_ASSERTCRASH on curRadius > m_maxGcoRadius is
            // debug-only — dropped)
            if (curRadius <= m_maxGcoRadius)
                m_radiusVec[(size_t)curRadius].push_back(cur);
        }
    }
}

// --- getClosestObjects and friends (PartitionManager.cpp, FASTER_GCO). ------

// PartitionManager::getClosestObjects (the FASTER_GCO branch, verbatim).
ObjectT *PartitionManagerT::getClosestObjects(ObjectT *obj, const Coord3D *pos, float maxDist,
                                              DistanceCalculationType dc, PartitionFilterT **filters,
                                              SimpleObjectIteratorT *iterArg, float *closestDistArg,
                                              Coord3D *closestVecArg)
{
    // (the entrancy assert is RTS_DEBUG-only — dropped)
    // (the DEBUG_ASSERTCRASH((obj==nullptr) != (pos == nullptr)) is
    // debug-only — the corpora always pass exactly one)

    DistCalcProcT distProc = theDistCalcProcs[dc];

    const Coord3D *objPos;
    const ObjectT *objToUse;
    if (pos) {
        objPos = pos;
        objToUse = nullptr;
    } else {
        objPos = &obj->m_pos;
        objToUse = obj;
    }
    int cellCenterX, cellCenterY;
    worldToCell(objPos->x, objPos->y, &cellCenterX, &cellCenterY);

    ObjectT *closestObj = nullptr;
    float closestDistSqr = maxDist * maxDist; // if it's not closer than this, we shouldn't consider it anyway...
    Coord3D closestVec;
    // TheSuperHackers @info This should be safe to initialize because it is
    // unused, but let us be extra safe for now. (The non-retail build
    // initializes; the corpora pin the no-match output.)
    closestVec.x = maxDist;
    closestVec.y = maxDist;
    closestVec.z = maxDist;

    int maxRadius = m_maxGcoRadius;
    if (maxDist < HUGE_DIST) {
        // don't go outwards any farther than necessary.
        maxRadius = std::min(m_maxGcoRadius, worldToCellDist(maxDist));
    }
    // (INTENSE_DEBUG's maxRadiusLimit = maxRadius + 3 clamp is not defined
    // in the build — the plain limit)
    int maxRadiusLimit = maxRadius;

    bool foundAny = false;

    ++m_iterFlag; // the function-static theIterFlag, hoisted to manager state

    /*
        m_radiusVec[curRadius] contains a list of the cells (foo) that could
        contain objects that are <= (curRadius * cellSize) distance away from cell (0,0).
    */
    for (int curRadius = 0; curRadius <= maxRadiusLimit; ++curRadius) {
        const OffsetVecT &offsets = m_radiusVec[(size_t)curRadius];
        if (offsets.empty())
            continue;
        for (OffsetVecT::const_iterator it = offsets.begin(); it != offsets.end(); ++it) {
            PartitionCell *thisCell = getCellAt(cellCenterX + it->x, cellCenterY + it->y);
            if (thisCell == nullptr)
                continue;

            for (CellAndObjectIntersection *thisCoi = thisCell->m_firstCoiInCell; thisCoi; thisCoi = thisCoi->m_nextCoi) {
                PartitionData *thisMod = thisCoi->m_module != INVALID_ID ? findModule(thisCoi->m_module) : nullptr;
                ObjectT *thisObj = thisMod ? resolveObject(thisMod->m_object) : nullptr;

                // never compare against ourself.
                if (thisObj == obj || thisObj == nullptr)
                    continue;

                // since an object can exist in multiple COIs, we use this to avoid processing
                // the same one more than once.
                if (thisMod->m_doneFlag == m_iterFlag)
                    continue;
                thisMod->m_doneFlag = m_iterFlag;

                float thisDistSqr;
                Coord3D distVec;
                if (!(*distProc)(objPos, objToUse, &thisObj->m_pos, thisObj, thisDistSqr, distVec, closestDistSqr))
                    continue;

                // check the filters now
                if (!filtersAllow(filters, thisObj))
                    continue;

                // ok, guess this is a winner!
                if (iterArg) {
                    iterArg->insert(thisObj->m_id, thisDistSqr);
                } else {
                    closestObj = thisObj;
                    closestDistSqr = thisDistSqr;
                    closestVec = distVec;

                    if (!foundAny) {
                        // if not adding to iterArg, we want to stop once we have the closest object.
                        maxRadiusLimit = curRadius;
                    }
                    foundAny = true;
                }
            }
        }
    }

    if (closestVecArg) {
        *closestVecArg = closestVec;
    }
    if (closestDistArg) {
        *closestDistArg = Sqrtf(closestDistSqr);
    }

    return closestObj; // might be null...
}

// PartitionManager::getClosestObject (the object form).
ObjectT *PartitionManagerT::getClosestObject(ObjectT *obj, float maxDist, DistanceCalculationType dc,
                                              PartitionFilterT **filters, float *closestDist,
                                              Coord3D *closestDistVec)
{
    return getClosestObjects(obj, nullptr, maxDist, dc, filters, nullptr, closestDist, closestDistVec);
}

// PartitionManager::getClosestObject (the position form).
ObjectT *PartitionManagerT::getClosestObject(const Coord3D *pos, float maxDist, DistanceCalculationType dc,
                                              PartitionFilterT **filters, float *closestDist,
                                              Coord3D *closestDistVec)
{
    return getClosestObjects(nullptr, pos, maxDist, dc, filters, nullptr, closestDist, closestDistVec);
}

// PartitionManager::getVectorTo (both forms).
void PartitionManagerT::getVectorTo(const ObjectT *obj, const ObjectT *otherObj, DistanceCalculationType dc, Coord3D &vec)
{
    DistCalcProcT distProc = theDistCalcProcs[dc];
    float distSqr;
    (*distProc)(&obj->m_pos, obj, &otherObj->m_pos, otherObj, distSqr, vec, HUGE_DIST_SQR);
}

void PartitionManagerT::getVectorTo(const ObjectT *obj, const Coord3D *pos, DistanceCalculationType dc, Coord3D &vec)
{
    DistCalcProcT distProc = theDistCalcProcs[dc];
    float distSqr;
    (*distProc)(&obj->m_pos, obj, pos, nullptr, distSqr, vec, HUGE_DIST_SQR);
}

// PartitionManager::getDistanceSquared (both forms).
float PartitionManagerT::getDistanceSquared(const ObjectT *obj, const ObjectT *otherObj, DistanceCalculationType dc, Coord3D *vec)
{
    DistCalcProcT distProc = theDistCalcProcs[dc];
    float thisDistSqr;
    Coord3D thisVec;
    (*distProc)(&obj->m_pos, obj, &otherObj->m_pos, otherObj, thisDistSqr, thisVec, HUGE_DIST_SQR);
    if (vec)
        *vec = thisVec;
    return thisDistSqr;
}

float PartitionManagerT::getDistanceSquared(const ObjectT *obj, const Coord3D *pos, DistanceCalculationType dc, Coord3D *vec)
{
    DistCalcProcT distProc = theDistCalcProcs[dc];
    float thisDistSqr;
    Coord3D thisVec;
    (*distProc)(&obj->m_pos, obj, pos, nullptr, thisDistSqr, thisVec, HUGE_DIST_SQR);
    if (vec)
        *vec = thisVec;
    return thisDistSqr;
}

// Gets the distance if obj were at goalPos.  Used to calculate attack position paths.
float PartitionManagerT::getGoalDistanceSquared(const ObjectT *obj, const Coord3D *goalPos, const ObjectT *otherObj,
                                                DistanceCalculationType dc, Coord3D *vec)
{
    DistCalcProcT distProc = theDistCalcProcs[dc];
    float thisDistSqr;
    Coord3D thisVec;
    (*distProc)(goalPos, obj, &otherObj->m_pos, otherObj, thisDistSqr, thisVec, HUGE_DIST_SQR);
    if (vec)
        *vec = thisVec;
    return thisDistSqr;
}

float PartitionManagerT::getGoalDistanceSquared(const ObjectT *obj, const Coord3D *goalPos, const Coord3D *otherPos,
                                                DistanceCalculationType dc, Coord3D *vec)
{
    DistCalcProcT distProc = theDistCalcProcs[dc];
    float thisDistSqr;
    Coord3D thisVec;
    (*distProc)(goalPos, obj, otherPos, nullptr, thisDistSqr, thisVec, HUGE_DIST_SQR);
    if (vec)
        *vec = thisVec;
    return thisDistSqr;
}

// PartitionManager::getRelativeAngle2D (both forms).
float PartitionManagerT::getRelativeAngle2D(const ObjectT *obj, const ObjectT *otherObj)
{
    return getRelativeAngle2D(obj, &otherObj->m_pos);
}

float PartitionManagerT::getRelativeAngle2D(const ObjectT *obj, const Coord3D *pos)
{
    Coord3D v;

    // compute vector to given position
    Coord3D objPos = obj->m_pos;
    v.x = pos->x - objPos.x;
    v.y = pos->y - objPos.y;
    v.z = 0.0f;

    float dist = Sqrtf(v.x * v.x + v.y * v.y);

    // normalize
    if (dist == 0.0f)
        return 0.0f;

    Coord3D dir = obj->getUnitDirectionVector2D();

    float distInv = 1.0f / dist;
    v.x *= distInv;
    v.y *= distInv;
    v.z *= distInv;

    // dot of two unit vectors is cos of angle
    float c = dir.x * v.x + dir.y * v.y; // + dir->z*v.z;

    // bound it in case of numerical error
    if (c < -1.0f)
        c = -1.0f;
    else if (c > 1.0f)
        c = 1.0f;

    float value = ACos(c);

    // Determine sign by checking Z component of dir cross v
    // Note this is assumes 2D, and is identical to dotting the perpendicular of v with dir
    float perpZ = dir.x * v.y - dir.y * v.x;
    if (perpZ < 0.0f)
        value = -value;

    return value;
}

// PartitionManager::iterateObjectsInRange (both forms): the fill, then the
// sort. (The original's MemoryPoolObjectHolder dance is memory management —
// the by-value move is the observable-preserving transcription.)
SimpleObjectIteratorT PartitionManagerT::iterateObjectsInRange(ObjectT *obj, float maxDist,
                                                                DistanceCalculationType dc,
                                                                PartitionFilterT **filters, IterOrderType order)
{
    SimpleObjectIteratorT iter;

    getClosestObjects(obj, nullptr, maxDist, dc, filters, &iter, nullptr, nullptr);

    iter.sort(order);
    return iter;
}

SimpleObjectIteratorT PartitionManagerT::iterateObjectsInRange(const Coord3D *pos, float maxDist,
                                                                DistanceCalculationType dc,
                                                                PartitionFilterT **filters, IterOrderType order)
{
    SimpleObjectIteratorT iter;

    getClosestObjects(nullptr, pos, maxDist, dc, filters, &iter, nullptr, nullptr);

    iter.sort(order);
    return iter;
}

// PartitionManager::iteratePotentialCollisions: the bounding-sphere slop
// range and the would-collide filter (no sort — the insertion order is
// the caller's).
SimpleObjectIteratorT PartitionManagerT::iteratePotentialCollisions(const Coord3D *pos, const GeometryInfoT &geom,
                                                                     float angle, bool use2D)
{
    float maxDist = geom.getBoundingSphereRadius();
    maxDist *= 1.1f; // just a little slop

    SimpleObjectIteratorT iter;

    PartitionFilterWouldCollideT filter(*pos, geom, angle, true);
    PartitionFilterT *filters[] = {&filter, nullptr};

    getClosestObjects(nullptr, pos, maxDist, use2D ? FROM_BOUNDINGSPHERE_2D : FROM_BOUNDINGSPHERE_3D, filters,
                      &iter, nullptr, nullptr);

    return iter;
}

// PartitionManager::iterateAllObjects: no distance constraints; we're gonna
// have to process 'em all anyway, so go thru in the fastest way we know how.
SimpleObjectIteratorT PartitionManagerT::iterateAllObjects(PartitionFilterT **filters)
{
    SimpleObjectIteratorT iter;

    PartitionData *mod, *nextMod;
    for (mod = m_moduleList; mod; mod = nextMod) {
        nextMod = mod->m_next;
        ObjectT *obj = mod->m_object != INVALID_ID ? resolveObject(mod->m_object) : nullptr;
        if (obj && filtersAllow(filters, obj)) {
            iter.insert(obj->m_id);
        }
    }

    return iter;
}


// ============================================================================
// The dumper.
// ============================================================================
static unsigned f32bits(float f) { return bits_of(f); }

static const char *evName(int e)
{
    switch (e) {
    case GameLogicHooks::EV_CLIENT_FRAME: return "clientframe";
    case GameLogicHooks::EV_SCRIPTS: return "scripts";
    case GameLogicHooks::EV_FROZEN: return "frozen";
    case GameLogicHooks::EV_TERRAIN: return "terrain";
    case GameLogicHooks::EV_RECORDER: return "recorder";
    case GameLogicHooks::EV_COMMANDS: return "commands";
    case GameLogicHooks::EV_UPDATE: return "update";
    case GameLogicHooks::EV_AI: return "ai";
    case GameLogicHooks::EV_BUILD: return "build";
    case GameLogicHooks::EV_PARTITION: return "partition";
    case GameLogicHooks::EV_DESTROY: return "destroy";
    case GameLogicHooks::EV_COMMANDS_RESET: return "commandsreset";
    case GameLogicHooks::EV_WEAPONSTORE: return "weaponstore";
    case GameLogicHooks::EV_LOCOMOTORSTORE: return "locomotorstore";
    case GameLogicHooks::EV_VICTORY: return "victory";
    case GameLogicHooks::EV_SWEEP: return "sweep";
    case GameLogicHooks::EV_FRAME_INC: return "frameinc";
    case GameLogicHooks::EV_STARTNEWGAME: return "startnewgame";
    case GameLogicHooks::EV_OBJ_CREATED: return "objcreated";
    case GameLogicHooks::EV_OBJ_DESTROYED: return "objdestroyed";
    case GameLogicHooks::EV_CELL_CHANGE: return "cellchange";
    default: return "?";
    }
}

// ============================================================================
// The players/teams/relationships block's methods (defined after the World —
// the same forward-definition dance the collide family uses).
// ============================================================================

// The free-function forwards the block's own order needs.
static void teamUpdateState(TeamT *t, World *w);
static void protoUpdateState(TeamProtoT *tp, World *w);

// ---- the players/teams partition filters (PartitionManager.cpp). The flag
// set is (1 << ALLIES / 1 << ENEMIES / 1 << NEUTRAL) — the (m_flags & (1<<r))
// test of PartitionFilterRelationship::allow.
struct PartitionFilterRelationshipT : PartitionFilterT {
    const ObjectT *m_obj;
    int m_flags;
    enum RelationshipAllowTypes {
        F_ALLOW_ALLIES = (1 << R_ALLIES),
        F_ALLOW_ENEMIES = (1 << R_ENEMIES),
        F_ALLOW_NEUTRAL = (1 << R_NEUTRAL),
    };
    PartitionFilterRelationshipT(const ObjectT *obj, int flags = F_ALLOW_ENEMIES) : m_obj(obj), m_flags(flags) {}
    bool allow(ObjectT *objOther) override;
};

// PartitionFilterSamePlayer (PartitionManager.cpp).
struct PartitionFilterSamePlayerT : PartitionFilterT {
    const PlayerT *m_player;
    PartitionFilterSamePlayerT(const PlayerT *player) : m_player(player) {}
    bool allow(ObjectT *objOther) override;
};

// PartitionFilterAcceptOnTeam (PartitionManager.cpp).
struct PartitionFilterAcceptOnTeamT : PartitionFilterT {
    const TeamT *m_team;
    PartitionFilterAcceptOnTeamT(const TeamT *team) : m_team(team) {}
    bool allow(ObjectT *objOther) override;
};

// TeamPrototype's ctor (Team.cpp): the factory registration, the owner's
// team list. The name key is interned by the CALLER (initTeam) — the map
// registration inside reads it.
TeamProtoT::TeamProtoT(TeamFactoryT *tf, const std::string &name, PlayerT *ownerPlayer,
                       bool isSingleton, const TeamSpecT *d, TeamProtoIDT id, unsigned nameKey)
    : m_id(id), m_factory(tf), m_name(name), m_nameKey(nameKey), m_owningPlayer(ownerPlayer),
      m_flags(isSingleton ? TeamProtoT::TEAM_SINGLETON : 0), m_template(d)
{
    if (m_factory)
        m_factory->addTeamPrototypeToList(this);
    if (m_owningPlayer)
        m_owningPlayer->addTeamToList(this);
}

// ---- the World's player/team handle plumbing (Team*/Player* → handles) ----

static TeamT *teamByID(World *w, TeamIDT id)
{
    if (id == TEAM_ID_T_INVALID)
        return nullptr;
    for (TeamT *t : w->teamFactory.m_arena)
        if (t->m_id == id)
            return t;
    return nullptr; // (a deleted team's id — the corpus never resolves one)
}

// PlayerT::setPlayerRelationship (Player.cpp): note that this creates the
// entry if it doesn't exist.
void playerSetPlayerRelationship(PlayerT *p, PlayerT *that, RelationshipT r)
{
    if (that != nullptr)
        p->setPlayerRelationEntry(that->m_playerIndex, r);
}

// PlayerT::removePlayerRelationship (Player.cpp): the null form clears the
// whole map; the empty map refuses.
bool playerRemovePlayerRelationship(PlayerT *p, PlayerT *that)
{
    if (!p->m_playerRelations.empty()) {
        if (that == nullptr) {
            p->m_playerRelations.clear();
            return true;
        }
        return p->erasePlayerRelation(that->m_playerIndex);
    }
    return false;
}

// PlayerT::setTeamRelationship / removeTeamRelationship (Player.cpp).
void playerSetTeamRelationship(PlayerT *p, TeamT *that, RelationshipT r)
{
    if (that != nullptr)
        p->setTeamRelationEntry(that->getID(), r);
}
bool playerRemoveTeamRelationship(PlayerT *p, TeamT *that)
{
    if (!p->m_teamRelations.empty()) {
        if (that == nullptr) {
            p->m_teamRelations.clear();
            return true;
        }
        return p->eraseTeamRelation(that->getID());
    }
    return false;
}

// PlayerT::getRelationship(const Team*) (Player.cpp): the team-override
// first, then that team's controlling player's entry, then NEUTRAL.
static RelationshipT playerGetRelationship(const PlayerT *p, const TeamT *that, World *w)
{
    if (that) {
        // do we have an override for that particular team? if so, return it.
        if (!p->m_teamRelations.empty()) {
            const RelationshipT *it = p->findTeamRelation(that->getID());
            if (it)
                return *it;
        }
        // hummm... well, do we have something for that team's player?
        if (!p->m_playerRelations.empty()) {
            const PlayerT *thatPlayer = that->getPrototype() ? that->getPrototype()->m_owningPlayer : nullptr;
            if (thatPlayer != nullptr) {
                const RelationshipT *it = p->findPlayerRelation(thatPlayer->m_playerIndex);
                if (it)
                    return *it;
            }
        }
    }
    (void)w;
    return R_NEUTRAL;
}

// TeamT::getRelationship(const Team*) (Team.cpp): the team-override, then
// the player-override, then our controlling player's view.
static RelationshipT teamGetRelationship(const TeamT *t, const TeamT *that, World *w)
{
    // do we have an override for that particular team? if so, return it.
    if (!t->m_teamRelations.empty() && that != nullptr) {
        const RelationshipT *it = t->findTeamRelation(that->getID());
        if (it)
            return *it;
    }
    // hummm... well, do we have an override for that team's player?
    if (!t->m_playerRelations.empty() && that != nullptr) {
        const PlayerT *thatPlayer = that->getPrototype() ? that->getPrototype()->m_owningPlayer : nullptr;
        if (thatPlayer != nullptr) {
            const RelationshipT *it = t->findPlayerRelation(thatPlayer->m_playerIndex);
            if (it)
                return *it;
        }
    }
    // nope -- go with our Player's view on the matter.
    return playerGetRelationship(t->getPrototype()->m_owningPlayer, that, w);
}

// Object::getRelationship (Object.cpp): the defector gates, then the team
// chain.
static RelationshipT objectGetRelationship(const ObjectT *obj, const ObjectT *that, World *w)
{
    const TeamT *myTeam = teamByID(w, obj->m_team);

    if (myTeam && that) {
        if (obj->getIsUndetectedDefector()) {
            return R_NEUTRAL; // so my AI does not give away my position by auto acquire
        }
        if (that->getIsUndetectedDefector()) {
            return R_ALLIES; // so I treat undetecteddefectors like they were my very own
        }
        return teamGetRelationship(myTeam, teamByID(w, that->m_team), w);
    }
    return R_NEUTRAL;
}

// Object::getControllingPlayer (Object.h): m_team->getControllingPlayer().
static PlayerT *objectControllingPlayer(const ObjectT *obj, World *w)
{
    const TeamT *t = teamByID(w, obj->m_team);
    if (!t || !t->getPrototype())
        return nullptr;
    return t->getPrototype()->m_owningPlayer;
}

// Player::becomingTeamMember (Player.cpp): the energy/money/battle-plan/UI
// bodies are the economy and client slices' — the CALL is the pinned
// observable (the object, the player, the direction).
static void playerBecomingTeamMember(PlayerT *p, ObjectT *obj, bool yes, World *w)
{
    (void)p;
    if (!obj)
        return;
    w->hooks.tev(GameLogicHooks::TEV_TEAM_MEMBER, obj->m_id, p ? p->m_playerIndex : -1, yes ? 1 : 0);
}

// Object::setOrRestoreTeam (Object.cpp): the member-list swap with the
// becomingTeamMember calls and the onCapture hook.
static void objectSetOrRestoreTeam(ObjectT *obj, TeamT *team, bool restoring, World *w)
{
    // don't do anything if the team hasn't changed (the pointer compare is
    // the id compare: ids are unique and never reused)
    if (obj->m_team == (team ? team->getID() : TEAM_ID_T_INVALID))
        return;
    TeamT *oldTeam = teamByID(w, obj->m_team);

    // Before Switch //////////////////////////
    if (oldTeam) {
        if (oldTeam->isInList_TeamMemberList(obj)) {
            oldTeam->removeFrom_TeamMemberList(obj);
            playerBecomingTeamMember(oldTeam->getPrototype()->m_owningPlayer, obj, false, w);
        }
    }

    // Switch //////////////////////////
    obj->m_team = team ? team->getID() : TEAM_ID_T_INVALID;

    // After Switch //////////////////////////
    if (team) {
        if (!team->isInList_TeamMemberList(obj)) {
            team->prependTo_TeamMemberList(obj);
            playerBecomingTeamMember(team->getPrototype()->m_owningPlayer, obj, true, w);
        }
        // (now, adjust the attitude of the unit to its new team — the AI
        // update's setAttitude / attack priority; the AI slice's. The
        // drawable's changedTeam is the client's.)
    }

    // This can't just go in ::defect, because some things just do setTeam...
    // And it needs to happen after the switch.
    if (oldTeam && team && !restoring) {
        // obj->onCapture(oldTeam's player, team's player) — the body is the
        // module framework's (contain, body, experience notifications); the
        // hook pins the call and its player pair.
        w->hooks.tev(GameLogicHooks::TEV_CAPTURE, obj->m_id,
                     oldTeam->getPrototype()->m_owningPlayer ? oldTeam->getPrototype()->m_owningPlayer->m_playerIndex : -1,
                     team->getPrototype()->m_owningPlayer ? team->getPrototype()->m_owningPlayer->m_playerIndex : -1);
    }
    // (the radar resort and TheInGameUI's objectChangedTeam are the
    // client's — stated, not modelled)
}

// Object::setTeam (Object.cpp): the isPlayerActive refusal, then the
// temporary-team path.
static void objectSetTeam(ObjectT *obj, TeamT *team, World *w)
{
    // In order to prevent spawning useful units for a player after he dies, we
    // just assign objects to the neutral player if we try to misbehave.
    if (team && !team->getPrototype()->m_owningPlayer->isPlayerActive())
        team = teamByID(w, w->playerList.getNeutralPlayer()->m_defaultTeam);

    // setTemporaryTeam(team) → setOrRestoreTeam(team, restoring=false)
    objectSetOrRestoreTeam(obj, team, false, w);
    // (m_originalTeamName = m_team ? m_team->getName() : "" — display state,
    // not dumped)
}

// ---- TeamFactory (Team.cpp) -------------------------------------------------

// TeamFactory::addTeamPrototypeToList: the duplicate refusal ("already
// exists... skipping") — the std::map insert semantics.
void TeamFactoryT::addTeamPrototypeToList(TeamProtoT *team)
{
    // (the key is the ctor-time interning of the name; the caller has
    // already interned it — the transcription stores it on the proto)
    unsigned nk = team->m_nameKey;
    for (const auto &kv : m_prototypes) {
        if (kv.first == nk) {
            // DEBUG_ASSERTCRASH((*it).second==team, ...) — the release build skips
            return; // already present
        }
    }
    // (std::map's operator[] insert — the sorted insert keeps the KEY order)
    m_prototypes.push_back({nk, team});
    std::sort(m_prototypes.begin(), m_prototypes.end(),
              [](const std::pair<unsigned, TeamProtoT *> &a, const std::pair<unsigned, TeamProtoT *> &b) { return a.first < b.first; });
}

void TeamFactoryT::removeTeamPrototypeFromList(TeamProtoT *team)
{
    unsigned nk = team->m_nameKey;
    for (size_t i = 0; i < m_prototypes.size(); ++i) {
        if (m_prototypes[i].first == nk) {
            m_prototypes.erase(m_prototypes.begin() + (long)i);
            return;
        }
    }
}

TeamProtoT *TeamFactoryT::findTeamPrototype(const std::string &name, World *w)
{
    unsigned nk = w->nameKeys.nameToKey(name);
    return findTeamProtoByKey(nk);
}

// TeamFactory::findTeamPrototypeByID: the map walk (key order).
TeamProtoT *TeamFactoryT::findTeamPrototypeByID(TeamProtoIDT id)
{
    for (const auto &kv : m_prototypes) {
        TeamProtoT *prototype = kv.second;
        if (prototype->m_id == id)
            return prototype;
    }
    return nullptr; // not found
}

// TeamPrototype::findTeamByID: the instance-list walk (newest first).
static TeamT *protoFindTeamByID(TeamProtoT *tp, TeamIDT teamID)
{
    for (auto it = tp->m_instances.rbegin(); it != tp->m_instances.rend(); ++it) {
        if ((*it)->getID() == teamID)
            return *it;
    }
    return nullptr;
}

// TeamFactory::findTeamByID: the prototype-map walk (key order), each
// prototype's instance list.
TeamT *TeamFactoryT::findTeamByID(TeamIDT teamID)
{
    // simple case
    if (teamID == TEAM_ID_T_INVALID)
        return nullptr;
    // search all prototypes for the matching team ID
    for (const auto &kv : m_prototypes) {
        TeamT *team = protoFindTeamByID(kv.second, teamID);
        if (team)
            return team;
    }
    return nullptr;
}

// Team's ctor (Team.cpp): the state birth, the instance-list prepend, the
// checkEnemySighted derivation.
static TeamT *teamNew(TeamFactoryT *tf, TeamProtoT *proto, TeamIDT id, World *w)
{
    (void)w;
    TeamT *t = new TeamT();
    tf->m_arena.push_back(t);
    t->m_id = id;
    t->m_proto = proto;
    if (proto) {
        proto->prependTo_TeamInstanceList(t);
        const TeamSpecT *info = proto->m_template;
        if (info && (!info->scriptOnAllClear.empty() || !info->scriptOnEnemySighted.empty())) {
            t->m_checkEnemySighted = true; // Only keep track of enemy sighted if there is a script that cares.
        }
        // (TheScriptEngine->AppendDebugMessage("%s - creating team instance.")
        // — debug-log only, not modelled)
    }
    return t;
}

// createInactiveTeam's executeActions block (the production-condition
// script run): findScriptByName + friend_executeAction.
static void protoRunProductionCondition(TeamProtoT *tp, World *w)
{
    if (tp->m_template && tp->m_template->executeActions) {
        // const Script *script = TheScriptEngine->findScriptByName(condition)
        if (!tp->m_template->productionCondition.empty() && tp->m_template->productionScriptExists) {
            // friend_executeAction(script->getAction()) — the proto's id
            // carries the team the production belongs to (the instance is
            // born or reused around this call)
            w->hooks.tev(GameLogicHooks::TEV_TEAM_SCRIPT, -1,
                         GameLogicHooks::SCRIPT_PRODUCTION_CONDITION, (int)tp->m_id);
        }
    }
}

// TeamFactory::createInactiveTeam (Team.cpp).
TeamT *TeamFactoryT::createInactiveTeam(const std::string &name, World *w)
{
    TeamProtoT *tp = findTeamPrototype(name, w);
    if (!tp) {
        // DEBUG_CRASH(("Team prototype '%s' does not exist"))
        return nullptr;
    }

    TeamT *t = nullptr;
    if (tp->getIsSingleton()) {
        t = tp->getFirstItemIn_TeamInstanceList();
        if (t) {
            protoRunProductionCondition(tp, w);
            return t;
        }
    }

    t = teamNew(this, tp, ++m_uniqueTeamID, w);
    protoRunProductionCondition(tp, w);
    return t;
}

// TeamFactory::createTeam.
TeamT *TeamFactoryT::createTeam(const std::string &name, World *w)
{
    TeamT *t = createInactiveTeam(name, w);
    if (t)
        t->setActive();
    return t;
}

// TeamFactory::createTeamOnPrototype.
TeamT *TeamFactoryT::createTeamOnPrototype(TeamProtoT *prototype, World *w)
{
    if (prototype == nullptr)
        return nullptr; // (throw ERROR_BAD_ARG)

    TeamT *t = nullptr;
    if (prototype->getIsSingleton()) {
        t = prototype->getFirstItemIn_TeamInstanceList();
        if (t)
            return t;
    }
    t = teamNew(this, prototype, ++m_uniqueTeamID, w);
    t->setActive();
    return t;
}

// TeamFactory::findTeam (Team.cpp): the auto-create of a non-singleton's
// first instance.
TeamT *TeamFactoryT::findTeam(const std::string &name, World *w)
{
    TeamProtoT *tp = findTeamPrototype(name, w);
    if (tp) {
        TeamT *t = tp->getFirstItemIn_TeamInstanceList();
        if (t == nullptr && !tp->getIsSingleton()) {
            t = createInactiveTeam(name, w);
        }
        return t;
    }
    return nullptr;
}

// TeamPrototype::teamAboutToBeDeleted: every remaining instance drops its
// override for the dying team's id.
static void protoTeamAboutToBeDeleted(TeamProtoT *tp, TeamT *team)
{
    for (auto it = tp->m_instances.rbegin(); it != tp->m_instances.rend(); ++it) {
        // iter.cur()->removeOverrideTeamRelationship(team ? team->getID() : TEAM_ID_INVALID)
        (*it)->removeOverrideTeamRelationship(team ? team->getID() : TEAM_ID_T_INVALID);
    }
}

// TeamFactory::teamAboutToBeDeleted (Team.cpp): the prototype walk (map
// order), then the player walk. During clear() the map is ALREADY EMPTY
// (the copy-then-clear dance) — the prototype half is a no-op there and
// only the player half runs; the corpora pin the contrast.
void TeamFactoryT::teamAboutToBeDeleted(TeamT *team, World *w)
{
    for (const auto &kv : m_prototypes) {
        protoTeamAboutToBeDeleted(kv.second, team);
    }
    // (the original checks the global here: "if (ThePlayerList)" — always
    // true once the world exists)
    w->playerListTeamAboutToBeDeleted(team);
}

// ~Team (Team.cpp): the notifyOfTeamDestruction hook, the players'
// preTeamDestroy walk, the member detach, the instance-list removal.
static void teamDelete(TeamT *t, World *w)
{
    // TheScriptEngine->notifyOfTeamDestruction(this)
    w->hooks.tev(GameLogicHooks::TEV_TEAM_DESTROYED, (int)t->getID());

    // Tell the players a team is going away.
    for (int i = 0; i < w->playerList.m_playerCount; i++) {
        PlayerT *plyr = w->playerList.getNthPlayer(i);
        if (plyr) {
            plyr->preTeamDestroy(t);
        }
    }

    ObjectT *tm;
    while ((tm = t->getFirstItemIn_TeamMemberList()) != nullptr) {
        // tm->setTeam(nullptr) — the full path (the detach walk fires the
        // becomingTeamMember(false) transitions)
        objectSetTeam(tm, nullptr, w);
    }

    if (t->m_proto && t->m_proto->isInList_TeamInstanceList(t))
        t->m_proto->removeFrom_TeamInstanceList(t);

    // (the relation maps and the xfer member-id list are destructors' — no
    // observable)
    for (size_t i = 0; i < w->teamFactory.m_arena.size(); ++i) {
        if (w->teamFactory.m_arena[i] == t) {
            w->teamFactory.m_arena.erase(w->teamFactory.m_arena.begin() + (long)i);
            break;
        }
    }
    delete t;
}

// deleteTeamCallback (Team.cpp): the per-team deletion the prototype's
// removeAll walks.
static void deleteTeamCallback(TeamT *o, World *w)
{
    if (o) {
        w->teamFactory.teamAboutToBeDeleted(o, w);
        teamDelete(o, w);
    }
}

// TeamFactory::clear (Team.cpp): the map copy, the empty, the ordered
// destruction (key order; each prototype's instances newest-first).
void TeamFactoryT::clear(World *w)
{
    // must remove it from the map before deleting the TeamProto, since
    // the TeamProto will try to remove itself from the list when it goes away
    std::vector<std::pair<unsigned, TeamProtoT *>> tmp = m_prototypes;
    m_prototypes.clear();
    for (auto &kv : tmp) {
        // ~TeamPrototype: removeAll_TeamInstanceList(deleteTeamCallback),
        // the owner's removeTeamFromList, the factory's
        // removeTeamPrototypeFromList (a no-op — the map is already empty)
        TeamProtoT *tp = kv.second;
        while (!tp->m_instances.empty()) {
            TeamT *t = tp->m_instances.back();
            tp->m_instances.pop_back();
            deleteTeamCallback(t, w);
        }
        if (tp->m_owningPlayer)
            tp->m_owningPlayer->removeTeamFromList(tp);
        delete tp;
    }
}

// TeamFactory::init / reset / update.
void TeamFactoryT::init(World *w) { clear(w); }
void TeamFactoryT::reset(World *w)
{
    m_uniqueTeamPrototypeID = TEAM_PROTO_ID_T_INVALID;
    m_uniqueTeamID = TEAM_ID_T_INVALID;
    clear(w);
}

// TeamFactory::initFromSides (Team.cpp).
void TeamFactoryT::initFromSides(const SidesSpecT &sides, World *w)
{
    clear(w);
    // create the teams we need.
    for (const TeamSpecT &t : sides.teams) {
        initTeam(t.name, t.owner, t.singleton, &t, w);
    }
}

// TeamFactory::initTeam (Team.cpp): the owner lookup with the neutral
// fallback, the prototype creation, the singleton instance.
void TeamFactoryT::initTeam(const std::string &name, const std::string &owner, bool isSingleton,
                            const TeamSpecT *d, World *w)
{
    PlayerT *pOwner = w->playerList.findPlayerWithNameKey(w->nameKeys.nameToKey(owner), w);
    if (!pOwner)
        pOwner = w->playerList.getNeutralPlayer();
    unsigned nk = w->nameKeys.nameToKey(name);
    TeamProtoT *tp = new TeamProtoT(this, name, pOwner, isSingleton, d, ++m_uniqueTeamPrototypeID, nk);
    if (isSingleton) {
        // Create the singleton team.
        createInactiveTeam(name, w);
    }
    (void)tp;
}

// ---- PlayerT's team-list plumbing (Player.cpp) -------------------------------

// Player::addTeamToList: the dedup scan, the push_back.
void PlayerT::addTeamToList(TeamProtoT *team)
{
    for (const TeamProtoT *t : m_playerTeamPrototypes) {
        if (team == t)
            return; // already present
    }
    m_playerTeamPrototypes.push_back(team);
}

// Player::removeTeamFromList: the first-match erase.
void PlayerT::removeTeamFromList(TeamProtoT *team)
{
    for (size_t i = 0; i < m_playerTeamPrototypes.size(); ++i) {
        if (m_playerTeamPrototypes[i] == team) {
            m_playerTeamPrototypes.erase(m_playerTeamPrototypes.begin() + (long)i);
            return;
        }
    }
}

// Player::preTeamDestroy: the AI notification (stated) and the default-team
// nulling (TheSuperHackers @bugfix Mauller/Xezon 03/05/2025).
void PlayerT::preTeamDestroy(const TeamT *team)
{
    // (m_ai->aiPreTeamDestroy(team) — the AI slice's)
    if (m_defaultTeam == (team ? team->getID() : TEAM_ID_T_INVALID))
        m_defaultTeam = TEAM_ID_T_INVALID;
}

// Player::setDefaultTeam (Player.cpp): the "team" + playerName concat, the
// findTeam, the setActive.
void PlayerT::setDefaultTeam(World *w)
{
    std::string tname = "team";
    tname += m_playerName;
    TeamT *dt = w->teamFactory.findTeam(tname, w);
    if (dt) {
        m_defaultTeam = dt->getID();
        dt->setActive();
    }
    // (DEBUG_ASSERTCRASH(dt, ("no team")) — the release build continues with
    // the null default team; the corpora pin the missing-prototype path)
}

// Player::updateTeamStates: the prototype walk (push_back order).
void PlayerT::updateTeamStates(World *w)
{
    for (TeamProtoT *tp : m_playerTeamPrototypes) {
        protoUpdateState(tp, w);
    }
}

// Player::update (Player.cpp): the AI/team-scripts/energy/academy/money
// bodies are later slices — the single hook pins the 16-slot walk's
// position (TheAI->UPDATE calls ThePlayerList->UPDATE from AI.cpp:363).
void PlayerT::update(World *w)
{
    // (m_ai->update(); the team generic-script walk; the power-sabotage
    // check; getAcademyStats()->update(); the retaliation-mode posting; the
    // tunnel heal; m_money.updateIncomeBucket() — the AI, script-engine,
    // economy and client slices' bodies)
    w->hooks.tev(GameLogicHooks::TEV_PLAYER_UPDATE, m_playerIndex);
}

// Player::newMap: the AI's newMap — stated (the AI slice's).
void PlayerT::newMap(World *w) { (void)w; }

// Player::setPlayerType (Player.cpp): the type write (the AI instance
// lifecycle — create/delete — is the AI slice's, stated).
void PlayerT::setPlayerType(PlayerTypeT t, bool skirmish)
{
    m_playerType = t;
    (void)skirmish;
    // (deleteInstance(m_ai); PLAYER_COMPUTER creates AISkirmishPlayer or
    // AIPlayer — the AI slice's)
}

// Player::init (Player.cpp), the kernel half: the neutral branch verbatim,
// the template branch distilled (the side/color/money/template fields are
// the rzh-data money block's).
void PlayerT::init(bool hasTemplate, World *w)
{
    m_isPlayerDead = false;
    m_observer = false;
    m_defaultTeam = TEAM_ID_T_INVALID;
    // (the squads, selection, tunnel system, build lists, upgrade lists,
    // rank/sciences, energy, stats, retaliation — the money/energy/ranks/
    // sciences block and the client's)

    if (hasTemplate) {
        // the pt branch: the names cleared, PLAYER_COMPUTER, the observer
        // bits from the template (the money/side/color halves stated)
        m_playerName.clear();
        m_playerNameKey = 0; // NAMEKEY_INVALID
        m_playerType = PLAYER_COMPUTER_T;
    } else {
        // no player template? must be the neutral player!
        m_playerName.clear();
        m_playerNameKey = w->nameKeys.nameToKey("");
        m_playerType = PLAYER_COMPUTER_T;

        // neutral is always "allied" with self -- this is the only thing ever allied with neutral!
        playerSetPlayerRelationship(this, this, R_ALLIES);
    }
    // NOTE (the pinned quirk): the relation maps are NOT cleared here — they
    // are born in the ctor and die in the dtor only, so entries survive
    // PlayerList::reset()/init() within a process lifetime. newGame's
    // assignment walk overwrites the entries the new sides name; the rest
    // stay stale. The corpora pin this.
}

// PlayerList::init (PlayerList.cpp).
void World::playerListInit()
{
    playerList.m_playerCount = 1;
    playerList.m_players[0]->init(false, this);

    for (int i = 1; i < MAX_PLAYER_COUNT; i++)
        playerList.m_players[i]->init(false, this);

    std::fill(playerList.m_slotIndices, playerList.m_slotIndices + MAX_PLAYER_COUNT, -1);
    std::fill(playerList.m_slotToPlayerIndices, playerList.m_slotToPlayerIndices + MAX_SLOTS_T, -1);

    // call setLocalPlayer so that becomingLocalPlayer() gets called appropriately
    playerListSetLocalPlayer(playerList.m_players[0]);
}

// PlayerList::reset (PlayerList.cpp): the AI delete walk (stated), the
// factory clear, the init.
void World::playerListReset()
{
    // for (int i = 0; i < MAX_PLAYER_COUNT; i++)
    //     m_players[i]->deletePlayerAI();   — the AI slice's (no observable)
    teamFactory.clear(this);
    playerListInit();
}

// Player::becomingLocalPlayer: the client body (team color, garrison
// recalc, disguiser colors) is the client phase's — the transition calls
// are the pinned observable.
static void playerBecomingLocalPlayer(PlayerT *p, bool yes, World *w)
{
    w->hooks.tev(GameLogicHooks::TEV_LOCAL, p->m_playerIndex, yes ? 1 : 0);
}

// PlayerList::setLocalPlayer (PlayerList.cpp).
void World::playerListSetLocalPlayer(PlayerT *player)
{
    // can't set local player to null -- if you try, you get neutral.
    if (player == nullptr) {
        player = playerList.getNeutralPlayer();
    }

    if (player != playerList.m_local) {
        // m_local can be null the very first time we call this.
        if (playerList.m_local)
            playerBecomingLocalPlayer(playerList.m_local, false, this);
        playerList.m_local = player;
        playerBecomingLocalPlayer(player, true, this);
    }
}

// PlayerList::findPlayerWithNameKey (PlayerList.cpp): the count-bounded
// walk.
PlayerT *PlayerListT::findPlayerWithNameKey(unsigned key, World *w) const
{
    (void)w;
    for (int i = 0; i < m_playerCount; i++) {
        if (m_players[i]->m_playerNameKey == key) {
            return m_players[i];
        }
    }
    return nullptr;
}

// PlayerList::update (PlayerList.cpp): ALL 16 slots (not the count!) — the
// pinned quirk.
void World::playerListUpdate()
{
    for (int i = 0; i < MAX_PLAYER_COUNT; i++) {
        playerList.m_players[i]->update(this);
    }
}

// PlayerList::newMap (PlayerList.cpp).
void World::playerListNewMap()
{
    for (int i = 0; i < MAX_PLAYER_COUNT; i++) {
        playerList.m_players[i]->newMap(this);
    }
}

// PlayerList::teamAboutToBeDeleted (PlayerList.cpp).
void World::playerListTeamAboutToBeDeleted(TeamT *team)
{
    for (int i = 0; i < MAX_PLAYER_COUNT; i++) {
        playerRemoveTeamRelationship(playerList.m_players[i], team);
    }
}

// PlayerList::updateTeamStates (PlayerList.cpp).
void World::playerListUpdateTeamStates()
{
    for (int i = 0; i < MAX_PLAYER_COUNT; i++) {
        playerList.m_players[i]->updateTeamStates(this);
    }
}

// PlayerList::validateTeam (PlayerList.cpp).
TeamT *World::playerListValidateTeam(const std::string &owner)
{
    // owner could be a player or team. first, check team names.
    TeamT *t = teamFactory.findTeam(owner, this);
    if (!t) {
        // DEBUG_CRASH(("no team or player named %s could be found!"))
        t = teamByID(this, playerList.getNeutralPlayer()->m_defaultTeam);
    }
    return t;
}

// PlayerList::getPlayerFromMask (PlayerList.cpp): the exact-equality scan.
PlayerT *World::playerListGetPlayerFromMask(PlayerMaskT mask)
{
    PlayerT *player = nullptr;
    for (int i = 0; i < MAX_PLAYER_COUNT; i++) {
        player = playerList.getNthPlayer(i);
        if (player && (PlayerMaskT)(1u << player->m_playerIndex) == mask)
            return player;
    }
    // DEBUG_CRASH(("Player does not exist for mask")) — the release build
    // returns the null
    return nullptr; // mask not found
}

// PlayerList::getEachPlayerFromMask (PlayerList.cpp): the peeling walk.
PlayerT *World::playerListGetEachPlayerFromMask(PlayerMaskT &maskToAdjust)
{
    PlayerT *player = nullptr;
    for (int i = 0; i < MAX_PLAYER_COUNT; i++) {
        player = playerList.getNthPlayer(i);
        if (player && (maskToAdjust & (PlayerMaskT)(1u << player->m_playerIndex)) != 0) {
            maskToAdjust = (PlayerMaskT)(maskToAdjust & ~(PlayerMaskT)(1u << player->m_playerIndex));
            return player;
        }
    }
    // DEBUG_CRASH(("No players found that contain any matching masks."))
    maskToAdjust = 0;
    return nullptr; // mask not found
}

// PlayerList::getPlayersWithRelationship (PlayerList.cpp): the
// relationship-to-mask query.
PlayerMaskT World::playerListGetPlayersWithRelationship(int srcPlayerIndex, unsigned allowedRelationships)
{
    PlayerMaskT retVal = PLAYERMASK_NONE_T;

    if (allowedRelationships == 0)
        return retVal;

    PlayerT *srcPlayer = playerList.getNthPlayer(srcPlayerIndex);
    if (!srcPlayer)
        return retVal;

    if ((allowedRelationships & ALLOW_SAME_PLAYER_T) != 0)
        retVal |= (PlayerMaskT)(1u << srcPlayer->m_playerIndex);

    for (int i = 0; i < playerList.m_playerCount; ++i) {
        PlayerT *player = playerList.getNthPlayer(i);
        if (!player)
            continue;

        if (player == srcPlayer)
            continue;

        switch (playerGetRelationship(srcPlayer, teamByID(this, player->m_defaultTeam), this)) {
        case R_ENEMIES:
            if ((allowedRelationships & ALLOW_ENEMIES_T) != 0)
                retVal |= (PlayerMaskT)(1u << player->m_playerIndex);
            break;
        case R_ALLIES:
            if ((allowedRelationships & ALLOW_ALLIES_T) != 0)
                retVal |= (PlayerMaskT)(1u << player->m_playerIndex);
            break;
        case R_NEUTRAL:
            if ((allowedRelationships & ALLOW_NEUTRAL_T) != 0)
                retVal |= (PlayerMaskT)(1u << player->m_playerIndex);
            break;
        }
    }

    return retVal;
}

// PlayerList::newGame (PlayerList.cpp), the distilled form.
void World::playerListNewGame(const SidesSpecT &sides)
{
    playerListReset();

    // ok, now create the rest of players we need.
    bool setLocal = false;
    for (size_t i = 0; i < sides.sides.size(); i++) {
        const SideSpecT &sd = sides.sides[i];
        if (sd.playerName.empty())
            continue; // it's neutral, which we've already done, so skip it.

        PlayerT *p = playerList.m_players[playerList.m_playerCount++];
        // initFromDict, distilled: init(pt) with a template, the name, the
        // type (the skirmish script duplication is the script-engine
        // slice's, stated)
        p->init(true, this);
        p->m_playerName = sd.playerName;
        p->m_playerNameKey = nameKeys.nameToKey(sd.playerName);
        p->m_observer = sd.observer;
        p->m_isPlayerDead = sd.observer; // observers are dead
        if (sd.isHuman)
            p->setPlayerType(PLAYER_HUMAN_T, false);
        else
            p->setPlayerType(PLAYER_COMPUTER_T, false);

        // Multiplayer override
        if (sd.multiplayerIsLocal) {
            playerListSetLocalPlayer(p);
            setLocal = true;
        }

        if (!setLocal && !m_networkPresent && sd.isHuman) {
            playerListSetLocalPlayer(p);
            setLocal = true;
        }

        // (Set the build list — the economy slice's, stated)
    }

    if (!setLocal) {
        for (size_t i = 0; i < sides.sides.size(); i++) {
            PlayerT *p = playerList.getNthPlayer((int)i);
            if (p != playerList.getNeutralPlayer()) {
                p->setPlayerType(PLAYER_HUMAN_T, false);
                playerListSetLocalPlayer(p);
                setLocal = true;
                break;
            }
        }
    }

    // must reset teams *after* creating players.
    teamFactory.initFromSides(sides, this);

    for (size_t i = 0; i < sides.sides.size(); i++) {
        const SideSpecT &sd = sides.sides[i];
        PlayerT *p = playerList.findPlayerWithNameKey(nameKeys.nameToKey(sd.playerName), this);

        for (const std::string &tok : sd.enemies) {
            PlayerT *p2 = playerList.findPlayerWithNameKey(nameKeys.nameToKey(tok), this);
            if (p2) {
                playerSetPlayerRelationship(p, p2, R_ENEMIES);
            } else {
                // DEBUG_LOG(("unknown enemy %s")) — stated
            }
        }

        for (const std::string &tok : sd.allies) {
            PlayerT *p2 = playerList.findPlayerWithNameKey(nameKeys.nameToKey(tok), this);
            if (p2) {
                playerSetPlayerRelationship(p, p2, R_ALLIES);
            } else {
                // DEBUG_LOG(("unknown ally %s")) — stated
            }
        }

        // finally, make sure self & neutral are correct.
        playerSetPlayerRelationship(p, p, R_ALLIES);
        if (p != playerList.getNeutralPlayer())
            playerSetPlayerRelationship(p, playerList.getNeutralPlayer(), R_NEUTRAL);

        p->setDefaultTeam(this);
    }

    if (sides.hasGameInfo) {
        playerListAssignSlotIndices(sides.gameInfo);
    }
}

// PlayerList::setSlotIndex / getSlotIndex / getPlayerIndexFromSlotIndex /
// getPlayerFromSlotIndex (PlayerList.cpp).
void World::playerListSetSlotIndex(int playerIndex, int slotIndex)
{
    if (playerIndex >= 0 && playerIndex < MAX_PLAYER_COUNT) {
        const int oldSlotIndex = playerList.m_slotIndices[playerIndex];
        if (oldSlotIndex >= 0 && oldSlotIndex < MAX_SLOTS_T) {
            playerList.m_slotToPlayerIndices[oldSlotIndex] = -1;
        }
        playerList.m_slotIndices[playerIndex] = slotIndex;
        if (slotIndex >= 0 && slotIndex < MAX_SLOTS_T) {
            playerList.m_slotToPlayerIndices[slotIndex] = playerIndex;
        }
    }
}

int World::playerListGetSlotIndex(int playerIndex) const
{
    if (playerIndex >= 0 && playerIndex < MAX_PLAYER_COUNT) {
        return playerList.m_slotIndices[playerIndex];
    }
    return -1;
}

int World::playerListGetPlayerIndexFromSlotIndex(int slotIndex) const
{
    if (slotIndex >= 0 && slotIndex < MAX_SLOTS_T) {
        return playerList.m_slotToPlayerIndices[slotIndex];
    }
    return -1;
}

PlayerT *World::playerListGetPlayerFromSlotIndex(int slotIndex) const
{
    const int playerIndex = playerListGetPlayerIndexFromSlotIndex(slotIndex);
    if (playerIndex >= 0 && playerIndex < playerList.m_playerCount) {
        return playerList.m_players[playerIndex];
    }
    return nullptr;
}

// PlayerList::assignSlotIndices (PlayerList.cpp): the "player%d" walk over
// the occupied slots.
void World::playerListAssignSlotIndices(const GameInfoSpecT &gameInfo)
{
    for (int slotIdx : gameInfo.occupiedSlots) {
        if (slotIdx < 0 || slotIdx >= MAX_SLOTS_T)
            continue; // (the const-slot bounds the walk guards)
        char playerName[32];
        snprintf(playerName, sizeof(playerName), "player%d", slotIdx);
        if (PlayerT *player = playerList.findPlayerWithNameKey(nameKeys.nameToKey(playerName), this)) {
            playerListSetSlotIndex(player->m_playerIndex, slotIdx);
        }
    }
}

// ---- Team's own surface (Team.cpp) -------------------------------------------

// Team::setActive (Team.h inline).
void TeamT::setActive()
{
    if (!m_active) {
        m_created = true;
        m_active = true;
    }
}

// Team::setOverrideTeamRelationship / removeOverrideTeamRelationship /
// setOverridePlayerRelationship / removeOverridePlayerRelationship.
void TeamT::setOverrideTeamRelationship(TeamIDT teamID, RelationshipT r)
{
    if (teamID != TEAM_ID_T_INVALID) {
        // note that this creates the entry if it doesn't exist.
        setTeamRelationEntry(teamID, r);
    }
}
bool TeamT::removeOverrideTeamRelationship(TeamIDT teamID)
{
    if (!m_teamRelations.empty()) {
        if (teamID == TEAM_ID_T_INVALID) {
            m_teamRelations.clear();
            return true;
        }
        return eraseTeamRelation(teamID);
    }
    return false;
}
void TeamT::setOverridePlayerRelationship(int playerIndex, RelationshipT r)
{
    if (playerIndex != PLAYER_INDEX_INVALID) {
        // note that this creates the entry if it doesn't exist.
        setPlayerRelationEntry(playerIndex, r);
    }
}
bool TeamT::removeOverridePlayerRelationship(int playerIndex)
{
    if (!m_playerRelations.empty()) {
        if (playerIndex == PLAYER_INDEX_INVALID) {
            m_playerRelations.clear();
            return true;
        }
        return erasePlayerRelation(playerIndex);
    }
    return false;
}

// Team::setControllingPlayer (Team.cpp): the prototype's owner move (the
// member onCapture walk — xezon 07/12/2025 — is folded into the per-object
// setTeam path the corpora drive; this form moves the prototype's owner).
void TeamT::setControllingPlayer(PlayerT *newController, World *w)
{
    // nullptr is not allowed, but is caught by TeamPrototype::setControllingPlayer()
    if (!newController)
        return;
    PlayerT *oldOwner = m_proto->m_owningPlayer;

    if (m_proto->m_owningPlayer)
        m_proto->m_owningPlayer->removeTeamFromList(m_proto);
    m_proto->m_owningPlayer = newController;
    // impossible to get here with a nullptr pointer.
    m_proto->m_owningPlayer->addTeamToList(m_proto);

    // The units' onCapture(oldOwner, newController) — the RETAIL_COMPATIBLE
    // branch's handle_partitionCellMaintenance is dropped (rule 1); the
    // non-retail walk notifies every member. The module bodies are the
    // module framework's; the hook pins the calls.
    for (auto it = m_members.rbegin(); it != m_members.rend(); ++it) {
        ObjectT *obj = *it;
        if (!obj)
            continue;
        w->hooks.tev(GameLogicHooks::TEV_CAPTURE, obj->m_id,
                     oldOwner ? oldOwner->m_playerIndex : -1, newController->m_playerIndex);
    }
}

// Team::notifyTeamOfObjectDeath (Team.cpp): the unit-destroyed script gate.
void TeamT::notifyTeamOfObjectDeath(World *w)
{
    const TeamSpecT *pInfo = m_proto->m_template;
    if (!pInfo) {
        return;
    }
    if (pInfo->scriptOnUnitDestroyed.empty()) {
        return;
    }
    // TheScriptEngine->runScript(pInfo->m_scriptOnUnitDestroyed, this)
    w->hooks.tev(GameLogicHooks::TEV_TEAM_SCRIPT, (int)m_id, GameLogicHooks::SCRIPT_ON_UNIT_DESTROYED);
}

// TeamPrototype::updateState (Team.cpp): the instance walk, then the
// empty-team removal sweep (the runtime team deletion — deleteTeam's
// comment: "the team itself will be deleted in updateState").
void protoUpdateState(TeamProtoT *tp, World *w)
{
    for (auto it = tp->m_instances.rbegin(); it != tp->m_instances.rend(); ++it) {
        teamUpdateState(*it, w);
    }
    /* remove empty teams. */
    bool done = false;
    while (!done) {
        done = true;
        for (auto it = tp->m_instances.rbegin(); it != tp->m_instances.rend(); ++it) {
            TeamT *t = *it;
            if (t->getFirstItemIn_TeamMemberList() == nullptr) {
                // Team has no members.
                if (tp->getIsSingleton()) {
                    continue; // Don't delete singleton teams, even if they are empty.
                }
                if (t->getPrototype()->m_owningPlayer && t->getPrototype()->m_owningPlayer->m_defaultTeam == t->getID()) {
                    // This is the player's default team, so don't remove it.
                    continue;
                }
                // don't delete inactive teams - they are under construction
                if (t->isActive() == false) {
                    continue;
                }
                // So remove it
                w->teamFactory.teamAboutToBeDeleted(t, w);
                teamDelete(t, w);
                done = false;
                break; // Not sure what state the iterator is in after deleting a member of the list. jba
            }
        }
    }
}

bool TeamT::isActive() const { return m_active; }

// Team::updateState (Team.cpp) — the state half: the enteredOrExited reset,
// the active gate, the created latch, the destroy-threshold compute, the
// enemy-sighted partition query, the onDestroyed trigger with its -1 latch,
// and the onIdle wasIdle latch. The script RUNS are hooks.
void teamUpdateState(TeamT *t, World *w)
{
    t->m_enteredOrExited = false;
    if (!t->m_active) {
        return;
    }
    const TeamSpecT *pInfo = t->m_proto->m_template;
    if (t->m_created) {
        t->m_created = false;
        // Run the on create script, if any.
        if (!pInfo->scriptOnCreate.empty()) {
            w->hooks.tev(GameLogicHooks::TEV_TEAM_SCRIPT, (int)t->getID(), GameLogicHooks::SCRIPT_ON_CREATE);
        }

        // Set up info for the onDestroyed script, if needed.
        if (!pInfo->scriptOnDestroyed.empty()) {
            for (auto it = t->m_members.rbegin(); it != t->m_members.rend(); ++it) {
                t->m_curUnits++;
            }
            // Int = Int - (Int * Real): the promoted float product, the
            // truncating assignment back.
            t->m_destroyThreshold = (int)((float)t->m_curUnits - ((float)t->m_curUnits * pInfo->destroyedThreshold));
            if (t->m_destroyThreshold > t->m_curUnits - 1)
                t->m_destroyThreshold = t->m_curUnits - 1;
            if (t->m_destroyThreshold < 0)
                t->m_destroyThreshold = 0;
        }
    }

    // Do enemy sighted/on clear checks.
    if (t->m_checkEnemySighted) {
        t->m_prevSeeEnemy = t->m_seeEnemy;
        t->m_seeEnemy = false;
        bool anyAliveInTeam = false; // If we're all dead, don't do all clear.
        // only consider enemies.
        for (auto it = t->m_members.rbegin(); it != t->m_members.rend(); ++it) {
            ObjectT *member = *it;
            if (member->isEffectivelyDead())
                continue;

            PartitionFilterRelationshipT filterTeam(member);
            // and only stuff that is not dead
            PartitionFilterAliveT filterAlive;
            PartitionFilterSameMapStatusT filterMapStatus(member);

            PartitionFilterT *filters[] = { &filterTeam, &filterAlive, &filterMapStatus, nullptr };
            float visionRange = member->m_visionRange;
            anyAliveInTeam = true;
            ObjectT *pObj = w->pm.getClosestObject(member, visionRange, FROM_CENTER_2D, filters, nullptr, nullptr);
            if (pObj) {
                t->m_seeEnemy = true;
                break;
            }
        }
        if (anyAliveInTeam) {
            if (t->m_prevSeeEnemy != t->m_seeEnemy) {
                if (t->m_seeEnemy) {
                    // fire onEnemySighted
                    w->hooks.tev(GameLogicHooks::TEV_TEAM_SCRIPT, (int)t->getID(), GameLogicHooks::SCRIPT_ON_ENEMY_SIGHTED);
                } else {
                    // fire on all clear.
                    w->hooks.tev(GameLogicHooks::TEV_TEAM_SCRIPT, (int)t->getID(), GameLogicHooks::SCRIPT_ON_ALL_CLEAR);
                }
            }
        }
    }

    // Do onDestroyed checks.
    if (!pInfo->scriptOnDestroyed.empty()) {
        int prevUnits = t->m_curUnits;
        t->m_curUnits = 0;
        for (auto it = t->m_members.rbegin(); it != t->m_members.rend(); ++it) {
            if ((*it)->isEffectivelyDead())
                continue;
            t->m_curUnits++;
        }
        if (t->m_curUnits != prevUnits && t->m_curUnits <= t->m_destroyThreshold) {
            w->hooks.tev(GameLogicHooks::TEV_TEAM_SCRIPT, (int)t->getID(), GameLogicHooks::SCRIPT_ON_DESTROYED);
            t->m_destroyThreshold = -1; // don't trigger again.
        }
    }

    // Do onIdle checks.
    if (!pInfo->scriptOnIdle.empty()) {
        bool isIdle = true;
        bool anyAliveInTeam = false; // If we're all dead, don't do all clear.
        for (auto it = t->m_members.rbegin(); it != t->m_members.rend(); ++it) {
            if ((*it)->isEffectivelyDead()) {
                continue;
            }
            if (!(*it)->m_hasAi)
                continue;
            anyAliveInTeam = true;
            if (!(*it)->m_aiIdle) {
                isIdle = false;
            }
        }
        if (anyAliveInTeam && isIdle && t->m_wasIdle) {
            w->hooks.tev(GameLogicHooks::TEV_TEAM_SCRIPT, (int)t->getID(), GameLogicHooks::SCRIPT_ON_IDLE);
        }
        t->m_wasIdle = isIdle;
    }
}

// ---- the players/teams partition filters' allow bodies (PartitionManager.cpp) --

// PartitionFilterRelationship::allow (PartitionManager.cpp).
bool PartitionFilterRelationshipT::allow(ObjectT *objOther)
{
    RelationshipT r = objectGetRelationship(m_obj, objOther, g_queryWorld);

    if ((m_flags & (1 << r)) != 0)
        return true;

    return false;
}

// PartitionFilterSamePlayer::allow (PartitionManager.cpp).
bool PartitionFilterSamePlayerT::allow(ObjectT *objOther)
{
    if (m_player == objectControllingPlayer(objOther, g_queryWorld))
        return true;

    return false;
}

// PartitionFilterAcceptOnTeam::allow (PartitionManager.cpp).
bool PartitionFilterAcceptOnTeamT::allow(ObjectT *objOther)
{
    // objOther is guaranteed to be non-null, so we don't need to check (srj)
    return (objOther->m_team == (m_team ? m_team->getID() : TEAM_ID_T_INVALID));
}

struct Dumper {
    FILE *out;

    void line(const char *fmt, ...)
    {
        va_list ap;
        va_start(ap, fmt);
        vfprintf(out, fmt, ap);
        va_end(ap);
        fputc('\n', out);
    }
};

static void dumpEvents(Dumper &d, World &w)
{
    d.line("events %d", (int)w.hooks.m_events.size());
    for (auto &e : w.hooks.m_events) {
        if (e[1] < 0)
            d.line("ev %s", evName(e[0]));
        else if (e[2] < 0)
            d.line("ev %s %d", evName(e[0]), e[1]);
        else
            d.line("ev %s %d %d", evName(e[0]), e[1], e[2]);
    }
    w.hooks.m_events.clear();
}

static void dumpHeap(Dumper &d, World &w)
{
    d.line("heap %d", (int)w.logic.m_sleepyUpdates.size());
    for (UpdateModuleT *u : w.logic.m_sleepyUpdates) {
        d.line("u o%d m%d pri=%08x idx=%d", w.logic.moduleOwner(u), w.logic.moduleIndexOf(u),
               friend_getPriority(u), u->m_indexInLogic);
    }
}

static void dumpRegistry(Dumper &d, World &w)
{
    d.line("frame %u", w.logic.getFrame());
    d.line("nextid %d", w.logic.m_nextObjID);
    d.line("objlist %d", (int)w.logic.m_objList.size());
    for (ObjectT *o : w.logic.m_objList)
        d.line("o id=%d upd=%d part=%d dis=%04x cellchg=%d", o->m_id, (int)o->m_updates.size(),
               o->m_partitionData, o->m_disabledMask.bits, o->m_cellChangeCount);
    d.line("destroylist %d", (int)w.logic.m_objectsToDestroy.size());
    for (ObjectT *o : w.logic.m_objectsToDestroy)
        d.line("d id=%d", o->m_id);
}

static void dumpPartition(Dumper &d, World &w)
{
    PartitionManagerT &pm = w.pm;
    d.line("grid size=%08x inv=%08x %dx%d total=%d lo=%08x,%08x hi=%08x,%08x updated=%d",
           f32bits(pm.m_cellSize), f32bits(pm.m_cellSizeInv), pm.m_cellCountX, pm.m_cellCountY,
           pm.m_totalCellCount, f32bits(pm.m_worldExtents.loX), f32bits(pm.m_worldExtents.loY),
           f32bits(pm.m_worldExtents.hiX), f32bits(pm.m_worldExtents.hiY), pm.m_updatedSinceLastReset ? 1 : 0);

    // the master list order (the reverse-registration order the queries walk)
    d.line("modules");
    for (PartitionData *m = pm.m_moduleList; m; m = m->m_next)
        d.line("m o%d coi=%d/%d dirty=%d last=%d shr=%d,%d,%d seen=%d,%d",
               m->m_object, m->m_coiInUseCount, m->m_coiArrayCount, (int)m->m_dirtyStatus, m->m_lastCell,
               m->m_shroudedness[0], m->m_shroudedness[1], m->m_shroudedness[15],
               m->m_everSeenByPlayer[0] ? 1 : 0, m->m_everSeenByPlayer[1] ? 1 : 0);

    // the dirty list order
    d.line("dirtylist");
    for (PartitionData *m = pm.m_dirtyModules; m; m = m->m_nextDirty)
        d.line("dl o%d status=%d", m->m_object, (int)m->m_dirtyStatus);

    // the per-object COI coverage, in slot order
    for (PartitionData *m = pm.m_moduleList; m; m = m->m_next) {
        d.line("coi o%d", m->m_object);
        for (int i = 0; i < m->m_coiInUseCount; ++i) {
            int ci = m->m_coiArray[(size_t)i].m_cell;
            PartitionCell *cell = pm.cellByIndex(ci);
            d.line("c %d,%d", cell ? (int)cell->m_cellX : -1, cell ? (int)cell->m_cellY : -1);
        }
    }

    // the per-cell COI lists, in list order (newest first — the prepend order)
    d.line("cells %d", (int)pm.m_cells.size());
    for (size_t i = 0; i < pm.m_cells.size(); ++i) {
        PartitionCell &cell = pm.m_cells[i];
        if (cell.m_coiCount == 0)
            continue;
        d.line("cell %d,%d n=%d shr=%d,%d,%d", cell.m_cellX, cell.m_cellY, cell.m_coiCount,
               cell.m_shroudLevel[0].m_currentShroud, cell.m_shroudLevel[1].m_currentShroud,
               cell.m_shroudLevel[0].m_activeShroudLevel);
        for (CellAndObjectIntersection *coi = cell.m_firstCoiInCell; coi; coi = coi->m_nextCoi)
            d.line("k o%d", coi->m_module);
    }
}

// The contact pairs a partition update collected (in contact-list order).
static void dumpContacts(Dumper &d, World &w)
{
    d.line("contacts %d", (int)w.pm.m_lastContacts.size());
    for (auto &p : w.pm.m_lastContacts)
        d.line("p o%d o%d", p.first, p.second);
}

// The collide-module dispatches the last partition update produced (in
// dispatch order, with the loc and normal the collide test computed).
static void dumpCollideEvents(Dumper &d, World &w)
{
    d.line("collideevents %d", (int)w.hooks.m_collideEvents.size());
    for (auto &e : w.hooks.m_collideEvents) {
        d.line("cv o%d o%d m%d loc=%08x,%08x,%08x nrm=%08x,%08x,%08x",
               e.m_obj, e.m_other, e.m_module, f32bits(e.m_loc.x), f32bits(e.m_loc.y), f32bits(e.m_loc.z),
               f32bits(e.m_normal.x), f32bits(e.m_normal.y), f32bits(e.m_normal.z));
    }
    w.hooks.m_collideEvents.clear();
}

// The players/teams event stream (drained at every dump point, like the
// collide stream).
static const char *tevName(int e)
{
    switch (e) {
    case GameLogicHooks::TEV_PLAYER_UPDATE: return "pupdate";
    case GameLogicHooks::TEV_LOCAL: return "local";
    case GameLogicHooks::TEV_TEAM_MEMBER: return "teammember";
    case GameLogicHooks::TEV_CAPTURE: return "capture";
    case GameLogicHooks::TEV_TEAM_SCRIPT: return "script";
    case GameLogicHooks::TEV_TEAM_DESTROYED: return "teamdestroyed";
    default: return "?";
    }
}
static const char *scriptKindName(int k)
{
    switch (k) {
    case GameLogicHooks::SCRIPT_ON_CREATE: return "oncreate";
    case GameLogicHooks::SCRIPT_ON_DESTROYED: return "ondestroyed";
    case GameLogicHooks::SCRIPT_ON_IDLE: return "onidle";
    case GameLogicHooks::SCRIPT_ON_ENEMY_SIGHTED: return "onenemysighted";
    case GameLogicHooks::SCRIPT_ON_ALL_CLEAR: return "onallclear";
    case GameLogicHooks::SCRIPT_ON_UNIT_DESTROYED: return "onunitdestroyed";
    case GameLogicHooks::SCRIPT_PRODUCTION_CONDITION: return "production";
    default: return "?";
    }
}
static void dumpTeamEvents(Dumper &d, World &w)
{
    d.line("teamevents %d", (int)w.hooks.m_teamEvents.size());
    for (auto &e : w.hooks.m_teamEvents) {
        // uniform: the four ints (the script events' 3rd field names the
        // kind; the production event's 4th is the prototype id)
        if (e[0] == GameLogicHooks::TEV_TEAM_SCRIPT)
            d.line("tv script %d %s %d", e[1], scriptKindName(e[2]), e[3]);
        else
            d.line("tv %s %d %d %d", tevName(e[0]), e[1], e[2], e[3]);
    }
    w.hooks.m_teamEvents.clear();
}

// The player list state: the count, the local player, every slot (all 16 —
// the uncreated slots' birth state is pinned), the relation maps in key
// order (the hash_map's bucket order is never observable — probed only),
// the default team, the team-prototype list and the slot bookkeeping.
static void dumpPlayers(Dumper &d, World &w)
{
    d.line("players count=%d local=%d", w.playerList.m_playerCount,
           w.playerList.m_local ? w.playerList.m_local->m_playerIndex : -1);
    for (int i = 0; i < MAX_PLAYER_COUNT; i++) {
        PlayerT *p = w.playerList.m_players[i];
        d.line("pl %d name=%s key=%u type=%d dead=%d obs=%d act=%d dflt=%u slot=%d npr=%d",
               i, p->m_playerName.c_str(), p->m_playerNameKey, (int)p->m_playerType,
               p->m_isPlayerDead ? 1 : 0, p->m_observer ? 1 : 0, p->isPlayerActive() ? 1 : 0,
               p->m_defaultTeam, w.playerListGetSlotIndex(i), (int)p->m_playerTeamPrototypes.size());
        for (const auto &kv : p->m_playerRelations)
            d.line("pr %d->%d %s", i, kv.first, relationshipName(kv.second));
        for (const auto &kv : p->m_teamRelations)
            d.line("tr %d->t%u %s", i, kv.first, relationshipName(kv.second));
        for (size_t j = 0; j < p->m_playerTeamPrototypes.size(); ++j)
            d.line("pt %d %d tp%u", i, (int)j, p->m_playerTeamPrototypes[j]->m_id);
    }
    // the slot bookkeeping
    for (int s = 0; s < MAX_SLOTS_T; s++)
        d.line("slot %d p%d", s, w.playerList.m_slotToPlayerIndices[s]);
}

// The team factory state: the unique-id counters, the prototype map in KEY
// order (the std::map walk every factory iteration uses), each prototype's
// template flags, and each live team's state + members (newest first).
static void dumpTeams(Dumper &d, World &w)
{
    d.line("teams nextproto=%u nextid=%u protos=%zu",
           w.teamFactory.m_uniqueTeamPrototypeID, w.teamFactory.m_uniqueTeamID,
           w.teamFactory.m_prototypes.size());
    for (const auto &kv : w.teamFactory.prototypesInMapOrder()) {
        TeamProtoT *tp = kv.second;
        const TeamSpecT *info = tp->m_template;
        d.line("tp %u name=%s key=%u owner=%d single=%d inst=%zu",
               tp->m_id, tp->m_name.c_str(), kv.first,
               tp->m_owningPlayer ? tp->m_owningPlayer->m_playerIndex : -1,
               tp->getIsSingleton() ? 1 : 0, tp->m_instances.size());
        if (info) {
            d.line("tpi %u oc=%d od=%d oi=%d oes=%d oac=%d oud=%d thr=%08x ea=%d pc=%s pe=%d",
                   tp->m_id, !info->scriptOnCreate.empty(), !info->scriptOnDestroyed.empty(),
                   !info->scriptOnIdle.empty(), !info->scriptOnEnemySighted.empty(),
                   !info->scriptOnAllClear.empty(), !info->scriptOnUnitDestroyed.empty(),
                   f32bits(info->destroyedThreshold), info->executeActions ? 1 : 0,
                   info->productionCondition.c_str(), info->productionScriptExists ? 1 : 0);
        }
        // the instance list order (newest first)
        for (auto it = tp->m_instances.rbegin(); it != tp->m_instances.rend(); ++it) {
            TeamT *t = *it;
            d.line("t %u tp%u act=%d cre=%d eo=%d see=%d psee=%d chk=%d cur=%d thr=%d idle=%d mem=%zu",
                   t->getID(), t->m_proto->m_id, t->m_active ? 1 : 0, t->m_created ? 1 : 0,
                   t->m_enteredOrExited ? 1 : 0, t->m_seeEnemy ? 1 : 0, t->m_prevSeeEnemy ? 1 : 0,
                   t->m_checkEnemySighted ? 1 : 0, t->m_curUnits, t->m_destroyThreshold,
                   t->m_wasIdle ? 1 : 0, t->m_members.size());
            for (const auto &kv2 : t->m_teamRelations)
                d.line("tot t%u->t%u %s", t->getID(), kv2.first, relationshipName(kv2.second));
            for (const auto &kv2 : t->m_playerRelations)
                d.line("top t%u->%d %s", t->getID(), kv2.first, relationshipName(kv2.second));
            for (auto mit = t->m_members.rbegin(); mit != t->m_members.rend(); ++mit)
                d.line("mem t%u o%d", t->getID(), (*mit)->m_id);
        }
    }
}

// The object→team attachment state (the arena order).
static void dumpObjectTeams(Dumper &d, World &w)
{
    d.line("objteams %d", (int)w.objects.size());
    for (auto &o : w.objects)
        d.line("ot o%d t%u ctrl=%d vis=%08x ai=%d/%d",
               o->m_id, o->m_team,
               objectControllingPlayer(o.get(), &w) ? objectControllingPlayer(o.get(), &w)->m_playerIndex : -1,
               f32bits(o->m_visionRange), o->m_hasAi ? 1 : 0, o->m_aiIdle ? 1 : 0);
}

// ============================================================================
// The corpora.
// ============================================================================
static World &freshWorld(float cellSize, const Region2DT &extents)
{
    World *w = new World();
    // GameLogic::init's reset (m_nextObjID = 1 — the ctor's INVALID_ID
    // start would hand out id 0, which no real object ever carries)
    w->logic.setObjectIDCounter(1);
    w->pm.init(cellSize, extents);
    // the cost-sorted iterators' object-table stand-in (each corpus owns
    // exactly one World at a time)
    g_queryWorld = w;
    return *w;
}

// --- kernel_ids: the ID counter scheme and the loadPostProcess compaction --
static void corpusIds(Dumper &d)
{
    d.line("corpus kernel_ids");
    // the frame constants (the WP1 "30 Hz, to verify" item): WWSyncPerSecond
    // and the GameCommon.h derived reals
    d.line("lfps %d msync %d", LOGICFRAMES_PER_SECOND, WWSyncMilliseconds);
    d.line("reals %08x %08x %08x %08x", f32bits(LOGICFRAMES_PER_MSEC_REAL), f32bits(MSEC_PER_LOGICFRAME_REAL),
           f32bits(LOGICFRAMES_PER_SECONDS_REAL), f32bits(SECONDS_PER_LOGICFRAME_REAL));
    World &w = freshWorld(10.0f, Region2DT{0, 0, 100, 100});
    d.line("alloc1 %d", w.logic.allocateObjectID());
    d.line("nextid %d", w.logic.m_nextObjID);
    // the init reset (GameLogic::init: m_nextObjID = 1)
    w.logic.setObjectIDCounter(1);
    GeometryInfoT g{GEOMETRY_SPHERE, true, 1, 1, 1};
    ObjectT *a = w.createObject(g, false, 0, {});
    ObjectT *b = w.createObject(g, false, 0, {});
    ObjectT *c = w.createObject(g, false, 0, {});
    d.line("ids %d %d %d", a->m_id, b->m_id, c->m_id);
    // destroy b (frame 0), then allocate — no reuse
    w.logic.destroyObject(b, &w.hooks);
    w.logic.processDestroyList(&w.pm, &w.hooks);
    d.line("afterdestroy nextid %d list %d", w.logic.m_nextObjID, (int)w.logic.m_objList.size());
    int e = w.logic.allocateObjectID();
    d.line("alloc4 %d", e);
    // the setObjectIDCounter mid-flight write (the save/load hook)
    w.logic.setObjectIDCounter(100);
    d.line("alloc5 %d", w.logic.allocateObjectID());
    // the compaction: max(id)+1 walked from INVALID_ID
    w.logic.setObjectIDCounter(500); // (a high climber, as the load would leave it)
    w.loadPostProcess();
    d.line("compacted %d", w.logic.m_nextObjID);
    // the wrap: the counter at 0x7fffffff wraps into the negative Int range
    w.logic.setObjectIDCounter(0x7fffffff);
    d.line("wrapalloc %d", w.logic.allocateObjectID());
    d.line("wrapnext %d", w.logic.m_nextObjID);
    dumpEvents(d, w);
    d.line("end");
}

// --- kernel_frame: the update() ordering, the freeze gate, the frame
// increment, preUpdate's pause-frame, startNewGame ---------------------------
static void corpusFrame(Dumper &d)
{
    d.line("corpus kernel_frame");
    World &w = freshWorld(10.0f, Region2DT{0, 0, 100, 100});
    GeometryInfoT g{GEOMETRY_SPHERE, true, 1, 1, 1};
    w.createObject(g, false, 1, {});

    // a plain frame: the full ordered walk
    w.preUpdate();
    w.update();
    d.line("f1 frame %u hasupd %d", w.logic.getFrame(), w.logic.m_hasUpdated ? 1 : 0);
    dumpEvents(d, w);

    // the freeze gate: scripts froze the engine — nothing runs, no frame inc
    w.hooks.m_engineTimeFrozen = true;
    w.preUpdate();
    w.update();
    d.line("f2 frame %u", w.logic.getFrame());
    dumpEvents(d, w);
    w.hooks.m_engineTimeFrozen = false;

    // the startNewGame gate: the kick runs, the frame still advances (the
    // flag is cleared before the tail)
    w.logic.m_startNewGame = true;
    w.preUpdate();
    w.update();
    d.line("f3 frame %u", w.logic.getFrame());
    dumpEvents(d, w);

    // the pause-frame transition: set for the next frame, fires in preUpdate
    // once the frame counter reaches it
    w.logic.m_pauseFrame = w.logic.getFrame() + 1;
    w.preUpdate();
    w.update(); // frame reaches pauseFrame here
    w.preUpdate();
    d.line("paused %d pauseFrame %u", w.logic.m_gamePaused ? 1 : 0, w.logic.m_pauseFrame);
    d.line("end");
}

// --- sched_basic: registration wakes, the dispatch loop, the sleep returns --
static void corpusSchedBasic(Dumper &d)
{
    d.line("corpus sched_basic");
    World &w = freshWorld(10.0f, Region2DT{0, 0, 100, 100});
    GeometryInfoT g{GEOMETRY_SPHERE, true, 1, 1, 1};

    // object born at frame 0: the modules' initial wake clamps to 1
    ObjectT *a = w.createObject(g, false, 2, {});
    // object born with constructor wakes (the not-in-list path)
    ObjectT *b = w.createObject(g, false, 2, {3, 5}, Coord3D{0, 0, 0}, 0.0f);
    dumpHeap(d, w);

    // scripted sleeps: a returns 1 (run every frame), a's second 7;
    // b's first 3, b's second FOREVER (the saturation: 0 + FOREVER)
    a->m_updates[0].m_scriptedSleep = 1;
    a->m_updates[1].m_scriptedSleep = 7;
    b->m_updates[0].m_scriptedSleep = 3;
    b->m_updates[1].m_scriptedSleep = UPDATE_SLEEP_FOREVER;

    for (int f = 0; f < 6; ++f) {
        w.preUpdate();
        w.update();
        dumpEvents(d, w);
    }
    dumpHeap(d, w);
    dumpRegistry(d, w);
    d.line("end");
}

// --- sched_ties: equal packed priorities — the heap-array order is the
// tie-breaker; the requeue rebalance shuffles are pinned by the array dump --
static void corpusSchedTies(Dumper &d)
{
    d.line("corpus sched_ties");
    World &w = freshWorld(10.0f, Region2DT{0, 0, 100, 100});
    GeometryInfoT g{GEOMETRY_SPHERE, true, 1, 1, 1};

    // five modules, all waking at frame 1 (born at frame 0: the 0→1 clamp)
    ObjectT *a = w.createObject(g, false, 5, {});
    dumpHeap(d, w);
    for (int i = 0; i < 5; ++i)
        a->m_updates[(size_t)i].m_scriptedSleep = 2;

    // frame 1: all five run (packed priorities equal — the array order
    // decides), each requeued at 1+2=3
    w.update(); // 0 → 1 (nothing runs: everyone wakes at 1 > 0)
    w.update(); // 1 → 2: the five run here
    dumpEvents(d, w);
    dumpHeap(d, w);
    // frame 3: again
    w.update(); // 2 → 3 (nothing)
    w.update(); // 3 → 4: the five run again
    dumpEvents(d, w);
    dumpHeap(d, w);
    d.line("end");
}

// --- sched_phases: the phase bits order same-frame modules ------------------
static void corpusSchedPhases(Dumper &d)
{
    d.line("corpus sched_phases");
    World &w = freshWorld(10.0f, Region2DT{0, 0, 100, 100});
    GeometryInfoT g{GEOMETRY_SPHERE, true, 1, 1, 1};

    ObjectT *a = w.createObject(g, false, 4, {}, Coord3D{0, 0, 0}, 0.0f,
                                {PHASE_FINAL, PHASE_INITIAL, PHASE_NORMAL, PHASE_PHYSICS});
    for (int i = 0; i < 4; ++i)
        a->m_updates[(size_t)i].m_scriptedSleep = 5;

    w.update(); // frame 0 → 1
    w.update(); // 1 → 2
    // wake all four at frame 3 (now = 2: setWakeFrame(1) → 3)
    for (int i = 0; i < 4; ++i)
        w.setWakeFrame(a, i, 1);
    dumpHeap(d, w);
    w.update(); // 2 → 3: all four run, in phase order
    dumpEvents(d, w);
    dumpHeap(d, w);
    d.line("end");
}

// --- sched_awaken: setWakeFrame's refusal family and the saturation --------
static void corpusSchedAwaken(Dumper &d)
{
    d.line("corpus sched_awaken");
    World &w = freshWorld(10.0f, Region2DT{0, 0, 100, 100});
    GeometryInfoT g{GEOMETRY_SPHERE, true, 1, 1, 1};
    ObjectT *a = w.createObject(g, false, 3, {});
    for (int i = 0; i < 3; ++i)
        a->m_updates[(size_t)i].m_scriptedSleep = 10;

    w.update(); // frame 0 → 1; all wake at 1, run, sleep to 11
    dumpHeap(d, w);

    unsigned now = w.logic.getFrame();
    // the easy return: waking exactly at the current wake frame — no change
    w.setWakeFrame(a, 0, 10); // when = 11 == nextCallFrame
    d.line("m0frame %d", friend_getNextCallFrame(&a->m_updates[0]));
    // the subtle case: nextCallFrame == now && when == now + 1 → refusal
    w.logic.friend_awakenUpdateModule(a, &a->m_updates[1], now); // set to now
    d.line("m1frame %d", friend_getNextCallFrame(&a->m_updates[1]));
    w.logic.friend_awakenUpdateModule(a, &a->m_updates[1], now + 1); // refused
    d.line("m1frame %d", friend_getNextCallFrame(&a->m_updates[1]));
    // a wake further out: accepted, rebalanced
    w.logic.friend_awakenUpdateModule(a, &a->m_updates[2], now + 30);
    dumpHeap(d, w);
    // the saturation: waking past FOREVER clamps to FOREVER
    w.logic.friend_awakenUpdateModule(a, &a->m_updates[0], 0x7ffffff0u);
    d.line("m0pri %08x", (unsigned)friend_getPriority(&a->m_updates[0]));
    // frameToSleepTime: the min-of-four and the past-frame fallback
    d.line("fts %d", w.logic.frameToSleepTime(5, 9, 3, 7));
    d.line("fts %d", w.logic.frameToSleepTime(now, (unsigned)FOREVER));
    d.line("fts %d", w.logic.frameToSleepTime(now - 1, (unsigned)FOREVER)); // past → NONE
    d.line("end");
}

// --- sched_disabled: the exclusive gate, the 1-frame retry, the sweep -------
static void corpusSchedDisabled(Dumper &d)
{
    d.line("corpus sched_disabled");
    World &w = freshWorld(10.0f, Region2DT{0, 0, 100, 100});
    GeometryInfoT g{GEOMETRY_SPHERE, true, 1, 1, 1};
    ObjectT *a = w.createObject(g, false, 2, {1, 1}, Coord3D{50, 50, 0}, 0.0f);
    a->m_updates[0].m_scriptedSleep = 4;
    a->m_updates[1].m_scriptedSleep = 4;
    // module 1 processes EMP-disabled objects; module 0 processes nothing
    DisabledMask emp;
    emp.set(DISABLED_EMP, true);
    a->m_updates[1].m_disabledTypesToProcess = emp;

    // disable by EMP for 3 frames (from frame 0: mask set, till 3)
    w.setDisabledUntil(a, DISABLED_EMP, 3);
    d.line("dis %04x till %d", a->m_disabledMask.bits, a->m_disabledTillFrame[DISABLED_EMP]);

    // frame 1: module 0 gated out (sleep NONE → 1-frame retry), module 1 runs
    w.update();
    dumpEvents(d, w);
    dumpHeap(d, w);
    // frame 2: still disabled
    w.update();
    dumpEvents(d, w);
    // frame 3: the sweep clears (now >= 3), module 0 wakes at 4
    w.update();
    dumpEvents(d, w);
    d.line("dis %04x", a->m_disabledMask.bits);
    dumpHeap(d, w);
    // frame 4: module 0 runs again (the mask is clear)
    w.update();
    dumpEvents(d, w);
    // frame 5: module 1 (its requeue from frame 1) runs too
    w.update();
    dumpEvents(d, w);

    // setDisabledUntil at now: the mask stays clear (frame > now is false)
    w.setDisabledUntil(a, DISABLED_HELD, w.logic.getFrame());
    d.line("held %04x", a->m_disabledMask.bits);
    // an unknown type index: refused
    w.setDisabledUntil(a, 99, 99);
    d.line("bad %04x", a->m_disabledMask.bits);
    d.line("end");
}

// --- sched_destroy: the two-pass erase order and the reentrancy ------------
static void corpusSchedDestroy(Dumper &d)
{
    d.line("corpus sched_destroy");
    World &w = freshWorld(10.0f, Region2DT{0, 0, 100, 100});
    GeometryInfoT g{GEOMETRY_SPHERE, true, 1, 1, 1};

    // three objects, modules interleaved in the heap by wake priorities
    ObjectT *a = w.createObject(g, false, 3, {}, Coord3D{50, 50, 0}, 0.0f);
    ObjectT *b = w.createObject(g, false, 3, {}, Coord3D{60, 60, 0}, 0.0f);
    ObjectT *c = w.createObject(g, false, 3, {}, Coord3D{70, 70, 0}, 0.0f);
    for (ObjectT *o : {a, b, c})
        for (int i = 0; i < 3; ++i)
            o->m_updates[(size_t)i].m_scriptedSleep = 100;
    // stagger the wakes so the heap interleaves them
    w.logic.friend_awakenUpdateModule(a, &a->m_updates[0], 5);
    w.logic.friend_awakenUpdateModule(b, &b->m_updates[0], 4);
    w.logic.friend_awakenUpdateModule(c, &c->m_updates[0], 6);
    w.logic.friend_awakenUpdateModule(b, &b->m_updates[1], 3);
    w.logic.friend_awakenUpdateModule(a, &a->m_updates[1], 7);
    dumpHeap(d, w);

    // destroy b: the destroy list processes at the next update's tail; the
    // two-pass erase walks the heap array forward, erases backward
    w.logic.destroyObject(b, &w.hooks);
    w.update();
    dumpEvents(d, w);
    dumpHeap(d, w);
    dumpRegistry(d, w);
    dumpPartition(d, w);

    // destroy-during-destroy: a's module 0 requests c's destruction from
    // inside its update (the same-pass append)
    a->m_updates[0].m_onUpdateDestroyObj = c->m_id;
    w.logic.friend_awakenUpdateModule(a, &a->m_updates[0], w.logic.getFrame());
    w.update();
    dumpEvents(d, w);
    dumpRegistry(d, w);
    dumpHeap(d, w);
    d.line("end");
}

// --- sched_destroy_cap: the MAX_SUO boundary (exactly 256 modules) ----------
static void corpusSchedDestroyCap(Dumper &d)
{
    d.line("corpus sched_destroy_cap");
    World &w = freshWorld(10.0f, Region2DT{0, 0, 100, 100});
    GeometryInfoT g{GEOMETRY_SPHERE, true, 0, 0, 0};
    // one object with exactly 256 update modules — the cap boundary: all
    // erase. (Beyond 256 the original leaves dangling heap entries — the
    // stated contract; not replayed.)
    ObjectT *a = w.createObject(g, false, 256, {}, Coord3D{50, 50, 0}, 0.0f);
    for (int i = 0; i < 256; ++i)
        a->m_updates[(size_t)i].m_scriptedSleep = 100;
    d.line("heap %d", (int)w.logic.m_sleepyUpdates.size());
    w.logic.destroyObject(a, &w.hooks);
    w.update();
    d.line("heap %d", (int)w.logic.m_sleepyUpdates.size());
    dumpRegistry(d, w);
    d.line("end");
}

// --- sched_loadpost: the heap rebuild + heapify ------------------------------
static void corpusSchedLoadpost(Dumper &d)
{
    d.line("corpus sched_loadpost");
    World &w = freshWorld(10.0f, Region2DT{0, 0, 100, 100});
    GeometryInfoT g{GEOMETRY_SPHERE, true, 1, 1, 1};
    ObjectT *a = w.createObject(g, false, 3, {}, Coord3D{50, 50, 0}, 0.0f);
    ObjectT *b = w.createObject(g, false, 2, {}, Coord3D{60, 60, 0}, 0.0f);
    // run a few frames to shuffle the heap
    for (int i = 0; i < 3; ++i) {
        w.update();
        w.hooks.m_events.clear();
    }
    // set raw wake frames as a save would load them
    w.logic.friend_awakenUpdateModule(a, &a->m_updates[0], 40);
    w.logic.friend_awakenUpdateModule(a, &a->m_updates[1], 1); // (the legacy-zero clamp target)
    w.logic.friend_awakenUpdateModule(a, &a->m_updates[2], 12);
    w.logic.friend_awakenUpdateModule(b, &b->m_updates[0], 9);
    w.logic.friend_awakenUpdateModule(b, &b->m_updates[1], 2);
    dumpHeap(d, w);
    // the legacy zero frame (a save with when == 0) — the rebuild clamps to now(≥1)
    friend_setNextCallFrame(&a->m_updates[1], 0);
    friend_setNextCallFrame(&b->m_updates[1], 0);
    w.loadPostProcess();
    d.line("after loadpost nextid %d", w.logic.m_nextObjID);
    dumpHeap(d, w);
    d.line("end");
}

// --- part_grid: the grid build, the coordinate mapping, the cell defaults ----
static void corpusPartGrid(Dumper &d)
{
    d.line("corpus part_grid");
    {
        World &w = freshWorld(10.0f, Region2DT{0, 0, 100, 100});
        dumpPartition(d, w);
        // the coordinate mapping: in-grid, boundary, negative, off-high
        int cx, cy;
        w.pm.worldToCell(55.0f, 55.0f, &cx, &cy);
        d.line("wtc %d %d", cx, cy);
        w.pm.worldToCell(-0.001f, 0.0f, &cx, &cy);
        d.line("wtc %d %d", cx, cy);
        w.pm.worldToCell(99.999f, 100.0f, &cx, &cy);
        d.line("wtc %d %d", cx, cy);
        d.line("wtd %d %d %d", w.pm.worldToCellDist(0.0f), w.pm.worldToCellDist(10.0f), w.pm.worldToCellDist(10.001f));
        d.line("cellat %d %d %d %d", w.pm.getCellAt(0, 0) != nullptr, w.pm.getCellAt(9, 9) != nullptr,
               w.pm.getCellAt(10, 0) != nullptr, w.pm.getCellAt(-1, 0) != nullptr);
    }
    {
        // the cell-size clamp (< 1 → 1) and the zero-extent patch
        World &w = freshWorld(0.5f, Region2DT{0, 0, 0, 0});
        dumpPartition(d, w);
    }
    {
        // the double-division cellSizeInv: 1/3 in f64 then narrowed
        World &w = freshWorld(3.0f, Region2DT{2.0f, 5.0f, 32.0f, 35.0f});
        dumpPartition(d, w);
        int cx, cy;
        w.pm.worldToCell(2.0f, 5.0f, &cx, &cy);
        d.line("wtc %d %d", cx, cy);
        w.pm.worldToCell(31.9999f, 34.9999f, &cx, &cy);
        d.line("wtc %d %d", cx, cy);
    }
    {
        // shutdown clears everything
        World &w = freshWorld(10.0f, Region2DT{0, 0, 100, 100});
        w.pm.shutdown();
        dumpPartition(d, w);
    }
    d.line("end");
}

// --- part_maxcoi: the COI count bounds ----------------------------------------
static void corpusPartMaxCoi(Dumper &d)
{
    d.line("corpus part_maxcoi");
    World &w = freshWorld(10.0f, Region2DT{0, 0, 100, 100});
    // sphere/cylinder: (ceil(2r/cell)+1)^2, floored at 4
    d.line("sph %d %d %d %d", w.pm.calcMaxCoiForShape(GEOMETRY_SPHERE, 1.0f, 0.0f, true),
           w.pm.calcMaxCoiForShape(GEOMETRY_SPHERE, 5.0f, 0.0f, false),
           w.pm.calcMaxCoiForShape(GEOMETRY_CYLINDER, 12.5f, 0.0f, false),
           w.pm.calcMaxCoiForShape(GEOMETRY_SPHERE, 0.0f, 0.0f, true));
    // box: the diagonal
    d.line("box %d %d", w.pm.calcMaxCoiForShape(GEOMETRY_BOX, 30.0f, 40.0f, false),
           w.pm.calcMaxCoiForShape(GEOMETRY_BOX, 1.0f, 1.0f, true));
    // out-of-range geometry: the default 4
    d.line("bad %d", w.pm.calcMaxCoiForShape(99, 5.0f, 5.0f, false));
    d.line("end");
}

// --- part_small: the small fill (≤ 4 cells, the truncation, the clipping) ----
static void corpusPartSmall(Dumper &d)
{
    d.line("corpus part_small");
    World &w = freshWorld(10.0f, Region2DT{0, 0, 100, 100});
    GeometryInfoT g{GEOMETRY_SPHERE, true, 1, 2.5f, 2.5f};
    // a small object centered mid-cell
    w.createObject(g, false, 0, {}, Coord3D{51.0f, 52.0f, 0.0f}, 0.0f);
    w.pm.update(nullptr);
    dumpPartition(d, w);

    // straddling a corner: four cells
    w.createObject(GeometryInfoT{GEOMETRY_SPHERE, true, 1, 4.9f, 4.9f}, false, 0, {},
                             Coord3D{50.0f, 50.0f, 0.0f}, 0.0f);
    w.pm.update(nullptr);
    dumpPartition(d, w);

    // the truncation: a small radius beyond the half-cell clamps down
    w.createObject(GeometryInfoT{GEOMETRY_BOX, true, 1, 9.0f, 9.0f}, false, 0, {},
                             Coord3D{25.0f, 25.0f, 0.0f}, 0.0f);
    w.pm.update(nullptr);
    dumpPartition(d, w);

    // off-grid center: the cells clip at the boundary
    w.createObject(GeometryInfoT{GEOMETRY_SPHERE, true, 1, 3.0f, 3.0f}, false, 0, {},
                             Coord3D{1.0f, 1.0f, 0.0f}, 0.0f);
    w.pm.update(nullptr);
    dumpPartition(d, w);
    d.line("end");
}

// --- part_circle: the precise circle fill --------------------------------------
static void corpusPartCircle(Dumper &d)
{
    d.line("corpus part_circle");
    World &w = freshWorld(10.0f, Region2DT{0, 0, 100, 100});
    // radius 0: the corner cells coincide and the strict overlap fails —
    // no coverage at all
    w.createObject(GeometryInfoT{GEOMETRY_CYLINDER, false, 1, 0.0f, 0.0f}, false, 0, {},
                             Coord3D{55.0f, 55.0f, 0.0f}, 0.0f);
    w.pm.update(nullptr);
    dumpPartition(d, w);

    // an exact boundary touch: distance == radius → strict < fails
    w.createObject(GeometryInfoT{GEOMETRY_SPHERE, false, 1, 5.0f, 5.0f}, false, 0, {},
                             Coord3D{50.0f, 55.0f, 0.0f}, 0.0f);
    w.pm.update(nullptr);
    dumpPartition(d, w);

    // a mid radius spanning several cells, plus off-grid clipping
    w.createObject(GeometryInfoT{GEOMETRY_SPHERE, false, 1, 7.3f, 7.3f}, false, 0, {},
                             Coord3D{17.0f, 13.0f, 0.0f}, 0.0f);
    w.pm.update(nullptr);
    dumpPartition(d, w);

    // a big radius (>= 20: the Stubbjax bugfix range)
    w.createObject(GeometryInfoT{GEOMETRY_SPHERE, false, 1, 24.5f, 24.5f}, false, 0, {},
                             Coord3D{50.0f, 50.0f, 0.0f}, 0.0f);
    w.pm.update(nullptr);
    dumpPartition(d, w);
    d.line("end");
}

// --- part_rect: the rotated rect fill -------------------------------------------
static void corpusPartRect(Dumper &d)
{
    d.line("corpus part_rect");
    World &w = freshWorld(10.0f, Region2DT{0, 0, 100, 100});
    // axis-aligned: halfsizes exactly on the step grid
    w.createObject(GeometryInfoT{GEOMETRY_BOX, false, 1, 5.0f, 5.0f}, false, 0, {},
                             Coord3D{55.0f, 55.0f, 0.0f}, 0.0f);
    w.pm.update(nullptr);
    dumpPartition(d, w);

    // rotated 45 degrees
    w.createObject(GeometryInfoT{GEOMETRY_BOX, false, 1, 10.0f, 10.0f}, false, 0, {},
                             Coord3D{50.0f, 50.0f, 0.0f}, 0.7853981633974483f);
    w.pm.update(nullptr);
    dumpPartition(d, w);

    // a tiny box: one step per axis → exactly the center cell
    w.createObject(GeometryInfoT{GEOMETRY_BOX, false, 1, 1.0f, 1.0f}, false, 0, {},
                             Coord3D{55.0f, 55.0f, 0.0f}, 0.0f);
    w.pm.update(nullptr);
    dumpPartition(d, w);

    // a degenerate box: halfsize 0 → zero steps → no cells at all
    w.createObject(GeometryInfoT{GEOMETRY_BOX, false, 1, 0.0f, 0.0f}, false, 0, {},
                             Coord3D{55.0f, 55.0f, 0.0f}, 0.0f);
    w.pm.update(nullptr);
    dumpPartition(d, w);

    // a long thin rotated box spanning the grid edge
    w.createObject(GeometryInfoT{GEOMETRY_BOX, false, 1, 23.0f, 3.0f}, false, 0, {},
                             Coord3D{12.0f, 12.0f, 0.0f}, 0.3f);
    w.pm.update(nullptr);
    dumpPartition(d, w);

    // halfsize just over a step multiple (the fudge: 2*ceil(1.04) steps)
    w.createObject(GeometryInfoT{GEOMETRY_BOX, false, 1, 2.6f, 2.6f}, false, 0, {},
                             Coord3D{65.0f, 65.0f, 0.0f}, 0.0f);
    w.pm.update(nullptr);
    dumpPartition(d, w);
    d.line("end");
}

// --- part_dirty: the dirty-list family and the movement thresholds -------------
static void corpusPartDirty(Dumper &d)
{
    d.line("corpus part_dirty");
    World &w = freshWorld(10.0f, Region2DT{0, 0, 100, 100});
    GeometryInfoT g{GEOMETRY_SPHERE, false, 1, 3.0f, 3.0f};
    ObjectT *a = w.createObject(g, false, 0, {}, Coord3D{55.0f, 55.0f, 0.0f}, 0.0f);
    ObjectT *b = w.createObject(g, false, 0, {}, Coord3D{35.0f, 35.0f, 0.0f}, 0.0f);
    dumpPartition(d, w); // both dirty (the attach makeDirty), LIFO order

    // the half-update path: makeDirty(false) on a clean module
    PartitionData *mod = w.pm.findModule(a->m_id);
    w.pm.makeDirty(mod, false);
    dumpPartition(d, w);

    // the upgrade path: makeDirty(true) on an in-list half-dirty module
    w.pm.makeDirty(mod, true);
    dumpPartition(d, w);

    // the refresh: LIFO processing (b then a)
    w.pm.update(&w.hooks);
    dumpEvents(d, w);
    dumpPartition(d, w);

    // a re-dirty while clean, and the idempotent re-dirty while in-list
    w.pm.makeDirty(mod, true);
    w.pm.makeDirty(mod, true);
    dumpPartition(d, w);
    w.pm.update(&w.hooks);

    // movement thresholds: below 0.01 no dirty; above it dirty
    Coord3D old = a->m_pos;
    a->m_pos = {55.005f, 55.0f, 0.0f};
    w.reactToTransformChange(a, old, 0.0f);
    d.line("dirty-after-tiny %d", w.pm.m_dirtyModules != nullptr ? 1 : 0);
    a->m_pos = {56.0f, 55.0f, 0.0f};
    w.reactToTransformChange(a, old, 0.0f);
    d.line("dirty-after-move %d", w.pm.m_dirtyModules != nullptr ? 1 : 0);
    // the angle threshold
    w.pm.update(&w.hooks);
    old = a->m_pos;
    a->m_angle = 0.005f;
    w.reactToTransformChange(a, old, 0.0f);
    d.line("dirty-after-tiny-angle %d", w.pm.m_dirtyModules != nullptr ? 1 : 0);
    a->m_angle = 0.02f;
    w.reactToTransformChange(a, old, 0.0f);
    d.line("dirty-after-angle %d", w.pm.m_dirtyModules != nullptr ? 1 : 0);
    w.pm.update(&w.hooks);
    dumpPartition(d, w);

    // setGeometryInfo's unregister/re-register refresh
    a->m_geometryInfo = GeometryInfoT{GEOMETRY_BOX, false, 1, 8.0f, 4.0f};
    w.pm.unRegisterObject(a);
    w.pm.registerObject(a);
    dumpPartition(d, w);
    w.pm.update(&w.hooks);
    dumpPartition(d, w);
    d.line("cellchg %d %d", a->m_cellChangeCount, b->m_cellChangeCount);
    d.line("end");
}

// --- part_contact: the pair collection -------------------------------------------
static void corpusPartContact(Dumper &d)
{
    d.line("corpus part_contact");
    World &w = freshWorld(10.0f, Region2DT{0, 0, 100, 100});
    GeometryInfoT g{GEOMETRY_SPHERE, false, 1, 6.0f, 6.0f};
    // three overlapping movers + one immobile structure + one far away
    w.createObject(g, false, 0, {}, Coord3D{50.0f, 50.0f, 0.0f}, 0.0f);
    w.createObject(g, false, 0, {}, Coord3D{52.0f, 52.0f, 0.0f}, 0.0f);
    w.createObject(g, false, 0, {}, Coord3D{60.0f, 60.0f, 0.0f}, 0.0f);
    ObjectT *immobile = w.createObject(g, true, 0, {}, Coord3D{50.0f, 50.0f, 0.0f}, 0.0f);
    (void)immobile;
    w.createObject(g, false, 0, {}, Coord3D{90.0f, 90.0f, 0.0f}, 0.0f);

    w.pm.update(&w.hooks);
    dumpContacts(d, w);
    dumpPartition(d, w);

    // a move that stops overlapping: the pairs drop
    ObjectT *b = w.object(2);
    Coord3D old = b->m_pos;
    b->m_pos = {80.0f, 80.0f, 0.0f};
    w.reactToTransformChange(b, old, 0.0f);
    w.pm.update(&w.hooks);
    dumpContacts(d, w);
    dumpPartition(d, w);
    d.line("end");
}

// --- part_register: the master-list and the refusals ------------------------------
static void corpusPartRegister(Dumper &d)
{
    d.line("corpus part_register");
    World &w = freshWorld(10.0f, Region2DT{0, 0, 100, 100});
    GeometryInfoT g{GEOMETRY_SPHERE, false, 1, 3.0f, 3.0f};
    ObjectT *a = w.createObject(g, false, 0, {}, Coord3D{55.0f, 55.0f, 0.0f}, 0.0f);
    ObjectT *b = w.createObject(g, false, 0, {}, Coord3D{35.0f, 35.0f, 0.0f}, 0.0f);
    dumpPartition(d, w);

    // the double-register refusal (the master list is the observable)
    w.pm.registerObject(a);
    int masterCount = 0;
    for (PartitionData *m = w.pm.m_moduleList; m; m = m->m_next)
        masterCount++;
    d.line("dbl %d", masterCount);

    // the unregister: the module leaves the master list
    w.pm.unRegisterObject(b);
    dumpPartition(d, w);
    // the unregister of an unregistered object: refused
    w.pm.unRegisterObject(b);
    masterCount = 0;
    for (PartitionData *m = w.pm.m_moduleList; m; m = m->m_next)
        masterCount++;
    d.line("again %d", masterCount);
    // the destroy path unregisters (the ~Object flow, driven through the
    // kernel's destroy list)
    w.logic.destroyObject(a, &w.hooks);
    w.logic.processDestroyList(&w.pm, &w.hooks);
    dumpPartition(d, w);
    d.line("end");
}

// --- collide_matrix: the collide-test family over crafted geometry pairs -------
static void corpusCollideMatrix(Dumper &d)
{
    d.line("corpus collide_matrix");

    // the three geometry flavors: sphere (r=10), cylinder (r=10, h=8),
    // box (10x6, h=8)
    GeometryInfoT sphere{GEOMETRY_SPHERE, false, 10.0f, 10.0f, 10.0f};
    GeometryInfoT cyl{GEOMETRY_CYLINDER, false, 8.0f, 10.0f, 10.0f};
    GeometryInfoT box{GEOMETRY_BOX, false, 8.0f, 10.0f, 6.0f};
    const GeometryInfoT *geoms[3] = {&sphere, &cyl, &box};

    // (a) the 3x3 matrix at an overlapping offset (the second geom rotated
    // 0.3 rad), through geomCollidesWithGeom — the boolean
    Coord3D p1{100.0f, 100.0f, 0.0f};
    Coord3D p2{104.0f, 100.0f, 0.0f};
    for (int i = 0; i < 3; ++i) {
        for (int j = 0; j < 3; ++j) {
            bool res = geomCollidesWithGeom(&p1, *geoms[i], 0.0f, &p2, *geoms[j], 0.3f);
            d.line("m %d %d %d", i, j, res ? 1 : 0);
        }
    }
    // ... and at a miss offset
    Coord3D p3{140.0f, 100.0f, 0.0f};
    for (int i = 0; i < 3; ++i) {
        for (int j = 0; j < 3; ++j) {
            bool res = geomCollidesWithGeom(&p1, *geoms[i], 0.0f, &p3, *geoms[j], 0.3f);
            d.line("miss %d %d %d", i, j, res ? 1 : 0);
        }
    }

    // (b) the same matrix through the proc table directly, with the cinfo —
    // the float observables (loc and normal)
    for (int i = 0; i < 3; ++i) {
        for (int j = 0; j < 3; ++j) {
            CollideInfoT a(p1, *geoms[i], 0.0f);
            CollideInfoT b(p2, *geoms[j], 0.3f);
            CollideLocAndNormal cinfo = {};
            bool res = theCollideTestProcs[(size_t)i * GEOMETRY_NUM_TYPES + j](&a, &b, &cinfo);
            d.line("d %d %d %d loc=%08x,%08x,%08x nrm=%08x,%08x,%08x", i, j, res ? 1 : 0,
                   f32bits(cinfo.loc.x), f32bits(cinfo.loc.y), f32bits(cinfo.loc.z),
                   f32bits(cinfo.normal.x), f32bits(cinfo.normal.y), f32bits(cinfo.normal.z));
        }
    }

    // (c) the null-cinfo guard: every proc takes a null cinfo fine
    {
        CollideInfoT a(p1, sphere, 0.0f);
        CollideInfoT b(p2, sphere, 0.0f);
        bool res = theCollideTestProcs[0](&a, &b, nullptr);
        d.line("nullcinfo %d", res ? 1 : 0);
    }

    // (d) the sphere-sphere touch boundary: dist == r1+r2 (the <=) and
    // the step past it; plus the coincident spheres (the normalize
    // zero-guard leaves the normal zero)
    {
        GeometryInfoT s15{GEOMETRY_SPHERE, false, 15.0f, 15.0f, 15.0f};
        CollideInfoT a(Coord3D{0.0f, 0.0f, 0.0f}, s15, 0.0f);
        CollideInfoT b(Coord3D{30.0f, 0.0f, 0.0f}, s15, 0.0f);
        CollideLocAndNormal cinfo = {};
        bool res = theCollideTestProcs[0](&a, &b, &cinfo);
        d.line("touch %d loc=%08x,%08x,%08x nrm=%08x,%08x,%08x", res ? 1 : 0,
               f32bits(cinfo.loc.x), f32bits(cinfo.loc.y), f32bits(cinfo.loc.z),
               f32bits(cinfo.normal.x), f32bits(cinfo.normal.y), f32bits(cinfo.normal.z));
        CollideInfoT c(Coord3D{30.1f, 0.0f, 0.0f}, s15, 0.0f);
        d.line("past %d", theCollideTestProcs[0](&a, &c, nullptr) ? 1 : 0);
        CollideInfoT z(Coord3D{0.0f, 0.0f, 0.0f}, s15, 0.0f);
        CollideLocAndNormal cz = {};
        bool resz = theCollideTestProcs[0](&a, &z, &cz);
        d.line("coincident %d loc=%08x,%08x,%08x nrm=%08x,%08x,%08x", resz ? 1 : 0,
               f32bits(cz.loc.x), f32bits(cz.loc.y), f32bits(cz.loc.z),
               f32bits(cz.normal.x), f32bits(cz.normal.y), f32bits(cz.normal.z));
    }

    // (e) the closeEnough preflight: a thin 10x1 box rotated 90 degrees
    // versus an unrotated 10x6 box, centers 0.9 apart — every corner of
    // either misses the other's frame (the plate's corners stand 10 off
    // the box's minor axis; the box's corners stand 9.1 off the plate's
    // minor axis), so the rect test says false; the minRadius=1 preflight
    // (0.9^2 < 1^2) forces the true — with loc.x/y and the normal left
    // unwritten (the pinned zero), loc.z the side-to-side average
    {
        GeometryInfoT plate{GEOMETRY_BOX, false, 8.0f, 10.0f, 1.0f};
        GeometryInfoT bx{GEOMETRY_BOX, false, 8.0f, 10.0f, 6.0f};
        CollideInfoT a(Coord3D{50.0f, 50.0f, 0.0f}, plate, 1.57079632f);
        CollideInfoT c(Coord3D{50.9f, 50.0f, 0.0f}, bx, 0.0f);
        CollideLocAndNormal cinfo = {};
        bool res = theCollideTestProcs[8](&a, &c, &cinfo); // Box_Box
        d.line("closeenough %d loc=%08x,%08x,%08x nrm=%08x,%08x,%08x", res ? 1 : 0,
               f32bits(cinfo.loc.x), f32bits(cinfo.loc.y), f32bits(cinfo.loc.z),
               f32bits(cinfo.normal.x), f32bits(cinfo.normal.y), f32bits(cinfo.normal.z));
        // the same pair one step further apart: the preflight misses too,
        // and the rect test still says false
        CollideInfoT c2(Coord3D{51.1f, 50.0f, 0.0f}, bx, 0.0f);
        d.line("toofar %d", theCollideTestProcs[8](&a, &c2, nullptr) ? 1 : 0);
        // and through geomCollidesWithGeom (the z-gate passes at z=0, so
        // the same verdict)
        d.line("closeenough-geom %d", geomCollidesWithGeom(&a.position, plate, 1.57079632f, &c.position, bx, 0.0f) ? 1 : 0);
    }

    // (f) the rect-circle square promotion: a cylinder r=5 diagonal from an
    // unrotated 10x6 box — the true circle misses (the closest box corner
    // is sqrt(3.5^2+4.5^2) > 5 away) but the promoted 10x10 square's
    // corner cap catches the box corner. Both orders (the flipped normal
    // of the Circle_Rect wrapper).
    {
        GeometryInfoT small{GEOMETRY_CYLINDER, false, 8.0f, 5.0f, 5.0f};
        GeometryInfoT bx{GEOMETRY_BOX, false, 8.0f, 10.0f, 6.0f};
        CollideInfoT cylI(Coord3D{63.5f, 60.5f, 0.0f}, small, 0.0f);
        CollideInfoT boxI(Coord3D{50.0f, 50.0f, 0.0f}, bx, 0.0f);
        CollideLocAndNormal c1 = {};
        bool r1 = theCollideTestProcs[1 * GEOMETRY_NUM_TYPES + 2](&cylI, &boxI, &c1); // Cylinder_Box
        d.line("promo-cyl-box %d loc=%08x,%08x,%08x nrm=%08x,%08x,%08x", r1 ? 1 : 0,
               f32bits(c1.loc.x), f32bits(c1.loc.y), f32bits(c1.loc.z),
               f32bits(c1.normal.x), f32bits(c1.normal.y), f32bits(c1.normal.z));
        CollideLocAndNormal c2 = {};
        bool r2 = theCollideTestProcs[2 * GEOMETRY_NUM_TYPES + 1](&boxI, &cylI, &c2); // Box_Cylinder
        d.line("promo-box-cyl %d loc=%08x,%08x,%08x nrm=%08x,%08x,%08x", r2 ? 1 : 0,
               f32bits(c2.loc.x), f32bits(c2.loc.y), f32bits(c2.loc.z),
               f32bits(c2.normal.x), f32bits(c2.normal.y), f32bits(c2.normal.z));
        // the true circle would miss: the same cylinder moved past the
        // box — both the circle and the promoted square miss
        CollideInfoT far(Coord3D{66.0f, 63.0f, 0.0f}, small, 0.0f);
        d.line("promo-far %d", theCollideTestProcs[1 * GEOMETRY_NUM_TYPES + 2](&far, &boxI, nullptr) ? 1 : 0);
    }

    // (g) the sphere-slice cases of z_collideTest_Sphere_Nonsphere: a
    // sphere above (2b) and below (2a) the nonsphere, within radius of its
    // top/bottom — the slice radius sqrt(r^2 - dz^2) decides
    {
        GeometryInfoT sph{GEOMETRY_SPHERE, false, 10.0f, 10.0f, 10.0f};
        GeometryInfoT small{GEOMETRY_CYLINDER, false, 8.0f, 6.0f, 6.0f};
        // 2b: sphere center z=15 above the cylinder top (z=8), 7 within
        // the radius 10 → slice radius sqrt(100-49)≈7.14; xy distance 6.5
        // → collides (6.5 <= 7.14+6); at 14 it does not
        CollideInfoT above(Coord3D{0.0f, 0.0f, 15.0f}, sph, 0.0f);
        CollideInfoT cylI(Coord3D{6.5f, 0.0f, 0.0f}, small, 0.0f);
        CollideLocAndNormal c1 = {};
        bool r1 = theCollideTestProcs[0 * GEOMETRY_NUM_TYPES + 1](&above, &cylI, &c1); // Sphere_Cylinder
        d.line("slice-above %d loc=%08x,%08x,%08x nrm=%08x,%08x,%08x", r1 ? 1 : 0,
               f32bits(c1.loc.x), f32bits(c1.loc.y), f32bits(c1.loc.z),
               f32bits(c1.normal.x), f32bits(c1.normal.y), f32bits(c1.normal.z));
        CollideInfoT farC(Coord3D{14.0f, 0.0f, 0.0f}, small, 0.0f);
        d.line("slice-above-far %d", theCollideTestProcs[0 * GEOMETRY_NUM_TYPES + 1](&above, &farC, nullptr) ? 1 : 0);
        // 2a: sphere center z=-5 below the cylinder bottom (z=0), 5 within
        // the radius 10 → slice radius sqrt(100-25)≈8.66; xy distance 7
        CollideInfoT below(Coord3D{0.0f, 0.0f, -5.0f}, sph, 0.0f);
        CollideInfoT cyl2(Coord3D{7.0f, 0.0f, 0.0f}, small, 0.0f);
        CollideLocAndNormal c2 = {};
        bool r2 = theCollideTestProcs[0 * GEOMETRY_NUM_TYPES + 1](&below, &cyl2, &c2);
        d.line("slice-below %d loc=%08x,%08x,%08x nrm=%08x,%08x,%08x", r2 ? 1 : 0,
               f32bits(c2.loc.x), f32bits(c2.loc.y), f32bits(c2.loc.z),
               f32bits(c2.normal.x), f32bits(c2.normal.y), f32bits(c2.normal.z));
        // the reversed order (Cylinder_Sphere): the same path with the
        // flipped normal
        CollideLocAndNormal c3 = {};
        bool r3 = theCollideTestProcs[1 * GEOMETRY_NUM_TYPES + 0](&cyl2, &below, &c3);
        d.line("slice-rev %d loc=%08x,%08x,%08x nrm=%08x,%08x,%08x", r3 ? 1 : 0,
               f32bits(c3.loc.x), f32bits(c3.loc.y), f32bits(c3.loc.z),
               f32bits(c3.normal.x), f32bits(c3.normal.y), f32bits(c3.normal.z));
    }

    // (h) the z-gates. collidesWith (the object path, with the
    // getMaxHeightBelowPosition extents) vs geomCollidesWithGeom (the
    // raw-z gate — narrower). The sphere-pair below-extent asymmetry: two
    // spheres r=10 at z=5 and z=-6, xy 3 apart — collidesWith's gate
    // passes (thisBot=-5 <= thatTop=4), geomCollidesWithGeom's does not
    // (this.z=5 > thatTop=4). Also the collidesWith boundary itself:
    // sphere z=18 vs cylinder h=8 (thisBot=8 == thatTop=8 passes; z=18.5
    // does not).
    {
        World &w = freshWorld(10.0f, Region2DT{0, 0, 100, 100});
        GeometryInfoT sph{GEOMETRY_SPHERE, false, 10.0f, 10.0f, 10.0f};
        ObjectT *s1 = w.createObject(sph, false, 0, {}, Coord3D{10.0f, 10.0f, 5.0f}, 0.0f);
        ObjectT *s2 = w.createObject(sph, false, 0, {}, Coord3D{13.0f, 10.0f, -6.0f}, 0.0f);
        CollideLocAndNormal cinfo = {};
        d.line("asym-collides %d", w.collidesWith(s1->m_id, s2->m_id, &cinfo) ? 1 : 0);
        d.line("asym-geom %d",
               geomCollidesWithGeom(&s1->m_pos, sph, 0.0f, &s2->m_pos, sph, 0.0f) ? 1 : 0);
        d.line("asym-geom-rev %d",
               geomCollidesWithGeom(&s2->m_pos, sph, 0.0f, &s1->m_pos, sph, 0.0f) ? 1 : 0);

        GeometryInfoT cy{GEOMETRY_CYLINDER, false, 8.0f, 6.0f, 6.0f};
        ObjectT *s3 = w.createObject(sph, false, 0, {}, Coord3D{30.0f, 10.0f, 18.0f}, 0.0f);
        ObjectT *cylO = w.createObject(cy, false, 0, {}, Coord3D{31.0f, 10.0f, 0.0f}, 0.0f);
        d.line("zbound %d", w.collidesWith(s3->m_id, cylO->m_id, nullptr) ? 1 : 0);
        ObjectT *s4 = w.createObject(sph, false, 0, {}, Coord3D{40.0f, 10.0f, 18.5f}, 0.0f);
        ObjectT *cylO2 = w.createObject(cy, false, 0, {}, Coord3D{41.0f, 10.0f, 0.0f}, 0.0f);
        d.line("zbound-past %d", w.collidesWith(s4->m_id, cylO2->m_id, nullptr) ? 1 : 0);
        // the KINDOF_NO_COLLIDE refusal (either side)
        ObjectT *nc = w.createObject(sph, false, 0, {}, Coord3D{50.0f, 10.0f, 0.0f}, 0.0f);
        ObjectT *plain = w.createObject(sph, false, 0, {}, Coord3D{52.0f, 10.0f, 0.0f}, 0.0f);
        nc->m_kindof |= (1u << KINDOF_NO_COLLIDE_BIT);
        d.line("nocol %d", w.collidesWith(nc->m_id, plain->m_id, nullptr) ? 1 : 0);
        d.line("nocol-rev %d", w.collidesWith(plain->m_id, nc->m_id, nullptr) ? 1 : 0);
    }
    d.line("end");
}

// --- collide_process: processContactList through the store --------------------
static void corpusCollideProcess(Dumper &d)
{
    d.line("corpus collide_process");
    World &w = freshWorld(10.0f, Region2DT{0, 0, 100, 100});
    GeometryInfoT sph{GEOMETRY_SPHERE, false, 6.0f, 6.0f, 6.0f};

    // (a) the dispatch order and the flipped normal: two overlapping
    // spheres, two plain colliders each
    ObjectT *a = w.createObject(sph, false, 0, {}, Coord3D{50.0f, 50.0f, 0.0f}, 0.0f, {}, 2);
    ObjectT *b = w.createObject(sph, false, 0, {}, Coord3D{52.0f, 52.0f, 0.0f}, 0.0f, {}, 2);
    w.pm.update(&w.hooks);
    dumpContacts(d, w);
    dumpCollideEvents(d, w);
    dumpPartition(d, w);

    // (b) the steady state: the re-dirtied pair re-processes every frame
    w.pm.update(&w.hooks);
    dumpContacts(d, w);
    dumpCollideEvents(d, w);

    // (c) the NO_COLLISIONS pre-gate: the pair is still collected, no
    // dispatch
    b->m_status |= (1u << OBJECT_STATUS_NO_COLLISIONS_BIT);
    w.pm.update(&w.hooks);
    dumpContacts(d, w);
    dumpCollideEvents(d, w);

    // (d) the mid-walk absorb: a's first collider sets the bit — a's
    // second is skipped, b's walk is unaffected; the NEXT update's
    // pre-gate refuses the pair. (The pair must be re-dirtied first: the
    // pre-gate refusal of part (c) left both clean.)
    b->m_status &= ~(1u << OBJECT_STATUS_NO_COLLISIONS_BIT);
    w.pm.makeDirty(w.pm.findModule(a->m_id), true);
    w.pm.makeDirty(w.pm.findModule(b->m_id), true);
    a->m_colliders[0].m_onCollideAbsorb = true;
    w.pm.update(&w.hooks);
    dumpCollideEvents(d, w);
    w.pm.update(&w.hooks);
    dumpCollideEvents(d, w);
    dumpPartition(d, w);
    a->m_colliders[0].m_onCollideAbsorb = false;
    a->m_status &= ~(1u << OBJECT_STATUS_NO_COLLISIONS_BIT);

    // (e) the destroyed-absorb: c's first collider destroys e during the
    // FIRST dispatch — e's onCollide is skipped (the isDestroyed gate),
    // e's re-dirty is refused; the destroy list processes at the corpus's
    // hand. (The first update after registration collects the pair from
    // the first-registered object's walk, so the pair is (c, e) — c
    // dispatches first.)
    ObjectT *c = w.createObject(sph, false, 0, {}, Coord3D{20.0f, 50.0f, 0.0f}, 0.0f, {}, 1);
    ObjectT *e = w.createObject(sph, false, 0, {}, Coord3D{22.0f, 52.0f, 0.0f}, 0.0f, {}, 1);
    c->m_colliders[0].m_onCollideDestroyObj = e->m_id;
    w.pm.update(&w.hooks);
    dumpCollideEvents(d, w);
    dumpRegistry(d, w);
    dumpPartition(d, w);
    w.logic.processDestroyList(&w.pm, &w.hooks);
    dumpPartition(d, w);

    // (f) the unregister mid-processing: g's first collider unregisters h
    // during the (g,f) pair's dispatch — removeSpecificPartitionData nulls
    // the later (h,g) node, and the walk skips it. h stays a live object
    // with no partition data (its re-dirty is refused, its dispatch in the
    // pair that spawned the unregister still runs)
    ObjectT *f = w.createObject(sph, false, 0, {}, Coord3D{20.0f, 20.0f, 0.0f}, 0.0f, {}, 1);
    ObjectT *g = w.createObject(sph, false, 0, {}, Coord3D{25.0f, 25.0f, 0.0f}, 0.0f, {}, 1);
    ObjectT *h = w.createObject(sph, false, 0, {}, Coord3D{30.0f, 30.0f, 0.0f}, 0.0f, {}, 1);
    g->m_colliders[0].m_onCollideUnregisterObj = h->m_id;
    w.pm.update(&w.hooks);
    dumpContacts(d, w);
    dumpCollideEvents(d, w);
    dumpRegistry(d, w);
    dumpPartition(d, w);

    // (g) the immobile re-dirty refusal: a mobile-immobile pair — both
    // dispatch, only the mobile side re-dirties (the steady state below
    // collects the pair through the mobile side alone)
    ObjectT *mob = w.createObject(sph, false, 0, {}, Coord3D{70.0f, 70.0f, 0.0f}, 0.0f, {}, 1);
    ObjectT *imm = w.createObject(sph, true, 0, {}, Coord3D{72.0f, 72.0f, 0.0f}, 0.0f, {}, 1);
    w.pm.update(&w.hooks);
    dumpCollideEvents(d, w);
    dumpPartition(d, w);
    w.pm.update(&w.hooks);
    dumpContacts(d, w);
    dumpCollideEvents(d, w);

    // (h) the z-miss: xy-overlapping but z-disjoint — the pair is
    // collected (the cells are 2d), the collide test refuses
    w.createObject(sph, false, 0, {}, Coord3D{80.0f, 80.0f, 0.0f}, 0.0f, {}, 1);
    w.createObject(sph, false, 0, {}, Coord3D{82.0f, 82.0f, 50.0f}, 0.0f, {}, 1);
    w.pm.update(&w.hooks);
    dumpContacts(d, w);
    dumpCollideEvents(d, w);
    (void)a;
    (void)b;
    (void)c;
    (void)e;
    (void)f;
    (void)g;
    (void)h;
    (void)mob;
    (void)imm;
    d.line("end");
}

// --- kernel_roundtrip: a mixed run — the whole kernel through several frames -------
// --- players_lifecycle: the ctor/init scheme, the neutral birth, the
// interning, the update walk's 16-slot quirk, the reset's stale relations --
static void corpusPlayersLifecycle(Dumper &d)
{
    d.line("corpus players_lifecycle");
    World &w = freshWorld(10.0f, Region2DT{0, 0, 100, 100});

    // the birth state: count 1, local 0 (the becomingLocalPlayer(0,true)
    // transition), the neutral self-relation, the empty-string key, the
    // -1 slot arrays
    dumpPlayers(d, w);
    dumpTeamEvents(d, w);

    // the name-key interning: the sequential ordinals, the case
    // sensitivity, the re-intern of the same name (sequenced — the
    // printf argument order is unspecified in C++)
    unsigned nkA = w.nameKeys.nameToKey("player1");
    unsigned nkB = w.nameKeys.nameToKey("player2");
    unsigned nkCap = w.nameKeys.nameToKey("Player1");
    unsigned nkA2 = w.nameKeys.nameToKey("player1");
    d.line("nk empty=%u", w.nameKeys.nameToKey(""));
    d.line("nk a=%u b=%u A=%u a2=%u players=%u", nkA, nkB, nkCap, nkA2, nkA2);

    // the 16-slot update walk (the count is 1 — every slot updates anyway)
    w.playerListUpdate();
    dumpTeamEvents(d, w);

    // getNthPlayer's bounds refusal
    d.line("nth %d %d", w.playerList.getNthPlayer(-1) ? 1 : 0, w.playerList.getNthPlayer(MAX_PLAYER_COUNT) ? 1 : 0);

    // the stale-relation quirk: relations set on a player, then reset() —
    // the maps survive init (they are born in the ctor and die in the dtor)
    PlayerT *p1 = w.playerList.getNthPlayer(1);
    PlayerT *p2 = w.playerList.getNthPlayer(2);
    playerSetPlayerRelationship(p1, p2, R_ENEMIES);
    playerSetTeamRelationship(p1, teamNew(&w.teamFactory, nullptr, 77, &w), R_ALLIES);
    d.line("setrel");
    dumpPlayers(d, w);
    w.playerListReset();
    d.line("afterreset");
    dumpPlayers(d, w);
    dumpTeamEvents(d, w);

    // setLocalPlayer's transitions and the null refusal (null → neutral)
    w.playerListSetLocalPlayer(w.playerList.getNthPlayer(3));
    w.playerListSetLocalPlayer(nullptr);
    dumpTeamEvents(d, w);
    dumpPlayers(d, w);
}

// --- players_newgame: the sides walk, the local-selection precedence, the
// enemies/allies assignment, the default teams, the slots -------------
static void corpusPlayersNewGame(Dumper &d)
{
    d.line("corpus players_newgame");
    {
        // a 4-side map: neutral, a human, two computers; the multiplayer
        // local flag on a LATER side beats the earlier human
        World &w = freshWorld(10.0f, Region2DT{0, 0, 100, 100});
        SidesSpecT sides;
        sides.sides.push_back(SideSpecT{}); // the neutral side (empty name)
        sides.sides.push_back(sideSpec("america", true, false, false, {"china"}, {}));
        sides.sides.push_back(sideSpec("china", false, false, false, {}, {"america"}));
        sides.sides.push_back(sideSpec("gla", false, true, false, {"america", "china", "nosuch"}, {}));
        // the standard team singletons the default teams resolve through
        sides.teams.push_back(teamSpec("team", "neutral", true));
        sides.teams.push_back(teamSpec("teamamerica", "america", true));
        sides.teams.push_back(teamSpec("teamchina", "china", true));
        // the multiplayer slot indices (player0..player3 names are what
        // assignSlotIndices probes — our names miss, pinning the miss)
        sides.hasGameInfo = true;
        sides.gameInfo.occupiedSlots = {0, 2, 3};

        w.playerListNewGame(sides);
        dumpPlayers(d, w);
        dumpTeams(d, w);
        dumpTeamEvents(d, w);

        // the local-selection precedence: the multiplayerIsLocal side (gla,
        // player 3) won over the earlier human (america, player 1)
        d.line("local %d", w.playerList.m_local->m_playerIndex);

        // the asymmetric relations: america→china ENEMIES, china→america
        // ALLIES (the walk assigns per side, no symmetry)
        d.line("rel a->c %s c->a %s a->g %s g->a %s",
               relationshipName(playerGetRelationship(w.playerList.getNthPlayer(1),
                                                      teamByID(&w, w.playerList.getNthPlayer(2)->m_defaultTeam), &w)),
               relationshipName(playerGetRelationship(w.playerList.getNthPlayer(2),
                                                      teamByID(&w, w.playerList.getNthPlayer(1)->m_defaultTeam), &w)),
               relationshipName(playerGetRelationship(w.playerList.getNthPlayer(1),
                                                      teamByID(&w, w.playerList.getNthPlayer(3)->m_defaultTeam), &w)),
               relationshipName(playerGetRelationship(w.playerList.getNthPlayer(3),
                                                      teamByID(&w, w.playerList.getNthPlayer(1)->m_defaultTeam), &w)));

        // the slot lookups (the "player%d" probe missed our names)
        d.line("slots %d %d %d %d", w.playerListGetSlotIndex(1), w.playerListGetSlotIndex(2),
               w.playerListGetPlayerIndexFromSlotIndex(0), w.playerListGetPlayerIndexFromSlotIndex(4));
    }
    {
        // the same map with the network present: no human wins, the
        // multiplayerIsLocal flag is the only local source
        World &w = freshWorld(10.0f, Region2DT{0, 0, 100, 100});
        w.m_networkPresent = true;
        SidesSpecT sides;
        sides.sides.push_back(SideSpecT{});
        sides.sides.push_back(sideSpec("america", true, false, false, {}, {}));
        sides.sides.push_back(sideSpec("china", false, true, false, {}, {}));
        w.playerListNewGame(sides);
        d.line("netlocal %d", w.playerList.m_local->m_playerIndex);
        dumpTeamEvents(d, w);
    }
    {
        // no local at all (all computers, no network): the fallback forces
        // the first non-neutral player HUMAN and local; and a neutral side
        // with enemies of its own (the second loop processes the neutral
        // player too — its enemies assignment lands)
        World &w = freshWorld(10.0f, Region2DT{0, 0, 100, 100});
        SidesSpecT sides;
        SideSpecT neutralSide;
        neutralSide.enemies = {"america"};
        sides.sides.push_back(neutralSide);
        sides.sides.push_back(sideSpec("america", false, false, false, {}, {}));
        sides.teams.push_back(teamSpec("team", "neutral", true));
        w.playerListNewGame(sides);
        d.line("forcedlocal %d type0=%d type1=%d", w.playerList.m_local->m_playerIndex,
               (int)w.playerList.getNthPlayer(0)->m_playerType, (int)w.playerList.getNthPlayer(1)->m_playerType);
        dumpPlayers(d, w);
        dumpTeamEvents(d, w);
    }
    {
        // the missing default-team prototype: setDefaultTeam's null path
        // (the release build continues; the default team stays 0)
        World &w = freshWorld(10.0f, Region2DT{0, 0, 100, 100});
        SidesSpecT sides;
        sides.sides.push_back(SideSpecT{});
        sides.sides.push_back(sideSpec("america", true, false, false, {}, {}));
        // no teams at all
        w.playerListNewGame(sides);
        d.line("nodefault %u", w.playerList.getNthPlayer(1)->m_defaultTeam);
    }
}

// --- teams_factory: the id scheme, the singleton reuse, the auto-create,
// the duplicate-name refusal, the map order -----------------------
static void corpusTeamsFactory(Dumper &d)
{
    d.line("corpus teams_factory");
    World &w = freshWorld(10.0f, Region2DT{0, 0, 100, 100});
    SidesSpecT sides;
    sides.sides.push_back(SideSpecT{});
    sides.sides.push_back(sideSpec("america", true, false, false, {}, {}));
    sides.sides.push_back(sideSpec("china", false, false, false, {}, {}));

    // the team list: the standard singletons, a non-singleton, a duplicate
    // name (the release refusal — the prototype exists in the owner's list
    // but not the map), an unknown owner (the neutral fallback), and an
    // executeActions production condition that exists / doesn't
    TeamSpecT squad;
    squad.name = "americaSquad";
    squad.owner = "america";
    squad.singleton = false;
    squad.scriptOnDestroyed = "sd";
    squad.destroyedThreshold = 0.5f;
    sides.teams.push_back(teamSpec("team", "neutral", true));
    sides.teams.push_back(teamSpec("teamamerica", "america", true));
    sides.teams.push_back(teamSpec("teamchina", "china", true));
    sides.teams.push_back(squad);
    sides.teams.push_back(teamSpec("teamamerica", "china", true)); // the duplicate name
    sides.teams.push_back(teamSpec("stray", "nosuchplayer", true));
    TeamSpecT prod;
    prod.name = "prodteam";
    prod.owner = "china";
    prod.singleton = false;
    prod.executeActions = true;
    prod.productionCondition = "condA";
    prod.productionScriptExists = true;
    sides.teams.push_back(prod);
    TeamSpecT prodMiss;
    prodMiss.name = "prodmiss";
    prodMiss.owner = "china";
    prodMiss.singleton = true;
    prodMiss.executeActions = true;
    prodMiss.productionCondition = "condB";
    prodMiss.productionScriptExists = false;
    sides.teams.push_back(prodMiss);

    w.playerListNewGame(sides);
    dumpTeams(d, w);
    dumpTeamEvents(d, w);

    // the ++id scheme and the singleton reuse: createTeam on a singleton
    // returns the SAME instance (no new id); createInactiveTeam ditto
    TeamT *ta = w.teamFactory.createTeam("teamamerica", &w);
    TeamT *tb = w.teamFactory.createInactiveTeam("teamamerica", &w);
    d.line("singleton %u %u", ta->getID(), tb->getID());

    // the non-singleton auto-create of findTeam, and the explicit creates
    TeamT *s1 = w.teamFactory.findTeam("americaSquad", &w);
    d.line("find1 %u act=%d", s1->getID(), s1->m_active ? 1 : 0);
    TeamT *s2 = w.teamFactory.createTeam("americaSquad", &w);
    TeamT *s3 = w.teamFactory.createTeam("americaSquad", &w);
    d.line("creates %u %u", s2->getID(), s3->getID());
    TeamT *s4 = w.teamFactory.findTeam("americaSquad", &w);
    d.line("find2 %u", s4->getID());

    // createTeamOnPrototype: the singleton's reuse and the null refusal
    TeamProtoT *tpA = w.teamFactory.findTeamPrototype("teamamerica", &w);
    TeamT *c1 = w.teamFactory.createTeamOnPrototype(tpA, &w);
    d.line("onproto %u null=%d", c1->getID(), w.teamFactory.createTeamOnPrototype(nullptr, &w) ? 1 : 0);

    // the unknown prototype refusal
    d.line("unknown %d %d", w.teamFactory.createInactiveTeam("nosuch", &w) ? 1 : 0,
           w.teamFactory.findTeam("nosuch", &w) ? 1 : 0);

    // findTeamByID: the prototype-map walk order (key order — the
    // interning order of the names), the INVALID refusal
    d.line("byid t1=%u t2=%u t0=%d",
           w.teamFactory.findTeamByID(1)->getID(), w.teamFactory.findTeamByID(2)->getID(),
           w.teamFactory.findTeamByID(TEAM_ID_T_INVALID) ? 1 : 0);
    // findTeamPrototypeByID: the map walk
    d.line("protobyid 1=%s 2=%s 99=%d",
           w.teamFactory.findTeamPrototypeByID(1)->m_name.c_str(),
           w.teamFactory.findTeamPrototypeByID(2)->m_name.c_str(),
           w.teamFactory.findTeamPrototypeByID(99) ? 1 : 0);

    dumpTeams(d, w);
    dumpTeamEvents(d, w);
}

// --- relationships: the override chain, the precedence ladder, the removal
// family, the defector gates, the setControllingPlayer move ------------
static void corpusRelationships(Dumper &d)
{
    d.line("corpus relationships");
    World &w = freshWorld(10.0f, Region2DT{0, 0, 200, 200});
    SidesSpecT sides;
    sides.sides.push_back(SideSpecT{});
    sides.sides.push_back(sideSpec("america", true, false, false, {}, {}));
    sides.sides.push_back(sideSpec("china", false, false, false, {}, {}));
    sides.sides.push_back(sideSpec("gla", false, false, false, {}, {}));
    sides.teams.push_back(teamSpec("team", "neutral", true));
    sides.teams.push_back(teamSpec("teamamerica", "america", true));
    sides.teams.push_back(teamSpec("teamchina", "china", true));
    sides.teams.push_back(teamSpec("teamgla", "gla", true));
    sides.teams.push_back(teamSpec("americaSquad", "america", false));
    sides.teams.push_back(teamSpec("chinaSquad", "china", false));
    w.playerListNewGame(sides);
    dumpTeamEvents(d, w);

    PlayerT *a = w.playerList.getNthPlayer(1);
    PlayerT *c = w.playerList.getNthPlayer(2);
    PlayerT *g = w.playerList.getNthPlayer(3);
    TeamT *ta = w.teamFactory.findTeam("teamamerica", &w);
    TeamT *tc = w.teamFactory.findTeam("teamchina", &w);
    TeamT *tg = w.teamFactory.findTeam("teamgla", &w);
    TeamT *sa = w.teamFactory.createTeam("americaSquad", &w);
    TeamT *sc = w.teamFactory.createTeam("chinaSquad", &w);

    // the ladder: player→team (through the default team), the team override,
    // the player override on the team, the owning player's view
    playerSetPlayerRelationship(a, c, R_ENEMIES);
    d.line("l0 %s", relationshipName(playerGetRelationship(a, tc, &w)));
    playerSetTeamRelationship(a, tc, R_ALLIES);
    d.line("l1 %s", relationshipName(playerGetRelationship(a, tc, &w)));
    playerSetTeamRelationship(a, tg, R_NEUTRAL);
    d.line("l2 %s", relationshipName(playerGetRelationship(a, tg, &w)));
    // the team-level chain: team override > team's player override > owner
    d.line("t0 %s", relationshipName(teamGetRelationship(sa, sc, &w)));
    sa->setOverrideTeamRelationship(sc->getID(), R_ALLIES);
    d.line("t1 %s", relationshipName(teamGetRelationship(sa, sc, &w)));
    sa->setOverridePlayerRelationship(c->m_playerIndex, R_NEUTRAL);
    d.line("t2 %s", relationshipName(teamGetRelationship(sa, sc, &w)));
    sa->removeOverrideTeamRelationship(sc->getID());
    d.line("t3 %s", relationshipName(teamGetRelationship(sa, sc, &w)));
    // the other team's side (asymmetry)
    d.line("t4 %s", relationshipName(teamGetRelationship(sc, sa, &w)));

    // the null-Team forms: NEUTRAL (the `if (that)` guard)
    d.line("n0 %s %s", relationshipName(playerGetRelationship(a, nullptr, &w)),
           relationshipName(teamGetRelationship(sa, nullptr, &w)));

    // the removal family: the empty-map refusal, the miss, the null
    // clear-alls, the INVALID clear-alls on the team overrides
    PlayerT *n = w.playerList.getNeutralPlayer();
    d.line("r0 %d", playerRemovePlayerRelationship(n, c) ? 1 : 0);
    d.line("r1 %d", playerRemovePlayerRelationship(a, g) ? 1 : 0);
    d.line("r2 %d", playerRemoveTeamRelationship(a, nullptr) ? 1 : 0);
    d.line("r3 %d", playerRemovePlayerRelationship(a, nullptr) ? 1 : 0);
    d.line("r4 %d %d", sa->removeOverrideTeamRelationship(TEAM_ID_T_INVALID) ? 1 : 0,
           sa->removeOverridePlayerRelationship(PLAYER_INDEX_INVALID) ? 1 : 0);
    d.line("r5 %d", sa->removeOverrideTeamRelationship(999) ? 1 : 0);
    d.line("r6 %s", relationshipName(teamGetRelationship(sa, sc, &w)));
    dumpPlayers(d, w);
    dumpTeams(d, w);

    // Object::getRelationship: the defector gates over the object chain
    // (re-establish the base enemy so each gate's verdict differs)
    playerSetPlayerRelationship(a, c, R_ENEMIES);
    GeometryInfoT sph{GEOMETRY_SPHERE, true, 1, 1, 1};
    ObjectT *oa = w.createObject(sph, false, 0, {}, Coord3D{50.0f, 50.0f, 0.0f});
    ObjectT *oc = w.createObject(sph, false, 0, {}, Coord3D{60.0f, 50.0f, 0.0f});
    objectSetTeam(oa, ta, &w);
    objectSetTeam(oc, tc, &w);
    d.line("d0 %s", relationshipName(objectGetRelationship(oa, oc, &w)));
    oa->m_privateStatus |= (1u << OBJECT_PRIVATE_UNDETECTED_DEFECTOR_BIT);
    d.line("d1 %s", relationshipName(objectGetRelationship(oa, oc, &w)));
    oa->m_privateStatus &= ~(1u << OBJECT_PRIVATE_UNDETECTED_DEFECTOR_BIT);
    oc->m_privateStatus |= (1u << OBJECT_PRIVATE_UNDETECTED_DEFECTOR_BIT);
    d.line("d2 %s", relationshipName(objectGetRelationship(oa, oc, &w)));
    oc->m_privateStatus &= ~(1u << OBJECT_PRIVATE_UNDETECTED_DEFECTOR_BIT);
    // a teamless source: NEUTRAL regardless
    ObjectT *onull = w.createObject(sph, false, 0, {}, Coord3D{70.0f, 50.0f, 0.0f});
    d.line("d3 %s", relationshipName(objectGetRelationship(onull, oc, &w)));

    // setControllingPlayer: the prototype's owner move, the lists, the
    // member capture events, and the null refusal
    objectSetTeam(oa, sa, &w);
    dumpTeamEvents(d, w);
    sa->setControllingPlayer(c, &w);
    d.line("ctrl %d", sa->getPrototype()->m_owningPlayer->m_playerIndex);
    dumpTeamEvents(d, w);
    sa->setControllingPlayer(nullptr, &w);
    d.line("ctrlnull %d", sa->getPrototype()->m_owningPlayer->m_playerIndex);
    dumpPlayers(d, w);
    dumpTeams(d, w);
    dumpObjectTeams(d, w);
}

// --- players_masks: getPlayerFromMask, the getEachPlayerFromMask peeling,
// getPlayersWithRelationship -------------------------------
static void corpusPlayersMasks(Dumper &d)
{
    d.line("corpus players_masks");
    World &w = freshWorld(10.0f, Region2DT{0, 0, 100, 100});
    SidesSpecT sides;
    sides.sides.push_back(SideSpecT{});
    sides.sides.push_back(sideSpec("america", true, false, false, {"china"}, {"gla"}));
    sides.sides.push_back(sideSpec("china", false, false, false, {}, {}));
    sides.sides.push_back(sideSpec("gla", false, false, false, {}, {}));
    sides.sides.push_back(sideSpec("observer", false, false, true, {}, {}));
    sides.teams.push_back(teamSpec("team", "neutral", true));
    sides.teams.push_back(teamSpec("teamamerica", "america", true));
    sides.teams.push_back(teamSpec("teamchina", "china", true));
    sides.teams.push_back(teamSpec("teamgla", "gla", true));
    sides.teams.push_back(teamSpec("teamobserver", "observer", true));
    w.playerListNewGame(sides);
    dumpTeamEvents(d, w);

    // getPlayerFromMask: the hit and the two misses (an unused bit and the
    // multi-bit mask — exact equality only)
    d.line("m0 %d", w.playerListGetPlayerFromMask((PlayerMaskT)(1u << 2))->m_playerIndex);
    d.line("m1 %d %d", w.playerListGetPlayerFromMask((PlayerMaskT)(1u << 9)) ? 1 : 0,
           w.playerListGetPlayerFromMask((PlayerMaskT)0x0006) ? 1 : 0);

    // the peeling walk: index order, the mask erasure, the exhaustion
    // (mask zeroed + null)
    PlayerMaskT peel = (PlayerMaskT)0x000e; // players 1, 2, 3
    for (int i = 0; i < 5; i++) {
        PlayerT *p = w.playerListGetEachPlayerFromMask(peel);
        d.line("peel %d %d %04x", i, p ? p->m_playerIndex : -1, peel);
    }
    // a mask with bits no player carries: the exhaustion zeroes it
    PlayerMaskT dead = (PlayerMaskT)0x8000;
    PlayerT *p = w.playerListGetEachPlayerFromMask(dead);
    d.line("peeldead %d %04x", p ? p->m_playerIndex : -1, dead);

    // getPlayersWithRelationship: the selector combos, the invalid src, the
    // zero-flags early exit, the observer's dead flag (still a player with
    // a mask — relationship NEUTRAL toward everyone)
    unsigned SAME = ALLOW_SAME_PLAYER_T, AL = ALLOW_ALLIES_T, EN = ALLOW_ENEMIES_T, NE = ALLOW_NEUTRAL_T;
    d.line("g0 %04x", w.playerListGetPlayersWithRelationship(1, SAME | AL));
    d.line("g1 %04x", w.playerListGetPlayersWithRelationship(1, EN));
    d.line("g2 %04x", w.playerListGetPlayersWithRelationship(1, NE));
    d.line("g3 %04x", w.playerListGetPlayersWithRelationship(1, SAME | AL | EN | NE));
    d.line("g4 %04x", w.playerListGetPlayersWithRelationship(1, 0));
    d.line("g5 %04x", w.playerListGetPlayersWithRelationship(-1, SAME | EN));
    d.line("g6 %04x", w.playerListGetPlayersWithRelationship(99, SAME | EN));
    d.line("g7 %04x", w.playerListGetPlayersWithRelationship(0, SAME | NE));
    d.line("g8 %04x", w.playerListGetPlayersWithRelationship(4, SAME | NE | AL | EN));
    dumpPlayers(d, w);
}

// --- team_members: the setTeam paths, the member-list choreography,
// updateState's latches and thresholds, the empty-team sweep ---------
static void corpusTeamMembers(Dumper &d)
{
    d.line("corpus team_members");
    World &w = freshWorld(10.0f, Region2DT{0, 0, 200, 200});
    SidesSpecT sides;
    sides.sides.push_back(SideSpecT{});
    sides.sides.push_back(sideSpec("america", true, false, false, {"china"}, {}));
    sides.sides.push_back(sideSpec("china", false, false, false, {"america"}, {}));
    sides.sides.push_back(sideSpec("dead", false, false, false, {}, {}));

    // the squad template with every script + a 0.4 threshold
    TeamSpecT squad;
    squad.name = "americaSquad";
    squad.owner = "america";
    squad.singleton = false;
    squad.scriptOnCreate = "oc1";
    squad.scriptOnDestroyed = "od1";
    squad.scriptOnIdle = "oi1";
    squad.scriptOnEnemySighted = "oes1";
    squad.scriptOnAllClear = "oac1";
    squad.scriptOnUnitDestroyed = "oud1";
    squad.destroyedThreshold = 0.4f;
    sides.teams.push_back(teamSpec("team", "neutral", true));
    sides.teams.push_back(teamSpec("teamamerica", "america", true));
    sides.teams.push_back(teamSpec("teamchina", "china", true));
    sides.teams.push_back(teamSpec("teamdead", "dead", true));
    sides.teams.push_back(squad);
    w.playerListNewGame(sides);
    dumpTeamEvents(d, w);

    PlayerT *a = w.playerList.getNthPlayer(1);
    PlayerT *dead = w.playerList.getNthPlayer(3);
    dead->m_isPlayerDead = true; // kill the player (the isPlayerActive gate)
    TeamT *ta = w.teamFactory.findTeam("teamamerica", &w);
    TeamT *sa = w.teamFactory.createTeam("americaSquad", &w); // active, 0 members
    TeamT *td = w.teamFactory.findTeam("teamdead", &w);

    GeometryInfoT sph{GEOMETRY_SPHERE, true, 1, 1, 1};
    ObjectT *o1 = w.createObject(sph, false, 0, {}, Coord3D{50.0f, 50.0f, 0.0f});
    ObjectT *o2 = w.createObject(sph, false, 0, {}, Coord3D{52.0f, 50.0f, 0.0f});
    ObjectT *o3 = w.createObject(sph, false, 0, {}, Coord3D{54.0f, 50.0f, 0.0f});
    ObjectT *o4 = w.createObject(sph, false, 0, {}, Coord3D{56.0f, 50.0f, 0.0f});
    o1->m_visionRange = 8.0f;
    o2->m_visionRange = 8.0f;
    o3->m_visionRange = 8.0f;
    o4->m_visionRange = 8.0f;
    o1->m_hasAi = true;
    o2->m_hasAi = true;
    o3->m_hasAi = true;
    o1->m_aiIdle = true; // the onIdle latch: warm on the first update, fire on the second
    o2->m_aiIdle = true;

    // the isPlayerActive refusal: assigning to the dead player's team
    // redirects to the NEUTRAL player's default team
    objectSetTeam(o1, td, &w);
    d.line("refused o1 t%u", o1->m_team);
    dumpTeamEvents(d, w);

    // the member choreography: the prepend order, the transitions, the
    // same-team no-op, the capture on a cross-player move
    objectSetTeam(o1, sa, &w);
    objectSetTeam(o2, sa, &w);
    objectSetTeam(o2, sa, &w); // no-op (the unchanged-team guard)
    dumpTeams(d, w);
    dumpTeamEvents(d, w);

    // updateState #1: the created latch (the onCreate script, the threshold
    // compute over 2 members: 2 - 2*0.4 = 1.2 → 1), the enemy-sighted check
    // (the china default team is empty — nothing to see... wait, o4 below)
    ObjectT *o5 = w.createObject(sph, false, 0, {}, Coord3D{57.0f, 50.0f, 0.0f});
    o5->m_visionRange = 8.0f;
    TeamT *tc = w.teamFactory.findTeam("teamchina", &w);
    objectSetTeam(o5, tc, &w);
    w.pm.update(&w.hooks); // settle the store before the queries
    w.playerListUpdateTeamStates();
    dumpTeams(d, w);
    dumpTeamEvents(d, w);

    // updateState #2: the enemy sighted (o1 sees o5 at distance 7 < 8), the
    // onIdle latch warming (wasIdle false → no fire), then the all-clear
    // path after o5's death
    w.playerListUpdateTeamStates();
    dumpTeams(d, w);
    dumpTeamEvents(d, w);
    o5->m_privateStatus |= (1u << OBJECT_PRIVATE_EFFECTIVELY_DEAD_BIT);
    w.playerListUpdateTeamStates();
    dumpTeams(d, w);
    dumpTeamEvents(d, w);

    // the onDestroyed trigger: mark o1/o2 dead → curUnits 0 ≤ threshold 1
    // fires, the -1 latch refuses the second fire; notifyTeamOfObjectDeath
    // on a unit death
    o1->m_privateStatus |= (1u << OBJECT_PRIVATE_EFFECTIVELY_DEAD_BIT);
    sa->notifyTeamOfObjectDeath(&w);
    o2->m_privateStatus |= (1u << OBJECT_PRIVATE_EFFECTIVELY_DEAD_BIT);
    w.playerListUpdateTeamStates();
    w.playerListUpdateTeamStates();
    dumpTeams(d, w);
    dumpTeamEvents(d, w);

    // the detach choreography: the swap back to the default team (the
    // capture event with the players), the null detach
    objectSetTeam(o3, sa, &w);
    objectSetTeam(o3, ta, &w);
    objectSetTeam(o3, nullptr, &w);
    dumpObjectTeams(d, w);
    dumpTeamEvents(d, w);

    // the empty-team sweep: sa is active and memberless (o1/o2 dead still
    // occupy the list? no — the dead stay members; detach them first),
    // then the sweep deletes sa (non-singleton, not a default team,
    // active, empty) — with the preTeamDestroy walk, the override cleanup
    // on the remaining teams and the player relation cleanup
    objectSetTeam(o1, nullptr, &w);
    objectSetTeam(o2, nullptr, &w);
    w.playerListUpdateTeamStates();
    dumpTeams(d, w);
    dumpTeamEvents(d, w);

    // a player-owning default team is spared by the sweep (the singleton
    // refusal and the default-team refusal): the inactive non-singleton is
    // spared too
    TeamT *s2 = w.teamFactory.createInactiveTeam("americaSquad", &w);
    w.playerListUpdateTeamStates();
    dumpTeams(d, w);
    (void)s2;
    (void)a;
}

// --- players_roundtrip: the integration corpus — objects on teams, the
// filters over the player state, the local switch, validateTeam -------
static void corpusPlayersRoundtrip(Dumper &d)
{
    d.line("corpus players_roundtrip");
    World &w = freshWorld(10.0f, Region2DT{0, 0, 200, 200});
    SidesSpecT sides;
    sides.sides.push_back(SideSpecT{});
    sides.sides.push_back(sideSpec("america", true, false, false, {"china"}, {}));
    sides.sides.push_back(sideSpec("china", false, false, false, {"america"}, {}));
    sides.sides.push_back(sideSpec("gla", false, false, false, {"america", "china"}, {}));
    sides.teams.push_back(teamSpec("team", "neutral", true));
    sides.teams.push_back(teamSpec("teamamerica", "america", true));
    sides.teams.push_back(teamSpec("teamchina", "china", true));
    sides.teams.push_back(teamSpec("teamgla", "gla", true));
    sides.teams.push_back(teamSpec("americaSquad", "america", false));
    w.playerListNewGame(sides);
    dumpTeamEvents(d, w);

    TeamT *ta = w.teamFactory.findTeam("teamamerica", &w);
    TeamT *tc = w.teamFactory.findTeam("teamchina", &w);
    TeamT *tg = w.teamFactory.findTeam("teamgla", &w);
    TeamT *sa = w.teamFactory.createTeam("americaSquad", &w);

    // the objects: three american (two on the default team, one on the
    // squad), two chinese, one gla, one neutral
    GeometryInfoT sph{GEOMETRY_SPHERE, true, 1, 1, 1};
    ObjectT *objs[7];
    for (int i = 0; i < 7; i++)
        objs[i] = w.createObject(sph, false, 0, {}, Coord3D{40.0f + 4.0f * i, 50.0f, 0.0f});
    objectSetTeam(objs[0], ta, &w);
    objectSetTeam(objs[1], ta, &w);
    objectSetTeam(objs[2], sa, &w);
    objectSetTeam(objs[3], tc, &w);
    objectSetTeam(objs[4], tc, &w);
    objectSetTeam(objs[5], tg, &w);
    objectSetTeam(objs[6], w.teamFactory.findTeam("team", &w), &w);
    dumpObjectTeams(d, w);
    dumpTeamEvents(d, w);

    w.pm.update(&w.hooks); // settle the store

    // the filters over the player state: SamePlayer, Relationship
    // (each direction), AcceptOnTeam — over a range query around a1
    {
        PartitionFilterSamePlayerT samePlayer(w.playerList.getNthPlayer(1));
        PartitionFilterRelationshipT allies(objs[0], PartitionFilterRelationshipT::F_ALLOW_ALLIES);
        PartitionFilterRelationshipT enemies(objs[0], PartitionFilterRelationshipT::F_ALLOW_ENEMIES);
        PartitionFilterRelationshipT neutral(objs[0], PartitionFilterRelationshipT::F_ALLOW_NEUTRAL);
        PartitionFilterAcceptOnTeamT onTeam(sa);

        PartitionFilterT *fs1[] = { &samePlayer, nullptr };
        PartitionFilterT *fs2[] = { &allies, nullptr };
        PartitionFilterT *fs3[] = { &enemies, nullptr };
        PartitionFilterT *fs4[] = { &neutral, nullptr };
        PartitionFilterT *fs5[] = { &onTeam, nullptr };

        float dist;
        Coord3D vec;
        ObjectT *hit;

        // the same-player query: a1, a2 (a3 is on the squad team — same
        // controlling player, so it passes too)
        hit = w.pm.getClosestObject(objs[0], 100.0f, FROM_CENTER_2D, fs1, &dist, &vec);
        d.line("q same %s d=%08x", hit ? "hit" : "miss", f32bits(dist));
        // the allies-of-a1 query: america considers gla+china enemies —
        // only the same-player and neutral objects pass
        hit = w.pm.getClosestObject(objs[0], 100.0f, FROM_CENTER_2D, fs2, &dist, &vec);
        d.line("q allies %s d=%08x", hit ? "hit" : "miss", f32bits(dist));
        // the enemies query: the chinese/gla objects
        hit = w.pm.getClosestObject(objs[0], 100.0f, FROM_CENTER_2D, fs3, &dist, &vec);
        d.line("q enemies %s o%d d=%08x", hit ? "hit" : "miss", hit ? hit->m_id : -1, f32bits(dist));
        // the neutral query: the neutral object
        hit = w.pm.getClosestObject(objs[0], 100.0f, FROM_CENTER_2D, fs4, &dist, &vec);
        d.line("q neutral %s o%d d=%08x", hit ? "hit" : "miss", hit ? hit->m_id : -1, f32bits(dist));
        // the on-team query: the squad member
        hit = w.pm.getClosestObject(objs[0], 100.0f, FROM_CENTER_2D, fs5, &dist, &vec);
        d.line("q onteam %s o%d d=%08x", hit ? "hit" : "miss", hit ? hit->m_id : -1, f32bits(dist));

        // the enemy-sighted path over the squad (the team updateState query
        // with the same filters): a3 sees the chinese pair
        objs[2]->m_visionRange = 12.0f;
        w.playerListUpdateTeamStates();
        dumpTeams(d, w);
        dumpTeamEvents(d, w);
    }

    // the local switch: the transitions fire; validateTeam resolves the
    // name (auto-creating the non-singleton) and falls back to the neutral
    // default team on a miss
    w.playerListSetLocalPlayer(w.playerList.getNthPlayer(2));
    d.line("v0 %u", w.playerListValidateTeam("americaSquad")->getID());
    d.line("v1 %u", w.playerListValidateTeam("nosuchteam")->getID());
    d.line("v2 %u", w.playerListValidateTeam("teamchina")->getID());
    dumpTeamEvents(d, w);
    dumpPlayers(d, w);

    // the stale-relation reset over a populated world: the factory clear
    // walks the prototypes in key order (the map pre-emptied — only the
    // player half of teamAboutToBeDeleted runs), the player↔player
    // relations survive
    w.playerListReset();
    dumpPlayers(d, w);
    dumpTeams(d, w);
    dumpTeamEvents(d, w);
    dumpObjectTeams(d, w);
}

static void corpusRoundtrip(Dumper &d)
{
    d.line("corpus kernel_roundtrip");
    World &w = freshWorld(10.0f, Region2DT{0, 0, 100, 100});
    GeometryInfoT g{GEOMETRY_SPHERE, true, 1, 2.5f, 2.5f};
    GeometryInfoT big{GEOMETRY_BOX, false, 1, 8.0f, 6.0f};

    // a small mover with a module (waking every 2 frames)
    ObjectT *a = w.createObject(g, false, 1, {}, Coord3D{51.0f, 51.0f, 0.0f}, 0.0f);
    a->m_updates[0].m_scriptedSleep = 2;
    // a big rotated immobile structure
    w.createObject(big, true, 0, {}, Coord3D{35.0f, 35.0f, 0.0f}, 0.4f);
    // a far mover with a module (every frame)
    ObjectT *c = w.createObject(g, false, 1, {}, Coord3D{90.0f, 12.0f, 0.0f}, 0.0f);
    c->m_updates[0].m_scriptedSleep = 1;

    for (int f = 0; f < 5; ++f) {
        // move a little each frame
        Coord3D old = a->m_pos;
        a->m_pos = {a->m_pos.x + 3.0f, a->m_pos.y + 1.0f, 0.0f};
        w.reactToTransformChange(a, old, a->m_angle);
        w.preUpdate();
        w.update();
        dumpEvents(d, w);
        dumpContacts(d, w);
    }
    dumpHeap(d, w);
    dumpRegistry(d, w);
    dumpPartition(d, w);

    // destroy c mid-run (its module's heap entry leaves via the two-pass)
    w.logic.destroyObject(c, &w.hooks);
    w.update();
    dumpEvents(d, w);
    dumpHeap(d, w);
    dumpRegistry(d, w);
    d.line("end");
}

// --- query_radiusvec: the FASTER_GCO ring precompute (calcMinRadius's
// corner-pair minimum with the corner-touch and edge-share radii, the
// maxGcoRadius diagonal bound, the y-outer/x-inner offset scan order) --
static void dumpRadiusVec(Dumper &d, World &w)
{
    d.line("maxgco %d rings %d", w.pm.m_maxGcoRadius, (int)w.pm.m_radiusVec.size());
    for (size_t r = 0; r < w.pm.m_radiusVec.size(); ++r) {
        const OffsetVecT &v = w.pm.m_radiusVec[r];
        d.line("ring %d n %d", (int)r, (int)v.size());
        for (size_t i = 0; i < v.size(); ++i)
            d.line("o %d,%d", v[i].x, v[i].y);
    }
}

static void corpusQueryRadiusVec(Dumper &d)
{
    d.line("corpus query_radiusvec");
    {
        // (a) the 1x1-cell world: the whole offset space is the single
        // cell (corner-touch); rings 1..2 exist but stay empty
        World &w = freshWorld(10.0f, Region2DT{0, 0, 10, 10});
        d.line("a cells %d,%d", w.pm.m_cellCountX, w.pm.m_cellCountY);
        dumpRadiusVec(d, w);
        // calcMinRadius probes: the same cell, the orthogonal neighbors
        // (edge share), the diagonal neighbors (corner touch), and the
        // beyond-diagonal offsets
        ICoord2DT probes[] = {{0, 0}, {1, 0}, {0, 1}, {-1, 0}, {0, -1},
                              {1, 1}, {-1, 1}, {1, -1}, {-1, -1},
                              {2, 0}, {2, 1}, {2, 2}, {3, 2}, {-2, 1}, {5, 7}};
        for (auto &p : probes)
            d.line("minr %d,%d %d", p.x, p.y, w.pm.calcMinRadius(p));
    }
    {
        // (b) a non-square fractional-cell grid (the f32 CEIL drift of
        // the count math rides along, pinned by both sides computing it)
        World &w = freshWorld(3.5f, Region2DT{0, 0, 21, 10.5f});
        d.line("b cells %d,%d", w.pm.m_cellCountX, w.pm.m_cellCountY);
        dumpRadiusVec(d, w);
    }
    {
        // (c) a mid-size square grid: the ring counts only (the ring
        // sum is the offset space) plus the first/last offsets of the
        // early rings — the scan order the walk takes
        World &w = freshWorld(10.0f, Region2DT{0, 0, 100, 100});
        d.line("c cells %d,%d", w.pm.m_cellCountX, w.pm.m_cellCountY);
        d.line("c maxgco %d rings %d", w.pm.m_maxGcoRadius, (int)w.pm.m_radiusVec.size());
        for (size_t r = 0; r < w.pm.m_radiusVec.size(); ++r)
            d.line("ring %d n %d", (int)r, (int)w.pm.m_radiusVec[r].size());
        for (int r = 1; r <= 3; ++r) {
            const OffsetVecT &v = w.pm.m_radiusVec[(size_t)r];
            for (size_t i = 0; i < v.size() && i < 4; ++i)
                d.line("r%d head %d,%d", r, v[i].x, v[i].y);
            for (size_t i = v.size() > 3 ? v.size() - 3 : 0; i < v.size(); ++i)
                d.line("r%d tail %d,%d", r, v[i].x, v[i].y);
        }
    }
    d.line("end");
}

// --- query_closest: getClosestObjects — the ring walk's early exit, the
// strict < boundary (the exact-distance reject and the no-match prefill),
// the same-cell tie (first walked wins), the boundary measures (the
// clamped-zero and the shrunk-vector cases), the filters, and the
// measure helpers --
static void corpusQueryClosest(Dumper &d)
{
    d.line("corpus query_closest");
    World &w = freshWorld(10.0f, Region2DT{0, 0, 100, 100});
    GeometryInfoT sph{GEOMETRY_SPHERE, true, 1.0f, 1.0f, 1.0f};
    GeometryInfoT cyl3{GEOMETRY_CYLINDER, false, 4.0f, 3.0f, 3.0f};
    GeometryInfoT cyl10{GEOMETRY_CYLINDER, false, 4.0f, 10.0f, 10.0f};

    ObjectT *a = w.createObject(sph, false, 0, {}, Coord3D{50.0f, 50.0f, 0.0f});
    ObjectT *b = w.createObject(sph, false, 0, {}, Coord3D{65.0f, 50.0f, 0.0f});
    w.createObject(sph, false, 0, {}, Coord3D{50.0f, 35.0f, 0.0f}); // c: 15 south of a
    w.createObject(sph, false, 0, {}, Coord3D{60.0f, 50.0f, 0.0f}); // d: 10 east of a, ring 1
    ObjectT *e = w.createObject(sph, false, 0, {}, Coord3D{85.0f, 85.0f, 0.0f});
    // the boundary-measure cylinders, kept away from the center so the
    // center queries stay unpolluted: m1 the r=3 pair partner, m2 the
    // r=10 overlap clamper, m3 the z-stepped 3d target
    ObjectT *m1 = w.createObject(cyl3, false, 0, {}, Coord3D{76.0f, 12.0f, 0.0f});
    ObjectT *m2 = w.createObject(cyl10, false, 0, {}, Coord3D{56.0f, 12.0f, 0.0f});
    ObjectT *m3 = w.createObject(cyl10, false, 0, {}, Coord3D{70.0f, 50.0f, 5.0f});
    // the same-cell equidistant pair (both exactly 5 from the query
    // position, both in cell (3,2) — the COI-list order decides)
    w.createObject(sph, false, 0, {}, Coord3D{30.0f, 30.0f, 0.0f}); // s1 (registered first)
    w.createObject(sph, false, 0, {}, Coord3D{30.0f, 20.0f, 0.0f}); // s2 (prepended: walked first)
    // the filter objects: f1 dies, e leaves the map, b carries an
    // anonymous kindof bit (bit 5 — the bit's meaning is the data
    // layer's; the filter semantics is what is pinned)
    ObjectT *f1 = w.createObject(sph, false, 0, {}, Coord3D{42.0f, 42.0f, 0.0f});
    f1->m_privateStatus |= (1u << OBJECT_PRIVATE_EFFECTIVELY_DEAD_BIT);
    e->m_privateStatus |= (1u << OBJECT_PRIVATE_OFF_MAP_BIT);
    b->m_kindof |= (1u << 5);

    // the store refresh: the COI coverage the queries walk
    w.pm.update(&w.hooks);

    // (a) from-object, wide: d wins at 10 — found in ring 1 (its cell is
    // the orthogonal neighbor), so the walk stops after ring 1 and the
    // farther b/c/e are never even walked
    {
        float dist;
        Coord3D vec;
        ObjectT *o = w.pm.getClosestObject(a, 100.0f, FROM_CENTER_2D, nullptr, &dist, &vec);
        d.line("a o%d dist=%08x vec=%08x,%08x,%08x", o ? o->m_id : INVALID_ID, bits_of(dist),
               bits_of(vec.x), bits_of(vec.y), bits_of(vec.z));
    }
    // (b) from-position at a's spot: no self-exclusion — a itself wins at
    // distance zero (replacing m1's earlier ring-0 find within the walk)
    {
        float dist;
        Coord3D vec;
        Coord3D pos{50.0f, 50.0f, 0.0f};
        ObjectT *o = w.pm.getClosestObject(&pos, 100.0f, FROM_CENTER_2D, nullptr, &dist, &vec);
        d.line("b o%d dist=%08x vec=%08x,%08x,%08x", o ? o->m_id : INVALID_ID, bits_of(dist),
               bits_of(vec.x), bits_of(vec.y), bits_of(vec.z));
    }
    // (c) the strict boundary: from (55,50) both a (5 west) and d (5
    // east) are exactly maxDist away — 25 < 25 fails for both, so no
    // match; the outputs carry the sqrt of the initial bound and the
    // non-retail prefill of maxDist^3
    {
        float dist = -1.0f;
        Coord3D vec;
        Coord3D pos{55.0f, 50.0f, 0.0f};
        ObjectT *o = w.pm.getClosestObject(&pos, 5.0f, FROM_CENTER_2D, nullptr, &dist, &vec);
        d.line("c o%d dist=%08x vec=%08x,%08x,%08x", o ? o->m_id : INVALID_ID, bits_of(dist),
               bits_of(vec.x), bits_of(vec.y), bits_of(vec.z));
    }
    // (d) the same-cell tie: from (30,25) s1 and s2 are both exactly 5
    // away in cell (3,2); the COI list is newest-first, so s2 is walked
    // first and wins — s1 then fails the strict < against the tightened
    // bound
    {
        float dist;
        Coord3D vec;
        Coord3D pos{30.0f, 25.0f, 0.0f};
        ObjectT *o = w.pm.getClosestObject(&pos, 10.0f, FROM_CENTER_2D, nullptr, &dist, &vec);
        d.line("d o%d dist=%08x vec=%08x,%08x,%08x", o ? o->m_id : INVALID_ID, bits_of(dist),
               bits_of(vec.x), bits_of(vec.y), bits_of(vec.z));
    }
    // (e) the boundary-2d measure, the shrunk case: from the position
    // (70,12) m1's r=3 cylinder sits 6 away — the position side
    // contributes no radius, so the 3 shrinks the distance to 3 and the
    // vector by half (m2's 14-away r=10 pair shrinks to 4 — farther)
    {
        float dist;
        Coord3D vec;
        Coord3D pos{70.0f, 12.0f, 0.0f};
        ObjectT *o = w.pm.getClosestObject(&pos, 20.0f, FROM_BOUNDINGSPHERE_2D, nullptr, &dist, &vec);
        d.line("e o%d dist=%08x vec=%08x,%08x,%08x", o ? o->m_id : INVALID_ID, bits_of(dist),
               bits_of(vec.x), bits_of(vec.y), bits_of(vec.z));
    }
    // (f) the boundary-2d measure, the clamped case: from (56,20) the
    // r=10 cylinder m2 is 8 away — well inside the 11 of combined radii
    // — so the distance clamps to zero and the vector scales by the
    // zero factor (the -0.0 of the southward y is bit-visible)
    {
        float dist;
        Coord3D vec;
        Coord3D pos{56.0f, 20.0f, 0.0f};
        ObjectT *o = w.pm.getClosestObject(&pos, 12.0f, FROM_BOUNDINGSPHERE_2D, nullptr, &dist, &vec);
        d.line("f o%d dist=%08x vec=%08x,%08x,%08x", o ? o->m_id : INVALID_ID, bits_of(dist),
               bits_of(vec.x), bits_of(vec.y), bits_of(vec.z));
    }
    // (g) the boundary-3d measure: from a the d pair (two r=1 spheres
    // 10 apart) shrinks to 8 — beating m3's 11.2 (the z=5 cylinder's
    // 21.2 diagonal minus the 11 of combined spheres; its z-delta path
    // is pinned by the measure helpers below)
    {
        float dist;
        Coord3D vec;
        ObjectT *o = w.pm.getClosestObject(a, 100.0f, FROM_BOUNDINGSPHERE_3D, nullptr, &dist, &vec);
        d.line("g o%d dist=%08x vec=%08x,%08x,%08x", o ? o->m_id : INVALID_ID, bits_of(dist),
               bits_of(vec.x), bits_of(vec.y), bits_of(vec.z));
    }
    // (h) the filters: the dead f1 is refused by Alive (and the raw
    // query finds it at zero), the off-map e by OnMap, everyone but b by
    // the kindof accept, b by the kindof reject, and everyone but e by
    // the same-map-status view from e
    {
        Coord3D pos{42.0f, 42.0f, 0.0f};
        float dist;
        ObjectT *o = w.pm.getClosestObject(&pos, 6.0f, FROM_CENTER_2D, nullptr, &dist, nullptr);
        d.line("h raw o%d", o ? o->m_id : INVALID_ID);
        PartitionFilterAliveT alive;
        PartitionFilterT *fAlive[] = {&alive, nullptr};
        o = w.pm.getClosestObject(&pos, 6.0f, FROM_CENTER_2D, fAlive, &dist, nullptr);
        d.line("h alive o%d", o ? o->m_id : INVALID_ID);
        PartitionFilterOnMapT onMap;
        PartitionFilterT *fOnMap[] = {&onMap, nullptr};
        Coord3D pos2{80.0f, 80.0f, 0.0f};
        o = w.pm.getClosestObject(&pos2, 100.0f, FROM_CENTER_2D, fOnMap, &dist, nullptr);
        d.line("h onmap o%d", o ? o->m_id : INVALID_ID);
        PartitionFilterAcceptByKindOfT accept((1u << 5), 0u);
        PartitionFilterT *fAccept[] = {&accept, nullptr};
        Coord3D pos3{65.0f, 50.0f, 0.0f};
        o = w.pm.getClosestObject(&pos3, 100.0f, FROM_CENTER_2D, fAccept, &dist, nullptr);
        d.line("h accept o%d", o ? o->m_id : INVALID_ID);
        PartitionFilterRejectByKindOfT reject((1u << 5), 0u);
        PartitionFilterT *fReject[] = {&reject, nullptr};
        o = w.pm.getClosestObject(&pos3, 100.0f, FROM_CENTER_2D, fReject, &dist, nullptr);
        d.line("h reject o%d", o ? o->m_id : INVALID_ID);
        PartitionFilterSameMapStatusT sameMap(e);
        PartitionFilterT *fSame[] = {&sameMap, nullptr};
        Coord3D pos4{84.0f, 84.0f, 0.0f};
        o = w.pm.getClosestObject(&pos4, 100.0f, FROM_CENTER_2D, fSame, &dist, nullptr);
        d.line("h samemap o%d", o ? o->m_id : INVALID_ID);
    }
    // (i) the measure helpers over all four dc types: the vector, the
    // distance and the goal distance to the z-stepped cylinder, and the
    // relative angles (the four quadrants, the coincident early-out and
    // a rotated source)
    {
        for (int dc = FROM_CENTER_2D; dc < DISTANCE_CALCULATION_TYPE_COUNT; ++dc) {
            Coord3D vec;
            w.pm.getVectorTo(a, m3, (DistanceCalculationType)dc, vec);
            d.line("i vec dc%d =%08x,%08x,%08x", dc, bits_of(vec.x), bits_of(vec.y), bits_of(vec.z));
            Coord3D vec2;
            float ds = w.pm.getDistanceSquared(a, m3, (DistanceCalculationType)dc, &vec2);
            d.line("i ds dc%d %08x =%08x,%08x,%08x", dc, bits_of(ds), bits_of(vec2.x), bits_of(vec2.y),
                   bits_of(vec2.z));
            Coord3D pos{70.0f, 50.0f, 5.0f};
            float ds2 = w.pm.getDistanceSquared(a, &pos, (DistanceCalculationType)dc, nullptr);
            d.line("i dsp dc%d %08x", dc, bits_of(ds2));
            Coord3D goal{60.0f, 60.0f, 0.0f};
            Coord3D vec3;
            float gs = w.pm.getGoalDistanceSquared(a, &goal, m3, (DistanceCalculationType)dc, &vec3);
            d.line("i gs dc%d %08x =%08x,%08x,%08x", dc, bits_of(gs), bits_of(vec3.x), bits_of(vec3.y),
                   bits_of(vec3.z));
            float gs2 = w.pm.getGoalDistanceSquared(a, &goal, &pos, (DistanceCalculationType)dc, nullptr);
            d.line("i gsp dc%d %08x", dc, bits_of(gs2));
        }
        ObjectT *rot = w.createObject(sph, false, 0, {}, Coord3D{30.0f, 30.0f, 0.0f}, 1.5707963f);
        struct
        {
            ObjectT *src;
            Coord3D pos;
            const char *tag;
        } angleQueries[] = {
            {a, {60.0f, 50.0f, 0.0f}, "east"},
            {a, {50.0f, 60.0f, 0.0f}, "north"},
            {a, {50.0f, 40.0f, 0.0f}, "south"},
            {a, {40.0f, 50.0f, 0.0f}, "west"},
            {a, {50.0f, 50.0f, 0.0f}, "same"},
            {rot, {40.0f, 30.0f, 0.0f}, "roteast"},
            {rot, {30.0f, 40.0f, 0.0f}, "rotnorth"},
        };
        for (auto &q : angleQueries) {
            d.line("i ang %s %08x", q.tag, bits_of(w.pm.getRelativeAngle2D(q.src, &q.pos)));
            d.line("i ango %s %08x", q.tag, bits_of(w.pm.getRelativeAngle2D(q.src, a)));
        }
    }
    (void)m1;
    (void)m2;
    d.line("end");
}

// --- query_iterate: iterateObjectsInRange over the SimpleObjectIterator
// (the FASTEST insertion order, the mergesort's tie stability in all four
// sorted orders, the pre-reset empty quirk, reset), iterateAllObjects
// (the master-list order and the filter set) and iteratePotentialCollisions
// (the 1.1 slop gate vs the would-collide filter, the 2d/3d numeric split) --
static void drainIter(Dumper &d, World &w, SimpleObjectIteratorT &iter, const char *tag)
{
    (void)w; // (the World is the cost compares' table; the drain itself only reads the iterator)
    // the fresh iterator's cursor is null — the original's callers must
    // first()/reset() before draining (sorted orders return post-sort
    // cursors; FASTEST does not — the quirk the dump pins)
    float num;
    int pre = iter.nextWithNumeric(&num);
    d.line("%s pre %d %08x", tag, pre, bits_of(num));
    iter.reset();
    int count = 0;
    for (int o = iter.nextWithNumeric(&num); o != INVALID_ID; o = iter.nextWithNumeric(&num)) {
        d.line("%s n o%d %08x", tag, o, bits_of(num));
        ++count;
    }
    d.line("%s count %d", tag, count);
}

static void corpusQueryIterate(Dumper &d)
{
    d.line("corpus query_iterate");
    {
        World &w = freshWorld(10.0f, Region2DT{0, 0, 100, 100});
        GeometryInfoT sph{GEOMETRY_SPHERE, true, 1.0f, 1.0f, 1.0f};
        GeometryInfoT big{GEOMETRY_SPHERE, false, 1.0f, 25.0f, 25.0f};

        // the cluster: a at the center, b/c/d/e at 12 on the axes (the
        // four-way tie), f the big multi-cell sphere (the done-flag
        // dedup), g far out, h the kindof carrier; e dead, g off-map
        ObjectT *a = w.createObject(sph, false, 0, {}, Coord3D{50.0f, 50.0f, 0.0f});
        ObjectT *b = w.createObject(sph, false, 0, {}, Coord3D{50.0f, 62.0f, 0.0f});
        ObjectT *c = w.createObject(sph, false, 0, {}, Coord3D{62.0f, 50.0f, 0.0f});
        ObjectT *dd = w.createObject(sph, false, 0, {}, Coord3D{38.0f, 50.0f, 0.0f});
        ObjectT *e = w.createObject(sph, false, 0, {}, Coord3D{50.0f, 38.0f, 0.0f});
        ObjectT *f = w.createObject(big, false, 0, {}, Coord3D{55.0f, 55.0f, 0.0f});
        ObjectT *g = w.createObject(sph, false, 0, {}, Coord3D{95.0f, 95.0f, 0.0f});
        ObjectT *h = w.createObject(sph, false, 0, {}, Coord3D{30.0f, 70.0f, 0.0f});
        // the corpus programs the template-supplied inputs post-creation
        a->m_buildCost = 1000.0f;
        b->m_buildCost = 100.0f;
        c->m_buildCost = 50.0f;
        dd->m_buildCost = 100.0f;
        e->m_buildCost = 25.0f;
        f->m_buildCost = 500.0f;
        g->m_buildCost = 10.0f;
        h->m_buildCost = 50.0f;
        e->m_privateStatus |= (1u << OBJECT_PRIVATE_EFFECTIVELY_DEAD_BIT);
        g->m_privateStatus |= (1u << OBJECT_PRIVATE_OFF_MAP_BIT);
        h->m_kindof |= (1u << 5);

        w.pm.update(&w.hooks);

        Coord3D center{50.0f, 50.0f, 0.0f};

        // (a) FASTEST: the ring-walk insertion order, numerics attached
        {
            SimpleObjectIteratorT it = w.pm.iterateObjectsInRange(&center, 20.0f, FROM_CENTER_2D, nullptr, ITER_FASTEST);
            drainIter(d, w, it, "a fastest");
        }
        // (b) NEAR_TO_FAR: the sorted drain with the tie order (a 0,
        // f 50, then the four at 144 in the mergesort's stable order)
        {
            SimpleObjectIteratorT it =
                w.pm.iterateObjectsInRange(&center, 20.0f, FROM_CENTER_2D, nullptr, ITER_SORTED_NEAR_TO_FAR);
            drainIter(d, w, it, "b near");
        }
        // (c) FAR_TO_NEAR
        {
            SimpleObjectIteratorT it =
                w.pm.iterateObjectsInRange(&center, 20.0f, FROM_CENTER_2D, nullptr, ITER_SORTED_FAR_TO_NEAR);
            drainIter(d, w, it, "c far");
        }
        // (d) the cost orders over the same cluster
        {
            SimpleObjectIteratorT it = w.pm.iterateObjectsInRange(&center, 20.0f, FROM_CENTER_2D, nullptr,
                                                                  ITER_SORTED_CHEAP_TO_EXPENSIVE);
            drainIter(d, w, it, "d cheap");
        }
        {
            SimpleObjectIteratorT it = w.pm.iterateObjectsInRange(&center, 20.0f, FROM_CENTER_2D, nullptr,
                                                                  ITER_SORTED_EXPENSIVE_TO_CHEAP);
            drainIter(d, w, it, "d exp");
        }
        // (e) the no-clamp branch: maxDist >= HUGE_DIST walks every ring
        // of the map — the whole cluster with its distances (the
        // master-list-free ring walk reaches everything)
        {
            SimpleObjectIteratorT it =
                w.pm.iterateObjectsInRange(&center, 1000000.0f, FROM_CENTER_2D, nullptr, ITER_FASTEST);
            drainIter(d, w, it, "e huge");
        }
        // (f) iterateAllObjects: the master-list prepend order (newest
        // first), then the filter refusals over the same walk
        {
            SimpleObjectIteratorT it = w.pm.iterateAllObjects(nullptr);
            drainIter(d, w, it, "f all");
        }
        {
            PartitionFilterOnMapT onMap;
            PartitionFilterAliveT alive;
            PartitionFilterT *filters[] = {&onMap, &alive, nullptr};
            SimpleObjectIteratorT it = w.pm.iterateAllObjects(filters);
            drainIter(d, w, it, "f onmapalive");
        }
        {
            PartitionFilterAcceptByKindOfT accept((1u << 5), 0u);
            PartitionFilterT *filters[] = {&accept, nullptr};
            SimpleObjectIteratorT it = w.pm.iterateAllObjects(filters);
            drainIter(d, w, it, "f accept");
        }
        {
            PartitionFilterRejectByKindOfT reject((1u << 5), 0u);
            PartitionFilterT *filters[] = {&reject, nullptr};
            SimpleObjectIteratorT it = w.pm.iterateAllObjects(filters);
            drainIter(d, w, it, "f reject");
        }
        // (g) the from-object form: a excludes itself, everything else
        // within 20 stays
        {
            SimpleObjectIteratorT it = w.pm.iterateObjectsInRange(a, 20.0f, FROM_CENTER_2D, nullptr, ITER_FASTEST);
            drainIter(d, w, it, "g fromobj");
        }
        // (h) the boundary measure in the range query: the 2d sphere
        // radii pull the far-apart big sphere in (its boundary touches
        // the center's)
        {
            SimpleObjectIteratorT it =
                w.pm.iterateObjectsInRange(&center, 30.0f, FROM_BOUNDINGSPHERE_2D, nullptr, ITER_SORTED_NEAR_TO_FAR);
            drainIter(d, w, it, "h bnd");
        }
    }
    {
        // (i) iteratePotentialCollisions: the small rotated box against a
        // z-stepped cluster — the 1.1-slop dist gate (the far sphere
        // never reaches the filter; the at-gate sphere reaches it and is
        // refused), the would-collide pass at z=3.9 with the 2d/3d
        // numeric split, and the centered overlap
        World &w = freshWorld(10.0f, Region2DT{0, 0, 100, 100});
        GeometryInfoT sph{GEOMETRY_SPHERE, true, 1.0f, 1.0f, 1.0f};
        GeometryInfoT box{GEOMETRY_BOX, false, 4.0f, 3.0f, 2.0f};

        ObjectT *a = w.createObject(sph, false, 0, {}, Coord3D{50.0f, 50.0f, 0.0f});
        w.createObject(sph, false, 0, {}, Coord3D{53.0f, 50.0f, 3.9f});   // k1: the z-stepped overlap
        w.createObject(sph, false, 0, {}, Coord3D{56.0f, 50.0f, 0.0f});   // k2: past the box edge — filter refuses
        w.createObject(sph, false, 0, {}, Coord3D{60.0f, 50.0f, 0.0f});   // k3: past the slop gate — never filtered
        (void)a;

        w.pm.update(&w.hooks);

        Coord3D pos{50.0f, 50.0f, 0.0f};
        {
            SimpleObjectIteratorT it = w.pm.iteratePotentialCollisions(&pos, box, 0.3f, true);
            drainIter(d, w, it, "i 2d");
        }
        {
            SimpleObjectIteratorT it = w.pm.iteratePotentialCollisions(&pos, box, 0.3f, false);
            drainIter(d, w, it, "i 3d");
        }
    }
    d.line("end");
}


int main(int argc, char **argv)
{
    if (argc != 2) {
        fprintf(stderr, "usage: %s <golden-out.txt>\n", argv[0]);
        return 1;
    }
    FILE *out = fopen(argv[1], "w");
    if (!out) {
        fprintf(stderr, "error: cannot open %s\n", argv[1]);
        return 1;
    }
    Dumper d{out};

    corpusIds(d);
    corpusFrame(d);
    corpusSchedBasic(d);
    corpusSchedTies(d);
    corpusSchedPhases(d);
    corpusSchedAwaken(d);
    corpusSchedDisabled(d);
    corpusSchedDestroy(d);
    corpusSchedDestroyCap(d);
    corpusSchedLoadpost(d);
    corpusPartGrid(d);
    corpusPartMaxCoi(d);
    corpusPartSmall(d);
    corpusPartCircle(d);
    corpusPartRect(d);
    corpusPartDirty(d);
    corpusPartContact(d);
    corpusPartRegister(d);
    corpusCollideMatrix(d);
    corpusCollideProcess(d);
    corpusQueryRadiusVec(d);
    corpusQueryClosest(d);
    corpusQueryIterate(d);
    corpusPlayersLifecycle(d);
    corpusPlayersNewGame(d);
    corpusTeamsFactory(d);
    corpusRelationships(d);
    corpusPlayersMasks(d);
    corpusTeamMembers(d);
    corpusPlayersRoundtrip(d);
    corpusRoundtrip(d);

    fclose(out);
    return 0;
}
