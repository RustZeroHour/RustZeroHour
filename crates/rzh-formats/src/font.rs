// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Ported from the Zero Hour source release (baseline 571a7c5):
//   Core/Libraries/Source/WWVegas/WW3D2/font3d.cpp
//     (Font3DDataClass — the Targa font-strike loader: Load_Font_Image's
//     strike and grid table builds, Make_Proportional's FindBB walk
//     with the swapped-empty-box rule and the >0x80 turn-off,
//     Minimize_Font_Image's packed-page re-layout with the line
//     overflow overlap and the V/new-width slip; Font3DInstanceClass —
//     the cached scaled tables, the space override, Set_Mono_Spaced and
//     String_Width; the strdup+strupr'd Name)
//   Core/Libraries/Source/WWVegas/WW3D2/assetmgr.cpp
//     (WW3DAssetManager::Get_Font3DData — the stricmp cache walk and
//     the head-prepending list)
//   Core/Libraries/Source/WWVegas/WW3D2/surfaceclass.cpp
//     (SurfaceClass — the file constructor's availability probes,
//     Get_Description, Clear, Is_Transparent_Column, FindBB and the
//     region Copy)
//   Core/Libraries/Source/WWVegas/WW3D2/textureloader.cpp
//     (TextureLoader::Load_Surface_Immediate — the Targa open, the
//     ImageDescriptor ^= TGAIDF_YORIGIN toggle, the Get_WW3D_Format
//     mapping, the 16-bit/paletted/luminance conversion branch and the
//     Copy_Image into the created surface)
//   Core/Libraries/Source/WWVegas/WW3D2/bitmaphandler.{h,cpp}
//     (Read_B8G8R8A8, Write_B8G8R8A8, Copy_Pixel and the driven
//     Copy_Image paths)
//   Core/Libraries/Source/WWVegas/WW3D2/ww3dformat.{h,cpp}
//     (Get_Bytes_Per_Pixel, Has_Alpha, Alpha_Bits, Get_WW3D_Format,
//     Get_Valid_Texture_Format)
//   Core/Libraries/Source/WWVegas/WWLib/TARGA.cpp
//     (the Targa reader, driven in the font's load shape — see the
//     FontTarga docs)
//   Core/Libraries/Source/WWVegas/WW3D2/missingtexture.cpp
//     (MissingTexture::_Create_Missing_Surface — the 128x128 solid
//     0x7FFF00FF placeholder)
//
// See the module docs for the format layout and the documented
// differences from the C++ (the pinned display environment, the D3DX
// load model, the zero-filled palette stand-in, the dimension ceiling
// and the crash verdicts).

//! The **Targa font strikes and grids** — the `.tga` font images the
//! game's 3D text path loads (`Font3DDataClass`, the loader the
//! `WW3DAssetManager::Get_Font3DData` cache fronts).
//!
//! A font file is a Targa image in one of two layouts, picked by aspect:
//! a **strike** (width > 8 x height) is a horizontal strip of glyphs
//! separated by fully transparent columns — the column scanner walks the
//! strip, handing each opaque run to the next character code (chars
//! `>= 0x7F` are zeroed before the scan even starts); anything else is a
//! **16x16 character grid**, where each cell's glyph is shrunk to its
//! alpha bounding box (`Make_Proportional`, chars `> 0x80` turned off).
//! Both paths then run `Minimize_Font_Image`: the glyphs are re-packed
//! left-to-right, top-to-bottom into a fresh 128x128 or 256x256
//! `A4R4G4B4` page (the size picks from the source width), the
//! per-character UV tables are rewritten to the packed positions, and a
//! glyph that does not fit the page is cropped (the blit rectangles
//! clamp) while a line that does not fit the page overlaps the one
//! before it (the `new_y -= height` slip, transcribed).
//!
//! This module ports the whole load path over the surface stack it
//! drives — the Targa reader (in the font's load shape: the caller's
//! palette buffer, `TGAF_IMAGE` only, the texture loader's Y-origin
//! toggle), the format mapping and conversion branch, the alpha
//! column/bounding-box walks, and the packed-page blits through the
//! D3DX `D3DX_FILTER_NONE` software model — parity level P0, pinned by
//! golden vectors generated from the transcription in
//! `tools/differential/gen_font_golden.cpp`.
//!
//! # The load path in one page
//!
//! ```text
//! SurfaceClass("font.tga")            the file constructor
//!   |- availability probes (.tga, then the munged .dds)
//!   |- Load_Surface_Immediate: Targa open; ImageDescriptor ^= YORIGIN;
//!   |  Get_WW3D_Format (32->A8R8G8B8, 24->X8R8G8B8, 16/pal/mono
//!   |  ->A8R8G8B8 through the conversion branch, other 8-bit->A8);
//!   |  Copy_Image into the surface
//!   |- missing/corrupt -> the 128x128 solid 0x7FFF00FF placeholder
//!   `- zero-sized/unknown-format -> the null surface (crash verdict)
//! Load_Font_Image
//!   |- width > 8*height: the strike column scan (chars >= 0x7F zeroed)
//!   `- else: the 16x16 grid cells -> Make_Proportional
//!        (FindBB per cell; empty cells keep position, width 0;
//!         chars > 0x80 turned off)
//! Minimize_Font_Image
//!   |- a fresh A4R4G4B4 page, 128 or 256 square, cleared to zero
//!   |- per char: clamp-checked blit at (new_x, new_y) through the
//!   |  D3DX model (1:1 top-left copy over the minimum extents, the
//!   |  destination beyond zero-filled, channels truncated to 4 bits)
//!   |- the tables rewritten: U = new_x/page, V = new_y/page (the
//!   |  original's divide-by-new_width slip — a no-op, the page is
//!   |  square), UWidth rescaled by source/page
//!   `- line overflow: new_y -= height, the row overlaps
//! Font3DInstanceClass                 the consumer's cached tables
//!   |- SpaceSpacing = char_width('H')/2, InterCharSpacing = 1
//!   |- ScaledWidth/ScaledSpacing per char (the space override),
//!   |  ScaledHeight = floor(char_height)
//!   `- Set_Mono_Spaced: spacing = char_width('W')+1 for every char
//! ```
//!
//! # The load-path quirks (transcribed)
//!
//! - **The Y-origin toggle**: the texture loader XORs the header's
//!   `YORIGIN` bit *before* the load, so a bottom-up file (the Targa
//!   default) gets YFlipped into a top-down surface and a top-down file
//!   is left alone — the surface is top-down either way. `XORIGIN`
//!   still triggers the reader's XFlip.
//! - **The strike exhaustion**: when the column scanner runs out of
//!   strip, every remaining char gets `start == end == width` — the
//!   tables record `U = 1.0`, width 0, and `Minimize_Font_Image`
//!   overwrites the offsets (but never the widths) again.
//! - **The `>= 0x7F` / `> 0x80` boundary**: the strike path zeroes
//!   chars from `0x7F` up; the grid path turns off chars strictly
//!   after `0x80` — so grid char `0x80` keeps its glyph and `0x81`
//!   does not, while a strike's `0x7E` is the last live char.
//! - **The uchar tables**: `CharWidthTable` and `CharHeight` store
//!   through `unsigned char` — a 300-column glyph records width 44
//!   (the UV tables keep the full floats, so the packed layout uses
//!   the true extent).
//! - **The swapped empty box**: `FindBB` starts `realmin` from `max`
//!   and `realmax` from `min`, so a cell with no alpha pixels returns
//!   an inverted box; `Make_Proportional`'s `x1 < x0` test turns that
//!   into width 0 (the position survives).
//! - **The no-alpha surface**: a 24-bit source lands as `X8R8G8B8`,
//!   whose `Alpha_Bits` is 0 — every column reads transparent, every
//!   glyph dies, and the font still "loads" with all-zero widths.
//! - **The D3DX crop and overlap**: a glyph wider than the page is
//!   cropped to the page (the destination rectangle clamps, the copy
//!   runs over the minimum of both extents); a line that overflows the
//!   page is stepped back and blitted over the previous line.
//! - **The missing font works**: an absent file, a failed open or a
//!   failed load all return the missing-texture placeholder — a
//!   128x128 solid half-alpha magenta surface the font tables build
//!   from like from any other image (129 chars of 8x8 solid glyphs).
//!
//! # Differences from the C++ (stated, pinned by the goldens)
//!
//! - **The display environment is pinned** to the retail standard: a
//!   32-bit desktop with every texture format supported and DXTC
//!   available. Under this pin `Get_Valid_Texture_Format` is the
//!   identity except `R8G8B8 -> X8R8G8B8`, so the Targa sources land
//!   as: 32-bit -> `A8R8G8B8`, 24-bit -> `X8R8G8B8`, 16-bit ->
//!   `A8R8G8B8` (through the conversion branch), paletted/mono 8-bit ->
//!   `A8R8G8B8` (same branch), other 8-bit -> `A8`. The 16-bit-desktop
//!   branches and the caps fallbacks of the original are dead under the
//!   pin.
//! - **The DDS shadow** (the loader tries the `.dds` sibling of the
//!   same basename first when the caps support DXTC) is modelled down
//!   to the name-munging availability probe. The game's font directory
//!   ships `.tga` files only; a present sibling would load through the
//!   [`dds`](crate::dds) module's path and feed the same font walk (the
//!   caller owns that lookup).
//! - **D3DXLoadSurfaceFromSurface** (`D3DX_FILTER_NONE`) is modelled
//!   after the documented software behaviour, verified against the Wine
//!   reference implementation: empty source or destination rectangles
//!   are successful no-ops; the copy is 1:1 top-left over the minimum
//!   of both extents with the destination beyond zero-filled; channels
//!   narrow by truncating their high bits (`a8 -> a4` is `>> 4`) and
//!   widen by replicating them; channels the source format lacks are
//!   set to their maximum (`X8R8G8B8 -> A4R4G4B4` gives alpha `0xf`,
//!   `A8 -> A4R4G4B4` gives rgb `0xf`). The same-format fast path of
//!   `SurfaceClass::Copy` is dead under the pin (the packed page is
//!   `A4R4G4B4`; no Targa source lands as `A4R4G4B4`), and the
//!   read+write converter pair is byte-identical to it for every
//!   reachable same-format shape.
//! - **The surface pitch** is `width * bytes_per_pixel` (the packed
//!   pitch D3D8 system-memory surfaces use); every observable access is
//!   within-row.
//! - **The caller's palette buffer** (`char palette[256*4]` on the
//!   loader's stack) is zero-filled where the file's colormap does not
//!   cover it; the original reads uninitialised stack there. The port
//!   keeps `CMapStart = 0` and full-length colormaps where the palette
//!   matters (as every real font file does); the original overflows the
//!   buffer for `CMapStart != 0` or oversized colormaps — the port
//!   reads those colormap bytes into a sink instead of writing out of
//!   bounds (the stream position matches; the palette keeps its zeros).
//! - **The null-surface crash**: an unsupported pixel depth or a zero
//!   dimension makes `CreateImageSurface` fail, and the original
//!   dereferences the null surface in `Get_Description` — stated as the
//!   [`FontVerdict::CrashNullSurface`] verdict (the tables stay zeroed,
//!   the name never assigns). A dimension outside `1..=2048` lands in
//!   the same verdict: the original's image allocation dies on such
//!   shapes instead (a `malloc`/`new` failure the original never
//!   checks — a stated stand-in; every real font fits the ceiling).
//! - **The `(int)` and `(unsigned char)` truncations** of the original
//!   are float-to-int casts (`as i32`/`as u8`); the corpora keep every
//!   casted value in range (the C++ casts are UB out of range, the
//!   Rust ones saturate — never distinguished by a real font).

