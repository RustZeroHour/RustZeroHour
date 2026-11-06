// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Ported from the Zero Hour source release (baseline 571a7c5):
//   GeneralsMD/Code/GameEngine/Source/Common/System/DataChunk.cpp
//     and GeneralsMD/Code/GameEngine/Include/Common/DataChunk.h
//     (DataChunkTableOfContents, DataChunkInput, DataChunkOutput, the
//     CachedFileInputStream byte-stream semantics and the DataChunkInfo /
//     UserParser registration surface)
//   Core/GameEngine/Include/Common/MapReaderWriterInfo.h
//     (the ChunkInputStream interface CachedFileInputStream implements)
//   Core/GameEngine/Source/Common/Dict.cpp and Include/Common/Dict.h
//     (the load-path half of Dict: findPairByKey's binary search, setPrep's
//     update-or-append and the shellsort that orders pairs by name key)
//   GeneralsMD/Code/GameEngine/Source/Common/NameKeyGenerator.cpp
//     (the name-key interning the dict and name-key primitives drive —
//     first-seen sequential ids, the reverse keyToName scan)
//
// See the module docs for the format layout and the documented differences
// from the C++ (assert-only contracts become errors or documented release
// behaviour; the RefPack decompression that precedes this layer is
// rzh-compress's job).

//! The **map data-chunk container** — the `.map` framing layer.
//!
//! Every chunk a Zero Hour map carries — height-map data, blend tiles,
//! global lighting, world info, object lists, polygon triggers, sides
//! lists — is stored in the format `Common/System/DataChunk.cpp` reads and
//! writes: a `CkMp`-tagged **table of contents** that interns chunk names
//! into small ids, followed by a tree of **data chunks** (a 10-byte header
//! of `u32` id, `u16` version, `i32` data size, then the bytes, which are
//! either primitive values or more chunks). [`DataChunkInput`] walks the
//! tree with a table of registered per-label parsers — the shape the
//! `WorldHeightMap` constructor drives — and [`DataChunkOutput`] builds it
//! the way WorldBuilder's save path does, patching each chunk's size word
//! in place and prepending the table of contents at the end. Bytes written
//! and read here are **identical to the original implementations** (parity
//! level P0, pinned by golden vectors generated from the transcription in
//! `tools/differential/gen_mapchunk_golden.cpp`).
//!
//! This is the map-files counterpart of [`crate::chunk`]: that module is
//! the IFF container the W3D asset family ships in, this one is the
//! container the map family ships in. The payload parsers (the
//! `HeightMapData`/`BlendTileData`/`GlobalLighting`/`ObjectsList`/…
//! readers of `WorldHeightMap.cpp`) are the follow-up slices that build on
//! it, exactly as the W3D mesh/skeleton/animation/HLOD readers built on
//! the chunk container.
//!
//! # The container in one page
//!
//! ```text
//! "CkMp"                      the tag (a wrong tag silently disables parsing)
//! i32   count                 table-of-contents entries that follow
//! { u8 name_len, name, u32 id } × count
//! chunk*                      the chunk tree
//!   u32 id                     table-of-contents id (names are interned)
//!   u16 version                per-chunk version word (never checked)
//!   i32 data_size              byte count of the payload that follows
//!   [u8; data_size]            primitives, or nested chunks, or both
//! ```
//!
//! All integers are little-endian (the original serializes its in-memory
//! structs directly on the 32-bit x86 build). Wide strings are UTF-16LE
//! code units (`WideChar` is 2 bytes on the game's Windows build — a
//! 4-byte `unichar` platform would double them; the wire format is the
//! Windows one).
//!
//! # Reader semantics (transcribed)
//!
//! - The **table of contents** is a prepend list on both sides: entries
//!   read are prepended (so with duplicate names the *last* one in the
//!   file wins lookups), and the writer's table is *written* in reverse
//!   allocation order. A `count > 0` table that ends exactly at EOF does
//!   **not** count as a valid header (`isValidFileType` is false), so
//!   every real map needs at least one chunk after its table.
//! - [`DataChunkInput::open_data_chunk`] reads the 10-byte header with
//!   **zero-filled short reads**, pushes the chunk, and returns the empty
//!   label when the header ended at EOF — while
//!   [`get_chunk_label`](DataChunkInput::get_chunk_label) still resolves
//!   the id through the table. A truncated file therefore leaves a
//!   phantom chunk on the stack.
//! - Reads decrement `data_left` for **every** open chunk on the stack,
//!   not just the innermost; the values are signed and go negative when a
//!   chunk over-declares or a caller reads past its end (the original's
//!   `DEBUG_ASSERTCRASH` is a debug-only contract; the release build
//!   reads on). [`close_data_chunk`](DataChunkInput::close_data_chunk)
//!   seeks forward past unread bytes — the seek target is the `u32` sum
//!   `tell + data_left`, clamped to EOF — and a negative remainder does
//!   not seek at all.
//! - [`parse`](DataChunkInput::parse) captures the enclosing chunk's
//!   label once (the parser scope), then loops: a parent with fewer than
//!   4 bytes left ends the walk, an opened chunk whose header lands at
//!   EOF ends the walk (with the chunk still pushed), an unmatched label
//!   is skipped whole, and the **last registered** parser for a
//!   (label, parent) pair wins — registration prepends. A parser that
//!   leaves a chunk open makes the loop's close pop the *wrong* chunk;
//!   the drift is transcribed (the original's `m_currentObject` comment
//!   describes the intended use of that shape).
//! - Dicts and name keys need a **name-key interning table**
//!   ([`NameKeys`]) supplied by the caller — the original reaches the
//!   global `TheNameKeyGenerator`, which the migration plan's no-globals
//!   rule replaces with an explicit context. Ids are assigned
//!   sequentially in first-seen order, so a dict read reorders its pairs
//!   by interning order, not file order — the goldens pin that with a
//!   pre-seeded table.
//!
//! # Writer semantics (transcribed)
//!
//! [`DataChunkOutput`] mirrors `DataChunkOutput`: chunks open with a
//! `0xffff` size placeholder that
//! [`close_data_chunk`](DataChunkOutput::close_data_chunk) patches in
//! place; dict key names
//! and name keys allocate table ids just like chunk names, interleaved in
//! first-use order; the table is emitted (reverse allocation order)
//! before the data on [`finish`]. The `u16` string lengths and the `u8`
//! table-of-contents name lengths truncate the way the original's
//! narrowing conversions do (a 65 539-byte string writes a length of 3; a
//! 259-character name writes 3 bytes).
//!
//! [`finish`]: DataChunkOutput::finish
//!
//! # Differences from the C++ (documented)
//!
//! - The stream's `open()` transparently RefPack-decompresses retail
//!   `.map` files through `CompressionManager`; this module sees the
//!   **decompressed** bytes. Feeding it is the caller's job
//!   (`rzh-compress::refpack` decodes; `rzh-vfs` reads), exactly where
//!   `CachedFileInputStream` hands the chunk reader its bytes after the
//!   decompress branch.
//! - Short reads leave the tail of the destination **zero** here; the
//!   original leaves uninitialized memory. The choice is pinned on both
//!   sides (the transcription zero-fills too) so truncated-header corpora
//!   stay deterministic.
//! - `readDict`'s unknown-pair-type `throw ERROR_CORRUPT_FILE_FORMAT`
//!   becomes [`MapChunkError::CorruptDictType`]; the chunk it was reading
//!   stays open exactly as the thrown-out-of original leaves it.
//! - A writer finished with chunks still open refuses with
//!   [`MapChunkError::UnclosedChunk`]; the original silently ships the
//!   `0xffff` placeholder (a corrupt file). The same decision the W3D
//!   chunk container made.
//! - `getID`/`getName` misses return `0`/empty (the original's release
//!   behaviour; its debug builds `DEBUG_CRASH`), and the name-key
//!   interning is the load-path transcription of `NameKeyGenerator`
//!   (first-seen sequential ids, exact-match names, reverse lookup) —
//!   `rzh-util::namekey` stays the authoritative full port; this private
//!   copy keeps the crate dependency-free, like the WWLib CRC-32 the mesh
//!   module transcribes.
//! - Parser registration is a [`ParserTable`] handed to
//!   [`parse`](DataChunkInput::parse) instead of a list inside the input:
//!   Rust cannot let a stored callback re-enter its own container, and
//!   the table parameter is what lets a parser recurse the way the
//!   original's `file.parse(userData)` calls do. The matching order,
//!   scope rules and last-registered-wins precedence are transcribed.
//! - `tell`, `first_chunk_pos`, `stack_depth` and the table-of-contents
//!   views are port additions so the golden vectors can pin the stream
//!   bookkeeping (the same role `tell` plays in [`crate::chunk`]).
//! - Positions are `u32` (the original's `UnsignedInt`); the close-seek
//!   sum wraps at 2³² the way the original's unsigned addition does, and
//!   `data_left` uses wrapping `i32` arithmetic where the original's
//!   signed underflow is undefined behaviour.

