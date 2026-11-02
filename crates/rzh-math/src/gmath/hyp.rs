// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Rust port Copyright 2026 the RustZeroHour contributors.
//
// Ported from the pinned GameMath sources (TheSuperHackers/GameMath,
// commit 59f7ccd494f7e7c916a784ac26ef266f9f09d78d, fetched by
// cmake/gamemath.cmake), FreeBSD msun float routines:
//
//   float/e_sinhf.c   (gm_sinhf, via the __ieee754_sinhf alias)
//   float/e_coshf.c   (gm_coshf, via the __ieee754_coshf alias)
//   float/s_tanhf.c   (gm_tanhf)
//
// The originals carry the SunPro copyright notice, preserved per the
// migration plan's porting rules (§3.3):
//
//   Copyright (C) 1993 by Sun Microsystems, Inc. All rights reserved.
//   Developed at SunPro, a Sun Microsystems, Inc. business.
//   Permission to use, copy, modify, and distribute this software is freely
//   granted, provided that this notice is preserved.
//
//! `sinhf`, `coshf`, `tanhf` — the float hyperbolic functions.
//!
//! These are the functions `WWMath::Sinhf/Coshf/Tanhf` call under
//! `USE_DETERMINISTIC_MATH = 1`.
//!
//! # Parity level
//!
//! **P0 exact**: golden vectors in `tests/golden/gmath.txt`.

// Clippy notes for this verbatim msun transcription:
// - `excessive_precision` / `approx_constant`: the float literals keep the
//   exact decimal forms of the C sources (several approximate pi/e by
//   design); rounding them to fewer digits changes bits.
// - `eq_op`: NaN/overflow sentinels written as `x - x` / `(x-x)/(x-x)` in
//   the original are behaviour.
#![allow(clippy::excessive_precision, clippy::approx_constant, clippy::eq_op)]

use super::explog::{expf, expm1f, ldexp_expf};

const ONE: f32 = 1.0;
const TWO: f32 = 2.0;
const HALF: f32 = 0.5;
const SHUGE: f32 = 1.0e37;

/// `gm_sinhf`: the hyperbolic sine of `x`.
///
/// Ported from `float/e_sinhf.c` (`__ieee754_sinhf`).
#[must_use]
pub fn sinhf(x: f32) -> f32 {
    let jx = x.to_bits() as i32;
    let ix = jx & 0x7fff_ffff;

    // x is INF or NaN
    if ix >= 0x7f80_0000 {
        return x + x;
    }
    let mut h = HALF;
    if jx < 0 {
        h = -h;
    }
    // |x| in [0, 9], return sign(x)*0.5*(E+E/(E+1)))
    if ix < 0x4110_0000 {
        // |x| < 9
        if ix < 0x3980_0000 {
            // |x| < 2**-12
            if SHUGE + x > ONE {
                return x; // sinh(tiny) = tiny with inexact
            }
        }
        let t = expm1f(f32::abs(x)); // gm_expm1f(gm_fabsf(x))
        if ix < 0x3f80_0000 {
            return h * (2.0 * t - t * t / (t + ONE));
        }
        return h * (t + t / (t + ONE));
    }
    // |x| in [9, logf(maxfloat)] return 0.5*exp(|x|)
    if ix < 0x42b1_7217 {
        return h * expf(f32::abs(x));
    }
    // |x| in [logf(maxfloat), overflowthresold]
    if ix <= 0x42b2d4fc {
        return h * 2.0 * ldexp_expf(f32::abs(x), -1);
    }
    // |x| > overflowthresold, sinh(x) overflow
    x * SHUGE
}

/// `gm_coshf`: the hyperbolic cosine of `x`.
///
/// Ported from `float/e_coshf.c` (`__ieee754_coshf`).
#[must_use]
pub fn coshf(x: f32) -> f32 {
    let mut ix = x.to_bits() as i32;
    ix &= 0x7fff_ffff;

    // x is INF or NaN
    if ix >= 0x7f80_0000 {
        return x * x;
    }
    // |x| in [0, 0.5*ln2], return 1+expm1(|x|)^2/(2*exp(|x|))
    if ix < 0x3eb1_7218 {
        let t = expm1f(f32::abs(x)); // gm_expm1f(gm_fabsf(x))
        let w = ONE + t;
        if ix < 0x3980_0000 {
            return ONE; // cosh(tiny) = 1
        }
        return ONE + (t * t) / (w + w);
    }
    // |x| in [0.5*ln2, 9], return (exp(|x|)+1/exp(|x|))/2;
    if ix < 0x4110_0000 {
        let t = expf(f32::abs(x));
        return HALF * t + HALF / t;
    }
    // |x| in [9, log(maxfloat)] return half*exp(|x|)
    if ix < 0x42b1_7217 {
        return HALF * expf(f32::abs(x));
    }
    // |x| in [log(maxfloat), overflowthresold]
    if ix <= 0x42b2d4fc {
        return ldexp_expf(f32::abs(x), -1);
    }
    // |x| > overflowthresold, cosh(x) overflow
    1.0e30 * 1.0e30 // huge*huge
}

/// `gm_tanhf`: the hyperbolic tangent of `x`.
///
/// Ported from `float/s_tanhf.c`.
#[must_use]
pub fn tanhf(x: f32) -> f32 {
    const TINY: f32 = 1.0e-30;

    let jx = x.to_bits() as i32;
    let ix = jx & 0x7fff_ffff;

    // x is INF or NaN
    if ix >= 0x7f80_0000 {
        if jx >= 0 {
            return ONE / x + ONE; // tanh(+-inf) = +-1
        }
        return ONE / x - ONE; // tanh(NaN) = NaN
    }
    let z;
    // |x| < 22
    if ix < 0x41b0_0000 {
        if ix < 0x2400_0000 {
            // |x| < 2**-55
            return x * (ONE + x); // tanh(small) = small
        }
        if ix >= 0x3f80_0000 {
            // |x| >= 1
            let t = expm1f(TWO * f32::abs(x));
            z = ONE - TWO / (t + TWO);
        } else {
            let t = expm1f(-TWO * f32::abs(x));
            z = -t / (t + TWO);
        }
    } else {
        // |x| > 22, return +-1
        z = ONE - TINY; // raised inexact flag
    }
    if jx >= 0 { z } else { -z }
}
