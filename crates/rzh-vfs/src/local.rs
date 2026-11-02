// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Ported from Core/GameEngineDevice/Source/Win32Device/Common/
// Win32LocalFileSystem.cpp @ 571a7c5, Zero Hour configuration (the
// behaviour of the platform layer, rebuilt on std::fs per migration plan
// §1 — platform code is re-implemented, not translated).
//
//! The loose-file side of the overlay (`LocalFileSystem`).
//!
//! The C++ engine resolves game-relative paths against the process
//! working directory with Win32 `FindFirstFile`/`_access`; the port
//! resolves them against a root directory given at construction (the
//! same strings come out — the root plays the process CWD).
//!
//! # Quirks preserved (they are behaviour)
//!
//! - `does_file_exist` uses existence (like `_access(path, 0)`), so a
//!   *directory* exists too.
//! - Listings match `search_name` against each leaf name
//!   **case-insensitively** (Win32 pattern matching); archive listings
//!   match case-sensitively against lowercased names instead — both
//!   behave like their C++ counterparts.
//! - Recursive listings walk into every subdirectory, building the
//!   returned paths with `\` separators regardless of the host platform
//!   (game data paths carry `\` everywhere).
//! - Path lookups follow the platform's case sensitivity: case-blind on
//!   Windows, case-sensitive elsewhere (exactly the `_access`/`fopen`
//!   behaviour the C++ has).
//!
//! # Differences from the C++ (documented)
//!
//! - Win32 `FindFirstFile` also matches 8.3 short names and has a few
//!   DOS-era pattern quirks; the port uses the engine's own wildcard
//!   matcher over the long names. The game only ever passes `*.big`,
//!   `*.ini`-style patterns, where both match identically.
//! - `get_file_info` reports the file's size and its modification time
//!   in seconds since the Unix epoch (the C++ reports the Windows
//!   FILETIME; timestamps are machine state, never compared for parity).
//!
//! # Parity level
//!
//! **P0 exact** for listings and existence (golden-pinned on a
//! synthetic directory tree).

use std::path::{Path, PathBuf};

use crate::path::FilenameList;
use crate::wildcard::search_string_matches;

/// `FileInfo` as far as the data path observes it: sizes are pinned,
/// timestamps are machine state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FileInfo {
    /// The file's size in bytes (`sizeHigh`/`sizeLow` joined).
    pub size: u64,
    /// The modification time in seconds since the Unix epoch, when
    /// available (the C++ stores the Windows FILETIME of the archive
    /// file; in-memory archives have none).
    pub timestamp: Option<u64>,
}

/// The loose-file side of the VFS (`Win32LocalFileSystem` behaviour on
/// `std::fs`). Game-relative paths resolve against `root`.
#[derive(Clone, Debug)]
pub struct LocalFileSystem {
    root: PathBuf,
}

impl LocalFileSystem {
    /// A local layer rooted at `root` (the directory the C++ process
    /// would run from).
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The root directory game-relative paths resolve against.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Joins the game-relative path onto the root.
    #[must_use]
    pub fn resolve(&self, path: &str) -> PathBuf {
        let mut out = self.root.clone();
        // the C++ passes the string to the OS as-is (both separators are
        // accepted on Windows); on other platforms map '\' to the native
        // separator so game paths still resolve
        if !cfg!(windows) {
            let native: String = path.replace('\\', "/");
            out.push(native);
        } else {
            out.push(path);
        }
        out
    }

    /// `doesFileExist` (`_access(filename, 0) == 0`): existence, not
    /// file-ness.
    #[must_use]
    pub fn does_file_exist(&self, path: &str) -> bool {
        std::fs::metadata(self.resolve(path)).is_ok()
    }

    /// `openFile(..., READ)`: an open read handle, like
    /// `Win32LocalFile::open`.
    pub fn open_read(&self, path: &str) -> std::io::Result<std::fs::File> {
        std::fs::File::open(self.resolve(path))
    }

    /// `getFileInfo`: size and modification time.
    #[must_use]
    pub fn get_file_info(&self, path: &str) -> Option<FileInfo> {
        let meta = std::fs::metadata(self.resolve(path)).ok()?;
        let timestamp = meta
            .modified()
            .ok()
            .and_then(|at| at.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs());
        Some(FileInfo {
            size: meta.len(),
            timestamp,
        })
    }

    /// `createDirectory` (`CreateDirectory`: one level, no parents).
    pub fn create_directory(&self, directory: &str) -> bool {
        std::fs::create_dir(self.resolve(directory)).is_ok()
    }

