// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Ported from the Zero Hour source release (baseline 571a7c5):
//   Core/GameEngine/Source/GameClient/GameText.cpp   (getCSFInfo, parseCSF,
//                                                     stripSpaces)
//   Core/Tools/Babylon/expimp.{h,cpp}                (CSF_HEADER, chunk ids,
//                                                     generate_csf, writeCSFLabel,
//                                                     writeCSFString)
// See the module docs for the format layout and the documented differences
// from the C++ (fixed-buffer overruns and ignored short reads become
// errors; the Babylon tool's content transforms stay out of the writer).

//! The **CSF** (compiled string table) format.
//!
//! A `.csf` file is the game's compiled localization database: a flat list
//! of labels, each holding one or more UTF-16 strings plus an optional
//! speech-wave path, with every string's code units stored **bitwise
//! inverted**. The runtime loads it through `GameTextManager::parseCSF`
//! (GameText.cpp); the localization tool writes it through Babylon's
//! `generate_csf` (Core/Tools/Babylon/expimp.cpp). This module ports both
//! directions so that `parse`/`to_bytes` round-trips are **byte-exact with
//! the original implementations** (parity level P0, pinned by golden
//! vectors generated from transcriptions of the two — see
//! `tools/differential/gen_csf_golden.cpp`).
//!
//! # Layering (deliberate)
//!
//! The original smears two concerns across the loader:
//!
//! 1. *format semantics* — the chunk layout, the code-unit inversion,
//! 2. *consumer normalizations* — `parseCSF` also strips spaces from the
//!    text and keeps only each label's first string.
//!
//! This port separates them. [`CsfFile::parse`] returns the file's raw
//! content (all strings, all code units, byte-faithful); the consumer-side
//! view is available through the `game_*` helpers ([`CsfLabel::game_text`],
//! [`CsfLabel::game_speech`], [`strip_spaces`]), which transcribe
//! `parseCSF`'s post-processing exactly — quirks included (see below). The
//! future game-text port keeps the original behaviour by calling the same
//! helpers the goldens pin, instead of re-deriving it inside the parser.
//!
//! # File layout (as implemented here)
//!
//! All integers are little-endian `i32`. The magic values are written by
//! the C++ as `('C'<<24)|('S'<<16)|('F'<<8)|(' ')` and hit the disk in
//! low-byte-first order, so the file literally starts with the bytes
//! `" FSC"` — the constants below are spelled through
//! [`u32::from_le_bytes`] to keep that quirk visible.
//!
//! ```text
//! header   : id=" FSC" | version | num_labels | num_strings | skip | langid
//! label    : id=" LBL" | num_strings | name_len | name_len bytes (not inverted)
//! string   : id=" RTS" | text_len | text_len * 2 bytes (UTF-16LE, inverted)
//!   ...or  : id="WRTS" | text_len | text bytes | wave_len | wave_len bytes
//! ```
//!
//! - `num_labels`/`num_strings` are metadata the original never validates
//!   when reading (the game ignores them; Babylon recomputes them on
//!   write). [`CsfFile::parse`] also ignores them; [`CsfFile::to_bytes`]
//!   always emits the correct counts (the canonical form Babylon writes).
//! - `skip` is unused by both reference implementations; it is preserved
//!   verbatim on round-trip.
//! - `version` and `langid` are preserved verbatim. The game's
//!   *language selection* rule (`getCSFInfo`: `version >= 2` uses the file's
//!   `langid`, otherwise US) is consumer logic — [`CsfFile::game_language`]
//!   transcribes it.
//! - A wave chunk stores `wave_len` bytes with `wave_len = strlen(path) + 1`,
//!   followed by exactly that payload: the path plus **one extra byte**.
//!   Babylon writes the language's `LANGINFO::character` letter there
//!   (TransDB.cpp: US → `'e'`, German → `'g'`, …), not a NUL; other CSF
//!   writers put `'\0'`. The runtime reads the whole chunk and
//!   NUL-terminates its copy, so the letter (when present) is part of the
//!   speech string it keeps — `GameText.cpp` never reads that field back.
//!   [`CsfString::wave`] keeps the raw chunk bytes so any file round-trips;
//!   [`CsfString::wave_path`] is the up-to-first-NUL view.
//!
//! # Consumer-view quirks (transcribed, not fixed)
//!
//! `parseCSF` decodes a string by inverting code units **until it reads a
//! zero** and hands the buffer to `UnicodeString`, which stops at the
//! first NUL. Combined with the inversion this means the game-visible text
//! ends at the first decoded `0xFFFF` (whose stored form is zero, stopping
//! the loop) *or* the first decoded `0x0000` (where the inverted buffer
//! becomes zero). [`CsfLabel::game_text`] reproduces exactly that —
//! including the truncation, which the Babylon writer can produce and
//! real files never contain.
//!
//! # Differences from the C++ (documented)
//!
//! - The C++ reader copies lengths into fixed `MAX_UITEXT_LENGTH` buffers
//!   without checking them (a hostile file smashes the stack) and ignores
//!   short reads mid-label (stale buffer reuse). This port returns
//!   [`CsfError::Truncated`] / [`CsfError::InvalidLength`]
//!   instead. Well-formed files parse identically.
//! - The C++ writer (`generate_csf`) is a Babylon *tool*: it strips spaces
//!   and converts meta characters before writing, and refuses labels with
//!   empty names. Those are editor-side transforms, not format semantics;
//!   [`CsfFile::to_bytes`] writes the structure it is given. The golden
//!   generator transcribes the writer minus those transforms, and its
//!   corpora never contain a space run, a meta character, or an empty
//!   label name — where the two agree byte-for-byte.
//! - A negative per-label `num_strings` behaves as zero in the original
//!   (its copy loop simply does not run); this port does the same.
//!
//! # Examples
//!
//! ```
//! use rzh_formats::csf::{CsfFile, CsfLabel, CsfString};
//!
//! let file = CsfFile {
//!     version: 3,
//!     skip: 0,
//!     language: 0,
//!     labels: vec![CsfLabel {
//!         name: b"GUI:Hello".to_vec(),
//!         strings: vec![CsfString::new("Hello World".encode_utf16().collect())],
//!     }],
//! };
//! let bytes = file.to_bytes().unwrap();
//! let parsed = CsfFile::parse(&bytes).unwrap();
//! assert_eq!(parsed, file);
//! assert_eq!(parsed.labels[0].game_text(), "Hello World".encode_utf16().collect::<Vec<_>>());
//! ```

