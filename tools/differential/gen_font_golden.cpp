// SPDX-License-Identifier: GPL-3.0-or-later
//
// Golden-vector driver for `rzh-formats::font` (parity level P0).
//
// Verbatim transcription of the Zero Hour font load path — the Targa
// font-strike loader of Core/Libraries/Source/WWVegas/WW3D2/font3d.cpp
// (Font3DDataClass::Load_Font_Image, Make_Proportional,
// Minimize_Font_Image, Font3DInstanceClass's cached tables and
// String_Width, and the WW3DAssetManager::Get_Font3DData stricmp cache)
// over the surface stack it drives:
//
//   - SurfaceClass (WW3D2/surfaceclass.cpp) — the file constructor
//     through DX8Wrapper::_Create_DX8_Surface, Get_Description, Clear,
//     Is_Transparent_Column, FindBB and the region Copy;
//   - TextureLoader::Load_Surface_Immediate (textureloader.cpp) — the
//     Targa open, the ImageDescriptor ^= TGAIDF_YORIGIN toggle, the
//     Get_WW3D_Format mapping, the 16-bit/paletted/luminance
//     conversion branch and the Copy_Image into the created surface;
//   - BitmapHandlerClass (bitmaphandler.{h,cpp}) — Read_B8G8R8A8,
//     Write_B8G8R8A8, Copy_Pixel and the driven Copy_Image paths;
//   - ww3dformat.{h,cpp} — Get_Bytes_Per_Pixel, Has_Alpha, Alpha_Bits,
//     Get_WW3D_Format and Get_Valid_Texture_Format;
//   - the Targa class (WWLib/TARGA.cpp) — the reader half, verbatim
//     from gen_tga_golden.cpp (the Open footer sniff, the flags-Load
//     with its allocation half, the buffer-Load with the palette read,
//     the image-type switch, the RLE decoder, XFlip/YFlip/InvertImage),
//     plus SetPalette (the caller's-buffer form Load_Surface_Immediate
//     drives).
//
// Deliberate deviations and pinned environment, stated identically on
// both sides (see the Rust module docs for the long forms):
//
// - **The display environment is pinned** to the retail standard: a
//   32-bit desktop with every texture format supported and DXTC
//   available. Under this pin Get_Valid_Texture_Format is the identity
//   except R8G8B8 -> X8R8G8B8, and the 16-bit-desktop branch is dead.
//   The Targa sources therefore land as: 32-bit -> A8R8G8B8,
//   24-bit -> X8R8G8B8, 16-bit -> A8R8G8B8 (through the conversion
//   branch), paletted/mono 8-bit -> A8R8G8B8 (same branch), other
//   8-bit -> A8.
// - **The DDS shadow** (Load_Surface_Immediate tries the munged .dds
//   sibling first when the caps support DXTC) is transcribed down to
//   the name-munging availability probe. Every corpus font ships
//   without a .dds sibling — as the game's font directory does — so
//   the compressed branch never fires; a present sibling would load
//   through the dds module's path and feed the same font walk.
// - **D3DXLoadSurfaceFromSurface** (the Minimize_Font_Image blits go
//   through it whenever the formats differ, which is always under the
//   pin) is modelled after the documented software behaviour, verified
//   against the Wine reference implementation (dlls/d3dx9_36): empty
//   source or destination rects are no-ops, the copy is 1:1 top-left
//   over the minimum of both extents with the destination beyond
//   zero-filled, channels narrow by truncating their high bits, and
//   channels the source lacks are set to their maximum. The
//   same-format fast path of SurfaceClass::Copy (equal formats AND
//   equal dimensions) is dead under the pin (the packed page is
//   A4R4G4B4; no Targa source lands as A4R4G4B4 on a 32-bit desktop).
// - **The surface pitch** is width * bytes-per-pixel (the packed pitch
//   D3D8 system-memory surfaces use); every access is within-row, so
//   no observable depends on it.
// - **The missing-texture placeholder** (a missing file, a failed
//   Targa open or a failed load all return
//   MissingTexture::_Create_Missing_Surface) is the 128x128 A8R8G8B8
//   solid 0x7FFF00FF surface _Init writes — the font tables build from
//   it like from any other surface (observable, pinned). An
//   unsupported pixel depth or a zero dimension makes
//   CreateImageSurface fail, and the original dereferences the null
//   surface in Get_Description — stated as the crash-nullsurface
//   verdict (the tables stay zeroed, the Name never assigned).
// - **The caller's palette buffer** (char palette[256*4] on
//   Load_Surface_Immediate's stack) is zero-filled where the file's
//   colormap does not cover it; the original reads its uninitialised
//   stack there. The corpora keep CMapStart at 0 (as every real file
//   does) and full-length colormaps where the palette matters; the
//   original overflows the buffer for CMapStart != 0 or oversized
//   colormaps (the tga transcription's note applies).
// - **The `_surface` static** of font3d.cpp is kept verbatim as a
//   file-scope static (Minimize_Font_Image publishes its result
//   through it; the strike and grid paths both re-assign it before
//   use, so nothing leaks between fonts).
// - **Reference counting** (RefCountClass, NEW_REF/REF_PTR_RELEASE)
//   and the TextureClass upload are lifetime plumbing; the harness
//   keeps plain ownership and pins the packed surface bytes as the
//   texture's level-0 content.
//
// Output grammar (one command per line, hex lowercase, '-' = empty):
//
//   font <id> <name>          the corpus: the font's asset name
//   file <id> <hex|->         the .tga bytes the name resolves to
//   verdict <id> ok|missing|tgaerr-<n>|crash-null-surface|crash-findbb-lock
//                             missing = the file was absent (the
//                             placeholder); tgaerr-<n> = the Targa open
//                             or load failed with error n (the
//                             placeholder again); the crashes = the
//                             null-surface and FindBB-rect-lock
//                             dereference sites (the tables stay
//                             zeroed, the Name never assigns)
//   sfmt <id> <w> <h> <fmt>   the source surface: A8R8G8B8, X8R8G8B8,
//                             A8 or PLACEHOLDER (128x128 solid)
//   ssum <id> <u64>           the source surface bytes, wrap-31 sum
//   geom <id> <charheight> <vheight-bits> <neww> <newh>
//                             the final geometry: the uchar char
//                             height, the VHeight bits, the packed
//                             page size
//   tab <id> <row> <16*(u v w cw)>
//                             rows 0..15 of the final tables: per char
//                             the UOffset, VOffset and UWidth f32 bits
//                             and the uchar CharWidth
//   ptex <id> <w> <h> <u64>   the packed A4R4G4B4 page, wrap-31 sum
//   ptexr <id> <y> <hex>      the packed page's non-zero rows (up to
//                             twelve, then the marker row 'more')
//   name <id> <uppername>     the strdup+strupr'd Name
//   inst <id> <space-bits> <height-bits> <tabsum>
//                             Font3DInstanceClass at construction: the
//                             SpaceSpacing and ScaledHeight bits and
//                             the wrap-31 sum over the scaled tables
//   iprobe <id> <char> <width-bits> <spacing-bits>
//                             the cached tables at the probe chars
//                             (0x20 0x48 0x57 0x7f 0x80 0x81 0xff)
//   iwidth <id> <text> <bits> String_Width of the probe string
//   imono <id> <mono-bits> <height-bits> <tabsum>
//                             after Set_Mono_Spaced
//   reuse <id> <t|f> <loads>  the second Get_Font3DData (case-flipped
//                             name) hit the cache or not; the number of
//                             Targa loads the harness performed
//
// Usage: gen_font_golden <output-file>
//
// Temporary differential-testing infrastructure: removed at cut-over (M5).
//
// Original: Copyright 2025 Electronic Arts Inc.
// Rust port Copyright 2026 the RustZeroHour contributors.
//
// This program is free software; you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the
// Free Software Foundation, either version 3 of the License, or (at your
// option) any later version.

#include <algorithm>
#include <cctype>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <map>
#include <string>
#include <vector>

namespace fontgen {

// --- the format constants (TARGA.h) ------------------------------------------

#define TGA_NOIMAGE 0
#define TGA_CMAPPED 1
#define TGA_TRUECOLOR 2
#define TGA_MONO 3
#define TGA_CMAPPED_ENCODED 9
#define TGA_TRUECOLOR_ENCODED 10
#define TGA_MONO_ENCODED 11

#define TGAIDF_XORIGIN (1 << 4)
#define TGAIDF_YORIGIN (1 << 5)

#define TGA_READMODE 0

#define TGAERR_OPEN (-1)
#define TGAERR_READ (-2)
#define TGAERR_WRITE (-3)
#define TGAERR_NOTSUPPORTED (-6)

#define TGAF_IMAGE (1 << 0)
#define TGAF_PAL (1 << 1)
#define TGAF_COMPRESS (1 << 2)
#define TGAF_TGA2 (1 << 3)

#define TGA_BytesPerPixel(a) ((a + 7) >> 3)

#define TGA2_SIGNATURE "TRUEVISION-XFILE"

#pragma pack(push, 1)

typedef struct _TGAHeader
{
        char IDLength;
        char ColorMapType;
        char ImageType;
        short CMapStart;
        short CMapLength;
        char CMapDepth;
        short XOffset;
        short YOffset;
        short Width;
        short Height;
        char PixelDepth;
        char ImageDescriptor;
} TGAHeader;

#pragma pack(pop)

static_assert(sizeof(TGAHeader) == 18, "header layout drift");

// --- the file plumbing (stands in for FileClass) ------------------------------
//
// Read side: a const byte buffer with lseek-shaped semantics — negative
// targets fail (-1), positions past EOF are allowed and read as nothing.

struct SrcFile
{
        const uint8_t *data = nullptr;
        size_t len = 0;
        long pos = 0;

        int seek(long offset, int origin)
        {
                long target;
                if (origin == 0)
                        target = offset; // SEEK_SET
                else if (origin == 1)
                        target = pos + offset; // SEEK_CUR
                else if (origin == 2)
                        target = (long)len + offset; // SEEK_END
                else
                        return -1;
                if (target < 0)
                        return -1;
                pos = target;
                return (int)pos;
        }

        int read(uint8_t *dst, int size)
        {
                if (pos < 0 || pos >= (long)len)
                        return 0;
                size_t avail = len - (size_t)pos;
                size_t n = std::min((size_t)size, avail);
                memcpy(dst, data + pos, n);
                pos += (long)n;
                return (int)n;
        }
};

// --- the Targa class transcription (TARGA.cpp, reader half) -------------------
//
// Verbatim from gen_tga_golden.cpp (the footer-sniff Open, the flags-Load
// with its allocation half, the buffer-Load with the palette read, the
// image-type switch, the RLE decoder and the flips), plus SetPalette /
// GetPalette / GetImage — the caller's-buffer form Load_Surface_Immediate
// drives (the original keeps the footer in a local in Open and discards
// it; mFooterInfo/mFooterExt/mFooterDev are view-only and unused here).

struct Targa
{
        TGAHeader Header{};
        long mAccess = TGA_READMODE;
        long mFlags = 0;
        uint8_t *mImage = nullptr;
        uint8_t *mPalette = nullptr;

        SrcFile src; // read side (TGA_READMODE)

        ~Targa()
        {
                // Targa::~Targa: only the buffers we allocated ourselves.
                if ((mPalette != nullptr) && (mFlags & TGAF_PAL))
                        delete[] mPalette;
                if ((mImage != nullptr) && (mFlags & TGAF_IMAGE))
                        delete[] mImage;
        }

        bool Is_File_Open() const { return src.data != nullptr; }

        void Clear_File()
        {
                src.data = nullptr;
                src.len = 0;
                src.pos = 0;
        }

        // Open, TGA_READMODE path only (TARGA.cpp Open, case TGA_READMODE).
        // The footer is read (and its signature checked) exactly like the
        // original, but the TGA2 extension bytes are kept out — no corpus
        // carries them and the original discards the footer view anyway.
        long Open_Read(const uint8_t *data, size_t len)
        {
                long error = 0;
                int size;
                unsigned char footer[26];

                /* File already open? */
                if (Is_File_Open() && (mAccess == TGA_READMODE))
                {
                        return (0);
                }

                Clear_File();

                /* Initialize the access mode. */
                mAccess = TGA_READMODE;
                mFlags &= ~TGAF_TGA2;

                src.data = data;
                src.len = len;
                src.pos = 0;

                /* Check for 2.0 targa file by loading the footer */
                if (src.seek(-26, 2 /* SEEK_END */) == -1)
                {
                        error = TGAERR_READ;
                }

                if (!error)
                {
                        if (src.read(footer, sizeof(footer)) != (int)sizeof(footer))
                        {
                                error = TGAERR_READ;
                        }
                        else
                        {
                                /* If this a 2.0 file with an extension? */
                                if (strncmp((const char *)footer + 8, TGA2_SIGNATURE, 16) == 0)
                                {
                                        int32_t extension;
                                        memcpy(&extension, footer, 4);
                                        if (extension != 0)
                                        {
                                                mFlags |= TGAF_TGA2;
                                        }
                                }
                        }
                }

                /* Read in header. */
                if (!error && (src.seek(0, 0 /* SEEK_SET */) == -1))
                {
                        error = TGAERR_READ;
                }
                else
                {
                        size = src.read((uint8_t *)&Header, sizeof(TGAHeader));
                        if (size != sizeof(TGAHeader))
                        {
                                error = TGAERR_READ;
                        }
                }

                /* Skip the ID field */
                if (!error && (Header.IDLength != 0))
                {
                        if (src.seek(Header.IDLength, 1 /* SEEK_CUR */) == -1)
                        {
                                error = TGAERR_READ;
                        }
                }

                /* Close on any error! */
                if (error)
                {
                        Close();
                }

                return (error);
        }