use crate::font_tga::{FontTarga, TGA_MONO, TGAIDF_YORIGIN};

/// The glyph count the tables hold (the original's fixed 256).
pub const GLYPH_COUNT: usize = 256;

/// The packed page's bytes per pixel (`WW3D_FORMAT_A4R4G4B4`).
pub const PACKED_BPP: usize = 2;

/// The missing-texture placeholder's solid colour, `0x7FFF00FF` —
/// half-alpha magenta (BGRA bytes `ff 00 ff 7f`).
pub const MISSING_TEXTURE_COLOR: u32 = 0x7f_ff_00_ff;

/// The missing-texture placeholder's size.
pub const MISSING_TEXTURE_SIZE: u32 = 128;

/// The dimension ceiling a font file may declare (the real font files
/// fit comfortably; larger shapes are the crash verdict — see the
/// module docs).
pub const MAX_SURFACE_DIM: i32 = 2048;

// --- the verdict ---------------------------------------------------------------

/// The load-path verdict, the golden script's `verdict` line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontVerdict {
    /// The Targa loaded and the tables built.
    Ok,
    /// The file was absent (the placeholder surface built the tables).
    Missing,
    /// The Targa open or load failed with the code (the placeholder
    /// surface built the tables); the codes are the `TGAERR_*` values
    /// (`-1` open, `-2` read, `-6` not supported).
    TgaError(i32),
    /// A dereference site the original crashes on (the port keeps the
    /// zeroed tables and the empty name): `null-surface` = the failed
    /// `CreateImageSurface` (an unsupported pixel depth, a zero
    /// dimension or an over-ceiling shape); `findbb-lock` = the failed
    /// `FindBB` rect lock (a grid dimension of 8 mod 16 pushes the 15th
    /// column/row's bounding rect past the surface edge).
    Crash(&'static str),
}

/// The null-surface crash site.
pub const CRASH_NULL_SURFACE: &str = "null-surface";

/// The FindBB rect-lock crash site.
pub const CRASH_FINDBB_LOCK: &str = "findbb-lock";

impl FontVerdict {
    /// The golden script's word.
    pub fn word(&self) -> String {
        match *self {
            Self::Ok => "ok".to_string(),
            Self::Missing => "missing".to_string(),
            Self::TgaError(code) => format!("tgaerr-{code}"),
            Self::Crash(site) => format!("crash-{site}"),
        }
    }
}

// --- the source surface ----------------------------------------------------------

/// The surface format the Targa source lands as under the pinned
/// environment (see the module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceFormat {
    /// 32-bit, or anything the conversion branch produced (16-bit,
    /// paletted, mono).
    A8R8G8B8,
    /// 24-bit sources (no alpha bits — every glyph dies).
    X8R8G8B8,
    /// 8-bit non-paletted non-mono sources.
    A8,
    /// The missing-texture placeholder (128x128 solid).
    Placeholder,
}

impl SourceFormat {
    /// The golden script's word.
    pub fn word(&self) -> &'static str {
        match self {
            Self::A8R8G8B8 => "A8R8G8B8",
            Self::X8R8G8B8 => "X8R8G8B8",
            Self::A8 => "A8",
            Self::Placeholder => "PLACEHOLDER",
        }
    }

    /// `Get_Bytes_Per_Pixel` for the formats the font path produces.
    pub fn bytes_per_pixel(self) -> usize {
        match self {
            Self::A8R8G8B8 | Self::X8R8G8B8 | Self::Placeholder => 4,
            Self::A8 => 1,
        }
    }

    /// `Alpha_Bits`: A8R8G8B8 and A8 carry 8, X8R8G8B8 carries none
    /// (every column reads transparent).
    fn alpha_bits(self) -> i32 {
        match self {
            Self::A8R8G8B8 | Self::A8 | Self::Placeholder => 8,
            Self::X8R8G8B8 => 0,
        }
    }
}

