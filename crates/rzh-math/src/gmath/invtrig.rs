// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Rust port Copyright 2026 the RustZeroHour contributors.
//
// Ported from the pinned GameMath sources (TheSuperHackers/GameMath,
// commit 59f7ccd494f7e7c916a784ac26ef266f9f09d78d, fetched by
// cmake/gamemath.cmake), FreeBSD msun float routines:
//
//   float/e_asinf.c    (gm_asinf, via the __ieee754_asinf alias)
//   float/e_acosf.c    (gm_acosf, via the __ieee754_acosf alias)
//   float/s_atanf.c    (gm_atanf)
//   float/e_atan2f.c   (gm_atan2f, via the __ieee754_atan2f alias)
//
// The originals carry the SunPro copyright notice, preserved per the
// migration plan's porting rules (§3.3):
//
//   Copyright (C) 1993 by Sun Microsystems, Inc. All rights reserved.
//   Developed at SunPro, a Sun Microsystems, Inc. business.
//   Permission to use, copy, modify, and distribute this software is freely
//   granted, provided that this notice is preserved.
//
//! `asinf`, `acosf`, `atanf`, `atan2f` — the float inverse trigonometry.
//!
//! These are the functions `WWMath::Asinf/Acosf/Atanf/Atan2f` call under
//! `USE_DETERMINISTIC_MATH = 1`. `gm_sqrtf` (used by asinf/acosf) is
//! correctly rounded by IEEE-754 specification, so it maps onto
//! [`f32::sqrt`] (ADR-001).
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

// --- e_asinf.c constants ------------------------------------------------

const ONE: f32 = 1.0;
const HUGE: f32 = 1.0e30;
const PIO2_HI: f32 = 1.570_796_3; // 0x3fc90fda
const PIO2_LO: f32 = 7.549_789_415_9e-8; // 0x33a22168
const PIO4_HI: f32 = 0.785_398_19; // 0x3f490fdb
/// coefficient for R(x^2)
const PS0: f32 = 0.166_666_67; // 0x3e2aaaab
const PS1: f32 = -0.325_565_82; // 0xbea6b090
const PS2: f32 = 0.201_212_53; // 0x3e4e0aa8
const PS3: f32 = -0.040_055_536; // 0xbd241146
const PS4: f32 = 0.000_791_535; // 0x3a4f7f04
const PS5: f32 = 0.000_034_793_309; // 0x3811ef08
const QS1: f32 = -2.403_394_9; // 0xc019d139
const QS2: f32 = 2.020_945_8; // 0x4001572d
const QS3: f32 = -0.688_283_98; // 0xbf303361
const QS4: f32 = 0.077_038_154; // 0x3d9dc62e

/// `gm_asinf`: the arc sine of `x`.
///
/// Ported from `float/e_asinf.c` (`__ieee754_asinf`).
#[must_use]
pub fn asinf(x: f32) -> f32 {
    let mut t;
    let w;

    let hx = x.to_bits() as i32;
    let ix = hx & 0x7fff_ffff;
    if ix == 0x3f80_0000 {
        // asin(1)=+-pi/2 with inexact
        return x * PIO2_HI + x * PIO2_LO;
    } else if ix > 0x3f80_0000 {
        // |x| >= 1
        return (x - x) / (x - x); // asin(|x|>1) is NaN
    } else if ix < 0x3f00_0000 {
        // |x| < 0.5
        if ix < 0x3200_0000 {
            // if |x| < 2**-27
            if HUGE + x > ONE {
                return x; // return x with inexact if x != 0
            }
        }
        t = x * x;
        let p = t * (PS0 + t * (PS1 + t * (PS2 + t * (PS3 + t * (PS4 + t * PS5)))));
        let q = ONE + t * (QS1 + t * (QS2 + t * (QS3 + t * QS4)));
        w = p / q;
        return x + x * w;
    }
    // 1 > |x| >= 0.5
    w = ONE - f32::abs(x); // gm_fabsf
    t = w * 0.5;
    let p = t * (PS0 + t * (PS1 + t * (PS2 + t * (PS3 + t * (PS4 + t * PS5)))));
    let q = ONE + t * (QS1 + t * (QS2 + t * (QS3 + t * QS4)));
    let s = f32::sqrt(t); // __ieee754_sqrtf
    if ix >= 0x3F79_999A {
        // if |x| > 0.975
        let w = p / q;
        t = PIO2_HI - (2.0 * (s + s * w) - PIO2_LO);
    } else {
        let mut w = s;
        let iw = w.to_bits();
        w = f32::from_bits(iw & 0xffff_f000);
        let c = (t - w * w) / (s + w);
        let r = p / q;
        let p = 2.0 * s * r - (PIO2_LO - 2.0 * c);
        let q = PIO4_HI - 2.0 * w;
        t = PIO4_HI - (p - q);
    }
    if hx > 0 { t } else { -t }
}

