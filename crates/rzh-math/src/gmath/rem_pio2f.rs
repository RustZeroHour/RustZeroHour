// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Rust port Copyright 2026 the RustZeroHour contributors.
//
// Ported from the pinned GameMath sources (TheSuperHackers/GameMath,
// commit 59f7ccd494f7e7c916a784ac26ef266f9f09d78d, fetched by
// cmake/gamemath.cmake), FreeBSD msun float routines:
//
//   float/e_rem_pio2f.c   (__ieee754_rem_pio2f)
//   float/k_rem_pio2f.c   (__kernel_rem_pio2f)
//
// The originals carry the SunPro copyright notice, preserved per the
// migration plan's porting rules (§3.3):
//
//   Copyright (C) 1993 by Sun Microsystems, Inc. All rights reserved.
//   Developed at SunPro, a Sun Microsystems, Inc. business.
//   Permission to use, copy, modify, and distribute this software is freely
//   granted, provided that this notice is preserved.
//
//! Argument reduction modulo pi/2 for `sinf`/`cosf`/`tanf`.
//!
//! `ieee754_rem_pio2f` splits `x` into `y[0] + y[1]` (two floats) such that
//! `x = y[0] + y[1] + n*pi/2` with `|y| < pi/4`, and returns `n`. Large
//! arguments go through `kernel_rem_pio2f`, a multi-precision fixed-point
//! distillation over the hex digits of 2/pi.
//!
//! # Transcription notes
//!
//! - `cliptofloat(v)` in the C source (`*(volatile float *)&(v)`) only
//!   exists to clip x87 excess precision. Rust never evaluates floats with
//!   excess precision on any supported target (SSE2/NEON), so it is the
//!   identity here — noted at each use site.
//! - The C `goto recompute` in `__kernel_rem_pio2f` becomes a `loop` with
//!   `continue`; control flow is otherwise statement-for-statement.
//! - Float words are handled as `i32` (as the C macros do); signed right
//!   shifts are arithmetic in both languages on every supported platform.
//! - C float→int casts (`(int32_t)f`) truncate toward zero, exactly like
//!   Rust's `as i32`; every cast site is range-guarded by the algorithm.
//!
//! # Parity level
//!
//! **P0 exact** via `sinf`/`cosf`/`tanf` golden vectors, which exercise all
//! three reduction paths (no reduction, medium, and huge arguments).

// Clippy notes for this verbatim msun transcription:
// - `excessive_precision` / `approx_constant`: the float literals keep the
//   exact decimal forms of the C sources (several approximate pi/e by
//   design); rounding them to fewer digits changes bits.
// - `eq_op`: NaN/overflow sentinels written as `x - x` / `(x-x)/(x-x)` in
//   the original are behaviour.
#![allow(clippy::excessive_precision, clippy::approx_constant, clippy::eq_op)]

/// `npio2_hw[]`: high words of the first 32 multiples of pi/2 (low 8 bits
/// dropped, further masking happens at run time).
const NPIO2_HW: [i32; 32] = [
    0x3fc90f00, 0x40490f00, 0x4096cb00, 0x40c90f00, 0x40fb5300, 0x4116cb00, 0x412fed00, 0x41490f00,
    0x41623100, 0x417b5300, 0x418a3a00, 0x4196cb00, 0x41a35c00, 0x41afed00, 0x41bc7e00, 0x41c90f00,
    0x41d5a000, 0x41e23100, 0x41eec200, 0x41fb5300, 0x4203f200, 0x420a3a00, 0x42108300, 0x4216cb00,
    0x421d1400, 0x42235c00, 0x4229a500, 0x422fed00, 0x42363600, 0x423c7e00, 0x4242c700, 0x42490f00,
];

const ZERO: f32 = 0.0;
const HALF: f32 = 0.5;
const TWO8: f32 = 256.0; // 0x43800000

/// `invpio2`: 24 bits of 2/pi.
const INVPIO2: f32 = 6.366_198_062_9e-1; // 0x3f22f984

