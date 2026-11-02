// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
//! The INI reader engine.
//!
//! Line-for-line port of the reader half of
//! `Core/GameEngine/Source/Common/INI/INI.cpp` @ 571a7c5 (Zero Hour
//! configuration, release build: `DEBUG_ASSERTCRASH`/`DEBUG_CRASH` are
//! no-ops, so tabs become spaces, oversized lines truncate silently and
//! unknown *fields* are skipped silently — each is pinned by a test).
//!
//! The original reads the whole file into a RAM buffer
//! (`File::readEntireAndClose`) and then walks it line by line; this port
//! takes the bytes by value, keeping the exact byte-level line semantics:
//!
//! * a line ends at `\n` (stripped), at the buffer cap of
//!   [`MAX_CHARS_PER_LINE`] raw bytes, or at end-of-input;
//! * `;` starts a comment by writing a NUL into the line buffer (the C
//!   string view ends there; any bytes after it are still consumed);
//! * control characters (`0 < c < 32`) become spaces;
//! * the line counter increments on every `read_line` call that was not
//!   already past EOF, including the synthetic final line produced when
//!   the input ends — the C++ counts it too.
//!
//! Tokens are produced with exact `strtok` semantics: the first call on a
//! line re-anchors the cursor, leading separators are skipped, empty
//! tokens never appear, and the separator set can change between calls
//! (the quoted-string readers rely on that).
//!
//! # Parity level
//!
//! **P0 exact** on the golden vectors in `tests/golden/` (byte-identical
//! dumps against the C++ transcription in `tools/differential/`).

use crate::error::{ErrorKind, IniError};
use crate::scan::{
    ScanError, scan_bool, scan_index_list, scan_int, scan_lookup_list, scan_percent_to_real,
    scan_real, scan_unsigned_int,
};

/// `INI_MAX_CHARS_PER_LINE` (INI.h): raw byte cap per line, before the
/// NUL terminator.
pub const MAX_CHARS_PER_LINE: usize = 1028;

/// `INI::getSeps()`.
pub const SEPS: &[u8] = b" \n\r\t=";
/// `INI::getSepsPercent()`.
pub const SEPS_PERCENT: &[u8] = b" \n\r\t=%";
/// `INI::getSepsColon()`.
pub const SEPS_COLON: &[u8] = b" \n\r\t=:";
/// `INI::getSepsQuote()`.
pub const SEPS_QUOTE: &[u8] = b"\"\n=";
/// `INI::getEndToken()`.
pub const END_TOKEN: &[u8] = b"End";

/// `WWMath::PI` (wwmath.h): 3.14159265358979323846f, i.e. the `f32`
/// nearest to π — bit-identical to `rzh-math`'s `scalar::PI` and to
/// `f32::consts::PI`. The rounded literal carries its provenance; the
/// allows silence the two literal lints.
#[allow(clippy::approx_constant, clippy::excessive_precision)]
const PI: f32 = 3.141_592_654;

// GameCommon.h frame constants, WWSyncPerSecond = 30.
/// `LOGICFRAMES_PER_MSEC_REAL`.
pub(crate) const LOGICFRAMES_PER_MSEC: f32 = 30.0f32 / 1000.0f32;
/// `SECONDS_PER_LOGICFRAME_REAL` (30 Hz logic frames).
pub const SECONDS_PER_LOGICFRAME: f32 = 1.0f32 / 30.0f32;

/// `INILoadType` (INI.h).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadType {
    /// `INI_LOAD_OVERWRITE`: create new or load over existing data.
    Overwrite,
    /// `INI_LOAD_CREATE_OVERRIDES`: create new or load into a new override
    /// instance (map/mod INI data).
    CreateOverrides,
    /// `INI_LOAD_MULTIFILE`: create new or continue loading into an
    /// existing instance.
    Multifile,
}

/// Converts a scan-level failure into an engine error at the current
/// file/line position.
impl From<ScanError> for ErrorKind {
    fn from(error: ScanError) -> Self {
        match error {
            ScanError::InvalidData => Self::InvalidData,
            ScanError::InvalidNameList => Self::InvalidNameList,
        }
    }
}

/// A parsed data block: the field-dispatch target of `initFromINI`.
///
/// The C++ dispatches through a `FieldParse` table of tokens, parse
/// functions and raw struct offsets; the port replaces the offset writes
/// with a typed `match` in the implementation (plan §3.3 rule 2). The
/// table's semantics are preserved:
///
/// * field lookup is **case-sensitive** (`strcmp` in `findFieldParse`);
/// * an unhandled field makes the implementation return `Ok(false)` and
///   the loop skips the line silently, like the release build does
///   (`DEBUG_CRASH` is a no-op outside debug builds);
/// * a trailing catch-all table entry translates to a `_ =>` arm that
///   receives the field token itself.
pub trait IniFieldTarget {
    /// Parses the value tokens of one field. Returns whether the field
    /// was recognised.
    fn parse_field(&mut self, ini: &mut Ini, field: &[u8]) -> Result<bool, IniError>;
}

