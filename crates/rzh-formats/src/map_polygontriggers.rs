// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Ported from the Zero Hour source release (baseline 571a7c5):
//   Core/GameEngine/Source/GameLogic/Map/PolygonTrigger.cpp
//     (PolygonTrigger::ParsePolygonTriggersDataChunk — the
//     PolygonTriggers chunk reader with its flat count loop, the
//     entry-time deleteTriggers, the numPoints<2 deletion, the
//     head-prepending linkage, the version-1 auto-added water area and
//     the final s_currentID; PolygonTrigger's constructor — the
//     numPoints+1 clamp and the minted-id overwrite; addPolygonTrigger
//     — the prepend; deleteTriggers — the list/counter reset; and the
//     constructor's registration-time deleteTriggers)
//   Core/GameEngine/Include/GameLogic/PolygonTrigger.h
//     (the saved members m_triggerName/m_triggerID/m_points/
//     m_riverStart/m_isWaterArea/m_isRiver/m_layerName)
//   Core/GameEngine/Include/Common/MapReaderWriterInfo.h
//     (K_TRIGGERS_VERSION_1..4 — the version words and their gates)
//   Core/GameEngine/Include/Common/MapObject.h
//     (MAP_XY_FACTOR — the world scale the auto area's corners derive
//     from; re-exported by this module's use of
//     crate::map_objects::MAP_XY_FACTOR)
//
// See the module docs for the format layout and the documented
// differences from the C++ (the linked list becomes the vector; the
// points array becomes a Vec; the extents are parameters).

