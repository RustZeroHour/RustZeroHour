// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Ported from the Zero Hour source release (baseline 571a7c5):
//   GeneralsMD/Code/Libraries/Source/WWVegas/WW3D2/hlod.cpp
//     (HLodDefClass::Load_W3D, HLodDefClass::read_header,
//      HLodDefClass::read_proxy_array,
//      HLodDefClass::SubObjectArrayClass::Load_W3D, and the file's own
//      ProxyRecordClass / ProxyArrayClass the proxy path fills)
//   GeneralsMD/Code/Libraries/Source/WWVegas/WW3D2/w3d_file.h
//     (W3dHLodHeaderStruct, W3dHLodArrayHeaderStruct,
//      W3dHLodSubObjectStruct, the W3D_CHUNK_HLOD_* ids,
//      W3D_CURRENT_HLOD_VERSION, NO_MAX_SCREEN_SIZE)
//   Core/Libraries/Source/WWVegas/WWLib/Vector.h
//     (VectorClass(int)'s `new T[size]` default-construction — the
//      proxy array's records are born zeroed; Length() is the
//      asked-for count)
// See the module docs for the format layout and the documented
// differences from the C++ (discardable sub-reads with their zeroed
// stance, the strdup caps, the allocation-size discipline).

//! The **W3D HLOD assemblies** — the hierarchical level-of-detail model
//! format Zero Hour's units and buildings are assembled in (the plan's
//! Phase 2 W3D track, on top of the chunk container; unblocked by the
//! mesh, hierarchy-tree and animation slices before it).
//!
//! A `.w3d` HLOD asset is a [`W3D_CHUNK_HLOD`] container whose children
//! describe one assembly: a fixed 40-byte [`W3D_CHUNK_HLOD_HEADER`]
//! naming the object and the hierarchy tree it animates against, one
//! [`W3D_CHUNK_HLOD_LOD_ARRAY`] wrapper per level of detail holding a
//! sub-object array (model name + bone index pairs — the models are
//! resolved by name when the render object is instantiated), and two
//! optional trailing arrays: the aggregates (extra models attached to
//! bones, always rendered) and the proxies (application-defined name +
//! bone pairs the game places turrets and muzzle flashes on). This
//! module ports `HLodDefClass::Load_W3D` (hlod.cpp) — the data path the
//! original's asset manager drives through `HLodLoaderClass::Load_W3D`
//! — so the structures produced here hold exactly what the game's
//! definition holds after a load, quirks included (parity level P0,
//! pinned by golden vectors generated from the transcription in
//! `tools/differential/gen_hlod_golden.cpp`).
//!
//! # The load-path quirks
//!
//! - **the version field is never checked** — any `Version` loads;
//! - the two header names are **`strdup`'d, not bounded copies**: a
//!   name field without a NUL runs through the following header words
//!   (`Name` runs into `HierarchyName` — the deterministic in-struct
//!   run the goldens pin; the sub-object `strdup`'s field ends at its
//!   struct end, so its run caps there). Past a struct the original
//!   reads stack bytes; the port caps at the struct (the corpora keep
//!   the same cap, see the differences below);
//! - **every sub-array read is discardable**: the LOD, aggregate and
//!   proxy readers return a verdict the walk *ignores* — a failing
//!   array leaves its partial state inside a load that still succeeds.
//!   This is the deliberate contrast with the hierarchy loader (a
//!   failing `read_pivots` fails that load) and the same stance as the
//!   mesh loader's discarded sub-readers. The entries a failing read
//!   leaves unread hold uninitialised heap in the original; the port
//!   pins the **zeroed stance** (empty name, bone 0);
//! - an **unbalanced** sub-array failure (a chunk left open) in a
//!   **non-final** LOD derails the walk: the wrapper's close closes the
//!   stray chunk instead, the next loop iteration opens inside the
//!   unexhausted wrapper, and the `LOD_ARRAY` id check fails the load —
//!   the *same* failure in the final LOD, or inside the trailing
//!   aggregate/proxy arrays, merely exits the walk early and **shadows**
//!   every chunk after it (never read);
//! - the proxy `Init` uses **`strlcpy`** — bounded, 31 characters plus
//!   the NUL, so a 32-character proxy name is *truncated*
//!   deterministically — where the model path uses the unbounded
//!   `strdup`;
//! - repeated aggregate or proxy arrays are **last-wins** (the proxy
//!   reader releases the previous array before allocating the new one,
//!   so a failing *second* array still replaces the first);
//! - `LodCount` 0 is a **legal load** (an HLOD of no LODs — only the
//!   trailing arrays are read), and every bone index is the header's
//!   `uint32` converted through `int` (`0x8000_0000` wraps to
//!   `i32::MIN`, `0xFFFF_FFFF` to `-1`).
//!
//! # Differences from the C++
//!
//! Release-build undefined behaviour becomes stated contracts, pinned
//! identically on both sides of the goldens: the `strdup` runs cap at
//! their structs (past them the original reads stack bytes — a
//! heap-layout result), and the unread tail of a failing sub-array read
//! reads as zeroed (the original's `new char*[n]` / `new int[n]` are
//! never value-initialised). Where the C++ allocation would terminate
//! the process — a negative or huge `LodCount`/`ModelCount` converted
//! through `int` into `new[n]` — the port's allocation aborts the same
//! way; the corpora avoid those counts (the htree generator's
//! allocation discipline), so the goldens pin neither.
//!
//! The render-object side is out of scope for the format layer:
//! `HLodClass` construction (the `Create_Render_Obj` name resolution,
//! the `HTree` the `HierarchyTreeName` resolves through the asset
//! manager, the LOD cost/value machinery) lands with the model-viewer
//! milestone (M3), and the `Save` path (W3DView's exporter) with the
//! tools phase. The query surface ported today is the def-level half:
//! [`W3dHlod::name_str`], [`W3dHlod::hierarchy_tree_name_str`] and
//! [`W3dHlod::proxy_count`] (`Get_Proxy_Count`'s null check included).
//!
//! # Examples
//!
//! ```
//! use rzh_formats::chunk::ChunkSave;
//! use rzh_formats::w3d_hlod::{
//!     W3dHlod, W3D_CHUNK_HLOD_HEADER, W3D_CHUNK_HLOD_LOD_ARRAY,
//!     W3D_CHUNK_HLOD_SUB_OBJECT, W3D_CHUNK_HLOD_SUB_OBJECT_ARRAY_HEADER,
//!     W3D_CURRENT_HLOD_VERSION,
//! };
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // A one-LOD HLOD whose single model sits on bone 2 of its tree.
//! let mut save = ChunkSave::new();
//! save.begin_chunk(W3D_CHUNK_HLOD_HEADER)?;
//! let mut header = [0u8; 40];
//! header[..4].copy_from_slice(&W3D_CURRENT_HLOD_VERSION.to_le_bytes());
//! header[4..8].copy_from_slice(&1u32.to_le_bytes());
//! header[8..14].copy_from_slice(b"AVTank");
//! header[24..30].copy_from_slice(b"AVTank");
//! save.write(&header)?;
//! save.end_chunk()?;
//!
//! save.begin_chunk(W3D_CHUNK_HLOD_LOD_ARRAY)?;
//! save.begin_chunk(W3D_CHUNK_HLOD_SUB_OBJECT_ARRAY_HEADER)?;
//! save.write(&1u32.to_le_bytes())?; // model count
//! save.write(&100.0f32.to_le_bytes())?; // max screen size
//! save.end_chunk()?;
//! save.begin_chunk(W3D_CHUNK_HLOD_SUB_OBJECT)?;
//! let mut name = [0u8; 32];
//! name[..8].copy_from_slice(b"AVTankT\0");
//! save.write(&2u32.to_le_bytes())?; // bone index
//! save.write(&name)?;
//! save.end_chunk()?;
//! save.end_chunk()?;
//!
//! let hlod = W3dHlod::parse(&save.into_bytes()?)?;
//! assert_eq!(hlod.name_str(), b"AVTank");
//! assert_eq!(hlod.hierarchy_tree_name_str(), b"AVTank");
//! assert_eq!(hlod.lods.len(), 1);
//! assert_eq!(hlod.lods[0].model_names, vec![b"AVTankT".to_vec()]);
//! assert_eq!(hlod.lods[0].bone_indices, vec![2]);
//! assert_eq!(hlod.proxy_count(), 0); // no proxy array in the file
//! # Ok(())
//! # }
//! ```