#![forbid(unsafe_code)]

use std::fmt;

// ---------------------------------------------------------------------------
// constants (DataChunk.h)
// ---------------------------------------------------------------------------

/// The size of the file tag (`"CkMp"` — "Chunky height map", as the
/// original's comment has it).
pub const CHUNK_TAG: [u8; 4] = *b"CkMp";

/// The minimum bytes of `data_left` the parse loop needs before it will
/// open another nested chunk (`DataChunkInput`'s `CHUNK_HEADER_BYTES`;
/// despite the name — "2 shorts in chunk file header" — the value is 4,
/// not a header size; it is the loop's exhaustion threshold).
pub const CHUNK_HEADER_BYTES: i32 = 4;

/// The size of one data-chunk header on disk: a `u32` id, a `u16`
/// version and an `i32` data size.
pub const DATA_CHUNK_HEADER_SIZE: u32 = 10;

// ---------------------------------------------------------------------------
// errors
// ---------------------------------------------------------------------------

/// Errors the map data-chunk layer reports.
///
/// The original has exactly one failure mode on the read side —
/// `readDict` throwing `ERROR_CORRUPT_FILE_FORMAT` on an impossible pair
/// type — and silently ships corrupt files on the write side; the port
/// keeps the one and refuses the other (see the module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapChunkError {
    /// `readDict` met a pair whose type byte is not one of the five
    /// `Dict::DataType` values. The original throws
    /// `ERROR_CORRUPT_FILE_FORMAT`; the chunk being read stays open on
    /// the stack in both.
    CorruptDictType(u8),
    /// The writer was finished with chunks still open. The original would
    /// silently emit their `0xffff` size placeholders (a corrupt file);
    /// the port refuses.
    UnclosedChunk,
}

impl fmt::Display for MapChunkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MapChunkError::CorruptDictType(t) => write!(
                f,
                "dict pair type {t:#04x} is not one of the five Dict data types"
            ),
            MapChunkError::UnclosedChunk => {
                write!(f, "data chunks still open when the writer finished")
            }
        }
    }
}

impl std::error::Error for MapChunkError {}

// ---------------------------------------------------------------------------
// name keys (NameKeyGenerator.cpp, load-path transcription)
// ---------------------------------------------------------------------------

/// A name-key id (`NameKeyType` in the original).
///
/// The values are assigned sequentially from 1 in first-seen order by a
/// [`NameKeys`] table; the dict layer sorts its pairs on them, so their
/// assignment order is the dict's serialization order. This is the
/// load-path transcription the map container needs (kept private to the
/// crate per the dependency-free policy); `rzh-util::namekey` is the
/// authoritative full port.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NameKey(u32);

impl NameKey {
    /// The value the original's `NAMEKEY_INVALID` never names; the
    /// interning table starts its ids at 1.
    pub const INVALID: NameKey = NameKey(0);

    /// The ordinal the dict layer sorts and serializes on.
    #[must_use]
    pub const fn ordinal(self) -> u32 {
        self.0
    }
}