//! The **map polygon-triggers payload** — the `PolygonTriggers` chunk
//! of a `.map` file, on top of [`crate::map_chunk`].
//!
//! A Zero Hour map's named areas — the script triggers units are tested
//! against, and the trigger areas that carry water — are one flat
//! record list at the top level of the data-chunk tree (no nested
//! chunks: a count word, then that many fixed-shape records).
//! `WorldHeightMap`'s constructor registers a reader for it in the
//! **logical-data-only** load only, and this module ports that reader
//! over the [`DataChunkInput`](crate::map_chunk::DataChunkInput) walk,
//! quirks included (parity level P0, pinned by golden vectors generated
//! from the transcription in `tools/differential/gen_mappolygons_golden.cpp`).
//!
//! # The chunk in one page
//!
//! ```text
//! PolygonTriggers chunk (the gates hang off its own version word)
//!   i32               the trigger count
//!   record × n        one per trigger:
//!     str16             the trigger name
//!     [v >= 4] str16    the layer name (Zero Hour)
//!     i32               the trigger id
//!     [v >= 2] u8       the water-area flag
//!     [v >= 3] u8       the river flag
//!     [v >= 3] i32      the river start point
//!     i32               the point count
//!     (i32, i32, i32) × points
//! ```
//!
//! # The load-path quirks (transcribed)
//!
//! - **The flat count loop**: the record shape is version-gated with
//!   ceilings (`v >= 2` the water byte, `v >= 3` the river words,
//!   `v >= 4` the layer), and a version-0 chunk reads the bare v1
//!   shape while a version-5 reads like a 4 — but the auto-area gate
//!   below is the **exact** `== 1`.
//! - **Triggers with fewer than two points are deleted after their
//!   points are read** (the non-retail branch Zero Hour always takes):
//!   the point words are consumed, the id still counts toward the
//!   maximum, and the linkage cursor is unchanged — the survivors chain
//!   through the deletions.
//! - **The version-1 auto-added water area**: a version-1 chunk
//!   appends one unnamed water trigger (the release name is empty;
//!   `RTS_DEBUG` builds name it `AutoAddedWaterAreaTrigger`) whose id
//!   is the **post-incremented** maximum — a duplicate of the largest
//!   id when any trigger was kept, 0 when none was — with the four
//!   corner points at ±30 squares offset by the water extents
//!   ([`MAP_XY_FACTOR`]-scaled, z 7 "the old water position").
//! - **The id counter** lands at `maxTriggerId + 1` — two past the
//!   pre-auto maximum in version 1 (the post-increment), one past in
//!   the later versions; negative ids never update the maximum, and a
//!   deleted trigger's id still does.
//! - The parser **resets the whole payload at entry**
//!   (`deleteTriggers`: the list cleared, the counter back to 1), so a
//!   repeated `PolygonTriggers` chunk owns the final state outright.
//! - The reader never fails (short reads zero-fill: an empty name, id
//!   0, a zero-point record that the deletion gate then drops) and
//!   never checks it consumed the chunk (the `atEndOfChunk` assert is
//!   debug-only); leftovers go to the walk's close.
//! - A `PolygonTriggers` chunk at any **non-global scope** never fires
//!   — the registration is `("PolygonTriggers", "")`.
//!
//! # Differences from the C++ (documented)
//!
//! - The linked list (`ThePolygonTriggerListPtr` + `m_nextPolygonTrigger`,
//!   prepended by `addPolygonTrigger`) becomes the vector of kept
//!   triggers in file order: the parser clears at entry, so the first
//!   kept trigger prepends to an empty head and the rest chain after
//!   the previous — both are the vector's push, and the vector is
//!   exactly the reachable list.
//! - The constructor's fixed-capacity points array (`numPoints + 1`
//!   slots, clamped to at least 2, doubled by `reallocate`) becomes a
//!   plain [`Vec`]: the allocation size is not a load observable (the
//!   capacity always holds the points the loop reads, so `reallocate`
//!   never runs on the load path), and the constructor's minted id
//!   (`s_currentID++`) is overwritten before anything can observe it.
//!   Absurd point counts are the same unbounded-work class the
//!   objects-list port documents for its angle wrap — the corpora keep
//!   them sane.
//! - The water extents are parameters (the original reads
//!   `TheGlobalData->m_waterExtentX/Y`, GlobalData INI state defaulting
//!   to 0): [`PolygonTriggersData::from_map`] uses the engine defaults
//!   and [`PolygonTriggersData::from_map_with_extents`] takes them
//!   explicitly. The corner arithmetic is the original's
//!   float-expression-then-truncate (`300.0 + extent as i32`).
//! - The runtime half — the water handle, the lazy bounds/radius
//!   caches, `pointInTrigger`'s ray cast, the editor's
//!   insert/delete/reorder points — is consumer-side state the map
//!   file never carries; the export-with-scripts flag defaults false
//!   and no load path ever sets it (not an observable of this slice).
//!
//! # Examples
//!
//! ```
//! use rzh_formats::map_chunk::DataChunkOutput;
//! use rzh_formats::map_polygontriggers::{
//!     PolygonTriggersData, POLYGON_TRIGGERS_LABEL, K_TRIGGERS_VERSION_4,
//! };
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // One version-4 trigger — the flat record the WorldBuilder writes.
//! let mut out = DataChunkOutput::new();
//! out.open_data_chunk(POLYGON_TRIGGERS_LABEL, K_TRIGGERS_VERSION_4);
//! out.write_int(1);                 // the trigger count
//! out.write_ascii_string(b"Ambush"); // the name
//! out.write_ascii_string(b"Ground"); // the layer (version 4)
//! out.write_int(17);                // the id
//! out.write_byte(0);                // the water flag (version 2)
//! out.write_byte(0);                // the river flag (version 3)
//! out.write_int(0);                 // the river start (version 3)
//! out.write_int(2);                 // the point count
//! out.write_int(0); out.write_int(0); out.write_int(0);
//! out.write_int(100); out.write_int(100); out.write_int(0);
//! out.close_data_chunk();
//! let bytes = out.into_bytes()?;
//!
//! // The constructor's logical-load walk: the map's triggers.
//! let data = PolygonTriggersData::from_map(&bytes)?.expect("the chunk fired");
//! assert_eq!(data.triggers.len(), 1);
//! assert_eq!(data.triggers[0].name, b"Ambush");
//! assert_eq!(data.triggers[0].id, 17);
//! assert_eq!(data.next_trigger_id, 18); // maxTriggerId + 1
//! # Ok(())
//! # }
//! ```

