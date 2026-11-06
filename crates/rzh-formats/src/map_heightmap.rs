// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Ported from the Zero Hour source release (baseline 571a7c5):
//   Core/GameEngineDevice/Source/W3DDevice/GameClient/WorldHeightMap.cpp
//     (WorldHeightMap::ParseHeightMapData and ParseSizeOnly — the
//     HeightMapData chunk readers the constructor's two load modes
//     register — plus isValidTotalMapSize, isValidPlayableMapSize and
//     isValidHeightDataSize, the size contracts the readers enforce, and
//     the constructor's registerParser/parse shape the walk follows)
//   Core/GameEngineDevice/Include/W3DDevice/GameClient/WorldHeightMap.h
//     (the m_width/m_height/m_borderSize/m_boundaries/m_dataSize/m_data
//     members the reader writes, K_MIN_HEIGHT/K_MAX_HEIGHT)
//   Core/GameEngine/Include/Common/MapReaderWriterInfo.h
//     (K_HEIGHT_MAP_VERSION_*, MAX_MAP_DIMENSION, MAX_MAP_AREA — the
//     version words and the map-size ceilings)
//   Core/Libraries/Source/Lib/Include/Lib/ICoord2D.h
//     (the x/y int pair a boundary is)
//
// See the module docs for the format layout and the documented differences
// from the C++ (the two ERROR_CORRUPT_FILE_FORMAT throws split into
// precise errors; the negative border count the original dies on refused;
// the zero-born seismic arrays left to the consumer).