/// The name-key interning table the dict and name-key primitives share.
///
/// The original is the `TheNameKeyGenerator` singleton; per the migration
/// plan's no-global-state rule this is an explicit value the caller owns
/// and passes to [`DataChunkInput::read_dict`] /
/// [`DataChunkOutput::write_dict`]. Interning order must stay
/// deterministic across lockstep clients — ids are sequence numbers, and
/// the dict layer serializes them.
///
/// The original hashes names into 45 007 sockets and allocates ids in
/// first-seen order; the socket table only affects lookup cost, so the
/// arena below keeps the observable behaviour (sequential first-seen ids,
/// exact-match case-sensitive lookup, reverse scan on miss returning the
/// empty name).
#[derive(Debug, Clone, Default)]
pub struct NameKeys {
    names: Vec<(Vec<u8>, NameKey)>,
    next_id: u32,
}

impl NameKeys {
    /// A fresh table; the first interned name gets ordinal 1.
    #[must_use]
    pub fn new() -> Self {
        NameKeys {
            names: Vec::new(),
            next_id: 1,
        }
    }

    /// `nameToKey`: the id for `name`, interning it (at the next
    /// sequential ordinal) if this is its first appearance.
    pub fn name_to_key(&mut self, name: impl AsRef<[u8]>) -> NameKey {
        let name = name.as_ref();
        if let Some(&(_, key)) = self.names.iter().find(|(n, _)| n == name) {
            return key;
        }
        let key = NameKey(self.next_id);
        self.next_id = self.next_id.wrapping_add(1);
        self.names.push((name.to_vec(), key));
        key
    }

    /// `keyToName`: the name `key` was interned under, or the empty name
    /// (the original's release behaviour; its debug build crashes).
    #[must_use]
    pub fn key_to_name(&self, key: NameKey) -> &[u8] {
        for (name, k) in &self.names {
            if *k == key {
                return name;
            }
        }
        &[]
    }

    /// The interned names with their ordinals, in first-seen order (a
    /// port addition for the golden replay — the same role the
    /// table-of-contents views play).
    pub fn names(&self) -> impl ExactSizeIterator<Item = (u32, &[u8])> {
        self.names.iter().map(|(n, k)| (k.0, n.as_slice()))
    }
}

// ---------------------------------------------------------------------------
// the dict (Dict.cpp, load-path half)
// ---------------------------------------------------------------------------

/// The `Dict::DataType` values, in wire order (the low byte of a pair's
/// `keyAndType` word).
pub mod dict_type {
    /// `DICT_BOOL` — the original notes the code relies on it being 0.
    pub const DICT_BOOL: u8 = 0;
    /// `DICT_INT`.
    pub const DICT_INT: u8 = 1;
    /// `DICT_REAL`.
    pub const DICT_REAL: u8 = 2;
    /// `DICT_ASCIISTRING`.
    pub const DICT_ASCIISTRING: u8 = 3;
    /// `DICT_UNICODESTRING`.
    pub const DICT_UNICODESTRING: u8 = 4;
}

/// One dict value (`Dict`'s five data types).
#[derive(Debug, Clone, PartialEq)]
pub enum DictValue {
    /// `DICT_BOOL` — one byte on the wire, `0`/`1`.
    Bool(bool),
    /// `DICT_INT` — four bytes.
    Int(i32),
    /// `DICT_REAL` — four bytes.
    Real(f32),
    /// `DICT_ASCIISTRING` — a `u16` length and the bytes up to the first
    /// NUL (the original's C-string semantics).
    AsciiString(Vec<u8>),
    /// `DICT_UNICODESTRING` — a `u16` unit count and UTF-16LE units up
    /// to the first zero unit.
    UnicodeString(Vec<u16>),
}

/// A dictionary of name-key → value pairs, kept **sorted by name key**
/// (the original's shellsort invariant); the serialization order is the
/// interning order, not the insertion order.
///
/// This is the load-path half of `Common/Dict.cpp` — `findPairByKey`,
/// `setPrep`, the shellsort and the nth accessors the chunk layer drives.
/// The lookup/removal/copy surface arrives with the typed data layer
/// (`rzh-data`, migration plan §3.4).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Dict {
    pairs: Vec<(NameKey, DictValue)>,
}

impl Dict {
    /// An empty dict.
    #[must_use]
    pub fn new() -> Self {
        Dict { pairs: Vec::new() }
    }

    /// The pair count (`getPairCount`).
    #[must_use]
    pub fn pair_count(&self) -> usize {
        self.pairs.len()
    }

    /// `getNthKey` — the name key of the nth pair, in stored order.
    #[must_use]
    pub fn nth_key(&self, n: usize) -> NameKey {
        self.pairs[n].0
    }

    /// `getNthType` — the wire type byte of the nth pair.
    #[must_use]
    pub fn nth_type(&self, n: usize) -> u8 {
        match self.pairs[n].1 {
            DictValue::Bool(_) => dict_type::DICT_BOOL,
            DictValue::Int(_) => dict_type::DICT_INT,
            DictValue::Real(_) => dict_type::DICT_REAL,
            DictValue::AsciiString(_) => dict_type::DICT_ASCIISTRING,
            DictValue::UnicodeString(_) => dict_type::DICT_UNICODESTRING,
        }
    }

    /// `getNthBool` and friends — the nth pair's value.
    #[must_use]
    pub fn nth_value(&self, n: usize) -> &DictValue {
        &self.pairs[n].1
    }

    /// `setBool`/`setInt`/… — insert or update the pair for `key`, then
    /// re-sort (an update changes the type and value in place, exactly as
    /// `setPrep` + `setNameAndType` do).
    pub fn set(&mut self, key: NameKey, value: DictValue) {
        match self.find_pair_by_key(key) {
            Some(idx) => self.pairs[idx].1 = value,
            None => self.pairs.push((key, value)),
        }
        self.sort_pairs();
    }

