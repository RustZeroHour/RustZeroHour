// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Ported from the Zero Hour source release (baseline 571a7c5):
//   Core/GameEngineDevice/Source/W3DDevice/GameClient/WorldHeightMap.cpp
//     (WorldHeightMap::ParseLightingDataChunk — the GlobalLighting chunk
//     reader the constructor's full load registers, with its init pass and
//     version-gated light reads, and the constructor's registerParser/
//     parse walk shape the reader fires from)
//   Core/GameEngine/Include/Common/GlobalData.h
//     (GlobalData::TerrainLighting and its RGBColor/Coord3D members, the
//     m_terrainLighting/m_terrainObjectsLighting tables the reader fills,
//     MAX_GLOBAL_LIGHTS, m_timeOfDay)
//   Core/GameEngine/Include/Common/GameType.h
//     (the TimeOfDay enum — TIME_OF_DAY_INVALID..TIME_OF_DAY_NIGHT,
//     TIME_OF_DAY_FIRST — and Coord3D)
//   Core/GameEngine/Include/Common/MapReaderWriterInfo.h
//     (K_LIGHTING_VERSION_1..3 — the version words and their gates)
//   GeneralsMD/Code/GameEngineDevice/Include/W3DDevice/GameClient/W3DShadow.h
//     (setShadowColor — the ARGB consumer action the shadow word feeds)
//
// See the module docs for the format layout and the documented differences
// from the C++ (the GlobalData and shadow-manager singletons become the
// returned value; the INVALID table row is INI state the chunk never
// writes).

