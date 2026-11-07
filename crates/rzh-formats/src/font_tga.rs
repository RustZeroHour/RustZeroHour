// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Ported from the Zero Hour source release (baseline 571a7c5):
//   Core/Libraries/Source/WWVegas/WWLib/TARGA.cpp
//     (Targa::Open's footer-sniffing read walk, the flags-Load's
//     allocation half, the buffer-Load's colormap/image reads, the
//     image-type switch, DecodeImage's RLE packets, XFlip/YFlip and
//     SetPalette — the reader half, driven in the font's load shape)
//
//! The Targa reader in the font's load shape — a private companion of
//! the [`font`](crate::font) module.
//!
//! The public [`tga`](crate::tga) module transcribes the Targa class in
//! the library's default shape (the palette read into the class's own
//! buffer, `TGAF_IMAGE | TGAF_PAL`, `invert_image = true` — the shape
//! `TgaFile::parse` exposes). The font path drives a different one:
//! `SetPalette` installs the *caller's* buffer, the load runs
//! `TGAF_IMAGE` only with `invert_image = false`, and the texture
//! loader XORs the Y-origin bit before it all (transcribed at the font
//! module's call site). This module re-walks the same reader in that
//! shape, verbatim, keeping the font module self-contained.
//!
//! The deviations match `tga`'s transcription notes: the RLE decoder
//! refuses an over-supplied stream where the original would write past
//! its image buffer (the corpora never supply one), and the palette
//! read for `CMapStart != 0` walks the caller's buffer past its start
//! in the original — the port reads the colormap bytes into a sink
//! instead (the stream position matches; the palette keeps its zeros;
//! every real font file keeps `CMapStart = 0`).

/// The Targa image types the load's switch routes (the mono-encoded
/// type is the reader's missing case — the `_` arm's NOTSUPPORTED).
pub const TGA_CMAPPED: u8 = 1;
pub const TGA_TRUECOLOR: u8 = 2;
pub const TGA_MONO: u8 = 3;
pub const TGA_CMAPPED_ENCODED: u8 = 9;
pub const TGA_TRUECOLOR_ENCODED: u8 = 10;

/// The origin bits of the image descriptor.
pub const TGAIDF_XORIGIN: u8 = 1 << 4;
pub const TGAIDF_YORIGIN: u8 = 1 << 5;

/// The Targa error codes the font verdicts carry (`TGAERR_*`; the open
/// failure surfaces as `-1` through the error channel).
pub const TGAERR_READ: i32 = -2;
pub const TGAERR_NOTSUPPORTED: i32 = -6;

/// The Targa 2.0 footer signature.
pub const TGA2_SIGNATURE: &[u8; 16] = b"TRUEVISION-XFILE";

/// `TGA_BytesPerPixel`.
fn tga_bytes_per_pixel(depth: u8) -> usize {
    (depth as usize + 7) >> 3
}

/// The Targa header (the 18-byte on-disk layout; the offset fields
/// are skipped by the reader like the original's walk does).
#[derive(Debug, Clone, Copy, Default)]
pub struct FontTgaHeader {
    pub id_length: u8,
    pub cmap_type: u8,
    pub image_type: u8,
    pub cmap_start: i16,
    pub cmap_length: i16,
    pub cmap_depth: u8,
    pub width: i16,
    pub height: i16,
    pub pixel_depth: u8,
    pub image_descriptor: u8,
}

/// The image allocation ceiling: a declared image larger than this
/// models the original's `malloc` failure (its buffer stays null and
/// the walk crashes on the null deref — see the font module's crash
/// verdicts; every real font image is far below).
pub(crate) const MAX_IMAGE_ALLOC: usize = 16 * 1024 * 1024;

/// The Targa class in the font's load shape (see the module docs).
pub(crate) struct FontTarga<'a> {
    pub header: FontTgaHeader,
    src: &'a [u8],
    pos: usize,
    /// The loaded image (the flags-Load's allocation; empty when the
    /// declared size is zero — the original leaves its buffer null and
    /// the load still succeeds).
    pub image: Vec<u8>,
}

impl<'a> FontTarga<'a> {
    pub(crate) fn new(src: &'a [u8]) -> Self {
        Self {
            header: FontTgaHeader::default(),
            src,
            pos: 0,
            image: Vec::new(),
        }
    }

