// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Ported from the Zero Hour source release (baseline 571a7c5):
//   Core/GameEngineDevice/Source/W3DDevice/GameClient/WorldHeightMap.cpp
//     (WorldHeightMap::ParseBlendTileData — the BlendTileData chunk
//     reader the constructor's full load registers; initCliffFlagsFrom-
//     Heights / setCellCliffFlagFromHeights / setCliffState, the
//     pre-version-7 cliff-bit derivation; the constructor's full-mode
//     registerParser walk and its post-walk "patch bad maps" loop)
//   Core/GameEngineDevice/Include/W3DDevice/GameClient/WorldHeightMap.h
//     (the m_tileNdxes/m_blendTileNdxes/m_extraBlendTileNdxes/
//     m_cliffInfoNdxes/m_cellFlipState/m_cellCliffState members the
//     reader writes, TCliffInfo, NUM_BLEND_TILES, NUM_CLIFF_INFO,
//     NUM_TEXTURE_CLASSES, FLAG_VAL)
//   Core/GameEngineDevice/Include/W3DDevice/GameClient/TileData.h
//     (TBlendTileInfo, INVERTED_MASK, FLIPPED_MASK)
//   Core/GameEngine/Include/Common/MapReaderWriterInfo.h
//     (K_BLEND_TILE_VERSION_1..8 — the version words and their gates)
//   Core/GameEngine/Include/Common/MapObject.h (MAP_HEIGHT_SCALE)
//   Core/GameEngine/Source/Common/GlobalData.cpp
//     (m_use3WayTerrainBlends, default 1 — the INI gate the reader
//     consults through TheGlobalData)
//
// See the module docs for the format layout and the documented differences
// from the C++ (the two ERROR_CORRUPT_FILE_FORMAT throws split into
// precise errors; the four count-over-array-bound in-object overflows
// refused; the null-array patch-loop death refused; the uninitialised
// allocations zero-born; readTexClass's texture loads left to the
// consumer).

//! The **map blend-tile payload** — the `BlendTileData` chunk of a `.map`
//! file, the largest map payload on top of [`crate::map_chunk`].
//!
//! A Zero Hour map's terrain texturing is one `BlendTileData` chunk at
//! the top level of the data-chunk tree: four `i16` index arrays with one
//! entry per height-map cell (which source tile each cell shows, which
//! blend record transitions it into its neighbour, which extra
//! three-way blend layer sits on top, which cliff u/v table stretches
//! it), the per-cell flip and cliff-state bitmaps, the texture classes
//! (the TGA tile sets the terrain reads its pixels from), the blend
//! records with their `FLAG_VAL` sentinels, and the cliff u/v tables.
//! `WorldHeightMap`'s constructor registers a reader for it in the
//! **full** load only (the logical-data-only load never looks at it),
//! and this module ports that reader over the [`DataChunkInput`] walk,
//! quirks included (parity level P0, pinned by golden vectors generated
//! from the transcription in `tools/differential/gen_mapblendtile_golden.cpp`).
//!
//! # The chunk in one page
//!
//! ```text
//! i32 len                must equal the height map's data size
//! [i16; len]             tile indices (into the source tiles)
//! [i16; len]             blend-tile indices (0 = no blend)
//! [v >= 6] [i16; len]    extra blend indices (the 3-way layer)
//! [v >= 5] [i16; len]    cliff-info indices (0 = no cliff)
//! [v >= 7] cliff state   v7: height × (width+1)/8 bytes, remapped into
//!                        the (width+7)/8 bitmap; v8: the bitmap direct
//! i32 num_bitmap_tiles   stored, never used by the load
//! i32 num_blended_tiles  records follow for i in 1..count
//! [v >= 5] i32 num_cliff_info   else the count is 1 (the default info)
//! i32 num_texture_classes
//!   × { i32 first, i32 num, i32 width, i32 legacy_gdf, str16 name }
//! [v >= 4] i32 num_edge_tiles, i32 num_edge_texture_classes
//!   × { i32 first, i32 num, i32 width, str16 name }   (no legacy word)
//!   × (num_blended_tiles - 1) {
//!       i32 blend_ndx, u8 horiz, vert, right_diag, left_diag, inverted,
//!       [v >= 3] u8 long_diag, [v >= 4] i32 custom_edge_class,
//!       i32 flag — must be FLAG_VAL (0x7ADA0000) }
//! [v >= 5] × (num_cliff_info - 1) {
//!       i32 tile_index (truncated to i16), 8 × f32 u/v,
//!       u8 flip, u8 mutant }
//! [v == 1] the 2× downsample of the four index arrays and the extents
//! ```
//!
//! Before version 7 the cliff-state bitmap is **derived from the height
//! samples**: a cell is a cliff when the spread of its four corner
//! heights (scaled by `MAP_HEIGHT_SCALE` = 10/16) exceeds
//! [`PATHFIND_CLIFF_SLOPE_LIMIT_F`] (9.8) — a raw sample spread of 16,
//! not 15.
//!
//! # The load-path quirks (transcribed)
//!
//! - The **version words are ceilings**: the extra blend layer exists
//!   from [`K_BLEND_TILE_VERSION_6`], the cliff indices and tables from
//!   5, the long diagonals from 3, the custom edge classes and the edge
//!   classes from 4, and the direct cliff-state bitmap from 8. A version
//!   0 chunk reads like a 2, a version 9 like an 8; only version 1
//!   exactly runs the downsample.
//! - The `len` word must equal **the height map's live `m_dataSize`** —
//!   not the height map's file extent: a version-1 blend resize halves
//!   the shared member first, and a later `BlendTileData` chunk's `len`
//!   is checked against the halved count.
//! - The **version-7 cliff-state remap**: the file stores
//!   `height × (width+1)/8` bytes per the "previous incorrect length"
//!   the original's comment records, and the load copies them into the
//!   `(width+7)/8`-wide bitmap row by row — every row's tail byte stays
//!   zero (17-wide maps: 2 bytes stored, 3 in the bitmap).
//! - The **version-1 downsample** compacts all four index arrays in
//!   place (every other column of every other row), zeroes the
//!   blend/extra/cliff heads, forces the blended-tile and cliff-info
//!   counts to 1 (record 0 is the implied transparent tile), and
//!   re-points the shared `m_width`/`m_height`/`m_dataSize` at the
//!   halved grid — the arrays keep their original length, so the tail
//!   beyond the new count holds the file's raw values.
//! - The **3-way gate** (`m_use3WayTerrainBlends`, default on): turned
//!   off, the extra-blend array is flushed to zero after its read and
//!   every blend record's `inverted` loses its
//!   [`FLIPPED_MASK`] bit — the "clearing of extra blend tiles via ini
//!   and resaving of map" the original's comments describe.
//! - The reader **never checks it consumed the chunk** (the
//!   `atEndOfChunk` assert is debug-only) and the counts are stored
//!   verbatim — `num_bitmap_tiles` is bounded only by a debug assert,
//!   and negative counts simply skip their loops.
//! - The walk is the constructor's: both payloads register at the
//!   **global scope**, so a `BlendTileData` nested inside another chunk
//!   never fires, repeated chunks overwrite each other (the last wins),
//!   and the constructor's post-walk **patch loop** clamps the cliff,
//!   blend and extra-blend indices into `[0, count)` over the *live*
//!   data size — the tile indices are never clamped.
//!
//! # Differences from the C++ (documented)
//!
//! - The two `throw ERROR_CORRUPT_FILE_FORMAT` sites become
//!   [`BlendTileError::DataSizeMismatch`] and
//!   [`BlendTileError::InvalidBlendFlag`]; the constructor's own throw
//!   for a refused walk becomes [`BlendTileError::CorruptFileFormat`],
//!   and the height-map half's refusals surface as
//!   [`BlendTileError::HeightMap`]. A payload error stops the walk
//!   through the parser's `false` — observably the same stop the
//!   original's escaping exception produces (the chunk stays open).
//! - The original indexes **four fixed member arrays** with the
//!   file-supplied counts (`m_blendedTiles[16192]`,
//!   `m_cliffInfo[32384]`, `m_textureClasses[256]`,
//!   `m_edgeTextureClasses[256]`); a count past its bound is an
//!   in-object overflow (silent corruption for small overruns, a crash
//!   for large ones — the `DEBUG_ASSERTCRASH`s that nominally bound
//!   them are debug-only). The port refuses those counts
//!   ([`BlendTileError::TooManyBlendedTiles`] and friends), the same
//!   decision the W3D modules made for the allocation-fatal shapes.
//! - The constructor's patch loop **dereferences the null index arrays**
//!   when a height map loaded but no `BlendTileData` did — a process
//!   death. The port refuses the load
//!   ([`BlendTileError::PatchWithoutBlendTiles`]). When the arrays are
//!   *shorter* than the live data size (a blend chunk that loaded before
//!   any height map, so `len == 0`), the original overreads its 0-length
//!   allocations; the port skips the missing tail — the deterministic
//!   stance the transcription pins.
//! - `new Short[]` / `new UnsignedByte[]` are zero-born here: the
//!   original's heap is uninitialised, which is observable exactly where
//!   the file never writes — the cliff and extra-blend index arrays of
//!   pre-version-5/6 chunks (read as garbage by the patch loop) and the
//!   version-7 remap's short-read tail. Both sides zero-fill (the
//!   container's pinned short-read stance).
//! - `readTexClass` — the TGA tile load through `TheFileSystem` and
//!   `TheTerrainTypes` — reads nothing from the chunk stream (it opens
//!   separate texture files) and is out of scope: the parsed class
//!   fields the texture manager drives (`first_tile`, `num_tiles`,
//!   `width`, `name`) are the port's observable, and loading the tiles
//!   is the future consumer's job (the crate is parsing only).
//! - The original's four fixed arrays persist **stale entries across
//!   repeated `BlendTileData` chunks** (the edge counts reset to 0
//!   outside the version gate; the entries are only rewritten inside
//!   it) — invisible to every count-driven consumer. The port's
//!   per-parse vectors are the count-driven view: each parse's
//!   `edge_texture_classes` holds what that chunk's version read.
//! - The runtime-only half of `TXTextureClass` (`globalTextureClass`,
//!   born -1 and resolved by the texture manager, and
//!   `positionInTexture`) is not represented.
//!
//! # Examples
//!
//! ```
//! use rzh_formats::map_blendtile::{
//!     BlendTileInfo, TerrainLoad, BLEND_TILE_DATA_LABEL, FLAG_VAL,
//!     K_BLEND_TILE_VERSION_8,
//! };
//! use rzh_formats::map_chunk::DataChunkOutput;
//! use rzh_formats::map_heightmap::{
//!     HeightMapData, HEIGHT_MAP_DATA_LABEL, K_HEIGHT_MAP_VERSION_2,
//! };
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // A 2x2 map (4 cells) with a version-8 blend payload: one blend
//! // record, no cliff tables — built with the container's writer the
//! // way WorldBuilder saves one.
//! let mut out = DataChunkOutput::new();
//! out.open_data_chunk(HEIGHT_MAP_DATA_LABEL, K_HEIGHT_MAP_VERSION_2);
//! out.write_int(2);                     // width
//! out.write_int(2);                     // height
//! out.write_int(4);                     // data size
//! out.write_array_of_bytes(&[10, 10, 10, 10]); // flat heights
//! out.close_data_chunk();
//! out.open_data_chunk(BLEND_TILE_DATA_LABEL, K_BLEND_TILE_VERSION_8);
//! out.write_int(4);                     // len == the height map's size
//! out.write_array_of_bytes(&[1, 0, 2, 0, 3, 0, 4, 0]); // tile indices
//! out.write_array_of_bytes(&[0; 8]);    // blend indices
//! out.write_array_of_bytes(&[0; 8]);    // extra blend indices
//! out.write_array_of_bytes(&[0; 8]);    // cliff indices
//! out.write_array_of_bytes(&[0; 2]);    // cliff state (1 byte/row)
//! out.write_int(1);                     // num bitmap tiles
//! out.write_int(2);                     // num blended tiles
//! out.write_int(1);                     // num cliff infos
//! out.write_int(0);                     // num texture classes
//! out.write_int(0);                     // num edge tiles
//! out.write_int(0);                     // num edge texture classes
//! out.write_int(7);                     // record 1: blend index
//! out.write_byte(1);                    // horiz
//! out.write_byte(0);                    // vert
//! out.write_byte(0);                    // right diagonal
//! out.write_byte(0);                    // left diagonal
//! out.write_byte(1);                    // inverted
//! out.write_byte(0);                    // long diagonal
//! out.write_int(3);                     // custom blend edge class
//! out.write_int(FLAG_VAL as i32);       // the sentinel
//! out.close_data_chunk();
//! let bytes = out.into_bytes()?;
//!
//! // The constructor's full-load walk: height map + blend tiles + the
//! // shared extent members as the load leaves them.
//! let load = TerrainLoad::from_map(&bytes, true)?;
//! assert_eq!((load.width, load.height, load.data_size), (2, 2, 4));
//! assert_eq!(load.blend_tiles.as_ref().unwrap().tile_ndxes, [1, 2, 3, 4]);
//! assert_eq!(
//!     load.blend_tiles.as_ref().unwrap().blended_tiles[1].blend_ndx,
//!     7
//! );
//! assert_eq!(load.blend_tiles.as_ref().unwrap().blended_tiles[1].inverted, 1);
//! # Ok(())
//! # }
//! ```
//!
//! [`DataChunkInput`]: crate::map_chunk::DataChunkInput

