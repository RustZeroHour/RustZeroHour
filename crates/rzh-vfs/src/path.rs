// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Ported from:
//
//   Core/GameEngine/Source/Common/System/FileSystem.cpp (toPathKeyChar,
//     getPathKeyPairEnd, makePathKey, comparePath, appendPathSeparator,
//     removeExtension)
//   Core/Libraries/Source/Lib/Include/Lib/PathUtil.h (isPathSeparator,
//     isFileSystemPathSeparator, getNativePathSeparator, isAbsolutePath,
//     getFileName, getExtension)
//
// The originals carry the notices: Copyright 2025 Electronic Arts Inc.
//
//! Game-path helpers and the **path key** ordering.
//!
//! Game data paths accept both `\` and `/` as separators and are matched
//! case-insensitively. Two paths refer to the same game file exactly when
//! their *path keys* are equal: lowercase ASCII with every run of
//! separators collapsed to one backslash. [`compare_path`] compares the
//! key forms without building them — it is the ordering behind the C++
//! `FilenameLess` set, which decides the order files are *loaded* in, so
//! it is parity-critical, not cosmetic.
//!
//! # Quirks preserved (they are behaviour)
//!
//! - On Windows the leading separator *pair* of a UNC or device path
//!   (`\\server\share`) stays a pair; other platforms (and this port on
//!   them) read two leading separators as one. The rule is selected per
//!   target platform exactly like the C `#ifdef _WIN32`. Game data paths
//!   are relative with single separators, so the ordering of everything
//!   the engine actually enumerates is identical on every platform.
//! - [`make_path_key`] mirrors the fixed C buffer: it fails when the key
//!   does not fit `max_len` bytes (the caller's `keySize`), defaulting to
//!   the C `_MAX_PATH` (260). [`compare_path`] has **no** length limit.
//! - [`get_extension`] only sees the dot when it is inside the file name
//!   (a dot in a directory component does not count).
//!
//! # Parity level
//!
//! **P0 exact** for every function here: golden vectors in
//! `tests/golden/vfs.txt` pin keys, orderings and slices.

use std::cmp::Ordering;

/// `_MAX_PATH`: the fixed key/name buffer size the C++ engine uses.
pub const MAX_PATH: usize = 260;

/// `isPathSeparator`: either separator is a game-path separator (game
/// data paths carry `\` on every platform).
#[must_use]
pub fn is_path_separator(ch: u8) -> bool {
    ch == b'/' || ch == b'\\'
}

/// `isFileSystemPathSeparator`: the separator(s) the host platform accepts
/// when opening files (`\` and `/` on Windows, only `/` elsewhere).
#[must_use]
pub fn is_file_system_path_separator(ch: u8) -> bool {
    if cfg!(windows) {
        ch == b'\\' || ch == b'/'
    } else {
        ch == b'/'
    }
}

/// `getNativePathSeparator`.
#[must_use]
pub fn native_path_separator() -> char {
    if cfg!(windows) { '\\' } else { '/' }
}

/// `isAbsolutePath` (the Windows drive-letter form is checked on Windows
/// only, like the C).
#[must_use]
pub fn is_absolute_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    if is_file_system_path_separator(bytes[0]) {
        return true;
    }
    if cfg!(windows) {
        let has_drive = bytes[0].is_ascii_alphabetic();
        if has_drive
            && bytes.get(1) == Some(&b':')
            && bytes
                .get(2)
                .is_some_and(|&c| is_file_system_path_separator(c))
        {
            return true;
        }
    }
    false
}

/// The character as it is written in a path key (`toPathKeyChar`).
#[must_use]
pub fn to_path_key_char(ch: u8) -> u8 {
    if is_path_separator(ch) {
        return b'\\';
    }
    if ch.is_ascii_uppercase() {
        return ch.to_ascii_lowercase();
    }
    ch
}

/// `getPathKeyPairEnd`: the end of the separator pair that starts a UNC or
/// device path. Windows keeps that pair apart from the single separator of
/// a rooted path; other platforms read two leading separators as one.
fn path_key_pair_end(path: &[u8]) -> usize {
    if cfg!(windows) && path.len() >= 2 && is_path_separator(path[0]) && is_path_separator(path[1])
    {
        return 2;
    }
    0
}

