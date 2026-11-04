// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Ported from the Zero Hour source release (baseline 571a7c5):
//   Core/Libraries/Source/WWVegas/WW3D2/htree.cpp
//     (HTreeClass::Load_W3D, HTreeClass::read_pivots, Get_Bone_Index,
//      Get_Parent_Index)
//   Core/Libraries/Source/WWVegas/WW3D2/pivot.cpp
//     (the PivotClass constructor state the pivot array is born with)
//   Core/Libraries/Source/WWVegas/WW3D2/pivot.h (PivotClass fields)
//   Core/Libraries/Source/WWVegas/WWMath/quat.h (Build_Matrix3D — the
//     WWINLINE quaternion-to-rotation-matrix builder)
//   Core/Libraries/Source/WWVegas/WWMath/matrix3d.h (Make_Identity,
//     Translate(const Vector3&), postMul — the AVOID_TEMP_IN_POSTMUL
//     body the #define inside the function always selects, submul)
//   Core/Libraries/Source/WWVegas/WWMath/vector3.h / vector4.h /
//     quat.h (Vector3, Vector4, Quaternion layouts the formulas read)
// See the module docs for the format layout and the documented
// differences from the C++ (update-path state deferred, release-UB
// turned into errors).

//! The **W3D hierarchy tree** — the skeleton module the skinning path
//! needs first (the plan's Phase 2 W3D track, on top of the chunk
//! container).
//!
//! A `.w3d` hierarchy asset is a [`W3D_CHUNK_HIERARCHY`] container whose
//! children describe one skeleton: a fixed 36-byte
//! [`hierarchy header`](W3dHierarchyHeader) and a pivots chunk holding
//! one 60-byte [`W3dPivotStruct`] per bone. This module ports
//! `HTreeClass::Load_W3D` (htree.cpp) — the data path the original's
//! asset manager drives through `HTreeManagerClass::Load_Tree` — so the
//! structures produced here hold exactly what the game's tree holds
//! after a load, quirks included (parity level P0, pinned by golden
//! vectors generated from the transcription in
//! `tools/differential/gen_htree_golden.cpp`).
//!
//! [`W3D_CHUNK_HIERARCHY`]: W3D_CHUNK_HIERARCHY
//!
//! # The load-path quirks
//!
//! - **the pre-3.0 root synthesis**: a file whose version is below
//!   `W3D_MAKE_VERSION(3, 0)` never had a root pivot, so the loader
//!   *increments the pivot count* and synthesizes pivot 0 —
//!   `RootTransform`, identity, no parent — and shifts every file parent
//!   index up by one (`ParentIdx + 1`, wrapping: `0xFFFFFFFE` becomes
//!   the root sentinel and a *second* root pivot, a loadable quirk the
//!   original's release build never asserts);
//! - **unlike the mesh loader**, a failing `read_pivots` **fails the
//!   load** — the original's `goto Error` path (`LOAD_ERROR`), not the
//!   mesh's discarded-error walk;
//! - a **missing pivots chunk is not an error**: the load succeeds with
//!   the default-constructed pivots (empty names, no parents, identity
//!   base transforms, `Index` 0); a **second** pivots chunk re-reads and
//!   overwrites them;
//! - **parent pointers are formed, never resolved**: `read_pivots`
//!   stores `&Pivot[ParentIdx]` for later use — the port stores the
//!   index (the plan §3.3 handle rule) and *states the contract*: an
//!   index outside the tree (other than the `0xFFFFFFFF` root sentinel)
//!   fails the load, where the C++ keeps a dangling pointer that the
//!   first `Base_Update` would dereference;
//! - the pivot count is the header's `uint32` **converted to `int`**
//!   (count `0x8000_0000` and up fail the `NumPivots < 1` check as
//!   negatives), and the pre-3.0 increment wraps (`0xFFFFFFFF` → 0 →
//!   load failure) — both pinned by the goldens;
//! - the base transforms are built with the original's exact
//!   mixed-precision arithmetic: `Translate` is pure `f32`,
//!   `Build_Matrix3D` promotes through `f64` on every term *except* the
//!   `[1][1]` diagonal (the original's `2.0f` literal slip), and
//!   `postMul` is the `AVOID_TEMP_IN_POSTMUL` `submul` body — nothing is
//!   normalized and nothing is fused.
//!
//! # Differences from the C++
//!
//! Release-build undefined behaviour becomes [`HtreeError`] (the load
//! fails): the out-of-range parent index above. Where the C++ reads
//! *uninitialised* state (the 16-byte name fields of freshly allocated
//! pivots) the port reads zeroes; the C-string views the queries use are
//! unaffected. The transcription's deliberate stance on the same set is
//! documented in `tools/differential/gen_htree_golden.cpp`'s header
//! comment.
//!
//! Update-path state is out of scope for the format layer: the computed
//! `Transform` member (identity after every load — `Base_Update` /
//! `Anim_Update` fill it), bone capture (`Capture_Bone` et al.), the
//! `Scale`/`Alter_Avatar_HTree`/`Create_Morphed`/`Create_Interpolated`
//! tree surgery, and the `HTreeManagerClass` pool itself land with the
//! animation and model-viewer slices. The query surface the skinning
//! path needs *today* is ported: [`get_bone_index`](W3dHtree::
//! get_bone_index) (the `stricmp` first-match scan) and
//! [`get_parent_index`](W3dHtree::get_parent_index).
//!
//! # Examples
//!
//! ```
//! use rzh_formats::chunk::ChunkLoad;
//! use rzh_formats::w3d_htree::{W3dHtree, W3D_CHUNK_HIERARCHY};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // A .w3d file holds named top-level assets; a hierarchy asset is a
//! // W3D_CHUNK_HIERARCHY container whose payload this parser consumes.
//! # let w3d_file: &[u8] = &[];
//! # let _ = w3d_file;
//! # let payload: Vec<u8> = Vec::new(); // the container's bytes
//! # if payload.is_empty() { return Ok(()); }
//! let mut load = ChunkLoad::new(&payload);
//! assert!(load.open_chunk()? && load.cur_chunk_id() == W3D_CHUNK_HIERARCHY);
//! let tree_bytes = &payload[8..8 + load.cur_chunk_length() as usize];
//! let tree = W3dHtree::parse(tree_bytes)?;
//! assert_eq!(tree.get_bone_index(b"RootTransform"), 0);
//! # Ok(())
//! # }
//! ```

use std::fmt;