use std::fmt;

/// `('C'<<24)|('S'<<16)|('F'<<8)|(' ')` — on disk (little-endian, low byte
/// first) the file literally begins with the bytes `" FSC"`.
pub const CSF_ID: u32 = u32::from_le_bytes(*b" FSC");

/// `('L'<<24)|('B'<<16)|('L'<<8)|(' ')` — a label chunk (`" LBL"` on disk).
pub const CSF_LABEL: u32 = u32::from_le_bytes(*b" LBL");

/// `('S'<<24)|('T'<<16)|('R'<<8)|(' ')` — a string chunk. On disk the
/// value hits the wire low-byte-first, so the bytes spell `" RTS"`
/// (every chunk name is reversed in the file).
pub const CSF_STRING: u32 = u32::from_le_bytes(*b" RTS");

/// `('S'<<24)|('T'<<16)|('R'<<8)|('W')` — a string-plus-wave chunk
/// (`"WRTS"` on disk).
pub const CSF_STRINGWITHWAVE: u32 = u32::from_le_bytes(*b"WRTS");

/// The version Babylon writes (`CSF_VERSION` in expimp.h). Parsing does
/// not reject other versions — the original never checks them either.
pub const CSF_VERSION: i32 = 3;

/// `sizeof(CSF_HEADER)`: six little-endian `i32`s. The game's readers read
/// exactly this many bytes (`getCSFInfo` and `parseCSF` both fail a file
/// whose header is shorter), so v1's 20-byte header is not readable by the
/// original either.
pub const HEADER_LEN: usize = 24;

