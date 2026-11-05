// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Ported from the Zero Hour source release (baseline 571a7c5):
//   GeneralsMD/Code/Libraries/Source/WWVegas/WW3D2/ddsfile.{h,cpp}
//     (DDSFileClass: the constructor, Load, the level queries, Get_Pixel,
//      Get_4x4_Block, Copy_Level_To_Surface, Copy_CubeMap_Level_To_Surface,
//      the dead Get_Volume_Memory_Pointer / Copy_Volume_Level_To_Surface,
//      Calculate_DXTC_Surface_Size, RGB565_To_ARGB8888,
//      ARGB8888_To_RGB565, Combine_Colors)
//   Core/Libraries/Source/WWVegas/WW3D2/formconv.{h,cpp}
//     (Init_D3D_To_WW3_Conversion, the conversion array,
//      D3DFormat_To_WW3DFormat — the FourCC lookup the header drives)
//   Core/Libraries/Source/WWVegas/WW3D2/ww3dformat.{h,cpp}
//     (the WW3DFormat enum, Get_Bytes_Per_Pixel)
//   Core/Libraries/Source/WWVegas/WW3D2/bitmaphandler.h
//     (BitmapHandlerClass::Write_B8G8R8A8 — the per-format pixel writer
//      the block decompressor drives)
//   Core/Libraries/Source/WWVegas/WW3D2/colorspace.h
//     (RGB_To_HSV, HSV_To_RGB, the two Recolor overloads)
//   Core/Libraries/Source/WWVegas/WW3D2/dx8wrapper.h,
//   Core/Libraries/Source/WWVegas/WWMath/vector3.h
//     (DX8Wrapper::Convert_Color(unsigned) and the
//      Convert_Color(Vector3, float) → Vector3::Convert_To_ARGB(alpha)
//      composition the Recolor(unsigned&) round trip drives)
// See the module docs for the format layout and the documented
// differences from the C++ (out-of-bounds reads become errors; the
// uninitialised-tail header shape and the sub-three-character name are
// refused).

//! The **DDS** (DirectDraw Surface) texture format.
//!
//! Every texture that ships as `.dds` enters the game through
//! Westwood's DDS reader (`WW3D2/ddsfile.cpp`): the texture loader's
//! `Load_Compressed_Texture` constructs a `DDSFileClass` under a name
//! munged to `.dds`, checks availability (falling back to TGA when the
//! file is absent), calls `Load()`, and walks the mip levels with
//! `Copy_Level_To_Surface`; the team-colour recolour paths drive the
//! same copies with a non-zero HSV shift. This module ports the data
//! path so that every observable — the munged name, the header decode,
//! the mip-level layout, the read window, the pixel queries and the
//! surface copies — is **byte-exact with the original** (parity level
//! P0, pinned by golden vectors generated from the transcription in
//! `tools/differential/gen_dds_golden.cpp`).
//!
//! # File layout (as implemented here)
//!
//! All multi-byte integers are little-endian; the surface description
//! is the legacy DX7 `DDSURFACEDESC2` — 124 bytes on the game's 32-bit
//! build, with the `void* Surface` slot modelled as the 4-byte
//! placeholder the file stores (the loader never reads it).
//!
//! ```text
//! magic (4)    : never validated — any four bytes pass
//! desc (124)   : Size | Flags | Height | Width | Pitch | Depth |
//!                MipMapCount | AlphaBitDepth | Reserved | Surface |
//!                4 color keys (8 each) | PixelFormat (32) | Caps (16) |
//!                TextureStage
//! data         : the mip chain, biggest level first
//! ```
//!
//! The constructor reads the magic (unchecked) and the 124-byte
//! description, then requires **the read size to equal the `Size`
//! field** — which ties a passing load to `Size == 124` (a smaller
//! declared size only passes when the file supplies exactly that many
//! description bytes, leaving the tail of the struct uninitialised in
//! the original; this port refuses that shape, see below). The pixel
//! format is the `FourCC` field run through `D3DFormat_To_WW3DFormat`:
//! `DXT1`..`DXT5` map to the compressed formats, small numeric values
//! route through the conversion array (22 → `X8R8G8B8` — the load
//! continues with an *uncompressed* format), anything above the table
//! bound is `UNKNOWN`. Nothing else in the header is validated.
//!
//! # Level layout (transcribed)
//!
//! `MipMapCount == 0` means one level. A non-zero **reduction factor**
//! (the texture-quality setting) shifts the whole view into the chain:
//! the count loses the factor (or collapses to one level and decrements
//! the factor by one when the factor overruns the count), then the two
//! finest mip levels are dropped from the *count only* — the file keeps
//! them, `Load()` still reads them, and the level arrays simply stop
//! two entries early. The level sizes come from
//! [`calculate_dxtc_surface_size`] (`(w/4)·(h/4)` blocks, ×8 for DXT1,
//! ×16 for DXT2-5, ×1 for anything else — integer division, so a 5×5
//! surface is one block), divided by 4 (8 for volume textures, whose
//! level sizes also multiply by the depth) whenever they exceed one
//! 16-byte block. Cube maps sum the addressable level sizes plus a
//! hardcoded 16 bytes per face stride — exact for DXT1 (the two dropped
//! levels are 8+8 bytes) but 16 bytes short for DXT3/5 (16+16), so face
//! 1 of a DXT5 cube starts inside face 0's chain; the goldens pin the
//! drift. The level queries clamp every dimension up to four.
//!
//! # Differences from the C++ (documented)
//!
//! - The name munging overwrites the last three characters with
//!   `"dds"` — a **negative-index write when the name is shorter than
//!   three characters** (and when a NUL makes the effective name
//!   shorter). The original scribbles before the buffer; this port
//!   refuses with [`DdsError::NameTooShort`]. [`munged_name`] is the
//!   helper, exposed so the future VFS port reproduces the exact
//!   open-name the texture loader sees.
//! - A header whose `Size` field is not 124 but whose file supplies
//!   exactly `Size` description bytes passes the original's check with
//!   the struct tail **uninitialised** (nondeterministic). This port
//!   refuses with [`DdsError::SizeFieldMismatch`]; the clean failures
//!   (a longer file, a short file, an empty file) are
//!   [`DdsError::TruncatedHeader`] exactly as the original leaves the
//!   object unavailable.
//! - The original allocates `MipLevels` entries unconditionally; a
//!   huge `MipMapCount` walks off the heap. This port refuses more
//!   than [`MAX_MIP_LEVELS`] levels (64 — a 2^31-texel chain needs 32)
//!   with [`DdsError::TooManyMipLevels`], and more than
//!   [`MAX_DATA_BYTES`] of mip data with [`DdsError::TooLarge`] where
//!   the original's `malloc` would fail.
//! - `Load()`'s size arithmetic is unsigned: when the skipped levels
//!   overrun the data the size **underflows**, and the original's
//!   `size < 0x80000000` guard refuses the read while still returning
//!   success — a "loaded" object with no memory whose every consumer
//!   dereferences a null pointer. This port reproduces the success (the
//!   goldens pin `load ok` with no data) but reports
//!   [`DdsError::NoData`] from the queries instead of reading through
//!   the null.
//! - The pixel queries (`get_pixel`, `get_4x4_block`) and the copies
//!   read past the level data whenever the declared layout overruns the
//!   file; this port returns [`DdsError::DataTooShort`] /
//!   [`DdsError::BlockOutOfRange`] / [`DdsError::PixelOutOfRange`]
//!   instead. Their level/coordinate bounds are release no-op asserts
//!   in the original; the port makes them
//!   [`DdsError::LevelOutOfRange`] / coordinate errors.
//! - The copies on an unloaded file silently return in the original;
//!   the port reports [`DdsError::NoData`].
//! - `Write_B8G8R8A8`'s R3G3B2 case stores its 8-bit value through a
//!   16-bit pointer — one byte past the pixel, zero-extended by both
//!   compilers the game builds with. The port performs the same
//!   two-byte store, so an R3G3B2 destination buffer needs one spare
//!   byte past the last row (`3·pitch + 4 + 1` for a 4×4 block); the
//!   goldens pin the stray `0x00` over the `0xcc` fill.
//! - The D3D-surface overloads of `Copy_Level_To_Surface` /
//!   `Copy_CubeMap_Level_To_Surface` (the `IDirect3DSurface8*`
//!   lock/copy/unlock wrappers) are the Phase 4 renderer's plumbing and
//!   stay out of this data-path port; the in-memory overloads they
//!   forward to are ported.
//! - `Get_Date_Time` is VFS metadata (the texture reload check), not
//!   file content; the byte-source transcription leaves it zero and the
//!   port drops it.
//! - The `Pitch`/`Depth`/`MipMapCount` union slots are decoded by their
//!   DDS names (the loader only reads `Depth` and `MipMapCount`); the
//!   dump shows the union's first member, same bytes either way.

#![forbid(unsafe_code)]

use std::fmt;

// ---------------------------------------------------------------------------
// Format constants (ww3dformat.h / ddraw.h)
// ---------------------------------------------------------------------------

/// Size of the legacy `DDSURFACEDESC2` the header stores after the magic
/// (the 32-bit build's `sizeof`, the value the `Size` field must carry).
pub const DDS_SURFACE_DESC_SIZE: usize = 124;

/// Size of the never-validated magic.
pub const DDS_MAGIC_SIZE: usize = 4;

/// Length of the `Name` buffer the constructor munges.
pub const MAX_NAME_LEN: usize = 256;

/// The most mip levels a load accepts (the original's allocation is
/// unbounded; a `MipMapCount` near `u32::MAX` walks off the heap).
pub const MAX_MIP_LEVELS: u32 = 64;

/// The largest mip-chain payload a load accepts — the same budget the
/// TGA port allows, and an order of magnitude above the biggest texture
/// the game ships.
pub const MAX_DATA_BYTES: usize = 256 * 1024 * 1024;

/// `DDSCAPS2_CUBEMAP` (the DirectX 8 `ddraw.h` value).
pub const DDSCAPS2_CUBEMAP: u32 = 0x0000_0200;

/// `DDSCAPS2_VOLUME` (the DirectX 8 `ddraw.h` value).
pub const DDSCAPS2_VOLUME: u32 = 0x0020_0000;

