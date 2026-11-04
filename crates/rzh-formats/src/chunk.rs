// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Ported from the Zero Hour source release (baseline 571a7c5):
//   Core/Libraries/Source/WWVegas/WWLib/chunkio.{h,cpp}  (ChunkSaveClass,
//     ChunkLoadClass, ChunkHeader, MicroChunkHeader, and the IOVector
//     read/write helpers of iostruct.h)
// See the module docs for the format layout and the documented differences
// from the C++ (assert-only contracts become errors; out-of-bounds stack
// accesses at depth boundaries become errors).

//! The **W3D chunk container** — the IFF-style framing layer.
//!
//! Every W3D asset the game loads — meshes, skeletons, animations, HLOD
//! assemblies, textures, particles — is a stream of chunks: an 8-byte
//! header (a `u32` type and a `u32` size) followed by the chunk's bytes,
//! which are either raw data or more chunks. [`WW3D2`] loaders walk the
//! tree with `ChunkLoadClass`; the exporter and the save paths build it
//! with `ChunkSaveClass`. This module ports both classes so that
//! byte streams written and read here are **identical to the original
//! implementations** (parity level P0, pinned by golden vectors generated
//! from the transcription in `tools/differential/gen_chunk_golden.cpp`).
//! It is the container the upcoming W3D model/animation/HLOD parsers
//! (migration plan §4, Phase 2) are built on.
//!
//! [`WW3D2`]: https://github.com/RustZeroHour/RustZeroHour
//!
//! # Format layout (as implemented here)
//!
//! All integers are little-endian (the original serializes its in-memory
//! structs directly on x86). A chunk header is
//!
//! ```text
//! u32 chunk_type   arbitrary id (the W3D chunk ids are defined per format)
//! u32 chunk_size   payload byte count; the MSB is NOT part of the count:
//!                  0x8000_0000 set → the chunk contains sub-chunks
//! ```
//!
//! so a chunk's effective payload size is `chunk_size & 0x7FFF_FFFF` and
//! the writer's [`ChunkSave::begin_chunk`] sets the parent's MSB the
//! moment a child begins. A **micro chunk** is a 2-byte header (a `u8`
//! type, a `u8` size ≤ 255) used to store single variables inside a data
//! chunk without paying for a full header; micro chunks cannot nest and
//! are plain data to the chunk framing (they never set the MSB).
//!
//! # Reader semantics (transcribed)
//!
//! [`ChunkLoad`] mirrors `ChunkLoadClass` method for method:
//!
//! - [`open_chunk`](ChunkLoad::open_chunk) reads the next 8-byte header —
//!   **clamped only by the innermost open chunk's declared size** (the
//!   equality check `position == size` ends a parent's child list; it is
//!   *not* `>=`, so an over-declared child makes the reader keep parsing
//!   headers past the parent's boundary — pinned by the goldens). At the
//!   top level it simply reads until the file runs out.
//! - [`read`](ChunkLoad::read) refuses (returns `Ok(0)`) any read that
//!   would cross the current chunk's declared end or the current micro
//!   chunk's end, and returns `Ok(0)` — after consuming the available
//!   bytes at the file level — when the file itself is short (the
//!   original's `File->Read(...) != nbytes` branch). Reads never consult
//!   the contains-chunks flag: data can be read from a container chunk.
//! - [`close_chunk`](ChunkLoad::close_chunk) seeks the underlying stream
//!   forward past whatever the caller left unread (a legal seek past
//!   EOF) and advances the parent's position by the child's *declared*
//!   size + 8.
//! - [`close_micro_chunk`](ChunkLoad::close_micro_chunk) does the same
//!   inside the parent chunk, unclamped by the parent's bounds.
//!
//! # Writer semantics (transcribed)
//!
//! [`ChunkSave`] mirrors `ChunkSaveClass`: [`begin_chunk`] writes a
//! temporary zero-size header and remembers its position;
//! [`end_chunk`](ChunkSave::end_chunk) seeks back, patches the real size,
//! and adds `size + 8` to the parent's accumulator. Data and sub-chunks
//! cannot mix inside one chunk (the original asserts; this port returns
//! [`ChunkError::DataAfterSubChunks`]). Micro chunk sizes are `u8`s: the
//! original's debug assert rejects a total of 255 (an off-by-one — its
//! release build happily stores 255 and wraps at 256); this port accepts
//! ≤ 255 and refuses the wrap with [`ChunkError::MicroChunkTooLarge`].
//!
//! [`begin_chunk`]: ChunkSave::begin_chunk
//!
//! # Differences from the C++ (documented)
//!
//! - The original's `assert`s are debug-only contracts; its release
//!   builds read and write past the stack arrays instead
//!   (`HeaderStack[StackIndex-1]` at depth 0, the 257th pushed writer
//!   frame, the 256th opened reader frame). This port returns
//!   [`ChunkError`] in all those states —
//!   [`ChunkError::NoOpenChunk`], [`ChunkError::NestedMicroChunk`],
//!   [`ChunkError::TooDeep`] — so no input and no call sequence
//!   can corrupt memory. The reader's depth cap follows the original's
//!   assert (255 open chunks); the writer's follows its array bound
//!   (256) — the original could write trees its own debug reader
//!   refuses, and the port transcribes that asymmetry.
//! - `ChunkSaveClass::Write` at depth 0 evaluates its first assert on
//!   `HeaderStack[-1]` (an out-of-bounds read that happens to precede
//!   the `StackIndex > 0` assert). The port checks the depth first; the
//!   observable behaviour for valid states is identical.
//! - Micro chunk writes accumulating to exactly 255 bytes abort the
//!   original's debug build (the off-by-one assert) and succeed in its
//!   release build; the port succeeds (the release behaviour) and
//!   refuses only the wrap at 256. Golden corpora stay within the
//!   debug-legal 254 so the transcription can pin them.
//! - A `ChunkSave` with chunks still open would leave the original's
//!   temporary zero-size headers unpatched (a silently corrupt file);
//!   [`ChunkSave::into_bytes`] refuses with [`ChunkError::UnclosedChunk`]
//!   instead.
//! - Positions are tracked in `u64`; the original's `int` position
//!   accounting can overflow for chunk sizes near `0x7FFF_FFFF` (its
//!   `PositionStack[...] += csize + sizeof(header)` is signed-overflow
//!   UB). The goldens pin the 1 GiB-range jumps that stay clear of the
//!   overflow; beyond that the port saturates where the original is UB.
//! - The IOVector helpers are transcribed with their bug: the original's
//!   `Read(IOVector2Struct*)` overloads pass `sizeof(v)` — the size of
//!   the **pointer**, 4 bytes on the game's 32-bit build — so
//!   [`read_io_vector2`](ChunkLoad::read_io_vector2) and friends read
//!   exactly 4 bytes (filling only the first component) while the write
//!   side (a reference, `sizeof` = the struct) writes the full 8/12/16
//!   bytes. No caller in the game tree uses the read helpers (checked:
//!   `IOVector3Struct` only aliases `W3dVectorStruct` in `w3d_file.h`);
//!   they are ported as found so the future W3D data modules inherit
//!   the documented quirk rather than rediscovering it.
//! - [`tell`](ChunkLoad::tell) is a port addition (the original queries
//!   `File->Tell()` directly); it exists so the golden vectors can pin
//!   the stream bookkeeping, including the consumes-bytes-then-returns-0
//!   short-read quirk.

