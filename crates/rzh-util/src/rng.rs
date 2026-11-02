// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Ported from Core/GameEngine/Source/Common/RandomValue.cpp @ 571a7c5, Zero
// Hour configuration, **non-retail** branches of the RETAIL_COMPATIBLE_CRC
// switches (ADR-000). The original carries the following notices, preserved
// here:
//
//   RandomValue.cpp — Pseudo-random number generators.
//   Author: Michael S. Booth, January 1998.
//
//   (c) 2001-2003 Electronic Arts Inc.
//
//! The game's deterministic pseudo random number generator.
//!
//! Zero Hour uses one additive-feedback generator per stream (game logic,
//! game client, game audio). The logic stream must produce identical values
//! on every machine in a lockstep match, so the algorithm is ported
//! bit-exactly (parity level **P0**) and never mixed with platform
//! randomness.
//!
//! The C++ implementation keeps the three streams in global arrays
//! (`theGameLogicSeed`, `theGameClientSeed`, `theGameAudioSeed`); per the
//! migration plan's rule "no global mutable state", this port turns each
//! stream into an explicit [`GameRng`] value owned by its context:
//!
//! - `TheGameLogic` owns a `GameRng` for simulation randomness,
//! - the client context owns one for cosmetic randomness,
//! - the audio context owns one for sound variation.
//!
//! # Examples
//!
//! ```
//! use rzh_util::GameRng;
//!
//! // InitRandom(seed): the logic stream of a match.
//! let mut rng = GameRng::seeded(42);
//! assert_eq!(rng.random_int(0, 100), 54);
//! assert_eq!(rng.random_int(0, 100), 34);
//!
//! // The default stream equals the C++ globals before InitRandom.
//! assert_eq!(GameRng::default(), GameRng::seeded(0));
//! ```

/// The six cumulative seed constants of `seedRandom()`.
///
/// `seedRandom` derives the six state words from one 32-bit seed by adding
/// these constants modulo 2³²; they also form the initial state of the three
/// C++ global streams before `InitRandom` (equivalent to a seed of 0).
const SEED_CONSTANTS: [u32; 6] = [
    0xf22d_0e56,
    0x8831_26e9,
    0xc624_dd2f,
    0x0702_c49c,
    0x9e35_3f7d,
    0x6fdf_3b64,
];

/// `theMultFactor`: `1.0f / (float)UINT_MAX`.
///
/// `UINT_MAX` rounds to `2³²` in `f32`, so the division is exactly `2⁻³²`;
/// spelling it out keeps the constant fold identical on every platform.
const THE_MULT_FACTOR: f32 = 1.0 / u32::MAX as f32;

/// `randomValue()`: advances the six-word state and returns the next raw value.
///
/// This is a verbatim transcription of the original x86-style C++: an add
/// with carry chain over `seed[5] … seed[0]` (the `ADC` macro; written with
/// [`u32::carrying_add`] so the compiler emits the hardware `add`/`adc`
/// chain the original's asm comments describe — the macro's
/// `((SUM < (A)) || (SUM < (B)))` carry test is exactly the carry flag),
/// followed by a plain increment of the state that bubbles its carries up.
fn random_value(seed: &mut [u32; 6]) -> u32 {
    let (mut ax, mut carry) = seed[5].carrying_add(seed[4], false);
    seed[4] = ax;

    (ax, carry) = ax.carrying_add(seed[3], carry);
    seed[3] = ax;

    (ax, carry) = ax.carrying_add(seed[2], carry);
    seed[2] = ax;

    (ax, carry) = ax.carrying_add(seed[1], carry);
    seed[1] = ax;

    // The final carry is dead, as in the C++ macro; discard it explicitly.
    (ax, _) = ax.carrying_add(seed[0], carry);
    seed[0] = ax;

    // Increment seed[5] down to seed[1], bubbling the carries up; when
    // every word wraps to zero the whole 192-bit counter rolled over and
    // the original also bumps seed[0] and the returned value. Transcribed
    // as the original's nested `if (!++seed[i])` unroll — the C++ compiler
    // emits straight-line code for it, and so does rustc (a loop over a
    // fixed-size array kept the bounds checks and cost ~2x per draw).
    seed[5] = seed[5].wrapping_add(1);
    if seed[5] == 0 {
        seed[4] = seed[4].wrapping_add(1);
        if seed[4] == 0 {
            seed[3] = seed[3].wrapping_add(1);
            if seed[3] == 0 {
                seed[2] = seed[2].wrapping_add(1);
                if seed[2] == 0 {
                    seed[1] = seed[1].wrapping_add(1);
                    if seed[1] == 0 {
                        seed[0] = seed[0].wrapping_add(1);
                        ax = ax.wrapping_add(1);
                    }
                }
            }
        }
    }

    ax
}