/// `makePathKey(key, keySize, path)`: the key form of `path`.
///
/// Writes lowercase ASCII with every run of separators as one backslash
/// (except the Windows UNC pair, see the module docs). Returns `None` and
/// no key when the key does not fit `max_len` bytes (the C returns FALSE
/// and writes an empty key; `max_len` must be at least 1).
///
/// Two paths refer to the same game file exactly when their keys are
/// equal.
pub fn make_path_key_with_limit(path: &str, max_len: usize) -> Option<String> {
    debug_assert!(max_len > 0, "key buffer is empty");

    let src = path.as_bytes();
    let pair_end = path_key_pair_end(src);
    let mut key = Vec::with_capacity(src.len());
    let mut i = 0usize;

    loop {
        let ch = to_path_key_char(*src.get(i).unwrap_or(&0));

        if ch == 0 {
            break;
        }

        if key.len() == max_len.saturating_sub(1) {
            return None;
        }

        key.push(ch);
        i += 1;

        if ch == b'\\' {
            while i > pair_end && src.get(i).is_some_and(|&c| is_path_separator(c)) {
                i += 1;
            }
        }
    }

    Some(String::from_utf8_lossy(&key).into_owned())
}

/// [`make_path_key_with_limit`] with the engine's `_MAX_PATH` buffer
/// (260 bytes), matching the fixed `Char keyBuffer[_MAX_PATH]` callers.
#[must_use]
pub fn make_path_key(path: &str) -> Option<String> {
    make_path_key_with_limit(path, MAX_PATH)
}

/// `comparePath(a, b)`: compares the key forms of two paths like `strcmp`
/// (negative / zero / positive), without building them and without any
/// length limit.
#[must_use]
pub fn compare_path(a: &str, b: &str) -> i32 {
    let a = a.as_bytes();
    let b = b.as_bytes();
    let pair_end_a = path_key_pair_end(a);
    let pair_end_b = path_key_pair_end(b);
    let mut ia = 0usize;
    let mut ib = 0usize;

    loop {
        let ca = to_path_key_char(*a.get(ia).unwrap_or(&0));
        let cb = to_path_key_char(*b.get(ib).unwrap_or(&0));

        if ca != cb || ca == 0 {
            return i32::from(ca) - i32::from(cb);
        }

        ia += 1;
        ib += 1;

        if ca == b'\\' {
            while ia > pair_end_a && a.get(ia).is_some_and(|&c| is_path_separator(c)) {
                ia += 1;
            }
            while ib > pair_end_b && b.get(ib).is_some_and(|&c| is_path_separator(c)) {
                ib += 1;
            }
        }
    }
}

/// [`compare_path`] as an [`Ordering`] (for sort keys).
#[must_use]
pub fn path_ordering(a: &str, b: &str) -> Ordering {
    match compare_path(a, b) {
        n if n < 0 => Ordering::Less,
        0 => Ordering::Equal,
        _ => Ordering::Greater,
    }
}

/// A path stored in the [`FilenameList`] ordering: `Ord`/`Eq` are the C++
/// `FilenameLess` set semantics (path-key order, so `a/b` and `A\B` are
/// the same element), which decides enumeration and load order.
///
/// `std::set<AsciiString, FilenameLess>` from `FileSystem.h`.
#[derive(Clone, Debug)]
pub struct PathKey(pub String);

impl PathKey {
    /// The original spelling of the path.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl PartialEq for PathKey {
    fn eq(&self, other: &Self) -> bool {
        compare_path(&self.0, &other.0) == 0
    }
}
impl Eq for PathKey {}
impl PartialOrd for PathKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for PathKey {
    fn cmp(&self, other: &Self) -> Ordering {
        path_ordering(&self.0, &other.0)
    }
}

/// `FilenameList`: the ordered, de-duplicated set of paths the engine
/// enumerates (`std::set<AsciiString, FilenameLess>`). Iteration order is
/// the path-key order — the order INI files load in — so the backing
/// vector stays **sorted** by [`PathKey`], like the C++ set.
#[derive(Clone, Debug, Default)]
pub struct FilenameList(std::vec::Vec<PathKey>);

impl FilenameList {
    /// An empty list.
    #[must_use]
    pub fn new() -> Self {
        Self(Vec::new())
    }

    /// Inserts a path if its key form is not present yet — in sorted
    /// position, exactly like `filenameList.insert(...)` into the
    /// C++ `std::set` (which orders by `FilenameLess` and collapses
    /// equal keys).
    pub fn insert(&mut self, path: &str) {
        let key = PathKey(path.to_string());
        match self.0.binary_search(&key) {
            Ok(_) => {} // already present under the path-key order
            Err(at) => self.0.insert(at, key),
        }
    }

    /// The number of distinct paths.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the list is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The paths, in path-key order.
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(PathKey::as_str)
    }

    /// The paths, in path-key order, as owned strings.
    #[must_use]
    pub fn into_paths(self) -> Vec<String> {
        self.0.into_iter().map(|k| k.0).collect()
    }
}