#![forbid(unsafe_code)]

use std::fmt;

// ---------------------------------------------------------------------------
// Format constants (chunkio.h)
// ---------------------------------------------------------------------------

/// Size of a chunk header on disk: two little-endian `u32`s.
pub const CHUNK_HEADER_SIZE: u32 = 8;
/// Size of a micro-chunk header on disk: two `u8`s.
pub const MICRO_CHUNK_HEADER_SIZE: u32 = 2;
/// Mask separating a chunk's payload size from its contains-chunks flag.
pub const CHUNK_SIZE_MASK: u32 = 0x7FFF_FFFF;
/// The MSB of a chunk's size word: set when the chunk contains sub-chunks.
pub const CHUNK_SUB_CHUNK_FLAG: u32 = 0x8000_0000;
/// Maximum number of chunks a reader may hold open (the original's
/// `MAX_STACK_DEPTH`-minus-one assert; its arrays hold 256).
pub const MAX_CHUNK_DEPTH: usize = 255;
/// Maximum number of chunks a writer may hold open (the original's array
/// bound — it has no assert, so its release build writes out of bounds on
/// the 257th).
pub const MAX_SAVE_DEPTH: usize = 256;
/// Maximum payload bytes of a micro chunk (a `u8` size field).
pub const MICRO_CHUNK_MAX_SIZE: u32 = 255;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Failures of the chunk container. The original's release build has no
/// failure modes here — it asserts in debug builds and corrupts memory in
/// release — so every variant is a port guarantee, not a transcribed
/// return value; see the module docs for the mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkError {
    /// A read/write/seek or a close with no chunk open (the original
    /// indexes its stack out of bounds in release builds).
    NoOpenChunk,
    /// A micro chunk begun while another is open (the original asserts:
    /// micro chunks cannot nest).
    NestedMicroChunk,
    /// A chunk-level operation (`open_chunk`/`close_chunk`,
    /// `end_chunk`) while a micro chunk is still open — the original
    /// asserts ("if user didn't close any micro chunks that he opened,
    /// bad things could happen"); its release build carries the stale
    /// micro-chunk state into the chunk walk.
    MicroChunkOpen,
    /// Data written into a chunk that already contains sub-chunks (the
    /// original asserts: "you mixed data and chunks within the same
    /// chunk NO NO!").
    DataAfterSubChunks,
    /// Micro chunk writes whose total would exceed the `u8` size field
    /// (the original's release build wraps; its debug assert is an
    /// off-by-one that rejects 255 as well).
    MicroChunkTooLarge,
    /// More chunks opened than the stack can hold (reader 255, writer
    /// 256; the original asserts on read and writes out of bounds on
    /// write).
    TooDeep,
    /// [`ChunkSave::into_bytes`] called with chunks (or a micro chunk)
    /// still open — the original would emit unpatched zero-size headers.
    UnclosedChunk,
}