use std::fmt;

use crate::map_chunk::{DataChunkInfo, DataChunkInput, ParserTable};
use crate::map_objects::MAP_XY_FACTOR;

// ---------------------------------------------------------------------------
// constants (MapReaderWriterInfo.h, MapObject.h)
// ---------------------------------------------------------------------------

/// The chunk label the constructor registers its polygon-triggers
/// reader under, at the global scope.
pub const POLYGON_TRIGGERS_LABEL: &[u8] = b"PolygonTriggers";

/// The triggers chunk version words (`K_TRIGGERS_VERSION_*`) — the
/// gates the record fields hang on (see the module docs).
pub mod triggers_version {
    /// Version 1: the flat shape, and the auto-added water area.
    pub const K_TRIGGERS_VERSION_1: u16 = 1;
    /// Version 2: adds the water-area byte.
    pub const K_TRIGGERS_VERSION_2: u16 = 2;
    /// Version 3: adds the river byte and the river-start word.
    pub const K_TRIGGERS_VERSION_3: u16 = 3;
    /// Version 4: adds the layer name — the version Zero Hour's
    /// WorldBuilder writes.
    pub const K_TRIGGERS_VERSION_4: u16 = 4;
}

pub use triggers_version::{
    K_TRIGGERS_VERSION_1, K_TRIGGERS_VERSION_2, K_TRIGGERS_VERSION_3, K_TRIGGERS_VERSION_4,
};

/// The auto-added water area's z word (`7` — "the old water position").
pub const AUTO_WATER_Z: i32 = 7;

// ---------------------------------------------------------------------------
// errors
// ---------------------------------------------------------------------------

/// Errors the polygon-triggers load path can produce.
///
/// The reader itself has no failure mode (every read is a zero-filled
/// short read at worst, and the leftover check is debug-only), so the
/// only failure is the walk's own refusal — the constructor throws the
/// same code for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolygonTriggersError {
    /// The walk itself failed — the header never opened as a `CkMp`
    /// table of contents (the constructor's `if (!file.parse(this))`
    /// throw).
    CorruptFileFormat,
}

impl fmt::Display for PolygonTriggersError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PolygonTriggersError::CorruptFileFormat => {
                write!(f, "map is not a chunky data-chunk file")
            }
        }
    }
}

impl std::error::Error for PolygonTriggersError {}

// ---------------------------------------------------------------------------
// the payload (the trigger's saved members, as the load leaves them)
// ---------------------------------------------------------------------------

/// One polygon trigger point (`ICoord3D` — the int triple).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ICoord3d {
    /// The x world coordinate (map squares scaled by the writer).
    pub x: i32,
    /// The y world coordinate.
    pub y: i32,
    /// The z word (7 for the auto-added water area).
    pub z: i32,
}

/// One kept trigger — the `PolygonTrigger` members the load writes, as
/// it leaves them.
#[derive(Debug, Clone, PartialEq)]
pub struct PolygonTriggerData {
    /// The trigger's name (`m_triggerName` — empty for the version-1
    /// auto-added area in release builds).
    pub name: Vec<u8>,
    /// The layer name (`m_layerName` — read from version 4, the
    /// constructor's empty default below).
    pub layer: Vec<u8>,
    /// The trigger id (`m_triggerID` — the file's word; the
    /// constructor's minted id is overwritten before anything observes
    /// it. The auto-added area's id duplicates the maximum).
    pub id: i32,
    /// The water-area flag (`m_isWaterArea`, from version 2).
    pub is_water: bool,
    /// The river flag (`m_isRiver`, from version 3).
    pub is_river: bool,
    /// The river start point (`m_riverStart`, from version 3).
    pub river_start: i32,
    /// The polygon's points (`m_points` — at least two for a kept
    /// trigger).
    pub points: Vec<ICoord3d>,
}