use crate::chunk::ChunkLoad;

// ---------------------------------------------------------------------------
// chunk ids and format constants (w3d_file.h)
// ---------------------------------------------------------------------------

/// The hierarchy-tree container a `.w3d` file wraps a hierarchy asset in.
pub const W3D_CHUNK_HIERARCHY: u32 = 0x0000_0100;
/// The 36-byte `W3dHierarchyStruct`.
pub const W3D_CHUNK_HIERARCHY_HEADER: u32 = 0x0000_0101;
/// The array of `W3dPivotStruct` bones.
pub const W3D_CHUNK_PIVOTS: u32 = 0x0000_0102;
/// Exporter-only pivot fixup transforms (the loader skips them).
pub const W3D_CHUNK_PIVOT_FIXUPS: u32 = 0x0000_0103;

/// The length of every fixed name field (`W3D_NAME_LEN`).
pub const W3D_NAME_LEN: usize = 16;
/// The size of the hierarchy header struct.
pub const HIERARCHY_HEADER_SIZE: u32 = 36;
/// The size of one pivot struct.
pub const PIVOT_SIZE: u32 = 60;
/// The pivot count that means "root pivot; no parent" (`0xffffffff`).
pub const NO_PARENT: u32 = 0xFFFF_FFFF;
/// The version the current exporter writes (`W3D_CURRENT_HTREE_VERSION`).
pub const W3D_CURRENT_HTREE_VERSION: u32 = w3d_make_version(4, 1);
/// The synthesized pre-3.0 root pivot's name.
pub const ROOT_TRANSFORM_NAME: &[u8; 13] = b"RootTransform";

/// `W3D_MAKE_VERSION` — the packed `(major, minor)` file version.
pub const fn w3d_make_version(major: u32, minor: u32) -> u32 {
    (major << 16) | minor
}

// ---------------------------------------------------------------------------
// errors
// ---------------------------------------------------------------------------

/// Errors the hierarchy load path can produce.
///
/// The C++ release build turns most contract violations into undefined
/// behaviour (the dangling parent pointer the port rejects); the
/// original's `Load_W3D` has exactly one failure code — `LOAD_ERROR` —
/// and the port states the same contract: a bad header, a short pivots
/// chunk or an unresolvable parent fails the load outright. Note the
/// deliberate contrast with the mesh loader: a failing sub-read here
/// does *not* produce a partial success.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum HtreeError {
    /// `LOAD_ERROR` — the load failed outright (bad first chunk, short
    /// header, the pivot count's `int` conversion going negative, a
    /// short pivots chunk, or an out-of-range parent index).
    LoadFailed,
    /// The chunk container itself errored (depth cap).
    Chunk(crate::chunk::ChunkError),
}

impl fmt::Display for HtreeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LoadFailed => write!(f, "W3D hierarchy load failed"),
            Self::Chunk(e) => write!(f, "W3D hierarchy chunk error: {e}"),
        }
    }
}

impl std::error::Error for HtreeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Chunk(e) => Some(e),
            _ => None,
        }
    }
}

impl From<crate::chunk::ChunkError> for HtreeError {
    fn from(e: crate::chunk::ChunkError) -> Self {
        Self::Chunk(e)
    }
}

// ---------------------------------------------------------------------------
// the fixed structs
// ---------------------------------------------------------------------------

/// The 36-byte `W3dHierarchyStruct`, decoded.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct W3dHierarchyHeader {
    /// File-format version (`W3D_MAKE_VERSION`).
    pub version: u32,
    /// The hierarchy name (a C string in a 16-byte field).
    pub name: [u8; W3D_NAME_LEN],
    /// Declared pivot count — **not** the loaded count: a pre-3.0 file's
    /// count is bumped by the root synthesis, so
    /// [`W3dHtree::pivots`]'s length is what the game's `NumPivots`
    /// member holds.
    pub num_pivots: u32,
    /// The hierarchy's bounding-sphere centre (read, never used by the
    /// loader — kept for the format layer).
    pub center: [f32; 3],
}

impl W3dHierarchyHeader {
    /// Decodes the struct from its 36 little-endian bytes.
    pub fn from_bytes(bytes: &[u8; HIERARCHY_HEADER_SIZE as usize]) -> Self {
        let u32at = |o: usize| u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
        let f32at = |o: usize| f32::from_bits(u32at(o));
        let mut name = [0u8; W3D_NAME_LEN];
        name.copy_from_slice(&bytes[4..20]);
        Self {
            version: u32at(0),
            name,
            num_pivots: u32at(20),
            center: [f32at(24), f32at(28), f32at(32)],
        }
    }

    /// The C-string view of the name field.
    pub fn name_str(&self) -> &[u8] {
        cstr(&self.name)
    }
}

/// The 60-byte `W3dPivotStruct`, decoded — the wire form of one bone.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct W3dPivotStruct {
    /// The bone name (a C string in a 16-byte field).
    pub name: [u8; W3D_NAME_LEN],
    /// The parent's index — `0xFFFFFFFF` for a root pivot.
    pub parent_idx: u32,
    /// Translation to the pivot point.
    pub translation: [f32; 3],
    /// Euler-angle orientation — dead data on the wire (the loader only
    /// uses `rotation`); kept for the format layer.
    pub euler_angles: [f32; 3],
    /// The orientation quaternion `(X, Y, Z, W)` — imaginary parts then
    /// real part, unnormalized.
    pub rotation: [f32; 4],
}

