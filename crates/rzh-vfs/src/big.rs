// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Ported from (baseline 571a7c5, Zero Hour configuration):
//
//   Core/GameEngine/Source/Common/System/BIGFileSystem.cpp (BIGF header,
//     entry table parse, openArchiveFile)
//   Core/GameEngine/Source/Common/System/ArchiveFile.cpp (per-archive
//     directory tree, getArchivedFileInfo, getFileListInDirectory)
//   Core/GameEngine/Source/Common/System/StreamingArchiveFile.cpp
//     (windowed reads over the open archive)
//
// The originals carry the notices: (c) 2001-2003 Electronic Arts Inc.,
// Westwood Studios Pacific.
//
//! The BIG archive format (`ArchiveFile` + `BIGFileSystem` parsing).
//!
//! A BIG archive starts with the 16-byte header
//!
//! ```text
//! struct BIGFileHeader {
//!     char     identifier[4];     // "BIGF"
//!     uint32_t archiveFileSize;   // little endian (read, never checked)
//!     uint32_t numFiles;          // big endian
//!     uint32_t firstEntryOffset;  // big endian (read, never used: the
//! }                                // entry table always starts at 16)
//! ```
//!
//! followed by `numFiles` directory entries, each
//!
//! ```text
//! struct BIGFileEntryHeader {
//!     uint32_t fileOffset;        // big endian, into the archive
//!     uint32_t fileSize;          // big endian
//!     char     path[];            // NUL-terminated, '\' or '/' separated
//! }                                // (max _MAX_PATH-1 bytes)
//! ```
//!
//! Sub-file payloads start at `fileOffset` and are exactly `fileSize`
//! bytes; they may be RefPack-compressed with the 8-byte `EAR\0` wrapper
//! (see [`crate::compression`]) — decompression happens at the consumer,
//! exactly like the C++ `CachedFileInputStream`.
//!
//! # Quirks preserved (they are behaviour)
//!
//! - The entry table is read **sequentially from offset 16**;
//!   `firstEntryOffset` is ignored (every shipped archive has it at 16).
//! - Entry paths are split into directory part + lowercased leaf name;
//!   listings expose only the lowercased form (original case is lost).
//! - Two entries with the same directory + name: the **last one wins**
//!   (`dirInfo->m_files[name] = *fileInfo` assigns through).
//! - The archive file stays **open** and is seeked on every sub-file
//!   read (the port keeps a backing handle for exactly that reason).
//! - A name longer than `_MAX_PATH - 1` (259) bytes marks the archive
//!   corrupt, like the C's fixed `char buffer[_MAX_PATH]`.
//! - Sub-files are read with the archive's position MUTATED (the C++
//!   seeks its one shared handle; sequential opens behave identically).
//!
//! # Differences from the C++ (documented)
//!
//! - The C++ reads the entry table via buffered `File::read` calls and
//!   crashes (or worse) on truncated input; the port returns
//!   [`Err(VfsError::CorruptArchive)`] for every short read and for
//!   entry names that are not UTF-8 (shipped data is ASCII).
//! - `closeArchiveFile` (the Music.big hot-swap) is not offered: the C++
//!   leaves the overlay tree pointing at the deleted `ArchiveFile`
//!   (a use-after-free). The unmount surface will be redesigned when
//!   the audio flow that needs it is ported.
//!
//! # Parity level
//!
//! **P0 exact**: golden vectors in `tests/golden/vfs.txt` pin the parse,
//! the listings and the payload bytes of synthetic archives (including
//! RefPack-compressed payloads produced by the real EAC encoder).

use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex};

use crate::VfsError;
use crate::path::{FilenameList, is_path_separator};
use crate::wildcard::search_string_matches;
/// The `BIGF` magic (`BIGFileIdentifier`).
pub const BIG_MAGIC: [u8; 4] = *b"BIGF";

/// `MUSIC_BIG`: the archive the C++ engine is allowed to close at
/// runtime (kept for documentation; see the module notes).
pub const MUSIC_BIG: &str = "Music.big";

/// `readNullTerminatedString`'s buffer: `char buffer[_MAX_PATH]`, so an
/// entry name may be at most 259 bytes plus the terminator.
const MAX_ENTRY_NAME: usize = crate::path::MAX_PATH;

