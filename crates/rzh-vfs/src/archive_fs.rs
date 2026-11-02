// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Ported from Core/GameEngine/Source/Common/System/ArchiveFileSystem.cpp
// and the ArchiveFileSystem half of ArchiveFileSystem.h @ 571a7c5, Zero
// Hour configuration. The original carries the notice: Westwood Studios
// Pacific. Copyright (C) 2001. All Rights Reserved.
//
//! The cross-archive overlay tree (`ArchiveFileSystem`).
//!
//! Every mounted BIG archive contributes its files into one shared
//! directory tree. A name can therefore resolve to **several** archives:
//! the C++ keeps a `std::multimap` per directory and selects entries by
//! *file instance* —
//!
//! - instance **0** is the entry that shadows all others (the one a
//!   normal open sees),
//! - instance *N* is the N-th entry counting from the front of the
//!   range, and
//! - **mods mount with `overwrite = true`**, which inserts their files
//!   *before* the existing entries (they become instance 0); base-game
//!   archives mount with `overwrite = false`, appending *after* (so the
//!   first archive mounted wins instance 0).
//!
//! The port models the multimap as a `BTreeMap<name, Vec<archive>>` where
//! the `Vec` is the equal range front-to-back — `overwrite` prepends,
//! plain mounting appends, instance *N* indexes the `Vec`, which is
//! exactly the C++'s `stl::get_range(..., instance)` walk (instance past
//! the end = not found).
//!
//! # Quirks preserved (they are behaviour)
//!
//! - Mounting the same archive twice stacks its entries twice (the C++
//!   re-inserts every file and overwrites only its map slot, leaking the
//!   old `ArchiveFile*`).
//! - Directory walks use the tokenizer-remainder condition, so a query
//!   with a trailing separator descends into its last component as a
//!   directory and then misses (see [`crate::big`]).
//!
//! # Parity level
//!
//! **P0 exact**: golden vectors in `tests/golden/vfs.txt` pin instance
//! resolution, overwrite shadowing and cross-archive listings.

use std::collections::BTreeMap;

use crate::big::{BigArchive, PathTokenizer};
use crate::path::FilenameList;

/// A handle to a mounted archive inside the overlay (index into the
/// overlay's archive storage).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArchiveId(pub(crate) usize);

/// `ArchivedDirectoryInfo`: one node of the shared overlay tree.
#[derive(Clone, Debug, Default)]
struct OverlayDir {
    /// `m_directories`: contained directories by lowercased name.
    directories: BTreeMap<String, OverlayDir>,
    /// `m_files` (`std::multimap<AsciiString, ArchiveFile*>`): the
    /// archives holding a file of this name, front (= instance 0) first.
    files: BTreeMap<String, Vec<ArchiveId>>,
}

/// The shared overlay tree over all mounted archives, plus the mounted
/// archives themselves (`ArchiveFileSystem` + its `m_archiveFileMap`).
#[derive(Clone, Default, Debug)]
pub struct ArchiveFileSystem {
    /// `m_archiveFileMap`: mounted archives by their path as opened
    /// (original case; the iteration order feeds listings, which the
    /// final ordered set normalizes).
    archive_map: BTreeMap<String, ArchiveId>,
    /// The mounted archives, in mount order (ids index this).
    archives: Vec<BigArchive>,
    /// `m_rootDirectory`.
    root: OverlayDir,
}

impl ArchiveFileSystem {
    /// An empty overlay (no archives mounted).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The number of mounted archives (counting remounts, like the C++
    /// map keyed by path would not — see [`Self::mount`] for the stacking
    /// behaviour; this counts distinct paths).
    #[must_use]
    pub fn archive_count(&self) -> usize {
        self.archive_map.len()
    }

    /// The mounted archive with the given path, if any.
    #[must_use]
    pub fn archive_by_name(&self, name: &str) -> Option<&BigArchive> {
        self.archive_map.get(name).map(|id| &self.archives[id.0])
    }

