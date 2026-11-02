// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Ported from Core/Libraries/Source/WWVegas/WWMath/quat.h and quat.cpp
// @ 571a7c5, Zero Hour configuration, deterministic branches
// (USE_DETERMINISTIC_MATH = 1).
//
//! The `WWMath` fast-trig lookup tables and their interpolation entry
//! points.
//!
//! In the C++ these are four global tables initialized by
//! `WWMath::Init()` at engine startup from the GameMath pins (`Acosf`,
//! `Asinf`, `Sinf` — the non-retail branches, which this port follows).
//! The Rust port computes them once, lazily, from the same bit-exact
//! [`crate::gmath`] functions and caches them in a [`OnceLock`]: the
//! tables are pure functions of deterministic code, immutable after
//! first use, and thread-safe. This is *not* the "global mutable state"
//! the migration plan §3.3 bans (that rule targets simulation
//! singletons); it is the moral equivalent of a constant, and it keeps
//! the original's zero-argument call sites (`fast_sin(x)`) working.
//!
//! # Parity level
//!
//! **P0 exact**: the table contents are bit-identical to a C++ run of
//! `WWMath::Init()` (each entry is a direct `gm_acosf`/`gm_asinf`/
//! `gm_sinf` call, all golden-pinned), and the interpolation code is a
//! verbatim transcription, pinned by `tests/golden/matrices.txt`.

use std::sync::OnceLock;

use crate::gmath;
use crate::scalar;

/// `ARC_TABLE_SIZE` (1024): size of the acos/asin tables.
const ARC_TABLE_SIZE: usize = 1024;

/// `SIN_TABLE_SIZE` (1024): size of the sin table.
const SIN_TABLE_SIZE: usize = 1024;

/// The four lookup tables of `WWMath::Init()`.
#[derive(Debug)]
pub(crate) struct FastTables {
    /// `_FastAcosTable[ARC_TABLE_SIZE]`.
    acos: [f32; ARC_TABLE_SIZE],
    /// `_FastAsinTable[ARC_TABLE_SIZE]`.
    asin: [f32; ARC_TABLE_SIZE],
    /// `_FastSinTable[SIN_TABLE_SIZE]`.
    sin: [f32; SIN_TABLE_SIZE],
    /// `_FastInvSinTable[SIN_TABLE_SIZE]`.
    inv_sin: [f32; SIN_TABLE_SIZE],
}

impl FastTables {
    fn new() -> Self {
        let mut tables = Self {
            acos: [0.0; ARC_TABLE_SIZE],
            asin: [0.0; ARC_TABLE_SIZE],
            sin: [0.0; SIN_TABLE_SIZE],
            inv_sin: [0.0; SIN_TABLE_SIZE],
        };

        let mut a = 0;
        while a < ARC_TABLE_SIZE {
            // cv = float(a-ARC_TABLE_SIZE/2) * (1.0f/(ARC_TABLE_SIZE/2))
            let cv = ((a as f32) - (ARC_TABLE_SIZE as f32) / 2.0)
                * (1.0 / (ARC_TABLE_SIZE as f32 / 2.0));
            tables.acos[a] = gmath::acosf(cv); // Acosf (non-retail branch)
            tables.asin[a] = gmath::asinf(cv); // Asinf (non-retail branch)
            a += 1;
        }

        let mut a = 0;
        while a < SIN_TABLE_SIZE {
            // cv = float(a) * 2.0f * WWMATH_PI / SIN_TABLE_SIZE
            let cv = (a as f32) * 2.0 * scalar::PI / (SIN_TABLE_SIZE as f32);
            tables.sin[a] = gmath::sinf(cv); // Sinf (non-retail branch)

            if a > 0 {
                tables.inv_sin[a] = 1.0 / tables.sin[a];
            } else {
                tables.inv_sin[a] = scalar::FLOAT_MAX;
            }
            a += 1;
        }

        tables
    }
}

/// Returns the process-wide tables, computing them on first use.
fn tables() -> &'static FastTables {
    static TABLES: OnceLock<FastTables> = OnceLock::new();
    TABLES.get_or_init(FastTables::new)
}

/// `WWMath::Fast_Acos(val)`.
///
/// Falls back to [`gmath::acosf`] near ±1 where the table is too
/// inaccurate, exactly like the original.
#[must_use]
pub fn fast_acos(val: f32) -> f32 {
    // Near -1 and +1, the table becomes too inaccurate
    if val.abs() > 0.975 {
        return gmath::acosf(val);
    }

    let t = tables();
    let val = val * (ARC_TABLE_SIZE as f32 / 2.0);

    let idx0 = scalar::float_to_int_floor(val);
    let idx1 = idx0 + 1;
    let frac = val - (idx0 as f32);

    // idx0 += ARC_TABLE_SIZE/2 — signed addition, non-negative afterwards
    // (|val| <= 0.975 keeps |idx0| below half the table).
    let idx0 = (idx0 + (ARC_TABLE_SIZE / 2) as i32) as usize;
    let idx1 = (idx1 + (ARC_TABLE_SIZE / 2) as i32) as usize;

    // compute and return the interpolated value
    (1.0 - frac) * t.acos[idx0] + frac * t.acos[idx1]
}

