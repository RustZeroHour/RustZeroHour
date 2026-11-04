// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Ported from the Zero Hour source release (baseline 571a7c5):
//   GeneralsMD/Code/Libraries/Source/WWVegas/WW3D2/meshmdlio.cpp
//     (MeshModelClass::Load_W3D, read_chunks and every sub-reader,
//      MeshLoadContextClass, install_materials)
//   GeneralsMD/Code/Libraries/Source/WWVegas/WW3D2/meshgeometry.cpp
//     (MeshGeometryClass::read_vertices/read_vertex_normals/read_triangles/
//      read_user_text/read_vertex_influences/read_vertex_shade_indices)
//   Core/Libraries/Source/WWVegas/WW3D2/aabtree.cpp (AABTreeClass::Load_W3D)
//   GeneralsMD/Code/Libraries/Source/WWVegas/WW3D2/vertmaterial.cpp
//     (VertexMaterialClass::Load_W3D, Init_From_Material3,
//      Parse_W3dVertexMaterialStruct, Compute_CRC)
//   Core/Libraries/Source/WWVegas/WW3D2/texture.cpp (::Load_Texture, data path)
//   Core/Libraries/Source/WWVegas/WW3D2/shader.{h,cpp} + w3d_util.cpp
//     (ShaderClass bit packing, Convert_Shader, Init_From_Material3)
//   Core/Libraries/Source/WWVegas/WW3D2/meshmatdesc.{h,cpp}
//     (the load-path half of MeshMatDescClass: Install_UV_Array,
//      Init_Alternate, Is_Empty)
//   Core/Libraries/Source/WWVegas/WWLib/realcrc.cpp (CRC_Memory)
// See the module docs for the format layout and the documented differences
// from the C++ (render-side work deferred, release-UB turned into errors).

//! The **W3D mesh chunk tree** — the first real W3D model module on top of
//! the chunk container.
//!
//! A `.w3d` mesh asset is a [`W3D_CHUNK_MESH`] container whose children
//! describe one renderable mesh: a fixed 116-byte
//! [`mesh header`](MeshHeader3), geometry chunks (vertices, normals,
//! triangles, per-vertex influences/shade indices, user text), an
//! optional AABTree, and the material tree — shaders, vertex materials,
//! textures and material passes, optionally wrapped in one of the four
//! *prelit* chunk wrappers. This module ports
//! `MeshModelClass::Load_W3D` (meshmdlio.cpp) — the data path of the
//! original loader — so the structures produced here hold exactly what
//! the game's model holds after a load, quirks included (parity level
//! P0, pinned by golden vectors generated from the transcription in
//! `tools/differential/gen_mesh_golden.cpp`).
//!
//! [`W3D_CHUNK_MESH`]: W3D_CHUNK_MESH
//!
//! # The load-path state machine
//!
//! The original loads the material tree through a scratch *load context*
//! that survives the whole walk, and the quirks live there:
//!
//! - the **pass counter** (`CurPass`) advances once per material pass
//!   chunk across the whole file — prelit passes continue the top-level
//!   numbering;
//! - repeated id chunks inside one pass route the **second occurrence**
//!   into an *alternate* material description (the load context's
//!   `AlternateMatDesc`) whenever the default description already holds
//!   data for that pass/stage; the two are merged at the end of the load
//!   (`Init_Alternate`);
//! - the **prelit selection** (`WW3D::PrelitMode`, which the game never
//!   changes from `LIGHTMAP_MULTI_PASS`) picks at most one prelit wrapper
//!   to process; the others are skipped whole;
//! - **UV arrays are deduplicated by CRC-32** across passes and stages,
//!   with at most 8 slots; an install that finds all 8 occupied silently
//!   does nothing;
//! - a first **DCG** owns the (single) colour array; a second routes to
//!   the alternate, and **DIG** values multiply into whichever array they
//!   land on (the second DIG always routes to the alternate);
//! - `Load_W3D` **discards `read_chunks`' error return**: a failing
//!   sub-reader aborts the chunk walk — the remaining chunks are never
//!   opened — but the load still succeeds with the partial state.
//!
//! # Differences from the C++
//!
//! Release-build undefined behaviour becomes [`MeshError`] (the load
//! fails): geometry readers running without their allocated arrays,
//! shader/material/texture indices outside the loaded lists, pass/stage
//! counters past `MAX_PASSES`/`MAX_TEX_STAGES`, name chunks longer than
//! the original's 256-byte stack buffers, short AABTree reads, the
//! pre-3.0 skin fixup without a bone-link array, and the obsolete
//! top-level texcoords path when the chunk length disagrees with the
//! vertex count (the C++ installs uninitialised scratch there). Where
//! the C++ reads *uninitialised* state (`W3dVertexMaterialStruct` with
//! no INFO chunk, C-string buffers) the port reads zeroes. Sub-reader
//! failures that the original ignores keep their ignored semantics —
//! see the last bullet above.
//!
//! Render-side work is out of scope for the format layer (the plan's
//! Appendix A splits `WW3D2` into `rzh-formats` + `rzh-render`): texture
//! objects (names + the `W3dTextureInfoStruct` survive; the asset
//! manager's name-keyed identity is modelled so the legacy material
//! dedup keeps its behaviour), mapper instantiation from the ARGS0/ARGS1
//! buffers (kept raw), `post_process` (fog activation, static sort
//! levels, overbright), `MeshMatDescClass::Post_Load_Process` (the DX8
//! pipeline adaptation), and `Generate_Culling_Tree` for collideable
//! meshes without an AABTree chunk (deferred to the collision slice).
//!
//! # Examples
//!
//! ```
//! use rzh_formats::chunk::ChunkLoad;
//! use rzh_formats::w3d_mesh::{W3dMesh, W3D_CHUNK_MESH};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // A .w3d file holds named top-level assets; a mesh asset is a
//! // W3D_CHUNK_MESH container whose payload this parser consumes.
//! # let w3d_file: &[u8] = &[];
//! # let _ = w3d_file;
//! # let mesh_payload: Vec<u8> = Vec::new(); // the container's bytes
//! # if mesh_payload.is_empty() { return Ok(()); }
//! let mut load = ChunkLoad::new(&mesh_payload);
//! assert!(load.open_chunk()? && load.cur_chunk_id() == W3D_CHUNK_MESH);
//! let payload = &mesh_payload[8..8 + load.cur_chunk_length() as usize];
//! let mesh = W3dMesh::parse(payload)?;
//! # Ok(())
//! # }
//! ```

use std::fmt;

use crate::chunk::ChunkLoad;

// ---------------------------------------------------------------------------
// chunk ids (w3d_file.h / w3d_obsolete.h)
// ---------------------------------------------------------------------------

/// The mesh container a `.w3d` file wraps a mesh asset in.
pub const W3D_CHUNK_MESH: u32 = 0x0000_0000;
/// Array of `W3dVectorStruct` vertices.
pub const W3D_CHUNK_VERTICES: u32 = 0x0000_0002;
/// Array of vertex normals.
pub const W3D_CHUNK_VERTEX_NORMALS: u32 = 0x0000_0003;
/// Obsolete synonym of [`W3D_CHUNK_VERTEX_NORMALS`].
pub const W3D_CHUNK_SURRENDER_NORMALS: u32 = 0x0000_0004;
/// Obsolete top-level texture coordinates.
pub const W3D_CHUNK_TEXCOORDS: u32 = 0x0000_0005;
/// Obsolete material array (skipped).
pub const O_W3D_CHUNK_MATERIALS: u32 = 0x0000_0006;
/// Obsolete surrender-format triangles (skipped).
pub const O_W3D_CHUNK_SURRENDER_TRIANGLES: u32 = 0x0000_0009;
/// Preset vertex colouring (`W3dRGBStruct` per vertex).
pub const W3D_CHUNK_VERTEX_COLORS: u32 = 0x0000_000D;
/// Text from the MAX comment field.
pub const W3D_CHUNK_MESH_USER_TEXT: u32 = 0x0000_000C;
/// Per-vertex bone attachments (`W3dVertInfStruct`).
pub const W3D_CHUNK_VERTEX_INFLUENCES: u32 = 0x0000_000E;
/// Obsolete mesh damage data (skipped).
pub const W3D_CHUNK_DAMAGE: u32 = 0x0000_000F;
/// Obsolete version-2 material array (skipped).
pub const O_W3D_CHUNK_MATERIALS2: u32 = 0x0000_0014;
/// Obsolete version-3 material array (legacy materials).
pub const W3D_CHUNK_MATERIALS3: u32 = 0x0000_0015;
/// One legacy version-3 material.
pub const W3D_CHUNK_MATERIAL3: u32 = 0x0000_0016;
/// Legacy material name.
pub const W3D_CHUNK_MATERIAL3_NAME: u32 = 0x0000_0017;
/// Legacy `W3dMaterial3Struct`.
pub const W3D_CHUNK_MATERIAL3_INFO: u32 = 0x0000_0018;
/// Legacy diffuse colour map wrapper.
pub const W3D_CHUNK_MATERIAL3_DC_MAP: u32 = 0x0000_0019;
/// Legacy map filename.
pub const W3D_CHUNK_MAP3_FILENAME: u32 = 0x0000_001A;
/// Legacy `W3dMap3Struct`.
pub const W3D_CHUNK_MAP3_INFO: u32 = 0x0000_001B;
/// Legacy diffuse illumination map.
pub const W3D_CHUNK_MATERIAL3_DI_MAP: u32 = 0x0000_001C;
/// Legacy specular colour map.
pub const W3D_CHUNK_MATERIAL3_SC_MAP: u32 = 0x0000_001D;
/// Legacy specular illumination map.
pub const W3D_CHUNK_MATERIAL3_SI_MAP: u32 = 0x0000_001E;
/// The 116-byte mesh header.
pub const W3D_CHUNK_MESH_HEADER3: u32 = 0x0000_001F;
/// Array of `W3dTriStruct` triangles.
pub const W3D_CHUNK_TRIANGLES: u32 = 0x0000_0020;
/// Obsolete per-triangle material indices.
pub const W3D_CHUNK_PER_TRI_MATERIALS: u32 = 0x0000_0021;
/// Per-vertex shade indices.
pub const W3D_CHUNK_VERTEX_SHADE_INDICES: u32 = 0x0000_0022;
/// The unlit prelit wrapper.
pub const W3D_CHUNK_PRELIT_UNLIT: u32 = 0x0000_0023;
/// The vertex-lit prelit wrapper.
pub const W3D_CHUNK_PRELIT_VERTEX: u32 = 0x0000_0024;
/// The multi-pass lightmap prelit wrapper.
pub const W3D_CHUNK_PRELIT_LIGHTMAP_MULTI_PASS: u32 = 0x0000_0025;
/// The multi-texture lightmap prelit wrapper.
pub const W3D_CHUNK_PRELIT_LIGHTMAP_MULTI_TEXTURE: u32 = 0x0000_0026;
/// The `W3dMaterialInfoStruct`.
pub const W3D_CHUNK_MATERIAL_INFO: u32 = 0x0000_0028;
/// Array of `W3dShaderStruct` shaders.
pub const W3D_CHUNK_SHADERS: u32 = 0x0000_0029;
/// Wrapper for the vertex materials.
pub const W3D_CHUNK_VERTEX_MATERIALS: u32 = 0x0000_002A;
/// One vertex material.
pub const W3D_CHUNK_VERTEX_MATERIAL: u32 = 0x0000_002B;
/// Vertex material name.
pub const W3D_CHUNK_VERTEX_MATERIAL_NAME: u32 = 0x0000_002C;
/// The `W3dVertexMaterialStruct`.
pub const W3D_CHUNK_VERTEX_MATERIAL_INFO: u32 = 0x0000_002D;
/// Stage-0 mapper arguments (raw string).
pub const W3D_CHUNK_VERTEX_MAPPER_ARGS0: u32 = 0x0000_002E;
/// Stage-1 mapper arguments (raw string).
pub const W3D_CHUNK_VERTEX_MAPPER_ARGS1: u32 = 0x0000_002F;
/// Wrapper for the textures.
pub const W3D_CHUNK_TEXTURES: u32 = 0x0000_0030;
/// One texture.
pub const W3D_CHUNK_TEXTURE: u32 = 0x0000_0031;
/// Texture filename.
pub const W3D_CHUNK_TEXTURE_NAME: u32 = 0x0000_0032;
/// The `W3dTextureInfoStruct`.
pub const W3D_CHUNK_TEXTURE_INFO: u32 = 0x0000_0033;
/// One material pass.
pub const W3D_CHUNK_MATERIAL_PASS: u32 = 0x0000_0038;
/// Vertex material indices (single or per-vertex).
pub const W3D_CHUNK_VERTEX_MATERIAL_IDS: u32 = 0x0000_0039;
/// Shader indices (single or per-triangle).
pub const W3D_CHUNK_SHADER_IDS: u32 = 0x0000_003A;
/// Per-vertex diffuse colours.
pub const W3D_CHUNK_DCG: u32 = 0x0000_003B;
/// Per-vertex diffuse illumination.
pub const W3D_CHUNK_DIG: u32 = 0x0000_003C;
/// Per-vertex specular colours (a no-op since the DX8 port).
pub const W3D_CHUNK_SCG: u32 = 0x0000_003E;
/// One texture stage inside a pass.
pub const W3D_CHUNK_TEXTURE_STAGE: u32 = 0x0000_0048;
/// Texture indices (single or per-triangle).
pub const W3D_CHUNK_TEXTURE_IDS: u32 = 0x0000_0049;
/// Per-vertex texture coordinates.
pub const W3D_CHUNK_STAGE_TEXCOORDS: u32 = 0x0000_004A;
/// Per-face texture coordinate indices (validated, then skipped).
pub const W3D_CHUNK_PER_FACE_TEXCOORD_IDS: u32 = 0x0000_004B;
/// Obsolete deform data (skipped).
pub const W3D_CHUNK_DEFORM: u32 = 0x0000_0058;
/// The mesh's AABTree.
pub const W3D_CHUNK_AABTREE: u32 = 0x0000_0090;
/// The `W3dMeshAABTreeHeader`.
pub const W3D_CHUNK_AABTREE_HEADER: u32 = 0x0000_0091;
/// The polygon index array.
pub const W3D_CHUNK_AABTREE_POLYINDICES: u32 = 0x0000_0092;
/// The node array.
pub const W3D_CHUNK_AABTREE_NODES: u32 = 0x0000_0093;

// mesh header attribute flags (w3d_file.h)
/// Mask of the collision-type bits.
pub const W3D_MESH_FLAG_COLLISION_TYPE_MASK: u32 = 0x0000_0FF0;
/// Shift of the collision-type bits.
pub const W3D_MESH_FLAG_COLLISION_TYPE_SHIFT: u32 = 4;
/// The mesh renders both faces.
pub const W3D_MESH_FLAG_TWO_SIDED: u32 = 0x0000_2000;
/// Obsolete lightmapped flag (backwards compatibility).
pub const OBSOLETE_W3D_MESH_FLAG_LIGHTMAPPED: u32 = 0x0000_4000;
/// The mesh casts shadows.
pub const W3D_MESH_FLAG_CAST_SHADOW: u32 = 0x0000_8000;
/// Mask of the (4.1+) geometry-type bits.
pub const W3D_MESH_FLAG_GEOMETRY_TYPE_MASK: u32 = 0x00FF_0000;
/// Normal geometry.
pub const W3D_MESH_FLAG_GEOMETRY_TYPE_NORMAL: u32 = 0x0000_0000;
/// Camera-aligned geometry.
pub const W3D_MESH_FLAG_GEOMETRY_TYPE_CAMERA_ALIGNED: u32 = 0x0001_0000;
/// Skin geometry.
pub const W3D_MESH_FLAG_GEOMETRY_TYPE_SKIN: u32 = 0x0002_0000;
/// Camera-oriented geometry.
pub const W3D_MESH_FLAG_GEOMETRY_TYPE_CAMERA_ORIENTED: u32 = 0x0006_0000;
/// Mask of the prelit flags.
pub const W3D_MESH_FLAG_PRELIT_MASK: u32 = 0x0F00_0000;
/// The mesh contains an unlit material wrapper.
pub const W3D_MESH_FLAG_PRELIT_UNLIT: u32 = 0x0100_0000;
/// The mesh contains a vertex-lit material wrapper.
pub const W3D_MESH_FLAG_PRELIT_VERTEX: u32 = 0x0200_0000;
/// The mesh contains a multi-pass lightmap wrapper.
pub const W3D_MESH_FLAG_PRELIT_LIGHTMAP_MULTI_PASS: u32 = 0x0400_0000;
/// The mesh contains a multi-texture lightmap wrapper.
pub const W3D_MESH_FLAG_PRELIT_LIGHTMAP_MULTI_TEXTURE: u32 = 0x0800_0000;
/// The mesh may be NPatched.
pub const W3D_MESH_FLAG_NPATCHABLE: u32 = 0x2000_0000;

