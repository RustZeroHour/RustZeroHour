// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Rust port Copyright 2026 the RustZeroHour contributors.
//
// Ported from the pinned GameMath sources (TheSuperHackers/GameMath,
// commit 59f7ccd494f7e7c916a784ac26ef266f9f09d78d, fetched by
// cmake/gamemath.cmake), FreeBSD msun float routines:
//
//   float/e_expf.c     (gm_expf, via the __ieee754_expf alias)
//   float/s_expm1f.c   (gm_expm1f)
//   float/k_expf.c     (__frexp_expf, __ldexp_expf)
//   float/e_logf.c     (gm_logf, via the __ieee754_logf alias)
//   float/k_logf.h     (k_log1pf)
//   float/e_log10f.c   (gm_log10f, via the __ieee754_log10f alias)
//
// The originals carry the SunPro copyright notice, preserved per the
// migration plan's porting rules (§3.3):
//
//   Copyright (C) 1993 by Sun Microsystems, Inc. All rights reserved.
//   Developed at SunPro, a Sun Microsystems, Inc. business.
//   Permission to use, copy, modify, and distribute this software is freely
//   granted, provided that this notice is preserved.
//
//! `expf`, `expm1f`, `logf`, `log10f` and the `ldexp_expf` scaling helper.
//!
//! # Transcription notes
//!
//! - The C sources leave `hi`/`lo`/`k` uninitialized on paths where they
//!   are provably never read (guarded by `k == 0`); Rust requires an
//!   initialization, so those get 0.0/0 sentinels that are never read —
//!   the control flow that reads them is exactly the control flow that
//!   assigns them.
//! - Exponent-word additions (`i + (k << 23)`) wrap on two's-complement
//!   machines in C and are transcribed with [`wrapping_add`](i32::wrapping_add).
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

// --- e_expf.c ------------------------------------------------------------

const ONE: f32 = 1.0;
/// `halF[2]`: ±0.5, selected by the sign bit.
const HALF_F: [f32; 2] = [0.5, -0.5];
const HUGE: f32 = 1.0e30;
const TINY: f32 = 1.0e-30;
const TWOM100: f32 = 7.888_609_052_2e-31; // 2**-100 = 0x0d800000
const O_THRESHOLD: f32 = 8.872_167_968_8e1; // 0x42b17180
const U_THRESHOLD: f32 = -1.039_720_840_5e2; // 0xc2cff1b5
/// `ln2HI[2]`: ±ln(2) high part.
const LN2_HI: [f32; 2] = [0.693_138_12, -0.693_138_12]; // 0x3f317180
/// `ln2LO[2]`: ±ln(2) low part.
const LN2_LO: [f32; 2] = [9.058_000_614_5e-6, -9.058_000_614_5e-6]; // 0x3717f7d1
const INVLN2: f32 = 1.442_695_021_6; // 0x3fb8aa3b
const P1: f32 = 0.166_666_67; // 0x3e2aaaab
const P2: f32 = -0.002_777_777_8; // 0xbb360b61
const P3: f32 = 6.613_756e-5; // 0x388ab355
const P4: f32 = -1.653_390_2e-6; // 0xb5ddea0e
const P5: f32 = 4.138_136_944_2e-8; // 0x3331bb4c