/// `ArchivedFileInfo`: one sub-file's directory entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchivedFileInfo {
    /// The lowercased leaf name (`m_filename`).
    pub filename: String,
    /// The lowercased path of the archive the entry came from
    /// (`m_archiveFilename`).
    pub archive_filename: String,
    /// The entry's offset in the archive (`m_offset`).
    pub offset: u32,
    /// The entry's stored (possibly still compressed) size (`m_size`).
    pub size: u32,
}

/// The still-open backing of a mounted archive: the C++ keeps one
/// `File*` open per `.big`; sub-file reads seek it.
#[derive(Clone)]
pub(crate) enum Backing {
    /// An archive held in memory (tests, RAM-mounted archives).
    Memory(Arc<Vec<u8>>),
    /// An archive open from disk.
    File(Arc<Mutex<std::fs::File>>),
}

impl Backing {
    /// `File::seek` + `File::read` on the shared handle: reads `buf.len()`
    /// bytes at `offset`, returning the short count at end of data.
    pub(crate) fn read_at(&self, offset: u64, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Backing::Memory(data) => {
                let start = usize::try_from(offset).unwrap_or(usize::MAX);
                if start >= data.len() {
                    return Ok(0);
                }
                let n = buf.len().min(data.len() - start);
                buf[..n].copy_from_slice(&data[start..start + n]);
                Ok(n)
            }
            Backing::File(file) => {
                let mut file = file.lock().expect("archive backing poisoned");
                file.seek(SeekFrom::Start(offset))?;
                read_up_to(&mut *file, buf)
            }
        }
    }
}

impl std::fmt::Debug for Backing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Memory(data) => f
                .debug_struct("Backing::Memory")
                .field("len", &data.len())
                .finish(),
            Self::File(_) => f.write_str("Backing::File"),
        }
    }
}

/// `File::read` semantics: fill as much as possible, short count at EOF.
fn read_up_to(reader: &mut impl Read, buf: &mut [u8]) -> std::io::Result<usize> {
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

/// A directory node of the per-archive tree (`DetailedArchivedDirectoryInfo`).
#[derive(Clone, Debug, Default)]
struct DirNode {
    /// `m_directories`: contained directories by lowercased name (the
    /// C++ `std::map` iteration order is the load order of listings —
    /// kept as `BTreeMap`'s byte order of the lowercased names).
    directories: BTreeMap<String, DirNode>,
    /// `m_files`: contained files by lowercased leaf name; a later
    /// entry with the same name overwrites (`operator[]` assignment).
    files: BTreeMap<String, ArchivedFileInfo>,
}

/// One mounted BIG archive: its still-open backing plus the parsed
/// directory tree (`ArchiveFile`).
#[derive(Clone)]
pub struct BigArchive {
    /// The archive path/name exactly as it was opened (`m_name`;
    /// original case, the overlay's map key).
    pub(crate) name: String,
    /// The lowercased archive path (`m_archiveFilename` of every entry).
    archive_filename: String,
    root: DirNode,
    backing: Backing,
    /// The archive file's size, like the C++ `FileInfo` captured at
    /// `attachFile` (the C++ also keeps its timestamp; timestamps are
    /// machine state, not data, and are not reproduced).
    archive_size: u64,
}

impl std::fmt::Debug for BigArchive {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BigArchive")
            .field("name", &self.name)
            .field("archive_filename", &self.archive_filename)
            .field("archive_size", &self.archive_size)
            .finish_non_exhaustive()
    }
}

impl BigArchive {
    /// `BIGFileSystem::openArchiveFile` over in-memory bytes (tests and
    /// RAM-mounted archives). `name` is the archive path exactly as the
    /// overlay should know it.
    pub fn open_memory(name: &str, data: Vec<u8>) -> Result<Self, VfsError> {
        let len = data.len() as u64;
        let backing = Backing::Memory(Arc::new(data));
        Self::parse(name, backing, len)
    }