impl fmt::Display for ChunkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoOpenChunk => write!(f, "no chunk is open"),
            Self::NestedMicroChunk => write!(f, "micro chunks cannot nest"),
            Self::MicroChunkOpen => {
                write!(f, "a micro chunk is still open")
            }
            Self::DataAfterSubChunks => {
                write!(f, "data written into a chunk that contains sub-chunks")
            }
            Self::MicroChunkTooLarge => write!(
                f,
                "micro chunk payload exceeds {MICRO_CHUNK_MAX_SIZE} bytes"
            ),
            Self::TooDeep => write!(f, "chunk nesting exceeds the stack depth"),
            Self::UnclosedChunk => write!(f, "chunks still open at end of stream"),
        }
    }
}

impl std::error::Error for ChunkError {}

// ---------------------------------------------------------------------------
// The on-disk headers
// ---------------------------------------------------------------------------

/// The 8-byte chunk header: a type id and a size whose MSB doubles as the
/// contains-sub-chunks flag (`ChunkHeader` in the original).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChunkHeader {
    /// The chunk's type id (arbitrary; the W3D formats define the meaning).
    pub chunk_type: u32,
    /// The raw size word as stored on disk: `size | flag`.
    pub chunk_size: u32,
}

impl ChunkHeader {
    /// The header's byte form (little-endian).
    pub fn to_bytes(self) -> [u8; CHUNK_HEADER_SIZE as usize] {
        let mut out = [0u8; CHUNK_HEADER_SIZE as usize];
        out[..4].copy_from_slice(&self.chunk_type.to_le_bytes());
        out[4..].copy_from_slice(&self.chunk_size.to_le_bytes());
        out
    }

    /// Parses the header from its byte form.
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < CHUNK_HEADER_SIZE as usize {
            return None;
        }
        Some(Self {
            chunk_type: u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            chunk_size: u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
        })
    }

    /// The payload size (the size word without its MSB).
    pub fn size(self) -> u32 {
        self.chunk_size & CHUNK_SIZE_MASK
    }

    /// Whether the chunk is flagged as containing sub-chunks.
    pub fn contains_chunks(self) -> bool {
        (self.chunk_size & CHUNK_SUB_CHUNK_FLAG) != 0
    }

    /// The original's `Set_Size`: keep the flag bit, replace the count
    /// (masking like the original, so oversized counts wrap the same way).
    pub fn set_size(&mut self, size: u32) {
        self.chunk_size &= CHUNK_SUB_CHUNK_FLAG;
        self.chunk_size |= size & CHUNK_SIZE_MASK;
    }

    /// The original's `Add_Size`.
    pub fn add_size(&mut self, add: u32) {
        let size = self.size().wrapping_add(add);
        self.set_size(size);
    }

    /// The original's `Set_Sub_Chunk_Flag`.
    pub fn set_sub_chunk_flag(&mut self, on: bool) {
        if on {
            self.chunk_size |= CHUNK_SUB_CHUNK_FLAG;
        } else {
            self.chunk_size &= CHUNK_SIZE_MASK;
        }
    }
}