use std::fmt;

use crate::map_chunk::{DataChunkInfo, DataChunkInput, ParserTable};
use crate::map_heightmap::{
    HEIGHT_MAP_DATA_LABEL, HeightMapData, HeightMapError, parse_height_map_data,
};

// ---------------------------------------------------------------------------
// constants (WorldHeightMap.h, TileData.h, MapReaderWriterInfo.h,
// MapObject.h, WorldHeightMap.cpp)
// ---------------------------------------------------------------------------

/// The chunk label the constructor registers its blend-tile reader
/// under, at the global scope.
pub const BLEND_TILE_DATA_LABEL: &[u8] = b"BlendTileData";

/// The blended-tile array bound (`NUM_BLEND_TILES` — the fixed
/// `m_blendedTiles` array the original's count indexes).
pub const NUM_BLEND_TILES: i32 = 16192;

/// The cliff-info array bound (`NUM_CLIFF_INFO`).
pub const NUM_CLIFF_INFO: i32 = 32384;

/// The texture-class array bound (`NUM_TEXTURE_CLASSES` — one array for
/// the classes, one for the edge classes).
pub const NUM_TEXTURE_CLASSES: i32 = 256;

/// The blend record sentinel (`FLAG_VAL`): every record closes with it.
pub const FLAG_VAL: i32 = 0x7ADA_0000;

/// The inverted state's own bit of a blend record's `inverted` byte
/// (`INVERTED_MASK`).
pub const INVERTED_MASK: u8 = 0x1;

/// The forced-flip bit of a blend record's `inverted` byte
/// (`FLIPPED_MASK`) — the bit the 3-way gate filters.
pub const FLIPPED_MASK: u8 = 0x2;

/// The height scale (`MAP_HEIGHT_SCALE` — `MAP_XY_FACTOR` 10.0 over 16):
/// the cliff derivation multiplies each sample by it.
pub const MAP_HEIGHT_SCALE: f32 = 10.0 / 16.0;

/// The cliff slope limit (`PATHFIND_CLIFF_SLOPE_LIMIT_F`): a cell whose
/// scaled corner-height spread exceeds it is a cliff.
pub const PATHFIND_CLIFF_SLOPE_LIMIT_F: f32 = 9.8;

/// The blend-tile chunk version words (`K_BLEND_TILE_VERSION_*`) — the
/// gates the reader's fields hang on (see the module docs).
pub mod blend_tile_version {
    /// Version 1: the 5.0-units-per-cell grid, downsampled on load.
    pub const K_BLEND_TILE_VERSION_1: u16 = 1;
    /// Version 2: the 10.0-units-per-cell grid.
    pub const K_BLEND_TILE_VERSION_2: u16 = 2;
    /// Version 3: adds the long diagonal blends.
    pub const K_BLEND_TILE_VERSION_3: u16 = 3;
    /// Version 4: adds the custom edge blends (and the edge classes).
    pub const K_BLEND_TILE_VERSION_4: u16 = 4;
    /// Version 5: adds the custom cliff u/v coordinates.
    pub const K_BLEND_TILE_VERSION_5: u16 = 5;
    /// Version 6: adds the extra blend layer for three textures in a
    /// cell.
    pub const K_BLEND_TILE_VERSION_6: u16 = 6;
    /// Version 7: adds the passable/impassable cell painting, stored
    /// with the incorrect row length.
    pub const K_BLEND_TILE_VERSION_7: u16 = 7;
    /// Version 8: the same painting with the correct row length — the
    /// version Zero Hour itself writes.
    pub const K_BLEND_TILE_VERSION_8: u16 = 8;
}

pub use blend_tile_version::{
    K_BLEND_TILE_VERSION_1, K_BLEND_TILE_VERSION_2, K_BLEND_TILE_VERSION_3, K_BLEND_TILE_VERSION_4,
    K_BLEND_TILE_VERSION_5, K_BLEND_TILE_VERSION_6, K_BLEND_TILE_VERSION_7, K_BLEND_TILE_VERSION_8,
};

// ---------------------------------------------------------------------------
// errors
// ---------------------------------------------------------------------------

/// Errors the blend-tile load path can produce.
///
/// The original has one failure code — `ERROR_CORRUPT_FILE_FORMAT`,
/// thrown by the payload reader at its `len` and sentinel contracts and
/// by the constructor when the walk itself refuses — plus the process
/// deaths its unchecked operations reach (the count-over-array-bound
/// overflows, the patch loop's null dereference). The port states each
/// site as its own variant; the payload errors stop the walk the way
/// the escaping exception does (the chunk stays open).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendTileError {
    /// The walk itself failed — the header never opened as a `CkMp`
    /// table of contents (the constructor's `if (!file.parse(this))`
    /// throw).
    CorruptFileFormat,
    /// The chunk's `len` word is not the height map's live data size
    /// (the reader's first `throw ERROR_CORRUPT_FILE_FORMAT`).
    DataSizeMismatch {
        /// The chunk's `len` word as read.
        len: i32,
        /// The live `m_dataSize` at the check.
        data_size: i32,
    },
    /// A blend record's closing word is not `FLAG_VAL` (the reader's
    /// second throw).
    InvalidBlendFlag {
        /// The refusing record's index (records run from 1).
        index: i32,
        /// The word as read.
        flag: i32,
    },
    /// `num_blended_tiles` is past the `m_blendedTiles` bound — the
    /// original overruns the fixed array in-object; the port refuses.
    TooManyBlendedTiles(i32),
    /// `num_cliff_info` is past the `m_cliffInfo` bound.
    TooManyCliffInfos(i32),
    /// `num_texture_classes` is past the `m_textureClasses` bound.
    TooManyTextureClasses(i32),
    /// `num_edge_texture_classes` is past the `m_edgeTextureClasses`
    /// bound.
    TooManyEdgeTextureClasses(i32),
    /// The constructor's patch loop would dereference the null index
    /// arrays: a height map loaded (so `m_dataSize > 0`) but no
    /// `BlendTileData` did — a process death in the original, refused
    /// here.
    PatchWithoutBlendTiles {
        /// The live `m_dataSize` the loop would have walked.
        data_size: i32,
    },
    /// The height-map half of the walk refused (the wrapped
    /// [`HeightMapError`]; the original throws the same
    /// `ERROR_CORRUPT_FILE_FORMAT` out of the same constructor).
    HeightMap(HeightMapError),
}

impl fmt::Display for BlendTileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BlendTileError::CorruptFileFormat => {
                write!(f, "map is not a chunky data-chunk file")
            }
            BlendTileError::DataSizeMismatch { len, data_size } => write!(
                f,
                "blend tile len {len} does not match the height map's {data_size} cells"
            ),
            BlendTileError::InvalidBlendFlag { index, flag } => {
                write!(f, "blend record {index} sentinel {flag:#x} is not FLAG_VAL")
            }
            BlendTileError::TooManyBlendedTiles(count) => {
                write!(
                    f,
                    "unlikely num blended tiles {count} (bound {NUM_BLEND_TILES})"
                )
            }
            BlendTileError::TooManyCliffInfos(count) => {
                write!(
                    f,
                    "unlikely num cliff infos {count} (bound {NUM_CLIFF_INFO})"
                )
            }
            BlendTileError::TooManyTextureClasses(count) => {
                write!(
                    f,
                    "unlikely num texture classes {count} (bound {NUM_TEXTURE_CLASSES})"
                )
            }
            BlendTileError::TooManyEdgeTextureClasses(count) => write!(
                f,
                "unlikely num edge texture classes {count} (bound {NUM_TEXTURE_CLASSES})"
            ),
            BlendTileError::PatchWithoutBlendTiles { data_size } => write!(
                f,
                "height map of {data_size} cells carries no blend tile data \
                 (the patch loop's null arrays)"
            ),
            BlendTileError::HeightMap(e) => write!(f, "height map: {e}"),
        }
    }
}