    /// `BIGFileSystem::openArchiveFile` over a `.big` file on disk.
    ///
    /// The engine passes one string for both opening the file and naming
    /// the archive; the port's local layer resolves a root-relative game
    /// path to a disk path, so the two are passed separately here.
    pub fn open_file_as(name: &str, path: &std::path::Path) -> Result<Self, VfsError> {
        let file = std::fs::File::open(path).map_err(|e| VfsError::Io {
            name: name.to_string(),
            source: e,
        })?;
        let len = file
            .metadata()
            .map_err(|e| VfsError::Io {
                name: name.to_string(),
                source: e,
            })?
            .len();
        let backing = Backing::File(Arc::new(Mutex::new(file)));
        Self::parse(name, backing, len)
    }

    /// The shared entry-point of the two constructors: reads the header
    /// and the whole entry table, then builds the tree
    /// (`BIGFileSystem::openArchiveFile`'s body).
    fn parse(name: &str, backing: Backing, backing_len: u64) -> Result<Self, VfsError> {
        let archive_filename = name.to_ascii_lowercase();

        // fp->read(&header, sizeof(header)) != sizeof(header) || bad magic
        let mut header = [0u8; 16];
        if backing.read_at(0, &mut header).map_err(|e| VfsError::Io {
            name: name.to_string(),
            source: e,
        })? != 16
        {
            return Err(VfsError::CorruptArchive {
                name: name.to_string(),
                reason: "short BIG header".to_string(),
            });
        }
        if header[..4] != BIG_MAGIC {
            return Err(VfsError::CorruptArchive {
                name: name.to_string(),
                reason: "bad BIG magic".to_string(),
            });
        }

        // archiveFileSize is little endian and read but never validated
        let _archive_file_size = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
        let num_little_files = u32::from_be_bytes([header[8], header[9], header[10], header[11]]);
        let _first_entry_offset =
            u32::from_be_bytes([header[12], header[13], header[14], header[15]]);

        let mut archive = Self {
            name: name.to_string(),
            archive_filename,
            root: DirNode::default(),
            backing,
            archive_size: backing_len,
        };

        // read in each directory listing; entries live right after the
        // 16-byte header (the header's firstEntryOffset is not used)
        let mut at = 16u64;
        for _ in 0..num_little_files {
            let mut entry = [0u8; 8];
            if archive
                .backing
                .read_at(at, &mut entry)
                .map_err(|e| VfsError::Io {
                    name: name.to_string(),
                    source: e,
                })?
                != 8
            {
                return Err(VfsError::CorruptArchive {
                    name: name.to_string(),
                    reason: "truncated directory entry header".to_string(),
                });
            }
            at += 8;

            let file_offset = u32::from_be_bytes([entry[0], entry[1], entry[2], entry[3]]);
            let file_size = u32::from_be_bytes([entry[4], entry[5], entry[6], entry[7]]);

            // readNullTerminatedString(fp, buffer, sizeof(buffer))
            let (path_name, consumed) =
                read_null_terminated_string(&archive.backing, at).map_err(|reason| {
                    VfsError::CorruptArchive {
                        name: name.to_string(),
                        reason,
                    }
                })?;
            at += consumed;

            // char *leafName = getFileName(buffer);
            let leaf = get_leaf_name(&path_name);
            let file_info = ArchivedFileInfo {
                filename: leaf.to_ascii_lowercase(),
                archive_filename: archive.archive_filename.clone(),
                offset: file_offset,
                size: file_size,
            };

            // *leafName = 0: the directory part is the path up to the leaf
            let dir_part = &path_name[..path_name.len() - leaf.len()];
            archive.add_file(dir_part, &file_info);
        }

        Ok(archive)
    }

    /// `ArchiveFile::addFile`: walk (creating) the directory nodes for the
    /// lowercased path components, then file the entry under its
    /// lowercased leaf name. The C++ loops on the token (`while
    /// (!token.isEmpty())`), which visits exactly the non-empty
    /// components.
    fn add_file(&mut self, path: &str, file_info: &ArchivedFileInfo) {
        let mut dir = &mut self.root;
        let lowered = path.to_ascii_lowercase();
        let mut tokenizer = PathTokenizer::new(&lowered);
        while let Some(component) = tokenizer.next_token() {
            dir = dir.directories.entry(component).or_default();
        }
        dir.files
            .insert(file_info.filename.clone(), file_info.clone());
    }