/// `HIGHEST_SUPPORTED_D3DFORMAT` = `D3DFMT_X8L8V8U8` (62): the bound of
/// the formconv conversion array.
pub const HIGHEST_SUPPORTED_D3DFORMAT: u32 = 62;

// ---------------------------------------------------------------------------
// WW3DFormat (ww3dformat.h) — the surface-format enum the DDS path drives
// ---------------------------------------------------------------------------

/// The WW3D surface formats (the `WW3DFormat` enum, ordinal values).
///
/// The DDS reader derives this from the header's `FourCC` field: the
/// DXT codes map directly, small numeric values index the formconv
/// conversion array, everything else is [`Ww3dFormat::Unknown`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum Ww3dFormat {
    /// `WW3D_FORMAT_UNKNOWN`
    Unknown = 0,
    /// `WW3D_FORMAT_R8G8B8`
    R8G8B8 = 1,
    /// `WW3D_FORMAT_A8R8G8B8`
    A8R8G8B8 = 2,
    /// `WW3D_FORMAT_X8R8G8B8`
    X8R8G8B8 = 3,
    /// `WW3D_FORMAT_R5G6B5`
    R5G6B5 = 4,
    /// `WW3D_FORMAT_X1R5G5B5`
    X1R5G5B5 = 5,
    /// `WW3D_FORMAT_A1R5G5B5`
    A1R5G5B5 = 6,
    /// `WW3D_FORMAT_A4R4G4B4`
    A4R4G4B4 = 7,
    /// `WW3D_FORMAT_R3G3B2`
    R3G3B2 = 8,
    /// `WW3D_FORMAT_A8`
    A8 = 9,
    /// `WW3D_FORMAT_A8R3G3B2`
    A8R3G3B2 = 10,
    /// `WW3D_FORMAT_X4R4G4B4`
    X4R4G4B4 = 11,
    /// `WW3D_FORMAT_A8P8`
    A8P8 = 12,
    /// `WW3D_FORMAT_P8`
    P8 = 13,
    /// `WW3D_FORMAT_L8`
    L8 = 14,
    /// `WW3D_FORMAT_A8L8`
    A8L8 = 15,
    /// `WW3D_FORMAT_A4L4`
    A4L4 = 16,
    /// `WW3D_FORMAT_U8V8`
    U8V8 = 17,
    /// `WW3D_FORMAT_L6V5U5`
    L6V5U5 = 18,
    /// `WW3D_FORMAT_X8L8V8U8`
    X8L8V8U8 = 19,
    /// `WW3D_FORMAT_DXT1`
    Dxt1 = 20,
    /// `WW3D_FORMAT_DXT2`
    Dxt2 = 21,
    /// `WW3D_FORMAT_DXT3`
    Dxt3 = 22,
    /// `WW3D_FORMAT_DXT4`
    Dxt4 = 23,
    /// `WW3D_FORMAT_DXT5`
    Dxt5 = 24,
}

impl Ww3dFormat {
    /// `D3DFormat_To_WW3DFormat`: the FourCC lookup — the DXT codes map
    /// directly, small numeric values index the conversion array
    /// (initialised exactly as `Init_D3D_To_WW3_Conversion` fills it),
    /// everything above [`HIGHEST_SUPPORTED_D3DFORMAT`] is unknown.
    #[must_use]
    pub fn from_d3d_format(d3d_format: u32) -> Ww3dFormat {
        match d3d_format {
            // The DXT-codes are created with FOURCC macro and thus can't be
            // placed in the conversion table
            0x3154_5844 => Ww3dFormat::Dxt1, // 'DXT1'
            0x3254_5844 => Ww3dFormat::Dxt2, // 'DXT2'
            0x3354_5844 => Ww3dFormat::Dxt3, // 'DXT3'
            0x3454_5844 => Ww3dFormat::Dxt4, // 'DXT4'
            0x3554_5844 => Ww3dFormat::Dxt5, // 'DXT5'
            _ => {
                if d3d_format > HIGHEST_SUPPORTED_D3DFORMAT {
                    Ww3dFormat::Unknown
                } else {
                    conversion_array(d3d_format)
                }
            }
        }
    }

    /// `Get_Bytes_Per_Pixel` (the DXT and paletted-unreachable default is
    /// the assert path's 0).
    #[must_use]
    pub fn bytes_per_pixel(self) -> u32 {
        match self {
            Self::A8R8G8B8 | Self::X8R8G8B8 | Self::X8L8V8U8 => 4,
            Self::R8G8B8 => 3,
            Self::R5G6B5
            | Self::X1R5G5B5
            | Self::A1R5G5B5
            | Self::A4R4G4B4
            | Self::A8R3G3B2
            | Self::X4R4G4B4
            | Self::A8P8
            | Self::A8L8
            | Self::U8V8
            | Self::L6V5U5 => 2,
            Self::R3G3B2 | Self::A8 | Self::P8 | Self::L8 | Self::A4L4 => 1,
            Self::Unknown | Self::Dxt1 | Self::Dxt2 | Self::Dxt3 | Self::Dxt4 | Self::Dxt5 => 0,
        }
    }

    /// The lowercase dump name the golden script uses.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::R8G8B8 => "r8g8b8",
            Self::A8R8G8B8 => "a8r8g8b8",
            Self::X8R8G8B8 => "x8r8g8b8",
            Self::R5G6B5 => "r5g6b5",
            Self::X1R5G5B5 => "x1r5g5b5",
            Self::A1R5G5B5 => "a1r5g5b5",
            Self::A4R4G4B4 => "a4r4g4b4",
            Self::R3G3B2 => "r3g3b2",
            Self::A8 => "a8",
            Self::A8R3G3B2 => "a8r3g3b2",
            Self::X4R4G4B4 => "x4r4g4b4",
            Self::A8P8 => "a8p8",
            Self::P8 => "p8",
            Self::L8 => "l8",
            Self::A8L8 => "a8l8",
            Self::A4L4 => "a4l4",
            Self::U8V8 => "u8v8",
            Self::L6V5U5 => "l6v5u5",
            Self::X8L8V8U8 => "x8l8v8u8",
            Self::Dxt1 => "dxt1",
            Self::Dxt2 => "dxt2",
            Self::Dxt3 => "dxt3",
            Self::Dxt4 => "dxt4",
            Self::Dxt5 => "dxt5",
        }
    }

    /// The dump name parsed back (the golden test's inverse).
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "unknown" => Self::Unknown,
            "r8g8b8" => Self::R8G8B8,
            "a8r8g8b8" => Self::A8R8G8B8,
            "x8r8g8b8" => Self::X8R8G8B8,
            "r5g6b5" => Self::R5G6B5,
            "x1r5g5b5" => Self::X1R5G5B5,
            "a1r5g5b5" => Self::A1R5G5B5,
            "a4r4g4b4" => Self::A4R4G4B4,
            "r3g3b2" => Self::R3G3B2,
            "a8" => Self::A8,
            "a8r3g3b2" => Self::A8R3G3B2,
            "x4r4g4b4" => Self::X4R4G4B4,
            "a8p8" => Self::A8P8,
            "p8" => Self::P8,
            "l8" => Self::L8,
            "a8l8" => Self::A8L8,
            "a4l4" => Self::A4L4,
            "u8v8" => Self::U8V8,
            "l6v5u5" => Self::L6V5U5,
            "x8l8v8u8" => Self::X8L8V8U8,
            "dxt1" => Self::Dxt1,
            "dxt2" => Self::Dxt2,
            "dxt3" => Self::Dxt3,
            "dxt4" => Self::Dxt4,
            "dxt5" => Self::Dxt5,
            _ => return None,
        })
    }
}

/// The formconv conversion array entry (the table
/// `Init_D3D_To_WW3_Conversion` fills; only the non-unknown entries are
/// listed, keyed by the `D3DFMT_*` value).
fn conversion_array(d3d_format: u32) -> Ww3dFormat {
    match d3d_format {
        20 => Ww3dFormat::R8G8B8,
        21 => Ww3dFormat::A8R8G8B8,
        22 => Ww3dFormat::X8R8G8B8,
        23 => Ww3dFormat::R5G6B5,
        24 => Ww3dFormat::X1R5G5B5,
        25 => Ww3dFormat::A1R5G5B5,
        26 => Ww3dFormat::A4R4G4B4,
        27 => Ww3dFormat::R3G3B2,
        28 => Ww3dFormat::A8,
        29 => Ww3dFormat::A8R3G3B2,
        30 => Ww3dFormat::X4R4G4B4,
        40 => Ww3dFormat::A8P8,
        41 => Ww3dFormat::P8,
        50 => Ww3dFormat::L8,
        51 => Ww3dFormat::A8L8,
        52 => Ww3dFormat::A4L4,
        60 => Ww3dFormat::U8V8,
        61 => Ww3dFormat::L6V5U5,
        62 => Ww3dFormat::X8L8V8U8,
        _ => Ww3dFormat::Unknown,
    }
}

// ---------------------------------------------------------------------------
// The header struct (ddsfile.h's legacy DX7 description)
// ---------------------------------------------------------------------------

/// One legacy color key (8 bytes).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LegacyDdColorKey {
    /// `ColorSpaceLowValue`
    pub color_space_low_value: u32,
    /// `ColorSpaceHighValue`
    pub color_space_high_value: u32,
}

/// The legacy DX7 caps2 block (16 bytes).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LegacyDdCaps2 {
    /// `Caps`
    pub caps: u32,
    /// `Caps2` — the cube-map/volume flags the type detection reads
    pub caps2: u32,
    /// `Caps3`
    pub caps3: u32,
    /// `Caps4`
    pub caps4: u32,
}

/// The legacy DX7 pixel format (32 bytes). The loader reads only
/// [`four_cc`](Self::four_cc); the union slots are decoded by their DDS
/// names.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LegacyDdPixelFormat {
    /// `Size`
    pub size: u32,
    /// `Flags`
    pub flags: u32,
    /// `FourCC` — the format selector
    pub four_cc: u32,
    /// `RGBBitCount` (the first union member)
    pub rgb_bit_count: u32,
    /// `RBitMask` (the first union member)
    pub r_bit_mask: u32,
    /// `GBitMask` (the first union member)
    pub g_bit_mask: u32,
    /// `BBitMask` (the first union member)
    pub b_bit_mask: u32,
    /// `RGBAlphaBitMask` (the first union member)
    pub rgb_alpha_bit_mask: u32,
}