impl std::error::Error for BlendTileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            BlendTileError::HeightMap(e) => Some(e),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// the payload structures (TileData.h / WorldHeightMap.h, as loaded)
// ---------------------------------------------------------------------------

/// One texture class (`TXTextureClass`, the parsed half): the tile set
/// one terrain texture contributes — `readTexClass` loads its TGA into
/// the source tiles at `first_tile` when the engine has the file.
#[derive(Debug, Clone, PartialEq, Eq, Default, Hash)]
pub struct TxTextureClass {
    /// The first source tile the class owns (`firstTile`).
    pub first_tile: i32,
    /// How many tiles the class owns (`numTiles`).
    pub num_tiles: i32,
    /// The tile grid width in tiles (`width` — a 128×128 bitmap at
    /// 64 pixels per tile makes 2).
    pub width: i32,
    /// The class's name (`name` — the terrain type it resolves to).
    pub name: Vec<u8>,
}

/// One blend record (`TBlendTileInfo`): how one tile transitions into
/// its neighbour. Record 0 is the implied transparent tile and is never
/// read from the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct BlendTileInfo {
    /// The record's own index into the blended-tile table (`blendNdx`).
    pub blend_ndx: i32,
    /// The horizontal blend weight (`horiz`).
    pub horiz: u8,
    /// The vertical blend weight (`vert`).
    pub vert: u8,
    /// The right-diagonal blend weight (`rightDiagonal`).
    pub right_diagonal: u8,
    /// The left-diagonal blend weight (`leftDiagonal`).
    pub left_diagonal: u8,
    /// The inversion/flip flags (`inverted` — bit 0 inverted, bit 1
    /// flipped; the 3-way gate clears bit 1).
    pub inverted: u8,
    /// The long-diagonal blend flag (`longDiagonal`, version 3+; 0
    /// before).
    pub long_diagonal: u8,
    /// The texture class for a blend edge (`customBlendEdgeClass`, -1
    /// means use alpha; version 4+).
    pub custom_blend_edge_class: i32,
}

/// One cliff u/v table (`TCliffInfo`): how a cliff-textured cell
/// stretches its tile. Info 0 is the default and is never read from the
/// file.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CliffInfo {
    /// The tile the cliff shows (`tileIndex` — the original stores the
    /// `i32` it read into a `Short`, truncating).
    pub tile_index: i16,
    /// The upper-left u (`u0`).
    pub u0: f32,
    /// The upper-left v (`v0`).
    pub v0: f32,
    /// The lower-left u (`u1`).
    pub u1: f32,
    /// The lower-left v (`v1`).
    pub v1: f32,
    /// The lower-right u (`u2`).
    pub u2: f32,
    /// The lower-right v (`v2`).
    pub v2: f32,
    /// The upper-right u (`u3`).
    pub u3: f32,
    /// The upper-right v (`v3`).
    pub v3: f32,
    /// Whether the tile is flipped (`flip` — any nonzero byte).
    pub flip: bool,
    /// Whether a mutant mapping is needed (`mutant`).
    pub mutant: bool,
}

/// The blend-tile state one `BlendTileData` chunk loads — the members
/// `WorldHeightMap`'s reader writes, as the load leaves them.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BlendTileData {
    /// The number of bitmap tiles (`m_numBitmapTiles`) — stored
    /// verbatim; only a debug assert bounds it.
    pub num_bitmap_tiles: i32,
    /// The blended-tile count (`m_numBlendedTiles`) — the version-1
    /// resize forces it to 1.
    pub num_blended_tiles: i32,
    /// The cliff-info count (`m_numCliffInfo`) — 1 before version 5,
    /// forced back to 1 by the version-1 resize.
    pub num_cliff_info: i32,
    /// The texture-class count (`m_numTextureClasses`, as read).
    pub num_texture_classes: i32,
    /// The edge tile count (`m_numEdgeTiles`).
    pub num_edge_tiles: i32,
    /// The edge texture-class count (`m_numEdgeTextureClasses`).
    pub num_edge_texture_classes: i32,
    /// The shared width as the parse leaves it (`m_width` — the
    /// version-1 resize halves it).
    pub width: i32,
    /// The shared height as the parse leaves it (`m_height`).
    pub height: i32,
    /// The shared sample-count word as the parse leaves it
    /// (`m_dataSize` — the version-1 resize re-points it; the sample
    /// array itself keeps its declared length).
    pub data_size: i32,
    /// The flip-state bitmap's row width (`m_flipStateWidth`,
    /// `(width + 7) / 8`).
    pub flip_state_width: i32,
    /// The per-cell source-tile indices (`m_tileNdxes`).
    pub tile_ndxes: Vec<i16>,
    /// The per-cell blend-tile indices (`m_blendTileNdxes`, 0 = no
    /// blend).
    pub blend_tile_ndxes: Vec<i16>,
    /// The per-cell extra-blend indices (`m_extraBlendTileNdxes`, the
    /// 3-way layer; 0 = none).
    pub extra_blend_tile_ndxes: Vec<i16>,
    /// The per-cell cliff-info indices (`m_cliffInfoNdxes`, 0 = no
    /// cliff).
    pub cliff_info_ndxes: Vec<i16>,
    /// The per-cell flip-state bitmap (`m_cellFlipState`) — born zero;
    /// no version of the chunk writes it.
    pub cell_flip_state: Vec<u8>,
    /// The per-cell cliff-state bitmap (`m_cellCliffState`) — read
    /// directly from version 8, remapped at 7, derived from the heights
    /// before.
    pub cell_cliff_state: Vec<u8>,
    /// The texture classes (`m_textureClasses`).
    pub texture_classes: Vec<TxTextureClass>,
    /// The edge texture classes (`m_edgeTextureClasses`).
    pub edge_texture_classes: Vec<TxTextureClass>,
    /// The blend records (`m_blendedTiles`) — entry 0 is the implied
    /// transparent tile.
    pub blended_tiles: Vec<BlendTileInfo>,
    /// The cliff u/v tables (`m_cliffInfo`) — entry 0 is the default.
    pub cliff_infos: Vec<CliffInfo>,
}

/// The height-map state the blend reader reads: the `WorldHeightMap`
/// members the constructor's walk holds when the blend chunk fires —
/// the height map's extent and samples plus the **live** sample-count
/// word, which a version-1 blend resize re-points and a later
/// `HeightMapData` overwrites (and which therefore cannot be derived
/// from the samples' length).
#[derive(Debug, Clone, Copy, Default)]
pub struct SharedHeightMap<'a> {
    /// The map width in cells (`m_width`).
    pub width: i32,
    /// The map height in cells (`m_height`).
    pub height: i32,
    /// The live sample-count word (`m_dataSize`).
    pub data_size: i32,
    /// The height samples (`m_data`).
    pub data: &'a [u8],
}

impl<'a> SharedHeightMap<'a> {
    /// The view of a height map as its own reader left it (the live
    /// count is the declared count, `data.len()`).
    #[must_use]
    pub fn of(height_map: &'a HeightMapData) -> Self {
        SharedHeightMap {
            width: height_map.width,
            height: height_map.height,
            data_size: height_map.data_size(),
            data: &height_map.data,
        }
    }
}

// ---------------------------------------------------------------------------
// the cliff-bit derivation (WorldHeightMap.cpp, pre-version-7 path)
// ---------------------------------------------------------------------------

/// `WorldHeightMap::getHeight` — the bounds-checked sample query (0
/// outside the map or when no height map loaded).
fn get_height(map: &SharedHeightMap<'_>, x_index: i32, y_index: i32) -> u8 {
    let ndx = y_index * map.width + x_index;
    if ndx >= 0 && ndx < map.data_size && !map.data.is_empty() {
        map.data[ndx as usize]
    } else {
        0
    }
}

/// `WorldHeightMap::setCliffState` — set one cell's cliff bit (the
/// bounds checks are the original's; the flip-state width is the
/// bitmap's row stride).
fn set_cliff_state(
    cell_cliff_state: &mut [u8],
    width: i32,
    height: i32,
    flip_state_width: i32,
    x_index: i32,
    y_index: i32,
    state: bool,
) {
    if x_index < 0 || y_index < 0 {
        return;
    }
    if y_index >= height {
        return;
    }
    if x_index >= width {
        return;
    }
    if cell_cliff_state.is_empty() {
        return;
    }
    let ndx = (y_index * flip_state_width + (x_index >> 3)) as usize;
    let flag_mask = 1u8 << (x_index & 0x7);
    if state {
        cell_cliff_state[ndx] |= flag_mask;
    } else {
        cell_cliff_state[ndx] &= !flag_mask;
    }
}

/// `WorldHeightMap::setCellCliffFlagFromHeights` — a cell is a cliff
/// when its four corner heights' scaled spread exceeds the slope limit.
fn set_cell_cliff_flag_from_heights(
    cell_cliff_state: &mut [u8],
    map: &SharedHeightMap<'_>,
    flip_state_width: i32,
    x_index: i32,
    y_index: i32,
) {
    let height1 = f32::from(get_height(map, x_index, y_index)) * MAP_HEIGHT_SCALE;
    let height2 = f32::from(get_height(map, x_index + 1, y_index)) * MAP_HEIGHT_SCALE;
    let height3 = f32::from(get_height(map, x_index, y_index + 1)) * MAP_HEIGHT_SCALE;
    let height4 = f32::from(get_height(map, x_index + 1, y_index + 1)) * MAP_HEIGHT_SCALE;
    let mut min_z = height1;
    if min_z > height2 {
        min_z = height2;
    }
    if min_z > height3 {
        min_z = height3;
    }
    if min_z > height4 {
        min_z = height4;
    }
    let mut max_z = height1;
    if max_z < height2 {
        max_z = height2;
    }
    if max_z < height3 {
        max_z = height3;
    }
    if max_z < height4 {
        max_z = height4;
    }
    let cliff_range = PATHFIND_CLIFF_SLOPE_LIMIT_F;
    let is_cliff = max_z - min_z > cliff_range;
    set_cliff_state(
        cell_cliff_state,
        map.width,
        map.height,
        flip_state_width,
        x_index,
        y_index,
        is_cliff,
    );
}

/// `WorldHeightMap::initCliffFlagsFromHeights` — the pre-version-7
/// cliff-bit derivation over every cell.
fn init_cliff_flags_from_heights(
    cell_cliff_state: &mut [u8],
    map: &SharedHeightMap<'_>,
    flip_state_width: i32,
) {
    for x_index in 0..map.width - 1 {
        for y_index in 0..map.height - 1 {
            set_cell_cliff_flag_from_heights(
                cell_cliff_state,
                map,
                flip_state_width,
                x_index,
                y_index,
            );
        }
    }
}

