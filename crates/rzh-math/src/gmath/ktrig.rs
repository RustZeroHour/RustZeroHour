// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Rust port Copyright 2026 the RustZeroHour contributors.
//
// Ported from the pinned GameMath sources (TheSuperHackers/GameMath,
// commit 59f7ccd494f7e7c916a784ac26ef266f9f09d78d, fetched by
// cmake/gamemath.cmake), FreeBSD msun float routines:
//
//   float/k_sinf.c   (__kernel_sinf)
//   float/k_cosf.c   (__kernel_cosf)
//   float/k_tanf.c   (__kernel_tanf)
//
// The originals carry the SunPro copyright notice, preserved per the
// migration plan's porting rules (§3.3):
//
//   Copyright (C) 1993 by Sun Microsystems, Inc. All rights reserved.
//   Developed at SunPro, a Sun Microsystems, Inc. business.
//   Permission to use, copy, modify, and distribute this software is freely
//   granted, provided that this notice is preserved.
//
//! Polynomial kernels for `sinf`/`cosf`/`tanf` on the reduced argument.
//!
//! # Parity level
//!
//! **P0 exact** via the `sinf`/`cosf`/`tanf` golden vectors.

// Clippy notes for this verbatim msun transcription:
// - `excessive_precision` / `approx_constant`: the float literals keep the
//   exact decimal forms of the C sources (several approximate pi/e by
//   design); rounding them to fewer digits changes bits.
// - `eq_op`: NaN/overflow sentinels written as `x - x` / `(x-x)/(x-x)` in
//   the original are behaviour.
#![allow(clippy::excessive_precision, clippy::approx_constant, clippy::eq_op)]

/// `half` = 0.5.
const HALF: f32 = 0.5;

/// `S1..S4`: sin polynomial coefficients (k_sinf.c).
///
/// Range of maximum relative error in polynomial: ~[-1.61e-10, 1.621e-10].
const S1: f32 = -0.166_666_67; // -0xaaaaab.0p-26
const S2: f32 = 0.008_333_379; //  0x8888ba.0p-30
const S3: f32 = -0.000_198_530_73; // -0xd02cb0.0p-36
const S4: f32 = 0.000_002_832_675_6; //  0xbe18ff.0p-42

/// `__kernel_sinf(x, y, iy)`: sin kernel on `|x| ~<= pi/4`.
///
/// `y` is the tail of the argument reduction, `iy` selects between the
/// reduced (1) and unreduced (0) path.
pub(super) fn kernel_sinf(x: f32, y: f32, iy: i32) -> f32 {
    let z = x * x;
    let v = z * x;
    let r = S2 + z * (S3 + z * S4);
    if iy == 0 {
        x + v * (S1 + z * r)
    } else {
        x - ((z * (HALF * y - v * r) - y) - v * S1)
    }
}

/// `one` = 1.0.
const ONE: f32 = 1.0;

/// `C1..C3`: cos polynomial coefficients (k_cosf.c).
///
/// Range of maximum relative error in polynomial: ~[-1.15e-10, 1.169e-10].
const C1: f32 = 0.041_666_646; //  0xaaaaa5.0p-28
const C2: f32 = -1.388_731_063_344_538_2e-3; // -0xb60615.0p-33
const C3: f32 = 0.000_024_432_543; //  0xccf47d.0p-39

/// `__kernel_cosf(x, y)`: cos kernel on `|x| ~<= pi/4`.
pub(super) fn kernel_cosf(x: f32, y: f32) -> f32 {
    let z = x * x;
    let r = z * (C1 + z * (C2 + z * C3));
    let hz = 0.5 * z;
    let w = ONE - hz;
    w + (((ONE - w) - hz) + (z * r - x * y))
}

/// `pio4` = pi/4 high part (k_tanf.c).
const PIO4: f32 = 0.785_398_13; // 0x3f490fda
/// `pio4lo` = pi/4 tail.
const PIO4LO: f32 = 3.774_894_707_9e-8; // 0x33222168

/// `T[]`: tan polynomial coefficients.
const T: [f32; 13] = [
    3.333_333_432_7e-1, // 0x3eaaaaab
    0.133_333_34,       // 0x3e088889
    0.053_968_254,      // 0x3d5d0dd1
    0.021_869_488,      // 0x3cb327a4
    0.008_863_24,       // 0x3c11371f
    0.003_592_079,      // 0x3b6b6916
    0.001_456_209_5,    // 0x3abede48
    0.000_588_041_27,   // 0x3a1a26c8
    0.000_246_463_14,   // 0x398137b9
    0.000_078_179_444,  // 0x38a3f445
    0.000_071_407_25,   // 0x3895c07a
    -0.000_018_558_638, // 0xb79bae5f
    0.000_025_907_306,  // 0x37d95384
];

/// `__kernel_tanf(x, y, iy)`: tan kernel on `|x| ~<= pi/4`.
///
/// `iy` selects the reciprocal output (`-1/x`) used after argument
/// reduction with an odd quadrant.
#[allow(clippy::too_many_lines)]
pub(super) fn kernel_tanf(mut x: f32, mut y: f32, iy: i32) -> f32 {
    let hx = x.to_bits() as i32;
    let ix = hx & 0x7fff_ffff;
    let big = ix >= 0x3f2ca140; // |x| >= 0.67434
    if big {
        if hx < 0 {
            x = -x;
            y = -y;
        }
        let z = PIO4 - x;
        let w = PIO4LO - y;
        x = z + w;
        y = 0.0;
    }
    let z = x * x;
    let w = z * z;
    // Break x^5*(T[1]+x^2*T[2]+...) into
    //     x^5*(T[1]+x^4*T[3]+x^8*T[5]) + x^5*(x^2*(T[2]+x^4*T[4]))
    let mut r = T[1] + w * (T[3] + w * T[5]);
    let v = z * (T[2] + w * T[4]);
    let s = z * x;
    r = y + z * (s * (r + v) + y);
    r += T[0] * s;
    let w = x + r;
    if big {
        let v = iy as f32;
        return ((1 - ((hx >> 30) & 2)) as f32) * (v - 2.0 * (x - (w * w / (w + v) - r)));
    }
    if iy == 1 {
        w
    } else {
        // if allow error up to 2 ulp, simply return -1.0/(x+r) here
        // compute -1.0/(x+r) accurately
        let mut z = w;
        let i = z.to_bits();
        z = f32::from_bits(i & 0xffff_f000);
        let v = r - (z - x); // z+v = r+x
        let a = (-1.0f32) / w; // a = -1.0/w
        let mut t = a;
        let i = t.to_bits();
        t = f32::from_bits(i & 0xffff_f000);
        let s = 1.0 + t * z;
        t + a * (s + t * v)
    }
}