// geometry flags (meshgeometry.h FlagsType) — the load path's reachable subset
/// The mesh must be sorted (set by alpha-blended shaders).
pub const FG_SORT: u32 = 0x0000_0010;
/// Both faces render.
pub const FG_TWO_SIDED: u32 = 0x0000_0100;
/// The mesh aligns with the camera.
pub const FG_ALIGNED: u32 = 0x0000_0200;
/// The mesh is skinned.
pub const FG_SKIN: u32 = 0x0000_0400;
/// The mesh points towards the camera.
pub const FG_ORIENTED: u32 = 0x0000_0800;
/// The mesh casts shadows.
pub const FG_CAST_SHADOW: u32 = 0x0000_1000;
/// The mesh is prelit with vertex lighting.
pub const FG_PRELIT_VERTEX: u32 = 0x0000_2000;
/// The mesh is prelit with multi-pass lightmaps.
pub const FG_PRELIT_LIGHTMAP_MULTI_PASS: u32 = 0x0000_4000;
/// The mesh is prelit with multi-texture lightmaps.
pub const FG_PRELIT_LIGHTMAP_MULTI_TEXTURE: u32 = 0x0000_8000;
/// NPatching is allowed.
pub const FG_ALLOW_NPATCHES: u32 = 0x0001_0000;

// vertex material attributes (w3d_file.h)
/// `W3DVERTMAT_USE_DEPTH_CUE`.
pub const W3DVERTMAT_USE_DEPTH_CUE: u32 = 0x0000_0001;
/// `W3DVERTMAT_COPY_SPECULAR_TO_DIFFUSE`.
pub const W3DVERTMAT_COPY_SPECULAR_TO_DIFFUSE: u32 = 0x0000_0004;

// material3 attributes (w3d_obsolete.h)
/// `W3DMATERIAL_USE_ALPHA`.
pub const W3DMATERIAL_USE_ALPHA: u32 = 0x0000_0001;

/// `W3D_MAKE_VERSION(major, minor)`.
pub const fn w3d_make_version(major: u32, minor: u32) -> u32 {
    (major << 16) | minor
}

/// The material description's pass bound (`MeshMatDescClass::MAX_PASSES`).
pub const MAX_PASSES: usize = 4;
/// The material description's texture stage bound.
pub const MAX_TEX_STAGES: usize = 2;
/// The colour array bound.
pub const MAX_COLOR_ARRAYS: usize = 2;
/// The UV array bound (`MAX_PASSES * MAX_TEX_STAGES`).
pub const MAX_UV_ARRAYS: usize = MAX_PASSES * MAX_TEX_STAGES;

/// The size of the fixed mesh header struct.
pub const MESH_HEADER3_SIZE: u32 = 116;

// ---------------------------------------------------------------------------
// errors
// ---------------------------------------------------------------------------

/// Errors the mesh load path can produce.
///
/// The C++ release build turns most of these into undefined behaviour
/// (out-of-bounds reads, writes through null, uninitialised state); the
/// port states the contracts instead. Note the original's `Load_W3D`
/// *discards* `read_chunks`' error: sub-reader failures abort the chunk
/// walk but the load still succeeds with the partial state — only the
/// errors raised before or after the walk (a bad header, the pre-3.0
/// fixup) actually fail a load.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MeshError {
    /// `WW3D_ERROR_LOAD_FAILED` — the load failed outright (bad first
    /// chunk, short header, or one of the contract violations below).
    LoadFailed,
    /// A sub-reader failed (short read, out-of-range index, unallocated
    /// array, name chunk over 256 bytes, pass/stage out of bounds). The
    /// original ignores this inside the walk; the port surfaces it as the
    /// walk-aborting error it is.
    SubReadFailed,
    /// The chunk container itself errored (depth cap).
    Chunk(crate::chunk::ChunkError),
}

impl fmt::Display for MeshError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LoadFailed => write!(f, "W3D mesh load failed"),
            Self::SubReadFailed => write!(f, "W3D mesh sub-reader failed"),
            Self::Chunk(e) => write!(f, "W3D mesh chunk error: {e}"),
        }
    }
}

impl std::error::Error for MeshError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Chunk(e) => Some(e),
            _ => None,
        }
    }
}

impl From<crate::chunk::ChunkError> for MeshError {
    fn from(e: crate::chunk::ChunkError) -> Self {
        Self::Chunk(e)
    }
}

// ---------------------------------------------------------------------------
// the fixed structs
// ---------------------------------------------------------------------------

/// The 116-byte `W3dMeshHeader3Struct`, decoded.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeshHeader3 {
    /// File-format version (`W3D_MAKE_VERSION`).
    pub version: u32,
    /// The mesh attribute flags (`W3D_MESH_FLAG_*`).
    pub attributes: u32,
    /// The mesh name (a C string in a 16-byte field).
    pub mesh_name: [u8; 16],
    /// The container name (a C string in a 16-byte field).
    pub container_name: [u8; 16],
    /// Declared triangle count (drives the geometry allocations).
    pub num_tris: u32,
    /// Declared vertex count.
    pub num_vertices: u32,
    /// Declared legacy material count.
    pub num_materials: u32,
    /// Declared damage stage count.
    pub num_damage_stages: u32,
    /// Static sorting level.
    pub sort_level: i32,
    /// The Lightmap Tool version that prelit the mesh.
    pub prelit_version: u32,
    /// The single `FutureCounts` entry.
    pub future_counts: [u32; 1],
    /// Per-vertex channel bitmask.
    pub vertex_channels: u32,
    /// Per-face channel bitmask.
    pub face_channels: u32,
    /// Bounding box minimum.
    pub min: [f32; 3],
    /// Bounding box maximum.
    pub max: [f32; 3],
    /// Bounding sphere centre.
    pub sph_center: [f32; 3],
    /// Bounding sphere radius.
    pub sph_radius: f32,
}

impl MeshHeader3 {
    /// Decodes the struct from its 116 little-endian bytes.
    pub fn from_bytes(bytes: &[u8; 116]) -> Self {
        let u32at = |o: usize| u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
        let f32at = |o: usize| f32::from_bits(u32at(o));
        let mut mesh_name = [0u8; 16];
        mesh_name.copy_from_slice(&bytes[8..24]);
        let mut container_name = [0u8; 16];
        container_name.copy_from_slice(&bytes[24..40]);
        Self {
            version: u32at(0),
            attributes: u32at(4),
            mesh_name,
            container_name,
            num_tris: u32at(40),
            num_vertices: u32at(44),
            num_materials: u32at(48),
            num_damage_stages: u32at(52),
            sort_level: u32at(56) as i32,
            prelit_version: u32at(60),
            future_counts: [u32at(64)],
            vertex_channels: u32at(68),
            face_channels: u32at(72),
            min: [f32at(76), f32at(80), f32at(84)],
            max: [f32at(88), f32at(92), f32at(96)],
            sph_center: [f32at(100), f32at(104), f32at(108)],
            sph_radius: f32at(112),
        }
    }

    /// The C-string view of the mesh name field.
    pub fn mesh_name_str(&self) -> &[u8] {
        cstr(&self.mesh_name)
    }

    /// The C-string view of the container name field.
    pub fn container_name_str(&self) -> &[u8] {
        cstr(&self.container_name)
    }
}

/// The bytes of a fixed character field up to the first NUL.
fn cstr(field: &[u8]) -> &[u8] {
    let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    &field[..end]
}

/// The 16-byte `W3dShaderStruct`, decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct W3dShaderStruct {
    /// The depth comparison function.
    pub depth_compare: u8,
    /// The depth write mask.
    pub depth_mask: u8,
    /// The colour mask (obsolete on the wire).
    pub color_mask: u8,
    /// The destination blend function.
    pub dest_blend: u8,
    /// The fog function (obsolete on the wire).
    pub fog_func: u8,
    /// The primary gradient.
    pub pri_gradient: u8,
    /// The secondary gradient.
    pub sec_gradient: u8,
    /// The source blend function.
    pub src_blend: u8,
    /// The texturing enable.
    pub texturing: u8,
    /// The detail colour function.
    pub detail_color_func: u8,
    /// The detail alpha function.
    pub detail_alpha_func: u8,
    /// The shader preset (obsolete on the wire).
    pub shader_preset: u8,
    /// The alpha test enable.
    pub alpha_test: u8,
    /// The post-detail colour function (copied from detail on conversion).
    pub post_detail_color_func: u8,
    /// The post-detail alpha function (copied from detail on conversion).
    pub post_detail_alpha_func: u8,
    /// The pad byte.
    pub pad: u8,
}

impl W3dShaderStruct {
    /// Decodes the struct from its 16 bytes.
    pub fn from_bytes(bytes: &[u8; 16]) -> Self {
        Self {
            depth_compare: bytes[0],
            depth_mask: bytes[1],
            color_mask: bytes[2],
            dest_blend: bytes[3],
            fog_func: bytes[4],
            pri_gradient: bytes[5],
            sec_gradient: bytes[6],
            src_blend: bytes[7],
            texturing: bytes[8],
            detail_color_func: bytes[9],
            detail_alpha_func: bytes[10],
            shader_preset: bytes[11],
            alpha_test: bytes[12],
            post_detail_color_func: bytes[13],
            post_detail_alpha_func: bytes[14],
            pad: bytes[15],
        }
    }
}

// The ShaderClass bit fields (shader.h): the shifts and masks.
const SHIFT_DEPTHCOMPARE: u32 = 0;
const SHIFT_DEPTHMASK: u32 = 3;
const SHIFT_COLORMASK: u32 = 4;
const SHIFT_DSTBLEND: u32 = 5;
const SHIFT_PRIGRADIENT: u32 = 10;
const SHIFT_SECGRADIENT: u32 = 13;
const SHIFT_SRCBLEND: u32 = 14;
const SHIFT_TEXTURING: u32 = 16;
const SHIFT_ALPHATEST: u32 = 18;
const SHIFT_CULLMODE: u32 = 19;
const SHIFT_POSTDETAILCOLORFUNC: u32 = 20;
const SHIFT_POSTDETAILALPHAFUNC: u32 = 24;

/// `ShaderClass::Reset()` — the preset a default-constructed shader holds
/// and every W3D conversion starts from.
pub const SHADER_DEFAULT_BITS: u32 = (3 << SHIFT_DEPTHCOMPARE)   // PASS_LEQUAL
    | (1 << SHIFT_DEPTHMASK)      // DEPTH_WRITE_ENABLE
    | (1 << SHIFT_COLORMASK)      // COLOR_WRITE_ENABLE
    | (1 << SHIFT_PRIGRADIENT)    // GRADIENT_MODULATE
    | (1 << SHIFT_SRCBLEND)       // SRCBLEND_ONE
    | (1 << SHIFT_CULLMODE); // CULL_MODE_ENABLE

/// Packs the raw W3D shader fields into the `ShaderClass` bit pattern
/// (`W3dUtilityClass::Convert_Shader`).
///
/// The colour mask is forced on, fog forced off, and the cull-mode /
/// npatch bits keep the default-constructed preset's values — exactly
/// the fields the original's conversion touches.
pub fn convert_shader(shader: &W3dShaderStruct) -> u32 {
    let set = set_shader_field;
    let mut bits = SHADER_DEFAULT_BITS;
    bits = set(bits, u32::from(shader.depth_compare), SHIFT_DEPTHCOMPARE, 3);
    bits = set(bits, u32::from(shader.depth_mask), SHIFT_DEPTHMASK, 1);
    bits = set(bits, 1, SHIFT_COLORMASK, 1); // COLOR_WRITE_ENABLE
    bits = set(bits, u32::from(shader.dest_blend), SHIFT_DSTBLEND, 3);
    bits = set(bits, 0, 8, 2); // FOG_DISABLE
    bits = set(bits, u32::from(shader.pri_gradient), SHIFT_PRIGRADIENT, 3);
    bits = set(bits, u32::from(shader.sec_gradient), SHIFT_SECGRADIENT, 1);
    bits = set(bits, u32::from(shader.src_blend), SHIFT_SRCBLEND, 2);
    bits = set(bits, u32::from(shader.texturing), SHIFT_TEXTURING, 1);
    bits = set(bits, u32::from(shader.alpha_test), SHIFT_ALPHATEST, 1);
    bits = set(
        bits,
        u32::from(shader.detail_color_func),
        SHIFT_POSTDETAILCOLORFUNC,
        4,
    );
    bits = set(
        bits,
        u32::from(shader.detail_alpha_func),
        SHIFT_POSTDETAILALPHAFUNC,
        3,
    );
    bits
}

/// Writes one `ShaderClass` bit field (the `Set_*` accessors' shared shape).
fn set_shader_field(bits: u32, val: u32, shift: u32, width: u32) -> u32 {
    let mask = ((1u32 << width) - 1) << shift;
    (bits & !mask) | ((val << shift) & mask)
}

/// The destination blend field of a packed shader (`Get_Dst_Blend_Func`).
pub fn shader_dst_blend(bits: u32) -> u32 {
    (bits >> SHIFT_DSTBLEND) & 7
}

/// The alpha test field of a packed shader (`Get_Alpha_Test`).
pub fn shader_alpha_test(bits: u32) -> u32 {
    (bits >> SHIFT_ALPHATEST) & 1
}

/// The texturing field of a packed shader (`Get_Texturing`).
pub fn shader_texturing(bits: u32) -> u32 {
    (bits >> SHIFT_TEXTURING) & 1
}

// ---------------------------------------------------------------------------
// the colour conversions (w3d_util.cpp + Vector3::Convert_To_ARGB)
// ---------------------------------------------------------------------------

/// `W3dUtilityClass::Convert_Color(W3dRGBStruct, Vector3)`.
fn convert_rgb(rgb: [u8; 4]) -> [f32; 3] {
    [
        f32::from(rgb[0]) / 255.0,
        f32::from(rgb[1]) / 255.0,
        f32::from(rgb[2]) / 255.0,
    ]
}

/// `DX8Wrapper::Convert_Color(Vector4)` — `Vector3::Convert_To_ARGB`:
/// `(a<<24) | (r<<16) | (g<<8) | b` with the float→unsigned truncation
/// the callers' in-range values define.
fn convert_color_argb(v: &[f32; 4]) -> u32 {
    let a = (v[3] * 255.0) as u32;
    let r = (v[0] * 255.0) as u32;
    let g = (v[1] * 255.0) as u32;
    let b = (v[2] * 255.0) as u32;
    (a << 24) | (r << 16) | (g << 8) | b
}

/// `DX8Wrapper::Convert_Color(unsigned)` — the unpack direction.
fn convert_argb_to_vector4(color: u32) -> [f32; 4] {
    [
        ((color & 0xff_0000) >> 16) as f32 / 255.0,
        ((color & 0xff00) >> 8) as f32 / 255.0,
        (color & 0xff) as f32 / 255.0,
        ((color & 0xff00_0000) >> 24) as f32 / 255.0,
    ]
}

// ---------------------------------------------------------------------------
// the WWLib CRC-32 (realcrc.cpp)
// ---------------------------------------------------------------------------

/// The reflected CRC-32 table realcrc.cpp embeds (poly `0x04C11DB7`),
/// generated — verified entry-for-entry against the original at the
/// baseline tag.
const CRC32_TABLE: [u32; 256] = build_crc32_table();

/// Builds `CRC32_Table`.
const fn build_crc32_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                (c >> 1) ^ 0xEDB8_8320
            } else {
                c >> 1
            };
            k += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
}

/// `CRC_Memory` — the chained WWLib CRC-32 update.
fn crc_memory(data: &[u8], crc: u32) -> u32 {
    let mut crc = crc ^ 0xFFFF_FFFF;
    for &byte in data {
        crc = CRC32_TABLE[((crc ^ u32::from(byte)) & 0xFF) as usize] ^ ((crc >> 8) & 0x00FF_FFFF);
    }
    crc ^ 0xFFFF_FFFF
}

/// The CRC of a UV array's bytes (the `Vector2` pairs as stored).
fn uv_array_crc(xy: &[[f32; 2]]) -> u32 {
    let mut bytes = Vec::with_capacity(xy.len() * 8);
    for pair in xy {
        bytes.extend_from_slice(&pair[0].to_bits().to_le_bytes());
        bytes.extend_from_slice(&pair[1].to_bits().to_le_bytes());
    }
    crc_memory(&bytes, 0)
}

// ---------------------------------------------------------------------------
// the vertex material (vertmaterial.cpp, load-path state)
// ---------------------------------------------------------------------------