/// The surface a font file loaded into (the pre-minimize image).
#[derive(Debug, Clone)]
pub struct SourceSurface {
    /// The surface width in pixels.
    pub width: u32,
    /// The surface height in pixels.
    pub height: u32,
    /// The surface format.
    pub format: SourceFormat,
    /// The row bytes (the packed pitch, `width * bytes_per_pixel`), in
    /// BGRA pixel order like the original's surfaces.
    pub bytes: Vec<u8>,
}

impl SourceSurface {
    /// The packed pitch.
    pub fn pitch(&self) -> usize {
        self.width as usize * self.format.bytes_per_pixel()
    }

    /// `SurfaceClass::Is_Transparent_Column`: the alpha is the pixel's
    /// last byte narrowed by the format's alpha bits (a no-alpha format
    /// reads every column transparent — the release assert is a no-op;
    /// the shift goes through `u32` like the original's int promotion,
    /// where `byte >> 8` is 0 and not a debug trap). The 32-bit hot
    /// path (the strike scan's driver) strides the alpha bytes
    /// directly (the §6 budget run's reshaping).
    fn is_transparent_column(&self, column: usize) -> bool {
        let alphabits = self.format.alpha_bits();
        let mask = alpha_mask(alphabits);
        let size = self.format.bytes_per_pixel();
        let shift = (8 - alphabits) as u32;
        match (self.format, shift) {
            // A8R8G8B8 (and the placeholder): the alpha byte as-is
            (SourceFormat::A8R8G8B8, 0) | (SourceFormat::Placeholder, 0) => self.bytes
                [column * 4 + 3..]
                .chunks_exact(self.pitch())
                .all(|row| row[0] == 0),
            _ => {
                for y in 0..self.height as usize {
                    let px = &self.bytes[y * self.pitch() + column * size..][..size];
                    let myalpha = ((px[size - 1] as u32) >> shift) as u8 & mask;
                    if myalpha != 0 {
                        return false;
                    }
                }
                true
            }
        }
    }

    /// `SurfaceClass::FindBB`: the bounding box of the non-zero-alpha
    /// pixels in `[min, max)`, with the swapped initial realmin/realmax
    /// (an empty region inverts the box). `None` = the rect lock
    /// failed: a rect reaching past the surface makes `LockRect` fail
    /// and the original dereferences the zeroed lock — the crash site
    /// (a grid dimension of 8 mod 16 pushes the 15th column/row's rect
    /// past the edge).
    fn find_bb(
        &self,
        min_i: i64,
        min_j: i64,
        max_i: i64,
        max_j: i64,
    ) -> Option<(i64, i64, i64, i64)> {
        if max_i > self.width as i64 || max_j > self.height as i64 {
            return None;
        }
        let alphabits = self.format.alpha_bits();
        let mask = alpha_mask(alphabits);
        let size = self.format.bytes_per_pixel();
        let shift = (8 - alphabits) as u32;
        let mut realmin_i = max_i;
        let mut realmin_j = max_j;
        let mut realmax_i = min_i;
        let mut realmax_j = min_j;
        for y in min_j..max_j {
            for x in min_i..max_i {
                let px = &self.bytes[y as usize * self.pitch() + x as usize * size..][..size];
                let myalpha = ((px[size - 1] as u32) >> shift) as u8 & mask;
                if myalpha != 0 {
                    realmin_i = realmin_i.min(x);
                    realmax_i = realmax_i.max(x);
                    realmin_j = realmin_j.min(y);
                    realmax_j = realmax_j.max(y);
                }
            }
        }
        Some((realmin_i, realmin_j, realmax_i, realmax_j))
    }
}

/// The alpha masks `Is_Transparent_Column`/`FindBB` narrow through
/// (`Alpha_Bits` of the format).
fn alpha_mask(alphabits: i32) -> u8 {
    match alphabits {
        1 => 1,
        4 => 0x0f,
        8 => 0xff,
        _ => 0,
    }
}

// --- the packed page -----------------------------------------------------------------

/// The packed glyph page — `Minimize_Font_Image`'s output (the
/// `TextureClass` level-0 content the original uploads).
#[derive(Debug, Clone)]
pub struct PackedPage {
    /// The page width (128 or 256).
    pub width: u32,
    /// The page height (always the width — the original's square pick).
    pub height: u32,
    /// The `A4R4G4B4` row bytes (little-endian u16 per pixel, packed
    /// pitch).
    pub bytes: Vec<u8>,
}

impl PackedPage {
    /// The packed pitch (`width * 2`).
    pub fn pitch(&self) -> usize {
        self.width as usize * PACKED_BPP
    }
}

/// `SurfaceClass::Copy` (the region form): the source rect clamped to
/// the source, the destination rect clamped to the page, then the D3DX
/// load (the same-format fast path is dead under the pinned
/// environment — the page is `A4R4G4B4`, no Targa source lands as it;
/// see the module docs). The three reachable source formats run as
/// dedicated inner loops (the §6 budget run's reshaping); the generic
/// channel model stays below for completeness.
#[allow(clippy::too_many_arguments)]
fn copy_region(
    page: &mut PackedPage,
    dstx: i32,
    dsty: i32,
    src: &SourceSurface,
    srcx: i32,
    srcy: i32,
    width: i32,
    height: i32,
) {
    // the source rect, clamped to the source surface
    let src_rect = (
        srcx as i64,
        srcy as i64,
        (srcx as i64 + width as i64).min(src.width as i64),
        (srcy as i64 + height as i64).min(src.height as i64),
    );
    // the destination rect, clamped to the page
    let dst_rect = (
        dstx as i64,
        dsty as i64,
        (dstx as i64 + width as i64).min(page.width as i64),
        (dsty as i64 + height as i64).min(page.height as i64),
    );

    let (sl, st, sr, sb) = src_rect;
    let (dl, dt, dr, db) = dst_rect;
    if sl >= sr || st >= sb || dl >= dr || dt >= db {
        return; // empty rects are successful no-ops
    }

    let sw = (sr - sl) as usize;
    let sh = (sb - st) as usize;
    let dw = (dr - dl) as usize;
    let dh = (db - dt) as usize;
    let min_w = sw.min(dw);
    let min_h = sh.min(dh);
    let src_pitch = src.pitch();
    let dst_pitch = page.pitch();
    let sbpp = src.format.bytes_per_pixel();

    match src.format {
        // A8R8G8B8 -> A4R4G4B4: each channel truncated to its high
        // four bits (the d3dx model's narrowing)
        SourceFormat::A8R8G8B8 | SourceFormat::Placeholder => {
            for y in 0..min_h {
                let drow = &mut page.bytes
                    [(dt as usize + y) * dst_pitch + dl as usize * PACKED_BPP..]
                    [..min_w * PACKED_BPP];
                let srow =
                    &src.bytes[(st as usize + y) * src_pitch + sl as usize * 4..][..min_w * 4];
                for (out, s) in drow
                    .as_chunks_mut::<2>()
                    .0
                    .iter_mut()
                    .zip(srow.as_chunks::<4>().0.iter())
                {
                    let v: u16 = ((s[0] & 0xf0) as u16) >> 4
                        | ((s[1] & 0xf0) as u16)
                        | ((s[2] & 0xf0) as u16) << 4
                        | ((s[3] & 0xf0) as u16) << 8;
                    *out = v.to_le_bytes();
                }
            }
        }
        // X8R8G8B8 -> A4R4G4B4: rgb truncated, the missing alpha
        // channel set to its maximum
        SourceFormat::X8R8G8B8 => {
            for y in 0..min_h {
                let drow = &mut page.bytes
                    [(dt as usize + y) * dst_pitch + dl as usize * PACKED_BPP..]
                    [..min_w * PACKED_BPP];
                let srow =
                    &src.bytes[(st as usize + y) * src_pitch + sl as usize * 4..][..min_w * 4];
                for (out, s) in drow
                    .as_chunks_mut::<2>()
                    .0
                    .iter_mut()
                    .zip(srow.as_chunks::<4>().0.iter())
                {
                    let v: u16 = ((s[0] & 0xf0) as u16) >> 4
                        | ((s[1] & 0xf0) as u16)
                        | ((s[2] & 0xf0) as u16) << 4
                        | (0xfu16 << 12);
                    *out = v.to_le_bytes();
                }
            }
        }
        // A8 -> A4R4G4B4: the alpha truncated, the missing rgb channels
        // set to their maximum
        SourceFormat::A8 => {
            for y in 0..min_h {
                let drow = (dt as usize + y) * dst_pitch + dl as usize * PACKED_BPP;
                let srow = (st as usize + y) * src_pitch + sl as usize;
                for x in 0..min_w {
                    let a = (src.bytes[srow + x] & 0xf0) as u16;
                    let out = 0x0fff | (a << 8);
                    page.bytes[drow + x * PACKED_BPP..][..2].copy_from_slice(&out.to_le_bytes());
                }
            }
        }
    }

    // the destination beyond the source extent is zero-filled (the
    // model's black-out; the font path's rects clamp the destination
    // only, so this never fires — kept for the model's totality)
    if sw < dw {
        for y in 0..min_h {
            let drow = (dt as usize + y) * dst_pitch + dl as usize * PACKED_BPP;
            for x in min_w..dw {
                page.bytes[drow + x * PACKED_BPP..][..PACKED_BPP].fill(0);
            }
        }
    }
    if sh < dh {
        for y in min_h..dh {
            let at = (dt as usize + y) * dst_pitch + dl as usize * PACKED_BPP;
            for x in 0..dw {
                page.bytes[at + x * PACKED_BPP..][..PACKED_BPP].fill(0);
            }
        }
    }
    let _ = sbpp;
}

