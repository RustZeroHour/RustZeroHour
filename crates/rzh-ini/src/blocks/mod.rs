// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
//! The task-10 validation block types.
//!
//! `Armor` (complete), `Locomotor` (complete) and `Weapon` (a documented
//! representative subset) exercise the whole INI pipeline end to end —
//! block dispatch, the field loop, every generic scan/conversion
//! procedure, the name-key interning and the store semantics (including
//! the override machinery and its quirks). The fully typed game data —
//! the remaining ~190 field tables — is Phase 2's `rzh-data` work
//! (migration plan §4), built on this engine.

pub mod armor;
pub mod locomotor;
pub mod weapon;