    /// `findPairByKey` — the binary search over the sorted pairs.
    fn find_pair_by_key(&self, key: NameKey) -> Option<usize> {
        let mut min_idx = 0usize;
        let mut max_idx = self.pairs.len();
        while min_idx < max_idx {
            let mid_idx = (min_idx + max_idx - 1) >> 1;
            let mid_key = self.pairs[mid_idx].0;
            if key > mid_key {
                min_idx = mid_idx + 1;
            } else if key < mid_key {
                max_idx = mid_idx;
            } else {
                return Some(mid_idx);
            }
        }
        None
    }

    /// `sortPairs` — the original's shellsort, comparison on the name key
    /// alone (the type never participates).
    fn sort_pairs(&mut self) {
        let n = self.pairs.len() as isize;
        let mut gap = n >> 1;
        while gap > 0 {
            let mut i = gap;
            while i < n {
                let mut j = i - gap;
                while j >= 0 {
                    let a = j as usize;
                    let b = (j + gap) as usize;
                    if self.pairs[a].0 > self.pairs[b].0 {
                        self.pairs.swap(a, b);
                    } else {
                        break;
                    }
                    j -= gap;
                }
                i += 1;
            }
            gap >>= 1;
        }
    }
}

// ---------------------------------------------------------------------------
// the byte stream (CachedFileInputStream)
// ---------------------------------------------------------------------------

/// The in-memory byte stream `DataChunkInput` reads through — the read /
/// tell / absolute-seek / eof semantics of `CachedFileInputStream` over
/// the **decompressed** map bytes (its `open()` RefPack branch lives in
/// `rzh-compress`; see the module docs).
#[derive(Debug, Clone)]
struct Stream<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Stream<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Stream { bytes, pos: 0 }
    }

    /// `read`: clamp the request to the available bytes (zero when the
    /// buffer is empty), copy what is there, and advance.
    #[inline]
    fn read(&mut self, out: &mut [u8]) -> usize {
        let available = self.bytes.len().saturating_sub(self.pos);
        let n = out.len().min(available);
        out[..n].copy_from_slice(&self.bytes[self.pos..self.pos + n]);
        self.pos += n;
        n
    }

    /// `tell`.
    fn tell(&self) -> u32 {
        self.pos as u32
    }

    /// `absoluteSeek`: clamps to the end of the buffer (the original's
    /// `pos > m_size` clamp; its `pos < 0` check is dead code — the
    /// parameter is unsigned).
    fn absolute_seek(&mut self, pos: u32) -> bool {
        self.pos = (pos as usize).min(self.bytes.len());
        true
    }

    /// `eof`: true when the position is at the end.
    fn eof(&self) -> bool {
        self.pos == self.bytes.len()
    }
}

// ---------------------------------------------------------------------------
// the table of contents (DataChunkTableOfContents)
// ---------------------------------------------------------------------------

/// One table-of-contents mapping (`Mapping`): a chunk name and the id the
/// file's chunk headers refer to it by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TocEntry {
    /// The interned name (the bytes up to the first NUL of what the file
    /// stored).
    pub name: Vec<u8>,
    /// The id chunk headers carry.
    pub id: u32,
}

/// The `CkMp` table of contents: the name ↔ id mapping every chunk header
/// resolves through, on both the read and the write side.
///
/// The list is prepend-ordered like the original's (`allocateID` and the
/// reader both prepend), so with duplicate names the *most recently*
/// added entry wins lookups; [`entries`](Self::entries) exposes the list
/// head-first, which is also the order the writer emits.
#[derive(Debug, Clone, Default)]
pub struct TableOfContents {
    /// Head-first is the *last* element (prepend order).
    list: Vec<TocEntry>,
    next_id: u32,
    header_opened: bool,
}

impl TableOfContents {
    fn new() -> Self {
        TableOfContents {
            list: Vec::new(),
            next_id: 1,
            header_opened: false,
        }
    }

    /// `findMapping`: the first head-first match.
    fn find_mapping(&self, name: &[u8]) -> Option<&TocEntry> {
        self.list.iter().rev().find(|m| m.name == name)
    }

    /// `getID`: the id for `name`, or `0` on a miss (the original's
    /// release behaviour; its debug builds `DEBUG_CRASH`).
    #[must_use]
    pub fn get_id(&self, name: &[u8]) -> u32 {
        self.find_mapping(name).map_or(0, |m| m.id)
    }

    /// `getName`: the name for `id`, or empty on a miss.
    #[must_use]
    pub fn get_name(&self, id: u32) -> &[u8] {
        for m in self.list.iter().rev() {
            if m.id == id {
                return &m.name;
            }
        }
        &[]
    }

    /// `allocateID`: the existing id for `name`, or the next sequential
    /// one (prepended).
    fn allocate_id(&mut self, name: &[u8]) -> u32 {
        if let Some(m) = self.find_mapping(name) {
            return m.id;
        }
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.list.push(TocEntry {
            name: name.to_vec(),
            id,
        });
        id
    }

    /// `isOpenedForRead`.
    fn is_opened_for_read(&self) -> bool {
        self.header_opened
    }

