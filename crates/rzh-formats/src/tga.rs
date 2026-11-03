// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Ported from the Zero Hour source release (baseline 571a7c5):
//   Core/Libraries/Source/WWVegas/WWLib/TARGA.{h,cpp}  (the Targa image-file
//     class: Open, Load, Save, DecodeImage, EncodeImage, XFlip, YFlip,
//     InvertImage)
// See the module docs for the format layout and the documented differences
// from the C++ (out-of-bounds reads become errors; the final-iteration
// overread of the encoder is pinned to its deterministic majority
// behaviour).

//! The **TGA** (Targa image) format.
//!
//! Every texture that ships as `.tga` and every font atlas enters the game
//! through Westwood's Targa class (`WWLib/TARGA.cpp`): the texture loader
//! (`WW3D2/textureloader.cpp`) opens the file, XORs the Y-origin bit of the
//! in-memory header, and calls `Load(filename, TGAF_IMAGE, false)`; the
//! screenshot path (`WW3D2/ww3d.cpp`) builds a header around a framebuffer
//! copy, `YFlip`s it and calls `Save(filename, TGAF_IMAGE)`; the
//! ImagePacker tool additionally saves with `TGAF_COMPRESS`. This module
//! ports the class so that `parse`/`to_bytes` round-trips are **byte-exact
//! with the original implementations** (parity level P0, pinned by golden
//! vectors generated from the transcription in
//! `tools/differential/gen_tga_golden.cpp`).
//!
//! # File layout (as implemented here)
//!
//! All multi-byte integers are little-endian. The original structs are
//! `#pragma pack(push, 1)` — no padding anywhere.
//!
//! ```text
//! header (18)  : id_len | cmap_type | image_type | cmap_start | cmap_len |
//!                cmap_depth | x_offset | y_offset | width | height |
//!                pixel_depth | image_descriptor
//! image id     : id_len bytes (skipped by the loader)
//! colormap     : cmap_len * (cmap_depth >> 3) bytes (when cmap_type == 1)
//! image data   : raw scanlines or RLE packets (types 9..11)
//! [extension]  : 495 bytes, at the offset the footer names (Targa 2.0)
//! footer (26)  : ext_offset | dev_offset | "TRUEVISION-XFILE" | '.' | 0
//! ```
//!
//! Image types (`TGA_NOIMAGE` 0, `TGA_CMAPPED` 1, `TGA_TRUECOLOR` 2,
//! `TGA_MONO` 3, and the RLE forms 9/10/11): types 1/2/3 are raw scanlines,
//! types 9/10 are RLE packets (a high bit on the packet count byte means a
//! run of `count & 0x7F + 1` copies of the next pixel, otherwise
//! `count + 1` literal pixels). **Type 11 (`TGA_MONO_ENCODED`) is rejected
//! by the original's `Load` switch — it simply has no case for it — and
//! this port reproduces that** ([`TgaError::UnsupportedImageType`]), even
//! though the original's `Save` is happy to *write* type 11 files
//! (mono + `TGAF_COMPRESS` mutates the type with `+= 8`).
//!
//! # Load semantics (transcribed)
//!
//! `Targa::Load` does, in order:
//!
//! 1. read the colormap chunk (when `cmap_type == 1`),
//! 2. read or RLE-decode `width * height * bytes_per_pixel` image bytes
//!    — inverting every pixel's bytes afterwards for **truecolor types
//!    only** (2 and 10) when `invert_image` is set (the 24/32-bit BGR↔RGB
//!    swap; depths ≤ 2 are left alone),
//! 3. normalize the origin: `TGAIDF_XORIGIN` → horizontal mirror,
//!    `TGAIDF_YORIGIN` → vertical mirror, each clearing its descriptor bit.
//!    (The texture loader pre-XORs `TGAIDF_YORIGIN` before this step —
//!    "DX8 uses image upside down compared to TGA" — which is why its
//!    bottom-left files come out flipped. Reproduce the caller shape with
//!    [`TgaFile::parse_ex`].)
//!
//! Colormapped and monochrome images are **never** byte-inverted, whatever
//! `invert_image` says — the original applies the swap on the image-type
//! switch, not on the pixel depth.
//!
//! The original `Load(name, flags)` allocates nothing when the computed
//! image size is zero (a zero-width or zero-height file): the whole image
//! switch — including its unsupported-type rejection — is skipped and the
//! origin bits are left untouched. [`TgaFile::parse_ex`] reproduces that.
//!
//! # Save semantics (transcribed)
//!
//! `Targa::Save` writes the header with `id_len` forced to 0, bumps the
//! image type by 8 when `TGAF_COMPRESS` is asked of a raw type, writes the
//! colormap chunk, then the image — **inverting truecolor data in memory
//! first and restoring it afterwards**, so a truecolor file is always
//! stored byte-swapped relative to the in-memory buffer, RLE-encoding when
//! `TGAF_COMPRESS` (the encoder is golden-pinned too). When
//! `addextension` is asked it writes a 495-byte extension area stamped
//! `SoftID = "Denzil's Targa Code"`, `SoftVer.Number = 100` and
//! `ExtSize = 495` (the original mutates its member extension; the port
//! stamps a copy), and **always** finishes with the 26-byte Targa 2.0
//! footer — a `Save`d file carries the footer even when no extension is
//! written (`ext_offset = 0`), which is what the screenshot path produces.
//!
//! # Differences from the C++ (documented)
//!
//! - The C++ reader treats a file smaller than the 26-byte footer as a
//!   failed seek (`TGAERR_READ`); so does this port
//!   ([`TgaError::Truncated`]) — an 18-byte header-only TGA does **not**
//!   open, exactly as in the original.
//! - The C++ RLE decoder writes whatever the stream supplies into a buffer
//!   sized from the header; an over-supplied stream overruns it (and the
//!   `pixel_count -= count` bookkeeping underflows, looping on). This port
//!   refuses both shapes with [`TgaError::ImageOverrun`]; well-formed
//!   streams decode identically. Truncated streams are
//!   [`TgaError::Truncated`] (the C++'s short-read error).
//! - The C++ colormap path writes the chunk at
//!   `palette + cmap_start * (cmap_depth >> 3)` — past the end of its own
//!   allocation whenever `cmap_start != 0`. Real files use `cmap_start = 0`
//!   (the tools write 0); this port refuses anything else with
//!   [`TgaError::CMapOrigin`] when a palette is wanted.
//! - The C++ RLE **encoder** dereferences one pixel past the image buffer
//!   on its final iteration (the `end` cursor outruns the data before the
//!   last compare) and lets that stray byte decide the fate of the last
//!   pixel: when it *happens* to equal the last pixel, the pending run is
//!   dropped and the output truncates. The transcription — and this port —
//!   pin the comparison to its deterministic majority outcome (mismatch),
//!   so the encoder always emits every pixel. Every golden `gen` corpus
//!   round-trips through both.
//! - `Save` mutates the in-memory `Targa` (id length forced to 0, image
//!   type bumped, extension stamped, truecolor inverted and restored);
//!   [`TgaFile::to_bytes`] takes `&self` and applies all of that to
//!   copies. The emitted bytes are identical.
//! - A constructed file whose palette buffer is shorter than
//!   `cmap_len * (cmap_depth >> 3)` overreads in the C++; [`TgaError`]
//!   reports [`TgaError::BufferTooShort`] (impossible for parsed files).
//! - A header declaring an image larger than [`MAX_IMAGE_BYTES`] (256 MiB)
//!   fails the original's `malloc` (`TGAERR_NOMEM`) — and would abort this
//!   port's allocation. The port refuses with [`TgaError::TooLarge`] at
//!   the same point the original fails (before the image-type switch).