/// The block dispatcher: the `theTypeTable` equivalent. Returning
/// `Ok(false)` reports an unknown block, which fails the load with
/// `INI_UNKNOWN_TOKEN` exactly like the original.
pub trait BlockParser {
    /// Parses one data block whose declaration line begins with `block`.
    fn parse_block(&mut self, ini: &mut Ini, block: &[u8]) -> Result<bool, IniError>;
}

/// The INI reader (`class INI`).
///
/// One value may load any number of files in sequence (the engine was a
/// single reusable instance in the C++, global state included; the port
/// keeps the per-file state here and the shared stores outside).
pub struct Ini {
    data: Vec<u8>,
    next: usize,
    filename: String,
    load_type: LoadType,
    line_num: u32,
    buffer: Vec<u8>,
    /// The C-string length of `buffer` (position of its first NUL).
    cend: usize,
    /// The `strtok` cursor into `buffer[..cend]`.
    tok: usize,
    eof: bool,
}

impl Default for Ini {
    fn default() -> Self {
        Self::new()
    }
}

impl Ini {
    /// A fresh reader (the `INI::INI()` defaults; no file open).
    #[must_use]
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            next: 0,
            filename: "None".to_owned(),
            load_type: LoadType::Overwrite,
            line_num: 0,
            buffer: Vec::new(),
            cend: 0,
            tok: 0,
            eof: false,
        }
    }

    /// The name of the file being loaded (`INI::getFilename`).
    #[must_use]
    pub fn filename(&self) -> &str {
        &self.filename
    }

    /// The load type of the file being loaded (`INI::getLoadType`).
    #[must_use]
    pub const fn load_type(&self) -> LoadType {
        self.load_type
    }

    /// The current line counter (`INI::getLineNum`).
    #[must_use]
    pub const fn line_num(&self) -> u32 {
        self.line_num
    }

    /// Whether end-of-input has been reached (`INI::isEOF`).
    #[must_use]
    pub const fn is_eof(&self) -> bool {
        self.eof
    }

    /// Builds an error carrying the current file/line context.
    pub(crate) fn error(&self, kind: ErrorKind) -> IniError {
        IniError::new(kind, &self.filename, self.line_num)
    }

    /// `INI::load`: reads one file's bytes top to bottom, dispatching each
    /// block declaration line to `parser`.
    ///
    /// The C++ opens the file through `TheFileSystem`; the port takes the
    /// bytes (the VFS integration arrives with the Phase 2 data loader).
    /// Returns the number of files loaded (always 1 on success, the
    /// original's return value).
    pub fn load(
        &mut self,
        filename: &str,
        data: Vec<u8>,
        load_type: LoadType,
        parser: &mut dyn BlockParser,
    ) -> Result<u32, IniError> {
        // prepFile: fresh cursor state for this file.
        self.data = data;
        self.next = 0;
        self.filename = filename.to_owned();
        self.load_type = load_type;
        self.line_num = 0;
        self.buffer.clear();
        self.cend = 0;
        self.tok = 0;
        self.eof = false;

        while !self.eof {
            self.read_line();

            // The first word is the type of data we're processing.
            let token = self.first_token(SEPS).map(<[u8]>::to_vec);
            if let Some(token) = token {
                let handled = parser
                    .parse_block(self, &token)
                    .map_err(|e| e.in_block(&token))?;
                if !handled {
                    // findBlockParse missed: INI_UNKNOWN_TOKEN.
                    return Err(self.error(ErrorKind::UnknownToken));
                }
            }
        }

        Ok(1)
    }

    /// `INI::readLine`: reads one line with the original's byte-level
    /// comment, control-character, truncation and EOF semantics.
    pub fn read_line(&mut self) {
        if self.eof {
            // *m_buffer = 0: the string becomes empty; the C++ leaves the
            // old bytes in place behind the NUL, which no caller observes.
            self.buffer.clear();
            self.cend = 0;
        } else {
            let mut line: Vec<u8> = Vec::with_capacity(MAX_CHARS_PER_LINE);
            let mut p = 0usize;
            while p != MAX_CHARS_PER_LINE {
                if self.next == self.data.len() {
                    self.eof = true;
                    break;
                }
                let c = self.data[self.next];
                self.next += 1;
                if c == b'\n' {
                    break;
                }
                // DEBUG_ASSERTCRASH on '\t' fires only in debug builds;
                // the release path converts it (with every other control
                // character) to a space below.
                if c == b';' {
                    // Comment: NUL-terminate the string but keep reading
                    // and consuming bytes up to the newline.
                    line.push(0);
                } else if (0..32).contains(&c) {
                    line.push(b' ');
                } else {
                    line.push(c);
                }
                p += 1;
            }
            // The trailing NUL (a double write in the C++ after '\n'/EOF).
            line.push(0);
            self.cend = line.iter().position(|c| *c == 0).unwrap_or(line.len() - 1);
            self.buffer = line;
            self.line_num += 1;
        }
        self.tok = 0;
    }

    /// `strtok(m_buffer, seps)`: re-anchors the cursor and returns the
    /// first token of the current line.
    fn first_token(&mut self, seps: &[u8]) -> Option<&[u8]> {
        self.tok = 0;
        self.next_token(seps)
    }

    /// `strtok(nullptr, seps)`: the next token of the current line.
    fn next_token(&mut self, seps: &[u8]) -> Option<&[u8]> {
        let buf = &self.buffer[..self.cend];
        let mut i = self.tok;
        while i < buf.len() && seps.contains(&buf[i]) {
            i += 1;
        }
        if i == buf.len() {
            self.tok = i;
            return None;
        }
        let start = i;
        while i < buf.len() && !seps.contains(&buf[i]) {
            i += 1;
        }
        // strtok saves the position after the separator it consumed, or
        // the string end when the token ran to the end.
        self.tok = if i == buf.len() { i } else { i + 1 };
        Some(&buf[start..i])
    }

    /// `INI::getNextToken(seps)`: the next token, or
    /// `INI_INVALID_DATA` when the line has none left.
    pub fn get_next_token_with(&mut self, seps: &[u8]) -> Result<Vec<u8>, IniError> {
        match self.next_token(seps) {
            Some(t) => Ok(t.to_vec()),
            None => Err(self.error(ErrorKind::InvalidData)),
        }
    }

    /// `INI::getNextToken()`: the next token with the default separators.
    pub fn get_next_token(&mut self) -> Result<Vec<u8>, IniError> {
        self.get_next_token_with(SEPS)
    }

    /// `INI::getNextTokenOrNull(seps)`: the next token or `None`.
    pub fn get_next_token_or_null_with(&mut self, seps: &[u8]) -> Option<Vec<u8>> {
        self.next_token(seps).map(<[u8]>::to_vec)
    }

    /// `INI::getNextTokenOrNull()`: the next token with the default
    /// separators, or `None`.
    pub fn get_next_token_or_null(&mut self) -> Option<Vec<u8>> {
        self.get_next_token_or_null_with(SEPS)
    }

    /// `INI::getNextSubToken(expected)`: expects `Tag:value`, consumes the
    /// `Tag` and returns `value` (colon separators).
    pub fn get_next_sub_token(&mut self, expected: &[u8]) -> Result<Vec<u8>, IniError> {
        let token = self.get_next_token_with(SEPS_COLON)?;
        if !token.eq_ignore_ascii_case(expected) {
            return Err(self.error(ErrorKind::InvalidData));
        }
        self.get_next_token_with(SEPS_COLON)
    }

    /// `INI::getNextAsciiString`: the (deliberately quirky) string reader
    /// whose quoted form glues the continuation token with a space — but
    /// keeps a trailing quote when the closing quote lands inside the
    /// continuation token, and strips it only when nothing follows.
    ///
    /// The original's shared static scratch buffer is per-call state here,
    /// which is the only behaviour it can exhibit (it is fully overwritten
    /// before every use).
    pub fn get_next_ascii_string(&mut self) -> Vec<u8> {
        let Some(token) = self.get_next_token_or_null() else {
            return Vec::new();
        };
        if token.first() != Some(&b'"') {
            return token;
        }
        // Quoted form.
        let mut buff: Vec<u8> = Vec::new();
        if token.len() > 1 {
            strlcpy(&mut buff, &token[1..]);
        }
        match self.next_token(SEPS_QUOTE) {
            Some(cont) => {
                if cont.len() > 1 && cont[1] != b'\t' {
                    strlcat(&mut buff, b" ");
                }
                strlcat(&mut buff, cont);
            }
            None => {
                // Strip one trailing quote (jba, 2/12/2003).
                if buff.last() == Some(&b'"') {
                    buff.pop();
                }
            }
        }
        buff
    }

    /// `INI::getNextQuotedAsciiString`: the "fixed" quoted-string reader.
    ///
    /// Quirks preserved from the original: a token that closes its own
    /// quote is stripped of it; a continuation token is glued with a
    /// space unless it is one character long (or starts with a tab, which
    /// `readLine` can never produce); and a one-character continuation
    /// strips a trailing quote from the buffer instead. When the
    /// continuation token carries the closing quote, the quote stays in
    /// the result (the original only strips it on the no-continuation
    /// path).
    pub fn get_next_quoted_ascii_string(&mut self) -> Result<Vec<u8>, IniError> {
        let Some(token) = self.get_next_token_or_null() else {
            return Ok(Vec::new());
        };
        if token.first() != Some(&b'"') {
            return Ok(token);
        }
        // Quoted form.
        let mut buff: Vec<u8> = Vec::new();
        let mut done = false;
        let str_len = token.len();
        if str_len > 1 {
            strlcpy(&mut buff, &token[1..]);
            // buff[strLen-2] in the original: the last byte copied for an
            // in-token closing quote. Out of range for a truncated buffer
            // (the original reads out of bounds there); treated as "no
            // closing quote".
            let index = str_len as isize - 2;
            if let Some(last) = buff.get(index as usize) {
                if *last == b'"' {
                    buff.truncate(str_len - 2);
                    done = true;
                }
            }
        }
        if !done {
            let cont = self.get_next_token_with(SEPS_QUOTE)?;
            if cont.len() > 1 && cont[1] != b'\t' {
                strlcat(&mut buff, b" ");
                strlcat(&mut buff, &cont);
            } else if buff.last() == Some(&b'"') {
                // The original also probes buff[-1] when the buffer is
                // empty (out of bounds); the empty case reads nothing.
                buff.pop();
            }
        }
        Ok(buff)
    }

    /// `INI::initFromINI`: the field loop of a data block.
    ///
    /// Reads lines until the (case-insensitive) `End` token; dispatches
    /// each field line to `target`; a missing `End` at end-of-input fails
    /// with `INI_MISSING_END_TOKEN`.
    pub fn init_from_ini(&mut self, target: &mut dyn IniFieldTarget) -> Result<(), IniError> {
        let mut done = false;
        while !done {
            self.read_line();

            if let Some(field) = self.first_token(SEPS) {
                let field = field.to_vec();
                if field.eq_ignore_ascii_case(END_TOKEN) {
                    done = true;
                } else {
                    target
                        .parse_field(self, &field)
                        .map_err(|e| e.in_field(&field))?;
                    // Unknown fields are skipped silently: the release
                    // build's DEBUG_CRASH is a no-op.
                }
            }

            if !done && self.eof {
                return Err(self.error(ErrorKind::MissingEndToken));
            }
        }
        Ok(())
    }

    /// `INI::isDeclarationOfType` (static): whether `bufferToCheck` is
    /// `[ws] blockType [ws] blockName [ws] EOL`, both compared
    /// case-insensitively.
    pub fn is_declaration_of_type(
        block_type: &[u8],
        block_name: &[u8],
        buffer_to_check: &[u8],
    ) -> bool {
        if block_type.is_empty() || block_name.is_empty() {
            return false;
        }
        let mut rest = buffer_to_check;
        rest = skip_space(rest);
        if rest.len() < block_type.len()
            || !rest[..block_type.len()].eq_ignore_ascii_case(block_type)
        {
            return false;
        }
        rest = &rest[block_type.len()..];
        if rest.first().is_none_or(|c| !is_space(*c)) {
            return false;
        }
        rest = &rest[1..];
        rest = skip_space(rest);
        if rest.len() < block_name.len()
            || !rest[..block_name.len()].eq_ignore_ascii_case(block_name)
        {
            return false;
        }
        rest = &rest[block_name.len()..];
        rest = skip_space(rest);
        rest.is_empty()
    }

    /// `INI::isEndOfBlock` (static): whether the line is `[ws] End [ws]`,
    /// case-insensitive.
    pub fn is_end_of_block(buffer_to_check: &[u8]) -> bool {
        let mut rest = skip_space(buffer_to_check);
        if rest.len() <= END_TOKEN.len() {
            // The original requires a character after "End" to even test
            // the comparison (strlen > endStringLength).
            return false;
        }
        if !rest[..END_TOKEN.len()].eq_ignore_ascii_case(END_TOKEN) {
            return false;
        }
        rest = &rest[END_TOKEN.len()..];
        rest.iter().all(|c| is_space(*c))
    }

    // ---- scan bridges: token + scan + file/line context ----------------

    /// `INI::scanInt` applied to the next token.
    pub fn scan_next_int(&mut self) -> Result<i32, IniError> {
        let token = self.get_next_token()?;
        scan_int(&token).map_err(|e| self.error(e.into()))
    }

    /// `INI::scanUnsignedInt` applied to the next token.
    pub fn scan_next_unsigned_int(&mut self) -> Result<u32, IniError> {
        let token = self.get_next_token()?;
        scan_unsigned_int(&token).map_err(|e| self.error(e.into()))
    }

    /// `INI::scanReal` applied to the next token.
    pub fn scan_next_real(&mut self) -> Result<f32, IniError> {
        let token = self.get_next_token()?;
        scan_real(&token).map_err(|e| self.error(e.into()))
    }

    /// `INI::scanPercentToReal` applied to the next token.
    pub fn scan_next_percent_to_real(&mut self) -> Result<f32, IniError> {
        let token = self.get_next_token()?;
        scan_percent_to_real(&token).map_err(|e| self.error(e.into()))
    }

    /// `INI::scanBool` applied to the next token.
    pub fn scan_next_bool(&mut self) -> Result<bool, IniError> {
        let token = self.get_next_token()?;
        scan_bool(&token).map_err(|e| self.error(e.into()))
    }

    /// `INI::scanIndexList` applied to the next token.
    pub fn scan_next_index_list(&mut self, names: &[&str]) -> Result<i32, IniError> {
        let token = self.get_next_token()?;
        scan_index_list(&token, names).map_err(|e| self.error(e.into()))
    }

    /// `INI::scanLookupList` applied to the next token.
    pub fn scan_next_lookup_list(&mut self, list: &[(&str, i32)]) -> Result<i32, IniError> {
        let token = self.get_next_token()?;
        scan_lookup_list(&token, list).map_err(|e| self.error(e.into()))
    }

    // ---- the ported parse procs (field value readers) ------------------

    /// `INI::parseUnsignedByte`: an `Int`-scanned value clamped to 0..=255.
    pub fn next_unsigned_byte(&mut self) -> Result<u8, IniError> {
        let value = self.scan_next_int()?;
        if !(0..=255).contains(&value) {
            return Err(self.error(ErrorKind::InvalidData));
        }
        Ok(value as u8)
    }

    /// `INI::parseShort`: an `Int`-scanned value clamped to the `i16` range.
    pub fn next_short(&mut self) -> Result<i16, IniError> {
        let value = self.scan_next_int()?;
        if !(-32768..=32767).contains(&value) {
            return Err(self.error(ErrorKind::InvalidData));
        }
        Ok(value as i16)
    }

    /// `INI::parseUnsignedShort`: an `Int`-scanned value clamped to
    /// 0..=65535.
    pub fn next_unsigned_short(&mut self) -> Result<u16, IniError> {
        let value = self.scan_next_int()?;
        if !(0..=65535).contains(&value) {
            return Err(self.error(ErrorKind::InvalidData));
        }
        Ok(value as u16)
    }

    /// `INI::parsePositiveNonZeroReal`: a real that must be > 0.
    pub fn next_positive_non_zero_real(&mut self) -> Result<f32, IniError> {
        let value = self.scan_next_real()?;
        if value <= 0.0 {
            return Err(self.error(ErrorKind::InvalidData));
        }
        Ok(value)
    }

    /// `INI::parseAngleReal`: degrees converted to radians.
    pub fn next_angle_real(&mut self) -> Result<f32, IniError> {
        let value = self.scan_next_real()?;
        let rads_per_degree = PI / 180.0f32;
        Ok(value * rads_per_degree)
    }

    /// `INI::parseVelocityReal`: dist/sec converted to dist/frame.
    pub fn next_velocity_real(&mut self) -> Result<f32, IniError> {
        let value = self.scan_next_real()?;
        Ok(value * SECONDS_PER_LOGICFRAME)
    }

    /// `INI::parseAccelerationReal`: dist/sec² converted to dist/frame².
    pub fn next_acceleration_real(&mut self) -> Result<f32, IniError> {
        let value = self.scan_next_real()?;
        let sec_per_frame_sqr = SECONDS_PER_LOGICFRAME * SECONDS_PER_LOGICFRAME;
        Ok(value * sec_per_frame_sqr)
    }

    /// `INI::parseAngularVelocityReal`: degrees/sec converted to
    /// radians/frame.
    pub fn next_angular_velocity_real(&mut self) -> Result<f32, IniError> {
        let value = self.scan_next_real()?;
        let rads_per_degree = PI / 180.0f32;
        Ok(value * (SECONDS_PER_LOGICFRAME * rads_per_degree))
    }

    /// `INI::parseDurationReal`: milliseconds converted to (fractional)
    /// frames.
    pub fn next_duration_real(&mut self) -> Result<f32, IniError> {
        let value = self.scan_next_real()?;
        Ok(value * LOGICFRAMES_PER_MSEC)
    }

    /// `INI::parseDurationUnsignedInt`: milliseconds converted to frames,
    /// rounded up (`WWMath::Ceilf`) and truncated through the x86
    /// float-to-unsigned conversion (the i64 step reproduces the
    /// two's-complement truncation for negative and oversized values).
    pub fn next_duration_unsigned_int(&mut self) -> Result<u32, IniError> {
        let value = self.scan_next_unsigned_int()?;
        let frames = (value as f32) * LOGICFRAMES_PER_MSEC;
        Ok(frames.ceil() as i64 as u32)
    }

    /// `INI::parseDurationUnsignedShort`: as [`Self::
    /// next_duration_unsigned_int`] but truncated to 16 bits.
    pub fn next_duration_unsigned_short(&mut self) -> Result<u16, IniError> {
        let value = self.scan_next_unsigned_int()?;
        let frames = (value as f32) * LOGICFRAMES_PER_MSEC;
        Ok(frames.ceil() as i64 as u16)
    }

    /// `INI::parseBitInInt32`: sets or clears `mask` in `word` per a
    /// Yes/No token.
    pub fn next_bit_in_int32(&mut self, mask: u32, word: &mut u32) -> Result<(), IniError> {
        if self.scan_next_bool()? {
            *word |= mask;
        } else {
            *word &= !mask;
        }
        Ok(())
    }

    /// `INI::parseBitString32`: reads the remaining tokens on the line as
    /// flag names (`NONE`, plain names that reset first, or `+name` /
    /// `-name` relative edits).
    ///
    /// `bits` carries the field's current value in, and the result out
    /// (the `+`/`-` forms read it; the plain form resets it).
    pub fn next_bit_string32(&mut self, names: &[&str], bits: &mut u32) -> Result<(), IniError> {
        if names.is_empty() {
            return Err(self.error(ErrorKind::InvalidNameList));
        }
        let mut found_normal = false;
        let mut found_add_or_sub = false;

        while let Some(token) = self.get_next_token_or_null() {
            if token.eq_ignore_ascii_case(b"NONE") {
                if found_normal || found_add_or_sub {
                    return Err(self.error(ErrorKind::InvalidNameList));
                }
                *bits = 0;
                break;
            }
            if token.first() == Some(&b'+') {
                if found_normal {
                    return Err(self.error(ErrorKind::InvalidNameList));
                }
                let bit_index =
                    scan_index_list(&token[1..], names).map_err(|e| self.error(e.into()))?;
                *bits |= 1 << bit_index;
                found_add_or_sub = true;
            } else if token.first() == Some(&b'-') {
                if found_normal {
                    return Err(self.error(ErrorKind::InvalidNameList));
                }
                let bit_index =
                    scan_index_list(&token[1..], names).map_err(|e| self.error(e.into()))?;
                *bits &= !(1 << bit_index);
                found_add_or_sub = true;
            } else {
                if found_add_or_sub {
                    return Err(self.error(ErrorKind::InvalidNameList));
                }
                if !found_normal {
                    *bits = 0;
                }
                let bit_index = scan_index_list(&token, names).map_err(|e| self.error(e.into()))?;
                *bits |= 1 << bit_index;
                found_normal = true;
            }
        }
        Ok(())
    }

    /// `INI::parseBitString8`: [`Self::next_bit_string32`] with the
    /// 8-bit range check of the original.
    pub fn next_bit_string8(&mut self, names: &[&str], byte: &mut u8) -> Result<(), IniError> {
        let mut tmp = u32::from(*byte);
        self.next_bit_string32(names, &mut tmp)?;
        if tmp & 0xffff_ff00 != 0 {
            return Err(self.error(ErrorKind::InvalidData));
        }
        *byte = tmp as u8;
        Ok(())
    }

    /// `INI::parsePercentToReal`: a percent token (the `getSepsPercent`
    /// separators, so a bare `%` splits off) scanned into 0..=1.
    pub fn next_percent_to_real(&mut self) -> Result<f32, IniError> {
        let token = self.get_next_token_with(SEPS_PERCENT)?;
        scan_percent_to_real(&token).map_err(|e| self.error(e.into()))
    }
}

