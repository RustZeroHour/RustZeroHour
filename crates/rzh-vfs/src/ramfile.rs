// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Ported from Core/GameEngine/Source/Common/System/RAMFile.cpp @ 571a7c5,
// Zero Hour configuration. The original carries the notice: Westwood
// Studios Pacific. Copyright(C) 2001. All Rights Reserved.
//
//! The in-memory sub-file (`RAMFile`).
//!
//! `ArchiveFile::openFile` returns one of these for every non-streaming
//! open: the whole sub-file is read into memory once (from the archive's
//! still-open backing file), then served from RAM. The INI and chunk
//! loaders read exclusively through this kind of handle, so the exact
//! seek clamping and the [`RamFile::scan_int`] family are parity surface.
//!
//! # Quirks preserved (they are behaviour)
//!
//! - [`RamFile::seek`] clamps to `[0, size]` and *returns the clamped
//!   position* (the C++ `RAMFile::seek` does the same; callers rely on
//!   it, e.g. `StreamingArchiveFile`'s open probe).
//! - [`RamFile::read`] with a buffer shorter than the request clamps to
//!   the number of bytes left, exactly like the C++.
//! - [`RamFile::scan_real`] collects at most one `.` into the number and
//!   parses with the C library's `atof`; the port parses with Rust's
//!   `f32::from_str` (same result for every shape the scanner can emit:
//!   `[-]digits[.digits]` with a possible lone leading `.`; a bare `-`
//!   or `.` yields 0.0 in both).
//! - `next_line` keeps the terminating `\n` in the output (like the
//!   C++) and stops at EOF.
//!
//! # Not ported
//!
//! `RAMFile::readChar`/`readWideChar`/`write*` — the C stubs return
//! `EOF`/`-1` unconditionally; the Rust type is read-only and has no
//! such methods.
//!
//! # Parity level
//!
//! **P0 exact** for reads/seeks/scans (golden-pinned where observable).

/// An in-memory file: a byte buffer plus a read position (`RAMFile`).
#[derive(Clone, Debug, Default)]
pub struct RamFile {
    data: Vec<u8>,
    pos: usize,
}

impl RamFile {
    /// Wraps whole bytes read from an archive (the `openFromArchive`
    /// read-the-whole-thing path).
    #[must_use]
    pub fn from_bytes(data: Vec<u8>) -> Self {
        Self { data, pos: 0 }
    }

    /// The whole buffer.
    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// `size()`: the buffer length.
    #[must_use]
    pub fn size(&self) -> usize {
        self.data.len()
    }

    /// The current position (`tell`).
    #[must_use]
    pub fn pos(&self) -> usize {
        self.pos
    }

    /// `read(buffer, bytes)`: copies up to `out.len()` bytes (clamped to
    /// what is left) and advances the position. Returns the byte count.
    pub fn read(&mut self, out: &mut [u8]) -> usize {
        let bytes = out.len().min(self.data.len().saturating_sub(self.pos));
        out[..bytes].copy_from_slice(&self.data[self.pos..self.pos + bytes]);
        self.pos += bytes;
        bytes
    }

    /// Reads the remaining bytes from the current position.
    #[must_use]
    pub fn read_remaining(&self) -> &[u8] {
        &self.data[self.pos.min(self.data.len())..]
    }

    /// `seek(pos, mode)`: moves the position, clamped to `[0, size]`,
    /// returning the new position (the C++ returns it; callers probe
    /// with it).
    pub fn seek(&mut self, pos: i64, mode: SeekMode) -> usize {
        let new_pos = match mode {
            SeekMode::Start => pos,
            SeekMode::Current => i64::try_from(self.pos).unwrap_or(i64::MAX) + pos,
            // the C++ asserts pos <= 0 here; it clamps anything anyway
            SeekMode::End => i64::try_from(self.data.len()).unwrap_or(i64::MAX) + pos,
        };
        let clamped = new_pos.clamp(0, i64::try_from(self.data.len()).unwrap_or(i64::MAX));
        self.pos = usize::try_from(clamped).unwrap_or(0);
        self.pos
    }

    /// `scanInt`: skips to the first digit or `-`, collects digits and
    /// parses them. Returns `None` at EOF without a number.
    ///
    /// Like the C++ `atoi` on the collected text, a lone `-` parses as 0.
    pub fn scan_int(&mut self) -> Option<i32> {
        while self.pos < self.data.len()
            && !self.data[self.pos].is_ascii_digit()
            && self.data[self.pos] != b'-'
        {
            self.pos += 1;
        }

        if self.pos >= self.data.len() {
            self.pos = self.data.len();
            return None;
        }

        let mut text = Vec::new();
        loop {
            text.push(self.data[self.pos]);
            self.pos += 1;
            if self.pos >= self.data.len() || !self.data[self.pos].is_ascii_digit() {
                break;
            }
        }

        let text = String::from_utf8_lossy(&text);
        Some(text.parse::<i32>().unwrap_or(0))
    }

    /// `scanReal`: skips to the first digit, `-` or `.`, collects digits
    /// plus at most one `.`, and parses them. Returns `None` at EOF
    /// without a number.
    pub fn scan_real(&mut self) -> Option<f32> {
        while self.pos < self.data.len()
            && !self.data[self.pos].is_ascii_digit()
            && self.data[self.pos] != b'-'
            && self.data[self.pos] != b'.'
        {
            self.pos += 1;
        }

        if self.pos >= self.data.len() {
            self.pos = self.data.len();
            return None;
        }

        let mut text = Vec::new();
        let mut saw_dot = false;
        loop {
            let ch = self.data[self.pos];
            text.push(ch);
            if ch == b'.' {
                saw_dot = true;
            }
            self.pos += 1;
            if self.pos >= self.data.len() {
                break;
            }
            let next = self.data[self.pos];
            if !(next.is_ascii_digit() || (next == b'.' && !saw_dot)) {
                break;
            }
        }

        let text = String::from_utf8_lossy(&text);
        Some(text.parse::<f32>().unwrap_or(0.0))
    }