/// A vertex material as the load path leaves it — the `D3DMATERIAL8`
/// state (the colour vectors, shininess and opacity), the flag bits the
/// parse kept, the name, and the raw mapper argument buffers.
///
/// The mapper *instantiation* the original performs on the argument
/// buffers is render-side; the format layer keeps the bytes.
#[derive(Debug, Clone, PartialEq)]
pub struct VertexMaterial {
    /// The ambient colour (`Material->Ambient.rgb`).
    pub ambient: [f32; 3],
    /// The diffuse colour (`Material->Diffuse.rgb`).
    pub diffuse: [f32; 3],
    /// The specular colour (`Material->Specular.rgb`).
    pub specular: [f32; 3],
    /// The emissive colour (`Material->Emissive.rgb`).
    pub emissive: [f32; 3],
    /// The shininess (`Material->Power`).
    pub shininess: f32,
    /// The opacity (`Material->Diffuse.a`).
    pub opacity: f32,
    /// The `FlagsType` bits set during the load.
    pub flags: u32,
    /// The material's name (empty when no name chunk was read).
    pub name: String,
    /// The stage-0 mapper arguments, raw (empty when absent).
    pub mapper_args0: Vec<u8>,
    /// The stage-1 mapper arguments, raw (empty when absent).
    pub mapper_args1: Vec<u8>,
}

impl Default for VertexMaterial {
    /// The `VertexMaterialClass` constructor's state: ambient and diffuse
    /// white, opacity 1, everything else zero.
    fn default() -> Self {
        Self {
            ambient: [1.0, 1.0, 1.0],
            diffuse: [1.0, 1.0, 1.0],
            specular: [0.0, 0.0, 0.0],
            emissive: [0.0, 0.0, 0.0],
            shininess: 0.0,
            opacity: 1.0,
            flags: 0,
            name: String::new(),
            mapper_args0: Vec::new(),
            mapper_args1: Vec::new(),
        }
    }
}

impl VertexMaterial {
    /// `VertexMaterialClass::Compute_CRC`, restricted to the state a mesh
    /// load can set: the `D3DMATERIAL8` bytes (diffuse, ambient,
    /// specular, emissive — each `r g b a` — then power), the flags, and
    /// the constants (colour sources `MATERIAL`, `UVSource` `[0, 1]`,
    /// lighting off, `UniqueID` 0, null mappers — the original's
    /// conditional skips those). Only the legacy material path consults
    /// it, and legacy materials never carry mappers.
    pub fn compute_crc(&self) -> u32 {
        let mut bytes = Vec::with_capacity(97);
        let push_f32 = |bytes: &mut Vec<u8>, f: f32| {
            bytes.extend_from_slice(&f.to_bits().to_le_bytes());
        };
        // diffuse: r, g, b, a — the alpha lane is the opacity
        for f in self.diffuse.iter().copied().chain([self.opacity]) {
            push_f32(&mut bytes, f);
        }
        // ambient, specular, emissive: r, g, b, 0
        for f in self.ambient.iter().copied().chain([0.0]) {
            push_f32(&mut bytes, f);
        }
        for f in self.specular.iter().copied().chain([0.0]) {
            push_f32(&mut bytes, f);
        }
        for f in self.emissive.iter().copied().chain([0.0]) {
            push_f32(&mut bytes, f);
        }
        // power
        push_f32(&mut bytes, self.shininess);
        // flags, colour sources, uv sources, lighting, unique id
        bytes.extend_from_slice(&self.flags.to_le_bytes());
        for word in [0u32; 3] {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        for word in [0u32, 1u32] {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        bytes.push(0);
        bytes.extend_from_slice(&0u32.to_le_bytes());
        crc_memory(&bytes, 0)
    }
}

/// `VertexMaterialClass::Parse_W3dVertexMaterialStruct`.
fn parse_w3d_vertex_material_struct(vmat: &W3dVertexMaterialInfo) -> VertexMaterial {
    let mut flags = 0;
    if vmat.attributes & W3DVERTMAT_USE_DEPTH_CUE != 0 {
        flags |= 1; // VertexMaterialClass::DEPTH_CUE
    }
    if vmat.attributes & W3DVERTMAT_COPY_SPECULAR_TO_DIFFUSE != 0 {
        flags |= 4; // VertexMaterialClass::COPY_SPECULAR_TO_DIFFUSE
    }
    VertexMaterial {
        ambient: convert_rgb(vmat.ambient),
        diffuse: convert_rgb(vmat.diffuse),
        specular: convert_rgb(vmat.specular),
        emissive: convert_rgb(vmat.emissive),
        shininess: vmat.shininess,
        opacity: vmat.opacity,
        flags,
        ..VertexMaterial::default()
    }
}

/// `VertexMaterialClass::Init_From_Material3` (the legacy path).
fn init_from_material3(mat3: &W3dMaterial3Struct) -> VertexMaterial {
    let mut m = VertexMaterial::default();
    let dc = convert_rgb(mat3.diffuse_color);
    let dfc = convert_rgb(mat3.diffuse_coefficients);
    m.diffuse = [dc[0] * dfc[0], dc[1] * dfc[1], dc[2] * dfc[2]];
    let sc = convert_rgb(mat3.specular_color);
    let sfc = convert_rgb(mat3.specular_coefficients);
    m.specular = [sc[0] * sfc[0], sc[1] * sfc[1], sc[2] * sfc[2]];
    m.emissive = convert_rgb(mat3.emissive_coefficients);
    m.ambient = convert_rgb(mat3.ambient_coefficients);
    m.shininess = mat3.shininess;
    m.opacity = mat3.opacity;
    m
}

/// The 32-byte `W3dVertexMaterialStruct`, decoded.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct W3dVertexMaterialInfo {
    /// The attribute flags (`W3DVERTMAT_*` and the stage mapping bits).
    pub attributes: u32,
    /// The ambient colour (RGB + pad).
    pub ambient: [u8; 4],
    /// The diffuse colour (RGB + pad).
    pub diffuse: [u8; 4],
    /// The specular colour (RGB + pad).
    pub specular: [u8; 4],
    /// The emissive colour (RGB + pad).
    pub emissive: [u8; 4],
    /// The shininess.
    pub shininess: f32,
    /// The opacity.
    pub opacity: f32,
    /// The translucency (dropped by the parse — `D3DMATERIAL8` has no
    /// field for it).
    pub translucency: f32,
}

impl W3dVertexMaterialInfo {
    /// Decodes the struct from its 32 bytes.
    pub fn from_bytes(bytes: &[u8; 32]) -> Self {
        let u32at = |o: usize| u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
        let f32at = |o: usize| f32::from_bits(u32at(o));
        Self {
            attributes: u32at(0),
            ambient: [bytes[4], bytes[5], bytes[6], bytes[7]],
            diffuse: [bytes[8], bytes[9], bytes[10], bytes[11]],
            specular: [bytes[12], bytes[13], bytes[14], bytes[15]],
            emissive: [bytes[16], bytes[17], bytes[18], bytes[19]],
            shininess: f32at(20),
            opacity: f32at(24),
            translucency: f32at(28),
        }
    }
}

// ---------------------------------------------------------------------------
// textures (texture.cpp, the data path)
// ---------------------------------------------------------------------------

/// The 12-byte `W3dTextureInfoStruct`, decoded.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct W3dTextureInfo {
    /// The texture attribute flags (`W3DTEXTURE_*`).
    pub attributes: u16,
    /// The animation type.
    pub anim_type: u16,
    /// The frame count (1 when not animated).
    pub frame_count: u32,
    /// The frame rate.
    pub frame_rate: f32,
}

impl W3dTextureInfo {
    /// Decodes the struct from its 12 bytes.
    pub fn from_bytes(bytes: &[u8; 12]) -> Self {
        Self {
            attributes: u16::from_le_bytes([bytes[0], bytes[1]]),
            anim_type: u16::from_le_bytes([bytes[2], bytes[3]]),
            frame_count: u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            frame_rate: f32::from_bits(u32::from_le_bytes(bytes[8..12].try_into().unwrap())),
        }
    }

    /// The struct's 12 bytes (round-trips [`from_bytes`](Self::from_bytes)).
    pub fn to_bytes(self) -> [u8; 12] {
        let mut bytes = [0u8; 12];
        bytes[0..2].copy_from_slice(&self.attributes.to_le_bytes());
        bytes[2..4].copy_from_slice(&self.anim_type.to_le_bytes());
        bytes[4..8].copy_from_slice(&self.frame_count.to_le_bytes());
        bytes[8..12].copy_from_slice(&self.frame_rate.to_bits().to_le_bytes());
        bytes
    }
}

/// A texture the mesh references — the object's name (lowercased, the
/// asset manager's identity) and the info struct it was created from,
/// when it was. Render-side state (mips, addressing, the surface) is out
/// of scope.
#[derive(Debug, Clone, PartialEq)]
pub struct TextureEntry {
    /// The texture filename, lowercased as the asset manager stores it.
    pub name: String,
    /// The `W3dTextureInfoStruct` the object was created from (absent for
    /// objects a legacy material map created).
    pub info: Option<W3dTextureInfo>,
}

// ---------------------------------------------------------------------------
// the material description (meshmatdesc.h / meshmatdesc.cpp, load path)
// ---------------------------------------------------------------------------

/// One UV array with the CRC `Install_UV_Array` deduplicates on
/// (`UVBufferClass` + `Update_CRC`).
#[derive(Debug, Clone, PartialEq)]
pub struct UvArray {
    /// The `(u, v)` pairs as installed (V already flipped).
    pub xy: Vec<[f32; 2]>,
    /// The CRC of the array's bytes.
    pub crc: u32,
}

/// A material/texture/shader list index ("pointer") — `-1` is the
/// original's null.
pub type Ref = i32;

/// The load-path half of `MeshMatDescClass`: per-pass single-or-array
/// material, shader and texture bindings, the colour sources, the colour
/// arrays and the CRC-deduplicated UV arrays.
///
/// The original's *pointers* are list indices here; the alternate
/// description the original merges by reference (`Init_Alternate`) is
/// merged by value — identical contents, single ownership. The default
/// material the original *clones* while merging ("Don't share vertex
/// materials!") is recorded as the same index it copies from; nothing in
/// the ported scope can tell the clone apart.
#[derive(Debug, Clone, PartialEq)]
pub struct MatDesc {
    /// The pass count (starts at 1; the material info chunk overwrites
    /// it, possibly beyond `MAX_PASSES` — the original stores whatever
    /// the chunk said).
    pub pass_count: i32,
    /// The vertex count the arrays are sized against.
    pub vertex_count: usize,
    /// The polygon count the arrays are sized against.
    pub poly_count: usize,
    /// The two colour arrays (packed ARGB, one entry per vertex).
    pub color_arrays: [Option<Vec<u32>>; MAX_COLOR_ARRAYS],
    /// Up to 8 UV arrays.
    pub uv_arrays: [Option<UvArray>; MAX_UV_ARRAYS],
    /// Per (pass, stage) UV array index, `-1` when absent.
    pub uv_source: [[Ref; MAX_TEX_STAGES]; MAX_PASSES],
    /// Per-pass diffuse colour source (`0` material, `1` colour 1,
    /// `2` colour 2).
    pub dcg_source: [u32; MAX_PASSES],
    /// Per-pass diffuse illumination source.
    pub dig_source: [u32; MAX_PASSES],
    /// Per-pass single vertex material index.
    pub material_single: [Ref; MAX_PASSES],
    /// Per-pass per-vertex vertex material indices.
    pub material_arrays: [Option<Vec<Ref>>; MAX_PASSES],
    /// Per-pass single shader bits (`0` = no data, `NullShader`).
    pub shader_single: [u32; MAX_PASSES],
    /// Per-pass per-polygon shader bits.
    pub shader_arrays: [Option<Vec<u32>>; MAX_PASSES],
    /// Per (pass, stage) single texture index.
    pub texture_single: [[Ref; MAX_TEX_STAGES]; MAX_PASSES],
    /// Per (pass, stage) per-polygon texture indices.
    pub texture_arrays: [[Option<Vec<Ref>>; MAX_TEX_STAGES]; MAX_PASSES],
}

impl Default for MatDesc {
    /// The `MeshMatDescClass` constructor's state.
    fn default() -> Self {
        Self {
            pass_count: 1,
            vertex_count: 0,
            poly_count: 0,
            color_arrays: [None, None],
            uv_arrays: [None, None, None, None, None, None, None, None],
            uv_source: [[-1; MAX_TEX_STAGES]; MAX_PASSES],
            dcg_source: [CS_MATERIAL; MAX_PASSES],
            dig_source: [CS_MATERIAL; MAX_PASSES],
            material_single: [-1; MAX_PASSES],
            material_arrays: [None, None, None, None],
            shader_single: [0; MAX_PASSES],
            shader_arrays: [None, None, None, None],
            texture_single: [[-1; MAX_TEX_STAGES]; MAX_PASSES],
            texture_arrays: std::array::from_fn(|_| [const { None }; MAX_TEX_STAGES]),
        }
    }
}

/// `VertexMaterialClass::MATERIAL` — the colour comes from the material.
pub const CS_MATERIAL: u32 = 0;
/// `VertexMaterialClass::COLOR1` — the colour comes from array 1.
pub const CS_COLOR1: u32 = 1;
/// `VertexMaterialClass::COLOR2` — the colour comes from array 2.
pub const CS_COLOR2: u32 = 2;

impl MatDesc {
    /// `MeshMatDescClass::Reset`.
    pub fn reset(&mut self, poly_count: usize, vertex_count: usize, pass_count: i32) {
        *self = Self::default();
        self.poly_count = poly_count;
        self.vertex_count = vertex_count;
        self.pass_count = pass_count;
    }

    /// `Get_UV_Array_Count` — the leading run of installed arrays.
    pub fn uv_array_count(&self) -> usize {
        let mut count = 0;
        while count < MAX_UV_ARRAYS && self.uv_arrays[count].is_some() {
            count += 1;
        }
        count
    }

    /// `Has_Color_Array`.
    pub fn has_color_array(&self, array: usize) -> bool {
        self.color_arrays[array].is_some()
    }

    /// `Has_UV`.
    pub fn has_uv(&self, pass: usize, stage: usize) -> bool {
        self.uv_source[pass][stage] != -1
    }

    /// `Has_Texture_Data`.
    pub fn has_texture_data(&self, pass: usize, stage: usize) -> bool {
        self.texture_single[pass][stage] != -1 || self.texture_arrays[pass][stage].is_some()
    }

    /// `Has_Shader_Data` (`NullShader` = bits 0 marks no data).
    pub fn has_shader_data(&self, pass: usize) -> bool {
        self.shader_single[pass] != 0 || self.shader_arrays[pass].is_some()
    }

    /// `Has_Material_Data`.
    pub fn has_material_data(&self, pass: usize) -> bool {
        self.material_single[pass] != -1 || self.material_arrays[pass].is_some()
    }

    /// `Get_Color_Array(index, create)` — zero-filled on creation.
    fn color_array_mut(&mut self, index: usize) -> &mut Vec<u32> {
        self.color_arrays[index].get_or_insert_with(|| vec![0u32; self.vertex_count])
    }

    /// `Set_Single_Material`.
    pub fn set_single_material(&mut self, vmat: Ref, pass: usize) {
        self.material_single[pass] = vmat;
    }

    /// `Set_Material` (the per-vertex buffer starts nulled).
    pub fn set_material(&mut self, vidx: usize, vmat: Ref, pass: usize) {
        let count = self.vertex_count;
        let array = self.material_arrays[pass].get_or_insert_with(|| vec![-1 as Ref; count]);
        array[vidx] = vmat;
    }

    /// `Set_Single_Shader`.
    pub fn set_single_shader(&mut self, bits: u32, pass: usize) {
        self.shader_single[pass] = bits;
    }

    /// `Set_Shader`.
    pub fn set_shader(&mut self, pidx: usize, bits: u32, pass: usize) {
        let count = self.poly_count;
        let array = self.shader_arrays[pass].get_or_insert_with(|| vec![0u32; count]);
        array[pidx] = bits;
    }

    /// `Set_Single_Texture`.
    pub fn set_single_texture(&mut self, tex: Ref, pass: usize, stage: usize) {
        self.texture_single[pass][stage] = tex;
    }

    /// `Set_Texture` (the per-polygon buffer starts nulled — the
    /// `0xffffffff` texture-id skip leaves holes).
    pub fn set_texture(&mut self, pidx: usize, tex: Ref, pass: usize, stage: usize) {
        let count = self.poly_count;
        let array = self.texture_arrays[pass][stage].get_or_insert_with(|| vec![-1 as Ref; count]);
        array[pidx] = tex;
    }

    /// `Install_UV_Array` — CRC dedup against the installed arrays, then
    /// the first empty slot. When all 8 slots are taken the install
    /// silently does nothing (the original's slot search reads past the
    /// array and its bound check discards the value).
    pub fn install_uv_array(&mut self, pass: usize, stage: usize, xy: &[[f32; 2]]) {
        let crc = uv_array_crc(xy);
        let uv_count = self.uv_array_count();
        for i in 0..uv_count {
            if let Some(existing) = &self.uv_arrays[i]
                && existing.crc == crc
            {
                self.uv_source[pass][stage] = i as Ref;
                return;
            }
        }
        let mut new_index = 0;
        while new_index < MAX_UV_ARRAYS && self.uv_arrays[new_index].is_some() {
            new_index += 1;
        }
        if new_index < MAX_UV_ARRAYS {
            self.uv_arrays[new_index] = Some(UvArray {
                xy: xy.to_vec(),
                crc,
            });
            self.uv_source[pass][stage] = new_index as Ref;
        }
    }