/// `pio2_1`: first 17 bits of pi/2.
const PIO2_1: f32 = 1.570_785_522_5; // 0x3fc90f80
/// `pio2_1t`: pi/2 - pio2_1.
const PIO2_1T: f32 = 1.080_433_412_4e-5; // 0x37354443
/// `pio2_2`: second 17 bits of pi/2.
const PIO2_2: f32 = 1.080_427_318_8e-5; // 0x37354400
/// `pio2_2t`: pi/2 - (pio2_1+pio2_2).
const PIO2_2T: f32 = 6.077_099_934_4e-11; // 0x2e85a308
/// `pio2_3`: third 17 bits of pi/2.
const PIO2_3: f32 = 6.077_094_383_3e-11; // 0x2e85a300
/// `pio2_3t`: pi/2 - (pio2_1+pio2_2+pio2_3).
const PIO2_3T: f32 = 6.123_234_262_9e-17; // 0x248d3132

/// `__ieee754_rem_pio2f(x, y)`: returns `n` and writes the remainder
/// `x - n*pi/2` into `y[0]` (high part) and `y[1]` (tail).
///
/// Ported from `float/e_rem_pio2f.c`. In the C build used by sinf/cosf/tanf
/// this function is compiled `static __inline` (`INLINE_REM_PIO2`); the
/// algorithm is identical either way.
pub(super) fn ieee754_rem_pio2f(x: f32, y: &mut [f32; 2]) -> i32 {
    let hx = x.to_bits() as i32;
    let ix = hx & 0x7fff_ffff;
    if ix <= 0x3f490fd8 {
        // |x| ~<= pi/4, no need for reduction
        y[0] = x;
        y[1] = 0.0;
        return 0;
    }
    if ix < 0x4016cbe4 {
        // |x| < 3pi/4, special case with n=+-1
        if hx > 0 {
            let mut z = x - PIO2_1;
            if (ix as u32 & 0xfffe_0000) != 0x3fc8_0000 {
                // 17+24 bit pi OK
                y[0] = z - PIO2_1T;
                y[1] = (z - y[0]) - PIO2_1T; // z - clip(y[0])
            } else {
                // near pi/2, use 17+17+24 bit pi
                z -= PIO2_2;
                y[0] = z - PIO2_2T;
                y[1] = (z - y[0]) - PIO2_2T; // z - clip(y[0])
            }
            return 1;
        }
        // negative x
        let mut z = x + PIO2_1;
        if (ix as u32 & 0xfffe_0000) != 0x3fc8_0000 {
            // 17+24 bit pi OK
            y[0] = z + PIO2_1T;
            y[1] = (z - y[0]) + PIO2_1T; // z - clip(y[0])
        } else {
            // near pi/2, use 17+17+24 bit pi
            z += PIO2_2;
            y[0] = z + PIO2_2T;
            y[1] = (z - y[0]) + PIO2_2T; // z - clip(y[0])
        }
        return -1;
    }
    if ix <= 0x43490f80 {
        // |x| ~<= 2^7*(pi/2), medium size
        let mut t = f32::abs(x); // gm_fabsf
        let n = (t * INVPIO2 + HALF) as i32;
        let fn_ = n as f32;
        let mut r = t - fn_ * PIO2_1;
        let mut w = fn_ * PIO2_1T; // 1st round good to 40 bit
        if n < 32 && (ix as u32 & 0xffe0_0000) != (NPIO2_HW[(n - 1) as usize] as u32 & 0xffe0_0000)
        {
            y[0] = r - w; // quick check no cancellation
        } else {
            let j = ix >> 23;
            y[0] = r - w;
            let mut high = y[0].to_bits() as i32;
            let mut i = j - ((high >> 23) & 0xff);
            if i > 8 {
                // 2nd iteration needed, good to 57
                t = r;
                w = fn_ * PIO2_2;
                r = t - w;
                w = fn_ * PIO2_2T - ((t - r) - w);
                y[0] = r - w;
                high = y[0].to_bits() as i32;
                i = j - ((high >> 23) & 0xff);
                if i > 25 {
                    // 3rd iteration need, 74 bits acc
                    t = r; // will cover all possible cases
                    w = fn_ * PIO2_3;
                    r = t - w;
                    w = fn_ * PIO2_3T - ((t - r) - w);
                    y[0] = r - w;
                }
            }
        }
        // The C sets y[1] here, after the quick-check if/else, on BOTH
        // paths (a transcription that nested it in the else left y[1] at
        // zero on the quick path and cost 1 ulp in sinf/cosf/tanf).
        y[1] = (r - y[0]) - w; // r - clip(y[0])
        if hx < 0 {
            y[0] = -y[0];
            y[1] = -y[1];
            return -n;
        }
        return n;
    }
    // all other (large) arguments
    if ix >= 0x7f80_0000 {
        // x is inf or NaN
        y[0] = x - x;
        y[1] = x - x;
        return 0;
    }
    // set z = scalbn(|x|, ilogb(x)-7)
    let e0 = (ix >> 23) - 134; // e0 = ilogb(z)-7;
    let mut z = f32::from_bits((ix - (e0 << 23)) as u32);
    let mut tx = [0.0f32; 3];
    for item in &mut tx[..2] {
        *item = (z as i32) as f32;
        z = (z - *item) * TWO8;
    }
    tx[2] = z;
    let mut nx = 3;
    while tx[(nx - 1) as usize] == ZERO {
        nx -= 1; // skip zero term
    }
    let n = kernel_rem_pio2f(&tx, y, e0, nx, 2);
    if hx < 0 {
        y[0] = -y[0];
        y[1] = -y[1];
        return -n;
    }
    n
}