//! The **map global-lighting payload** — the `GlobalLighting` chunk of a
//! `.map` file, on top of [`crate::map_chunk`].
//!
//! A Zero Hour map's lighting is one `GlobalLighting` chunk at the top
//! level of the data-chunk tree: the time of day the map plays in, the
//! ambient/diffuse colours and positions of up to three terrain lights
//! and three object lights for each of the four times of day, and the
//! shadow colour. `WorldHeightMap`'s constructor registers a reader for
//! it in the **full** load only (the logical-data-only load never looks
//! at it), and this module ports that reader over the
//! [`DataChunkInput`](crate::map_chunk::DataChunkInput) walk, quirks
//! included (parity level P0, pinned by golden vectors generated from
//! the transcription in `tools/differential/gen_maplighting_golden.cpp`).
//!
//! # The chunk in one page
//!
//! ```text
//! i32 time_of_day         the raw TimeOfDay word (never validated)
//! 4 × {                   one block per time of day (Morning..Night)
//!   9 × f32               terrain light 0: ambient rgb, diffuse rgb, pos
//!   9 × f32               object light 0: the same nine words
//!   [v >= 2] 2 × 9 × f32  object lights 1-2
//!   [v >= 3] 2 × 9 × f32  terrain lights 1-2
//! }
//! [i32 shadow_color]      present exactly when the chunk has bytes left
//! ```
//!
//! The blocks are **interleaved per time of day**, not grouped by table:
//! each block reads terrain light 0, then object light 0, then the extra
//! object lights, then the extra terrain lights — the order WorldBuilder
//! writes (`WHeightMapEdit.cpp`) and the reader transcribes.
//!
//! # The load-path quirks (transcribed)
//!
//! - The **version words are ceilings**: the extra object lights exist
//!   from [`K_LIGHTING_VERSION_2`], the extra terrain lights from
//!   [`K_LIGHTING_VERSION_3`]; a version 0 chunk reads like a 1, a
//!   version 4 like a 3.
//! - Before the reads the reader fills **both tables completely** with
//!   [`TerrainLighting::INIT`] — ambient and diffuse black, position
//!   (0, 0, -1) — to keep the directions "not totally invalid, in case
//!   old maps are read", so the lights a version's gates skip keep the
//!   init values.
//! - The time-of-day word is stored **raw** (`(TimeOfDay)readInt()`,
//!   never validated): the INVALID slot's 0, NIGHT's 4 and garbage alike
//!   survive as the loaded `m_timeOfDay`.
//! - The shadow colour is gated on **bytes left**, not version: any
//!   chunk with data remaining after the lights reads one more word (an
//!   ARGB colour for the shadow manager). A version-1 chunk with four
//!   trailing bytes reads it; a version-3 chunk exactly consumed does
//!   not.
//! - The reader **never checks it consumed the chunk** (the
//!   `atEndOfChunk` assert is debug-only) and **has no failure mode**:
//!   every read is a zero-filled short read at worst, so the only
//!   refusal the walk can see is the container's own (a non-`CkMp`
//!   header).
//! - The walk is the constructor's: `GlobalLighting` registers at the
//!   **global scope**, so a chunk nested inside another chunk never
//!   fires the reader, and repeated chunks **overwrite** each other
//!   (the last one wins).
//!
//! # Differences from the C++ (documented)
//!
//! - The original writes `TheWritableGlobalData` (`m_timeOfDay`,
//!   `m_terrainLighting[TIME_OF_DAY_COUNT][MAX_GLOBAL_LIGHTS]`,
//!   `m_terrainObjectsLighting` — the same shape) and hands the shadow
//!   colour to `TheW3DShadowManager`. Those singletons become the
//!   returned [`GlobalLighting`]: the chunk's content is the
//!   observable, and merging it into GlobalData's INI state is the
//!   consumer's job. The tables' `TIME_OF_DAY_INVALID` row — INI state
//!   the chunk never writes — is not represented; the four
//!   Morning..Night rows are.
//! - `setShadowColor` is a consumer action on a singleton; the value
//!   ([`GlobalLighting::shadow_color`], ARGB) is what the load keeps.
//! - Parser registration is a [`ParserTable`](crate::map_chunk::ParserTable)
//!   handed to [`DataChunkInput::parse`](crate::map_chunk::DataChunkInput::parse)
//!   instead of a list inside the input — the container's documented
//!   deviation, the same shape the other payload modules use.
//!
//! # Examples
//!
//! ```
//! use rzh_formats::map_chunk::{DataChunkInfo, DataChunkInput, DataChunkOutput};
//! use rzh_formats::map_lighting::{
//!     GLOBAL_LIGHTING_LABEL, GlobalLighting, TimeOfDay, TerrainLighting,
//!     K_LIGHTING_VERSION_2,
//! };
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // A version-2 lighting chunk (terrain and object light 0 per time of
//! // day, plus the two extra object lights), built with the container's
//! // writer the way WorldBuilder saves one.
//! fn light(out: &mut DataChunkOutput, base: f32) {
//!     for k in 0..9 {
//!         out.write_real(base + k as f32 * 0.25);
//!     }
//! }
//! let mut out = DataChunkOutput::new();
//! out.open_data_chunk(GLOBAL_LIGHTING_LABEL, K_LIGHTING_VERSION_2);
//! out.write_int(4); // time of day: NIGHT, stored raw
//! for i in 0..4 {
//!     light(&mut out, i as f32);        // terrain light 0
//!     light(&mut out, 10.0 + i as f32); // object light 0
//!     for j in 1..3 {
//!         light(&mut out, 20.0 + (j * 4 + i) as f32); // object lights 1-2
//!     }
//!     // no version-3 terrain lights 1-2: they keep the init values
//! }
//! out.close_data_chunk();
//! let bytes = out.into_bytes()?;
//!
//! // The constructor's walk: the map's GlobalLighting, or None.
//! let gl = GlobalLighting::from_map(&bytes)?.expect("the chunk is there");
//! assert_eq!(gl.time_of_day, 4);
//! assert_eq!(gl.terrain_lighting[TimeOfDay::Morning.index()][0].ambient.red, 0.0);
//! assert_eq!(gl.terrain_objects_lighting[TimeOfDay::Night.index()][2].light_pos.z, 20.0 + 11.0 + 8.0 * 0.25);
//! // the version-2 gates skip the terrain lights 1-2: init values
//! assert_eq!(gl.terrain_lighting[TimeOfDay::Afternoon.index()][1], TerrainLighting::INIT);
//! assert_eq!(gl.shadow_color, None);
//!
//! // The chunk reader, driven directly the way a registered parser is
//! // (the info's version word is the reader's only field).
//! let mut input = DataChunkInput::new(&bytes);
//! input.open_data_chunk();
//! let info = DataChunkInfo {
//!     label: GLOBAL_LIGHTING_LABEL.to_vec(),
//!     parent_label: Vec::new(),
//!     version: input.get_chunk_version(),
//!     data_size: input.get_chunk_data_size() as i32,
//! };
//! let gl = rzh_formats::map_lighting::parse_lighting_data_chunk(&mut input, &info);
//! assert_eq!(gl.time_of_day, 4);
//! # Ok(())
//! # }
//! ```

use std::fmt;

use crate::map_chunk::{DataChunkInfo, DataChunkInput, ParserTable};

