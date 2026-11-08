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
//     / getMaxHeightBelowPosition / setMajorRadius / setMinorRadius.
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
//   not transcribed: no path of the collide family reads the bounding
//   circle/sphere radii, so setMajorRadius/setMinorRadius write their
//   field only (stated; the bounding radii are a later slice's
//   observable). The dead five-argument forward declaration of
//   xy_collideTest_Circle_Circle (declared, never defined, never called)
//   is not transcribed.
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
#include <vector>

// The pinned GameMath sources (TheSuperHackers/GameMath @ 59f7ccd4)
// supply the deterministic trig (USE_DETERMINISTIC_MATH = 1 in
// Lib/BaseDefines.h, always on for this code base).
extern "C" {
float gm_sinf(float);
float gm_cosf(float);
float gm_sqrtf(float);
float gm_fabsf(float);
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

// GeometryInfo's collide-relevant surface (Geometry.h / Geometry.cpp).
// The bounding-circle/sphere radius recalculation (calcBoundingStuff) is
// not modelled: no path of the collide family reads the bounding radii,
// so setMajorRadius/setMinorRadius write the field only (the original
// also recomputes m_boundingCircleRadius/m_boundingSphereRadius — stated
// deviation, identical observables).
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
        // (calcBoundingStuff — the bounding radii this family never reads)
    }
    void setMinorRadius(float r)
    {
        minorRadius = r;
        // (calcBoundingStuff — the bounding radii this family never reads)
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
    DisabledMask m_disabledMask;
    int m_disabledTillFrame[DISABLED_COUNT] = {0};
    std::vector<UpdateModuleT> m_updates;
    std::vector<CollideModuleT> m_colliders; // the behavior list's collide modules (onCollide's walk)
    int m_partitionData = INVALID_ID; // the object-id handle of the PartitionData
    int m_cellChangeCount = 0;        // onPartitionCellChange() invocations

    bool isKindOfImmobile() const { return (m_kindof & (1u << KINDOF_IMMOBILE_BIT)) != 0; }
    bool isKindOfNoCollide() const { return (m_kindof & (1u << KINDOF_NO_COLLIDE_BIT)) != 0; }
    bool isDestroyed() const { return (m_status & (1u << OBJECT_STATUS_DESTROYED_BIT)) != 0; }
    bool isNoCollisions() const { return (m_status & (1u << OBJECT_STATUS_NO_COLLISIONS_BIT)) != 0; }
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
    }

    void shutdown()
    {
        m_updatedSinceLastReset = false;
        removeAllDirtyModules();

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
// The World: the driver's singletons (TheGameLogic / ThePartitionManager /
// the object table), the scripted module bodies and the corpus commands.
// ============================================================================
struct World {
    GameLogicT logic;
    PartitionManagerT pm;
    GameLogicHooks hooks;
    std::vector<std::unique_ptr<ObjectT>> objects; // the arena (ids never reused)

    World() { pm.m_world = this; }

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

// ============================================================================
// main.
// ============================================================================
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
    corpusRoundtrip(d);

    fclose(out);
    return 0;
}