use std::fmt;

use crate::chunk::ChunkLoad;

// ---------------------------------------------------------------------------
// chunk ids and format constants (w3d_file.h)
// ---------------------------------------------------------------------------

/// The HLOD-assembly container a `.w3d` file wraps an HLOD asset in.
pub const W3D_CHUNK_HLOD: u32 = 0x0000_0700;
/// The 40-byte `W3dHLodHeaderStruct`.
pub const W3D_CHUNK_HLOD_HEADER: u32 = 0x0000_0701;
/// The wrapper around one level of detail's sub-object array.
pub const W3D_CHUNK_HLOD_LOD_ARRAY: u32 = 0x0000_0702;
/// The 8-byte `W3dHLodArrayHeaderStruct` inside a LOD/aggregate array.
pub const W3D_CHUNK_HLOD_SUB_OBJECT_ARRAY_HEADER: u32 = 0x0000_0703;
/// The 36-byte `W3dHLodSubObjectStruct` — one named model or proxy.
pub const W3D_CHUNK_HLOD_SUB_OBJECT: u32 = 0x0000_0704;
/// The aggregates — extra models attached to bones, always rendered.
pub const W3D_CHUNK_HLOD_AGGREGATE_ARRAY: u32 = 0x0000_0705;
/// The proxies — application-defined name + bone pairs.
pub const W3D_CHUNK_HLOD_PROXY_ARRAY: u32 = 0x0000_0706;

/// The length of every fixed name field (`W3D_NAME_LEN`).
pub const W3D_NAME_LEN: usize = 16;
/// The length of a sub-object's name field (`W3D_NAME_LEN * 2`).
pub const SUB_OBJECT_NAME_LEN: usize = 2 * W3D_NAME_LEN;
/// The size of the HLOD header struct.
pub const HLOD_HEADER_SIZE: u32 = 40;
/// The size of the sub-object array header struct.
pub const HLOD_ARRAY_HEADER_SIZE: u32 = 8;
/// The size of one sub-object struct.
pub const HLOD_SUB_OBJECT_SIZE: u32 = 36;
/// The version the current exporter writes (`W3D_CURRENT_HLOD_VERSION`)
/// — stored, never consulted by the loader.
pub const W3D_CURRENT_HLOD_VERSION: u32 = w3d_make_version(1, 0);
/// The `MaxScreenSize` a constructed sub-object array holds
/// (`NO_MAX_SCREEN_SIZE` = `WWMATH_FLOAT_MAX`).
pub const NO_MAX_SCREEN_SIZE: f32 = f32::MAX;

/// `W3D_MAKE_VERSION` — the packed `(major, minor)` file version.
pub const fn w3d_make_version(major: u32, minor: u32) -> u32 {
    (major << 16) | minor
}

// ---------------------------------------------------------------------------
// errors
// ---------------------------------------------------------------------------

/// Errors the HLOD load path can produce.
///
/// The original's `Load_W3D` has exactly one failure code —
/// `WW3D_ERROR_LOAD_FAILED` — and reaches it in exactly three places:
/// a bad first chunk (not open, not the HLOD header, or a short 40-byte
/// read) and the LOD loop's two checks (an `Open_Chunk` that fails, or a
/// chunk whose id is not `W3D_CHUNK_HLOD_LOD_ARRAY`). Every other
/// failure — including every malformed sub-array — is discarded by the
/// walk and leaves a *successful* load carrying partial state (see the
/// module docs); the port states the same contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum HlodError {
    /// `WW3D_ERROR_LOAD_FAILED` — the load failed outright (bad first
    /// chunk, short header, or the LOD loop's open/id check).
    LoadFailed,
    /// The chunk container itself errored (depth cap, no open chunk).
    Chunk(crate::chunk::ChunkError),
}

impl fmt::Display for HlodError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LoadFailed => write!(f, "W3D HLOD load failed"),
            Self::Chunk(e) => write!(f, "W3D HLOD chunk error: {e}"),
        }
    }
}

impl std::error::Error for HlodError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Chunk(e) => Some(e),
            _ => None,
        }
    }
}

impl From<crate::chunk::ChunkError> for HlodError {
    fn from(e: crate::chunk::ChunkError) -> Self {
        Self::Chunk(e)
    }
}

// ---------------------------------------------------------------------------
// the fixed structs
// ---------------------------------------------------------------------------

