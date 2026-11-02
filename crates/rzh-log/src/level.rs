// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Re-implements the level mask of Core/Libraries/Source/Lib/Debug.cpp
// (`DebugLevelMask`, `TheDebugLevels`) at baseline 571a7c5.
//
//! The `DebugLevelMask` bitmask: per-subsystem log gates.
//!
//! The original held a global `UnsignedInt DebugLevelMask = 0` consulted
//! by `DEBUG_LOG_LEVEL(l, m)` as `if (l & DebugLevelMask)`, with one
//! named level, `"NET"`. Its enum `DEBUG_LEVEL_NET = 0` made the gate
//! dead code (a value of `0` ANDed with anything is `0`); the
//! re-implementation fixes that by defining the levels as real bit
//! patterns — see ADR-002.
//!
//! ```
//! use rzh_log::level;
//!
//! assert!(!level::enabled(level::LEVEL_NET));
//! level::set_level_mask(level::LEVEL_NET);
//! assert!(level::enabled(level::LEVEL_NET));
//! level::set_level_mask(0);
//! ```

use std::sync::atomic::{AtomicU32, Ordering};

/// `DEBUG_LEVEL_NET`: the in-game network level, bit 0.
///
/// The original's enum value was `0`, which could never pass the
/// `l & DebugLevelMask` gate; here it is the bit pattern `1 << 0` so
/// the mask actually works (ADR-002 deviation).
pub const LEVEL_NET: u32 = 1 << 0;

/// `DEBUG_LEVEL_MAX`: the number of named levels.
pub const LEVEL_MAX: u32 = 1;

/// `TheDebugLevels`: the printable names, indexed by level bit.
pub const LEVEL_NAMES: [&str; LEVEL_MAX as usize] = ["NET"];

/// The global mask, `DebugLevelMask = 0` in the original: everything
/// gated off until the game (or the command line) turns levels on.
static MASK: AtomicU32 = AtomicU32::new(0);

/// `DebugLevelMask = mask`: replaces the whole mask.
pub fn set_level_mask(mask: u32) {
    MASK.store(mask, Ordering::Relaxed);
}

/// Reads `DebugLevelMask`.
#[must_use]
pub fn level_mask() -> u32 {
    MASK.load(Ordering::Relaxed)
}

/// The `l & DebugLevelMask` gate: true when any of the level's bits are
/// enabled in the mask.
#[must_use]
pub fn enabled(level: u32) -> bool {
    level & level_mask() != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mask_starts_empty_and_gates_levels() {
        // The original default: DebugLevelMask = 0, so even NET is off.
        set_level_mask(0);
        assert!(!enabled(LEVEL_NET));
        assert!(!enabled(u32::MAX));

        set_level_mask(LEVEL_NET);
        assert!(enabled(LEVEL_NET));
        // Unknown bits gate nothing on their own.
        assert!(!enabled(1 << 31));

        set_level_mask(0);
        assert!(!enabled(LEVEL_NET));
    }

    #[test]
    fn level_names_stay_indexed() {
        assert_eq!(LEVEL_NAMES.len(), LEVEL_MAX as usize);
        assert_eq!(LEVEL_NAMES[0], "NET");
    }
}