        void Close() { Clear_File(); }

        // Targa::SetPalette, verbatim (the caller's-buffer form).
        void SetPalette(uint8_t *buffer)
        {
                /* Free any palette buffer before assigning another. */
                if ((mPalette != nullptr) && (mFlags & TGAF_PAL))
                {
                        delete[] mPalette;
                        mPalette = nullptr;
                        mFlags &= ~TGAF_PAL;
                }

                /* Assign the new buffer. */
                mPalette = buffer;
        }

        uint8_t *GetImage() { return mImage; }
        uint8_t *GetPalette() { return mPalette; }

        // Targa::Load (buffer form) — the palette/image reading half. The
        // original re-enters Open by name here; the already-open check makes
        // that a no-op, transcribed as the same call.
        long Load_Buffers(uint8_t *palette, uint8_t *image, bool invert_image)
        {
                long size;
                long depth;
                long error = 0;

                /* Open the Targa (short-circuits: already open in read mode). */
                if (Open_Read(src.data, src.len) == 0)
                {
                        /* Process ColorMap (palette) */
                        if (Header.ColorMapType == 1)
                        {
                                depth = (Header.CMapDepth >> 3);
                                size = (Header.CMapLength * depth);

                                if ((palette != nullptr) && (Header.CMapLength > 0))
                                {
                                        /* Adjust palette to the starting color entry. */
                                        palette += (Header.CMapStart * depth);

                                        if (src.read(palette, size) != size)
                                        {
                                                error = TGAERR_READ;
                                        }
                                }
                                else
                                {
                                        if (src.seek(size, 1 /* SEEK_CUR */) == -1)
                                        {
                                                error = TGAERR_READ;
                                        }
                                }
                        }

                        if (!error && (image != nullptr))
                        {
                                depth = TGA_BytesPerPixel(Header.PixelDepth);
                                size = ((Header.Width * Header.Height) * depth);

                                switch (Header.ImageType)
                                {
                                case TGA_CMAPPED:
                                        if (src.read(image, size) != size)
                                        {
                                                error = TGAERR_READ;
                                        }
                                        break;

                                case TGA_TRUECOLOR:
                                        if (src.read(image, size) == size)
                                        {
                                                if (invert_image)
                                                        InvertImage();
                                        }
                                        else
                                        {
                                                error = TGAERR_READ;
                                        }
                                        break;

                                case TGA_MONO:
                                        if (src.read(image, size) != size)
                                        {
                                                error = TGAERR_READ;
                                        }
                                        break;

                                case TGA_CMAPPED_ENCODED:
                                        error = DecodeImage();
                                        break;

                                case TGA_TRUECOLOR_ENCODED:
                                        if ((error = DecodeImage()) == 0)
                                        {
                                                if (invert_image)
                                                        InvertImage();
                                        }
                                        break;

                                default:
                                        error = TGAERR_NOTSUPPORTED;
                                        break;
                                }

                                if (!error)
                                {
                                        if (Header.ImageDescriptor & TGAIDF_XORIGIN)
                                        {
                                                XFlip();
                                                Header.ImageDescriptor &= ~TGAIDF_XORIGIN;
                                        }

                                        // Mod (IML) : Locate the origin at the bottom-left
                                        // corner instead. Bug fix (IML) : Clear this flag to
                                        // indicate to the targa reader that the Y-origin is at
                                        // the bottom of the image.
                                        if (Header.ImageDescriptor & TGAIDF_YORIGIN)
                                        {
                                                YFlip();
                                                Header.ImageDescriptor &= ~TGAIDF_YORIGIN;
                                        }
                                }
                        }

                        Close();
                }
                else
                {
                        error = -1; // TGAERR_OPEN
                }

                return (error);
        }

        // Targa::Load (flags form) — the allocation half.
        long Load_Flags(long flags, bool invert_image)
        {
                long size;
                long error = 0;

                if (Open_Read(src.data, src.len) == 0)
                {
                        /* Allocate palette memory if requested to and the targa has one. */
                        if ((flags & TGAF_PAL) && (Header.ColorMapType == 1))
                        {
                                if ((mPalette != nullptr) && (mFlags & TGAF_PAL))
                                {
                                        delete[] mPalette;
                                        mPalette = nullptr;
                                        mFlags &= ~TGAF_PAL;
                                }

                                if ((mPalette == nullptr) && !(mFlags & TGAF_PAL))
                                {
                                        size = (Header.CMapLength * (Header.CMapDepth >> 3));
                                        if (size != 0)
                                        {
                                                mPalette = new uint8_t[size];
                                                mFlags |= TGAF_PAL;
                                        }
                                }
                        }

                        /* Allocate image memory if requested to. */
                        if (!error && (flags & TGAF_IMAGE))
                        {
                                if ((mImage != nullptr) && (mFlags & TGAF_IMAGE))
                                {
                                        delete[] mImage;
                                        mImage = nullptr;
                                        mFlags &= ~TGAF_IMAGE;
                                }

                                if ((mImage == nullptr) && !(mFlags & TGAF_IMAGE))
                                {
                                        size = ((Header.Width * Header.Height)
                                                * TGA_BytesPerPixel(Header.PixelDepth));
                                        if (size != 0)
                                        {
                                                mImage = new uint8_t[size];
                                                mFlags |= TGAF_IMAGE;
                                        }
                                }
                        }

                        if (!error)
                        {
                                error = Load_Buffers(mPalette, mImage, invert_image);
                        }
                }
                else
                {
                        error = -1; // TGAERR_OPEN
                }

                return (error);
        }

        // Targa::DecodeImage, verbatim (the reads go against the byte source).
        long DecodeImage()
        {
                uint8_t *image;
                unsigned char count;
                unsigned char depth;
                unsigned long pixel_count;
                unsigned long size;
                unsigned long c, i;
                long error = 0;

                image = mImage;
                depth = TGA_BytesPerPixel(Header.PixelDepth);
                pixel_count = (unsigned long)(Header.Width * Header.Height);

                while ((pixel_count > 0) && !error)
                {
                        if (src.read(&count, 1) == 1)
                        {
                                if (count & 0x80)
                                {
                                        count &= 0x7F;
                                        count++;

                                        if (src.read(image, depth) == depth)
                                        {
                                                uint8_t *color = image;
                                                image += depth;
                                                for (c = 1; c < count; c++)
                                                        for (i = 0; i < depth; i++)
                                                                *image++ = *(color + i);
                                        }
                                        else
                                        {
                                                error = TGAERR_READ;
                                        }
                                }
                                else
                                {
                                        count++;
                                        size = (count * depth);

                                        if ((unsigned)src.read(image, size) == size)
                                                image += size;
                                        else
                                                error = TGAERR_READ;
                                }

                                pixel_count -= count;
                        }
                        else
                        {
                                error = TGAERR_READ;
                        }
                }

                return (error);
        }

        // Targa::XFlip, verbatim.
        void XFlip()
        {
                uint8_t *ptr, *ptr1;
                long x, y, d;
                uint8_t v, v1;
                char depth;

                depth = TGA_BytesPerPixel(Header.PixelDepth);

                for (y = 0; y < Header.Height; y++)
                {
                        ptr = (mImage + ((Header.Width * depth) * y));
                        ptr1 = (ptr + ((Header.Width * depth) - depth));

                        for (x = 0; x < (Header.Width / 2); x++)
                        {
                                for (d = 0; d < depth; d++)
                                {
                                        v = *(ptr + d);
                                        v1 = *(ptr1 + d);
                                        *(ptr + d) = v1;
                                        *(ptr1 + d) = v;
                                }

                                ptr += depth;
                                ptr1 -= depth;
                        }
                }
        }

        // Targa::YFlip (the _swapBytes form), verbatim.
        void YFlip()
        {
                unsigned stride = Header.Width * TGA_BytesPerPixel(Header.PixelDepth);
                uint8_t *ptrTop = mImage, *ptrBottom = mImage + stride * (Header.Height - 1);
                for (unsigned y = Header.Height / 2; y; --y, ptrTop += stride, ptrBottom -= stride)
                        for (unsigned i = 0; i < stride; i++)
                        {
                                uint8_t t = ptrTop[i];
                                ptrTop[i] = ptrBottom[i];
                                ptrBottom[i] = t;
                        }
        }

