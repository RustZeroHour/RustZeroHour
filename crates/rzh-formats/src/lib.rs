// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
//! Asset-format parsers for the RustZeroHour data pipeline.
//!
//! Phase 2 crate of the Rust migration plan (§3.4, stage B "Data" of
//! §3.1): parsing only — asset bytes go in, typed structures come out,
//! byte-exactly (parity level P0, golden-pinned against C++
//! transcriptions in `tools/differential/`). Currently:
//!
//! - [`csf`]: the compiled string tables (`.csf`) — the runtime reader
//!   (`GameText.cpp`) and the Babylon writer (`Core/Tools/Babylon`).
//! - [`tga`]: the Targa images (`.tga`) — the WWLib `TARGA.cpp` class the
//!   texture loader, the font loader and the screenshot path all drive.
//! - [`dds`]: the DDS textures (`.dds`) — the WW3D `ddsfile.cpp` reader
//!   the compressed-texture load path drives: the `.dds` name munging,
//!   the never-validated magic, the legacy `DDSURFACEDESC2` decode, the
//!   FourCC format conversion, the reduction/drop mip-level layout,
//!   the read window, the DXT1/DXT5 pixel queries and the block
//!   decompressor with its per-destination-format writes, the surface
//!   copies (the DXT1→DXT2 expansion and the hue-shift re-encodes) and
//!   the HSV recolour stack the team-colour paths drive.
//! - [`chunk`]: the W3D chunk container — the IFF-style framing layer
//!   (`WWLib/chunkio.cpp`) every W3D asset (meshes, skeletons,
//!   animations, HLOD) is stored in; the base the W3D parsers
//!   are built on.
//! - [`w3d_mesh`]: the W3D mesh chunk tree — the data path of
//!   `MeshModelClass::Load_W3D` (`WW3D2/meshmdlio.cpp`): the header,
//!   the geometry chunks, the material tree with its passes, prelit
//!   selection and alternate-description routing, and the AABTree. The
//!   first real W3D model module, unlocking the plan's model-viewer
//!   milestone (M3).
//! - [`w3d_htree`]: the W3D hierarchy trees — the data path of
//!   `HTreeClass::Load_W3D` (`WW3D2/htree.cpp`): the 36-byte header, the
//!   60-byte pivots with their base-pose transforms (the original's
//!   mixed-precision `Build_Matrix3D` included), the pre-3.0 root
//!   synthesis and parent fixup, and the `stricmp` bone lookup the
//!   skinning path resolves bones against.
//! - [`w3d_hanim`]: the W3D raw animations — the data path of
//!   `HRawAnimClass::Load_W3D` (`WW3D2/hrawanim.cpp`) and the classic
//!   motion/bit channels it drives (`WW3D2/motchan.cpp`): the 44-byte
//!   header with its integer frame rate and strcpy-run name
//!   composition, the pre-3.0 pivot shift, the per-bone channel slots,
//!   and the frame queries (`Get_Transform` and friends, the
//!   table-based `Fast_Slerp` included) the tree update samples to fill
//!   a skeleton's transforms.
//! - [`w3d_hcanim`]: the W3D compressed animations — the data path of
//!   `HCompressedAnimClass::Load_W3D` (`WW3D2/hcanim.cpp`) and the
//!   compressed channel readers it drives (`WW3D2/motchan.cpp`'s
//!   time-coded and adaptive-delta channels): the flavor-picked header
//!   decode, the stateful packet queries with their scan/binary-search
//!   and two-frame decompression caches (the cache-continue off-by-one
//!   transcribed verbatim), and the same query surface the tree update
//!   samples — the compressed motion the game's assets ship in.
//! - [`w3d_hlod`]: the W3D HLOD assemblies — the data path of
//!   `HLodDefClass::Load_W3D` (`WW3D2/hlod.cpp`): the 40-byte header
//!   with its `strdup`'d names (the run through the following header
//!   words included), one sub-object array per level of detail
//!   (model name + bone pairs), and the aggregate and proxy arrays —
//!   the model format Zero Hour's units and buildings are assembled
//!   in, on top of the chunk container and the hierarchy/animation
//!   slices before it.
//!
//! Deferred to later Phase 2 slices, in plan order: the remaining W3D
//! modules (…), map files, WND layouts, fonts
//! and audio containers (the DDS textures are done). Consumer-side
//! normalizations the original performs on top
//! of a format (space stripping, first-string selection, the texture
//! loader's descriptor XOR, …) live in the format modules as explicit
//! helpers or parameters so the parser stays byte-faithful and the
//! behaviour stays pinned.
//!
//! This crate is data-path code, not simulation code: no `unsafe`, no
//! global state, and malformed input returns [`Result`] errors where the
//! C++ reads out of bounds.

#![forbid(unsafe_code)]

pub mod chunk;
pub mod csf;
pub mod dds;
pub mod tga;
pub mod w3d_hanim;
pub mod w3d_hcanim;
pub mod w3d_hlod;
pub mod w3d_htree;
pub mod w3d_mesh;