    /// `write`: the tag, the count, then the entries head-first. The
    /// name length is the `u8` narrowing of the original — names longer
    /// than 255 characters are truncated on disk.
    fn write(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&CHUNK_TAG);
        out.extend_from_slice(&(self.list.len() as i32).to_le_bytes());
        for m in self.list.iter().rev() {
            out.push(m.name.len() as u8);
            out.extend_from_slice(&m.name[..(m.name.len() as u8 as usize)]);
            out.extend_from_slice(&m.id.to_le_bytes());
        }
    }

    /// `read`: the tag (mismatching consumes the tag bytes and silently
    /// leaves the table unopened — "may happen with legacy files"), the
    /// count, then the entries (prepended, so the file's last duplicate
    /// name wins). A `count > 0` table that ends at EOF does not count as
    /// opened.
    fn read(&mut self, s: &mut Stream<'_>) {
        // the tag buffer's tail keeps its 'x' fill on a short read
        let mut tag = [b'x'; 4];
        s.read(&mut tag);
        if tag != CHUNK_TAG {
            return; // Don't throw, may happen with legacy files.
        }

        let mut count_buf = [0u8; 4];
        s.read(&mut count_buf);
        let count = i32::from_le_bytes(count_buf);

        let mut max_id = 0u32;
        for _ in 0..count {
            let mut len_buf = [0u8; 1];
            s.read(&mut len_buf);
            let len = len_buf[0] as usize;

            let mut name = vec![0u8; len];
            if len > 0 {
                s.read(&mut name);
                let visible = name.iter().position(|&b| b == 0).unwrap_or(len);
                name.truncate(visible);
            }

            let mut id_buf = [0u8; 4];
            s.read(&mut id_buf);
            let id = u32::from_le_bytes(id_buf);

            self.list.push(TocEntry { name, id });
            if id > max_id {
                max_id = id;
            }
        }
        self.header_opened = count > 0 && !s.eof();

        // adjust next ID so no ID's are reused
        self.next_id = self.next_id.max(max_id.wrapping_add(1));
    }

    /// The entries, head-first (the writer's emission order).
    pub fn entries(&self) -> impl DoubleEndedIterator<Item = &TocEntry> {
        self.list.iter().rev()
    }

    /// The entry count (`m_listLength`).
    #[must_use]
    pub fn len(&self) -> usize {
        self.list.len()
    }

    /// Whether the table is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }
}

// ---------------------------------------------------------------------------
// the input (DataChunkInput)
// ---------------------------------------------------------------------------

/// One open input chunk (`InputChunk`): the id, the version, the declared
/// data size and the signed bytes-left bookkeeping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct InputChunk {
    id: u32,
    version: u16,
    /// `chunkStart` — recorded by the original, never queried (kept for
    /// the struct's fidelity; the Debug derive is its only reader).
    chunk_start: u32,
    data_size: i32,
    data_left: i32,
}

/// The chunk info handed to a registered parser (`DataChunkInfo`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataChunkInfo {
    /// The chunk's label (its table-of-contents name).
    pub label: Vec<u8>,
    /// The enclosing chunk's label at `parse` entry (the scope), empty at
    /// the top level.
    pub parent_label: Vec<u8>,
    /// The chunk's version word.
    pub version: u16,
    /// The chunk's declared data size (signed; the negative-declared
    /// corpora pin that it stays negative).
    pub data_size: i32,
}

/// A registered parser: called for chunks whose label and parent scope
/// match its registration (`DataChunkParserPtr` + the `userData` the
/// original passes through `parse`).
///
/// The table parameter lets a parser recurse — the original calls
/// `file.parse(userData)` inside its container parsers, which Rust cannot
/// express through a callback stored in the same object.
pub type ParserFn<'t, U> =
    fn(&mut DataChunkInput<'_>, &DataChunkInfo, &mut ParserTable<'t, U>, &mut U) -> bool;

/// The registered parsers (`UserParser` list). Registration prepends, so
/// the **last** registered parser for a (label, parent) pair wins —
/// transcribed exactly.
#[derive(Debug, Clone)]
pub struct ParserTable<'t, U: ?Sized> {
    entries: Vec<(Vec<u8>, Vec<u8>, ParserFn<'t, U>)>,
}

impl<'t, U: ?Sized> ParserTable<'t, U> {
    /// An empty table.
    #[must_use]
    pub fn new() -> Self {
        ParserTable {
            entries: Vec::new(),
        }
    }

    /// `registerParser`: match chunks labelled `label` whose enclosing
    /// chunk at `parse` entry is labelled `parent_label` (empty for the
    /// global scope).
    pub fn register(
        &mut self,
        label: impl Into<Vec<u8>>,
        parent_label: impl Into<Vec<u8>>,
        parser: ParserFn<'t, U>,
    ) {
        self.entries
            .push((label.into(), parent_label.into(), parser));
    }
}

impl<'t, U: ?Sized> Default for ParserTable<'t, U> {
    fn default() -> Self {
        Self::new()
    }
}

/// The map data-chunk reader (`DataChunkInput` over a
/// `CachedFileInputStream` of the decompressed map bytes).
#[derive(Debug, Clone)]
pub struct DataChunkInput<'a> {
    stream: Stream<'a>,
    toc: TableOfContents,
    first_chunk_pos: u32,
    stack: Vec<InputChunk>,
}

impl<'a> DataChunkInput<'a> {
    /// Constructs the input over `bytes`: reads the table of contents and
    /// records where the chunk tree starts (four bytes in, whatever the
    /// tag said — the tag read always consumes).
    #[must_use]
    pub fn new(bytes: &'a [u8]) -> Self {
        let mut stream = Stream::new(bytes);
        let mut toc = TableOfContents::new();
        toc.read(&mut stream);
        let first_chunk_pos = stream.tell();
        DataChunkInput {
            stream,
            toc,
            first_chunk_pos,
            stack: Vec::new(),
        }
    }

    /// `isValidFileType`: whether the header counted as a `CkMp` table of
    /// contents (a `count > 0` table not ending at EOF).
    #[must_use]
    pub fn is_valid_file_type(&self) -> bool {
        self.toc.is_opened_for_read()
    }

    /// The table of contents the header loaded.
    #[must_use]
    pub fn toc(&self) -> &TableOfContents {
        &self.toc
    }