/// The legacy DX7 surface description — the 124 bytes after the magic.
/// The loader reads `size`, `height`, `width`, `depth`, `mip_map_count`,
/// `pixel_format.four_cc` and `caps.caps2`; everything else rides along.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LegacyDdSurfaceDesc2 {
    /// `Size` — must equal 124 for a deterministic load
    pub size: u32,
    /// `Flags` (never read)
    pub flags: u32,
    /// `Height`
    pub height: u32,
    /// `Width`
    pub width: u32,
    /// `Pitch` / `LinearSize` union (never read)
    pub pitch: u32,
    /// `Depth` / `BackBufferCount` union — the volume slice count
    pub depth: u32,
    /// `MipMapCount` / `RefreshRate` union
    pub mip_map_count: u32,
    /// `AlphaBitDepth` (never read)
    pub alpha_bit_depth: u32,
    /// `Reserved` (never read)
    pub reserved: u32,
    /// `Surface` — the 32-bit pointer slot (never read)
    pub surface: u32,
    /// `CKDestOverlay` / `EmptyFaceColor` union
    pub ck_dest_overlay: LegacyDdColorKey,
    /// `CKDestBlt`
    pub ck_dest_blt: LegacyDdColorKey,
    /// `CKSrcOverlay`
    pub ck_src_overlay: LegacyDdColorKey,
    /// `CKSrcBlt`
    pub ck_src_blt: LegacyDdColorKey,
    /// `PixelFormat`
    pub pixel_format: LegacyDdPixelFormat,
    /// `Caps`
    pub caps: LegacyDdCaps2,
    /// `TextureStage` (never read)
    pub texture_stage: u32,
}

impl LegacyDdSurfaceDesc2 {
    /// Decodes the 124-byte description (little-endian, no padding).
    fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < DDS_SURFACE_DESC_SIZE {
            return None;
        }
        let u32_at = |at: usize| -> u32 {
            u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
        };
        let key_at = |at: usize| -> LegacyDdColorKey {
            LegacyDdColorKey {
                color_space_low_value: u32_at(at),
                color_space_high_value: u32_at(at + 4),
            }
        };
        Some(Self {
            size: u32_at(0),
            flags: u32_at(4),
            height: u32_at(8),
            width: u32_at(12),
            pitch: u32_at(16),
            depth: u32_at(20),
            mip_map_count: u32_at(24),
            alpha_bit_depth: u32_at(28),
            reserved: u32_at(32),
            surface: u32_at(36),
            ck_dest_overlay: key_at(40),
            ck_dest_blt: key_at(48),
            ck_src_overlay: key_at(56),
            ck_src_blt: key_at(64),
            pixel_format: LegacyDdPixelFormat {
                size: u32_at(72),
                flags: u32_at(76),
                four_cc: u32_at(80),
                rgb_bit_count: u32_at(84),
                r_bit_mask: u32_at(88),
                g_bit_mask: u32_at(92),
                b_bit_mask: u32_at(96),
                rgb_alpha_bit_mask: u32_at(100),
            },
            caps: LegacyDdCaps2 {
                caps: u32_at(104),
                caps2: u32_at(108),
                caps3: u32_at(112),
                caps4: u32_at(116),
            },
            texture_stage: u32_at(120),
        })
    }
}

/// `DDSType` — which kind of surface the caps flags declare.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum DdsType {
    /// `DDS_TEXTURE` — a plain 2D surface.
    #[default]
    Texture = 0,
    /// `DDS_CUBEMAP` — six faces of chains.
    Cubemap = 1,
    /// `DDS_VOLUME` — depth slices (the pointer half is dead in the
    /// original; the layout half is not).
    Volume = 2,
}

// ---------------------------------------------------------------------------
// The HSV recolor stack (colorspace.h / dx8wrapper.h / vector3.h)
// ---------------------------------------------------------------------------

/// The HSV shift the recolouring copies apply — the `(h, s, v)` triple
/// the original passes as a `Vector3` (degrees, 0..1, 0..1, added
/// verbatim; hue wraps at ±360, saturation and value clamp).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct HsvShift {
    /// The hue shift in degrees (added, then wrapped into `[0, 360)`).
    pub h: f32,
    /// The saturation shift (clamped to `[0, 1]`).
    pub s: f32,
    /// The value shift (clamped to `[0, 1]`).
    pub v: f32,
}

impl HsvShift {
    /// A zero shift — the no-recolor default every texture load uses.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            h: 0.0,
            s: 0.0,
            v: 0.0,
        }
    }

    fn is_nonzero(self) -> bool {
        self.h != 0.0 || self.s != 0.0 || self.v != 0.0
    }
}

/// `RGB_To_HSV` (Foley et al.): `rgb` in 0..1 → `h` in `[0, 360)`, `s`
/// and `v` in 0..1; a negative hue flags a monochrome colour.
fn rgb_to_hsv(rgb: [f32; 3]) -> [f32; 3] {
    let mut max = rgb[0].max(rgb[1]);
    max = max.max(rgb[2]);
    let mut min = rgb[0].min(rgb[1]);
    min = min.min(rgb[2]);

    let v = max;
    let s = if max != 0.0 { (max - min) / max } else { 0.0 };
    if s == 0.0 {
        [-1.0, s, v]
    } else {
        let delta = max - min;
        // The original's if/else-if chain: X first, then Y, then Z.
        let mut h = if rgb[0] == max {
            (rgb[1] - rgb[2]) / delta
        } else if rgb[1] == max {
            2.0 + (rgb[2] - rgb[0]) / delta
        } else {
            4.0 + (rgb[0] - rgb[1]) / delta
        };
        h *= 60.0;
        if h < 0.0 {
            h += 360.0;
        }
        [h, s, v]
    }
}

/// `HSV_To_RGB` — the six sextant cases of the inverse.
fn hsv_to_rgb(hsv: [f32; 3]) -> [f32; 3] {
    let mut h = hsv[0];
    let s = hsv[1];
    let v = hsv[2];

    if s == 0.0 {
        return [v, v, v];
    }

    if h == 360.0 {
        h = 0.0;
    }
    h /= 60.0;
    let i = h.floor();
    let f = h - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - (s * f));
    let t = v * (1.0 - (s * (1.0 - f)));
    match i as i32 {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        5 => [v, p, q],
        // Unreachable for in-range hues; the original's switch falls
        // through leaving the output untouched (it never happens after
        // the caller's angular mod).
        _ => [0.0, 0.0, 0.0],
    }
}

/// `Recolor(Vector3&, shift)` — the hue/saturation/value shift with its
/// monochrome special case (an undefined hue shifts only the value).
fn recolor_rgb(rgb: [f32; 3], shift: HsvShift) -> [f32; 3] {
    let mut hsv = rgb_to_hsv(rgb);

    // If the Hue has the "undefined flag" (a negative value), this means
    // that the color is pure monochrome: don't shift the hue (it is
    // undefined) or the saturation — only the value.
    if hsv[0] < 0.0 {
        hsv[2] += shift.v;
    } else {
        hsv[0] += shift.h;
        hsv[1] += shift.s;
        hsv[2] += shift.v;
    }

    // angular mod
    if hsv[0] < 0.0 {
        hsv[0] += 360.0;
    }
    if hsv[0] > 360.0 {
        hsv[0] -= 360.0;
    }
    // clamp saturation and value
    hsv[1] = hsv[1].clamp(0.0, 1.0);
    hsv[2] = hsv[2].clamp(0.0, 1.0);
    hsv_to_rgb(hsv)
}

/// `Recolor(unsigned&, shift)` — the packed-colour round trip:
/// `Convert_Color(unsigned)` unpacks BGRA bytes to floats, the shift
/// runs, and `Convert_To_ARGB(alpha)` truncates back (the alpha byte
/// rides through unchanged).
#[must_use]
pub fn recolor(rgba: u32, shift: HsvShift) -> u32 {
    // DX8Wrapper::Convert_Color(unsigned) — the byte lanes to floats
    // (exact for the 0..255 range)
    let a = ((rgba & 0xff00_0000) >> 24) as f32 / 255.0;
    let rgb = [
        ((rgba & 0xff0000) >> 16) as f32 / 255.0,
        ((rgba & 0xff00) >> 8) as f32 / 255.0,
        (rgba & 0xff) as f32 / 255.0,
    ];
    let out = recolor_rgb(rgb, shift);
    // Vector3::Convert_To_ARGB(alpha): the float→int conversions
    // truncate toward zero, exactly like the C++'s `unsigned(float)`.
    ((a * 255.0) as u32) << 24
        | ((out[0] * 255.0) as u32) << 16
        | ((out[1] * 255.0) as u32) << 8
        | (out[2] * 255.0) as u32
}

// ---------------------------------------------------------------------------
// The pixel helpers (ddsfile.cpp)
// ---------------------------------------------------------------------------

/// `RGB565_To_ARGB8888` — the 5/6/5 fields shifted into the truncated
/// 8-bit lanes of the internal "ARGB" (blue low, red at bits 16..20;
/// the low bits of every lane stay zero).
#[must_use]
pub fn rgb565_to_argb8888(rgb: u16) -> u32 {
    let rgb = u32::from(rgb);
    (rgb & 0x001f) << 3 | (rgb & 0x07e0) << 5 | (rgb & 0xf800) << 8
}

/// `ARGB8888_To_RGB565` — the byte-lane masks recover the 5/6/5 fields
/// (a round trip through both is the identity).
#[must_use]
pub fn argb8888_to_rgb565(argb: u32) -> u16 {
    let bytes = argb.to_le_bytes();
    (((bytes[2]) & 0xf8) as u16) << 8
        | (((bytes[1]) & 0xfc) as u16) << 3
        | (((bytes[0]) & 0xf8) as u16) >> 3
}