    /// `openArchiveFile` + `loadIntoDirectoryTree` + the map insert: mount
    /// a parsed archive, contributing all of its files to the overlay.
    ///
    /// `overwrite` mirrors the C++ multimap insert hint: `true` places
    /// the archive's files **before** any existing entries under the same
    /// names (mods shadow the base game), `false` appends after them.
    pub fn mount(&mut self, archive: BigArchive, overwrite: bool) {
        let id = ArchiveId(self.archives.len());
        self.archives.push(archive);

        // loadIntoDirectoryTree: list everything, walk the path, insert
        let mut listing = FilenameList::new();
        self.archives[id.0].get_file_list_in_directory("", "*", &mut listing);

        for path in listing.iter() {
            let mut dir = &mut self.root;
            let mut tokenizer = PathTokenizer::new(path); // already lowercase
            let mut token = tokenizer.next_token();
            while !tokenizer.is_empty() {
                let component = token.expect("tokenizer non-empty yields a token");
                dir = dir.directories.entry(component).or_default();
                token = tokenizer.next_token();
            }
            // listing paths always end in a leaf
            if let Some(leaf) = token {
                let entries = dir.files.entry(leaf).or_default();
                if overwrite {
                    // insert before the first equal element
                    entries.insert(0, id);
                } else {
                    // insert at the end of the map (after equals)
                    entries.push(id);
                }
            }
        }

        // m_archiveFileMap[(*it)] = archiveFile (a remount overwrites the
        // slot; the tree entries stack — both like the C++)
        self.archive_map
            .insert(self.archives[id.0].name.clone(), id);
    }

    /// `getArchivedDirectoryInfo`: walks `directory` (mixed case and
    /// separators allowed) to its node, returning the node and the last
    /// token (the file name when the path is a file path).
    fn directory_info(&self, directory: &str) -> Option<(&OverlayDir, String)> {
        let mut dir = &self.root;
        let lowered = directory.to_ascii_lowercase();
        let mut tokenizer = PathTokenizer::new(&lowered);
        let mut token = tokenizer.next_token();
        while !tokenizer.is_empty() {
            let component = token?;
            dir = dir.directories.get(&component)?;
            token = tokenizer.next_token();
        }
        Some((dir, token.unwrap_or_default()))
    }

    /// `getArchiveFile(filename, instance)`: the archive that provides
    /// instance `instance` of `filename`.
    #[must_use]
    pub fn get_archive(&self, filename: &str, instance: u8) -> Option<&BigArchive> {
        let (dir, leaf) = self.directory_info(filename)?;
        let id = dir.files.get(&leaf)?.get(usize::from(instance))?;
        Some(&self.archives[id.0])
    }

    /// `doesFileExist(filename, instance)`.
    #[must_use]
    pub fn does_file_exist(&self, filename: &str, instance: u8) -> bool {
        let Some((dir, leaf)) = self.directory_info(filename) else {
            return false;
        };
        dir.files
            .get(&leaf)
            .is_some_and(|entries| usize::from(instance) < entries.len())
    }

    /// `openFile(filename, instance)` (read path): the archive entry's
    /// bytes, read whole like `RAMFile::openFromArchive`.
    pub fn read_file(&self, filename: &str, instance: u8) -> Option<Vec<u8>> {
        let archive = self.get_archive(filename, instance)?;
        let info = archive.archived_file_info(filename)?;
        archive.read_entry(info).ok()
    }

    /// `openFile(filename, instance)` with `File::STREAMING`: a windowed
    /// view over the archive backing.
    pub fn open_streaming(
        &self,
        filename: &str,
        instance: u8,
    ) -> Option<crate::big::StreamingFile> {
        let archive = self.get_archive(filename, instance)?;
        let info = archive.archived_file_info(filename)?;
        archive.open_streaming(info).ok()
    }

    /// `getFileInfo(filename, instance)`: the entry's size (the C++ also
    /// copies the archive file's timestamp; the port reports sizes only).
    #[must_use]
    pub fn file_size(&self, filename: &str, instance: u8) -> Option<u64> {
        let archive = self.get_archive(filename, instance)?;
        let info = archive.archived_file_info(filename)?;
        Some(u64::from(info.size))
    }