/// Scales a raw value into an integer range (`GetGameXxxRandomValue`).
#[inline]
fn scaled_int(raw: u32, delta: u32, lo: i32) -> i32 {
    ((raw % delta) as i32).wrapping_add(lo)
}

/// Scales a raw value into a real range (`GetGameXxxRandomValueReal`).
///
/// The operation order matches the C++ expression
/// `((Real)(randomValue(...)) * theMultFactor) * delta + lo` exactly.
#[inline]
fn scaled_real(raw: u32, delta: f32, lo: f32) -> f32 {
    (raw as f32 * THE_MULT_FACTOR) * delta + lo
}

/// One stream of the Zero Hour pseudo random number generator.
///
/// Owns the six-word state that the C++ implementation kept in a global
/// array. Create one per stream (logic / client / audio) and keep it in the
/// owning context; do **not** share a logic stream with client-side code,
/// because that is what broke determinism in the original architecture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GameRng {
    seed: [u32; 6],
}

impl GameRng {
    /// Builds a stream from a 32-bit seed, like `seedRandom(SEED, seed)`.
    ///
    /// The six state words are `seed + SEED_CONSTANTS[i]` (mod 2³²), exactly
    /// as the cumulative additions in the original.
    #[inline]
    #[must_use]
    pub fn seeded(seed: u32) -> Self {
        let mut state = [0_u32; 6];
        for (index, word) in state.iter_mut().enumerate() {
            *word = seed.wrapping_add(SEED_CONSTANTS[index]);
        }
        Self { seed: state }
    }

    /// The state the C++ globals have before `InitRandom` runs (seed 0).
    #[must_use]
    pub fn default_seed() -> Self {
        Self::seeded(0)
    }

    /// Returns the next raw 32-bit value, like `randomValue(theGameXxxSeed)`.
    ///
    /// This is the primitive everything else builds on; it is public so
    /// parity tests and future ports can drive it directly. `#[inline]`
    /// keeps the whole six-word carry chain inside the caller's loop —
    /// across crate boundaries (no LTO) a call per draw costs ~1.5x.
    #[inline]
    pub fn raw(&mut self) -> u32 {
        random_value(&mut self.seed)
    }

    /// Returns the current state words (diagnostics and snapshotting only).
    #[must_use]
    pub const fn seed_words(&self) -> [u32; 6] {
        self.seed
    }

    /// A uniform integer in `[lo, hi]`, like `GetGameLogicRandomValue`.
    ///
    /// Port of the **non-retail** branch: when `lo >= hi` the call returns
    /// `hi` **without consuming a raw value**. Otherwise it draws one raw
    /// value and maps it with `raw % (hi - lo + 1) + lo`.
    ///
    /// # Panics
    ///
    /// Panics on a modulo-by-zero for the pathological input
    /// `lo = i32::MIN, hi = i32::MAX`, where `hi - lo + 1` overflows to
    /// zero — the C++ original is equally undefined there.
    #[inline]
    pub fn random_int(&mut self, lo: i32, hi: i32) -> i32 {
        if lo >= hi {
            return hi;
        }
        let delta = hi.wrapping_sub(lo).wrapping_add(1) as u32;
        scaled_int(self.raw(), delta, lo)
    }

    /// A uniform real in `[lo, hi]`, like `GetGameLogicRandomValueReal`.
    ///
    /// Port of the **non-retail** branch: when `lo >= hi` the call returns
    /// `hi` without consuming a raw value. Otherwise it draws one raw value
    /// and maps it with `(raw * 2⁻³²) * (hi - lo) + lo` in `f32`, keeping the
    /// original's operation order for bit-exact results.
    #[inline]
    pub fn random_real(&mut self, lo: f32, hi: f32) -> f32 {
        if lo >= hi {
            return hi;
        }
        let delta = hi - lo;
        scaled_real(self.raw(), delta, lo)
    }