#![forbid(unsafe_code)]

use std::fmt;

// ---------------------------------------------------------------------------
// Format constants (TARGA.h)
// ---------------------------------------------------------------------------

/// No image data included in the file.
pub const TGA_NOIMAGE: u8 = 0;
/// Color-mapped image data.
pub const TGA_CMAPPED: u8 = 1;
/// Truecolor image data.
pub const TGA_TRUECOLOR: u8 = 2;
/// Monochrome image data.
pub const TGA_MONO: u8 = 3;
/// Color-mapped image data (RLE encoded).
pub const TGA_CMAPPED_ENCODED: u8 = 9;
/// Truecolor image data (RLE encoded).
pub const TGA_TRUECOLOR_ENCODED: u8 = 10;
/// Monochrome image data (RLE encoded) — **rejected by the original's
/// `Load` switch**; kept as a named constant so the rejection is explicit.
pub const TGA_MONO_ENCODED: u8 = 11;

/// Number of attribute (alpha) bits per pixel, low nibble of the image
/// descriptor.
pub const TGAIDF_ATTRIB_BITS: u8 = 0x0F;
/// Image descriptor bit 4: X origin at the right edge (the loader mirrors
/// the image and clears the bit).
pub const TGAIDF_XORIGIN: u8 = 1 << 4;
/// Image descriptor bit 5: Y origin at the top edge (the loader mirrors the
/// image and clears the bit).
pub const TGAIDF_YORIGIN: u8 = 1 << 5;

/// The Targa 2.0 footer signature.
pub const TGA2_SIGNATURE: &[u8; 16] = b"TRUEVISION-XFILE";

/// Size of the 18-byte Targa header.
pub const TGA_HEADER_SIZE: usize = 18;
/// Size of the 26-byte Targa 2.0 footer.
pub const TGA2_FOOTER_SIZE: usize = 26;
/// Size of the 495-byte Targa 2.0 extension area.
pub const TGA2_EXTENSION_SIZE: usize = 495;

/// `TGA_BytesPerPixel(a) ((a+7) >> 3)` — the pixel depth rounded up to
/// whole bytes.
#[inline]
pub fn tga_bytes_per_pixel(pixel_depth: u8) -> usize {
    ((pixel_depth as usize) + 7) >> 3
}

/// The largest image allocation [`TgaFile::parse_ex`] will make (256 MiB —
/// a 8064×8064 32-bit texture, three orders beyond anything the game
/// ships). Beyond it the original fails its `malloc` (`TGAERR_NOMEM`);
/// the port refuses with [`TgaError::TooLarge`] instead of attempting the
/// allocation.
pub const MAX_IMAGE_BYTES: usize = 256 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Everything that can go wrong while parsing or re-encoding a Targa.
///
/// The original signals `TGAERR_OPEN/READ/WRITE/SYNTAX/NOMEM/NOTSUPPORTED`
/// through one `long`; the cases this port can hit are split by cause so
/// callers can distinguish "not this format" from "hostile input".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TgaError {
    /// A read ran past the end of the input — the original's short-read
    /// `TGAERR_READ` cases, including the footer seek on a file shorter
    /// than 26 bytes (an 18-byte header-only TGA does not open).
    Truncated,
    /// The RLE stream decodes more pixels than the header declares. The
    /// original writes them past its image buffer (and its
    /// `pixel_count -= count` bookkeeping underflows); refused here.
    ImageOverrun,
    /// An image type the original's `Load` switch does not handle
    /// (`TGAERR_NOTSUPPORTED`) — anything outside 1/2/3/9/10, **including
    /// type 11 (`TGA_MONO_ENCODED`), which the original forgets to case**.
    UnsupportedImageType,
    /// A colormap with a non-zero `cmap_start` was requested. The original
    /// writes the chunk at `palette + cmap_start * depth` — past its own
    /// allocation. Real files use 0; anything else is refused.
    CMapOrigin,
    /// The header declares an image larger than [`MAX_IMAGE_BYTES`]. The
    /// original's `malloc` simply fails there (`TGAERR_NOMEM`); the port
    /// refuses deterministically — a 65535×65535 32-bit declaration is
    /// 17 GB, three orders beyond anything the game ships.
    TooLarge,
    /// The in-memory image or palette is smaller than the header declares
    /// (a construction error; impossible for [`TgaFile::parse`] output).
    BufferTooShort,
}

impl fmt::Display for TgaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TgaError::Truncated => write!(f, "targa: read past end of input"),
            TgaError::ImageOverrun => {
                write!(f, "targa: RLE stream overruns the declared image")
            }
            TgaError::UnsupportedImageType => {
                write!(f, "targa: image type not supported by the original loader")
            }
            TgaError::CMapOrigin => write!(f, "targa: non-zero colormap origin"),
            TgaError::TooLarge => write!(f, "targa: declared image size beyond the sane cap"),
            TgaError::BufferTooShort => {
                write!(
                    f,
                    "targa: in-memory buffer shorter than the header declares"
                )
            }
        }
    }
}

impl std::error::Error for TgaError {}

// ---------------------------------------------------------------------------
// Header / footer / extension (TARGA.h, packed)
// ---------------------------------------------------------------------------

/// The 18-byte Targa header (`TGAHeader`), fields in file order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TgaHeader {
    /// Size of the image ID field that follows the header.
    pub id_length: u8,
    /// Color map type (1 = a colormap chunk follows the ID).
    pub color_map_type: u8,
    /// Image type code (see the `TGA_*` constants).
    pub image_type: u8,
    /// Color map origin (the entry index the chunk starts at).
    pub cmap_start: u16,
    /// Color map length (entries).
    pub cmap_length: u16,
    /// Depth of color map entries in bits.
    pub cmap_depth: u8,
    /// X origin of the image.
    pub x_offset: u16,
    /// Y origin of the image.
    pub y_offset: u16,
    /// Width of the image in pixels.
    pub width: u16,
    /// Height of the image in pixels.
    pub height: u16,
    /// Image pixel size in bits.
    pub pixel_depth: u8,
    /// Image descriptor byte (origin bits + attribute-bit count).
    pub image_descriptor: u8,
}