// --- e_acosf.c constants ------------------------------------------------

const PI_ACOS: f32 = 3.141_592_5; // 0x40490fda

/// `gm_acosf`: the arc cosine of `x`.
///
/// Ported from `float/e_acosf.c` (`__ieee754_acosf`).
#[must_use]
pub fn acosf(x: f32) -> f32 {
    let z;

    let hx = x.to_bits() as i32;
    let ix = hx & 0x7fff_ffff;
    if ix == 0x3f80_0000 {
        // |x| == 1
        if hx > 0 {
            return 0.0; // acos(1) = 0
        }
        return PI_ACOS + 2.0 * PIO2_LO; // acos(-1) = pi
    } else if ix > 0x3f80_0000 {
        // |x| >= 1
        return (x - x) / (x - x); // acos(|x|>1) is NaN
    }
    if ix < 0x3f00_0000 {
        // |x| < 0.5
        if ix <= 0x2300_0000 {
            return PIO2_HI + PIO2_LO; // if |x| < 2**-57
        }
        z = x * x;
        let p = z * (PS0 + z * (PS1 + z * (PS2 + z * (PS3 + z * (PS4 + z * PS5)))));
        let q = ONE + z * (QS1 + z * (QS2 + z * (QS3 + z * QS4)));
        let r = p / q;
        return PIO2_HI - (x - (PIO2_LO - x * r));
    } else if hx < 0 {
        // x < -0.5
        z = (ONE + x) * 0.5;
        let p = z * (PS0 + z * (PS1 + z * (PS2 + z * (PS3 + z * (PS4 + z * PS5)))));
        let q = ONE + z * (QS1 + z * (QS2 + z * (QS3 + z * QS4)));
        let s = f32::sqrt(z);
        let r = p / q;
        let w = r * s - PIO2_LO;
        return PI_ACOS - 2.0 * (s + w);
    }
    // x > 0.5
    z = (ONE - x) * 0.5;
    let s = f32::sqrt(z);
    let mut df = s;
    let idf = df.to_bits();
    df = f32::from_bits(idf & 0xffff_f000);
    let c = (z - df * df) / (s + df);
    let p = z * (PS0 + z * (PS1 + z * (PS2 + z * (PS3 + z * (PS4 + z * PS5)))));
    let q = ONE + z * (QS1 + z * (QS2 + z * (QS3 + z * QS4)));
    let r = p / q;
    let w = r * s + c;
    2.0 * (df + w)
}

// --- s_atanf.c -----------------------------------------------------------

/// `atanhi[]`: atan reference values, high parts.
const ATANHI: [f32; 4] = [
    0.463_647_6,  // atan(0.5)hi  0x3eed6338
    0.785_398_13, // atan(1.0)hi  0x3f490fda
    0.982_793_7,  // atan(1.5)hi  0x3f7b985e
    1.570_796_3,  // atan(inf)hi  0x3fc90fda
];

/// `atanlo[]`: atan reference values, low parts.
const ATANLO: [f32; 4] = [
    5.012_158_244e-9,   // atan(0.5)lo  0x31ac3769
    3.774_894_707_9e-8, // atan(1.0)lo  0x33222168
    3.447_321_717e-8,   // atan(1.5)lo  0x33140fb4
    7.549_789_415_9e-8, // atan(inf)lo 0x33a22168
];

/// `aT[]`: atan polynomial coefficients.
const AT: [f32; 11] = [
    3.333_333_432_7e-1,  // 0x3eaaaaaa (stale comment; decimal rules)
    -2.000_000_029_8e-1, // 0xbe4ccccd
    1.428_571_492_4e-1,  // 0x3e124925
    -1.111_111_044_9e-1, // 0xbde38e38
    0.090_908_87,        // 0x3dba2e6e
    -0.076_918_76,       // 0xbd9d8795
    0.066_610_73,        // 0x3d886b35
    -0.058_335_703,      // 0xbd6ef16b
    0.049_768_778,       // 0x3d4bda59
    -0.036_531_571,      // 0xbd15a221
    0.016_285_82,        // 0x3c8569d7
];