// --- the font data --------------------------------------------------------------------

/// A loaded font — `Font3DDataClass` after `Load_Font_Image`.
#[derive(Debug, Clone)]
pub struct Font3DData {
    /// The `strdup`+`strupr`'d name (empty on the crash verdict — the
    /// crash site precedes the assignment).
    pub name: Vec<u8>,
    /// The load verdict.
    pub verdict: FontVerdict,
    /// The surface the file loaded into (the placeholder for the
    /// missing/error paths; `None` on the crash verdict).
    pub source: Option<SourceSurface>,
    /// The normalized per-character U offsets (the packed page's).
    pub u_offset: [f32; GLYPH_COUNT],
    /// The normalized per-character V offsets.
    pub v_offset: [f32; GLYPH_COUNT],
    /// The normalized per-character U widths.
    pub u_width: [f32; GLYPH_COUNT],
    /// The normalized glyph height.
    pub v_height: f32,
    /// The uchar per-character pixel widths (the truncated table — see
    /// the module docs).
    pub char_width: [u8; GLYPH_COUNT],
    /// The uchar pixel height.
    pub char_height: u8,
    /// The packed glyph page (`None` on the crash verdict).
    pub packed: Option<PackedPage>,
}

impl Default for Font3DData {
    fn default() -> Self {
        Self {
            name: Vec::new(),
            verdict: FontVerdict::Ok,
            source: None,
            u_offset: [0.0; GLYPH_COUNT],
            v_offset: [0.0; GLYPH_COUNT],
            u_width: [0.0; GLYPH_COUNT],
            v_height: 0.0,
            char_width: [0; GLYPH_COUNT],
            char_height: 0,
            packed: None,
        }
    }
}

impl Font3DData {
    /// `Font3DDataClass::Char_Width` — the uchar table, the index
    /// masked to the table's range.
    pub fn char_width(&self, ch: u16) -> u8 {
        self.char_width[(ch & 0xff) as usize]
    }

    /// `Font3DDataClass::Char_Height` (the argument is ignored, like
    /// the original).
    pub fn char_height(&self) -> u8 {
        self.char_height
    }

    /// `Font3DDataClass::Char_U_Offset`.
    pub fn char_u_offset(&self, ch: u16) -> f32 {
        self.u_offset[(ch & 0xff) as usize]
    }

    /// `Font3DDataClass::Char_V_Offset`.
    pub fn char_v_offset(&self, ch: u16) -> f32 {
        self.v_offset[(ch & 0xff) as usize]
    }

    /// `Font3DDataClass::Char_U_Width`.
    pub fn char_u_width(&self, ch: u16) -> f32 {
        self.u_width[(ch & 0xff) as usize]
    }

    /// `Font3DDataClass::Char_V_Height` (the argument is ignored, like
    /// the original).
    pub fn char_v_height(&self) -> f32 {
        self.v_height
    }

    /// `Minimize_Font_Image` — the packed-page re-layout (the load
    /// runs it; exposed as the module's core walk).
    fn minimize_font_image(
        u_offset: &mut [f32; GLYPH_COUNT],
        v_offset: &mut [f32; GLYPH_COUNT],
        u_width: &mut [f32; GLYPH_COUNT],
        v_height: &mut f32,
        surface: &SourceSurface,
    ) -> PackedPage {
        let current_width = surface.width as f32;
        let current_height = surface.height as f32;

        // determine new width make the size of the new image either
        // 128x128 or 256x256, dependent on the width of the original
        // image
        let new_width: i32 = if current_width < 256.0 { 128 } else { 256 };
        let new_height = new_width;
        // create a new 4 bit alpha image to build into, fill with
        // transparent black
        let mut page = PackedPage {
            width: new_width as u32,
            height: new_height as u32,
            bytes: vec![0; new_width as usize * new_height as usize * PACKED_BPP],
        };

        // indices for the location of each added char
        let mut new_x: i32 = 0;
        let mut new_y: i32 = 0;

        // for each character, copy minimum bounding area to (new_x,
        // new_y) in the new image
        for char_index in 0..GLYPH_COUNT {
            // (the normalized uv tables converted to pixels, rounded
            // off)
            let src_x = (u_offset[char_index] * current_width + 0.5) as i32;
            let src_y = (v_offset[char_index] * current_height + 0.5) as i32;
            let width = (u_width[char_index] * current_width + 0.5) as i32;
            let height = (*v_height * current_height + 0.5) as i32;

            // if the character has any visible pixels at all...
            if width != 0 {
                // if this character will not fit on the current line,
                // goto the next line
                if new_x + width > new_width {
                    new_x = 0;
                    new_y += height;

                    // if we have run out of lines, we have a problem
                    // (the original steps back and overlaps the row)
                    if new_y + height > new_height {
                        new_y -= height;
                    }
                }

                // blit from original image to new image
                copy_region(
                    &mut page, new_x, new_y, surface, src_x, src_y, width, height,
                );
            }

            // update the U and V tables to show new character location
            // (the original divides V by new_width — a no-op, the page
            // is square)
            u_offset[char_index] = new_x as f32 / new_width as f32;
            v_offset[char_index] = new_y as f32 / new_width as f32;

            // update width in terms of new normal image width
            u_width[char_index] *= current_width / new_width as f32;

            new_x += width;
        }

        // update height in terms of new normal image height
        *v_height *= current_height / new_height as f32;

        page
    }