    /// `ArchiveFile::getArchivedFileInfo`: the entry for a full
    /// (mixed-case, mixed-separator) path, or `None`.
    ///
    /// Transcribed with the original's tokenizer loop (condition on the
    /// *remainder*, not the token): a query with a trailing separator
    /// descends into its last component as a directory and then misses,
    /// exactly like the C++.
    #[must_use]
    pub fn archived_file_info(&self, filename: &str) -> Option<&ArchivedFileInfo> {
        let mut dir = &self.root;
        let lowered = filename.to_ascii_lowercase();
        let mut tokenizer = PathTokenizer::new(&lowered);
        let mut token = tokenizer.next_token();
        while !tokenizer.is_empty() {
            let component = token?;
            dir = dir.directories.get(&component)?;
            token = tokenizer.next_token();
        }
        match token {
            Some(leaf) => dir.files.get(&leaf),
            None => None,
        }
    }

    /// `ArchiveFile::getFileListInDirectory` (the archive-private
    /// listing, used by the overlay and by searches):
    /// every stored path whose leaf matches `search_name`, rooted at
    /// `original_directory`, de-duplicated in `out`. The
    /// `search_subdirectories` flag is accepted for signature parity but
    /// ignored — the C++ recursion always descends the whole tree.
    pub fn get_file_list_in_directory(
        &self,
        original_directory: &str,
        search_name: &str,
        out: &mut FilenameList,
    ) {
        // walk to the search directory, looping on the token (nothing to
        // list when a component is missing)
        let mut dir = &self.root;
        let lowered = original_directory.to_ascii_lowercase();
        let mut tokenizer = PathTokenizer::new(&lowered);
        let mut token = tokenizer.next_token();
        while let Some(component) = token {
            dir = match dir.directories.get(&component) {
                Some(next) => next,
                None => return,
            };
            token = tokenizer.next_token();
        }
        self.list_recursive(dir, original_directory, search_name, out);
    }

    /// The private recursion of the C++ (directories first, then the
    /// matching files, in the tree maps' iteration order).
    fn list_recursive(
        &self,
        dir: &DirNode,
        current_directory: &str,
        search_name: &str,
        out: &mut FilenameList,
    ) {
        for (name, sub) in &dir.directories {
            let sub_dir = join_listing_path(current_directory, name);
            self.list_recursive(sub, &sub_dir, search_name, out);
        }
        for (name, info) in &dir.files {
            if search_string_matches(&info.filename, search_name) {
                let path = join_listing_path(current_directory, name);
                out.insert(&path);
            }
        }
    }

    /// `ArchiveFile::openFile` (read path): reads the entry's bytes from
    /// the open backing in one go — the `RAMFile::openFromArchive` half.
    /// Streaming opens go through [`BigArchive::open_streaming`].
    pub fn read_entry(&self, info: &ArchivedFileInfo) -> Result<Vec<u8>, VfsError> {
        let mut data = vec![0u8; info.size as usize];
        let n = self
            .backing
            .read_at(u64::from(info.offset), &mut data)
            .map_err(|e| VfsError::Io {
                name: self.name.clone(),
                source: e,
            })?;
        if n != info.size as usize {
            // RAMFile::openFromArchive fails when the read comes up short
            return Err(VfsError::CorruptArchive {
                name: self.name.clone(),
                reason: format!(
                    "entry '{}' wants {} bytes at {} but only {} are there",
                    info.filename, info.size, info.offset, n
                ),
            });
        }
        Ok(data)
    }

    /// `StreamingArchiveFile::openFromArchive`: a windowed read view of
    /// `[offset, offset+size)` over the open backing. The C++ probes the
    /// window with two seeks first; the port checks the same thing
    /// directly against the backing length.
    pub fn open_streaming(&self, info: &ArchivedFileInfo) -> Result<StreamingFile, VfsError> {
        let start = u64::from(info.offset);
        let end = start + u64::from(info.size);
        if end > self.archive_size {
            // the C++ probe: seek(offset) then seek(size) must land exactly
            // at offset+size; reads past the archive would fail later
            return Err(VfsError::CorruptArchive {
                name: self.name.clone(),
                reason: format!(
                    "entry '{}' spans [{start}, {end}) but the archive is {} bytes",
                    info.filename, self.archive_size
                ),
            });
        }
        Ok(StreamingFile {
            backing: self.backing.clone(),
            start,
            size: u64::from(info.size),
            pos: 0,
        })
    }