// ---------------------------------------------------------------------------
// the chunk reader (WorldHeightMap::ParseBlendTileData)
// ---------------------------------------------------------------------------

/// Reads one `i16` index array (`readArrayOfBytes` into `Short[]`):
/// `len` little-endian pairs, zero-filled on a short read.
fn read_short_array(file: &mut DataChunkInput<'_>, len: i32) -> Vec<i16> {
    let mut bytes = vec![0u8; len as usize * 2];
    file.read_array_of_bytes(&mut bytes);
    bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| i16::from_le_bytes(*p))
        .collect()
}

/// `WorldHeightMap::ParseBlendTileData` — the **full** load's blend-tile
/// chunk reader: the four index arrays, the flip/cliff bitmaps (read
/// directly from version 8, remapped at 7, derived from the height
/// samples before), the counts and their records, and — at version 1
/// exactly — the in-place downsample that halves the shared extent
/// members.
///
/// `map` is the height-map state as the walk holds it (see
/// [`SharedHeightMap`]); `use_3way_blends` is the
/// `Use3WayTerrainBlends` INI setting (the retail default is on) —
/// turned off, the extra-blend array is flushed and the blend records'
/// flip bits filtered.
///
/// The patch loop the constructor runs after the whole walk is
/// [`BlendTileData::patch_bad_maps`], not part of this reader.
pub fn parse_blend_tile_data(
    file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
    map: &SharedHeightMap<'_>,
    use_3way_blends: bool,
) -> Result<BlendTileData, BlendTileError> {
    let len = file.read_int();
    if map.data_size != len {
        return Err(BlendTileError::DataSizeMismatch {
            len,
            data_size: map.data_size,
        });
    }

    // the four index arrays: new Short[m_dataSize], zero-born here (the
    // original's heap is uninitialised; the never-read arrays are the
    // zero-fill stance — see the module docs)
    let data_size = map.data_size;
    let mut tile_ndxes = read_short_array(file, data_size);
    let mut blend_tile_ndxes = read_short_array(file, data_size);
    let mut extra_blend_tile_ndxes = if info.version >= K_BLEND_TILE_VERSION_6 {
        let mut a = read_short_array(file, data_size);
        // Allow clearing of extra blend tiles via ini and resaving of
        // map.  Useful for flushing out initial maps made with buggy
        // 3-way blending.
        if !use_3way_blends {
            a.iter_mut().for_each(|v| *v = 0);
        }
        a
    } else {
        vec![0i16; data_size as usize]
    };
    let mut cliff_info_ndxes = if info.version >= K_BLEND_TILE_VERSION_5 {
        read_short_array(file, data_size)
    } else {
        vec![0i16; data_size as usize]
    };

    // Note - we have one less cell than the width & height. But for
    // paranoia, allocate extra row. jba.
    let num_bytes_x = (map.width + 7) / 8; // how many bytes to fit all bitflags
    let num_bytes_y = map.height;
    let flip_state_width = num_bytes_x;

    let bitmap_len = num_bytes_x * num_bytes_y;
    let cell_flip_state = vec![0u8; bitmap_len as usize]; // clear all flags
    let mut cell_cliff_state = vec![0u8; bitmap_len as usize]; // clear all flags

    if info.version >= K_BLEND_TILE_VERSION_7 {
        if info.version == K_BLEND_TILE_VERSION_7 {
            // previous incorrect length that got used to save the file.
            // jba. [4/3/2003]
            let byte_width = (map.width + 1) / 8;
            let mut data = vec![0u8; (map.height * byte_width) as usize];
            file.read_array_of_bytes(&mut data);
            for j in 0..map.height {
                for i in 0..byte_width {
                    cell_cliff_state[(j * flip_state_width + i) as usize] =
                        data[(j * byte_width + i) as usize];
                }
            }
        } else {
            file.read_array_of_bytes(&mut cell_cliff_state);
        }
    } else {
        init_cliff_flags_from_heights(&mut cell_cliff_state, map, flip_state_width);
    }

    let num_bitmap_tiles = file.read_int(); // stored verbatim (the
    // "Unlikely numBitmapTiles"
    // assert is debug-only)
    let mut num_blended_tiles = file.read_int();
    if num_blended_tiles > NUM_BLEND_TILES {
        return Err(BlendTileError::TooManyBlendedTiles(num_blended_tiles));
    }
    let mut blended_tiles = vec![BlendTileInfo::default(); num_blended_tiles.max(0) as usize];
    let mut num_cliff_info = if info.version >= K_BLEND_TILE_VERSION_5 {
        let n = file.read_int();
        if n > NUM_CLIFF_INFO {
            return Err(BlendTileError::TooManyCliffInfos(n));
        }
        n
    } else {
        1 // cliffInfo[0] is the default info.
    };
    let mut cliff_infos = vec![CliffInfo::default(); num_cliff_info.max(0) as usize];

    // --> file loading here
    let num_texture_classes = file.read_int();
    if num_texture_classes > NUM_TEXTURE_CLASSES {
        return Err(BlendTileError::TooManyTextureClasses(num_texture_classes));
    }
    let mut texture_classes = Vec::new();
    for _ in 0..num_texture_classes {
        let first_tile = file.read_int();
        let num_tiles = file.read_int();
        let width = file.read_int();
        // legacy GDF data — read and discarded
        let _legacy = file.read_int();
        let name = file.read_ascii_string();
        // readTexClass (the TGA asset load) reads nothing from the chunk
        // stream — out of scope, see the module docs
        texture_classes.push(TxTextureClass {
            first_tile,
            num_tiles,
            width,
            name,
        });
    }

    let mut num_edge_texture_classes = 0;
    let mut num_edge_tiles = 0;
    let mut edge_texture_classes = Vec::new();
    if info.version >= K_BLEND_TILE_VERSION_4 {
        num_edge_tiles = file.read_int();
        num_edge_texture_classes = file.read_int();
        if num_edge_texture_classes > NUM_TEXTURE_CLASSES {
            return Err(BlendTileError::TooManyEdgeTextureClasses(
                num_edge_texture_classes,
            ));
        }
        for _ in 0..num_edge_texture_classes {
            let first_tile = file.read_int();
            let num_tiles = file.read_int();
            let width = file.read_int();
            let name = file.read_ascii_string();
            edge_texture_classes.push(TxTextureClass {
                first_tile,
                num_tiles,
                width,
                name,
            });
        }
    }

    for i in 1..num_blended_tiles {
        let blend_ndx = file.read_int();
        let horiz = file.read_byte();
        let vert = file.read_byte();
        let right_diagonal = file.read_byte();
        let left_diagonal = file.read_byte();
        let mut inverted = file.read_byte();
        // Allow clearing of extra blend tiles via ini and resaving of
        // map.  Useful for flushing out initial maps made with buggy
        // 3-way blending.
        if !use_3way_blends {
            inverted &= !FLIPPED_MASK; // filter out extra flips from 3-way
        }
        let long_diagonal = if info.version >= K_BLEND_TILE_VERSION_3 {
            file.read_byte()
        } else {
            0 // false
        };
        let custom_blend_edge_class = if info.version >= K_BLEND_TILE_VERSION_4 {
            file.read_int()
        } else {
            -1
        };
        let flag = file.read_int();
        if flag != FLAG_VAL {
            return Err(BlendTileError::InvalidBlendFlag { index: i, flag });
        }
        blended_tiles[i as usize] = BlendTileInfo {
            blend_ndx,
            horiz,
            vert,
            right_diagonal,
            left_diagonal,
            inverted,
            long_diagonal,
            custom_blend_edge_class,
        };
    }

    if info.version >= K_BLEND_TILE_VERSION_5 {
        for i in 1..num_cliff_info {
            let tile_index = file.read_int() as i16; // the Short truncation
            let u0 = file.read_real();
            let v0 = file.read_real();
            let u1 = file.read_real();
            let v1 = file.read_real();
            let u2 = file.read_real();
            let v2 = file.read_real();
            let u3 = file.read_real();
            let v3 = file.read_real();
            let flip = file.read_byte() != 0;
            let mutant = file.read_byte() != 0;
            cliff_infos[i as usize] = CliffInfo {
                tile_index,
                u0,
                v0,
                u1,
                v1,
                u2,
                v2,
                u3,
                v3,
                flip,
                mutant,
            };
        }
    }

    // Resize me. (version 1: the 5.0-per-cell grid, halved in place)
    let mut width = map.width;
    let mut height = map.height;
    let mut final_data_size = map.data_size;
    if info.version == K_BLEND_TILE_VERSION_1 {
        let new_width = (map.width + 1) / 2;
        let new_height = (map.height + 1) / 2;
        for i in 0..new_height {
            for j in 0..new_width {
                tile_ndxes[(i * new_width + j) as usize] =
                    tile_ndxes[(2 * i * map.width + 2 * j) as usize];
                blend_tile_ndxes[(i * new_width + j) as usize] = 0;
                extra_blend_tile_ndxes[(i * new_width + j) as usize] = 0;
                cliff_info_ndxes[(i * new_width + j) as usize] = 0;
            }
        }
        num_blended_tiles = 1;
        num_cliff_info = 1;
        blended_tiles = vec![BlendTileInfo::default(); 1];
        cliff_infos = vec![CliffInfo::default(); 1];
        width = new_width;
        height = new_height;
        final_data_size = new_width * new_height;
    }

    // DEBUG_ASSERTCRASH(file.atEndOfChunk(), ("Unexpected data left
    // over.")) — a debug-only contract; the release load leaves the
    // leftovers to the walk's close, and so does the port.

    Ok(BlendTileData {
        num_bitmap_tiles,
        num_blended_tiles,
        num_cliff_info,
        num_texture_classes,
        num_edge_tiles,
        num_edge_texture_classes,
        width,
        height,
        data_size: final_data_size,
        flip_state_width,
        tile_ndxes,
        blend_tile_ndxes,
        extra_blend_tile_ndxes,
        cliff_info_ndxes,
        cell_flip_state,
        cell_cliff_state,
        texture_classes,
        edge_texture_classes,
        blended_tiles,
        cliff_infos,
    })
}

