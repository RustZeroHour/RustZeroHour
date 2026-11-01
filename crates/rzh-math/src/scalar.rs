// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Ported from Core/Libraries/Source/WWVegas/WWMath/wwmath.h @ 571a7c5, Zero
// Hour configuration, deterministic branches (USE_DETERMINISTIC_MATH = 1).
// The original carries the following notices, preserved here:
//
//   WWMath — Some simple math functions which work on the built-in types.
//   Westwood Studios. Project Name: WWMath.
//
//   (c) 2001-2003 Electronic Arts Inc.
//
//! Scalar constants and helpers of the WWMath library.
//!
//! This module ports the part of `WWMath` whose results are **IEEE-754
//! exact** on every conforming platform: the constants, `Sqrt` /
//! `Inv_Sqrt_Legacy`, `Fabs`, `Floor`/`Ceil`/`Round`, the clamp/wrap/lerp
//! family and the bit-level float conversions. GameMath (FreeBSD msun), the
//! system libm and Rust's `f32` methods agree bit-for-bit on these (ADR-001
//! records the empirical verification), so the port maps them onto the Rust
//! operations.
//!
//! # Not ported yet
//!
//! The transcendental entry points (`Sin`, `Cos`, `Tan`, `Asin`, `Acos`,
//! `Atan`, `Atan2`, `Pow`, `Exp`, `Log` and the `Fast_*` table lookups) and
//! `Float_To_Long` (needs `gm_lrintf`) are **deferred**: the game routes
//! them to its GameMath (msun) pin, which the generic `libm` crate does
//! not match bit-for-bit (ADR-001). They get a dedicated port before any
//! code that needs them lands; using them now would silently break P0
//! parity with the C++ oracle.
//!
//! # Parity level
//!
//! **P0 exact** for everything in this module, enforced by the golden
//! vectors in `tests/golden/` (generated from a verbatim C++
//! transcription, `tools/differential/gen_golden.cpp`).
//!
//! # Examples
//!
//! ```
//! use rzh_math::scalar;
//!
//! assert_eq!(scalar::clamp(2.0, 0.0, 1.0), 1.0);
//! // Round is floor(x + 0.5), not round-half-away-from-zero.
//! assert_eq!(scalar::round(-0.5), 0.0);
//! assert_eq!(scalar::float_to_int_chop(-1.75), -1);
//! assert_eq!(scalar::float_to_int_floor(-1.25), -2);
//! ```

/// `WWMATH_EPSILON`: the engine's default comparison tolerance.
pub const EPSILON: f32 = 0.0001;

/// `WWMATH_EPSILON2`: [`EPSILON`] squared.
pub const EPSILON2: f32 = EPSILON * EPSILON;

// The constants below keep the original C++ literals verbatim (provenance
// over brevity). They carry more digits than `f32` can hold, so clippy's
// `excessive_precision` and `approx_constant` fire; the rounded values are
// bit-identical to clippy's suggestions (verified against the original
// literals), so the allow is safe.

/// `WWMATH_HALF_PI`.
#[allow(clippy::approx_constant, clippy::excessive_precision)]
pub const HALF_PI: f32 = 1.570_796_327;

/// `WWMATH_PI`.
#[allow(clippy::approx_constant, clippy::excessive_precision)]
pub const PI: f32 = 3.141_592_654;

/// `WWMATH_TWO_PI`.
#[allow(clippy::approx_constant, clippy::excessive_precision)]
pub const TWO_PI: f32 = 6.283_185_308;

/// `WWMATH_FLOAT_MAX` (`FLT_MAX`).
pub const FLOAT_MAX: f32 = f32::MAX;

/// `WWMATH_FLOAT_MIN` (`FLT_MIN`, the smallest *normalized* positive float).
pub const FLOAT_MIN: f32 = f32::MIN_POSITIVE;

/// `WWMATH_SQRT2`.
#[allow(clippy::approx_constant, clippy::excessive_precision)]
pub const SQRT2: f32 = 1.414_213_562;

/// `WWMATH_SQRT3`.
#[allow(clippy::approx_constant, clippy::excessive_precision)]
pub const SQRT3: f32 = 1.732_050_808;

/// `WWMATH_OOSQRT2`: one over the square root of two.
#[allow(clippy::approx_constant, clippy::excessive_precision)]
pub const OOSQRT2: f32 = 0.707_106_781;