/// `Combine_Colors(col1, col2, rel)` — the 8-bit-lane interpolation the
/// DXT decoders use (85 ≈ one third, 128 ≈ one half).
#[must_use]
pub fn combine_colors(col1: u32, col2: u32, rel: u32) -> u32 {
    const R_B_MASK: u32 = 0x00ff_00ff;
    const G_MASK: u32 = 0x0000_ff00;

    let rel2 = 255 - rel;

    let mut r_b_col1 = col1 & R_B_MASK;
    r_b_col1 = r_b_col1.wrapping_mul(rel);
    let mut r_b_col2 = col2 & R_B_MASK;
    r_b_col2 = r_b_col2.wrapping_mul(rel2);
    r_b_col1 = r_b_col1.wrapping_add(r_b_col2);
    r_b_col1 >>= 8;
    r_b_col1 &= R_B_MASK;

    let mut g_col1 = col1 & G_MASK;
    g_col1 = g_col1.wrapping_mul(rel);
    let mut g_col2 = col2 & G_MASK;
    g_col2 = g_col2.wrapping_mul(rel2);
    g_col1 = g_col1.wrapping_add(g_col2);
    g_col1 >>= 8;
    g_col1 &= G_MASK;

    r_b_col1 | g_col1
}

/// `Calculate_DXTC_Surface_Size` — `(w/4)·(h/4)` blocks, ×8 for DXT1,
/// ×16 for DXT2-5, ×1 for anything else (the unknown-format fallback:
/// no multiplier at all).
#[must_use]
pub fn calculate_dxtc_surface_size(width: u32, height: u32, format: Ww3dFormat) -> u32 {
    let mut level_size = (width / 4).wrapping_mul(height / 4);
    match format {
        Ww3dFormat::Dxt1 => level_size = level_size.wrapping_mul(8),
        Ww3dFormat::Dxt2 | Ww3dFormat::Dxt3 | Ww3dFormat::Dxt4 | Ww3dFormat::Dxt5 => {
            level_size = level_size.wrapping_mul(16);
        }
        _ => {}
    }
    level_size
}

// ---------------------------------------------------------------------------
// Write_B8G8R8A8 (bitmaphandler.h) — the destination-format pixel writer
// ---------------------------------------------------------------------------

/// `BitmapHandlerClass::Write_B8G8R8A8` — writes one "B8G8R8A8"-ordered
/// 32-bit pixel (`b` in the low byte) into `dest` at `at`, converting
/// to the destination format.
///
/// The R3G3B2 case stores **two** bytes (the original's 16-bit store of
/// a zero-extended 8-bit value), so the last pixel of a row writes one
/// byte past the pixel — the caller's buffer must have the spare byte.
/// The DXT/paletted destinations are the original's assert path (a
/// release no-op that writes nothing); the port reports
/// [`DdsError::UnsupportedDestination`].
fn write_block_pixels(
    dest: &mut [u8],
    dest_pitch: usize,
    format: Ww3dFormat,
    pixels: &[u32; 16],
) -> Result<(), DdsError> {
    // One bounds check for the whole block: three row strides plus one
    // pixel row, plus the R3G3B2 stray byte.
    let bpp = format.bytes_per_pixel() as usize;
    let stray = if format == Ww3dFormat::R3G3B2 { 1 } else { 0 };
    let need = 3 * dest_pitch + 4 * bpp + stray;
    if dest.len() < need {
        return Err(DdsError::DestTooShort);
    }
    match format {
        Ww3dFormat::A8R8G8B8 | Ww3dFormat::X8R8G8B8 => {
            for y in 0..4usize {
                for x in 0..4usize {
                    let at = y * dest_pitch + x * 4;
                    dest[at..at + 4].copy_from_slice(&pixels[y * 4 + x].to_le_bytes());
                }
            }
        }
        Ww3dFormat::R8G8B8 => {
            for y in 0..4usize {
                for x in 0..4usize {
                    let at = y * dest_pitch + x * 3;
                    let b = pixels[y * 4 + x].to_le_bytes();
                    dest[at..at + 3].copy_from_slice(&b[..3]);
                }
            }
        }
        Ww3dFormat::A4R4G4B4 => {
            for y in 0..4usize {
                for x in 0..4usize {
                    let b = pixels[y * 4 + x].to_le_bytes();
                    let tmp = ((u32::from(b[3]) & 0xf0) << 8
                        | (u32::from(b[2]) & 0xf0) << 4
                        | (u32::from(b[1]) & 0xf0)
                        | (u32::from(b[0]) & 0xf0) >> 4) as u16;
                    let at = y * dest_pitch + x * 2;
                    dest[at..at + 2].copy_from_slice(&tmp.to_le_bytes());
                }
            }
        }
        Ww3dFormat::A1R5G5B5 => {
            for y in 0..4usize {
                for x in 0..4usize {
                    let b = pixels[y * 4 + x].to_le_bytes();
                    let tmp = (if b[3] != 0 { 0x8000u16 } else { 0x0 })
                        | (((u32::from(b[2]) & 0xf8) as u16) << 7)
                        | (((u32::from(b[1]) & 0xf8) as u16) << 2)
                        | (((u32::from(b[0]) & 0xf8) as u16) >> 3);
                    let at = y * dest_pitch + x * 2;
                    dest[at..at + 2].copy_from_slice(&tmp.to_le_bytes());
                }
            }
        }
        Ww3dFormat::R5G6B5 => {
            for y in 0..4usize {
                for x in 0..4usize {
                    let b = pixels[y * 4 + x].to_le_bytes();
                    let tmp = (((u32::from(b[2]) & 0xf8) as u16) << 8)
                        | (((u32::from(b[1]) & 0xfc) as u16) << 3)
                        | (((u32::from(b[0]) & 0xf8) as u16) >> 3);
                    let at = y * dest_pitch + x * 2;
                    dest[at..at + 2].copy_from_slice(&tmp.to_le_bytes());
                }
            }
        }
        Ww3dFormat::R3G3B2 => {
            for y in 0..4usize {
                for x in 0..4usize {
                    let b = pixels[y * 4 + x].to_le_bytes();
                    let tmp: u8 = (b[2] & 0xe0) | (b[1] & 0xe0) >> 3 | (b[0] & 0xc0) >> 6;
                    // The original's 16-bit store — one byte past the
                    // pixel, zero-extended (see the module docs).
                    let at = y * dest_pitch + x;
                    dest[at..at + 2].copy_from_slice(&u16::from(tmp).to_le_bytes());
                }
            }
        }
        Ww3dFormat::L8 => {
            for y in 0..4usize {
                for x in 0..4usize {
                    let b = pixels[y * 4 + x].to_le_bytes();
                    // CIE Req. 709: Y709 = 0.2125R + 0.7154G + 0.0721B
                    let tmp = ((u32::from(b[0]) * 0x1275)
                        + (u32::from(b[1]) * 0xB725)
                        + (u32::from(b[2]) * 0x3666))
                        >> 16;
                    dest[y * dest_pitch + x] = tmp as u8;
                }
            }
        }
        Ww3dFormat::A8 => {
            for y in 0..4usize {
                for x in 0..4usize {
                    dest[y * dest_pitch + x] = pixels[y * 4 + x] as u8;
                }
            }
        }
        // Paletted destination not supported (the original's assert).
        _ => return Err(DdsError::UnsupportedDestination),
    }
    Ok(())
}

fn read_u16(bytes: &[u8], at: usize) -> Result<u16, DdsError> {
    let end = at + 2;
    if end > bytes.len() || end < at {
        return Err(DdsError::DataTooShort);
    }
    Ok(u16::from_le_bytes([bytes[at], bytes[at + 1]]))
}

fn read_u32(bytes: &[u8], at: usize) -> Result<u32, DdsError> {
    let end = at + 4;
    if end > bytes.len() || end < at {
        return Err(DdsError::DataTooShort);
    }
    Ok(u32::from_le_bytes([
        bytes[at],
        bytes[at + 1],
        bytes[at + 2],
        bytes[at + 3],
    ]))
}

// ---------------------------------------------------------------------------
// The file (DDSFileClass)
// ---------------------------------------------------------------------------

/// A parsed-and-loaded DDS file: the munged name, the header view, the
/// mip-level layout and the level-0-relative data `load` read.
#[derive(Debug, Clone)]
pub struct DdsFile {
    name: String,
    width: u32,
    height: u32,
    depth: u32,
    full_width: u32,
    full_height: u32,
    full_depth: u32,
    mip_levels: u32,
    reduction_factor: u32,
    format: Ww3dFormat,
    kind: DdsType,
    level_sizes: Vec<u32>,
    level_offsets: Vec<u32>,
    cube_face_size: u32,
    surface_desc: LegacyDdSurfaceDesc2,
    data: Vec<u8>,
    loaded: bool,
}

