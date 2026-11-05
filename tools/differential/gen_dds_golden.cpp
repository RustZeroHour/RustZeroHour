// SPDX-License-Identifier: GPL-3.0-or-later
//
// Golden-dump driver for `rzh-formats::dds` (parity level P0).
//
// Verbatim transcription of the Zero Hour DDS texture reader —
// GeneralsMD/Code/Libraries/Source/WWVegas/WW3D2/ddsfile.{h,cpp} at
// commit 571a7c5 — with the FileClass plumbing replaced by a byte
// source (constructor + Load). Everything else is the original code,
// comments and all: the name munging that overwrites the last three
// characters with "dds", the magic read that is never validated, the
// LegacyDDSURFACEDESC2 decode with its read-size == Size-field check,
// the FourCC format conversion, the mip-level arithmetic (reduction
// factor, the two dropped levels, the 4/8 block divisor, the cube face
// stride with its hardcoded +16), the DXTC level layout, the Get_Pixel
// queries (DXT1's both color modes, DXT5's 8/6-alpha tables), the
// Get_4x4_Block decompressor with its per-destination-format writes,
// the Copy_Level_To_Surface / Copy_CubeMap_Level_To_Surface / volume
// paths, the dead Get_Volume_Memory_Pointer, and the HSV recolor stack
// the hue-shifted copies drive (colorspace.h's RGB_To_HSV/HSV_To_RGB/
// Recolor, DX8Wrapper::Convert_Color, Vector3::Convert_To_ARGB).
//
// Supporting transcriptions (only the parts the DDS path drives):
//
// - formconv.cpp: Init_D3D_To_WW3_Conversion, the conversion array and
//   D3DFormat_To_WW3DFormat (the FourCC → WW3DFormat lookup, including
//   the numeric small-value fallback into the conversion array).
// - ww3dformat.cpp: Get_Bytes_Per_Pixel.
// - bitmaphandler.h: BitmapHandlerClass::Write_B8G8R8A8 (the byte
//   writer Get_4x4_Block drives for every destination format).
// - dx8wrapper.h: DX8Wrapper::Convert_Color(unsigned) and the
//   Convert_Color(Vector3, float) → Vector3::Convert_To_ARGB(alpha)
//   composition (vector3.h), the Recolor(unsigned&) round trip.
//
// The ddraw.h constants the reader indexes (DDSCAPS2_CUBEMAP /
// DDSCAPS2_VOLUME, the D3DFMT_* values) come from the Windows DirectX 8
// SDK headers the original includes; the tree does not vendor them. The
// numeric values are pinned by formconv.cpp's own commented conversion
// table (which lists the array indices 20..52 per format) plus the SDK's
// published enum: R8G8B8=20, A8R8G8B8=21, X8R8G8B8=22, R5G6B5=23,
// X1R5G5B5=24, A1R5G5B5=25, A4R4G4B4=26, R3G3B2=27, A8=28,
// A8R3G3B2=29, X4R4G4B4=30, A8P8=40, P8=41, L8=50, A8L8=51, A4L4=52,
// V8U8=60, L6V5U5=61, X8L8V8U8=62 (= HIGHEST_SUPPORTED_D3DFORMAT).
// The DXT codes are FOURCC('D','X','T','n').
//
// Deliberate deviations, pinned identically on both sides:
//
// - The game is a 32-bit build: LegacyDDSURFACEDESC2::Surface is a
//   4-byte pointer slot on disk. The transcription models it as an
//   `unsigned` placeholder (the field is never dereferenced or read by
//   the loader) so the struct is 124 bytes on a 64-bit host — the same
//   trick the mesh transcription uses for its pointer-sized fields.
// - The name munging writes Name[len-3..len) — out of bounds before the
//   buffer when the name is shorter than three characters. The original
//   scribbles over whatever precedes the buffer (UB); the transcription
//   and the Rust port both refuse such names (the game only ever passes
//   path-shaped names).
// - A header whose Size field is < 124 and whose file supplies exactly
//   Size desc bytes passes the original's check with the rest of the
//   struct left uninitialized (nondeterministic). Both sides refuse it;
//   only Size == 124 loads deterministically. The clean failure shapes
//   (Size != 124 with a longer file, or a short file) are pinned.
// - Write_B8G8R8A8's R3G3B2 case stores a 16-bit word built from an
//   8-bit value (`*(unsigned short*)dest_ptr = tmp;`) — one byte past
//   the pixel. Both compilers zero-extend the byte into the register
//   (movzbl), so the stray byte is 0x00; the transcription pins that
//   zero-extension and the Rust port performs the same two-byte store.
// - The WWASSERT / WWDEBUG_SAY / Bitmap_Assert sites are release-build
//   no-ops in the original; the transcription transcribes them as
//   comments and takes the fall-through path (the shape the game's
//   Release build executes).
// - Get_Date_Time returns 0: the byte source has no file-system stamp.
//   The date-time is VFS metadata (texture.cpp's reload check), not
//   file content.
//
// Output grammar (one command per line, hex lowercase, '-' = empty):
//
//   file <id> <hex|->            the corpus bytes the case loads
//   name <id> <hex|->            the munged Name buffer as a C string
//   ok <id> / err <id>           the constructor verdict (Is_Available)
//   hdr <id> <11 ints>           Width Height Depth FullWidth
//                                FullHeight FullDepth MipLevels
//                                ReductionFactor Format Type CubeFaceSize
//   lvls <id> <size:off ...|->   the level arrays the constructor built
//   load <id> <ok|err>           Load()'s verdict
//   data <id> <hex|->            the DDSMemory bytes Load() read
//   px <id> <lvl> <x> <y> <hex8> Get_Pixel
//   blk <id> <lvl> <sx> <sy> <fmt> <hex> <a>
//                                Get_4x4_Block into a 4x4 surface of
//                                <fmt> (pitch 4*bpp; the dump includes
//                                four trailing bytes so the R3G3B2
//                                stray store is visible), <a> = the
//                                contains-alpha verdict
//   cpy <id> <lvl> <fmt> <w> <h> <h> <s> <v> <hex>
//                                Copy_Level_To_Surface (memory overload)
//                                into a w*h buffer of <fmt>; the dump
//                                includes four trailing bytes. An
//                                untouched buffer dumps its 0xcc fill.
//   cpycube <id> <face> <lvl> <fmt> <w> <h> <h> <s> <v> <hex>
//                                Copy_CubeMap_Level_To_Surface, same
//                                shape as cpy
//   cpyvol <id> <lvl> <fmt> <w> <h> <h> <s> <v> <hex>
//                                Copy_Volume_Level_To_Surface (always
//                                the untouched fill — the dead volume
//                                pointer early-returns)
//   cubeptr <id> <face> <lvl> <int>
//                                Get_CubeMap_Memory_Pointer as an
//                                offset from the data base
//   volptr <id> <lvl> <- >       Get_Volume_Memory_Pointer (always null)
//   recolor <id> <in-hex8> <h> <s> <v> <out-hex8>
//                                the Recolor(unsigned&, hsv) round trip
//   rgb565 <id> <in-hex4> <argb-hex8> <back-hex4>
//                                RGB565_To_ARGB8888 then
//                                ARGB8888_To_RGB565
//
// Usage: gen_dds_golden <output-file>
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
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>
#include <vector>

// ----------------------------------------------------------------------------
// The transcription's byte source — the FileClass surface ddsfile.cpp drives
// ----------------------------------------------------------------------------

struct ByteFile
{
        // A cheap handle over the caller's bytes — the file_auto_ptr's
        // fresh-open stand-in (copying the handle is copying the seek
        // state, not the file).
        const uint8_t *data = nullptr;
        size_t len = 0;
        size_t pos = 0;

        ByteFile() = default;
        explicit ByteFile(const std::vector<uint8_t> &b) : data(b.data()), len(b.size()) {}
        ByteFile(const uint8_t *d, size_t n) : data(d), len(n) {}

        // file_auto_ptr/Is_Available: the corpus always "exists"; the
        // not-available shape is a VFS concern (the texture loader's
        // fallback to TGA), not a byte-shape.
        bool Is_Available() const { return true; }
        bool Open() { return true; }
        void Close() {}

        // No file-system stamp in a byte source (see the header comment).
        unsigned long Get_Date_Time() const { return 0; }

        unsigned Size() const { return (unsigned)len; }

        unsigned Read(void *buf, unsigned n)
        {
                size_t at = std::min(pos, len);
                size_t can = std::min<size_t>(n, len - at);
                if (can)
                        memcpy(buf, data + at, can);
                pos = at + can;
                return (unsigned)can;
        }

        // RawFileClass::Seek: plain lseek semantics — the position may
        // legally run past the end (a later read returns 0).
        int Seek(int to, int dir = 0 /*SEEK_SET*/)
        {
                (void)dir;
                if (to < 0)
                        to = 0;
                pos = (size_t)to;
                return (int)pos;
        }
};

// BSD strlcpy (the CRT helper the original calls; only its copy
// behaviour is observable through Name).
static void strlcpy_(char *dst, const char *src, size_t siz)
{
        size_t n = 0;
        while (n + 1 < siz && src[n] != 0)
        {
                dst[n] = src[n];
                n++;
        }
        dst[n] = 0;
}

// ----------------------------------------------------------------------------
// ddsfile.h — the legacy DX7 structures (packed as the file stores them)
// ----------------------------------------------------------------------------

struct LegacyDDCOLORKEY
{
        unsigned ColorSpaceLowValue;
        unsigned ColorSpaceHighValue;
};

struct LegacyDDSCAPS2
{
        unsigned Caps;
        unsigned Caps2;
        unsigned Caps3;
        unsigned Caps4;
};

struct LegacyDDPIXELFORMAT
{
        unsigned Size;
        unsigned Flags;
        unsigned FourCC;
        // The six union slots — the loader only reads FourCC; the dump
        // shows the first member of each union (the 32-bit C++ reads
        // the same bytes whichever union name it uses).
        unsigned RGBBitCount;
        unsigned RBitMask;
        unsigned GBitMask;
        unsigned BBitMask;
        unsigned RGBAlphaBitMask;
};

struct LegacyDDSURFACEDESC2
{
        unsigned Size;
        unsigned Flags;
        unsigned Height;
        unsigned Width;
        unsigned Pitch;         // union LinearSize
        unsigned Depth;         // union BackBufferCount
        unsigned MipMapCount;   // union RefreshRate
        unsigned AlphaBitDepth;
        unsigned Reserved;
        unsigned Surface;       // void* on the 32-bit build — placeholder
        LegacyDDCOLORKEY CKDestOverlay; // union EmptyFaceColor
        LegacyDDCOLORKEY CKDestBlt;
        LegacyDDCOLORKEY CKSrcOverlay;
        LegacyDDCOLORKEY CKSrcBlt;
        LegacyDDPIXELFORMAT PixelFormat;
        LegacyDDSCAPS2 Caps;
        unsigned TextureStage;
};

static_assert(sizeof(LegacyDDCOLORKEY) == 8, "LegacyDDCOLORKEY layout");
static_assert(sizeof(LegacyDDSCAPS2) == 16, "LegacyDDSCAPS2 layout");
static_assert(sizeof(LegacyDDPIXELFORMAT) == 32, "LegacyDDPIXELFORMAT layout");
static_assert(sizeof(LegacyDDSURFACEDESC2) == 124, "LegacyDDSURFACEDESC2 layout");

enum DDSType
{
        DDS_TEXTURE,
        DDS_CUBEMAP,
        DDS_VOLUME
};

// ----------------------------------------------------------------------------
// ww3dformat.h — the WW3DFormat enum (the subset the conversion table
// reaches; values are the enum ordinals)
// ----------------------------------------------------------------------------

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
        WW3D_FORMAT_U8V8,
        WW3D_FORMAT_L6V5U5,
        WW3D_FORMAT_X8L8V8U8,
        WW3D_FORMAT_DXT1,
        WW3D_FORMAT_DXT2,
        WW3D_FORMAT_DXT3,
        WW3D_FORMAT_DXT4,
        WW3D_FORMAT_DXT5,
        WW3D_FORMAT_COUNT
};

static const char *ww3d_format_name(WW3DFormat f)
{
        switch (f)
        {
        case WW3D_FORMAT_R8G8B8: return "r8g8b8";
        case WW3D_FORMAT_A8R8G8B8: return "a8r8g8b8";
        case WW3D_FORMAT_X8R8G8B8: return "x8r8g8b8";
        case WW3D_FORMAT_R5G6B5: return "r5g6b5";
        case WW3D_FORMAT_X1R5G5B5: return "x1r5g5b5";
        case WW3D_FORMAT_A1R5G5B5: return "a1r5g5b5";
        case WW3D_FORMAT_A4R4G4B4: return "a4r4g4b4";
        case WW3D_FORMAT_R3G3B2: return "r3g3b2";
        case WW3D_FORMAT_A8: return "a8";
        case WW3D_FORMAT_A8R3G3B2: return "a8r3g3b2";
        case WW3D_FORMAT_X4R4G4B4: return "x4r4g4b4";
        case WW3D_FORMAT_A8P8: return "a8p8";
        case WW3D_FORMAT_P8: return "p8";
        case WW3D_FORMAT_L8: return "l8";
        case WW3D_FORMAT_A8L8: return "a8l8";
        case WW3D_FORMAT_A4L4: return "a4l4";
        case WW3D_FORMAT_U8V8: return "u8v8";
        case WW3D_FORMAT_L6V5U5: return "l6v5u5";
        case WW3D_FORMAT_X8L8V8U8: return "x8l8v8u8";
        case WW3D_FORMAT_DXT1: return "dxt1";
        case WW3D_FORMAT_DXT2: return "dxt2";
        case WW3D_FORMAT_DXT3: return "dxt3";
        case WW3D_FORMAT_DXT4: return "dxt4";
        case WW3D_FORMAT_DXT5: return "dxt5";
        default: return "unknown";
        }
}

// ww3dformat.cpp — Get_Bytes_Per_Pixel (the WWASSERT default is a
// release no-op that falls through to the return 0).
static unsigned Get_Bytes_Per_Pixel(WW3DFormat format)
{
        switch (format)
        {
        case WW3D_FORMAT_A8R8G8B8:
        case WW3D_FORMAT_X8R8G8B8:
        case WW3D_FORMAT_X8L8V8U8:
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
        case WW3D_FORMAT_U8V8:
        case WW3D_FORMAT_L6V5U5:
                return 2;
        case WW3D_FORMAT_R3G3B2:
        case WW3D_FORMAT_A8:
        case WW3D_FORMAT_P8:
        case WW3D_FORMAT_L8:
        case WW3D_FORMAT_A4L4:
                return 1;
        default:
                // WWASSERT(0) — release: no-op
                break;
        }
        return 0;
}

// ----------------------------------------------------------------------------
// formconv.cpp — the D3DFMT → WW3DFormat conversion the FourCC field
// drives (D3DFMT values from the Windows DirectX 8 SDK's ddraw.h)
// ----------------------------------------------------------------------------

#define MAKEFOURCC(ch0, ch1, ch2, ch3) \
        ((unsigned)(char)(ch0) | ((unsigned)(char)(ch1) << 8) | \
         ((unsigned)(char)(ch2) << 16) | ((unsigned)(char)(ch3) << 24))

