// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Ported from the Zero Hour source release (baseline 571a7c5):
//   Core/GameEngineDevice/Source/W3DDevice/GameClient/WorldHeightMap.cpp
//     (WorldHeightMap::ParseObjectsDataChunk — the ObjectsList container
//     reader the constructor's logical-data-only load registers, with
//     its m_currentObject link-cursor reset and its nested Object
//     parser registration; WorldHeightMap::ParseObjectDataChunk's
//     readDict dispatch; WorldHeightMap::ParseObjectData — the Object
//     chunk reader with the version-gated z zeroing, the z-range drop,
//     the MapObject construction and the three well-known property
//     derivations; MapObject's constructor — validateName, the
//     normalizeAngle wrap, the property-sheet copy; and the
//     constructor's registerParser/parse walk shape)
//   Core/GameEngine/Include/Common/MapObject.h
//     (MAP_XY_FACTOR, MAP_HEIGHT_SCALE, the m_flags bit values, the
//     saved members m_location/m_objectName/m_angle/m_flags/m_properties)
//   Core/GameEngine/Include/Common/MapReaderWriterInfo.h
//     (K_OBJECTS_VERSION_1..3 — the version words and their gates)
//   Core/GameEngine/Source/Common/System/GameCommon.cpp
//     (normalizeAngle — the NaN early-out and the double-precision
//     wrap loops)
//   GeneralsMD/Code/GameEngine/Include/Common/WellKnownKeys.h
//     (the TheKey_waypointID/TheKey_lightHeightAboveTerrain/
//     TheKey_scorchType startup keys the derivations consult)
//   GeneralsMD/Code/Tools/WorldBuilder/src/WHeightMapEdit.cpp
//     (the writer this reader mirrors — the cross-check reference)
//
// See the module docs for the format layout and the documented differences
// from the C++ (the linked list becomes the vector of reachable objects;
// the null-this m_userData quirk; the infinite-angle refusal; the
// well-known-key seeding).

//! The **map objects-list payload** — the `ObjectsList` chunk of a `.map`
//! file, on top of [`crate::map_chunk`].
//!
//! A Zero Hour map's placed content is one `ObjectsList` chunk at the top
//! level of the data-chunk tree, carrying one nested `Object` chunk per
//! placement: the location, the angle, the editor flags, the object's
//! type name and its property sheet (a dict — the health, ownership,
//! waypoint and light properties the game and the WorldBuilder both
//! drive). `WorldHeightMap`'s constructor registers a reader for it in
//! the **logical-data-only** load only (the full load never looks at
//! it), and this module ports that reader over the
//! [`DataChunkInput`](crate::map_chunk::DataChunkInput) walk, quirks
//! included (parity level P0, pinned by golden vectors generated from
//! the transcription in `tools/differential/gen_mapobjects_golden.cpp`).
//!
//! # The chunks in one page
//!
//! ```text
//! ObjectsList chunk (any version; the reader never reads its own)
//!   Object chunk × n
//!     3 × f32     the location (x, y, z — the z is zeroed for v <= 2)
//!     f32         the angle (normalized into (-π, π] on construction)
//!     i32         the m_flags bit word
//!     str16       the object's type name
//!     [v >= 2] dict  the property sheet
//! ```
//!
//! # The load-path quirks (transcribed)
//!
//! - The **version gates hang off the `Object` chunk's own word**, not
//!   the list's: the dict exists from [`K_OBJECTS_VERSION_2`] and the z
//!   zeroing runs at `version <= 2` — a version-2 object reads its dict
//!   and still drops its z, a version-3 one keeps both, a version 0
//!   reads like a 1 and a version 4 like a 3. An `ObjectsList` chunk's
//!   version word is never read.
//! - The **z-range contract** drops objects whole: outside
//!   [`MIN_OBJECT_Z`]..[`MAX_OBJECT_Z`] (the world's floor and the
//!   height ceiling scaled by [`MAP_HEIGHT_SCALE`]) the object is
//!   silently removed — the original's `DEBUG_LOG` is its only trace —
//!   and because version 2 and lower zero the z first, those objects
//!   never drop.
//! - The stored angle is **`normalizeAngle`'s** (the constructor's), not
//!   the file's: wrapped into (-π, π] by loops that compare and step in
//!   **double precision** and narrow back to `f32` per iteration (a
//!   `(float)π` angle sits above the double π and wraps negative; its
//!   negation wraps back positive), with NaN an early `0` (the
//!   original's "ARGH!!!!" release path).
//! - The **linkage is head-overwrite, never clear**: each `ObjectsList`
//!   chunk resets the link cursor (`m_currentObject`), and its first
//!   *kept* object becomes the new list head — whatever the previous
//!   chunks loaded is orphaned (leaked, unreachable in the original).
//!   A later chunk whose objects all drop (or that carries none) never
//!   writes the head, so the earlier list survives it. A map with no
//!   kept object at all ends with the null head — an empty list here.
//! - The three **well-known derivations** are exact-type matches on the
//!   property sheet: `waypointID` as `INT` marks a waypoint,
//!   `lightHeightAboveTerrain` as `REAL` a light, `scorchType` as
//!   `INT` a scorch — a `waypointID` stored as a `REAL` derives
//!   nothing.
//! - The property sheet's stored order is the **name-key interning
//!   order**, not the file's pair order: the dict sorts its pairs by
//!   the key table's ordinals, and this walk seeds the three
//!   well-known keys before the first dict read (see below).
//! - An `Object` chunk at the **global scope** (or inside any chunk
//!   other than an `ObjectsList`) never fires — the registration is
//!   `("Object", "ObjectsList")` — and the walk skips it whole.
//! - The reader never checks it consumed the chunk (the `atEndOfChunk`
//!   assert is debug-only); leftovers go to the walk's close, and a
//!   dict read past a short file mints zero-filled `keyAndType` words
//!   as empty-name `BOOL` pairs (the container's pinned stance).
//!
//! # Differences from the C++ (documented)
//!
//! - The linked list (`TheMapObjectListPtr` + `m_nextMapObject`)
//!   becomes the vector of **reachable** objects: the orphaned lists
//!   of overwritten heads were unreachable in the original (leaked
//!   memory), so clearing on the first kept object of a later chunk is
//!   the same observable. The drops are silent in both (their
//!   `DEBUG_LOG` is the original's only trace).
//! - `ParseObjectDataChunk` reads its `WorldHeightMap*` through
//!   `file.m_userData` — a hook **nothing on the load path ever sets**
//!   (the constructor leaves it null), and `ParseObjectData` never
//!   touches it: the null-`this` call works by accident on the game's
//!   compiler. The port passes its walk state directly.
//! - `normalizeAngle`'s loops **never terminate** on ±infinite angles
//!   (infinity minus 2π stays infinity — a process hang, not a game
//!   error): the port refuses with [`ObjectsError::NonFiniteAngle`],
//!   the same decision the generator's marker makes. NaN is
//!   transcribed (the early `0`).
//! - The `ThingFactory` template lookup the constructor performs is
//!   consumer-side (the typed data layer's job, like the blend-tile
//!   texture loads): the name is the parser's observable.
//! - The name-key interning: the original's `TheNameKeyGenerator`
//!   pre-interns the whole `WellKnownKeys.h` table at static
//!   initialization (link-order-dependent) before any dict is read;
//!   the port's walk seeds exactly the **three** keys this load
//!   consults, in the header's declaration order. A dict mixing other
//!   well-known names orders them by file-first-seen here where the
//!   original orders them by its startup table — the full set is the
//!   future typed layer's (callers can pre-seed wider tables through
//!   [`parse_object_data`]'s `keys` parameter).
//! - The `MapObject` runtime half — `m_color`'s bright green, the
//!   render and shadow objects, the bridge towers, the `s_nextTreeID`
//!   counter — is editor state the map file never carries; the three
//!   bits the load itself derives (`MO_WAYPOINT`/`MO_LIGHT`/
//!   `MO_SCORCH`) are kept as the `is_*` booleans.
//! - `readDict`'s escaping `ERROR_CORRUPT_FILE_FORMAT` becomes
//!   [`ObjectsError::Dict`] and stops the walk through the parser's
//!   `false` (the chunk stays open, as in both originals).
//!
//! # Examples
//!
//! ```
//! use rzh_formats::map_chunk::DataChunkOutput;
//! use rzh_formats::map_objects::{
//!     ObjectsListData, OBJECTS_LIST_LABEL, K_OBJECTS_VERSION_3,
//! };
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // A version-3 list with one waypoint object, built with the
//! // container's writer the way WorldBuilder saves one.
//! let mut out = DataChunkOutput::new();
//! out.open_data_chunk(OBJECTS_LIST_LABEL, K_OBJECTS_VERSION_3);
//! out.open_data_chunk(b"Object", K_OBJECTS_VERSION_3);
//! out.write_real(250.0);   // x
//! out.write_real(260.0);   // y
//! out.write_real(0.0);     // z (kept at version 3)
//! out.write_real(0.0);     // angle
//! out.write_int(0);        // flags
//! out.write_ascii_string(b"Waypoint");
//! out.write_int(0);        // dict pair count — built through the
//!                          // writer's dict path in real maps; zero
//!                          // pairs written directly here
//! out.close_data_chunk();
//! out.close_data_chunk();
//! let bytes = out.into_bytes()?;
//!
//! // The constructor's logical-load walk: the map's objects.
//! let list = ObjectsListData::from_map(&bytes)?;
//! assert_eq!(list.objects.len(), 1);
//! assert_eq!(list.objects[0].name, b"Waypoint");
//! assert_eq!(list.objects[0].location.x, 250.0);
//! # Ok(())
//! # }
//! ```