    /// `Is_Empty` — note it does **not** look at the shader data: an
    /// alternate holding only shaders counts as empty (pinned by the
    /// goldens).
    pub fn is_empty(&self) -> bool {
        for array in &self.color_arrays {
            if array.is_some() {
                return false;
            }
        }
        for uvarray in &self.uv_arrays {
            if uvarray.is_some() {
                return false;
            }
        }
        for pass in 0..MAX_PASSES {
            for stage in 0..MAX_TEX_STAGES {
                if self.texture_single[pass][stage] != -1
                    || self.texture_arrays[pass][stage].is_some()
                {
                    return false;
                }
            }
            if self.material_single[pass] != -1 || self.material_arrays[pass].is_some() {
                return false;
            }
        }
        true
    }

    /// `Init_Alternate` — the merge of the default and the load context's
    /// alternate description into the mesh's alternate. Deep-copies the
    /// shared arrays (the dump values are identical; the ownership is
    /// single).
    pub fn init_alternate(&mut self, default_materials: &MatDesc, alternate_materials: &MatDesc) {
        self.pass_count = default_materials.pass_count;
        self.vertex_count = default_materials.vertex_count;
        self.poly_count = default_materials.poly_count;

        for array in 0..MAX_COLOR_ARRAYS {
            self.color_arrays[array] = match (
                &alternate_materials.color_arrays[array],
                &default_materials.color_arrays[array],
            ) {
                (Some(alt), _) => Some(alt.clone()),
                (None, Some(def)) => Some(def.clone()),
                (None, None) => None,
            };
        }

        let alt_count = alternate_materials.uv_array_count();
        for i in 0..alt_count {
            self.uv_arrays[i] = alternate_materials.uv_arrays[i].clone();
        }

        for pass in 0..MAX_PASSES {
            for stage in 0..MAX_TEX_STAGES {
                if alternate_materials.uv_source[pass][stage] == -1
                    && default_materials.uv_source[pass][stage] != -1
                {
                    // bring the default's UV array over, deduplicating by CRC
                    let default_uv_source = default_materials.uv_source[pass][stage] as usize;
                    let uvarray = default_materials.uv_arrays[default_uv_source]
                        .as_ref()
                        .unwrap();
                    let crc = uvarray.crc;
                    let have = self.uv_array_count();
                    let mut found_index: Option<usize> = None;
                    for i in 0..have {
                        if let Some(existing) = &self.uv_arrays[i]
                            && existing.crc == crc
                        {
                            found_index = Some(i);
                            break;
                        }
                    }
                    match found_index {
                        Some(i) => self.uv_source[pass][stage] = i as Ref,
                        None => {
                            let new_index = self.uv_array_count();
                            self.uv_arrays[new_index] = Some(uvarray.clone());
                            self.uv_source[pass][stage] = new_index as Ref;
                        }
                    }
                } else if alternate_materials.uv_source[pass][stage] != -1 {
                    self.uv_source[pass][stage] = alternate_materials.uv_source[pass][stage];
                }

                if alternate_materials.texture_single[pass][stage] != -1
                    || alternate_materials.texture_arrays[pass][stage].is_some()
                {
                    self.texture_single[pass][stage] =
                        alternate_materials.texture_single[pass][stage];
                    self.texture_arrays[pass][stage] =
                        alternate_materials.texture_arrays[pass][stage].clone();
                } else {
                    self.texture_single[pass][stage] =
                        default_materials.texture_single[pass][stage];
                    self.texture_arrays[pass][stage] =
                        default_materials.texture_arrays[pass][stage].clone();
                }
            }

            // vertex colour configuration
            self.dcg_source[pass] = if alternate_materials.dcg_source[pass] == CS_MATERIAL {
                default_materials.dcg_source[pass]
            } else {
                alternate_materials.dcg_source[pass]
            };

            // shaders: "Can't override the shader for now" — always the
            // default's
            self.shader_single[pass] = default_materials.shader_single[pass];
            self.shader_arrays[pass] = default_materials.shader_arrays[pass].clone();

            // vertex materials
            if alternate_materials.material_single[pass] != -1
                || alternate_materials.material_arrays[pass].is_some()
            {
                self.material_single[pass] = alternate_materials.material_single[pass];
                self.material_arrays[pass] = alternate_materials.material_arrays[pass].clone();
            } else {
                // "Don't share vertex materials!" — the original clones the
                // default material; the index is what the port records
                self.material_single[pass] = default_materials.material_single[pass];
                self.material_arrays[pass] = default_materials.material_arrays[pass].clone();
            }
        }
    }
}

// ---------------------------------------------------------------------------
// the AABTree (aabtree.cpp)
// ---------------------------------------------------------------------------

/// One `W3dMeshAABTreeNode`: a box plus either child indices (an inner
/// node) or, when the MSB of `front_or_poly0` is set, the leaf's
/// `poly0` and polygon count.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AabNode {
    /// The box minimum.
    pub min: [f32; 3],
    /// The box maximum.
    pub max: [f32; 3],
    /// The front child, or the leaf's first polygon (MSB set).
    pub front_or_poly0: u32,
    /// The back child, or the leaf's polygon count.
    pub back_or_poly_count: u32,
}

/// The mesh's AABTree, exactly as `AABTreeClass::Load_W3D` stores it.
#[derive(Debug, Clone, PartialEq)]
pub struct AabTree {
    /// The declared node count.
    pub node_count: u32,
    /// The declared polygon index count.
    pub poly_count: u32,
    /// The nodes.
    pub nodes: Vec<AabNode>,
    /// The polygon indices.
    pub poly_indices: Vec<u32>,
}

/// The 32-byte `W3dMeshAABTreeHeader`'s fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AabTreeHeader {
    /// The node count.
    pub node_count: u32,
    /// The polygon index count.
    pub poly_count: u32,
}

// ---------------------------------------------------------------------------
// legacy materials
// ---------------------------------------------------------------------------

/// One legacy (version-3) material: the indices `Add_Legacy_Material`
/// resolved — unique parts added to the shared lists, duplicates
/// remapped to the first occurrence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LegacyMaterial {
    /// The shader index into [`W3dMesh::shaders`].
    pub shader_idx: Ref,
    /// The vertex material index into [`W3dMesh::vertex_materials`]
    /// (`-1` when the material had no vertex material).
    pub vertex_material_idx: Ref,
    /// The texture index into [`W3dMesh::textures`] (`-1` when absent).
    pub texture_idx: Ref,
}

// ---------------------------------------------------------------------------
// the geometry (meshgeometry.cpp, load-path state)
// ---------------------------------------------------------------------------

/// The mesh geometry exactly as the load path leaves it.
///
/// `Reset_Geometry` allocates (and zeroes) the vertex-indexed arrays only
/// when **both** counts are non-zero — a mesh with a zero count keeps
/// `None` arrays, and the readers that would have filled them fail
/// instead of writing through the original's null pointers. The plane
/// equations live in a scratch vector the Zero Hour build grows
/// uninitialised (`OPTIMIZE_PLANEEQ_RAM`); `read_triangles` writes every
/// entry, so the array is fully defined whenever a triangles chunk was
/// read.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MeshGeometry {
    /// The polygon connectivity — `TriIndex` **u16** triples, so the u32
    /// file values truncate.
    pub polys: Option<Vec<[u16; 3]>>,
    /// The per-polygon surface types (the triangle attributes truncated
    /// to u8).
    pub surface_types: Option<Vec<u8>>,
    /// The vertex positions.
    pub vertices: Option<Vec<[f32; 3]>>,
    /// The vertex normals.
    pub vertex_normals: Option<Vec<[f32; 3]>>,
    /// The per-vertex shade indices.
    pub shade_indices: Option<Vec<u32>>,
    /// The per-vertex bone links (after the pre-3.0 fixup).
    pub bone_links: Option<Vec<u16>>,
    /// The per-polygon plane equations (`(n, -dist)` quadruples) — empty
    /// when no triangles chunk was read.
    pub plane_equations: Vec<[f32; 4]>,
    /// The user text (the first chunk wins; `None` when absent).
    pub user_text: Option<Vec<u8>>,
}

// ---------------------------------------------------------------------------
// the mesh
// ---------------------------------------------------------------------------

/// A loaded W3D mesh — the state `MeshModelClass::Load_W3D` leaves
/// behind, minus the render-side post-processing (see the module docs).
#[derive(Debug, Clone, PartialEq)]
pub struct W3dMesh {
    /// The decoded mesh header.
    pub header: MeshHeader3,
    /// The concatenated name (`"Container.Mesh"` or `"Mesh"`).
    pub name: String,
    /// The header's attribute flags, as `W3dAttributes` held them.
    pub attributes: u32,
    /// The geometry flags the load path set (`FG_*`).
    pub flags: u32,
    /// The static sort level.
    pub sort_level: i32,
    /// The selected prelit chunk id (`0xffffffff` when none matched).
    pub prelit_chunk_id: u32,
    /// The geometry.
    pub geometry: MeshGeometry,
    /// The last `W3dMaterialInfoStruct` the load read (zeroed when no
    /// material info chunk appeared).
    pub material_info: MaterialInfo,
    /// The shaders, as packed `ShaderClass` bits.
    pub shaders: Vec<u32>,
    /// The vertex materials.
    pub vertex_materials: Vec<VertexMaterial>,
    /// The texture list (may hold the same object twice — the entries
    /// are the objects' data).
    pub textures: Vec<TextureEntry>,
    /// The legacy (version-3) materials.
    pub legacy_materials: Vec<LegacyMaterial>,
    /// The default material description.
    pub materials: MatDesc,
    /// The merged alternate material description (`None` when the load
    /// context's alternate was empty — note shader-only data counts as
    /// empty).
    pub alternate_materials: Option<MatDesc>,
    /// The AABTree.
    pub aabtree: Option<AabTree>,
}

/// The 16-byte `W3dMaterialInfoStruct`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MaterialInfo {
    /// How many material passes the mesh uses.
    pub pass_count: u32,
    /// How many vertex materials it uses.
    pub vertex_material_count: u32,
    /// How many shaders it uses.
    pub shader_count: u32,
    /// How many textures it uses.
    pub texture_count: u32,
}

impl MaterialInfo {
    /// Decodes the struct from its 16 bytes.
    pub fn from_bytes(bytes: &[u8; 16]) -> Self {
        Self {
            pass_count: u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            vertex_material_count: u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            shader_count: u32::from_le_bytes(bytes[8..12].try_into().unwrap()),
            texture_count: u32::from_le_bytes(bytes[12..16].try_into().unwrap()),
        }
    }
}

impl W3dMesh {
    /// Parses the payload of a [`W3D_CHUNK_MESH`] container — the bytes
    /// between the container's header and its end. The caller (the future
    /// W3D asset loader) opens the container first, exactly like the
    /// original's asset manager does before calling `Load_W3D`.
    ///
    /// Sub-reader failures do **not** fail this parse: the original's
    /// `Load_W3D` discards `read_chunks`' error, so the walk stops at the
    /// failing chunk and the load succeeds with the partial state
    /// (pinned by the goldens). Only the outright failures — a missing or
    /// truncated mesh header, or the pre-3.0 skin fixup without bone
    /// links — return [`Err`].
    pub fn parse(data: &[u8]) -> Result<Self, MeshError> {
        MeshLoader::new().load(data)
    }

    /// Whether the header's collision-type bits are non-zero (the
    /// condition for the original's `Generate_Culling_Tree` — the
    /// generated tree itself is deferred to the collision slice).
    pub fn is_collideable(&self) -> bool {
        (self.attributes & W3D_MESH_FLAG_COLLISION_TYPE_MASK) >> W3D_MESH_FLAG_COLLISION_TYPE_SHIFT
            != 0
    }
}

// ---------------------------------------------------------------------------
// the loader (MeshLoadContextClass + the read_* methods)
// ---------------------------------------------------------------------------

/// The 44-byte `W3dMaterial3Struct`, decoded.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct W3dMaterial3Struct {
    /// The attribute flags (`W3DMATERIAL_*`).
    pub attributes: u32,
    /// The diffuse colour.
    pub diffuse_color: [u8; 4],
    /// The specular colour.
    pub specular_color: [u8; 4],
    /// The emissive coefficients.
    pub emissive_coefficients: [u8; 4],
    /// The ambient coefficients.
    pub ambient_coefficients: [u8; 4],
    /// The diffuse coefficients.
    pub diffuse_coefficients: [u8; 4],
    /// The specular coefficients.
    pub specular_coefficients: [u8; 4],
    /// The shininess.
    pub shininess: f32,
    /// The opacity.
    pub opacity: f32,
    /// The translucency.
    pub translucency: f32,
    /// The fog coefficient.
    pub fog_coeff: f32,
}

/// The 8-byte `W3dMap3Struct`, decoded.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct W3dMap3Struct {
    /// The mapping type.
    pub mapping_type: u16,
    /// The frame count.
    pub frame_count: u16,
    /// The frame rate.
    pub frame_rate: f32,
}

/// The load state machine — the scratch the original's
/// `MeshLoadContextClass` holds plus the mesh under construction.
struct MeshLoader {
    // the mesh under construction
    mesh: W3dMesh,
    // the load context
    prelit_chunk_id: u32,
    cur_pass: i32,
    cur_tex_stage: i32,
    loaded_dig: bool,
    vertex_material_crcs: Vec<u32>,
    // the asset manager's texture pool: lowercased name -> index into
    // texture_objects
    texture_pool: Vec<(String, usize)>,
    texture_objects: Vec<TextureEntry>,
    // the alternate material description collected during the load
    alt: MatDesc,
}

impl MeshLoader {
    fn new() -> Self {
        Self {
            mesh: W3dMesh {
                header: MeshHeader3 {
                    version: 0,
                    attributes: 0,
                    mesh_name: [0; 16],
                    container_name: [0; 16],
                    num_tris: 0,
                    num_vertices: 0,
                    num_materials: 0,
                    num_damage_stages: 0,
                    sort_level: 0,
                    prelit_version: 0,
                    future_counts: [0],
                    vertex_channels: 0,
                    face_channels: 0,
                    min: [0.0; 3],
                    max: [0.0; 3],
                    sph_center: [0.0; 3],
                    sph_radius: 0.0,
                },
                name: String::new(),
                attributes: 0,
                sort_level: 0,
                flags: 0,
                prelit_chunk_id: 0xffff_ffff,
                geometry: MeshGeometry::default(),
                material_info: MaterialInfo::default(),
                shaders: Vec::new(),
                vertex_materials: Vec::new(),
                textures: Vec::new(),
                legacy_materials: Vec::new(),
                materials: MatDesc::default(),
                alternate_materials: None,
                aabtree: None,
            },
            prelit_chunk_id: 0xffff_ffff,
            cur_pass: 0,
            cur_tex_stage: 0,
            loaded_dig: false,
            vertex_material_crcs: Vec::new(),
            texture_pool: Vec::new(),
            texture_objects: Vec::new(),
            alt: MatDesc::default(),
        }
    }

    fn vertex_count(&self) -> usize {
        self.mesh.header.num_vertices.min(i32::MAX as u32) as usize
    }

    fn poly_count(&self) -> usize {
        self.mesh.header.num_tris.min(i32::MAX as u32) as usize
    }

    fn set_flag(&mut self, flag: u32, on: bool) {
        if on {
            self.mesh.flags |= flag;
        } else {
            self.mesh.flags &= !flag;
        }
    }

    // --- Load_W3D ----------------------------------------------------------