typedef unsigned D3DFORMAT;
#define D3DFMT_R8G8B8 20
#define D3DFMT_A8R8G8B8 21
#define D3DFMT_X8R8G8B8 22
#define D3DFMT_R5G6B5 23
#define D3DFMT_X1R5G5B5 24
#define D3DFMT_A1R5G5B5 25
#define D3DFMT_A4R4G4B4 26
#define D3DFMT_R3G3B2 27
#define D3DFMT_A8 28
#define D3DFMT_A8R3G3B2 29
#define D3DFMT_X4R4G4B4 30
#define D3DFMT_A8P8 40
#define D3DFMT_P8 41
#define D3DFMT_L8 50
#define D3DFMT_A8L8 51
#define D3DFMT_A4L4 52
#define D3DFMT_V8U8 60
#define D3DFMT_L6V5U5 61
#define D3DFMT_X8L8V8U8 62
#define D3DFMT_DXT1 MAKEFOURCC('D', 'X', 'T', '1')
#define D3DFMT_DXT2 MAKEFOURCC('D', 'X', 'T', '2')
#define D3DFMT_DXT3 MAKEFOURCC('D', 'X', 'T', '3')
#define D3DFMT_DXT4 MAKEFOURCC('D', 'X', 'T', '4')
#define D3DFMT_DXT5 MAKEFOURCC('D', 'X', 'T', '5')

#define HIGHEST_SUPPORTED_D3DFORMAT D3DFMT_X8L8V8U8

static WW3DFormat D3DFormatToWW3DFormatConversionArray[HIGHEST_SUPPORTED_D3DFORMAT + 1];

static void Init_D3D_To_WW3_Conversion()
{
        int i = 0;
        for (; i < HIGHEST_SUPPORTED_D3DFORMAT; ++i)
        {
                D3DFormatToWW3DFormatConversionArray[i] = WW3D_FORMAT_UNKNOWN;
        }

        D3DFormatToWW3DFormatConversionArray[D3DFMT_R8G8B8] = WW3D_FORMAT_R8G8B8;
        D3DFormatToWW3DFormatConversionArray[D3DFMT_A8R8G8B8] = WW3D_FORMAT_A8R8G8B8;
        D3DFormatToWW3DFormatConversionArray[D3DFMT_X8R8G8B8] = WW3D_FORMAT_X8R8G8B8;
        D3DFormatToWW3DFormatConversionArray[D3DFMT_R5G6B5] = WW3D_FORMAT_R5G6B5;
        D3DFormatToWW3DFormatConversionArray[D3DFMT_X1R5G5B5] = WW3D_FORMAT_X1R5G5B5;
        D3DFormatToWW3DFormatConversionArray[D3DFMT_A1R5G5B5] = WW3D_FORMAT_A1R5G5B5;
        D3DFormatToWW3DFormatConversionArray[D3DFMT_A4R4G4B4] = WW3D_FORMAT_A4R4G4B4;
        D3DFormatToWW3DFormatConversionArray[D3DFMT_R3G3B2] = WW3D_FORMAT_R3G3B2;
        D3DFormatToWW3DFormatConversionArray[D3DFMT_A8] = WW3D_FORMAT_A8;
        D3DFormatToWW3DFormatConversionArray[D3DFMT_A8R3G3B2] = WW3D_FORMAT_A8R3G3B2;
        D3DFormatToWW3DFormatConversionArray[D3DFMT_X4R4G4B4] = WW3D_FORMAT_X4R4G4B4;
        D3DFormatToWW3DFormatConversionArray[D3DFMT_A8P8] = WW3D_FORMAT_A8P8;
        D3DFormatToWW3DFormatConversionArray[D3DFMT_P8] = WW3D_FORMAT_P8;
        D3DFormatToWW3DFormatConversionArray[D3DFMT_L8] = WW3D_FORMAT_L8;
        D3DFormatToWW3DFormatConversionArray[D3DFMT_A8L8] = WW3D_FORMAT_A8L8;
        D3DFormatToWW3DFormatConversionArray[D3DFMT_A4L4] = WW3D_FORMAT_A4L4;
        D3DFormatToWW3DFormatConversionArray[D3DFMT_V8U8] = WW3D_FORMAT_U8V8;
        D3DFormatToWW3DFormatConversionArray[D3DFMT_L6V5U5] = WW3D_FORMAT_L6V5U5;
        D3DFormatToWW3DFormatConversionArray[D3DFMT_X8L8V8U8] = WW3D_FORMAT_X8L8V8U8;
}

static WW3DFormat D3DFormat_To_WW3DFormat(D3DFORMAT d3d_format)
{
        switch (d3d_format)
        {
        // The DXT-codes are created with FOURCC macro and thus can't be
        // placed in the conversion table
        case D3DFMT_DXT1: return WW3D_FORMAT_DXT1;
        case D3DFMT_DXT2: return WW3D_FORMAT_DXT2;
        case D3DFMT_DXT3: return WW3D_FORMAT_DXT3;
        case D3DFMT_DXT4: return WW3D_FORMAT_DXT4;
        case D3DFMT_DXT5: return WW3D_FORMAT_DXT5;
        default:
                if (d3d_format > HIGHEST_SUPPORTED_D3DFORMAT)
                {
                        return WW3D_FORMAT_UNKNOWN;
                }
                else
                {
                        return D3DFormatToWW3DFormatConversionArray[(unsigned int)d3d_format];
                }
        }
}

// ddraw.h — the caps2 flags the type detection reads.
#define DDSCAPS2_CUBEMAP 0x00000200L
#define DDSCAPS2_VOLUME 0x00200000L

// ----------------------------------------------------------------------------
// colorspace.h — the HSV recolor stack (and its Vector3/Vector4 stand-ins)
// ----------------------------------------------------------------------------

struct Vector3
{
        float X, Y, Z;
        Vector3() : X(0), Y(0), Z(0) {}
        Vector3(float x, float y, float z) : X(x), Y(y), Z(z) {}
        void Set(float x, float y, float z) { X = x; Y = y; Z = z; }
        float &operator[](int i) { return (&X)[i]; }
        float operator[](int i) const { return (&X)[i]; }
        Vector3 operator+=(const Vector3 &o) { X += o.X; Y += o.Y; Z += o.Z; return *this; }
};

struct Vector4
{
        float X, Y, Z, W;
};

// WWMath::Max/Min/Clamp/Floorf — IEEE-exact scalar helpers (ADR-001:
// floor is exact on every conforming platform; no pin linking needed).
static float WW_Max(float a, float b) { return (a >= b) ? a : b; }
static float WW_Min(float a, float b) { return (a < b) ? a : b; }
static float WW_Clamp(float val, float lo, float hi)
{
        if (val < lo) return lo;
        if (val > hi) return hi;
        return val;
}
static float WW_Floorf(float x) { return std::floor(x); }

// modified from Foley et al. page 592
// converts rgb[0..1] to h [0,360), s and v in [0,1]
// negative h values are to signify undefined
static void RGB_To_HSV(Vector3 &hsv, const Vector3 &rgb)
{
        float max = WW_Max(rgb.X, rgb.Y);
        max = WW_Max(max, rgb.Z);
        float min = WW_Min(rgb.X, rgb.Y);
        min = WW_Min(min, rgb.Z);

        // value
        hsv.Z = max;

        // saturation
        hsv.Y = (max != 0.0f) ? ((max - min) / max) : 0.0f;
        if (hsv.Y == 0.0f)
                hsv.X = -1.0f;
        else
        {
                float delta = max - min;
                if (rgb.X == max)
                        hsv.X = (rgb.Y - rgb.Z) / delta;
                else if (rgb.Y == max)
                        hsv.X = 2.0f + (rgb.Z - rgb.X) / delta;
                else if (rgb.Z == max)
                        hsv.X = 4.0f + (rgb.X - rgb.Y) / delta;
                hsv.X *= 60.0f;
                if (hsv.X < 0.0f) hsv.X += 360.0f;
        }
}

static void HSV_To_RGB(Vector3 &rgb, const Vector3 &hsv)
{
        float h = hsv.X;
        float s = hsv.Y;
        float v = hsv.Z;

        if (hsv.Y == 0.0f)
        {
                rgb.Set(v, v, v);
        }
        else
        {
                float f, p, q, t;

                int i;

                if (h == 360.0f) h = 0.0f;

                h /= 60.0f;
                i = (int)WW_Floorf(h);
                f = h - i;
                p = v * (1.0f - s);
                q = v * (1.0f - (s * f));
                t = v * (1.0f - (s * (1.0f - f)));
                switch (i)
                {
                case 0:
                        rgb.Set(v, t, p);
                        break;
                case 1:
                        rgb.Set(q, v, p);
                        break;
                case 2:
                        rgb.Set(p, v, t);
                        break;
                case 3:
                        rgb.Set(p, q, v);
                        break;
                case 4:
                        rgb.Set(t, p, v);
                        break;
                case 5:
                        rgb.Set(v, p, q);
                        break;
                }
        }
}

static void Recolor(Vector3 &rgb, const Vector3 &hsv_shift)
{
        Vector3 hsv;
        RGB_To_HSV(hsv, rgb);

        // If the Hue has the "undefined flag" (a negative value), this means that the color is pure
        // monochrome. In this case do not shift the hue (it is undefined) or the saturation (it is 0
        // so it cannot be decreased, and increasing it would cause the undefined hue to actually
        // become visible). In this case, we only modify the value.
        if (hsv.X < 0.0f)
                hsv += Vector3(0.0f, 0.0f, hsv_shift.Z);
        else
                hsv += hsv_shift;

        // angular mod
        if (hsv.X < 0.0f) hsv.X += 360.0f;
        if (hsv.X > 360.0f) hsv.X -= 360.0f;
        // clamp saturation and value
        hsv.Y = WW_Clamp(hsv.Y, 0.0f, 1.0f);
        hsv.Z = WW_Clamp(hsv.Z, 0.0f, 1.0f);
        HSV_To_RGB(rgb, hsv);
}

// dx8wrapper.h — DX8Wrapper::Convert_Color(unsigned)
static Vector4 Convert_Color(unsigned color)
{
        Vector4 col;
        col.W = ((color & 0xff000000) >> 24) / 255.0f;
        col.X = ((color & 0xff0000) >> 16) / 255.0f;
        col.Y = ((color & 0xff00) >> 8) / 255.0f;
        col.Z = ((color & 0xff) >> 0) / 255.0f;
        return col;
}

// vector3.h — Vector3::Convert_To_ARGB(float alpha)
static unsigned long Convert_To_ARGB(const Vector3 &color, float alpha)
{
        return (unsigned(alpha * 255) << 24) |
               (unsigned(color.X * 255.0f) << 16) |
               (unsigned(color.Y * 255.0f) << 8) |
               (unsigned(color.Z * 255.0f));
}

// dx8wrapper.h — DX8Wrapper::Convert_Color(const Vector3&, float) is
// Convert_To_ARGB on every compiler the game builds with (the inline
// asm path is MSVC < 1300 only).
static unsigned Convert_Color(const Vector3 &color, float alpha)
{
        return Convert_To_ARGB(color, alpha);
}

// colorspace.h — Recolor(unsigned&, const Vector3&)
static void Recolor(unsigned &rgba, const Vector3 &hsv_shift)
{
        Vector4 rgba_v = Convert_Color(rgba);
        Vector3 rgb(rgba_v.X, rgba_v.Y, rgba_v.Z);
        Recolor(rgb, hsv_shift);
        rgba = Convert_Color(rgb, rgba_v.W);
}

// ----------------------------------------------------------------------------
// bitmaphandler.h — Write_B8G8R8A8 (the pixel writer Get_4x4_Block
// drives; Bitmap_Assert is a release no-op)
// ----------------------------------------------------------------------------

static void Bitmap_Assert(bool condition)
{
        (void)condition; // release build: no-op
}

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
                // byte is 0x00 (see the file comment).
                *(unsigned short *)dest_ptr = tmp;
        }
        break;
        case WW3D_FORMAT_L8:
        {
                // CIE Req. 709: Y709 = 0.2125R + 0.7154G + 0.0721B
                unsigned char tmp = (unsigned char)(
                    (
                        ((unsigned int)argb[0] * (unsigned int)0x1275) + // 0.0721B
                        ((unsigned int)argb[1] * (unsigned int)0xB725) + // 0.7154G (rounded up so FF, FF, FF becomes FF)
                        ((unsigned int)argb[2] * (unsigned int)0x3666)   // 0.2125R
                        ) >>
                    16);
                *dest_ptr++ = tmp;
        }
        break;
        case WW3D_FORMAT_A8:
        {
                *dest_ptr++ = *argb++;
        }
        break;
        case WW3D_FORMAT_DXT1:
        case WW3D_FORMAT_DXT2:
        case WW3D_FORMAT_DXT3:
        case WW3D_FORMAT_DXT4:
        case WW3D_FORMAT_DXT5:
        case WW3D_FORMAT_P8: // Paletted destination not supported
        default:
                Bitmap_Assert(0);
                break;
        }
}

static void Write_B8G8R8A8(unsigned char *dest_ptr, WW3DFormat dest_format, const unsigned &argb)
{
        Write_B8G8R8A8(dest_ptr, dest_format, (const unsigned char *)&argb);
}

// ----------------------------------------------------------------------------
// ddsfile.cpp — the DDSFileClass transcription
// ----------------------------------------------------------------------------

class DDSFileClass
{
        unsigned Width;
        unsigned Height;
        unsigned Depth;
        unsigned FullWidth;
        unsigned FullHeight;
        unsigned FullDepth;
        unsigned MipLevels;
        unsigned long DateTime;
        unsigned ReductionFactor;
        unsigned char *DDSMemory;
        unsigned DDSMemorySize; // transcription-only: the byte source's
                                // read length (the original relies on its
                                // allocation size; the reads are always
                                // full — see Load)
        WW3DFormat Format;
        DDSType Type;
        unsigned *LevelSizes;
        unsigned *LevelOffsets;
        unsigned CubeFaceSize;
        LegacyDDSURFACEDESC2 SurfaceDesc;
        char Name[256];

        static unsigned Calculate_DXTC_Surface_Size(unsigned width, unsigned height, WW3DFormat format);

        // Transcription-only: the dump driver reads the private state
        // (Width/Height/Depth before the Get_* clamps, the mutated
        // ReductionFactor, the level arrays, DDSMemory). The original
        // class has no such friend; the dump needs it.
        friend struct DdsDump;

public:
        // Transcription: the constructor parses from the byte source the
        // caller would have opened under the munged name.
        DDSFileClass(const char *name, unsigned reduction_factor, const ByteFile &file);
        ~DDSFileClass();

        unsigned Get_Width(unsigned level) const;
        unsigned Get_Height(unsigned level) const;
        unsigned Get_Depth(unsigned level) const;
        unsigned Get_Full_Width() const { return FullWidth; }
        unsigned Get_Full_Height() const { return FullHeight; }
        unsigned Get_Full_Depth() const { return FullDepth; }
        unsigned long Get_Date_Time() const { return DateTime; }
        unsigned Get_Mip_Level_Count() const { return MipLevels; }
        const unsigned char *Get_Memory_Pointer(unsigned level) const;
        unsigned Get_Level_Size(unsigned level) const;
        WW3DFormat Get_Format() const { return Format; }
        DDSType Get_Type() const { return Type; }
        unsigned Get_CubeFaceSize() const { return CubeFaceSize; }
        unsigned Get_ReductionFactor() const { return ReductionFactor; }