impl BlendTileData {
    /// The constructor's post-walk **"patch bad maps"** loop: clamp the
    /// cliff, blend and extra-blend indices into `[0, count)` over the
    /// live data size (the tile indices are never clamped). A later
    /// `HeightMapData` can shrink the live count below the arrays'
    /// length — the entries beyond it keep their raw values, exactly the
    /// tail the original's loop leaves unwalked.
    ///
    /// The loop bound is the **live** `m_dataSize` the constructor
    /// holds after the walk (the version-1 blend resize may have
    /// halved it); [`TerrainLoad::data_size`] is that value.
    pub fn patch_bad_maps(&mut self, data_size: i32) {
        for i in 0..data_size {
            let i = i as usize;
            if let Some(v) = self.cliff_info_ndxes.get_mut(i)
                && (*v < 0 || i32::from(*v) >= self.num_cliff_info)
            {
                *v = 0;
            }
            if let Some(v) = self.blend_tile_ndxes.get_mut(i)
                && (*v < 0 || i32::from(*v) >= self.num_blended_tiles)
            {
                *v = 0;
            }
            if let Some(v) = self.extra_blend_tile_ndxes.get_mut(i)
                && (*v < 0 || i32::from(*v) >= self.num_blended_tiles)
            {
                *v = 0;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// the constructor's full-load walk
// ---------------------------------------------------------------------------

/// The constructor's full-load result: the height map and the blend
/// tiles as their readers left them, plus the **shared extent members**
/// as the whole load leaves them.
///
/// The two payloads share `m_width`/`m_height`/`m_dataSize` in the
/// original: a version-1 blend resize re-points them (halved) after the
/// height-map reader is done, and a later `HeightMapData` overwrites
/// them again. [`Self::width`]/[`Self::height`]/[`Self::data_size`]
/// are those live members; `height_map`'s own extent fields are what
/// *its* reader wrote (the two disagree exactly when a version-1 blend
/// resize ran and no later height map followed), and
/// `height_map.data_size()` stays the declared sample count — the
/// samples array is never resized.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TerrainLoad {
    /// The `HeightMapData` chunk's state (absent when the map carries
    /// none).
    pub height_map: Option<HeightMapData>,
    /// The `BlendTileData` chunk's state, **post-patch** (absent when
    /// the map carries none — and refused by
    /// [`from_map`](Self::from_map) when a height map loaded without
    /// one).
    pub blend_tiles: Option<BlendTileData>,
    /// The final `m_width` (border included).
    pub width: i32,
    /// The final `m_height`.
    pub height: i32,
    /// The final `m_dataSize` — the live sample-count word (the height
    /// map's declared count, or the halved count a version-1 blend
    /// resize left).
    pub data_size: i32,
}

/// The walk's user state: the shared height-map members (the original's
/// `this`, cut down to the terrain half), the live data-size word, and
/// the blend payload.
struct TerrainWalk {
    hm: HeightMapData,
    hm_loaded: bool,
    live_data_size: i32,
    bt: Option<BlendTileData>,
    use_3way: bool,
    error: Option<BlendTileError>,
}

/// The registered `HeightMapData` parser (the full load's reader,
/// `ParseHeightMapDataChunk`'s dispatch).
fn height_map_chunk_parser(
    file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
    _table: &mut ParserTable<'_, TerrainWalk>,
    user: &mut TerrainWalk,
) -> bool {
    match parse_height_map_data(file, info) {
        Ok(hm) => {
            user.live_data_size = hm.data_size();
            user.hm = hm;
            user.hm_loaded = true;
            true
        }
        Err(e) => {
            user.error = Some(BlendTileError::HeightMap(e));
            false
        }
    }
}

/// The registered `BlendTileData` parser (`ParseBlendTileDataChunk`'s
/// dispatch): the reader over the live shared members, then the
/// version-1 resize's re-point of them.
fn blend_tile_chunk_parser(
    file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
    _table: &mut ParserTable<'_, TerrainWalk>,
    user: &mut TerrainWalk,
) -> bool {
    let map = SharedHeightMap {
        width: user.hm.width,
        height: user.hm.height,
        data_size: user.live_data_size,
        data: &user.hm.data,
    };
    match parse_blend_tile_data(file, info, &map, user.use_3way) {
        Ok(bt) => {
            // the version-1 resize re-points the shared members (a
            // no-op for every other version: the parse-end extents are
            // the ones it read)
            user.hm.width = bt.width;
            user.hm.height = bt.height;
            user.live_data_size = bt.data_size;
            user.bt = Some(bt);
            true
        }
        Err(e) => {
            user.error = Some(e);
            false
        }
    }
}

impl TerrainLoad {
    /// The `WorldHeightMap` constructor's **full-load** walk, restricted
    /// to the terrain payloads: `HeightMapData` and `BlendTileData`
    /// registered at the global scope (last-wins repeats, unknown
    /// labels skipped whole), followed by the constructor's post-walk
    /// patch loop over the live data size.
    ///
    /// `use_3way_blends` is the `Use3WayTerrainBlends` INI setting (the
    /// retail default is on). The other payloads the constructor
    /// registers (`GlobalLighting`) arrive with their own slices — here
    /// their chunks are skipped whole, the walk's behaviour for an
    /// unmatched label. A height map without a blend chunk is the
    /// patch loop's null-array death in the original — refused here
    /// with [`BlendTileError::PatchWithoutBlendTiles`].
    pub fn from_map(bytes: &[u8], use_3way_blends: bool) -> Result<Self, BlendTileError> {
        let mut input = DataChunkInput::new(bytes);
        let mut table: ParserTable<'_, TerrainWalk> = ParserTable::new();
        table.register(HEIGHT_MAP_DATA_LABEL, b"", height_map_chunk_parser);
        table.register(BLEND_TILE_DATA_LABEL, b"", blend_tile_chunk_parser);
        let mut user = TerrainWalk {
            hm: HeightMapData::default(),
            hm_loaded: false,
            live_data_size: 0,
            bt: None,
            use_3way: use_3way_blends,
            error: None,
        };
        if !input.parse(&mut table, &mut user) {
            // A parser's false is either payload's refused load or the
            // walk's own refusal; the original throws the same code for
            // both (the constructor's `if (!file.parse(this))`).
            return Err(user.error.unwrap_or(BlendTileError::CorruptFileFormat));
        }

        // patch bad maps. (the constructor's post-walk fixup over the
        // live data size)
        let data_size = user.live_data_size;
        let width = user.hm.width;
        let height = user.hm.height;
        if data_size > 0 && user.bt.is_none() {
            return Err(BlendTileError::PatchWithoutBlendTiles { data_size });
        }
        if let Some(bt) = &mut user.bt {
            bt.patch_bad_maps(data_size);
        }

        Ok(TerrainLoad {
            height_map: if user.hm_loaded { Some(user.hm) } else { None },
            blend_tiles: user.bt,
            width,
            height,
            data_size,
        })
    }
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map_chunk::DataChunkOutput;
    use crate::map_heightmap::{K_HEIGHT_MAP_VERSION_1, K_HEIGHT_MAP_VERSION_2};

    /// Builds a map file around height-map and blend-tile payloads.
    struct MapFile {
        out: DataChunkOutput,
    }

    impl MapFile {
        fn new() -> Self {
            MapFile {
                out: DataChunkOutput::new(),
            }
        }

        /// A version-2 height map (no border word, synthesised
        /// boundary) over the given samples.
        fn height_map(&mut self, w: i32, h: i32, data: &[u8]) -> &mut Self {
            self.out
                .open_data_chunk(HEIGHT_MAP_DATA_LABEL, K_HEIGHT_MAP_VERSION_2);
            self.out.write_int(w);
            self.out.write_int(h);
            self.out.write_int(data.len() as i32);
            self.out.write_array_of_bytes(data);
            self.out.close_data_chunk();
            self
        }

        /// A version-1 height map (the downsample flavour).
        fn height_map_v1(&mut self, w: i32, h: i32, data: &[u8]) -> &mut Self {
            self.out
                .open_data_chunk(HEIGHT_MAP_DATA_LABEL, K_HEIGHT_MAP_VERSION_1);
            self.out.write_int(w);
            self.out.write_int(h);
            self.out.write_int(data.len() as i32);
            self.out.write_array_of_bytes(data);
            self.out.close_data_chunk();
            self
        }

        fn blend(&mut self, version: u16, payload: &[u8]) -> &mut Self {
            self.out.open_data_chunk(BLEND_TILE_DATA_LABEL, version);
            self.out.write_array_of_bytes(payload);
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
    fn put_i16s(b: &mut Vec<u8>, v: &[i16]) {
        for x in v {
            b.extend_from_slice(&x.to_le_bytes());
        }
    }
    fn put_f32(b: &mut Vec<u8>, v: f32) {
        b.extend_from_slice(&v.to_le_bytes());
    }
    fn put_name(b: &mut Vec<u8>, s: &[u8]) {
        b.extend_from_slice(&(s.len() as u16).to_le_bytes());
        b.extend_from_slice(s);
    }

    /// One blend record the payload builder writes.
    #[derive(Clone)]
    struct Rec {
        ndx: i32,
        horiz: u8,
        vert: u8,
        rdiag: u8,
        ldiag: u8,
        inv: u8,
        longd: u8,
        custom: i32,
        flag: i32,
    }

    impl Rec {
        fn good() -> Self {
            Rec {
                ndx: 1,
                horiz: 1,
                vert: 2,
                rdiag: 3,
                ldiag: 4,
                inv: 1,
                longd: 0,
                custom: -1,
                flag: FLAG_VAL,
            }
        }
    }

    /// One cliff record the payload builder writes.
    #[derive(Clone)]
    struct Cliff {
        tile_idx: i32,
        uvs: [f32; 8],
        flip: u8,
        mutant: u8,
    }

    /// The version-gated payload builder (the writer side of the
    /// WHeightMapEdit.cpp shape — the same layout the golden generator
    /// assembles). `cliff_state` is sized by the caller: the
    /// `height × (width+1)/8` bytes version 7 stores, or the
    /// `height × (width+7)/8` bitmap version 8 reads.
    struct Parts {
        len: i32,
        width: i32,
        height: i32,
        tile: Vec<i16>,
        blend: Vec<i16>,
        extra: Vec<i16>,
        cliff: Vec<i16>,
        /// The stored cliff-state bytes; `None` lays down the
        /// version-correct zero-filled size (the `height x (width+1)/8`
        /// bytes version 7 stores, the `height x (width+7)/8` bitmap
        /// version 8 reads).
        cliff_state: Option<Vec<u8>>,
        nbmp: i32,
        nblend: i32,
        ncliff: i32,
        ntex: i32,
        names: Vec<Vec<u8>>,
        nedgetiles: i32,
        netex: i32,
        etex: Vec<Vec<u8>>,
        records: Vec<Rec>,
        cliffs: Vec<Cliff>,
    }

    impl Parts {
        /// `width`/`height` are the map's extent as the blend reader
        /// sees them (the version-8 cliff-state bitmap is
        /// `(width+7)/8 × height` bytes).
        fn new(len: i32, width: i32, height: i32) -> Self {
            Parts {
                len,
                width,
                height,
                tile: vec![0; len as usize],
                blend: vec![0; len as usize],
                extra: vec![0; len as usize],
                cliff: vec![0; len as usize],
                cliff_state: None,
                nbmp: 1,
                nblend: 1,
                ncliff: 1,
                ntex: 0,
                names: Vec::new(),
                nedgetiles: 0,
                netex: 0,
                etex: Vec::new(),
                records: Vec::new(),
                cliffs: Vec::new(),
            }
        }

        fn bytes(&self, version: u16) -> Vec<u8> {
            let mut b = Vec::new();
            put_i32(&mut b, self.len);
            put_i16s(&mut b, &self.tile);
            put_i16s(&mut b, &self.blend);
            if version >= K_BLEND_TILE_VERSION_6 {
                put_i16s(&mut b, &self.extra);
            }
            if version >= K_BLEND_TILE_VERSION_5 {
                put_i16s(&mut b, &self.cliff);
            }
            if version >= K_BLEND_TILE_VERSION_7 {
                match &self.cliff_state {
                    Some(cs) => b.extend_from_slice(cs),
                    None => {
                        let w = if version == K_BLEND_TILE_VERSION_7 {
                            (self.width + 1) / 8
                        } else {
                            (self.width + 7) / 8
                        };
                        b.resize(b.len() + (self.height * w) as usize, 0);
                    }
                }
            }
            put_i32(&mut b, self.nbmp);
            put_i32(&mut b, self.nblend);
            if version >= K_BLEND_TILE_VERSION_5 {
                put_i32(&mut b, self.ncliff);
            }
            put_i32(&mut b, self.ntex);
            for name in &self.names {
                put_i32(&mut b, 0); // first tile
                put_i32(&mut b, 4); // num tiles
                put_i32(&mut b, 2); // width
                put_i32(&mut b, 0); // the legacy GDF word
                put_name(&mut b, name);
            }
            if version >= K_BLEND_TILE_VERSION_4 {
                put_i32(&mut b, self.nedgetiles);
                put_i32(&mut b, self.netex);
                for name in &self.etex {
                    put_i32(&mut b, 0);
                    put_i32(&mut b, 4);
                    put_i32(&mut b, 2);
                    put_name(&mut b, name); // (no legacy word)
                }
            }
            for r in &self.records {
                put_i32(&mut b, r.ndx);
                b.extend_from_slice(&[r.horiz, r.vert, r.rdiag, r.ldiag, r.inv]);
                if version >= K_BLEND_TILE_VERSION_3 {
                    b.push(r.longd);
                }
                if version >= K_BLEND_TILE_VERSION_4 {
                    put_i32(&mut b, r.custom);
                }
                put_i32(&mut b, r.flag);
            }
            if version >= K_BLEND_TILE_VERSION_5 {
                for c in &self.cliffs {
                    put_i32(&mut b, c.tile_idx);
                    for f in c.uvs {
                        put_f32(&mut b, f);
                    }
                    b.push(c.flip);
                    b.push(c.mutant);
                }
            }
            b
        }
    }

    #[test]
    fn the_constants_transcribe() {
        assert_eq!(BLEND_TILE_DATA_LABEL, b"BlendTileData");
        assert_eq!(NUM_BLEND_TILES, 16192);
        assert_eq!(NUM_CLIFF_INFO, 32384);
        assert_eq!(NUM_TEXTURE_CLASSES, 256);
        assert_eq!(FLAG_VAL, 0x7ADA_0000);
        assert_eq!(INVERTED_MASK, 0x1);
        assert_eq!(FLIPPED_MASK, 0x2);
        assert_eq!(MAP_HEIGHT_SCALE, 0.625);
        assert_eq!(K_BLEND_TILE_VERSION_1, 1);
        assert_eq!(K_BLEND_TILE_VERSION_8, 8);
    }

    #[test]
    fn a_modern_v8_map_loads() {
        let mut map = MapFile::new();
        map.height_map(4, 2, &[5; 8]);
        let mut p = Parts::new(8, 4, 2);
        p.tile = vec![1, 2, 3, 4, 5, 6, 7, 8];
        p.cliff_state = Some(vec![0x0f, 0xf0]); // fsw 1 * height 2
        p.nbmp = 3;
        p.nblend = 2;
        p.ncliff = 2;
        p.ntex = 1;
        p.names = vec![b"GrassA".to_vec()];
        p.nedgetiles = 8;
        p.netex = 1;
        p.etex = vec![b"RoadC".to_vec()];
        p.records = vec![Rec::good()];
        p.cliffs = vec![Cliff {
            tile_idx: 70000, // -> 4464 as i16
            uvs: [0.0, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.5],
            flip: 2, // nonzero -> true
            mutant: 0,
        }];
        map.blend(K_BLEND_TILE_VERSION_8, &p.bytes(K_BLEND_TILE_VERSION_8));

        let load = TerrainLoad::from_map(&map.bytes(), true).expect("loads");
        assert_eq!((load.width, load.height, load.data_size), (4, 2, 8));
        let bt = load.blend_tiles.as_ref().unwrap();
        assert_eq!(bt.tile_ndxes, [1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(bt.cell_cliff_state, [0x0f, 0xf0]);
        assert_eq!(bt.num_bitmap_tiles, 3);
        assert_eq!(bt.num_blended_tiles, 2);
        assert_eq!(bt.num_cliff_info, 2);
        assert_eq!(bt.num_texture_classes, 1);
        assert_eq!(bt.num_edge_tiles, 8);
        assert_eq!(bt.num_edge_texture_classes, 1);
        assert_eq!(bt.texture_classes[0].name, b"GrassA");
        assert_eq!(bt.edge_texture_classes[0].name, b"RoadC");
        assert_eq!(bt.blended_tiles.len(), 2);
        assert_eq!(bt.blended_tiles[1].horiz, 1);
        assert_eq!(bt.blended_tiles[1].custom_blend_edge_class, -1);
        // the i32 -> i16 truncation
        assert_eq!(bt.cliff_infos[1].tile_index, 4464);
        assert_eq!(bt.cliff_infos[1].u3, 3.0);
        assert!(bt.cliff_infos[1].flip); // the byte 2
        assert!(!bt.cliff_infos[1].mutant);
        // the patch: 0 is in range for both counts, the arrays stand
        assert_eq!(bt.blend_tile_ndxes, [0; 8]);
    }

    #[test]
    fn the_cliff_boundary_is_sixteen_samples() {
        // heights: a 16-unit jump on row 0 (a cliff), a 15-unit jump on
        // row 2 (below the 9.8 limit over the 10/16 scale — no cliff)
        let mut h = Vec::new();
        for y in 0..3 {
            for x in 0..5 {
                let v = if y == 0 && x >= 3 {
                    16
                } else if y == 2 && x >= 3 {
                    15
                } else {
                    0
                };
                h.push(v);
            }
        }
        let mut map = MapFile::new();
        map.height_map(5, 3, &h);
        let p = Parts::new(15, 5, 3);
        // version 6: no cliff state is stored — it is derived
        map.blend(K_BLEND_TILE_VERSION_6, &p.bytes(K_BLEND_TILE_VERSION_6));

        let load = TerrainLoad::from_map(&map.bytes(), true).expect("loads");
        let bt = load.blend_tiles.as_ref().unwrap();
        // cells (2,0) and (3,0) span the 16-unit step: bits 2 and 3 of
        // row 0; the 15-unit step on row 2 makes no cliff
        assert_eq!(bt.cell_cliff_state, [0x0c, 0, 0]);
        // the never-read arrays of a version-6 chunk (cliff indices are
        // read; the extra layer is) — and version 5 reads neither extra
        // nor cliff-state
        assert_eq!(bt.extra_blend_tile_ndxes, [0; 15]);
    }

    #[test]
    fn the_v5_zero_filled_arrays() {
        let mut map = MapFile::new();
        map.height_map(4, 2, &[0; 8]);
        let mut p = Parts::new(8, 4, 2);
        // version 5: no extra layer stored (the zero-fill stance), the
        // cliff indices ARE stored
        p.ncliff = 5; // the patch keeps values below the count
        let mut b = p.bytes(K_BLEND_TILE_VERSION_5);
        // rewrite the cliff index array (offset: len 4 + tile 16 +
        // blend 16): the i16s 1, 2, 3, 4 then zeros
        let cliff_at = 4 + 16 + 16;
        let mut cliff_bytes = Vec::new();
        put_i16s(&mut cliff_bytes, &[1, 2, 3, 4, 0, 0, 0, 0]);
        b[cliff_at..cliff_at + 16].copy_from_slice(&cliff_bytes);
        map.blend(K_BLEND_TILE_VERSION_5, &b);

        let load = TerrainLoad::from_map(&map.bytes(), true).expect("loads");
        let bt = load.blend_tiles.as_ref().unwrap();
        assert_eq!(bt.cliff_info_ndxes, [1, 2, 3, 4, 0, 0, 0, 0]);
        assert_eq!(bt.extra_blend_tile_ndxes, [0; 8]); // never read
        assert_eq!(bt.num_cliff_info, 5); // the word as read
    }

    #[test]
    fn the_v7_remap_pads_the_rows() {
        // 17 wide: byteWidth (17+1)/8 = 2 stored per row, fsw
        // (17+7)/8 = 3 in the bitmap — each row's third byte stays zero
        let mut map = MapFile::new();
        map.height_map(17, 3, &[0; 51]);
        let mut p = Parts::new(51, 17, 3);
        p.cliff_state = Some(vec![0xaa, 0x55, 0x11, 0x22, 0x33, 0x44]); // 3 rows x 2
        p.nblend = 2;
        p.records = vec![Rec::good()];
        p.ncliff = 2;
        p.cliffs = vec![Cliff {
            tile_idx: 300,
            uvs: [0.0, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.5],
            flip: 1,
            mutant: 1,
        }];
        map.blend(K_BLEND_TILE_VERSION_7, &p.bytes(K_BLEND_TILE_VERSION_7));

        let load = TerrainLoad::from_map(&map.bytes(), true).expect("loads");
        let bt = load.blend_tiles.as_ref().unwrap();
        assert_eq!(bt.flip_state_width, 3);
        assert_eq!(
            bt.cell_cliff_state,
            [0xaa, 0x55, 0, 0x11, 0x22, 0, 0x33, 0x44, 0]
        );
        // the 1-wide extreme: (1+1)/8 = 0 bytes stored per row
        let mut map = MapFile::new();
        map.height_map(1, 2, &[0, 0]);
        let p = Parts::new(2, 1, 2);
        map.blend(K_BLEND_TILE_VERSION_7, &p.bytes(K_BLEND_TILE_VERSION_7));
        let load = TerrainLoad::from_map(&map.bytes(), true).expect("loads");
        assert_eq!(load.blend_tiles.as_ref().unwrap().cell_cliff_state, [0, 0]);
    }

    #[test]
    fn the_v1_resize_halves_everything() {
        let mut map = MapFile::new();
        map.height_map_v1(
            5,
            5,
            &(0..25).map(|i| (i * 11 % 251) as u8).collect::<Vec<u8>>(),
        );
        // a v1 blend chunk: len 25, tile indices 0..25, 2 records
        let mut p = Parts::new(25, 5, 5);
        p.tile = (0..25).map(|i| i as i16).collect();
        p.blend = vec![7; 25]; // nonzero (the head is zeroed by the resize)
        p.nbmp = 1;
        p.nblend = 3; // forced to 1
        p.records = vec![Rec::good(), Rec::good()];
        map.blend(K_BLEND_TILE_VERSION_1, &p.bytes(K_BLEND_TILE_VERSION_1));

        let load = TerrainLoad::from_map(&map.bytes(), true).expect("loads");
        // the shared members are halved; the height-map half reports
        // them (its own reader left the file extents — the resize
        // re-pointed the shared ones after)
        assert_eq!((load.width, load.height, load.data_size), (3, 3, 9));
        let bt = load.blend_tiles.as_ref().unwrap();
        assert_eq!((bt.width, bt.height, bt.data_size), (3, 3, 9));
        assert_eq!(bt.num_blended_tiles, 1);
        assert_eq!(bt.num_cliff_info, 1);
        assert_eq!(bt.blended_tiles.len(), 1);
        assert_eq!(bt.cliff_infos.len(), 1);
        // the compaction keeps every other column of every other row
        let compacted: Vec<i16> = [0, 2, 4, 10, 12, 14, 20, 22, 24]
            .iter()
            .map(|&i| i as i16)
            .collect();
        assert_eq!(&bt.tile_ndxes[..9], &compacted[..]);
        // the tail keeps the raw values
        assert_eq!(&bt.tile_ndxes[9..], &(9..25i16).collect::<Vec<_>>()[..]);
        // the blend/extra/cliff heads were zeroed by the resize; the
        // patch (bound 9) never touches the tail
        assert_eq!(&bt.blend_tile_ndxes[..9], &[0; 9]);
        assert_eq!(&bt.blend_tile_ndxes[9..], &vec![7i16; 16][..]);
        // the height-map half: the halved extents with the declared
        // sample count the samples array keeps
        let hm = load.height_map.as_ref().unwrap();
        assert_eq!((hm.width, hm.height), (3, 3));
        assert_eq!(hm.data_size(), 25);
    }

    #[test]
    fn the_len_check_reads_the_live_data_size() {
        // a v1 blend halves the live m_dataSize; a second, version-8
        // chunk's len word must match the HALVED count
        let mut map = MapFile::new();
        map.height_map_v1(4, 4, &[9; 16]);
        let mut p1 = Parts::new(16, 4, 4);
        p1.nbmp = 1;
        p1.nblend = 1;
        map.blend(K_BLEND_TILE_VERSION_1, &p1.bytes(K_BLEND_TILE_VERSION_1));

        // len 16 (the stale file extent): refused — the live count is 4
        let mut stale = Parts::new(16, 4, 4);
        stale.nbmp = 1;
        let mut map2 = MapFile::new();
        map2.out = map.out.clone();
        map2.blend(K_BLEND_TILE_VERSION_8, &stale.bytes(K_BLEND_TILE_VERSION_8));
        assert_eq!(
            TerrainLoad::from_map(&map2.bytes(), true),
            Err(BlendTileError::DataSizeMismatch {
                len: 16,
                data_size: 4
            })
        );

        // len 4 (the halved count): loads
        let mut fresh = Parts::new(4, 2, 2);
        fresh.cliff_state = Some(vec![0x77, 0x7e]); // fsw 1 * h 2
        map.blend(K_BLEND_TILE_VERSION_8, &fresh.bytes(K_BLEND_TILE_VERSION_8));
        let load = TerrainLoad::from_map(&map.bytes(), true).expect("loads");
        assert_eq!(load.blend_tiles.as_ref().unwrap().tile_ndxes.len(), 4);
        assert_eq!(
            load.blend_tiles.as_ref().unwrap().cell_cliff_state,
            [0x77, 0x7e]
        );
    }

    #[test]
    fn the_3way_gate_filters_flips_and_flushes_extras() {
        let mut map = MapFile::new();
        map.height_map(2, 1, &[0, 0]);
        let mut p = Parts::new(2, 2, 1);
        p.extra = vec![1, 0];
        p.nblend = 2;
        let mut r = Rec::good();
        r.inv = 0x03;
        p.records = vec![r];
        map.blend(K_BLEND_TILE_VERSION_8, &p.bytes(K_BLEND_TILE_VERSION_8));
        let bytes = map.bytes();

        let on = TerrainLoad::from_map(&bytes, true).expect("on");
        assert_eq!(
            on.blend_tiles.as_ref().unwrap().blended_tiles[1].inverted,
            0x03
        );
        assert_eq!(
            on.blend_tiles.as_ref().unwrap().extra_blend_tile_ndxes,
            [1, 0]
        );

        let off = TerrainLoad::from_map(&bytes, false).expect("off");
        assert_eq!(
            off.blend_tiles.as_ref().unwrap().blended_tiles[1].inverted,
            0x01
        );
        assert_eq!(
            off.blend_tiles.as_ref().unwrap().extra_blend_tile_ndxes,
            [0, 0]
        );
    }

    #[test]
    fn the_error_contracts_transcribe() {
        // the len mismatch
        let mut map = MapFile::new();
        map.height_map(2, 2, &[1, 2, 3, 4]);
        let p = Parts::new(3, 2, 2);
        map.blend(K_BLEND_TILE_VERSION_8, &p.bytes(K_BLEND_TILE_VERSION_8));
        assert_eq!(
            TerrainLoad::from_map(&map.bytes(), true),
            Err(BlendTileError::DataSizeMismatch {
                len: 3,
                data_size: 4
            })
        );

        // the sentinel
        let mut map = MapFile::new();
        map.height_map(2, 1, &[0, 0]);
        let mut p = Parts::new(2, 2, 1);
        p.nblend = 2;
        let mut r = Rec::good();
        r.flag = 0x7ADA_0001u32 as i32;
        p.records = vec![r];
        map.blend(K_BLEND_TILE_VERSION_8, &p.bytes(K_BLEND_TILE_VERSION_8));
        assert_eq!(
            TerrainLoad::from_map(&map.bytes(), true),
            Err(BlendTileError::InvalidBlendFlag {
                index: 1,
                flag: 0x7ADA_0001u32 as i32
            })
        );

        // a height map without a blend chunk: the patch loop's null
        // arrays, refused
        let mut map = MapFile::new();
        map.height_map(3, 2, &[7; 6]);
        assert_eq!(
            TerrainLoad::from_map(&map.bytes(), true),
            Err(BlendTileError::PatchWithoutBlendTiles { data_size: 6 })
        );

        // not a chunky file
        assert_eq!(
            TerrainLoad::from_map(b"not a map", true),
            Err(BlendTileError::CorruptFileFormat)
        );

        // the height-map half's refusal, wrapped
        let mut map = MapFile::new();
        map.out
            .open_data_chunk(HEIGHT_MAP_DATA_LABEL, K_HEIGHT_MAP_VERSION_2);
        map.out.write_int(0); // width: refuses
        map.out.write_int(2);
        map.out.write_int(4);
        map.out.close_data_chunk();
        assert_eq!(
            TerrainLoad::from_map(&map.bytes(), true),
            Err(BlendTileError::HeightMap(
                crate::map_heightmap::HeightMapError::InvalidMapSize {
                    width: 0,
                    height: 2,
                    border_size: 0
                }
            ))
        );
    }

    #[test]
    fn the_count_bounds_are_the_originals_arrays() {
        // a 1x1 map with the payload under test
        let read = |p: &Parts| {
            let mut map = MapFile::new();
            map.height_map(1, 1, &[42]);
            map.blend(K_BLEND_TILE_VERSION_8, &p.bytes(K_BLEND_TILE_VERSION_8));
            TerrainLoad::from_map(&map.bytes(), true)
        };

        // numBlendedTiles == NUM_BLEND_TILES passes its bound: the
        // records zero-fill at EOF and the sentinel reads 0 (the flag
        // refusal — one record in — proves the bound itself passed)
        let mut p = Parts::new(1, 1, 1);
        p.nblend = NUM_BLEND_TILES;
        assert_eq!(
            read(&p),
            Err(BlendTileError::InvalidBlendFlag { index: 1, flag: 0 })
        );
        // one past: the bound refuses before any record
        let mut p = Parts::new(1, 1, 1);
        p.nblend = NUM_BLEND_TILES + 1;
        assert_eq!(
            read(&p),
            Err(BlendTileError::TooManyBlendedTiles(NUM_BLEND_TILES + 1))
        );

        // numCliffInfo: the exact bound loads (the tables zero-fill at
        // EOF), one past refuses
        let mut p = Parts::new(1, 1, 1);
        p.ncliff = NUM_CLIFF_INFO;
        assert!(read(&p).is_ok());
        let mut p = Parts::new(1, 1, 1);
        p.ncliff = NUM_CLIFF_INFO + 1;
        assert_eq!(
            read(&p),
            Err(BlendTileError::TooManyCliffInfos(NUM_CLIFF_INFO + 1))
        );

        // the class bounds (the exact bound's classes zero-fill at EOF)
        let mut p = Parts::new(1, 1, 1);
        p.ntex = NUM_TEXTURE_CLASSES;
        let load = read(&p).expect("loads");
        assert_eq!(
            load.blend_tiles.as_ref().unwrap().texture_classes.len(),
            NUM_TEXTURE_CLASSES as usize
        );
        let mut p = Parts::new(1, 1, 1);
        p.ntex = NUM_TEXTURE_CLASSES + 1;
        assert_eq!(
            read(&p),
            Err(BlendTileError::TooManyTextureClasses(
                NUM_TEXTURE_CLASSES + 1
            ))
        );

        // numEdgeTextureClasses over its bound
        let mut p = Parts::new(1, 1, 1);
        p.nedgetiles = 4;
        p.netex = NUM_TEXTURE_CLASSES + 1;
        assert_eq!(
            read(&p),
            Err(BlendTileError::TooManyEdgeTextureClasses(
                NUM_TEXTURE_CLASSES + 1
            ))
        );

        // negative counts skip their loops and clamp everything through
        // the patch (nothing is below a non-positive bound)
        let mut map = MapFile::new();
        map.height_map(3, 2, &[0; 6]);
        let mut p = Parts::new(6, 3, 2);
        p.blend = vec![-1, 2, -3, 4, -5, 6];
        p.nbmp = 2_000_000_000; // the assert is debug-only: stored verbatim
        p.nblend = -3;
        p.ncliff = -4;
        p.ntex = -1;
        p.nedgetiles = -7;
        p.netex = -2;
        map.blend(K_BLEND_TILE_VERSION_8, &p.bytes(K_BLEND_TILE_VERSION_8));
        let load = TerrainLoad::from_map(&map.bytes(), true).expect("loads");
        let bt = load.blend_tiles.as_ref().unwrap();
        assert_eq!(bt.num_bitmap_tiles, 2_000_000_000);
        assert_eq!(bt.num_blended_tiles, -3);
        assert_eq!(bt.num_cliff_info, -4);
        assert_eq!(bt.num_texture_classes, -1);
        assert_eq!(bt.num_edge_tiles, -7);
        assert_eq!(bt.num_edge_texture_classes, -2);
        assert!(bt.texture_classes.is_empty());
        assert!(bt.blended_tiles.is_empty());
        assert!(bt.cliff_infos.is_empty());
        // the patch clamps every index to 0 against the negative bounds
        assert_eq!(bt.blend_tile_ndxes, [0; 6]);
    }

    #[test]
    fn the_patch_loop_clamps_and_skips() {
        // out-of-range and negative indices clamp; the tile indices
        // never do; a later HeightMapData shrinks the walked bound
        let mut map = MapFile::new();
        map.height_map(5, 2, &[0; 10]);
        let mut p = Parts::new(10, 5, 2);
        p.tile = (100..110).map(|i| i as i16).collect();
        p.blend = vec![0, 4, 5, -2, 1, 16192, -16192, 3, 2, 0];
        p.extra = vec![0, 0, 1, 99, 2, -5, 4, 4, 4, 4];
        p.cliff = vec![0, 1, 2, 99, -1, 2, 3, -32768, 32767, 5];
        p.nblend = 5;
        p.ncliff = 3;
        p.records = vec![Rec::good(); 4];
        p.cliffs = vec![
            Cliff {
                tile_idx: 1,
                uvs: [0.0; 8],
                flip: 0,
                mutant: 0,
            };
            2
        ];
        map.blend(K_BLEND_TILE_VERSION_8, &p.bytes(K_BLEND_TILE_VERSION_8));
        // a second, smaller height map: the live count is 4, so only the
        // first four entries are clamped
        map.height_map(2, 2, &[1, 2, 3, 4]);

        let load = TerrainLoad::from_map(&map.bytes(), true).expect("loads");
        assert_eq!(load.data_size, 4);
        let bt = load.blend_tiles.as_ref().unwrap();
        assert_eq!(
            bt.tile_ndxes,
            (100..110).map(|i| i as i16).collect::<Vec<_>>()
        );
        assert_eq!(
            bt.blend_tile_ndxes,
            vec![0, 4, 0, 0, 1, 16192, -16192, 3, 2, 0]
        );
        assert_eq!(
            bt.extra_blend_tile_ndxes,
            vec![0, 0, 1, 0, 2, -5, 4, 4, 4, 4]
        );
        assert_eq!(
            bt.cliff_info_ndxes,
            vec![0, 1, 2, 0, -1, 2, 3, -32768, 32767, 5]
        );
    }

    #[test]
    fn the_walk_shapes_transcribe() {
        // no payloads at all: both halves absent, the patch is a no-op
        let mut map = MapFile::new();
        map.out.open_data_chunk(b"SidesList", 1);
        map.out.write_int(1);
        map.out.close_data_chunk();
        let load = TerrainLoad::from_map(&map.bytes(), true).expect("loads");
        assert!(load.height_map.is_none());
        assert!(load.blend_tiles.is_none());
        assert_eq!(load.data_size, 0);

        // unknown chunks around the payloads: skipped whole
        let mut map = MapFile::new();
        map.out.open_data_chunk(b"GlobalLighting", 2);
        map.out.write_int(0);
        map.out.close_data_chunk();
        map.height_map(2, 2, &[1, 2, 3, 4]);
        let p = Parts::new(4, 2, 2);
        map.blend(K_BLEND_TILE_VERSION_8, &p.bytes(K_BLEND_TILE_VERSION_8));
        let load = TerrainLoad::from_map(&map.bytes(), true).expect("loads");
        assert_eq!(load.blend_tiles.as_ref().unwrap().tile_ndxes, [0; 4]);

        // repeated chunks: the last wins (the second blend's version-3
        // shape drops the edge classes the first declared)
        let mut map = MapFile::new();
        map.height_map(3, 2, &[0; 6]);
        let mut first = Parts::new(6, 3, 2);
        first.nblend = 1;
        first.ncliff = 1;
        first.nedgetiles = 8;
        first.netex = 1;
        first.etex = vec![b"RE".to_vec()];
        map.blend(K_BLEND_TILE_VERSION_8, &first.bytes(K_BLEND_TILE_VERSION_8));
        let mut second = Parts::new(6, 3, 2);
        second.tile = vec![9, 8, 7, 6, 5, 4];
        second.blend = vec![0, 2, 0, 2, 0, 2];
        second.nbmp = 1;
        second.nblend = 2;
        let mut r = Rec::good();
        r.ndx = 2;
        r.longd = 1;
        second.records = vec![r];
        map.blend(
            K_BLEND_TILE_VERSION_3,
            &second.bytes(K_BLEND_TILE_VERSION_3),
        );
        let load = TerrainLoad::from_map(&map.bytes(), true).expect("loads");
        let bt = load.blend_tiles.as_ref().unwrap();
        // the version-3 reset clears the first chunk's edge counts
        assert_eq!(bt.num_edge_tiles, 0);
        assert_eq!(bt.num_edge_texture_classes, 0);
        assert_eq!(bt.num_cliff_info, 1); // the pre-version-5 default
        assert!(bt.texture_classes.is_empty());
        assert_eq!(bt.tile_ndxes, [9, 8, 7, 6, 5, 4]);
        assert_eq!(bt.blended_tiles[1].long_diagonal, 1);
        assert_eq!(bt.blended_tiles[1].custom_blend_edge_class, -1);

        // a blend chunk before any height map: len must be 0, the
        // arrays are empty, the patch skips the missing tail
        let mut map = MapFile::new();
        let mut first0 = Parts::new(0, 0, 0);
        first0.nbmp = 2;
        first0.nblend = 2;
        first0.ncliff = 2;
        first0.records = vec![Rec::good()];
        first0.cliffs = vec![Cliff {
            tile_idx: 4,
            uvs: [1.0; 8],
            flip: 1,
            mutant: 1,
        }];
        map.blend(
            K_BLEND_TILE_VERSION_8,
            &first0.bytes(K_BLEND_TILE_VERSION_8),
        );
        map.height_map(3, 2, &[0; 6]);
        let load = TerrainLoad::from_map(&map.bytes(), true).expect("loads");
        let bt = load.blend_tiles.as_ref().unwrap();
        assert!(bt.tile_ndxes.is_empty());
        assert_eq!((bt.width, bt.height, bt.data_size), (0, 0, 0));
        assert_eq!((load.width, load.height, load.data_size), (3, 2, 6));
        assert_eq!(load.height_map.as_ref().unwrap().width, 3);
    }

    #[test]
    fn the_direct_reader_matches_the_walk() {
        // the registered-parser core, driven the way a manual walk does
        let mut map = MapFile::new();
        map.height_map(3, 2, &[1, 2, 3, 4, 5, 6]);
        let mut p = Parts::new(6, 3, 2);
        p.tile = vec![1, 2, 3, 4, 5, 6];
        map.blend(K_BLEND_TILE_VERSION_8, &p.bytes(K_BLEND_TILE_VERSION_8));
        let bytes = map.bytes();

        let mut input = DataChunkInput::new(&bytes);
        input.open_data_chunk(); // HeightMapData
        let info = DataChunkInfo {
            label: HEIGHT_MAP_DATA_LABEL.to_vec(),
            parent_label: Vec::new(),
            version: input.get_chunk_version(),
            data_size: input.get_chunk_data_size() as i32,
        };
        let hm = parse_height_map_data(&mut input, &info).expect("height map");
        input.close_data_chunk();
        input.open_data_chunk(); // BlendTileData
        let info = DataChunkInfo {
            label: BLEND_TILE_DATA_LABEL.to_vec(),
            parent_label: Vec::new(),
            version: input.get_chunk_version(),
            data_size: input.get_chunk_data_size() as i32,
        };
        let bt = parse_blend_tile_data(&mut input, &info, &SharedHeightMap::of(&hm), true)
            .expect("blend");
        assert_eq!(bt.tile_ndxes, [1, 2, 3, 4, 5, 6]);

        // and from_map agrees with the manual walk's payload
        let load = TerrainLoad::from_map(&bytes, true).expect("loads");
        assert_eq!(load.blend_tiles.as_ref().unwrap().tile_ndxes, bt.tile_ndxes);
    }

    #[test]
    fn error_display_is_stable() {
        assert_eq!(
            BlendTileError::CorruptFileFormat.to_string(),
            "map is not a chunky data-chunk file"
        );
        assert_eq!(
            BlendTileError::DataSizeMismatch {
                len: 12,
                data_size: 16
            }
            .to_string(),
            "blend tile len 12 does not match the height map's 16 cells"
        );
        assert_eq!(
            BlendTileError::InvalidBlendFlag {
                index: 3,
                flag: 0x7ADA_0001u32 as i32
            }
            .to_string(),
            "blend record 3 sentinel 0x7ada0001 is not FLAG_VAL"
        );
        assert_eq!(
            BlendTileError::TooManyBlendedTiles(16193).to_string(),
            "unlikely num blended tiles 16193 (bound 16192)"
        );
        assert_eq!(
            BlendTileError::PatchWithoutBlendTiles { data_size: 6 }.to_string(),
            "height map of 6 cells carries no blend tile data (the patch loop's null arrays)"
        );
        // the wrapped height-map error sources through
        let e = BlendTileError::HeightMap(HeightMapError::CorruptFileFormat);
        assert_eq!(
            e.to_string(),
            "height map: map is not a chunky data-chunk file"
        );
        assert!(std::error::Error::source(&e).is_some());
    }
}