    /// `parse`: walk the chunk stream with the registered parsers,
    /// returning false when the header never opened or a parser returns
    /// false.
    ///
    /// The scope is the enclosing chunk's label at entry; see the module
    /// docs for the loop's transcribed quirks (the 4-byte exhaustion
    /// break, the EOF-header phantom chunk, skipped unmatched labels,
    /// last-registered-wins, and the close that pops whatever the parser
    /// left on top).
    pub fn parse<'t, U: ?Sized>(&mut self, table: &mut ParserTable<'t, U>, user: &mut U) -> bool {
        // If the header wasn't a chunk table of contents, we can't parse.
        if !self.toc.is_opened_for_read() {
            return false;
        }

        // if we are inside a data chunk right now, get its name
        let parent_label = if let Some(top) = self.stack.last() {
            self.toc.get_name(top.id).to_vec()
        } else {
            Vec::new()
        };

        while !self.at_end_of_file() {
            if let Some(top) = self.stack.last() {
                // If we are parsing chunks in a chunk, check current length.
                if top.data_left < CHUNK_HEADER_BYTES {
                    // (the original's DEBUG_ASSERTCRASH that dataLeft be
                    // exactly 0 here is a debug-only contract; the release
                    // build breaks out)
                    break;
                }
            }
            // open the chunk
            let label = self.open_data_chunk();
            if self.at_end_of_file() {
                // FILE * returns eof after you read past end of file, so check.
                break;
            }

            // find a registered parser for this chunk
            for idx in (0..table.entries.len()).rev() {
                let matches = {
                    let (label_, parent, _) = &table.entries[idx];
                    *label_ == label && *parent == parent_label
                };
                if matches {
                    let info = DataChunkInfo {
                        label,
                        parent_label: parent_label.clone(),
                        version: self.get_chunk_version(),
                        data_size: self.get_chunk_data_size() as i32,
                    };
                    let parser = table.entries[idx].2;
                    if !parser(self, &info, table, user) {
                        return false;
                    }
                    break;
                }
            }

            // close chunk (and skip to end if need be)
            self.close_data_chunk();
        }

        true
    }

    /// `openDataChunk`: reads the 10-byte header (zero-filled short
    /// reads), pushes the chunk, and returns its label — empty when the
    /// header ended at EOF (while [`get_chunk_label`](Self::get_chunk_label)
    /// still resolves the id).
    pub fn open_data_chunk(&mut self) -> Vec<u8> {
        let mut chunk = InputChunk {
            id: 0,
            version: 0,
            chunk_start: 0,
            data_size: 0,
            data_left: 0,
        };

        // read the chunk ID
        let mut buf4 = [0u8; 4];
        self.stream.read(&mut buf4);
        chunk.id = u32::from_le_bytes(buf4);
        self.decrement_data_left(4);

        // read the chunk version number
        let mut buf2 = [0u8; 2];
        self.stream.read(&mut buf2);
        chunk.version = u16::from_le_bytes(buf2);
        self.decrement_data_left(2);

        // read the chunk data size
        let mut buf4 = [0u8; 4];
        self.stream.read(&mut buf4);
        chunk.data_size = i32::from_le_bytes(buf4);
        self.decrement_data_left(4);

        // all of the data remains to be read
        chunk.data_left = chunk.data_size;
        chunk.chunk_start = self.stream.tell();

        self.stack.push(chunk);
        if self.at_end_of_file() {
            return Vec::new();
        }
        self.toc.get_name(chunk.id).to_vec()
    }

    /// `closeDataChunk`: seeks past any unread bytes (the `u32`
    /// `tell + data_left` target, clamped to EOF) and pops; a negative
    /// remainder does not seek. Closing with nothing open is the
    /// original's silent no-op.
    pub fn close_data_chunk(&mut self) {
        let Some(top) = self.stack.last() else {
            // TODO: Throw exception (the original's comment)
            return;
        };
        let data_left = top.data_left;
        if data_left > 0 {
            // skip past the remainder of this chunk
            let target = self.stream.tell().wrapping_add(data_left as u32);
            self.stream.absolute_seek(target);
            self.decrement_data_left(data_left);
        }
        self.stack.pop();
    }

    /// `getChunkLabel`: the current chunk's name (empty with nothing
    /// open).
    #[must_use]
    pub fn get_chunk_label(&self) -> &[u8] {
        match self.stack.last() {
            Some(top) => self.toc.get_name(top.id),
            None => &[],
        }
    }

    /// `getChunkVersion`.
    #[must_use]
    pub fn get_chunk_version(&self) -> u16 {
        match self.stack.last() {
            Some(top) => top.version,
            None => 0,
        }
    }

    /// `getChunkDataSize`: the declared size, as the original's unsigned
    /// view of it (the golden replay reads the signed bits back with
    /// `as i32`).
    #[must_use]
    pub fn get_chunk_data_size(&self) -> u32 {
        match self.stack.last() {
            Some(top) => top.data_size as u32,
            None => 0,
        }
    }

    /// `getChunkDataSizeLeft`.
    #[must_use]
    pub fn get_chunk_data_size_left(&self) -> u32 {
        match self.stack.last() {
            Some(top) => top.data_left as u32,
            None => 0,
        }
    }

    /// `atEndOfFile`.
    #[must_use]
    pub fn at_end_of_file(&self) -> bool {
        self.stream.eof()
    }

    /// `atEndOfChunk` (true with nothing open).
    #[must_use]
    pub fn at_end_of_chunk(&self) -> bool {
        match self.stack.last() {
            Some(top) => top.data_left <= 0,
            None => true,
        }
    }

    /// `reset`: back to the just-opened state, ready to parse the first
    /// chunk again.
    pub fn reset(&mut self) {
        self.stack.clear();
        self.stream.absolute_seek(self.first_chunk_pos);
    }

    /// `readReal`.
    #[inline]
    pub fn read_real(&mut self) -> f32 {
        let mut buf = [0u8; 4];
        self.stream.read(&mut buf);
        self.decrement_data_left(4);
        f32::from_le_bytes(buf)
    }

    /// `readInt`.
    #[inline]
    pub fn read_int(&mut self) -> i32 {
        let mut buf = [0u8; 4];
        self.stream.read(&mut buf);
        self.decrement_data_left(4);
        i32::from_le_bytes(buf)
    }

    /// `readByte`.
    #[inline]
    pub fn read_byte(&mut self) -> u8 {
        let mut buf = [0u8; 1];
        self.stream.read(&mut buf);
        self.decrement_data_left(1);
        buf[0]
    }

    /// `readArrayOfBytes`: fills `out` (a short read leaves the tail
    /// zero) and decrements by the **requested** length, not the bytes
    /// actually seen.
    pub fn read_array_of_bytes(&mut self, out: &mut [u8]) {
        self.stream.read(out);
        self.decrement_data_left(out.len() as i32);
    }

    /// `readNameKey`: the `keyAndType` word, the id out of its high
    /// bytes, the name through the table, the ordinal through `keys`.
    pub fn read_name_key(&mut self, keys: &mut NameKeys) -> NameKey {
        let mut key_and_type = self.read_int();
        key_and_type >>= 8;
        let name = self.toc.get_name(key_and_type as u32).to_vec();
        keys.name_to_key(name)
    }

    /// `readDict`: the `u16` pair count, then each pair's `keyAndType`
    /// word, name-key interning and typed value. An impossible type byte
    /// is [`MapChunkError::CorruptDictType`] where the original throws
    /// `ERROR_CORRUPT_FILE_FORMAT` (the chunk stays open either way).
    pub fn read_dict(&mut self, keys: &mut NameKeys) -> Result<Dict, MapChunkError> {
        let mut buf = [0u8; 2];
        self.stream.read(&mut buf);
        let len = u16::from_le_bytes(buf);
        self.decrement_data_left(2);

        let mut dict = Dict::new();
        for _ in 0..len {
            let mut key_and_type = self.read_int();
            let t = (key_and_type & 0xff) as u8;
            key_and_type >>= 8;

            let name = self.toc.get_name(key_and_type as u32).to_vec();
            let key = keys.name_to_key(name);

            match t {
                dict_type::DICT_BOOL => dict.set(key, DictValue::Bool(self.read_byte() != 0)),
                dict_type::DICT_INT => dict.set(key, DictValue::Int(self.read_int())),
                dict_type::DICT_REAL => dict.set(key, DictValue::Real(self.read_real())),
                dict_type::DICT_ASCIISTRING => {
                    dict.set(key, DictValue::AsciiString(self.read_ascii_string()));
                }
                dict_type::DICT_UNICODESTRING => {
                    dict.set(key, DictValue::UnicodeString(self.read_unicode_string()));
                }
                _ => return Err(MapChunkError::CorruptDictType(t)),
            }
        }
        Ok(dict)
    }

    /// `readAsciiString`: a `u16` byte length, the bytes (visible content
    /// up to the first NUL; all `len` bytes consumed regardless).
    pub fn read_ascii_string(&mut self) -> Vec<u8> {
        let mut buf = [0u8; 2];
        self.stream.read(&mut buf);
        let len = u16::from_le_bytes(buf) as usize;
        self.decrement_data_left(2);
        if len > 0 {
            let mut bytes = vec![0u8; len];
            self.stream.read(&mut bytes);
            self.decrement_data_left(len as i32);
            let visible = bytes.iter().position(|&b| b == 0).unwrap_or(len);
            bytes.truncate(visible);
            bytes
        } else {
            Vec::new()
        }
    }

    /// `readUnicodeString`: a `u16` **unit** count, `count × 2` bytes of
    /// UTF-16LE (visible units up to the first zero; all consumed).
    pub fn read_unicode_string(&mut self) -> Vec<u16> {
        let mut buf = [0u8; 2];
        self.stream.read(&mut buf);
        let len = u16::from_le_bytes(buf) as usize;
        self.decrement_data_left(2);
        if len > 0 {
            let mut bytes = vec![0u8; len * 2];
            self.stream.read(&mut bytes);
            self.decrement_data_left((len * 2) as i32);
            let mut units: Vec<u16> = bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|p| u16::from_le_bytes(*p))
                .collect();
            let visible = units.iter().position(|&u| u == 0).unwrap_or(len);
            units.truncate(visible);
            units
        } else {
            Vec::new()
        }
    }

    /// `decrementDataLeft`: every open chunk, wrapping like the
    /// original's unsigned arithmetic (its signed underflow is UB; the
    /// port defines it).
    #[inline]
    fn decrement_data_left(&mut self, size: i32) {
        for chunk in &mut self.stack {
            chunk.data_left = chunk.data_left.wrapping_sub(size);
        }
    }

    // --- port additions for the golden replay (see the module docs) ---

    /// The stream position (`Tell` through the cached stream; a port
    /// addition, the same role [`crate::chunk::ChunkLoad::tell`] plays).
    #[must_use]
    pub fn tell(&self) -> u32 {
        self.stream.tell()
    }

    /// Where the chunk tree starts — the position `reset` seeks to (a
    /// port addition; the tag read always consumed four bytes, whatever
    /// the tag said).
    #[must_use]
    pub fn first_chunk_pos(&self) -> u32 {
        self.first_chunk_pos
    }

    /// How many chunks are open (a port addition for the golden replay).
    #[must_use]
    pub fn stack_depth(&self) -> usize {
        self.stack.len()
    }
}