impl DdsFile {
    /// The constructor: munge `name` (the `.tga`-or-`.dds` contract —
    /// see [`munged_name`]), then parse the header out of `bytes` and
    /// build the mip-level layout for `reduction_factor` (the
    /// texture-quality setting the texture loader passes).
    ///
    /// The data payload is **not** read here — the original's two-step
    /// shape (constructor for the info, [`load`](Self::load) for the
    /// bytes) is preserved.
    pub fn open(name: &str, reduction_factor: u32, bytes: &[u8]) -> Result<Self, DdsError> {
        let name = munged_name(name)?;

        // int result=file->Open(); — the byte source is always open;
        // the empty-file shape dies on the magic read below.

        // unsigned read_bytes=file->Read(header,4);
        // if (!read_bytes) fail;
        if bytes.len() < DDS_MAGIC_SIZE {
            return Err(DdsError::Unavailable);
        }

        // read_bytes=file->Read(&SurfaceDesc,sizeof(LegacyDDSURFACEDESC2));
        // if (read_bytes==0 || read_bytes!=SurfaceDesc.Size) fail;
        let desc_read = (bytes.len() - DDS_MAGIC_SIZE).min(DDS_SURFACE_DESC_SIZE);
        if desc_read == 0 {
            return Err(DdsError::Unavailable);
        }
        if desc_read != DDS_SURFACE_DESC_SIZE {
            // A short file: the read size can never match the declared
            // size (which the passing path pins to 124).
            return Err(DdsError::TruncatedHeader);
        }
        let surface_desc = LegacyDdSurfaceDesc2::from_bytes(&bytes[DDS_MAGIC_SIZE..])
            .ok_or(DdsError::TruncatedHeader)?;
        if surface_desc.size != DDS_SURFACE_DESC_SIZE as u32 {
            // The only other passing shape in the original reads
            // exactly `size` bytes and leaves the struct tail
            // uninitialised — refused (see the module docs).
            return Err(DdsError::SizeFieldMismatch);
        }

        // Format=D3DFormat_To_WW3DFormat((D3DFORMAT)FourCC);
        // (the DXT-only assert is a release no-op — the load continues
        // with whatever the conversion produced, UNKNOWN included)
        let format = Ww3dFormat::from_d3d_format(surface_desc.pixel_format.four_cc);

        // MipLevels=SurfaceDesc.MipMapCount;
        // if (MipLevels==0) MipLevels=1;
        let mut mip_levels = surface_desc.mip_map_count;
        if mip_levels == 0 {
            mip_levels = 1;
        }

        let mut reduction_factor = reduction_factor;
        if mip_levels > reduction_factor {
            mip_levels -= reduction_factor;
        } else {
            mip_levels = 1;
            reduction_factor -= mip_levels;
        }

        // Drop the two lowest miplevels!
        if mip_levels > 2 {
            mip_levels -= 2;
        } else {
            mip_levels = 1;
        }

        if mip_levels > MAX_MIP_LEVELS {
            // The original's `new unsigned[MipLevels]` would walk off
            // the heap for a huge declared count.
            return Err(DdsError::TooManyMipLevels);
        }

        // check texture type, normal, cube or volume
        let kind = if surface_desc.caps.caps2 & DDSCAPS2_CUBEMAP != 0 {
            DdsType::Cubemap
        } else if surface_desc.caps.caps2 & DDSCAPS2_VOLUME != 0 {
            DdsType::Volume
        } else {
            DdsType::Texture
        };

        let full_width = surface_desc.width;
        let full_height = surface_desc.height;
        let full_depth = surface_desc.depth;
        let width = full_width >> reduction_factor;
        let height = full_height >> reduction_factor;
        let depth = full_depth;

        let mut level_size =
            calculate_dxtc_surface_size(surface_desc.width, surface_desc.height, format);
        let mut level_offset: u32 = 0;

        let mut level_mip_dec: u32 = 4;
        if kind == DdsType::Volume {
            // add slices to level data size
            level_size = level_size.wrapping_mul(surface_desc.depth);
            level_mip_dec = 8;
        }

        let mut level_sizes = vec![0u32; mip_levels as usize];
        let mut level_offsets = vec![0u32; mip_levels as usize];
        for _ in 0..reduction_factor {
            if level_size > 16 {
                // If surface is bigger than one block (8 or 16 bytes)...
                level_size /= level_mip_dec;
            }
        }
        for level in 0..mip_levels as usize {
            level_sizes[level] = level_size;
            level_offsets[level] = level_offset;
            level_offset = level_offset.wrapping_add(level_size);
            if level_size > 16 {
                // If surface is bigger than one block (8 or 16 bytes)...
                level_size /= level_mip_dec;
            }
        }

        let mut cube_face_size = 0;
        if kind == DdsType::Cubemap {
            cube_face_size = level_sizes
                .iter()
                .fold(0u32, |acc, &size| acc.wrapping_add(size));

            // this accounts for dropping 2 lowest mip levels
            if mip_levels > 2 {
                cube_face_size += 16;
            }
        }

        Ok(Self {
            name,
            width,
            height,
            depth,
            full_width,
            full_height,
            full_depth,
            mip_levels,
            reduction_factor,
            format,
            kind,
            level_sizes,
            level_offsets,
            cube_face_size,
            surface_desc,
            data: Vec::new(),
            loaded: false,
        })
    }

    /// `Load()` — seek past the header and the skipped mip levels and
    /// read the rest of the file as the level data.
    ///
    /// Returns `Ok(())` in every shape the original's `true` covers —
    /// **including the underflow shape** (the skipped levels overrun
    /// the data, the size arithmetic wraps past `0x80000000` and the
    /// read is refused): the file stays without data and the queries
    /// report [`DdsError::NoData`], exactly where the original hands
    /// its consumers a null pointer.
    pub fn load(&mut self, bytes: &[u8]) -> Result<(), DdsError> {
        if self.loaded {
            // if (DDSMemory) return false;
            return Err(DdsError::AlreadyLoaded);
        }
        if self.level_sizes.is_empty() {
            // if (!LevelSizes || !LevelOffsets) return false;
            return Err(DdsError::NoData);
        }

        // Data size is file size minus the header and info block
        let mut size = (bytes.len() as u64)
            .wrapping_sub(u64::from(self.surface_desc.size) + DDS_MAGIC_SIZE as u64);

        if size == 0 {
            return Err(DdsError::NoData);
        }

        // Skip mip levels if reduction factor is not zero
        let mut level_size = calculate_dxtc_surface_size(
            self.surface_desc.width,
            self.surface_desc.height,
            self.format,
        );

        let mut skipped_offset: u32 = 0;
        for _ in 0..self.reduction_factor {
            skipped_offset = skipped_offset.wrapping_add(level_size);
            size = size.wrapping_sub(u64::from(level_size));
            if level_size > 16 {
                // If surface is bigger than one block (8 or 16 bytes)...
                level_size /= 4;
            }
        }

        // Skip the header and info block and possible unused mip levels
        let seek_target =
            DDS_MAGIC_SIZE as u64 + u64::from(self.surface_desc.size) + u64::from(skipped_offset);

        if size != 0 && size < 0x8000_0000 {
            if size as usize > MAX_DATA_BYTES {
                // The original's `new unsigned char[size]` would throw.
                return Err(DdsError::TooLarge);
            }
            let start = usize::try_from(seek_target).map_err(|_| DdsError::SeekPastEnd)?;
            let end = start + size as usize;
            if end > bytes.len() || end < start {
                // The seek ran past the file: the original reads zero
                // bytes into an uninitialised allocation.
                return Err(DdsError::SeekPastEnd);
            }
            self.data = bytes[start..end].to_vec();
        }
        self.loaded = true;
        Ok(())
    }

    /// The munged name (the constructor's `.dds` overwrite).
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// `Get_Full_Width` — the unreduced level-0 width.
    #[must_use]
    pub fn full_width(&self) -> u32 {
        self.full_width
    }

    /// `Get_Full_Height` — the unreduced level-0 height.
    #[must_use]
    pub fn full_height(&self) -> u32 {
        self.full_height
    }

    /// `Get_Full_Depth` — the unreduced depth.
    #[must_use]
    pub fn full_depth(&self) -> u32 {
        self.full_depth
    }

    /// `Get_Mip_Level_Count` — the addressable levels (two fewer than
    /// the chain holds, after the reduction).
    #[must_use]
    pub fn mip_level_count(&self) -> u32 {
        self.mip_levels
    }

    /// The constructor's `Width` member — the reduced, **unclamped**
    /// width (the level queries clamp it up to four; the golden dump
    /// pins the raw value).
    #[must_use]
    pub fn reduced_width(&self) -> u32 {
        self.width
    }

    /// The constructor's `Height` member — reduced, unclamped.
    #[must_use]
    pub fn reduced_height(&self) -> u32 {
        self.height
    }

    /// The constructor's `Depth` member — the unreduced slice count
    /// (the level queries clamp it).
    #[must_use]
    pub fn reduced_depth(&self) -> u32 {
        self.depth
    }

    /// The constructor's (mutated) reduction factor — the else branch
    /// of the level arithmetic decrements it.
    #[must_use]
    pub fn reduction_factor(&self) -> u32 {
        self.reduction_factor
    }

    /// `Get_Format` — the FourCC-derived surface format.
    #[must_use]
    pub fn format(&self) -> Ww3dFormat {
        self.format
    }

    /// `Get_Type` — texture, cube map or volume.
    #[must_use]
    pub fn kind(&self) -> DdsType {
        self.kind
    }

    /// `Get_CubeFaceSize` — the per-face stride (with its hardcoded
    /// 16-byte correction for the two dropped levels).
    #[must_use]
    pub fn cube_face_size(&self) -> u32 {
        self.cube_face_size
    }

    /// The decoded surface description.
    #[must_use]
    pub fn surface_desc(&self) -> &LegacyDdSurfaceDesc2 {
        &self.surface_desc
    }

    /// `Get_Width(level)` — the reduced width, shifted per level and
    /// clamped up to four (a 1-texel level reports four).
    #[must_use]
    pub fn width(&self, level: u32) -> u32 {
        let width = self.width >> level;
        if width < 4 { 4 } else { width }
    }

    /// `Get_Height(level)` — clamped up to four.
    #[must_use]
    pub fn height(&self, level: u32) -> u32 {
        let height = self.height >> level;
        if height < 4 { 4 } else { height }
    }

    /// `Get_Depth(level)` — clamped up to four.
    #[must_use]
    pub fn depth(&self, level: u32) -> u32 {
        let depth = self.depth >> level;
        if depth < 4 { 4 } else { depth }
    }

    /// `Get_Level_Size(level)` — the DXTC block size of the level.
    pub fn level_size(&self, level: u32) -> Result<u32, DdsError> {
        self.level(level).map(|(size, _)| size)
    }

    /// The stored `LevelOffsets` entry — the level's byte offset into
    /// the read window (a prefix sum of the sizes, built by the
    /// constructor's walk).
    pub fn level_offset(&self, level: u32) -> Option<u32> {
        let idx = usize::try_from(level).ok()?;
        self.level_offsets.get(idx).copied()
    }

    /// `Get_Memory_Pointer(level)` — the level's data, from its offset
    /// to the end of the read window (the original returns a bare
    /// pointer; the level's [`size`](Self::level_size) bounds it).
    pub fn memory_pointer(&self, level: u32) -> Result<&[u8], DdsError> {
        let (_, offset) = self.level_loaded(level)?;
        let start = offset as usize;
        if start > self.data.len() {
            return Err(DdsError::DataTooShort);
        }
        Ok(&self.data[start..])
    }

    /// `Get_CubeMap_Memory_Pointer(face, level)` — the face's level
    /// data, `CubeFaceSize · face` bytes into the window.
    pub fn cube_map_memory_pointer(&self, face: u32, level: u32) -> Result<&[u8], DdsError> {
        if face > 5 {
            return Err(DdsError::FaceOutOfRange);
        }
        let (_, offset) = self.level_loaded(level)?;
        let start = (self.cube_face_size.wrapping_mul(face).wrapping_add(offset)) as usize;
        if start > self.data.len() {
            return Err(DdsError::DataTooShort);
        }
        Ok(&self.data[start..])
    }