    /// Like [`random_int`](Self::random_int) but does not advance the stream.
    ///
    /// Port of `GetGameLogicRandomValueUnchanged` (non-retail branch): the
    /// draw happens on a copy of the state, so consecutive calls with the
    /// same range return the same value until the stream is advanced by
    /// something else. Used for synchronized-but-tolerant values such as
    /// scripted audio variation.
    #[inline]
    #[must_use]
    pub fn random_int_unchanged(&self, lo: i32, hi: i32) -> i32 {
        if lo >= hi {
            return hi;
        }
        let delta = hi.wrapping_sub(lo).wrapping_add(1) as u32;
        let mut scratch = self.seed;
        scaled_int(random_value(&mut scratch), delta, lo)
    }

    /// Like [`random_real`](Self::random_real) but does not advance the
    /// stream. Port of `GetGameLogicRandomValueRealUnchanged`.
    #[inline]
    #[must_use]
    pub fn random_real_unchanged(&self, lo: f32, hi: f32) -> f32 {
        if lo >= hi {
            return hi;
        }
        let delta = hi - lo;
        let mut scratch = self.seed;
        scaled_real(random_value(&mut scratch), delta, lo)
    }
}

impl Default for GameRng {
    /// The state of the C++ globals before `InitRandom` (seed 0).
    fn default() -> Self {
        Self::default_seed()
    }
}

#[cfg(test)]
mod tests {
    use super::{GameRng, SEED_CONSTANTS};

    #[test]
    fn default_matches_the_initial_global_constants() {
        assert_eq!(GameRng::default().seed_words(), SEED_CONSTANTS);
        assert_eq!(GameRng::default_seed(), GameRng::seeded(0));
    }

    #[test]
    fn seeding_is_cumulative_modulo_2_pow_32() {
        let rng = GameRng::seeded(1);
        let words = rng.seed_words();
        for (i, word) in words.iter().enumerate() {
            assert_eq!(*word, SEED_CONSTANTS[i].wrapping_add(1));
        }
    }

    #[test]
    fn int_range_edge_cases_do_not_consume() {
        let mut rng = GameRng::seeded(7);
        let before = rng.seed_words();
        assert_eq!(rng.random_int(5, 5), 5);
        assert_eq!(rng.random_int(9, 3), 3);
        assert_eq!(rng.seed_words(), before);
    }

    #[test]
    fn real_range_edge_cases_do_not_consume() {
        let mut rng = GameRng::seeded(7);
        let before = rng.seed_words();
        assert_eq!(rng.random_real(2.0, 2.0), 2.0);
        assert_eq!(rng.random_real(4.0, 1.0), 1.0);
        assert_eq!(rng.seed_words(), before);
    }

    #[test]
    fn unchanged_variants_do_not_advance_and_repeat() {
        let rng = GameRng::seeded(0x1234_5678);
        let before = rng.seed_words();
        let first = rng.random_int_unchanged(0, 100);
        let second = rng.random_int_unchanged(0, 100);
        assert_eq!(first, second);
        assert_eq!(rng.seed_words(), before);

        let real_first = rng.random_real_unchanged(0.0, 1.0);
        let real_second = rng.random_real_unchanged(0.0, 1.0);
        assert_eq!(real_first.to_bits(), real_second.to_bits());
        assert_eq!(rng.seed_words(), before);
    }

    #[test]
    fn int_values_stay_in_range() {
        let mut rng = GameRng::seeded(99);
        for _ in 0..1000 {
            let value = rng.random_int(-50, 49);
            assert!((-50..=49).contains(&value));
        }
    }

    #[test]
    fn real_values_stay_in_range() {
        let mut rng = GameRng::seeded(99);
        for _ in 0..1000 {
            let value = rng.random_real(-1.0, 1.0);
            assert!((-1.0..=1.0).contains(&value));
        }
    }

    #[test]
    fn streams_with_equal_seeds_agree() {
        let mut logic = GameRng::seeded(0xdead_beef);
        let mut client = GameRng::seeded(0xdead_beef);
        for _ in 0..64 {
            assert_eq!(logic.raw(), client.raw());
        }
    }

    #[test]
    fn wide_int_range_survives_the_wrapping_delta() {
        // lo = 0, hi = i32::MAX makes hi - lo + 1 overflow into 0x8000_0000;
        // the original relies on two's-complement wrap here.
        let mut rng = GameRng::seeded(5);
        let value = rng.random_int(0, i32::MAX);
        assert!((0..=i32::MAX).contains(&value));
    }
}