    /// `Make_Proportional` — the grid path's per-cell bounding-box
    /// walk, ending in the minimize. `None` = the FindBB rect-lock
    /// crash site (see [`find_bb`](SourceSurface::find_bb)).
    fn make_proportional(
        u_offset: &mut [f32; GLYPH_COUNT],
        v_offset: &mut [f32; GLYPH_COUNT],
        u_width: &mut [f32; GLYPH_COUNT],
        char_width: &mut [u8; GLYPH_COUNT],
        v_height: &mut f32,
        surface: &SourceSurface,
    ) -> Option<PackedPage> {
        let width = surface.width as f32;
        let height = surface.height as f32;

        // for each character in the font...
        for char_index in 0..GLYPH_COUNT {
            // find the current bounding box (the normalized uv tables
            // converted to pixels, rounded off)
            let x0 = (u_offset[char_index] * width + 0.5) as i32;
            let y0 = (v_offset[char_index] * height + 0.5) as i32;
            let x1 = x0 + (u_width[char_index] * width + 0.5) as i32;
            let y1 = y0 + (*v_height * height + 0.5) as i32;

            // find minimum bounding box by finding the minimum and
            // maximum non-0 x pixel location
            let (minb_i, _minb_j, maxb_i, _maxb_j) =
                surface.find_bb(x0 as i64, y0 as i64, x1 as i64, y1 as i64)?;

            // set the new edges
            let x0 = minb_i as i32;
            let mut x1 = maxb_i as i32 + 1;

            // if we didn't find ANY non-transparent pixels, the char
            // has no width.
            if x1 < x0 {
                x1 = x0;
            }

            // turn off all character after del
            if char_index > 0x80 {
                x1 = x0;
            }

            // update the U and width tables
            u_offset[char_index] = x0 as f32 / width;
            u_width[char_index] = (x1 - x0) as f32 / width;
            char_width[char_index] = (x1 - x0) as u8;
        }

        // now shrink the image given the minimum char sizes
        Some(Self::minimize_font_image(
            u_offset, v_offset, u_width, v_height, surface,
        ))
    }
}

// --- the load entry ---------------------------------------------------------------------

/// `Font3DDataClass::Font3DDataClass` — the whole load: the surface
/// stack, the strike/grid branch, the minimize and the uppercased name.
///
/// `file` is the `.tga` bytes the name resolves to (`None` = the file
/// is absent; the missing-texture placeholder builds the tables). See
/// the module docs for the pinned environment (the `.dds` shadow and
/// the display format selection).
pub fn parse_font(name: &[u8], file: Option<&[u8]>) -> Font3DData {
    let mut font = Font3DData::default();

    // get the font surface (the SurfaceClass file constructor)
    let (surface, verdict) = match load_surface_immediate(file) {
        Ok((surface, verdict)) => (surface, verdict),
        Err(()) => {
            // the null-surface crash site: the tables never build, the
            // name never assigns
            font.verdict = FontVerdict::Crash(CRASH_NULL_SURFACE);
            return font;
        }
    };
    font.verdict = verdict;
    font.source = Some(surface);

    let surface = font.source.as_ref().expect("just set");

    let mut u_offset = [0f32; GLYPH_COUNT];
    let mut v_offset = [0f32; GLYPH_COUNT];
    let mut u_width = [0f32; GLYPH_COUNT];
    let mut char_width = [0u8; GLYPH_COUNT];
    let mut v_height;
    let char_height;
    let packed;

    // If input is a font strike (strip) process it as such
    if surface.width > 8 * surface.height {
        // the height of the strike is the height of the characters
        v_height = 1.0;
        char_height = surface.height as u8;

        let mut column: i32 = 0;
        let width = surface.width as i32;

        // for each char, find the uv start location and set the
        // mono-spaced width and height in normalized screen units
        for char_index in 0..GLYPH_COUNT {
            if char_index >= 0x7f {
                u_offset[char_index] = 0.0;
                v_offset[char_index] = 0.0;
                u_width[char_index] = 0.0;
                char_width[char_index] = 0;
            } else {
                // find the first non-transparent column...
                while column < width && surface.is_transparent_column(column as usize) {
                    column += 1;
                }
                let start = column;

                // find the first transparent column...
                while column < width && !surface.is_transparent_column(column as usize) {
                    column += 1;
                }
                let end = column;

                // (the original's end <= start debug message is a
                // release no-op)

                u_offset[char_index] = start as f32 / width as f32;
                v_offset[char_index] = 0.0;
                u_width[char_index] = (end - start) as f32 / width as f32;
                char_width[char_index] = (end - start) as u8;
            }
        }

        // convert the just created mono-spaced font to proportional
        // (the original's Make_Proportional call is commented out on
        // the strike path)
        packed = Font3DData::minimize_font_image(
            &mut u_offset,
            &mut v_offset,
            &mut u_width,
            &mut v_height,
            surface,
        );
    } else {
        // Determine the width and height of each mono spaced character
        // in pixels (assumes 16x16 array of chars)
        let font_width = surface.width as f32;
        let font_height = surface.height as f32;
        let mono_pixel_width = font_width / 16.0;
        let mono_pixel_height = font_height / 16.0;

        // for each char, find the uv start location and set the
        // mono-spaced width and height in normalized screen units
        for char_index in 0..GLYPH_COUNT {
            u_offset[char_index] = ((char_index % 16) as f32 * mono_pixel_width) / font_width;
            v_offset[char_index] = ((char_index / 16) as f32 * mono_pixel_height) / font_height;
            u_width[char_index] = mono_pixel_width / font_width;
            char_width[char_index] = mono_pixel_width as u8;
        }
        v_height = mono_pixel_height / font_height;
        char_height = mono_pixel_height as u8;

        // convert the just created mono-spaced font to proportional
        // (the FindBB rect-lock crash site lives inside — the object
        // dies there, so the port keeps the zeroed tables)
        packed = match Font3DData::make_proportional(
            &mut u_offset,
            &mut v_offset,
            &mut u_width,
            &mut char_width,
            &mut v_height,
            surface,
        ) {
            Some(packed) => packed,
            None => {
                font.verdict = FontVerdict::Crash(CRASH_FINDBB_LOCK);
                font.source = None;
                return font;
            }
        };
    }

    font.u_offset = u_offset;
    font.v_offset = v_offset;
    font.u_width = u_width;
    font.v_height = v_height;
    font.char_width = char_width;
    font.char_height = char_height;
    font.packed = Some(packed);

    // the strdup+strupr'd name (after the load, like the original)
    font.name = name.to_ascii_uppercase();

    font
}

