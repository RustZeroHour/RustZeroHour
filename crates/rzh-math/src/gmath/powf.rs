// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Rust port Copyright 2026 the RustZeroHour contributors.
//
// Ported from the pinned GameMath sources (TheSuperHackers/GameMath,
// commit 59f7ccd494f7e7c916a784ac26ef266f9f09d78d, fetched by
// cmake/gamemath.cmake), FreeBSD msun float routines:
//
//   float/e_powf.c   (gm_powf, via the __ieee754_powf alias)
//
// The original carries the SunPro copyright notice, preserved per the
// migration plan's porting rules (§3.3):
//
//   Copyright (C) 1993 by Sun Microsystems, Inc. All rights reserved.
//   Developed at SunPro, a Sun Microsystems, Inc. business.
//   Permission to use, copy, modify, and distribute this software is freely
//   granted, provided that this notice is preserved.
//
//! `powf`: x raised to the power y.
//!
//! This is the function `WWMath::Powf` and (via the float cast) the
//! deterministic `WWMath::Pow` call under `USE_DETERMINISTIC_MATH = 1`.
//!
//! # Transcription notes
//!
//! - The C high/low splits (`GET/SET_FLOAT_WORD`) become
//!   [`f32::to_bits`]/[`f32::from_bits`]; word additions that can wrap in C
//!   use [`wrapping_add`](i32::wrapping_add).
//! - The C function reuses its `t`, `t1`, `p_h` locals across the reduction
//!   and final-scaling blocks; this port keeps the same mutable flow so the
//!   statement order stays line-for-line comparable.
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

use super::scalbn::scalbnf;

/// `bp[]`: interval base points 1.0 and 1.5.
const BP: [f32; 2] = [1.0, 1.5];
/// `dp_h[]`: high part of the interval corrections.
const DP_H: [f32; 2] = [0.0, 0.584_960_94]; // 0x3f15c000
/// `dp_l[]`: low part of the interval corrections.
const DP_L: [f32; 2] = [0.0, 1.563_220_85e-6]; // 0x35d1cfdc
const ZERO: f32 = 0.0;
const HALF: f32 = 0.5;
const QRTR: f32 = 0.25;
const THRD: f32 = 0.333_333_34; // 0x3eaaaaab
const ONE: f32 = 1.0;
const TWO: f32 = 2.0;
const TWO24: f32 = 16_777_216.0; // 0x4b800000
const HUGE: f32 = 1.0e30;
const TINY: f32 = 1.0e-30;
// poly coefs for (3/2)*(log(x)-2s-2/3*s**3
const L1: f32 = 0.600_000_02; // 0x3f19999a
const L2: f32 = 0.428_571_43; // 0x3edb6db7
const L3: f32 = 0.333_333_34; // 0x3eaaaaab
const L4: f32 = 0.272_728_12; // 0x3e8ba305
const L5: f32 = 0.230_660_75; // 0x3e6c3255
const L6: f32 = 0.206_975_01; // 0x3e53f142
const P1: f32 = 0.166_666_67; // 0x3e2aaaab
const P2: f32 = -0.002_777_777_8; // 0xbb360b61
const P3: f32 = 6.613_756e-5; // 0x388ab355
const P4: f32 = -1.653_390_2e-6; // 0xb5ddea0e
const P5: f32 = 4.138_136_944_2e-8; // 0x3331bb4c
const LG2: f32 = 0.693_147_18; // 0x3f317218
const LG2_H: f32 = 0.693_145_75; // 0x3f317200
const LG2_L: f32 = 1.428_606_54e-6; // 0x35bfbe8c
/// `ovt` = -(128-log2(ovfl+.5ulp)).
const OVT: f32 = 4.299_566_569_4e-8;
const CP: f32 = 0.961_796_7; // 0x3f76384f = 2/(3ln2)
const CP_H: f32 = 0.961_914_06; // 0x3f764000 = 12b cp
const CP_L: f32 = -1.173_685_740_2e-4; // 0xb8f623c6 = tail of cp_h
const IVLN2: f32 = 1.442_695; // 0x3fb8aa3b = 1/ln2
const IVLN2_H: f32 = 1.442_687_988_3; // 0x3fb8aa00 = 16b 1/ln2
const IVLN2_L: f32 = 7.052_607_543_3e-6; // 0x36eca570 = 1/ln2 tail