    /// `ArchiveFile::getName`.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// `ArchiveFile::getPath` (always the empty string in the engine's
    /// BIG flow; kept for API parity).
    #[must_use]
    pub fn path(&self) -> &str {
        ""
    }

    /// `ArchiveFile::getFileInfo`: the entry's size. The C++ also copies
    /// the *archive file's* timestamp; the port reports sizes only (see
    /// the module notes).
    #[must_use]
    pub fn file_info(&self, filename: &str) -> Option<u64> {
        self.archived_file_info(filename)
            .map(|info| u64::from(info.size))
    }

    /// The number of stored entries (across the whole tree) — used by the
    /// crate's own tests and tools.
    #[must_use]
    pub fn entry_count(&self) -> usize {
        fn count(dir: &DirNode) -> usize {
            dir.files.len() + dir.directories.values().map(count).sum::<usize>()
        }
        count(&self.root)
    }
}

/// `readNullTerminatedString(fp, buffer, sizeof(buffer))`: reads one byte
/// at a time, at most `MAX_ENTRY_NAME` bytes; returns the string and the
/// number of bytes consumed (including the terminator). Errors carry the
/// C's two failure reasons as text.
fn read_null_terminated_string(backing: &Backing, at: u64) -> Result<(String, u64), String> {
    let mut bytes = Vec::new();
    for i in 0..MAX_ENTRY_NAME {
        let mut one = [0u8; 1];
        let n = backing
            .read_at(at + u64::try_from(i).unwrap_or(u64::MAX), &mut one)
            .map_err(|e| format!("read error in entry name: {e}"))?;
        if n != 1 {
            // the read came up short: -1 in the C
            return Err("truncated entry name".to_string());
        }
        if one[0] == 0 {
            let text = String::from_utf8(bytes.clone())
                .map_err(|_| "entry name is not UTF-8".to_string())?;
            return Ok((text, u64::try_from(i + 1).unwrap_or(u64::MAX)));
        }
        bytes.push(one[0]);
    }
    // no terminator within the buffer: -1 in the C
    Err("entry name longer than _MAX_PATH-1".to_string())
}

/// `getFileName(path)`: the component after the last separator (the
/// whole path when there is none), on the byte level like the C.
fn get_leaf_name(path: &str) -> &str {
    let bytes = path.as_bytes();
    match bytes.iter().rposition(|&c| is_path_separator(c)) {
        Some(at) => &path[at + 1..],
        None => path,
    }
}

/// `AsciiString::nextToken(&tok, "\\/")` as a state machine: yields the
/// non-empty path components; when only separators remain, the remainder
/// is cleared (the C clears both the string and the token). The input is
/// already lowercased by the caller, like every C++ call site.
pub(crate) struct PathTokenizer<'a> {
    rest: &'a str,
}

impl<'a> PathTokenizer<'a> {
    pub(crate) fn new(path: &'a str) -> Self {
        Self { rest: path }
    }

    /// `while (!tokenizer.isEmpty())` in the C++.
    pub(crate) fn is_empty(&self) -> bool {
        self.rest.is_empty()
    }

    /// The next component; `None` when the remainder is empty or
    /// separators only.
    pub(crate) fn next_token(&mut self) -> Option<String> {
        if self.rest.is_empty() {
            return None;
        }
        let bytes = self.rest.as_bytes();
        let mut i = 0;
        while i < bytes.len() && is_path_separator(bytes[i]) {
            i += 1; // skipSeps
        }
        let start = i;
        while i < bytes.len() && !is_path_separator(bytes[i]) {
            i += 1; // skipNonSeps
        }
        if i == start {
            self.rest = "";
            return None;
        }
        let token = self.rest[start..i].to_owned();
        self.rest = &self.rest[i..];
        Some(token)
    }
}

/// The C++ builds listing paths as `currentDirectory` `'\'` `name`,
/// inserting the separator only when the current directory is nonempty
/// and does not already end with one.
fn join_listing_path(current_directory: &str, name: &str) -> String {
    let needs_sep = !current_directory.is_empty()
        && !current_directory
            .as_bytes()
            .last()
            .is_some_and(|&c| is_path_separator(c));
    let mut out = String::with_capacity(current_directory.len() + 1 + name.len());
    out.push_str(current_directory);
    if needs_sep {
        out.push('\\');
    }
    out.push_str(name);
    out
}