// ---------------------------------------------------------------------------
// constants (MapReaderWriterInfo.h, GameType.h, GlobalData.h)
// ---------------------------------------------------------------------------

/// The chunk label the constructor registers its lighting reader under,
/// at the global scope.
pub const GLOBAL_LIGHTING_LABEL: &[u8] = b"GlobalLighting";

/// The light-table width (`MAX_GLOBAL_LIGHTS` — one light 0 plus the two
/// extra lights versions 2 and 3 added).
pub const MAX_GLOBAL_LIGHTS: usize = 3;

/// How many time-of-day slots the chunk carries lighting for (the
/// original's `for (i = 0; i < 4; i++)` loop — Morning through Night;
/// the tables' INVALID row is GlobalData INI state, not chunk content).
pub const LIGHTING_TIME_OF_DAY_COUNT: usize = 4;

/// The `TimeOfDay` slot values the chunk's content lives between
/// (`GameType.h`; the arrays are indexed `i + TIME_OF_DAY_FIRST`).
pub mod time_of_day {
    /// `TIME_OF_DAY_INVALID` — the GlobalData slot the chunk never
    /// writes (its value stays the INI default).
    pub const TIME_OF_DAY_INVALID: i32 = 0;
    /// `TIME_OF_DAY_FIRST` — `TIME_OF_DAY_MORNING`, the first slot the
    /// reader writes.
    pub const TIME_OF_DAY_FIRST: i32 = 1;
    /// `TIME_OF_DAY_COUNT` — the full table width, INVALID included.
    pub const TIME_OF_DAY_COUNT: usize = 5;
}

/// The four times of day the lighting tables carry (`TimeOfDay`, minus
/// the INVALID slot the chunk never writes).
///
/// Index the [`GlobalLighting`] tables with
/// [`index`](Self::index) — the `i + TIME_OF_DAY_FIRST` row of the
/// original's arrays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(i32)]
pub enum TimeOfDay {
    /// `TIME_OF_DAY_MORNING`.
    Morning = 1,
    /// `TIME_OF_DAY_AFTERNOON`.
    Afternoon = 2,
    /// `TIME_OF_DAY_EVENING`.
    Evening = 3,
    /// `TIME_OF_DAY_NIGHT`.
    Night = 4,
}

impl TimeOfDay {
    /// All four slots, in the reader's loop order.
    pub const ALL: [TimeOfDay; LIGHTING_TIME_OF_DAY_COUNT] = [
        TimeOfDay::Morning,
        TimeOfDay::Afternoon,
        TimeOfDay::Evening,
        TimeOfDay::Night,
    ];

    /// The table row — the original's `i + TIME_OF_DAY_FIRST` index.
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize - 1
    }

    /// The original's enum value (what the raw time-of-day word compares
    /// against).
    #[must_use]
    pub const fn value(self) -> i32 {
        self as i32
    }
}

/// The lighting chunk version words (`K_LIGHTING_VERSION_*`) — the gates
/// the reader's light reads hang on (see the module docs).
pub mod lighting_version {
    /// Version 1: light 0 of each table, per time of day.
    pub const K_LIGHTING_VERSION_1: u16 = 1;
    /// Version 2: added 2 additional global lights for objects.
    pub const K_LIGHTING_VERSION_2: u16 = 2;
    /// Version 3: added 2 additional global lights for terrain — the
    /// version Zero Hour's WorldBuilder writes.
    pub const K_LIGHTING_VERSION_3: u16 = 3;
}

pub use lighting_version::{K_LIGHTING_VERSION_1, K_LIGHTING_VERSION_2, K_LIGHTING_VERSION_3};

// ---------------------------------------------------------------------------
// errors
// ---------------------------------------------------------------------------

/// Errors the global-lighting load path can produce.
///
/// The payload reader has no failure mode of its own — every read is a
/// zero-filled short read at worst, and the version gates are ceilings —
/// so the only refusal is the walk's own (the container's
/// `if (!file.parse(this))` throw).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LightingError {
    /// The walk itself failed — the header never opened as a `CkMp`
    /// table of contents (the constructor's `if (!file.parse(this))`
    /// throw).
    CorruptFileFormat,
}

impl fmt::Display for LightingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LightingError::CorruptFileFormat => {
                write!(f, "map is not a chunky data-chunk file")
            }
        }
    }
}

impl std::error::Error for LightingError {}

// ---------------------------------------------------------------------------
// the payload (GlobalData's lighting members, as the load leaves them)
// ---------------------------------------------------------------------------