        void Copy_Level_To_Surface(
            unsigned level,
            WW3DFormat dest_format,
            unsigned dest_width,
            unsigned dest_height,
            unsigned char *dest_surface,
            unsigned dest_pitch,
            const Vector3 &hsv_shift);

        const unsigned char *Get_CubeMap_Memory_Pointer(unsigned face, unsigned level) const;
        void Copy_CubeMap_Level_To_Surface(
            unsigned face,
            unsigned level,
            WW3DFormat dest_format,
            unsigned dest_width,
            unsigned dest_height,
            unsigned char *dest_surface,
            unsigned dest_pitch,
            const Vector3 &hsv_shift);

        const unsigned char *Get_Volume_Memory_Pointer(unsigned level) const;
        void Copy_Volume_Level_To_Surface(
            unsigned level,
            unsigned depth,
            WW3DFormat dest_format,
            unsigned dest_width,
            unsigned dest_height,
            unsigned char *dest_surface,
            unsigned row_pitch,
            unsigned slice_pitch,
            const Vector3 &hsv_shift);

        unsigned Get_Pixel(unsigned level, unsigned x, unsigned y) const;
        bool Get_4x4_Block(
            unsigned char *dest_ptr,
            unsigned dest_pitch,
            WW3DFormat dest_format,
            unsigned level,
            unsigned source_x,
            unsigned source_y,
            const Vector3 &hsv_shift) const;

        bool Load(const ByteFile &file);
        bool Is_Available() const { return !!LevelSizes; }
};

// ----------------------------------------------------------------------------

DDSFileClass::DDSFileClass(const char *name, unsigned reduction_factor, const ByteFile &file)
    // The original's initializer list names these out of declaration
    // order (MSVC accepts); reordered to the declaration order so the
    // transcription compiles clean — every value is a constant, so the
    // order carries no behaviour.
    : Width(0),
      Height(0),
      Depth(0),
      FullWidth(0),
      FullHeight(0),
      FullDepth(0),
      MipLevels(0),
      DateTime(0),
      ReductionFactor(reduction_factor),
      DDSMemory(nullptr),
      DDSMemorySize(0),
      Format(WW3D_FORMAT_UNKNOWN),
      Type(DDS_TEXTURE),
      LevelSizes(nullptr),
      LevelOffsets(nullptr),
      CubeFaceSize(0)
{
        strlcpy_(Name, name, sizeof(Name));
        // The name could be given in .tga or .dds format, so ensure we're opening .dds...
        int len = (int)strlen(Name);
        // The transcription refuses the len < 3 shapes the original's
        // negative-index writes would scribble (see the file comment).
        if (len >= 3)
        {
                Name[len - 3] = 'd';
                Name[len - 2] = 'd';
                Name[len - 1] = 's';
        }

        ByteFile f = file; // file_auto_ptr file(_TheFileFactory,Name);
        if (!f.Is_Available())
        {
                return;
        }

        int result = f.Open();
        if (!result)
        {
                // WWASSERT("File would not open")
                return;
        }

        DateTime = f.Get_Date_Time();
        char header[4];

        unsigned read_bytes = f.Read(header, 4);
        if (!read_bytes)
        {
                // WWASSERT("File loading failed trying to read header")
                return;
        }
        // Now, we read DDSURFACEDESC2 defining the compressed data
        read_bytes = f.Read(&SurfaceDesc, sizeof(LegacyDDSURFACEDESC2));
        // Verify the structure size matches the read size
        if (read_bytes == 0 || read_bytes != SurfaceDesc.Size)
        {
                // WWASSERT_PRINT(0,tmp)
                return;
        }
        // The Size < 124 pass shape reads uninitialised tail bytes in the
        // original — refused identically on both sides (see the file
        // comment); the transcription's caller never builds one.
        if (SurfaceDesc.Size != sizeof(LegacyDDSURFACEDESC2))
        {
                return;
        }

        Format = D3DFormat_To_WW3DFormat((D3DFORMAT)SurfaceDesc.PixelFormat.FourCC);
        // WWASSERT(DXT1..DXT5) — release: no-op, the load continues with
        // whatever the conversion produced (including UNKNOWN).

        MipLevels = SurfaceDesc.MipMapCount;
        if (MipLevels == 0) MipLevels = 1;

        if (MipLevels > ReductionFactor) MipLevels -= ReductionFactor;
        else
        {
                MipLevels = 1;
                ReductionFactor = ReductionFactor - MipLevels;
        }

        // Drop the two lowest miplevels!
        if (MipLevels > 2) MipLevels -= 2;
        else MipLevels = 1;

        // check texture type, normal, cube or volume
        if (SurfaceDesc.Caps.Caps2 & DDSCAPS2_CUBEMAP)
        {
                Type = DDS_CUBEMAP;
        }
        else if (SurfaceDesc.Caps.Caps2 & DDSCAPS2_VOLUME)
        {
                Type = DDS_VOLUME;
        }

        FullWidth = SurfaceDesc.Width;
        FullHeight = SurfaceDesc.Height;
        FullDepth = SurfaceDesc.Depth;
        Width = SurfaceDesc.Width >> ReductionFactor;
        Height = SurfaceDesc.Height >> ReductionFactor;
        Depth = SurfaceDesc.Depth;

        unsigned level_size = Calculate_DXTC_Surface_Size(
            SurfaceDesc.Width,
            SurfaceDesc.Height,
            Format);

        unsigned level_offset = 0;

        unsigned level_mip_dec = 4;
        if (Type == DDS_VOLUME)
        {
                // add slices to level data size
                level_size *= SurfaceDesc.Depth;
                level_mip_dec = 8;
        }

        LevelSizes = new unsigned[MipLevels];
        LevelOffsets = new unsigned[MipLevels];
        unsigned level = 0;
        for (; level < ReductionFactor; ++level)
        {
                if (level_size > 16)
                {
                        // If surface is bigger than one block (8 or 16 bytes)...
                        level_size /= level_mip_dec;
                }
        }
        for (level = 0; level < MipLevels; ++level)
        {
                LevelSizes[level] = level_size;
                LevelOffsets[level] = level_offset;
                level_offset += level_size;
                if (level_size > 16)
                {
                        // If surface is bigger than one block (8 or 16 bytes)...
                        level_size /= level_mip_dec;
                }
        }

        if (Type == DDS_CUBEMAP)
        {
                for (level = 0; level < MipLevels; ++level)
                {
                        CubeFaceSize += LevelSizes[level];
                }

                // this accounts for dropping 2 lowest mip levels
                if (MipLevels > 2)
                        CubeFaceSize += 16;
        }
        f.Close();
}

// ----------------------------------------------------------------------------

DDSFileClass::~DDSFileClass()
{
        delete[] DDSMemory;
        delete[] LevelSizes;
        delete[] LevelOffsets;
}

unsigned DDSFileClass::Get_Width(unsigned level) const
{
        // WWASSERT(level<MipLevels) — release: no-op (the corpus only
        // queries in-range levels; the port errors)
        unsigned width = Width >> level;
        if (width < 4) width = 4;
        return width;
}

unsigned DDSFileClass::Get_Height(unsigned level) const
{
        unsigned height = Height >> level;
        if (height < 4) height = 4;
        return height;
}

unsigned DDSFileClass::Get_Depth(unsigned level) const
{
        unsigned depth = Depth >> level;
        if (depth < 4) depth = 4;
        return depth;
}

const unsigned char *DDSFileClass::Get_Memory_Pointer(unsigned level) const
{
        return DDSMemory + LevelOffsets[level];
}

unsigned DDSFileClass::Get_Level_Size(unsigned level) const
{
        return LevelSizes[level];
}

// For some reason DX-Tex tool doesn't fill the surface size field, so we need to calculate it...
unsigned DDSFileClass::Calculate_DXTC_Surface_Size(
    unsigned width,
    unsigned height,
    WW3DFormat format)
{
        unsigned level_size = (width / 4) * (height / 4);
        switch (format)
        {
        case WW3D_FORMAT_DXT1:
                level_size *= 8;
                break;
        case WW3D_FORMAT_DXT2:
        case WW3D_FORMAT_DXT3:
        case WW3D_FORMAT_DXT4:
        case WW3D_FORMAT_DXT5:
                level_size *= 16;
                break;
        default:
                break; // not in the original; the switch falls through the
                       // same way for every non-DXT format
        }

        return level_size;
}

// ----------------------------------------------------------------------------

bool DDSFileClass::Load(const ByteFile &file)
{
        if (DDSMemory) return false;
        if (!LevelSizes || !LevelOffsets) return false;

        ByteFile f = file; // file_auto_ptr file(_TheFileFactory,Name)
        if (!f.Is_Available())
        {
                return false;
        }

        f.Open();
        // Data size is file size minus the header and info block
        unsigned size = f.Size() - SurfaceDesc.Size - 4;

        if (!size)
        {
                return false;
        }

        // Skip mip levels if reduction factor is not zero
        unsigned level_size = Calculate_DXTC_Surface_Size(
            SurfaceDesc.Width,
            SurfaceDesc.Height,
            Format);

        unsigned skipped_offset = 0;
        for (unsigned i = 0; i < ReductionFactor; ++i)
        {
                skipped_offset += level_size;
                size -= level_size;
                if (level_size > 16)
                {
                        // If surface is bigger than one block (8 or 16 bytes)...
                        level_size /= 4;
                }
        }

        // Skip the header and info block and possible unused mip levels
        unsigned seek_size = f.Seek(SurfaceDesc.Size + 4 + skipped_offset);
        (void)seek_size;
        // WWASSERT(seek_size==(SurfaceDesc.Size+4+skipped_offset)) — the
        // plain seek always lands where asked

        if (size && size < 0x80000000)
        {
                // Allocate memory for the data excluding the headers
                DDSMemory = new unsigned char[size];
                // Read data
                unsigned read_size = f.Read(DDSMemory, size);
                (void)read_size;
                DDSMemorySize = read_size;
                // WWASSERT(read_size==size) — the arithmetic above always
                // leaves exactly `size` bytes at the seek target
        }
        f.Close();
        return true;
}

// ----------------------------------------------------------------------------

static unsigned RGB565_To_ARGB8888(unsigned short rgb)
{
        unsigned rgba = 0;
        rgba |= unsigned(rgb & 0x001f) << 3;
        rgba |= unsigned(rgb & 0x07e0) << 5;
        rgba |= unsigned(rgb & 0xf800) << 8;
        return rgba;
}

static unsigned short ARGB8888_To_RGB565(unsigned argb_)
{
        unsigned char *argb = (unsigned char *)&argb_;
        unsigned short rgb;
        rgb = ((argb[2]) & 0xf8) << 8;
        rgb |= ((argb[1]) & 0xfc) << 3;
        rgb |= ((argb[0]) & 0xf8) >> 3;
        return rgb;
}

// ----------------------------------------------------------------------------

void DDSFileClass::Copy_Level_To_Surface(
    unsigned level,
    WW3DFormat dest_format,
    unsigned dest_width,
    unsigned dest_height,
    unsigned char *dest_surface,
    unsigned dest_pitch,
    const Vector3 &hsv_shift)
{
        // WWASSERT(DDSMemory) / WWASSERT(dest_surface) — release: no-ops

        if (!DDSMemory || !Get_Memory_Pointer(level))
        {
                // WWASSERT_PRINT — release: nothing
                return;
        }

        // If the format and size is a match just copy the contents
        bool has_hsv_shift = hsv_shift[0] != 0.0f || hsv_shift[1] != 0.0f || hsv_shift[2] != 0.0f;
        if (dest_format == Format && dest_width == Get_Width(level) && dest_height == Get_Height(level))
        {
                // If hue shift, we can't just copy...
                if (has_hsv_shift)
                {
                        if (Format == WW3D_FORMAT_DXT1)
                        {
                                const unsigned *src_ptr = (const unsigned *)Get_Memory_Pointer(level);
                                unsigned *dest_ptr = (unsigned *)dest_surface;
                                for (unsigned y = 0; y < dest_height; y += 4)
                                {
                                        for (unsigned x = 0; x < dest_width; x += 4)
                                        {
                                                unsigned cols = *src_ptr++; // Bytes 1-4 of color block
                                                unsigned col0 = RGB565_To_ARGB8888((unsigned short)(cols >> 16));
                                                unsigned col1 = RGB565_To_ARGB8888((unsigned short)(cols & 0xffff));
                                                Recolor(col0, hsv_shift);
                                                Recolor(col1, hsv_shift);
                                                col0 = ARGB8888_To_RGB565(col0);
                                                col1 = ARGB8888_To_RGB565(col1);
                                                cols = unsigned(col0) << 16 | col1;
                                                *dest_ptr++ = cols;

                                                *dest_ptr++ = *src_ptr++; // Bytes 5-8 of color block
                                        }
                                }
                        }
                        else if (Format == WW3D_FORMAT_DXT5)
                        {
                                const unsigned *src_ptr = (const unsigned *)Get_Memory_Pointer(level);
                                unsigned *dest_ptr = (unsigned *)dest_surface;
                                for (unsigned y = 0; y < dest_height; y += 4)
                                {
                                        for (unsigned x = 0; x < dest_width; x += 4)
                                        {
                                                *dest_ptr++ = *src_ptr++; // Bytes 1-4 of alpha block
                                                *dest_ptr++ = *src_ptr++; // Bytes 5-8 of alpha block
                                                unsigned cols = *src_ptr++; // Bytes 1-4 of color block
                                                unsigned col0 = RGB565_To_ARGB8888((unsigned short)(cols >> 16));
                                                unsigned col1 = RGB565_To_ARGB8888((unsigned short)(cols & 0xffff));
                                                Recolor(col0, hsv_shift);
                                                Recolor(col1, hsv_shift);
                                                col0 = ARGB8888_To_RGB565(col0);
                                                col1 = ARGB8888_To_RGB565(col1);
                                                cols = unsigned(col0) << 16 | col1;
                                                *dest_ptr++ = cols;

                                                *dest_ptr++ = *src_ptr++; // Bytes 5-8 of color block
                                        }
                                }
                        }
                        else
                        {
                                // WWASSERT(0) — release: nothing (the destination is left untouched)
                        }
                }
                else
                {
                        unsigned compressed_size = Get_Level_Size(level);
                        memcpy(dest_surface, Get_Memory_Pointer(level), compressed_size);
                }
        }
        else
        {
                // If size matches, copy each pixel linearly with color space conversion
                if (dest_width == Get_Width(level) && dest_height == Get_Height(level))
                {
                        // An exception here - if the source format is DXT1 and the destination
                        // is DXT2, just copy the contents and create an empty alpha channel.
                        // This is needed on NVidia cards that have problems with DXT1 compression.
                        if (Format == WW3D_FORMAT_DXT1 && dest_format == WW3D_FORMAT_DXT2)
                        {
                                // If hue shift, we can't just copy...
                                if (has_hsv_shift)
                                {
                                        const unsigned *src_ptr = (const unsigned *)Get_Memory_Pointer(level);
                                        unsigned *dest_ptr = (unsigned *)dest_surface;
                                        for (unsigned y = 0; y < dest_height; y += 4)
                                        {
                                                for (unsigned x = 0; x < dest_width; x += 4)
                                                {
                                                        *dest_ptr++ = 0xffffffff; // Bytes 1-4 of alpha block
                                                        *dest_ptr++ = 0xffffffff; // Bytes 5-8 of alpha block

                                                        unsigned cols = *src_ptr++; // Bytes 1-4 of color block
                                                        unsigned col0 = RGB565_To_ARGB8888((unsigned short)(cols >> 16));
                                                        unsigned col1 = RGB565_To_ARGB8888((unsigned short)(cols & 0xffff));
                                                        Recolor(col0, hsv_shift);
                                                        Recolor(col1, hsv_shift);
                                                        col0 = ARGB8888_To_RGB565(col0);
                                                        col1 = ARGB8888_To_RGB565(col1);
                                                        cols = unsigned(col0) << 16 | col1;
                                                        *dest_ptr++ = cols;

                                                        *dest_ptr++ = *src_ptr++; // Bytes 5-8 of color block
                                                }
                                        }
                                }
                                else
                                {
                                        const unsigned *src_ptr = (const unsigned *)Get_Memory_Pointer(level);
                                        unsigned *dest_ptr = (unsigned *)dest_surface;
                                        for (unsigned y = 0; y < dest_height; y += 4)
                                        {
                                                for (unsigned x = 0; x < dest_width; x += 4)
                                                {
                                                        *dest_ptr++ = 0xffffffff; // Bytes 1-4 of alpha block
                                                        *dest_ptr++ = 0xffffffff; // Bytes 5-8 of alpha block
                                                        *dest_ptr++ = *src_ptr++; // Bytes 1-4 of color block
                                                        *dest_ptr++ = *src_ptr++; // Bytes 5-8 of color block
                                                }
                                        }
                                }
                        }
                        else
                        {
                                unsigned dest_bpp = Get_Bytes_Per_Pixel(dest_format);

                                // Copy 4x4 block at a time
                                bool contains_alpha = false;
                                for (unsigned y = 0; y < dest_height; y += 4)
                                {
                                        unsigned char *dest_ptr = dest_surface;
                                        dest_ptr += y * dest_pitch;
                                        for (unsigned x = 0; x < dest_width; x += 4, dest_ptr += dest_bpp * 4)
                                        {
                                                contains_alpha |= Get_4x4_Block(dest_ptr, dest_pitch, dest_format, level, x, y, hsv_shift);
                                        }
                                }
                                if (Format == WW3D_FORMAT_DXT1 && contains_alpha)
                                {
                                        // WWDEBUG_SAY — release: nothing
                                }
                        }
                }
        }
}