/// `strlcpy(dst, src, sizeof(dst))` with the INI line buffer size.
fn strlcpy(dst: &mut Vec<u8>, src: &[u8]) {
    dst.clear();
    dst.extend_from_slice(&src[..src.len().min(MAX_CHARS_PER_LINE - 1)]);
}

/// `strlcat(dst, src, sizeof(dst))` with the INI line buffer size.
fn strlcat(dst: &mut Vec<u8>, src: &[u8]) {
    let room = MAX_CHARS_PER_LINE - 1 - dst.len().min(MAX_CHARS_PER_LINE - 1);
    dst.extend_from_slice(&src[..src.len().min(room)]);
}

/// `isspace` on the INI byte vocabulary (the `std::isspace` the original
/// calls with unsigned chars: space, `\t`, `\n`, `\v`, `\f`, `\r`).
const fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

fn skip_space(mut rest: &[u8]) -> &[u8] {
    while let Some((&first, tail)) = rest.split_first() {
        if !is_space(first) {
            break;
        }
        rest = tail;
    }
    rest
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A no-op block parser for engine-level tests.
    struct NoBlocks;
    impl BlockParser for NoBlocks {
        fn parse_block(&mut self, _ini: &mut Ini, _block: &[u8]) -> Result<bool, IniError> {
            Ok(false)
        }
    }

    #[test]
    fn read_line_converts_comments_and_controls() {
        let mut ini = Ini::new();
        ini.data = b"Foo ; comment\x01\x02 Bar\r\nNext\r\n".to_vec();
        ini.read_line();
        assert_eq!(&ini.buffer[..ini.cend], b"Foo ");
        ini.read_line();
        // The \r became a space before the \n ended the line.
        assert_eq!(&ini.buffer[..ini.cend], b"Next ");
        assert!(!ini.eof); // the trailing \r\n produced the synthetic line
        ini.read_line();
        assert_eq!(&ini.buffer[..ini.cend], b"");
        assert!(ini.eof);
        // The synthetic EOF line still counts (lineNum++ in the else
        // branch), but repeated reads do not.
        assert_eq!(ini.line_num, 3);
        ini.read_line();
        assert_eq!(ini.line_num, 3);
    }

    #[test]
    fn read_line_truncates_at_1028_raw_bytes() {
        let mut ini = Ini::new();
        let mut data = vec![b'A'; 5000];
        data.push(b'\n');
        data.extend_from_slice(b"tail\n");
        ini.data = data;
        ini.read_line();
        assert_eq!(ini.cend, MAX_CHARS_PER_LINE);
        // The cap applies to every readLine call: the remainder of the
        // over-long line continues on the next reads, in 1028-byte
        // chunks (5000 bytes = 4 + 916).
        ini.read_line();
        assert_eq!(ini.cend, MAX_CHARS_PER_LINE);
        ini.read_line();
        assert_eq!(ini.cend, MAX_CHARS_PER_LINE);
        ini.read_line();
        assert_eq!(ini.cend, MAX_CHARS_PER_LINE);
        ini.read_line();
        assert_eq!(
            &ini.buffer[..ini.cend],
            &vec![b'A'; 5000 - 4 * MAX_CHARS_PER_LINE]
        );
        // "tail" follows after the newline.
        ini.read_line();
        assert_eq!(&ini.buffer[..ini.cend], b"tail");
    }

    #[test]
    fn read_line_without_trailing_newline_sets_eof_with_content() {
        let mut ini = Ini::new();
        ini.data = b"Last".to_vec();
        ini.read_line();
        assert_eq!(&ini.buffer[..ini.cend], b"Last");
        assert!(ini.eof);
        assert_eq!(ini.line_num, 1);
    }

    #[test]
    fn tokenizer_matches_strtok_semantics() {
        let mut ini = Ini::new();
        ini.data = b"  Alpha = Beta  =  Gamma  \n".to_vec();
        ini.read_line();
        assert_eq!(ini.first_token(SEPS), Some(&b"Alpha"[..]));
        assert_eq!(ini.next_token(SEPS), Some(&b"Beta"[..]));
        assert_eq!(ini.next_token(SEPS), Some(&b"Gamma"[..]));
        assert_eq!(ini.next_token(SEPS), None);
    }

    #[test]
    fn quote_separators_exclude_the_space() {
        let mut ini = Ini::new();
        ini.data = b"Name = \"hello world\" more\n".to_vec();
        ini.read_line();
        assert_eq!(ini.first_token(SEPS), Some(&b"Name"[..]));
        assert_eq!(ini.next_token(SEPS), Some(&b"\"hello"[..]));
        // The quote separator set does not contain the space, but the
        // space before "world" was already consumed as the separator
        // that ended the previous token.
        assert_eq!(ini.next_token(SEPS_QUOTE), Some(&b"world"[..]));
        // The space after the closing quote is NOT a quote separator, so
        // it rides inside the next token.
        assert_eq!(ini.next_token(SEPS_QUOTE), Some(&b" more"[..]));
    }

    #[test]
    fn quoted_string_readers_preserve_their_quirks() {
        let mut ini = Ini::new();
        ini.data = b"Name = \"hello world\" more\n".to_vec();
        ini.read_line();
        ini.first_token(SEPS);
        // getNextAsciiString: the continuation token is glued with a
        // single space (the space ended the previous token).
        let value = ini.get_next_ascii_string();
        assert_eq!(value, b"hello world");

        let mut ini = Ini::new();
        ini.data = b"Name = \"quoted\"\n".to_vec();
        ini.read_line();
        ini.first_token(SEPS);
        // getNextQuotedAsciiString: self-closing quote token.
        let value = ini.get_next_quoted_ascii_string().unwrap();
        assert_eq!(value, b"quoted");

        let mut ini = Ini::new();
        ini.data = b"Name = \"trailing quote\" tail\n".to_vec();
        ini.read_line();
        ini.first_token(SEPS);
        // getNextAsciiString: the continuation token ends AT the closing
        // quote (it is a quote separator), so the result carries no
        // quote; "tail" stays unread on the line.
        let value = ini.get_next_ascii_string();
        assert_eq!(value, b"trailing quote");

        let mut ini = Ini::new();
        ini.data = b"Name = plain\n".to_vec();
        ini.read_line();
        ini.first_token(SEPS);
        assert_eq!(ini.get_next_ascii_string(), b"plain");

        let mut ini = Ini::new();
        ini.data = b"Name =\n".to_vec();
        ini.read_line();
        ini.first_token(SEPS);
        // No token: empty string, not an error.
        assert_eq!(ini.get_next_ascii_string(), b"");
        assert_eq!(ini.get_next_quoted_ascii_string().unwrap(), b"");
    }

    #[test]
    fn end_of_block_and_declaration_checks() {
        // The original requires a character after "End" to even attempt
        // the comparison, so a bare "End" is NOT an end-of-block here
        // (initFromINIMulti's separate stricmp check is what accepts it).
        assert!(Ini::is_end_of_block(b" End "));
        assert!(!Ini::is_end_of_block(b"eNd"));
        assert!(Ini::is_end_of_block(b"eNd "));
        assert!(!Ini::is_end_of_block(b"Ends"));
        assert!(!Ini::is_end_of_block(b"End x"));
        assert!(!Ini::is_end_of_block(b""));

        assert!(Ini::is_declaration_of_type(
            b"Armor",
            b"TemplateA",
            b"  Armor   TemplateA  "
        ));
        assert!(Ini::is_declaration_of_type(
            b"Armor",
            b"TemplateA",
            b"armor templatea"
        ));
        assert!(!Ini::is_declaration_of_type(
            b"Armor",
            b"TemplateA",
            b"ArmorTemplateA"
        ));
        assert!(!Ini::is_declaration_of_type(
            b"Armor",
            b"TemplateA",
            b"Armor TemplateA Extra"
        ));
        assert!(!Ini::is_declaration_of_type(
            b"Armor",
            b"TemplateA",
            b"ArmorTemplate"
        ));
    }

    #[test]
    fn duration_conversions_round_up() {
        let mut ini = Ini::new();
        ini.data = b"1000 1001 999 0\n".to_vec();
        ini.read_line();
        assert_eq!(ini.next_duration_unsigned_int().unwrap(), 30);
        assert_eq!(ini.next_duration_unsigned_int().unwrap(), 31);
        assert_eq!(ini.next_duration_unsigned_int().unwrap(), 30);
        assert_eq!(ini.next_duration_unsigned_int().unwrap(), 0);
    }

    #[test]
    fn bit_strings_match_the_original_state_machine() {
        let names = ["GROUND", "WATER", "CLIFF", "AIR", "RUBBLE"];

        let mut ini = Ini::new();
        ini.data = b"GROUND WATER\n".to_vec();
        ini.read_line();
        let mut bits = 0u32;
        ini.next_bit_string32(&names, &mut bits).unwrap();
        assert_eq!(bits, 0b0011);

        // NONE resets and stops reading: the WATER behind it is ignored.
        let mut ini = Ini::new();
        ini.data = b"NONE WATER\n".to_vec();
        ini.read_line();
        let mut bits = 0xffff_ffffu32;
        ini.next_bit_string32(&names, &mut bits).unwrap();
        assert_eq!(bits, 0);

        // Relative edits read the current value.
        let mut ini = Ini::new();
        ini.data = b"-WATER +AIR\n".to_vec();
        ini.read_line();
        let mut bits = 0b0111u32;
        ini.next_bit_string32(&names, &mut bits).unwrap();
        assert_eq!(bits, 0b1101);

        // Mixing forms is an error.
        let mut ini = Ini::new();
        ini.data = b"GROUND +WATER\n".to_vec();
        ini.read_line();
        let mut bits = 0u32;
        assert_eq!(
            ini.next_bit_string32(&names, &mut bits).unwrap_err().kind,
            ErrorKind::InvalidNameList
        );
    }

    #[test]
    fn unknown_block_fails_the_load() {
        let mut ini = Ini::new();
        let mut parser = NoBlocks;
        let error = ini
            .load(
                "x.ini",
                b"Mystery Block\n".to_vec(),
                LoadType::Overwrite,
                &mut parser,
            )
            .unwrap_err();
        assert_eq!(error.kind, ErrorKind::UnknownToken);
        assert_eq!(error.line, 1);
    }
}