impl TgaHeader {
    /// Parse the 18-byte header. `Ok((header, consumed))` — `consumed` is
    /// always 18 here; the ID field is skipped separately by the loader.
    pub fn parse(bytes: &[u8]) -> Result<(Self, usize), TgaError> {
        if bytes.len() < TGA_HEADER_SIZE {
            return Err(TgaError::Truncated);
        }
        let b = &bytes[..TGA_HEADER_SIZE];
        let header = TgaHeader {
            id_length: b[0],
            color_map_type: b[1],
            image_type: b[2],
            cmap_start: u16::from_le_bytes([b[3], b[4]]),
            cmap_length: u16::from_le_bytes([b[5], b[6]]),
            cmap_depth: b[7],
            x_offset: u16::from_le_bytes([b[8], b[9]]),
            y_offset: u16::from_le_bytes([b[10], b[11]]),
            width: u16::from_le_bytes([b[12], b[13]]),
            height: u16::from_le_bytes([b[14], b[15]]),
            pixel_depth: b[16],
            image_descriptor: b[17],
        };
        Ok((header, TGA_HEADER_SIZE))
    }

    /// Serialize the header in file order.
    pub fn to_bytes(self) -> [u8; TGA_HEADER_SIZE] {
        let mut out = [0u8; TGA_HEADER_SIZE];
        out[0] = self.id_length;
        out[1] = self.color_map_type;
        out[2] = self.image_type;
        out[3..5].copy_from_slice(&self.cmap_start.to_le_bytes());
        out[5..7].copy_from_slice(&self.cmap_length.to_le_bytes());
        out[7] = self.cmap_depth;
        out[8..10].copy_from_slice(&self.x_offset.to_le_bytes());
        out[10..12].copy_from_slice(&self.y_offset.to_le_bytes());
        out[12..14].copy_from_slice(&self.width.to_le_bytes());
        out[14..16].copy_from_slice(&self.height.to_le_bytes());
        out[16] = self.pixel_depth;
        out[17] = self.image_descriptor;
        out
    }

    /// `Targa::IsCompressed` — the image type is one of the RLE forms
    /// (literally `image_type > 8`, as in the original).
    #[inline]
    pub fn is_compressed(self) -> bool {
        self.image_type > 8
    }

    /// The image size in bytes the original's allocation path computes:
    /// `width * height * TGA_BytesPerPixel(pixel_depth)`.
    #[inline]
    pub fn image_size(self) -> usize {
        self.width as usize * self.height as usize * tga_bytes_per_pixel(self.pixel_depth)
    }

    /// The colormap chunk size the original computes:
    /// `cmap_length * (cmap_depth >> 3)`.
    #[inline]
    pub fn cmap_size(self) -> usize {
        self.cmap_length as usize * (self.cmap_depth >> 3) as usize
    }
}

/// The 26-byte Targa 2.0 footer (`TGA2Footer`): the two offsets plus the
/// fixed signature/dot/zero tail. The signature, `RsvdChar` (`'.'`) and
/// `BZST` (`0`) are constants on the wire; only the offsets are data.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tga2Footer {
    /// Offset of the extension area from the start of the file (0 = none).
    pub extension: i32,
    /// Offset of the developer area from the start of the file (0 = none).
    pub developer: i32,
}

impl Tga2Footer {
    /// The writer's fixed 26-byte tail: signature, reserved dot, terminator.
    pub fn signature_tail() -> [u8; 18] {
        let mut tail = [0u8; 18];
        tail[..16].copy_from_slice(TGA2_SIGNATURE);
        tail[16] = b'.';
        tail[17] = 0;
        tail
    }

    /// Parse the footer from its 26 bytes. The signature is checked by the
    /// caller (that is what decides whether this *is* a footer).
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < TGA2_FOOTER_SIZE {
            return None;
        }
        Some(Tga2Footer {
            extension: i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            developer: i32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
        })
    }

    /// Serialize the footer: offsets plus the fixed signature tail.
    pub fn to_bytes(self) -> [u8; TGA2_FOOTER_SIZE] {
        let mut out = [0u8; TGA2_FOOTER_SIZE];
        out[..4].copy_from_slice(&self.extension.to_le_bytes());
        out[4..8].copy_from_slice(&self.developer.to_le_bytes());
        out[8..].copy_from_slice(&Self::signature_tail());
        out
    }
}

/// The 495-byte Targa 2.0 extension area (`TGA2Extension`), every field
/// preserved byte-for-byte (the C-string fields are kept as raw arrays so
/// arbitrary files round-trip; the original never validates them either).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tga2Extension {
    /// Extension area size (495 for 2.0).
    pub ext_size: i16,
    /// Name of the person who created the image (NUL-terminated ASCII).
    pub auth_name: [u8; 41],
    /// Comments of the author (NUL-terminated ASCII).
    pub auth_comment: [u8; 324],
    /// Date stamp: month (1–12), day (1–31), four-digit year.
    pub date_month: i16,
    /// Date stamp day.
    pub date_day: i16,
    /// Date stamp year.
    pub date_year: i16,
    /// Time stamp: military hour, minute, second.
    pub time_hour: i16,
    /// Time stamp minute.
    pub time_minute: i16,
    /// Time stamp second.
    pub time_second: i16,
    /// Name of the job the image belongs to (NUL-terminated ASCII).
    pub job_name: [u8; 41],
    /// Elapsed time of the job: hour, minute, second.
    pub job_time_hour: i16,
    /// Job elapsed-time minute.
    pub job_time_minute: i16,
    /// Job elapsed-time second.
    pub job_time_second: i16,
    /// ID of the software used to create the image (NUL-terminated ASCII).
    pub soft_id: [u8; 41],
    /// Version number of the software (×100) and its letter.
    pub soft_ver_number: i16,
    /// Version letter of the software.
    pub soft_ver_letter: u8,
    /// Transparent color value.
    pub key_color: i32,
    /// Pixel aspect ratio numerator (denominator 0 = not specified).
    pub aspect_numer: i16,
    /// Pixel aspect ratio denominator.
    pub aspect_denom: i16,
    /// Fractional gamma value numerator.
    pub gamma_numer: i16,
    /// Fractional gamma value denominator.
    pub gamma_denom: i16,
    /// Color correction table offset.
    pub color_cor: i32,
    /// Postage stamp image offset.
    pub post_stamp: i32,
    /// Scan line table offset.
    pub scan_line: i32,
    /// Alpha channel attribute type (the `EXTA_*` values).
    pub attributes: u8,
}

impl Default for Tga2Extension {
    fn default() -> Self {
        // memset(&mExtension, 0, sizeof(TGA2Extension)) — the constructor's
        // zeroing; the game never fills the fields in.
        Tga2Extension {
            ext_size: 0,
            auth_name: [0; 41],
            auth_comment: [0; 324],
            date_month: 0,
            date_day: 0,
            date_year: 0,
            time_hour: 0,
            time_minute: 0,
            time_second: 0,
            job_name: [0; 41],
            job_time_hour: 0,
            job_time_minute: 0,
            job_time_second: 0,
            soft_id: [0; 41],
            soft_ver_number: 0,
            soft_ver_letter: 0,
            key_color: 0,
            aspect_numer: 0,
            aspect_denom: 0,
            gamma_numer: 0,
            gamma_denom: 0,
            color_cor: 0,
            post_stamp: 0,
            scan_line: 0,
            attributes: 0,
        }
    }
}