/// `gm_expf`: e raised to the power `x`.
///
/// Ported from `float/e_expf.c` (`__ieee754_expf`).
#[allow(unused_assignments)] // dummy init mirrors C's guarded-uninitialized hi/lo
#[must_use]
pub fn expf(x: f32) -> f32 {
    let mut x = x;
    // Only read on the paths that assign them (see module notes).
    let mut hi = 0.0f32;
    let mut lo = 0.0f32;
    let mut k: i32 = 0;

    let hx0 = x.to_bits() as i32;
    let xsb = (hx0 >> 31) & 1; // sign bit of x
    let hx = (hx0 as u32) & 0x7fff_ffff; // high word of |x|

    // filter out non-finite argument
    if hx >= 0x42b1_7218 {
        // if |x| >= 88.721...
        if hx > 0x7f80_0000 {
            return x + x; // NaN
        }
        if hx == 0x7f80_0000 {
            return if xsb == 0 { x } else { 0.0 }; // exp(+-inf)={inf,0}
        }
        if x > O_THRESHOLD {
            return HUGE * HUGE; // overflow
        }
        if x < U_THRESHOLD {
            return TWOM100 * TWOM100; // underflow
        }
    }

    // argument reduction
    if hx > 0x3eb1_7218 {
        // if |x| > 0.5 ln2
        if hx < 0x3F85_1592 {
            // and |x| < 1.5 ln2
            hi = x - LN2_HI[xsb as usize];
            lo = LN2_LO[xsb as usize];
            k = 1 - xsb - xsb;
        } else {
            k = (INVLN2 * x + HALF_F[xsb as usize]) as i32;
            let t = k as f32;
            hi = x - t * LN2_HI[0]; // t*ln2HI is exact here
            lo = t * LN2_LO[0];
        }
        x = hi - lo;
    } else if hx < 0x3180_0000 {
        // when |x| < 2**-28
        if HUGE + x > ONE {
            return ONE + x; // trigger inexact
        }
    }

    // x is now in primary range
    let t = x * x;
    let c = x - t * (P1 + t * (P2 + t * (P3 + t * (P4 + t * P5))));
    if k == 0 {
        return ONE - ((x * c) / (c - 2.0) - x);
    }
    let y = ONE - ((lo - (x * c) / (2.0 - c)) - hi);
    if k >= -125 {
        let hy = y.to_bits();
        // add k to y's exponent
        return f32::from_bits(hy.wrapping_add((k << 23) as u32));
    }
    let hy = y.to_bits();
    // add (k+100) to y's exponent
    let y = f32::from_bits(hy.wrapping_add(((k + 100) << 23) as u32));
    y * TWOM100
}

// --- s_expm1f.c ----------------------------------------------------------

const LN2_HI_S: f32 = 0.693_138_12; // 0x3f317180
const LN2_LO_S: f32 = 9.058_000_614_5e-6; // 0x3717f7d1
const Q1: f32 = -0.033_333_335; // 0xbd088889
const Q2: f32 = 0.001_587_301_6; // 0x3ad00d01
const Q3: f32 = -7.936_507_609e-5; // 0xb8a670cd
const Q4: f32 = 4.008_217_729_3e-6; // 0x36867e54
const Q5: f32 = -2.010_992_119_5e-7; // 0xb457edbb

/// `gm_expm1f`: e^x - 1, accurate for small `x`.
///
/// Ported from `float/s_expm1f.c`. Needed by `sinhf`/`coshf`/`tanhf`.
#[allow(clippy::too_many_lines, unused_assignments)] // dummy init mirrors C's guarded-uninitialized hi/lo/c
#[must_use]
pub fn expm1f(mut x: f32) -> f32 {
    // Only read on the paths that assign them (see module notes).
    let mut hi = 0.0f32;
    let mut lo = 0.0f32;
    let mut c = 0.0f32;
    let mut k: i32 = 0;

    let hx0 = x.to_bits() as i32;
    let xsb = (hx0 as u32 & 0x0080_000000) as i32; // sign bit of x
    let hx = (hx0 as u32) & 0x7fff_ffff; // high word of |x|

    // filter out huge and non-finite argument
    if hx >= 0x4195b844 {
        // if |x| >= 27*ln2
        if hx >= 0x42b1_7218 {
            // if |x| >= 88.721...
            if hx > 0x7f80_0000 {
                return x + x; // NaN
            }
            if hx == 0x7f80_0000 {
                return if xsb == 0 { x } else { -1.0 }; // exp(+-inf)={inf,-1}
            }
            if x > O_THRESHOLD {
                return HUGE * HUGE; // overflow
            }
        }
        if xsb != 0 {
            // x < -27*ln2, return -1.0 with inexact
            if x + TINY < 0.0 {
                // raise inexact
                return TINY - ONE; // return -1
            }
        }
    }

    // argument reduction
    if hx > 0x3eb1_7218 {
        // if |x| > 0.5 ln2
        if hx < 0x3F85_1592 {
            // and |x| < 1.5 ln2
            if xsb == 0 {
                hi = x - LN2_HI_S;
                lo = LN2_LO_S;
                k = 1;
            } else {
                hi = x + LN2_HI_S;
                lo = -LN2_LO_S;
                k = -1;
            }
        } else {
            k = (INVLN2 * x + if xsb == 0 { 0.5 } else { -0.5 }) as i32;
            let t = k as f32;
            hi = x - t * LN2_HI_S; // t*ln2_hi is exact here
            lo = t * LN2_LO_S;
        }
        x = hi - lo;
        c = (hi - x) - lo;
    } else if hx < 0x3300_0000 {
        // when |x| < 2**-25, return x
        let t = HUGE + x; // return x with inexact flags when x != 0
        return x - (t - (HUGE + x));
    }

    // x is now in primary range
    let hfx = 0.5 * x;
    let hxs = x * hfx;
    let r1 = ONE + hxs * (Q1 + hxs * (Q2 + hxs * (Q3 + hxs * (Q4 + hxs * Q5))));
    let t = 3.0 - r1 * hfx;
    let mut e = hxs * ((r1 - t) / (6.0 - x * t));
    if k == 0 {
        return x - (x * e - hxs); // c is 0
    }
    e = x * (e - c) - c;
    e -= hxs;
    if k == -1 {
        return 0.5 * (x - e) - 0.5;
    }
    if k == 1 {
        if x < -0.25 {
            return -2.0 * (e - (x + 0.5));
        }
        return ONE + 2.0 * (x - e);
    }
    if k <= -2 || k > 56 {
        // suffice to return exp(x)-1
        let y = ONE - (e - x);
        let i = y.to_bits() as i32;
        // add k to y's exponent
        let y = f32::from_bits(i.wrapping_add(k << 23) as u32);
        return y - ONE;
    }
    if k < 23 {
        // t = 1-2^-k
        let t = f32::from_bits((0x3f80_0000i32.wrapping_sub(0x100_0000 >> k)) as u32);
        let y = t - (e - x);
        let i = y.to_bits() as i32;
        // add k to y's exponent
        f32::from_bits(i.wrapping_add(k << 23) as u32)
    } else {
        // 2^-k
        let t = f32::from_bits(((0x7f - k) << 23) as u32);
        let mut y = x - (e + t);
        y += ONE;
        let i = y.to_bits() as i32;
        // add k to y's exponent
        f32::from_bits(i.wrapping_add(k << 23) as u32)
    }
}