        // Targa::InvertImage, verbatim (a within-pixel byte swap; the font
        // path always passes invert_image = false, so it never runs — kept
        // because the load's image-type switch routes through it).
        void InvertImage()
        {
                uint8_t *buffer;
                long depth;
                long pixel_count;
                long i;
                uint8_t c;

                buffer = mImage;
                depth = TGA_BytesPerPixel(Header.PixelDepth);
                pixel_count = ((long)Header.Width * Header.Height);

                if (depth > 2)
                {
                        while (pixel_count > 0)
                        {
                                for (i = 0; i < (depth / 2); i++)
                                {
                                        c = *(buffer + i);
                                        *(buffer + i) = *(buffer + ((depth - 1) - i));
                                        *(buffer + ((depth - 1) - i)) = c;
                                }

                                pixel_count--;
                                buffer += depth;
                        }
                }
        }
};

// --- WW3DFormat (ww3dformat.h) and the format helpers -------------------------

enum WW3DFormat
{
        WW3D_FORMAT_UNKNOWN = 0,
        WW3D_FORMAT_R8G8B8,
        WW3D_FORMAT_A8R8G8B8,
        WW3D_FORMAT_X8R8G8B8,
        WW3D_FORMAT_R5G6B5,
        WW3D_FORMAT_X1R5G5B5,
        WW3D_FORMAT_A1R5G5B5,
        WW3D_FORMAT_A4R4G4B4,
        WW3D_FORMAT_R3G3B2,
        WW3D_FORMAT_A8,
        WW3D_FORMAT_A8R3G3B2,
        WW3D_FORMAT_X4R4G4B4,
        WW3D_FORMAT_A8P8,
        WW3D_FORMAT_P8,
        WW3D_FORMAT_L8,
        WW3D_FORMAT_A8L8,
        WW3D_FORMAT_A4L4,
};

// ww3dformat.cpp — Get_Bytes_Per_Pixel, verbatim (the formats the font
// path can produce; the assert is the release no-op the dds transcription
// uses).
static unsigned Get_Bytes_Per_Pixel(WW3DFormat format)
{
        switch (format)
        {
        case WW3D_FORMAT_A8R8G8B8:
        case WW3D_FORMAT_X8R8G8B8:
                return 4;
        case WW3D_FORMAT_R8G8B8:
                return 3;
        case WW3D_FORMAT_R5G6B5:
        case WW3D_FORMAT_X1R5G5B5:
        case WW3D_FORMAT_A1R5G5B5:
        case WW3D_FORMAT_A4R4G4B4:
        case WW3D_FORMAT_A8R3G3B2:
        case WW3D_FORMAT_X4R4G4B4:
        case WW3D_FORMAT_A8P8:
        case WW3D_FORMAT_A8L8:
                return 2;
        case WW3D_FORMAT_R3G3B2:
        case WW3D_FORMAT_A8:
        case WW3D_FORMAT_P8:
        case WW3D_FORMAT_L8:
        case WW3D_FORMAT_A4L4:
                return 1;

        default:
                return 0;
        }
}

// ww3dformat.h — Has_Alpha, verbatim.
static bool Has_Alpha(WW3DFormat format)
{
        switch (format)
        {
        case WW3D_FORMAT_A8R8G8B8:
        case WW3D_FORMAT_A1R5G5B5:
        case WW3D_FORMAT_A4R4G4B4:
        case WW3D_FORMAT_A8:
        case WW3D_FORMAT_A8R3G3B2:
        case WW3D_FORMAT_A8P8:
        case WW3D_FORMAT_A8L8:
        case WW3D_FORMAT_A4L4:
                return true;
                break;
        default:
                return false;
                break;
        };
}

// ww3dformat.h — Alpha_Bits, verbatim.
static int Alpha_Bits(WW3DFormat format)
{
        switch (format)
        {
        case WW3D_FORMAT_A8R8G8B8:
        case WW3D_FORMAT_A8:
        case WW3D_FORMAT_A8R3G3B2:
        case WW3D_FORMAT_A8P8:
        case WW3D_FORMAT_A8L8:
                return 8;
                break;
        case WW3D_FORMAT_A4R4G4B4:
        case WW3D_FORMAT_A4L4:
                return 4;
                break;
        case WW3D_FORMAT_A1R5G5B5:
                return 1;
                break;
        default:
                return 0;
                break;
        };
}

// ww3dformat.cpp — Get_Valid_Texture_Format under the pinned environment
// (a 32-bit desktop, every format supported): the identity except the
// R8G8B8 -> X8R8G8B8 rule. The 16-bit-desktop branch, the DXTC remaps
// and the caps fallbacks are dead under the pin (see the file comment).
static WW3DFormat Get_Valid_Texture_Format(WW3DFormat format)
{
        if (format == WW3D_FORMAT_R8G8B8)
        {
                format = WW3D_FORMAT_X8R8G8B8;
        }
        return format;
}

// ww3dformat.cpp — Get_WW3D_Format (both forms), verbatim: the Targa
// header's pixel depth picks the source format, the destination is the
// source with the paletted/luminance formats remapped, then validated.
static void Get_WW3D_Format(WW3DFormat &src_format, unsigned &src_bpp, const Targa &targa)
{
        /* Guess the format from the Targa Header bits: */
        src_format = WW3D_FORMAT_UNKNOWN;
        src_bpp = 0;
        switch (targa.Header.PixelDepth)
        {
                case 32:
                        src_format = WW3D_FORMAT_A8R8G8B8;
                        src_bpp = 4;
                        break;
                case 24:
                        src_format = WW3D_FORMAT_R8G8B8;
                        src_bpp = 3;
                        break;
                case 16:
                        src_format = WW3D_FORMAT_A1R5G5B5;
                        src_bpp = 2;
                        break;
                case 8:
                        src_bpp = 1;
                        if (targa.Header.ColorMapType == 1)
                                src_format = WW3D_FORMAT_P8;
                        else if (targa.Header.ImageType == TGA_MONO)
                                src_format = WW3D_FORMAT_L8;
                        else
                                src_format = WW3D_FORMAT_A8;
                        break;
                default:
                        break;
        }
}

static void Get_WW3D_Format(WW3DFormat &dest_format, WW3DFormat &src_format, unsigned &src_bpp,
                            const Targa &targa)
{
        Get_WW3D_Format(src_format, src_bpp, targa);
        dest_format = src_format;
        if ((dest_format == WW3D_FORMAT_P8) || (dest_format == WW3D_FORMAT_L8))
        {
                dest_format = WW3D_FORMAT_X8R8G8B8;
        }
        dest_format = Get_Valid_Texture_Format(dest_format);
}

// --- BitmapHandlerClass (bitmaphandler.h) — the pixel converters ---------------

static void Bitmap_Assert(bool condition)
{
        (void)condition; // release build: no-op
}

// bitmaphandler.h — Read_B8G8R8A8 (byte form), verbatim: the source pixel
// converted to 32-bit BGRA byte order.
static void Read_B8G8R8A8(
        unsigned char *argb,
        const unsigned char *src_ptr,
        WW3DFormat src_format,
        const unsigned char *palette,
        unsigned palette_bpp)
{
        switch (src_format)
        {
        case WW3D_FORMAT_A8R8G8B8:
        case WW3D_FORMAT_X8R8G8B8:
                *(unsigned *)argb = *(const unsigned *)src_ptr;
                break;
        case WW3D_FORMAT_R8G8B8:
                *argb++ = src_ptr[0];
                *argb++ = src_ptr[1];
                *argb++ = src_ptr[2];
                *argb++ = 0xff;
                break;
        case WW3D_FORMAT_A4R4G4B4:
        {
                unsigned short tmp;
                tmp = *(const unsigned short *)src_ptr;
                *argb++ = ((tmp & 0x000f) << 4);
                *argb++ = ((tmp & 0x00f0));
                *argb++ = ((tmp & 0x0f00) >> 4);
                *argb++ = ((tmp & 0xf000) >> 8);
        }
        break;
        case WW3D_FORMAT_A1R5G5B5:
        {
                unsigned short tmp;
                tmp = *(const unsigned short *)src_ptr;
                argb[3] = tmp & 0x8000 ? 0xff : 0x0;
                argb[2] = (tmp >> 7) & 0xf8;
                argb[1] = (tmp >> 2) & 0xf8;
                argb[0] = (tmp << 3) & 0xf8;
        }
        break;
        case WW3D_FORMAT_R5G6B5:
        {
                unsigned short tmp;
                tmp = *(const unsigned short *)src_ptr;
                argb[3] = 0xff;
                argb[2] = (tmp >> 8) & 0xf8;
                argb[1] = (tmp >> 3) & 0xfc;
                argb[0] = (tmp << 3) & 0xf8;
        }
        break;
        case WW3D_FORMAT_R3G3B2:
        {
                unsigned char tmp = *src_ptr;
                argb[3] = 0xff;
                argb[2] = tmp & 0xe0;
                argb[1] = (tmp << 3) & 0xe0;
                argb[0] = (tmp << 6) & 0xc0;
        }
        break;
        case WW3D_FORMAT_L8:
        {
                unsigned char tmp = *src_ptr++;
                *argb++ = tmp;
                *argb++ = tmp;
                *argb++ = tmp;
                *argb++ = 0xff;
        }
        break;
        case WW3D_FORMAT_A8:
        {
                *argb++ = 0;
                *argb++ = 0;
                *argb++ = 0;
                *argb++ = *src_ptr++;
        }
        break;
        case WW3D_FORMAT_P8:
        {
                unsigned char index = *src_ptr++;
                switch (palette_bpp)
                {
                case 4:
                        *argb++ = palette[palette_bpp * index + 3];
                        *argb++ = palette[palette_bpp * index + 2];
                        *argb++ = palette[palette_bpp * index + 1];
                        *argb++ = palette[palette_bpp * index + 0];
                        break;
                case 3:
                        *argb++ = palette[palette_bpp * index + 2];
                        *argb++ = palette[palette_bpp * index + 1];
                        *argb++ = palette[palette_bpp * index + 0];
                        *argb++ = 0xff;
                        break;
                case 2:
                case 1:
                default:
                        Bitmap_Assert(0);
                        break;
                }
        }
        break;
        default:
                Bitmap_Assert(0);
                break;
        }
}

// bitmaphandler.h — Write_B8G8R8A8 (byte form), verbatim (the same
// transcription gen_dds_golden.cpp carries; the R3G3B2 stray-byte quirk
// included, though no font destination reaches it).
static void Write_B8G8R8A8(
        unsigned char *dest_ptr,
        WW3DFormat dest_format,
        const unsigned char *argb)
{
        switch (dest_format)
        {
        case WW3D_FORMAT_A8R8G8B8:
        case WW3D_FORMAT_X8R8G8B8:
                *(unsigned *)dest_ptr = *(const unsigned *)argb;
                break;
        case WW3D_FORMAT_R8G8B8:
                *dest_ptr++ = *argb++;
                *dest_ptr++ = *argb++;
                *dest_ptr++ = *argb++;
                break;
        case WW3D_FORMAT_A4R4G4B4:
        {
                unsigned short tmp;
                tmp = ((argb[3]) & 0xf0) << 8;
                tmp |= ((argb[2]) & 0xf0) << 4;
                tmp |= ((argb[1]) & 0xf0);
                tmp |= ((argb[0]) & 0xf0) >> 4;
                *(unsigned short *)dest_ptr = tmp;
        }
        break;
        case WW3D_FORMAT_A1R5G5B5:
        {
                unsigned short tmp;
                tmp = argb[3] ? 0x8000 : 0x0;
                tmp |= ((argb[2]) & 0xf8) << 7;
                tmp |= ((argb[1]) & 0xf8) << 2;
                tmp |= ((argb[0]) & 0xf8) >> 3;
                *(unsigned short *)dest_ptr = tmp;
        }
        break;
        case WW3D_FORMAT_R5G6B5:
        {
                unsigned short tmp;
                tmp = ((argb[2]) & 0xf8) << 8;
                tmp |= ((argb[1]) & 0xfc) << 3;
                tmp |= ((argb[0]) & 0xf8) >> 3;
                *(unsigned short *)dest_ptr = tmp;
        }
        break;
        case WW3D_FORMAT_R3G3B2:
        {
                unsigned char tmp;
                tmp = ((argb[2]) & 0xe0);
                tmp |= ((argb[1]) & 0xe0) >> 3;
                tmp |= ((argb[0]) & 0xc0) >> 6;
                // The original stores the 8-bit value through a 16-bit
                // pointer — one byte past the pixel. Both compilers
                // zero-extend the byte into the register, so the stray
                // byte is 0x00 (see gen_dds_golden.cpp's file comment).
                *(unsigned short *)dest_ptr = tmp;
        }
        break;
        case WW3D_FORMAT_L8:
        {
                // CIE Req. 709: Y709 = 0.2125R + 0.7154G + 0.0721B
                unsigned char tmp = (unsigned char)(
                    (((unsigned int)argb[0] * (unsigned int)0x1275) + // 0.0721B
                     ((unsigned int)argb[1] * (unsigned int)0xB725) + // 0.7154G
                     ((unsigned int)argb[2] * (unsigned int)0x3666))  // 0.2125R
                    >> 16);
                *dest_ptr++ = tmp;
        }
        break;
        case WW3D_FORMAT_A8:
        {
                *dest_ptr++ = *argb++;
        }
        break;
        default:
                Bitmap_Assert(0);
                break;
        }
}

// bitmaphandler.h — Copy_Pixel (no HSV form), verbatim.
static void Copy_Pixel(
        unsigned char *dest_ptr,
        WW3DFormat dest_format,
        const unsigned char *src_ptr,
        WW3DFormat src_format,
        const unsigned char *palette,
        unsigned palette_bpp)
{
        /* Color space conversion needed? */
        if (dest_format == src_format)
        {
                switch (dest_format)
                {
                case WW3D_FORMAT_A8R8G8B8:
                case WW3D_FORMAT_X8R8G8B8:
                        *(unsigned *)dest_ptr = *(const unsigned *)src_ptr;
                        break;
                case WW3D_FORMAT_R8G8B8:
                        *dest_ptr++ = src_ptr[0];
                        *dest_ptr++ = src_ptr[1];
                        *dest_ptr++ = src_ptr[2];
                        break;
                case WW3D_FORMAT_A4R4G4B4:
                {
                        unsigned short tmp = *(const unsigned short *)src_ptr;
                        *(unsigned short *)dest_ptr =
                            ((tmp & 0x000f) << 12) | ((tmp & 0x00f0) << 4) | ((tmp & 0x0f00) >> 4)
                            | ((tmp & 0xf000) >> 12);
                }
                break;
                case WW3D_FORMAT_A1R5G5B5:
                {
                        unsigned short tmp = *(const unsigned short *)src_ptr;
                        *(unsigned short *)dest_ptr =
                            ((tmp & 0x001f) << 11) | ((tmp & 0x03e0) << 1) | ((tmp & 0x7c00) >> 9)
                            | ((tmp & 0x8000) >> 15);
                }
                break;
                case WW3D_FORMAT_R5G6B5:
                {
                        unsigned short tmp = *(const unsigned short *)src_ptr;
                        *(unsigned short *)dest_ptr =
                            ((tmp & 0x001f) << 11) | (tmp & 0x07e0) | ((tmp & 0xf800) >> 11);
                }
                break;
                case WW3D_FORMAT_R3G3B2:
                case WW3D_FORMAT_L8:
                case WW3D_FORMAT_A8:
                        *dest_ptr++ = *src_ptr++;
                        break;
                default:
                        Bitmap_Assert(0);
                        break;
                }
        }
        else
        {
                unsigned b8g8r8a8;
                Read_B8G8R8A8((unsigned char *)&b8g8r8a8, src_ptr, src_format, palette, palette_bpp);
                Write_B8G8R8A8(dest_ptr, dest_format, (const unsigned char *)&b8g8r8a8);
        }
}

// bitmaphandler.cpp — Copy_Image, the two paths the font loader drives
// (no mip generation, no HSV shift, no bumpmap destination — those
// branches are dead here): the same-size per-pixel walk and the
// point-sampled scaling walk.
static void Copy_Image(
        unsigned char *dest_surface,
        unsigned dest_surface_width,
        unsigned dest_surface_height,
        unsigned dest_surface_pitch,
        WW3DFormat dest_surface_format,
        unsigned char *src_surface,
        unsigned src_surface_width,
        unsigned src_surface_height,
        unsigned src_surface_pitch,
        WW3DFormat src_surface_format,
        const unsigned char *src_palette,
        unsigned src_palette_bpp)
{
        /* One-to-one copy or scaling? */
        if (dest_surface_width == src_surface_width && dest_surface_height == src_surface_height)
        {
                for (unsigned y = 0; y < dest_surface_height; ++y)
                {
                        unsigned char *dest_ptr = dest_surface + y * dest_surface_pitch;
                        const unsigned char *src_ptr = src_surface + y * src_surface_pitch;
                        for (unsigned x = 0; x < dest_surface_width; ++x, dest_ptr += Get_Bytes_Per_Pixel(dest_surface_format), src_ptr += Get_Bytes_Per_Pixel(src_surface_format))
                        {
                                Copy_Pixel(dest_ptr, dest_surface_format, src_ptr, src_surface_format,
                                           src_palette, src_palette_bpp);
                        }
                }
        }
        else
        {
                /* For now do only point-sampling */
                unsigned dest_bpp = Get_Bytes_Per_Pixel(dest_surface_format);
                unsigned src_bpp = Get_Bytes_Per_Pixel(src_surface_format);
                for (unsigned y = 0; y < dest_surface_height; ++y)
                {
                        unsigned char *dest_ptr = dest_surface + y * dest_surface_pitch;
                        unsigned src_y = y * src_surface_height / dest_surface_height;
                        const unsigned char *src_ptr = src_surface + src_y * src_surface_pitch;
                        for (unsigned x = 0; x < dest_surface_width; ++x, dest_ptr += dest_bpp)
                        {
                                unsigned src_x = x * src_surface_width / dest_surface_width;
                                src_x *= src_bpp;
                                Copy_Pixel(dest_ptr, dest_surface_format, src_ptr + src_x,
                                           src_surface_format, src_palette, src_palette_bpp);
                        }
                }
        }
}

// --- the D3DXLoadSurfaceFromSurface model -------------------------------------
//
// SurfaceClass::Copy's second branch calls
// D3DXLoadSurfaceFromSurface(..., D3DX_FILTER_NONE, 0) for every format-
// or size-mismatched region blit — the only branch the font path can
// reach under the pinned environment (the packed page is A4R4G4B4 and
// no Targa source lands as A4R4G4B4). The model follows the software
// path the reference implementation (Wine's d3dx9_36) documents:
//
//   - an empty source or destination rect is a successful no-op;
//   - an out-of-bounds rect would be refused (never reachable here:
//     SurfaceClass::Copy pre-clamps both rects to their surfaces);
//   - the copy is 1:1 top-left over the minimum of both extents; the
//     destination rows and columns beyond the source extent are
//     zero-filled;
//   - channels narrow by truncating their high bits (a8 -> a4 is >>4)
//     and widen by replicating the high bits; channels the source
//     format lacks are set to their maximum (X8R8G8B8 -> A4R4G4B4
//     gives alpha 0xf, A8 -> A4R4G4B4 gives rgb 0xf).

struct D3DXRect
{
        long left, top, right, bottom;
};

// The per-channel layout of the formats the font path converts between:
// bits[0..3] = the channel's bit count at shift[0..3] (BGRA order, the
// byte addressable order Read/Write_B8G8R8A8 use — the model converts
// through 32-bit BGRA exactly like the reference's argb conversion).
struct ChannelLayout
{
        unsigned bits[4];
        unsigned shift[4];
};

static void format_channels(WW3DFormat format, ChannelLayout &out)
{
        for (int i = 0; i < 4; i++)
        {
                out.bits[i] = 0;
                out.shift[i] = 0;
        }
        switch (format)
        {
        case WW3D_FORMAT_A8R8G8B8:
                out.bits[3] = 8; out.shift[3] = 24;
                out.bits[2] = 8; out.shift[2] = 16;
                out.bits[1] = 8; out.shift[1] = 8;
                out.bits[0] = 8; out.shift[0] = 0;
                break;
        case WW3D_FORMAT_X8R8G8B8:
                out.bits[2] = 8; out.shift[2] = 16;
                out.bits[1] = 8; out.shift[1] = 8;
                out.bits[0] = 8; out.shift[0] = 0;
                break;
        case WW3D_FORMAT_A4R4G4B4:
                out.bits[3] = 4; out.shift[3] = 12;
                out.bits[2] = 4; out.shift[2] = 8;
                out.bits[1] = 4; out.shift[1] = 4;
                out.bits[0] = 4; out.shift[0] = 0;
                break;
        case WW3D_FORMAT_A1R5G5B5:
                out.bits[3] = 1; out.shift[3] = 15;
                out.bits[2] = 5; out.shift[2] = 10;
                out.bits[1] = 5; out.shift[1] = 5;
                out.bits[0] = 5; out.shift[0] = 0;
                break;
        case WW3D_FORMAT_R5G6B5:
                out.bits[2] = 5; out.shift[2] = 11;
                out.bits[1] = 6; out.shift[1] = 5;
                out.bits[0] = 5; out.shift[0] = 0;
                break;
        case WW3D_FORMAT_A8:
                out.bits[3] = 8; out.shift[3] = 0;
                break;
        default:
                break;
        }
}

// The reference's get_relevant_argb_components + make_argb_color pair:
// read each channel the destination also has (dropping the low bits the
// destination cannot hold), then re-emit it into the destination (with
// bit replication when widening); channels the source lacks are set to
// their maximum.
static unsigned d3dx_convert_pixel(unsigned src, const ChannelLayout &sf, const ChannelLayout &df)
{
        unsigned val = 0;
        for (int i = 0; i < 4; i++)
        {
                if (df.bits[i] == 0)
                        continue;
                if (sf.bits[i] == 0)
                {
                        /* new channels are set to their maximal value */
                        val |= ((1u << df.bits[i]) - 1u) << df.shift[i];
                        continue;
                }

                /* srcshift extracts the relevant (high) bits of the source */
                unsigned srcshift = sf.shift[i] + (sf.bits[i] > df.bits[i] ? sf.bits[i] - df.bits[i] : 0);
                unsigned in = (src >> srcshift) & ((1u << df.bits[i]) - 1u);

                /* widening replicates the high bits downward */
                unsigned destshift = df.shift[i] + (df.bits[i] > sf.bits[i] ? df.bits[i] - sf.bits[i] : 0);
                unsigned v = 0;
                unsigned shift = destshift;
                while (shift > df.shift[i])
                {
                        v |= in << shift;
                        shift -= sf.bits[i];
                }
                v |= (in >> (df.shift[i] - shift)) << df.shift[i];
                val |= v;
        }
        return val;
}

static void d3dx_load_surface_from_surface(
        uint8_t *dst, unsigned dst_pitch, WW3DFormat dst_format, const D3DXRect &dst_rect,
        const uint8_t *src, unsigned src_pitch, WW3DFormat src_format, const D3DXRect &src_rect)
{
        /* Empty rects are successful no-ops. */
        if (src_rect.left >= src_rect.right || src_rect.top >= src_rect.bottom)
                return;
        if (dst_rect.left >= dst_rect.right || dst_rect.top >= dst_rect.bottom)
                return;

        /* The reference's convert_argb_pixels: 1:1 top-left over the
         * minimum of both extents, the destination beyond zero-filled. */
        unsigned sw = (unsigned)(src_rect.right - src_rect.left);
        unsigned sh = (unsigned)(src_rect.bottom - src_rect.top);
        unsigned dw = (unsigned)(dst_rect.right - dst_rect.left);
        unsigned dh = (unsigned)(dst_rect.bottom - dst_rect.top);
        unsigned min_w = std::min(sw, dw);
        unsigned min_h = std::min(sh, dh);

        unsigned dbpp = Get_Bytes_Per_Pixel(dst_format);
        unsigned sbpp = Get_Bytes_Per_Pixel(src_format);

        ChannelLayout sf, df;
        format_channels(src_format, sf);
        format_channels(dst_format, df);

        bool same = (src_format == dst_format);

        for (unsigned y = 0; y < min_h; y++)
        {
                uint8_t *drow = dst + (dst_rect.top + (long)y) * (long)dst_pitch
                                + dst_rect.left * (long)dbpp;
                const uint8_t *srow = src + (src_rect.top + (long)y) * (long)src_pitch
                                      + src_rect.left * (long)sbpp;
                for (unsigned x = 0; x < min_w; x++)
                {
                        if (same)
                        {
                                memcpy(drow + x * dbpp, srow + x * sbpp, dbpp);
                        }
                        else
                        {
                                unsigned s;
                                memcpy(&s, srow + x * sbpp, sbpp);
                                unsigned d = d3dx_convert_pixel(s, sf, df);
                                memcpy(drow + x * dbpp, &d, dbpp);
                        }
                }
                if (sw < dw) /* black out remaining pixels */
                        memset(drow + min_w * dbpp, 0, dbpp * (dw - min_w));
        }
        if (sh < dh) /* black out remaining rows */
                memset(dst + (dst_rect.top + (long)min_h) * (long)dst_pitch + dst_rect.left * (long)dbpp, 0,
                       (long)dst_pitch * (dh - min_h));
}

// --- SurfaceClass (surfaceclass.cpp) — the model -------------------------------
//
// The D3D surface is modelled as its description (width, height, format)
// plus the packed row bytes (pitch = width * bytes per pixel, the packed
// pitch D3D8 system-memory surfaces use; every observable access is
// within-row). The file constructor runs the Load_Surface_Immediate
// walk; a failed CreateImageSurface (zero dimensions or an unknown
// format) leaves the surface null, and Get_Description on a null surface
// is the crash site the verdicts state.

struct VerdictStop
{
        const char *site;
};

// The release-build WWASSERT shape (a no-op the transcription keeps so
// the transcribed assert sites stay visible).
static void WWASSERT_RELEASE2(bool) {}
#define WWASSERT_RELEASE(x) WWASSERT_RELEASE2(x)

struct SurfaceClass
{
        struct SurfaceDescription
        {
                WW3DFormat Format = WW3D_FORMAT_UNKNOWN;
                unsigned Height = 0;
                unsigned Width = 0;
        };

