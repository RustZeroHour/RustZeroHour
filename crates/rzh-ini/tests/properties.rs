// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
//! Property tests (plan §6 test layers) for the INI scalar scanners: the
//! round-trip invariants `scan_int(format!(x)) == x` and
//! `scan_real(format!(x)) == x` over exactly representable values.
//!
//! The legacy-parser quirks (out-of-range saturation, unsigned negation
//! wrap, real range errors) are pinned by the ported GoogleTest suites in
//! `src/scan.rs`; these properties state the contracts that must hold for
//! every in-range input.

use proptest::prelude::*;

use rzh_ini::scan::{scan_int, scan_real};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// Every `i32` scans back from its decimal spelling.
    #[test]
    fn ints_round_trip_through_their_decimal_spelling(value in any::<i32>()) {
        let token = format!("{value}");
        prop_assert_eq!(scan_int(token.as_bytes()).ok(), Some(value));
    }

    /// Every non-negative `u32` scans back too (the legacy grammar's
    /// unsigned arm).
    #[test]
    fn unsigned_ints_round_trip(value in any::<u32>()) {
        let token = format!("{value}");
        prop_assert_eq!(scan_int(token.as_bytes()).ok(), Some(value as i32));
    }

    /// Quarter-step floats — exactly representable, like the golden
    /// vectors' `stepFloat` — scan back from their shortest spelling.
    #[test]
    fn quarter_step_reals_round_trip(quarter in 0i64..80_000) {
        let value = ((quarter - 40_000) as f32) * 0.25;
        let token = format!("{value}");
        prop_assert_eq!(scan_real(token.as_bytes()).ok(), Some(value));
    }
}
