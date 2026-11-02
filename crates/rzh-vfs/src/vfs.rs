// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Ported from (baseline 571a7c5, Zero Hour configuration):
//
//   Core/GameEngine/Source/Common/System/FileSystem.cpp (openFile,
//     doesFileExist, getFileInfo, getFileListInDirectory: the local-first
//     overlay routing and the instance decrement)
//   Core/GameEngine/Source/Common/System/BIGFileSystem.cpp
//     (loadArchiveFilesFromDirectory, loadMods, the INIZH.big duplicate
//     skip)
//
// The originals carry the notices: (c) 2001-2003 Electronic Arts Inc.
//
//! The virtual file system (`FileSystem` + `BIGFileSystem`).
//!
//! One [`Vfs`] owns the local layer and the archive overlay and answers
//! every query the engine asks — existence, opening, listing, info —
//! with the C++'s exact routing:
//!
//! 1. **instance 0 opens resolve local-first**: a loose file shadows the
//!    same name inside any archive;
//! 2. **instance N first "spends" the local layer**: if a loose file of
//!    that name exists, the archive lookup runs with instance N-1 (so
//!    instance 1 means "the file behind the loose one"); without a loose
//!    file, instance N goes straight to the N-th archive entry;
//! 3. archive listings merge across every mounted archive, and
//!    `load_archives_from_directory` mounts `*.big` files **recursively**
//!    in path-key order — that order fixes which archive wins instance 0.
//!
//! # The global singletons (design note)
//!
//! The C++ reaches this through `TheFileSystem`, `TheLocalFileSystem`
//!   and `TheArchiveFileSystem`. Per the plan's porting rules (§3.3,
//!   "no global mutable state") the Rust side is one owned [`Vfs`].
//!
//! # Not ported (documented, with their consumers)
//!
//! - `FileSystem::normalizePath` / `isPathInDirectory` (Win32
//!   `GetFullPathName` semantics; used by map previews and user paths —
//!   Phase 4+).
//! - The write side of `openFile` (create/truncate + the copy-out of
//!   archive files to disk): the Rust [`Vfs`] is a read path; writing
//!   arrives with the user-data and map-preview flows that need it.
//! - `ENABLE_FILESYSTEM_EXISTENCE_CACHE` (a pure memoization of
//!   `doesFileExist` behind a mutex; no observable effect for a static
//!   mount set).
//! - `BIGFileSystem::init`'s Generals-install registry lookup
//!   (Windows-only; the Rust game reads `RZH_GAME_DIR`, see
//!   `docs/DEVELOPMENT.md`).
//!
//! # Parity level
//!
//! **P0 exact** for the routing table and listings (golden-pinned),
//! including the instance decrement and the INIZH.big skip.
//!
//! # Example
//!
//! ```
//! use rzh_vfs::Vfs;
//!
//! let mut vfs = Vfs::new("game-root");          // where loose files live
//! vfs.load_archives_from_directory("", "*.big", false); // base game
//! if let Some(mod_dir) = None::<&str> {         // mods mount last, on top
//!     vfs.load_archives_from_directory(mod_dir, "*.big", true);
//! }
//! if let Some(data) = vfs.read("Data\\INI\\Weapon.ini") {
//!     // whole-file bytes, from the loose file or the winning archive
//! #     let _ = data;
//! }
//! ```

use std::io::Seek as _;
use std::path::PathBuf;

use crate::archive_fs::ArchiveFileSystem;
use crate::big::BigArchive;
use crate::local::{FileInfo, LocalFileSystem};
use crate::path::FilenameList;
use crate::ramfile::{RamFile, SeekMode};

/// Errors from opening and mounting.
#[derive(Debug)]
pub enum VfsError {
    /// The file was not found at that instance.
    NotFound {
        /// The queried game path.
        name: String,
    },
    /// The archive (or an entry in it) is malformed.
    CorruptArchive {
        /// The archive's path as opened.
        name: String,
        /// What exactly is wrong.
        reason: String,
    },
    /// The host I/O layer failed (open/read/seek).
    Io {
        /// The path involved.
        name: String,
        /// The underlying error.
        source: std::io::Error,
    },
}

impl std::error::Error for VfsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl std::fmt::Display for VfsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound { name } => write!(f, "no file '{name}' at that instance"),
            Self::CorruptArchive { name, reason } => {
                write!(f, "corrupt archive '{name}': {reason}")
            }
            Self::Io { name, source } => write!(f, "I/O error on '{name}': {source}"),
        }
    }
}