    /// `getFileListInDirectory(currentDirectory, originalDirectory,
    /// searchName, ...)`: the files under `original_directory` whose leaf
    /// names match `search_name`, plus — when `search_subdirectories` —
    /// the same match in every subdirectory, recursively.
    ///
    /// Returned paths are `original_directory` + walk + name, joined
    /// with `\` like the C++ builds them (the caller's directory string
    /// is used verbatim, so callers pass trailing separators for
    /// non-empty directories, as the engine does).
    pub fn get_file_list_in_directory(
        &self,
        current_directory: &str,
        original_directory: &str,
        search_name: &str,
        out: &mut FilenameList,
        search_subdirectories: bool,
    ) {
        let directory = format!("{original_directory}{current_directory}");
        let dir_path = self.resolve(&directory);

        let Ok(entries) = std::fs::read_dir(&dir_path) else {
            return; // FindFirstFile failing: nothing to enumerate
        };

        // FindFirstFile(directory + searchName): the non-directory
        // matches (Win32 matching is case-insensitive)
        let pattern = search_name.to_ascii_lowercase();
        for entry in entries.flatten() {
            let is_dir = entry.file_type().is_ok_and(|t| t.is_dir());
            if is_dir {
                continue; // !(findData.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY)
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if search_string_matches(&name.to_ascii_lowercase(), &pattern) {
                out.insert(&format!("{directory}{name}"));
            }
        }

        // FindFirstFile(directory + "*."): every subdirectory, recursing
        // with currentDirectory grown by the subdirectory
        if search_subdirectories {
            let Ok(entries) = std::fs::read_dir(&dir_path) else {
                return;
            };
            for entry in entries.flatten() {
                if !entry.file_type().is_ok_and(|t| t.is_dir()) {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().into_owned();
                // skip "." and ".." like the C++ (never yielded here)
                if name == "." || name == ".." {
                    continue;
                }
                let next_current = format!("{current_directory}{name}\\");
                self.get_file_list_in_directory(
                    &next_current,
                    original_directory,
                    search_name,
                    out,
                    search_subdirectories,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rzh-vfs-local-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn lists_and_matches_case_insensitively() {
        let root = scratch("list");
        std::fs::create_dir_all(root.join("Data/INI")).unwrap();
        std::fs::create_dir_all(root.join("Art/Sub")).unwrap();
        std::fs::write(root.join("Data/INI/Weapon.ini"), b"w").unwrap();
        std::fs::write(root.join("Data/INI/Armor.INI"), b"a").unwrap();
        std::fs::write(root.join("Art/Sub/Model.W3D"), b"m").unwrap();

        let local = LocalFileSystem::new(&root);
        let mut list = FilenameList::new();
        local.get_file_list_in_directory("", "Data\\INI\\", "*.ini", &mut list, false);
        assert_eq!(
            list.into_paths(),
            ["Data\\INI\\Armor.INI", "Data\\INI\\Weapon.ini"]
        );

        // recursive: subdirectories included, backslash-joined paths
        let mut all = FilenameList::new();
        local.get_file_list_in_directory("", "Art\\", "*", &mut all, true);
        assert_eq!(all.into_paths(), ["Art\\Sub\\Model.W3D"]);

        // the pattern is matched case-insensitively (Win32 behaviour)
        let mut upper = FilenameList::new();
        local.get_file_list_in_directory("", "Data\\INI\\", "*.INI", &mut upper, false);
        assert_eq!(upper.len(), 2);

        // missing directory: empty, no panic
        let mut none = FilenameList::new();
        local.get_file_list_in_directory("", "Nope\\", "*", &mut none, true);
        assert!(none.is_empty());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn existence_covers_directories_and_info_reports_sizes() {
        let root = scratch("exist");
        std::fs::create_dir_all(root.join("Data")).unwrap();
        std::fs::write(root.join("Data/File.txt"), b"12345").unwrap();

        let local = LocalFileSystem::new(&root);
        assert!(local.does_file_exist("Data\\File.txt"));
        assert!(local.does_file_exist("Data/File.txt"));
        assert!(local.does_file_exist("Data")); // _access on a directory
        assert!(!local.does_file_exist("Data/Missing.txt"));

        let info = local.get_file_info("Data\\File.txt").unwrap();
        assert_eq!(info.size, 5);
        assert!(info.timestamp.is_some());

        let _ = std::fs::remove_dir_all(&root);
    }
}