//! The **map height-map payload** — the `HeightMapData` chunk of a `.map`
//! file, the first map payload parser on top of [`crate::map_chunk`].
//!
//! A Zero Hour map's terrain grid is one `HeightMapData` chunk at the top
//! level of the data-chunk tree: the width and height in cells, the
//! unplayable border size, the in-game boundary rectangles, and the
//! one-byte-per-cell height samples. `WorldHeightMap`'s constructor
//! registers a reader for it in two flavours — the **full** load
//! (`ParseHeightMapData`, which also prepares the runtime-only seismic
//! scratch arrays) and the **logical-data-only** load (`ParseSizeOnly`,
//! what the map-validation and preview paths drive) — and this module
//! ports both over the [`DataChunkInput`] walk, quirks included (parity
//! level P0, pinned by golden vectors generated from the transcription
//! in `tools/differential/gen_mapheightmap_golden.cpp`).
//!
//! # The chunk in one page
//!
//! ```text
//! i32 width          cells across, border included (1..=8192)
//! i32 height         cells down, border included (1..=8192)
//! i32 border_size    version >= 3 only; 0 before
//! [version >= 4]     i32 count, then count × (i32 x, i32 y)
//! i32 data_size      must equal width × height
//! [u8; data_size]    the height samples, row-major
//! ```
//!
//! Before version 4 the boundary list is synthesised as the single
//! playable rectangle `width - 2×border` × `height - 2×border`; at
//! version 1 exactly, the samples are the old 5.0-units-per-cell grid
//! and the load **downsamples them in place** (every other sample),
//! leaving the tail of the buffer stale (see the quirks below).
//!
//! # The load-path quirks (transcribed)
//!
//! - The **version word gates three reads**: `border_size` exists from
//!   [`K_HEIGHT_MAP_VERSION_3`], the boundary list from
//!   [`K_HEIGHT_MAP_VERSION_4`], and the downsample runs at
//!   [`K_HEIGHT_MAP_VERSION_1`] exactly. A version 0 chunk reads like a
//!   version 2 one, and a future version like a 4 — the words are
//!   ceilings, never equality checks.
//! - The size contracts are the original's two `throw
//!   ERROR_CORRUPT_FILE_FORMAT` sites: [`is_valid_total_map_size`]
//!   (dimensions in `1..=MAX_MAP_DIMENSION`, border at most
//!   `(width - 1) / 2` on each axis, `width × height` at most
//!   [`MAX_MAP_AREA`]) and the data-size check (`data_size > 0`, `==`
//!   `width × height`, and its `u32` view at most the chunk's
//!   data-left — an **over-declared parent reads as a huge data-left**,
//!   so the check passes exactly where the original's signed underflow
//!   does).
//! - The **version-1 downsample compacts in place and keeps the
//!   original allocation**: the samples stay `data_size` bytes long with
//!   a `new_width × new_height` grid at the head and the file's raw
//!   bytes trailing after it. The **full** load leaves `width`/`height`
//!   at the *file's* extent (the engine's retail maps are version 3+;
//!   the mismatch is the transcribed state), while the **logical** load
//!   re-points them at the downsampled grid — but neither updates
//!   `data_size`, so a version-1 logical load ends with
//!   `data.len() != width * height`.
//! - The reader **never checks it consumed the chunk**: the original's
//!   `DEBUG_ASSERTCRASH(atEndOfChunk)` is a debug-only contract; the
//!   release load leaves leftovers to the walk's close (skipped), and a
//!   short file zero-fills the sample tail through the container's
//!   short-read semantics.
//! - The walk is the constructor's: `HeightMapData` registers at the
//!   **global scope**, so a `HeightMapData` nested inside another chunk
//!   never fires the reader (the container skips it whole), repeated
//!   `HeightMapData` chunks **overwrite** each other (the last one
//!   wins), and a map that carries none at all loads with `None` — the
//!   original's null `m_data`, which it survives until the first
//!   terrain query.
//!
//! # Differences from the C++ (documented)
//!
//! - The two `throw ERROR_CORRUPT_FILE_FORMAT` sites become the precise
//!   [`HeightMapError::InvalidMapSize`] and
//!   [`HeightMapError::InvalidHeightDataSize`]; the constructor's own
//!   throw when the walk itself refuses (a non-`CkMp` header) becomes
//!   [`HeightMapError::CorruptFileFormat`]. A payload error stops the
//!   walk through the parser's `false` — observably the same stop the
//!   original's escaping exception produces (the chunk stays open in
//!   both).
//! - A **negative boundary count** is [`HeightMapError::CorruptBorderCount`]:
//!   the original's `m_boundaries.resize(numBorders)` converts the
//!   negative `int` to a huge `size_t` and dies inside the standard
//!   library (`std::length_error`, uncaught — a process death, not a
//!   game error). The port refuses the load instead, the same decision
//!   the W3D modules made for the allocation-fatal shapes. A huge
//!   *positive* count allocates the way the original's `resize` does —
//!   a count the machine cannot satisfy aborts in both (the goldens
//!   avoid those counts, like the HLOD corpus discipline).
//! - The **seismic scratch arrays** (`m_seismicUpdateFlag`,
//!   `m_seismicZVelocities`, both born zero, and `m_seismicUpdateWidth`)
//!   are per-frame simulation state, not file content: the port returns
//!   the chunk's content and leaves the scratch to the consumer that
//!   will drive it (the same split the module list draws between
//!   format and simulation state).
//! - The height samples are the file's bytes verbatim; the original
//!   reads them into `new UnsignedByte[m_dataSize]` (uninitialised on a
//!   short read tail) where the port's `Vec` is zero-born — the same
//!   zero-filled-short-read stance the container pins.
//! - `m_data`'s allocation succeeds for every accepted load (the size
//!   contract caps it at [`MAX_MAP_AREA`]), so unlike the boundary
//!   count the sample array never aborts.
//!
//! # Examples
//!
//! ```
//! use rzh_formats::map_chunk::{DataChunkInfo, DataChunkInput, DataChunkOutput};
//! use rzh_formats::map_heightmap::{
//!     parse_height_map_data, HeightMapData, HEIGHT_MAP_DATA_LABEL,
//!     K_HEIGHT_MAP_VERSION_4,
//! };
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // A version-4 4×3 map with a border of 1 and two boundaries, built
//! // with the container's writer the way WorldBuilder saves one.
//! let mut out = DataChunkOutput::new();
//! out.open_data_chunk(HEIGHT_MAP_DATA_LABEL, K_HEIGHT_MAP_VERSION_4);
//! out.write_int(4);   // width
//! out.write_int(3);   // height
//! out.write_int(1);   // border size (version 3+)
//! out.write_int(2);   // boundary count (version 4+)
//! out.write_int(2);   // boundary 0: 2 × 1
//! out.write_int(1);
//! out.write_int(3);   // boundary 1: 3 × 2
//! out.write_int(2);
//! out.write_int(12);  // data size == width * height
//! out.write_array_of_bytes(&[0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110]);
//! out.close_data_chunk();
//! let bytes = out.into_bytes()?;
//!
//! // The constructor's walk: the map's HeightMapData, or None.
//! let hm = HeightMapData::from_map(&bytes, false)?.expect("the chunk is there");
//! assert_eq!((hm.width, hm.height, hm.border_size), (4, 3, 1));
//! assert_eq!(hm.data_size(), 12);
//! assert_eq!(hm.boundaries.len(), 2);
//! assert_eq!((hm.boundaries[1].x, hm.boundaries[1].y), (3, 2));
//! assert_eq!(hm.data[5], 50);
//!
//! // The chunk reader, driven directly the way a registered parser is
//! // (the info's version word is the reader's only field).
//! let mut input = DataChunkInput::new(&bytes);
//! input.open_data_chunk();
//! let info = DataChunkInfo {
//!     label: HEIGHT_MAP_DATA_LABEL.to_vec(),
//!     parent_label: Vec::new(),
//!     version: input.get_chunk_version(),
//!     data_size: input.get_chunk_data_size() as i32,
//! };
//! let hm = parse_height_map_data(&mut input, &info)?;
//! assert_eq!(hm.width, 4);
//! # Ok(())
//! # }
//! ```