/// `FileInstance` selects which of several same-named files to use:
/// 0 is the top one (loose file, else first-loaded archive entry), N
/// digs one level deeper.
pub type FileInstance = u8;

/// An open file from the [`Vfs`]: loose, archive-backed in memory
/// (`RAMFile`), or archive-backed streaming.
pub enum VfsFile {
    /// A loose file on disk (`Win32LocalFile`).
    Local(std::fs::File),
    /// An archive sub-file read whole into memory (`RAMFile`).
    Ram(RamFile),
    /// An archive sub-file as a window over the open archive
    /// (`StreamingArchiveFile`).
    Streaming(crate::big::StreamingFile),
}

impl VfsFile {
    /// Reads up to `out.len()` bytes at the current position.
    pub fn read(&mut self, out: &mut [u8]) -> usize {
        match self {
            Self::Local(file) => read_up_to(file, out).unwrap_or(0),
            Self::Ram(ram) => ram.read(out),
            Self::Streaming(stream) => stream.read(out),
        }
    }

    /// Reads everything from the current position (`readEntireAndClose`'s
    /// read half).
    #[must_use]
    pub fn read_to_end(&mut self) -> Vec<u8> {
        let mut out = Vec::new();
        match self {
            Self::Local(file) => {
                let mut buf = [0u8; 8192];
                loop {
                    let n = read_up_to(file, &mut buf).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    out.extend_from_slice(&buf[..n]);
                }
            }
            Self::Ram(ram) => out.extend_from_slice(ram.read_remaining()),
            Self::Streaming(stream) => {
                let left = stream.size().saturating_sub(stream.pos()) as usize;
                out.resize(left, 0);
                let n = stream.read(&mut out);
                out.truncate(n);
            }
        }
        out
    }

    /// Moves the position (clamped to the data, like every C++ `seek`).
    pub fn seek(&mut self, pos: i64, mode: SeekMode) -> u64 {
        match self {
            Self::Local(file) => {
                let from = match mode {
                    SeekMode::Start => std::io::SeekFrom::Start(pos.unsigned_abs()),
                    SeekMode::Current => std::io::SeekFrom::Current(pos),
                    SeekMode::End => std::io::SeekFrom::End(pos),
                };
                file.seek(from).unwrap_or(0)
            }
            Self::Ram(ram) => u64::try_from(ram.seek(pos, mode)).unwrap_or(0),
            Self::Streaming(stream) => stream.seek(pos, mode),
        }
    }

    /// The total size.
    #[must_use]
    pub fn size(&self) -> u64 {
        match self {
            Self::Local(file) => file.metadata().map(|m| m.len()).unwrap_or(0),
            Self::Ram(ram) => ram.size() as u64,
            Self::Streaming(stream) => stream.size(),
        }
    }

    /// The current position (`tell`). Takes `&mut self` because the
    /// loose-file variant asks the OS (`ftell`).
    pub fn pos(&mut self) -> u64 {
        match self {
            Self::Local(file) => file.stream_position().unwrap_or(0),
            Self::Ram(ram) => ram.pos() as u64,
            Self::Streaming(stream) => stream.pos(),
        }
    }
}

/// `File::read` semantics: fill as much as possible, short count at EOF.
fn read_up_to(reader: &mut impl std::io::Read, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut filled = 0usize;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(filled)
}

/// The virtual file system: local loose files plus the BIG archive
/// overlay (`FileSystem`).
#[derive(Clone, Debug)]
pub struct Vfs {
    local: LocalFileSystem,
    archives: ArchiveFileSystem,
}