/// The `PolygonTriggers` payload: the kept triggers in list order and
/// the id counter as the load leaves it.
#[derive(Debug, Clone, PartialEq)]
pub struct PolygonTriggersData {
    /// The kept triggers, head first (file order).
    pub triggers: Vec<PolygonTriggerData>,
    /// The id counter (`PolygonTrigger::s_currentID`) after the load —
    /// `maxTriggerId + 1`, two past the pre-auto maximum when the
    /// version-1 auto area fired (its post-increment).
    pub next_trigger_id: i32,
}

// ---------------------------------------------------------------------------
// the reader (PolygonTrigger.cpp)
// ---------------------------------------------------------------------------

/// `PolygonTrigger::ParsePolygonTriggersDataChunk` — the
/// `PolygonTriggers` chunk reader: the entry reset, the flat count
/// loop with its version gates, the numPoints<2 deletion, the linkage,
/// the version-1 auto-added water area and the final counter.
///
/// The water extents are `TheGlobalData->m_waterExtentX/Y` (GlobalData
/// INI state, engine defaults 0) — pass the loaded INI values when the
/// consumer tracks them. The reader never fails.
#[allow(clippy::too_many_arguments)]
pub fn parse_polygon_triggers_data_chunk(
    file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
    water_extent_x: f32,
    water_extent_y: f32,
) -> PolygonTriggersData {
    // PolygonTrigger::deleteTriggers(); // just in case. — the parser
    // resets the whole payload at entry (a repeated chunk owns the
    // state outright).
    let mut triggers: Vec<PolygonTriggerData> = Vec::new();

    // the version gates are loop-invariant (the chunk's own word) —
    // hoisted once instead of re-tested per record (the same reads
    // either way, the same shape the original's per-record tests drive)
    let read_layer = info.version >= K_TRIGGERS_VERSION_4;
    let read_water = info.version >= K_TRIGGERS_VERSION_2;
    let read_river = info.version >= K_TRIGGERS_VERSION_3;

    let mut max_trigger_id = 0i32;
    let mut count = file.read_int();
    while count > 0 {
        count -= 1;

        let name = file.read_ascii_string();
        let layer = if read_layer {
            file.read_ascii_string()
        } else {
            Vec::new() // the constructor's default (never set below v4)
        };
        let id = file.read_int();
        let is_water = read_water && file.read_byte() != 0;
        let (is_river, river_start) = if read_river {
            (file.read_byte() != 0, file.read_int())
        } else {
            (false, 0)
        };

        let num_points = file.read_int();
        // newInstance(PolygonTrigger)(numPoints+1): the constructor
        // allocates max(numPoints+1, 2) slots once (reallocate never
        // runs on the load path), mints an id from s_currentID++ that
        // is overwritten below (not an observable) and sets the
        // editor-only water handle — modelled as the one capacity
        // reservation the original's array is. (The original's
        // `numPoints + 1` wrap on an absurd count is its own death —
        // the same unbounded class; the saturating add just keeps the
        // port from panicking first.)
        let mut points = Vec::new();
        if id > max_trigger_id {
            max_trigger_id = id;
        }
        // the point triplets read as one 12-byte block: a fresh zeroed
        // buffer per point leaves the same words three `read_int`s
        // leave on a short read (only the available prefix is
        // overwritten, the tail stays zero), with the stream
        // bookkeeping paid once per point instead of three times.
        for _ in 0..num_points {
            let mut word = [0u8; 12];
            file.read_array_of_bytes(&mut word);
            let loc = ICoord3d {
                x: i32::from_le_bytes([word[0], word[1], word[2], word[3]]),
                y: i32::from_le_bytes([word[4], word[5], word[6], word[7]]),
                z: i32::from_le_bytes([word[8], word[9], word[10], word[11]]),
            };
            points.push(loc); // addPoint (the capacity never runs out)
        }

        // the non-retail branch Zero Hour always takes: the trigger is
        // deleted after its points are read — the id already counted
        // toward the max, and the linkage cursor is unchanged.
        if num_points < 2 {
            // DEBUG_LOG(("Deleting polygon trigger '%s' with %d points.", ...))
            continue;
        }

        // the linkage: chain after the previous kept trigger, else
        // prepend to the (empty) head — both the vector's push.
        triggers.push(PolygonTriggerData {
            name,
            layer,
            id,
            is_water,
            is_river,
            river_start,
            points,
        });
    }

    if info.version == K_TRIGGERS_VERSION_1 {
        // before water areas existed, so create a default one.
        let auto = auto_water_area(&mut max_trigger_id, water_extent_x, water_extent_y);
        triggers.push(auto);
    }

    // s_currentID = maxTriggerId+1 (the deleteTriggers reset's 1 never
    // survives a parse — every chunk sets the counter at its end)
    PolygonTriggersData {
        triggers,
        next_trigger_id: max_trigger_id + 1,
    }
}