impl Tga2Extension {
    /// Parse the 495-byte extension area (`#pragma pack(1)` layout, fields
    /// in declaration order).
    pub fn parse(bytes: &[u8]) -> Result<Self, TgaError> {
        if bytes.len() < TGA2_EXTENSION_SIZE {
            return Err(TgaError::Truncated);
        }
        let b = &bytes[..TGA2_EXTENSION_SIZE];
        let mut o = 0usize;
        fn u16le(b: &[u8], o: &mut usize) -> i16 {
            let v = i16::from_le_bytes([b[*o], b[*o + 1]]);
            *o += 2;
            v
        }
        fn i32le(b: &[u8], o: &mut usize) -> i32 {
            let v = i32::from_le_bytes([b[*o], b[*o + 1], b[*o + 2], b[*o + 3]]);
            *o += 4;
            v
        }
        fn cstr<const N: usize>(b: &[u8], o: &mut usize) -> [u8; N] {
            let mut a = [0u8; N];
            a.copy_from_slice(&b[*o..*o + N]);
            *o += N;
            a
        }
        let ext_size = u16le(b, &mut o);
        let auth_name = cstr::<41>(b, &mut o);
        let auth_comment = cstr::<324>(b, &mut o);
        let date_month = u16le(b, &mut o);
        let date_day = u16le(b, &mut o);
        let date_year = u16le(b, &mut o);
        let time_hour = u16le(b, &mut o);
        let time_minute = u16le(b, &mut o);
        let time_second = u16le(b, &mut o);
        let job_name = cstr::<41>(b, &mut o);
        let job_time_hour = u16le(b, &mut o);
        let job_time_minute = u16le(b, &mut o);
        let job_time_second = u16le(b, &mut o);
        let soft_id = cstr::<41>(b, &mut o);
        let soft_ver_number = u16le(b, &mut o);
        let soft_ver_letter = b[o];
        o += 1;
        let key_color = i32le(b, &mut o);
        let aspect_numer = u16le(b, &mut o);
        let aspect_denom = u16le(b, &mut o);
        let gamma_numer = u16le(b, &mut o);
        let gamma_denom = u16le(b, &mut o);
        let color_cor = i32le(b, &mut o);
        let post_stamp = i32le(b, &mut o);
        let scan_line = i32le(b, &mut o);
        let attributes = b[o];
        o += 1;
        debug_assert_eq!(o, TGA2_EXTENSION_SIZE, "extension layout drift");
        Ok(Tga2Extension {
            ext_size,
            auth_name,
            auth_comment,
            date_month,
            date_day,
            date_year,
            time_hour,
            time_minute,
            time_second,
            job_name,
            job_time_hour,
            job_time_minute,
            job_time_second,
            soft_id,
            soft_ver_number,
            soft_ver_letter,
            key_color,
            aspect_numer,
            aspect_denom,
            gamma_numer,
            gamma_denom,
            color_cor,
            post_stamp,
            scan_line,
            attributes,
        })
    }

    /// Serialize the 495-byte extension area.
    pub fn to_bytes(&self) -> [u8; TGA2_EXTENSION_SIZE] {
        let mut out = [0u8; TGA2_EXTENSION_SIZE];
        let mut o = 0usize;
        let mut put = |src: &[u8]| {
            out[o..o + src.len()].copy_from_slice(src);
            o += src.len();
        };
        put(&self.ext_size.to_le_bytes());
        put(&self.auth_name);
        put(&self.auth_comment);
        put(&self.date_month.to_le_bytes());
        put(&self.date_day.to_le_bytes());
        put(&self.date_year.to_le_bytes());
        put(&self.time_hour.to_le_bytes());
        put(&self.time_minute.to_le_bytes());
        put(&self.time_second.to_le_bytes());
        put(&self.job_name);
        put(&self.job_time_hour.to_le_bytes());
        put(&self.job_time_minute.to_le_bytes());
        put(&self.job_time_second.to_le_bytes());
        put(&self.soft_id);
        put(&self.soft_ver_number.to_le_bytes());
        put(&[self.soft_ver_letter]);
        put(&self.key_color.to_le_bytes());
        put(&self.aspect_numer.to_le_bytes());
        put(&self.aspect_denom.to_le_bytes());
        put(&self.gamma_numer.to_le_bytes());
        put(&self.gamma_denom.to_le_bytes());
        put(&self.color_cor.to_le_bytes());
        put(&self.post_stamp.to_le_bytes());
        put(&self.scan_line.to_le_bytes());
        put(&[self.attributes]);
        debug_assert_eq!(o, TGA2_EXTENSION_SIZE, "extension layout drift");
        out
    }
}

// ---------------------------------------------------------------------------
// File
// ---------------------------------------------------------------------------

/// What a parsed Targa holds: the header, the Targa 2.0 footer/extension
/// when present, the colormap chunk and the decoded, origin-normalized
/// image — the state `Targa::Load` leaves behind, minus the file handle.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TgaFile {
    /// The 18-byte header. After a parse the origin bits of
    /// [`TgaHeader::image_descriptor`] are cleared exactly as the
    /// original's `Load` clears them (only when an image was actually
    /// loaded — a zero-size image leaves the header untouched).
    pub header: TgaHeader,
    /// The Targa 2.0 footer, when the file ends with the signature. The
    /// original keeps the footer only to decide whether an extension
    /// exists; it is preserved here so files round-trip.
    pub footer: Option<Tga2Footer>,
    /// The Targa 2.0 extension area, when the footer names a non-zero
    /// offset for it (`GetExtension`'s `TGAF_TGA2` rule).
    pub extension: Option<Tga2Extension>,
    /// The raw colormap chunk: `cmap_length * (cmap_depth >> 3)` bytes as
    /// they sit in the file (the original copies them into the caller's
    /// buffer at `cmap_start * depth`, which is the origin quirk this port
    /// refuses; see [`TgaError::CMapOrigin`]). Empty when
    /// `color_map_type != 1` or the chunk size is zero.
    pub palette: Vec<u8>,
    /// The decoded image: `width * height * bytes_per_pixel` bytes, after
    /// the truecolor byte-invert (when requested) and the origin
    /// normalization. Empty when no image was requested or the image size
    /// is zero.
    pub image: Vec<u8>,
}

/// Flags selecting what [`TgaFile::to_bytes`] writes (`TGAF_IMAGE`,
/// `TGAF_PAL`, `TGAF_COMPRESS`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SaveFlags {
    /// `TGAF_IMAGE`: write the image data block.
    pub image: bool,
    /// `TGAF_PAL`: write the colormap block (only when
    /// `cmap_length > 0` and a palette is held — the original also requires
    /// its `mPalette` to be non-null here).
    pub palette: bool,
    /// `TGAF_COMPRESS`: RLE-encode the image block (and bump the written
    /// image type by 8 when the type is one of the raw forms).
    pub compress: bool,
}

impl SaveFlags {
    /// `TGAF_IMAGE` — the screenshot/WorldBuilder shape.
    pub const IMAGE: Self = Self {
        image: true,
        palette: false,
        compress: false,
    };
    /// `TGAF_IMAGE | TGAF_PAL` — the library-default load's inverse shape.
    pub const IMAGE_PAL: Self = Self {
        image: true,
        palette: true,
        compress: false,
    };
    /// `TGAF_IMAGE | TGAF_COMPRESS` — the ImagePacker shape.
    pub const IMAGE_COMPRESS: Self = Self {
        image: true,
        palette: false,
        compress: true,
    };
    /// `TGAF_IMAGE | TGAF_PAL | TGAF_COMPRESS`.
    pub const IMAGE_PAL_COMPRESS: Self = Self {
        image: true,
        palette: true,
        compress: true,
    };
}