impl W3dPivotStruct {
    /// Decodes the struct from its 60 little-endian bytes.
    pub fn from_bytes(bytes: &[u8; PIVOT_SIZE as usize]) -> Self {
        let u32at = |o: usize| u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
        let f32at = |o: usize| f32::from_bits(u32at(o));
        let mut name = [0u8; W3D_NAME_LEN];
        name.copy_from_slice(&bytes[0..16]);
        Self {
            name,
            parent_idx: u32at(16),
            translation: [f32at(20), f32at(24), f32at(28)],
            euler_angles: [f32at(32), f32at(36), f32at(40)],
            rotation: [f32at(44), f32at(48), f32at(52), f32at(56)],
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

// ---------------------------------------------------------------------------
// the loaded pivot (PivotClass, load-path state)
// ---------------------------------------------------------------------------

/// One loaded bone — the `PivotClass` state `read_pivots` leaves behind.
///
/// The parent is the *index* of the parent pivot (the plan's handle
/// rule; the original stores a pointer), `None` for a root. `index` is
/// the pivot's own `Index` field — the array position for read pivots,
/// 0 for the default-constructed ones a missing pivots chunk leaves.
/// `base_transform` is the row-major 3x4 base pose (three `[x y z w]`
/// rows), built exactly as the original builds it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct W3dPivot {
    /// The bone name (a C string in a 16-byte field).
    pub name: [u8; W3D_NAME_LEN],
    /// The parent pivot's index, `None` for a root.
    pub parent: Option<u32>,
    /// The pivot's `Index` field.
    pub index: u32,
    /// The visibility flag (always `true` after a load).
    pub is_visible: bool,
    /// The base-pose transform, row-major: `[x y z w]` per row.
    pub base_transform: [f32; 12],
}

impl Default for W3dPivot {
    /// The `PivotClass` constructor state: empty name, no parent,
    /// identity transforms, `Index` 0, visible.
    fn default() -> Self {
        Self {
            name: [0; W3D_NAME_LEN],
            parent: None,
            index: 0,
            is_visible: true,
            base_transform: make_identity(),
        }
    }
}

impl W3dPivot {
    /// The C-string view of the name field.
    pub fn name_str(&self) -> &[u8] {
        cstr(&self.name)
    }
}

// ---------------------------------------------------------------------------
// the tree
// ---------------------------------------------------------------------------

/// A loaded W3D hierarchy tree — the state `HTreeClass::Load_W3D` leaves
/// behind, minus the update-path state (see the module docs).
#[derive(Debug, Clone, PartialEq)]
pub struct W3dHtree {
    /// The decoded hierarchy header (the pristine 36 bytes; see
    /// [`W3dHierarchyHeader::num_pivots`] for the pre-3.0 count bump).
    pub header: W3dHierarchyHeader,
    /// The pivots — `header.num_pivots`, or one more for a pre-3.0 file.
    pub pivots: Vec<W3dPivot>,
}

impl W3dHtree {
    /// Parses the payload of a [`W3D_CHUNK_HIERARCHY`] container — the
    /// bytes between the container's header and its end. The caller
    /// (the future W3D asset loader) opens the container first, exactly
    /// like the original's asset manager does before calling
    /// `Load_W3D` through `HTreeManagerClass::Load_Tree`.
    ///
    /// A failing pivots read **fails** this parse (the original's
    /// `goto Error`) — a short pivots chunk, an unresolvable parent
    /// index or a negative pivot count all return [`Err`] — and a
    /// *missing* pivots chunk succeeds with the default-constructed
    /// pivots.
    pub fn parse(data: &[u8]) -> Result<Self, HtreeError> {
        let mut cload = ChunkLoad::new(data);

        // Read the first chunk, it should be the hierarchy header
        if !cload.open_chunk()? {
            return Err(HtreeError::LoadFailed);
        }
        if cload.cur_chunk_id() != W3D_CHUNK_HIERARCHY_HEADER {
            // "ERROR: Expected Hierarchy Header"
            return Err(HtreeError::LoadFailed);
        }
        let mut header_bytes = [0u8; HIERARCHY_HEADER_SIZE as usize];
        if cload.read(&mut header_bytes)? != HIERARCHY_HEADER_SIZE {
            return Err(HtreeError::LoadFailed);
        }
        let header = W3dHierarchyHeader::from_bytes(&header_bytes);
        cload.close_chunk()?;

        // Check the version, if < 3.0 add a root node for everything to
        // attach to (read_pivots will also be notified of this).
        let pre30 = header.version < w3d_make_version(3, 0);

        // The member count: the uint32 converted to int, after the
        // pre-3.0 bump (which wraps).
        let declared = if pre30 {
            header.num_pivots.wrapping_add(1)
        } else {
            header.num_pivots
        };
        let num_pivots = declared as i32;
        if num_pivots < 1 {
            return Err(HtreeError::LoadFailed);
        }
        // Pivot = new PivotClass[NumPivots] — the constructor state
        let mut pivots = vec![W3dPivot::default(); num_pivots as usize];

        // Now, read in all of the other chunks for this hierarchy.
        while cload.open_chunk()? {
            match cload.cur_chunk_id() {
                W3D_CHUNK_PIVOTS if !read_pivots(&mut pivots, &mut cload, pre30)? => {
                    // goto Error — unlike the mesh loader, the failing
                    // sub-read fails the load
                    return Err(HtreeError::LoadFailed);
                }
                _ => {
                    // "ERROR: expected W3D_CHUNK_PIVOTS!" — release
                    // build: nothing
                }
            }
            cload.close_chunk()?;
        }

        Ok(Self { header, pivots })
    }

    /// The tree's name (`HTreeClass::Get_Name`) — the C-string view of
    /// the header's name field.
    pub fn name_str(&self) -> &[u8] {
        self.header.name_str()
    }

    /// The pivot's name (`HTreeClass::Get_Bone_Name`). Panics when the
    /// index is out of range (the original asserts the same).
    pub fn pivot_name_str(&self, index: usize) -> &[u8] {
        self.pivots[index].name_str()
    }

    /// The number of loaded pivots (`HTreeClass::Num_Pivots`).
    pub fn num_pivots(&self) -> usize {
        self.pivots.len()
    }

    /// Finds a bone by name (`HTreeClass::Get_Bone_Index`): the first
    /// pivot whose name matches `name` **case-insensitively** (the
    /// original's `stricmp`), or 0 when nothing matches — the root is
    /// the fallback, exactly like the original.
    pub fn get_bone_index(&self, name: &[u8]) -> usize {
        for (i, pivot) in self.pivots.iter().enumerate() {
            if pivot.name_str().eq_ignore_ascii_case(name) {
                return i;
            }
        }
        0
    }

    /// The index of a bone's parent (`HTreeClass::Get_Parent_Index`):
    /// the *parent's `Index` field* (which equals its array position
    /// for read pivots and 0 for the default-constructed ones), 0 for a
    /// root. Panics when the index is out of range (the original
    /// asserts the same).
    pub fn get_parent_index(&self, bone_index: usize) -> usize {
        match self.pivots[bone_index].parent {
            None => 0,
            Some(parent) => self.pivots[parent as usize].index as usize,
        }
    }
}

// ---------------------------------------------------------------------------
// read_pivots
// ---------------------------------------------------------------------------

/// `HTreeClass::read_pivots` — `Ok(true)` on success, `Ok(false)` on
/// the short-read failure that fails the load, `Err` for a container
/// error.
fn read_pivots(
    pivots: &mut [W3dPivot],
    cload: &mut ChunkLoad<'_>,
    pre30: bool,
) -> Result<bool, crate::chunk::ChunkError> {
    let num_pivots = pivots.len();
    let mut first_piv = 0usize;

    // At (w3d file format) version 3.0, I added a node for the root.
    // Pre-3.0 htrees didn't have this so we just put one in.
    if pre30 {
        pivots[0].index = 0;
        pivots[0].parent = None;
        pivots[0].base_transform = make_identity();
        // Pivot[0].Transform.Make_Identity() — identity already
        pivots[0].is_visible = true;
        let mut root_name = [0u8; W3D_NAME_LEN];
        root_name[..ROOT_TRANSFORM_NAME.len()].copy_from_slice(ROOT_TRANSFORM_NAME);
        pivots[0].name = root_name;
        first_piv = 1;
    }

    for (pidx, pivot) in pivots.iter_mut().enumerate().skip(first_piv) {
        let mut piv_bytes = [0u8; PIVOT_SIZE as usize];
        if cload.read(&mut piv_bytes)? != PIVOT_SIZE {
            return Ok(false);
        }
        let piv = W3dPivotStruct::from_bytes(&piv_bytes);

        pivot.name = piv.name;
        pivot.index = pidx as u32;

        let mut base = make_identity();
        translate_v(&mut base, &piv.translation);
        post_mul(&mut base, &build_matrix3d(&piv.rotation));
        pivot.base_transform = base;

        // At version 3.0 a root node was added, this "fixes up" pre-3.0
        // files to have that root node.
        let mut parent_idx = piv.parent_idx;
        if pre30 {
            parent_idx = parent_idx.wrapping_add(1);
        }

        // Set the parent pointer. The first pivot will have a parent
        // index of -1 (in post-3.0 files) so set its parent to null.
        // (the uint32 field against the wrapped -1; the release build
        // skips assert(pidx == 0))
        if parent_idx == NO_PARENT {
            pivot.parent = None;
        } else if parent_idx as usize >= num_pivots {
            // The release build stores an out-of-bounds PivotClass*
            // that the first Base_Update would dereference; the port
            // states the contract instead (see the module docs).
            return Ok(false);
        } else {
            pivot.parent = Some(parent_idx);
        }
    }

    // Pivot[0].Transform.Make_Identity(); Pivot[0].IsVisible = true;
    pivots[0].is_visible = true;

    Ok(true)
}

// ---------------------------------------------------------------------------
// the WWMath transcription (matrix3d.h / quat.h)
// ---------------------------------------------------------------------------

/// `Matrix3D::Make_Identity` — the row-major `[x y z w]` identity.
fn make_identity() -> [f32; 12] {
    [
        1.0, 0.0, 0.0, 0.0, //
        0.0, 1.0, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0,
    ]
}

/// `Matrix3D::Translate(const Vector3&)` — post-multiplies by a
/// translation, pure `f32` in the original's expression order:
/// `Row[r][3] += Row[r][0]*t[0] + Row[r][1]*t[1] + Row[r][2]*t[2]`.
fn translate_v(m: &mut [f32; 12], t: &[f32; 3]) {
    for r in 0..3 {
        m[r * 4 + 3] += m[r * 4] * t[0] + m[r * 4 + 1] * t[1] + m[r * 4 + 2] * t[2];
    }
}

/// `submul` — one row dotted with one column:
/// `row.X * tmp1 + row.Y * tmp2 + row.Z * tmp3`.
#[inline]
fn submul(row: [f32; 3], col: [f32; 3]) -> f32 {
    row[0] * col[0] + row[1] * col[1] + row[2] * col[2]
}

/// `Matrix3D::postMul` — `this = this * that` through the
/// `AVOID_TEMP_IN_POSTMUL` body (the `submul` temporaries; the W column
/// accumulates because of the implicit `[0 0 0 1]` fourth row).
fn post_mul(m: &mut [f32; 12], that: &[f32; 12]) {
    // that's columns: X = [0 4 8], Y = [1 5 9], Z = [2 6 10], W = [3 7 11]
    let cols = [
        [that[0], that[4], that[8]],
        [that[1], that[5], that[9]],
        [that[2], that[6], that[10]],
        [that[3], that[7], that[11]],
    ];
    for r in 0..3 {
        let row = [m[r * 4], m[r * 4 + 1], m[r * 4 + 2]];

        let tmp_x = submul(row, cols[0]);
        let tmp_y = submul(row, cols[1]);
        let tmp_z = submul(row, cols[2]);
        let tmp_w = submul(row, cols[3]);

        m[r * 4] = tmp_x;
        m[r * 4 + 1] = tmp_y;
        m[r * 4 + 2] = tmp_z;
        m[r * 4 + 3] += tmp_w;
    }
}

/// `Build_Matrix3D(q)` (quat.h WWINLINE) — the rotation matrix of a
/// quaternion. The `f64`/`f32` literal mix is the original's: every term
/// promotes through `double` **except** the `[1][1]` diagonal, whose
/// `2.0f` keeps the sum in `f32` until the final `1.0 -` promotion.
fn build_matrix3d(q: &[f32; 4]) -> [f32; 12] {
    let mut m = [0f32; 12];
    m[0] = (1.0_f64 - 2.0_f64 * f64::from(q[1] * q[1] + q[2] * q[2])) as f32;
    m[1] = (2.0_f64 * f64::from(q[0] * q[1] - q[2] * q[3])) as f32;
    m[2] = (2.0_f64 * f64::from(q[2] * q[0] + q[1] * q[3])) as f32;

    m[4] = (2.0_f64 * f64::from(q[0] * q[1] + q[2] * q[3])) as f32;
    // the [1][1] term keeps its float 2.0f until the final promotion
    m[5] = (1.0_f64 - f64::from(2.0_f32 * (q[2] * q[2] + q[0] * q[0]))) as f32;
    m[6] = (2.0_f64 * f64::from(q[1] * q[2] - q[0] * q[3])) as f32;

    m[8] = (2.0_f64 * f64::from(q[2] * q[0] - q[1] * q[3])) as f32;
    m[9] = (2.0_f64 * f64::from(q[1] * q[2] + q[0] * q[3])) as f32;
    m[10] = (1.0_f64 - 2.0_f64 * f64::from(q[1] * q[1] + q[0] * q[0])) as f32;

    // no translation
    m[3] = 0.0;
    m[7] = 0.0;
    m[11] = 0.0;
    m
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// A pivot's 60 bytes from its raw fields.
    fn pivot_bytes(
        name: &[u8; 16],
        parent: u32,
        translation: [f32; 3],
        rotation: [f32; 4],
    ) -> [u8; 60] {
        let mut bytes = [0u8; 60];
        bytes[0..16].copy_from_slice(name);
        bytes[16..20].copy_from_slice(&parent.to_le_bytes());
        for (i, word) in translation.iter().chain(rotation.iter()).enumerate() {
            // translation at 20..32, eulerAngles zeroed at 32..44,
            // rotation at 44..60
            let offset = if i < 3 { 20 + i * 4 } else { 44 + (i - 3) * 4 };
            bytes[offset..offset + 4].copy_from_slice(&word.to_bits().to_le_bytes());
        }
        bytes
    }

    /// A raw pivot's wire fields: (name, parent, translation, rotation).
    type RawPivot = ([u8; 16], u32, [f32; 3], [f32; 4]);

    /// A whole hierarchy payload from raw pivots.
    fn hierarchy(version: u32, name: &str, raw_pivots: &[RawPivot]) -> Vec<u8> {
        let mut out = Vec::new();
        // the header chunk
        out.extend_from_slice(&W3D_CHUNK_HIERARCHY_HEADER.to_le_bytes());
        out.extend_from_slice(&HIERARCHY_HEADER_SIZE.to_le_bytes());
        out.extend_from_slice(&version.to_le_bytes());
        let mut name_field = [0u8; 16];
        name_field[..name.len()].copy_from_slice(name.as_bytes());
        out.extend_from_slice(&name_field);
        out.extend_from_slice(&(raw_pivots.len() as u32).to_le_bytes());
        for word in [0.0f32, 0.0, 0.0] {
            out.extend_from_slice(&word.to_bits().to_le_bytes());
        }
        // the pivots chunk
        out.extend_from_slice(&W3D_CHUNK_PIVOTS.to_le_bytes());
        let pivots_len = (raw_pivots.len() * 60) as u32;
        out.extend_from_slice(&pivots_len.to_le_bytes());
        for (name, parent, translation, rotation) in raw_pivots {
            out.extend_from_slice(&pivot_bytes(name, *parent, *translation, *rotation));
        }
        out
    }

    fn name16(s: &str) -> [u8; 16] {
        let mut n = [0u8; 16];
        n[..s.len()].copy_from_slice(s.as_bytes());
        n
    }

    #[test]
    fn header_decodes_every_field() {
        let mut bytes = [0u8; 36];
        bytes[0..4].copy_from_slice(&0x0002_0001u32.to_le_bytes());
        bytes[4..20].copy_from_slice(b"AVTank\0\0\0\0\0\0\0\0\0\0");
        bytes[20..24].copy_from_slice(&37u32.to_le_bytes());
        for (i, word) in [1.5f32, -2.25, 0.5].iter().enumerate() {
            bytes[24 + i * 4..28 + i * 4].copy_from_slice(&word.to_bits().to_le_bytes());
        }
        let header = W3dHierarchyHeader::from_bytes(&bytes);
        assert_eq!(header.version, 0x0002_0001);
        assert_eq!(header.name_str(), b"AVTank");
        assert_eq!(header.num_pivots, 37);
        assert_eq!(header.center, [1.5, -2.25, 0.5]);
    }

    #[test]
    fn pivot_struct_decodes_every_field() {
        let mut bytes = [0u8; 60];
        bytes[0..16].copy_from_slice(b"TORSO\0\0\0\0\0\0\0\0\0\0\0");
        bytes[16..20].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        for (i, word) in [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0]
            .iter()
            .enumerate()
        {
            bytes[20 + i * 4..24 + i * 4].copy_from_slice(&word.to_bits().to_le_bytes());
        }
        let piv = W3dPivotStruct::from_bytes(&bytes);
        assert_eq!(piv.name_str(), b"TORSO");
        assert_eq!(piv.parent_idx, 0xFFFF_FFFF);
        assert_eq!(piv.translation, [1.0, 2.0, 3.0]);
        assert_eq!(piv.euler_angles, [4.0, 5.0, 6.0]);
        assert_eq!(piv.rotation, [7.0, 8.0, 9.0, 10.0]);
    }

    #[test]
    fn empty_and_garbage_payloads_fail() {
        assert_eq!(W3dHtree::parse(&[]), Err(HtreeError::LoadFailed));
        // a first chunk that is not the hierarchy header
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&W3D_CHUNK_PIVOTS.to_le_bytes());
        bytes.extend_from_slice(&4u32.to_le_bytes());
        bytes.extend_from_slice(&[0, 0, 0, 0]);
        assert_eq!(W3dHtree::parse(&bytes), Err(HtreeError::LoadFailed));
        // a truncated header payload (chunk declares 36, only 20 bytes)
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&W3D_CHUNK_HIERARCHY_HEADER.to_le_bytes());
        bytes.extend_from_slice(&HIERARCHY_HEADER_SIZE.to_le_bytes());
        bytes.extend_from_slice(&[0u8; 20]);
        assert_eq!(W3dHtree::parse(&bytes), Err(HtreeError::LoadFailed));
        // error display is stable
        assert_eq!(
            HtreeError::LoadFailed.to_string(),
            "W3D hierarchy load failed"
        );
    }