    /// `scanString`: skips whitespace, collects one whitespace-free run.
    /// Returns `None` at EOF.
    pub fn scan_string(&mut self) -> Option<String> {
        while self.pos < self.data.len() && is_c_space(self.data[self.pos]) {
            self.pos += 1;
        }

        if self.pos >= self.data.len() {
            self.pos = self.data.len();
            return None;
        }

        let mut text = Vec::new();
        loop {
            text.push(self.data[self.pos]);
            self.pos += 1;
            if self.pos >= self.data.len() || is_c_space(self.data[self.pos]) {
                break;
            }
        }

        Some(String::from_utf8_lossy(&text).into_owned())
    }

    /// `nextLine(buf, bufSize)`: collects bytes until (and including) the
    /// next `\n` and advances past it; at EOF returns what is left. The
    /// returned text keeps the `\n`.
    pub fn next_line(&mut self) -> String {
        let mut text = Vec::new();
        while self.pos < self.data.len() && self.data[self.pos] != b'\n' {
            text.push(self.data[self.pos]);
            self.pos += 1;
        }
        // we got to the new-line character, now go one past it
        if self.pos < self.data.len() {
            text.push(self.data[self.pos]);
            self.pos += 1;
        }
        String::from_utf8_lossy(&text).into_owned()
    }

    /// `readEntireAndClose`: hands out the whole buffer (the position
    /// and the buffer are emptied, mirroring the C++ ownership transfer).
    pub fn into_inner(self) -> Vec<u8> {
        self.data
    }
}

/// `File::seekMode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeekMode {
    /// Position is relative to the start of the file.
    Start,
    /// Position is relative to the current file position.
    Current,
    /// Position is relative to the end of the file (should be <= 0).
    End,
}

/// `isspace` in the C locale (Rust's `is_ascii_whitespace` misses the
/// vertical tab).
fn is_c_space(ch: u8) -> bool {
    matches!(ch, b' ' | b'\t' | b'\n' | b'\x0B' | b'\x0C' | b'\r')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_clamps_and_advances() {
        let mut f = RamFile::from_bytes(b"0123456789".to_vec());
        let mut out = [0u8; 4];
        assert_eq!(f.read(&mut out), 4);
        assert_eq!(&out, b"0123");
        let mut big = [0u8; 32];
        assert_eq!(f.read(&mut big), 6);
        assert_eq!(&big[..6], b"456789");
        assert_eq!(f.read(&mut big), 0);
    }

    #[test]
    fn seek_clamps_like_the_c() {
        let mut f = RamFile::from_bytes(b"abcdef".to_vec());
        assert_eq!(f.seek(-4, SeekMode::Current), 0); // clamped to 0
        assert_eq!(f.seek(3, SeekMode::Start), 3);
        assert_eq!(f.seek(-2, SeekMode::End), 4);
        assert_eq!(f.seek(10, SeekMode::Start), 6); // clamped to size
        assert_eq!(f.seek(1, SeekMode::Start), 1);
    }

    #[test]
    fn scan_ints() {
        let mut f = RamFile::from_bytes(b"hp = 100, armor -42 done".to_vec());
        assert_eq!(f.scan_int(), Some(100));
        assert_eq!(f.scan_int(), Some(-42));
        assert_eq!(f.scan_int(), None); // at EOF
    }

    #[test]
    fn scan_int_stops_right_after_the_dash_for_double_dashes() {
        // the scanner stops at the first '-' and collects only it: the
        // C++ atoi("--42") path can never happen (text is just "-"),
        // and atoi("-") is 0
        let mut f = RamFile::from_bytes(b"armor --42".to_vec());
        assert_eq!(f.scan_int(), Some(0));
    }

    #[test]
    fn scan_reals() {
        let mut f = RamFile::from_bytes(b"rate 2.5, junk 1.2.3, neg -.5".to_vec());
        assert_eq!(f.scan_real(), Some(2.5));
        // the second dot is not collected; the next scan starts right at
        // it and reads ".3", then "-.5" follows — the scanner has no
        // sign-after-dot notion, exactly like the C++
        assert_eq!(f.scan_real(), Some(1.2));
        assert_eq!(f.scan_real(), Some(0.3));
        assert_eq!(f.scan_real(), Some(-0.5));
        assert_eq!(f.scan_real(), None);
    }

    #[test]
    fn scan_real_lone_dot_is_zero() {
        let mut f = RamFile::from_bytes(b"= . ;".to_vec());
        assert_eq!(f.scan_real(), Some(0.0));
    }

    #[test]
    fn scan_strings_and_lines() {
        let mut f = RamFile::from_bytes(b"  alpha\tbeta\ngamma ".to_vec());
        assert_eq!(f.scan_string().unwrap(), "alpha");
        assert_eq!(f.scan_string().unwrap(), "beta");
        assert_eq!(f.next_line(), "\n");
        assert_eq!(f.next_line(), "gamma ");
        assert_eq!(f.next_line(), "");
    }
}