/// The 2-byte micro-chunk header: a `u8` type and a `u8` size
/// (`MicroChunkHeader` in the original).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MicroChunkHeader {
    /// The micro chunk's type id (0–255).
    pub chunk_type: u8,
    /// The payload size (0–255).
    pub chunk_size: u8,
}

impl MicroChunkHeader {
    /// The header's byte form.
    pub fn to_bytes(self) -> [u8; MICRO_CHUNK_HEADER_SIZE as usize] {
        [self.chunk_type, self.chunk_size]
    }

    /// Parses the header from its byte form.
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < MICRO_CHUNK_HEADER_SIZE as usize {
            return None;
        }
        Some(Self {
            chunk_type: bytes[0],
            chunk_size: bytes[1],
        })
    }
}

// ---------------------------------------------------------------------------
// ChunkSave — the writer (ChunkSaveClass)
// ---------------------------------------------------------------------------

/// One stacked writer frame: where the chunk's header sits in the output
/// and the header being accumulated.
#[derive(Debug, Clone, Copy)]
struct SaveFrame {
    header_pos: usize,
    header: ChunkHeader,
}

/// The chunk writer (`ChunkSaveClass`): wraps a byte sink the way the
/// original wraps a `FileClass`, tracking the open chunk stack and
/// patching each header when its chunk closes.
///
/// ```ignore
/// let mut save = ChunkSave::new();
/// save.begin_chunk(0x5E)?;            // a container
///     save.begin_chunk(1)?;           //   a data chunk
///     save.write(b"payload")?;
///     save.end_chunk()?;
/// save.end_chunk()?;
/// let bytes = save.into_bytes()?;     // refuses while anything is open
/// # Ok::<(), ChunkError>(())
/// ```
///
/// The output byte stream is identical to the original's for every
/// legal call sequence; the error variants are port guarantees for the
/// states where the original asserts (debug) or corrupts memory
/// (release).
#[derive(Debug, Default)]
pub struct ChunkSave {
    sink: Vec<u8>,
    stack: Vec<SaveFrame>,
    micro: Option<(usize, MicroChunkHeader)>, // (header position, header)
}

impl ChunkSave {
    /// A writer over an empty byte sink.
    pub fn new() -> Self {
        Self::default()
    }

    /// Begins a chunk: sets the parent's contains-chunks flag, writes a
    /// temporary zero-size header and stacks the frame
    /// (`ChunkSaveClass::Begin_Chunk`).
    pub fn begin_chunk(&mut self, id: u32) -> Result<(), ChunkError> {
        if self.stack.len() >= MAX_SAVE_DEPTH {
            return Err(ChunkError::TooDeep);
        }
        if let Some(parent) = self.stack.last_mut() {
            parent.header.set_sub_chunk_flag(true);
        }
        let header = ChunkHeader {
            chunk_type: id,
            chunk_size: 0,
        };
        let header_pos = self.sink.len();
        self.sink.extend_from_slice(&header.to_bytes());
        self.stack.push(SaveFrame { header_pos, header });
        Ok(())
    }

    /// Closes the innermost chunk: patches its header with the real size
    /// and adds `size + 8` to the parent's accumulator
    /// (`ChunkSaveClass::End_Chunk`).
    pub fn end_chunk(&mut self) -> Result<(), ChunkError> {
        if self.micro.is_some() {
            return Err(ChunkError::MicroChunkOpen);
        }
        let frame = self.stack.pop().ok_or(ChunkError::NoOpenChunk)?;
        let patched = frame.header.to_bytes();
        self.sink[frame.header_pos..frame.header_pos + CHUNK_HEADER_SIZE as usize]
            .copy_from_slice(&patched);
        if let Some(parent) = self.stack.last_mut() {
            parent
                .header
                .add_size(frame.header.size() + CHUNK_HEADER_SIZE);
        }
        Ok(())
    }

    /// The number of open chunks (`ChunkSaveClass::Cur_Chunk_Depth`).
    pub fn cur_chunk_depth(&self) -> usize {
        self.stack.len()
    }

    /// Begins a micro chunk inside the innermost data chunk: writes a
    /// temporary zero-size 2-byte header (`ChunkSaveClass::
    /// Begin_Micro_Chunk`). The bytes count toward the wrapping chunk's
    /// size exactly like any other data.
    pub fn begin_micro_chunk(&mut self, id: u8) -> Result<(), ChunkError> {
        if self.micro.is_some() {
            return Err(ChunkError::NestedMicroChunk);
        }
        let header = MicroChunkHeader {
            chunk_type: id,
            chunk_size: 0,
        };
        let header_pos = self.sink.len();
        // Through the Write path, like the original: the header bytes are
        // chunk data.
        let bytes = header.to_bytes();
        self.write(&bytes)?;
        self.micro = Some((header_pos, header));
        Ok(())
    }

