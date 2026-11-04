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
//! - [`chunk`]: the W3D chunk container — the IFF-style framing layer
//!   (`WWLib/chunkio.cpp`) every W3D asset (meshes, skeletons,
//!   animations, HLOD) is stored in; the base the upcoming W3D parsers
//!   are built on.
//!
//! Deferred to later Phase 2 slices, in plan order: the W3D format
//! modules themselves (mesh, HLOD, animation, …), map files, WND
//! layouts, DDS textures, fonts and audio containers. Consumer-side
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
pub mod tga;

pub use chunk::{
    CHUNK_HEADER_SIZE, CHUNK_SIZE_MASK, CHUNK_SUB_CHUNK_FLAG, ChunkError, ChunkHeader, ChunkLoad,
    ChunkSave, MAX_CHUNK_DEPTH, MAX_SAVE_DEPTH, MICRO_CHUNK_HEADER_SIZE, MICRO_CHUNK_MAX_SIZE,
    MicroChunkHeader,
};
pub use csf::{CsfError, CsfFile, CsfHeader, CsfLabel, CsfString};
pub use tga::{
    TGA_CMAPPED, TGA_CMAPPED_ENCODED, TGA_MONO, TGA_MONO_ENCODED, TGA_NOIMAGE, TGA_TRUECOLOR,
    TGA_TRUECOLOR_ENCODED, TGA2_SIGNATURE, TGAIDF_ATTRIB_BITS, TGAIDF_XORIGIN, TGAIDF_YORIGIN,
    Tga2Extension, Tga2Footer, TgaError, TgaFile, TgaHeader,
};