        unsigned Width = 0;
        unsigned Height = 0;
        WW3DFormat Format = WW3D_FORMAT_UNKNOWN;
        std::vector<uint8_t> Bytes;
        bool Null = false;
        bool IsPlaceholder = false;

        // SurfaceClass::SurfaceClass(w,h,format) — DX8Wrapper::
        // _Create_DX8_Surface refuses zero dimensions and unknown formats
        // (CreateImageSurface fails; the WWASSERTs are release no-ops).
        static SurfaceClass *Create(unsigned width, unsigned height, WW3DFormat format)
        {
                SurfaceClass *s = new SurfaceClass();
                if (width == 0 || height == 0 || format == WW3D_FORMAT_UNKNOWN)
                {
                        s->Null = true;
                        return s;
                }
                s->Width = width;
                s->Height = height;
                s->Format = format;
                s->Bytes.assign((size_t)width * height * Get_Bytes_Per_Pixel(format), 0);
                return s;
        }

        void Get_Description(SurfaceDescription &sd)
        {
                if (Null)
                        throw VerdictStop{"null-surface"};
                sd.Format = Format;
                sd.Height = Height;
                sd.Width = Width;
        }

        unsigned pitch() const { return Width * Get_Bytes_Per_Pixel(Format); }

        // SurfaceClass::Clear — zero every row.
        void Clear()
        {
                unsigned size = Get_Bytes_Per_Pixel(Format);
                for (unsigned i = 0; i < Height; i++)
                        memset(Bytes.data() + i * pitch(), 0, size * Width);
        }

        // SurfaceClass::Is_Transparent_Column, verbatim walk: the alpha is
        // the pixel's last byte (the MSB in BGRA byte order), narrowed by
        // the format's alpha bits. A no-alpha format (mask 0) reads every
        // column as transparent — the release assert is a no-op.
        bool Is_Transparent_Column(unsigned int column)
        {
                WWASSERT_RELEASE(Has_Alpha(Format));
                int alphabits = Alpha_Bits(Format);
                int mask = 0;
                switch (alphabits)
                {
                case 1: mask = 1; break;
                case 4: mask = 0xf; break;
                case 8: mask = 0xff; break;
                }

                unsigned int size = Get_Bytes_Per_Pixel(Format);

                for (int y = 0; y < (int)Height; y++)
                {
                        const unsigned char *alpha = Bytes.data() + y * (long)pitch() + column * size;
                        unsigned char myalpha = alpha[size - 1];
                        myalpha = (unsigned char)((myalpha >> (8 - alphabits)) & mask);
                        if (myalpha)
                        {
                                return false;
                        }
                }

                return true;
        }