/// `TextureLoader::Load_Surface_Immediate` under the pinned
/// environment (see the module docs). `Err(())` is the null-surface
/// crash site.
fn load_surface_immediate(file: Option<&[u8]>) -> Result<(SourceSurface, FontVerdict), ()> {
    // the SurfaceClass file constructor's availability probes: an
    // absent .tga (with its absent .dds fallback) is the
    // missing-texture path
    let bytes = match file {
        Some(bytes) => bytes,
        None => return Ok((placeholder_surface(), FontVerdict::Missing)),
    };

    // the DDS shadow (the munged .dds sibling) is the caller's — the
    // stated environment ships none (see the module docs)

    // Make sure the file can be opened. If not, return missing texture.
    let mut targa = FontTarga::new(bytes);
    if let Err(code) = targa.open_read() {
        return Ok((placeholder_surface(), FontVerdict::TgaError(code)));
    }

    // DX8 uses image upside down compared to TGA
    targa.header.image_descriptor ^= TGAIDF_YORIGIN;

    let (src_format, src_bpp) = get_ww3d_format(&targa);
    let mut dest_format = surface_format_of(src_format);
    if matches!(src_format, TgaSrcFormat::P8 | TgaSrcFormat::L8) {
        dest_format = SurfaceFormat::X8R8G8B8;
    }
    let mut dest_format = valid_texture_format(dest_format);

    let width = targa.header.width.max(0) as u32;
    let height = targa.header.height.max(0) as u32;
    let src_width = width;
    let src_height = height;

    // the image allocation ceiling: an over-ceiling or negative shape
    // dies on the original's image malloc (never checked) — the crash
    // verdict is the stated stand-in (see the module docs)
    if targa.header.width as i32 > MAX_SURFACE_DIM || targa.header.height as i32 > MAX_SURFACE_DIM {
        return Err(());
    }

    // NOTE: We load the palette but we do not yet support paletted
    // textures! (the caller's 1024-byte buffer, zero-filled — see the
    // module docs)
    let mut palette = [0u8; 256 * 4];
    if let Err(code) = targa.load_image(&mut palette) {
        return Ok((placeholder_surface(), FontVerdict::TgaError(code)));
    }

    // the created surface refuses zero dimensions and unknown formats —
    // the null-surface crash site; a null image (the allocation
    // ceiling) is the same family: the original dereferences the null
    // buffer in Copy_Image
    if targa.header.width <= 0
        || targa.header.height <= 0
        || dest_format == SurfaceFormat::Unknown
        || targa.image.is_empty()
    {
        return Err(());
    }

    let mut src_surface: &[u8] = &targa.image;
    let mut src_format = src_format;
    let mut src_width = src_width;
    let mut src_height = src_height;
    let mut src_bpp = src_bpp;

    // No paletted destination format allowed
    let converted;
    if matches!(
        src_format,
        TgaSrcFormat::A1R5G5B5 | TgaSrcFormat::P8 | TgaSrcFormat::L8
    ) || src_width != width
        || src_height != height
    {
        let mut surface = vec![0u8; width as usize * height as usize * 4];
        copy_image(
            &mut surface,
            width,
            height,
            width as usize * 4,
            SurfaceFormat::A8R8G8B8,
            src_surface,
            src_width,
            src_height,
            src_width as usize * src_bpp,
            src_format,
            &palette,
            (targa.header.cmap_depth >> 3) as usize,
        );
        converted = surface;
        src_surface = &converted;
        src_format = TgaSrcFormat::A8R8G8B8;
        src_width = width;
        src_height = height;
        src_bpp = 4;
        // the branch reassigns the destination (the original's
        // dest_format = Get_Valid_Texture_Format(A8R8G8B8))
        dest_format = SurfaceFormat::A8R8G8B8;
    }

    let src_pitch = src_width as usize * src_bpp;

    let format = match dest_format {
        SurfaceFormat::A8R8G8B8 => SourceFormat::A8R8G8B8,
        SurfaceFormat::X8R8G8B8 => SourceFormat::X8R8G8B8,
        SurfaceFormat::A8 => SourceFormat::A8,
        SurfaceFormat::Unknown => return Err(()),
    };
    let surface_pitch = width as usize * format.bytes_per_pixel();
    let mut surface = SourceSurface {
        width,
        height,
        format,
        bytes: vec![0; surface_pitch * height as usize],
    };

    copy_image(
        &mut surface.bytes,
        width,
        height,
        surface_pitch,
        dest_format,
        src_surface,
        src_width,
        src_height,
        src_pitch,
        src_format,
        &palette,
        (targa.header.cmap_depth >> 3) as usize,
    );

    Ok((surface, FontVerdict::Ok))
}

/// `MissingTexture::_Create_Missing_Surface`: the 128x128 `A8R8G8B8`
/// surface `_Init` fills with the solid `0x7FFF00FF`.
fn placeholder_surface() -> SourceSurface {
    let mut bytes =
        Vec::with_capacity(MISSING_TEXTURE_SIZE as usize * MISSING_TEXTURE_SIZE as usize * 4);
    for _ in 0..MISSING_TEXTURE_SIZE * MISSING_TEXTURE_SIZE {
        bytes.extend_from_slice(&MISSING_TEXTURE_COLOR.to_le_bytes());
    }
    SourceSurface {
        width: MISSING_TEXTURE_SIZE,
        height: MISSING_TEXTURE_SIZE,
        format: SourceFormat::Placeholder,
        bytes,
    }
}

// --- the Targa formats and the conversion stack ------------------------------------------

/// The Targa source formats `Get_WW3D_Format` maps the pixel depths to
/// (`R5G6B5`/`A4R4G4B4` never occur — the mapping's 16-bit case is
/// always `A1R5G5B5`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TgaSrcFormat {
    A8R8G8B8,
    R8G8B8,
    A1R5G5B5,
    P8,
    L8,
    A8,
    Unknown,
}

/// The destination formats the pinned `Get_Valid_Texture_Format` can
/// produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SurfaceFormat {
    A8R8G8B8,
    X8R8G8B8,
    A8,
    Unknown,
}

/// `ww3dformat.cpp`'s `Get_WW3D_Format` (the source form): the pixel
/// depth picks the format, the colormap and image type refine 8-bit.
fn get_ww3d_format(targa: &FontTarga) -> (TgaSrcFormat, usize) {
    match targa.header.pixel_depth {
        32 => (TgaSrcFormat::A8R8G8B8, 4),
        24 => (TgaSrcFormat::R8G8B8, 3),
        16 => (TgaSrcFormat::A1R5G5B5, 2),
        8 => {
            if targa.header.cmap_type == 1 {
                (TgaSrcFormat::P8, 1)
            } else if targa.header.image_type == TGA_MONO {
                (TgaSrcFormat::L8, 1)
            } else {
                (TgaSrcFormat::A8, 1)
            }
        }
        _ => (TgaSrcFormat::Unknown, 0),
    }
}

/// The `Get_WW3D_Format` destination pre-map (`P8`/`L8 -> X8R8G8B8`;
/// an unknown source depth stays unknown — the null-surface crash).
fn surface_format_of(src: TgaSrcFormat) -> SurfaceFormat {
    match src {
        TgaSrcFormat::A8R8G8B8 => SurfaceFormat::A8R8G8B8,
        TgaSrcFormat::R8G8B8 => SurfaceFormat::X8R8G8B8,
        TgaSrcFormat::P8 | TgaSrcFormat::L8 => SurfaceFormat::X8R8G8B8,
        TgaSrcFormat::A8 => SurfaceFormat::A8,
        // unreachable from a Targa source; kept total
        TgaSrcFormat::A1R5G5B5 => SurfaceFormat::A8,
        TgaSrcFormat::Unknown => SurfaceFormat::Unknown,
    }
}

/// `Get_Valid_Texture_Format` under the pinned environment: the
/// identity except `R8G8B8 -> X8R8G8B8` (see the module docs).
fn valid_texture_format(format: SurfaceFormat) -> SurfaceFormat {
    format
}