/// Language id for US English (`LANGUAGE_ID_US`, Common/Language.h) — the
/// default `getCSFInfo` applies to files with `version < 2`.
pub const LANGUAGE_ID_US: i32 = 0;

/// Why a byte slice is not a valid CSF file.
///
/// The taxonomy mirrors the original's failure points; anything the C++
/// handles by reading out of bounds or reusing a stale buffer is an error
/// here instead (see the module docs). `needed`/`got` are bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CsfError {
    /// Fewer than [`HEADER_LEN`] bytes: `getCSFInfo`'s header read fails
    /// and the file would not be treated as a CSF at all.
    TruncatedHeader {
        /// Bytes actually available.
        got: usize,
    },
    /// The header id is not [`CSF_ID`].
    BadMagic {
        /// The id that was read.
        got: u32,
    },
    /// A mid-file read came up short: `field` names the item ("label
    /// string count", "label name", "string text", "wave path", ...).
    /// The C++ ignores these short reads and reuses stale buffer content;
    /// this port refuses the file.
    Truncated {
        /// Which item was being read.
        field: &'static str,
        /// Bytes the item needs.
        needed: usize,
        /// Bytes actually available.
        got: usize,
    },
    /// A length field is negative. The C++ would index its fixed buffer
    /// before its start (`m_buffer[len]` with `len < 0`).
    InvalidLength {
        /// Which length field.
        field: &'static str,
        /// The value as read.
        got: i32,
    },
    /// A position where a [`CSF_LABEL`] chunk id was required holds
    /// something else. `parseCSF` bails out (`goto quit`) with a failure —
    /// the whole file is rejected, including the labels before it.
    LabelIdMismatch {
        /// The id that was read.
        got: u32,
        /// Byte offset of the id.
        offset: usize,
    },
    /// A string chunk id is neither [`CSF_STRING`] nor
    /// [`CSF_STRINGWITHWAVE`]; same bail-out as above.
    StringIdMismatch {
        /// The id that was read.
        got: u32,
        /// Byte offset of the id.
        offset: usize,
    },
    /// A count or length the writer must emit does not fit the format's
    /// `i32` fields (labels, strings, name/text/wave bytes).
    CountOverflow {
        /// Which field would overflow.
        field: &'static str,
        /// The value that does not fit.
        got: usize,
    },
}

impl fmt::Display for CsfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CsfError::TruncatedHeader { got } => write!(
                f,
                "truncated CSF header: need {HEADER_LEN} bytes, got {got}"
            ),
            CsfError::BadMagic { got } => {
                write!(f, "bad CSF magic: expected {CSF_ID:#010x}, got {got:#010x}")
            }
            CsfError::Truncated { field, needed, got } => {
                write!(f, "truncated CSF {field}: need {needed} bytes, got {got}")
            }
            CsfError::InvalidLength { field, got } => {
                write!(f, "invalid negative CSF {field} length: {got}")
            }
            CsfError::LabelIdMismatch { got, offset } => write!(
                f,
                "expected a \" LBL\" chunk at offset {offset}, got {got:#010x}"
            ),
            CsfError::StringIdMismatch { got, offset } => write!(
                f,
                "expected a \" RTS\"/\"WRTS\" chunk at offset {offset}, got {got:#010x}"
            ),
            CsfError::CountOverflow { field, got } => {
                write!(f, "CSF {field} does not fit an i32: {got}")
            }
        }
    }
}

impl std::error::Error for CsfError {}

/// The six header fields of a CSF file (`CSF_HEADER`, expimp.h).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CsfHeader {
    /// `version` — [`CSF_VERSION`] in files Babylon writes.
    pub version: i32,
    /// `num_labels` — metadata; the original never validates it on read.
    pub num_labels: i32,
    /// `num_strings` — metadata; the original never validates it on read.
    pub num_strings: i32,
    /// `skip` — unused by both reference implementations; preserved.
    pub skip: i32,
    /// `langid` — the raw language field (`LanguageID`, Common/Language.h).
    pub language: i32,
}

impl CsfHeader {
    /// The language the runtime would select for this header
    /// (`getCSFInfo`: the file's `langid` when `version >= 2`, US
    /// English otherwise).
    pub fn game_language(&self) -> i32 {
        if self.version >= 2 {
            self.language
        } else {
            LANGUAGE_ID_US
        }
    }
}