        // SurfaceClass::FindBB, verbatim walk: the bounding box of the
        // non-zero-alpha pixels in [min, max), with the swapped initial
        // realmin/realmax (an empty region inverts the box — Make_
        // Proportional's x1 < x0 test is the consumer). The rect lock
        // comes first: a rect reaching past the surface fails
        // (INVALIDCALL) and the original dereferences the zeroed lock —
        // the crash site (a grid dimension of 8 mod 16 pushes the 15th
        // column/row's rect past the edge).
        void FindBB(long *min_i, long *min_j, long *max_i, long *max_j)
        {
                WWASSERT_RELEASE(Has_Alpha(Format));
                if (*max_i > (long)Width || *max_j > (long)Height)
                        throw VerdictStop{"findbb-lock"};
                int alphabits = Alpha_Bits(Format);
                int mask = 0;
                switch (alphabits)
                {
                case 1: mask = 1; break;
                case 4: mask = 0xf; break;
                case 8: mask = 0xff; break;
                }

                unsigned int size = Get_Bytes_Per_Pixel(Format);
                long realmin_i = *max_i, realmin_j = *max_j;
                long realmax_i = *min_i, realmax_j = *min_j;

                // the original locks the sub-rect and addresses the rows
                // through the full surface pitch; the model addresses the
                // surface directly (same bytes: a rect lock aliases the
                // surface rows)
                for (long y = *min_j; y < *max_j; y++)
                {
                        for (long x = *min_i; x < *max_i; x++)
                        {
                                const unsigned char *px =
                                    Bytes.data() + y * (long)pitch() + x * (long)size;
                                unsigned char myalpha = px[size - 1];
                                myalpha = (unsigned char)((myalpha >> (8 - alphabits)) & mask);
                                if (myalpha)
                                {
                                        realmin_i = std::min(realmin_i, x);
                                        realmax_i = std::max(realmax_i, x);
                                        realmin_j = std::min(realmin_j, y);
                                        realmax_j = std::max(realmax_j, y);
                                }
                        }
                }

                *max_i = realmax_i;
                *max_j = realmax_j;
                *min_i = realmin_i;
                *min_j = realmin_j;
        }

        // SurfaceClass::Copy (the region form), verbatim: the source rect
        // clamped to the source, the destination rect clamped to the
        // destination, then the same-format-same-size fast path (dead
        // under the pin — the packed page is A4R4G4B4, no Targa source
        // lands as A4R4G4B4) or the D3DX load.
        void Copy(
            unsigned int dstx, unsigned int dsty,
            unsigned int srcx, unsigned int srcy,
            unsigned int width, unsigned int height,
            const SurfaceClass *other)
        {
                SurfaceDescription sd, osd;
                Get_Description(sd);
                const_cast<SurfaceClass *>(other)->Get_Description(osd);

                D3DXRect src;
                src.left = (long)srcx;
                src.right = (long)(srcx + width);
                src.top = (long)srcy;
                src.bottom = (long)(srcy + height);

                if (src.right > (long)osd.Width) src.right = (long)osd.Width;
                if (src.bottom > (long)osd.Height) src.bottom = (long)osd.Height;

                if (sd.Format == osd.Format && sd.Width == osd.Width && sd.Height == osd.Height)
                {
                        /* DX8Wrapper::_Copy_DX8_Rects: the src rect copied
                         * 1:1 at the point offset (dead under the pin; the
                         * D3DX model's 1:1 walk is byte-identical to it) */
                        D3DXRect dst;
                        dst.left = (long)dstx;
                        dst.top = (long)dsty;
                        dst.right = dst.left + (src.right - src.left);
                        dst.bottom = dst.top + (src.bottom - src.top);
                        d3dx_load_surface_from_surface(
                            Bytes.data(), pitch(), sd.Format, dst,
                            other->Bytes.data(), other->pitch(), osd.Format, src);
                }
                else
                {
                        D3DXRect dest;
                        dest.left = (long)dstx;
                        dest.right = (long)(dstx + width);
                        dest.top = (long)dsty;
                        dest.bottom = (long)(dsty + height);

                        if (dest.right > (long)sd.Width) dest.right = (long)sd.Width;
                        if (dest.bottom > (long)sd.Height) dest.bottom = (long)sd.Height;

                        d3dx_load_surface_from_surface(
                            Bytes.data(), pitch(), sd.Format, dest,
                            other->Bytes.data(), other->pitch(), osd.Format, src);
                }
        }
};

// --- the harness environment ---------------------------------------------------
//
// The pinned file factory: a name -> bytes map standing in for the
// RawFileClass factory. g_loads counts the Targa image loads (the reuse
// pin); the DDS shadow probes the munged name and always misses (no
// corpus ships a .dds sibling — see the file comment).

static std::map<std::string, std::vector<uint8_t>> g_files;
static long g_loads = 0;

// The Load_Font_Image verdict channel (the harness's view of the file
// and Targa error codes; a plain global like font3d.cpp's own statics).
static long g_font_tga_error = 0;
static bool g_font_file_missing = false;

// A portable strupr (the CRT one is Windows-only).
static char *xstrupr(char *s)
{
        for (char *p = s; *p; p++)
                *p = (char)toupper((unsigned char)*p);
        return s;
}

// ddsfile.cpp's name munging (the availability probe Load_Surface_
// _Immediate's compressed branch drives): the last three characters
// overwritten with "dds".
static std::string munged_dds_name(const char *filename)
{
        std::string name(filename);
        if (name.size() < 3)
                return name; // the original writes before its buffer; refused
        name[name.size() - 3] = 'd';
        name[name.size() - 2] = 'd';
        name[name.size() - 1] = 's';
        return name;
}

static bool file_available(const char *name)
{
        return g_files.find(name) != g_files.end();
}

// MissingTexture::_Create_Missing_Surface: the 128x128 A8R8G8B8 surface
// _Init fills with the solid 0x7FFF00FF (BGRA bytes ff 00 ff 7f).
static SurfaceClass *Create_Missing_Surface()
{
        SurfaceClass *s = SurfaceClass::Create(128, 128, WW3D_FORMAT_A8R8G8B8);
        for (size_t i = 0; i + 4 <= s->Bytes.size(); i += 4)
        {
                s->Bytes[i + 0] = 0xff;
                s->Bytes[i + 1] = 0x00;
                s->Bytes[i + 2] = 0xff;
                s->Bytes[i + 3] = 0x7f;
        }
        s->IsPlaceholder = true;
        return s;
}

// TextureLoader::Load_Surface_Immediate, verbatim walk (texture_format
// UNKNOWN, allow_compression true, under the pinned caps: the compressed
// branch probes the munged .dds sibling and misses). The caller's
// palette buffer is the zero-filled stand-in for the original's
// uninitialised stack (see the file comment). The SurfaceClass file
// constructor's availability probe leads: an absent .tga (with its
// absent .dds fallback) is the missing-texture path. Returns the
// surface; on a missing file or a Targa error returns the placeholder
// and reports which.
static SurfaceClass *Load_Surface_Immediate(const char *filename, long &tga_error, bool &file_missing)
{
        tga_error = 0;
        file_missing = false;

        // DX8Wrapper::_Create_DX8_Surface's availability probe: the .tga,
        // then the .dds sibling the extension test accepts, then the
        // missing-texture placeholder.
        if (!file_available(filename))
        {
                std::string dds = munged_dds_name(filename);
                if (!file_available(dds.c_str()))
                {
                        file_missing = true;
                        return Create_Missing_Surface();
                }
                /* a present .dds sibling would load through the dds
                 * module's path; no corpus ships one */
                fprintf(stderr, "error: corpus %s ships a .dds sibling\n", filename);
                exit(1);
        }

        bool compressed = file_available(munged_dds_name(filename).c_str());
        if (compressed)
        {
                /* unreachable under the stated environment */
                fprintf(stderr, "error: corpus %s ships a .dds sibling\n", filename);
                exit(1);
        }

        /* Make sure the file can be opened. If not, return missing texture. */
        Targa targa;
        const std::vector<uint8_t> &bytes = g_files[filename];
        long open_error = targa.Open_Read(bytes.data(), bytes.size());
        if (open_error)
        {
                tga_error = open_error;
                return Create_Missing_Surface();
        }

        /* DX8 uses image upside down compared to TGA */
        targa.Header.ImageDescriptor ^= TGAIDF_YORIGIN;

        WW3DFormat src_format, dest_format;
        unsigned src_bpp = 0;
        Get_WW3D_Format(dest_format, src_format, src_bpp, targa);

        /* Destination size will be the next power of two square from the larger width and height... */
        unsigned width, height;
        width = (unsigned)targa.Header.Width;
        height = (unsigned)targa.Header.Height;
        unsigned src_width = (unsigned)targa.Header.Width;
        unsigned src_height = (unsigned)targa.Header.Height;

        /* NOTE: We load the palette but we do not yet support paletted textures! */
        std::vector<uint8_t> palette(256 * 4, 0);
        targa.SetPalette(palette.data());
        long load_error = targa.Load_Flags(TGAF_IMAGE, false);
        if (load_error)
        {
                tga_error = load_error;
                return Create_Missing_Surface();
        }
        g_loads++;

        unsigned char *src_surface = targa.GetImage();

        /* No paletted destination format allowed */
        unsigned char *converted_surface = nullptr;
        if (src_format == WW3D_FORMAT_A1R5G5B5 || src_format == WW3D_FORMAT_R5G6B5
            || src_format == WW3D_FORMAT_A4R4G4B4 || src_format == WW3D_FORMAT_P8
            || src_format == WW3D_FORMAT_L8 || src_width != width || src_height != height)
        {
                converted_surface = new unsigned char[(size_t)width * height * 4];
                dest_format = Get_Valid_Texture_Format(WW3D_FORMAT_A8R8G8B8);
                Copy_Image(
                    converted_surface,
                    width,
                    height,
                    width * 4,
                    WW3D_FORMAT_A8R8G8B8,
                    src_surface,
                    src_width,
                    src_height,
                    src_width * src_bpp,
                    src_format,
                    targa.GetPalette(),
                    (unsigned)(targa.Header.CMapDepth >> 3));
                src_surface = converted_surface;
                src_format = WW3D_FORMAT_A8R8G8B8;
                src_width = width;
                src_height = height;
                src_bpp = Get_Bytes_Per_Pixel(src_format);
        }

        unsigned src_pitch = src_width * src_bpp;

        SurfaceClass *d3d_surface = SurfaceClass::Create(width, height, dest_format);
        if (d3d_surface->Null)
        {
                /* CreateImageSurface refused (zero dimensions or an
                 * unknown format): the original constructs the object and
                 * dereferences the null D3D surface in Get_Description —
                 * the crash site. */
                delete[] converted_surface;
                throw VerdictStop{"null-surface"};
        }

        Copy_Image(
            d3d_surface->Bytes.data(),
            width,
            height,
            d3d_surface->pitch(),
            dest_format,
            src_surface,
            src_width,
            src_height,
            src_pitch,
            src_format,
            targa.GetPalette(),
            (unsigned)(targa.Header.CMapDepth >> 3)); /* No mipmap */

        delete[] converted_surface;

        return d3d_surface;
}

// --- font3d.cpp — the font loader, verbatim ------------------------------------

// font3d.cpp's file-scope static (kept verbatim: Minimize_Font_Image
// publishes its result through it).
static SurfaceClass *_surface = nullptr;

struct Font3DDataClass
{
        char *Name = nullptr;

        float UOffsetTable[256] = {0};
        float VOffsetTable[256] = {0};
        float UWidthTable[256] = {0};
        float VHeight = 0;
        unsigned char CharWidthTable[256] = {0};
        unsigned char CharHeight = 0;

        SurfaceClass *Texture = nullptr; // the packed page (the model keeps the surface)

        Font3DDataClass(const char *filename);
        ~Font3DDataClass();

        unsigned char Char_Width(unsigned short ch) { return CharWidthTable[ch & 0xFF]; }
        unsigned char Char_Height(unsigned short) { return CharHeight; }
        float Char_U_Offset(unsigned short ch) { return UOffsetTable[ch & 0xFF]; }
        float Char_V_Offset(unsigned short ch) { return VOffsetTable[ch & 0xFF]; }
        float Char_U_Width(unsigned short ch) { return UWidthTable[ch & 0xFF]; }
        float Char_V_Height(unsigned short) { return VHeight; }