    #[test]
    fn modern_tree_loads_with_exact_base_transforms() {
        // ROOT at identity; TORSO translated (0.25, 1.0, -0.75) with a
        // 90-degree Y rotation (0, s45, 0, s45) — the same f32 the
        // golden corpus's 0.70710678118654752440f literal rounds to
        let s45: f32 = std::f32::consts::FRAC_1_SQRT_2;
        let bytes = hierarchy(
            w3d_make_version(4, 1),
            "AVTank",
            &[
                (
                    name16("ROOT"),
                    NO_PARENT,
                    [0.0, 0.0, 0.0],
                    [0.0, 0.0, 0.0, 1.0],
                ),
                (name16("TORSO"), 0, [0.25, 1.0, -0.75], [0.0, s45, 0.0, s45]),
            ],
        );
        let tree = W3dHtree::parse(&bytes).expect("the tree loads");
        assert_eq!(tree.name_str(), b"AVTank");
        assert_eq!(tree.num_pivots(), 2);
        assert_eq!(tree.pivots[0].name_str(), b"ROOT");
        assert_eq!(tree.pivots[0].parent, None);
        assert_eq!(tree.pivots[0].base_transform, make_identity());

        // TORSO: the translation survives through the identity rows, the
        // rotation is Build_Matrix3D of the 45-degree quaternion
        let t = &tree.pivots[1];
        assert_eq!(t.name_str(), b"TORSO");
        assert_eq!(t.parent, Some(0));
        assert_eq!(t.index, 1);
        assert!(t.is_visible);
        assert_eq!(t.base_transform[3], 0.25);
        assert_eq!(t.base_transform[7], 1.0);
        assert_eq!(t.base_transform[11], -0.75);
        // the exact bits the golden pins: the Y-rotation diagonal lands
        // on 5.96e-8 (not 0) through the f64 promotion
        assert_eq!(t.base_transform[0].to_bits(), 0x3380_0000);
        assert_eq!(t.base_transform[2].to_bits(), 0x3f7f_ffff);
        assert_eq!(t.base_transform[8].to_bits(), 0xbf7f_ffff);
        assert_eq!(t.base_transform[10].to_bits(), 0x3380_0000);
    }