impl Vfs {
    /// An empty VFS whose loose-file layer is rooted at `root` (the
    /// directory the C++ process would run from). Mount archives with
    /// the `mount_*` / `load_*` methods.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            local: LocalFileSystem::new(root),
            archives: ArchiveFileSystem::new(),
        }
    }

    /// The local layer's root directory.
    #[must_use]
    pub fn root(&self) -> &std::path::Path {
        self.local.root()
    }

    /// `BIGFileSystem::openArchiveFile` + `loadIntoDirectoryTree` +
    /// the map insert, over a `.big` file on disk (resolved against the
    /// local root when relative). The archive's name — what listings
    /// and the archive map see — is `path` exactly as passed, like the
    /// C++, which uses one string for both opening and naming.
    pub fn mount_big_file(&mut self, path: &str, overwrite: bool) -> Result<(), VfsError> {
        let resolved = self.local.resolve(path);
        let archive = BigArchive::open_file_as(path, &resolved)?;
        self.archives.mount(archive, overwrite);
        Ok(())
    }

    /// [`Vfs::mount_big_file`] over in-memory bytes (tests, tooling).
    /// `name` names the archive for listings.
    pub fn mount_big_memory(
        &mut self,
        name: &str,
        data: Vec<u8>,
        overwrite: bool,
    ) -> Result<(), VfsError> {
        let archive = BigArchive::open_memory(name, data)?;
        self.archives.mount(archive, overwrite);
        Ok(())
    }

    /// `BIGFileSystem::loadArchiveFilesFromDirectory(dir, fileMask,
    /// overwrite)`: mount every archive matching `mask` under `dir`
    /// (recursively, in path-key order) and report whether anything was
    /// mounted.
    ///
    /// Includes the Zero Hour duplicate-INIZH skip: a `Data\INI\INIZH.big`
    /// (either separator, case-insensitive) is not mounted, matching the
    /// upstream fix for the SKU that shipped two copies.
    pub fn load_archives_from_directory(&mut self, dir: &str, mask: &str, overwrite: bool) -> bool {
        // TheLocalFileSystem->getFileListInDirectory(dir, "", mask, list, TRUE)
        let mut list = FilenameList::new();
        self.local
            .get_file_list_in_directory(dir, "", mask, &mut list, true);

        let mut actually_added = false;
        for path in list.iter() {
            if path.ends_with_no_case("Data\\INI\\INIZH.big")
                || path.ends_with_no_case("Data/INI/INIZH.big")
            {
                continue; // TheSuperHackers @bugfix bobtista 18/11/2025
            }
            if self.mount_big_file(path, overwrite).is_ok() {
                actually_added = true;
            }
        }
        actually_added
    }

    /// `BIGFileSystem::loadMods` (the GlobalData-driven part): mount
    /// `mod_big` with overwrite on top of everything, then every
    /// `*.big` under `mod_dir`, also with overwrite.
    pub fn load_mods(&mut self, mod_big: Option<&str>, mod_dir: Option<&str>) {
        if let Some(big) = mod_big {
            // failures are logged-and-skipped in the C++ (DEBUG_LOG)
            let _ = self.mount_big_file(big, true);
        }
        if let Some(dir) = mod_dir {
            let _ = self.load_archives_from_directory(dir, "*.big", true);
        }
    }

    /// `FileSystem::openFile(filename, READ)` at instance 0 — see
    /// [`Vfs::open_instance`] for the full routing.
    pub fn open(&self, name: &str) -> Result<VfsFile, VfsError> {
        self.open_inner(name, 0, false)
    }

    /// `FileSystem::openFile(filename, READ, ..., instance)`.
    pub fn open_instance(&self, name: &str, instance: FileInstance) -> Result<VfsFile, VfsError> {
        self.open_inner(name, instance, false)
    }

    /// The `File::STREAMING` open: an archive entry served as a window
    /// over the still-open archive instead of a whole-file read.
    pub fn open_streaming(&self, name: &str) -> Result<VfsFile, VfsError> {
        self.open_inner(name, 0, true)
    }

    /// The transcription of `FileSystem::openFile`'s body (read path).
    fn open_inner(
        &self,
        name: &str,
        instance: FileInstance,
        streaming: bool,
    ) -> Result<VfsFile, VfsError> {
        let mut archive_instance = instance;

        if instance != 0 {
            // spend the local layer first, if a loose file exists
            if self.local.does_file_exist(name) {
                archive_instance = instance - 1;
            }
        } else {
            // instance 0: the loose file shadows the archives
            if let Ok(file) = self.local.open_read(name) {
                return Ok(VfsFile::Local(file));
            }
        }

        if streaming {
            self.archives
                .open_streaming(name, archive_instance)
                .map(VfsFile::Streaming)
                .ok_or_else(|| VfsError::NotFound {
                    name: name.to_string(),
                })
        } else {
            self.archives
                .read_file(name, archive_instance)
                .map(|data| VfsFile::Ram(RamFile::from_bytes(data)))
                .ok_or_else(|| VfsError::NotFound {
                    name: name.to_string(),
                })
        }
    }

    /// Open and read whole: the `readEntireAndClose` convenience the
    /// INI/chunk loaders use.
    pub fn read(&self, name: &str) -> Option<Vec<u8>> {
        self.read_instance(name, 0)
    }

    /// [`Vfs::read`] with an explicit file instance.
    pub fn read_instance(&self, name: &str, instance: FileInstance) -> Option<Vec<u8>> {
        self.open_instance(name, instance)
            .ok()
            .map(|mut file| file.read_to_end())
    }

    /// `FileSystem::doesFileExist(filename, instance)`.
    #[must_use]
    pub fn does_file_exist(&self, name: &str, instance: FileInstance) -> bool {
        let mut archive_instance = instance;
        if self.local.does_file_exist(name) {
            if instance == 0 {
                return true;
            }
            archive_instance = instance - 1;
        }
        self.archives.does_file_exist(name, archive_instance)
    }

    /// `FileSystem::getFileInfo(filename, instance)`.
    #[must_use]
    pub fn file_info(&self, name: &str, instance: FileInstance) -> Option<FileInfo> {
        let mut archive_instance = instance;
        if let Some(info) = self.local.get_file_info(name) {
            if instance == 0 {
                return Some(info);
            }
            archive_instance = instance - 1;
        }
        self.archives
            .file_size(name, archive_instance)
            .map(|size| FileInfo {
                size,
                timestamp: None,
            })
    }

    /// `FileSystem::getFileListInDirectory(directory, searchName,
    /// searchSubdirectories)`: the merged local + archive listing in
    /// path-key order.
    #[must_use]
    pub fn get_file_list_in_directory(
        &self,
        directory: &str,
        search_name: &str,
        search_subdirectories: bool,
    ) -> FilenameList {
        let mut list = FilenameList::new();
        self.local.get_file_list_in_directory(
            "",
            directory,
            search_name,
            &mut list,
            search_subdirectories,
        );
        self.archives
            .get_file_list_in_directory(directory, search_name, &mut list);
        list
    }
}