/// `WWMath::Fast_Asin(val)`.
#[must_use]
pub fn fast_asin(val: f32) -> f32 {
    // Near -1 and +1, the table becomes too inaccurate
    if val.abs() > 0.975 {
        return gmath::asinf(val);
    }

    let t = tables();
    let val = val * (ARC_TABLE_SIZE as f32 / 2.0);

    let idx0 = scalar::float_to_int_floor(val);
    let idx1 = idx0 + 1;
    let frac = val - (idx0 as f32);

    let idx0 = (idx0 + (ARC_TABLE_SIZE / 2) as i32) as usize;
    let idx1 = (idx1 + (ARC_TABLE_SIZE / 2) as i32) as usize;

    (1.0 - frac) * t.asin[idx0] + frac * t.asin[idx1]
}

/// `WWMath::Fast_Sin(val)`.
#[must_use]
pub fn fast_sin(val: f32) -> f32 {
    let t = tables();
    // val *= float(SIN_TABLE_SIZE) / (2.0f * WWMATH_PI): the scale pair is
    // ONE folded f32 constant in the C, so it must be here too (two
    // operations would round differently).
    let val = val * (SIN_TABLE_SIZE as f32 / (2.0 * scalar::PI));

    let idx0 = scalar::float_to_int_floor(val);
    let idx1 = idx0 + 1;
    let frac = val - (idx0 as f32);

    let idx0 = (idx0 as u32 as usize) & (SIN_TABLE_SIZE - 1);
    let idx1 = (idx1 as u32 as usize) & (SIN_TABLE_SIZE - 1);

    (1.0 - frac) * t.sin[idx0] + frac * t.sin[idx1]
}

/// `WWMath::Fast_Cos(val)`.
#[must_use]
pub fn fast_cos(val: f32) -> f32 {
    let t = tables();
    let val = val + (scalar::PI * 0.5);
    let val = val * (SIN_TABLE_SIZE as f32 / (2.0 * scalar::PI));

    let idx0 = scalar::float_to_int_floor(val);
    let idx1 = idx0 + 1;
    let frac = val - (idx0 as f32);

    let idx0 = (idx0 as u32 as usize) & (SIN_TABLE_SIZE - 1);
    let idx1 = (idx1 as u32 as usize) & (SIN_TABLE_SIZE - 1);

    (1.0 - frac) * t.sin[idx0] + frac * t.sin[idx1]
}

/// `WWMath::Fast_Inv_Sin(val)`: `1 / Fast_Sin(val)` — the table version is
/// disabled behind `#if 0` in the original ("not reliable!").
#[must_use]
pub fn fast_inv_sin(val: f32) -> f32 {
    1.0 / fast_sin(val)
}

/// `WWMath::Fast_Inv_Cos(val)`: `1 / Fast_Cos(val)` — likewise.
#[must_use]
pub fn fast_inv_cos(val: f32) -> f32 {
    1.0 / fast_cos(val)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fast_tables_track_the_exact_functions() {
        // In their valid ranges the table lookups stay within ~1e-3 of
        // the gmath originals (they are linear interpolations over 1024
        // points), and the fallback ranges match exactly.
        let mut x = -0.97f32;
        while x <= 0.97 {
            let d = (fast_acos(x) - gmath::acosf(x)).abs();
            assert!(d < 2.0e-3, "fast_acos({x}) off by {d}");
            let d = (fast_asin(x) - gmath::asinf(x)).abs();
            assert!(d < 2.0e-3, "fast_asin({x}) off by {d}");
            x += 0.0137;
        }
        // outside the table range the exact path is used
        assert_eq!(fast_acos(0.99).to_bits(), gmath::acosf(0.99).to_bits());
        assert_eq!(fast_asin(-0.99).to_bits(), gmath::asinf(-0.99).to_bits());

        let mut x = -6.2f32;
        while x <= 6.2 {
            let d = (fast_sin(x) - gmath::sinf(x)).abs();
            assert!(d < 2.0e-3, "fast_sin({x}) off by {d}");
            let d = (fast_cos(x) - gmath::cosf(x)).abs();
            assert!(d < 2.0e-3, "fast_cos({x}) off by {d}");
            x += 0.05;
        }
    }
}
