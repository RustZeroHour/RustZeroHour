// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
//! Property tests (plan §6 test layers) for the path-key layer: the
//! invariants that make the key the canonical form of a game file path —
//! idempotence, ASCII case folding, separator equivalence and the
//! `FilenameList` dedup that follows from key equality.
//!
//! Paths are generated from a printable ASCII pool that avoids the
//! leading-separator pair (the Windows UNC special case, which is
//! intentionally platform-dependent and stays covered by the unit
//! tests).

use proptest::collection::vec;
use proptest::prelude::*;

use rzh_vfs::path::{FilenameList, compare_path, make_path_key};

/// The character pool: safe characters plus separators.
const POOL: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_-./\\";

/// A pool byte for any position.
fn any_byte() -> impl Strategy<Value = u8> {
    any::<u8>().prop_map(|b| POOL[usize::from(b) % POOL.len()])
}

/// A game-style path: pool characters, never a leading-separator pair
/// (the platform-dependent UNC case).
fn path() -> impl Strategy<Value = String> {
    vec(any_byte(), 1..64).prop_map(|bytes| {
        let out: Vec<u8> = bytes
            .iter()
            .enumerate()
            .filter(|&(i, &byte)| !(i < 2 && (byte == b'/' || byte == b'\\')))
            .map(|(_, &byte)| byte)
            .collect();
        String::from_utf8(out).expect("pool is ASCII")
    })
}

/// Replaces every `/` with `\\`.
fn all_backslashes(path: &str) -> String {
    path.chars()
        .map(|ch| if ch == '/' { '\\' } else { ch })
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// The key of a key is the key: folding is idempotent.
    #[test]
    fn path_keys_are_idempotent(path in path()) {
        let key = make_path_key(&path).expect("short enough");
        let rekey = make_path_key(&key);
        prop_assert_eq!(rekey.as_deref(), Some(key.as_str()));
    }

    /// Upper and lower spellings of the same path share one key.
    #[test]
    fn path_keys_fold_ascii_case(path in path(), upper in any::<bool>()) {
        let spelled: String = if upper {
            path.chars().map(|ch| ch.to_ascii_uppercase()).collect()
        } else {
            path.chars().map(|ch| ch.to_ascii_lowercase()).collect()
        };
        let key_of_spelled = make_path_key(&spelled);
        let key_of_path = make_path_key(&path);
        prop_assert_eq!(key_of_spelled.as_deref(), key_of_path.as_deref());
    }

    /// `/` and `\\` are the same separator to the key.
    #[test]
    fn path_keys_treat_separators_as_equal(path in path()) {
        let key_of_backslashes = make_path_key(&all_backslashes(&path));
        let key_of_path = make_path_key(&path);
        prop_assert_eq!(key_of_backslashes.as_deref(), key_of_path.as_deref());
    }

    /// Equal keys mean `compare_path` reports equality — and the
    /// `FilenameList` holds such paths exactly once.
    #[test]
    fn filename_lists_hold_equal_keys_once(path in path(), upper in any::<bool>()) {
        let other: String = if upper {
            path.chars().map(|ch| ch.to_ascii_uppercase()).collect()
        } else {
            all_backslashes(&path)
        };

        // The two spellings must actually differ for the dedup to mean
        // anything.
        prop_assume!(other != path);

        prop_assert_eq!(compare_path(&path, &other), 0);

        let mut list = FilenameList::new();
        list.insert(&path);
        list.insert(&other);
        prop_assert_eq!(list.len(), 1);
    }
}