/// A parsed CSF file: the header fields plus every label in file order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CsfFile {
    /// `version`, preserved verbatim.
    pub version: i32,
    /// `skip`, preserved verbatim (semantics undefined in the originals).
    pub skip: i32,
    /// `langid`, preserved verbatim.
    pub language: i32,
    /// Every label, in file order (duplicates included — the runtime
    /// builds its lookup later and this layer must not lose bytes).
    pub labels: Vec<CsfLabel>,
}

/// One `" LBL"` chunk: a label name and its string chunks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CsfLabel {
    /// The label bytes exactly as stored (the format allows any non-NUL
    /// byte; game labels are ASCII keys like `GUI:Hello`). The runtime's
    /// C-string view is [`CsfLabel::game_name`]; a UTF-8 view (when the
    /// bytes are ASCII/UTF-8) is [`CsfLabel::name_str`].
    pub name: Vec<u8>,
    /// The label's string chunks, in file order.
    pub strings: Vec<CsfString>,
}

/// One `" RTS"`/`"WRTS"` chunk: a UTF-16 text plus an optional wave path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CsfString {
    /// The text as UTF-16 code units, **after** undoing the on-disk
    /// bitwise inversion. Kept as raw units (not `String`) so unpaired
    /// surrogates and embedded NULs round-trip byte-exactly.
    pub text: Vec<u16>,
    /// The raw wave-path bytes of a `"WRTS"` chunk exactly as stored:
    /// `wave_len = strlen(path) + 1` bytes — the path plus one trailing
    /// byte, which is the Babylon language letter (`'e'` for US) in
    /// EA-written files and `NUL` in files from other writers. `None`
    /// for a plain `" RTS"` chunk.
    pub wave: Option<Vec<u8>>,
}

impl CsfString {
    /// A plain string chunk (`" RTS"`, no wave).
    pub fn new(text: Vec<u16>) -> Self {
        CsfString { text, wave: None }
    }

    /// A string chunk with a speech wave (`"WRTS"`). Stores `path` plus
    /// one NUL byte, with the length field counting it
    /// (`strlen(path) + 1`, the byte count every writer agrees on;
    /// Babylon's actual trailing byte is the language letter — see the
    /// field docs — so round-tripping an EA file preserves its letter).
    pub fn with_wave(text: Vec<u16>, wave_path: &[u8]) -> Self {
        let mut wave = Vec::with_capacity(wave_path.len() + 1);
        wave.extend_from_slice(wave_path);
        wave.push(0);
        CsfString {
            text,
            wave: Some(wave),
        }
    }

    /// The wave path as a C string: everything before the first NUL.
    /// For EA/Babylon-written files the chunk has no NUL, so this is the
    /// full stored payload including the trailing language letter.
    /// `None` when the chunk has no wave; empty when it stored nothing.
    pub fn wave_path(&self) -> Option<&[u8]> {
        self.wave
            .as_deref()
            .map(|w| match w.iter().position(|&b| b == 0) {
                Some(stop) => &w[..stop],
                None => w,
            })
    }

    /// The runtime view of this string (`parseCSF`): the decoded units
    /// truncated at the first `0x0000`/`0xFFFF` (the loader's
    /// invert-until-zero loop meeting `UnicodeString`'s NUL-terminated
    /// copy — see the module docs), then [`strip_spaces`].
    pub fn game_text(&self) -> Vec<u16> {
        let mut out = Vec::with_capacity(self.text.len());
        self.game_text_into(&mut out);
        out
    }

    /// [`CsfString::game_text`] into a caller-owned buffer, mirroring the
    /// original's reused member buffers (`m_tbuffer`). Clears `out`.
    pub fn game_text_into(&self, out: &mut Vec<u16>) {
        out.clear();
        // parseCSF reads `len` stored (inverted) units into m_tbuffer and
        // inverts them in place until it reads a zero. Reconstructed on
        // the decoded representation: the loop stops at the first unit
        // whose stored form is zero (decoded 0xFFFF), and the game's
        // UnicodeString stops at the first unit the inversion turned into
        // zero (decoded 0x0000). Either way the visible text ends there.
        let end = self
            .text
            .iter()
            .position(|&u| u == 0x0000 || u == 0xFFFF)
            .unwrap_or(self.text.len());
        out.extend_from_slice(&self.text[..end]);
        strip_spaces(out);
    }
}