    fn load(mut self, data: &[u8]) -> Result<W3dMesh, MeshError> {
        let mut cload = ChunkLoad::new(data);

        // Open the first chunk, it should be the mesh header. (The
        // original ignores Open_Chunk's return and reads Cur_Chunk_ID()
        // through an empty stack on an empty file — the port states the
        // contract instead.)
        if !cload.open_chunk()? {
            return Err(MeshError::LoadFailed);
        }
        if cload.cur_chunk_id() != W3D_CHUNK_MESH_HEADER3 {
            // "Old format mesh mesh, no longer supported."
            return Err(MeshError::LoadFailed);
        }
        let mut header_bytes = [0u8; 116];
        if cload.read(&mut header_bytes)? != MESH_HEADER3_SIZE {
            return Err(MeshError::LoadFailed);
        }
        self.mesh.header = MeshHeader3::from_bytes(&header_bytes);
        cload.close_chunk()?;

        // Process the header: Reset(NumTris, NumVertices, 1)
        let num_tris = self.mesh.header.num_tris;
        let num_vertices = self.mesh.header.num_vertices;
        self.mesh.geometry = MeshGeometry::default();
        self.mesh
            .materials
            .reset(num_tris as usize, num_vertices as usize, 1);
        if num_tris != 0 && num_vertices != 0 {
            self.mesh.geometry.polys = Some(vec![[0u16; 3]; num_tris as usize]);
            self.mesh.geometry.surface_types = Some(vec![0u8; num_tris as usize]);
            self.mesh.geometry.vertices = Some(vec![[0.0f32; 3]; num_vertices as usize]);
            self.mesh.geometry.vertex_normals = Some(vec![[0.0f32; 3]; num_vertices as usize]);
        }

        // the concatenated name
        let container = self.mesh.header.container_name_str();
        let mesh_name = self.mesh.header.mesh_name_str();
        let mut name = Vec::with_capacity(container.len() + mesh_name.len() + 1);
        if !container.is_empty() {
            name.extend_from_slice(container);
            name.push(b'.');
        }
        name.extend_from_slice(mesh_name);
        self.mesh.name = String::from_utf8_lossy(&name).into_owned();

        self.mesh.attributes = self.mesh.header.attributes;
        self.mesh.sort_level = self.mesh.header.sort_level;
        self.alt.vertex_count = num_vertices as usize;
        self.alt.poly_count = num_tris as usize;

        // Flags
        let attrs = self.mesh.header.attributes;
        let version = self.mesh.header.version;
        if version >= w3d_make_version(4, 1) {
            match attrs & W3D_MESH_FLAG_GEOMETRY_TYPE_MASK {
                W3D_MESH_FLAG_GEOMETRY_TYPE_NORMAL => {}
                W3D_MESH_FLAG_GEOMETRY_TYPE_CAMERA_ALIGNED => self.set_flag(FG_ALIGNED, true),
                W3D_MESH_FLAG_GEOMETRY_TYPE_CAMERA_ORIENTED => self.set_flag(FG_ORIENTED, true),
                W3D_MESH_FLAG_GEOMETRY_TYPE_SKIN => {
                    self.set_flag(FG_SKIN, true);
                    self.set_flag(FG_ALLOW_NPATCHES, true);
                }
                _ => {} // OBBOX / obsolete SHADOW / AABOX: nothing
            }
        }
        if attrs & W3D_MESH_FLAG_TWO_SIDED != 0 {
            self.set_flag(FG_TWO_SIDED, true);
        }
        if attrs & W3D_MESH_FLAG_CAST_SHADOW != 0 {
            self.set_flag(FG_CAST_SHADOW, true);
        }
        if attrs & W3D_MESH_FLAG_NPATCHABLE != 0 {
            self.set_flag(FG_ALLOW_NPATCHES, true);
        }

        // Configure the load sequence for prelighting. The original's
        // switch enters at WW3D::Get_Prelit_Mode() — LIGHTMAP_MULTI_PASS,
        // which the game tree never changes — and falls through from
        // there: the multi-texture flag is never even consulted (dead
        // code in the game's configuration; a mesh carrying only it falls
        // straight to the unlit default, pinned by the goldens).
        if attrs & W3D_MESH_FLAG_PRELIT_MASK != 0 {
            'selection: {
                if attrs & W3D_MESH_FLAG_PRELIT_LIGHTMAP_MULTI_PASS != 0 {
                    self.prelit_chunk_id = W3D_CHUNK_PRELIT_LIGHTMAP_MULTI_PASS;
                    self.set_flag(FG_PRELIT_LIGHTMAP_MULTI_PASS, true);
                    break 'selection;
                }
                if attrs & W3D_MESH_FLAG_PRELIT_VERTEX != 0 {
                    self.prelit_chunk_id = W3D_CHUNK_PRELIT_VERTEX;
                    self.set_flag(FG_PRELIT_VERTEX, true);
                    break 'selection;
                }
                // the default case: the unlit chunk id (the debug assert
                // demands the unlit flag; release sets it regardless)
                self.prelit_chunk_id = W3D_CHUNK_PRELIT_UNLIT;
            }
        } else {
            // backwards compatibility: the obsolete lightmap flag
            if attrs & OBSOLETE_W3D_MESH_FLAG_LIGHTMAPPED != 0 {
                self.set_flag(FG_PRELIT_LIGHTMAP_MULTI_PASS, true);
            }
        }
        self.mesh.prelit_chunk_id = self.prelit_chunk_id;

        // read_chunks — the original DISCARDS the error: a failing
        // sub-reader aborts the walk and the load still succeeds with the
        // partial state
        let _ = self.read_chunks(&mut cload);

        // If this is a pre-3.0 mesh and it has vertex influences, fixup
        // the bone indices to account for the new root node
        if version < w3d_make_version(3, 0) && self.mesh.flags & FG_SKIN != 0 {
            // the release build dereferences a null bone-link array when
            // the influences chunk was never read (the flag cannot be set
            // without it at this version) — stated as a load failure
            let Some(links) = self.mesh.geometry.bone_links.as_mut() else {
                return Err(MeshError::LoadFailed);
            };
            for link in links.iter_mut() {
                *link = link.wrapping_add(1);
            }
        }

        // a collideable mesh without an AABTree chunk generates one here
        // (Generate_Culling_Tree — deferred to the collision slice)

        self.install_materials();