pub use chunk::{
    CHUNK_HEADER_SIZE, CHUNK_SIZE_MASK, CHUNK_SUB_CHUNK_FLAG, ChunkError, ChunkHeader, ChunkLoad,
    ChunkSave, MAX_CHUNK_DEPTH, MAX_SAVE_DEPTH, MICRO_CHUNK_HEADER_SIZE, MICRO_CHUNK_MAX_SIZE,
    MicroChunkHeader,
};
pub use csf::{CsfError, CsfFile, CsfHeader, CsfLabel, CsfString};
pub use dds::{
    DDS_MAGIC_SIZE, DDS_SURFACE_DESC_SIZE, DDSCAPS2_CUBEMAP, DDSCAPS2_VOLUME, DdsError, DdsFile,
    DdsType, HIGHEST_SUPPORTED_D3DFORMAT, HsvShift, LegacyDdCaps2, LegacyDdColorKey,
    LegacyDdPixelFormat, LegacyDdSurfaceDesc2, MAX_DATA_BYTES, MAX_MIP_LEVELS, MAX_NAME_LEN,
    Ww3dFormat, argb8888_to_rgb565, calculate_dxtc_surface_size, munged_name, recolor,
    rgb565_to_argb8888,
};
pub use tga::{
    TGA_CMAPPED, TGA_CMAPPED_ENCODED, TGA_MONO, TGA_MONO_ENCODED, TGA_NOIMAGE, TGA_TRUECOLOR,
    TGA_TRUECOLOR_ENCODED, TGA2_SIGNATURE, TGAIDF_ATTRIB_BITS, TGAIDF_XORIGIN, TGAIDF_YORIGIN,
    Tga2Extension, Tga2Footer, TgaError, TgaFile, TgaHeader,
};
pub use w3d_hanim::{
    ANIM_CHANNEL_Q, ANIM_CHANNEL_X, ANIM_CHANNEL_XR, ANIM_CHANNEL_Y, ANIM_CHANNEL_YR,
    ANIM_CHANNEL_Z, ANIM_CHANNEL_ZR, BIT_CHANNEL_VIS, CLASSID_HRAWANIM, HanimError,
    MAX_CHANNEL_FLOATS, W3D_CHUNK_ANIMATION, W3D_CHUNK_ANIMATION_CHANNEL,
    W3D_CHUNK_ANIMATION_HEADER, W3D_CHUNK_BIT_CHANNEL, W3dAnimHeader, W3dBitChannel,
    W3dMotionChannel, W3dNodeMotion, W3dRawAnim,
};
pub use w3d_hcanim::{
    ADAPTIVE_DELTA_CHANNEL_STRUCT_SIZE, ADAPTIVE_DELTA_PACKET_SIZE, ANIM_FLAVOR_ADAPTIVE_DELTA,
    ANIM_FLAVOR_TIMECODED, COMPRESSED_ANIM_HEADER_SIZE, HcanimError,
    TIMECODED_ANIM_CHANNEL_STRUCT_SIZE, TIMECODED_BIT_CHANNEL_STRUCT_SIZE,
    W3D_CHUNK_COMPRESSED_ANIMATION, W3D_CHUNK_COMPRESSED_ANIMATION_CHANNEL,
    W3D_CHUNK_COMPRESSED_ANIMATION_HEADER, W3D_CHUNK_COMPRESSED_BIT_CHANNEL,
    W3D_TIMECODED_BINARY_MOVEMENT_FLAG, W3D_TIMECODED_BIT_MASK, W3dCompressedAnim,
    W3dCompressedAnimHeader, W3dCompressedChannel, W3dCompressedNodeMotion,
};
pub use w3d_hlod::{
    HLOD_ARRAY_HEADER_SIZE, HLOD_HEADER_SIZE, HLOD_SUB_OBJECT_SIZE, HlodError, NO_MAX_SCREEN_SIZE,
    SUB_OBJECT_NAME_LEN, W3D_CHUNK_HLOD, W3D_CHUNK_HLOD_AGGREGATE_ARRAY, W3D_CHUNK_HLOD_HEADER,
    W3D_CHUNK_HLOD_LOD_ARRAY, W3D_CHUNK_HLOD_PROXY_ARRAY, W3D_CHUNK_HLOD_SUB_OBJECT,
    W3D_CHUNK_HLOD_SUB_OBJECT_ARRAY_HEADER, W3D_CURRENT_HLOD_VERSION, W3dHlod, W3dHlodArrayHeader,
    W3dHlodHeader, W3dHlodSubObjectStruct, W3dProxyRecord, W3dSubObjectArray,
};
pub use w3d_htree::{
    HtreeError, W3D_CHUNK_HIERARCHY, W3D_CHUNK_HIERARCHY_HEADER, W3D_CHUNK_PIVOT_FIXUPS,
    W3D_CHUNK_PIVOTS, W3D_CURRENT_HTREE_VERSION, W3D_NAME_LEN, W3dHierarchyHeader, W3dHtree,
    W3dPivot, W3dPivotStruct,
};
pub use w3d_mesh::{
    AabNode, AabTree, LegacyMaterial, MAX_PASSES, MAX_TEX_STAGES, MatDesc, MaterialInfo, MeshError,
    MeshGeometry, MeshHeader3, TextureEntry, UvArray, VertexMaterial, W3dMesh, W3dShaderStruct,
    W3dTextureInfo, W3dVertexMaterialInfo,
};