impl CsfLabel {
    /// The label as a C string: the stored bytes up to the first NUL
    /// (`AsciiString::operator=(const char*)` semantics — `parseCSF`
    /// assigns from its NUL-terminated fixed buffer).
    pub fn game_name(&self) -> &[u8] {
        match self.name.iter().position(|&b| b == 0) {
            Some(stop) => &self.name[..stop],
            None => &self.name,
        }
    }

    /// The label as UTF-8/ASCII, when the stored bytes are valid UTF-8.
    pub fn name_str(&self) -> Option<&str> {
        std::str::from_utf8(&self.name).ok()
    }

    /// The runtime view of the label's text (`parseCSF` keeps **only the
    /// first string** of a label: "only use the first string found").
    /// Empty when the label has no strings.
    pub fn game_text(&self) -> Vec<u16> {
        self.strings
            .first()
            .map(CsfString::game_text)
            .unwrap_or_default()
    }

    /// The runtime view of the label's speech (`parseCSF`: the first
    /// string's wave chunk read as a C string; a zero-length wave chunk
    /// leaves the speech empty — `if (num == 0 && len)`). For
    /// Babylon-written files this includes the trailing language letter,
    /// exactly like the original's `m_stringInfo[].speech`.
    pub fn game_speech(&self) -> &[u8] {
        self.strings
            .first()
            .and_then(|s| s.wave.as_deref())
            .filter(|w| !w.is_empty())
            .map(|w| match w.iter().position(|&b| b == 0) {
                Some(stop) => &w[..stop],
                None => w,
            })
            .unwrap_or(&[])
    }
}

impl CsfFile {
    /// The language the runtime would select for this file
    /// (`getCSFInfo`: the file's `langid` when `version >= 2`, US
    /// English otherwise).
    pub fn game_language(&self) -> i32 {
        if self.version >= 2 {
            self.language
        } else {
            LANGUAGE_ID_US
        }
    }

    /// The `getCSFInfo` view: parse just the 24-byte header and validate
    /// the magic. This is exactly what the runtime does to decide whether
    /// a file is a CSF at all (it never validates the declared counts).
    pub fn parse_header(bytes: &[u8]) -> Result<CsfHeader, CsfError> {
        if bytes.len() < HEADER_LEN {
            return Err(CsfError::TruncatedHeader { got: bytes.len() });
        }
        let id = read_u32(bytes, 0);
        if id != CSF_ID {
            return Err(CsfError::BadMagic { got: id });
        }
        Ok(CsfHeader {
            version: read_i32(bytes, 4),
            num_labels: read_i32(bytes, 8),
            num_strings: read_i32(bytes, 12),
            skip: read_i32(bytes, 16),
            language: read_i32(bytes, 20),
        })
    }