// ---------------------------------------------------------------------------
// the output (DataChunkOutput)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct OutputChunk {
    /// The chunk's table id — the original stores it for its VERBOSE
    /// logging; the port's only reader is the Debug derive.
    #[expect(dead_code, reason = "kept for the struct's fidelity to the original")]
    id: u32,
    filepos: usize,
}

/// The map data-chunk writer (`DataChunkOutput` writing through its
/// temp-file patching into an output stream).
///
/// Chunks open with a `0xffff` size placeholder and close by patching the
/// real size in place; the table of contents (chunk names, dict key
/// names, name-key names — interleaved in first-use order, emitted in
/// reverse) is prepended by [`finish`](Self::finish), which is the
/// original's destructor.
#[derive(Debug, Clone)]
pub struct DataChunkOutput {
    toc: TableOfContents,
    tmp: Vec<u8>,
    stack: Vec<OutputChunk>,
    out: Vec<u8>,
    finished: bool,
    max_depth: usize,
}

impl Default for DataChunkOutput {
    fn default() -> Self {
        Self::new()
    }
}

impl DataChunkOutput {
    /// A fresh writer over its own byte sink.
    #[must_use]
    pub fn new() -> Self {
        DataChunkOutput {
            toc: TableOfContents::new(),
            tmp: Vec::new(),
            stack: Vec::new(),
            out: Vec::new(),
            finished: false,
            max_depth: 0,
        }
    }