    #[test]
    fn zero_quaternion_is_identity_rotation() {
        let bytes = hierarchy(
            w3d_make_version(4, 1),
            "ZeroQ",
            &[
                (
                    name16("A"),
                    NO_PARENT,
                    [1.0, 2.0, 3.0],
                    [0.0, 0.0, 0.0, 0.0],
                ),
                (name16("B"), 0, [-1.5, 0.25, 4.75], [0.0, 0.0, 0.0, 0.0]),
            ],
        );
        let tree = W3dHtree::parse(&bytes).expect("the tree loads");
        let expected = [
            1.0, 0.0, 0.0, 1.0, //
            0.0, 1.0, 0.0, 2.0, //
            0.0, 0.0, 1.0, 3.0,
        ];
        assert_eq!(tree.pivots[0].base_transform, expected);
        let expected = [
            1.0, 0.0, 0.0, -1.5, //
            0.0, 1.0, 0.0, 0.25, //
            0.0, 0.0, 1.0, 4.75,
        ];
        assert_eq!(tree.pivots[1].base_transform, expected);
    }

    #[test]
    fn negative_zero_translations_round_to_positive_zero() {
        // Row[r][3] += 1.0*-0.0 + 0.0*-0.0 + 0.0*-0.0 = -0.0 + -0.0 +
        // -0.0 = -0.0, then 0.0 + -0.0 = +0.0 (the IEEE quirk)
        let bytes = hierarchy(
            w3d_make_version(4, 1),
            "Ident",
            &[(name16("P1"), 0, [-0.0, -0.0, -0.0], [0.0, 0.0, 0.0, 1.0])],
        );
        let tree = W3dHtree::parse(&bytes).expect("the tree loads");
        assert_eq!(tree.pivots[0].base_transform[3].to_bits(), 0x0000_0000);
        assert_eq!(tree.pivots[0].base_transform[7].to_bits(), 0x0000_0000);
        assert_eq!(tree.pivots[0].base_transform[11].to_bits(), 0x0000_0000);
    }