/// `BitmapHandlerClass::Read_B8G8R8A8` — the source pixel converted to
/// 32-bit BGRA bytes.
fn read_b8g8r8a8(
    src: &[u8],
    src_format: TgaSrcFormat,
    palette: &[u8],
    palette_bpp: usize,
) -> [u8; 4] {
    let mut argb = [0u8; 4];
    match src_format {
        TgaSrcFormat::A8R8G8B8 => {
            argb[..4].copy_from_slice(&src[..4]);
        }
        TgaSrcFormat::R8G8B8 => {
            argb[0] = src[0];
            argb[1] = src[1];
            argb[2] = src[2];
            argb[3] = 0xff;
        }
        TgaSrcFormat::A1R5G5B5 => {
            let tmp = u16::from_le_bytes([src[0], src[1]]);
            argb[3] = if tmp & 0x8000 != 0 { 0xff } else { 0x00 };
            argb[2] = ((tmp >> 7) & 0xf8) as u8;
            argb[1] = ((tmp >> 2) & 0xf8) as u8;
            argb[0] = ((tmp << 3) & 0xf8) as u8;
        }
        TgaSrcFormat::L8 => {
            argb[0] = src[0];
            argb[1] = src[0];
            argb[2] = src[0];
            argb[3] = 0xff;
        }
        TgaSrcFormat::A8 => {
            argb[0] = 0;
            argb[1] = 0;
            argb[2] = 0;
            argb[3] = src[0];
        }
        TgaSrcFormat::P8 => {
            let index = src[0] as usize;
            match palette_bpp {
                4 => {
                    argb[0] = palette[palette_bpp * index + 3];
                    argb[1] = palette[palette_bpp * index + 2];
                    argb[2] = palette[palette_bpp * index + 1];
                    argb[3] = palette[palette_bpp * index];
                }
                3 => {
                    argb[0] = palette[palette_bpp * index + 2];
                    argb[1] = palette[palette_bpp * index + 1];
                    argb[2] = palette[palette_bpp * index];
                    argb[3] = 0xff;
                }
                _ => {
                    // the 1/2-byte colormaps assert in the original (a
                    // release no-op) and leave the output unwritten —
                    // the port pins the zero-filled result
                }
            }
        }
        TgaSrcFormat::Unknown => {
            // unreachable from a Targa source; the zero-filled stand-in
        }
    }
    argb
}

/// `BitmapHandlerClass::Write_B8G8R8A8` — the 32-bit BGRA value
/// converted to the destination bytes.
fn write_b8g8r8a8(argb: &[u8; 4], dest_format: SurfaceFormat, dest: &mut [u8]) {
    match dest_format {
        SurfaceFormat::A8R8G8B8 | SurfaceFormat::X8R8G8B8 => {
            dest[..4].copy_from_slice(argb);
        }
        SurfaceFormat::A8 => {
            dest[0] = argb[3];
        }
        SurfaceFormat::Unknown => {}
    }
}

/// `BitmapHandlerClass::Copy_Pixel` (the no-HSV form). The original's
/// same-format fast branches are byte-identical to the read+write pair
/// for every reachable combination (a raw 4-byte copy for the 32-bit
/// formats, the byte copy for A8), so the port always converts.
fn copy_pixel(
    dest: &mut [u8],
    dest_format: SurfaceFormat,
    src: &[u8],
    src_format: TgaSrcFormat,
    palette: &[u8],
    palette_bpp: usize,
) {
    let b8g8r8a8 = read_b8g8r8a8(src, src_format, palette, palette_bpp);
    write_b8g8r8a8(&b8g8r8a8, dest_format, dest);
}

/// `BitmapHandlerClass::Copy_Image`, the two paths the font loader
/// drives (the same-size walk and the point-sampled scaling walk; no
/// mips, no HSV — those branches are dead here). The reachable format
/// pairs run as dedicated inner loops (the §6 budget run's reshaping —
/// the generic per-pixel walk below is the transcription's shape and
/// stays for the unreachable pairs).
#[allow(clippy::too_many_arguments)]
fn copy_image(
    dest_surface: &mut [u8],
    dest_width: u32,
    dest_height: u32,
    dest_pitch: usize,
    dest_format: SurfaceFormat,
    src_surface: &[u8],
    src_width: u32,
    src_height: u32,
    src_pitch: usize,
    src_format: TgaSrcFormat,
    src_palette: &[u8],
    src_palette_bpp: usize,
) {
    // the same-size shape every font drives
    if dest_width == src_width && dest_height == src_height {
        let w = dest_width as usize;
        let h = dest_height as usize;
        match (dest_format, src_format) {
            // A8R8G8B8 -> A8R8G8B8: the verbatim row copy (the fill
            // pass of the converted sources and the 32-bit fonts)
            (SurfaceFormat::A8R8G8B8, TgaSrcFormat::A8R8G8B8) => {
                for (d_row, s_row) in dest_surface
                    .chunks_exact_mut(dest_pitch)
                    .zip(src_surface.chunks_exact(src_pitch))
                    .take(h)
                {
                    d_row[..w * 4].copy_from_slice(&s_row[..w * 4]);
                }
            }
            // A1R5G5B5 -> A8R8G8B8: the conversion branch's expansion
            (SurfaceFormat::A8R8G8B8, TgaSrcFormat::A1R5G5B5) => {
                for (d_row, s_row) in dest_surface
                    .chunks_exact_mut(dest_pitch)
                    .zip(src_surface.chunks_exact(src_pitch))
                    .take(h)
                {
                    for (out, px) in d_row
                        .as_chunks_mut::<4>()
                        .0
                        .iter_mut()
                        .zip(s_row.as_chunks::<2>().0.iter())
                        .take(w)
                    {
                        let tmp = u16::from_le_bytes(*px);
                        out[0] = ((tmp << 3) & 0xf8) as u8;
                        out[1] = ((tmp >> 2) & 0xf8) as u8;
                        out[2] = ((tmp >> 7) & 0xf8) as u8;
                        out[3] = if tmp & 0x8000 != 0 { 0xff } else { 0x00 };
                    }
                }
            }
            // R8G8B8 -> X8R8G8B8: the 24-bit fill (the alpha forced
            // to 0xff by the reader)
            (SurfaceFormat::X8R8G8B8, TgaSrcFormat::R8G8B8) => {
                for y in 0..h {
                    let d = y * dest_pitch;
                    let s = y * src_pitch;
                    for x in 0..w {
                        let at = d + x * 4;
                        dest_surface[at] = src_surface[s + x * 3];
                        dest_surface[at + 1] = src_surface[s + x * 3 + 1];
                        dest_surface[at + 2] = src_surface[s + x * 3 + 2];
                        dest_surface[at + 3] = 0xff;
                    }
                }
            }
            // A8 -> A8: the byte copy
            (SurfaceFormat::A8, TgaSrcFormat::A8) => {
                for y in 0..h {
                    let d = y * dest_pitch;
                    let s = y * src_pitch;
                    dest_surface[d..d + w].copy_from_slice(&src_surface[s..s + w]);
                }
            }
            // P8 -> A8R8G8B8 (32- and 24-bit colormaps) and L8 ->
            // A8R8G8B8: the palette/luminance expansions
            (SurfaceFormat::A8R8G8B8, TgaSrcFormat::P8) => {
                let bpp = src_palette_bpp;
                for y in 0..h {
                    let d = y * dest_pitch;
                    let s = y * src_pitch;
                    for x in 0..w {
                        let out = read_b8g8r8a8(
                            &src_surface[s + x..][..1],
                            TgaSrcFormat::P8,
                            src_palette,
                            bpp,
                        );
                        dest_surface[d + x * 4..][..4].copy_from_slice(&out);
                    }
                }
            }
            (SurfaceFormat::A8R8G8B8, TgaSrcFormat::L8) => {
                for y in 0..h {
                    let d = y * dest_pitch;
                    let s = y * src_pitch;
                    for x in 0..w {
                        let l = src_surface[s + x];
                        let at = d + x * 4;
                        dest_surface[at] = l;
                        dest_surface[at + 1] = l;
                        dest_surface[at + 2] = l;
                        dest_surface[at + 3] = 0xff;
                    }
                }
            }
            // everything else: the generic walk (the transcription's
            // shape)
            _ => copy_image_generic(
                dest_surface,
                dest_width,
                dest_height,
                dest_pitch,
                dest_format,
                src_surface,
                src_width,
                src_height,
                src_pitch,
                src_format,
                src_palette,
                src_palette_bpp,
            ),
        }
        return;
    }

    // the point-sampled scaling shape (unreachable from the font's
    // same-size loads; kept for the transcription's totality)
    copy_image_generic(
        dest_surface,
        dest_width,
        dest_height,
        dest_pitch,
        dest_format,
        src_surface,
        src_width,
        src_height,
        src_pitch,
        src_format,
        src_palette,
        src_palette_bpp,
    );
}