impl TgaFile {
    /// `Open(name, TGA_READMODE)` + `Load(name, TGAF_IMAGE | TGAF_PAL)` —
    /// the library-default shape: palette and image both wanted, the
    /// truecolor invert applied (`invert_image` defaults to true in the
    /// original), no descriptor pre-XOR.
    pub fn parse(bytes: &[u8]) -> Result<Self, TgaError> {
        Self::parse_ex(bytes, true, true, true, 0)
    }

    /// The caller-controlled shape of `Open` + `Load(name, flags,
    /// invert_image)`:
    ///
    /// - `want_palette` / `want_image` stand in for the `TGAF_PAL` /
    ///   `TGAF_IMAGE` flags (the original's user-buffer plumbing —
    ///   `SetPalette`/`SetImage` — ends in the same places);
    /// - `invert_image` is the original's parameter (the texture loader
    ///   passes **false**);
    /// - `descriptor_pre_xor` is XORed into the in-memory
    ///   `image_descriptor` before the load logic — the texture loader's
    ///   `targa.Header.ImageDescriptor ^= TGAIDF_YORIGIN` ("DX8 uses image
    ///   upside down compared to TGA").
    ///
    /// The original's two-step `Open` + `Load` re-enters `Open` from the
    /// flags-Load, which short-circuits on the already-open file — one pass
    /// over the bytes, exactly as modelled here.
    pub fn parse_ex(
        bytes: &[u8],
        want_palette: bool,
        want_image: bool,
        invert_image: bool,
        descriptor_pre_xor: u8,
    ) -> Result<Self, TgaError> {
        // --- Open(TGA_READMODE) --------------------------------------------
        // The footer sniff seeks to -26 from the end; a file shorter than
        // the footer fails the seek (TGAERR_READ) and never opens.
        if bytes.len() < TGA2_FOOTER_SIZE {
            return Err(TgaError::Truncated);
        }
        let mut footer = None;
        let mut extension = None;
        let tail = &bytes[bytes.len() - TGA2_FOOTER_SIZE..];
        if &tail[8..24] == TGA2_SIGNATURE {
            let f = Tga2Footer::parse(tail).expect("footer length checked");
            // "If this a 2.0 file with an extension?" — TGAF_TGA2 needs a
            // non-zero extension offset; the footer itself is preserved.
            if f.extension != 0 {
                // The original seeks to the offset and reads the whole
                // struct; a short or out-of-range read is TGAERR_READ.
                let off = f.extension;
                if off < 0 {
                    return Err(TgaError::Truncated);
                }
                let off = off as usize;
                if off > bytes.len() || bytes.len() - off < TGA2_EXTENSION_SIZE {
                    return Err(TgaError::Truncated);
                }
                extension = Some(Tga2Extension::parse(&bytes[off..])?);
            }
            footer = Some(f);
        }

        // Header from offset 0, then the ID field is skipped.
        let (mut header, mut cursor) = TgaHeader::parse(bytes)?;
        cursor += header.id_length as usize;
        if cursor > bytes.len() {
            return Err(TgaError::Truncated);
        }
        header.image_descriptor ^= descriptor_pre_xor;

        let mut file = TgaFile {
            header,
            footer,
            extension,
            palette: Vec::new(),
            image: Vec::new(),
        };

        // --- Load(name, mPalette, mImage, invert_image) ---------------------
        // Colormap chunk: read when wanted and non-empty, skipped (cursor
        // advanced) otherwise — the C++ reads or seeks over the same size.
        if header.color_map_type == 1 {
            let size = header.cmap_size();
            if cursor + size > bytes.len() {
                return Err(TgaError::Truncated);
            }
            if want_palette {
                // The original writes the chunk at palette +
                // cmap_start*depth — past its own allocation whenever
                // cmap_start != 0 (it sizes the buffer from cmap_length).
                // Real files use 0; refuse the rest.
                if header.cmap_start != 0 {
                    return Err(TgaError::CMapOrigin);
                }
                file.palette = bytes[cursor..cursor + size].to_vec();
            }
            cursor += size;
        }

        // Image block — everything below runs only when a buffer exists,
        // and the flags-Load allocates nothing for a zero-size image (the
        // switch, including its unsupported-type rejection, is skipped and
        // the origin bits survive).
        if want_image && header.image_size() != 0 {
            // Refuse absurd dimensions before allocating (the original's
            // malloc fails there; see MAX_IMAGE_BYTES).
            if header.image_size() > MAX_IMAGE_BYTES {
                return Err(TgaError::TooLarge);
            }
            let depth = tga_bytes_per_pixel(header.pixel_depth);
            let mut image = vec![0u8; header.image_size()];
            match header.image_type {
                TGA_CMAPPED | TGA_MONO => {
                    read_exact(bytes, &mut cursor, &mut image)?;
                    // No invert: the original's switch cases for the raw
                    // non-truecolor types never call InvertImage.
                }
                TGA_TRUECOLOR => {
                    read_exact(bytes, &mut cursor, &mut image)?;
                    if invert_image {
                        invert_in_place(&mut image, depth);
                    }
                }
                TGA_CMAPPED_ENCODED => {
                    decode_rle(bytes, &mut cursor, &mut image, depth)?;
                }
                TGA_TRUECOLOR_ENCODED => {
                    decode_rle(bytes, &mut cursor, &mut image, depth)?;
                    if invert_image {
                        invert_in_place(&mut image, depth);
                    }
                }
                // The original's default — including TGA_MONO_ENCODED (11),
                // which it forgets to case.
                _ => return Err(TgaError::UnsupportedImageType),
            }

            // Origin normalization: mirror and clear, exactly as the
            // original leaves the header for the next caller.
            if header.image_descriptor & TGAIDF_XORIGIN != 0 {
                xflip_in_place(&mut image, header.width, header.height, depth);
                file.header.image_descriptor &= !TGAIDF_XORIGIN;
            }
            if header.image_descriptor & TGAIDF_YORIGIN != 0 {
                yflip_in_place(&mut image, header.width, header.height, depth);
                file.header.image_descriptor &= !TGAIDF_YORIGIN;
            }
            file.image = image;
        }

        Ok(file)
    }