use std::fmt;

use crate::map_chunk::{
    DataChunkInfo, DataChunkInput, Dict, MapChunkError, NameKeys, ParserTable, dict_type,
};
use crate::map_lighting::Coord3d;

// ---------------------------------------------------------------------------
// constants (MapReaderWriterInfo.h, MapObject.h, WellKnownKeys.h)
// ---------------------------------------------------------------------------

/// The chunk label the constructor registers its objects-list reader
/// under, at the global scope.
pub const OBJECTS_LIST_LABEL: &[u8] = b"ObjectsList";

/// The nested chunk label the list reader registers the object parser
/// under, in the list's scope.
pub const OBJECT_LABEL: &[u8] = b"Object";

/// How wide and tall each height-map square is in world space
/// (`MAP_XY_FACTOR`).
pub const MAP_XY_FACTOR: f32 = 10.0;

/// The height scale (`MAP_HEIGHT_SCALE` — `MAP_XY_FACTOR / 16`): the
/// object z ceiling derives from it.
pub const MAP_HEIGHT_SCALE: f32 = MAP_XY_FACTOR / 16.0;

/// The object z floor: `-100 * MAP_XY_FACTOR` (`ParseObjectData`'s
/// `minZ`). Objects below it are dropped.
pub const MIN_OBJECT_Z: f32 = -100.0 * MAP_XY_FACTOR;

/// The object z ceiling: `(255 * 10) * MAP_HEIGHT_SCALE`
/// (`ParseObjectData`'s `maxZ` — the height-sample ceiling scaled into
/// world units). Objects above it are dropped.
pub const MAX_OBJECT_Z: f32 = (255.0 * 10.0) * MAP_HEIGHT_SCALE;

/// The `m_flags` bit values (`MapObject.h`'s anonymous enum) — the
/// editor semantics of the flags word the reader stores verbatim.
pub mod object_flags {
    /// If set, draws in water mirror.
    pub const FLAG_DRAWS_IN_MIRROR: i32 = 0x0000_0001;
    /// If set, is the first point in a road segment.
    pub const FLAG_ROAD_POINT1: i32 = 0x0000_0002;
    /// If set, is the second point in a road segment.
    pub const FLAG_ROAD_POINT2: i32 = 0x0000_0004;
    /// `FLAG_ROAD_POINT1 | FLAG_ROAD_POINT2` — nonzero makes a road piece.
    pub const FLAG_ROAD_FLAGS: i32 = FLAG_ROAD_POINT1 | FLAG_ROAD_POINT2;
    /// If set, the road corner is angled rather than curved.
    pub const FLAG_ROAD_CORNER_ANGLED: i32 = 0x0000_0008;
    /// If set, is the first point in a bridge.
    pub const FLAG_BRIDGE_POINT1: i32 = 0x0000_0010;
    /// If set, is the second point in a bridge.
    pub const FLAG_BRIDGE_POINT2: i32 = 0x0000_0020;
    /// `FLAG_BRIDGE_POINT1 | FLAG_BRIDGE_POINT2` — nonzero makes a
    /// bridge piece.
    pub const FLAG_BRIDGE_FLAGS: i32 = FLAG_BRIDGE_POINT1 | FLAG_BRIDGE_POINT2;
    /// A tight road corner.
    pub const FLAG_ROAD_CORNER_TIGHT: i32 = 0x0000_0040;
    /// If set, this road end does a generic alpha join.
    pub const FLAG_ROAD_JOIN: i32 = 0x0000_0080;
    /// If set, do not render this object (only WorldBuilder pays
    /// attention to this — right now, anyways).
    pub const FLAG_DONT_RENDER: i32 = 0x0000_0100;
}