    #[test]
    fn unnormalized_quaternions_scale_the_matrix() {
        // (2,0,0,0): [0][0] = 1-2*(0+0) = 1, [0][1] = 0, [1][1] =
        // 1 - 2*0 = 1... but [0][0] uses q[1]*q[1] + q[2]*q[2] = 0 —
        // the identity. The off-diagonal q[0] terms DO scale: [1][0] =
        // 2*(q[0]*q[1] + q[2]*q[3]) = 0; the [1][1] uses q[2],q[0] = 0.
        // The diagonal [0][0] stays 1 — the unnormalized magnitude shows
        // only where the q[0] self products land.
        let bytes = hierarchy(
            w3d_make_version(4, 1),
            "Unnorm",
            &[(
                name16("B"),
                NO_PARENT,
                [1.0, 1.0, 1.0],
                [2.0, 0.0, 0.0, 0.0],
            )],
        );
        let tree = W3dHtree::parse(&bytes).expect("the tree loads");
        let b = &tree.pivots[0].base_transform;
        assert_eq!(b[0], 1.0); // 1 - 2*(q1*q1 + q2*q2) = 1 - 0
        assert_eq!(b[5], -7.0); // 1 - 2f*(q2*q2 + q0*q0) = 1 - 8
        assert_eq!(b[10], -7.0); // 1 - 2*(q1*q1 + q0*q0) = 1 - 8
        assert_eq!(b[1], 0.0);
        assert_eq!(b[4], 0.0);
    }

    #[test]
    fn pre30_synthesizes_the_root_and_fixes_parents() {
        // old file: TORSO (root), UR_ARM (parent TORSO), HEAD (parent
        // TORSO) — after the load: RootTransform + the three shifted
        let bytes = hierarchy(
            w3d_make_version(2, 1),
            "OldTank",
            &[
                (
                    name16("TORSO"),
                    NO_PARENT,
                    [0.0, 0.0, 0.0],
                    [0.0, 0.0, 0.0, 1.0],
                ),
                (name16("UR_ARM"), 0, [1.25, 0.5, 0.0], [0.5, 0.5, 0.5, 0.5]),
                (name16("HEAD"), 0, [0.0, 2.0, 0.0], [0.0, 0.0, 0.0, 1.0]),
            ],
        );
        let tree = W3dHtree::parse(&bytes).expect("the tree loads");
        assert_eq!(tree.header.num_pivots, 3); // the pristine header count
        assert_eq!(tree.num_pivots(), 4); // the bumped member count
        assert_eq!(tree.pivots[0].name_str(), b"RootTransform");
        assert_eq!(tree.pivots[0].parent, None);
        assert_eq!(tree.pivots[0].base_transform, make_identity());
        assert_eq!(tree.pivots[1].name_str(), b"TORSO");
        assert_eq!(tree.pivots[1].parent, Some(0)); // -1 wrapped to the root
        assert_eq!(tree.pivots[2].name_str(), b"UR_ARM");
        assert_eq!(tree.pivots[2].parent, Some(1)); // old 0 + 1
        assert_eq!(tree.pivots[3].name_str(), b"HEAD");
        assert_eq!(tree.pivots[3].parent, Some(1));
    }