        bool Load_Font_Image(const char *filename);
        SurfaceClass *Make_Proportional(SurfaceClass *font_image);
        SurfaceClass *Minimize_Font_Image(SurfaceClass *font_image);
};

// Font3DDataClass::Font3DDataClass — the constructor: load the image,
// then the uppercased name (the crash site precedes the name, so a
// crash verdict leaves it null).
Font3DDataClass::Font3DDataClass(const char *filename)
{
        Texture = nullptr;
        Load_Font_Image(filename);
        Name = xstrupr(strdup(filename));
}

Font3DDataClass::~Font3DDataClass()
{
        free(Name);
        Name = nullptr;

        delete Texture;
}

// Font3DDataClass::Minimize_Font_Image, verbatim (the WWDEBUG_SAY is the
// release no-op; the debug print's %c of the index is dropped with it).
SurfaceClass *Font3DDataClass::Minimize_Font_Image(SurfaceClass *surface)
{
        SurfaceClass::SurfaceDescription sd;

        surface->Get_Description(sd);

        float current_width = sd.Width;
        float current_height = sd.Height;

        // determine new width make the size of the new image either 128x128 or 256x256,
        // dependent on the width of the original image
        int new_width;
        if (current_width < 256)
        {
                new_width = 128;
        }
        else
        {
                new_width = 256;
        }

        int new_height = new_width;
        //  create a new 4 bit alpha image to build into
        // We dont support non-homogeneous copies just yet
        SurfaceClass *new_surface = SurfaceClass::Create(new_width, new_height, WW3D_FORMAT_A4R4G4B4);
        // fill with transparent black
        new_surface->Clear();

        // indices for the location of each added char
        int new_x = 0;
        int new_y = 0;

        // for each character, copy minimum bounding area to (new_x, new_y) in the new image
        for (int char_index = 0; char_index < 256; char_index++)
        {

                // find the lop left coordinate and the height and width of the char's bounding box
                // (must convert the normalized uv tables to pixels and round off)
                int src_x = (int)(UOffsetTable[char_index] * current_width + 0.5);
                int src_y = (int)(VOffsetTable[char_index] * current_height + 0.5);
                int width = (int)(UWidthTable[char_index] * current_width + 0.5);
                int height = (int)(VHeight * current_height + 0.5);

                // if the character has any visible pixels at all...
                if (width != 0)
                {

                        // if this charactger will not fit on the current line, goto the next line
                        if (new_x + width > new_width)
                        {
                                new_x = 0;
                                new_y += height;

                                // if we have run out of lines, we have a problem
                                // we assert because we have already modified tables for some of the chars
                                if (new_y + height > new_height)
                                {
                                        new_y -= height;
                                        // "Font doesn't fit texture 2 on char %c"
                                }
                        }

                        // blit from original image to new image
                        new_surface->Copy(new_x, new_y, src_x, src_y, width, height, surface);
                }

                // update the U and V tables to show new character location
                UOffsetTable[char_index] = (float)(new_x) / (float)new_width;
                VOffsetTable[char_index] = (float)(new_y) / (float)new_width;

                // update width in terms of new normal image width
                UWidthTable[char_index] *= (float)current_width / (float)new_width;

                new_x += width;
        }

        // update height in terms of new normal image height
        VHeight *= (float)current_height / (float)new_height;

        // release the old surface and return the new one
        delete surface;

        _surface = new_surface;

        return _surface;
}

// Font3DDataClass::Make_Proportional, verbatim.
SurfaceClass *Font3DDataClass::Make_Proportional(SurfaceClass *surface)
{
        SurfaceClass::SurfaceDescription sd;
        surface->Get_Description(sd);
        float width = sd.Width;
        float height = sd.Height;

        // for each character in the font...
        for (int char_index = 0; char_index < 256; char_index++)
        {

                // find the current bounding box
                // (must convert the normalized uv tables to pixels and round off)
                int x0 = (int)(UOffsetTable[char_index] * width + 0.5);
                int y0 = (int)(VOffsetTable[char_index] * height + 0.5);
                int x1 = x0 + (int)(UWidthTable[char_index] * width + 0.5);
                int y1 = y0 + (int)(VHeight * height + 0.5);

                //      find minimum bounding box by finding the minimum and maximum non-0 x pixel location
                long minb_i = x0, minb_j = y0;
                long maxb_i = x1, maxb_j = y1;

                surface->FindBB(&minb_i, &minb_j, &maxb_i, &maxb_j);

                // set the new edges
                x0 = (int)minb_i;
                x1 = (int)maxb_i + 1;

                // if we didn't find ANY non-transparent pixels, the char has no width.
                if (x1 < x0)
                {
                        x1 = x0;
                }

                // turn off all character after del
                if (char_index > 0x80)
                {
                        x1 = x0;
                }

                // update the U and width tables
                UOffsetTable[char_index] = (float)x0 / width;
                UWidthTable[char_index] = (float)(x1 - x0) / width;
                CharWidthTable[char_index] = (unsigned char)(x1 - x0);
        }

        // now shink the image given the minimum char sizes
        Minimize_Font_Image(_surface);
        return nullptr;
}

// Font3DDataClass::Load_Font_Image, verbatim (the surface plumbing: the
// file constructor, the strike/grid branch, and the texture creation
// that wraps the packed page).
bool Font3DDataClass::Load_Font_Image(const char *filename)
{
        // get the font surface (the SurfaceClass file constructor)
        SurfaceClass *surface = Load_Surface_Immediate(filename, g_font_tga_error, g_font_file_missing);

        SurfaceClass::SurfaceDescription sd;
        surface->Get_Description(sd);

        // If input is a font strike (strip) process it as such
        if (sd.Width > 8 * sd.Height)
        {

                // the height of the strike is the height of the characters
                VHeight = 1;
                CharHeight = (unsigned char)sd.Height;

                int column = 0;
                int width = (int)sd.Width;

                // for each char, find the uv start location and set the
                // mono-spaced width and height in normalized screen units
                for (int char_index = 0; char_index < 256; char_index++)
                {

                        if (char_index >= 0x7F)
                        {

                                UOffsetTable[char_index] = 0;
                                VOffsetTable[char_index] = 0;
                                UWidthTable[char_index] = 0;
                                CharWidthTable[char_index] = 0;
                        }
                        else
                        {

                                // find the first non-transparent column...
                                while ((column < width) && (surface->Is_Transparent_Column((unsigned)column)))
                                        column++;
                                int start = column;

                                // find the first transparent column...
                                while ((column < width) && (!surface->Is_Transparent_Column((unsigned)column)))
                                        column++;
                                int end = column;

                                if (end <= start)
                                {
                                        // "Error Char %d start %d end %d width %d"
                                }

                                UOffsetTable[char_index] = (float)start / width;
                                VOffsetTable[char_index] = 0;
                                UWidthTable[char_index] = (float)(end - start) / width;
                                CharWidthTable[char_index] = (unsigned char)(end - start);
                        }
                }

                // convert the just created mon-spaced font to proportional (optional)
                _surface = surface;
                surface = nullptr;
                Minimize_Font_Image(_surface);
        }
        else
        {

                // Determine the width and height of each mono spaced character in pixels
                // (assumes 16x16 array of chars)
                float font_width = sd.Width;
                float font_height = sd.Height;
                float mono_pixel_width = (font_width / 16);
                float mono_pixel_height = (font_height / 16);

                // for each char, find the uv start location and set the
                // mono-spaced width and height in normalized screen units
                for (int char_index = 0; char_index < 256; char_index++)
                {
                        UOffsetTable[char_index] =
                            (float)((char_index % 16) * mono_pixel_width) / font_width;
                        VOffsetTable[char_index] =
                            (float)((char_index / 16) * mono_pixel_height) / font_height;
                        UWidthTable[char_index] = mono_pixel_width / font_width;
                        CharWidthTable[char_index] = (unsigned char)mono_pixel_width;
                }
                VHeight = mono_pixel_height / font_height;
                CharHeight = (unsigned char)mono_pixel_height;

                // convert the just created mon-spaced font to proportional (optional)

                _surface = surface;
                surface = nullptr;
                Make_Proportional(_surface);
        }

        // create the texture
        if (_surface)
        {
                Texture = _surface;
                _surface = nullptr;
        }

        // return SUCCESS!
        return true;
}

// --- font3d.cpp — the instance class and the asset cache -----------------------

// A portable stricmp (the CRT one is not standard).
static int xstricmp(const char *a, const char *b)
{
        while (*a && *b)
        {
                int ca = tolower((unsigned char)*a);
                int cb = tolower((unsigned char)*b);
                if (ca != cb)
                        return ca - cb;
                a++;
                b++;
        }
        return tolower((unsigned char)*a) - tolower((unsigned char)*b);
}

// The asset manager's Font3DDatas list (a head-prepending list; the
// harness keeps it as explicit state per the no-globals rule).
struct Font3DAssetCache
{
        std::vector<Font3DDataClass *> list; // index 0 = the head
        long constructions = 0; // view-only: the builds (the reuse pin)

        ~Font3DAssetCache()
        {
                for (Font3DDataClass *f : list)
                        delete f;
        }

        // WW3DAssetManager::Get_Font3DData: the stricmp walk, then the
        // create-and-prepend. The file bytes come from the harness map
        // (a miss = the missing-texture path).
        Font3DDataClass *Get(const char *name)
        {
                for (Font3DDataClass *font : list)
                {
                        if (xstricmp(name, font->Name) == 0)
                        {
                                return font;
                        }
                }

                Font3DDataClass *font = new Font3DDataClass(name);
                list.insert(list.begin(), font);
                constructions++;
                return font;
        }
};

struct Font3DInstanceClass
{
        Font3DDataClass *FontData;
        float Scale;
        float SpaceSpacing;
        float InterCharSpacing;
        float MonoSpacing;

        float ScaledWidthTable[256] = {0};
        float ScaledSpacingTable[256] = {0};
        float ScaledHeight = 0;

        Font3DInstanceClass(Font3DAssetCache &cache, const char *filename)
        {
                FontData = cache.Get(filename);
                MonoSpacing = 0.0f;
                Scale = 1.0f;
                SpaceSpacing = (int)(FontData->Char_Width('H') / 2.0f);
                InterCharSpacing = 1;
                Build_Cached_Tables();
        }

        void Set_Mono_Spaced()
        {
                MonoSpacing = FontData->Char_Width('W') + 1;
                Build_Cached_Tables();
        }

        void Build_Cached_Tables()
        {
                // Rebuild the cached tables
                for (int a = 0; a < 256; ++a)
                {
                        float width = (float)FontData->Char_Width((unsigned short)a);
                        if (a == ' ')
                        {
                                width = SpaceSpacing;
                        }

                        ScaledWidthTable[a] = Scale * width;
                        if (MonoSpacing != 0.0f)
                        {
                                ScaledSpacingTable[a] = Scale * MonoSpacing;
                        }
                        else
                        {
                                ScaledSpacingTable[a] = Scale * (width + InterCharSpacing);
                        }
                }
                ScaledHeight = floorf(Scale * (float)FontData->Char_Height('A'));
        }

        float Char_Width(unsigned short ch) const { return ScaledWidthTable[ch & 0xFF]; }
        float Char_Spacing(unsigned short ch) const { return ScaledSpacingTable[ch & 0xFF]; }