/// `AsciiString::endsWithNoCase`, spelled for `str`.
trait EndsWithNoCase {
    fn ends_with_no_case(&self, suffix: &str) -> bool;
}

impl EndsWithNoCase for str {
    fn ends_with_no_case(&self, suffix: &str) -> bool {
        self.len() >= suffix.len() && self[self.len() - suffix.len()..].eq_ignore_ascii_case(suffix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_big(entries: &[(&str, &[u8])]) -> Vec<u8> {
        crate::test_support::build_big(entries)
    }

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rzh-vfs-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn loose_files_shadow_archives_at_instance_zero() {
        let root = scratch("shadow");
        std::fs::create_dir_all(root.join("Data/INI")).unwrap();
        std::fs::write(root.join("Data/INI/Shared.ini"), b"loose").unwrap();

        let mut vfs = Vfs::new(&root);
        vfs.mount_big_memory(
            "Base.big",
            build_big(&[("Data\\INI\\Shared.ini", b"archive")]),
            false,
        )
        .unwrap();

        // instance 0: the loose file wins
        assert_eq!(vfs.read("Data\\INI\\Shared.ini").unwrap(), b"loose");
        // instance 1: the archive behind it
        assert_eq!(
            vfs.read_instance("Data\\INI\\Shared.ini", 1).unwrap(),
            b"archive"
        );
        // instance 2: nothing
        assert!(vfs.read_instance("Data\\INI\\Shared.ini", 2).is_none());

        // archive-only file: instance 0 from the archive
        vfs.mount_big_memory(
            "A.big",
            build_big(&[("Data\\INI\\OnlyArchive.ini", b"archive-only")]),
            false,
        )
        .unwrap();
        assert_eq!(
            vfs.read("Data\\INI\\OnlyArchive.ini").unwrap(),
            b"archive-only"
        );
        assert!(!vfs.does_file_exist("Data\\INI\\OnlyArchive.ini", 1));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn exists_and_info_follow_the_instance_routing() {
        let root = scratch("routing");
        std::fs::create_dir_all(root.join("Data")).unwrap();
        std::fs::write(root.join("Data/File.txt"), b"12345678").unwrap();

        let mut vfs = Vfs::new(&root);
        vfs.mount_big_memory(
            "A.big",
            build_big(&[("Data\\File.txt", b"ab"), ("Data\\Deep\\X.bin", b"xyz")]),
            false,
        )
        .unwrap();

        assert!(vfs.does_file_exist("Data\\File.txt", 0));
        assert!(vfs.does_file_exist("Data\\File.txt", 1)); // behind the loose file
        assert!(!vfs.does_file_exist("Data\\File.txt", 2));
        assert_eq!(vfs.file_info("Data\\File.txt", 0).unwrap().size, 8);
        assert_eq!(vfs.file_info("Data\\File.txt", 1).unwrap().size, 2);
        assert_eq!(vfs.file_info("Data/Deep/X.bin", 0).unwrap().size, 3);
        assert!(vfs.file_info("Data\\Nope.txt", 0).is_none());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn listings_merge_local_and_archives() {
        let root = scratch("merge");
        std::fs::create_dir_all(root.join("Data/INI")).unwrap();
        std::fs::write(root.join("Data/INI/Loose.ini"), b"l").unwrap();

        let mut vfs = Vfs::new(&root);
        vfs.mount_big_memory(
            "A.big",
            build_big(&[("Data\\INI\\Archive.ini", b"a"), ("Art\\M.w3d", b"m")]),
            false,
        )
        .unwrap();

        let list = vfs.get_file_list_in_directory("Data\\INI\\", "*.ini", false);
        // archive listings carry lowercased names (the tree stores them
        // that way); local listings keep the on-disk spelling
        assert_eq!(
            list.into_paths(),
            ["Data\\INI\\archive.ini", "Data\\INI\\Loose.ini"]
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn streaming_opens_read_the_same_bytes() {
        let mut vfs = Vfs::new("no-such-root");
        vfs.mount_big_memory(
            "A.big",
            build_big(&[("Data\\Blob.bin", b"\x00\x01\x02\x03\x04\x05\x06\x07")]),
            false,
        )
        .unwrap();

        let mut stream = vfs.open_streaming("Data\\Blob.bin").unwrap();
        assert_eq!(stream.size(), 8);
        let mut head = [0u8; 4];
        assert_eq!(stream.read(&mut head), 4);
        assert_eq!(&head, b"\x00\x01\x02\x03");
        assert_eq!(stream.read_to_end(), b"\x04\x05\x06\x07");

        // and the RAM open agrees
        assert_eq!(
            vfs.read("Data\\Blob.bin").unwrap(),
            b"\x00\x01\x02\x03\x04\x05\x06\x07"
        );
    }

    #[test]
    fn load_archives_from_directory_mounts_recursively_in_order() {
        let root = scratch("load");
        // two nested archives; the path-key order decides instance 0
        // (host paths use the native separator; the paths INSIDE the
        // archives stay backslashed, like shipped data)
        std::fs::create_dir_all(root.join("Patch")).unwrap();
        std::fs::write(root.join("A.big"), build_big(&[("Data\\X.ini", b"a")])).unwrap();
        std::fs::write(
            root.join("Patch/B.big"),
            build_big(&[("Data\\X.ini", b"b")]),
        )
        .unwrap();
        std::fs::write(
            root.join("Patch/C.big"),
            build_big(&[("Data\\X.ini", b"c")]),
        )
        .unwrap();
        // the duplicate-INIZH skip (path spelled with the Data\INI prefix)
        std::fs::create_dir_all(root.join("Data/INI")).unwrap();
        std::fs::write(
            root.join("Data/INI/INIZH.big"),
            build_big(&[("Data\\X.ini", b"skip")]),
        )
        .unwrap();

        let mut vfs = Vfs::new(&root);
        assert!(vfs.load_archives_from_directory("", "*.big", false));
        // a.big mounted first (path-key order), then patch\b.big, patch\c.big
        assert_eq!(vfs.read("Data\\X.ini").unwrap(), b"a");
        assert_eq!(vfs.read_instance("Data\\X.ini", 1).unwrap(), b"b");
        assert_eq!(vfs.read_instance("Data\\X.ini", 2).unwrap(), b"c");
        assert!(vfs.read_instance("Data\\X.ini", 3).is_none()); // INIZH was skipped

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn mods_overwrite_and_stack() {
        let mut vfs = Vfs::new("no-such-root");
        vfs.mount_big_memory("Base.big", build_big(&[("Data\\X.ini", b"base")]), false)
            .unwrap();
        vfs.load_mods(Some("nope.big"), None);
        vfs.mount_big_memory("Mod.big", build_big(&[("Data\\X.ini", b"mod")]), true)
            .unwrap();
        assert_eq!(vfs.read("Data\\X.ini").unwrap(), b"mod");
        assert_eq!(vfs.read_instance("Data\\X.ini", 1).unwrap(), b"base");
    }
}