    #[test]
    fn pre30_zero_pivots_synthesizes_the_root() {
        let bytes = hierarchy(w3d_make_version(2, 1), "EmptyOld", &[]);
        let tree = W3dHtree::parse(&bytes).expect("the tree loads");
        assert_eq!(tree.num_pivots(), 1);
        assert_eq!(tree.pivots[0].name_str(), b"RootTransform");
    }

    #[test]
    fn pre30_parent_wrap_creates_a_second_root() {
        // ParentIdx 0xFFFFFFFE + 1 wraps to the 0xFFFFFFFF sentinel
        let bytes = hierarchy(
            w3d_make_version(2, 255),
            "WrapRoot",
            &[
                (
                    name16("FIRST"),
                    NO_PARENT,
                    [1.0, 0.0, 0.0],
                    [0.0, 0.0, 0.0, 1.0],
                ),
                (
                    name16("SECOND"),
                    0xFFFF_FFFE,
                    [2.0, 0.0, 0.0],
                    [0.0, 0.0, 0.0, 1.0],
                ),
            ],
        );
        let tree = W3dHtree::parse(&bytes).expect("the tree loads");
        assert_eq!(tree.num_pivots(), 3);
        assert_eq!(tree.pivots[2].name_str(), b"SECOND");
        assert_eq!(tree.pivots[2].parent, None); // the wrapped sentinel
    }

    #[test]
    fn pre30_count_wrap_fails_the_load() {
        // NumPivots 0xFFFFFFFF + 1 wraps to 0 — below the minimum
        let mut bytes = hierarchy(w3d_make_version(2, 1), "WrapCount", &[]);
        // patch the header's count field (offset 8 + 4 + 16)
        bytes[28..32].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        assert_eq!(W3dHtree::parse(&bytes), Err(HtreeError::LoadFailed));
    }

    #[test]
    fn post30_zero_pivots_fail() {
        let bytes = hierarchy(w3d_make_version(4, 1), "NoPivots", &[]);
        assert_eq!(W3dHtree::parse(&bytes), Err(HtreeError::LoadFailed));
    }

    #[test]
    fn version_exactly_30_is_not_pre30() {
        let bytes = hierarchy(
            w3d_make_version(3, 0),
            "V30",
            &[(
                name16("R"),
                NO_PARENT,
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            )],
        );
        let tree = W3dHtree::parse(&bytes).expect("the tree loads");
        assert_eq!(tree.num_pivots(), 1); // no synthesized root
    }

    #[test]
    fn short_pivots_chunk_fails_the_load() {
        // two structs for three pivots — unlike the mesh loader, the
        // failing sub-read fails the load
        let mut bytes = hierarchy(
            w3d_make_version(4, 1),
            "ShortPivots",
            &[
                (
                    name16("R"),
                    NO_PARENT,
                    [0.0, 0.0, 0.0],
                    [0.0, 0.0, 0.0, 1.0],
                ),
                (name16("A"), 0, [1.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0]),
            ],
        );
        // patch the header's pivot count to 3 (offset 8 + 4 + 16)
        bytes[28..32].copy_from_slice(&3u32.to_le_bytes());
        assert_eq!(W3dHtree::parse(&bytes), Err(HtreeError::LoadFailed));
    }

    #[test]
    fn out_of_range_parent_fails_the_load() {
        let bytes = hierarchy(
            w3d_make_version(4, 1),
            "BadParent",
            &[
                (
                    name16("R"),
                    NO_PARENT,
                    [0.0, 0.0, 0.0],
                    [0.0, 0.0, 0.0, 1.0],
                ),
                (name16("B"), 5, [1.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0]),
            ],
        );
        assert_eq!(W3dHtree::parse(&bytes), Err(HtreeError::LoadFailed));
        // 0xFFFFFFFE is out of range post-3.0 too (only 0xFFFFFFFF is
        // the sentinel)
        let bytes = hierarchy(
            w3d_make_version(4, 1),
            "FeRoot",
            &[(
                name16("B"),
                0xFFFF_FFFE,
                [1.0, 0.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            )],
        );
        assert_eq!(W3dHtree::parse(&bytes), Err(HtreeError::LoadFailed));
    }

    #[test]
    fn missing_pivots_chunk_leaves_the_default_state() {
        // the load succeeds; every pivot keeps its constructor state
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&W3D_CHUNK_HIERARCHY_HEADER.to_le_bytes());
        bytes.extend_from_slice(&HIERARCHY_HEADER_SIZE.to_le_bytes());
        bytes.extend_from_slice(&w3d_make_version(4, 1).to_le_bytes());
        let mut name_field = [0u8; 16];
        name_field[..8].copy_from_slice(b"NoPivots");
        bytes.extend_from_slice(&name_field);
        bytes.extend_from_slice(&3u32.to_le_bytes());
        for _ in 0..3 {
            bytes.extend_from_slice(&0u32.to_le_bytes());
        }
        let tree = W3dHtree::parse(&bytes).expect("the load succeeds");
        assert_eq!(tree.num_pivots(), 3);
        for i in 0..3 {
            let pivot = &tree.pivots[i];
            assert_eq!(pivot.name_str(), b"");
            assert_eq!(pivot.parent, None);
            assert_eq!(pivot.index, 0); // never written by a read
            assert!(pivot.is_visible);
            assert_eq!(pivot.base_transform, make_identity());
            assert_eq!(tree.get_parent_index(i), 0);
        }
    }

    #[test]
    fn unknown_chunks_are_skipped() {
        let mut bytes = hierarchy(
            w3d_make_version(4, 1),
            "Unknowns",
            &[(
                name16("R"),
                NO_PARENT,
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            )],
        );
        // a PIVOT_FIXUPS chunk and an unknown id after the pivots
        let mut tail = Vec::new();
        tail.extend_from_slice(&W3D_CHUNK_PIVOT_FIXUPS.to_le_bytes());
        tail.extend_from_slice(&8u32.to_le_bytes());
        tail.extend_from_slice(&[0u8; 8]);
        tail.extend_from_slice(&0x999u32.to_le_bytes());
        tail.extend_from_slice(&4u32.to_le_bytes());
        tail.extend_from_slice(&0xDEAD_BEEFu32.to_le_bytes());
        bytes.extend_from_slice(&tail);
        let tree = W3dHtree::parse(&bytes).expect("the load succeeds");
        assert_eq!(tree.num_pivots(), 1);
        assert_eq!(tree.pivots[0].name_str(), b"R");
    }