use std::fmt;

use crate::map_chunk::{DataChunkInfo, DataChunkInput, ParserTable};

// ---------------------------------------------------------------------------
// constants (MapReaderWriterInfo.h, WorldHeightMap.h)
// ---------------------------------------------------------------------------

/// The chunk label the constructor registers its height-map readers
/// under, at the global scope.
pub const HEIGHT_MAP_DATA_LABEL: &[u8] = b"HeightMapData";

/// The largest map dimension, border included (`MAX_MAP_DIMENSION` — the
/// shroud texture spans the map and shoreline tiles pack each coordinate
/// into 16 bits).
pub const MAX_MAP_DIMENSION: i32 = 8192;

/// The largest cell count a map may cover (`MAX_MAP_AREA` — terrain,
/// pathfinding and partition arrays allocate per cell in a 32-bit
/// process).
pub const MAX_MAP_AREA: i32 = 4096 * 4096;

/// The height sample floor (`K_MIN_HEIGHT`).
pub const K_MIN_HEIGHT: u8 = 0;
/// The height sample ceiling (`K_MAX_HEIGHT`).
pub const K_MAX_HEIGHT: u8 = 255;

/// The height-map chunk version words (`K_HEIGHT_MAP_VERSION_*`) — the
/// gates the reader's fields hang on (see the module docs).
pub mod height_map_version {
    /// Version 1: the 5.0-units-per-cell grid, downsampled on load.
    pub const K_HEIGHT_MAP_VERSION_1: u16 = 1;
    /// Version 2: the 10.0-units-per-cell grid (no border, no list).
    pub const K_HEIGHT_MAP_VERSION_2: u16 = 2;
    /// Version 3: adds `m_borderSize`.
    pub const K_HEIGHT_MAP_VERSION_3: u16 = 3;
    /// Version 4: the multiple-boundary list.
    pub const K_HEIGHT_MAP_VERSION_4: u16 = 4;
}

pub use height_map_version::{
    K_HEIGHT_MAP_VERSION_1, K_HEIGHT_MAP_VERSION_2, K_HEIGHT_MAP_VERSION_3, K_HEIGHT_MAP_VERSION_4,
};

// ---------------------------------------------------------------------------
// errors
// ---------------------------------------------------------------------------

/// Errors the height-map load path can produce.
///
/// The original has one failure code — `ERROR_CORRUPT_FILE_FORMAT`,
/// thrown by the payload reader at its two size contracts and by the
/// constructor when the walk itself refuses — plus the process deaths
/// its unchecked allocations can reach (see
/// [`CorruptBorderCount`](Self::CorruptBorderCount)). The port states
/// each site as its own variant; the payload errors stop the walk the
/// way the escaping exception does (the chunk stays open).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeightMapError {
    /// The walk itself failed — the header never opened as a `CkMp`
    /// table of contents (the constructor's `if (!file.parse(this))`
    /// throw).
    CorruptFileFormat,
    /// `isValidTotalMapSize` refused the dimensions — width or height
    /// outside `1..=MAX_MAP_DIMENSION`, the border negative or past
    /// `(width - 1) / 2` on either axis, or the area past `MAX_MAP_AREA`.
    InvalidMapSize {
        /// The chunk's declared width.
        width: i32,
        /// The chunk's declared height.
        height: i32,
        /// The border size (0 for version < 3 chunks).
        border_size: i32,
    },
    /// `isValidHeightDataSize` refused the sample count — zero or
    /// negative, not `width * height`, or more bytes than the chunk
    /// has left (its `u32` view).
    InvalidHeightDataSize {
        /// The chunk's declared sample count (`m_dataSize`).
        data_size: i32,
        /// The chunk's width as read.
        width: i32,
        /// The chunk's height as read.
        height: i32,
        /// The chunk's data-left word at the check, the original's
        /// unsigned view of it.
        size_left: u32,
    },
    /// The version-4 boundary count is negative. The original's
    /// `m_boundaries.resize(numBorders)` converts it to a huge `size_t`
    /// and dies inside the standard library (an uncaught
    /// `std::length_error`); the port refuses the load.
    CorruptBorderCount(i32),
}