/// The well-known property-sheet key the waypoint derivation consults
/// (`TheKey_waypointID` — `WellKnownKeys.h`'s `DEFINE_KEY(waypointID)`).
pub const WAYPOINT_ID_KEY: &[u8] = b"waypointID";

/// The well-known property-sheet key the light derivation consults
/// (`TheKey_lightHeightAboveTerrain`).
pub const LIGHT_HEIGHT_ABOVE_TERRAIN_KEY: &[u8] = b"lightHeightAboveTerrain";

/// The well-known property-sheet key the scorch derivation consults
/// (`TheKey_scorchType`).
pub const SCORCH_TYPE_KEY: &[u8] = b"scorchType";

/// The objects chunk version words (`K_OBJECTS_VERSION_*`) — the gates
/// the `Object` reader's fields hang on (see the module docs).
pub mod objects_version {
    /// Version 1: no property dict.
    pub const K_OBJECTS_VERSION_1: u16 = 1;
    /// Version 2: includes the dict (the z is still zeroed).
    pub const K_OBJECTS_VERSION_2: u16 = 2;
    /// Version 3: includes the dict and keeps the z — the version
    /// Zero Hour's WorldBuilder writes.
    pub const K_OBJECTS_VERSION_3: u16 = 3;
}

pub use objects_version::{K_OBJECTS_VERSION_1, K_OBJECTS_VERSION_2, K_OBJECTS_VERSION_3};

// ---------------------------------------------------------------------------
// errors
// ---------------------------------------------------------------------------

/// Errors the objects-list load path can produce.
///
/// The original has one failure code — `ERROR_CORRUPT_FILE_FORMAT`,
/// thrown by `readDict` on an impossible pair type and by the
/// constructor when the walk itself refuses — plus the process deaths
/// its unchecked operations reach (the infinite-angle hang). The port
/// states each site as its own variant; the payload errors stop the
/// walk the way the escaping exception does (the chunk stays open).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectsError {
    /// The walk itself failed — the header never opened as a `CkMp`
    /// table of contents (the constructor's `if (!file.parse(this))`
    /// throw).
    CorruptFileFormat,
    /// The property dict met a pair whose type byte is not one of the
    /// five `Dict::DataType` values (`readDict`'s escaping
    /// `ERROR_CORRUPT_FILE_FORMAT`, its type byte kept).
    Dict(MapChunkError),
    /// The angle is ±infinite: the original's `normalizeAngle` loops
    /// never terminate (infinity minus 2π stays infinity — a process
    /// hang, not a game error); the port refuses the load.
    NonFiniteAngle,
}

impl fmt::Display for ObjectsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ObjectsError::CorruptFileFormat => {
                write!(f, "map is not a chunky data-chunk file")
            }
            ObjectsError::Dict(e) => write!(f, "object property dict: {e}"),
            ObjectsError::NonFiniteAngle => {
                write!(f, "object angle is infinite (the wrap never terminates)")
            }
        }
    }
}

impl std::error::Error for ObjectsError {}

impl From<MapChunkError> for ObjectsError {
    fn from(e: MapChunkError) -> Self {
        ObjectsError::Dict(e)
    }
}

// ---------------------------------------------------------------------------
// the payload (MapObject's saved members, as the load leaves them)
// ---------------------------------------------------------------------------

/// One placed object — the `MapObject` members the load writes, as it
/// leaves them.
///
/// The location's z is the post-zeroing word (0 for version 2 and
/// lower), the angle is the constructor's normalized wrap, and the
/// three `is_*` booleans are the runtime bits the load derives from
/// the property sheet.
#[derive(Debug, Clone, PartialEq)]
pub struct MapObjectData {
    /// The object's center (`m_location` — z zeroed for version 2 and
    /// lower, dropped objects absent entirely).
    pub location: Coord3d,
    /// The object's type name (`m_objectName` — the `ThingTemplate`
    /// lookup key; resolving it is the consumer's job).
    pub name: Vec<u8>,
    /// The stored angle (`m_angle` — `normalizeAngle`'s wrap of the
    /// file's word, in (-π, π], NaN an early 0).
    pub angle: f32,
    /// The editor bit flags (`m_flags` — see [`object_flags`]).
    pub flags: i32,
    /// The property sheet (`m_properties` — the read dict, or empty
    /// for version 1 objects).
    pub properties: Dict,
    /// The waypoint derivation: a `waypointID` pair of type `INT`
    /// (`MO_WAYPOINT`).
    pub is_waypoint: bool,
    /// The light derivation: a `lightHeightAboveTerrain` pair of type
    /// `REAL` (`MO_LIGHT`).
    pub is_light: bool,
    /// The scorch derivation: a `scorchType` pair of type `INT`
    /// (`MO_SCORCH`).
    pub is_scorch: bool,
}

/// The `ObjectsList` payload: the map's objects as the logical load
/// leaves them — the reachable list, head first (the head is null
/// exactly when the vector is empty).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ObjectsListData {
    /// The kept objects, in list order.
    pub objects: Vec<MapObjectData>,
}

// ---------------------------------------------------------------------------
// the reader (WorldHeightMap.cpp)
// ---------------------------------------------------------------------------