/// One colour triple (`RGBColor`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RgbColor {
    /// The red component.
    pub red: f32,
    /// The green component.
    pub green: f32,
    /// The blue component.
    pub blue: f32,
}

/// One position triple (`Coord3D` — the common `GameType.h` type the
/// light positions use; the objects payload reuses it for its
/// locations).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Coord3d {
    /// The x component.
    pub x: f32,
    /// The y component.
    pub y: f32,
    /// The z component.
    pub z: f32,
}

/// One light's state (`GlobalData::TerrainLighting`): the ambient and
/// diffuse colours and the position the light shines from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TerrainLighting {
    /// The ambient colour (`ambient`).
    pub ambient: RgbColor,
    /// The diffuse colour (`diffuse`).
    pub diffuse: RgbColor,
    /// The light's position (`lightPos`).
    pub light_pos: Coord3d,
}

impl TerrainLighting {
    /// The `initLightValues` the reader fills both tables with before
    /// the version-gated reads — ambient and diffuse black, position
    /// (0, 0, -1) — "not totally invalid, in case old maps are read".
    pub const INIT: TerrainLighting = TerrainLighting {
        ambient: RgbColor {
            red: 0.0,
            green: 0.0,
            blue: 0.0,
        },
        diffuse: RgbColor {
            red: 0.0,
            green: 0.0,
            blue: 0.0,
        },
        light_pos: Coord3d {
            x: 0.0,
            y: 0.0,
            z: -1.0,
        },
    };
}

/// The global-lighting state one `GlobalLighting` chunk loads — the
/// GlobalData members the reader writes, as the load leaves them.
///
/// The tables are indexed by [`TimeOfDay::index`] (Morning is row 0) and
/// light number (0..[`MAX_GLOBAL_LIGHTS`]); the lights a version's gates
/// skip hold [`TerrainLighting::INIT`].
#[derive(Debug, Clone, PartialEq)]
pub struct GlobalLighting {
    /// The raw time-of-day word (`m_timeOfDay` — a `TimeOfDay` cast of
    /// the `i32` read, never validated; garbage survives).
    pub time_of_day: i32,
    /// The terrain light table (`m_terrainLighting`), Morning..Night
    /// rows of [`MAX_GLOBAL_LIGHTS`] lights.
    pub terrain_lighting: [[TerrainLighting; MAX_GLOBAL_LIGHTS]; LIGHTING_TIME_OF_DAY_COUNT],
    /// The object light table (`m_terrainObjectsLighting`), the same
    /// shape.
    pub terrain_objects_lighting:
        [[TerrainLighting; MAX_GLOBAL_LIGHTS]; LIGHTING_TIME_OF_DAY_COUNT],
    /// The shadow colour word (`setShadowColor`'s ARGB argument), read
    /// exactly when the chunk had bytes left after the lights — `None`
    /// when it did not (the shadow manager keeps its prior colour).
    pub shadow_color: Option<u32>,
}

impl Default for GlobalLighting {
    fn default() -> Self {
        Self::new()
    }
}

impl GlobalLighting {
    /// The pre-read state: both tables at [`TerrainLighting::INIT`], the
    /// raw time-of-day word 0, no shadow colour — what an absent chunk
    /// leaves GlobalData's members at (the INI defaults are the
    /// consumer's).
    #[must_use]
    pub fn new() -> Self {
        GlobalLighting {
            time_of_day: 0,
            terrain_lighting: [[TerrainLighting::INIT; MAX_GLOBAL_LIGHTS];
                LIGHTING_TIME_OF_DAY_COUNT],
            terrain_objects_lighting: [[TerrainLighting::INIT; MAX_GLOBAL_LIGHTS];
                LIGHTING_TIME_OF_DAY_COUNT],
            shadow_color: None,
        }
    }