impl fmt::Display for HeightMapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HeightMapError::CorruptFileFormat => {
                write!(f, "map is not a chunky data-chunk file")
            }
            HeightMapError::InvalidMapSize {
                width,
                height,
                border_size,
            } => write!(
                f,
                "invalid total map size {width}x{height} with border {border_size}"
            ),
            HeightMapError::InvalidHeightDataSize {
                data_size,
                width,
                height,
                size_left,
            } => write!(
                f,
                "height sample count {data_size} does not match the \
                 {width}x{height} map (chunk has {size_left} bytes left)"
            ),
            HeightMapError::CorruptBorderCount(count) => {
                write!(f, "negative boundary count {count}")
            }
        }
    }
}

impl std::error::Error for HeightMapError {}

// ---------------------------------------------------------------------------
// the size contracts (WorldHeightMap.cpp)
// ---------------------------------------------------------------------------

/// `WorldHeightMap::isValidTotalMapSize` — whether a width × height
/// (border included) with that border is a loadable map: dimensions in
/// `1..=MAX_MAP_DIMENSION`, the border at most `(width - 1) / 2` and
/// `(height - 1) / 2` (integer division, the original's), and the area
/// at most `MAX_MAP_AREA`.
#[must_use]
pub fn is_valid_total_map_size(width: i32, height: i32, border_size: i32) -> bool {
    if width <= 0 || width > MAX_MAP_DIMENSION {
        return false;
    }
    if height <= 0 || height > MAX_MAP_DIMENSION {
        return false;
    }

    if border_size < 0 || border_size > (width - 1) / 2 || border_size > (height - 1) / 2 {
        return false;
    }

    if width * height > MAX_MAP_AREA {
        return false;
    }

    true
}

/// `WorldHeightMap::isValidPlayableMapSize` — the same contract for a
/// *playable* extent: the border is added to both dimensions before the
/// total check (the guard first keeps the original's `width +
/// 2 * borderSize` from overflowing).
#[must_use]
pub fn is_valid_playable_map_size(width: i32, height: i32, border_size: i32) -> bool {
    if width <= 0 || height <= 0 || border_size < 0 {
        return false;
    }

    if border_size > (i32::MAX - width) / 2 || border_size > (i32::MAX - height) / 2 {
        return false;
    }

    is_valid_total_map_size(
        width + 2 * border_size,
        height + 2 * border_size,
        border_size,
    )
}

/// `isValidHeightDataSize` — the sample-count contract: positive,
/// exactly the cell count, and no more than the chunk has left (the
/// `u32` view of `data_left`, so an over-declared chunk's negative
/// remainder reads as a huge budget and passes).
fn is_valid_height_data_size(data_size: i32, width: i32, height: i32, size_left: u32) -> bool {
    data_size > 0 && data_size == width * height && (data_size as u32) <= size_left
}

// ---------------------------------------------------------------------------
// the payload (WorldHeightMap's height-map members, as the load leaves them)
// ---------------------------------------------------------------------------

/// One in-game boundary rectangle (`ICoord2D` — the top-right corner;
/// the lower-left is always the origin).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Boundary {
    /// The boundary's x extent.
    pub x: i32,
    /// The boundary's y extent.
    pub y: i32,
}

/// The height-map state one `HeightMapData` chunk loads — the members
/// `WorldHeightMap`'s reader writes, as the load leaves them (see the
/// module docs for the version-1 shapes).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HeightMapData {
    /// The map width in cells, border included (`m_width`).
    pub width: i32,
    /// The map height in cells, border included (`m_height`).
    pub height: i32,
    /// The unplayable border size (`m_borderSize`; 0 for version < 3).
    pub border_size: i32,
    /// The in-game boundary rectangles (`m_boundaries`; the synthesised
    /// single playable rectangle for version < 4).
    pub boundaries: Vec<Boundary>,
    /// The height samples (`m_data`), `data_size` bytes row-major — a
    /// version-1 load compacts a half-resolution grid at the head and
    /// leaves the file's bytes in the tail.
    pub data: Vec<u8>,
}

impl HeightMapData {
    /// The sample-array length (`m_dataSize`) — always the *declared*
    /// `width * height`, even after a version-1 logical load re-points
    /// `width`/`height` at the downsampled grid.
    #[must_use]
    pub fn data_size(&self) -> i32 {
        self.data.len() as i32
    }