/// The 40-byte `W3dHLodHeaderStruct`, decoded.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct W3dHlodHeader {
    /// File-format version (`W3D_MAKE_VERSION`) — stored, never
    /// checked by the loader.
    pub version: u32,
    /// The declared level-of-detail count — the loader converts it
    /// through `int` and allocates exactly that many sub-object arrays;
    /// 0 is legal (an HLOD of no LODs).
    pub lod_count: u32,
    /// The object's name (a C string in a 16-byte field).
    pub name: [u8; W3D_NAME_LEN],
    /// The hierarchy tree to animate against (a C string in a 16-byte
    /// field; `\0` when the assembly has none — the static-building
    /// shape).
    pub hierarchy_name: [u8; W3D_NAME_LEN],
}

impl W3dHlodHeader {
    /// Decodes the struct from its 40 little-endian bytes.
    pub fn from_bytes(bytes: &[u8; HLOD_HEADER_SIZE as usize]) -> Self {
        let mut name = [0u8; W3D_NAME_LEN];
        name.copy_from_slice(&bytes[8..24]);
        let mut hierarchy_name = [0u8; W3D_NAME_LEN];
        hierarchy_name.copy_from_slice(&bytes[24..40]);
        Self {
            version: u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            lod_count: u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            name,
            hierarchy_name,
        }
    }

    /// The C-string view of the name field (the *bounded* view — the
    /// def's `strdup`'d name, which runs through the following header
    /// words when the field has no NUL, is [`W3dHlod::name`]).
    pub fn name_str(&self) -> &[u8] {
        cstr(&self.name)
    }

    /// The C-string view of the hierarchy-name field.
    pub fn hierarchy_name_str(&self) -> &[u8] {
        cstr(&self.hierarchy_name)
    }
}

/// The 8-byte `W3dHLodArrayHeaderStruct`, decoded — the header inside
/// every LOD, aggregate or proxy array.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct W3dHlodArrayHeader {
    /// The declared entry count — converted through `int` by every
    /// reader (the model arrays and the proxy array are allocated to
    /// exactly this size).
    pub model_count: u32,
    /// The maximum screen size for this LOD (`NO_MAX_SCREEN_SIZE` when
    /// unbounded) — stored verbatim, no arithmetic.
    pub max_screen_size: f32,
}

impl W3dHlodArrayHeader {
    /// Decodes the struct from its 8 little-endian bytes.
    pub fn from_bytes(bytes: &[u8; HLOD_ARRAY_HEADER_SIZE as usize]) -> Self {
        Self {
            model_count: u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            max_screen_size: f32::from_bits(u32::from_le_bytes(bytes[4..8].try_into().unwrap())),
        }
    }
}

/// The 36-byte `W3dHLodSubObjectStruct`, decoded — one named model (in
/// the LOD and aggregate arrays) or one named proxy (in the proxy
/// array).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct W3dHlodSubObjectStruct {
    /// The bone the sub-object sits on — a `uint32` converted through
    /// `int` by every reader (`0x8000_0000` wraps to `i32::MIN`).
    pub bone_index: u32,
    /// The sub-object's name (a C string in a 32-byte field — the
    /// field ends at the struct end, so the model path's `strdup` caps
    /// there, while the proxy path's `strlcpy` truncates at 31).
    pub name: [u8; SUB_OBJECT_NAME_LEN],
}

impl W3dHlodSubObjectStruct {
    /// Decodes the struct from its 36 little-endian bytes.
    pub fn from_bytes(bytes: &[u8; HLOD_SUB_OBJECT_SIZE as usize]) -> Self {
        let mut name = [0u8; SUB_OBJECT_NAME_LEN];
        name.copy_from_slice(&bytes[4..36]);
        Self {
            bone_index: u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            name,
        }
    }

    /// The C-string view of the name field.
    pub fn name_str(&self) -> &[u8] {
        cstr(&self.name)
    }
}

/// The bytes of a fixed character field up to the first NUL.
fn cstr(field: &[u8]) -> &[u8] {
    let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    &field[..end]
}

/// The C string starting at `start` inside the 40 header bytes, capped
/// at the struct end — the source the loader's two `strdup` calls read
/// (they run through the following header words when a name field has
/// no NUL — `Name` runs into `HierarchyName`; past the struct itself
/// the read is undefined and the port caps it, see the module docs).
fn header_cstr(bytes: &[u8; HLOD_HEADER_SIZE as usize], start: usize) -> &[u8] {
    let end = bytes[start..]
        .iter()
        .position(|&b| b == 0)
        .map_or(HLOD_HEADER_SIZE as usize, |p| start + p);
    &bytes[start..end]
}

/// `strlcpy(dst, src, siz)` — the BSD copy the proxy `Init` performs:
/// at most `siz - 1` bytes then a NUL, so a full 32-byte source field
/// truncates to 31 characters (deterministic, pinned by the goldens).
/// The original's return-value traversal reads the source to its NUL —
/// past the field when unterminated — but `Init` ignores the return, so
/// the copy is the part both sides transcribe.
fn strlcpy_field(dst: &mut [u8; SUB_OBJECT_NAME_LEN], src: &[u8; SUB_OBJECT_NAME_LEN]) {
    let mut d = 0usize;
    for &b in src.iter().take(SUB_OBJECT_NAME_LEN - 1) {
        dst[d] = b;
        d += 1;
        if b == 0 {
            // the NUL is written; the canonical loop stops here
            return;
        }
    }
    // not enough room in dst: add the NUL (the canonical tail)
    dst[d] = 0;
}

// ---------------------------------------------------------------------------
// the loaded sub-object array (HLodDefClass::SubObjectArrayClass)
// ---------------------------------------------------------------------------

/// One loaded sub-object array — the `SubObjectArrayClass` state
/// `Load_W3D` leaves behind: the LOD arrays and the aggregates share
/// this shape (the aggregates' `MaxScreenSize` is read but unused by
/// the original, exactly as by the port).
///
/// The parallel `model_names`/`bone_indices` vectors mirror the
/// original's parallel `char**`/`int*` arrays, allocated to the
/// header's count and filled in lockstep; the entries a failing read
/// leaves unread hold the zeroed stance (see the module docs) where the
/// original holds uninitialised heap.
#[derive(Debug, Clone, PartialEq)]
pub struct W3dSubObjectArray {
    /// The array header's `MaxScreenSize` (`NO_MAX_SCREEN_SIZE` for a
    /// never-loaded array — the constructed state).
    pub max_screen_size: f32,
    /// The `ModelCount` member — the header's `uint32` converted
    /// through `int`, set before the entries are read.
    pub model_count: i32,
    /// The `strdup`'d model names, one per entry (empty for the
    /// zeroed-stance tail of a failing read).
    pub model_names: Vec<Vec<u8>>,
    /// The bone indices, one per entry (`0` for the zeroed-stance
    /// tail).
    pub bone_indices: Vec<i32>,
}