/// `init_jk[]`: initial `jk` for each precision.
const INIT_JK: [i32; 3] = [4, 7, 9];

/// `ipio2[]`: table of constants for 2/pi, 396 hex digits.
const IPIO2: [i32; 198] = [
    0xA2, 0xF9, 0x83, 0x6E, 0x4E, 0x44, 0x15, 0x29, 0xFC, //
    0x27, 0x57, 0xD1, 0xF5, 0x34, 0xDD, 0xC0, 0xDB, 0x62, //
    0x95, 0x99, 0x3C, 0x43, 0x90, 0x41, 0xFE, 0x51, 0x63, //
    0xAB, 0xDE, 0xBB, 0xC5, 0x61, 0xB7, 0x24, 0x6E, 0x3A, //
    0x42, 0x4D, 0xD2, 0xE0, 0x06, 0x49, 0x2E, 0xEA, 0x09, //
    0xD1, 0x92, 0x1C, 0xFE, 0x1D, 0xEB, 0x1C, 0xB1, 0x29, //
    0xA7, 0x3E, 0xE8, 0x82, 0x35, 0xF5, 0x2E, 0xBB, 0x44, //
    0x84, 0xE9, 0x9C, 0x70, 0x26, 0xB4, 0x5F, 0x7E, 0x41, //
    0x39, 0x91, 0xD6, 0x39, 0x83, 0x53, 0x39, 0xF4, 0x9C, //
    0x84, 0x5F, 0x8B, 0xBD, 0xF9, 0x28, 0x3B, 0x1F, 0xF8, //
    0x97, 0xFF, 0xDE, 0x05, 0x98, 0x0F, 0xEF, 0x2F, 0x11, //
    0x8B, 0x5A, 0x0A, 0x6D, 0x1F, 0x6D, 0x36, 0x7E, 0xCF, //
    0x27, 0xCB, 0x09, 0xB7, 0x4F, 0x46, 0x3F, 0x66, 0x9E, //
    0x5F, 0xEA, 0x2D, 0x75, 0x27, 0xBA, 0xC7, 0xEB, 0xE5, //
    0xF1, 0x7B, 0x3D, 0x07, 0x39, 0xF7, 0x8A, 0x52, 0x92, //
    0xEA, 0x6B, 0xFB, 0x5F, 0xB1, 0x1F, 0x8D, 0x5D, 0x08, //
    0x56, 0x03, 0x30, 0x46, 0xFC, 0x7B, 0x6B, 0xAB, 0xF0, //
    0xCF, 0xBC, 0x20, 0x9A, 0xF4, 0x36, 0x1D, 0xA9, 0xE3, //
    0x91, 0x61, 0x5E, 0xE6, 0x1B, 0x08, 0x65, 0x99, 0x85, //
    0x5F, 0x14, 0xA0, 0x68, 0x40, 0x8D, 0xFF, 0xD8, 0x80, //
    0x4D, 0x73, 0x27, 0x31, 0x06, 0x06, 0x15, 0x56, 0xCA, //
    0x73, 0xA8, 0xC9, 0x60, 0xE2, 0x7B, 0xC0, 0x8C, 0x6B, //
];