// --- k_expf.c ------------------------------------------------------------

/// `k` = 235, the exp scaling constant.
const K_EXP: u32 = 235;
/// `kln2` = 235 * ln2.
const KLN2: f32 = 162.889_59; // 162.88958740F

/// `__frexp_expf`: exp(x) rescaled to [1, 2) with the used power of two
/// reported through `expt`.
fn frexp_expf(x: f32, expt: &mut i32) -> f32 {
    let mut exp_x = expf(x - KLN2);
    let hx = exp_x.to_bits();
    *expt = ((hx >> 23) as i32) - (0x7f + 127) + K_EXP as i32;
    exp_x = f32::from_bits((hx & 0x7f_ffff) | ((0x7f + 127) << 23));
    exp_x
}

/// `__ldexp_expf(x, expt)`: exp(x) * 2^expt, computed so the intermediate
/// never overflows first.
pub(super) fn ldexp_expf(x: f32, expt: i32) -> f32 {
    let mut ex_expt = 0;
    let exp_x = frexp_expf(x, &mut ex_expt);
    let expt = expt + ex_expt;
    let scale = f32::from_bits(((0x7f + expt) << 23) as u32);
    exp_x * scale
}

// --- e_logf.c ------------------------------------------------------------

const LN2_HI_L: f32 = 0.693_138_12; // 0x3f317180
const LN2_LO_L: f32 = 9.058_000_614_5e-6; // 0x3717f7d1
const TWO25: f32 = 3.355_443_2e7; // 0x4c000000
/// |(log(1+s)-log(1-s))/s - Lg(s)| < 2**-34.24
const LG1: f32 = 0.666_666_63; // 0xaaaaaa.0p-24
const LG2: f32 = 0.400_009_72; // 0xccce13.0p-25
const LG3: f32 = 0.284_987_87; // 0x91e9ee.0p-25
const LG4: f32 = 0.242_790_79; // 0xf89e26.0p-26