    /// `SrcFile::read` — the byte source's read (positions past EOF
    /// read as nothing).
    fn read(&mut self, out: &mut [u8]) -> usize {
        let avail = self.src.len().saturating_sub(self.pos);
        let n = avail.min(out.len());
        out[..n].copy_from_slice(&self.src[self.pos..self.pos + n]);
        self.pos += n;
        n
    }

    /// `SrcFile::seek` — negative targets fail, positions past EOF are
    /// allowed.
    fn seek(&mut self, offset: i64, origin: i32) -> bool {
        let target = match origin {
            0 => offset,                         // SEEK_SET
            1 => self.pos as i64 + offset,       // SEEK_CUR
            _ => self.src.len() as i64 + offset, // SEEK_END
        };
        if target < 0 || target > self.src.len() as i64 {
            return false;
        }
        self.pos = target as usize;
        true
    }

    /// `Targa::Open` (the read-mode walk): the last 26 bytes are read
    /// and only the signature decides anything (the TGA2 extension
    /// bytes an extension offset would gate are dead under
    /// `TGAF_IMAGE` — the font corpora carry none and the original
    /// discards the footer view); the header follows, then the ID
    /// field skip. Errors close the file (a no-op for the model).
    pub(crate) fn open_read(&mut self) -> Result<(), i32> {
        // Check for 2.0 targa file by loading the footer
        if !self.seek(-26, 2) {
            return Err(TGAERR_READ);
        }
        let mut footer = [0u8; 26];
        if self.read(&mut footer) != 26 {
            return Err(TGAERR_READ);
        }
        if &footer[8..24] == TGA2_SIGNATURE.as_slice() {
            let extension = i32::from_le_bytes([footer[0], footer[1], footer[2], footer[3]]);
            if extension != 0 {
                // the TGA2 extension read the flags would gate; the
                // font path never sets the flag, and no corpus carries
                // an extension — the read is dead (stated)
            }
        }

        // Read in header (the seek-to-start never fails)
        self.pos = 0;
        let mut hdr = [0u8; 18];
        if self.read(&mut hdr) != 18 {
            return Err(TGAERR_READ);
        }
        self.header = FontTgaHeader {
            id_length: hdr[0],
            cmap_type: hdr[1],
            image_type: hdr[2],
            cmap_start: i16::from_le_bytes([hdr[3], hdr[4]]),
            cmap_length: i16::from_le_bytes([hdr[5], hdr[6]]),
            cmap_depth: hdr[7],
            // hdr[8..12] are the x/y offsets — the reader never uses them
            width: i16::from_le_bytes([hdr[12], hdr[13]]),
            height: i16::from_le_bytes([hdr[14], hdr[15]]),
            pixel_depth: hdr[16],
            image_descriptor: hdr[17],
        };

        // Skip the ID field
        if self.header.id_length != 0 && !self.seek(self.header.id_length as i64, 1) {
            return Err(TGAERR_READ);
        }
        Ok(())
    }