/// `StreamingArchiveFile`: a read window over the still-open archive
/// backing, positioned independently of it.
#[derive(Debug)]
pub struct StreamingFile {
    backing: Backing,
    start: u64,
    size: u64,
    pos: u64,
}

impl StreamingFile {
    /// `StreamingArchiveFile::read(buffer, bytes)`: seeks the backing to
    /// `start + pos` and reads up to `out.len()` bytes, clamped to the
    /// window. Returns the byte count.
    pub fn read(&mut self, out: &mut [u8]) -> usize {
        let left = self.size.saturating_sub(self.pos);
        let want = (out.len() as u64).min(left) as usize;
        if want == 0 {
            return 0;
        }
        let n = self
            .backing
            .read_at(self.start + self.pos, &mut out[..want])
            .unwrap_or(0);
        self.pos += n as u64;
        n
    }

    /// `StreamingArchiveFile::seek`: clamps to `[0, size]` and returns
    /// the new position.
    pub fn seek(&mut self, pos: i64, mode: crate::ramfile::SeekMode) -> u64 {
        let new_pos = match mode {
            crate::ramfile::SeekMode::Start => pos,
            crate::ramfile::SeekMode::Current => i64::try_from(self.pos).unwrap_or(i64::MAX) + pos,
            crate::ramfile::SeekMode::End => i64::try_from(self.size).unwrap_or(i64::MAX) + pos,
        };
        let clamped = new_pos.clamp(0, i64::try_from(self.size).unwrap_or(i64::MAX));
        self.pos = u64::try_from(clamped).unwrap_or(0);
        self.pos
    }

    /// The window size.
    #[must_use]
    pub fn size(&self) -> u64 {
        self.size
    }

