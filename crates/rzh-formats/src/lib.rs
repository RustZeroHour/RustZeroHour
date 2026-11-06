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
//! - [`map_chunk`]: the map data-chunk container — the `CkMp` framing
//!   layer of `Common/System/DataChunk.cpp` every `.map` payload
//!   (height maps, blend tiles, global lighting, world info, object
//!   lists, polygon triggers, sides lists) is stored in: the
//!   name-interning table of contents, the nested chunk tree with its
//!   signed data-left bookkeeping, the parser registration surface the
//!   map loaders drive, the load-path half of `Dict` and the name-key
//!   interning its pairs sort on — the map-files counterpart of the
//!   W3D [`chunk`] container, and the base the map payload parsers
//!   (the next slices) build on.
//! - [`map_heightmap`]: the map height-map payload — the
//!   `HeightMapData` chunk reader of `WorldHeightMap.cpp`
//!   (`ParseHeightMapData` and the logical-data-only `ParseSizeOnly`):
//!   the version-gated border and boundary reads, the map-size and
//!   sample-count contracts, and the version-1 in-place downsample
//!   with its mode-divergent `width`/`height` update — the first map
//!   payload parser on top of the data-chunk container.
//! - [`map_blendtile`]: the map blend-tile payload — the
//!   `BlendTileData` chunk reader of `WorldHeightMap.cpp`
//!   (`ParseBlendTileData`, the largest map payload): the four
//!   per-cell index arrays, the flip/cliff state bitmaps with the
//!   version-7 row-length remap and the pre-version-7 height
//!   derivation, the texture and edge classes, the blend records with
//!   their `FLAG_VAL` sentinels, the cliff u/v tables, the version-1
//!   downsample that halves the shared extent members, the 3-way
//!   blend gate, and the constructor's post-walk patch loop — over
//!   the same container, on top of [`map_heightmap`]'s half of the
//!   shared walk state.
//! - [`map_lighting`]: the map global-lighting payload — the
//!   `GlobalLighting` chunk reader of `WorldHeightMap.cpp`
//!   (`ParseLightingDataChunk`, the full load's other payload): the
//!   raw time-of-day word, the init pass that fills both light tables
//!   with the not-totally-invalid defaults, the four interleaved
//!   per-time-of-day blocks of terrain and object lights with their
//!   version gates, and the bytes-left-gated shadow colour — a reader
//!   with no failure mode at all, over the same container.
//! - [`map_objects`]: the map objects-list payload — the
//!   `ObjectsList`/`Object` chunk readers of `WorldHeightMap.cpp`
//!   (`ParseObjectsDataChunk` and `ParseObjectData`, the logical
//!   load's other payload): the location triple with the
//!   version-gated z zeroing, the `normalizeAngle` wrap in its mixed
//!   precision, the editor flags, the type name, the property dict
//!   with the well-known-key derivations (waypoint, light, scorch),
//!   the z-range drops, and the head-overwrite list semantics — the
//!   first payload with a nested parser registration, over the same
//!   container.
//! - [`map_worldinfo`]: the map world-info payload — the `WorldInfo`
//!   chunk reader of `WorldHeightMap.cpp`
//!   (`ParseWorldDictDataChunk`, the logical load's settings dict): the
//!   dict that replaces the world global whole on every chunk, and the
//!   weather word its exact-type `INT` query re-derives (the raw word,
//!   never range-checked, sticky across chunks without the pair) —
//!   over the same container.
//! - [`map_polygontriggers`]: the map polygon-triggers payload — the
//!   `PolygonTriggers` chunk reader of `PolygonTrigger.cpp`
//!   (`ParsePolygonTriggersDataChunk`, the logical load's named areas):
//!   the flat version-gated record loop, the fewer-than-two-point
//!   deletion, the version-1 auto-added water area whose id duplicates
//!   the maximum, and the id counter the load leaves behind — over the
//!   same container.
//! - [`map_sideslist`]: the map sides-list payload — the
//!   `SidesList` chunk reader of `SidesList.cpp`
//!   (`ParseSidesDataChunk` with `validateSides`, the logical load's
//!   player setup): the bounded side loop with its build lists, the
//!   team list, the nested scripts walk (the script-engine slice's
//!   boundary) and the post-load cleanup (the neutral ensure, the
//!   default teams, the allies/enemies token cleanup, the collision
//!   removal and the owner reparent) — the last of the map payload
//!   family, over the same container.
//!
//! Deferred to later Phase 2 slices, in plan order: the remaining W3D
//! modules (…), the map payloads' nested `PlayerScriptsList` (the
//! script-engine slice, plan §3.4 WP7), WND layouts, fonts
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
pub mod map_blendtile;
pub mod map_chunk;
pub mod map_heightmap;
pub mod map_lighting;
pub mod map_objects;
pub mod map_polygontriggers;
pub mod map_sideslist;
pub mod map_worldinfo;
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