/// `WWMATH_OOSQRT3`: one over the square root of three.
#[allow(clippy::approx_constant, clippy::excessive_precision)]
pub const OOSQRT3: f32 = 0.577_350_269;

/// `WWMath::Sqrtf` / `Sqrt(float)`: the square root.
///
/// The game's deterministic build calls `gm_sqrtf` (FreeBSD msun software
/// square root); msun's `sqrtf`, the system `sqrtf` and Rust's `f32::sqrt`
/// are all correctly rounded per IEEE-754 and therefore bit-identical
/// (ADR-001).
#[must_use]
#[inline]
pub fn sqrt(x: f32) -> f32 {
    f32::sqrt(x)
}

/// `WWMath::Inv_Sqrt_Legacy`, deterministic branch: `1 / sqrt(a)`.
///
/// Not the reciprocal-square-root of the name (that assembly path only
/// compiled for retail x86); the deterministic build is a plain division.
#[must_use]
#[inline]
pub fn inv_sqrt(a: f32) -> f32 {
    1.0 / f32::sqrt(a)
}

/// `WWMath::Fabsf` / `Fabsf_Legacy`, deterministic branch: the absolute
/// value (a sign-bit clear, bit-identical everywhere).
#[must_use]
#[inline]
pub fn fabs(x: f32) -> f32 {
    f32::abs(x)
}

/// `WWMath::Floorf`.
#[must_use]
#[inline]
pub fn floor(x: f32) -> f32 {
    f32::floor(x)
}

/// `WWMath::Ceilf`.
#[must_use]
#[inline]
pub fn ceil(x: f32) -> f32 {
    f32::ceil(x)
}

/// `WWMath::Roundf`: **`floor(x + 0.5)`**, the original's definition.
///
/// This differs from Rust's `f32::round` (round-half-away-from-zero) for
/// negative halves: `round(-0.5)` is `0.0` here and `-1.0` for `f32::round`.
#[must_use]
#[inline]
pub fn round(x: f32) -> f32 {
    floor(x + 0.5)
}

/// `WWMath::Clamp(float)`: clamps `val` into `[min, max]`.
#[must_use]
#[inline]
pub fn clamp(val: f32, min: f32, max: f32) -> f32 {
    if val < min {
        return min;
    }
    if val > max {
        return max;
    }
    val
}

/// `WWMath::Clamp_Int`: clamps `val` into `[min_val, max_val]`.
#[must_use]
#[inline]
pub fn clamp_int(val: i32, min_val: i32, max_val: i32) -> i32 {
    if val < min_val {
        return min_val;
    }
    if val > max_val {
        return max_val;
    }
    val
}

/// `WWMath::Wrap`: wraps `val` into `[min, max]` with a single correction
/// step (and clamps what the step cannot fix, as the original does).
#[must_use]
#[inline]
pub fn wrap(mut val: f32, min: f32, max: f32) -> f32 {
    // Implemented as an if rather than a while, to long loops
    if val >= max {
        val -= max - min;
    }
    if val < min {
        val += max - min;
    }

    if val < min {
        val = min;
    }
    if val > max {
        val = max;
    }
    val
}

/// `WWMath::Min(float)`.
#[must_use]
#[inline]
pub fn min(a: f32, b: f32) -> f32 {
    if a < b { a } else { b }
}

/// `WWMath::Max(float)`.
#[must_use]
#[inline]
pub fn max(a: f32, b: f32) -> f32 {
    if a > b { a } else { b }
}

/// `WWMath::Lerp(float)`: linear interpolation `a + (b - a) * t`.
#[must_use]
#[inline]
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// `WWMath::Inverse_Lerp`: the `t` for which `lerp(a, b, t) == v`.
#[must_use]
#[inline]
pub fn inverse_lerp(a: f32, b: f32, v: f32) -> f32 {
    (v - a) / (b - a)
}

/// `WWMath::Normalize_Angle`: normalizes an angle into `-PI .. PI`.
#[must_use]
#[inline]
pub fn normalize_angle(angle: f32) -> f32 {
    angle - (TWO_PI * floor((angle + PI) / TWO_PI))
}

/// `WWMath::Div_Safe`, deterministic branch: divides by `divisor` unless it
/// is below `epsilon` (the CRT `max` macro semantics, NaN divisor included).
#[must_use]
#[inline]
pub fn div_safe(dividend: f32, divisor: f32, epsilon: f32) -> f32 {
    dividend / if divisor > epsilon { divisor } else { epsilon }
}