impl Default for W3dSubObjectArray {
    /// The `SubObjectArrayClass` constructor state:
    /// `MaxScreenSize(NO_MAX_SCREEN_SIZE)`, no models.
    fn default() -> Self {
        Self {
            max_screen_size: NO_MAX_SCREEN_SIZE,
            model_count: 0,
            model_names: Vec::new(),
            bone_indices: Vec::new(),
        }
    }
}

impl W3dSubObjectArray {
    /// `SubObjectArrayClass::Load_W3D` — `Ok(true)` on success,
    /// `Ok(false)` on the failure the walk discards, `Err` for a
    /// container error.
    ///
    /// A failing call still leaves the partial state: the count and
    /// screen size are set from the array header (when it was read),
    /// and the entries read before the failure keep their values.
    fn load_w3d(&mut self, cload: &mut ChunkLoad<'_>) -> Result<bool, crate::chunk::ChunkError> {
        // Open the first chunk, it should be a Lod Array Header
        if !cload.open_chunk()? {
            return Ok(false);
        }
        if cload.cur_chunk_id() != W3D_CHUNK_HLOD_SUB_OBJECT_ARRAY_HEADER {
            return Ok(false);
        }

        let mut header_bytes = [0u8; HLOD_ARRAY_HEADER_SIZE as usize];
        if cload.read(&mut header_bytes)? != HLOD_ARRAY_HEADER_SIZE {
            return Ok(false);
        }
        if cload.close_chunk().is_err() {
            return Ok(false);
        }

        let header = W3dHlodArrayHeader::from_bytes(&header_bytes);
        self.model_count = header.model_count as i32;
        self.max_screen_size = header.max_screen_size;

        // ModelName = new char*[ModelCount]; BoneIndex = new int[ModelCount];
        // (the zeroed stance — see the module docs)
        self.model_names = vec![Vec::new(); self.model_count as usize];
        self.bone_indices = vec![0; self.model_count as usize];

        // Read each sub object definition
        for imodel in 0..self.model_count {
            if !cload.open_chunk()? {
                return Ok(false);
            }
            if cload.cur_chunk_id() != W3D_CHUNK_HLOD_SUB_OBJECT {
                return Ok(false);
            }

            let mut sub_bytes = [0u8; HLOD_SUB_OBJECT_SIZE as usize];
            if cload.read(&mut sub_bytes)? != HLOD_SUB_OBJECT_SIZE {
                return Ok(false);
            }
            if cload.close_chunk().is_err() {
                return Ok(false);
            }

            let sub = W3dHlodSubObjectStruct::from_bytes(&sub_bytes);
            // ModelName[imodel] = strdup(subobjdef.Name) — the Name
            // field ends at the struct end, so cstr is the capped scan
            self.model_names[imodel as usize] = sub.name_str().to_vec();
            self.bone_indices[imodel as usize] = sub.bone_index as i32;
        }
        Ok(true)
    }
}

// ---------------------------------------------------------------------------
// the loaded proxy (hlod.cpp's ProxyRecordClass)
// ---------------------------------------------------------------------------

/// One loaded proxy record — the `ProxyRecordClass` state `Init`
/// leaves behind (the array is born default-constructed; the records a
/// failing read leaves unread keep this state).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct W3dProxyRecord {
    /// The proxy's bone — the sub-object's `uint32` converted through
    /// `int`.
    pub bone_index: i32,
    /// The proxy's name — the `strlcpy`'d 32-byte field (always
    /// NUL-terminated; a 32-character source name truncates to 31).
    pub name: [u8; SUB_OBJECT_NAME_LEN],
}

impl Default for W3dProxyRecord {
    /// The `ProxyRecordClass` constructor state: bone 0, empty name.
    fn default() -> Self {
        Self {
            bone_index: 0,
            name: [0; SUB_OBJECT_NAME_LEN],
        }
    }
}

impl W3dProxyRecord {
    /// `ProxyRecordClass::Init` — the bone conversion and the bounded
    /// `strlcpy` (31 characters plus the NUL).
    fn init(&mut self, sub: &W3dHlodSubObjectStruct) {
        self.bone_index = sub.bone_index as i32;
        strlcpy_field(&mut self.name, &sub.name);
    }

    /// The C-string view of the name field.
    pub fn name_str(&self) -> &[u8] {
        cstr(&self.name)
    }
}

// ---------------------------------------------------------------------------
// the loaded definition (HLodDefClass)
// ---------------------------------------------------------------------------

/// A loaded W3D HLOD definition — the state `HLodDefClass::Load_W3D`
/// leaves behind, minus the render-object side (see the module docs).
#[derive(Debug, Clone, PartialEq)]
pub struct W3dHlod {
    /// The decoded HLOD header (the pristine 40 bytes; the def's own
    /// names are the `strdup`'d copies below, run-through included).
    pub header: W3dHlodHeader,
    /// The `strdup`'d object name — runs through the following header
    /// words when the field has no NUL (`Name` into `HierarchyName`),
    /// capped at the struct.
    pub name: Vec<u8>,
    /// The `strdup`'d hierarchy-tree name — the key the render object
    /// resolves its `HTree` through (empty when the assembly has none).
    pub hierarchy_tree_name: Vec<u8>,
    /// The `LodCount` member — the header's `uint32` converted through
    /// `int`.
    pub lod_count: i32,
    /// The LOD arrays — exactly `lod_count` of them, allocated by
    /// `read_header` (a never-loaded one keeps its constructed state).
    pub lods: Vec<W3dSubObjectArray>,
    /// The aggregates array (the always-rendered extras).
    pub aggregates: W3dSubObjectArray,
    /// The proxy array — `None` until a `W3D_CHUNK_HLOD_PROXY_ARRAY`
    /// chunk allocates it (last-wins across repeated chunks; a failing
    /// *second* array still replaces the first, partially filled).
    pub proxies: Option<Vec<W3dProxyRecord>>,
}