    /// `Get_Volume_Memory_Pointer(level)` — the original's dead stub:
    /// it returns null for every level, so the volume copies do
    /// nothing. The port states the contract with `None`.
    #[must_use]
    pub fn volume_memory_pointer(&self, level: u32) -> Option<&[u8]> {
        let _ = level;
        None
    }

    fn level(&self, level: u32) -> Result<(u32, u32), DdsError> {
        let idx = usize::try_from(level).map_err(|_| DdsError::LevelOutOfRange)?;
        self.level_sizes
            .get(idx)
            .zip(self.level_offsets.get(idx))
            .map(|(s, o)| (*s, *o))
            .ok_or(DdsError::LevelOutOfRange)
    }

    /// The level entry plus the loaded-data gate the original's
    /// `DDSMemory` dereference stands for.
    fn level_loaded(&self, level: u32) -> Result<(u32, u32), DdsError> {
        if self.data.is_empty() {
            // Every consumer of the original's null DDSMemory
            // dereferences it; the port refuses instead.
            return Err(DdsError::NoData);
        }
        self.level(level)
    }

    /// The data window `load` read (empty until then).
    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    // -- the pixel queries ------------------------------------------------

    /// `Get_Pixel(level, x, y)` — one pixel in the internal "ARGB"
    /// form (truncated 5-bit lanes, alpha in the high byte).
    ///
    /// DXT1 decodes both colour modes (the 8-colour interpolation and
    /// the 4-colour one whose code 3 is transparent black); DXT5 adds
    /// the 8/6-alpha tables over the always-interpolated colour block;
    /// DXT2-4 and every non-DXT format return opaque white
    /// (`0xffffffff`), the original's switch default.
    ///
    /// The alpha interpolation formulas keep the original's literal
    /// `1 * alpha` shapes (transcribed verbatim).
    #[allow(clippy::identity_op)]
    pub fn get_pixel(&self, level: u32, x: u32, y: u32) -> Result<u32, DdsError> {
        if x >= self.width(level) || y >= self.height(level) {
            return Err(DdsError::PixelOutOfRange);
        }
        let mem = self.memory_pointer(level)?;
        match self.format {
            Ww3dFormat::Dxt1 => {
                let base = (x / 4 * 8).wrapping_add((y / 4 * (self.width(level) / 4)) * 8) as usize;
                let col0 = rgb565_to_argb8888(read_u16(mem, base)?);
                let col1 = rgb565_to_argb8888(read_u16(mem, base + 2)?);
                let mut line = *mem
                    .get(base + 4 + (y % 4) as usize)
                    .ok_or(DdsError::DataTooShort)?;
                line >>= (x % 4) * 2;
                line &= 3;
                if col0 > col1 {
                    Ok(match line {
                        0 => col0 | 0xff00_0000,
                        1 => col1 | 0xff00_0000,
                        2 => combine_colors(col1, col0, 85) | 0xff00_0000,
                        _ => combine_colors(col0, col1, 85) | 0xff00_0000,
                    })
                } else {
                    Ok(match line {
                        0 => col0 | 0xff00_0000,
                        1 => col1 | 0xff00_0000,
                        2 => combine_colors(col1, col0, 128) | 0xff00_0000,
                        _ => 0x0000_0000,
                    })
                }
            }
            Ww3dFormat::Dxt2 | Ww3dFormat::Dxt3 | Ww3dFormat::Dxt4 => Ok(0xffff_ffff),
            Ww3dFormat::Dxt5 => {
                let base =
                    (x / 4 * 16).wrapping_add((y / 4 * (self.width(level) / 4)) * 16) as usize;
                let alpha0 = u32::from(*mem.get(base).ok_or(DdsError::DataTooShort)?);
                let alpha1 = u32::from(*mem.get(base + 1).ok_or(DdsError::DataTooShort)?);

                let mut bit_idx = ((x % 4) + 4 * (y % 4)) * 3;
                let mut byte_idx = (bit_idx / 8) as usize;
                bit_idx %= 8;
                let mut alpha_index: u32 = 0;
                for i in 0..3 {
                    // WWASSERT(byte_idx<6) — release: no-op
                    let byte = *mem.get(base + 2 + byte_idx).ok_or(DdsError::DataTooShort)?;
                    let alpha_bit = u32::from((byte >> bit_idx) & 1);
                    alpha_index |= alpha_bit << i;
                    bit_idx += 1;
                    if bit_idx >= 8 {
                        bit_idx = 0;
                        byte_idx += 1;
                    }
                }

                // 8-alpha or 6-alpha block?
                let alpha_value: u32 = if alpha0 > alpha1 {
                    // 8-alpha block: derive the other six alphas.
                    match alpha_index {
                        0 => alpha0,
                        1 => alpha1,
                        2 => (6 * alpha0 + 1 * alpha1 + 3) / 7, // bit code 010
                        3 => (5 * alpha0 + 2 * alpha1 + 3) / 7, // bit code 011
                        4 => (4 * alpha0 + 3 * alpha1 + 3) / 7, // bit code 100
                        5 => (3 * alpha0 + 4 * alpha1 + 3) / 7, // bit code 101
                        6 => (2 * alpha0 + 5 * alpha1 + 3) / 7, // bit code 110
                        _ => (1 * alpha0 + 6 * alpha1 + 3) / 7, // bit code 111
                    }
                } else {
                    // 6-alpha block.
                    match alpha_index {
                        0 => alpha0,
                        1 => alpha1,
                        2 => (4 * alpha0 + 1 * alpha1 + 2) / 5, // bit code 010
                        3 => (3 * alpha0 + 2 * alpha1 + 2) / 5, // bit code 011
                        4 => (2 * alpha0 + 3 * alpha1 + 2) / 5, // bit code 100
                        5 => (1 * alpha0 + 4 * alpha1 + 2) / 5, // bit code 101
                        6 => 0,                                 // bit code 110
                        _ => 255,                               // bit code 111
                    }
                };
                let alpha_value = alpha_value << 24;

                // Extract color
                let color_block = base + 8;
                let col0 = rgb565_to_argb8888(read_u16(mem, color_block)?);
                let col1 = rgb565_to_argb8888(read_u16(mem, color_block + 2)?);
                let mut line = *mem
                    .get(color_block + 4 + (y % 4) as usize)
                    .ok_or(DdsError::DataTooShort)?;
                line >>= (x % 4) * 2;
                line &= 3;
                Ok(match line {
                    0 => col0 | alpha_value,
                    1 => col1 | alpha_value,
                    2 => combine_colors(col1, col0, 85) | alpha_value,
                    _ => combine_colors(col0, col1, 85) | alpha_value,
                })
            }
            // Every other format: the switch default.
            _ => Ok(0xffff_ffff),
        }
    }