    /// The `WorldHeightMap` constructor's **full-load** walk, restricted
    /// to this payload: the registered-parser stroll over the map's
    /// top-level chunks that returns the **last** `GlobalLighting` the
    /// map carries — or `None` when it carries none (GlobalData keeps
    /// its INI state).
    ///
    /// The other payloads the constructor registers (the height map and
    /// blend tiles of the full load) arrive with their own slices —
    /// here their chunks are skipped whole, the walk's behaviour for an
    /// unmatched label. The only failure is the walk's own refusal (a
    /// non-`CkMp` header); the reader itself never fails.
    pub fn from_map(bytes: &[u8]) -> Result<Option<Self>, LightingError> {
        let mut input = DataChunkInput::new(bytes);
        let mut table: ParserTable<'_, LightingWalk> = ParserTable::new();
        table.register(GLOBAL_LIGHTING_LABEL, b"", lighting_chunk_parser);
        let mut user = LightingWalk { result: None };
        if !input.parse(&mut table, &mut user) {
            // The reader has no failure mode, so a parser's false is the
            // walk's own refusal; the original throws the same code (the
            // constructor's `if (!file.parse(this))`).
            return Err(LightingError::CorruptFileFormat);
        }
        Ok(user.result)
    }
}

/// The walk's user data: the lighting state the parser wrote (the
/// original's `this`, cut down to the GlobalData lighting half).
struct LightingWalk {
    result: Option<GlobalLighting>,
}

/// The registered parser the constructor's shape implies: dispatch to
/// the reader and keep the last good state — the reader never refuses,
/// so the parser never returns false.
fn lighting_chunk_parser(
    file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
    _table: &mut ParserTable<'_, LightingWalk>,
    user: &mut LightingWalk,
) -> bool {
    user.result = Some(parse_lighting_data_chunk(file, info));
    true
}

// ---------------------------------------------------------------------------
// the reader (WorldHeightMap.cpp)
// ---------------------------------------------------------------------------

/// `WorldHeightMap::ParseLightingDataChunk` — the full load's chunk
/// reader: the raw time-of-day word, the init pass over both tables, the
/// four interleaved per-time-of-day blocks (terrain light 0, object
/// light 0, the version-2 object lights 1-2, the version-3 terrain
/// lights 1-2), and the bytes-left-gated shadow colour.
///
/// The reader has no failure mode: short reads zero-fill, the version
/// gates are ceilings, and the leftover bytes go to the walk's close.
pub fn parse_lighting_data_chunk(
    file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
) -> GlobalLighting {
    let mut out = GlobalLighting::new();
    out.time_of_day = file.read_int();

    // (both tables were born at INIT by new() — the original's explicit
    // init pass, which fills the slots the version gates will skip)
    for i in 0..LIGHTING_TIME_OF_DAY_COUNT {
        read_light(file, &mut out.terrain_lighting[i][0]);
        read_light(file, &mut out.terrain_objects_lighting[i][0]);

        if info.version >= K_LIGHTING_VERSION_2 {
            // added support for 2 extra object lights
            for j in 1..MAX_GLOBAL_LIGHTS {
                read_light(file, &mut out.terrain_objects_lighting[i][j]);
            }
        }
        if info.version >= K_LIGHTING_VERSION_3 {
            // added support for 2 extra terrain lights
            for j in 1..MAX_GLOBAL_LIGHTS {
                read_light(file, &mut out.terrain_lighting[i][j]);
            }
        }
    }

    if !file.at_end_of_chunk() {
        // TheW3DShadowManager->setShadowColor(shadowColor) — the
        // consumer action; the ARGB word is the observable.
        out.shadow_color = Some(file.read_int() as u32);
    }

    // DEBUG_ASSERTCRASH(file.atEndOfChunk(), ("Unexpected data left
    // over.")) — a debug-only contract; the release load leaves the
    // leftovers to the walk's close, and so does the port.

    out
}

