// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
//! EA compression codecs for the RustZeroHour data pipeline.
//!
//! Phase 1 crate of the Rust migration plan (§3.4): the codecs the BIG
//! archive reader needs. Currently:
//!
//! - [`refpack`]: the RefPack LZ codec (`EAC/refdecode.cpp`,
//!   `EAC/refencode.cpp`) — byte-exact with the C++ implementation, in
//!   both directions (P0, golden-pinned).
//!
//! Deferred (documented in the plan): the Huffman and B-tree EAC codecs
//! (not reachable from Zero Hour data), the Nox LZH codec, and the
//! `CompressionManager` factory (arrives with its consumers in
//! `rzh-vfs`).
//!
//! This crate is data-path code, not simulation code: no `unsafe`, no
//! global state, and malformed input returns [`Result`] errors instead
//! of the C's undefined behaviour.

#![forbid(unsafe_code)]

pub mod refpack;

pub use refpack::{RefPackError, compress, decompress, is_refpack};