// cube map
const unsigned char *DDSFileClass::Get_CubeMap_Memory_Pointer(
    unsigned int face,
    unsigned int level) const
{
        return &DDSMemory[CubeFaceSize * face + LevelOffsets[level]];
}

void DDSFileClass::Copy_CubeMap_Level_To_Surface(
    unsigned face,
    unsigned level,
    WW3DFormat dest_format,
    unsigned dest_width,
    unsigned dest_height,
    unsigned char *dest_surface,
    unsigned dest_pitch,
    const Vector3 &hsv_shift)
{
        // WWASSERT(DDSMemory) / WWASSERT(dest_surface) — release: no-ops

        if (!DDSMemory || !Get_CubeMap_Memory_Pointer(face, level))
        {
                // WWASSERT_PRINT — release: nothing
                return;
        }

        // If the format and size is a match just copy the contents
        bool has_hsv_shift = hsv_shift[0] != 0.0f || hsv_shift[1] != 0.0f || hsv_shift[2] != 0.0f;
        if (dest_format == Format && dest_width == Get_Width(level) && dest_height == Get_Height(level))
        {
                // If hue shift, we can't just copy...
                if (has_hsv_shift)
                {
                        if (Format == WW3D_FORMAT_DXT1)
                        {
                                const unsigned *src_ptr = (const unsigned *)Get_CubeMap_Memory_Pointer(face, level);
                                unsigned *dest_ptr = (unsigned *)dest_surface;
                                for (unsigned y = 0; y < dest_height; y += 4)
                                {
                                        for (unsigned x = 0; x < dest_width; x += 4)
                                        {
                                                unsigned cols = *src_ptr++; // Bytes 1-4 of color block
                                                unsigned col0 = RGB565_To_ARGB8888((unsigned short)(cols >> 16));
                                                unsigned col1 = RGB565_To_ARGB8888((unsigned short)(cols & 0xffff));
                                                Recolor(col0, hsv_shift);
                                                Recolor(col1, hsv_shift);
                                                col0 = ARGB8888_To_RGB565(col0);
                                                col1 = ARGB8888_To_RGB565(col1);
                                                cols = unsigned(col0) << 16 | col1;
                                                *dest_ptr++ = cols;

                                                *dest_ptr++ = *src_ptr++; // Bytes 5-8 of color block
                                        }
                                }
                        }
                        else if (Format == WW3D_FORMAT_DXT5)
                        {
                                const unsigned *src_ptr = (const unsigned *)Get_CubeMap_Memory_Pointer(face, level);
                                unsigned *dest_ptr = (unsigned *)dest_surface;
                                for (unsigned y = 0; y < dest_height; y += 4)
                                {
                                        for (unsigned x = 0; x < dest_width; x += 4)
                                        {
                                                *dest_ptr++ = *src_ptr++; // Bytes 1-4 of alpha block
                                                *dest_ptr++ = *src_ptr++; // Bytes 5-8 of alpha block
                                                unsigned cols = *src_ptr++; // Bytes 1-4 of color block
                                                unsigned col0 = RGB565_To_ARGB8888((unsigned short)(cols >> 16));
                                                unsigned col1 = RGB565_To_ARGB8888((unsigned short)(cols & 0xffff));
                                                Recolor(col0, hsv_shift);
                                                Recolor(col1, hsv_shift);
                                                col0 = ARGB8888_To_RGB565(col0);
                                                col1 = ARGB8888_To_RGB565(col1);
                                                cols = unsigned(col0) << 16 | col1;
                                                *dest_ptr++ = cols;

                                                *dest_ptr++ = *src_ptr++; // Bytes 5-8 of color block
                                        }
                                }
                        }
                        else
                        {
                                // WWASSERT(0) — release: nothing
                        }
                }
                else
                {
                        unsigned compressed_size = Get_Level_Size(level);
                        memcpy(dest_surface, Get_CubeMap_Memory_Pointer(face, level), compressed_size);
                }
        }
        else
        {
                // If size matches, copy each pixel linearly with color space conversion
                if (dest_width == Get_Width(level) && dest_height == Get_Height(level))
                {
                        // An exception here - if the source format is DXT1 and the destination
                        // is DXT2, just copy the contents and create an empty alpha channel.
                        // This is needed on NVidia cards that have problems with DXT1 compression.
                        if (Format == WW3D_FORMAT_DXT1 && dest_format == WW3D_FORMAT_DXT2)
                        {
                                // If hue shift, we can't just copy...
                                if (has_hsv_shift)
                                {
                                        const unsigned *src_ptr = (const unsigned *)Get_CubeMap_Memory_Pointer(face, level);
                                        unsigned *dest_ptr = (unsigned *)dest_surface;
                                        for (unsigned y = 0; y < dest_height; y += 4)
                                        {
                                                for (unsigned x = 0; x < dest_width; x += 4)
                                                {
                                                        *dest_ptr++ = 0xffffffff; // Bytes 1-4 of alpha block
                                                        *dest_ptr++ = 0xffffffff; // Bytes 5-8 of alpha block

                                                        unsigned cols = *src_ptr++; // Bytes 1-4 of color block
                                                        unsigned col0 = RGB565_To_ARGB8888((unsigned short)(cols >> 16));
                                                        unsigned col1 = RGB565_To_ARGB8888((unsigned short)(cols & 0xffff));
                                                        Recolor(col0, hsv_shift);
                                                        Recolor(col1, hsv_shift);
                                                        col0 = ARGB8888_To_RGB565(col0);
                                                        col1 = ARGB8888_To_RGB565(col1);
                                                        cols = unsigned(col0) << 16 | col1;
                                                        *dest_ptr++ = cols;

                                                        *dest_ptr++ = *src_ptr++; // Bytes 5-8 of color block
                                                }
                                        }
                                }
                                else
                                {
                                        const unsigned *src_ptr = (const unsigned *)Get_CubeMap_Memory_Pointer(face, level);
                                        unsigned *dest_ptr = (unsigned *)dest_surface;
                                        for (unsigned y = 0; y < dest_height; y += 4)
                                        {
                                                for (unsigned x = 0; x < dest_width; x += 4)
                                                {
                                                        *dest_ptr++ = 0xffffffff; // Bytes 1-4 of alpha block
                                                        *dest_ptr++ = 0xffffffff; // Bytes 5-8 of alpha block
                                                        *dest_ptr++ = *src_ptr++; // Bytes 1-4 of color block
                                                        *dest_ptr++ = *src_ptr++; // Bytes 5-8 of color block
                                                }
                                        }
                                }
                        }
                        else
                        {
                                unsigned dest_bpp = Get_Bytes_Per_Pixel(dest_format);

                                // Copy 4x4 block at a time
                                bool contains_alpha = false;
                                for (unsigned y = 0; y < dest_height; y += 4)
                                {
                                        unsigned char *dest_ptr = dest_surface;
                                        dest_ptr += y * dest_pitch;
                                        for (unsigned x = 0; x < dest_width; x += 4, dest_ptr += dest_bpp * 4)
                                        {
                                                contains_alpha |= Get_4x4_Block(dest_ptr, dest_pitch, dest_format, level, x, y, hsv_shift);
                                        }
                                }
                                if (Format == WW3D_FORMAT_DXT1 && contains_alpha)
                                {
                                        // WWDEBUG_SAY — release: nothing
                                }
                        }
                }
        }
}

// volume texture copy
const unsigned char *DDSFileClass::Get_Volume_Memory_Pointer(unsigned int level) const
{
        (void)level;
        return nullptr; // DDSMemory[ — the original's dead stub
}

void DDSFileClass::Copy_Volume_Level_To_Surface(
    unsigned level,
    unsigned depth,
    WW3DFormat dest_format,
    unsigned dest_width,
    unsigned dest_height,
    unsigned char *dest_surface,
    unsigned row_pitch,
    unsigned slice_pitch,
    const Vector3 &hsv_shift)
{
        (void)depth;
        (void)dest_format;
        (void)dest_width;
        (void)dest_height;
        (void)dest_surface;
        (void)row_pitch;
        (void)slice_pitch;
        (void)hsv_shift;
        (void)level;

        // WWASSERT(DDSMemory) / WWASSERT(dest_surface) — release: no-ops

        if (!DDSMemory || !Get_Volume_Memory_Pointer(level))
        {
                // WWASSERT_PRINT — release: nothing; the null volume
                // pointer always takes this path
                return;
        }

        // Unreachable in the original: Get_Volume_Memory_Pointer is a
        // null stub, so the body below never executes. Transcribed for
        // documentation only.
}

// ----------------------------------------------------------------------------

static unsigned Combine_Colors(unsigned col1, unsigned col2, unsigned rel)
{
        const unsigned R_B_MASK = 0x00ff00ff;
        const unsigned G_MASK = 0x0000ff00;

        unsigned rel2 = 255 - rel;

        unsigned r_b_col1 = col1 & R_B_MASK;
        r_b_col1 *= rel;
        unsigned r_b_col2 = col2 & R_B_MASK;
        r_b_col2 *= rel2;
        r_b_col1 += r_b_col2;
        r_b_col1 >>= 8;
        r_b_col1 &= R_B_MASK;

        unsigned g_col1 = col1 & G_MASK;
        g_col1 *= rel;
        unsigned g_col2 = col2 & G_MASK;
        g_col2 *= rel2;
        g_col1 += g_col2;
        g_col1 >>= 8;
        g_col1 &= G_MASK;

        return r_b_col1 | g_col1;
}

// ----------------------------------------------------------------------------

unsigned DDSFileClass::Get_Pixel(unsigned level, unsigned x, unsigned y) const
{
        // The WWASSERTs (level/mips, x/width, y/height) are release
        // no-ops; the corpus only queries in-range pixels.

        switch (Format)
        {
        // Note that we don't currently really support alpha on DXT1 - all alpha textures should use DXT5.
        // The reason for this is that when converting from DXT1 to 16 bit uncompressed texture we want
        // to be able to use RGB565 format instead of ARGB4444. As the alpha is encoded in DXT1 per-block
        // basis there isn't really a way to tell if the surface has an alpha or not so either we use alpha
        // or we don't.
        case WW3D_FORMAT_DXT1:
        {
                const unsigned char *block_memory = Get_Memory_Pointer(level) + (x / 4) * 8 + ((y / 4) * (Get_Width(level) / 4)) * 8;

                unsigned col0 = RGB565_To_ARGB8888(*(unsigned short *)&block_memory[0]);
                unsigned col1 = RGB565_To_ARGB8888(*(unsigned short *)&block_memory[2]);
                unsigned char line = block_memory[4 + (y % 4)];
                line >>= (x % 4) * 2;
                line = (line & 3);
                if (col0 > col1)
                {
                        switch (line)
                        {
                        case 0: return col0 | 0xff000000;
                        case 1: return col1 | 0xff000000;
                        case 2: return Combine_Colors(col1, col0, 85) | 0xff000000;
                        case 3: return Combine_Colors(col0, col1, 85) | 0xff000000;
                        }
                }
                else
                {
                        switch (line)
                        {
                        case 0: return col0 | 0xff000000;
                        case 1: return col1 | 0xff000000;
                        case 2: return Combine_Colors(col1, col0, 128) | 0xff000000;
                        case 3: return 0x00000000;
                        }
                }
        }
        break;
        case WW3D_FORMAT_DXT2:
                return 0xffffffff;
        case WW3D_FORMAT_DXT3:
                return 0xffffffff;
        case WW3D_FORMAT_DXT4:
                return 0xffffffff;
        case WW3D_FORMAT_DXT5:
        {
                const unsigned char *alpha_block = Get_Memory_Pointer(level) + (x / 4) * 16 + ((y / 4) * (Get_Width(level) / 4)) * 16;

                unsigned alpha0 = alpha_block[0];
                unsigned alpha1 = alpha_block[1];

                unsigned bit_idx = ((x % 4) + 4 * (y % 4)) * 3;
                unsigned byte_idx = bit_idx / 8;
                bit_idx %= 8;
                unsigned alpha_index = 0;
                for (int i = 0; i < 3; ++i)
                {
                        // WWASSERT(byte_idx<6) — release: no-op
                        unsigned alpha_bit = (alpha_block[2 + byte_idx] >> (bit_idx)) & 1;
                        alpha_index |= alpha_bit << (i);
                        bit_idx++;
                        if (bit_idx >= 8)
                        {
                                bit_idx = 0;
                                byte_idx++;
                        }
                }
                // WWASSERT(alpha_index<8) — release: no-op

                // 8-alpha or 6-alpha block?
                unsigned alpha_value = 0;
                if (alpha0 > alpha1)
                {
                        // 8-alpha block:  derive the other six alphas.
                        // Bit code 000 = alpha_0, 001 = alpha_1, others are interpolated.
                        switch (alpha_index)
                        {
                        case 0: alpha_value = alpha0; break;
                        case 1: alpha_value = alpha1; break;
                        case 2: alpha_value = (6 * alpha0 + 1 * alpha1 + 3) / 7; break; // bit code 010
                        case 3: alpha_value = (5 * alpha0 + 2 * alpha1 + 3) / 7; break; // bit code 011
                        case 4: alpha_value = (4 * alpha0 + 3 * alpha1 + 3) / 7; break; // bit code 100
                        case 5: alpha_value = (3 * alpha0 + 4 * alpha1 + 3) / 7; break; // bit code 101
                        case 6: alpha_value = (2 * alpha0 + 5 * alpha1 + 3) / 7; break; // bit code 110
                        case 7: alpha_value = (1 * alpha0 + 6 * alpha1 + 3) / 7; break; // bit code 111
                        }
                }
                else
                {
                        // 6-alpha block.
                        // Bit code 000 = alpha_0, 001 = alpha_1, others are interpolated.
                        switch (alpha_index)
                        {
                        case 0: alpha_value = alpha0; break;
                        case 1: alpha_value = alpha1; break;
                        case 2: alpha_value = (4 * alpha0 + 1 * alpha1 + 2) / 5; break; // Bit code 010
                        case 3: alpha_value = (3 * alpha0 + 2 * alpha1 + 2) / 5; break; // Bit code 011
                        case 4: alpha_value = (2 * alpha0 + 3 * alpha1 + 2) / 5; break; // Bit code 100
                        case 5: alpha_value = (1 * alpha0 + 4 * alpha1 + 2) / 5; break; // Bit code 101
                        case 6: alpha_value = 0; break;                                  // Bit code 110
                        case 7: alpha_value = 255; break;                                // Bit code 111
                        }
                }
                alpha_value <<= 24;

                // Extract color

                const unsigned char *color_block = alpha_block + 8;
                unsigned col0 = RGB565_To_ARGB8888(*(unsigned short *)&color_block[0]);
                unsigned col1 = RGB565_To_ARGB8888(*(unsigned short *)&color_block[2]);
                unsigned char line = color_block[4 + (y % 4)];
                line >>= (x % 4) * 2;
                line = (line & 3);
                switch (line)
                {
                case 0: return col0 | alpha_value;
                case 1: return col1 | alpha_value;
                case 2: return Combine_Colors(col1, col0, 85) | alpha_value;
                case 3: return Combine_Colors(col0, col1, 85) | alpha_value;
                }
        }
        break;
        default:
                break; // not in the original; the switch falls through the
                       // same way for every non-DXT format
        }
        return 0xffffffff;
}