        float String_Width(const char *test_str)
        {
                float width = 0.0;
                for (; *test_str; test_str++)
                {
                        width += Char_Spacing((unsigned char)*test_str);
                }

                return width;
        }
};

// --- the golden dump -------------------------------------------------------------

static FILE *g_out = nullptr;

static const char *hex(const uint8_t *data, size_t len)
{
        static std::string g_hex;
        g_hex.clear();
        char buf[3];
        for (size_t i = 0; i < len; i++)
        {
                snprintf(buf, sizeof(buf), "%02x", data[i]);
                g_hex += buf;
        }
        return g_hex.c_str();
}

// The wrap-31 sum the other slices' checksums use.
static uint64_t sum31(const uint8_t *p, size_t n)
{
        uint64_t s = 0;
        for (size_t i = 0; i < n; i++)
                s = s * 31 + p[i];
        return s;
}

static uint32_t fbits(float f)
{
        uint32_t u;
        memcpy(&u, &f, 4);
        return u;
}

static const char *fmt_word(WW3DFormat f)
{
        switch (f)
        {
        case WW3D_FORMAT_A8R8G8B8: return "A8R8G8B8";
        case WW3D_FORMAT_X8R8G8B8: return "X8R8G8B8";
        case WW3D_FORMAT_A8: return "A8";
        default: return "UNKNOWN";
        }
}

static std::string flip_case(const std::string &s)
{
        std::string out = s;
        for (char &c : out)
                c = isupper((unsigned char)c) ? (char)tolower((unsigned char)c)
                                              : (char)toupper((unsigned char)c);
        return out;
}

// The probe chars the iprobe lines dump (the space override, the 'H' the
// space spacing derives from, the 'W' the mono spacing derives from, and
// the turn-off boundary family).
static const int PROBE_CHARS[] = {0x20, 0x48, 0x57, 0x7f, 0x80, 0x81, 0xff};

static uint64_t instance_tabsum(const Font3DInstanceClass &inst)
{
        uint64_t s = 0;
        for (int a = 0; a < 256; a++)
        {
                s = s * 31 + fbits(inst.ScaledWidthTable[a]);
                s = s * 31 + fbits(inst.ScaledSpacingTable[a]);
        }
        return s;
}

static void dump_corpus(const char *id, const char *name, bool has_file,
                        const std::vector<uint8_t> &bytes)
{
        fprintf(g_out, "font %s %s\n", id, name);
        fprintf(g_out, "file %s %s\n", id,
                has_file ? hex(bytes.data(), bytes.size()) : "-");

        // The scenario's file factory.
        g_files.clear();
        if (has_file)
                g_files[name] = bytes;
        g_loads = 0;
        g_font_tga_error = 0;
        g_font_file_missing = false;

        // The source surface (a deterministic replay of the font's own
        // load; its counters are reset before the real flow).
        long src_err = 0;
        bool src_missing = false;
        SurfaceClass *source = nullptr;
        try
        {
                source = Load_Surface_Immediate(name, src_err, src_missing);
        }
        catch (VerdictStop &v)
        {
                // The null-surface crash site: the tables never build,
                // the Name never assigns
                fprintf(g_out, "verdict %s crash-%s\n", id, v.site);
                return;
        }
        (void)src_err;
        (void)src_missing;

        // The real flow: the font through the asset cache (its load
        // counts, its tables, its packed page). The walk can crash at
        // the FindBB rect lock — the same verdict family.
        g_loads = 0;
        g_font_tga_error = 0;
        g_font_file_missing = false;

        Font3DAssetCache cache;
        Font3DDataClass *font = nullptr;
        try
        {
                font = cache.Get(name);
        }
        catch (VerdictStop &v)
        {
                fprintf(g_out, "verdict %s crash-%s\n", id, v.site);
                delete source;
                return;
        }

        if (g_font_tga_error)
                fprintf(g_out, "verdict %s tgaerr-%ld\n", id, g_font_tga_error);
        else if (g_font_file_missing)
                fprintf(g_out, "verdict %s missing\n", id);
        else
                fprintf(g_out, "verdict %s ok\n", id);

        fprintf(g_out, "name %s %s\n", id, font->Name);

        fprintf(g_out, "sfmt %s %u %u %s\n", id, source->Width, source->Height,
                source->IsPlaceholder ? "PLACEHOLDER" : fmt_word(source->Format));
        fprintf(g_out, "ssum %s %llu\n", id,
                (unsigned long long)sum31(source->Bytes.data(), source->Bytes.size()));
        delete source;

        SurfaceClass *packed = font->Texture;
        fprintf(g_out, "geom %s %d %08x %u %u\n", id, (int)font->CharHeight,
                fbits(font->VHeight), packed->Width, packed->Height);

        for (int row = 0; row < 16; row++)
        {
                fprintf(g_out, "tab %s %d", id, row);
                for (int col = 0; col < 16; col++)
                {
                        int c = row * 16 + col;
                        fprintf(g_out, " %08x %08x %08x %02x", fbits(font->UOffsetTable[c]),
                                fbits(font->VOffsetTable[c]), fbits(font->UWidthTable[c]),
                                (unsigned)font->CharWidthTable[c]);
                }
                fprintf(g_out, "\n");
        }

        fprintf(g_out, "ptex %s %u %u %llu\n", id, packed->Width, packed->Height,
                (unsigned long long)sum31(packed->Bytes.data(), packed->Bytes.size()));

        // The packed page's non-zero rows (up to six, then the marker).
        {
                unsigned row_bytes = packed->Width * 2;
                int shown = 0;
                bool more = false;
                for (unsigned y = 0; y < packed->Height; y++)
                {
                        const uint8_t *row = packed->Bytes.data() + y * packed->pitch();
                        bool nonzero = false;
                        for (unsigned i = 0; i < row_bytes; i++)
                                if (row[i] != 0)
                                {
                                        nonzero = true;
                                        break;
                                }
                        if (!nonzero)
                                continue;
                        if (shown == 6)
                        {
                                more = true;
                                break;
                        }
                        fprintf(g_out, "ptexr %s %u %s\n", id, y, hex(row, row_bytes));
                        shown++;
                }
                if (more)
                        fprintf(g_out, "ptexr %s more -\n", id);
        }

        // The instance layer (the cached font reused; no second load).
        {
                Font3DInstanceClass inst(cache, name);

                fprintf(g_out, "inst %s %08x %08x %llu\n", id, fbits(inst.SpaceSpacing),
                        fbits(inst.ScaledHeight), (unsigned long long)instance_tabsum(inst));

                for (int ch : PROBE_CHARS)
                {
                        fprintf(g_out, "iprobe %s %02x %08x %08x\n", id, ch,
                                fbits(inst.Char_Width((unsigned short)ch)),
                                fbits(inst.Char_Spacing((unsigned short)ch)));
                }

                // String_Width probes: "Hi W!" and two spaces.
                fprintf(g_out, "iwidth %s %s %08x\n", id, "4869205721",
                        fbits(inst.String_Width("Hi W!")));
                fprintf(g_out, "iwidth %s %s %08x\n", id, "2020",
                        fbits(inst.String_Width("  ")));

                inst.Set_Mono_Spaced();
                fprintf(g_out, "imono %s %08x %08x %llu\n", id, fbits(inst.MonoSpacing),
                        fbits(inst.ScaledHeight), (unsigned long long)instance_tabsum(inst));
        }

        // The asset-cache reuse pin: the case-flipped name must hit
        // (the construction count stays at one build).
        {
                std::string flipped = flip_case(name);
                Font3DInstanceClass inst2(cache, flipped.c_str());
                bool hit = (inst2.FontData == font);
                fprintf(g_out, "reuse %s %s %ld\n", id,
                        hit ? "t" : "f", cache.constructions);
        }
}

// --- corpus builders -------------------------------------------------------------

static void put_u16(std::vector<uint8_t> &v, uint16_t x)
{
        v.push_back((uint8_t)(x & 0xff));
        v.push_back((uint8_t)(x >> 8));
}

static void put_u32(std::vector<uint8_t> &v, uint32_t x)
{
        v.push_back((uint8_t)(x & 0xff));
        v.push_back((uint8_t)((x >> 8) & 0xff));
        v.push_back((uint8_t)((x >> 16) & 0xff));
        v.push_back((uint8_t)((x >> 24) & 0xff));
}

static void put_header(std::vector<uint8_t> &v, int cmap_type, int image_type, int cmap_start,
                       int cmap_len, int cmap_depth, int w, int h, int depth, int descriptor)
{
        TGAHeader hdr{};
        hdr.IDLength = 0;
        hdr.ColorMapType = (char)cmap_type;
        hdr.ImageType = (char)image_type;
        hdr.CMapStart = (short)cmap_start;
        hdr.CMapLength = (short)cmap_len;
        hdr.CMapDepth = (char)cmap_depth;
        hdr.XOffset = 0;
        hdr.YOffset = 0;
        hdr.Width = (short)w;
        hdr.Height = (short)h;
        hdr.PixelDepth = (char)depth;
        hdr.ImageDescriptor = (char)descriptor;
        v.insert(v.end(), (const uint8_t *)&hdr, (const uint8_t *)&hdr + sizeof(hdr));
}

// A 16-bit A1R5G5B5 pixel (the documented font format).
static uint16_t px16(int a, int r, int g, int b)
{
        return (uint16_t)((a ? 0x8000 : 0) | ((r & 31) << 10) | ((g & 31) << 5) | (b & 31));
}

// A greedy RLE encoder (the TGA type-9/10 packet shapes): a run of two or
// more equal pixels becomes a run packet, anything else raw packets of up
// to 128 pixels.
static void put_rle(std::vector<uint8_t> &v, const std::vector<uint8_t> &pixels, size_t depth,
                    size_t count)
{
        size_t i = 0;
        while (i < count)
        {
                // the run length at i (capped at 128)
                size_t run = 1;
                while (run < 128 && i + run < count
                       && memcmp(&pixels[(i + run - 1) * depth], &pixels[(i + run) * depth], depth) == 0)
                        run++;
                if (run >= 2)
                {
                        v.push_back((uint8_t)(0x80 | (run - 1)));
                        v.insert(v.end(), pixels.begin() + (long)(i * depth),
                                 pixels.begin() + (long)((i + 1) * depth));
                        i += run;
                }
                else
                {
                        size_t raw = 1;
                        while (raw < 128 && i + raw < count)
                        {
                                // stop if a run of two starts here
                                if (i + raw + 1 < count
                                    && memcmp(&pixels[(i + raw) * depth], &pixels[(i + raw + 1) * depth],
                                              depth) == 0)
                                        break;
                                raw++;
                        }
                        v.push_back((uint8_t)(raw - 1));
                        v.insert(v.end(), pixels.begin() + (long)(i * depth),
                                 pixels.begin() + (long)((i + raw) * depth));
                        i += raw;
                }
        }
}

// A 16-bit truecolor corpus (raw or RLE) from a pixel buffer.
static std::vector<uint8_t> tga16(int w, int h, const std::vector<uint16_t> &pixels, int descriptor,
                                  bool rle)
{
        std::vector<uint8_t> v;
        put_header(v, 0, rle ? TGA_TRUECOLOR_ENCODED : TGA_TRUECOLOR, 0, 0, 0, w, h, 16, descriptor);
        std::vector<uint8_t> raw;
        for (uint16_t p : pixels)
                put_u16(raw, p);
        if (rle)
                put_rle(v, raw, 2, (size_t)w * h);
        else
                v.insert(v.end(), raw.begin(), raw.end());
        return v;
}

// A 32-bit truecolor corpus from BGRA pixels.
static std::vector<uint8_t> tga32(int w, int h, const std::vector<uint32_t> &pixels, int descriptor,
                                  bool rle)
{
        std::vector<uint8_t> v;
        put_header(v, 0, rle ? TGA_TRUECOLOR_ENCODED : TGA_TRUECOLOR, 0, 0, 0, w, h, 32, descriptor);
        std::vector<uint8_t> raw;
        for (uint32_t p : pixels)
                put_u32(raw, p);
        if (rle)
                put_rle(v, raw, 4, (size_t)w * h);
        else
                v.insert(v.end(), raw.begin(), raw.end());
        return v;
}

// A strike: glyphs of `glyph` columns separated by `gap` transparent
// columns, `count` of them, from the top. `plain` glyphs are a solid
// color (RLE-friendly); the default varies per pixel.
static std::vector<uint16_t> strike_pixels(int w, int h, int glyph, int gap, int count, bool plain = false)
{
        std::vector<uint16_t> px((size_t)w * h, px16(0, 0, 0, 0));
        int col = 0;
        for (int c = 0; c < count && col < w; c++)
        {
                for (int x = 0; x < glyph && col + x < w; x++)
                        for (int y = 0; y < h; y++)
                                px[(size_t)y * w + col + x] =
                                    plain ? px16(1, c & 31, (c * 2) & 31, (c * 3) & 31)
                                          : px16(1, (c * 3 + x) & 31, (c * 5 + y) & 31,
                                                 (c * 7 + x + y) & 31);
                col += glyph + gap;
        }
        return px;
}

// A grid: a blob in the cell of each index in `cells`, at the given offset
// inside the cell.
static std::vector<uint16_t> grid_pixels(int w, int h, const std::vector<int> &cells, int bw, int bh,
                                         int ox, int oy)
{
        std::vector<uint16_t> px((size_t)w * h, px16(0, 0, 0, 0));
        float mono_w = (float)w / 16, mono_h = (float)h / 16;
        for (int c : cells)
        {
                int cx = (int)((c % 16) * mono_w);
                int cy = (int)((c / 16) * mono_h);
                for (int y = 0; y < bh; y++)
                        for (int x = 0; x < bw; x++)
                        {
                                int xx = cx + ox + x, yy = cy + oy + y;
                                if (xx >= 0 && xx < w && yy >= 0 && yy < h)
                                        px[(size_t)yy * w + xx] = px16(1, (c + x) & 31, (c * 2 + y) & 31,
                                                                      (c * 3 + x + y) & 31);
                        }
        }
        return px;
}

} // namespace fontgen

using namespace fontgen;

