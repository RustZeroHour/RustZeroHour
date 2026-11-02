// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
//! Property tests (plan §6 test layers) for the RefPack codec: the
//! round-trip invariant `decompress(compress(x)) == x` over arbitrary
//! byte strings, including the empty input, incompressible random data
//! and long runs of repeated bytes (the codec's best case).
//!
//! The bit-exact parity with the C++ codec is pinned separately by the
//! committed golden vectors; these properties guard the algebraic
//! contract instead.

use proptest::collection::vec;
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// Arbitrary bytes round-trip through compress → decompress.
    #[test]
    fn refpack_round_trips_arbitrary_bytes(data in vec(any::<u8>(), 0..8192)) {
        let packed = rzh_compress::refpack::compress(&data);
        let unpacked = rzh_compress::refpack::decompress(&packed)
            .unwrap_or_else(|error| panic!("decompress failed: {error:?}"));
        prop_assert_eq!(unpacked, data);
    }

    /// Two equal inputs pack to the same bytes (the encoder is a pure
    /// function of its input).
    #[test]
    fn refpack_is_deterministic(data in vec(any::<u8>(), 0..4096)) {
        prop_assert_eq!(
            rzh_compress::refpack::compress(&data),
            rzh_compress::refpack::compress(&data)
        );
    }

    /// Compressible data (long runs) stays a round-trip and actually
    /// shrinks or stays equal, never grows beyond the header overhead
    /// bound.
    #[test]
    fn refpack_round_trips_runs(
        filler in any::<u8>(),
        run_len in 16usize..8192,
        tail in vec(any::<u8>(), 0..64),
    ) {
        let mut data = vec![filler; run_len];
        data.extend_from_slice(&tail);

        let packed = rzh_compress::refpack::compress(&data);
        // A whole-input run compresses to at most the header plus a
        // handful of reference commands per ~4 KB window.
        prop_assert!(
            packed.len() < data.len() + 64,
            "runs should not expand: {} -> {}",
            data.len(),
            packed.len()
        );

        let unpacked = rzh_compress::refpack::decompress(&packed)
            .unwrap_or_else(|error| panic!("decompress failed: {error:?}"));
        prop_assert_eq!(unpacked, data);
    }
}