    /// `Get_4x4_Block` — decompress the 4×4 block at `(source_x,
    /// source_y)` (block-aligned) into `dest` as `dest_format` pixels,
    /// row-stride `dest_pitch`.
    ///
    /// `dest` starts at the block's top-left pixel, exactly like the
    /// original's pointer; the writes cover `3·pitch + 4·bpp` bytes —
    /// **plus one** for an R3G3B2 destination (the 16-bit store's
    /// stray byte, see the module docs). Returns whether the block
    /// contained alpha (DXT1's transparent code; DXT5's fully-
    /// transparent block).
    // The signature mirrors the original's parameter list; the alpha
    // interpolation formulas keep the literal `1 * alpha` shapes.
    #[allow(clippy::too_many_arguments, clippy::identity_op)]
    pub fn get_4x4_block(
        &self,
        dest: &mut [u8],
        dest_pitch: usize,
        dest_format: Ww3dFormat,
        level: u32,
        source_x: u32,
        source_y: u32,
        hsv_shift: HsvShift,
    ) -> Result<bool, DdsError> {
        if source_x >= self.width(level) || source_y >= self.height(level) {
            return Err(DdsError::BlockOutOfRange);
        }
        let has_hsv_shift = hsv_shift.is_nonzero();
        let mem = self.memory_pointer(level)?;
        // The decoded block: one "B8G8R8A8" word per pixel, in the
        // original's write order (the R3G3B2 stray-byte store makes the
        // per-pixel write order observable, so the writes below stay
        // sequential).
        let mut pixels = [0u32; 16];
        let contains_alpha;
        match self.format {
            Ww3dFormat::Dxt1 => {
                let base = (source_x / 4 * 8)
                    .wrapping_add((source_y / 4 * (self.width(level) / 4)) * 8)
                    as usize;
                let col0 = rgb565_to_argb8888(read_u16(mem, base)?);
                let col1 = rgb565_to_argb8888(read_u16(mem, base + 2)?);
                // Even if we don't support alpha, decompression is different if source has alpha
                if col0 > col1 {
                    let (col0, col1) = if has_hsv_shift {
                        (recolor(col0, hsv_shift), recolor(col1, hsv_shift))
                    } else {
                        (col0, col1)
                    };
                    for y in 0..4usize {
                        let mut line = *mem.get(base + 4 + y).ok_or(DdsError::DataTooShort)?;
                        for x in 0..4usize {
                            pixels[y * 4 + x] = match line & 3 {
                                0 => col0 | 0xff00_0000,
                                1 => col1 | 0xff00_0000,
                                2 => combine_colors(col1, col0, 85) | 0xff00_0000,
                                _ => combine_colors(col0, col1, 85) | 0xff00_0000,
                            };
                            line >>= 2;
                        }
                    }
                    contains_alpha = false; // No alpha found in the block
                } else {
                    let (col0, col1) = if has_hsv_shift {
                        (recolor(col0, hsv_shift), recolor(col1, hsv_shift))
                    } else {
                        (col0, col1)
                    };
                    let mut found = false;
                    for y in 0..4usize {
                        let mut line = *mem.get(base + 4 + y).ok_or(DdsError::DataTooShort)?;
                        for x in 0..4usize {
                            pixels[y * 4 + x] = match line & 3 {
                                0 => col0 | 0xff00_0000,
                                1 => col1 | 0xff00_0000,
                                2 => combine_colors(col1, col0, 128) | 0xff00_0000,
                                _ => {
                                    found = true;
                                    0x0000_0000
                                }
                            };
                            line >>= 2;
                        }
                    }
                    contains_alpha = found; // Alpha block...?
                }
            }
            Ww3dFormat::Dxt2 | Ww3dFormat::Dxt3 | Ww3dFormat::Dxt4 => {
                // The original's switch default: nothing written, no
                // alpha verdict.
                return Ok(false);
            }
            Ww3dFormat::Dxt5 => {
                // Init alphas
                let base = (source_x / 4 * 16)
                    .wrapping_add((source_y / 4 * (self.width(level) / 4)) * 16)
                    as usize;
                let mut alphas = [0u32; 8];
                alphas[0] = u32::from(*mem.get(base).ok_or(DdsError::DataTooShort)?);
                alphas[1] = u32::from(*mem.get(base + 1).ok_or(DdsError::DataTooShort)?);

                // 8-alpha or 6-alpha block?
                if alphas[0] > alphas[1] {
                    alphas[2] = (6 * alphas[0] + 1 * alphas[1] + 3) / 7; // bit code 010
                    alphas[3] = (5 * alphas[0] + 2 * alphas[1] + 3) / 7; // bit code 011
                    alphas[4] = (4 * alphas[0] + 3 * alphas[1] + 3) / 7; // bit code 100
                    alphas[5] = (3 * alphas[0] + 4 * alphas[1] + 3) / 7; // bit code 101
                    alphas[6] = (2 * alphas[0] + 5 * alphas[1] + 3) / 7; // bit code 110
                    alphas[7] = (1 * alphas[0] + 6 * alphas[1] + 3) / 7; // bit code 111
                } else {
                    alphas[2] = (4 * alphas[0] + 1 * alphas[1] + 2) / 5; // bit code 010
                    alphas[3] = (3 * alphas[0] + 2 * alphas[1] + 2) / 5; // bit code 011
                    alphas[4] = (2 * alphas[0] + 3 * alphas[1] + 2) / 5; // bit code 100
                    alphas[5] = (1 * alphas[0] + 4 * alphas[1] + 2) / 5; // bit code 101
                    alphas[6] = 0; // bit code 110
                    alphas[7] = 255; // bit code 111
                }

                // Init colors
                let color_block = base + 8;
                let col0 = rgb565_to_argb8888(read_u16(mem, color_block)?);
                let col1 = rgb565_to_argb8888(read_u16(mem, color_block + 2)?);
                let (col0, col1) = if has_hsv_shift {
                    (recolor(col0, hsv_shift), recolor(col1, hsv_shift))
                } else {
                    (col0, col1)
                };

                let mut contains: u32 = 0xff;

                // The original's unrolled two-half index decode over
                // bytes [2..8) of the alpha block (the second half
                // starts three bytes in — the `alpha_block += 3`).
                let mut alpha_indices = [0u32; 16];
                {
                    let mut ai = 0;
                    for half in 0..2usize {
                        let b2 = *mem.get(base + 2 + half * 3).ok_or(DdsError::DataTooShort)?;
                        let b3 = *mem.get(base + 3 + half * 3).ok_or(DdsError::DataTooShort)?;
                        let b4 = *mem.get(base + 4 + half * 3).ok_or(DdsError::DataTooShort)?;
                        alpha_indices[ai] = u32::from(b2 & 0x7);
                        alpha_indices[ai + 1] = u32::from(b2 >> 3) & 0x7;
                        alpha_indices[ai + 2] = u32::from(b2 >> 6) | (u32::from(b3 & 1) << 2);
                        alpha_indices[ai + 3] = u32::from(b3 >> 1) & 0x7;
                        alpha_indices[ai + 4] = u32::from(b3 >> 4) & 0x7;
                        alpha_indices[ai + 5] = u32::from(b3 >> 7) | (u32::from(b4 & 3) << 1);
                        alpha_indices[ai + 6] = u32::from(b4 >> 2) & 0x7;
                        alpha_indices[ai + 7] = u32::from(b4 >> 5);
                        ai += 8;
                    }
                }

                let mut aii = 0;
                for y in 0..4usize {
                    let mut line = *mem.get(color_block + 4 + y).ok_or(DdsError::DataTooShort)?;
                    for x in 0..4usize {
                        let mut alpha_value = alphas
                            .get(alpha_indices[aii] as usize)
                            .copied()
                            .unwrap_or(0);
                        aii += 1;
                        contains &= alpha_value;
                        alpha_value <<= 24;

                        // Extract color
                        pixels[y * 4 + x] = match line & 3 {
                            0 => col0 | alpha_value,
                            1 => col1 | alpha_value,
                            2 => combine_colors(col1, col0, 85) | alpha_value,
                            _ => combine_colors(col0, col1, 85) | alpha_value,
                        };
                        line >>= 2;
                    }
                }
                contains_alpha = contains != 0xff; // Alpha block...? DXT5 should only be used when the image needs alpha
                // but for now check anyway...
            }
            // Every other format: the switch default (nothing written).
            _ => return Ok(false),
        }

        write_block_pixels(dest, dest_pitch, dest_format, &pixels)?;
        Ok(contains_alpha)
    }

    // -- the surface copies ------------------------------------------------

    /// `Copy_Level_To_Surface` (the in-memory overload the D3D wrapper
    /// forwards to): copy one mip level into a caller-owned surface.
    ///
    /// A format-and-size match copies (or, with a hue shift, re-encodes
    /// block by block — DXT1 and DXT5 only; a DXT3 match with a shift
    /// leaves the destination untouched, the original's dead branch);
    /// a size-matched format mismatch decompresses through the 4×4
    /// block walk (with the DXT1→DXT2 expansion special case); a size
    /// mismatch writes nothing at all.
    ///
    /// `dest` needs `dest_height · dest_pitch` bytes (the DXT2
    /// expansion and the decompressed destinations included — use a
    /// tightly packed pitch of `dest_width · bpp`), plus the R3G3B2
    /// spare byte when it applies.
    // The signature mirrors the original's parameter list.
    #[allow(clippy::too_many_arguments)]
    pub fn copy_level_to_surface(
        &self,
        level: u32,
        dest_format: Ww3dFormat,
        dest_width: u32,
        dest_height: u32,
        dest: &mut [u8],
        dest_pitch: usize,
        hsv_shift: HsvShift,
    ) -> Result<(), DdsError> {
        if self.data.is_empty() {
            // The original's silent return on a null DDSMemory.
            return Err(DdsError::NoData);
        }
        self.copy_level_impl(
            self.memory_pointer(level)?,
            level,
            dest_format,
            dest_width,
            dest_height,
            dest,
            dest_pitch,
            hsv_shift,
        )
    }

    /// `Copy_CubeMap_Level_To_Surface` — the same copy from a cube
    /// face's base.
    // The signature mirrors the original's parameter list.
    #[allow(clippy::too_many_arguments)]
    pub fn copy_cube_map_level_to_surface(
        &self,
        face: u32,
        level: u32,
        dest_format: Ww3dFormat,
        dest_width: u32,
        dest_height: u32,
        dest: &mut [u8],
        dest_pitch: usize,
        hsv_shift: HsvShift,
    ) -> Result<(), DdsError> {
        if self.data.is_empty() {
            return Err(DdsError::NoData);
        }
        self.copy_level_impl(
            self.cube_map_memory_pointer(face, level)?,
            level,
            dest_format,
            dest_width,
            dest_height,
            dest,
            dest_pitch,
            hsv_shift,
        )
    }

    /// `Copy_Volume_Level_To_Surface` — the dead stub: the volume
    /// pointer is null, so the original always returns before touching
    /// the destination. The port states the contract (nothing written,
    /// `Ok(())`).
    // The signature mirrors the original's parameter list.
    #[allow(clippy::too_many_arguments)]
    pub fn copy_volume_level_to_surface(
        &self,
        level: u32,
        _depth: u32,
        dest_format: Ww3dFormat,
        dest_width: u32,
        dest_height: u32,
        _dest: &mut [u8],
        _row_pitch: usize,
        _slice_pitch: usize,
        _hsv_shift: HsvShift,
    ) -> Result<(), DdsError> {
        if self.data.is_empty() {
            return Err(DdsError::NoData);
        }
        // Get_Volume_Memory_Pointer returns null: the early return.
        let _ = (level, dest_format, dest_width, dest_height);
        Ok(())
    }

    /// The shared body of the two live copies, parameterised by the
    /// source base (level base or cube-face base) — the original's two
    /// functions are line-for-line identical from there on.
    #[allow(clippy::too_many_arguments)]
    fn copy_level_impl(
        &self,
        src: &[u8],
        level: u32,
        dest_format: Ww3dFormat,
        dest_width: u32,
        dest_height: u32,
        dest: &mut [u8],
        dest_pitch: usize,
        hsv_shift: HsvShift,
    ) -> Result<(), DdsError> {
        // If the format and size is a match just copy the contents
        let has_hsv_shift = hsv_shift.is_nonzero();
        if dest_format == self.format
            && dest_width == self.width(level)
            && dest_height == self.height(level)
        {
            // If hue shift, we can't just copy...
            if has_hsv_shift {
                match self.format {
                    Ww3dFormat::Dxt1 => {
                        self.rewrite_dxt1_colors(src, dest_width, dest_height, dest, hsv_shift)?;
                    }
                    Ww3dFormat::Dxt5 => {
                        self.rewrite_dxt5_colors(src, dest_width, dest_height, dest, hsv_shift)?;
                    }
                    // WWASSERT(0) — release: nothing (the destination is
                    // left untouched).
                    _ => {}
                }
            } else {
                let compressed_size = self.level_size(level)? as usize;
                if compressed_size > src.len() || compressed_size > dest.len() {
                    // The original memcpy's its declared level size out
                    // of (and into) whatever it finds.
                    return Err(DdsError::DataTooShort);
                }
                dest[..compressed_size].copy_from_slice(&src[..compressed_size]);
            }
        } else {
            // If size matches, copy each pixel linearly with color space conversion
            if dest_width == self.width(level) && dest_height == self.height(level) {
                // An exception here - if the source format is DXT1 and the destination
                // is DXT2, just copy the contents and create an empty alpha channel.
                // This is needed on NVidia cards that have problems with DXT1 compression.
                if self.format == Ww3dFormat::Dxt1 && dest_format == Ww3dFormat::Dxt2 {
                    self.expand_dxt1_to_dxt2(src, dest_width, dest_height, dest, hsv_shift)?;
                } else {
                    let dest_bpp = dest_format.bytes_per_pixel() as usize;

                    // Copy 4x4 block at a time (the original advances its
                    // destination pointer by dest_bpp*4 per block through
                    // the for-statement's step)
                    let mut _contains_alpha = false;
                    let mut y = 0;
                    while y < dest_height {
                        let mut x = 0;
                        while x < dest_width {
                            let base = y as usize * dest_pitch + (x as usize / 4) * dest_bpp * 4;
                            let Some(block_dest) = dest.get_mut(base..) else {
                                return Err(DdsError::DestTooShort);
                            };
                            _contains_alpha |= self.get_4x4_block(
                                block_dest,
                                dest_pitch,
                                dest_format,
                                level,
                                x,
                                y,
                                hsv_shift,
                            )?;
                            x += 4;
                        }
                        y += 4;
                    }
                    // The DXT1 contains-alpha warning is a WWDEBUG_SAY —
                    // a release no-op.
                }
            }
            // A size mismatch writes nothing (the original's missing
            // scaling branch).
        }
        Ok(())
    }