    /// Closes the micro chunk, patching its size byte
    /// (`ChunkSaveClass::End_Micro_Chunk`).
    pub fn end_micro_chunk(&mut self) -> Result<(), ChunkError> {
        let (header_pos, header) = self.micro.take().ok_or(ChunkError::NoOpenChunk)?;
        let bytes = header.to_bytes();
        self.sink[header_pos..header_pos + MICRO_CHUNK_HEADER_SIZE as usize]
            .copy_from_slice(&bytes);
        Ok(())
    }

    /// Writes data into the innermost chunk, tracking it in the chunk's
    /// (and the open micro chunk's) accumulator
    /// (`ChunkSaveClass::Write`). Returns the byte count written; data
    /// cannot follow sub-chunks inside one chunk.
    pub fn write(&mut self, data: &[u8]) -> Result<u32, ChunkError> {
        let frame = self.stack.last().ok_or(ChunkError::NoOpenChunk)?;
        if frame.header.contains_chunks() {
            return Err(ChunkError::DataAfterSubChunks);
        }
        self.sink.extend_from_slice(data);
        let nbytes = data.len() as u32;
        let frame = self.stack.last_mut().expect("checked above");
        frame.header.add_size(nbytes);
        if let Some((_, micro)) = self.micro.as_mut() {
            let total = micro.chunk_size as u32 + nbytes;
            if total > MICRO_CHUNK_MAX_SIZE {
                return Err(ChunkError::MicroChunkTooLarge);
            }
            micro.chunk_size = total as u8;
        }
        Ok(nbytes)
    }

    /// Writes an `IOVector2Struct` — the full 8 bytes (the original's
    /// reference-taking overload has the correct `sizeof`).
    pub fn write_io_vector2(&mut self, v: &[f32; 2]) -> Result<u32, ChunkError> {
        let mut bytes = [0u8; 8];
        bytes[..4].copy_from_slice(&v[0].to_le_bytes());
        bytes[4..].copy_from_slice(&v[1].to_le_bytes());
        self.write(&bytes)
    }

    /// Writes an `IOVector3Struct` — the full 12 bytes.
    pub fn write_io_vector3(&mut self, v: &[f32; 3]) -> Result<u32, ChunkError> {
        let mut bytes = [0u8; 12];
        for (i, x) in v.iter().enumerate() {
            bytes[i * 4..i * 4 + 4].copy_from_slice(&x.to_le_bytes());
        }
        self.write(&bytes)
    }

    /// Writes an `IOVector4Struct` — the full 16 bytes.
    pub fn write_io_vector4(&mut self, v: &[f32; 4]) -> Result<u32, ChunkError> {
        let mut bytes = [0u8; 16];
        for (i, x) in v.iter().enumerate() {
            bytes[i * 4..i * 4 + 4].copy_from_slice(&x.to_le_bytes());
        }
        self.write(&bytes)
    }

    /// Writes an `IOQuaternionStruct` — the full 16 bytes.
    pub fn write_io_quaternion(&mut self, q: &[f32; 4]) -> Result<u32, ChunkError> {
        self.write_io_vector4(q)
    }

    /// Finishes the stream, returning the bytes. Refuses while any chunk
    /// or micro chunk is still open — the original would emit the
    /// unpatched zero-size headers it wrote on `begin`.
    pub fn into_bytes(self) -> Result<Vec<u8>, ChunkError> {
        if self.micro.is_some() || !self.stack.is_empty() {
            return Err(ChunkError::UnclosedChunk);
        }
        Ok(self.sink)
    }

    /// The bytes written so far (a port convenience for progress
    /// reporting; the original asks its `FileClass`).
    pub fn as_bytes(&self) -> &[u8] {
        &self.sink
    }
}

// ---------------------------------------------------------------------------
// ChunkLoad — the reader (ChunkLoadClass)
// ---------------------------------------------------------------------------

/// One stacked reader frame: the bytes consumed inside the chunk and its
/// header.
#[derive(Debug, Clone, Copy)]
struct LoadFrame {
    position: u64,
    header: ChunkHeader,
}