/// `getFileName`: the component after the last separator (the whole path
/// when it contains no separator).
#[must_use]
pub fn get_file_name(path: &str) -> &str {
    let bytes = path.as_bytes();
    match bytes.iter().rposition(|&c| is_path_separator(c)) {
        Some(at) => &path[at + 1..],
        None => path,
    }
}

/// `getExtension`: the byte index of the last dot inside the file name, or
/// `None` when there is no such dot.
#[must_use]
pub fn get_extension(path: &str) -> Option<usize> {
    let bytes = path.as_bytes();
    let last_dot = bytes.iter().rposition(|&c| c == b'.')?;
    let last_sep = bytes.iter().rposition(|&c| is_path_separator(c));
    // a dot inside a directory component is not an extension
    if let Some(sep) = last_sep
        && last_dot < sep
    {
        return None;
    }
    Some(last_dot)
}

/// `removeExtension`: truncates the path at its extension dot (in place),
/// returning whether an extension was removed.
pub fn remove_extension(path: &mut String) -> bool {
    match get_extension(path) {
        Some(at) => {
            path.truncate(at);
            true
        }
        None => false,
    }
}

/// `appendPathSeparator`: appends the native separator to nonempty paths
/// unless either separator is already at the end.
pub fn append_path_separator(path: &mut String) {
    if !path.is_empty() && !is_path_separator(*path.as_bytes().last().unwrap()) {
        path.push(native_path_separator());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_keys_collapse_runs_and_case() {
        assert_eq!(
            make_path_key("Data\\\\INI\\\\Weapon.ini").unwrap(),
            "data\\ini\\weapon.ini"
        );
        assert_eq!(
            make_path_key("Data/INI/Weapon.INI").unwrap(),
            "data\\ini\\weapon.ini"
        );
        assert_eq!(make_path_key("a\\\\\\\\b").unwrap(), "a\\b");
        assert_eq!(make_path_key("").unwrap(), "");
        assert_eq!(
            make_path_key("C:\\Dir\\\\File.txt").unwrap(),
            "c:\\dir\\file.txt"
        );
    }

    #[test]
    fn path_keys_fail_when_too_long() {
        let long = "a".repeat(MAX_PATH);
        assert_eq!(make_path_key(&long), None);
        let fits = "a".repeat(MAX_PATH - 1);
        assert!(make_path_key(&fits).is_some());
    }

    #[test]
    fn compare_path_matches_key_equality() {
        assert_eq!(compare_path("Data/INI/x.ini", "data\\INI\\\\x.INI"), 0);
        assert!(compare_path("a\\b", "a\\c") < 0);
        assert!(compare_path("a\\b", "a") > 0);
        // a separator orders before any letter
        assert!(compare_path("a\\b", "ab") < 0);
    }

    #[test]
    fn filename_list_dedups_by_key() {
        let mut list = FilenameList::new();
        list.insert("Data\\INI\\B.ini");
        list.insert("data/ini/a.INI");
        list.insert("Data\\INI\\A.ini");
        // two entries: the key forms of "data/ini/a.INI" and "Data\\INI\\A.ini"
        // are equal, so the set keeps whichever arrived first — but it
        // ORDERS by key (a before b), like the C++ set over original
        // spellings
        assert_eq!(list.len(), 2);
        assert_eq!(
            list.iter().collect::<Vec<_>>(),
            ["data/ini/a.INI", "Data\\INI\\B.ini"]
        );
    }

    #[test]
    fn file_name_and_extension() {
        assert_eq!(get_file_name("Data\\INI\\Weapon.ini"), "Weapon.ini");
        assert_eq!(get_file_name("Data/INI/Weapon.ini"), "Weapon.ini");
        assert_eq!(get_file_name("Weapon.ini"), "Weapon.ini");
        assert_eq!(get_extension("Data\\INI\\Weapon.ini"), Some(15));
        // dot in a directory component is not an extension
        assert_eq!(get_extension("My.Dir\\Weapon"), None);
        let mut p = String::from("Map\\Preview.bmp");
        assert!(remove_extension(&mut p));
        assert_eq!(p, "Map\\Preview");
        let mut q = String::from("NoExt");
        assert!(!remove_extension(&mut q));
    }

    #[test]
    fn append_separator_only_when_needed() {
        let mut a = String::from("Data\\INI");
        append_path_separator(&mut a);
        assert_eq!(a, format!("Data\\INI{}", native_path_separator()));
        let mut b = String::from("Data\\INI/");
        append_path_separator(&mut b);
        assert_eq!(b, "Data\\INI/");
        let mut c = String::new();
        append_path_separator(&mut c);
        assert!(c.is_empty());
    }
}
