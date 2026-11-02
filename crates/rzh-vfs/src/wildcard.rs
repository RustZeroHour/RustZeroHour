// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Ported from Core/GameEngine/Source/Common/System/ArchiveFile.cpp
// (SearchStringMatches) @ 571a7c5, Zero Hour configuration.
//
//! The engine's `*` / `?` wildcard matcher.
//!
//! Used by every archive and directory enumeration (`*.big`, `*.ini`,
//! `*`). The match is **case-sensitive** and only sees the file's leaf
//! name; the engine lowercases archive leaf names when it builds its
//! trees, so in practice patterns must be lowercase to match archived
//! files (the C++ behaves the same way — the search string is never
//! lowercased).
//!
//! # Quirks preserved (they are behaviour)
//!
//! - The recursion is the original's: on `*` the pattern advances one
//!   character and retries at every remaining position of the subject,
//!   which makes `*` match across the whole remainder (there is no
//!   separator awareness).
//! - An empty subject only matches an empty pattern or a pattern that
//!   reduces to `*`s.
//!
//! # Parity level
//!
//! **P0 exact**: golden vectors in `tests/golden/vfs.txt` pin the truth
//! table, including the corner cases below.

/// `SearchStringMatches(str, searchString)`: does `subject` match
/// `pattern`? `*` denotes any number of characters, `?` a single one.
#[must_use]
pub fn search_string_matches(subject: &str, pattern: &str) -> bool {
    // The C works on NUL-terminated char pointers; the empty-string cases
    // are handled up front exactly like the original.
    if subject.is_empty() {
        return pattern.is_empty();
    }
    if pattern.is_empty() {
        return false;
    }

    let s = subject.as_bytes();
    let p = pattern.as_bytes();
    matches_at(s, 0, p, 0)
}

/// The outer `while` loop of the original, at byte offsets.
fn matches_at(s: &[u8], mut si: usize, p: &[u8], mut pi: usize) -> bool {
    loop {
        let sc = *s.get(si).unwrap_or(&0);
        let pc = *p.get(pi).unwrap_or(&0);

        // while ((*c1 == *c2) || (*c2 == '?') || (*c2 == '*'))
        if !(sc == pc || pc == b'?' || pc == b'*') {
            return false;
        }

        if sc == pc || pc == b'?' {
            si += 1;
            pi += 1;
        } else if pc == b'*' {
            pi += 1;
            if pi >= p.len() || p[pi] == 0 {
                // "*" at the end matches everything that is left
                return true;
            }
            // while (*c1 != 0) { if (match) return TRUE; ++c1; }
            while si < s.len() && s[si] != 0 {
                if matches_at(s, si, p, pi) {
                    return true;
                }
                si += 1;
            }
        }

        if si >= s.len() || s[si] == 0 {
            if pi >= p.len() || p[pi] == 0 {
                return true;
            }
            return false;
        }
        if pi >= p.len() || p[pi] == 0 {
            return false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_and_single_wildcard() {
        assert!(search_string_matches("weapon.ini", "weapon.ini"));
        assert!(search_string_matches("weapon.ini", "weapon.in?"));
        assert!(search_string_matches("weapon.ini", "w?????.in?"));
        assert!(!search_string_matches("weapon.ini", "weapon.txt"));
        assert!(!search_string_matches("weapon.ini", "weapon.i"));
    }

    #[test]
    fn star_patterns() {
        assert!(search_string_matches("weapon.ini", "*.ini"));
        assert!(search_string_matches("weapon.ini", "*"));
        assert!(search_string_matches("weapon.ini", "weapon*"));
        assert!(search_string_matches("weapon.ini", "w*n*i"));
        assert!(search_string_matches("weapon.ini", "**"));
        assert!(!search_string_matches("weapon.ini", "*.txt"));
        assert!(!search_string_matches("weapon.ini", "*x"));
        // multiple stars with a tail that never appears
        assert!(!search_string_matches("weapon.ini", "*z*"));
    }

    #[test]
    fn subject_exhausted_before_a_trailing_star_or_question() {
        // Quirk of the original: the subject reaching its end returns
        // FALSE before a pending trailing '*' (or '?') is processed, so
        // "weapon" does NOT match "weapon*" — the star only matches a
        // nonempty remainder ("weapon.i" would match).
        assert!(!search_string_matches("weapon", "weapon*"));
        assert!(search_string_matches("weapon.i", "weapon*"));
        assert!(!search_string_matches("weapon", "weapon?"));
        assert!(search_string_matches("weaponx", "weapon?"));
    }

    #[test]
    fn empty_cases() {
        assert!(search_string_matches("", ""));
        assert!(!search_string_matches("", "*"));
        assert!(!search_string_matches("", "a"));
        assert!(!search_string_matches("a", ""));
    }

    #[test]
    fn case_sensitivity_is_kept() {
        // the engine lowercases archived leaf names, so an uppercase
        // pattern fails against them; the matcher itself is case-sensitive
        assert!(!search_string_matches("weapon.ini", "*.INI"));
        assert!(search_string_matches("weapon.ini", "*.ini"));
    }
}