    /// The `WorldHeightMap` constructor's walk, restricted to this
    /// payload: the registered-parser stroll over the map's top-level
    /// chunks that returns the **last** `HeightMapData` the map carries
    /// — or `None` when it carries none (the original's null `m_data`).
    ///
    /// `logical_only` picks the constructor's two readers: the full
    /// load (`ParseHeightMapData`) or the logical-data-only load
    /// (`ParseSizeOnly`, whose version-1 resize updates
    /// `width`/`height`). A payload error stops the walk and fails the
    /// load, exactly where the original's throw escapes the constructor.
    ///
    /// The other payloads the constructor registers (the blend tiles
    /// and global lighting of the full load; the world info, objects,
    /// polygon triggers and sides list of the logical one) arrive with
    /// their own slices — here their chunks are skipped whole, the
    /// walk's behaviour for an unmatched label.
    pub fn from_map(bytes: &[u8], logical_only: bool) -> Result<Option<Self>, HeightMapError> {
        let mut input = DataChunkInput::new(bytes);
        let mut table: ParserTable<'_, HeightMapLoad> = ParserTable::new();
        table.register(HEIGHT_MAP_DATA_LABEL, b"", height_map_chunk_parser);
        let mut user = HeightMapLoad {
            logical_only,
            result: None,
            error: None,
        };
        if !input.parse(&mut table, &mut user) {
            // A parser's false is either this payload's refused load or
            // the walk's own refusal; the original throws the same code
            // for both (the constructor's `if (!file.parse(this))`).
            return Err(user.error.unwrap_or(HeightMapError::CorruptFileFormat));
        }
        Ok(user.result)
    }
}

/// The walk's user data: which reader is registered and what it loaded
/// (the original's `this`, cut down to the height-map half).
struct HeightMapLoad {
    logical_only: bool,
    result: Option<HeightMapData>,
    error: Option<HeightMapError>,
}

/// The registered parser the constructor's shape implies: dispatch to
/// the mode's reader, keep the last good state, refuse the walk on a
/// refused load (the observable equivalent of the original's escaping
/// `ERROR_CORRUPT_FILE_FORMAT`).
fn height_map_chunk_parser(
    file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
    _table: &mut ParserTable<'_, HeightMapLoad>,
    user: &mut HeightMapLoad,
) -> bool {
    let parsed = if user.logical_only {
        parse_size_only(file, info)
    } else {
        parse_height_map_data(file, info)
    };
    match parsed {
        Ok(hm) => {
            user.result = Some(hm);
            true
        }
        Err(e) => {
            user.error = Some(e);
            false
        }
    }
}

/// `WorldHeightMap::ParseHeightMapData` — the **full** load's chunk
/// reader: the dimensions, the border (version 3+), the boundary list
/// (version 4+, else the synthesised playable rectangle), the sample
/// array, and — at version 1 exactly — the in-place downsample that
/// leaves `width`/`height` at the file's extent.
///
/// The runtime-only seismic scratch the original allocates here is the
/// consumer's business (see the module docs); the samples themselves
/// are the file's bytes, zero-filled on a short read tail.
pub fn parse_height_map_data(
    file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
) -> Result<HeightMapData, HeightMapError> {
    read_height_map(file, info, false)
}

/// `WorldHeightMap::ParseSizeOnly` — the **logical-data-only** load's
/// chunk reader: the same reads, the same contracts, and a version-1
/// resize that *does* re-point `width`/`height` at the downsampled
/// grid (leaving `data_size` at the declared extent, so the two no
/// longer agree — the transcribed state the preview paths run on).
pub fn parse_size_only(
    file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
) -> Result<HeightMapData, HeightMapError> {
    read_height_map(file, info, true)
}