/// The version-1 auto-added water area: the id is the
/// **post-incremented** maximum (a duplicate of the largest id when any
/// trigger was kept), the release name is empty (an `RTS_DEBUG` build
/// names it `AutoAddedWaterAreaTrigger`), and the four corners are the
/// ±30-square bounds offset by the water extents (z 7, the old water
/// position).
fn auto_water_area(
    max_trigger_id: &mut i32,
    water_extent_x: f32,
    water_extent_y: f32,
) -> PolygonTriggerData {
    let mut trigger = PolygonTriggerData {
        name: Vec::new(),
        layer: Vec::new(),
        id: *max_trigger_id, // pTrig->m_triggerID = maxTriggerId++;
        is_water: true,
        is_river: false,
        river_start: 0,
        points: Vec::new(),
    };
    *max_trigger_id += 1;

    // the corner arithmetic: the float expressions truncate into the
    // int words exactly as the C++ assignment does (in-range values;
    // the port's `as` saturates where the C++ is undefined — the
    // corpora keep the extents sane).
    let lo = (-30.0 * MAP_XY_FACTOR) as i32;
    let hi_x = (30.0 * MAP_XY_FACTOR + water_extent_x) as i32;
    let hi_y = (30.0 * MAP_XY_FACTOR + water_extent_y) as i32;
    trigger.points = vec![
        ICoord3d {
            x: lo,
            y: lo,
            z: AUTO_WATER_Z,
        },
        ICoord3d {
            x: hi_x,
            y: lo,
            z: AUTO_WATER_Z,
        },
        ICoord3d {
            x: hi_x,
            y: hi_y,
            z: AUTO_WATER_Z,
        },
        ICoord3d {
            x: lo,
            y: hi_y,
            z: AUTO_WATER_Z,
        },
    ];
    trigger
}

// ---------------------------------------------------------------------------
// the walk (the constructor's logical-load shape, triggers only)
// ---------------------------------------------------------------------------

impl PolygonTriggersData {
    /// The `WorldHeightMap` constructor's **logical-data-only** walk,
    /// restricted to this payload: `PolygonTriggers` registered at the
    /// global scope, each chunk resetting and replacing the whole
    /// payload (the last chunk owns the final state).
    ///
    /// `Ok(None)` is the map with no `PolygonTriggers` chunk at all —
    /// the original's statics keep their registration-time state (the
    /// empty list, the counter at 1).
    ///
    /// The water extents are the engine defaults (0); use
    /// [`PolygonTriggersData::from_map_with_extents`] when the
    /// consumer's GlobalData carries other values. The other payloads
    /// the logical load registers arrive with their own slices — here
    /// their chunks are skipped whole, the walk's behaviour for an
    /// unmatched label.
    pub fn from_map(bytes: &[u8]) -> Result<Option<Self>, PolygonTriggersError> {
        Self::from_map_with_extents(bytes, 0.0, 0.0)
    }

