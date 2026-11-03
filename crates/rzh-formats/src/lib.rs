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
//!
//! Deferred to later Phase 2 slices, in plan order: map files, WND
//! layouts, W3D meshes/animations, TGA/DDS textures, fonts and audio
//! containers. Consumer-side normalizations the original performs on top
//! of a format (space stripping, first-string selection, …) live in the
//! format modules as explicit `game_*` helpers so the parser stays
//! byte-faithful and the behaviour stays pinned.
//!
//! This crate is data-path code, not simulation code: no `unsafe`, no
//! global state, and malformed input returns [`Result`] errors where the
//! C++ reads out of bounds.

#![forbid(unsafe_code)]

pub mod csf;

pub use csf::{CsfError, CsfFile, CsfHeader, CsfLabel, CsfString};