impl W3dHlod {
    /// Parses the payload of a [`W3D_CHUNK_HLOD`] container — the bytes
    /// between the container's header and its end. The caller (the
    /// future W3D asset loader) opens the container first, exactly like
    /// the original's asset manager does before calling
    /// `HLodLoaderClass::Load_W3D` through `Load_Prototype`.
    ///
    /// The only failures are the first chunk (not the HLOD header, or a
    /// short 40-byte read) and the LOD loop's open/id check — every
    /// malformed *sub-array* is discarded by the walk and leaves its
    /// partial state inside a successful load (see the module docs).
    pub fn parse(data: &[u8]) -> Result<Self, HlodError> {
        let mut cload = ChunkLoad::new(data);

        // ---- read_header ----

        // Open the first chunk, it should be the LOD header
        if !cload.open_chunk()? {
            return Err(HlodError::LoadFailed);
        }
        if cload.cur_chunk_id() != W3D_CHUNK_HLOD_HEADER {
            // "ERROR: Expected HLOD Header!"
            return Err(HlodError::LoadFailed);
        }
        let mut header_bytes = [0u8; HLOD_HEADER_SIZE as usize];
        if cload.read(&mut header_bytes)? != HLOD_HEADER_SIZE {
            return Err(HlodError::LoadFailed);
        }
        cload.close_chunk()?;

        let header = W3dHlodHeader::from_bytes(&header_bytes);
        // strdup(header.Name) / strdup(header.HierarchyName) — the
        // scans run through the following header words, capped at the
        // struct (Name's offset is 8, HierarchyName's is 24)
        let name = header_cstr(&header_bytes, 8).to_vec();
        let hierarchy_tree_name = header_cstr(&header_bytes, 24).to_vec();
        let lod_count = header.lod_count as i32;
        // Lod = new SubObjectArrayClass[LodCount] — the constructed
        // state per entry
        let mut lods = vec![W3dSubObjectArray::default(); lod_count as usize];

        // ---- the LOD loop ----

        // Loop through all the LODs and read the info from its chunk
        for ilod in 0..lod_count {
            // Open the next chunk, it should be a LOD struct
            if !cload.open_chunk()? {
                return Err(HlodError::LoadFailed);
            }
            if cload.cur_chunk_id() != W3D_CHUNK_HLOD_LOD_ARRAY {
                // "ERROR: Expected LOD struct!"
                return Err(HlodError::LoadFailed);
            }

            // Lod[iLOD].Load_W3D(cload) — the verdict is discarded
            let _ = lods[ilod as usize].load_w3d(&mut cload)?;

            // Close-out the chunk
            cload.close_chunk()?;
        }

        // ---- the trailing arrays ----

        let mut aggregates = W3dSubObjectArray::default();
        let mut proxies: Option<Vec<W3dProxyRecord>> = None;
        // Parse the rest of the chunks
        while cload.open_chunk()? {
            match cload.cur_chunk_id() {
                W3D_CHUNK_HLOD_AGGREGATE_ARRAY => {
                    // Aggregates.Load_W3D(cload) — the verdict is
                    // discarded (last-wins across repeated chunks)
                    let _ = aggregates.load_w3d(&mut cload)?;
                }
                W3D_CHUNK_HLOD_PROXY_ARRAY => {
                    // read_proxy_array(cload) — the verdict is discarded
                    let _ = read_proxy_array(&mut proxies, &mut cload)?;
                }
                _ => {}
            }
            cload.close_chunk()?;
        }

        Ok(Self {
            header,
            name,
            hierarchy_tree_name,
            lod_count,
            lods,
            aggregates,
            proxies,
        })
    }

    /// The assembly's name (`HLodDefClass::Get_Name`) — the `strdup`'d
    /// header name, run-through quirk included.
    pub fn name_str(&self) -> &[u8] {
        &self.name
    }

    /// The hierarchy-tree name the render object resolves its `HTree`
    /// through — the `strdup`'d field (empty when the assembly has
    /// none).
    pub fn hierarchy_tree_name_str(&self) -> &[u8] {
        &self.hierarchy_tree_name
    }

    /// The number of proxies (`HLodClass::Get_Proxy_Count`'s def-side
    /// half): the array's length, 0 when the def holds no array.
    pub fn proxy_count(&self) -> usize {
        self.proxies.as_ref().map_or(0, Vec::len)
    }
}

// ---------------------------------------------------------------------------
// read_proxy_array
// ---------------------------------------------------------------------------

