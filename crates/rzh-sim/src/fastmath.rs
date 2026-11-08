//! The Hoffesommer float conversions of `Lib/BaseFunctions.h` — the
//! Zero Hour non-retail build's `REAL_TO_INT_CEIL`/`REAL_TO_INT_FLOOR`
//! backing (origin at baseline 571a7c5; the `RTS_GENERALS &&
//! RETAIL_COMPATIBLE_CRC` branch that uses `ceilf`/`floorf` directly is
//! the retail one, dropped per plan §3.3 rule 1).
//!
//! These are bit-trick routines, not IEEE `floor`/`ceil`: their quirks
//! (the unsigned-char exponent wrap, the `& 31` shift mask, the
//! positive-zero truncation of negative fractions) are pinned by the
//! golden corpora. The domain contract: `fast_float2long_round` follows
//! `lroundf` (round half away from zero) and is exact for the
//! post-floor/ceil integral values the kernel feeds it; the corrupted
//! outputs the tricks produce for |x| ≥ 2³¹ and NaN inputs are out of
//! contract (the original reads freed memory or UB-converts there too).
//!
//! Original: Copyright 2025 Electronic Arts Inc.
//! Rust port Copyright 2026 the RustZeroHour contributors.

/// `fast_float_trunc`: the mantissa-mask truncation ("super fast float
/// trunc routine, works always", courtesy of Martin Hoffesommer). The
/// biased exponent is an *unsigned char*: `exp -= 127` wraps mod 256;
/// the mask is a **signed** int, so its right shift is arithmetic — the
/// sign extension is what keeps the integer mantissa bits (0xff800000
/// shifted by `exp & 31` grows downward from the exponent field, not
/// upward).
pub fn fast_float_trunc(f: f32) -> f32 {
    let x = f.to_bits();
    let exp = (x >> 23) as u8;
    // int mask = exp < 127 ? 0 : 0xff800000;  (negative!)
    let mut mask: i32 = if exp < 127 { 0 } else { 0xff80_0000u32 as i32 };
    let exp = exp.wrapping_sub(127);
    mask >>= (exp & 31) as i32; // the arithmetic shift
    f32::from_bits(x & mask as u32)
}

/// `fast_float_floor`: subtract the one-ulp-below-one constant from
/// negatives, then truncate.
pub fn fast_float_floor(f: f32) -> f32 {
    const ALMOST1: u32 = (126 << 23) | 0x7f_ffff;
    let mut f = f;
    if f.to_bits() & 0x8000_0000 != 0 {
        f -= f32::from_bits(ALMOST1);
    }
    fast_float_trunc(f)
}

/// `fast_float_ceil`: add the one-ulp-below-one constant to positives,
/// then truncate.
pub fn fast_float_ceil(f: f32) -> f32 {
    const ALMOST1: u32 = (126 << 23) | 0x7f_ffff;
    let mut f = f;
    if f.to_bits() & 0x8000_0000 == 0 {
        f += f32::from_bits(ALMOST1);
    }
    fast_float_trunc(f)
}

/// `fast_float2long_round`: `lroundf` — round half away from zero. For
/// the integral values the floors above produce this is exact; the
/// out-of-range/NaN domain is a stated contract (see the module docs).
#[must_use]
pub fn fast_float2long_round(f: f32) -> i32 {
    // f32::round is round-half-away-from-zero, lroundf's rule; the value
    // is integral post-floor/ceil, so the f32 round-trip is exact.
    f.round() as i32
}

/// `REAL_TO_INT_CEIL(x)` (the Zero Hour non-retail build):
/// `fast_float2long_round(fast_float_ceil(x))`.
#[must_use]
pub fn real_to_int_ceil(x: f32) -> i32 {
    fast_float2long_round(fast_float_ceil(x))
}

/// `REAL_TO_INT_FLOOR(x)` (the Zero Hour non-retail build):
/// `fast_float2long_round(fast_float_floor(x))`.
#[must_use]
pub fn real_to_int_floor(x: f32) -> i32 {
    fast_float2long_round(fast_float_floor(x))
}