    /// `Targa::Load(name, TGAF_IMAGE, false)` — the allocation half and
    /// the buffer-Load walk: the colormap is read into the caller's
    /// buffer (when the file has one), the image into ours, the RLE
    /// types decode, and the origin bits normalize (XFlip/YFlip — the
    /// descriptor was already XORed by the caller).
    pub(crate) fn load_image(&mut self, palette: &mut [u8; 256 * 4]) -> Result<(), i32> {
        // the allocation half: the image buffer (the palette stays the
        // caller's; the negative-size and over-ceiling malloc failures
        // of the original leave its buffer null and the load succeeds
        // — the font module turns the null image into the crash
        // verdict)
        let size = (self.header.width as usize)
            .saturating_mul(self.header.height as usize)
            .saturating_mul(tga_bytes_per_pixel(self.header.pixel_depth));
        if self.header.width > 0 && self.header.height > 0 && size <= MAX_IMAGE_ALLOC {
            self.image = vec![0u8; size];
        } else {
            self.image.clear();
        }

        // the buffer-Load walk (the file is already open past the ID
        // field; the original's re-open short-cuits)
        if self.header.cmap_type == 1 {
            let depth = (self.header.cmap_depth >> 3) as usize;
            // the original's int arithmetic (a negative CMapLength
            // skips the read and seeks by the negative size)
            let size = self.header.cmap_length as i64 * depth as i64;
            if self.header.cmap_length > 0 {
                // Adjust palette to the starting color entry (the
                // original walks the caller's buffer past its start for
                // CMapStart != 0 — see the module docs)
                let start = self.header.cmap_start.max(0) as usize * depth;
                let size = size as usize;
                if self.header.cmap_start >= 0 && start.saturating_add(size) <= palette.len() {
                    if self.read(&mut palette[start..start + size]) != size {
                        return Err(TGAERR_READ);
                    }
                } else {
                    // the overflow read: the original writes past the
                    // buffer; the port reads into a sink (the stream
                    // position matches)
                    let mut sink = vec![0u8; size];
                    if self.read(&mut sink) != size {
                        return Err(TGAERR_READ);
                    }
                }
            } else if !self.seek(size, 1) {
                return Err(TGAERR_READ);
            }
        }

        if !self.image.is_empty() {
            let depth = tga_bytes_per_pixel(self.header.pixel_depth);
            let size = self.image.len();
            match self.header.image_type {
                TGA_CMAPPED | TGA_MONO => {
                    self.read_into_image(0, size)?;
                }
                TGA_TRUECOLOR => {
                    self.read_into_image(0, size)?;
                    // invert_image is false on the font path — the
                    // within-pixel byte swap never runs
                }
                TGA_CMAPPED_ENCODED | TGA_TRUECOLOR_ENCODED => {
                    self.decode_image()?;
                }
                _ => return Err(TGAERR_NOTSUPPORTED),
            }

            // the origin normalization (the descriptor was already
            // XORed by the caller)
            if self.header.image_descriptor & TGAIDF_XORIGIN != 0 {
                self.xflip(depth);
                self.header.image_descriptor &= !TGAIDF_XORIGIN;
            }
            if self.header.image_descriptor & TGAIDF_YORIGIN != 0 {
                self.yflip(depth);
                self.header.image_descriptor &= !TGAIDF_YORIGIN;
            }
        }
        Ok(())
    }

    /// Reads `size` bytes into the image at `at` (a helper for the two
    /// borrows — `self.read` advances the position while `self.image`
    /// receives).
    fn read_into_image(&mut self, at: usize, size: usize) -> Result<(), i32> {
        let mut tmp = vec![0u8; size];
        let n = self.read(&mut tmp);
        if n != size {
            return Err(TGAERR_READ);
        }
        self.image[at..at + size].copy_from_slice(&tmp);
        Ok(())
    }

    /// `Targa::DecodeImage`, verbatim walk (the over-supplied stream
    /// the original would write past its image buffer is refused —
    /// see the module docs).
    fn decode_image(&mut self) -> Result<(), i32> {
        let depth = tga_bytes_per_pixel(self.header.pixel_depth);
        let mut pixel_count = self.header.width as i64 * self.header.height as i64;
        let mut image_pos = 0usize;
        while pixel_count > 0 {
            let mut count = [0u8; 1];
            if self.read(&mut count) != 1 {
                return Err(TGAERR_READ);
            }
            let mut count = count[0];
            if count & 0x80 != 0 {
                count &= 0x7f;
                count += 1;
                let mut color = [0u8; 8];
                if self.read(&mut color[..depth]) != depth {
                    return Err(TGAERR_READ);
                }
                // the color pixel itself, then the repeats
                for _ in 0..count {
                    if image_pos + depth > self.image.len() {
                        return Err(TGAERR_READ);
                    }
                    self.image[image_pos..image_pos + depth].copy_from_slice(&color[..depth]);
                    image_pos += depth;
                }
            } else {
                count += 1;
                let size = count as usize * depth;
                if image_pos + size > self.image.len() {
                    return Err(TGAERR_READ);
                }
                self.read_into_image(image_pos, size)?;
                image_pos += size;
            }
            pixel_count -= count as i64;
        }
        Ok(())
    }

    /// `Targa::XFlip`.
    fn xflip(&mut self, depth: usize) {
        let width = self.header.width as usize;
        let height = self.header.height as usize;
        for y in 0..height {
            let row = y * width * depth;
            for x in 0..width / 2 {
                let a = row + x * depth;
                let b = row + (width - 1 - x) * depth;
                for d in 0..depth {
                    self.image.swap(a + d, b + d);
                }
            }
        }
    }

    /// `Targa::YFlip`.
    fn yflip(&mut self, depth: usize) {
        let width = self.header.width as usize;
        let height = self.header.height as usize;
        if height == 0 {
            return;
        }
        let stride = width * depth;
        let mut top = 0usize;
        let mut bottom = stride * (height - 1);
        for _ in 0..height / 2 {
            for i in 0..stride {
                self.image.swap(top + i, bottom + i);
            }
            top += stride;
            bottom -= stride;
        }
    }
}
