// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Rust port Copyright 2026 the RustZeroHour contributors.
//
// Ported from the pinned GameMath sources (TheSuperHackers/GameMath,
// commit 59f7ccd494f7e7c916a784ac26ef266f9f09d78d, fetched by
// cmake/gamemath.cmake), FreeBSD msun float routines:
//
//   float/s_rintf.c   (gm_rintf)
//   float/s_lrintf.c  (gm_lrintf)
//
// The originals carry the SunPro copyright notice, preserved per the
// migration plan's porting rules (§3.3):
//
//   Copyright (C) 1993 by Sun Microsystems, Inc. All rights reserved.
//   Developed at SunPro, a Sun Microsystems, Inc. business.
//   Permission to use, copy, modify, and distribute this software is freely
//   granted, provided that this notice is preserved.
//
//! `rintf` and `lrintf`: round to integer (nearest, ties to even).
//!
//! `gm_lrintf` is what `WWMath::Float_To_Long` calls under
//! `USE_DETERMINISTIC_MATH = 1`.
//!
//! # Transcription notes
//!
//! The pin builds `gm_lrintf` with CPU intrinsics where available
//! (`BUILD_WITH_INTRINSICS`, on by default): `cvtss2si` on x86/x64, `fcvt`
//! on arm64. Both round to nearest-even, matching the generic
//! `(long)gm_rintf(x)` template path on every value where the C conversion
//! is defined. Where the C conversion would be undefined (NaN, or a value
//! outside the destination range) the intrinsics return the "integer
//! indefinite" value (i64::MIN / i32::MIN); the Rust port replicates that
//! behaviour explicitly instead of relying on Rust's saturating `as` casts,
//! which differ (NaN converts to 0).
//!
//! The `volatile` qualifiers in `s_rintf.c` only work around an old gcc
//! optimisation bug and have no semantic effect here.
//!
//! # Parity level
//!
//! **P0 exact**: golden vectors in `tests/golden/gmath.txt` (in-range
//! values compared against the compiled pin; the out-of-range sentinel
//! values are pinned by unit tests documenting the intrinsic semantics).

// Clippy notes for this verbatim msun transcription:
// - `excessive_precision` / `approx_constant`: the float literals keep the
//   exact decimal forms of the C sources (several approximate pi/e by
//   design); rounding them to fewer digits changes bits.
// - `eq_op`: NaN/overflow sentinels written as `x - x` / `(x-x)/(x-x)` in
//   the original are behaviour.
#![allow(clippy::excessive_precision, clippy::approx_constant, clippy::eq_op)]

/// `TWO23[2]`: ±2^23, the round-to-integer magic constant.
const TWO23: [f32; 2] = [8_388_608.0, -8_388_608.0]; // 0x4b000000, 0xcb000000

/// `gm_rintf`: round to the nearest integer value (ties to even), returned
/// as a float, in the current rounding mode (nearest in practice — the game
/// never changes it).
#[must_use]
pub fn rintf(x: f32) -> f32 {
    let i0 = x.to_bits() as i32;
    let sx = (i0 >> 31) & 1;
    let j0 = ((i0 >> 23) & 0xff) - 0x7f;
    if j0 < 23 {
        if j0 < 0 {
            if (i0 & 0x7fff_ffff) == 0 {
                return x;
            }
            let w = TWO23[sx as usize] + x;
            return w - TWO23[sx as usize];
        }
        let w = TWO23[sx as usize] + x;
        return w - TWO23[sx as usize];
    }
    if j0 == 0x80 {
        return x + x; // inf or NaN
    }
    x // x is integral
}

/// 2^63: the smallest f32 that converts to a value outside `i64`.
const TWO_P63: f32 = 9_223_372_036_854_775_808.0;

/// `gm_lrintf`: round to the nearest integer (ties to even) as an `i64`.
///
/// Matches `cvtss2si`/`fcvt` semantics: NaN and out-of-range values return
/// [`i64::MIN`] (the "integer indefinite" pattern). For every in-range
/// input this equals the C pin's `gm_lrintf` on all supported platforms.
#[must_use]
pub fn lrintf(x: f32) -> i64 {
    let r = rintf(x);
    if r.is_nan() || r >= TWO_P63 || r < -TWO_P63 {
        return i64::MIN;
    }
    r as i64
}