    /// `getFileListInDirectory(currentDirectory, originalDirectory,
    /// searchName, ...)`: every file of every mounted archive under
    /// `original_directory` whose leaf matches `search_name`, merged into
    /// `out` (the C++ loops over `m_archiveFileMap`; the ordered set
    /// deduplicates).
    pub fn get_file_list_in_directory(
        &self,
        original_directory: &str,
        search_name: &str,
        out: &mut FilenameList,
    ) {
        for archive in &self.archives {
            archive.get_file_list_in_directory(original_directory, search_name, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal BIG builder for the overlay tests (shared shape with the
    /// big.rs tests, kept local so the modules stay independent).
    fn build_big(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"BIGF");
        let table_len: usize = entries.iter().map(|(path, _)| 8 + path.len() + 1).sum();
        let total = 16 + table_len + entries.iter().map(|(_, d)| d.len()).sum::<usize>();
        out.extend_from_slice(&(total as u32).to_le_bytes());
        out.extend_from_slice(&(entries.len() as u32).to_be_bytes());
        out.extend_from_slice(&16u32.to_be_bytes());
        let mut at = 16 + table_len as u32;
        for (path, data) in entries {
            out.extend_from_slice(&at.to_be_bytes());
            out.extend_from_slice(&(data.len() as u32).to_be_bytes());
            out.extend_from_slice(path.as_bytes());
            out.push(0);
            at += data.len() as u32;
        }
        for (_, data) in entries {
            out.extend_from_slice(data);
        }
        out
    }

    #[test]
    fn instances_order_by_mount_and_overwrite() {
        let base = BigArchive::open_memory("Base.big", build_big(&[("Data\\Shared.ini", b"base")]))
            .unwrap();
        let mut fs = ArchiveFileSystem::new();
        fs.mount(base, false);
        assert!(fs.does_file_exist("Data\\Shared.ini", 0));
        assert!(!fs.does_file_exist("Data\\Shared.ini", 1));

        // a mod mounts with overwrite and shadows the base game
        let mod1 =
            BigArchive::open_memory("Mod.big", build_big(&[("Data\\Shared.ini", b"mod")])).unwrap();
        fs.mount(mod1, true);
        assert_eq!(fs.read_file("data/shared.ini", 0).unwrap(), b"mod");
        assert_eq!(fs.read_file("data/shared.ini", 1).unwrap(), b"base");
        assert!(!fs.does_file_exist("data/shared.ini", 2));

        // a non-overwriting archive appends behind everything
        let patch =
            BigArchive::open_memory("Patch.big", build_big(&[("Data\\Shared.ini", b"patch")]))
                .unwrap();
        fs.mount(patch, false);
        assert_eq!(fs.read_file("data/shared.ini", 0).unwrap(), b"mod");
        assert_eq!(fs.read_file("data/shared.ini", 1).unwrap(), b"base");
        assert_eq!(fs.read_file("data/shared.ini", 2).unwrap(), b"patch");
    }

    #[test]
    fn queries_are_case_and_separator_insensitive() {
        let a =
            BigArchive::open_memory("A.big", build_big(&[("Art\\Textures\\Sand.tga", b"sand")]))
                .unwrap();
        let mut fs = ArchiveFileSystem::new();
        fs.mount(a, false);
        for query in [
            "Art\\Textures\\Sand.tga",
            "art/textures/sand.TGA",
            "ART\\textures\\Sand.TGA",
        ] {
            assert_eq!(fs.read_file(query, 0).unwrap(), b"sand", "{query}");
        }
        // trailing separator: descends into "sand.tga" as a directory and
        // misses (tokenizer-remainder condition, like the C++)
        assert!(!fs.does_file_exist("art/textures/sand.tga/", 0));
    }

    #[test]
    fn listings_merge_across_archives() {
        let a = BigArchive::open_memory("A.big", build_big(&[("Data\\A.ini", b"a")])).unwrap();
        let b = BigArchive::open_memory("B.big", build_big(&[("Data\\B.ini", b"b")])).unwrap();
        let mut fs = ArchiveFileSystem::new();
        fs.mount(a, false);
        fs.mount(b, false);

        let mut list = FilenameList::new();
        fs.get_file_list_in_directory("data\\", "*.ini", &mut list);
        assert_eq!(list.into_paths(), ["data\\a.ini", "data\\b.ini"]);

        let mut all = FilenameList::new();
        fs.get_file_list_in_directory("", "*", &mut all);
        assert_eq!(all.into_paths(), ["data\\a.ini", "data\\b.ini"]);
    }
}