    /// `openDataChunk`: allocates (or reuses) the name's table id, writes
    /// the header with its `0xffff` size placeholder, and pushes the
    /// chunk. The name's visible extent stops at its first NUL byte (the
    /// original passes a `const char*`).
    pub fn open_data_chunk(&mut self, name: impl AsRef<[u8]>, version: u16) {
        let name = name.as_ref();
        let visible = name.iter().position(|&b| b == 0).unwrap_or(name.len());
        let id = self.toc.allocate_id(&name[..visible]);

        let filepos = self.tmp.len() + 6; // after the id and version words
        self.tmp.extend_from_slice(&id.to_le_bytes());
        self.tmp.extend_from_slice(&version.to_le_bytes());
        self.stack.push(OutputChunk { id, filepos });
        self.max_depth = self.max_depth.max(self.stack.len());

        // store a placeholder for the data size
        let dummy: i32 = 0xffff;
        self.tmp.extend_from_slice(&dummy.to_le_bytes());
    }

    /// `closeDataChunk`: patches the real size over the placeholder and
    /// pops. Closing with nothing open is the original's silent no-op.
    pub fn close_data_chunk(&mut self) {
        let Some(top) = self.stack.pop() else {
            // TODO: Throw exception (the original's comment)
            return;
        };

        // compute data size (not including the actual data size itself)
        let size = (self.tmp.len() - top.filepos - 4) as i32;
        self.tmp[top.filepos..top.filepos + 4].copy_from_slice(&size.to_le_bytes());
    }

    /// `writeReal`.
    pub fn write_real(&mut self, r: f32) {
        self.tmp.extend_from_slice(&r.to_le_bytes());
    }

    /// `writeInt`.
    pub fn write_int(&mut self, i: i32) {
        self.tmp.extend_from_slice(&i.to_le_bytes());
    }

    /// `writeByte`.
    pub fn write_byte(&mut self, b: u8) {
        self.tmp.push(b);
    }

    /// `writeArrayOfBytes`.
    pub fn write_array_of_bytes(&mut self, bytes: &[u8]) {
        self.tmp.extend_from_slice(bytes);
    }

    /// `writeAsciiString`: a `u16` length (the original's narrowing
    /// conversion — 65 536-plus lengths wrap) and the bytes up to the
    /// first NUL.
    pub fn write_ascii_string(&mut self, s: impl AsRef<[u8]>) {
        let s = s.as_ref();
        let visible = s.iter().position(|&b| b == 0).unwrap_or(s.len());
        let len = visible as u16;
        self.tmp.extend_from_slice(&len.to_le_bytes());
        self.tmp.extend_from_slice(&s[..len as usize]);
    }

    /// `writeUnicodeString`: a `u16` **unit** count (wrapping like the
    /// ascii length) and the units up to the first zero, UTF-16LE.
    pub fn write_unicode_string(&mut self, units: &[u16]) {
        let visible = units.iter().position(|&u| u == 0).unwrap_or(units.len());
        let len = visible as u16;
        self.tmp.extend_from_slice(&len.to_le_bytes());
        for unit in &units[..len as usize] {
            self.tmp.extend_from_slice(&unit.to_le_bytes());
        }
    }

    /// `writeNameKey`: the key's name through `keys` (the empty name for
    /// an unknown key, which allocates a table entry for ""), typed
    /// `DICT_ASCIISTRING`, as one `keyAndType` word.
    pub fn write_name_key(&mut self, keys: &NameKeys, key: NameKey) {
        let kname = keys.key_to_name(key).to_vec();
        let mut key_and_type = self.toc.allocate_id(&kname);
        key_and_type <<= 8;
        key_and_type |= dict_type::DICT_ASCIISTRING as u32;
        self.write_int(key_and_type as i32);
    }

    /// `writeDict`: the `u16` pair count (wrapping) and the pairs in
    /// stored order — the interning order, which is the dict's sort, not
    /// the caller's insertion order.
    pub fn write_dict(&mut self, keys: &NameKeys, dict: &Dict) {
        let len = dict.pair_count() as u16;
        self.tmp.extend_from_slice(&len.to_le_bytes());
        for i in 0..len as usize {
            let k = dict.nth_key(i);
            let kname = keys.key_to_name(k).to_vec();

            let mut key_and_type = self.toc.allocate_id(&kname);
            key_and_type <<= 8;
            key_and_type |= dict.nth_type(i) as u32;
            self.write_int(key_and_type as i32);

            match dict.nth_value(i) {
                DictValue::Bool(b) => self.write_byte(u8::from(*b)),
                DictValue::Int(v) => self.write_int(*v),
                DictValue::Real(v) => self.write_real(*v),
                DictValue::AsciiString(s) => self.write_ascii_string(s),
                DictValue::UnicodeString(u) => self.write_unicode_string(u),
            }
        }
    }

    /// The writer's destructor: emits the table of contents, then the
    /// chunk tree. Refuses with [`MapChunkError::UnclosedChunk`] when
    /// chunks are still open (the original silently ships their
    /// placeholders); idempotent, so the finished bytes stay readable.
    pub fn finish(&mut self) -> Result<(), MapChunkError> {
        if self.finished {
            return Ok(());
        }
        if !self.stack.is_empty() {
            return Err(MapChunkError::UnclosedChunk);
        }
        self.finished = true;
        self.toc.write(&mut self.out);
        self.out.extend_from_slice(&self.tmp);
        Ok(())
    }

    /// The finished bytes (empty before [`finish`](Self::finish)).
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.out
    }

    /// Consumes the writer and returns the finished bytes.
    pub fn into_bytes(mut self) -> Result<Vec<u8>, MapChunkError> {
        self.finish()?;
        Ok(std::mem::take(&mut self.out))
    }

    /// The table of contents (allocation order is observable through
    /// [`TableOfContents::entries`] before or after finishing).
    #[must_use]
    pub fn toc(&self) -> &TableOfContents {
        &self.toc
    }

    /// How many chunks are open.
    #[must_use]
    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    /// The deepest stack the writer reached (a port addition for the
    /// golden replay).
    #[must_use]
    pub fn max_depth(&self) -> usize {
        self.max_depth
    }
}