// ----------------------------------------------------------------------------

bool DDSFileClass::Get_4x4_Block(
    unsigned char *dest_ptr,
    unsigned dest_pitch,
    WW3DFormat dest_format,
    unsigned level,
    unsigned source_x,
    unsigned source_y,
    const Vector3 &hsv_shift) const
{
        // The WWASSERTs (alignment, level, bounds) are release no-ops;
        // the corpus only queries in-range blocks.

        unsigned dest_bpp = Get_Bytes_Per_Pixel(dest_format);

        bool has_hsv_shift = hsv_shift[0] != 0.0f || hsv_shift[1] != 0.0f || hsv_shift[2] != 0.0f;
        switch (Format)
        {
        // Note that we don't currently really support alpha on DXT1 - all alpha textures should use DXT5.
        // The reason for this is that when converting from DXT1 to 16 bit uncompressed texture we want
        // to be able to use RGB565 format instead of ARGB4444. As the alpha is encoded in DXT1 per-block
        // basis there isn't really a way to tell if the surface has an alpha or not so either we use alpha
        // or we don't.
        case WW3D_FORMAT_DXT1:
        {
                const unsigned char *block_memory = Get_Memory_Pointer(level) + (source_x / 4) * 8 + ((source_y / 4) * (Get_Width(level) / 4)) * 8;

                unsigned col0 = RGB565_To_ARGB8888(*(unsigned short *)&block_memory[0]);
                unsigned col1 = RGB565_To_ARGB8888(*(unsigned short *)&block_memory[2]);
                // Even if we don't support alpha, decompression is different if source has alpha
                unsigned dest_pixel = 0;
                if (col0 > col1)
                {
                        if (has_hsv_shift)
                        {
                                Recolor(col0, hsv_shift);
                                Recolor(col1, hsv_shift);
                        }

                        for (int y = 0; y < 4; ++y)
                        {
                                unsigned char *tmp_dest_ptr = dest_ptr;
                                dest_ptr += dest_pitch;
                                unsigned char line = block_memory[4 + y];
                                for (int x = 0; x < 4; ++x)
                                {
                                        switch (line & 3)
                                        {
                                        case 0: dest_pixel = col0 | 0xff000000; break;
                                        case 1: dest_pixel = col1 | 0xff000000; break;
                                        case 2: dest_pixel = Combine_Colors(col1, col0, 85) | 0xff000000; break;
                                        case 3: dest_pixel = Combine_Colors(col0, col1, 85) | 0xff000000; break;
                                        }
                                        line >>= 2;

                                        Write_B8G8R8A8(tmp_dest_ptr, dest_format, dest_pixel);
                                        tmp_dest_ptr += dest_bpp;
                                }
                        }
                        return false; // No alpha found in the block
                }
                else
                {
                        if (has_hsv_shift)
                        {
                                Recolor(col0, hsv_shift);
                                Recolor(col1, hsv_shift);
                        }
                        bool contains_alpha = false;
                        for (int y = 0; y < 4; ++y)
                        {
                                unsigned char *tmp_dest_ptr = dest_ptr;
                                dest_ptr += dest_pitch;
                                unsigned char line = block_memory[4 + y];
                                for (int x = 0; x < 4; ++x)
                                {
                                        switch (line & 3)
                                        {
                                        case 0: dest_pixel = col0 | 0xff000000; break;
                                        case 1: dest_pixel = col1 | 0xff000000; break;
                                        case 2: dest_pixel = Combine_Colors(col1, col0, 128) | 0xff000000; break;
                                        case 3: dest_pixel = 0x00000000; contains_alpha = true; break;
                                        }
                                        line >>= 2;

                                        Write_B8G8R8A8(tmp_dest_ptr, dest_format, dest_pixel);
                                        tmp_dest_ptr += dest_bpp;
                                }
                        }
                        return contains_alpha; // Alpha block...?
                }
        }
        break;
        case WW3D_FORMAT_DXT2:
                return false;
        case WW3D_FORMAT_DXT3:
                return false;
        case WW3D_FORMAT_DXT4:
                return false;
        case WW3D_FORMAT_DXT5:
        {
                // Init alphas
                const unsigned char *alpha_block = Get_Memory_Pointer(level) + (source_x / 4) * 16 + ((source_y / 4) * (Get_Width(level) / 4)) * 16;

                unsigned alphas[8];
                alphas[0] = alpha_block[0];
                alphas[1] = alpha_block[1];

                // 8-alpha or 6-alpha block?
                if (alphas[0] > alphas[1])
                {
                        alphas[2] = (6 * alphas[0] + 1 * alphas[1] + 3) / 7; // bit code 010
                        alphas[3] = (5 * alphas[0] + 2 * alphas[1] + 3) / 7; // bit code 011
                        alphas[4] = (4 * alphas[0] + 3 * alphas[1] + 3) / 7; // bit code 100
                        alphas[5] = (3 * alphas[0] + 4 * alphas[1] + 3) / 7; // bit code 101
                        alphas[6] = (2 * alphas[0] + 5 * alphas[1] + 3) / 7; // bit code 110
                        alphas[7] = (1 * alphas[0] + 6 * alphas[1] + 3) / 7; // bit code 111
                }
                else
                {
                        alphas[2] = (4 * alphas[0] + 1 * alphas[1] + 2) / 5; // Bit code 010
                        alphas[3] = (3 * alphas[0] + 2 * alphas[1] + 2) / 5; // Bit code 011
                        alphas[4] = (2 * alphas[0] + 3 * alphas[1] + 2) / 5; // Bit code 100
                        alphas[5] = (1 * alphas[0] + 4 * alphas[1] + 2) / 5; // Bit code 101
                        alphas[6] = 0;                                          // Bit code 110
                        alphas[7] = 255;                                        // Bit code 111
                }

                // Init colors
                const unsigned char *color_block = alpha_block + 8;
                unsigned col0 = RGB565_To_ARGB8888(*(unsigned short *)&color_block[0]);
                unsigned col1 = RGB565_To_ARGB8888(*(unsigned short *)&color_block[2]);
                if (has_hsv_shift)
                {
                        Recolor(col0, hsv_shift);
                        Recolor(col1, hsv_shift);
                }

                unsigned dest_pixel = 0;
                unsigned bit_idx = 0;
                unsigned contains_alpha = 0xff;

                unsigned alpha_indices[16];
                unsigned *ai_ptr = alpha_indices;
                for (int a = 0; a < 2; ++a)
                {
                        ai_ptr[0] = alpha_block[2] & 0x7;
                        ai_ptr[1] = (alpha_block[2] >> 3) & 0x7;
                        ai_ptr[2] = (alpha_block[2] >> 6) | ((alpha_block[3] & 1) << 2);
                        ai_ptr[3] = (alpha_block[3] >> 1) & 0x7;
                        ai_ptr[4] = (alpha_block[3] >> 4) & 0x7;
                        ai_ptr[5] = (alpha_block[3] >> 7) | ((alpha_block[4] & 3) << 1);
                        ai_ptr[6] = (alpha_block[4] >> 2) & 0x7;
                        ai_ptr[7] = (alpha_block[4] >> 5);
                        ai_ptr += 8;
                        alpha_block += 3;
                }

                unsigned aii = 0;
                for (int y = 0; y < 4; ++y)
                {
                        unsigned char *tmp_dest_ptr = dest_ptr;
                        dest_ptr += dest_pitch;
                        unsigned char line = color_block[4 + y];
                        for (int x = 0; x < 4; ++x, bit_idx += 3)
                        {
                                unsigned alpha_value = alphas[alpha_indices[aii++]];
                                contains_alpha &= alpha_value;
                                alpha_value <<= 24;

                                // Extract color

                                switch (line & 3)
                                {
                                case 0: dest_pixel = col0 | alpha_value; break;
                                case 1: dest_pixel = col1 | alpha_value; break;
                                case 2: dest_pixel = Combine_Colors(col1, col0, 85) | alpha_value; break;
                                case 3: dest_pixel = Combine_Colors(col0, col1, 85) | alpha_value; break;
                                }
                                line >>= 2;

                                Write_B8G8R8A8(tmp_dest_ptr, dest_format, dest_pixel);
                                tmp_dest_ptr += dest_bpp;
                        }
                }

                return contains_alpha != 0xff; // Alpha block...? DXT5 should only be used when the image needs alpha
                                               // but for now check anyway...
        }
        default:
                break; // not in the original; the switch falls through the
                       // same way for every non-DXT format
        }
        return false;
}

// ----------------------------------------------------------------------------
// The corpus builders
// ----------------------------------------------------------------------------

static void put_u16(std::vector<uint8_t> &v, unsigned x)
{
        v.push_back((uint8_t)(x & 0xff));
        v.push_back((uint8_t)((x >> 8) & 0xff));
}

static void put_u32(std::vector<uint8_t> &v, unsigned x)
{
        v.push_back((uint8_t)(x & 0xff));
        v.push_back((uint8_t)((x >> 8) & 0xff));
        v.push_back((uint8_t)((x >> 16) & 0xff));
        v.push_back((uint8_t)((x >> 24) & 0xff));
}

struct DescBuild
{
        unsigned Size = 124;
        unsigned Flags = 0;
        unsigned Height = 4;
        unsigned Width = 4;
        unsigned Pitch = 0;
        unsigned Depth = 0;
        unsigned MipMapCount = 1;
        unsigned AlphaBitDepth = 0;
        unsigned FourCC = MAKEFOURCC('D', 'X', 'T', '1');
        unsigned Caps2 = 0;
};

static std::vector<uint8_t> build_desc(const DescBuild &d, const char *magic = "DDS ")
{
        std::vector<uint8_t> v;
        v.insert(v.end(), magic, magic + 4);
        put_u32(v, d.Size);
        put_u32(v, d.Flags);
        put_u32(v, d.Height);
        put_u32(v, d.Width);
        put_u32(v, d.Pitch);
        put_u32(v, d.Depth);
        put_u32(v, d.MipMapCount);
        put_u32(v, d.AlphaBitDepth); // AlphaBitDepth
        put_u32(v, 0);               // Reserved
        put_u32(v, 0);               // Surface (the 32-bit pointer slot)
        put_u32(v, 0); put_u32(v, 0); // CKDestOverlay
        put_u32(v, 0); put_u32(v, 0); // CKDestBlt
        put_u32(v, 0); put_u32(v, 0); // CKSrcOverlay
        put_u32(v, 0); put_u32(v, 0); // CKSrcBlt
        // PixelFormat
        put_u32(v, 32);  // Size
        put_u32(v, 0x4); // Flags (DDPF_RGB | DDPF_FOURCC shape; never read)
        put_u32(v, d.FourCC);
        put_u32(v, 0); // RGBBitCount
        put_u32(v, 0); // RBitMask
        put_u32(v, 0); // GBitMask
        put_u32(v, 0); // BBitMask
        put_u32(v, 0); // RGBAlphaBitMask
        // Caps
        put_u32(v, 0x1000); // Caps (DDSCAPS_TEXTURE; never read)
        put_u32(v, d.Caps2);
        put_u32(v, 0); // Caps3
        put_u32(v, 0); // Caps4
        put_u32(v, 0); // TextureStage
        return v;
}

// Formula bytes anchored at an absolute file offset (the established
// differential formula — identical bytes in the Rust corpus builders).
static std::vector<uint8_t> formula_at(size_t offset, size_t n)
{
        std::vector<uint8_t> out(n);
        for (size_t i = 0; i < n; i++)
                out[i] = (uint8_t)(((offset + i) * 11 + 7) & 0xFF);
        return out;
}

// A full mip chain's worth of formula data for a width×height DXTn
// texture (the level sizes Calculate_DXTC_Surface_Size computes).
static std::vector<uint8_t> mip_data(unsigned width, unsigned height, WW3DFormat format, unsigned levels, size_t base)
{
        std::vector<uint8_t> out;
        unsigned w = width, h = height;
        for (unsigned l = 0; l < levels; l++)
        {
                unsigned size = (w / 4) * (h / 4);
                if (format == WW3D_FORMAT_DXT1)
                        size *= 8;
                else if (format >= WW3D_FORMAT_DXT2)
                        size *= 16;
                std::vector<uint8_t> chunk = formula_at(base + out.size(), size);
                out.insert(out.end(), chunk.begin(), chunk.end());
                if (w > 4) w /= 2;
                if (h > 4) h /= 2;
        }
        return out;
}

// One hand-built DXT1 color block: two RGB565 endpoints and four line
// bytes (each pixel's 2-bit code, low pixel first).
static std::vector<uint8_t> dxt1_block(unsigned short c0, unsigned short c1,
                                        unsigned char l0, unsigned char l1,
                                        unsigned char l2, unsigned char l3)
{
        std::vector<uint8_t> v;
        put_u16(v, c0);
        put_u16(v, c1);
        v.push_back(l0);
        v.push_back(l1);
        v.push_back(l2);
        v.push_back(l3);
        return v;
}