/// `gm_atanf`: the arc tangent of `x`.
///
/// Ported from `float/s_atanf.c`.
#[must_use]
pub fn atanf(mut x: f32) -> f32 {
    let id;

    let hx = x.to_bits() as i32;
    let ix = hx & 0x7fff_ffff;
    if ix >= 0x5080_0000 {
        // if |x| >= 2^34
        if ix > 0x7f80_0000 {
            return x + x; // NaN
        }
        if hx > 0 {
            return ATANHI[3] + ATANLO[3];
        }
        return -ATANHI[3] - ATANLO[3];
    }
    if ix < 0x3ee0_0000 {
        // |x| < 0.4375
        if ix < 0x3100_0000 {
            // |x| < 2^-29
            if HUGE + x > ONE {
                return x; // raise inexact
            }
        }
        id = -1;
    } else {
        x = f32::abs(x); // gm_fabsf
        if ix < 0x3f98_0000 {
            // |x| < 1.1875
            if ix < 0x3f30_0000 {
                // 7/16 <= |x| < 11/16
                id = 0;
                x = (2.0 * x - ONE) / (2.0 + x);
            } else {
                // 11/16 <= |x| < 19/16
                id = 1;
                x = (x - ONE) / (x + ONE);
            }
        } else if ix < 0x401c_0000 {
            // |x| < 2.4375
            id = 2;
            x = (x - 1.5) / (ONE + 1.5 * x);
        } else {
            // 2.4375 <= |x| < 2^66
            id = 3;
            x = (-1.0f32) / x;
        }
    }
    // end of argument reduction
    let z = x * x;
    let w = z * z;
    // break sum from i=0 to 10 aT[i]z**(i+1) into odd and even poly
    let s1 = z * (AT[0] + w * (AT[2] + w * (AT[4] + w * (AT[6] + w * (AT[8] + w * AT[10])))));
    let s2 = w * (AT[1] + w * (AT[3] + w * (AT[5] + w * (AT[7] + w * AT[9]))));
    if id < 0 {
        x - x * (s1 + s2)
    } else {
        let z = ATANHI[id as usize] - ((x * (s1 + s2) - ATANLO[id as usize]) - x);
        if hx < 0 { -z } else { z }
    }
}

// --- e_atan2f.c ----------------------------------------------------------

const TINY: f32 = 1.0e-30; // volatile in C; value-only here
const ZERO: f32 = 0.0;
const PI_O_4: f32 = 0.785_398_19; // 0x3f490fdb
const PI_O_2: f32 = 1.570_796_4; // 0x3fc90fdb
const PI: f32 = 3.141_592_7; // 0x40490fdb
const PI_LO: f32 = -8.742_277_657_3e-8; // 0xb3bbbd2e, volatile in C

/// `gm_atan2f(y, x)`: the angle of the point `(x, y)`.
///
/// Ported from `float/e_atan2f.c` (`__ieee754_atan2f`). Note the original
/// signature is `atan2f(y, x)` — `y` first.
#[must_use]
pub fn atan2f(y: f32, x: f32) -> f32 {
    let z;

    let hx = x.to_bits() as i32;
    let ix = hx & 0x7fff_ffff;
    let hy = y.to_bits() as i32;
    let iy = hy & 0x7fff_ffff;
    if ix > 0x7f80_0000 || iy > 0x7f80_0000 {
        // x or y is NaN
        return x + y;
    }
    if hx == 0x3f80_0000 {
        return atanf(y); // x = 1.0
    }
    let mut m = ((hy >> 31) & 1) | ((hx >> 30) & 2); // 2*sign(x)+sign(y)

    // when y = 0
    if iy == 0 {
        match m {
            // atan(+-0, +anything) = +-0
            0 | 1 => return y,
            // atan(+0, -anything) = pi
            2 => return PI + TINY,
            // atan(-0, -anything) = -pi
            _ => return -PI - TINY,
        }
    }
    // when x = 0
    if ix == 0 {
        return if hy < 0 {
            -PI_O_2 - TINY
        } else {
            PI_O_2 + TINY
        };
    }
    // when x is INF
    if ix == 0x7f80_0000 {
        if iy == 0x7f80_0000 {
            match m {
                0 => return PI_O_4 + TINY,             // atan(+INF, +INF)
                1 => return -PI_O_4 - TINY,            // atan(-INF, +INF)
                2 => return 3.0 * PI_O_4 + TINY,       // atan(+INF, -INF)
                _ => return (-3.0f32) * PI_O_4 - TINY, // atan(-INF, -INF)
            }
        }
        match m {
            0 => return ZERO,       // atan(+..., +INF)
            1 => return -ZERO,      // atan(-..., +INF)
            2 => return PI + TINY,  // atan(+..., -INF)
            _ => return -PI - TINY, // atan(-..., -INF)
        }
    }
    // when y is INF
    if iy == 0x7f80_0000 {
        return if hy < 0 {
            -PI_O_2 - TINY
        } else {
            PI_O_2 + TINY
        };
    }

    // compute y/x
    let k = (iy - ix) >> 23;
    if k > 26 {
        // |y/x| > 2**26
        z = PI_O_2 + 0.5 * PI_LO;
        m &= 1;
    } else if hx < 0 && k < -26 {
        // 0 > |y|/x > -2**-26
        z = 0.0;
    } else {
        z = atanf(f32::abs(y / x)); // safe to do y/x
    }
    match m {
        0 => z,                // atan(+, +)
        1 => -z,               // atan(-, +)
        2 => PI - (z - PI_LO), // atan(+, -)
        _ => (z - PI_LO) - PI, // atan(-, -)
    }
}