/// `WorldHeightMap::ParseObjectData` — the `Object` chunk reader: the
/// location triple with the version-gated z zeroing, the angle, the
/// flags, the name, the version-gated property dict, the z-range drop,
/// the construction's angle wrap, and the three well-known
/// derivations.
///
/// `Ok(None)` is the z-range drop — the object was read whole and then
/// silently removed, exactly the original's `DEBUG_LOG`-and-return. The
/// `keys` table is the walk's interning state (the well-known keys
/// pre-seeded); a dict read interns its pair names through it, so the
/// property sheet's stored order is the interning order.
pub fn parse_object_data(
    file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
    keys: &mut NameKeys,
) -> Result<Option<MapObjectData>, ObjectsError> {
    let mut location = Coord3d {
        x: file.read_real(),
        y: file.read_real(),
        z: file.read_real(),
    };

    let min_z = -100.0 * MAP_XY_FACTOR;
    let max_z = (255.0 * 10.0) * MAP_HEIGHT_SCALE;

    if info.version <= K_OBJECTS_VERSION_2 {
        location.z = 0.0;
    }

    let angle = file.read_real();
    let flags = file.read_int();
    let name = file.read_ascii_string();
    let properties = if info.version >= K_OBJECTS_VERSION_2 {
        file.read_dict(keys)?
    } else {
        Dict::new()
    };

    if location.z < min_z || location.z > max_z {
        // DEBUG_LOG(("Removing object at z height %f", loc.z)); — the
        // object is dropped: no construction, no linkage, the walk runs on.
        return Ok(None);
    }

    // newInstance(MapObject)(loc, name, angle, flags, &d, template) —
    // the constructor's normalizeAngle is the only failure mode (the
    // template lookup is the consumer's; validateName is the identity).
    let angle = normalize_angle(angle)?;

    let is_waypoint = pair_type(keys, &properties, WAYPOINT_ID_KEY) == Some(dict_type::DICT_INT);
    let is_light =
        pair_type(keys, &properties, LIGHT_HEIGHT_ABOVE_TERRAIN_KEY) == Some(dict_type::DICT_REAL);
    let is_scorch = pair_type(keys, &properties, SCORCH_TYPE_KEY) == Some(dict_type::DICT_INT);

    Ok(Some(MapObjectData {
        location,
        name,
        angle,
        flags,
        properties,
        is_waypoint,
        is_light,
        is_scorch,
    }))
}

/// `Dict::getType` through the walk's table: the pair named `name`'s
/// wire type, `None` when no such pair (`DICT_NONE`).
///
/// The name interned here finds the pre-seeded well-known key (or the
/// id the dict's own read assigned it), so an absent name matches
/// nothing — the same pair the original's `findPairByKey` binary
/// search finds (both key on the name key alone, and `set` keeps one
/// pair per key).
fn pair_type(keys: &mut NameKeys, dict: &Dict, name: &[u8]) -> Option<u8> {
    let key = keys.name_to_key(name);
    for i in 0..dict.pair_count() {
        if dict.nth_key(i) == key {
            return Some(dict.nth_type(i));
        }
    }
    None
}

/// `normalizeAngle` (GameCommon.cpp) — the wrap into (-π, π], in the
/// original's mixed precision: the comparisons and the ±2π steps run
/// in `f64` and the accumulator narrows back to `f32` per iteration.
///
/// NaN is the original's own early `0` (the "ARGH!!!!" release path).
/// ±infinite angles make the original's loops never terminate — a
/// process hang — and are refused; the loop counts are otherwise
/// unbounded in both (keep test angles within ~1000 rad).
fn normalize_angle(angle: f32) -> Result<f32, ObjectsError> {
    // if (_isnan(angle)) return 0; // ARGH!!!! Don't assert and then not
    // handle it! Error bad! Fix error!
    if angle.is_nan() {
        return Ok(0.0);
    }
    if angle.is_infinite() {
        return Err(ObjectsError::NonFiniteAngle);
    }

    const PI: f64 = std::f64::consts::PI; // the C++ double 3.14159265358979323846

    let mut angle = angle;
    while f64::from(angle) > PI {
        angle = (f64::from(angle) - 2.0 * PI) as f32;
    }
    while f64::from(angle) <= -PI {
        angle = (f64::from(angle) + 2.0 * PI) as f32;
    }
    Ok(angle)
}

// ---------------------------------------------------------------------------
// the walk (the constructor's logical-load shape, objects only)
// ---------------------------------------------------------------------------

impl ObjectsListData {
    /// The `WorldHeightMap` constructor's **logical-data-only** walk,
    /// restricted to this payload: `ObjectsList` registered at the
    /// global scope (each chunk resetting the link cursor and
    /// registering the nested `Object` parser in its scope), the kept
    /// objects accumulated in list order — the last chunk that kept an
    /// object owns the list, the orphaned ones dropped.
    ///
    /// The other payloads the logical load registers (the size-only
    /// height map, the world info, the polygon triggers and the sides
    /// list) arrive with their own slices — here their chunks are
    /// skipped whole, the walk's behaviour for an unmatched label.
    /// The walk seeds the three well-known property keys into its
    /// name-key table before the first dict read (see the module docs'
    /// interning note).
    pub fn from_map(bytes: &[u8]) -> Result<Self, ObjectsError> {
        let mut input = DataChunkInput::new(bytes);
        let mut table: ParserTable<'_, ObjectsWalk> = ParserTable::new();
        table.register(OBJECTS_LIST_LABEL, b"", objects_list_parser);
        let mut user = ObjectsWalk::new();
        if !input.parse(&mut table, &mut user) {
            // A parser's false is either this payload's refused load or
            // the walk's own refusal; the original throws the same code
            // for both (the constructor's `if (!file.parse(this))`).
            return Err(user.error.unwrap_or(ObjectsError::CorruptFileFormat));
        }
        Ok(ObjectsListData {
            objects: user.objects,
        })
    }
}

/// The walk's user state: the reachable object list, the link cursor
/// (`m_currentObject`'s null-ness — the pointer value itself is never
/// an observable), the name-key table and the error capture.
struct ObjectsWalk {
    objects: Vec<MapObjectData>,
    linked: bool,
    keys: NameKeys,
    error: Option<ObjectsError>,
}

impl ObjectsWalk {
    /// The pre-walk state: `freeListOfMapObjects`' empty list, the null
    /// cursor, and the well-known keys seeded the way the engine's
    /// static initialization does (WellKnownKeys.h's declaration
    /// order).
    fn new() -> Self {
        let mut keys = NameKeys::new();
        keys.name_to_key(LIGHT_HEIGHT_ABOVE_TERRAIN_KEY);
        keys.name_to_key(WAYPOINT_ID_KEY);
        keys.name_to_key(SCORCH_TYPE_KEY);
        ObjectsWalk {
            objects: Vec::new(),
            linked: false,
            keys,
            error: None,
        }
    }
}

/// `WorldHeightMap::ParseObjectsDataChunk` — the registered
/// `ObjectsList` parser: reset the link cursor (the list head is NOT
/// reset — a later chunk's first kept object overwrites it), register
/// the `Object` parser in this chunk's scope, and walk the children.
fn objects_list_parser(
    file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
    table: &mut ParserTable<'_, ObjectsWalk>,
    user: &mut ObjectsWalk,
) -> bool {
    // file.m_currentObject = nullptr; // just in case.
    user.linked = false;
    // registerParser("Object", info->label, ParseObjectDataChunk)
    table.register(OBJECT_LABEL, info.label.clone(), object_chunk_parser);
    // return (file.parse(userData));
    file.parse(table, user)
}