/// `HLodDefClass::read_proxy_array` — `Ok(true)` on success, `Ok(false)`
/// on the failure the walk discards, `Err` for a container error. The
/// array is bound to the def *before* the record loop (the original's
/// `NEW_REF`), so a failing read leaves the partially-filled array in
/// place — its unread records keep their default-constructed state.
fn read_proxy_array(
    proxies: &mut Option<Vec<W3dProxyRecord>>,
    cload: &mut ChunkLoad<'_>,
) -> Result<bool, crate::chunk::ChunkError> {
    // REF_PTR_RELEASE(ProxyArray)
    *proxies = None;

    // Open the first chunk, it should be a Lod Array Header
    if !cload.open_chunk()? {
        return Ok(false);
    }
    if cload.cur_chunk_id() != W3D_CHUNK_HLOD_SUB_OBJECT_ARRAY_HEADER {
        return Ok(false);
    }

    let mut header_bytes = [0u8; HLOD_ARRAY_HEADER_SIZE as usize];
    if cload.read(&mut header_bytes)? != HLOD_ARRAY_HEADER_SIZE {
        return Ok(false);
    }
    if cload.close_chunk().is_err() {
        return Ok(false);
    }

    let header = W3dHlodArrayHeader::from_bytes(&header_bytes);
    // ProxyArray = NEW_REF(ProxyArrayClass, (header.ModelCount)) — the
    // records are default-constructed by the allocation
    // (VectorClass's `new T[size]`) and the member is bound before the
    // loop, so a failing read leaves the partial array in place.
    let count = header.model_count as i32;
    *proxies = Some(vec![W3dProxyRecord::default(); count as usize]);
    let records = proxies.as_mut().expect("bound above");

    // Read each sub object definition
    for imodel in 0..count {
        if !cload.open_chunk()? {
            return Ok(false);
        }
        if cload.cur_chunk_id() != W3D_CHUNK_HLOD_SUB_OBJECT {
            return Ok(false);
        }

        let mut sub_bytes = [0u8; HLOD_SUB_OBJECT_SIZE as usize];
        if cload.read(&mut sub_bytes)? != HLOD_SUB_OBJECT_SIZE {
            return Ok(false);
        }
        if cload.close_chunk().is_err() {
            return Ok(false);
        }

        let sub = W3dHlodSubObjectStruct::from_bytes(&sub_bytes);
        records[imodel as usize].init(&sub);
    }
    Ok(true)
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::ChunkSave;

    /// Builds a 40-byte header payload.
    fn header_bytes(version: u32, lod_count: u32, name: &[u8], hname: &[u8]) -> [u8; 40] {
        let mut out = [0u8; 40];
        out[..4].copy_from_slice(&version.to_le_bytes());
        out[4..8].copy_from_slice(&lod_count.to_le_bytes());
        out[8..8 + name.len()].copy_from_slice(name);
        out[24..24 + hname.len()].copy_from_slice(hname);
        out
    }

    /// Writes a sub-object array header chunk.
    fn array_header(save: &mut ChunkSave, count: u32, screen: f32) {
        save.begin_chunk(W3D_CHUNK_HLOD_SUB_OBJECT_ARRAY_HEADER)
            .unwrap();
        save.write(&count.to_le_bytes()).unwrap();
        save.write(&screen.to_le_bytes()).unwrap();
        save.end_chunk().unwrap();
    }

    /// Writes a 36-byte sub-object chunk.
    fn sub_object(save: &mut ChunkSave, bone: u32, name: &[u8]) {
        assert!(name.len() <= SUB_OBJECT_NAME_LEN);
        let mut field = [0u8; SUB_OBJECT_NAME_LEN];
        field[..name.len()].copy_from_slice(name);
        save.begin_chunk(W3D_CHUNK_HLOD_SUB_OBJECT).unwrap();
        save.write(&bone.to_le_bytes()).unwrap();
        save.write(&field).unwrap();
        save.end_chunk().unwrap();
    }

    /// A two-LOD HLOD payload: one model on LOD 0, none on LOD 1.
    fn sample_hlod() -> Vec<u8> {
        let mut save = ChunkSave::new();
        save.begin_chunk(W3D_CHUNK_HLOD_HEADER).unwrap();
        save.write(&header_bytes(
            W3D_CURRENT_HLOD_VERSION,
            2,
            b"AVTank",
            b"AVTank",
        ))
        .unwrap();
        save.end_chunk().unwrap();
        save.begin_chunk(W3D_CHUNK_HLOD_LOD_ARRAY).unwrap();
        array_header(&mut save, 1, 100.0);
        sub_object(&mut save, 2, b"AVTankT");
        save.end_chunk().unwrap();
        save.begin_chunk(W3D_CHUNK_HLOD_LOD_ARRAY).unwrap();
        array_header(&mut save, 0, NO_MAX_SCREEN_SIZE);
        save.end_chunk().unwrap();
        save.into_bytes().unwrap()
    }

    #[test]
    fn decodes_the_fixed_structs() {
        let hdr = W3dHlodHeader::from_bytes(&header_bytes(0x1234_5678, 3, b"Obj", b"Tree"));
        assert_eq!(hdr.version, 0x1234_5678);
        assert_eq!(hdr.lod_count, 3);
        assert_eq!(hdr.name_str(), b"Obj");
        assert_eq!(hdr.hierarchy_name_str(), b"Tree");

        let arr = W3dHlodArrayHeader::from_bytes(&{
            let mut b = [0u8; 8];
            b[..4].copy_from_slice(&7u32.to_le_bytes());
            b[4..].copy_from_slice(&(-0.5f32).to_bits().to_le_bytes());
            b
        });
        assert_eq!(arr.model_count, 7);
        assert_eq!(arr.max_screen_size.to_bits(), (-0.5f32).to_bits());

        let sub = W3dHlodSubObjectStruct::from_bytes(&{
            let mut b = [0u8; 36];
            b[..4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
            b[4..9].copy_from_slice(b"bone0");
            b
        });
        assert_eq!(sub.bone_index, 0xFFFF_FFFF);
        assert_eq!(sub.name_str(), b"bone0");
    }

    #[test]
    fn parses_the_happy_path() {
        let hlod = W3dHlod::parse(&sample_hlod()).unwrap();
        assert_eq!(hlod.header.version, W3D_CURRENT_HLOD_VERSION);
        assert_eq!(hlod.name_str(), b"AVTank");
        assert_eq!(hlod.hierarchy_tree_name_str(), b"AVTank");
        assert_eq!(hlod.lod_count, 2);
        assert_eq!(hlod.lods.len(), 2);
        assert_eq!(hlod.lods[0].max_screen_size.to_bits(), 100.0f32.to_bits());
        assert_eq!(hlod.lods[0].model_count, 1);
        assert_eq!(hlod.lods[0].model_names, vec![b"AVTankT".to_vec()]);
        assert_eq!(hlod.lods[0].bone_indices, vec![2]);
        assert_eq!(hlod.lods[1].model_count, 0);
        // no aggregate or proxy chunks in the file
        assert_eq!(hlod.aggregates, W3dSubObjectArray::default());
        assert_eq!(hlod.proxies, None);
        assert_eq!(hlod.proxy_count(), 0);
    }

    #[test]
    fn the_version_is_never_checked() {
        for version in [0u32, 0xFFFF_FFFF, w3d_make_version(99, 99)] {
            let mut save = ChunkSave::new();
            save.begin_chunk(W3D_CHUNK_HLOD_HEADER).unwrap();
            save.write(&header_bytes(version, 1, b"V", b"T")).unwrap();
            save.end_chunk().unwrap();
            save.begin_chunk(W3D_CHUNK_HLOD_LOD_ARRAY).unwrap();
            array_header(&mut save, 0, 1.0);
            save.end_chunk().unwrap();
            let hlod = W3dHlod::parse(&save.into_bytes().unwrap()).unwrap();
            assert_eq!(hlod.header.version, version);
        }
    }

    #[test]
    fn the_header_strdup_runs_through_the_struct() {
        // An unterminated 16-byte Name runs into HierarchyName; the
        // combined scan stops at the NUL inside the second field.
        let mut save = ChunkSave::new();
        save.begin_chunk(W3D_CHUNK_HLOD_HEADER).unwrap();
        let mut raw = [0u8; 40];
        raw[8..24].copy_from_slice(b"AVeryLongHlodNam");
        raw[24..29].copy_from_slice(b"tree\0");
        save.write(&raw).unwrap();
        save.end_chunk().unwrap();
        save.begin_chunk(W3D_CHUNK_HLOD_LOD_ARRAY).unwrap();
        array_header(&mut save, 0, 1.0);
        save.end_chunk().unwrap();
        let hlod = W3dHlod::parse(&save.into_bytes().unwrap()).unwrap();
        assert_eq!(hlod.name_str(), b"AVeryLongHlodNamtree");
        assert_eq!(hlod.hierarchy_tree_name_str(), b"tree");
        // the bounded views of the pristine header differ
        assert_eq!(hlod.header.name_str(), b"AVeryLongHlodNam");
    }

    #[test]
    fn a_failing_lod_array_is_discarded() {
        // The second LOD's array opens with a sub-object chunk instead
        // of the array header: the read is discarded and the load
        // succeeds with that LOD's constructed state.
        let mut save = ChunkSave::new();
        save.begin_chunk(W3D_CHUNK_HLOD_HEADER).unwrap();
        save.write(&header_bytes(W3D_CURRENT_HLOD_VERSION, 2, b"D", b"T"))
            .unwrap();
        save.end_chunk().unwrap();
        save.begin_chunk(W3D_CHUNK_HLOD_LOD_ARRAY).unwrap();
        array_header(&mut save, 1, 10.0);
        sub_object(&mut save, 0, b"M0");
        save.end_chunk().unwrap();
        save.begin_chunk(W3D_CHUNK_HLOD_LOD_ARRAY).unwrap();
        sub_object(&mut save, 0, b"Stray");
        save.end_chunk().unwrap();
        let hlod = W3dHlod::parse(&save.into_bytes().unwrap()).unwrap();
        assert_eq!(hlod.lods[1], W3dSubObjectArray::default());
        assert_eq!(
            hlod.lods[1].max_screen_size.to_bits(),
            NO_MAX_SCREEN_SIZE.to_bits()
        );
    }

    #[test]
    fn an_overdeclared_count_leaves_the_zeroed_stance() {
        // Two models declared, one present: the second entry reads as
        // the zeroed stance (empty name, bone 0).
        let mut save = ChunkSave::new();
        save.begin_chunk(W3D_CHUNK_HLOD_HEADER).unwrap();
        save.write(&header_bytes(W3D_CURRENT_HLOD_VERSION, 1, b"H", b"T"))
            .unwrap();
        save.end_chunk().unwrap();
        save.begin_chunk(W3D_CHUNK_HLOD_LOD_ARRAY).unwrap();
        array_header(&mut save, 2, 20.0);
        sub_object(&mut save, 3, b"First");
        save.end_chunk().unwrap();
        let hlod = W3dHlod::parse(&save.into_bytes().unwrap()).unwrap();
        assert_eq!(hlod.lods[0].model_count, 2);
        assert_eq!(
            hlod.lods[0].model_names,
            vec![b"First".to_vec(), Vec::new()]
        );
        assert_eq!(hlod.lods[0].bone_indices, vec![3, 0]);
    }

    #[test]
    fn proxies_truncate_and_default() {
        // One 32-character proxy name (strlcpy truncates to 31) and an
        // over-declared count leaving default records behind.
        let long_name: Vec<u8> = (0..32u8).map(|i| b'A' + i % 26).collect();
        assert_eq!(long_name.len(), 32);
        assert!(!long_name.contains(&0));
        let mut save = ChunkSave::new();
        save.begin_chunk(W3D_CHUNK_HLOD_HEADER).unwrap();
        save.write(&header_bytes(W3D_CURRENT_HLOD_VERSION, 1, b"P", b"T"))
            .unwrap();
        save.end_chunk().unwrap();
        save.begin_chunk(W3D_CHUNK_HLOD_LOD_ARRAY).unwrap();
        array_header(&mut save, 0, 1.0);
        save.end_chunk().unwrap();
        save.begin_chunk(W3D_CHUNK_HLOD_PROXY_ARRAY).unwrap();
        array_header(&mut save, 3, 0.0);
        sub_object(&mut save, 0xFFFF_FFFF, &long_name);
        save.end_chunk().unwrap();
        let hlod = W3dHlod::parse(&save.into_bytes().unwrap()).unwrap();
        let proxies = hlod.proxies.as_ref().unwrap();
        assert_eq!(hlod.proxy_count(), 3);
        assert_eq!(proxies[0].name_str(), &long_name[..31]);
        assert_eq!(proxies[0].bone_index, -1);
        assert_eq!(proxies[1], W3dProxyRecord::default());
        assert_eq!(proxies[2], W3dProxyRecord::default());
    }

    #[test]
    fn repeated_arrays_are_last_wins() {
        let mut save = ChunkSave::new();
        save.begin_chunk(W3D_CHUNK_HLOD_HEADER).unwrap();
        save.write(&header_bytes(W3D_CURRENT_HLOD_VERSION, 1, b"W", b"T"))
            .unwrap();
        save.end_chunk().unwrap();
        save.begin_chunk(W3D_CHUNK_HLOD_LOD_ARRAY).unwrap();
        array_header(&mut save, 0, 1.0);
        save.end_chunk().unwrap();
        for name in [b"FIRST".as_slice(), b"SECOND".as_slice()] {
            save.begin_chunk(W3D_CHUNK_HLOD_AGGREGATE_ARRAY).unwrap();
            array_header(&mut save, 1, 0.0);
            sub_object(&mut save, 1, name);
            save.end_chunk().unwrap();
        }
        // a failing second proxy array still replaces the first
        save.begin_chunk(W3D_CHUNK_HLOD_PROXY_ARRAY).unwrap();
        array_header(&mut save, 1, 0.0);
        sub_object(&mut save, 1, b"OLD");
        save.end_chunk().unwrap();
        save.begin_chunk(W3D_CHUNK_HLOD_PROXY_ARRAY).unwrap();
        array_header(&mut save, 2, 0.0);
        save.end_chunk().unwrap();
        let hlod = W3dHlod::parse(&save.into_bytes().unwrap()).unwrap();
        assert_eq!(hlod.aggregates.model_names, vec![b"SECOND".to_vec()]);
        assert_eq!(hlod.proxy_count(), 2);
        assert_eq!(hlod.proxies.as_ref().unwrap()[0], W3dProxyRecord::default());
    }

    #[test]
    fn a_zero_lod_count_is_a_legal_load() {
        let mut save = ChunkSave::new();
        save.begin_chunk(W3D_CHUNK_HLOD_HEADER).unwrap();
        save.write(&header_bytes(W3D_CURRENT_HLOD_VERSION, 0, b"Z", b""))
            .unwrap();
        save.end_chunk().unwrap();
        save.begin_chunk(W3D_CHUNK_HLOD_AGGREGATE_ARRAY).unwrap();
        array_header(&mut save, 1, 0.0);
        sub_object(&mut save, 0, b"AGG");
        save.end_chunk().unwrap();
        let hlod = W3dHlod::parse(&save.into_bytes().unwrap()).unwrap();
        assert_eq!(hlod.lod_count, 0);
        assert!(hlod.lods.is_empty());
        assert_eq!(hlod.aggregates.model_names, vec![b"AGG".to_vec()]);
    }

    #[test]
    fn the_err_paths() {
        // not the HLOD header first
        let mut save = ChunkSave::new();
        save.begin_chunk(W3D_CHUNK_HLOD_LOD_ARRAY).unwrap();
        array_header(&mut save, 0, 1.0);
        save.end_chunk().unwrap();
        assert_eq!(
            W3dHlod::parse(&save.into_bytes().unwrap()),
            Err(HlodError::LoadFailed)
        );

        // a short header read: the chunk declares 20 of the 40 bytes
        let mut save = ChunkSave::new();
        save.begin_chunk(W3D_CHUNK_HLOD_HEADER).unwrap();
        save.write(&header_bytes(W3D_CURRENT_HLOD_VERSION, 1, b"S", b"T")[..20])
            .unwrap();
        save.end_chunk().unwrap();
        assert_eq!(
            W3dHlod::parse(&save.into_bytes().unwrap()),
            Err(HlodError::LoadFailed)
        );

        // the LOD loop's id check: an aggregate where LOD 1 belongs
        let mut save = ChunkSave::new();
        save.begin_chunk(W3D_CHUNK_HLOD_HEADER).unwrap();
        save.write(&header_bytes(W3D_CURRENT_HLOD_VERSION, 2, b"M", b"T"))
            .unwrap();
        save.end_chunk().unwrap();
        save.begin_chunk(W3D_CHUNK_HLOD_LOD_ARRAY).unwrap();
        array_header(&mut save, 0, 1.0);
        save.end_chunk().unwrap();
        save.begin_chunk(W3D_CHUNK_HLOD_AGGREGATE_ARRAY).unwrap();
        save.end_chunk().unwrap();
        assert_eq!(
            W3dHlod::parse(&save.into_bytes().unwrap()),
            Err(HlodError::LoadFailed)
        );

        // the LOD loop's open check: LodCount 2 with one array present
        let mut save = ChunkSave::new();
        save.begin_chunk(W3D_CHUNK_HLOD_HEADER).unwrap();
        save.write(&header_bytes(W3D_CURRENT_HLOD_VERSION, 2, b"N", b"T"))
            .unwrap();
        save.end_chunk().unwrap();
        save.begin_chunk(W3D_CHUNK_HLOD_LOD_ARRAY).unwrap();
        array_header(&mut save, 0, 1.0);
        save.end_chunk().unwrap();
        assert_eq!(
            W3dHlod::parse(&save.into_bytes().unwrap()),
            Err(HlodError::LoadFailed)
        );

        // an empty payload
        assert_eq!(W3dHlod::parse(&[]), Err(HlodError::LoadFailed));
    }

    #[test]
    fn bone_indices_wrap_through_int() {
        let mut save = ChunkSave::new();
        save.begin_chunk(W3D_CHUNK_HLOD_HEADER).unwrap();
        save.write(&header_bytes(W3D_CURRENT_HLOD_VERSION, 1, b"B", b"T"))
            .unwrap();
        save.end_chunk().unwrap();
        save.begin_chunk(W3D_CHUNK_HLOD_LOD_ARRAY).unwrap();
        array_header(&mut save, 3, 1.0);
        sub_object(&mut save, 0x7FFF_FFFF, b"BMax");
        sub_object(&mut save, 0x8000_0000, b"BMin");
        sub_object(&mut save, 0xFFFF_FFFF, b"BNeg");
        save.end_chunk().unwrap();
        let hlod = W3dHlod::parse(&save.into_bytes().unwrap()).unwrap();
        assert_eq!(hlod.lods[0].bone_indices, vec![i32::MAX, i32::MIN, -1]);
    }

    #[test]
    fn the_sub_object_strdup_caps_at_the_struct() {
        // A full 32-byte name with no NUL: the model path's capped
        // strdup keeps all 32 bytes (the original reads past the
        // struct — the documented stance).
        let full: Vec<u8> = (0..32u8).map(|i| b'0' + i % 10).collect();
        assert_eq!(full.len(), 32);
        assert!(!full.contains(&0));
        let mut save = ChunkSave::new();
        save.begin_chunk(W3D_CHUNK_HLOD_HEADER).unwrap();
        save.write(&header_bytes(W3D_CURRENT_HLOD_VERSION, 1, b"C", b"T"))
            .unwrap();
        save.end_chunk().unwrap();
        save.begin_chunk(W3D_CHUNK_HLOD_LOD_ARRAY).unwrap();
        array_header(&mut save, 1, 1.0);
        sub_object(&mut save, 0, &full);
        save.end_chunk().unwrap();
        let hlod = W3dHlod::parse(&save.into_bytes().unwrap()).unwrap();
        assert_eq!(hlod.lods[0].model_names, vec![full]);
    }

    #[test]
    fn the_constants_match_the_format() {
        assert_eq!(HLOD_HEADER_SIZE, 40);
        assert_eq!(HLOD_ARRAY_HEADER_SIZE, 8);
        assert_eq!(HLOD_SUB_OBJECT_SIZE, 36);
        assert_eq!(SUB_OBJECT_NAME_LEN, 32);
        assert_eq!(W3D_CURRENT_HLOD_VERSION, 0x0001_0000);
        assert_eq!(W3D_CHUNK_HLOD, 0x0000_0700);
        assert_eq!(W3D_CHUNK_HLOD_PROXY_ARRAY, 0x0000_0706);
        assert_eq!(NO_MAX_SCREEN_SIZE, f32::MAX);
    }
}
