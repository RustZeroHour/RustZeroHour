// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
//! Property tests (plan §6 test layers) for the name-key layer: the
//! invariants of the hash functions the interning is built on.
//!
//! The interning table itself is pinned by the golden vectors
//! (deterministic bucket layout); these properties state the algebraic
//! contract of the hashes.

use proptest::collection::vec;
use proptest::prelude::*;

use rzh_util::namekey::{calc_hash_for_lowercase_string, calc_hash_for_string};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// The lowercase hash of a name equals the plain hash of its ASCII
    /// lowercased spelling — the "C locale tolower" fold, for any bytes
    /// (including the non-ASCII ones the fold leaves alone).
    #[test]
    fn lowercase_hash_equals_hash_of_lowercased_name(name in vec(any::<u8>(), 0..64)) {
        let lowered: Vec<u8> = name.iter().map(|&byte| byte.to_ascii_lowercase()).collect();
        prop_assert_eq!(
            calc_hash_for_lowercase_string(&name),
            calc_hash_for_string(&lowered)
        );
    }

    /// Hashing is a pure function of the byte string.
    #[test]
    fn hashes_are_deterministic(name in vec(any::<u8>(), 0..64)) {
        prop_assert_eq!(
            calc_hash_for_string(&name),
            calc_hash_for_string(&name)
        );
    }

    /// Names of equal length that differ only in their last byte hash
    /// differently — provable for the shift-add construction
    /// `h = h*33 + c`: the states differ by `33*(a-b)` modulo 2^32,
    /// and `gcd(33, 2^32) = 1` forces `a == b` for a collision.
    #[test]
    fn hashes_separate_names_differing_in_the_last_byte(
        body in vec(any::<u8>(), 0..64),
        a in any::<u8>(),
        b in any::<u8>(),
    ) {
        prop_assume!(a != b);
        let mut with_a = body.clone();
        with_a.push(a);
        let mut with_b = body;
        with_b.push(b);
        prop_assert_ne!(
            calc_hash_for_string(&with_a),
            calc_hash_for_string(&with_b)
        );
    }
}