        Ok(self.mesh)
    }

    // --- read_chunks ---------------------------------------------------------

    fn read_chunks(&mut self, cload: &mut ChunkLoad) -> Result<(), MeshError> {
        while cload.open_chunk()? {
            let error: Result<(), MeshError> = match cload.cur_chunk_id() {
                W3D_CHUNK_VERTICES => self.read_vertices(cload),
                W3D_CHUNK_SURRENDER_NORMALS | W3D_CHUNK_VERTEX_NORMALS => {
                    self.read_vertex_normals(cload)
                }
                W3D_CHUNK_TEXCOORDS => self.read_texcoords(cload),
                O_W3D_CHUNK_MATERIALS | O_W3D_CHUNK_MATERIALS2 => {
                    // "Obsolete material chunk encountered in mesh: ..."
                    // WWASSERT(0) — release: nothing
                    Ok(())
                }
                W3D_CHUNK_MATERIALS3 => self.read_v3_materials(cload),
                O_W3D_CHUNK_SURRENDER_TRIANGLES => {
                    // "Obsolete Triangle Chunk Encountered!" — release: nothing
                    Ok(())
                }
                W3D_CHUNK_TRIANGLES => self.read_triangles(cload),
                W3D_CHUNK_PER_TRI_MATERIALS => self.read_per_tri_materials(cload),
                W3D_CHUNK_MESH_USER_TEXT => self.read_user_text(cload),
                W3D_CHUNK_VERTEX_COLORS => self.read_vertex_colors(cload),
                W3D_CHUNK_VERTEX_INFLUENCES => self.read_vertex_influences(cload),
                W3D_CHUNK_VERTEX_SHADE_INDICES => self.read_vertex_shade_indices(cload),
                W3D_CHUNK_MATERIAL_INFO => self.read_material_info(cload),
                W3D_CHUNK_SHADERS => self.read_shaders(cload),
                W3D_CHUNK_VERTEX_MATERIALS => self.read_vertex_materials(cload),
                W3D_CHUNK_TEXTURES => self.read_textures(cload),
                W3D_CHUNK_MATERIAL_PASS => self.read_material_pass(cload),
                W3D_CHUNK_DEFORM => {
                    // "Obsolete deform chunk encountered in mesh: ..."
                    Ok(())
                }
                W3D_CHUNK_DAMAGE => {
                    // "Obsolete damage chunk encountered in mesh: ..."
                    Ok(())
                }
                W3D_CHUNK_PRELIT_UNLIT
                | W3D_CHUNK_PRELIT_VERTEX
                | W3D_CHUNK_PRELIT_LIGHTMAP_MULTI_PASS
                | W3D_CHUNK_PRELIT_LIGHTMAP_MULTI_TEXTURE => self.read_prelit_material(cload),
                W3D_CHUNK_AABTREE => self.read_aabtree(cload),
                _ => Ok(()),
            };
            cload.close_chunk()?;
            error?;
        }
        Ok(())
    }

    // --- the geometry readers (meshgeometry.cpp) -----------------------------

    /// `read_vertices`.
    fn read_vertices(&mut self, cload: &mut ChunkLoad) -> Result<(), MeshError> {
        let Some(vertices) = self.mesh.geometry.vertices.as_mut() else {
            return Err(MeshError::SubReadFailed); // release: null write (UB)
        };
        for vert in vertices.iter_mut() {
            let mut bytes = [0u8; 12];
            if cload.read(&mut bytes)? != 12 {
                return Err(MeshError::SubReadFailed);
            }
            *vert = [
                f32::from_le_bytes(bytes[0..4].try_into().unwrap()),
                f32::from_le_bytes(bytes[4..8].try_into().unwrap()),
                f32::from_le_bytes(bytes[8..12].try_into().unwrap()),
            ];
        }
        Ok(())
    }

    /// `read_vertex_normals`.
    fn read_vertex_normals(&mut self, cload: &mut ChunkLoad) -> Result<(), MeshError> {
        let Some(normals) = self.mesh.geometry.vertex_normals.as_mut() else {
            return Err(MeshError::SubReadFailed);
        };
        for norm in normals.iter_mut() {
            let mut bytes = [0u8; 12];
            if cload.read(&mut bytes)? != 12 {
                return Err(MeshError::SubReadFailed);
            }
            *norm = [
                f32::from_le_bytes(bytes[0..4].try_into().unwrap()),
                f32::from_le_bytes(bytes[4..8].try_into().unwrap()),
                f32::from_le_bytes(bytes[8..12].try_into().unwrap()),
            ];
        }
        Ok(())
    }

    /// `read_triangles` — the vertex indices land in `TriIndex` (u16)
    /// fields, the plane equation's W is `-Dist`, and the surface type is
    /// the attributes truncated to u8.
    fn read_triangles(&mut self, cload: &mut ChunkLoad) -> Result<(), MeshError> {
        let count = self.poly_count();
        if self.mesh.geometry.polys.is_none() || self.mesh.geometry.surface_types.is_none() {
            return Err(MeshError::SubReadFailed); // release: null write (UB)
        }
        self.mesh.geometry.plane_equations = vec![[0.0f32; 4]; count];
        for i in 0..count {
            let mut tri = [0u8; 32];
            if cload.read(&mut tri)? != 32 {
                return Err(MeshError::SubReadFailed);
            }
            let u32at = |o: usize| u32::from_le_bytes(tri[o..o + 4].try_into().unwrap());
            let f32at = |o: usize| f32::from_bits(u32at(o));
            let vindex = [u32at(0), u32at(4), u32at(8)];
            let attributes = u32at(12);
            let normal = [f32at(16), f32at(20), f32at(24)];
            let dist = f32at(28);

            let polys = self.mesh.geometry.polys.as_mut().unwrap();
            polys[i] = [vindex[0] as u16, vindex[1] as u16, vindex[2] as u16];
            self.mesh.geometry.plane_equations[i] = [normal[0], normal[1], normal[2], -dist];
            self.mesh.geometry.surface_types.as_mut().unwrap()[i] = attributes as u8;
        }
        Ok(())
    }

    /// `read_user_text` — only the first chunk is kept.
    fn read_user_text(&mut self, cload: &mut ChunkLoad) -> Result<(), MeshError> {
        let textlen = cload.cur_chunk_length();
        if self.mesh.geometry.user_text.is_some() {
            return Ok(());
        }
        let mut text = vec![0u8; textlen as usize];
        if cload.read(&mut text)? != textlen {
            return Err(MeshError::SubReadFailed);
        }
        self.mesh.geometry.user_text = Some(text);
        Ok(())
    }

    /// `read_vertex_influences` — also sets the SKIN flag.
    fn read_vertex_influences(&mut self, cload: &mut ChunkLoad) -> Result<(), MeshError> {
        let count = self.vertex_count();
        let links = self
            .mesh
            .geometry
            .bone_links
            .get_or_insert_with(|| vec![0u16; count]);
        for link in links.iter_mut() {
            let mut vinf = [0u8; 8];
            if cload.read(&mut vinf)? != 8 {
                return Err(MeshError::SubReadFailed);
            }
            *link = u16::from_le_bytes([vinf[0], vinf[1]]);
        }
        self.set_flag(FG_SKIN, true);
        Ok(())
    }

    /// `read_vertex_shade_indices`.
    fn read_vertex_shade_indices(&mut self, cload: &mut ChunkLoad) -> Result<(), MeshError> {
        let count = self.vertex_count();
        let shades = self
            .mesh
            .geometry
            .shade_indices
            .get_or_insert_with(|| vec![0u32; count]);
        for shade in shades.iter_mut() {
            let mut si = [0u8; 4];
            if cload.read(&mut si)? != 4 {
                return Err(MeshError::SubReadFailed);
            }
            *shade = u32::from_le_bytes(si);
        }
        Ok(())
    }

    /// `read_aabtree` — a failed read fails the sub-read (the original
    /// cannot fail here but leaves uninitialised arrays).
    fn read_aabtree(&mut self, cload: &mut ChunkLoad) -> Result<(), MeshError> {
        let mut tree = AabTree {
            node_count: 0,
            poly_count: 0,
            nodes: Vec::new(),
            poly_indices: Vec::new(),
        };
        // the header (the original asserts the chunk id; release parses
        // whatever it is into the same fields)
        cload.open_chunk()?;
        let mut header = [0u8; 32];
        if cload.read(&mut header)? != 32 {
            return Err(MeshError::SubReadFailed);
        }
        cload.close_chunk()?;
        tree.node_count = u32::from_le_bytes(header[0..4].try_into().unwrap());
        tree.poly_count = u32::from_le_bytes(header[4..8].try_into().unwrap());
        tree.nodes = vec![
            AabNode {
                min: [0.0; 3],
                max: [0.0; 3],
                front_or_poly0: 0,
                back_or_poly_count: 0,
            };
            tree.node_count as usize
        ];
        tree.poly_indices = vec![0u32; tree.poly_count as usize];

        // the original installs the tree object before its children load;
        // a failed child read aborts the walk with the partial tree in place

        while cload.open_chunk()? {
            let error = match cload.cur_chunk_id() {
                W3D_CHUNK_AABTREE_POLYINDICES => {
                    let mut buf = vec![0u8; tree.poly_count as usize * 4];
                    let ok = cload.read(&mut buf)? == buf.len() as u32;
                    if ok {
                        for (i, chunk) in buf.as_chunks::<4>().0.iter().enumerate() {
                            tree.poly_indices[i] = u32::from_le_bytes(*chunk);
                        }
                    }
                    ok
                }
                W3D_CHUNK_AABTREE_NODES => {
                    let mut ok = true;
                    for i in 0..tree.node_count as usize {
                        let mut node = [0u8; 32];
                        if cload.read(&mut node)? != 32 {
                            ok = false;
                            break;
                        }
                        let f32at = |o: usize| {
                            f32::from_bits(u32::from_le_bytes(node[o..o + 4].try_into().unwrap()))
                        };
                        tree.nodes[i] = AabNode {
                            min: [f32at(0), f32at(4), f32at(8)],
                            max: [f32at(12), f32at(16), f32at(20)],
                            front_or_poly0: u32::from_le_bytes(node[24..28].try_into().unwrap()),
                            back_or_poly_count: u32::from_le_bytes(
                                node[28..32].try_into().unwrap(),
                            ),
                        };
                    }
                    ok
                }
                _ => true,
            };
            cload.close_chunk()?;
            if !error {
                self.mesh.aabtree = Some(tree);
                return Err(MeshError::SubReadFailed);
            }
        }
        self.mesh.aabtree = Some(tree);
        Ok(())
    }

    // --- read_texcoords (the obsolete top-level path) -------------------------

    /// `read_texcoords`: reads `VertexCount` pairs into an
    /// element-count-sized scratch and installs `elementcount` of them —
    /// only `elementcount == VertexCount` is defined, so any disagreement
    /// fails the sub-read (the C++ installs uninitialised scratch on one
    /// side and fails the reads on the other).
    fn read_texcoords(&mut self, cload: &mut ChunkLoad) -> Result<(), MeshError> {
        let elementcount = (cload.cur_chunk_length() / 8) as usize;
        if elementcount != self.vertex_count() {
            return Err(MeshError::SubReadFailed);
        }
        let mut uvarray = vec![[0.0f32; 2]; elementcount];
        for pair in uvarray.iter_mut() {
            let mut bytes = [0u8; 8];
            if cload.read(&mut bytes)? != 8 {
                return Err(MeshError::SubReadFailed);
            }
            let u = f32::from_le_bytes(bytes[0..4].try_into().unwrap());
            let v = f32::from_le_bytes(bytes[4..8].try_into().unwrap());
            *pair = [u, 1.0 - v];
        }
        let pass = self.cur_pass as usize;
        let stage = self.cur_tex_stage as usize;
        if pass >= MAX_PASSES || stage >= MAX_TEX_STAGES {
            return Err(MeshError::SubReadFailed);
        }
        self.mesh.materials.install_uv_array(pass, stage, &uvarray);
        Ok(())
    }

    // --- read_v3_materials (the legacy material path) --------------------------

    /// `read_v3_materials` — the obsolete version-3 material tree with
    /// its name/info/map structure and the `Add_Legacy_Material` dedup.
    fn read_v3_materials(&mut self, cload: &mut ChunkLoad) -> Result<(), MeshError> {
        for _ in 0..self.mesh.header.num_materials {
            // a W3D_CHUNK_MATERIAL3 wraps each material
            if !cload.open_chunk()? {
                return Err(MeshError::SubReadFailed);
            }
            if cload.cur_chunk_id() != W3D_CHUNK_MATERIAL3 {
                return Err(MeshError::SubReadFailed);
            }

            let mut name = [0u8; 256];
            let mut shader_bits = SHADER_DEFAULT_BITS;
            let mut vmat;
            let mut tex_obj: Option<usize> = None;

            // read the material name
            if !cload.open_chunk()? {
                return Err(MeshError::SubReadFailed);
            }
            if cload.cur_chunk_id() != W3D_CHUNK_MATERIAL3_NAME {
                return Err(MeshError::SubReadFailed);
            }
            let namelen = cload.cur_chunk_length() as usize;
            if namelen > name.len() {
                return Err(MeshError::SubReadFailed); // the original overruns the stack buffer
            }
            cload.read(&mut name[..namelen])?;
            let hasname = true;
            cload.close_chunk()?;

            // read the vertex material parameters
            if !cload.open_chunk()? {
                return Err(MeshError::SubReadFailed);
            }
            if cload.cur_chunk_id() != W3D_CHUNK_MATERIAL3_INFO {
                return Err(MeshError::SubReadFailed);
            }
            let mut mat_bytes = [0u8; 44];
            if cload.read(&mut mat_bytes)? != 44 {
                return Err(MeshError::SubReadFailed);
            }
            let mat = decode_material3(&mat_bytes);
            vmat = init_from_material3(&mat);
            if hasname {
                vmat.name = String::from_utf8_lossy(cstr(&name)).into_owned();
            }
            // ShaderClass::Init_From_Material3
            if mat.attributes & W3DMATERIAL_USE_ALPHA != 0 {
                shader_bits = set_shader_field(shader_bits, 0, SHIFT_DEPTHMASK, 1); // DEPTH_WRITE_DISABLE
                shader_bits = set_shader_field(shader_bits, 5, SHIFT_DSTBLEND, 3); // ONE_MINUS_SRC_ALPHA
                shader_bits = set_shader_field(shader_bits, 2, SHIFT_SRCBLEND, 2); // SRC_ALPHA
            }

            // if this shader does alpha blending, the mesh must be sorted
            if shader_dst_blend(shader_bits) != 0 {
                self.set_flag(FG_SORT, true);
            }

            cload.close_chunk()?;

            // look for the DC map and read it in
            while cload.open_chunk()? {
                let chunk_id = cload.cur_chunk_id();
                if chunk_id == W3D_CHUNK_MATERIAL3_DC_MAP {
                    let filename = self.read_map3(cload)?;
                    tex_obj = Some(self.asset_get_texture(&filename));
                    shader_bits = set_shader_field(shader_bits, 1, SHIFT_TEXTURING, 1); // TEXTURING_ENABLE
                } else if chunk_id == W3D_CHUNK_MATERIAL3_SI_MAP && vmat.diffuse == [0.0, 0.0, 0.0]
                {
                    let filename = self.read_map3(cload)?;
                    // if (tex) tex->Release_Ref(); — no data effect
                    tex_obj = Some(self.asset_get_texture(&filename));
                    shader_bits = set_shader_field(shader_bits, 1, SHIFT_TEXTURING, 1);
                    shader_bits = set_shader_field(shader_bits, 1, SHIFT_DSTBLEND, 3); // DSTBLEND_ONE
                    shader_bits = set_shader_field(shader_bits, 1, SHIFT_SRCBLEND, 2); // SRCBLEND_ONE
                    shader_bits = set_shader_field(shader_bits, 0, SHIFT_PRIGRADIENT, 3); // GRADIENT_DISABLE
                }
                cload.close_chunk()?;
            }

            // If not texturing, move the diffuse color to ambient
            // (simulating old behavior)
            if shader_texturing(shader_bits) == 0 {
                let color = vmat.diffuse;
                vmat.ambient = color;
                vmat.diffuse = [0.0, 0.0, 0.0];
            }

            // Add_Legacy_Material(shader, vmat, tex) — the original
            // dereferences a null material here when the INFO chunk was
            // missing, which the early error returns above already cover
            self.legacy_add_material(shader_bits, &vmat, tex_obj);

            // close the W3D_CHUNK_MATERIAL3
            cload.close_chunk()?;
        }

        // install the default materials to use in the absence of an array
        if !self.mesh.vertex_materials.is_empty() {
            self.mesh.materials.set_single_material(0, 0);
        }
        if !self.texture_list_is_empty() {
            self.mesh.materials.set_single_texture(0, 0, 0);
        }
        if !self.mesh.shaders.is_empty() {
            self.mesh
                .materials
                .set_single_shader(self.mesh.shaders[0], 0);
        }
        Ok(())
    }

    /// Reads a MATERIAL3 map's filename + info (`W3dMap3Struct`).
    fn read_map3(&mut self, cload: &mut ChunkLoad) -> Result<String, MeshError> {
        let mut filename = [0u8; 259]; // _MAX_FNAME + _MAX_EXT
        if !cload.open_chunk()? {
            return Err(MeshError::SubReadFailed);
        }
        if cload.cur_chunk_id() != W3D_CHUNK_MAP3_FILENAME {
            return Err(MeshError::SubReadFailed);
        }
        let len = cload.cur_chunk_length() as usize;
        if len >= filename.len() {
            return Err(MeshError::SubReadFailed);
        }
        cload.read(&mut filename[..len])?;
        cload.close_chunk()?;

        let mut mapinfo = [0u8; 8];
        if !cload.open_chunk()? {
            return Err(MeshError::SubReadFailed);
        }
        if cload.cur_chunk_id() != W3D_CHUNK_MAP3_INFO {
            return Err(MeshError::SubReadFailed);
        }
        if cload.read(&mut mapinfo)? != 8 {
            return Err(MeshError::SubReadFailed);
        }
        cload.close_chunk()?;
        // the frame-count warning is debug output; no state changes
        Ok(String::from_utf8_lossy(cstr(&filename)).into_owned())
    }

    /// `WW3DAssetManager::Get_Texture`'s data path: the name lowercases
    /// and keys the pool; the object is created on a miss (with no info
    /// struct). An empty filename returns null.
    fn asset_get_texture(&mut self, filename: &str) -> usize {
        let lower = filename.to_ascii_lowercase();
        if lower.is_empty() {
            return usize::MAX; // "null"
        }
        for (name, index) in &self.texture_pool {
            if *name == lower {
                return *index;
            }
        }
        let index = self.texture_objects.len();
        self.texture_objects.push(TextureEntry {
            name: lower.clone(),
            info: None,
        });
        self.texture_pool.push((lower, index));
        index
    }

    fn texture_list_is_empty(&self) -> bool {
        self.mesh.textures.is_empty()
    }

    /// `Add_Legacy_Material` — unique-part detection: the shader by
    /// value, the vertex material by CRC (duplicates remap to the first),
    /// the texture by object identity.
    fn legacy_add_material(
        &mut self,
        shader_bits: u32,
        vmat: &VertexMaterial,
        tex_obj: Option<usize>,
    ) {
        let mut shader_idx = -1i32;
        for (i, bits) in self.mesh.shaders.iter().enumerate() {
            if *bits == shader_bits {
                shader_idx = i as Ref;
                break;
            }
        }
        if shader_idx == -1 {
            self.mesh.shaders.push(shader_bits);
            shader_idx = (self.mesh.shaders.len() - 1) as Ref;
        }

        let mut vmat_idx = -1i32;
        let crc = vmat.compute_crc();
        let mut vi = 0;
        while vi < self.vertex_material_crcs.len() {
            if self.vertex_material_crcs[vi] == crc {
                vmat_idx = vi as Ref;
                break;
            }
            vi += 1;
        }
        if vmat_idx == -1 {
            self.mesh.vertex_materials.push(vmat.clone());
            self.vertex_material_crcs.push(crc);
            vmat_idx = (self.mesh.vertex_materials.len() - 1) as Ref;
        }

        let mut tex_idx = -1i32;
        if let Some(obj) = tex_obj
            && obj != usize::MAX
        {
            // the object identity is the (lowercased) name — the first list
            // entry carrying it is the same object the pointer compare finds
            let name = self.texture_objects[obj].name.clone();
            for (ti, entry) in self.mesh.textures.iter().enumerate() {
                if entry.name == name {
                    tex_idx = ti as Ref;
                    break;
                }
            }
            if tex_idx == -1 {
                self.mesh.textures.push(self.texture_objects[obj].clone());
                tex_idx = (self.mesh.textures.len() - 1) as Ref;
            }
        }

        self.mesh.legacy_materials.push(LegacyMaterial {
            shader_idx,
            vertex_material_idx: vmat_idx,
            texture_idx: tex_idx,
        });
    }

    // --- read_per_tri_materials -------------------------------------------------

    /// `read_per_tri_materials` — the legacy per-polygon material
    /// assignment.
    fn read_per_tri_materials(&mut self, cload: &mut ChunkLoad) -> Result<(), MeshError> {
        if self.mesh.header.num_materials == 1 {
            return Ok(());
        }

        let multi_mtl = self.mesh.vertex_materials.len() > 1;
        let multi_tex = self.mesh.textures.len() > 1;
        let multi_shad = self.mesh.shaders.len() > 1;

        if !multi_mtl {
            let idx = self.legacy_vmat_index(0);
            self.mesh.materials.set_single_material(idx, 0);
        }
        if !multi_tex {
            let idx = self.legacy_tex_index(0);
            self.mesh.materials.set_single_texture(idx, 0, 0);
        }
        if !multi_shad {
            let bits = self.legacy_shader_bits(0);
            self.mesh.materials.set_single_shader(bits, 0);
        }

        // read in each polygon's material id and assign pointer to the
        // shader, texture, and vertex material as needed
        let poly_count = self.poly_count();
        for i in 0..poly_count {
            let mut matid = [0u8; 2];
            if cload.read(&mut matid)? != 2 {
                return Err(MeshError::SubReadFailed);
            }
            let matid = u16::from_le_bytes(matid) as usize;

            if multi_shad {
                let bits = self.legacy_shader_bits(matid);
                self.mesh.materials.set_shader(i, bits, 0);
            }
            if multi_tex {
                let idx = self.legacy_tex_index(matid);
                self.mesh.materials.set_texture(i, idx, 0, 0);
            }
            if multi_mtl {
                let vmat = self.legacy_vmat_index(matid);
                let polys = self.mesh.geometry.polys.as_ref().unwrap();
                for vidx in polys[i] {
                    self.mesh.materials.set_material(vidx as usize, vmat, 0);
                }
            }
        }
        Ok(())
    }

    fn legacy_shader_bits(&self, legacy_material_index: usize) -> u32 {
        let mat = &self.mesh.legacy_materials[legacy_material_index];
        self.mesh.shaders[mat.shader_idx as usize]
    }

    fn legacy_vmat_index(&self, legacy_material_index: usize) -> Ref {
        self.mesh.legacy_materials[legacy_material_index].vertex_material_idx
    }

    fn legacy_tex_index(&self, legacy_material_index: usize) -> Ref {
        self.mesh.legacy_materials[legacy_material_index].texture_idx
    }

    // --- read_vertex_colors -------------------------------------------------------

    /// `read_vertex_colors` — the first diffuse colour array wins; the
    /// DCG source is always set for the pass.
    fn read_vertex_colors(&mut self, cload: &mut ChunkLoad) -> Result<(), MeshError> {
        if self.cur_pass < 0 || self.cur_pass >= MAX_PASSES as i32 {
            return Err(MeshError::SubReadFailed); // release: OOB write (UB)
        }
        let count = self.vertex_count();
        if !self.mesh.materials.has_color_array(0) {
            for i in 0..count {
                let mut color = [0u8; 4];
                if cload.read(&mut color)? != 4 {
                    return Err(MeshError::SubReadFailed);
                }
                let col = [
                    f32::from(color[0]) / 255.0,
                    f32::from(color[1]) / 255.0,
                    f32::from(color[2]) / 255.0,
                    1.0,
                ];
                let dcg = self.mesh.materials.color_array_mut(0);
                dcg[i] = convert_color_argb(&col);
            }
        }
        let pass = self.cur_pass as usize;
        self.mesh.materials.dcg_source[pass] = CS_COLOR1;
        Ok(())
    }

    // --- read_material_info -------------------------------------------------------

    /// `read_material_info`.
    fn read_material_info(&mut self, cload: &mut ChunkLoad) -> Result<(), MeshError> {
        let mut info = [0u8; 16];
        if cload.read(&mut info)? != 16 {
            return Err(MeshError::SubReadFailed);
        }
        self.mesh.material_info = MaterialInfo::from_bytes(&info);
        self.mesh.materials.pass_count = self.mesh.material_info.pass_count as i32;
        Ok(())
    }

    // --- read_shaders ---------------------------------------------------------------

    /// `read_shaders` — reads `MatInfo.ShaderCount` structs; extra
    /// structs in the chunk are skipped.
    fn read_shaders(&mut self, cload: &mut ChunkLoad) -> Result<(), MeshError> {
        for _ in 0..self.mesh.material_info.shader_count {
            let mut shader = [0u8; 16];
            if cload.read(&mut shader)? != 16 {
                return Err(MeshError::SubReadFailed);
            }
            let bits = convert_shader(&W3dShaderStruct::from_bytes(&shader));
            self.mesh.shaders.push(bits);
        }
        Ok(())
    }

    // --- read_vertex_materials --------------------------------------------------------

    /// `read_vertex_materials` — any child of the wrapper is fed to the
    /// vertex material reader (the assert is debug-only).
    fn read_vertex_materials(&mut self, cload: &mut ChunkLoad) -> Result<(), MeshError> {
        while cload.open_chunk()? {
            let error = self.read_one_vertex_material(cload);
            cload.close_chunk()?;
            error?;
        }
        Ok(())
    }

    /// `VertexMaterialClass::Load_W3D`.
    fn read_one_vertex_material(&mut self, cload: &mut ChunkLoad) -> Result<(), MeshError> {
        let mut name = [0u8; 256];
        let mut hasname = false;
        // the original's struct is uninitialised when no INFO chunk is
        // read — the port reads zeroes
        let mut info = [0u8; 32];
        let mut args0: Vec<u8> = Vec::new();
        let mut args1: Vec<u8> = Vec::new();

        while cload.open_chunk()? {
            let chunk_id = cload.cur_chunk_id();
            match chunk_id {
                W3D_CHUNK_VERTEX_MATERIAL_NAME => {
                    let len = cload.cur_chunk_length() as usize;
                    if len > name.len() {
                        return Err(MeshError::SubReadFailed); // the original overruns the stack buffer
                    }
                    cload.read(&mut name[..len])?;
                    hasname = true;
                }
                W3D_CHUNK_VERTEX_MATERIAL_INFO => {
                    if cload.read(&mut info)? != 32 {
                        return Err(MeshError::SubReadFailed);
                    }
                }
                W3D_CHUNK_VERTEX_MAPPER_ARGS0 => {
                    // a second ARGS0 chunk leaks the first buffer in the
                    // original; the last one wins here
                    let len = cload.cur_chunk_length() as usize;
                    let mut buf = vec![0u8; len];
                    if !buf.is_empty() && cload.read(&mut buf)? != len as u32 {
                        return Err(MeshError::SubReadFailed);
                    }
                    args0 = buf;
                }
                W3D_CHUNK_VERTEX_MAPPER_ARGS1 => {
                    let len = cload.cur_chunk_length() as usize;
                    let mut buf = vec![0u8; len];
                    if !buf.is_empty() && cload.read(&mut buf)? != len as u32 {
                        return Err(MeshError::SubReadFailed);
                    }
                    args1 = buf;
                }
                _ => {}
            }
            cload.close_chunk()?;
        }

        let mut vmat = parse_w3d_vertex_material_struct(&W3dVertexMaterialInfo::from_bytes(&info));
        if hasname {
            vmat.name = String::from_utf8_lossy(cstr(&name)).into_owned();
        }
        // Parse_Mapping_Args (the mapper instantiation) is render-side;
        // the raw buffers are kept
        vmat.mapper_args0 = args0;
        vmat.mapper_args1 = args1;
        self.mesh.vertex_materials.push(vmat);
        Ok(())
    }

    // --- read_textures ---------------------------------------------------------------------

    /// `read_textures` — `::Load_Texture` until it returns null. An empty
    /// texture name makes the asset manager return null and the loop
    /// ends; a first child that is not a TEXTURE chunk also ends it, with
    /// the chunk left open on the reader's stack exactly like the
    /// original.
    fn read_textures(&mut self, cload: &mut ChunkLoad) -> Result<(), MeshError> {
        loop {
            let mut obj: Option<usize> = None;
            if cload.open_chunk()? && cload.cur_chunk_id() == W3D_CHUNK_TEXTURE {
                let mut name = [0u8; 256];
                let mut info = [0u8; 12];
                let mut hastexinfo = false;

                while cload.open_chunk()? {
                    match cload.cur_chunk_id() {
                        W3D_CHUNK_TEXTURE_NAME => {
                            let len = cload.cur_chunk_length() as usize;
                            if len > name.len() {
                                return Err(MeshError::SubReadFailed); // the original overruns the stack buffer
                            }
                            cload.read(&mut name[..len])?;
                        }
                        W3D_CHUNK_TEXTURE_INFO => {
                            if cload.read(&mut info)? != 12 {
                                return Err(MeshError::SubReadFailed);
                            }
                            hastexinfo = true;
                        }
                        _ => {}
                    }
                    cload.close_chunk()?;
                }
                cload.close_chunk()?;

                // Get the texture from the asset manager: an empty name
                // returns null; the hash key lowercases
                let lower = String::from_utf8_lossy(cstr(&name)).to_ascii_lowercase();
                if !lower.is_empty() {
                    let mut found = None;
                    for (pool_name, index) in &self.texture_pool {
                        if *pool_name == lower {
                            found = Some(*index);
                            break;
                        }
                    }
                    match found {
                        Some(index) => obj = Some(index),
                        None => {
                            // created now, with this chunk's info
                            let index = self.texture_objects.len();
                            self.texture_objects.push(TextureEntry {
                                name: lower.clone(),
                                info: if hastexinfo {
                                    Some(W3dTextureInfo::from_bytes(&info))
                                } else {
                                    None
                                },
                            });
                            self.texture_pool.push((lower, index));
                            obj = Some(index);
                        }
                    }
                }
            }

            let Some(obj) = obj else {
                // Load_Texture returned null
                break;
            };

            // add this texture to the list (the object may already be
            // listed — the original appends the pointer regardless)
            self.mesh.textures.push(self.texture_objects[obj].clone());
        }
        Ok(())
    }

    // --- read_material_pass --------------------------------------------------------

    /// `read_material_pass`.
    fn read_material_pass(&mut self, cload: &mut ChunkLoad) -> Result<(), MeshError> {
        self.cur_tex_stage = 0;

        while cload.open_chunk()? {
            let error = match cload.cur_chunk_id() {
                W3D_CHUNK_VERTEX_MATERIAL_IDS => self.read_vertex_material_ids(cload),
                W3D_CHUNK_SHADER_IDS => self.read_shader_ids(cload),
                W3D_CHUNK_DCG => self.read_dcg(cload),
                W3D_CHUNK_DIG => self.read_dig(cload),
                W3D_CHUNK_SCG => self.read_scg(cload),
                W3D_CHUNK_TEXTURE_STAGE => self.read_texture_stage(cload),
                _ => Ok(()),
            };
            cload.close_chunk()?;
            error?;
        }

        self.cur_pass += 1;
        Ok(())
    }

    /// Routes a pass-level read into the default or the load context's
    /// alternate material description, exactly as the "already has data"
    /// checks do.
    fn route<'a>(def: &'a mut MatDesc, alt: &'a mut MatDesc, to_alt: bool) -> &'a mut MatDesc {
        if to_alt { alt } else { def }
    }

    /// `read_vertex_material_ids` — single or per-vertex array.
    fn read_vertex_material_ids(&mut self, cload: &mut ChunkLoad) -> Result<(), MeshError> {
        if self.cur_pass < 0 || self.cur_pass >= MAX_PASSES as i32 {
            return Err(MeshError::SubReadFailed); // release: OOB read (UB)
        }
        let pass = self.cur_pass as usize;
        let to_alt = self.mesh.materials.has_material_data(pass);
        let count = self.vertex_count();

        let matdesc = Self::route(&mut self.mesh.materials, &mut self.alt, to_alt);
        if cload.cur_chunk_length() == 4 {
            let mut vmat = [0u8; 4];
            if cload.read(&mut vmat)? != 4 {
                return Err(MeshError::SubReadFailed);
            }
            let vmat = u32::from_le_bytes(vmat) as Ref;
            if vmat < 0 || vmat as usize >= self.mesh.vertex_materials.len() {
                return Err(MeshError::SubReadFailed); // release: OOB read (UB)
            }
            matdesc.set_single_material(vmat, pass);
        } else {
            for i in 0..count {
                let mut vmat = [0u8; 4];
                if cload.read(&mut vmat)? != 4 {
                    return Err(MeshError::SubReadFailed);
                }
                let vmat = u32::from_le_bytes(vmat) as Ref;
                if vmat < 0 || vmat as usize >= self.mesh.vertex_materials.len() {
                    return Err(MeshError::SubReadFailed);
                }
                matdesc.set_material(i, vmat, pass);
            }
        }
        Ok(())
    }

    /// `read_shader_ids` — single or per-polygon array, with the pass-0
    /// SORT-flag side effect.
    fn read_shader_ids(&mut self, cload: &mut ChunkLoad) -> Result<(), MeshError> {
        if self.cur_pass < 0 || self.cur_pass >= MAX_PASSES as i32 {
            return Err(MeshError::SubReadFailed);
        }
        let pass = self.cur_pass as usize;
        let to_alt = self.mesh.materials.has_shader_data(pass);
        let sort_level = self.mesh.sort_level;

        let count = self.poly_count();
        if cload.cur_chunk_length() == 4 {
            let mut shaderid = [0u8; 4];
            if cload.read(&mut shaderid)? != 4 {
                return Err(MeshError::SubReadFailed);
            }
            let shaderid = u32::from_le_bytes(shaderid) as usize;
            let Some(&bits) = self.mesh.shaders.get(shaderid) else {
                return Err(MeshError::SubReadFailed); // release: OOB read (UB)
            };
            let matdesc = Self::route(&mut self.mesh.materials, &mut self.alt, to_alt);
            matdesc.set_single_shader(bits, pass);

            // turn on sorting of pass 0 has non-zero dest blend (unless
            // alpha testing on)
            if pass == 0
                && shader_dst_blend(bits) != 0
                && shader_alpha_test(bits) == 0
                && sort_level == 0
            {
                self.set_flag(FG_SORT, true);
            }
        } else {
            for i in 0..count {
                let mut shaderid = [0u8; 4];
                if cload.read(&mut shaderid)? != 4 {
                    return Err(MeshError::SubReadFailed);
                }
                let shaderid = u32::from_le_bytes(shaderid) as usize;
                let Some(&bits) = self.mesh.shaders.get(shaderid) else {
                    return Err(MeshError::SubReadFailed);
                };
                let matdesc = Self::route(&mut self.mesh.materials, &mut self.alt, to_alt);
                matdesc.set_shader(i, bits, pass);

                if pass == 0
                    && shader_dst_blend(bits) != 0
                    && shader_alpha_test(bits) == 0
                    && sort_level == 0
                {
                    self.set_flag(FG_SORT, true);
                }
            }
        }
        Ok(())
    }

    /// `read_dcg` — the first array wins; a second routes to the
    /// alternate, which merges the alpha in vertex-prelit mode.
    fn read_dcg(&mut self, cload: &mut ChunkLoad) -> Result<(), MeshError> {
        if self.cur_pass < 0 || self.cur_pass >= MAX_PASSES as i32 {
            return Err(MeshError::SubReadFailed);
        }
        let pass = self.cur_pass as usize;
        let to_alt = self.mesh.materials.dcg_source[pass] != CS_MATERIAL;
        let prelit_vertex = self.prelit_chunk_id == W3D_CHUNK_PRELIT_VERTEX;
        let count = self.vertex_count();

        let matdesc = Self::route(&mut self.mesh.materials, &mut self.alt, to_alt);
        if !matdesc.has_color_array(0) {
            for i in 0..count {
                let mut color = [0u8; 4];
                if cload.read(&mut color)? != 4 {
                    return Err(MeshError::SubReadFailed);
                }
                let col = [
                    f32::from(color[0]) / 255.0,
                    f32::from(color[1]) / 255.0,
                    f32::from(color[2]) / 255.0,
                    f32::from(color[3]) / 255.0,
                ];
                matdesc.color_array_mut(0)[i] = convert_color_argb(&col);
            }
        } else if prelit_vertex {
            for i in 0..count {
                let mut color = [0u8; 4];
                if cload.read(&mut color)? != 4 {
                    return Err(MeshError::SubReadFailed);
                }
                let array = matdesc.color_array_mut(0);
                let mut col = convert_argb_to_vector4(array[i]);
                col[3] = f32::from(color[3]) / 255.0;
                array[i] = convert_color_argb(&col);
            }
        }
        matdesc.dcg_source[pass] = CS_COLOR1;
        Ok(())
    }

    /// `read_dig` — the values multiply into whichever colour array they
    /// land on; the first DIG takes the default routing, later ones the
    /// alternate.
    fn read_dig(&mut self, cload: &mut ChunkLoad) -> Result<(), MeshError> {
        if self.cur_pass < 0 || self.cur_pass >= MAX_PASSES as i32 {
            return Err(MeshError::SubReadFailed);
        }
        let pass = self.cur_pass as usize;
        let to_alt = self.loaded_dig;
        self.loaded_dig = true;
        let count = self.vertex_count();

        let matdesc = Self::route(&mut self.mesh.materials, &mut self.alt, to_alt);
        if !matdesc.has_color_array(0) {
            for i in 0..count {
                let mut color = [0u8; 4];
                if cload.read(&mut color)? != 4 {
                    return Err(MeshError::SubReadFailed);
                }
                let col = [
                    f32::from(color[0]) / 255.0,
                    f32::from(color[1]) / 255.0,
                    f32::from(color[2]) / 255.0,
                    1.0,
                ];
                matdesc.color_array_mut(0)[i] = convert_color_argb(&col);
            }
        } else {
            for i in 0..count {
                let mut color = [0u8; 4];
                if cload.read(&mut color)? != 4 {
                    return Err(MeshError::SubReadFailed);
                }
                let array = matdesc.color_array_mut(0);
                let mut col = convert_argb_to_vector4(array[i]);
                col[0] *= f32::from(color[0]) / 255.0;
                col[1] *= f32::from(color[1]) / 255.0;
                col[2] *= f32::from(color[2]) / 255.0;
                array[i] = convert_color_argb(&col);
            }
        }
        matdesc.dcg_source[pass] = CS_COLOR1;
        Ok(())
    }

    /// `read_scg` — the DX8 port no longer supports this chunk; the data
    /// is skipped by the chunk close.
    fn read_scg(&mut self, _cload: &mut ChunkLoad) -> Result<(), MeshError> {
        Ok(())
    }

    /// `read_texture_stage`.
    fn read_texture_stage(&mut self, cload: &mut ChunkLoad) -> Result<(), MeshError> {
        while cload.open_chunk()? {
            let error = match cload.cur_chunk_id() {
                W3D_CHUNK_TEXTURE_IDS => self.read_texture_ids(cload),
                W3D_CHUNK_STAGE_TEXCOORDS | W3D_CHUNK_TEXCOORDS => self.read_stage_texcoords(cload),
                W3D_CHUNK_PER_FACE_TEXCOORD_IDS => self.read_per_face_texcoord_ids(cload),
                _ => Ok(()),
            };
            cload.close_chunk()?;
            error?;
        }
        self.cur_tex_stage += 1;
        Ok(())
    }

    /// `read_texture_ids` — single or per-polygon array with the
    /// `0xffffffff` skip.
    fn read_texture_ids(&mut self, cload: &mut ChunkLoad) -> Result<(), MeshError> {
        if self.cur_pass < 0 || self.cur_pass >= MAX_PASSES as i32 {
            return Err(MeshError::SubReadFailed);
        }
        if self.cur_tex_stage < 0 || self.cur_tex_stage >= MAX_TEX_STAGES as i32 {
            return Err(MeshError::SubReadFailed);
        }
        let pass = self.cur_pass as usize;
        let stage = self.cur_tex_stage as usize;
        let to_alt = self.mesh.materials.has_texture_data(pass, stage);

        let count = self.poly_count();
        if cload.cur_chunk_length() == 4 {
            let mut texid = [0u8; 4];
            if cload.read(&mut texid)? != 4 {
                return Err(MeshError::SubReadFailed);
            }
            let texid = u32::from_le_bytes(texid);
            if texid == 0xffff_ffff || texid as usize >= self.mesh.textures.len() {
                return Err(MeshError::SubReadFailed); // release: OOB read (UB)
            }
            let matdesc = Self::route(&mut self.mesh.materials, &mut self.alt, to_alt);
            matdesc.set_single_texture(texid as Ref, pass, stage);
        } else {
            for i in 0..count {
                let mut texid = [0u8; 4];
                if cload.read(&mut texid)? != 4 {
                    return Err(MeshError::SubReadFailed);
                }
                let texid = u32::from_le_bytes(texid);
                if texid != 0xffff_ffff {
                    if texid as usize >= self.mesh.textures.len() {
                        return Err(MeshError::SubReadFailed);
                    }
                    let matdesc = Self::route(&mut self.mesh.materials, &mut self.alt, to_alt);
                    matdesc.set_texture(i, texid as Ref, pass, stage);
                }
            }
        }
        Ok(())
    }

    /// `read_stage_texcoords` — the chunk length is authoritative (the
    /// lightmap support: the vertex count is derived from it).
    fn read_stage_texcoords(&mut self, cload: &mut ChunkLoad) -> Result<(), MeshError> {
        if self.cur_pass < 0 || self.cur_pass >= MAX_PASSES as i32 {
            return Err(MeshError::SubReadFailed);
        }
        if self.cur_tex_stage < 0 || self.cur_tex_stage >= MAX_TEX_STAGES as i32 {
            return Err(MeshError::SubReadFailed);
        }
        let pass = self.cur_pass as usize;
        let stage = self.cur_tex_stage as usize;
        let to_alt = self.mesh.materials.has_uv(pass, stage);

        let elementcount = (cload.cur_chunk_length() / 8) as usize;
        let mut uvs = vec![[0.0f32; 2]; elementcount];
        for uv in uvs.iter_mut() {
            let mut bytes = [0u8; 8];
            if cload.read(&mut bytes)? != 8 {
                return Err(MeshError::SubReadFailed);
            }
            let u = f32::from_le_bytes(bytes[0..4].try_into().unwrap());
            let v = f32::from_le_bytes(bytes[4..8].try_into().unwrap());
            *uv = [u, 1.0 - v];
        }

        let matdesc = Self::route(&mut self.mesh.materials, &mut self.alt, to_alt);
        matdesc.install_uv_array(pass, stage, &uvs);
        Ok(())
    }

    /// `read_per_face_texcoord_ids` — the size must match 12 bytes per
    /// polygon; the data itself is skipped.
    fn read_per_face_texcoord_ids(&mut self, cload: &mut ChunkLoad) -> Result<(), MeshError> {
        let size = 12u32 * self.poly_count() as u32;
        if cload.cur_chunk_length() == size {
            cload.seek(size)?;
            Ok(())
        } else {
            Err(MeshError::SubReadFailed)
        }
    }

    // --- read_prelit_material --------------------------------------------------------

    /// `read_prelit_material` — only the selected prelit wrapper's
    /// children are processed.
    fn read_prelit_material(&mut self, cload: &mut ChunkLoad) -> Result<(), MeshError> {
        if cload.cur_chunk_id() == self.prelit_chunk_id {
            while cload.open_chunk()? {
                let error = match cload.cur_chunk_id() {
                    W3D_CHUNK_MATERIAL_INFO => self.read_material_info(cload),
                    W3D_CHUNK_VERTEX_MATERIALS => self.read_vertex_materials(cload),
                    W3D_CHUNK_SHADERS => self.read_shaders(cload),
                    W3D_CHUNK_TEXTURES => self.read_textures(cload),
                    W3D_CHUNK_MATERIAL_PASS => self.read_material_pass(cload),
                    _ => Ok(()), // unknown chunk
                };
                cload.close_chunk()?;
                error?;
            }
        }
        Ok(())
    }

    // --- install_materials --------------------------------------------------------------

    /// `install_materials` — the alternate merge plus the flat list
    /// transfers (`Post_Load_Process`, the DX8 pipeline adaptation, is
    /// render-side and out of scope).
    fn install_materials(&mut self) {
        // if alternate material chunks were loaded, initialize the
        // AlternateMatDesc
        if !self.alt.is_empty() {
            let mut merged = MatDesc::default();
            merged.init_alternate(&self.mesh.materials, &self.alt);
            self.mesh.alternate_materials = Some(merged);
        }
    }
}