int main(int argc, char **argv)
{
        if (argc != 2)
        {
                fprintf(stderr, "usage: gen_font_golden <output-file>\n");
                return 1;
        }
        g_out = fopen(argv[1], "w");
        if (!g_out)
        {
                fprintf(stderr, "error: cannot open %s\n", argv[1]);
                return 1;
        }
        fprintf(g_out, "# gen_font_golden.cpp — font3d.cpp Font3DDataClass transcription (baseline 571a7c5).\n");
        fprintf(g_out, "# Golden script for rzh-formats::font; see tools/differential/README.md.\n");
        fprintf(g_out, "# Grammar: font/file/verdict/name/sfmt/ssum/geom/tab/ptex/ptexr/inst/iprobe/iwidth/imono/reuse.\n");

        // --- the strike family (16-bit A1R5G5B5, the documented font format)
        dump_corpus("strike16_basic", "Airborne.tga", true,
                    tga16(160, 16, strike_pixels(160, 16, 8, 2, 10), 0x20, false));
        dump_corpus("strike16_1024", "Wide.tga", true,
                    tga16(1024, 16, strike_pixels(1024, 16, 15, 2, 60, true), 0x20, true));
        // one 300-column glyph: the packed-page crop and the uchar width wrap
        dump_corpus("strike16_300", "Huge.tga", true,
                    tga16(300, 16, strike_pixels(300, 16, 300, 0, 1), 0x20, false));
        // 130 one-column glyph runs: the 0x7F turn-off zeroes the tail runs
        dump_corpus("strike16_runs", "Runs.tga", true,
                    tga16(260, 8, strike_pixels(260, 8, 1, 1, 130, true), 0x20, true));
        dump_corpus("strike16_empty", "Empty.tga", true,
                    tga16(96, 8, std::vector<uint16_t>(96 * 8), 0x20, true));
        dump_corpus("strike16_rle", "Rle.tga", true,
                    tga16(160, 16, strike_pixels(160, 16, 8, 2, 10), 0x20, true));
        // the bottom-up file: the XOR flips it back to the basic image
        {
                std::vector<uint16_t> px = strike_pixels(160, 16, 8, 2, 10);
                std::vector<uint16_t> rev(px.size());
                for (int y = 0; y < 16; y++)
                        for (int x = 0; x < 160; x++)
                                rev[(size_t)(15 - y) * 160 + x] = px[(size_t)y * 160 + x];
                dump_corpus("strike16_bottomup", "Flip.tga", true, tga16(160, 16, rev, 0x00, false));
        }
        // the left-origin file: XFlip mirrors the glyph order
        {
                std::vector<uint16_t> px = strike_pixels(160, 16, 8, 2, 10);
                std::vector<uint16_t> mir(px.size());
                for (int y = 0; y < 16; y++)
                        for (int x = 0; x < 160; x++)
                                mir[(size_t)y * 160 + x] = px[(size_t)y * 160 + (159 - x)];
                dump_corpus("strike16_xflip", "Mirror.tga", true, tga16(160, 16, mir, 0x30, false));
        }
        // 22 glyphs of 44 columns: the fifth line overflows and overlaps
        dump_corpus("strike16_overlap", "Pile.tga", true,
                    tga16(1024, 64, strike_pixels(1024, 64, 44, 2, 22, true), 0x20, true));

        // --- the grid family (16-bit)
        dump_corpus("grid16_128", "Grid128.tga", true,
                    tga16(128, 128,
                          grid_pixels(128, 128, {0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 20, 30, 40, 50, 60},
                                      4, 6, 2, 1),
                          0x20, true));
        dump_corpus("grid16_256", "Grid256.tga", true,
                    tga16(256, 256,
                          grid_pixels(256, 256,
                                      {0, 1, 2, 9, 0x2f, 0x30, 0x39, 0x40, 0x7f, 0x80, 0x81, 0xfe, 0xff},
                                      5, 7, 3, 2),
                          0x20, true));
        // a blob straddling two cells: each cell's FindBB is bounded
        {
                std::vector<uint16_t> px((size_t)128 * 128, px16(0, 0, 0, 0));
                for (int y = 0; y < 8; y++)
                        for (int x = 4; x < 12; x++)
                                px[(size_t)y * 128 + x] = px16(1, x, y, 3);
                dump_corpus("grid16_bleed", "Bleed.tga", true, tga16(128, 128, px, 0x20, true));
        }
        dump_corpus("grid16_empty", "Ghost.tga", true,
                    tga16(128, 128, std::vector<uint16_t>(128 * 128), 0x20, true));
        dump_corpus("grid16_100", "Odd.tga", true,
                    tga16(100, 100, grid_pixels(100, 100, {0, 3, 9, 0x41, 0xff}, 3, 4, 1, 1), 0x20, true));
        {
                // 20x20: fractional 1.25-pixel cells, the overlapping
                // FindBB rects they produce (a width below 16 would push
                // the 15th column's rect past the surface — the
                // original reads heap garbage there; the stated bound
                // keeps every corpus rect in surface)
                std::vector<uint16_t> px((size_t)20 * 20, px16(0, 0, 0, 0));
                for (int i = 0; i < 20; i++)
                {
                        px[(size_t)i * 20 + i] = px16(1, 5, 6, 7);
                        px[(size_t)i * 20 + ((i * 3 + 1) % 20)] = px16(1, 9, 10, 11);
                }
                px[17] = px16(1, 13, 14, 15);
                dump_corpus("grid16_tiny", "Tiny.tga", true, tga16(20, 20, px, 0x20, false));
        }
        {
                // 8x4: a grid width of 8 (the half-pixel cell) pushes the
                // 15th column's bounding rect past the surface — the
                // rect lock fails and the original dereferences the
                // zeroed lock (the crash verdict)
                std::vector<uint16_t> px(32, px16(0, 0, 0, 0));
                px[0] = px16(1, 5, 6, 7);
                px[17] = px16(1, 9, 10, 11);
                dump_corpus("grid16_8x4", "Flat.tga", true, tga16(8, 4, px, 0x20, false));
        }

        // --- the depth family
        {
                // 32-bit ARGB with mid alphas (the full alpha byte matters)
                std::vector<uint32_t> px((size_t)128 * 128, 0x80402010u);
                for (int c : {0, 1, 2, 5, 0x28, 0x30, 0x7f, 0x80})
                {
                        int cx = (c % 16) * 8, cy = (c / 16) * 8;
                        for (int y = 1; y < 6; y++)
                                for (int x = 1; x < 6; x++)
                                        px[(size_t)(cy + y) * 128 + cx + x] = 0xff302010u;
                }
                dump_corpus("font32_grid", "Argb.tga", true, tga32(128, 128, px, 0x20, true));
        }
        {
                // 32-bit RLE strike
                std::vector<uint32_t> px((size_t)160 * 16, 0);
                for (int c = 0; c < 10; c++)
                        for (int y = 0; y < 16; y++)
                                for (int x = 0; x < 8; x++)
                                        px[(size_t)y * 160 + c * 16 + x] = 0xff102030u;
                dump_corpus("font32_rle", "ArgbRle.tga", true, tga32(160, 16, px, 0x20, true));
        }
        {
                // 24-bit: the X8R8G8B8 surface has no alpha bits — every
                // column reads transparent, every glyph dies
                std::vector<uint8_t> v;
                put_header(v, 0, TGA_TRUECOLOR_ENCODED, 0, 0, 0, 128, 128, 24, 0x20);
                std::vector<uint8_t> raw;
                for (int y = 0; y < 128; y++)
                        for (int x = 0; x < 128; x++)
                        {
                                uint8_t b = (uint8_t)(y & 0x1f);
                                raw.push_back(b);
                                raw.push_back((uint8_t)(b * 3));
                                raw.push_back((uint8_t)(b * 7));
                        }
                put_rle(v, raw, 3, 128 * 128);
                dump_corpus("font24_grid", "Opaque.tga", true, v);
        }
        {
                // 8-bit mono (L8): every pixel opaque after the conversion
                // (MONO_ENCODED is the reader's missing case, so this stays
                // raw — a 64x64 grid pins the same all-opaque walk)
                std::vector<uint8_t> v;
                put_header(v, 0, TGA_MONO, 0, 0, 0, 64, 64, 8, 0x20);
                for (int y = 0; y < 64; y++)
                        for (int x = 0; x < 64; x++)
                                v.push_back((uint8_t)((x / 4 + y / 4) & 0xff));
                dump_corpus("font8_mono", "Luma.tga", true, v);
        }
        {
                // 8-bit paletted, 32-bit colormap: the entry's first byte
                // is the alpha the game reads (see the file comment)
                std::vector<uint8_t> v;
                put_header(v, 1, TGA_CMAPPED_ENCODED, 0, 256, 32, 128, 128, 8, 0x20);
                for (int e = 0; e < 256; e++)
                {
                        uint8_t a = (e % 3 == 0) ? 0 : (uint8_t)((e * 37) & 0xff);
                        v.push_back(a);             // the game's alpha
                        v.push_back((uint8_t)(e));      // the game's R
                        v.push_back((uint8_t)(e * 3));  // the game's G
                        v.push_back((uint8_t)(e * 7));  // the game's B
                }
                std::vector<uint8_t> raw;
                for (int y = 0; y < 128; y++)
                        for (int x = 0; x < 128; x++)
                                raw.push_back((uint8_t)((x / 8 + y / 8) & 0xff));
                put_rle(v, raw, 1, 128 * 128);
                dump_corpus("font8_pal", "Pal32.tga", true, v);
        }
        {
                // 8-bit paletted, 24-bit colormap: the alpha forced to 0xff
                std::vector<uint8_t> v;
                put_header(v, 1, TGA_CMAPPED_ENCODED, 0, 256, 24, 128, 128, 8, 0x20);
                for (int e = 0; e < 256; e++)
                {
                        v.push_back((uint8_t)(e * 7));
                        v.push_back((uint8_t)(e * 3));
                        v.push_back((uint8_t)e);
                }
                std::vector<uint8_t> raw;
                for (int y = 0; y < 128; y++)
                        for (int x = 0; x < 128; x++)
                                raw.push_back((uint8_t)((x / 16 + y / 16) & 0xff));
                put_rle(v, raw, 1, 128 * 128);
                dump_corpus("font8_pal24", "Pal24.tga", true, v);
        }
        {
                // 8-bit truecolor: the A8 surface (alpha-only) — the
                // minimize conversion sets the missing rgb channels to max
                std::vector<uint8_t> v;
                put_header(v, 0, TGA_TRUECOLOR_ENCODED, 0, 0, 0, 128, 128, 8, 0x20);
                std::vector<uint8_t> raw;
                for (int y = 0; y < 128; y++)
                        for (int x = 0; x < 128; x++)
                        {
                                int cx = (x / 8), cy = (y / 8);
                                int cell = cy * 16 + cx;
                                bool on = (cell % 5) != 0 && (x % 8) >= 1 && (x % 8) <= 5
                                          && (y % 8) >= 1 && (y % 8) <= 6;
                                raw.push_back(on ? (uint8_t)(0x40 + cell) : 0);
                        }
                put_rle(v, raw, 1, 128 * 128);
                dump_corpus("font8_alpha", "Alpha.tga", true, v);
        }

        // --- the error and crash family
        {
                // depth 15: the unknown format, the null surface, the crash
                std::vector<uint8_t> v;
                put_header(v, 0, TGA_TRUECOLOR, 0, 0, 0, 64, 8, 15, 0x20);
                v.resize(v.size() + 64 * 8 * 2, 0);
                dump_corpus("font15_crash", "D15.tga", true, v);
        }
        {
                // zero height: the load succeeds (the image is empty),
                // the surface cannot exist
                std::vector<uint8_t> v;
                put_header(v, 0, TGA_TRUECOLOR, 0, 0, 0, 16, 0, 16, 0x20);
                v.resize(v.size() + 26, 0); // a plain tail past the footer sniff
                dump_corpus("font0h_crash", "Zero.tga", true, v);
        }
        {
                // truncated image data: the read fails, the placeholder
                std::vector<uint8_t> v = tga16(160, 16, strike_pixels(160, 16, 8, 2, 10), 0x20, false);
                v.resize(v.size() - 5000);
                dump_corpus("font_truncated", "Cut.tga", true, v);
        }
        {
                // unsupported image type: the NOTSUPPORTED error
                std::vector<uint8_t> v;
                put_header(v, 0, 5, 0, 0, 0, 64, 8, 16, 0x20);
                v.resize(v.size() + 64 * 8 * 2, 0);
                dump_corpus("font_badtype", "Bad.tga", true, v);
        }
        dump_corpus("font_missing", "Absent.tga", false, {});

        fclose(g_out);
        return 0;
}