    /// `Targa::Save` — structures → bytes. The file is the header (with
    /// `id_length` forced to 0, as the original mutates its member), the
    /// colormap chunk when [`SaveFlags::palette`] and the header declare
    /// one, the image block, then the always-present 26-byte Targa 2.0
    /// footer — with a 495-byte extension area inserted before it when
    /// `addextension` is set (stamped `ExtSize = 495`,
    /// `SoftID = "Denzil's Targa Code"`, `SoftVer.Number = 100`, and
    /// `footer.extension` pointed at it).
    ///
    /// Truecolor data (raw or RLE, after the compress type-bump) is
    /// byte-inverted for the write and restored — the original's
    /// in-memory inversion — so what lands in the file is the swapped
    /// form, exactly as every `Save`d Zero Hour texture has it.
    pub fn to_bytes(&self, flags: SaveFlags, addextension: bool) -> Result<Vec<u8>, TgaError> {
        let mut out = Vec::new();

        // Header: IDLength forced to 0 (the original's `Header.IDLength =
        // 0`), type bumped by 8 when compression is asked of a raw type —
        // both on the written copy; unknown types clear the compress flag.
        let mut header = self.header;
        header.id_length = 0;
        if flags.compress {
            match header.image_type {
                TGA_CMAPPED | TGA_TRUECOLOR | TGA_MONO => header.image_type += 8,
                TGA_CMAPPED_ENCODED | TGA_TRUECOLOR_ENCODED | TGA_MONO_ENCODED => {}
                // Turn off compression for unknown types.
                _ => return Err(TgaError::UnsupportedImageType),
            }
        }
        out.extend_from_slice(&header.to_bytes());

        // Colormap chunk, from the stored raw form.
        if flags.palette && !self.palette.is_empty() && header.cmap_length > 0 {
            let size = header.cmap_size();
            if self.palette.len() < size {
                return Err(TgaError::BufferTooShort);
            }
            out.extend_from_slice(&self.palette[..size]);
        }

        // Image block. The original checks the *mutated* type here, so a
        // compressed raw truecolor file (2 → 10) still inverts, while a
        // compressed mono file (3 → 11) still does not.
        if flags.image && !self.image.is_empty() {
            let depth = tga_bytes_per_pixel(header.pixel_depth);
            let truecolor =
                header.image_type == TGA_TRUECOLOR || header.image_type == TGA_TRUECOLOR_ENCODED;
            if self.image.len() < header.image_size() {
                return Err(TgaError::BufferTooShort);
            }
            let mut image = self.image[..header.image_size()].to_vec();
            if truecolor {
                invert_in_place(&mut image, depth);
            }
            if flags.compress {
                out.extend_from_slice(&encode_rle(&image, depth));
            } else {
                out.extend_from_slice(&image);
            }
            // The original restores its buffer here; the port worked on a
            // copy, so there is nothing to undo.
        }

        // Extension area, when asked: the original stamps its member
        // extension and records the current position as the footer's
        // extension offset.
        let mut footer = Tga2Footer {
            extension: 0,
            developer: 0,
        };
        if addextension {
            let mut extension = self.extension.clone().unwrap_or_default();
            extension.ext_size = 495;
            let soft_id = b"Denzil's Targa Code";
            extension.soft_id[..soft_id.len()].copy_from_slice(soft_id);
            // strlcpy NUL-terminates within the 41-byte field; the source
            // is shorter, so the remainder stays as it was (zeros for a
            // fresh extension).
            extension.soft_id[soft_id.len()] = 0;
            extension.soft_ver_number = 100;
            extension.soft_ver_letter = 0;
            footer.extension = out.len() as i32;
            out.extend_from_slice(&extension.to_bytes());
        }

        // The footer is written unconditionally — even with no extension
        // (offset 0), which is what the screenshot files carry.
        out.extend_from_slice(&footer.to_bytes());
        Ok(out)
    }

    /// `Targa::XFlip` — mirror every row left-to-right.
    pub fn xflip(&mut self) {
        let depth = tga_bytes_per_pixel(self.header.pixel_depth);
        xflip_in_place(
            &mut self.image,
            self.header.width,
            self.header.height,
            depth,
        );
    }

    /// `Targa::YFlip` — exchange rows top-to-bottom.
    pub fn yflip(&mut self) {
        let depth = tga_bytes_per_pixel(self.header.pixel_depth);
        yflip_in_place(
            &mut self.image,
            self.header.width,
            self.header.height,
            depth,
        );
    }

    /// `Targa::InvertImage` — reverse each pixel's bytes (depths > 2 only;
    /// the 16-bit layout is deliberately left alone, as in the original).
    pub fn invert_image(&mut self) {
        let depth = tga_bytes_per_pixel(self.header.pixel_depth);
        invert_in_place(&mut self.image, depth);
    }
}

/// `File_Read` with the original's exact-read contract: a short read is the
/// caller's error (`TGAERR_READ`).
fn read_exact(bytes: &[u8], cursor: &mut usize, out: &mut [u8]) -> Result<(), TgaError> {
    let end = *cursor + out.len();
    if end > bytes.len() {
        return Err(TgaError::Truncated);
    }
    out.copy_from_slice(&bytes[*cursor..end]);
    *cursor = end;
    Ok(())
}

/// `Targa::DecodeImage` — the Targa RLE stream into a pre-sized buffer.
///
/// A packet count with the high bit set is a run: `count & 0x7F + 1`
/// copies of the next pixel. Otherwise `count + 1` literal pixels follow.
/// The original writes whatever arrives into the buffer and underflows its
/// `pixel_count` bookkeeping when the stream oversupplies — this port
/// refuses both shapes ([`TgaError::ImageOverrun`]) since the buffer check
/// and the pixel-count check coincide (`out` is exactly
/// `pixel_count * depth` bytes).
///
/// Public for the same reason as [`encode_rle`]: the stream format is its
/// own layer, and the texture pipeline may need it without a full file.
/// `cursor` advanced by the bytes consumed.
pub fn decode_rle(
    bytes: &[u8],
    cursor: &mut usize,
    image: &mut [u8],
    depth: usize,
) -> Result<(), TgaError> {
    let mut pos = 0usize;
    let mut remaining = image.len() / depth.max(1);
    while remaining > 0 {
        let count = {
            let end = *cursor + 1;
            if end > bytes.len() {
                return Err(TgaError::Truncated);
            }
            let c = bytes[*cursor];
            *cursor = end;
            c
        };
        if count & 0x80 != 0 {
            let run = (count & 0x7F) as usize + 1;
            let pixel_end = *cursor + depth;
            if pixel_end > bytes.len() {
                return Err(TgaError::Truncated);
            }
            let pixel = &bytes[*cursor..pixel_end];
            *cursor = pixel_end;
            if run > remaining {
                return Err(TgaError::ImageOverrun);
            }
            for _ in 0..run {
                image[pos..pos + depth].copy_from_slice(pixel);
                pos += depth;
            }
            remaining -= run;
        } else {
            let n = count as usize + 1;
            let size = n * depth;
            let end = *cursor + size;
            if end > bytes.len() {
                return Err(TgaError::Truncated);
            }
            if n > remaining {
                return Err(TgaError::ImageOverrun);
            }
            image[pos..pos + size].copy_from_slice(&bytes[*cursor..end]);
            *cursor = end;
            pos += size;
            remaining -= n;
        }
    }
    Ok(())
}