/// `WWMath::Is_Valid_Float`: false for NaN and infinities (exponent 0xFF).
#[must_use]
#[inline]
pub fn is_valid_float(x: f32) -> bool {
    (x.to_bits() & 0x7f80_0000) != 0x7f80_0000
}

/// `WWMath::Float_To_Int_Chop`: float to `int`, truncating toward zero.
///
/// A pure bit-level conversion (no FPU rounding-mode involvement). The
/// original's shifts are only defined for `|f| < 2^31`; this port keeps the
/// original's x86 behaviour for everything else by masking shift counts,
/// so the result is identical on every platform.
#[must_use]
#[inline]
pub fn float_to_int_chop(f: f32) -> i32 {
    let a = f.to_bits() as i32;
    let sign = a >> 31; // 0xFFFFFFFF if negative, 0 if positive
    let mantissa = (a & ((1 << 23) - 1)) | (1 << 23); // add the hidden bit
    let exponent = ((a & 0x7fff_ffff) >> 23) - 127;
    let r = (mantissa as u32)
        .wrapping_shl(8)
        .wrapping_shr((31 - exponent) as u32);
    ((r as i32 ^ sign).wrapping_sub(sign)) & !(exponent >> 31)
}

/// `WWMath::Float_To_Int_Floor`: float to `int`, rounding toward negative
/// infinity, with the original's exact tie and sub-unit handling.
///
/// Same bit-level discipline and domain as [`float_to_int_chop`].
#[must_use]
#[inline]
pub fn float_to_int_floor(f: f32) -> i32 {
    let mut a = f.to_bits() as i32;
    let sign = a >> 31; // 0xFFFFFFFF if negative, 0 if positive
    a &= 0x7fff_ffff; // we don't need the sign any more

    let exponent = (a >> 23) - 127;
    let expsign = !(exponent >> 31); // 0xFFFFFFFF if exponent is positive
    let imask = 1_i32.wrapping_shl((31 - exponent) as u32).wrapping_sub(1); // mask for true integer values
    let mantissa = a & ((1 << 23) - 1); // without the hidden bit
    let r = ((mantissa | (1 << 23)) as u32)
        .wrapping_shl(8)
        .wrapping_shr((31 - exponent) as u32);
    let r = r as i32;

    // if (fabs(value)<1.0) value = 0; copy sign;
    // if (value < 0 && value==(int)(value)) value++;
    // `!` in the original is a LOGICAL not (0 or 1), not a bitwise not.
    let low_bits_zero = i32::from((mantissa << 8) & imask == 0);
    let adjusted = low_bits_zero & (expsign ^ ((a - 1) >> 31)) & sign;
    ((r & expsign) ^ sign).wrapping_add(adjusted)
}

/// `WWMath::Unit_Float_To_Byte`: `(unsigned char)(f * 255.0f)`.
///
/// Domain of the original: `f` in `[0, 1]`. The C++ conversion is undefined
/// outside `[0, 256)`; Rust's `as` saturates. The game never feeds it
/// out-of-range values.
#[must_use]
#[inline]
pub fn unit_float_to_byte(f: f32) -> u8 {
    (f * 255.0) as u8
}

/// `WWMath::Byte_To_Unit_Float`.
#[must_use]
#[inline]
pub fn byte_to_unit_float(byte: u8) -> f32 {
    f32::from(byte) / 255.0
}

/// `WWMath::Fast_Is_Float_Positive`: true unless the sign bit is set.
///
/// `-0.0` counts as negative here (the original checks the bit, not the
/// value).
#[must_use]
#[inline]
pub fn fast_is_float_positive(val: f32) -> bool {
    val.to_bits() & 0x8000_0000 == 0
}