/// The chunk reader (`ChunkLoadClass`) over an in-memory byte slice —
/// the original wraps an opened `FileClass`; this port borrows the bytes
/// the way the Targa port's source does.
///
/// The reader is a low-level cursor: [`open_chunk`](Self::open_chunk)
/// pushes, [`close_chunk`](Self::close_chunk) pops, and the `cur_*`
/// accessors peek at the innermost open chunk. Walk it exactly like the
/// original loaders do:
///
/// ```ignore
/// let mut load = ChunkLoad::new(&bytes);
/// while load.open_chunk()? {
///     match load.cur_chunk_id() {
///         0x5E => { /* read or recurse */ }
///         _ => {}
///     }
///     load.close_chunk()?;
/// }
/// # Ok::<(), ChunkError>(())
/// ```
///
/// The accessors panic when no chunk is open — the original's assert is
/// the same contract, and its release build reads garbage instead. The
/// data paths ([`read`](Self::read), [`seek`](Self::seek)) return
/// [`Err(ChunkError::NoOpenChunk)`](ChunkError::NoOpenChunk) rather than
/// panicking, so no input byte stream can take them down.
#[derive(Debug)]
pub struct ChunkLoad<'a> {
    data: &'a [u8],
    file_pos: u64,
    stack: Vec<LoadFrame>,
    micro: Option<(u64, MicroChunkHeader)>, // (bytes consumed, header)
}