/// The generic per-pixel walk (`Copy_Pixel` for every pixel).
#[allow(clippy::too_many_arguments)]
fn copy_image_generic(
    dest_surface: &mut [u8],
    dest_width: u32,
    dest_height: u32,
    dest_pitch: usize,
    dest_format: SurfaceFormat,
    src_surface: &[u8],
    src_width: u32,
    src_height: u32,
    src_pitch: usize,
    src_format: TgaSrcFormat,
    src_palette: &[u8],
    src_palette_bpp: usize,
) {
    let dest_bpp = match dest_format {
        SurfaceFormat::A8R8G8B8 | SurfaceFormat::X8R8G8B8 => 4,
        SurfaceFormat::A8 => 1,
        SurfaceFormat::Unknown => 0,
    };
    let src_bpp = match src_format {
        TgaSrcFormat::A8R8G8B8 => 4,
        TgaSrcFormat::R8G8B8 => 3,
        TgaSrcFormat::A1R5G5B5 => 2,
        TgaSrcFormat::P8 | TgaSrcFormat::L8 | TgaSrcFormat::A8 => 1,
        TgaSrcFormat::Unknown => 0,
    };

    if dest_width == src_width && dest_height == src_height {
        for y in 0..dest_height as usize {
            let d = y * dest_pitch;
            let s = y * src_pitch;
            for x in 0..dest_width as usize {
                copy_pixel(
                    &mut dest_surface[d + x * dest_bpp..],
                    dest_format,
                    &src_surface[s + x * src_bpp..],
                    src_format,
                    src_palette,
                    src_palette_bpp,
                );
            }
        }
    } else {
        // For now do only point-sampling
        for y in 0..dest_height as usize {
            let d = y * dest_pitch;
            let src_y = y * src_height as usize / dest_height as usize;
            let s = src_y * src_pitch;
            for x in 0..dest_width as usize {
                let src_x = x * src_width as usize / dest_width as usize;
                copy_pixel(
                    &mut dest_surface[d + x * dest_bpp..],
                    dest_format,
                    &src_surface[s + src_x * src_bpp..],
                    src_format,
                    src_palette,
                    src_palette_bpp,
                );
            }
        }
    }
}

// --- the instance class -------------------------------------------------------------------

/// `Font3DInstanceClass` — the consumer's cached scaled tables.
#[derive(Debug, Clone)]
pub struct Font3DInstance {
    /// The font the instance fronts.
    pub font: Font3DData,
    /// The current scale factor.
    pub scale: f32,
    /// The non-scaled width of the space char in pixels (defaults to
    /// half the 'H' width).
    pub space_spacing: f32,
    /// The non-scaled width between chars in pixels.
    pub inter_char_spacing: f32,
    /// The non-scaled monospace char width in pixels (0 for
    /// proportional).
    pub mono_spacing: f32,
    /// The scaled per-char pixel widths.
    pub scaled_width: [f32; GLYPH_COUNT],
    /// The scaled per-char pixel spacings.
    pub scaled_spacing: [f32; GLYPH_COUNT],
    /// The scaled pixel height.
    pub scaled_height: f32,
}

impl Font3DInstance {
    /// `Font3DInstanceClass::Font3DInstanceClass` — the constructor's
    /// defaults and the cached-table build.
    pub fn new(font: Font3DData) -> Self {
        let space_spacing = (font.char_width('H' as u16) as f32 / 2.0) as i32 as f32;
        let mut inst = Self {
            font,
            scale: 1.0,
            space_spacing,
            inter_char_spacing: 1.0,
            mono_spacing: 0.0,
            scaled_width: [0.0; GLYPH_COUNT],
            scaled_spacing: [0.0; GLYPH_COUNT],
            scaled_height: 0.0,
        };
        inst.build_cached_tables();
        inst
    }

    /// `Font3DInstanceClass::Build_Cached_Tables` — the scaled tables
    /// with the space override and the mono/proportional spacing.
    pub fn build_cached_tables(&mut self) {
        // Rebuild the cached tables
        for a in 0..GLYPH_COUNT {
            let mut width = self.font.char_width(a as u16) as f32;
            if a == b' ' as usize {
                width = self.space_spacing;
            }

            self.scaled_width[a] = self.scale * width;
            if self.mono_spacing != 0.0 {
                self.scaled_spacing[a] = self.scale * self.mono_spacing;
            } else {
                self.scaled_spacing[a] = self.scale * (width + self.inter_char_spacing);
            }
        }
        self.scaled_height = (self.scale * self.font.char_height() as f32).floor();
    }

    /// `Font3DInstanceClass::Set_Mono_Spaced` — every char advances by
    /// the 'W' width plus one.
    pub fn set_mono_spaced(&mut self) {
        self.mono_spacing = (self.font.char_width('W' as u16) as i32 + 1) as f32;
        self.build_cached_tables();
    }

    /// `Font3DInstanceClass::Set_Proportional`.
    pub fn set_proportional(&mut self) {
        self.mono_spacing = 0.0;
        self.build_cached_tables();
    }

    /// `Font3DInstanceClass::Set_Scale`.
    pub fn set_scale(&mut self, scale: f32) {
        self.scale = scale;
        self.build_cached_tables();
    }

    /// `Font3DInstanceClass::Char_Width` (the index masked).
    pub fn char_width(&self, ch: u16) -> f32 {
        self.scaled_width[(ch & 0xff) as usize]
    }

    /// `Font3DInstanceClass::Char_Spacing` (the index masked).
    pub fn char_spacing(&self, ch: u16) -> f32 {
        self.scaled_spacing[(ch & 0xff) as usize]
    }

    /// `Font3DInstanceClass::String_Width` (the char form; a NUL byte
    /// ends the string like the original's C-string walk).
    pub fn string_width(&self, text: &[u8]) -> f32 {
        let mut width = 0.0f32;
        for &b in text {
            if b == 0 {
                break;
            }
            width += self.char_spacing(b as u16);
        }
        width
    }
}

// --- the asset cache ------------------------------------------------------------------------

/// `WW3DAssetManager`'s `Font3DDatas` list — the stricmp cache with
/// the head-prepending insertion. The file bytes come from the caller
/// (a cache miss builds the font; `None` builds the missing-texture
/// placeholder font).
#[derive(Debug, Default)]
pub struct Font3DCache {
    fonts: Vec<Font3DData>,
    parses: usize,
}

impl Font3DCache {
    /// An empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// `WW3DAssetManager::Get_Font3DData`: the case-insensitive walk,
    /// then the create-and-prepend. Returns the font (a clone on a
    /// cache hit — value-equal, no second parse). A crashed build is
    /// returned but never stored: the original's constructor throws
    /// through `Get_Font3DData`, so nothing enters the list and every
    /// call re-attempts (and re-crashes).
    pub fn get(&mut self, name: &[u8], file: Option<&[u8]>) -> Font3DData {
        for font in &self.fonts {
            if font.name.eq_ignore_ascii_case(name) {
                return font.clone();
            }
        }
        let font = parse_font(name, file);
        if matches!(font.verdict, FontVerdict::Crash(_)) {
            return font;
        }
        self.fonts.insert(0, font.clone());
        self.parses += 1;
        font
    }

    /// The number of cached fonts.
    pub fn len(&self) -> usize {
        self.fonts.len()
    }

    /// Whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.fonts.is_empty()
    }

    /// The number of fonts the cache built (the reuse pin: a cache hit
    /// does not parse).
    pub fn parse_count(&self) -> usize {
        self.parses
    }
}