/// `gm_logf`: the natural logarithm of `x`.
///
/// Ported from `float/e_logf.c` (`__ieee754_logf`).
#[must_use]
pub fn logf(mut x: f32) -> f32 {
    let mut k: i32 = 0;

    let mut ix = x.to_bits() as i32;
    if ix < 0x0080_0000 {
        // x < 2**-126
        if (ix & 0x7fff_ffff) == 0 {
            return -TWO25 / 0.0; // log(+-0) = -inf
        }
        if ix < 0 {
            return (x - x) / 0.0; // log(-#) = NaN
        }
        k -= 25;
        x *= TWO25; // subnormal number, scale up x
        ix = x.to_bits() as i32;
    }
    if ix >= 0x7f80_0000 {
        return x + x;
    }
    k += (ix >> 23) - 127;
    ix &= 0x007f_ffff;
    let mut i = (ix.wrapping_add(0x0009_5f64 << 3)) & 0x0080_0000;
    // normalize x or x/2
    x = f32::from_bits((ix | (i ^ 0x3f80_0000)) as u32);
    k += i >> 23;
    let f = x - 1.0;
    if (0x007f_ffff & (0x8000 + ix)) < 0xc000 {
        // -2**-9 <= f < 2**-9
        if f == 0.0 {
            if k == 0 {
                return 0.0;
            }
            let dk = k as f32;
            return dk * LN2_HI_L + dk * LN2_LO_L;
        }
        let r = f * f * (0.5 - 0.333_333_34 * f);
        if k == 0 {
            return f - r;
        }
        let dk = k as f32;
        return dk * LN2_HI_L - ((r - dk * LN2_LO_L) - f);
    }
    let s = f / (2.0 + f);
    let dk = k as f32;
    let z = s * s;
    i = ix - (0x0006_147a << 3);
    let w = z * z;
    let j = (0x0006_b851 << 3) - ix;
    let t1 = w * (LG2 + w * LG4);
    let t2 = z * (LG1 + w * LG3);
    i |= j;
    let r = t2 + t1;
    if i > 0 {
        let hfsq = 0.5 * f * f;
        if k == 0 {
            return f - (hfsq - s * (hfsq + r));
        }
        return dk * LN2_HI_L - ((hfsq - (s * (hfsq + r) + dk * LN2_LO_L)) - f);
    }
    if k == 0 {
        return f - s * (f - r);
    }
    dk * LN2_HI_L - ((s * (f - r) - dk * LN2_LO_L) - f)
}

// --- k_logf.h / e_log10f.c -----------------------------------------------

const TWO25_L: f32 = 3.355_443_2e7; // 0x4c000000
const IVLN10HI: f32 = 0.434_326_17; // 0x3ede6000
const IVLN10LO: f32 = -3.168_997_136_5e-5; // 0xb804ead9
const LOG10_2HI: f32 = 0.301_029_21; // 0x3e9a2080
const LOG10_2LO: f32 = 7.903_415_166_8e-7; // 0x355427db

/// `k_log1pf(f)`: log1p kernel shared by the log family
/// (from `float/k_logf.h`).
fn k_log1pf(f: f32) -> f32 {
    let s = f / (2.0 + f);
    let z = s * s;
    let w = z * z;
    let t1 = w * (LG2 + w * LG4);
    let t2 = z * (LG1 + w * LG3);
    let r = t2 + t1;
    let hfsq = 0.5 * f * f;
    s * (hfsq + r)
}

/// `gm_log10f`: the base-10 logarithm of `x`.
///
/// Ported from `float/e_log10f.c` (`__ieee754_log10f`). The
/// `sizeof(float) > sizeof(float)` branch of the original is dead code and
/// dropped.
#[must_use]
pub fn log10f(mut x: f32) -> f32 {
    let mut k: i32 = 0;

    let mut hx = x.to_bits() as i32;
    if hx < 0x0080_0000 {
        // x < 2**-126
        if (hx & 0x7fff_ffff) == 0 {
            return -TWO25_L / 0.0; // log(+-0) = -inf
        }
        if hx < 0 {
            return (x - x) / 0.0; // log(-#) = NaN
        }
        k -= 25;
        x *= TWO25_L; // subnormal number, scale up x
        hx = x.to_bits() as i32;
    }
    if hx >= 0x7f80_0000 {
        return x + x;
    }
    if hx == 0x3f80_0000 {
        return 0.0; // log(1) = +0
    }
    k += (hx >> 23) - 127;
    hx &= 0x007f_ffff;
    let i = (hx.wrapping_add(0x004a_fb0d)) & 0x0080_0000;
    // normalize x or x/2
    x = f32::from_bits((hx | (i ^ 0x3f80_0000)) as u32);
    k += i >> 23;
    let y = k as f32;
    let f = x - 1.0;
    let hfsq = 0.5 * f * f;
    let r = k_log1pf(f);

    let mut hi = f - hfsq;
    let hx2 = hi.to_bits();
    hi = f32::from_bits(hx2 & 0xffff_f000);
    let lo = (f - hi) - hfsq + r;
    y * LOG10_2LO + (lo + hi) * IVLN10LO + lo * IVLN10HI + hi * IVLN10HI + y * LOG10_2HI
}