    /// [`PolygonTriggersData::from_map`] with the `TheGlobalData`
    /// water-extent words the version-1 auto area's corners derive
    /// from (`WaterExtentX`/`WaterExtentY` INI state).
    pub fn from_map_with_extents(
        bytes: &[u8],
        water_extent_x: f32,
        water_extent_y: f32,
    ) -> Result<Option<Self>, PolygonTriggersError> {
        let mut input = DataChunkInput::new(bytes);
        let mut table: ParserTable<'_, TriggersWalk> = ParserTable::new();
        table.register(POLYGON_TRIGGERS_LABEL, b"", triggers_chunk_parser);
        let mut user = TriggersWalk {
            result: None,
            water_extent_x,
            water_extent_y,
        };
        if !input.parse(&mut table, &mut user) {
            // The reader has no failure mode, so a parser's false is
            // the walk's own refusal; the original throws the same code
            // (the constructor's `if (!file.parse(this))`).
            return Err(PolygonTriggersError::CorruptFileFormat);
        }
        Ok(user.result)
    }
}

/// The walk's user state: the payload the last chunk left and the
/// water extents (GlobalData INI state).
struct TriggersWalk {
    result: Option<PolygonTriggersData>,
    water_extent_x: f32,
    water_extent_y: f32,
}