    /// Parse a whole CSF file. Mirrors `GameTextManager::parseCSF` at the
    /// format level: every label chunk is read, all of each label's
    /// strings are kept, and the on-disk inversion is undone. Trailing
    /// bytes shorter than a chunk id are accepted (the original's read
    /// loop simply ends); four or more trailing bytes must start a label
    /// chunk or the file is rejected, exactly like the original's
    /// `goto quit`.
    pub fn parse(bytes: &[u8]) -> Result<CsfFile, CsfError> {
        let header = CsfFile::parse_header(bytes)?;
        let mut pos = HEADER_LEN;
        let mut labels = Vec::new();

        loop {
            // while (file->read(&id, sizeof(id)) == sizeof(id)): a final
            // 1..3-byte remnant ends the loop with the parse accepted.
            if bytes.len() - pos < 4 {
                break;
            }
            let id_offset = pos;
            let id = read_u32(bytes, pos);
            pos += 4;
            if id != CSF_LABEL {
                return Err(CsfError::LabelIdMismatch {
                    got: id,
                    offset: id_offset,
                });
            }

            let num_strings = read_at(bytes, &mut pos, "label string count")?;
            // The original's copy loop (num = 0; num < num_strings) simply
            // never runs for a negative count.
            let num_strings = num_strings.max(0) as usize;
            let name_len = read_at(bytes, &mut pos, "label name length")?;
            if name_len < 0 {
                return Err(CsfError::InvalidLength {
                    field: "label name",
                    got: name_len,
                });
            }
            let name_len = name_len as usize;
            let name = take(bytes, &mut pos, name_len, "label name")?;

            let mut strings = Vec::with_capacity(num_strings.min(64));
            for _ in 0..num_strings {
                let id_offset = pos;
                let id = take(bytes, &mut pos, 4, "string chunk id")?;
                let id = u32::from_le_bytes([id[0], id[1], id[2], id[3]]);
                if id != CSF_STRING && id != CSF_STRINGWITHWAVE {
                    return Err(CsfError::StringIdMismatch {
                        got: id,
                        offset: id_offset,
                    });
                }

                let text_len = read_at(bytes, &mut pos, "string text length")?;
                if text_len < 0 {
                    return Err(CsfError::InvalidLength {
                        field: "string text",
                        got: text_len,
                    });
                }
                let text_len = text_len as usize;
                let text_bytes = take(bytes, &mut pos, text_len * 2, "string text")?;
                let text = decode_units(text_bytes);

                let wave = if id == CSF_STRINGWITHWAVE {
                    let wave_len = read_at(bytes, &mut pos, "wave path length")?;
                    if wave_len < 0 {
                        return Err(CsfError::InvalidLength {
                            field: "wave path",
                            got: wave_len,
                        });
                    }
                    let wave_len = wave_len as usize;
                    Some(take(bytes, &mut pos, wave_len, "wave path")?.to_vec())
                } else {
                    None
                };

                strings.push(CsfString { text, wave });
            }

            labels.push(CsfLabel {
                name: name.to_vec(),
                strings,
            });
        }

        Ok(CsfFile {
            version: header.version,
            skip: header.skip,
            language: header.language,
            labels,
        })
    }

    /// Serialize in the canonical form Babylon's `generate_csf` writes:
    /// `version`/`skip`/`langid` as stored, the label and string counts
    /// recomputed, `"WRTS"` chunks emitted for strings carrying a wave.
    /// Byte-identical to the C++ writer for every structure the tool can
    /// express (see the module docs for the tool-only transforms this
    /// deliberately omits).
    pub fn to_bytes(&self) -> Result<Vec<u8>, CsfError> {
        let mut out = Vec::new();
        self.write_into(&mut out)?;
        Ok(out)
    }

    /// [`CsfFile::to_bytes`] into a caller-owned buffer (cleared first).
    pub fn write_into(&self, out: &mut Vec<u8>) -> Result<(), CsfError> {
        out.clear();
        let num_strings: usize = self.labels.iter().map(|l| l.strings.len()).sum();
        count_fits("num_labels", self.labels.len())?;
        count_fits("num_strings", num_strings)?;

        // The header, then the body — same bytes as Babylon's
        // write-header/seek/write-body/seek-back/rewrite-header dance.
        out.extend_from_slice(&CSF_ID.to_le_bytes());
        out.extend_from_slice(&self.version.to_le_bytes());
        out.extend_from_slice(&(self.labels.len() as i32).to_le_bytes());
        out.extend_from_slice(&(num_strings as i32).to_le_bytes());
        out.extend_from_slice(&self.skip.to_le_bytes());
        out.extend_from_slice(&self.language.to_le_bytes());

        for label in &self.labels {
            count_fits("label name length", label.name.len())?;
            out.extend_from_slice(&CSF_LABEL.to_le_bytes());
            out.extend_from_slice(&(label.strings.len() as i32).to_le_bytes());
            out.extend_from_slice(&(label.name.len() as i32).to_le_bytes());
            out.extend_from_slice(&label.name);

            for string in &label.strings {
                count_fits("string text length", string.text.len())?;
                let id = if string.wave.is_some() {
                    CSF_STRINGWITHWAVE
                } else {
                    CSF_STRING
                };
                out.extend_from_slice(&id.to_le_bytes());
                out.extend_from_slice(&(string.text.len() as i32).to_le_bytes());
                for unit in &string.text {
                    // writeCSFString inverts each code unit before the
                    // fwrite — the on-disk obfuscation.
                    out.extend_from_slice(&(!unit).to_le_bytes());
                }
                if let Some(wave) = &string.wave {
                    count_fits("wave path length", wave.len())?;
                    out.extend_from_slice(&(wave.len() as i32).to_le_bytes());
                    out.extend_from_slice(wave);
                }
            }
        }

        Ok(())
    }
}