/// One light's nine reals, in the reader's field order (ambient rgb,
/// diffuse rgb, position xyz).
fn read_light(file: &mut DataChunkInput<'_>, light: &mut TerrainLighting) {
    light.ambient.red = file.read_real();
    light.ambient.green = file.read_real();
    light.ambient.blue = file.read_real();
    light.diffuse.red = file.read_real();
    light.diffuse.green = file.read_real();
    light.diffuse.blue = file.read_real();
    light.light_pos.x = file.read_real();
    light.light_pos.y = file.read_real();
    light.light_pos.z = file.read_real();
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map_chunk::DataChunkOutput;

    /// Builds a map file around one GlobalLighting payload.
    struct MapFile {
        out: DataChunkOutput,
    }

    impl MapFile {
        fn new() -> Self {
            MapFile {
                out: DataChunkOutput::new(),
            }
        }

        fn lighting(&mut self, version: u16, payload: &[u8]) -> &mut Self {
            self.out.open_data_chunk(GLOBAL_LIGHTING_LABEL, version);
            self.out.write_array_of_bytes(payload);
            self.out.close_data_chunk();
            self
        }

        /// A container chunk (the ObjectsList shape) whose children the
        /// walk descends into.
        fn container(&mut self, children: &[u8]) -> &mut Self {
            self.out.open_data_chunk(b"ObjectsList", 3);
            self.out.write_array_of_bytes(children);
            self.out.close_data_chunk();
            self
        }

        fn bytes(&mut self) -> Vec<u8> {
            self.out.clone().into_bytes().expect("finish")
        }
    }

    fn put_i32(b: &mut Vec<u8>, v: i32) {
        b.extend_from_slice(&v.to_le_bytes());
    }

    fn put_f32(b: &mut Vec<u8>, v: f32) {
        b.extend_from_slice(&v.to_le_bytes());
    }

    /// One light's nine words: `base + k/4` — exactly representable.
    fn put_light(b: &mut Vec<u8>, base: f32) {
        for k in 0..9u32 {
            put_f32(b, base + k as f32 * 0.25);
        }
    }

    /// A full payload in the reader's interleaved order.
    fn payload(version: u16, tod: i32, base: f32, shadow: Option<u32>) -> Vec<u8> {
        let mut p = Vec::new();
        put_i32(&mut p, tod);
        for i in 0..4u32 {
            put_light(&mut p, base + i as f32); // terrain light 0
            put_light(&mut p, base + 10.0 + i as f32); // object light 0
            if version >= K_LIGHTING_VERSION_2 {
                for j in 1..3u32 {
                    put_light(&mut p, base + 20.0 + (j * 4 + i) as f32);
                }
            }
            if version >= K_LIGHTING_VERSION_3 {
                for j in 1..3u32 {
                    put_light(&mut p, base + 40.0 + (j * 4 + i) as f32);
                }
            }
        }
        if let Some(s) = shadow {
            put_i32(&mut p, s as i32);
        }
        p
    }

    #[test]
    fn the_version_gates_match_the_format() {
        assert_eq!(K_LIGHTING_VERSION_1, 1);
        assert_eq!(K_LIGHTING_VERSION_2, 2);
        assert_eq!(K_LIGHTING_VERSION_3, 3);
        assert_eq!(GLOBAL_LIGHTING_LABEL, b"GlobalLighting");
        assert_eq!(MAX_GLOBAL_LIGHTS, 3);
        assert_eq!(LIGHTING_TIME_OF_DAY_COUNT, 4);
        assert_eq!(time_of_day::TIME_OF_DAY_INVALID, 0);
        assert_eq!(time_of_day::TIME_OF_DAY_FIRST, 1);
        assert_eq!(time_of_day::TIME_OF_DAY_COUNT, 5);
        for (i, tod) in TimeOfDay::ALL.iter().enumerate() {
            assert_eq!(tod.index(), i);
            assert_eq!(tod.value(), i as i32 + 1);
        }
        assert_eq!(TimeOfDay::Morning.value(), 1);
        assert_eq!(TimeOfDay::Night.value(), 4);
        // the init values
        assert_eq!(TerrainLighting::INIT.ambient, RgbColor::default());
        assert_eq!(TerrainLighting::INIT.diffuse, RgbColor::default());
        assert_eq!(TerrainLighting::INIT.light_pos.z, -1.0);
    }

    #[test]
    fn the_interleaved_block_order_reads_into_the_right_slots() {
        // Every word distinct: light L's word k is 10·L + 0.125·k — the
        // payload pins which slot each read lands in. The per-block light
        // order is terrain0, objects0, objects1, objects2, terrain1,
        // terrain2.
        let mut p = Vec::new();
        put_i32(&mut p, 2);
        let mut flat = 0u32;
        for _ in 0..4 {
            for _ in 0..6 {
                for k in 0..9u32 {
                    put_f32(&mut p, flat as f32 * 10.0 + k as f32 * 0.125);
                }
                flat += 1;
            }
        }
        let mut map = MapFile::new();
        map.lighting(K_LIGHTING_VERSION_3, &p);
        let bytes = map.bytes();
        let gl = GlobalLighting::from_map(&bytes)
            .expect("loads")
            .expect("present");

        let word = |l: &TerrainLighting, k: usize| {
            let nine = [
                l.ambient.red,
                l.ambient.green,
                l.ambient.blue,
                l.diffuse.red,
                l.diffuse.green,
                l.diffuse.blue,
                l.light_pos.x,
                l.light_pos.y,
                l.light_pos.z,
            ];
            nine[k]
        };
        // morning block: lights 0..5
        assert_eq!(word(&gl.terrain_lighting[0][0], 0), 0.0);
        assert_eq!(
            word(&gl.terrain_objects_lighting[0][0], 3),
            10.0 + 3.0 * 0.125
        );
        assert_eq!(word(&gl.terrain_objects_lighting[0][1], 0), 20.0);
        assert_eq!(word(&gl.terrain_objects_lighting[0][2], 8), 30.0 + 1.0);
        assert_eq!(word(&gl.terrain_lighting[0][1], 0), 40.0);
        assert_eq!(word(&gl.terrain_lighting[0][2], 1), 50.0 + 0.125);
        // the night block (the fourth): lights 18..24
        assert_eq!(word(&gl.terrain_lighting[3][0], 0), 180.0);
        assert_eq!(word(&gl.terrain_lighting[3][2], 8), 230.0 + 1.0);
        assert_eq!(word(&gl.terrain_objects_lighting[3][2], 8), 210.0 + 1.0);
        assert_eq!(gl.shadow_color, None);
    }

    #[test]
    fn a_v2_chunk_leaves_the_extra_terrain_lights_at_init() {
        let mut map = MapFile::new();
        map.lighting(
            K_LIGHTING_VERSION_2,
            &payload(K_LIGHTING_VERSION_2, 4, 1.5, None),
        );
        let bytes = map.bytes();
        let gl = GlobalLighting::from_map(&bytes)
            .expect("loads")
            .expect("present");

        assert_eq!(gl.time_of_day, 4);
        for i in 0..LIGHTING_TIME_OF_DAY_COUNT {
            // light 0 and the object lights 1-2 read
            assert_eq!(gl.terrain_lighting[i][0].ambient.red, 1.5 + i as f32);
            assert_eq!(
                gl.terrain_objects_lighting[i][2].light_pos.z,
                1.5 + 20.0 + (8 + i) as f32 + 2.0
            );
            // the terrain lights 1-2 keep the init values
            assert_eq!(gl.terrain_lighting[i][1], TerrainLighting::INIT);
            assert_eq!(gl.terrain_lighting[i][2], TerrainLighting::INIT);
            // the object lights 1-2 are read
            assert_ne!(gl.terrain_objects_lighting[i][1], TerrainLighting::INIT);
        }
        assert_eq!(gl.shadow_color, None);
    }

    #[test]
    fn the_version_ceilings_read_like_their_family() {
        // version 0 reads like a 1: only light 0 of each table, and the
        // shadow word still gates on the bytes left.
        let mut map = MapFile::new();
        map.lighting(0, &payload(0, 1, 2.5, Some(0x0a0b0c0d)));
        let bytes = map.bytes();
        let gl = GlobalLighting::from_map(&bytes)
            .expect("loads")
            .expect("present");
        assert_eq!(gl.terrain_lighting[0][0].ambient.red, 2.5);
        assert_eq!(gl.terrain_objects_lighting[2][0].ambient.red, 2.5 + 12.0);
        for i in 0..LIGHTING_TIME_OF_DAY_COUNT {
            assert_eq!(gl.terrain_lighting[i][1], TerrainLighting::INIT);
            assert_eq!(gl.terrain_objects_lighting[i][1], TerrainLighting::INIT);
            assert_eq!(gl.terrain_lighting[i][2], TerrainLighting::INIT);
            assert_eq!(gl.terrain_objects_lighting[i][2], TerrainLighting::INIT);
        }
        assert_eq!(gl.shadow_color, Some(0x0a0b0c0d));

        // a future version 4 reads like a 3: every light read.
        let mut map = MapFile::new();
        map.lighting(4, &payload(4, 1, 3.5, None));
        let bytes = map.bytes();
        let gl = GlobalLighting::from_map(&bytes)
            .expect("loads")
            .expect("present");
        assert_eq!(gl.terrain_lighting[0][1].ambient.red, 3.5 + 44.0);
        assert_eq!(
            gl.terrain_objects_lighting[3][2].light_pos.z,
            3.5 + 20.0 + 11.0 + 2.0
        );
    }

    #[test]
    fn the_shadow_word_gates_on_bytes_left_not_version() {
        // a version-1 chunk with a trailing word reads the shadow colour
        let mut map = MapFile::new();
        let mut p = payload(K_LIGHTING_VERSION_1, 3, 5.0, None);
        put_i32(&mut p, 0x4040_4040);
        map.lighting(K_LIGHTING_VERSION_1, &p);
        let bytes = map.bytes();
        let gl = GlobalLighting::from_map(&bytes)
            .expect("loads")
            .expect("present");
        assert_eq!(gl.shadow_color, Some(0x4040_4040));

        // a version-3 chunk exactly consumed does not
        let mut map = MapFile::new();
        map.lighting(
            K_LIGHTING_VERSION_3,
            &payload(K_LIGHTING_VERSION_3, 2, 6.0, None),
        );
        let bytes = map.bytes();
        let gl = GlobalLighting::from_map(&bytes)
            .expect("loads")
            .expect("present");
        assert_eq!(gl.shadow_color, None);
    }

    #[test]
    fn short_reads_zero_fill_and_skip_the_shadow() {
        // a chunk with only the time-of-day word and one stray real
        let mut map = MapFile::new();
        let mut p = Vec::new();
        put_i32(&mut p, 2);
        put_i32(&mut p, 0x3f80_0000);
        map.lighting(K_LIGHTING_VERSION_3, &p);
        let bytes = map.bytes();
        let gl = GlobalLighting::from_map(&bytes)
            .expect("loads")
            .expect("present");
        assert_eq!(gl.time_of_day, 2);
        // the stray real becomes terrain light 0's ambient red; the rest
        // zero-fills (not INIT — the reads ran and wrote zeros)
        assert_eq!(
            gl.terrain_lighting[0][0].ambient.red,
            f32::from_bits(0x3f80_0000)
        );
        assert_eq!(gl.terrain_lighting[0][0].ambient.green, 0.0);
        assert_eq!(gl.terrain_lighting[3][2].light_pos.z, 0.0);
        // the data-left word went negative: no shadow read
        assert_eq!(gl.shadow_color, None);
    }

    #[test]
    fn repeated_chunks_overwrite_and_nested_never_fire() {
        // last-wins at the top level
        let mut map = MapFile::new();
        map.lighting(
            K_LIGHTING_VERSION_2,
            &payload(K_LIGHTING_VERSION_2, 1, 9.0, None),
        );
        map.lighting(
            K_LIGHTING_VERSION_3,
            &payload(K_LIGHTING_VERSION_3, 4, 10.0, Some(7)),
        );
        let bytes = map.bytes();
        let gl = GlobalLighting::from_map(&bytes)
            .expect("loads")
            .expect("present");
        assert_eq!(gl.time_of_day, 4);
        assert_eq!(gl.terrain_lighting[0][0].ambient.red, 10.0);
        assert_eq!(gl.shadow_color, Some(7));

        // a nested lighting chunk never fires (the global-scope rule)
        let mut nested_map = MapFile::new();
        let mut nested = Vec::new();
        for k in 0..9u32 {
            put_f32(&mut nested, 11.0 + k as f32 * 0.25);
        }
        nested_map.container(&nested);
        nested_map.lighting(
            K_LIGHTING_VERSION_1,
            &payload(K_LIGHTING_VERSION_1, 3, 12.0, None),
        );
        let bytes = nested_map.bytes();
        let gl = GlobalLighting::from_map(&bytes)
            .expect("loads")
            .expect("present");
        assert_eq!(gl.terrain_lighting[0][0].ambient.red, 12.0);
    }

    #[test]
    fn absent_and_refused_walks() {
        // no GlobalLighting: None
        let mut map = MapFile::new();
        map.out.open_data_chunk(b"HeightMapData", 3);
        map.out.write_int(2);
        map.out.write_int(2);
        map.out.write_int(4);
        map.out.write_array_of_bytes(&[0x10; 4]);
        map.out.close_data_chunk();
        let bytes = map.bytes();
        assert_eq!(GlobalLighting::from_map(&bytes), Ok(None));

        // not a chunky file: the walk's refusal
        assert_eq!(
            GlobalLighting::from_map(b"not a map at all"),
            Err(LightingError::CorruptFileFormat)
        );
        // a CkMp table of contents that ends at EOF never opens
        let mut toc = Vec::new();
        toc.extend_from_slice(b"CkMp");
        toc.extend_from_slice(&1i32.to_le_bytes());
        toc.push(GLOBAL_LIGHTING_LABEL.len() as u8);
        toc.extend_from_slice(GLOBAL_LIGHTING_LABEL);
        toc.extend_from_slice(&1u32.to_le_bytes());
        assert_eq!(
            GlobalLighting::from_map(&toc),
            Err(LightingError::CorruptFileFormat)
        );
    }
}