    #[test]
    fn double_pivots_chunk_rereads() {
        // the first read fills two pivots; the second overwrites both
        let mut bytes = hierarchy(
            w3d_make_version(4, 1),
            "Twice",
            &[
                (
                    name16("A"),
                    NO_PARENT,
                    [1.0, 0.0, 0.0],
                    [0.0, 0.0, 0.0, 1.0],
                ),
                (name16("B"), 0, [2.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0]),
            ],
        );
        let mut second = Vec::new();
        second.extend_from_slice(&W3D_CHUNK_PIVOTS.to_le_bytes());
        second.extend_from_slice(&120u32.to_le_bytes());
        second.extend_from_slice(&pivot_bytes(
            &name16("X"),
            NO_PARENT,
            [5.0, 5.0, 5.0],
            [0.5, 0.5, 0.5, 0.5],
        ));
        second.extend_from_slice(&pivot_bytes(
            &name16("Y"),
            0,
            [6.0, 6.0, 6.0],
            [0.1, 0.2, 0.3, 0.4],
        ));
        bytes.extend_from_slice(&second);
        let tree = W3dHtree::parse(&bytes).expect("the load succeeds");
        assert_eq!(tree.pivots[0].name_str(), b"X");
        assert_eq!(tree.pivots[0].base_transform[3], 5.0);
        assert_eq!(tree.pivots[1].name_str(), b"Y");
        assert_eq!(tree.pivots[1].parent, Some(0));
        assert_eq!(tree.pivots[1].base_transform[3], 6.0);
    }

    #[test]
    fn bone_lookup_is_case_insensitive_and_first_match() {
        let bytes = hierarchy(
            w3d_make_version(4, 1),
            "Case",
            &[
                (
                    name16("R"),
                    NO_PARENT,
                    [0.0, 0.0, 0.0],
                    [0.0, 0.0, 0.0, 1.0],
                ),
                (name16("TORSO"), 0, [1.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0]),
                (name16("UR_ARM"), 1, [0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]),
                (name16("torso"), 2, [0.0, 0.0, 1.0], [0.0, 0.0, 0.0, 1.0]),
                (name16("UR_ARM"), 3, [1.0, 1.0, 1.0], [0.0, 0.0, 0.0, 1.0]),
            ],
        );
        let tree = W3dHtree::parse(&bytes).expect("the tree loads");
        // exact matches
        assert_eq!(tree.get_bone_index(b"R"), 0);
        assert_eq!(tree.get_bone_index(b"TORSO"), 1);
        // case-insensitive match
        assert_eq!(tree.get_bone_index(b"torso"), 1);
        assert_eq!(tree.get_bone_index(b"tOrSo"), 1);
        // duplicates resolve to the first occurrence
        assert_eq!(tree.get_bone_index(b"UR_ARM"), 2);
        // misses fall back to the root, like the original
        assert_eq!(tree.get_bone_index(b"nomatch"), 0);
        assert_eq!(tree.get_bone_index(b""), 0);
        // the pre-3.0 synthesized root is findable by name
        let old = hierarchy(
            w3d_make_version(2, 1),
            "Old",
            &[(
                name16("A"),
                NO_PARENT,
                [1.0, 0.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            )],
        );
        let tree = W3dHtree::parse(&old).expect("the tree loads");
        assert_eq!(tree.get_bone_index(b"roottransform"), 0);
        assert_eq!(tree.get_bone_index(b"A"), 1);
    }

    #[test]
    fn parent_index_reports_the_field_and_zero_for_roots() {
        let bytes = hierarchy(
            w3d_make_version(4, 1),
            "Parents",
            &[
                (
                    name16("R"),
                    NO_PARENT,
                    [0.0, 0.0, 0.0],
                    [0.0, 0.0, 0.0, 1.0],
                ),
                (name16("A"), 0, [1.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0]),
                (name16("B"), 1, [2.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0]),
            ],
        );
        let tree = W3dHtree::parse(&bytes).expect("the tree loads");
        assert_eq!(tree.get_parent_index(0), 0); // root
        assert_eq!(tree.get_parent_index(1), 0);
        assert_eq!(tree.get_parent_index(2), 1);
        // a forward reference resolves by array position
        let bytes = hierarchy(
            w3d_make_version(4, 1),
            "Fwd",
            &[
                (
                    name16("R"),
                    NO_PARENT,
                    [0.0, 0.0, 0.0],
                    [0.0, 0.0, 0.0, 1.0],
                ),
                (name16("A"), 2, [1.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0]),
                (name16("B"), 0, [0.5, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0]),
            ],
        );
        let tree = W3dHtree::parse(&bytes).expect("the tree loads");
        assert_eq!(tree.get_parent_index(1), 2);
    }

    #[test]
    fn self_parenting_and_single_root_trees_load() {
        // a pivot whose parent is its own index
        let bytes = hierarchy(
            w3d_make_version(4, 1),
            "Self",
            &[
                (
                    name16("R"),
                    NO_PARENT,
                    [0.0, 0.0, 0.0],
                    [0.0, 0.0, 0.0, 1.0],
                ),
                (name16("Ouro"), 1, [1.0, 1.0, 1.0], [0.0, 0.0, 0.0, 1.0]),
            ],
        );
        let tree = W3dHtree::parse(&bytes).expect("the tree loads");
        assert_eq!(tree.pivots[1].parent, Some(1));
        assert_eq!(tree.get_parent_index(1), 1);
        // a single root pivot whose parent is 0 — itself
        let bytes = hierarchy(
            w3d_make_version(4, 1),
            "SelfRoot",
            &[(name16("Loop"), 0, [1.0, 2.0, 3.0], [0.0, 0.0, 0.0, 1.0])],
        );
        let tree = W3dHtree::parse(&bytes).expect("the tree loads");
        assert_eq!(tree.pivots[0].parent, Some(0));
    }

    #[test]
    fn non_nul_terminated_names_use_the_whole_field() {
        let mut name = [0u8; 16];
        name.copy_from_slice(b"1234567890abcdef");
        let bytes = hierarchy(
            w3d_make_version(4, 1),
            "Wide",
            &[(name, NO_PARENT, [1.0, 2.0, 3.0], [0.0, 0.0, 0.0, 1.0])],
        );
        let tree = W3dHtree::parse(&bytes).expect("the tree loads");
        assert_eq!(tree.pivots[0].name_str(), b"1234567890abcdef");
        assert_eq!(tree.get_bone_index(b"1234567890abcdef"), 0);
    }
}