/// The shared body of the two chunk readers (the original repeats it
/// verbatim in `ParseHeightMapData` and `ParseSizeOnly`; the port keeps
/// one body whose only mode difference is the version-1 resize's
/// `width`/`height` update).
fn read_height_map(
    file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
    size_only: bool,
) -> Result<HeightMapData, HeightMapError> {
    let width = file.read_int();
    let height = file.read_int();
    let border_size = if info.version >= K_HEIGHT_MAP_VERSION_3 {
        file.read_int()
    } else {
        0
    };

    if !is_valid_total_map_size(width, height, border_size) {
        return Err(HeightMapError::InvalidMapSize {
            width,
            height,
            border_size,
        });
    }

    let boundaries = if info.version >= K_HEIGHT_MAP_VERSION_4 {
        let num_borders = file.read_int();
        if num_borders < 0 {
            // The original's m_boundaries.resize(numBorders) converts
            // the negative int to a huge size_t and dies inside the
            // standard library (an uncaught std::length_error); the
            // port refuses the load.
            return Err(HeightMapError::CorruptBorderCount(num_borders));
        }
        let mut boundaries = Vec::with_capacity(num_borders as usize);
        for _ in 0..num_borders {
            let x = file.read_int();
            let y = file.read_int();
            boundaries.push(Boundary { x, y });
        }
        boundaries
    } else {
        // the pre-version-4 single boundary: the playable extent
        vec![Boundary {
            x: width - 2 * border_size,
            y: height - 2 * border_size,
        }]
    };

    let data_size = file.read_int();
    if !is_valid_height_data_size(data_size, width, height, file.get_chunk_data_size_left()) {
        return Err(HeightMapError::InvalidHeightDataSize {
            data_size,
            width,
            height,
            size_left: file.get_chunk_data_size_left(),
        });
    }

    // m_data = new UnsignedByte[m_dataSize] — zero-born here (the
    // original's is uninitialised, but every accepted load fills it
    // whole unless the file runs short, where the container's
    // zero-filled short read is the pinned stance).
    let mut data = vec![0u8; data_size as usize];
    file.read_array_of_bytes(&mut data);

    // Resize me. (version 1: the 5.0-per-cell grid, halved in place —
    // the full load leaves m_width/m_height at the file's extent, the
    // logical load re-points them; neither touches m_dataSize.)
    let mut final_width = width;
    let mut final_height = height;
    if info.version == K_HEIGHT_MAP_VERSION_1 {
        let new_width = (width + 1) / 2;
        let new_height = (height + 1) / 2;
        for i in 0..new_height {
            for j in 0..new_width {
                data[(i * new_width + j) as usize] = data[(2 * i * width + 2 * j) as usize];
            }
        }
        if size_only {
            final_width = new_width;
            final_height = new_height;
        }
    }

    // DEBUG_ASSERTCRASH(file.atEndOfChunk(), ("Unexpected data left
    // over.")) — a debug-only contract; the release load leaves the
    // leftovers to the walk's close, and so does the port.

    Ok(HeightMapData {
        width: final_width,
        height: final_height,
        border_size,
        boundaries,
        data,
    })
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map_chunk::DataChunkOutput;

    /// Builds a map file around one HeightMapData payload.
    struct MapFile {
        out: DataChunkOutput,
    }

    impl MapFile {
        fn new() -> Self {
            MapFile {
                out: DataChunkOutput::new(),
            }
        }

        fn height_map(&mut self, version: u16, payload: &[u8]) -> &mut Self {
            self.out.open_data_chunk(HEIGHT_MAP_DATA_LABEL, version);
            self.out.write_array_of_bytes(payload);
            self.out.close_data_chunk();
            self
        }

        fn bytes(&mut self) -> Vec<u8> {
            self.out.clone().into_bytes().expect("finish")
        }
    }

    fn v4_payload(
        width: i32,
        height: i32,
        border: i32,
        boundaries: &[(i32, i32)],
        data: &[u8],
    ) -> Vec<u8> {
        let mut p = Vec::new();
        put_i32(&mut p, width);
        put_i32(&mut p, height);
        put_i32(&mut p, border);
        put_i32(&mut p, boundaries.len() as i32);
        for &(x, y) in boundaries {
            put_i32(&mut p, x);
            put_i32(&mut p, y);
        }
        put_i32(&mut p, data.len() as i32);
        p.extend_from_slice(data);
        p
    }

    fn put_i32(b: &mut Vec<u8>, v: i32) {
        b.extend_from_slice(&v.to_le_bytes());
    }

    #[test]
    fn the_version_gates_match_the_format() {
        assert_eq!(K_HEIGHT_MAP_VERSION_1, 1);
        assert_eq!(K_HEIGHT_MAP_VERSION_2, 2);
        assert_eq!(K_HEIGHT_MAP_VERSION_3, 3);
        assert_eq!(K_HEIGHT_MAP_VERSION_4, 4);
        assert_eq!(MAX_MAP_DIMENSION, 8192);
        assert_eq!(MAX_MAP_AREA, 4096 * 4096);
        assert_eq!(HEIGHT_MAP_DATA_LABEL, b"HeightMapData");
        assert_eq!(K_MIN_HEIGHT, 0);
        assert_eq!(K_MAX_HEIGHT, 255);
    }

    #[test]
    fn the_size_contracts_transcribe() {
        // isValidTotalMapSize: the documented edges.
        assert!(is_valid_total_map_size(1, 1, 0));
        assert!(is_valid_total_map_size(MAX_MAP_DIMENSION, 1, 0));
        assert!(is_valid_total_map_size(4096, 4096, 0)); // exactly MAX_MAP_AREA
        assert!(!is_valid_total_map_size(4097, 4096, 0)); // area over
        assert!(!is_valid_total_map_size(0, 1, 0));
        assert!(!is_valid_total_map_size(-1, 1, 0));
        assert!(!is_valid_total_map_size(MAX_MAP_DIMENSION + 1, 1, 0));
        assert!(!is_valid_total_map_size(5, 5, 3)); // border > (5-1)/2
        assert!(is_valid_total_map_size(5, 5, 2)); // border == (5-1)/2
        assert!(!is_valid_total_map_size(5, 5, -1));

        // isValidPlayableMapSize adds the border before the check (the
        // added total always satisfies the border halves — the failures
        // come from the dimension and area ceilings).
        assert!(is_valid_playable_map_size(3, 3, 1)); // 5x5 total
        assert!(is_valid_playable_map_size(1, 1, 3)); // 7x7 total, border 3
        assert!(!is_valid_playable_map_size(8192, 8192, 1)); // 8194 > MAX
        assert!(!is_valid_playable_map_size(4096, 4096, 1)); // 4098^2 > MAX_AREA
        assert!(!is_valid_playable_map_size(1, 1, -1));
        assert!(!is_valid_playable_map_size(0, 1, 0));
        // the overflow guard keeps huge borders from wrapping
        assert!(!is_valid_playable_map_size(
            i32::MAX / 4,
            i32::MAX / 4,
            i32::MAX / 2
        ));
    }

    #[test]
    fn a_modern_v4_map_loads_in_both_modes() {
        let mut map = MapFile::new();
        map.height_map(
            K_HEIGHT_MAP_VERSION_4,
            &v4_payload(4, 3, 1, &[(2, 1), (3, 2)], &(0..12).collect::<Vec<u8>>()),
        );
        let bytes = map.bytes();

        for logical in [false, true] {
            let hm = HeightMapData::from_map(&bytes, logical)
                .expect("loads")
                .expect("present");
            assert_eq!((hm.width, hm.height, hm.border_size), (4, 3, 1));
            assert_eq!(hm.data_size(), 12);
            assert_eq!(
                hm.boundaries,
                vec![Boundary { x: 2, y: 1 }, Boundary { x: 3, y: 2 }]
            );
            assert_eq!(hm.data, (0..12u8).collect::<Vec<u8>>());
        }
    }

    #[test]
    fn a_v2_map_synthesises_the_playable_boundary() {
        let mut p = Vec::new();
        put_i32(&mut p, 6);
        put_i32(&mut p, 4);
        put_i32(&mut p, 24);
        p.extend_from_slice(&[7; 24]);
        let mut map = MapFile::new();
        map.height_map(K_HEIGHT_MAP_VERSION_2, &p);
        let bytes = map.bytes();

        let hm = HeightMapData::from_map(&bytes, false)
            .expect("loads")
            .expect("present");
        assert_eq!(hm.border_size, 0);
        assert_eq!(hm.boundaries, vec![Boundary { x: 6, y: 4 }]);
        assert_eq!(hm.data_size(), 24);
    }

    #[test]
    fn the_v1_downsample_differs_between_the_modes() {
        // A 5x5 version-1 grid: the compaction keeps samples 0, 2, 4,
        // 10, 12, 14, ... (every other column of every other row).
        let samples: Vec<u8> = (0..25).map(|i| (i * 11 % 251) as u8).collect();
        let mut p = Vec::new();
        put_i32(&mut p, 5);
        put_i32(&mut p, 5);
        put_i32(&mut p, 25);
        p.extend_from_slice(&samples);
        let mut map = MapFile::new();
        map.height_map(K_HEIGHT_MAP_VERSION_1, &p);
        let bytes = map.bytes();

        // the full load: width/height stay 5x5, the head is compacted
        // and the tail keeps the file's bytes.
        let full = HeightMapData::from_map(&bytes, false)
            .expect("loads")
            .expect("present");
        assert_eq!((full.width, full.height), (5, 5));
        assert_eq!(full.data_size(), 25);
        let mut compacted = Vec::new();
        for i in 0..3 {
            for j in 0..3 {
                compacted.push(samples[(2 * i * 5 + 2 * j) as usize]);
            }
        }
        assert_eq!(&full.data[..9], &compacted[..]);
        assert_eq!(&full.data[9..], &samples[9..]);

        // the logical load: width/height re-point at the 3x3 grid, the
        // data length stays the declared 25.
        let logical = HeightMapData::from_map(&bytes, true)
            .expect("loads")
            .expect("present");
        assert_eq!((logical.width, logical.height), (3, 3));
        assert_eq!(logical.data_size(), 25);
        assert_eq!(logical.data, full.data);
    }

    #[test]
    fn the_error_contracts_transcribe() {
        // invalid width: the payload refuses, the walk stops.
        let mut map = MapFile::new();
        map.height_map(K_HEIGHT_MAP_VERSION_3, &{
            let mut p = Vec::new();
            put_i32(&mut p, 0);
            put_i32(&mut p, 4);
            put_i32(&mut p, 0);
            put_i32(&mut p, 16);
            p
        });
        assert_eq!(
            HeightMapData::from_map(&map.bytes(), false),
            Err(HeightMapError::InvalidMapSize {
                width: 0,
                height: 4,
                border_size: 0
            })
        );

        // data size mismatch (the payload carries no samples at all —
        // the chunk's data-left is exactly 0 at the check).
        let mut map = MapFile::new();
        map.height_map(K_HEIGHT_MAP_VERSION_2, &{
            let mut p = Vec::new();
            put_i32(&mut p, 4);
            put_i32(&mut p, 4);
            put_i32(&mut p, 12);
            p
        });
        assert_eq!(
            HeightMapData::from_map(&map.bytes(), false),
            Err(HeightMapError::InvalidHeightDataSize {
                data_size: 12,
                width: 4,
                height: 4,
                size_left: 0
            })
        );

        // a negative boundary count (version 4+).
        let mut map = MapFile::new();
        map.height_map(K_HEIGHT_MAP_VERSION_4, &{
            let mut p = Vec::new();
            put_i32(&mut p, 4);
            put_i32(&mut p, 4);
            put_i32(&mut p, 0);
            put_i32(&mut p, -2);
            p
        });
        assert_eq!(
            HeightMapData::from_map(&map.bytes(), false),
            Err(HeightMapError::CorruptBorderCount(-2))
        );
    }

    #[test]
    fn the_walk_skips_what_it_does_not_know() {
        // no HeightMapData at all: the load succeeds with None.
        let mut map = MapFile::new();
        map.out.open_data_chunk(b"BlendTileData", 7);
        map.out.write_int(3);
        map.out.close_data_chunk();
        assert_eq!(HeightMapData::from_map(&map.bytes(), false), Ok(None));

        // unknown before and after: both skipped, the payload loads.
        let mut map = MapFile::new();
        map.out.open_data_chunk(b"WorldInfo", 1);
        map.out.write_int(1);
        map.out.close_data_chunk();
        map.height_map(K_HEIGHT_MAP_VERSION_3, &{
            let mut p = Vec::new();
            put_i32(&mut p, 2);
            put_i32(&mut p, 2);
            put_i32(&mut p, 0);
            put_i32(&mut p, 4);
            p.extend_from_slice(&[1, 2, 3, 4]);
            p
        });
        map.out.open_data_chunk(b"GlobalLighting", 2);
        map.out.write_int(0);
        map.out.close_data_chunk();
        let hm = HeightMapData::from_map(&map.bytes(), false)
            .expect("loads")
            .expect("present");
        assert_eq!(hm.data, vec![1, 2, 3, 4]);
    }

    #[test]
    fn repeated_chunks_are_last_wins() {
        let mut map = MapFile::new();
        map.height_map(K_HEIGHT_MAP_VERSION_3, &{
            let mut p = Vec::new();
            put_i32(&mut p, 2);
            put_i32(&mut p, 2);
            put_i32(&mut p, 0);
            put_i32(&mut p, 4);
            p.extend_from_slice(&[1, 1, 1, 1]);
            p
        });
        map.height_map(K_HEIGHT_MAP_VERSION_3, &{
            let mut p = Vec::new();
            put_i32(&mut p, 5);
            put_i32(&mut p, 4);
            put_i32(&mut p, 1);
            put_i32(&mut p, 20);
            p.extend_from_slice(&[9; 20]);
            p
        });
        let hm = HeightMapData::from_map(&map.bytes(), false)
            .expect("loads")
            .expect("present");
        assert_eq!((hm.width, hm.height, hm.border_size), (5, 4, 1));
        assert_eq!(hm.data_size(), 20);
    }

    #[test]
    fn a_non_chunky_file_refuses_the_walk() {
        assert_eq!(
            HeightMapData::from_map(b"not a map", false),
            Err(HeightMapError::CorruptFileFormat)
        );
        assert_eq!(
            HeightMapData::from_map(&[], false),
            Err(HeightMapError::CorruptFileFormat)
        );
    }

    #[test]
    fn error_display_is_stable() {
        assert_eq!(
            HeightMapError::CorruptFileFormat.to_string(),
            "map is not a chunky data-chunk file"
        );
        assert_eq!(
            HeightMapError::InvalidMapSize {
                width: -1,
                height: 3,
                border_size: 0
            }
            .to_string(),
            "invalid total map size -1x3 with border 0"
        );
        assert_eq!(
            HeightMapError::InvalidHeightDataSize {
                data_size: 12,
                width: 4,
                height: 4,
                size_left: 4
            }
            .to_string(),
            "height sample count 12 does not match the 4x4 map (chunk has 4 bytes left)"
        );
        assert_eq!(
            HeightMapError::CorruptBorderCount(-3).to_string(),
            "negative boundary count -3"
        );
    }
}