/// `PIo2[]`: pi/2 in 24-bit chunks.
const PIO2: [f32; 11] = [
    1.570_312_5,         // 0x3fc90000
    4.577_636_718_8e-4,  // 0x39f00000
    2.598_762_512_2e-5,  // 0x37da0000
    7.543_712_854_4e-8,  // 0x33a20000
    6.002_665_031_7e-11, // 0x2e840000
    7.389_644_451_9e-13, // 0x2b500000
    5.384_581_669_4e-15, // 0x27c20000
    5.637_851_296_9e-18, // 0x22d00000
    8.300_922_883_1e-20, // 0x1fc40000
    3.275_635_225_7e-22, // 0x1bc60000
    6.333_101_564_9e-25, // 0x17440000
];

const ONE: f32 = 1.0;
const TWON8: f32 = 3.906_25e-3; // 0x3b800000

/// `__kernel_rem_pio2f(x, y, e0, nx, prec)`: the multi-precision tail of the
/// argument reduction for huge arguments.
///
/// Ported from `float/k_rem_pio2f.c`. The C `goto recompute` is a `loop`
/// here; `continue` plays the role of the goto. Array bounds are the C
/// originals' (20 entries); a Rust panic would flag a transcription bug, the
/// algorithm keeps every index in range.
#[allow(clippy::too_many_lines)]
fn kernel_rem_pio2f(x: &[f32; 3], y: &mut [f32; 2], e0: i32, nx: i32, prec: i32) -> i32 {
    let mut iq = [0i32; 20];
    let mut f = [0f32; 20];
    let mut fq = [0f32; 20];
    let mut q = [0f32; 20];

    // initialize jk
    let jk = INIT_JK[prec as usize];
    let jp = jk;

    // determine jx, jv, q0, note that 3>q0
    let jx = nx - 1;
    let mut jv = (e0 - 3) / 8;
    if jv < 0 {
        jv = 0;
    }
    let mut q0 = e0 - 8 * (jv + 1);

    // set up f[0] to f[jx+jk] where f[jx+jk] = ipio2[jv+jk]
    let mut j = jv - jx;
    let m = jx + jk;
    let mut i = 0;
    while i <= m {
        f[i as usize] = if j < 0 {
            ZERO
        } else {
            IPIO2[j as usize] as f32
        };
        i += 1;
        j += 1;
    }

    // compute q[0],q[1],...q[jk]
    for i in 0..=jk {
        let mut fw = 0.0f32;
        for j in 0..=jx {
            fw += x[j as usize] * f[(jx + i - j) as usize];
        }
        q[i as usize] = fw;
    }

    let mut jz = jk;
    // `z`, `n` and `ih` are assigned at the top of every iteration below;
    // the `loop` form lets rustc prove they are initialized on exit.
    let mut z;
    let mut n;
    let mut ih;

    // "recompute:" label of the C source — the goto becomes this loop.
    loop {
        // distill q[] into iq[] reversingly
        let mut i2 = 0;
        let mut j2 = jz;
        z = q[jz as usize];
        while j2 > 0 {
            let fw = ((TWON8 * z) as i32) as f32;
            iq[i2 as usize] = (z - TWO8 * fw) as i32;
            z = q[(j2 - 1) as usize] + fw;
            i2 += 1;
            j2 -= 1;
        }

        // compute n
        z = super::scalbn::scalbnf(z, q0); // actual value of z
        z -= 8.0 * f32::floor(z * 0.125); // trim off integer >= 8
        n = z as i32;
        z -= n as f32;
        ih = 0;
        if q0 > 0 {
            // need iq[jz-1] to determine n
            let i = iq[(jz - 1) as usize] >> (8 - q0);
            n += i;
            iq[(jz - 1) as usize] -= i << (8 - q0);
            ih = iq[(jz - 1) as usize] >> (7 - q0);
        } else if q0 == 0 {
            ih = iq[(jz - 1) as usize] >> 7;
        } else if z >= 0.5 {
            ih = 2;
        }

        if ih > 0 {
            // q > 0.5
            n += 1;
            let mut carry = 0;
            for i in 0..jz {
                // compute 1-q
                let j = iq[i as usize];
                if carry == 0 {
                    if j != 0 {
                        carry = 1;
                        iq[i as usize] = 0x100 - j;
                    }
                } else {
                    iq[i as usize] = 0xff - j;
                }
            }
            if q0 > 0 {
                // rare case: chance is 1 in 12
                match q0 {
                    1 => iq[(jz - 1) as usize] &= 0x7f,
                    2 => iq[(jz - 1) as usize] &= 0x3f,
                    _ => {}
                }
            }
            if ih == 2 {
                z = ONE - z;
                if carry != 0 {
                    z -= super::scalbn::scalbnf(ONE, q0);
                }
            }
        }

        // check if recomputation is needed
        let mut need_recompute = false;
        if z == ZERO {
            let mut j = 0;
            let mut i = jz - 1;
            while i >= jk {
                j |= iq[i as usize];
                i -= 1;
            }
            if j == 0 {
                // need recomputation
                let mut k = 1;
                while iq[(jk - k) as usize] == 0 {
                    k += 1; // k = no. of terms needed
                }

                let i0 = jz + 1;
                for i in i0..=jz + k {
                    // add q[jz+1] to q[jz+k]
                    f[(jx + i) as usize] = IPIO2[(jv + i) as usize] as f32;
                    let mut fw = 0.0f32;
                    for j in 0..=jx {
                        fw += x[j as usize] * f[(jx + i - j) as usize];
                    }
                    q[i as usize] = fw;
                }
                jz += k;
                need_recompute = true; // goto recompute
            }
        }
        if !need_recompute {
            break;
        }
    }

    // chop off zero terms
    if z == 0.0 {
        jz -= 1;
        q0 -= 8;
        while iq[jz as usize] == 0 {
            jz -= 1;
            q0 -= 8;
        }
    } else {
        // break z into 8-bit if necessary
        z = super::scalbn::scalbnf(z, -q0);
        if z >= TWO8 {
            let fw = ((TWON8 * z) as i32) as f32;
            iq[jz as usize] = (z - TWO8 * fw) as i32;
            jz += 1;
            q0 += 8;
            iq[jz as usize] = fw as i32;
        } else {
            iq[jz as usize] = z as i32;
        }
    }

    // convert integer "bit" chunk to floating-point value
    let mut fw = super::scalbn::scalbnf(ONE, q0);
    let mut i = jz;
    while i >= 0 {
        q[i as usize] = fw * iq[i as usize] as f32;
        fw *= TWON8;
        i -= 1;
    }

    // compute PIo2[0,...,jp]*q[jz,...,0]
    let mut i = jz;
    while i >= 0 {
        let mut fw = 0.0f32;
        let mut k = 0;
        while k <= jp && k <= jz - i {
            fw += PIO2[k as usize] * q[(i + k) as usize];
            k += 1;
        }
        fq[(jz - i) as usize] = fw;
        i -= 1;
    }

    // compress fq[] into y[]
    match prec {
        0 => {
            let mut fw = 0.0f32;
            for i in (0..=jz).rev() {
                fw += fq[i as usize];
            }
            y[0] = if ih == 0 { fw } else { -fw };
        }
        1 | 2 => {
            let mut fw = 0.0f32;
            for i in (0..=jz).rev() {
                fw += fq[i as usize];
            }
            let fw_clipped = fw; // clip any extra precision (identity here)
            y[0] = if ih == 0 { fw_clipped } else { -fw_clipped };
            fw = fq[0] - fw_clipped;
            for i in 1..=jz {
                fw += fq[i as usize];
            }
            y[1] = if ih == 0 { fw } else { -fw };
        }
        _ => {
            // case 3, "painful" — unreachable from the float entry points
            // (only long double paths use it); kept unreachable in Rust.
            unreachable!("prec=3 is not used by the float argument reduction");
        }
    }
    n & 7
}