/// `Targa::EncodeImage` — the Targa RLE stream from an image buffer.
///
/// The transcription of the original's packet loop: a pending run count is
/// kept until either the run breaks (a `0x80 | count` packet with the
/// reference pixel) or the raw packet fills (127 pixels, flushed with the
/// off-by-one count byte the format stores). The final iteration's compare
/// reads one pixel past the buffer in the original — see the module docs —
/// and is pinned here to its deterministic majority outcome: a mismatch.
pub fn encode_rle(image: &[u8], depth: usize) -> Vec<u8> {
    let pixels = image.len() / depth.max(1);
    let mut out = Vec::new();
    // The packet buffer of the original: 128 max-count pixels plus the
    // count byte. Depth 0 (a degenerate header) only ever writes count
    // bytes; the buffer stays one byte so packet[0] exists.
    let mut packet = vec![0u8; 128 * depth.max(1)];
    let mut packet_index = 1usize;
    let mut count: i32 = 0;
    let mut start = 0usize; // byte offset of the reference pixel
    let mut end = 0usize; // byte offset of the scan pixel
    let mut pixels = pixels;
    while pixels != 0 {
        end += depth;
        pixels -= 1;

        // Compare the reference pixel against the scan pixel. When `end`
        // is past the data (the original's final-iteration overread) the
        // comparison is pinned to "differ".
        let matched = if end + depth > image.len() {
            false
        } else {
            image[start..start + depth] == image[end..end + depth]
        };

        if matched {
            count += 1;
            if count < 128 {
                // A run packet is not open yet — keep counting.
                if packet[0] == 0 {
                    continue;
                }
            } else {
                count -= 1;
            }
        }

        if count != 0 && packet[0] == 0 {
            // Run packet: count byte (with the high bit) plus one pixel.
            packet[0] = (count as u8) | 0x80;
            packet[1..1 + depth].copy_from_slice(&image[start..start + depth]);
            out.extend_from_slice(&packet[..depth + 1]);
            start = end;
            count = 0;
            packet[0] = 0;
        } else {
            if count == 0 {
                // Raw pixel into the open raw packet.
                packet[packet_index..packet_index + depth]
                    .copy_from_slice(&image[start..start + depth]);
                packet[0] += 1;
                start = end;
                packet_index += depth;
            }
            // Flush the raw packet when full, when a run interrupts it, or
            // at the end of the image. The stored count byte is one less
            // than the number of pixels (the format's raw-packet rule).
            if packet[0] == 127 || count != 0 || pixels == 0 {
                let n = packet[0] as usize;
                packet[0] -= 1;
                out.extend_from_slice(&packet[..n * depth + 1]);
                packet_index = 1;
                packet[0] = 0;
            }
        }
    }
    out
}

/// The body of `Targa::XFlip`: mirror every row left-to-right.
fn xflip_in_place(image: &mut [u8], width: u16, height: u16, depth: usize) {
    let row = width as usize * depth;
    for y in 0..height as usize {
        let line = &mut image[y * row..(y + 1) * row];
        for x in 0..width as usize / 2 {
            let left = x * depth;
            let right = row - (x + 1) * depth;
            // The two windows never overlap (right >= left + depth); the
            // split lets the borrow checker see it.
            let (head, tail) = line.split_at_mut(right);
            head[left..left + depth].swap_with_slice(&mut tail[..depth]);
        }
    }
}

/// The body of `Targa::YFlip` (the `_swapBytes` form): exchange the top and
/// bottom halves row by row.
fn yflip_in_place(image: &mut [u8], width: u16, height: u16, depth: usize) {
    let stride = width as usize * depth;
    let height = height as usize;
    let mut top = 0usize;
    let mut bottom = stride * height.saturating_sub(1);
    for _ in 0..height / 2 {
        // The rows never overlap (top + stride <= bottom); the split lets
        // the borrow checker see it.
        let (head, tail) = image.split_at_mut(top + stride);
        let tail_at = bottom - (top + stride);
        head[top..top + stride].swap_with_slice(&mut tail[tail_at..tail_at + stride]);
        top += stride;
        bottom -= stride;
    }
}