impl<'a> ChunkLoad<'a> {
    /// A reader at the start of `data`.
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            file_pos: 0,
            stack: Vec::new(),
            micro: None,
        }
    }

    /// Opens the next chunk inside the innermost open one (or the next
    /// top-level chunk at depth 0): reads the 8-byte header — unclamped
    /// by the parent's remaining bytes — and pushes the frame
    /// (`ChunkLoadClass::Open_Chunk`).
    ///
    /// Returns `Ok(false)` when the parent's child list is exhausted
    /// (its consumed-bytes counter reached its declared size) or the
    /// header read comes up short at the end of the file. Note the
    /// original's exhaustion test is **equality**: a child that
    /// over-declares its size pushes the parent's counter past its
    /// declared size, and the reader keeps parsing headers beyond the
    /// parent's boundary — transcribed as found and pinned by the
    /// goldens. Like the original's assert, the depth guard fires
    /// *before* the exhaustion check: calling this at the depth cap is
    /// [`Err(ChunkError::TooDeep)`](ChunkError::TooDeep) even on an
    /// exhausted parent (the debug build aborts the same way).
    pub fn open_chunk(&mut self) -> Result<bool, ChunkError> {
        if self.micro.is_some() {
            // The original asserts ("if user didn't close any micro
            // chunks that he opened, bad things could happen"); its
            // release build reads the header through the raw file read
            // and carries the stale micro-chunk state into the walk.
            // The port states the contract instead.
            return Err(ChunkError::MicroChunkOpen);
        }
        if self.stack.len() >= MAX_CHUNK_DEPTH {
            return Err(ChunkError::TooDeep);
        }
        if let Some(frame) = self.stack.last()
            && frame.position == u64::from(frame.header.size())
        {
            return Ok(false);
        }
        let mut header_bytes = [0u8; CHUNK_HEADER_SIZE as usize];
        // Raw file read (NOT the clamped Read): a header may straddle its
        // parent's remaining bytes; a short read consumes the available
        // bytes and fails, exactly like the original's File->Read (which
        // reads nothing once the position is past the end — a legal state
        // after the unclamped close seeks).
        let avail = ((self.data.len() as u64).saturating_sub(self.file_pos))
            .min(CHUNK_HEADER_SIZE as u64) as usize;
        if avail > 0 {
            let start = self.file_pos as usize;
            header_bytes[..avail].copy_from_slice(&self.data[start..start + avail]);
            self.file_pos += avail as u64;
        }
        if avail < CHUNK_HEADER_SIZE as usize {
            // The original's `File->Read(...) != sizeof(ChunkHeader)`
            // branch: a short header read fails the open (having consumed
            // the available bytes).
            return Ok(false);
        }
        let Some(header) = ChunkHeader::parse(&header_bytes) else {
            return Ok(false);
        };
        self.stack.push(LoadFrame {
            position: 0,
            header,
        });
        Ok(true)
    }

    /// Closes the innermost chunk, seeking the stream past whatever the
    /// caller left unread (a legal forward seek, even past the end of
    /// the data) and advancing the parent's counter by the child's
    /// declared size + 8 (`ChunkLoadClass::Close_Chunk`).
    pub fn close_chunk(&mut self) -> Result<(), ChunkError> {
        if self.micro.is_some() {
            return Err(ChunkError::MicroChunkOpen);
        }
        let frame = self.stack.pop().ok_or(ChunkError::NoOpenChunk)?;
        let csize = u64::from(frame.header.size());
        if frame.position < csize {
            self.file_pos += csize - frame.position;
        }
        if let Some(parent) = self.stack.last_mut() {
            parent.position += csize + u64::from(CHUNK_HEADER_SIZE);
        }
        Ok(())
    }

    /// The innermost open chunk's type id (`ChunkLoadClass::
    /// Cur_Chunk_ID`). Panics at depth 0, matching the original's assert.
    pub fn cur_chunk_id(&self) -> u32 {
        assert!(
            !self.stack.is_empty(),
            "cur_chunk_id with no chunk open (the original asserts the same)"
        );
        self.stack[self.stack.len() - 1].header.chunk_type
    }

    /// The innermost open chunk's declared payload size
    /// (`ChunkLoadClass::Cur_Chunk_Length`). Panics at depth 0.
    pub fn cur_chunk_length(&self) -> u32 {
        assert!(
            !self.stack.is_empty(),
            "cur_chunk_length with no chunk open (the original asserts the same)"
        );
        self.stack[self.stack.len() - 1].header.size()
    }

    /// The number of open chunks (`ChunkLoadClass::Cur_Chunk_Depth`).
    pub fn cur_chunk_depth(&self) -> usize {
        self.stack.len()
    }

    /// Whether the innermost open chunk is flagged as containing
    /// sub-chunks (`ChunkLoadClass::Contains_Chunks`). Panics at depth 0.
    /// Note the flag is advisory to callers — [`read`](Self::read) never
    /// consults it.
    pub fn contains_chunks(&self) -> bool {
        assert!(
            !self.stack.is_empty(),
            "contains_chunks with no chunk open (the original asserts the same)"
        );
        self.stack[self.stack.len() - 1].header.contains_chunks()
    }

    /// Opens the next micro chunk inside the innermost open chunk:
    /// reads the 2-byte header **through the clamping [`read`](Self::read)**
    /// (a header straddling the chunk's end fails cleanly), so micro
    /// chunks are bounded by their parent (`ChunkLoadClass::
    /// Open_Micro_Chunk`).
    pub fn open_micro_chunk(&mut self) -> Result<bool, ChunkError> {
        if self.micro.is_some() {
            return Err(ChunkError::NestedMicroChunk);
        }
        let mut header_bytes = [0u8; MICRO_CHUNK_HEADER_SIZE as usize];
        let n = self.read(&mut header_bytes)?;
        if n != MICRO_CHUNK_HEADER_SIZE {
            return Ok(false);
        }
        let Some(header) = MicroChunkHeader::parse(&header_bytes) else {
            return Ok(false);
        };
        self.micro = Some((0, header));
        Ok(true)
    }

    /// Closes the micro chunk, seeking the stream past whatever is
    /// unread **without clamping to the parent chunk's bounds** — an
    /// over-declared micro chunk can push the parent's counter past its
    /// declared size (`ChunkLoadClass::Close_Micro_Chunk`, transcribed
    /// as found).
    pub fn close_micro_chunk(&mut self) -> Result<(), ChunkError> {
        let (position, header) = self.micro.take().ok_or(ChunkError::NoOpenChunk)?;
        let csize = u64::from(header.chunk_size);
        if position < csize {
            self.file_pos += csize - position;
            if let Some(parent) = self.stack.last_mut() {
                parent.position += csize - position;
            }
        }
        Ok(())
    }

    /// The open micro chunk's type id (`ChunkLoadClass::
    /// Cur_Micro_Chunk_ID`). Panics when none is open.
    pub fn cur_micro_chunk_id(&self) -> u8 {
        assert!(
            self.micro.is_some(),
            "cur_micro_chunk_id with no micro chunk open (the original asserts the same)"
        );
        self.micro.expect("checked above").1.chunk_type
    }

    /// The open micro chunk's payload size (`ChunkLoadClass::
    /// Cur_Micro_Chunk_Length`). Panics when none is open.
    pub fn cur_micro_chunk_length(&self) -> u8 {
        assert!(
            self.micro.is_some(),
            "cur_micro_chunk_length with no micro chunk open (the original asserts the same)"
        );
        self.micro.expect("checked above").1.chunk_size
    }

    /// Reads up to `buf.len()` bytes from the innermost open chunk
    /// (`ChunkLoadClass::Read`).
    ///
    /// Returns `Ok(nbytes)` on a full read and `Ok(0)` — consuming
    /// whatever was available at the stream level — when the read would
    /// cross the chunk's declared end, the micro chunk's end, or the
    /// data runs out (the original's `File->Read(...) != nbytes`
    /// branch). Reads never check the contains-chunks flag.
    pub fn read(&mut self, buf: &mut [u8]) -> Result<u32, ChunkError> {
        let frame = self.stack.last().ok_or(ChunkError::NoOpenChunk)?;
        let nbytes = buf.len() as u64;
        if frame.position + nbytes > u64::from(frame.header.size()) {
            return Ok(0);
        }
        if let Some((position, header)) = self.micro
            && position + nbytes > u64::from(header.chunk_size)
        {
            return Ok(0);
        }
        // The file read: short reads consume the available bytes and
        // report failure, exactly like the original's RawFileClass (which
        // reads nothing once the position is past the end).
        let avail = ((self.data.len() as u64).saturating_sub(self.file_pos)).min(nbytes) as usize;
        if avail > 0 {
            let start = self.file_pos as usize;
            buf[..avail].copy_from_slice(&self.data[start..start + avail]);
            self.file_pos += avail as u64;
        }
        if avail as u64 != nbytes {
            return Ok(0);
        }
        let frame = self.stack.last_mut().expect("checked above");
        frame.position += nbytes;
        if let Some((position, _)) = self.micro.as_mut() {
            *position += nbytes;
        }
        Ok(nbytes as u32)
    }

    /// Skips forward `nbytes` inside the innermost open chunk
    /// (`ChunkLoadClass::Seek`): same clamps as [`read`](Self::read),
    /// then a raw forward seek (legal past the end of the data).
    pub fn seek(&mut self, nbytes: u32) -> Result<u32, ChunkError> {
        let frame = self.stack.last().ok_or(ChunkError::NoOpenChunk)?;
        let nbytes64 = u64::from(nbytes);
        if frame.position + nbytes64 > u64::from(frame.header.size()) {
            return Ok(0);
        }
        if let Some((position, header)) = self.micro
            && position + nbytes64 > u64::from(header.chunk_size)
        {
            return Ok(0);
        }
        self.file_pos += nbytes64;
        let frame = self.stack.last_mut().expect("checked above");
        frame.position += nbytes64;
        if let Some((position, _)) = self.micro.as_mut() {
            *position += nbytes64;
        }
        Ok(nbytes)
    }

    /// Reads an `IOVector2Struct` the way the original's 32-bit build
    /// does — its `Read` overload passes `sizeof(v)`, the size of the
    /// **pointer**, so exactly 4 bytes are consumed and only the first
    /// component is filled (`[f32; 2]`'s second element is left as the
    /// caller had it, like the original leaves the struct stale).
    pub fn read_io_vector2(&mut self, v: &mut [f32; 2]) -> Result<u32, ChunkError> {
        let mut bytes = [0u8; 4];
        let n = self.read(&mut bytes)?;
        v[0] = f32::from_le_bytes(bytes);
        Ok(n)
    }

    /// Reads an `IOVector3Struct` with the original's pointer-size bug:
    /// 4 bytes consumed, only `v[0]` filled.
    pub fn read_io_vector3(&mut self, v: &mut [f32; 3]) -> Result<u32, ChunkError> {
        let mut bytes = [0u8; 4];
        let n = self.read(&mut bytes)?;
        v[0] = f32::from_le_bytes(bytes);
        Ok(n)
    }

    /// Reads an `IOVector4Struct` with the original's pointer-size bug:
    /// 4 bytes consumed, only `v[0]` filled.
    pub fn read_io_vector4(&mut self, v: &mut [f32; 4]) -> Result<u32, ChunkError> {
        let mut bytes = [0u8; 4];
        let n = self.read(&mut bytes)?;
        v[0] = f32::from_le_bytes(bytes);
        Ok(n)
    }

    /// Reads an `IOQuaternionStruct` with the original's pointer-size
    /// bug: 4 bytes consumed, only `q[0]` filled.
    pub fn read_io_quaternion(&mut self, q: &mut [f32; 4]) -> Result<u32, ChunkError> {
        self.read_io_vector4(q)
    }

    /// The current position in the underlying byte stream (a port
    /// addition standing in for `File->Tell()`; the goldens use it to
    /// pin the stream bookkeeping).
    pub fn tell(&self) -> u64 {
        self.file_pos
    }
}