    /// The DXT1 hue-shift rewrite: re-encode each block's colour
    /// endpoints through the recolour round trip.
    fn rewrite_dxt1_colors(
        &self,
        src: &[u8],
        dest_width: u32,
        dest_height: u32,
        dest: &mut [u8],
        hsv_shift: HsvShift,
    ) -> Result<(), DdsError> {
        let mut at = 0usize; // src cursor in u32 words
        let mut out = 0usize; // dest cursor in u32 words
        let mut y = 0;
        while y < dest_height {
            let mut x = 0;
            while x < dest_width {
                let cols = read_u32(src, at * 4)?;
                at += 1;
                let mut col0 = rgb565_to_argb8888((cols >> 16) as u16);
                let mut col1 = rgb565_to_argb8888((cols & 0xffff) as u16);
                col0 = recolor(col0, hsv_shift);
                col1 = recolor(col1, hsv_shift);
                let col0 = argb8888_to_rgb565(col0);
                let col1 = argb8888_to_rgb565(col1);
                let cols = (u32::from(col0)) << 16 | u32::from(col1);
                write_u32(dest, out * 4, cols)?;
                out += 1;

                let line = read_u32(src, at * 4)?;
                at += 1;
                write_u32(dest, out * 4, line)?;
                out += 1;

                x += 4;
            }
            y += 4;
        }
        Ok(())
    }

    /// The DXT5 hue-shift rewrite: the alpha words ride through, the
    /// colour endpoints re-encode.
    fn rewrite_dxt5_colors(
        &self,
        src: &[u8],
        dest_width: u32,
        dest_height: u32,
        dest: &mut [u8],
        hsv_shift: HsvShift,
    ) -> Result<(), DdsError> {
        let mut at = 0usize;
        let mut out = 0usize;
        let mut y = 0;
        while y < dest_height {
            let mut x = 0;
            while x < dest_width {
                // Bytes 1-4 and 5-8 of the alpha block ride through.
                for _ in 0..2 {
                    let word = read_u32(src, at * 4)?;
                    at += 1;
                    write_u32(dest, out * 4, word)?;
                    out += 1;
                }
                let cols = read_u32(src, at * 4)?;
                at += 1;
                let mut col0 = rgb565_to_argb8888((cols >> 16) as u16);
                let mut col1 = rgb565_to_argb8888((cols & 0xffff) as u16);
                col0 = recolor(col0, hsv_shift);
                col1 = recolor(col1, hsv_shift);
                let col0 = argb8888_to_rgb565(col0);
                let col1 = argb8888_to_rgb565(col1);
                let cols = (u32::from(col0)) << 16 | u32::from(col1);
                write_u32(dest, out * 4, cols)?;
                out += 1;

                let line = read_u32(src, at * 4)?;
                at += 1;
                write_u32(dest, out * 4, line)?;
                out += 1;

                x += 4;
            }
            y += 4;
        }
        Ok(())
    }

    /// The DXT1→DXT2 expansion: an explicit opaque alpha block in front
    /// of each copied (or recoloured) colour block.
    fn expand_dxt1_to_dxt2(
        &self,
        src: &[u8],
        dest_width: u32,
        dest_height: u32,
        dest: &mut [u8],
        hsv_shift: HsvShift,
    ) -> Result<(), DdsError> {
        let do_recolor = hsv_shift.is_nonzero();
        let mut at = 0usize;
        let mut out = 0usize;
        let mut y = 0;
        while y < dest_height {
            let mut x = 0;
            while x < dest_width {
                write_u32(dest, out * 4, 0xffff_ffff)?; // Bytes 1-4 of alpha block
                out += 1;
                write_u32(dest, out * 4, 0xffff_ffff)?; // Bytes 5-8 of alpha block
                out += 1;

                let cols = read_u32(src, at * 4)?;
                at += 1;
                let cols = if do_recolor {
                    let mut col0 = rgb565_to_argb8888((cols >> 16) as u16);
                    let mut col1 = rgb565_to_argb8888((cols & 0xffff) as u16);
                    col0 = recolor(col0, hsv_shift);
                    col1 = recolor(col1, hsv_shift);
                    let col0 = argb8888_to_rgb565(col0);
                    let col1 = argb8888_to_rgb565(col1);
                    (u32::from(col0)) << 16 | u32::from(col1)
                } else {
                    cols
                };
                write_u32(dest, out * 4, cols)?;
                out += 1;

                let line = read_u32(src, at * 4)?;
                at += 1;
                write_u32(dest, out * 4, line)?;
                out += 1;

                x += 4;
            }
            y += 4;
        }
        Ok(())
    }
}

fn write_u32(dest: &mut [u8], at: usize, value: u32) -> Result<(), DdsError> {
    let end = at + 4;
    if end > dest.len() || end < at {
        return Err(DdsError::DestTooShort);
    }
    dest[at..end].copy_from_slice(&value.to_le_bytes());
    Ok(())
}

// ---------------------------------------------------------------------------
// munged_name (the constructor's name handling)
// ---------------------------------------------------------------------------

/// The constructor's name munging: `strlcpy` into a 256-byte buffer
/// (truncating at 255 characters), then **overwrite the last three
/// characters with `"dds"`** — whatever they were. `"tex.tga"` becomes
/// `"tex.dds"`, `"tank"` becomes `"tdds"`, and a name whose effective
/// length (up to the first NUL, like the original's `strlen`) is
/// shorter than three would write before the buffer — refused here
/// with [`DdsError::NameTooShort`].
pub fn munged_name(name: &str) -> Result<String, DdsError> {
    // The C-string view the original's strlen sees.
    let effective = name.split('\0').next().unwrap_or("");
    let bytes = effective.as_bytes();
    let mut buf: Vec<u8> = if bytes.len() >= MAX_NAME_LEN - 1 {
        bytes[..MAX_NAME_LEN - 1].to_vec()
    } else {
        bytes.to_vec()
    };
    if buf.len() < 3 {
        // Name[len-3] with len < 3 writes before the buffer.
        return Err(DdsError::NameTooShort);
    }
    let n = buf.len();
    buf[n - 3] = b'd';
    buf[n - 2] = b'd';
    buf[n - 1] = b's';
    String::from_utf8(buf).map_err(|_| DdsError::NameNotUtf8)
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// What a DDS load can fail with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DdsError {
    /// The name's effective length is under three characters — the
    /// munging's negative-index write (refused; see [`munged_name`]).
    NameTooShort,
    /// The munged name is not valid UTF-8 (the original stores raw
    /// bytes; this port's [`DdsFile::name`] returns `&str`).
    NameNotUtf8,
    /// The file is too short to hold a magic (the original's
    /// zero-byte magic read).
    Unavailable,
    /// The description read came back short — the file is smaller than
    /// the header it declares.
    TruncatedHeader,
    /// The `Size` field is not 124: the only other shape that passes
    /// the original's check leaves the struct tail uninitialised
    /// (nondeterministic — refused).
    SizeFieldMismatch,
    /// The declared mip count exceeds [`MAX_MIP_LEVELS`] (the
    /// original's unbounded allocation).
    TooManyMipLevels,
    /// The mip-chain payload exceeds [`MAX_DATA_BYTES`] (the
    /// original's failing `new`).
    TooLarge,
    /// `load` was called twice (the original's second `Load` returns
    /// false).
    AlreadyLoaded,
    /// No data: `load` has not been called, the file had no payload,
    /// or the underflow guard refused the read (the original hands its
    /// consumers a null pointer here).
    NoData,
    /// The level data the layout declares overruns what `load` read.
    DataTooShort,
    /// The load's seek target ran past the file (the original reads
    /// zeroes into an uninitialised allocation).
    SeekPastEnd,
    /// A level index outside the addressable mip levels.
    LevelOutOfRange,
    /// A pixel coordinate outside the (clamped) level dimensions.
    PixelOutOfRange,
    /// A block coordinate outside the level's block grid.
    BlockOutOfRange,
    /// A face index outside the cube's six faces.
    FaceOutOfRange,
    /// The destination surface buffer is too small for the copy shape.
    DestTooShort,
    /// A DXT or paletted destination in the decompressing copy (the
    /// original's assert path).
    UnsupportedDestination,
}

impl fmt::Display for DdsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::NameTooShort => {
                "name shorter than three characters (the munging's out-of-bounds write)"
            }
            Self::NameNotUtf8 => "munged name is not valid UTF-8",
            Self::Unavailable => "file too short for the magic",
            Self::TruncatedHeader => "file smaller than the declared header",
            Self::SizeFieldMismatch => {
                "surface description size is not 124 (the uninitialised-tail shape)"
            }
            Self::TooManyMipLevels => "mip count exceeds the port's bound",
            Self::TooLarge => "mip-chain payload exceeds the port's bound",
            Self::AlreadyLoaded => "load called twice",
            Self::NoData => "no level data (not loaded, empty file, or the underflow guard)",
            Self::DataTooShort => "level data overruns the read window",
            Self::SeekPastEnd => "the skipped mip levels run past the file",
            Self::LevelOutOfRange => "level index out of range",
            Self::PixelOutOfRange => "pixel coordinate out of range",
            Self::BlockOutOfRange => "block coordinate out of range",
            Self::FaceOutOfRange => "cube face index out of range",
            Self::DestTooShort => "destination surface too small",
            Self::UnsupportedDestination => "destination format cannot take decompressed writes",
        };
        f.write_str(s)
    }
}

impl std::error::Error for DdsError {}