// One hand-built DXT5 block: the two alpha bytes, six index bytes (16
// 3-bit indices, pixel 0 in the low bits of byte 0), then the DXT1-style
// color half.
static std::vector<uint8_t> dxt5_block(unsigned char a0, unsigned char a1,
                                        const unsigned char idx[6],
                                        unsigned short c0, unsigned short c1,
                                        unsigned char l0, unsigned char l1,
                                        unsigned char l2, unsigned char l3)
{
        std::vector<uint8_t> v;
        v.push_back(a0);
        v.push_back(a1);
        for (int i = 0; i < 6; i++)
                v.push_back(idx[i]);
        put_u16(v, c0);
        put_u16(v, c1);
        v.push_back(l0);
        v.push_back(l1);
        v.push_back(l2);
        v.push_back(l3);
        v.push_back(l3);
        return v;
}

// One DXT5 block's six index bytes: 16 three-bit indices packed
// LSB-first (pixel 0 in the low bits of byte 0).
static std::vector<uint8_t> dxt5_alpha_indices(const unsigned values[16])
{
        std::vector<uint8_t> bytes(6, 0);
        for (int k = 0; k < 16; k++)
        {
                unsigned v = values[k] & 7;
                unsigned bit = (unsigned)k * 3;
                for (int b = 0; b < 3; b++)
                {
                        if (v & (1u << b))
                                bytes[bit / 8] |= (uint8_t)(1u << (bit % 8));
                        bit++;
                }
        }
        return bytes;
}

// ----------------------------------------------------------------------------
// The dump driver
// ----------------------------------------------------------------------------

struct Out
{
        FILE *f;
        explicit Out(FILE *f_) : f(f_) {}

        void line(const std::string &s)
        {
                fprintf(f, "%s\n", s.c_str());
        }

        static std::string hex(const uint8_t *p, size_t n)
        {
                static const char digits[] = "0123456789abcdef";
                std::string s;
                s.reserve(n * 2);
                for (size_t i = 0; i < n; i++)
                {
                        s.push_back(digits[p[i] >> 4]);
                        s.push_back(digits[p[i] & 0xf]);
                }
                return s;
        }
        static std::string hexv(const std::vector<uint8_t> &v)
        {
                return v.empty() ? std::string("-") : hex(v.data(), v.size());
        }
};

// One corpus entry: the file bytes, the name to construct with and the
// reduction factor.
struct Case
{
        std::string id;
        std::string name = "corpus.dds";
        unsigned reduction = 0;
        std::vector<uint8_t> bytes;
};

// The munged Name the constructor builds — the constructor's own code,
// verbatim, over a copy (the class member is private; this mirrors it
// for the dump).
static std::string munged_name(const char *name)
{
        char buf[256];
        strlcpy_(buf, name, sizeof(buf));
        int len = (int)strlen(buf);
        if (len >= 3)
        {
                buf[len - 3] = 'd';
                buf[len - 2] = 'd';
                buf[len - 1] = 's';
        }
        return std::string(buf);
}

struct DdsDump
{
        Out &out;

        void name_line(const std::string &id, const char *given)
        {
                std::string m = munged_name(given);
                std::vector<uint8_t> bytes(m.begin(), m.end());
                out.line("name " + id + " " + Out::hexv(bytes));
        }

        void hdr_line(const std::string &id, const DDSFileClass &d)
        {
                char buf[512];
                snprintf(buf, sizeof(buf), "hdr %s %u %u %u %u %u %u %u %u %d %d %u",
                         id.c_str(),
                         d.Width, d.Height, d.Depth,
                         d.FullWidth, d.FullHeight, d.FullDepth,
                         d.MipLevels, d.ReductionFactor,
                         (int)d.Format, (int)d.Type, d.CubeFaceSize);
                out.line(buf);
        }

        void lvls_line(const std::string &id, const DDSFileClass &d)
        {
                std::string s = "lvls " + id;
                if (!d.LevelSizes)
                {
                        out.line(s + " -");
                        return;
                }
                for (unsigned l = 0; l < d.MipLevels; l++)
                {
                        char buf[64];
                        snprintf(buf, sizeof(buf), " %u:%u", d.LevelSizes[l], d.LevelOffsets[l]);
                        s += buf;
                }
                out.line(s);
        }

        void data_line(const std::string &id, const DDSFileClass &d)
        {
                if (!d.DDSMemory)
                {
                        out.line("data " + id + " -");
                        return;
                }
                out.line("data " + id + " " + Out::hex(d.DDSMemory, d.DDSMemorySize));
        }

        void px_line(const std::string &id, const DDSFileClass &d, unsigned level, unsigned x, unsigned y)
        {
                char buf[128];
                snprintf(buf, sizeof(buf), "px %s %u %u %u %08x",
                         id.c_str(), level, x, y, d.Get_Pixel(level, x, y));
                out.line(buf);
        }

        void blk_line(const std::string &id, const DDSFileClass &d, unsigned level,
                      unsigned sx, unsigned sy, WW3DFormat fmt)
        {
                unsigned bpp = Get_Bytes_Per_Pixel(fmt);
                std::vector<uint8_t> scratch(4 * 4 * bpp + 4, 0xcc);
                bool alpha = d.Get_4x4_Block(scratch.data(), 4 * bpp, fmt, level, sx, sy, Vector3(0.0f, 0.0f, 0.0f));
                char head[128];
                snprintf(head, sizeof(head), "blk %s %u %u %u %s ",
                         id.c_str(), level, sx, sy, ww3d_format_name(fmt));
                out.line(std::string(head) + Out::hex(scratch.data(), scratch.size()) + (alpha ? " 1" : " 0"));
        }

        void blk_line_hsv(const std::string &id, const DDSFileClass &d, unsigned level,
                          unsigned sx, unsigned sy, WW3DFormat fmt, float h, float s, float v)
        {
                unsigned bpp = Get_Bytes_Per_Pixel(fmt);
                std::vector<uint8_t> scratch(4 * 4 * bpp + 4, 0xcc);
                bool alpha = d.Get_4x4_Block(scratch.data(), 4 * bpp, fmt, level, sx, sy, Vector3(h, s, v));
                char head[192];
                snprintf(head, sizeof(head), "blkhsv %s %u %u %u %s %g %g %g ",
                         id.c_str(), level, sx, sy, ww3d_format_name(fmt), h, s, v);
                out.line(std::string(head) + Out::hex(scratch.data(), scratch.size()) + (alpha ? " 1" : " 0"));
        }

        void cpy_line(const std::string &id, DDSFileClass &d, unsigned level,
                      WW3DFormat fmt, unsigned w, unsigned h, float hs, float ss, float vs)
        {
                // A tightly packed destination surface: pitch one row of
                // the destination's pixels (the DXT destinations never
                // use the pitch — their copies are contiguous u32s).
                unsigned bpp = Get_Bytes_Per_Pixel(fmt);
                unsigned pitch = w * (bpp ? bpp : 1);
                std::vector<uint8_t> scratch((size_t)pitch * h + 4, 0xcc);
                d.Copy_Level_To_Surface(level, fmt, w, h, scratch.data(), pitch, Vector3(hs, ss, vs));
                char head[192];
                snprintf(head, sizeof(head), "cpy %s %u %s %u %u %g %g %g ",
                         id.c_str(), level, ww3d_format_name(fmt), w, h, hs, ss, vs);
                out.line(std::string(head) + Out::hex(scratch.data(), scratch.size()));
        }

        void cpycube_line(const std::string &id, DDSFileClass &d, unsigned face, unsigned level,
                          WW3DFormat fmt, unsigned w, unsigned h, float hs, float ss, float vs)
        {
                unsigned bpp = Get_Bytes_Per_Pixel(fmt);
                unsigned pitch = w * (bpp ? bpp : 1);
                std::vector<uint8_t> scratch((size_t)pitch * h + 4, 0xcc);
                d.Copy_CubeMap_Level_To_Surface(face, level, fmt, w, h, scratch.data(), pitch, Vector3(hs, ss, vs));
                char head[192];
                snprintf(head, sizeof(head), "cpycube %s %u %u %s %u %u %g %g %g ",
                         id.c_str(), face, level, ww3d_format_name(fmt), w, h, hs, ss, vs);
                out.line(std::string(head) + Out::hex(scratch.data(), scratch.size()));
        }

        void cpyvol_line(const std::string &id, DDSFileClass &d, unsigned level,
                         WW3DFormat fmt, unsigned w, unsigned h, float hs, float ss, float vs)
        {
                unsigned bpp = Get_Bytes_Per_Pixel(fmt);
                unsigned pitch = w * (bpp ? bpp : 1);
                std::vector<uint8_t> scratch((size_t)pitch * h + 4, 0xcc);
                d.Copy_Volume_Level_To_Surface(level, 0, fmt, w, h, scratch.data(), pitch, pitch, Vector3(hs, ss, vs));
                char head[192];
                snprintf(head, sizeof(head), "cpyvol %s %u %s %u %u %g %g %g ",
                         id.c_str(), level, ww3d_format_name(fmt), w, h, hs, ss, vs);
                out.line(std::string(head) + Out::hex(scratch.data(), scratch.size()));
        }

        void cubeptr_line(const std::string &id, const DDSFileClass &d, unsigned face, unsigned level)
        {
                char buf[128];
                snprintf(buf, sizeof(buf), "cubeptr %s %u %u %u",
                         id.c_str(), face, level, d.CubeFaceSize * face + d.LevelOffsets[level]);
                out.line(buf);
        }

        void volptr_line(const std::string &id, const DDSFileClass &d, unsigned level)
        {
                (void)d;
                out.line("volptr " + id + " " + std::to_string(level) + " -");
        }

        void recolor_line(const std::string &id, unsigned color, float h, float s, float v)
        {
                unsigned c = color;
                Recolor(c, Vector3(h, s, v));
                char buf[192];
                snprintf(buf, sizeof(buf), "recolor %s %08x %g %g %g %08x",
                         id.c_str(), color, h, s, v, c);
                out.line(buf);
        }

        void rgb565_line(const std::string &id, unsigned short rgb)
        {
                unsigned argb = RGB565_To_ARGB8888(rgb);
                unsigned short back = ARGB8888_To_RGB565(argb);
                char buf[192];
                snprintf(buf, sizeof(buf), "rgb565 %s %04x %08x %04x",
                         id.c_str(), rgb, argb, back);
                out.line(buf);
        }
};

// Constructs a case, dumps name/ok and (when full) hdr/lvls/load/data.
// Returns the live object for the query dumps (null when the
// constructor failed).
static DDSFileClass *run_case(Out &out, const Case &c, bool full)
{
        DdsDump dump{out};
        out.line("file " + c.id + " " + Out::hexv(c.bytes));
        dump.name_line(c.id, c.name.c_str());

        ByteFile file(c.bytes);
        DDSFileClass *dds = new DDSFileClass(c.name.c_str(), c.reduction, file);

        if (!dds->Is_Available())
        {
                out.line("err " + c.id);
                return dds;
        }
        out.line("ok " + c.id);
        if (!full)
                return dds;

        dump.hdr_line(c.id, *dds);
        dump.lvls_line(c.id, *dds);

        bool loaded = dds->Load(file);
        out.line(std::string("load ") + c.id + (loaded ? " ok" : " err"));
        dump.data_line(c.id, *dds);
        return dds;
}