/// `WorldHeightMap::ParseObjectDataChunk` — the registered `Object`
/// parser's dispatch (`readDict` from version 2; the original's
/// `file.m_userData` this-pointer is always null and never touched —
/// see the module docs), with the linkage's head-overwrite semantics.
fn object_chunk_parser(
    file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
    _table: &mut ParserTable<'_, ObjectsWalk>,
    user: &mut ObjectsWalk,
) -> bool {
    match parse_object_data(file, info, &mut user.keys) {
        Ok(Some(obj)) => {
            // the linkage: a previous object chains, else the list head
            // is replaced — the orphaned list is unreachable either way
            if !user.linked {
                user.objects.clear();
            }
            user.objects.push(obj);
            user.linked = true;
            true
        }
        // the z-range drop: no construction, no linkage, the cursor
        // stays where it was
        Ok(None) => true,
        Err(e) => {
            user.error = Some(e);
            false
        }
    }
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map_chunk::DictValue;

    /// A test file's table of contents: the chunk labels and the dict
    /// pair names the corpora reference (the reader resolves every
    /// `keyAndType` word's id through it).
    struct Toc {
        entries: Vec<(Vec<u8>, u32)>,
    }

    impl Toc {
        fn new(names: &[&[u8]]) -> Self {
            Toc {
                entries: names
                    .iter()
                    .enumerate()
                    .map(|(i, n)| ((*n).to_vec(), i as u32 + 1))
                    .collect(),
            }
        }

        fn id(&self, name: &[u8]) -> u32 {
            self.entries
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, id)| *id)
                .unwrap_or(0)
        }

        fn bytes(&self) -> Vec<u8> {
            let mut b = Vec::new();
            b.extend_from_slice(b"CkMp");
            b.extend_from_slice(&(self.entries.len() as i32).to_le_bytes());
            for (name, id) in &self.entries {
                b.push(name.len() as u8);
                b.extend_from_slice(name);
                b.extend_from_slice(&id.to_le_bytes());
            }
            b
        }
    }

    fn put_i32(b: &mut Vec<u8>, v: i32) {
        b.extend_from_slice(&v.to_le_bytes());
    }
    fn put_f32(b: &mut Vec<u8>, v: f32) {
        b.extend_from_slice(&v.to_le_bytes());
    }

    fn put_chunk(b: &mut Vec<u8>, id: u32, ver: u16, data: &[u8]) {
        b.extend_from_slice(&id.to_le_bytes());
        b.extend_from_slice(&ver.to_le_bytes());
        b.extend_from_slice(&(data.len() as i32).to_le_bytes());
        b.extend_from_slice(data);
    }

    /// One Object payload: loc, angle, flags, name, and (with_dict) the
    /// pair count and pairs.
    #[allow(clippy::too_many_arguments)]
    fn object_payload(
        x: f32,
        y: f32,
        z: f32,
        angle: f32,
        flags: i32,
        name: &[u8],
        pairs: &[Vec<u8>],
        with_dict: bool,
    ) -> Vec<u8> {
        let mut p = Vec::new();
        put_f32(&mut p, x);
        put_f32(&mut p, y);
        put_f32(&mut p, z);
        put_f32(&mut p, angle);
        put_i32(&mut p, flags);
        p.extend_from_slice(&(name.len() as u16).to_le_bytes());
        p.extend_from_slice(name);
        if with_dict {
            p.extend_from_slice(&(pairs.len() as u16).to_le_bytes());
            for pair in pairs {
                p.extend_from_slice(pair);
            }
        }
        p
    }

    /// A dict pair: the `keyAndType` word (the name's TOC id shifted
    /// up, the type byte below) plus the value.
    fn pair(toc: &Toc, kind: u8, key: &[u8], value: &[u8]) -> Vec<u8> {
        let mut p = Vec::new();
        let key_and_type = ((toc.id(key) << 8) | kind as u32) as i32;
        put_i32(&mut p, key_and_type);
        p.extend_from_slice(value);
        p
    }

    fn pair_int(toc: &Toc, key: &[u8], v: i32) -> Vec<u8> {
        let mut p = pair(toc, 1, key, &[]);
        p.extend_from_slice(&v.to_le_bytes());
        p
    }
    fn pair_real(toc: &Toc, key: &[u8], v: f32) -> Vec<u8> {
        let mut p = pair(toc, 2, key, &[]);
        p.extend_from_slice(&v.to_le_bytes());
        p
    }
    fn pair_bool(toc: &Toc, key: &[u8], v: bool) -> Vec<u8> {
        let mut p = pair(toc, 0, key, &[]);
        p.push(u8::from(v));
        p
    }

    /// An ObjectsList chunk (id 1) with `Object` children (id 2).
    fn objects_list_chunk(children: &[(u16, Vec<u8>)]) -> Vec<u8> {
        let mut inner = Vec::new();
        for &(version, ref payload) in children {
            put_chunk(&mut inner, 2, version, payload);
        }
        let mut b = Vec::new();
        put_chunk(&mut b, 1, K_OBJECTS_VERSION_3, &inner);
        b
    }

    /// A map file: the TOC plus the top-level chunk bytes.
    fn build_map(toc: &Toc, chunks: &[Vec<u8>]) -> Vec<u8> {
        let mut b = toc.bytes();
        for chunk in chunks {
            b.extend_from_slice(chunk);
        }
        b
    }

    #[test]
    fn the_version_gates_match_the_format() {
        assert_eq!(K_OBJECTS_VERSION_1, 1);
        assert_eq!(K_OBJECTS_VERSION_2, 2);
        assert_eq!(K_OBJECTS_VERSION_3, 3);
        assert_eq!(OBJECTS_LIST_LABEL, b"ObjectsList");
        assert_eq!(OBJECT_LABEL, b"Object");
        assert_eq!(MAP_XY_FACTOR, 10.0);
        assert_eq!(MAP_HEIGHT_SCALE, 10.0 / 16.0);
        assert_eq!(MIN_OBJECT_Z, -1000.0);
        assert_eq!(MAX_OBJECT_Z, 1593.75);
        assert_eq!(WAYPOINT_ID_KEY, b"waypointID");
        assert_eq!(LIGHT_HEIGHT_ABOVE_TERRAIN_KEY, b"lightHeightAboveTerrain");
        assert_eq!(SCORCH_TYPE_KEY, b"scorchType");
        assert_eq!(object_flags::FLAG_DRAWS_IN_MIRROR, 0x1);
        assert_eq!(object_flags::FLAG_DONT_RENDER, 0x100);
        assert_eq!(
            object_flags::FLAG_ROAD_FLAGS,
            object_flags::FLAG_ROAD_POINT1 | object_flags::FLAG_ROAD_POINT2
        );
    }

    #[test]
    fn a_v3_list_loads_with_its_derivations() {
        let toc = Toc::new(&[
            b"ObjectsList",
            b"Object",
            b"objectEnabled",
            b"waypointID",
            b"lightHeightAboveTerrain",
            b"scorchType",
            b"objectSelectable",
        ]);
        let children = vec![
            (
                K_OBJECTS_VERSION_3,
                object_payload(
                    100.0,
                    200.0,
                    10.0,
                    0.0,
                    0x101,
                    b"ProductionPlant",
                    &[],
                    true,
                ),
            ),
            (
                K_OBJECTS_VERSION_3,
                object_payload(
                    250.0,
                    260.0,
                    0.0,
                    0.0,
                    0,
                    b"Waypoint",
                    &[pair_int(&toc, b"waypointID", 7)],
                    true,
                ),
            ),
            (
                K_OBJECTS_VERSION_3,
                object_payload(
                    -50.0,
                    60.0,
                    25.0,
                    0.0,
                    0,
                    b"CruiseMissile",
                    &[pair_real(&toc, b"lightHeightAboveTerrain", 12.5)],
                    true,
                ),
            ),
            (
                K_OBJECTS_VERSION_3,
                object_payload(
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    0,
                    b"Scorch1",
                    &[
                        pair_int(&toc, b"scorchType", 2),
                        pair_bool(&toc, b"objectSelectable", false),
                    ],
                    true,
                ),
            ),
        ];
        let bytes = build_map(&toc, &[objects_list_chunk(&children)]);
        let list = ObjectsListData::from_map(&bytes).expect("loads");
        assert_eq!(list.objects.len(), 4);

        assert_eq!(list.objects[0].name, b"ProductionPlant");
        assert_eq!(list.objects[0].location.z, 10.0); // version 3 keeps z
        assert_eq!(list.objects[0].flags, 0x101);
        assert!(
            !list.objects[0].is_waypoint && !list.objects[0].is_light && !list.objects[0].is_scorch
        );

        assert!(list.objects[1].is_waypoint);
        assert!(!list.objects[1].is_light && !list.objects[1].is_scorch);
        assert_eq!(list.objects[1].properties.pair_count(), 1);
        assert_eq!(list.objects[1].properties.nth_value(0), &DictValue::Int(7));

        assert!(list.objects[2].is_light);
        assert!(!list.objects[2].is_waypoint && !list.objects[2].is_scorch);

        assert!(list.objects[3].is_scorch);
        assert_eq!(list.objects[3].properties.pair_count(), 2);
        // the stored order is the interning order: scorchType (the
        // seeded ordinal 3) before objectSelectable (first-seen 4)
        assert_eq!(list.objects[3].properties.nth_type(0), dict_type::DICT_INT);
        assert_eq!(list.objects[3].properties.nth_type(1), dict_type::DICT_BOOL);
    }

    #[test]
    fn the_derivations_are_exact_type_matches() {
        let toc = Toc::new(&[
            b"ObjectsList",
            b"Object",
            b"waypointID",
            b"lightHeightAboveTerrain",
            b"scorchType",
        ]);
        let children = vec![(
            K_OBJECTS_VERSION_3,
            object_payload(
                0.0,
                0.0,
                0.0,
                0.0,
                0,
                b"WrongTyped",
                &[
                    pair_real(&toc, b"waypointID", 9.0),            // REAL, not INT
                    pair_int(&toc, b"lightHeightAboveTerrain", 20), // INT, not REAL
                    pair_bool(&toc, b"scorchType", true),           // BOOL, not INT
                ],
                true,
            ),
        )];
        let bytes = build_map(&toc, &[objects_list_chunk(&children)]);
        let list = ObjectsListData::from_map(&bytes).expect("loads");
        assert_eq!(list.objects.len(), 1);
        assert!(!list.objects[0].is_waypoint);
        assert!(!list.objects[0].is_light);
        assert!(!list.objects[0].is_scorch);
    }

    #[test]
    fn the_version_gates_zero_the_z_and_gate_the_dict() {
        // version 2: the dict is read, the z is still zeroed
        let toc = Toc::new(&[b"ObjectsList", b"Object", b"objectEnabled"]);
        let children = vec![(
            K_OBJECTS_VERSION_2,
            object_payload(
                10.0,
                20.0,
                500.0,
                0.5,
                0,
                b"NukeSilo",
                &[pair_bool(&toc, b"objectEnabled", true)],
                true,
            ),
        )];
        let bytes = build_map(&toc, &[objects_list_chunk(&children)]);
        let list = ObjectsListData::from_map(&bytes).expect("loads");
        assert_eq!(list.objects[0].location.z, 0.0); // zeroed at v <= 2
        assert_eq!(list.objects[0].properties.pair_count(), 1);

        // version 1: no dict bytes at all, the z zeroed
        let toc = Toc::new(&[b"ObjectsList", b"Object"]);
        let children = vec![(
            K_OBJECTS_VERSION_1,
            object_payload(30.0, 40.0, 500.0, 1.0, 2, b"TreeOak", &[], false),
        )];
        let bytes = build_map(&toc, &[objects_list_chunk(&children)]);
        let list = ObjectsListData::from_map(&bytes).expect("loads");
        assert_eq!(list.objects[0].location.z, 0.0);
        assert_eq!(list.objects[0].properties.pair_count(), 0);
        assert_eq!(list.objects[0].flags, 2);

        // version 0 reads like a 1; version 4 like a 3
        let toc = Toc::new(&[b"ObjectsList", b"Object", b"scorchType"]);
        let children = vec![
            (0, object_payload(1.0, 2.0, 3.0, 0.0, 0, b"Old", &[], false)),
            (
                4,
                object_payload(
                    4.0,
                    5.0,
                    6.0,
                    0.0,
                    0,
                    b"New",
                    &[pair_int(&toc, b"scorchType", 1)],
                    true,
                ),
            ),
        ];
        let bytes = build_map(&toc, &[objects_list_chunk(&children)]);
        let list = ObjectsListData::from_map(&bytes).expect("loads");
        assert_eq!(list.objects[0].location.z, 0.0);
        assert_eq!(list.objects[0].properties.pair_count(), 0);
        assert_eq!(list.objects[1].location.z, 6.0);
        assert!(list.objects[1].is_scorch);
    }

    #[test]
    fn the_list_version_word_is_not_the_gate() {
        // an ObjectsList chunk with a version-1 word carrying version-3
        // children: the Object's word governs
        let toc = Toc::new(&[b"ObjectsList", b"Object", b"objectEnabled"]);
        let mut inner = Vec::new();
        put_chunk(
            &mut inner,
            2,
            K_OBJECTS_VERSION_3,
            &object_payload(
                8.0,
                9.0,
                10.0,
                0.0,
                0,
                b"Mismatched",
                &[pair_bool(&toc, b"objectEnabled", true)],
                true,
            ),
        );
        let mut list_chunk = Vec::new();
        put_chunk(&mut list_chunk, 1, K_OBJECTS_VERSION_1, &inner);
        let bytes = build_map(&toc, &[list_chunk]);
        let list = ObjectsListData::from_map(&bytes).expect("loads");
        assert_eq!(list.objects[0].location.z, 10.0); // kept: v3 child
        assert_eq!(list.objects[0].properties.pair_count(), 1);
    }

    #[test]
    fn the_z_range_drops_whole_objects() {
        let toc = Toc::new(&[b"ObjectsList", b"Object"]);
        let children = vec![
            (
                K_OBJECTS_VERSION_3,
                object_payload(0.0, 0.0, 2000.0, 0.0, 0, b"TooHigh", &[], true),
            ),
            (
                K_OBJECTS_VERSION_3,
                object_payload(0.0, 0.0, -2000.0, 0.0, 0, b"TooLow", &[], true),
            ),
            (
                K_OBJECTS_VERSION_3,
                object_payload(0.0, 0.0, -1000.0, 0.0, 0, b"AtFloor", &[], true),
            ),
            (
                K_OBJECTS_VERSION_3,
                object_payload(0.0, 0.0, 1593.75, 0.0, 0, b"AtCeiling", &[], true),
            ),
            (
                K_OBJECTS_VERSION_3,
                object_payload(1.0, 1.0, 1.0, 0.0, 0, b"Survivor", &[], true),
            ),
        ];
        let bytes = build_map(&toc, &[objects_list_chunk(&children)]);
        let list = ObjectsListData::from_map(&bytes).expect("loads");
        // the two out-of-range objects are gone, the boundaries stay,
        // and the drop never breaks the chain
        assert_eq!(
            list.objects
                .iter()
                .map(|o| o.name.as_slice())
                .collect::<Vec<_>>(),
            vec![
                b"AtFloor".as_slice(),
                b"AtCeiling".as_slice(),
                b"Survivor".as_slice()
            ]
        );
        assert_eq!(list.objects[0].location.z, -1000.0);
        assert_eq!(list.objects[1].location.z, 1593.75);
    }

    #[test]
    fn the_angle_wraps_in_double_precision() {
        // the pinned golden values: (float)pi wraps negative, its
        // negation wraps positive, 100 wraps several times, NaN is 0.
        // (f32::consts::PI is the same bits the golden corpus's
        // 3.14159265f writes — the nearest f32 to π.)
        let toc = Toc::new(&[b"ObjectsList", b"Object"]);
        let pi = std::f32::consts::PI;
        let angles = [0.0f32, pi, -pi, 100.0f32, -100.0f32, 7.5f32];
        let mut children: Vec<(u16, Vec<u8>)> = angles
            .iter()
            .map(|&a| {
                (
                    K_OBJECTS_VERSION_3,
                    object_payload(0.0, 0.0, 0.0, a, 0, b"Angled", &[], true),
                )
            })
            .collect();
        // the NaN angle child (the angle word patched to a quiet NaN)
        let mut nan_payload = object_payload(0.0, 0.0, 0.0, 0.0, 0, b"NanAngle", &[], true);
        nan_payload[12..16].copy_from_slice(&0x7fc0_0000u32.to_le_bytes());
        children.push((K_OBJECTS_VERSION_3, nan_payload));
        let bytes = build_map(&toc, &[objects_list_chunk(&children)]);

        let list = ObjectsListData::from_map(&bytes).expect("loads");
        assert_eq!(list.objects.len(), 7);
        assert_eq!(list.objects[0].angle.to_bits(), 0.0f32.to_bits());
        assert_eq!(list.objects[1].angle.to_bits(), 0xc049_0fda);
        assert_eq!(list.objects[2].angle.to_bits(), 0x4049_0fda);
        assert_eq!(list.objects[3].angle.to_bits(), 0xbf07_eddd);
        assert_eq!(list.objects[4].angle.to_bits(), 0x3f07_eddd);
        assert_eq!(list.objects[5].angle.to_bits(), 0x3f9b_c095);
        assert_eq!(list.objects[6].angle.to_bits(), 0.0f32.to_bits()); // NaN -> 0
    }

    #[test]
    fn an_infinite_angle_is_refused() {
        let toc = Toc::new(&[b"ObjectsList", b"Object"]);
        let mut inf_payload = object_payload(0.0, 0.0, 0.0, 0.0, 0, b"InfAngle", &[], true);
        inf_payload[12..16].copy_from_slice(&0x7f80_0000u32.to_le_bytes());
        let children = vec![
            (K_OBJECTS_VERSION_3, inf_payload),
            (
                K_OBJECTS_VERSION_3,
                object_payload(1.0, 1.0, 1.0, 0.0, 0, b"Never", &[], true),
            ),
        ];
        let bytes = build_map(&toc, &[objects_list_chunk(&children)]);
        assert_eq!(
            ObjectsListData::from_map(&bytes),
            Err(ObjectsError::NonFiniteAngle)
        );
    }

    #[test]
    fn a_later_list_owns_the_head_but_only_when_it_keeps() {
        // two lists: the second's kept object orphans the first list
        let toc = Toc::new(&[b"ObjectsList", b"Object"]);
        let first = vec![
            (
                K_OBJECTS_VERSION_3,
                object_payload(0.0, 0.0, 0.0, 0.0, 0, b"FirstA", &[], true),
            ),
            (
                K_OBJECTS_VERSION_3,
                object_payload(1.0, 0.0, 0.0, 0.0, 0, b"FirstB", &[], true),
            ),
        ];
        let second = vec![(
            K_OBJECTS_VERSION_3,
            object_payload(2.0, 0.0, 0.0, 0.0, 0, b"SecondC", &[], true),
        )];
        let bytes = build_map(
            &toc,
            &[objects_list_chunk(&first), objects_list_chunk(&second)],
        );
        let list = ObjectsListData::from_map(&bytes).expect("loads");
        assert_eq!(list.objects.len(), 1);
        assert_eq!(list.objects[0].name, b"SecondC");

        // a later list whose objects all drop never writes the head
        let kept = vec![(
            K_OBJECTS_VERSION_3,
            object_payload(0.0, 0.0, 1.0, 0.0, 0, b"Kept", &[], true),
        )];
        let dropped = vec![(
            K_OBJECTS_VERSION_3,
            object_payload(0.0, 0.0, 3000.0, 0.0, 0, b"Dropped", &[], true),
        )];
        let bytes = build_map(
            &toc,
            &[objects_list_chunk(&kept), objects_list_chunk(&dropped)],
        );
        let list = ObjectsListData::from_map(&bytes).expect("loads");
        assert_eq!(list.objects.len(), 1);
        assert_eq!(list.objects[0].name, b"Kept");

        // an empty later list never writes the head either
        let bytes = build_map(&toc, &[objects_list_chunk(&kept), objects_list_chunk(&[])]);
        let list = ObjectsListData::from_map(&bytes).expect("loads");
        assert_eq!(list.objects.len(), 1);
        assert_eq!(list.objects[0].name, b"Kept");
    }

    #[test]
    fn scope_rules_and_absent_payloads() {
        let toc = Toc::new(&[b"ObjectsList", b"Object", b"WorldInfo", b"HeightMapData"]);

        // a bare Object at global scope: skipped whole (no parser at
        // ("Object", "")), the list stays empty — the null head
        let mut b = toc.bytes();
        put_chunk(
            &mut b,
            2,
            K_OBJECTS_VERSION_3,
            &object_payload(0.0, 0.0, 0.0, 0.0, 0, b"Bare", &[], true),
        );
        assert_eq!(ObjectsListData::from_map(&b).unwrap().objects.len(), 0);

        // an Object nested inside a non-ObjectsList container: the
        // container skips it whole; the top-level list after it loads
        let mut b = toc.bytes();
        {
            let mut inner = Vec::new();
            put_chunk(
                &mut inner,
                2,
                K_OBJECTS_VERSION_3,
                &object_payload(0.0, 0.0, 0.0, 0.0, 0, b"InWorldInfo", &[], true),
            );
            put_chunk(&mut b, 3, 1, &inner); // WorldInfo
        }
        b.extend_from_slice(&objects_list_chunk(&[(
            K_OBJECTS_VERSION_3,
            object_payload(2.0, 0.0, 0.0, 0.0, 0, b"TopLevel", &[], true),
        )]));
        let list = ObjectsListData::from_map(&b).expect("loads");
        assert_eq!(list.objects.len(), 1);
        assert_eq!(list.objects[0].name, b"TopLevel");

        // no ObjectsList at all: the walk succeeds, the list is empty
        let mut b = toc.bytes();
        {
            let mut d = Vec::new();
            put_i32(&mut d, 2);
            put_i32(&mut d, 2);
            put_i32(&mut d, 4);
            d.extend_from_slice(&[0x10; 4]);
            put_chunk(&mut b, 4, 3, &d); // HeightMapData
        }
        assert_eq!(ObjectsListData::from_map(&b).unwrap().objects.len(), 0);

        // the walk's own refusals
        assert_eq!(
            ObjectsListData::from_map(b"not a map at all"),
            Err(ObjectsError::CorruptFileFormat)
        );
    }

    #[test]
    fn short_reads_and_corrupt_dicts() {
        // a corrupt dict pair type stops the walk with the chunk open
        let toc = Toc::new(&[b"ObjectsList", b"Object", b"objectEnabled"]);
        let bad_pair = pair(&toc, 7, b"objectEnabled", &0i32.to_le_bytes());
        let children = vec![(
            K_OBJECTS_VERSION_3,
            object_payload(0.0, 0.0, 0.0, 0.0, 0, b"Corrupt", &[bad_pair], true),
        )];
        let bytes = build_map(&toc, &[objects_list_chunk(&children)]);
        assert_eq!(
            ObjectsListData::from_map(&bytes),
            Err(ObjectsError::Dict(MapChunkError::CorruptDictType(7)))
        );

        // a file cut inside the object's dict: the zero-filled count
        // mints nothing, the name's bytes are what survived
        let mut payload = object_payload(
            0.0,
            0.0,
            0.0,
            0.0,
            0,
            b"Sho",
            &[pair_bool(&toc, b"objectEnabled", true)],
            true,
        );
        payload.truncate(payload.len() - 5); // cut into the pair bytes
        let children = vec![(K_OBJECTS_VERSION_3, payload)];
        let bytes = build_map(&toc, &[objects_list_chunk(&children)]);
        let list = ObjectsListData::from_map(&bytes).expect("loads");
        assert_eq!(list.objects.len(), 1);
        assert_eq!(list.objects[0].name, b"Sho");
    }

    #[test]
    fn the_interning_order_is_the_stored_order() {
        // a dict whose file order is [objectEnabled, waypointID]: the
        // seeded waypointID (ordinal 2) sorts before objectEnabled
        // (first-seen ordinal 4)
        let toc = Toc::new(&[b"ObjectsList", b"Object", b"objectEnabled", b"waypointID"]);
        let children = vec![(
            K_OBJECTS_VERSION_3,
            object_payload(
                0.0,
                0.0,
                0.0,
                0.0,
                0,
                b"Ordered",
                &[
                    pair_bool(&toc, b"objectEnabled", true),
                    pair_int(&toc, b"waypointID", 3),
                ],
                true,
            ),
        )];
        let bytes = build_map(&toc, &[objects_list_chunk(&children)]);
        let list = ObjectsListData::from_map(&bytes).expect("loads");
        assert_eq!(list.objects[0].properties.pair_count(), 2);
        assert_eq!(list.objects[0].properties.nth_type(0), dict_type::DICT_INT); // waypointID
        assert_eq!(list.objects[0].properties.nth_type(1), dict_type::DICT_BOOL); // objectEnabled
        assert!(list.objects[0].is_waypoint);
    }

    #[test]
    fn error_display_is_stable() {
        assert_eq!(
            ObjectsError::CorruptFileFormat.to_string(),
            "map is not a chunky data-chunk file"
        );
        assert_eq!(
            ObjectsError::NonFiniteAngle.to_string(),
            "object angle is infinite (the wrap never terminates)"
        );
        assert_eq!(
            ObjectsError::Dict(MapChunkError::CorruptDictType(9)).to_string(),
            "object property dict: dict pair type 0x09 is not one of the five Dict data types"
        );
    }
}