/// `GameTextManager::stripSpaces` (GameText.cpp), verbatim on UTF-16 code
/// units: leading spaces dropped, space runs collapsed to one, a space
/// before a newline/tab dropped, trailing space dropped, everything else
/// (including the newline/tab themselves and embedded NUL units) kept.
///
/// The C++ compacts in place through two pointers and NUL-terminates; the
/// port compacts the `Vec` and truncates it. The C version also stops at
/// the terminator it appended after the inverted buffer — the caller
/// (`game_text_into`) has already truncated at that point, so the
/// observable behaviour is identical.
pub fn strip_spaces(string: &mut Vec<u16>) {
    const SPACE: u16 = b' ' as u16;
    const LF: u16 = b'\n' as u16;
    const TAB: u16 = b'\t' as u16;

    let mut write = 0usize; // `str`
    let mut read = 0usize; // `ptr`
    let mut last: u16 = 0;
    let mut skip_all = true;

    while read < string.len() {
        let ch = string[read];
        read += 1;

        if ch == SPACE && (last == SPACE || skip_all) {
            continue;
        }

        if ch == LF || ch == TAB {
            // remove last space
            if last == SPACE && write > 0 {
                write -= 1;
            }
            skip_all = true; // skip all spaces
            last = ch;
            string[write] = ch;
            write += 1;
            continue;
        }

        last = ch;
        string[write] = ch;
        write += 1;
        skip_all = false;
    }

    if last == SPACE && write > 0 {
        write -= 1;
    }

    string.truncate(write);
}

// --- internal helpers --------------------------------------------------------

/// Undo the on-disk inversion: every code unit is stored bitwise-inverted
/// (`*ptr = ~*ptr` in both the Babylon writer and the runtime reader).
fn decode_units(bytes: &[u8]) -> Vec<u16> {
    bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| !u16::from_le_bytes(*pair))
        .collect()
}

fn read_u32(bytes: &[u8], pos: usize) -> u32 {
    u32::from_le_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]])
}

fn read_i32(bytes: &[u8], pos: usize) -> i32 {
    i32::from_le_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]])
}

/// A checked `i32` read that advances `pos` (the original ignores the
/// `File::read` return value here; we refuse the file instead).
fn read_at(bytes: &[u8], pos: &mut usize, field: &'static str) -> Result<i32, CsfError> {
    if bytes.len() - *pos < 4 {
        return Err(CsfError::Truncated {
            field,
            needed: 4,
            got: bytes.len() - *pos,
        });
    }
    let value = read_i32(bytes, *pos);
    *pos += 4;
    Ok(value)
}

/// A checked byte-slice take that advances `pos`.
fn take<'a>(
    bytes: &'a [u8],
    pos: &mut usize,
    len: usize,
    field: &'static str,
) -> Result<&'a [u8], CsfError> {
    if bytes.len() - *pos < len {
        return Err(CsfError::Truncated {
            field,
            needed: len,
            got: bytes.len() - *pos,
        });
    }
    let slice = &bytes[*pos..*pos + len];
    *pos += len;
    Ok(slice)
}

fn count_fits(field: &'static str, got: usize) -> Result<(), CsfError> {
    if got > i32::MAX as usize {
        return Err(CsfError::CountOverflow { field, got });
    }
    Ok(())
}