/// `WWMath::Is_Power_Of_2`.
///
/// Quirk preserved: `0` reports as a power of two (`0 & -1 == 0`), exactly
/// like the original.
#[must_use]
#[inline]
pub fn is_power_of_2(val: u32) -> bool {
    val & val.wrapping_sub(1) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_is_floor_plus_half() {
        assert_eq!(round(0.5), 1.0);
        assert_eq!(round(-0.5), 0.0); // NOT f32::round's -1.0
        assert_eq!(round(-1.5), -1.0);
        assert_eq!(round(2.4), 2.0);
        assert_eq!(round(-0.4), 0.0);
    }

    #[test]
    fn chop_truncates_toward_zero() {
        assert_eq!(float_to_int_chop(1.75), 1);
        assert_eq!(float_to_int_chop(-1.75), -1);
        assert_eq!(float_to_int_chop(0.9), 0);
        assert_eq!(float_to_int_chop(-0.9), 0);
        assert_eq!(float_to_int_chop(0.0), 0);
        assert_eq!(float_to_int_chop(-0.0), 0);
        assert_eq!(float_to_int_chop(123_456.0), 123_456);
        assert_eq!(float_to_int_chop(1e9), 1_000_000_000);
    }

    #[test]
    fn floor_conversion_matches_semantics() {
        assert_eq!(float_to_int_floor(1.75), 1);
        assert_eq!(float_to_int_floor(-1.25), -2);
        assert_eq!(float_to_int_floor(-1.0), -1); // exact integer stays
        assert_eq!(float_to_int_floor(0.9), 0);
        assert_eq!(float_to_int_floor(-0.9), -1);
        assert_eq!(float_to_int_floor(0.0), 0);
        assert_eq!(float_to_int_floor(1e9), 1_000_000_000);
    }

    #[test]
    fn wrap_single_steps_and_clamps() {
        assert_eq!(wrap(0.5, 0.0, 1.0), 0.5);
        assert_eq!(wrap(1.5, 0.0, 1.0), 0.5);
        assert_eq!(wrap(-0.5, 0.0, 1.0), 0.5);
        // Two steps away from the range: the single step cannot fix it,
        // the original clamps.
        assert_eq!(wrap(2.5, 0.0, 1.0), 1.0);
        assert_eq!(wrap(-1.5, 0.0, 1.0), 0.0);
    }

    #[test]
    fn normalize_angle_brings_into_range() {
        assert_eq!(normalize_angle(0.0), 0.0);
        // The formula maps the boundary +PI to -PI (the division rounds
        // just above 1, so one period is subtracted); both ends of the
        // range are the same angle.
        assert!((normalize_angle(PI) - PI).abs() < 1e-6 || (normalize_angle(PI) + PI).abs() < 1e-6);
        assert!(
            (normalize_angle(-PI) + PI).abs() < 1e-6 || (normalize_angle(-PI) - PI).abs() < 1e-6
        );
        assert!((normalize_angle(TWO_PI) - 0.0).abs() < 1e-6);
        assert!((normalize_angle(7.0 * HALF_PI) - (-HALF_PI)).abs() < 1e-5);
    }

    #[test]
    fn is_valid_float_rejects_non_finite() {
        assert!(is_valid_float(0.0));
        assert!(is_valid_float(-1.0));
        assert!(is_valid_float(1e38));
        assert!(!is_valid_float(f32::INFINITY));
        assert!(!is_valid_float(f32::NEG_INFINITY));
        assert!(!is_valid_float(f32::NAN));
    }

    #[test]
    fn fast_positive_checks_the_sign_bit() {
        assert!(fast_is_float_positive(0.0));
        assert!(!fast_is_float_positive(-0.0)); // sign bit set
        assert!(!fast_is_float_positive(-1.0));
        assert!(fast_is_float_positive(1.0));
    }

    #[test]
    fn zero_is_a_power_of_two_like_the_original() {
        assert!(is_power_of_2(0)); // preserved quirk
        assert!(is_power_of_2(1));
        assert!(is_power_of_2(1 << 30));
        assert!(!is_power_of_2(3));
        assert!(!is_power_of_2((1 << 23) | 1));
    }

    #[test]
    fn inv_sqrt_is_one_over_sqrt() {
        assert_eq!(inv_sqrt(4.0), 0.5);
        assert_eq!(inv_sqrt(0.0), f32::INFINITY);
        assert!(inv_sqrt(-1.0).is_nan());
    }

    #[test]
    fn unit_conversions_round_trip() {
        for byte in 0..=255_u8 {
            let f = byte_to_unit_float(byte);
            assert!((0.0..=1.0).contains(&f));
            assert_eq!(unit_float_to_byte(f), byte);
        }
    }

    #[test]
    fn div_safe_floors_the_divisor_at_epsilon() {
        assert_eq!(div_safe(1.0, 0.5, EPSILON), 2.0);
        assert_eq!(div_safe(1.0, 0.000_01, EPSILON), 1.0 / EPSILON);
        // NaN divisor: the CRT max macro picks epsilon.
        assert_eq!(div_safe(1.0, f32::NAN, EPSILON), 1.0 / EPSILON);
    }
}