/// The body of `Targa::InvertImage`: reverse each pixel's bytes when the
/// pixel is wider than two bytes (16-bit pixels are deliberately untouched).
///
/// The depths the game ships (24/32-bit pixels) are specialized so the
/// per-pixel reversal fully unrolls — a runtime `depth` keeps the reversal
/// in a generic per-byte loop that measures 2x slower on the raw parse
/// budget (the pass structure stays the original's: copy, then invert).
fn invert_in_place(image: &mut [u8], depth: usize) {
    match depth {
        3 => {
            for pixel in image.as_chunks_mut::<3>().0 {
                pixel.reverse();
            }
        }
        4 => {
            for pixel in image.as_chunks_mut::<4>().0 {
                pixel.reverse();
            }
        }
        d if d > 2 => {
            for pixel in image.chunks_exact_mut(depth) {
                pixel.reverse();
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The header of [`sample_raw24`], for building expectations.
    fn sample_header() -> TgaHeader {
        TgaHeader {
            id_length: 0,
            color_map_type: 0,
            image_type: TGA_TRUECOLOR,
            cmap_start: 0,
            cmap_length: 0,
            cmap_depth: 0,
            x_offset: 0,
            y_offset: 0,
            width: 2,
            height: 1,
            pixel_depth: 24,
            image_descriptor: TGAIDF_YORIGIN,
        }
    }

    /// A minimal well-formed 2×1 24-bit TGA with top-left origin: header,
    /// two BGR pixels, and a non-signature tail (a pre-TGA-2.0 file). The
    /// loader flips it (descriptor bit) and inverts the pixels (invert_image
    /// default).
    fn sample_raw24() -> Vec<u8> {
        let mut bytes = sample_header().to_bytes().to_vec();
        bytes.extend_from_slice(&[1, 2, 3, 4, 5, 6]); // two BGR pixels
        // A 26-byte non-signature tail: short of the TGA 2.0 signature, so
        // the footer sniff treats the file as 1.0.
        bytes.extend_from_slice(&[0u8; TGA2_FOOTER_SIZE]);
        bytes
    }

    #[test]
    fn parse_raw24_load_shape() {
        let file = TgaFile::parse(&sample_raw24()).unwrap();
        // InvertImage swapped each pixel's bytes, YFlip... a one-row image
        // exchanges nothing; the descriptor bit is cleared.
        assert_eq!(file.image, vec![3, 2, 1, 6, 5, 4]);
        assert_eq!(file.header.image_descriptor & TGAIDF_YORIGIN, 0);
        assert!(file.footer.is_none());
        assert!(file.extension.is_none());
    }

    #[test]
    fn texture_loader_shape_is_the_flip_inverse() {
        // The texture loader XORs the Y-origin bit and passes
        // invert_image=false. For this top-left sample the XOR clears the
        // bit, so no flip runs and the pixels keep the file's scanline
        // order, un-inverted.
        let file = TgaFile::parse_ex(&sample_raw24(), false, true, false, TGAIDF_YORIGIN).unwrap();
        assert_eq!(file.image, vec![1, 2, 3, 4, 5, 6]);
        assert_eq!(file.header.image_descriptor & TGAIDF_YORIGIN, 0);
    }

    #[test]
    fn bottom_left_file_is_flipped_for_dx8() {
        // The standard TGA origin is bottom-left (descriptor bit clear);
        // the texture loader's XOR sets it, so Load mirrors the rows —
        // "DX8 uses image upside down compared to TGA" — and clears it.
        let header = TgaHeader {
            image_type: TGA_TRUECOLOR,
            width: 1,
            height: 2,
            pixel_depth: 24,
            ..TgaHeader::default()
        };
        let mut bytes = header.to_bytes().to_vec();
        // Bottom row first (bottom-left origin): [10,20,30] then [40,50,60].
        bytes.extend_from_slice(&[10, 20, 30, 40, 50, 60]);
        bytes.extend_from_slice(&[0u8; TGA2_FOOTER_SIZE]);
        let file = TgaFile::parse_ex(&bytes, false, true, false, TGAIDF_YORIGIN).unwrap();
        assert_eq!(file.image, vec![40, 50, 60, 10, 20, 30]);
        assert_eq!(file.header.image_descriptor & TGAIDF_YORIGIN, 0);
    }

    #[test]
    fn save_round_trip_bytes() {
        // parse → to_bytes (default save shape) must reproduce the file
        // byte-for-byte except two canonicalizations: the load cleared the
        // descriptor's origin bit (the image is now origin-normalized), and
        // the save appends the footer the original always adds — a real
        // Targa 2.0 footer replacing the zero tail.
        let original = sample_raw24();
        let file = TgaFile::parse(&original).unwrap();
        let saved = file.to_bytes(SaveFlags::IMAGE, false).unwrap();
        let mut expected = TgaHeader {
            image_descriptor: 0,
            ..sample_header()
        }
        .to_bytes()
        .to_vec();
        expected.extend_from_slice(&[1, 2, 3, 4, 5, 6]);
        expected.extend_from_slice(&Tga2Footer::default().to_bytes());
        assert_eq!(saved, expected);
        // …and the saved form is the fixed point.
        let reparsed = TgaFile::parse(&saved).unwrap();
        assert_eq!(reparsed.to_bytes(SaveFlags::IMAGE, false).unwrap(), saved);
    }

    #[test]
    fn sub_footer_file_never_opens() {
        // The original's footer seek fails on an 18-byte header-only file.
        let mut bytes = sample_raw24();
        bytes.truncate(TGA_HEADER_SIZE);
        assert_eq!(TgaFile::parse(&bytes), Err(TgaError::Truncated));
    }

    #[test]
    fn mono_encoded_is_rejected_by_load() {
        let header = TgaHeader {
            image_type: TGA_MONO_ENCODED,
            width: 2,
            height: 1,
            pixel_depth: 8,
            ..TgaHeader::default()
        };
        let mut bytes = header.to_bytes().to_vec();
        bytes.extend_from_slice(&[0, 0, 0]);
        // Pad past the footer size.
        bytes.extend_from_slice(&[0u8; TGA2_FOOTER_SIZE]);
        assert_eq!(TgaFile::parse(&bytes), Err(TgaError::UnsupportedImageType));
    }

    #[test]
    fn rle_overrun_is_refused() {
        // A 2-pixel image whose RLE stream supplies three: the C++ reads
        // the packet (the bytes are there) and writes it past its buffer;
        // the port refuses with ImageOverrun once the bytes are present.
        let header = TgaHeader {
            image_type: TGA_TRUECOLOR_ENCODED,
            width: 2,
            height: 1,
            pixel_depth: 24,
            ..TgaHeader::default()
        };
        let mut bytes = header.to_bytes().to_vec();
        bytes.extend_from_slice(&[0x02]); // raw packet: 3 pixels (one too many)
        bytes.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8, 9]);
        bytes.extend_from_slice(&[0u8; TGA2_FOOTER_SIZE]);
        assert_eq!(TgaFile::parse(&bytes), Err(TgaError::ImageOverrun));
    }

    #[test]
    fn encode_rle_shapes() {
        // One pixel: a raw packet with the off-by-one count byte.
        assert_eq!(encode_rle(&[10, 20, 30], 3), vec![0x00, 10, 20, 30]);
        // Three identical pixels: a run packet, count byte = 2 | 0x80.
        assert_eq!(encode_rle(&[7, 7, 7], 1), vec![0x82, 7]);
        // 128 identical pixels: exactly one 128-run (count byte 127 | 0x80).
        let long = encode_rle(&[9; 128], 1);
        assert_eq!(long, vec![0xFF, 9]);
        // A run interrupting a raw packet: the raw packet flushes first
        // (count byte 0x01 = one less than its two pixels) and the pending
        // run closes at the next mismatch — a 2-pixel run (0x81) — before
        // the final pixel lands in a closing raw packet.
        assert_eq!(
            encode_rle(&[1, 2, 3, 3, 4], 1),
            vec![0x01, 1, 2, 0x81, 3, 0x00, 4]
        );
        // 129 identical pixels: the 128-run saturates, the last pixel is a
        // single raw one (the final compare is pinned to mismatch).
        let longer = encode_rle(&[9; 129], 1);
        assert_eq!(longer, vec![0xFF, 9, 0x00, 9]);
    }

    #[test]
    fn decode_rejects_oversized_header() {
        // A 4×4 mono image whose data extends past a 26-byte file: the
        // footer seek succeeds (the file is exactly footer-sized, the
        // signature does not match), the header reads, and the image read
        // runs past EOF — the original's TGAERR_READ, the port's
        // Truncated.
        let header = TgaHeader {
            image_type: TGA_MONO,
            width: 4,
            height: 4,
            pixel_depth: 8,
            ..TgaHeader::default()
        };
        let mut bytes = header.to_bytes().to_vec();
        bytes.extend_from_slice(&[0u8; 8]); // 26 bytes total
        assert_eq!(bytes.len(), 26);
        assert_eq!(TgaFile::parse(&bytes), Err(TgaError::Truncated));
    }

    #[test]
    fn extension_stamps_match_the_original() {
        // Save with addextension stamps the extension the original stamps.
        let file = TgaFile::parse(&sample_raw24()).unwrap();
        let saved = file.to_bytes(SaveFlags::IMAGE, true).unwrap();
        let parsed = TgaFile::parse(&saved).unwrap();
        let extension = parsed.extension.expect("extension present");
        assert_eq!(extension.ext_size, 495);
        assert_eq!(&extension.soft_id[..19], b"Denzil's Targa Code");
        assert_eq!(extension.soft_ver_number, 100);
        let footer = parsed.footer.expect("footer present");
        assert_eq!(
            footer.extension as usize,
            saved.len() - TGA2_FOOTER_SIZE - TGA2_EXTENSION_SIZE
        );
        assert_eq!(footer.developer, 0);
    }

    #[test]
    fn zero_size_image_skips_everything() {
        // A zero-width file: the original allocates nothing, skips the
        // switch (even the unsupported-type rejection) and the origin bits.
        let header = TgaHeader {
            image_type: TGA_MONO_ENCODED,
            width: 0,
            height: 4,
            pixel_depth: 8,
            image_descriptor: TGAIDF_YORIGIN,
            ..TgaHeader::default()
        };
        let mut bytes = header.to_bytes().to_vec();
        bytes.extend_from_slice(&[0u8; TGA2_FOOTER_SIZE]);
        let file = TgaFile::parse(&bytes).unwrap();
        assert!(file.image.is_empty());
        assert_eq!(file.header.image_descriptor, TGAIDF_YORIGIN);
    }
}