/// Decodes a `W3dMaterial3Struct` from its 44 bytes.
fn decode_material3(bytes: &[u8; 44]) -> W3dMaterial3Struct {
    let u32at = |o: usize| u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
    let f32at = |o: usize| f32::from_bits(u32at(o));
    W3dMaterial3Struct {
        attributes: u32at(0),
        diffuse_color: [bytes[4], bytes[5], bytes[6], bytes[7]],
        specular_color: [bytes[8], bytes[9], bytes[10], bytes[11]],
        emissive_coefficients: [bytes[12], bytes[13], bytes[14], bytes[15]],
        ambient_coefficients: [bytes[16], bytes[17], bytes[18], bytes[19]],
        diffuse_coefficients: [bytes[20], bytes[21], bytes[22], bytes[23]],
        specular_coefficients: [bytes[24], bytes[25], bytes[26], bytes[27]],
        shininess: f32at(28),
        opacity: f32at(32),
        translucency: f32at(36),
        fog_coeff: f32at(40),
    }
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_matches_the_well_known_check_value() {
        // the standard reflected CRC-32 check value for "123456789"
        assert_eq!(crc_memory(b"123456789", 0), 0xCBF4_3926);
        // the chaining is additive like the original
        let whole = crc_memory(b"abcdefgh", 0);
        let split = crc_memory(b"efgh", crc_memory(b"abcd", 0));
        assert_eq!(whole, split);
    }

    #[test]
    fn shader_conversion_packs_the_documented_fields() {
        // a default-shaped shader: opaque, textured, modulate
        let shader = W3dShaderStruct {
            depth_compare: 3,
            depth_mask: 1,
            color_mask: 0,
            dest_blend: 0,
            fog_func: 0,
            pri_gradient: 1,
            sec_gradient: 0,
            src_blend: 1,
            texturing: 1,
            detail_color_func: 0,
            detail_alpha_func: 0,
            shader_preset: 0,
            alpha_test: 0,
            post_detail_color_func: 0,
            post_detail_alpha_func: 0,
            pad: 0,
        };
        let bits = convert_shader(&shader);
        // default preset bits | texturing
        assert_eq!(bits, SHADER_DEFAULT_BITS | (1 << SHIFT_TEXTURING));
        assert_eq!(shader_dst_blend(bits), 0);
        assert_eq!(shader_alpha_test(bits), 0);
        assert_eq!(shader_texturing(bits), 1);

        // an alpha-blended shader: dst SRC_ALPHA, src SRC_ALPHA, no depth write
        let shader = W3dShaderStruct {
            dest_blend: 4,
            depth_mask: 0,
            src_blend: 2,
            ..shader
        };
        let bits = convert_shader(&shader);
        assert_eq!(shader_dst_blend(bits), 4);
        // the depth mask field cleared
        assert_eq!(bits & (1 << SHIFT_DEPTHMASK), 0);
    }

    #[test]
    fn header_decodes_every_field() {
        let mut bytes = [0u8; 116];
        bytes[0..4].copy_from_slice(&w3d_make_version(4, 2).to_le_bytes());
        bytes[4..8].copy_from_slice(&0x0200_8000u32.to_le_bytes());
        bytes[8..16].copy_from_slice(b"Body\0\0\0\0");
        bytes[24..32].copy_from_slice(b"AVTank\0\0");
        bytes[40..44].copy_from_slice(&2u32.to_le_bytes());
        bytes[44..48].copy_from_slice(&3u32.to_le_bytes());
        bytes[48..52].copy_from_slice(&1u32.to_le_bytes());
        bytes[52..56].copy_from_slice(&4u32.to_le_bytes());
        bytes[56..60].copy_from_slice(&7i32.to_le_bytes());
        bytes[60..64].copy_from_slice(&9u32.to_le_bytes());
        bytes[64..68].copy_from_slice(&11u32.to_le_bytes());
        bytes[68..72].copy_from_slice(&0x1Fu32.to_le_bytes());
        bytes[72..76].copy_from_slice(&1u32.to_le_bytes());
        for (i, word) in [-1.0f32, 0.5, 1.0, 2.0, 0.25, -0.5, 3.0, 4.0, 5.0, 6.0]
            .iter()
            .enumerate()
        {
            bytes[76 + i * 4..80 + i * 4].copy_from_slice(&word.to_bits().to_le_bytes());
        }
        let header = MeshHeader3::from_bytes(&bytes);
        assert_eq!(header.version, w3d_make_version(4, 2));
        assert_eq!(header.attributes, 0x0200_8000);
        assert_eq!(header.mesh_name_str(), b"Body");
        assert_eq!(header.container_name_str(), b"AVTank");
        assert_eq!(header.num_tris, 2);
        assert_eq!(header.num_vertices, 3);
        assert_eq!(header.num_materials, 1);
        assert_eq!(header.num_damage_stages, 4);
        assert_eq!(header.sort_level, 7);
        assert_eq!(header.prelit_version, 9);
        assert_eq!(header.future_counts, [11]);
        assert_eq!(header.vertex_channels, 0x1F);
        assert_eq!(header.face_channels, 1);
        assert_eq!(header.min, [-1.0f32, 0.5, 1.0]);
        assert_eq!(header.max, [2.0, 0.25, -0.5]);
        assert_eq!(header.sph_center, [3.0, 4.0, 5.0]);
        assert_eq!(header.sph_radius, 6.0);
    }

    #[test]
    fn empty_and_garbage_payloads_fail() {
        assert_eq!(W3dMesh::parse(&[]), Err(MeshError::LoadFailed));
        // a first chunk that is not the mesh header
        let mut bytes = vec![];
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&4u32.to_le_bytes());
        bytes.extend_from_slice(&[0, 0, 0, 0]);
        assert_eq!(W3dMesh::parse(&bytes), Err(MeshError::LoadFailed));
        // a truncated header payload
        let mut bytes = vec![];
        bytes.extend_from_slice(&W3D_CHUNK_MESH_HEADER3.to_le_bytes());
        bytes.extend_from_slice(&116u32.to_le_bytes());
        bytes.extend_from_slice(&[0u8; 60]);
        assert_eq!(W3dMesh::parse(&bytes), Err(MeshError::LoadFailed));
    }

    #[test]
    fn sub_reader_failures_do_not_fail_the_load() {
        // a short vertices chunk: the walk aborts after it, the load still
        // succeeds with the partial state (the discarded error quirk)
        let mut bytes = Vec::new();
        let mut header = [0u8; 116];
        header[0..4].copy_from_slice(&w3d_make_version(4, 2).to_le_bytes());
        header[40..44].copy_from_slice(&2u32.to_le_bytes());
        header[44..48].copy_from_slice(&3u32.to_le_bytes());
        bytes.extend_from_slice(&W3D_CHUNK_MESH_HEADER3.to_le_bytes());
        bytes.extend_from_slice(&116u32.to_le_bytes());
        bytes.extend_from_slice(&header);
        // two of three vertex records
        bytes.extend_from_slice(&W3D_CHUNK_VERTICES.to_le_bytes());
        bytes.extend_from_slice(&24u32.to_le_bytes());
        bytes.extend_from_slice(&[0u8; 24]);
        // a user text chunk that must never be reached
        bytes.extend_from_slice(&W3D_CHUNK_MESH_USER_TEXT.to_le_bytes());
        bytes.extend_from_slice(&6u32.to_le_bytes());
        bytes.extend_from_slice(b"never\0");

        let mesh = W3dMesh::parse(&bytes).expect("the load succeeds despite the short read");
        let vertices = mesh.geometry.vertices.unwrap();
        assert_eq!(vertices.len(), 3);
        assert_eq!(vertices[2], [0.0, 0.0, 0.0]); // the zero-filled tail
        assert!(mesh.geometry.user_text.is_none()); // never reached
    }

    #[test]
    fn prelit_selection_enters_at_the_game_mode() {
        // the mode is LIGHTMAP_MULTI_PASS: a mesh carrying only the
        // multi-texture flag falls straight to the unlit default
        fn mesh_with(attributes: u32) -> W3dMesh {
            let mut bytes = Vec::new();
            let mut header = [0u8; 116];
            header[0..4].copy_from_slice(&w3d_make_version(4, 2).to_le_bytes());
            header[4..8].copy_from_slice(&attributes.to_le_bytes());
            bytes.extend_from_slice(&W3D_CHUNK_MESH_HEADER3.to_le_bytes());
            bytes.extend_from_slice(&116u32.to_le_bytes());
            bytes.extend_from_slice(&header);
            W3dMesh::parse(&bytes).unwrap()
        }
        let mesh = mesh_with(W3D_MESH_FLAG_PRELIT_LIGHTMAP_MULTI_TEXTURE);
        assert_eq!(mesh.prelit_chunk_id, W3D_CHUNK_PRELIT_UNLIT);
        assert_eq!(mesh.flags & FG_PRELIT_LIGHTMAP_MULTI_TEXTURE, 0);

        let mesh = mesh_with(W3D_MESH_FLAG_PRELIT_LIGHTMAP_MULTI_PASS);
        assert_eq!(mesh.prelit_chunk_id, W3D_CHUNK_PRELIT_LIGHTMAP_MULTI_PASS);
        assert_eq!(
            mesh.flags & FG_PRELIT_LIGHTMAP_MULTI_PASS,
            FG_PRELIT_LIGHTMAP_MULTI_PASS
        );

        let mesh = mesh_with(W3D_MESH_FLAG_PRELIT_VERTEX);
        assert_eq!(mesh.prelit_chunk_id, W3D_CHUNK_PRELIT_VERTEX);

        let mesh = mesh_with(W3D_MESH_FLAG_PRELIT_UNLIT);
        assert_eq!(mesh.prelit_chunk_id, W3D_CHUNK_PRELIT_UNLIT);
        assert_eq!(mesh.flags & FG_PRELIT_VERTEX, 0);

        // no prelit bits at all: the obsolete lightmapped flag still sets
        // the multi-pass geometry flag
        let mesh = mesh_with(OBSOLETE_W3D_MESH_FLAG_LIGHTMAPPED);
        assert_eq!(mesh.prelit_chunk_id, 0xffff_ffff);
        assert_eq!(
            mesh.flags & FG_PRELIT_LIGHTMAP_MULTI_PASS,
            FG_PRELIT_LIGHTMAP_MULTI_PASS
        );
    }

    #[test]
    fn geometry_flags_follow_the_header() {
        fn mesh_with(attributes: u32) -> W3dMesh {
            let mut bytes = Vec::new();
            let mut header = [0u8; 116];
            header[0..4].copy_from_slice(&w3d_make_version(4, 2).to_le_bytes());
            header[4..8].copy_from_slice(&attributes.to_le_bytes());
            header[40..44].copy_from_slice(&1u32.to_le_bytes());
            header[44..48].copy_from_slice(&3u32.to_le_bytes());
            bytes.extend_from_slice(&W3D_CHUNK_MESH_HEADER3.to_le_bytes());
            bytes.extend_from_slice(&116u32.to_le_bytes());
            bytes.extend_from_slice(&header);
            W3dMesh::parse(&bytes).unwrap()
        }
        let mesh = mesh_with(W3D_MESH_FLAG_GEOMETRY_TYPE_CAMERA_ALIGNED);
        assert_eq!(mesh.flags & FG_ALIGNED, FG_ALIGNED);
        let mesh = mesh_with(W3D_MESH_FLAG_GEOMETRY_TYPE_SKIN);
        assert_eq!(mesh.flags & FG_SKIN, FG_SKIN);
        assert_eq!(mesh.flags & FG_ALLOW_NPATCHES, FG_ALLOW_NPATCHES);
        let mesh = mesh_with(W3D_MESH_FLAG_GEOMETRY_TYPE_CAMERA_ORIENTED);
        assert_eq!(mesh.flags & FG_ORIENTED, FG_ORIENTED);
        // OBBOX hits the default branch: no geometry flags
        let mesh = mesh_with(0x0005_0000);
        assert_eq!(mesh.flags, 0);
        // below 4.1 the geometry-type mask is not consulted at all
        let mut bytes = Vec::new();
        let mut header = [0u8; 116];
        header[0..4].copy_from_slice(&w3d_make_version(4, 0).to_le_bytes());
        header[4..8].copy_from_slice(&W3D_MESH_FLAG_GEOMETRY_TYPE_SKIN.to_le_bytes());
        bytes.extend_from_slice(&W3D_CHUNK_MESH_HEADER3.to_le_bytes());
        bytes.extend_from_slice(&116u32.to_le_bytes());
        bytes.extend_from_slice(&header);
        let mesh = W3dMesh::parse(&bytes).unwrap();
        assert_eq!(mesh.flags & FG_SKIN, 0);
    }

    #[test]
    fn pre30_skin_fixup_adds_the_root_bone() {
        let mut bytes = Vec::new();
        let mut header = [0u8; 116];
        header[0..4].copy_from_slice(&w3d_make_version(2, 0).to_le_bytes());
        header[40..44].copy_from_slice(&1u32.to_le_bytes());
        header[44..48].copy_from_slice(&3u32.to_le_bytes());
        bytes.extend_from_slice(&W3D_CHUNK_MESH_HEADER3.to_le_bytes());
        bytes.extend_from_slice(&116u32.to_le_bytes());
        bytes.extend_from_slice(&header);
        // three influences: bones 0, 1, 2
        bytes.extend_from_slice(&W3D_CHUNK_VERTEX_INFLUENCES.to_le_bytes());
        bytes.extend_from_slice(&24u32.to_le_bytes());
        for bone in [0u16, 1, 2] {
            let mut vinf = [0u8; 8];
            vinf[0..2].copy_from_slice(&bone.to_le_bytes());
            bytes.extend_from_slice(&vinf);
        }
        let mesh = W3dMesh::parse(&bytes).unwrap();
        assert_eq!(mesh.geometry.bone_links, Some(vec![1u16, 2, 3]));
        assert_eq!(mesh.flags & FG_SKIN, FG_SKIN);
    }

    #[test]
    fn uv_install_dedups_by_crc_and_stops_at_eight() {
        // distinct arrays in all eight (pass, stage) slots
        let mut desc = MatDesc::default();
        for pass in 0..MAX_PASSES {
            for stage in 0..MAX_TEX_STAGES {
                let unique = [[f32::from(pass as u8 * 8 + stage as u8), 0.5]];
                desc.install_uv_array(pass, stage, &unique);
            }
        }
        assert_eq!(desc.uv_array_count(), MAX_UV_ARRAYS);
        // a ninth distinct array: the install silently does nothing (the
        // original's slot search reads past the array and its bound check
        // discards the value)
        desc.install_uv_array(0, 0, &[[9.5f32, 9.5]]);
        assert_eq!(desc.uv_array_count(), MAX_UV_ARRAYS);
        assert_eq!(desc.uv_source[0][0], 0);

        // the same pairs in another slot: deduplicated to the same array
        let mut desc = MatDesc::default();
        let pairs_a = [[0.0f32, 0.5], [0.25, 0.75]];
        let pairs_b = [[1.0f32, 0.125]];
        desc.install_uv_array(0, 0, &pairs_a);
        assert_eq!(desc.uv_source[0][0], 0);
        desc.install_uv_array(0, 1, &pairs_a);
        assert_eq!(desc.uv_source[0][1], 0);
        assert_eq!(desc.uv_array_count(), 1);
        // a distinct array takes the next slot
        desc.install_uv_array(1, 0, &pairs_b);
        assert_eq!(desc.uv_source[1][0], 1);
        assert_eq!(desc.uv_array_count(), 2);
    }

    #[test]
    fn shader_only_alternates_count_as_empty() {
        let mut desc = MatDesc::default();
        desc.set_single_shader(0x0009_441b, 0);
        assert!(desc.has_shader_data(0));
        // Is_Empty never looks at the shader data
        assert!(desc.is_empty());
        // one colour array is enough to make it non-empty
        desc.vertex_count = 1;
        let _ = desc.color_arrays[0].get_or_insert(vec![0u32; 1]);
        assert!(!desc.is_empty());
    }

    #[test]
    fn vertex_material_crc_is_field_sensitive() {
        let a = VertexMaterial::default();
        let mut b = VertexMaterial::default();
        assert_eq!(a.compute_crc(), b.compute_crc());
        b.opacity = 0.5;
        assert_ne!(a.compute_crc(), b.compute_crc());
        b = VertexMaterial::default();
        b.ambient = [0.5, 1.0, 1.0];
        assert_ne!(a.compute_crc(), b.compute_crc());
        // the name is not part of the CRC (the original excludes it)
        b = VertexMaterial::default();
        b.name = "different".to_string();
        assert_eq!(a.compute_crc(), b.compute_crc());
    }

    #[test]
    fn texture_info_round_trips() {
        let info = W3dTextureInfo {
            attributes: 0x000C,
            anim_type: 1,
            frame_count: 7,
            frame_rate: 15.5,
        };
        assert_eq!(W3dTextureInfo::from_bytes(&info.to_bytes()), info);
    }

    #[test]
    fn error_display_is_stable() {
        assert_eq!(MeshError::LoadFailed.to_string(), "W3D mesh load failed");
        assert_eq!(
            MeshError::SubReadFailed.to_string(),
            "W3D mesh sub-reader failed"
        );
    }
}