/// `gm_powf`: `x` raised to the power `y`.
///
/// Ported from `float/e_powf.c` (`__ieee754_powf`).
#[allow(clippy::too_many_lines)]
#[must_use]
pub fn powf(x: f32, y: f32) -> f32 {
    let hx = x.to_bits() as i32;
    let hy = y.to_bits() as i32;
    let mut ix = hx & 0x7fff_ffff;
    let iy = hy & 0x7fff_ffff;

    // y == zero: x**0 = 1
    if iy == 0 {
        return ONE;
    }
    // x == 1: 1**y = 1, even if y is NaN
    if hx == 0x3f80_0000 {
        return ONE;
    }
    // y != zero: result is NaN if either arg is NaN
    if ix > 0x7f80_0000 || iy > 0x7f80_0000 {
        return (x + 0.0) + (y + 0.0);
    }

    // determine if y is an odd int when x < 0
    //     yisint = 0 ... y is not an integer
    //     yisint = 1 ... y is an odd int
    //     yisint = 2 ... y is an even int
    let mut yisint: i32 = 0;
    if hx < 0 {
        if iy >= 0x4b80_0000 {
            yisint = 2; // even integer y
        } else if iy >= 0x3f80_0000 {
            let k = (iy >> 23) - 0x7f; // exponent
            let j = iy >> (23 - k);
            if (j << (23 - k)) == iy {
                yisint = 2 - (j & 1);
            }
        }
    }

    // special value of y
    if iy == 0x7f80_0000 {
        // y is +-inf
        if ix == 0x3f80_0000 {
            return ONE; // (-1)**+-inf is NaN
        } else if ix > 0x3f80_0000 {
            // (|x|>1)**+-inf = inf,0
            return if hy >= 0 { y } else { ZERO };
        }
        // (|x|<1)**-,+inf = inf,0
        return if hy < 0 { -y } else { ZERO };
    }
    if iy == 0x3f80_0000 {
        // y is +-1
        if hy < 0 {
            return ONE / x;
        }
        return x;
    }
    if hy == 0x4000_0000 {
        return x * x; // y is 2
    }
    if hy == 0x3f00_0000 {
        // y is 0.5
        if hx >= 0 {
            // x >= +0
            return f32::sqrt(x); // __ieee754_sqrtf
        }
    }

    let mut ax = f32::abs(x); // gm_fabsf
    // special value of x
    if ix == 0x7f80_0000 || ix == 0 || ix == 0x3f80_0000 {
        let mut z = ax; // x is +-0, +-inf, +-1
        if hy < 0 {
            z = ONE / z; // z = (1/|x|)
        }
        if hx < 0 {
            if ((ix - 0x3f80_0000) | yisint) == 0 {
                z = (z - z) / (z - z); // (-1)**non-int is NaN
            } else if yisint == 1 {
                z = -z; // (x<0)**odd = -(|x|**odd)
            }
        }
        return z;
    }

    let mut n = (((hx as u32) >> 31) as i32).wrapping_sub(1);

    // (x<0)**(non-int) is NaN
    if (n | yisint) == 0 {
        return (x - x) / (x - x);
    }

    let mut sn = ONE; // s (sign of result -ve**odd) = -1 else = 1
    if (n | (yisint - 1)) == 0 {
        sn = -ONE; // (-ve)**(odd int)
    }

    // `t1`/`t2` are assigned by exactly one of the two branches below, as
    // in the C original; clippy's needless_late_init does not understand
    // the dual-branch definite assignment pattern used here.
    #[allow(clippy::needless_late_init)]
    let t1;
    #[allow(clippy::needless_late_init)]
    let t2;
    if iy > 0x4d00_0000 {
        // if |y| > 2**27
        // if |y| > 2**27
        // over/underflow if x is not close to one
        if ix < 0x3f7f_fff8 {
            return if hy < 0 {
                sn * HUGE * HUGE
            } else {
                sn * TINY * TINY
            };
        }
        if ix > 0x3f80_0007 {
            return if hy > 0 {
                sn * HUGE * HUGE
            } else {
                sn * TINY * TINY
            };
        }
        // now |1-x| is tiny <= 2**-20, suffice to compute
        // log(x) by x-x^2/2+x^3/3-x^4/4
        let t = ax - 1.0; // t has 20 trailing zeros
        let w = (t * t) * (HALF - t * (THRD - t * QRTR));
        let u = IVLN2_H * t; // ivln2_h has 16 sig. bits
        let v = t * IVLN2_L - w * IVLN2;
        let mut t1_local = u + v;
        let is = t1_local.to_bits();
        t1_local = f32::from_bits(is & 0xffff_f000);
        t1 = t1_local;
        t2 = v - (t1 - u);
    } else {
        n = 0;
        // take care subnormal number
        if ix < 0x0080_0000 {
            ax *= TWO24;
            n -= 24;
            ix = ax.to_bits() as i32;
        }
        n += (ix >> 23) - 0x7f;
        let j = ix & 0x007f_ffff;
        // determine interval
        ix = j | 0x3f80_0000; // normalize ix
        let k;
        if j <= 0x1cc471 {
            k = 0; // |x| < sqrt(3/2)
        } else if j < 0x5db3d7 {
            k = 1; // |x| < sqrt(3)
        } else {
            k = 0;
            n += 1;
            ix -= 0x0080_0000;
        }
        ax = f32::from_bits(ix as u32);

        // compute s = s_h+s_l = (x-1)/(x+1) or (x-1.5)/(x+1.5)
        let u = ax - BP[k as usize]; // bp[0]=1.0, bp[1]=1.5
        let v = ONE / (ax + BP[k as usize]);
        let s = u * v;
        let mut s_h = s;
        let is = s_h.to_bits();
        s_h = f32::from_bits(is & 0xffff_f000);
        // t_h = ax+bp[k] high
        let is = (((ix as u32) >> 1) & 0xffff_f000) | 0x2000_0000;
        let t_h = f32::from_bits(is.wrapping_add(0x0040_0000 + ((k as u32) << 21)));
        let t_l = ax - (t_h - BP[k as usize]);
        let s_l = v * ((u - s_h * t_h) - s_h * t_l);
        // compute log(ax)
        let mut s2 = s * s;
        let mut r = s2 * s2 * (L1 + s2 * (L2 + s2 * (L3 + s2 * (L4 + s2 * (L5 + s2 * L6)))));
        r += s_l * (s_h + s);
        s2 = s_h * s_h;
        let mut t_h2 = 3.0 + s2 + r;
        let is2 = t_h2.to_bits();
        t_h2 = f32::from_bits(is2 & 0xffff_f000);
        let t_l2 = r - ((t_h2 - 3.0) - s2);
        // u+v = s*(1+...)
        let u2 = s_h * t_h2;
        let v2 = s_l * t_h2 + t_l2 * s;
        // 2/(3log2)*(s+...)
        let mut p_h = u2 + v2;
        let is3 = p_h.to_bits();
        p_h = f32::from_bits(is3 & 0xffff_f000);
        let p_l = v2 - (p_h - u2);
        let z_h = CP_H * p_h; // cp_h+cp_l = 2/(3*log2)
        let z_l = CP_L * p_h + p_l * CP + DP_L[k as usize];
        // log2(ax) = (s+..)*2/(3*log2) = n + dp_h + z_h + z_l
        let t = n as f32;
        let mut t1_local = (z_h + z_l) + DP_H[k as usize] + t;
        let is4 = t1_local.to_bits();
        t1_local = f32::from_bits(is4 & 0xffff_f000);
        t1 = t1_local;
        t2 = z_l - (((t1 - t) - DP_H[k as usize]) - z_h);
    }

    // split up y into y1+y2 and compute (y1+y2)*(t1+t2)
    let is = y.to_bits();
    let y1 = f32::from_bits(is & 0xffff_f000);
    let p_l = (y - y1) * t1 + y * t2;
    let mut p_h = y1 * t1;
    let mut z = p_l + p_h;
    let j = z.to_bits() as i32;
    if j > 0x4300_0000 {
        // if z > 128
        return sn * HUGE * HUGE; // overflow
    } else if j == 0x4300_0000 && p_l + OVT > z - p_h {
        // if z == 128
        return sn * HUGE * HUGE; // overflow
    } else if (j & 0x7fff_ffff) > 0x4316_0000 {
        // z <= -150
        return sn * TINY * TINY; // underflow
    } else if j == (0xc316_0000u32 as i32) && p_l <= z - p_h {
        // z == -150
        return sn * TINY * TINY; // underflow
    }

    // compute 2**(p_h+p_l)
    let i = j & 0x7fff_ffff;
    let mut k = (i >> 23) - 0x7f;
    let mut n = 0;
    if i > 0x3f00_0000 {
        // if |z| > 0.5, set n = [z+0.5]
        n = j + (0x0080_0000 >> (k + 1));
        k = ((n & 0x7fff_ffff) >> 23) - 0x7f; // new k for n
        let t = f32::from_bits((n & !(0x007f_ffff >> k)) as u32);
        n = ((n & 0x007f_ffff) | 0x0080_0000) >> (23 - k);
        if j < 0 {
            n = -n;
        }
        p_h -= t;
    }
    let mut t = p_l + p_h;
    let is2 = t.to_bits();
    t = f32::from_bits(is2 & 0xffff_8000);
    let u = t * LG2_H;
    let v = (p_l - (t - p_h)) * LG2 + t * LG2_L;
    z = u + v;
    let w = v - (z - u);
    let t_z = z * z;
    let t1_z = z - t_z * (P1 + t_z * (P2 + t_z * (P3 + t_z * (P4 + t_z * P5))));
    let r = (z * t1_z) / (t1_z - TWO) - (w + z * w);
    z = ONE - (r - z);
    let j2 = z.to_bits() as i32;
    let j3 = j2.wrapping_add(n << 23);
    if (j3 >> 23) <= 0 {
        z = scalbnf(z, n); // subnormal output
    } else {
        z = f32::from_bits(j3 as u32);
    }
    sn * z
}