/// The registered `PolygonTriggers` parser: dispatch to the reader and
/// keep the last chunk's state — the reader never refuses, so the
/// parser never returns false.
fn triggers_chunk_parser(
    file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
    _table: &mut ParserTable<'_, TriggersWalk>,
    user: &mut TriggersWalk,
) -> bool {
    user.result = Some(parse_polygon_triggers_data_chunk(
        file,
        info,
        user.water_extent_x,
        user.water_extent_y,
    ));
    true
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map_chunk::DataChunkOutput;

    /// A test file's table of contents.
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

    fn put_chunk(b: &mut Vec<u8>, id: u32, ver: u16, data: &[u8]) {
        b.extend_from_slice(&id.to_le_bytes());
        b.extend_from_slice(&ver.to_le_bytes());
        b.extend_from_slice(&(data.len() as i32).to_le_bytes());
        b.extend_from_slice(data);
    }

    /// One trigger's flat record at the given version's shape.
    #[allow(clippy::too_many_arguments)]
    fn trigger_record(
        version: u16,
        name: &[u8],
        layer: &[u8],
        id: i32,
        points: &[(i32, i32, i32)],
    ) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&(name.len() as u16).to_le_bytes());
        p.extend_from_slice(name);
        if version >= K_TRIGGERS_VERSION_4 {
            p.extend_from_slice(&(layer.len() as u16).to_le_bytes());
            p.extend_from_slice(layer);
        }
        put_i32(&mut p, id);
        if version >= K_TRIGGERS_VERSION_2 {
            p.push(1); // water
        }
        if version >= K_TRIGGERS_VERSION_3 {
            p.push(0); // river
            put_i32(&mut p, 0); // riverStart
        }
        put_i32(&mut p, points.len() as i32);
        for &(x, y, z) in points {
            put_i32(&mut p, x);
            put_i32(&mut p, y);
            put_i32(&mut p, z);
        }
        p
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
        assert_eq!(K_TRIGGERS_VERSION_1, 1);
        assert_eq!(K_TRIGGERS_VERSION_2, 2);
        assert_eq!(K_TRIGGERS_VERSION_3, 3);
        assert_eq!(K_TRIGGERS_VERSION_4, 4);
        assert_eq!(POLYGON_TRIGGERS_LABEL, b"PolygonTriggers");
        assert_eq!(MAP_XY_FACTOR, 10.0);
        assert_eq!(AUTO_WATER_Z, 7);
    }

    #[test]
    fn a_v4_list_loads_with_its_flags() {
        let toc = Toc::new(&[b"PolygonTriggers"]);
        let mut payload = Vec::new();
        put_i32(&mut payload, 1);
        payload.extend_from_slice(&trigger_record(
            K_TRIGGERS_VERSION_4,
            b"RiverNorth",
            b"Water",
            3,
            &[(10, 20, 0), (30, 20, 0), (30, 40, 0)],
        ));
        let mut chunk = Vec::new();
        put_chunk(
            &mut chunk,
            toc.id(b"PolygonTriggers"),
            K_TRIGGERS_VERSION_4,
            &payload,
        );
        let bytes = build_map(&toc, &[chunk]);

        let data = PolygonTriggersData::from_map(&bytes)
            .unwrap()
            .expect("fired");
        assert_eq!(data.triggers.len(), 1);
        assert_eq!(data.triggers[0].name, b"RiverNorth");
        assert_eq!(data.triggers[0].layer, b"Water");
        assert_eq!(data.triggers[0].id, 3);
        assert!(data.triggers[0].is_water); // the version-2 byte
        assert!(!data.triggers[0].is_river);
        assert_eq!(data.triggers[0].points.len(), 3);
        assert_eq!(data.triggers[0].points[2], ICoord3d { x: 30, y: 40, z: 0 });
        assert_eq!(data.next_trigger_id, 4);
    }

    #[test]
    fn the_v1_auto_area_duplicates_the_max_id() {
        let toc = Toc::new(&[b"PolygonTriggers"]);
        let mut payload = Vec::new();
        put_i32(&mut payload, 1);
        payload.extend_from_slice(&trigger_record(
            K_TRIGGERS_VERSION_1,
            b"AmbushV1",
            b"",
            5,
            &[(0, 0, 0), (10, 0, 0), (10, 10, 0)],
        ));
        let mut chunk = Vec::new();
        put_chunk(
            &mut chunk,
            toc.id(b"PolygonTriggers"),
            K_TRIGGERS_VERSION_1,
            &payload,
        );
        let bytes = build_map(&toc, &[chunk]);

        let data = PolygonTriggersData::from_map(&bytes)
            .unwrap()
            .expect("fired");
        assert_eq!(data.triggers.len(), 2);
        // the auto area: the max id 5 duplicated, unnamed, water
        let auto = &data.triggers[1];
        assert!(auto.name.is_empty());
        assert!(auto.is_water);
        assert!(!auto.is_river);
        assert_eq!(auto.id, 5);
        assert_eq!(
            auto.points,
            vec![
                ICoord3d {
                    x: -300,
                    y: -300,
                    z: 7
                },
                ICoord3d {
                    x: 300,
                    y: -300,
                    z: 7
                },
                ICoord3d {
                    x: 300,
                    y: 300,
                    z: 7
                },
                ICoord3d {
                    x: -300,
                    y: 300,
                    z: 7
                },
            ]
        );
        // maxTriggerId++ then +1: two past the pre-auto maximum
        assert_eq!(data.next_trigger_id, 7);
    }

    #[test]
    fn degenerate_triggers_are_deleted_after_their_points() {
        let toc = Toc::new(&[b"PolygonTriggers"]);
        let mut payload = Vec::new();
        put_i32(&mut payload, 3);
        // a zero-point trigger (id 21) — deleted, but the id counts
        payload.extend_from_slice(&trigger_record(
            K_TRIGGERS_VERSION_4,
            b"Empty",
            b"Ground",
            21,
            &[],
        ));
        // a survivor
        payload.extend_from_slice(&trigger_record(
            K_TRIGGERS_VERSION_4,
            b"Survivor",
            b"Ground",
            8,
            &[(0, 0, 0), (5, 5, 5)],
        ));
        // a one-point trigger (id 23) — deleted
        payload.extend_from_slice(&trigger_record(
            K_TRIGGERS_VERSION_4,
            b"Lonely",
            b"Water",
            23,
            &[(9, 9, 9)],
        ));
        let mut chunk = Vec::new();
        put_chunk(
            &mut chunk,
            toc.id(b"PolygonTriggers"),
            K_TRIGGERS_VERSION_4,
            &payload,
        );
        let bytes = build_map(&toc, &[chunk]);

        let data = PolygonTriggersData::from_map(&bytes)
            .unwrap()
            .expect("fired");
        assert_eq!(data.triggers.len(), 1);
        assert_eq!(data.triggers[0].name, b"Survivor");
        // the deleted ids still counted: max 23 → 24
        assert_eq!(data.next_trigger_id, 24);
    }

    #[test]
    fn the_water_extents_offset_the_auto_corners() {
        let toc = Toc::new(&[b"PolygonTriggers"]);
        let mut payload = Vec::new();
        put_i32(&mut payload, 0); // no triggers: the auto area alone
        let mut chunk = Vec::new();
        put_chunk(
            &mut chunk,
            toc.id(b"PolygonTriggers"),
            K_TRIGGERS_VERSION_1,
            &payload,
        );
        let bytes = build_map(&toc, &[chunk]);

        let data = PolygonTriggersData::from_map_with_extents(&bytes, 12.75, -0.9)
            .unwrap()
            .expect("fired");
        let auto = &data.triggers[0];
        assert_eq!(auto.id, 0); // the max started at 0
        assert_eq!(auto.points[1].x, 312); // 300 + 12.75 truncated
        assert_eq!(auto.points[2].y, 299); // 300 - 0.9 truncated
        assert_eq!(auto.points[3].y, 299);
        assert_eq!(data.next_trigger_id, 2);
    }

    #[test]
    fn an_absent_chunk_is_none_and_a_nested_one_never_fires() {
        // no PolygonTriggers at all: the counter keeps its
        // registration-time 1
        let toc = Toc::new(&[b"HeightMapData"]);
        let mut chunk = Vec::new();
        put_chunk(&mut chunk, 1, 1, &[]);
        let bytes = build_map(&toc, &[chunk]);
        assert_eq!(PolygonTriggersData::from_map(&bytes).unwrap(), None);

        // nested inside a foreign container: the walk skips it whole
        let toc = Toc::new(&[b"Container", b"PolygonTriggers"]);
        let mut inner = Vec::new();
        let mut payload = Vec::new();
        put_i32(&mut payload, 1);
        payload.extend_from_slice(&trigger_record(
            K_TRIGGERS_VERSION_4,
            b"Hidden",
            b"L",
            1,
            &[(1, 2, 3), (4, 5, 6)],
        ));
        put_chunk(&mut inner, 2, K_TRIGGERS_VERSION_4, &payload);
        let mut chunk = Vec::new();
        put_chunk(&mut chunk, 1, 1, &inner);
        let bytes = build_map(&toc, &[chunk]);
        assert_eq!(PolygonTriggersData::from_map(&bytes).unwrap(), None);
    }

    #[test]
    fn the_walk_refuses_non_chunky_bytes() {
        assert_eq!(
            PolygonTriggersData::from_map(b"not a map at all"),
            Err(PolygonTriggersError::CorruptFileFormat)
        );
    }

    /// The example in the module docs (kept here so a doctest failure
    /// names the unit).
    #[test]
    fn the_doctest_shape_loads() {
        let mut out = DataChunkOutput::new();
        out.open_data_chunk(POLYGON_TRIGGERS_LABEL, K_TRIGGERS_VERSION_4);
        out.write_int(1);
        out.write_ascii_string(b"Ambush");
        out.write_ascii_string(b"Ground");
        out.write_int(17);
        out.write_byte(0);
        out.write_byte(0);
        out.write_int(0);
        out.write_int(2);
        out.write_int(0);
        out.write_int(0);
        out.write_int(0);
        out.write_int(100);
        out.write_int(100);
        out.write_int(0);
        out.close_data_chunk();
        let bytes = out.into_bytes().expect("writes");
        let data = PolygonTriggersData::from_map(&bytes)
            .unwrap()
            .expect("fired");
        assert_eq!(data.triggers.len(), 1);
        assert_eq!(data.next_trigger_id, 18);
    }
}