int main(int argc, char **argv)
{
        if (argc != 2)
        {
                fprintf(stderr, "usage: gen_dds_golden <output-file>\n");
                return 2;
        }
        FILE *f = fopen(argv[1], "w");
        if (!f)
        {
                fprintf(stderr, "error: cannot open %s\n", argv[1]);
                return 1;
        }
        Out out(f);
        DdsDump dump{out};

        Init_D3D_To_WW3_Conversion();

        out.line("# gen_dds_golden — the ddsfile.cpp transcription's dump");
        out.line("# (see the file header for the grammar)");

        // ------------------------------------------------------------------
        // The layout corpora: constructor + Load over formula-built mip
        // chains (the level sizes match Calculate_DXTC_Surface_Size's
        // walk, so the data windows pin the skip/drop arithmetic).
        // ------------------------------------------------------------------

        {
                DescBuild d;
                d.Width = 32; d.Height = 32; d.MipMapCount = 6;
                Case c{"dxt1_32", "dxt1_32.dds", 0, build_desc(d)};
                std::vector<uint8_t> data = mip_data(32, 32, WW3D_FORMAT_DXT1, 6, c.bytes.size());
                c.bytes.insert(c.bytes.end(), data.begin(), data.end());
                delete run_case(out, c, true);
        }
        {
                DescBuild d;
                d.Width = 32; d.Height = 32; d.MipMapCount = 6;
                d.FourCC = MAKEFOURCC('D', 'X', 'T', '5');
                Case c{"dxt5_32", "dxt5_32.dds", 0, build_desc(d)};
                std::vector<uint8_t> data = mip_data(32, 32, WW3D_FORMAT_DXT5, 6, c.bytes.size());
                c.bytes.insert(c.bytes.end(), data.begin(), data.end());
                delete run_case(out, c, true);
        }
        {
                DescBuild d;
                d.Width = 16; d.Height = 16; d.MipMapCount = 5;
                Case c{"dxt1_16", "dxt1_16.dds", 0, build_desc(d)};
                std::vector<uint8_t> data = mip_data(16, 16, WW3D_FORMAT_DXT1, 5, c.bytes.size());
                c.bytes.insert(c.bytes.end(), data.begin(), data.end());
                delete run_case(out, c, true);
        }
        {
                DescBuild d;
                d.Width = 16; d.Height = 16; d.MipMapCount = 5;
                d.FourCC = MAKEFOURCC('D', 'X', 'T', '5');
                Case c{"dxt5_16", "dxt5_16.dds", 0, build_desc(d)};
                std::vector<uint8_t> data = mip_data(16, 16, WW3D_FORMAT_DXT5, 5, c.bytes.size());
                c.bytes.insert(c.bytes.end(), data.begin(), data.end());
                delete run_case(out, c, true);
        }
        {
                DescBuild d;
                d.Width = 8; d.Height = 8; d.MipMapCount = 3;
                Case c{"dxt1_8", "dxt1_8.dds", 0, build_desc(d)};
                std::vector<uint8_t> data = mip_data(8, 8, WW3D_FORMAT_DXT1, 3, c.bytes.size());
                c.bytes.insert(c.bytes.end(), data.begin(), data.end());
                delete run_case(out, c, true);
        }
        {
                DescBuild d;
                d.Width = 8; d.Height = 8; d.MipMapCount = 3;
                d.FourCC = MAKEFOURCC('D', 'X', 'T', '5');
                Case c{"dxt5_8", "dxt5_8.dds", 0, build_desc(d)};
                std::vector<uint8_t> data = mip_data(8, 8, WW3D_FORMAT_DXT5, 3, c.bytes.size());
                c.bytes.insert(c.bytes.end(), data.begin(), data.end());
                delete run_case(out, c, true);
        }
        {
                // MipMapCount 0 → one level; DXT3 blocks are 16 bytes.
                DescBuild d;
                d.Width = 8; d.Height = 8; d.MipMapCount = 0;
                d.FourCC = MAKEFOURCC('D', 'X', 'T', '3');
                Case c{"dxt3_8", "dxt3_8.dds", 0, build_desc(d)};
                std::vector<uint8_t> data = mip_data(8, 8, WW3D_FORMAT_DXT3, 1, c.bytes.size());
                c.bytes.insert(c.bytes.end(), data.begin(), data.end());
                delete run_case(out, c, true);
        }
        {
                // The minimum block stays 8 bytes (the level_size > 16
                // guard never divides it).
                DescBuild d;
                d.Width = 4; d.Height = 4; d.MipMapCount = 1;
                Case c{"dxt1_4", "dxt1_4.dds", 0, build_desc(d)};
                std::vector<uint8_t> data = formula_at(c.bytes.size(), 8);
                c.bytes.insert(c.bytes.end(), data.begin(), data.end());
                delete run_case(out, c, true);
        }
        {
                // Not a multiple of four: the integer division floors
                // both ways, one block.
                DescBuild d;
                d.Width = 5; d.Height = 5; d.MipMapCount = 1;
                Case c{"w5h5", "w5h5.dds", 0, build_desc(d)};
                std::vector<uint8_t> data = formula_at(c.bytes.size(), 8);
                c.bytes.insert(c.bytes.end(), data.begin(), data.end());
                delete run_case(out, c, true);
        }
        {
                // A zero-sized surface: the level arrays hold a zero
                // size, Load still reads the trailing bytes, and every
                // dimension query clamps up to four.
                DescBuild d;
                d.Width = 0; d.Height = 0; d.MipMapCount = 1;
                Case c{"w0h0", "w0h0.dds", 0, build_desc(d)};
                std::vector<uint8_t> data = formula_at(c.bytes.size(), 8);
                c.bytes.insert(c.bytes.end(), data.begin(), data.end());
                DDSFileClass *dds = run_case(out, c, true);
                dump.px_line(c.id, *dds, 0, 0, 0);
                delete dds;
        }
        {
                DescBuild d;
                d.Width = 64; d.Height = 64; d.MipMapCount = 7;
                Case c{"red1_64", "red1_64.dds", 1, build_desc(d)};
                std::vector<uint8_t> data = mip_data(64, 64, WW3D_FORMAT_DXT1, 7, c.bytes.size());
                c.bytes.insert(c.bytes.end(), data.begin(), data.end());
                delete run_case(out, c, true);
        }
        {
                DescBuild d;
                d.Width = 64; d.Height = 64; d.MipMapCount = 7;
                Case c{"red3_64", "red3_64.dds", 3, build_desc(d)};
                std::vector<uint8_t> data = mip_data(64, 64, WW3D_FORMAT_DXT1, 7, c.bytes.size());
                c.bytes.insert(c.bytes.end(), data.begin(), data.end());
                delete run_case(out, c, true);
        }
        {
                // Reduction eats every level: the else branch forces one
                // mip level, decrements the reduction factor and shifts
                // the width down to 1 (clamped up to 4 by the queries).
                DescBuild d;
                d.Width = 64; d.Height = 64; d.MipMapCount = 7;
                Case c{"red6_64", "red6_64.dds", 6, build_desc(d)};
                std::vector<uint8_t> data = mip_data(64, 64, WW3D_FORMAT_DXT1, 7, c.bytes.size());
                c.bytes.insert(c.bytes.end(), data.begin(), data.end());
                delete run_case(out, c, true);
        }
        {
                // Reduction past the mip count: the else branch leaves
                // ReductionFactor = 5-1 = 4, the skip walk steps four
                // levels (176 bytes) past a file that only has eight —
                // the size arithmetic underflows, the 0x80000000 guard
                // refuses the read and Load still returns true with no
                // memory.
                DescBuild d;
                d.Width = 16; d.Height = 16; d.MipMapCount = 3;
                Case c{"red_over", "red_over.dds", 5, build_desc(d)};
                std::vector<uint8_t> data = formula_at(c.bytes.size(), 8);
                c.bytes.insert(c.bytes.end(), data.begin(), data.end());
                delete run_case(out, c, true);
        }
        {
                // A cube map: six faces of the full chain, the face
                // stride sums the addressable levels plus the hardcoded
                // 16 bytes of the two dropped ones (exact for DXT1:
                // 8 + 8).
                DescBuild d;
                d.Width = 16; d.Height = 16; d.MipMapCount = 5;
                d.Caps2 = 0x200; // DDSCAPS2_CUBEMAP
                Case c{"cubemap_dxt1", "cubemap_dxt1.dds", 0, build_desc(d)};
                for (int face = 0; face < 6; face++)
                {
                        std::vector<uint8_t> data = mip_data(16, 16, WW3D_FORMAT_DXT1, 5, c.bytes.size());
                        c.bytes.insert(c.bytes.end(), data.begin(), data.end());
                }
                DDSFileClass *dds = run_case(out, c, true);
                for (unsigned face = 0; face < 6; face++)
                        dump.cubeptr_line(c.id, *dds, face, 0);
                dump.cubeptr_line(c.id, *dds, 1, 1);
                dump.cpycube_line(c.id, *dds, 0, 0, WW3D_FORMAT_DXT1, 16, 16, 0, 0, 0);
                dump.cpycube_line(c.id, *dds, 5, 0, WW3D_FORMAT_DXT1, 16, 16, 0, 0, 0);
                dump.cpycube_line(c.id, *dds, 0, 0, WW3D_FORMAT_DXT1, 16, 16, 0.4f, 0.1f, 0.05f);
                delete dds;
        }
        {
                // The DXT5 cube: the two dropped levels are 16 + 16
                // bytes but the stride still adds 16 — face 1's pointer
                // starts 16 bytes inside face 0's chain.
                DescBuild d;
                d.Width = 16; d.Height = 16; d.MipMapCount = 5;
                d.FourCC = MAKEFOURCC('D', 'X', 'T', '5');
                d.Caps2 = 0x200; // DDSCAPS2_CUBEMAP
                Case c{"cubemap_dxt5", "cubemap_dxt5.dds", 0, build_desc(d)};
                for (int face = 0; face < 6; face++)
                {
                        std::vector<uint8_t> data = mip_data(16, 16, WW3D_FORMAT_DXT5, 5, c.bytes.size());
                        c.bytes.insert(c.bytes.end(), data.begin(), data.end());
                }
                DDSFileClass *dds = run_case(out, c, true);
                dump.cubeptr_line(c.id, *dds, 1, 0);
                dump.cpycube_line(c.id, *dds, 1, 0, WW3D_FORMAT_DXT5, 16, 16, 0, 0, 0);
                delete dds;
        }
        {
                // A volume texture: the level sizes multiply by the
                // depth and the divisor becomes 8.
                DescBuild d;
                d.Width = 8; d.Height = 8; d.Depth = 4; d.MipMapCount = 4;
                d.FourCC = MAKEFOURCC('D', 'X', 'T', '5');
                d.Caps2 = 0x200000; // DDSCAPS2_VOLUME
                Case c{"volume_dxt5", "volume_dxt5.dds", 0, build_desc(d)};
                std::vector<uint8_t> data = formula_at(c.bytes.size(), 296);
                c.bytes.insert(c.bytes.end(), data.begin(), data.end());
                DDSFileClass *dds = run_case(out, c, true);
                dump.volptr_line(c.id, *dds, 0);
                dump.cpyvol_line(c.id, *dds, 0, WW3D_FORMAT_DXT5, 8, 8, 0, 0, 0);
                delete dds;
        }
        {
                DescBuild d;
                d.Width = 8; d.Height = 8; d.Depth = 4; d.MipMapCount = 4;
                d.FourCC = MAKEFOURCC('D', 'X', 'T', '5');
                d.Caps2 = 0x200000; // DDSCAPS2_VOLUME
                Case c{"volume_red1", "volume_red1.dds", 1, build_desc(d)};
                std::vector<uint8_t> data = formula_at(c.bytes.size(), 296);
                c.bytes.insert(c.bytes.end(), data.begin(), data.end());
                delete run_case(out, c, true);
        }
        {
                // The magic is never validated.
                DescBuild d;
                d.Width = 16; d.Height = 16; d.MipMapCount = 5;
                Case c{"junk_magic", "junk_magic.dds", 0, build_desc(d, "JUNK")};
                std::vector<uint8_t> data = mip_data(16, 16, WW3D_FORMAT_DXT1, 5, c.bytes.size());
                c.bytes.insert(c.bytes.end(), data.begin(), data.end());
                delete run_case(out, c, true);
        }
        {
                // A numeric FourCC below the table bound routes through
                // the conversion array (22 = D3DFMT_X8R8G8B8): the load
                // continues with an uncompressed format whose level
                // size is the raw block count (no DXT multiplier).
                DescBuild d;
                d.Width = 8; d.Height = 8; d.MipMapCount = 1;
                d.FourCC = 22;
                Case c{"fourcc_rgb", "fourcc_rgb.dds", 0, build_desc(d)};
                std::vector<uint8_t> data = formula_at(c.bytes.size(), 4);
                c.bytes.insert(c.bytes.end(), data.begin(), data.end());
                DDSFileClass *dds = run_case(out, c, true);
                dump.px_line(c.id, *dds, 0, 0, 0);
                dump.blk_line(c.id, *dds, 0, 0, 0, WW3D_FORMAT_A8R8G8B8);
                dump.cpy_line(c.id, *dds, 0, WW3D_FORMAT_A8R8G8B8, 8, 8, 0, 0, 0);
                delete dds;
        }
        {
                // An unknown FourCC above the table bound is UNKNOWN —
                // the same no-multiplier layout, opaque pixel queries.
                DescBuild d;
                d.Width = 8; d.Height = 8; d.MipMapCount = 1;
                d.FourCC = MAKEFOURCC('1', '2', '3', '4');
                Case c{"fourcc_unknown", "fourcc_unknown.dds", 0, build_desc(d)};
                std::vector<uint8_t> data = formula_at(c.bytes.size(), 4);
                c.bytes.insert(c.bytes.end(), data.begin(), data.end());
                DDSFileClass *dds = run_case(out, c, true);
                dump.px_line(c.id, *dds, 0, 0, 0);
                delete dds;
        }
        {
                // Size 100 with a longer desc block: the read returns
                // 124 bytes, which is not 100 — the check fails cleanly.
                DescBuild d;
                d.Width = 8; d.Height = 8; d.Size = 100;
                Case c{"size_100_long", "size_100_long.dds", 0, build_desc(d)};
                std::vector<uint8_t> tail = formula_at(c.bytes.size(), 100);
                c.bytes.insert(c.bytes.end(), tail.begin(), tail.end());
                delete run_case(out, c, true);
        }
        {
                // Size 124 but the file runs out of desc bytes.
                DescBuild d;
                d.Width = 8; d.Height = 8;
                Case c{"size_124_short", "size_124_short.dds", 0, build_desc(d)};
                c.bytes.resize(c.bytes.size() - 64);
                delete run_case(out, c, true);
        }
        {
                // A Size larger than the read can ever return.
                DescBuild d;
                d.Width = 8; d.Height = 8; d.Size = 200;
                Case c{"size_big", "size_big.dds", 0, build_desc(d)};
                std::vector<uint8_t> tail = formula_at(c.bytes.size(), 120);
                c.bytes.insert(c.bytes.end(), tail.begin(), tail.end());
                delete run_case(out, c, true);
        }
        {
                Case c{"empty", "empty.dds", 0, {}};
                delete run_case(out, c, true);
        }
        {
                Case c{"magic_only", "magic_only.dds", 0, {'D', 'D', 'S', ' '}};
                delete run_case(out, c, true);
        }
        {
                // A header-only file: the constructor succeeds, Load
                // computes a zero size and refuses.
                DescBuild d;
                d.Width = 4; d.Height = 4; d.MipMapCount = 1;
                Case c{"no_data", "no_data.dds", 0, build_desc(d)};
                delete run_case(out, c, true);
        }

        // ------------------------------------------------------------------
        // The name-munging corpora (the same tiny file, different names).
        // ------------------------------------------------------------------

        {
                DescBuild d;
                d.Width = 4; d.Height = 4; d.MipMapCount = 1;
                std::vector<uint8_t> base = build_desc(d);
                std::vector<uint8_t> data = formula_at(base.size(), 8);
                base.insert(base.end(), data.begin(), data.end());

                struct NameCase { const char *id; const char *name; };
                static const NameCase names[] = {
                        {"n_tga", "textures/tank.tga"},
                        {"n_dds", "tank.dds"},
                        {"n_bare", "tank"},
                        {"n_abc3", "abc"},
                        {"n_dot", "a.b"},
                        {"n_upper", "TANK.TGA"},
                };
                for (const NameCase &n : names)
                {
                        Case c{n.id, n.name, 0, base};
                        delete run_case(out, c, false);
                }
                // A name longer than the 256-byte buffer: strlcpy
                // truncates to 255 characters and the munging replaces
                // the last three.
                {
                        std::string long_name(300, 'x');
                        long_name[10] = '.';
                        long_name[11] = 't';
                        long_name[12] = 'g';
                        long_name[13] = 'a';
                        Case c{"n_long", long_name, 0, base};
                        delete run_case(out, c, false);
                }
        }

        // ------------------------------------------------------------------
        // The pixel corpora: hand-built blocks driving Get_Pixel and
        // Get_4x4_Block through every decode branch.
        // ------------------------------------------------------------------

        {
                // DXT1, col0 > col1: the 8-color mode (code 2/3 are the
                // 85/255 interpolations).
                std::vector<uint8_t> block = dxt1_block(0xF800, 0x001F, 0x00, 0x55, 0xAA, 0xFF);
                DescBuild d;
                d.Width = 4; d.Height = 4; d.MipMapCount = 1;
                Case c{"px8", "px8.dds", 0, build_desc(d)};
                c.bytes.insert(c.bytes.end(), block.begin(), block.end());
                DDSFileClass *dds = run_case(out, c, true);
                for (unsigned y = 0; y < 4; y++)
                        dump.px_line(c.id, *dds, 0, 0, y);
                for (unsigned y = 0; y < 4; y++)
                        dump.px_line(c.id, *dds, 0, 3, y);
                static const WW3DFormat fmts[] = {
                        WW3D_FORMAT_A8R8G8B8, WW3D_FORMAT_X8R8G8B8, WW3D_FORMAT_R8G8B8,
                        WW3D_FORMAT_R5G6B5, WW3D_FORMAT_A4R4G4B4, WW3D_FORMAT_A1R5G5B5,
                        WW3D_FORMAT_R3G3B2, WW3D_FORMAT_L8, WW3D_FORMAT_A8};
                for (WW3DFormat fmt : fmts)
                        dump.blk_line(c.id, *dds, 0, 0, 0, fmt);
                dump.blk_line_hsv(c.id, *dds, 0, 0, 0, WW3D_FORMAT_A8R8G8B8, 0.5f, 0.25f, -0.1f);
                delete dds;
        }
        {
                // DXT1, col0 <= col1: the 4-color mode (code 2 is the
                // 128/255 midpoint, code 3 is transparent black).
                std::vector<uint8_t> block = dxt1_block(0x001F, 0xF800, 0x00, 0x55, 0xAA, 0xFF);
                DescBuild d;
                d.Width = 4; d.Height = 4; d.MipMapCount = 1;
                Case c{"px4", "px4.dds", 0, build_desc(d)};
                c.bytes.insert(c.bytes.end(), block.begin(), block.end());
                DDSFileClass *dds = run_case(out, c, true);
                for (unsigned y = 0; y < 4; y++)
                        dump.px_line(c.id, *dds, 0, 0, y);
                for (unsigned y = 0; y < 4; y++)
                        dump.px_line(c.id, *dds, 0, 3, y);
                dump.blk_line(c.id, *dds, 0, 0, 0, WW3D_FORMAT_A8R8G8B8);
                dump.blk_line(c.id, *dds, 0, 0, 0, WW3D_FORMAT_R5G6B5);
                dump.blk_line_hsv(c.id, *dds, 0, 0, 0, WW3D_FORMAT_A8R8G8B8, -0.3f, 0.2f, 0.1f);
                delete dds;
        }
        {
                // DXT5, alpha0 > alpha1: the 8-alpha table. The index
                // pattern runs 0..7 then 7..0 — the two decoded halves
                // differ, so the unrolled reader's byte advance is
                // pinned (a reader that re-reads the first half's
                // bytes would emit 0..7 twice).
                unsigned idx[16];
                for (int k = 0; k < 16; k++)
                        idx[k] = (unsigned)(k < 8 ? k : 15 - k);
                std::vector<uint8_t> idxb = dxt5_alpha_indices(idx);
                std::vector<uint8_t> block = dxt5_block(200, 100, idxb.data(), 0xF800, 0x07E0, 0x00, 0x55, 0xAA, 0xFF);
                DescBuild d;
                d.Width = 4; d.Height = 4; d.MipMapCount = 1;
                d.FourCC = MAKEFOURCC('D', 'X', 'T', '5');
                Case c{"px5a8", "px5a8.dds", 0, build_desc(d)};
                c.bytes.insert(c.bytes.end(), block.begin(), block.end());
                DDSFileClass *dds = run_case(out, c, true);
                dump.px_line(c.id, *dds, 0, 0, 0); // index 0, color code 0
                dump.px_line(c.id, *dds, 0, 1, 0); // index 1, color code 1
                dump.px_line(c.id, *dds, 0, 2, 0); // index 2, color code 2
                dump.px_line(c.id, *dds, 0, 3, 0); // index 3, color code 3
                dump.px_line(c.id, *dds, 0, 0, 1); // index 4
                dump.px_line(c.id, *dds, 0, 1, 1); // index 5
                dump.px_line(c.id, *dds, 0, 2, 1); // index 6
                dump.px_line(c.id, *dds, 0, 3, 1); // index 7
                dump.px_line(c.id, *dds, 0, 0, 2); // index 8 -> 0
                dump.px_line(c.id, *dds, 0, 3, 3); // index 15 -> 7, code 3
                dump.blk_line(c.id, *dds, 0, 0, 0, WW3D_FORMAT_A8R8G8B8);
                dump.blk_line(c.id, *dds, 0, 0, 0, WW3D_FORMAT_A4R4G4B4);
                dump.blk_line(c.id, *dds, 0, 0, 0, WW3D_FORMAT_L8);
                dump.blk_line(c.id, *dds, 0, 0, 0, WW3D_FORMAT_A8);
                dump.blk_line_hsv(c.id, *dds, 0, 0, 0, WW3D_FORMAT_A8R8G8B8, 0.25f, -0.5f, 0.2f);
                delete dds;
        }
        {
                // DXT5, alpha0 <= alpha1: the 6-alpha table (index 6 is
                // fully transparent, 7 fully opaque) — the same
                // two-half index pattern.
                unsigned idx[16];
                for (int k = 0; k < 16; k++)
                        idx[k] = (unsigned)(k < 8 ? k : 15 - k);
                std::vector<uint8_t> idxb = dxt5_alpha_indices(idx);
                std::vector<uint8_t> block = dxt5_block(100, 200, idxb.data(), 0x07E0, 0x001F, 0x00, 0x55, 0xAA, 0xFF);
                DescBuild d;
                d.Width = 4; d.Height = 4; d.MipMapCount = 1;
                d.FourCC = MAKEFOURCC('D', 'X', 'T', '5');
                Case c{"px5a6", "px5a6.dds", 0, build_desc(d)};
                c.bytes.insert(c.bytes.end(), block.begin(), block.end());
                DDSFileClass *dds = run_case(out, c, true);
                dump.px_line(c.id, *dds, 0, 0, 0);
                dump.px_line(c.id, *dds, 0, 1, 0);
                dump.px_line(c.id, *dds, 0, 2, 0);
                dump.px_line(c.id, *dds, 0, 3, 0);
                dump.px_line(c.id, *dds, 0, 0, 1);
                dump.px_line(c.id, *dds, 0, 1, 1);
                dump.px_line(c.id, *dds, 0, 2, 1); // index 6 -> alpha 0
                dump.px_line(c.id, *dds, 0, 3, 1); // index 7 -> alpha 255
                dump.blk_line(c.id, *dds, 0, 0, 0, WW3D_FORMAT_A8R8G8B8);
                delete dds;
        }
        {
                // A mip chain of hand-built blocks: the level-1 and
                // level-2 queries pin the block addressing across the
                // clamped level widths. Five declared levels leave
                // three addressable after the constructor drops two.
                DescBuild d;
                d.Width = 16; d.Height = 16; d.MipMapCount = 5;
                Case c{"pxmip", "pxmip.dds", 0, build_desc(d)};
                // level 0: 4x4 blocks
                for (unsigned j = 0; j < 4; j++)
                        for (unsigned i = 0; i < 4; i++)
                        {
                                unsigned short c0 = (unsigned short)(0xF800 - (i * 0x40 + j * 0x20));
                                unsigned short c1 = (unsigned short)(0x001F + i * 0x30 + j * 0x10);
                                std::vector<uint8_t> b = dxt1_block(c0, c1, 0x00, 0x55, 0xAA, 0xFF);
                                c.bytes.insert(c.bytes.end(), b.begin(), b.end());
                        }
                // level 1: 2x2 blocks (8x8 pixels)
                for (unsigned j = 0; j < 2; j++)
                        for (unsigned i = 0; i < 2; i++)
                        {
                                unsigned short c0 = (unsigned short)(0x7E0 - (i * 0x40 + j * 0x20));
                                unsigned short c1 = (unsigned short)(0xF800 + i * 0x30);
                                std::vector<uint8_t> b = dxt1_block(c0, c1, 0xFF, 0xAA, 0x55, 0x00);
                                c.bytes.insert(c.bytes.end(), b.begin(), b.end());
                        }
                // level 2: one block (4x4 pixels)
                {
                        std::vector<uint8_t> b = dxt1_block(0xFFE0, 0x001F, 0x00, 0x55, 0xAA, 0xFF);
                        c.bytes.insert(c.bytes.end(), b.begin(), b.end());
                }
                // levels 3 and 4 (the two the constructor drops): 8
                // bytes each, present in the file but unreachable
                {
                        std::vector<uint8_t> b = formula_at(c.bytes.size(), 16);
                        c.bytes.insert(c.bytes.end(), b.begin(), b.end());
                }
                DDSFileClass *dds = run_case(out, c, true);
                dump.px_line(c.id, *dds, 1, 0, 0);  // level-1 block (0,0)
                dump.px_line(c.id, *dds, 1, 4, 0);  // level-1 block (1,0)
                dump.px_line(c.id, *dds, 1, 0, 4);  // level-1 block (0,1)
                dump.px_line(c.id, *dds, 1, 5, 3);  // level-1 block (1,0), pixel (1,3)
                dump.px_line(c.id, *dds, 2, 0, 0);  // the level-2 block
                dump.px_line(c.id, *dds, 2, 3, 3);
                delete dds;
        }
        {
                // DXT2/3/4 pixel queries return opaque white.
                static const struct { const char *id; unsigned fourcc; } fades[] = {
                        {"px_dxt2", MAKEFOURCC('D', 'X', 'T', '2')},
                        {"px_dxt3", MAKEFOURCC('D', 'X', 'T', '3')},
                        {"px_dxt4", MAKEFOURCC('D', 'X', 'T', '4')},
                };
                for (const auto &e : fades)
                {
                        DescBuild d;
                        d.Width = 4; d.Height = 4; d.MipMapCount = 1;
                        d.FourCC = e.fourcc;
                        Case c{e.id, std::string(e.id) + ".dds", 0, build_desc(d)};
                        std::vector<uint8_t> data = formula_at(c.bytes.size(), d.FourCC == MAKEFOURCC('D', 'X', 'T', '2') ? 8 : 16);
                        c.bytes.insert(c.bytes.end(), data.begin(), data.end());
                        DDSFileClass *dds = run_case(out, c, true);
                        dump.px_line(c.id, *dds, 0, 0, 0);
                        dump.blk_line(c.id, *dds, 0, 0, 0, WW3D_FORMAT_A8R8G8B8);
                        delete dds;
                }
        }

        // ------------------------------------------------------------------
        // The copy corpora.
        // ------------------------------------------------------------------

        {
                // A fresh dxt1_16 for the copy family (the earlier
                // object already consumed its Load).
                DescBuild d;
                d.Width = 16; d.Height = 16; d.MipMapCount = 5;
                Case c{"cpy_dxt1", "cpy_dxt1.dds", 0, build_desc(d)};
                std::vector<uint8_t> data = mip_data(16, 16, WW3D_FORMAT_DXT1, 5, c.bytes.size());
                c.bytes.insert(c.bytes.end(), data.begin(), data.end());
                DDSFileClass *dds = run_case(out, c, true);
                dump.cpy_line(c.id, *dds, 0, WW3D_FORMAT_DXT1, 16, 16, 0, 0, 0);
                dump.cpy_line(c.id, *dds, 0, WW3D_FORMAT_DXT1, 16, 16, 0.4f, 0.1f, 0.05f);
                dump.cpy_line(c.id, *dds, 0, WW3D_FORMAT_DXT2, 16, 16, 0, 0, 0);
                dump.cpy_line(c.id, *dds, 0, WW3D_FORMAT_DXT2, 16, 16, 0.4f, 0.1f, 0.05f);
                dump.cpy_line(c.id, *dds, 0, WW3D_FORMAT_A8R8G8B8, 16, 16, 0, 0, 0);
                dump.cpy_line(c.id, *dds, 0, WW3D_FORMAT_DXT1, 16, 15, 0, 0, 0); // size mismatch: untouched
                dump.cpy_line(c.id, *dds, 1, WW3D_FORMAT_DXT1, 8, 8, 0, 0, 0);
                delete dds;
        }
        {
                DescBuild d;
                d.Width = 16; d.Height = 16; d.MipMapCount = 5;
                d.FourCC = MAKEFOURCC('D', 'X', 'T', '5');
                Case c{"cpy_dxt5", "cpy_dxt5.dds", 0, build_desc(d)};
                std::vector<uint8_t> data = mip_data(16, 16, WW3D_FORMAT_DXT5, 5, c.bytes.size());
                c.bytes.insert(c.bytes.end(), data.begin(), data.end());
                DDSFileClass *dds = run_case(out, c, true);
                dump.cpy_line(c.id, *dds, 0, WW3D_FORMAT_DXT5, 16, 16, 0, 0, 0);
                dump.cpy_line(c.id, *dds, 0, WW3D_FORMAT_DXT5, 16, 16, -0.45f, 0.3f, 0.0f);
                delete dds;
        }
        {
                // DXT3 with a hue shift: the format matches and the
                // rewrite path has no DXT3 branch — the destination is
                // left untouched.
                DescBuild d;
                d.Width = 8; d.Height = 8; d.MipMapCount = 1;
                d.FourCC = MAKEFOURCC('D', 'X', 'T', '3');
                Case c{"cpy_dxt3", "cpy_dxt3.dds", 0, build_desc(d)};
                std::vector<uint8_t> data = formula_at(c.bytes.size(), 64);
                c.bytes.insert(c.bytes.end(), data.begin(), data.end());
                DDSFileClass *dds = run_case(out, c, true);
                dump.cpy_line(c.id, *dds, 0, WW3D_FORMAT_DXT3, 8, 8, 0, 0, 0);
                dump.cpy_line(c.id, *dds, 0, WW3D_FORMAT_DXT3, 8, 8, 0.5f, 0.5f, 0.5f);
                delete dds;
        }
        {
                // The conversion walk on a small DXT1 texture.
                DescBuild d;
                d.Width = 8; d.Height = 8; d.MipMapCount = 1;
                Case c{"cpy_conv", "cpy_conv.dds", 0, build_desc(d)};
                std::vector<uint8_t> data = formula_at(c.bytes.size(), 32);
                c.bytes.insert(c.bytes.end(), data.begin(), data.end());
                DDSFileClass *dds = run_case(out, c, true);
                dump.cpy_line(c.id, *dds, 0, WW3D_FORMAT_A8R8G8B8, 8, 8, 0, 0, 0);
                dump.cpy_line(c.id, *dds, 0, WW3D_FORMAT_R5G6B5, 8, 8, 0, 0, 0);
                delete dds;
        }
        {
                // The conversion walk with a hue shift on DXT5.
                DescBuild d;
                d.Width = 8; d.Height = 8; d.MipMapCount = 1;
                d.FourCC = MAKEFOURCC('D', 'X', 'T', '5');
                Case c{"cpy_conv5", "cpy_conv5.dds", 0, build_desc(d)};
                std::vector<uint8_t> data = formula_at(c.bytes.size(), 64);
                c.bytes.insert(c.bytes.end(), data.begin(), data.end());
                DDSFileClass *dds = run_case(out, c, true);
                dump.cpy_line(c.id, *dds, 0, WW3D_FORMAT_A8R8G8B8, 8, 8, 0.15f, 0.2f, 0.05f);
                delete dds;
        }

        // ------------------------------------------------------------------
        // The recolor / RGB565 unit corpora.
        // ------------------------------------------------------------------

        {
                struct R { const char *id; unsigned color; float h, s, v; };
                static const R cases[] = {
                        {"rc_red", 0x00F80000, 0.5f, 0.25f, -0.1f},
                        {"rc_blue", 0x000000F8, -3.0f, 0.0f, 0.0f},
                        {"rc_white", 0x00FFFFFF, 0.5f, 0.25f, -0.1f},
                        {"rc_gray", 0x00808080, 10.0f, 5.0f, -0.5f},
                        {"rc_green", 0x0000FF00, 0.0f, 0.0f, 0.0f},
                        {"rc_yellow", 0x00F8E000, 0.0f, -0.5f, 0.0f},
                        {"rc_cyan", 0x0000F8F8, 1.0f, 0.5f, 0.2f},
                        {"rc_alpha", 0xFF102030, 0.2f, 0.2f, 0.2f},
                        {"rc_black", 0x00000000, 0.3f, 0.4f, 0.5f},
                        {"rc_magenta", 0x00F800F8, 0.0f, 0.9f, 0.0f},
                        {"rc_lightgray", 0x00E0E0E0, -0.5f, 0.0f, 0.3f},
                        {"rc_deep", 0x000010F8, 0.9f, 0.1f, -0.2f},
                };
                for (const R &r : cases)
                        dump.recolor_line(r.id, r.color, r.h, r.s, r.v);
        }
        {
                static const unsigned short rgbs[] = {
                        0x0000, 0xFFFF, 0xF800, 0x07E0, 0x001F, 0x1234, 0xABCD,
                        0x8000, 0x0400, 0x0020, 0xF81F, 0x07FF, 0xFFE0};
                for (int i = 0; i < (int)(sizeof(rgbs) / sizeof(rgbs[0])); i++)
                {
                        char id[32];
                        snprintf(id, sizeof(id), "rgb%d", i);
                        dump.rgb565_line(id, rgbs[i]);
                }
        }

        fclose(f);
        return 0;
}