    /// The current position in the window.
    #[must_use]
    pub fn pos(&self) -> u64 {
        self.pos
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shared BIG image builder (see `crate::test_support`).
    fn build_big(entries: &[(&str, &[u8])]) -> Vec<u8> {
        crate::test_support::build_big(entries)
    }

    #[test]
    fn parses_a_minimal_archive() {
        let big = build_big(&[
            ("Data\\INI\\Weapon.ini", b"weapon-data"),
            ("data\\ini\\armor.INI", b"armor-data"),
            ("Music\\TrackA.wav", b"wav-bytes"),
            ("rootfile.txt", b"root"),
        ]);
        let archive = BigArchive::open_memory("Sample.big", big).unwrap();
        assert_eq!(archive.name(), "Sample.big");
        assert_eq!(archive.entry_count(), 4);

        // lookups are case-insensitive with either separator
        for query in [
            "Data\\INI\\Weapon.ini",
            "data/ini/weapon.ini",
            "DATA\\ini\\WEAPON.ini",
        ] {
            assert_eq!(
                archive.archived_file_info(query).unwrap().filename,
                "weapon.ini"
            );
        }
        assert_eq!(
            archive
                .archived_file_info("data/ini/armor.ini")
                .unwrap()
                .archive_filename,
            "sample.big"
        );
        assert!(archive.archived_file_info("data/ini/nope.ini").is_none());
        assert!(archive.archived_file_info("ini/weapon.ini").is_none());
    }

    #[test]
    fn reads_entries_and_streams_them() {
        let big = build_big(&[("Art\\Model.w3d", b"\x01\x02\x03\x04\x05")]);
        let archive = BigArchive::open_memory("A.big", big).unwrap();
        let info = archive.archived_file_info("art/model.w3d").unwrap().clone();
        assert_eq!(archive.read_entry(&info).unwrap(), b"\x01\x02\x03\x04\x05");

        let mut stream = archive.open_streaming(&info).unwrap();
        let mut out = [0u8; 3];
        assert_eq!(stream.read(&mut out), 3);
        assert_eq!(&out, b"\x01\x02\x03");
        assert_eq!(stream.seek(0, crate::ramfile::SeekMode::Start), 0);
        let mut all = [0u8; 16];
        assert_eq!(stream.read(&mut all), 5);
        assert_eq!(&all[..5], b"\x01\x02\x03\x04\x05");
    }

    #[test]
    fn listings_are_lowercase_and_deduplicated() {
        let big = build_big(&[
            ("Data\\INI\\Weapon.ini", b"w"),
            ("Data\\INI\\Armor.INI", b"a"),
            ("Data\\Audio\\Sound.wav", b"s"),
            ("Root.txt", b"r"),
        ]);
        let archive = BigArchive::open_memory("A.big", big).unwrap();
        let mut list = FilenameList::new();
        archive.get_file_list_in_directory("", "*", &mut list);
        assert_eq!(
            list.into_paths(),
            [
                "data\\audio\\sound.wav",
                "data\\ini\\armor.ini",
                "data\\ini\\weapon.ini",
                "root.txt"
            ]
        );

        let mut ini = FilenameList::new();
        archive.get_file_list_in_directory("data\\ini\\", "*.ini", &mut ini);
        assert_eq!(
            ini.into_paths(),
            ["data\\ini\\armor.ini", "data\\ini\\weapon.ini"]
        );

        // a missing directory lists nothing (and does not panic)
        let mut none = FilenameList::new();
        archive.get_file_list_in_directory("nope\\", "*", &mut none);
        assert!(none.is_empty());
    }

    #[test]
    fn duplicate_names_keep_the_last_entry() {
        let big = build_big(&[("Data\\File.txt", b"first"), ("Data\\File.txt", b"second")]);
        let archive = BigArchive::open_memory("A.big", big).unwrap();
        assert_eq!(archive.entry_count(), 1);
        let info = archive.archived_file_info("data/file.txt").unwrap();
        assert_eq!(archive.read_entry(info).unwrap(), b"second");
    }

    #[test]
    fn corrupt_archives_are_errors_not_crashes() {
        // bad magic
        let mut bad = build_big(&[("a.txt", b"x")]);
        bad[0] = b'X';
        assert!(matches!(
            BigArchive::open_memory("A.big", bad),
            Err(VfsError::CorruptArchive { .. })
        ));

        // truncated header
        let short = build_big(&[("a.txt", b"x")]);
        assert!(matches!(
            BigArchive::open_memory("A.big", short[..10].to_vec()),
            Err(VfsError::CorruptArchive { .. })
        ));

        // entry count promises more entries than the table holds
        let mut hollow = build_big(&[]);
        hollow[8..12].copy_from_slice(&3u32.to_be_bytes());
        assert!(matches!(
            BigArchive::open_memory("A.big", hollow),
            Err(VfsError::CorruptArchive { .. })
        ));

        // a name with no terminator inside _MAX_PATH
        let mut unterminated = Vec::new();
        unterminated.extend_from_slice(&BIG_MAGIC);
        unterminated.extend_from_slice(&100u32.to_le_bytes());
        unterminated.extend_from_slice(&1u32.to_be_bytes());
        unterminated.extend_from_slice(&16u32.to_be_bytes());
        unterminated.extend_from_slice(&0u32.to_be_bytes());
        unterminated.extend_from_slice(&1u32.to_be_bytes());
        unterminated.extend(std::iter::repeat_n(b'x', MAX_ENTRY_NAME));
        assert!(matches!(
            BigArchive::open_memory("A.big", unterminated),
            Err(VfsError::CorruptArchive { .. })
        ));
    }

    #[test]
    fn entries_past_the_archive_end_fail_the_read() {
        // hand-build an archive whose entry points past the end
        let mut big = Vec::new();
        big.extend_from_slice(&BIG_MAGIC);
        big.extend_from_slice(&40u32.to_le_bytes());
        big.extend_from_slice(&1u32.to_be_bytes());
        big.extend_from_slice(&16u32.to_be_bytes());
        big.extend_from_slice(&1000u32.to_be_bytes()); // offset way out
        big.extend_from_slice(&4u32.to_be_bytes()); // size
        big.extend_from_slice(b"a.txt\0");
        let archive = BigArchive::open_memory("A.big", big).unwrap();
        let info = archive.archived_file_info("a.txt").unwrap().clone();
        assert!(archive.read_entry(&info).is_err());
        assert!(archive.open_streaming(&info).is_err());
    }
}
