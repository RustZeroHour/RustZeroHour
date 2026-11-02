// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Rust port Copyright 2026 the RustZeroHour contributors.
//
// Ported from the pinned GameMath sources (TheSuperHackers/GameMath,
// commit 59f7ccd494f7e7c916a784ac26ef266f9f09d78d, fetched by
// cmake/gamemath.cmake), FreeBSD msun float routines:
//
//   float/s_sinf.c   (gm_sinf)
//   float/s_cosf.c   (gm_cosf)
//   float/s_tanf.c   (gm_tanf)
//
// The originals carry the SunPro copyright notice, preserved per the
// migration plan's porting rules (§3.3):
//
//   Copyright (C) 1993 by Sun Microsystems, Inc. All rights reserved.
//   Developed at SunPro, a Sun Microsystems, Inc. business.
//   Permission to use, copy, modify, and distribute this software is freely
//   granted, provided that this notice is preserved.
//
//! `sinf`, `cosf`, `tanf` — the float trigonometric entry points.
//!
//! These are the functions `WWMath::Sinf/Cosf/Tanf` call under
//! `USE_DETERMINISTIC_MATH = 1` (the branch this port follows, ADR-000/001).
//!
//! # Parity level
//!
//! **P0 exact**: golden vectors in `tests/golden/gmath.txt` cover the
//! small-argument path, both medium reduction paths and huge arguments
//! (up to 2^120), plus infinities and NaNs.

// Clippy notes for this verbatim msun transcription:
// - `excessive_precision` / `approx_constant`: the float literals keep the
//   exact decimal forms of the C sources (several approximate pi/e by
//   design); rounding them to fewer digits changes bits.
// - `eq_op`: NaN/overflow sentinels written as `x - x` / `(x-x)/(x-x)` in
//   the original are behaviour.
#![allow(clippy::excessive_precision, clippy::approx_constant, clippy::eq_op)]

use super::ktrig::{kernel_cosf, kernel_sinf, kernel_tanf};
use super::rem_pio2f::ieee754_rem_pio2f;

/// `gm_sinf`: the sine of `x` (radians).
///
/// Ported from `float/s_sinf.c`. The `if(((int)x)==0)` inexact-trigger of
/// the original is dropped: Rust has no floating-point exception state and
/// the value returned is identical.
#[must_use]
pub fn sinf(x: f32) -> f32 {
    let mut y = [0.0f32; 2];
    let z = 0.0f32;

    let ix = (x.to_bits() as i32) & 0x7fff_ffff;
    // |x| ~< pi/4
    if ix <= 0x3f490fd8 {
        // if x < 2**-12
        if ix < 0x3980_0000 && (x as i32) == 0 {
            return x; // generate inexact
        }
        return kernel_sinf(x, z, 0);
    }
    // sin(Inf or NaN) is NaN
    if ix >= 0x7f80_0000 {
        return x - x;
    }

    // argument reduction needed
    let n = ieee754_rem_pio2f(x, &mut y);
    match n & 3 {
        0 => kernel_sinf(y[0], y[1], 1),
        1 => kernel_cosf(y[0], y[1]),
        2 => -kernel_sinf(y[0], y[1], 1),
        _ => -kernel_cosf(y[0], y[1]),
    }
}

/// `gm_cosf`: the cosine of `x` (radians).
///
/// Ported from `float/s_cosf.c`.
#[must_use]
pub fn cosf(x: f32) -> f32 {
    let mut y = [0.0f32; 2];
    let z = 0.0f32;

    let ix = (x.to_bits() as i32) & 0x7fff_ffff;
    // |x| ~< pi/4
    if ix <= 0x3f490fd8 {
        // if x < 2**-12
        if ix < 0x3980_0000 && (x as i32) == 0 {
            return 1.0; // generate inexact
        }
        return kernel_cosf(x, z);
    }
    // cos(Inf or NaN) is NaN
    if ix >= 0x7f80_0000 {
        return x - x;
    }

    // argument reduction needed
    let n = ieee754_rem_pio2f(x, &mut y);
    match n & 3 {
        0 => kernel_cosf(y[0], y[1]),
        1 => -kernel_sinf(y[0], y[1], 1),
        2 => -kernel_cosf(y[0], y[1]),
        _ => kernel_sinf(y[0], y[1], 1),
    }
}

/// `gm_tanf`: the tangent of `x` (radians).
///
/// Ported from `float/s_tanf.c`.
#[must_use]
pub fn tanf(x: f32) -> f32 {
    let mut y = [0.0f32; 2];
    let z = 0.0f32;

    let ix = (x.to_bits() as i32) & 0x7fff_ffff;
    // |x| ~< pi/4
    if ix <= 0x3f490fda {
        return kernel_tanf(x, z, 1);
    }
    // tan(Inf or NaN) is NaN
    if ix >= 0x7f80_0000 {
        return x - x; // NaN
    }

    // argument reduction needed
    let n = ieee754_rem_pio2f(x, &mut y);
    //   1 -- n even, -1 -- n odd
    kernel_tanf(y[0], y[1], 1 - ((n & 1) << 1))
}
