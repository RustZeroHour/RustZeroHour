// SPDX-License-Identifier: GPL-3.0-or-later
//
// Golden-vector driver for `rzh-formats::tga` (parity level P0).
//
// Verbatim transcription of the baseline tree's Targa class —
// Core/Libraries/Source/WWVegas/WWLib/TARGA.{h,cpp} at commit 571a7c5 —
// with the FileClass plumbing replaced by a byte source (reads) and a byte
// sink (writes). Everything else is the original code, comments and all:
// the Open footer sniff, the two-step Load with its short-circuiting
// re-Open, the image-type switch (with its missing TGA_MONO_ENCODED case),
// the truecolor-only InvertImage, the origin normalization, the RLE
// decoder, the RLE encoder, the flips and the Save path.
//
// Deliberate deviations, pinned identically on both sides:
//
// - `Targa::EncodeImage`'s final iteration compares one pixel PAST the
//   image buffer (the `end` cursor outruns the data before the last
//   compare) and lets that stray byte decide the fate of the trailing
//   run — a match drops it. Reading it here would be nondeterministic UB,
//   so the transcription pins that compare to its deterministic majority
//   outcome (a mismatch, i.e. the trailing run survives). The Rust port
//   carries the same rule; see the Rust module docs for the long form.
// - The colormap chunk is written from `palette + CMapStart * depth` in
//   the original — past its own allocation whenever CMapStart != 0. Every
//   corpus uses CMapStart = 0 (as every real file does); the Rust port
//   refuses anything else.
//
// Output grammar (one command per line, hex lowercase, '-' = empty):
//
//   file <id> <hex>          the corpus bytes (assembled or Save'd)
//   gen <id> <hex>           the transcribed Save's bytes (writer corpora;
//                            must equal `file`)
//   save <id> <i> <p> <c> <x>  the Save flags of a writer corpus: TGAF_IMAGE,
//                            TGAF_PAL, TGAF_COMPRESS, addextension
//   ok <id> / err <id>       the transcribed reader's verdict (Open +
//                            Load(TGAF_IMAGE | TGAF_PAL), invert_image true)
//   hdr <id> <12 ints>       the header as the reader left it: id_len
//                            cmap_type image_type cmap_start cmap_len
//                            cmap_depth x_off y_off width height pix_depth
//                            descriptor (origin bits cleared by the load
//                            when an image was normalized)
//   pal <id> <hex>           the colormap chunk the reader holds
//   img <id> <hex>           the decoded, normalized image bytes
//   ext <id> <hex|->         the 495-byte Targa 2.0 extension ('-' = none)
//   footer <id> <ext> <dev>  the footer offsets when the signature matched
//
// Usage: gen_tga_golden <output-file>
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
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>
#include <vector>

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

// The original compiles on Windows (MSVC), where `long` is 32-bit; pin the
// field width explicitly so the Linux build sees the on-disk layout (the
// same trick the CSF transcription uses for WideChar).
typedef struct _TGA2Footer
{
        int32_t Extension;
        int32_t Developer;
        char Signature[16];
        char RsvdChar;
        char BZST;
} TGA2Footer;

typedef struct _TGA2Extension
{
        short ExtSize;
        char AuthName[41];
        char AuthComment[324];
        short DateMonth;
        short DateDay;
        short DateYear;
        short TimeHour;
        short TimeMinute;
        short TimeSecond;
        char JobName[41];
        short JobTimeHour;
        short JobTimeMinute;
        short JobTimeSecond;
        char SoftID[41];
        short SoftVerNumber;
        char SoftVerLetter;
        int32_t KeyColor;
        short AspectNumer;
        short AspectDenom;
        short GammaNumer;
        short GammaDenom;
        int32_t ColorCor;
        int32_t PostStamp;
        int32_t ScanLine;
        char Attributes;
} TGA2Extension;

#pragma pack(pop)

static_assert(sizeof(TGAHeader) == 18, "header layout drift");
static_assert(sizeof(TGA2Footer) == 26, "footer layout drift");
static_assert(sizeof(TGA2Extension) == 495, "extension layout drift");

// --- strlcpy (BSD semantics; not in glibc) ------------------------------------

static void xstrlcpy(char *dst, const char *src, size_t size)
{
        size_t i = 0;
        if (size != 0)
        {
                for (; i + 1 < size && src[i] != '\0'; ++i)
                        dst[i] = src[i];
                dst[i] = '\0';
        }
}

// --- the file plumbing (stands in for FileClass) ------------------------------
//
// Read side: a const byte buffer with lseek-shaped semantics — negative
// targets fail (-1), positions past EOF are allowed and read as nothing.
// Write side: an appending vector; the extension offset is its length.

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

typedef std::vector<uint8_t> Sink;

static int sink_write(Sink &file, const void *buffer, int size)
{
        const uint8_t *p = (const uint8_t *)buffer;
        file.insert(file.end(), p, p + size);
        return size;
}

// --- the Targa class transcription (TARGA.cpp) --------------------------------
//
// The member order and the control flow follow the original; only the file
// handle is replaced. mFooterInfo/mFooterExt/mFooterDev are view-only
// additions the golden dump reads (the original keeps the footer in a local
// in Open and discards it).

struct Targa
{
        TGAHeader Header{};
        long mAccess = TGA_READMODE;
        long mFlags = 0;
        uint8_t *mImage = nullptr;
        uint8_t *mPalette = nullptr;
        TGA2Extension mExtension{};

        SrcFile src;    // read side (TGA_READMODE)
        Sink *dst = nullptr; // write side (TGA_WRITEMODE)
        Sink save_sink; // the bytes the last Save produced
        bool mFooterInfo = false; // view-only: the file ends with the signature
        long mFooterExt = 0;      // view-only: the footer's Extension offset
        long mFooterDev = 0;      // view-only: the footer's Developer offset

        ~Targa()
        {
                delete[] mImage;
                delete[] mPalette;
        }

        bool Is_File_Open() const { return src.data != nullptr || dst != nullptr; }

        void Clear_File()
        {
                src.data = nullptr;
                src.len = 0;
                src.pos = 0;
                dst = nullptr;
        }

        // Open, TGA_READMODE path only (TARGA.cpp Open, case TGA_READMODE).
        long Open_Read(const uint8_t *data, size_t len)
        {
                long error = 0;
                int size;
                TGA2Footer footer;

                /* File already open? */
                if (Is_File_Open() && (mAccess == TGA_READMODE))
                {
                        return (0);
                }

                Clear_File();

                /* Initialize the access mode. */
                mAccess = TGA_READMODE;
                mFlags &= ~TGAF_TGA2;
                mFooterInfo = false;

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
                        if (src.read((uint8_t *)&footer, sizeof(TGA2Footer)) != sizeof(TGA2Footer))
                        {
                                error = TGAERR_READ;
                        }
                        else
                        {
                                /* If this a 2.0 file with an extension? */
                                if (strncmp(footer.Signature, TGA2_SIGNATURE, 16) == 0)
                                {
                                        mFooterInfo = true; // view-only
                                        mFooterExt = footer.Extension;
                                        mFooterDev = footer.Developer;
                                        if (footer.Extension != 0)
                                        {
                                                mFlags |= TGAF_TGA2;
                                        }
                                }
                        }
                }

                /* Read in Extension data */
                if (!error && (mFlags & TGAF_TGA2))
                {
                        if (src.seek(footer.Extension, 0 /* SEEK_SET */) == -1)
                        {
                                error = TGAERR_READ;
                        }

                        if (!error)
                        {
                                if (src.read((uint8_t *)&mExtension, sizeof(TGA2Extension))
                                    != sizeof(TGA2Extension))
                                {
                                        error = TGAERR_READ;
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
                        error = TGAERR_OPEN;
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

                        Close();
                }
                else
                {
                        error = -1; // TGAERR_OPEN
                }

                return (error);
        }

        // Targa::DecodeImage, verbatim (the reads go against the byte source).
        // The corpora never enter the shapes where the original would write
        // past its image buffer (an over-supplied stream); the Rust port
        // refuses those with errors.
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

        // Targa::EncodeImage, verbatim except the final-iteration overread —
        // see the file comment.
        long EncodeImage()
        {
                uint8_t *packet;
                long packet_index;
                uint8_t *start;
                uint8_t *end;
                long depth;
                long pixels;
                long count;
                long match;
                long i;
                long error = 0;

                depth = TGA_BytesPerPixel(Header.PixelDepth);

                packet = new uint8_t[128 * (depth > 0 ? depth : 1)];
                pixels = Header.Width * Header.Height;
                start = mImage;
                end = start;
                count = 0;
                packet[0] = 0;
                packet_index = 1;

                const uint8_t *image_end = mImage + (size_t)pixels * depth;

                while ((pixels != 0) && !error)
                {
                        match = 1;

                        /* Advance to the next pixel */
                        end += depth;
                        pixels--;

                        /* Compare pixels. */
                        if (end + depth > image_end)
                        {
                                /* The original dereferences `end` here regardless —
                                 * one pixel past the buffer on the final iteration.
                                 * Pinned to a mismatch; see the file comment. */
                                match = 0;
                        }
                        else
                        {
                                for (i = 0; i < depth; i++)
                                {
                                        if (start[i] != end[i])
                                        {
                                                match = 0;
                                                break;
                                        }
                                }
                        }

                        /* Run of pixels */
                        if (match == 1)
                        {
                                count++;

                                if (count < 128)
                                {
                                        if (packet[0] == 0)
                                                continue;
                                }
                                else
                                        count--;
                        }

                        if ((count != 0) && (packet[0] == 0))
                        {
                                /* Run count */
                                packet[0] = (uint8_t)(count | 0x80);

                                /* Run pixel */
                                for (i = 0; i < depth; i++)
                                        packet[i + 1] = start[i];

                                if (sink_write(*dst, packet, (int)(depth + 1)) != (depth + 1))
                                        error = TGAERR_WRITE;

                                /* Reposition start and reset. */
                                start = end;
                                count = 0;
                                packet[0] = 0;
                        }
                        else
                        {
                                if (count == 0)
                                {
                                        /* Copy the raw pixel to the packet. */
                                        for (i = 0; i < depth; i++)
                                                packet[packet_index + i] = start[i];

                                        packet[0]++;

                                        start = end;
                                        packet_index += depth;
                                }

                                /* Write the raw packet if the packet is full or a run
                                 * has started or all the pixels have been processed. */
                                if ((packet[0] == 127) || (count != 0) || (pixels == 0))
                                {
                                        i = packet[0];
                                        packet[0]--;

                                        if (sink_write(*dst, packet, (int)((i * depth) + 1))
                                            != ((i * depth) + 1))
                                                error = TGAERR_WRITE;

                                        packet_index = 1;
                                        packet[0] = 0;
                                }
                        }
                }

                delete[] packet;
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

        // Targa::InvertImage, verbatim.
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

                /* 16-bit pixel layout is different that 24-bit and 32-bit. */
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

                                /* Next pixel */
                                pixel_count--;
                                buffer += depth;
                        }
                }
        }

        // Targa::Save, verbatim, writing into save_sink. The caller fills
        // Header/mPalette/mImage before calling (the SetImage/SetPalette
        // plumbing of the original collapses into direct member assignment).
        long Save(long flags, bool addextension)
        {
                long size;
                long depth;
                long error = 0;
                TGA2Footer footer;

                save_sink.clear();
                dst = &save_sink;

                Header.IDLength = 0;

                /* Set the ImageType for compression. */
                if (flags & TGAF_COMPRESS)
                {
                        switch (Header.ImageType)
                        {
                        case TGA_CMAPPED:
                        case TGA_TRUECOLOR:
                        case TGA_MONO:
                                Header.ImageType += 8;
                                break;

                        case TGA_CMAPPED_ENCODED:
                        case TGA_TRUECOLOR_ENCODED:
                        case TGA_MONO_ENCODED:
                                break;

                        /* Turn off compression for unknown types. */
                        default:
                                flags &= ~TGAF_COMPRESS;
                                break;
                        }
                }

                /* WRITE THE HEADER DATA SECTION */
                if (sink_write(*dst, &Header, sizeof(TGAHeader)) != sizeof(TGAHeader))
                        error = TGAERR_WRITE;

                /* WRITE THE COLORMAP (PALETTE) DATA SECTION */
                if (!error && (flags & TGAF_PAL) && (mPalette != nullptr)
                    && (Header.CMapLength > 0))
                {
                        depth = (Header.CMapDepth >> 3);
                        size = (Header.CMapLength * depth);

                        /* The original copies through a temppal buffer — the same
                         * bytes. */
                        if (sink_write(*dst, mPalette + (Header.CMapStart * depth), size) != size)
                                error = TGAERR_WRITE;
                }

                /* WRITE THE IMAGE DATA SECTION */
                if (!error && (flags & TGAF_IMAGE) && (mImage != nullptr))
                {
                        bool imageinverted;

                        /* Invert truecolor data. */
                        if ((Header.ImageType == TGA_TRUECOLOR)
                            || (Header.ImageType == TGA_TRUECOLOR_ENCODED))
                        {
                                InvertImage();
                                imageinverted = true;
                        }
                        else
                        {
                                imageinverted = false;
                        }

                        /* Write the image. */
                        if (flags & TGAF_COMPRESS)
                                EncodeImage();
                        else
                        {
                                depth = TGA_BytesPerPixel(Header.PixelDepth);
                                size = (((Header.Width * Header.Height)) * depth);

                                if (sink_write(*dst, mImage, size) != size)
                                        error = TGAERR_WRITE;
                        }

                        // Bug fix (IML) : If the image was inverted, invert it again
                        // to restore it to its prior state.
                        if (imageinverted)
                                InvertImage();
                }

                /* WRITE THE EXTENSION DATA SECTION — Mod (IML): optionally add
                 * an extension to the file. */
                if (addextension)
                {
                        if (!error)
                        {
                                mExtension.ExtSize = 495;
                                xstrlcpy(mExtension.SoftID, "Denzil's Targa Code",
                                        sizeof(mExtension.SoftID));
                                mExtension.SoftVerNumber = (1 * 100);
                                mExtension.SoftVerLetter = 0;

                                /* Save position of extension area. */
                                footer.Extension = (long)save_sink.size();

                                if (sink_write(*dst, &mExtension, sizeof(TGA2Extension))
                                    != sizeof(TGA2Extension))
                                        error = TGAERR_WRITE;
                        }
                }
                else
                {
                        footer.Extension = 0;
                }

                /* WRITE THE FOOTER DATA SECTION */
                if (!error)
                {
                        footer.Developer = 0;
                        static_assert(sizeof(TGA2_SIGNATURE) - 1 == sizeof(footer.Signature),
                                "TGA2 signature length mismatch");
                        memcpy(footer.Signature, TGA2_SIGNATURE, sizeof(footer.Signature));
                        footer.RsvdChar = '.';
                        footer.BZST = 0;

                        if (sink_write(*dst, &footer, sizeof(TGA2Footer)) != sizeof(TGA2Footer))
                                error = TGAERR_WRITE;
                }

                /* Close targa file. */
                Close();

                return (error);
        }
};

// --- golden output helpers ----------------------------------------------------

static FILE *g_out;

static std::string g_hex;

static const char *hex(const uint8_t *data, size_t len)
{
        g_hex.clear();
        if (len == 0)
                return "-";
        char buf[4];
        for (size_t i = 0; i < len; ++i)
        {
                snprintf(buf, sizeof(buf), "%02x", data[i]);
                g_hex += buf;
        }
        return g_hex.c_str();
}

// The reader view of a corpus: the verdict, the header as Load left it, the
// palette chunk, the decoded image, the footer/extension views.
static void emit_reader_view(const char *id, const Targa &t, long error)
{
        if (error != 0)
        {
                fprintf(g_out, "err %s\n", id);
                return;
        }
        fprintf(g_out, "ok %s\n", id);
        fprintf(g_out,
                "hdr %s %d %d %d %d %d %d %d %d %d %d %d %d\n", id,
                (int)t.Header.IDLength, (int)t.Header.ColorMapType, (int)t.Header.ImageType,
                (int)t.Header.CMapStart, (int)t.Header.CMapLength, (int)t.Header.CMapDepth,
                (int)t.Header.XOffset, (int)t.Header.YOffset, (int)t.Header.Width,
                (int)t.Header.Height, (int)t.Header.PixelDepth, (int)t.Header.ImageDescriptor);

        // The palette chunk the flags-Load holds (empty when there is none).
        long pal_size = 0;
        if (t.Header.ColorMapType == 1)
                pal_size = (long)t.Header.CMapLength * (t.Header.CMapDepth >> 3);
        bool has_pal = (t.mFlags & TGAF_PAL) && t.mPalette != nullptr && pal_size > 0;
        fprintf(g_out, "pal %s %s\n", id,
                has_pal ? hex(t.mPalette, (size_t)pal_size) : "-");

        // The image the flags-Load holds (empty when not allocated).
        long img_size = (long)t.Header.Width * t.Header.Height
                * TGA_BytesPerPixel(t.Header.PixelDepth);
        bool has_img = (t.mFlags & TGAF_IMAGE) && t.mImage != nullptr && img_size > 0;
        fprintf(g_out, "img %s %s\n", id,
                has_img ? hex(t.mImage, (size_t)img_size) : "-");

        if (t.mFooterInfo)
                fprintf(g_out, "footer %s %ld %ld\n", id, t.mFooterExt, t.mFooterDev);
        if (t.mFlags & TGAF_TGA2)
                fprintf(g_out, "ext %s %s\n", id,
                        hex((const uint8_t *)&t.mExtension, sizeof(TGA2Extension)));
}

// Open + Load(TGAF_IMAGE | TGAF_PAL, invert_image = true) — the library
// default shape the Rust `TgaFile::parse` transcribes — over a byte blob.
static void emit_read(const char *id, const std::vector<uint8_t> &bytes)
{
        fprintf(g_out, "file %s %s\n", id, hex(bytes.data(), bytes.size()));
        Targa targa;
        long error = targa.Open_Read(bytes.data(), bytes.size());
        if (error == 0)
                error = targa.Load_Flags(TGAF_IMAGE | TGAF_PAL, true);
        emit_reader_view(id, targa, error);
}

// A writer corpus: fill the Targa in memory, Save, and emit both the saved
// bytes (`file` and `gen`) and the reader view of them.
static void emit_save(const char *id, Targa &targa, long flags, bool addextension)
{
        long error = targa.Save(flags, addextension);
        if (error != 0)
        {
                fprintf(stderr, "error: save corpus %s failed (%ld)\n", id, error);
                exit(1);
        }
        std::vector<uint8_t> bytes = targa.save_sink;
        fprintf(g_out, "file %s %s\n", id, hex(bytes.data(), bytes.size()));
        fprintf(g_out, "gen %s %s\n", id, hex(bytes.data(), bytes.size()));
        fprintf(g_out, "save %s %d %d %d %d\n", id, (flags & TGAF_IMAGE) ? 1 : 0,
                (flags & TGAF_PAL) ? 1 : 0, (flags & TGAF_COMPRESS) ? 1 : 0,
                addextension ? 1 : 0);

        // The reader view of the saved bytes (a fresh Targa).
        Targa reader;
        long rerror = reader.Open_Read(bytes.data(), bytes.size());
        if (rerror == 0)
                rerror = reader.Load_Flags(TGAF_IMAGE | TGAF_PAL, true);
        emit_reader_view(id, reader, rerror);
}

// --- corpus builders ----------------------------------------------------------

static void put(std::vector<uint8_t> &v, const void *data, size_t len)
{
        const uint8_t *p = (const uint8_t *)data;
        v.insert(v.end(), p, p + len);
}

static void put_byte(std::vector<uint8_t> &v, uint8_t b) { v.push_back(b); }

static void put_header(std::vector<uint8_t> &v, const TGAHeader &h) { put(v, &h, sizeof(h)); }

// A deterministic pixel formula.
static uint8_t px(int i, int salt) { return (uint8_t)((i * 37 + salt * 11 + 5) & 0xff); }

// A 26-byte non-signature tail: short of the TGA 2.0 signature, so the
// footer sniff treats the file as 1.0 (the original reads the last 26 bytes
// regardless and only the signature decides).
static void put_plain_tail(std::vector<uint8_t> &v) { v.resize(v.size() + 26, 0); }

static void footer_bytes(std::vector<uint8_t> &v, long extension)
{
        TGA2Footer footer{};
        footer.Extension = extension;
        footer.Developer = 0;
        memcpy(footer.Signature, TGA2_SIGNATURE, sizeof(footer.Signature));
        footer.RsvdChar = '.';
        footer.BZST = 0;
        put(v, &footer, sizeof(footer));
}

int main(int argc, char **argv)
{
        if (argc != 2)
        {
                fprintf(stderr, "usage: gen_tga_golden <output-file>\n");
                return 1;
        }
        g_out = fopen(argv[1], "w");
        if (!g_out)
        {
                perror("fopen");
                return 1;
        }

        fprintf(g_out, "# golden vectors for rzh-formats::tga — generated by\n");
        fprintf(g_out, "# tools/differential/gen_tga_golden.cpp (the TARGA.cpp\n");
        fprintf(g_out, "# transcription); do not edit by hand.\n");

        // --- raw truecolor, every origin ---------------------------------------
        {
                TGAHeader h{};
                h.ImageType = TGA_TRUECOLOR;
                h.Width = 4;
                h.Height = 3;
                h.PixelDepth = 24;

                // Top-left origin: the loader YFlips.
                h.ImageDescriptor = TGAIDF_YORIGIN;
                std::vector<uint8_t> bytes;
                put_header(bytes, h);
                for (int i = 0; i < 12; ++i)
                {
                        uint8_t p[3] = {px(i, 1), px(i, 2), px(i, 3)};
                        put(bytes, p, 3);
                }
                put_plain_tail(bytes);
                emit_read("raw24_topleft", bytes);

                // Bottom-left origin: no flip.
                h.ImageDescriptor = 0;
                bytes.clear();
                put_header(bytes, h);
                for (int i = 0; i < 12; ++i)
                {
                        uint8_t p[3] = {px(i, 1), px(i, 2), px(i, 3)};
                        put(bytes, p, 3);
                }
                put_plain_tail(bytes);
                emit_read("raw24_bottomleft", bytes);

                // Left origin: the loader XFlips.
                h.Width = 3;
                h.Height = 2;
                h.ImageDescriptor = TGAIDF_XORIGIN;
                bytes.clear();
                put_header(bytes, h);
                for (int i = 0; i < 6; ++i)
                {
                        uint8_t p[3] = {px(i, 4), px(i, 5), px(i, 6)};
                        put(bytes, p, 3);
                }
                put_plain_tail(bytes);
                emit_read("raw24_leftorigin", bytes);
        }

        // --- 32-bit with attribute bits ----------------------------------------
        {
                TGAHeader h{};
                h.ImageType = TGA_TRUECOLOR;
                h.Width = 2;
                h.Height = 2;
                h.PixelDepth = 32;
                h.ImageDescriptor = TGAIDF_YORIGIN | 8; // 8 attribute bits
                std::vector<uint8_t> bytes;
                put_header(bytes, h);
                for (int i = 0; i < 4; ++i)
                {
                        uint8_t p[4] = {px(i, 7), px(i, 8), px(i, 9), px(i, 10)};
                        put(bytes, p, 4);
                }
                put_plain_tail(bytes);
                emit_read("raw32_alpha", bytes);
        }

        // --- 16-bit (InvertImage skips depth 2) ---------------------------------
        {
                TGAHeader h{};
                h.ImageType = TGA_TRUECOLOR;
                h.Width = 2;
                h.Height = 2;
                h.PixelDepth = 16;
                std::vector<uint8_t> bytes;
                put_header(bytes, h);
                for (int i = 0; i < 4; ++i)
                {
                        uint8_t p[2] = {px(i, 11), px(i, 12)};
                        put(bytes, p, 2);
                }
                put_plain_tail(bytes);
                emit_read("raw16", bytes);
        }

        // --- RLE truecolor, mixed packets ---------------------------------------
        {
                TGAHeader h{};
                h.ImageType = TGA_TRUECOLOR_ENCODED;
                h.Width = 7;
                h.Height = 3; // 21 pixels
                h.PixelDepth = 24;
                h.ImageDescriptor = TGAIDF_YORIGIN;
                std::vector<uint8_t> bytes;
                put_header(bytes, h);
                // Packets: run(3), raw(2), run(1), raw(3), run(6), raw(6) = 21.
                uint8_t run3[3] = {10, 20, 30};
                put_byte(bytes, 0x82);
                put(bytes, run3, 3);
                uint8_t raw2[6] = {1, 2, 3, 4, 5, 6};
                put_byte(bytes, 0x01);
                put(bytes, raw2, 6);
                uint8_t run1[3] = {99, 98, 97};
                put_byte(bytes, 0x80);
                put(bytes, run1, 3);
                uint8_t raw3[9] = {11, 12, 13, 14, 15, 16, 17, 18, 19};
                put_byte(bytes, 0x02);
                put(bytes, raw3, 9);
                uint8_t run6[3] = {70, 80, 90};
                put_byte(bytes, 0x85);
                put(bytes, run6, 3);
                uint8_t raw6[18] = {21, 22, 23, 24, 25, 26, 27, 28, 29,
                        30, 31, 32, 33, 34, 35, 36, 37, 38};
                put_byte(bytes, 0x05);
                put(bytes, raw6, 18);
                put_plain_tail(bytes);
                emit_read("rle24_mixed", bytes);
        }

        // --- RLE with a saturated 128-run ---------------------------------------
        {
                TGAHeader h{};
                h.ImageType = TGA_TRUECOLOR_ENCODED;
                h.Width = 130;
                h.Height = 1;
                h.PixelDepth = 32;
                std::vector<uint8_t> bytes;
                put_header(bytes, h);
                uint8_t p[4] = {9, 8, 7, 6};
                put_byte(bytes, 0xFF); // run of 128
                put(bytes, p, 4);
                put_byte(bytes, 0x81); // run of 2
                put(bytes, p, 4);
                put_plain_tail(bytes);
                emit_read("rle32_run128", bytes);
        }

        // --- RLE with trailing junk (the extra bytes are never read) ------------
        {
                TGAHeader h{};
                h.ImageType = TGA_TRUECOLOR_ENCODED;
                h.Width = 2;
                h.Height = 1;
                h.PixelDepth = 24;
                std::vector<uint8_t> bytes;
                put_header(bytes, h);
                uint8_t p[3] = {5, 5, 5};
                put_byte(bytes, 0x81);
                put(bytes, p, 3);
                put_byte(bytes, 0xAA); // trailing junk
                put_byte(bytes, 0xBB);
                put_byte(bytes, 0xCC);
                put_plain_tail(bytes);
                emit_read("rle24_trailing", bytes);
        }

        // --- colormapped, 24-bit entries ----------------------------------------
        {
                TGAHeader h{};
                h.ImageType = TGA_CMAPPED;
                h.ColorMapType = 1;
                h.CMapStart = 0;
                h.CMapLength = 4;
                h.CMapDepth = 24;
                h.Width = 4;
                h.Height = 2;
                h.PixelDepth = 8;
                h.ImageDescriptor = TGAIDF_YORIGIN;
                std::vector<uint8_t> bytes;
                put_header(bytes, h);
                uint8_t cmap[12] = {200, 100, 50, 10, 20, 30, 240, 230, 220, 1, 2, 3};
                put(bytes, cmap, 12);
                uint8_t idx[8] = {0, 1, 2, 3, 3, 2, 1, 0};
                put(bytes, idx, 8);
                put_plain_tail(bytes);
                emit_read("cmapped8", bytes);
        }

        // --- colormapped, 32-bit entries ----------------------------------------
        {
                TGAHeader h{};
                h.ImageType = TGA_CMAPPED;
                h.ColorMapType = 1;
                h.CMapLength = 2;
                h.CMapDepth = 32;
                h.Width = 2;
                h.Height = 1;
                h.PixelDepth = 8;
                std::vector<uint8_t> bytes;
                put_header(bytes, h);
                uint8_t cmap[8] = {9, 9, 9, 9, 8, 8, 8, 8};
                put(bytes, cmap, 8);
                uint8_t idx[2] = {1, 0};
                put(bytes, idx, 2);
                put_plain_tail(bytes);
                emit_read("cmapped8_cmapdepth32", bytes);
        }

        // --- monochrome ----------------------------------------------------------
        {
                TGAHeader h{};
                h.ImageType = TGA_MONO;
                h.Width = 3;
                h.Height = 2;
                h.PixelDepth = 8;
                std::vector<uint8_t> bytes;
                put_header(bytes, h);
                uint8_t pxs[6] = {0, 64, 128, 192, 255, 7};
                put(bytes, pxs, 6);
                put_plain_tail(bytes);
                emit_read("mono8", bytes);
        }

        // --- an ID field is skipped ----------------------------------------------
        {
                TGAHeader h{};
                h.ImageType = TGA_TRUECOLOR;
                h.IDLength = 8;
                h.Width = 2;
                h.Height = 1;
                h.PixelDepth = 24;
                std::vector<uint8_t> bytes;
                put_header(bytes, h);
                uint8_t id[8] = {'R', 'Z', 'H', 'I', 'D', '0', '0', '1'};
                put(bytes, id, 8);
                uint8_t p[6] = {1, 2, 3, 4, 5, 6};
                put(bytes, p, 6);
                put_plain_tail(bytes);
                emit_read("id_field", bytes);
        }

        // --- Targa 2.0 with an extension area ------------------------------------
        {
                TGAHeader h{};
                h.ImageType = TGA_TRUECOLOR;
                h.Width = 2;
                h.Height = 1;
                h.PixelDepth = 24;
                std::vector<uint8_t> bytes;
                put_header(bytes, h);
                uint8_t p[6] = {1, 2, 3, 4, 5, 6};
                put(bytes, p, 6);
                long ext_offset = (long)bytes.size();

                TGA2Extension ext{};
                ext.ExtSize = 495;
                xstrlcpy(ext.AuthName, "Westwood", sizeof(ext.AuthName));
                ext.DateMonth = 3;
                ext.DateDay = 7;
                ext.DateYear = 2003;
                ext.TimeHour = 12;
                ext.TimeMinute = 34;
                ext.TimeSecond = 56;
                ext.KeyColor = 0x11223344;
                ext.AspectNumer = 1;
                ext.AspectDenom = 1;
                ext.Attributes = 3;
                put(bytes, &ext, sizeof(ext));

                footer_bytes(bytes, ext_offset);
                emit_read("tga2_ext", bytes);
        }

        // --- Targa 2.0 footer without an extension --------------------------------
        {
                TGAHeader h{};
                h.ImageType = TGA_MONO;
                h.Width = 2;
                h.Height = 1;
                h.PixelDepth = 8;
                std::vector<uint8_t> bytes;
                put_header(bytes, h);
                uint8_t pxs[2] = {3, 4};
                put(bytes, pxs, 2);
                footer_bytes(bytes, 0);
                emit_read("tga2_noext", bytes);
        }

        // --- rejections ------------------------------------------------------------
        {
                // Shorter than the footer: the seek fails, the file never opens.
                TGAHeader h{};
                h.ImageType = TGA_TRUECOLOR;
                h.Width = 2;
                h.Height = 1;
                h.PixelDepth = 24;
                std::vector<uint8_t> bytes;
                put_header(bytes, h);
                uint8_t p[6] = {1, 2, 3, 4, 5, 6};
                put(bytes, p, 6);
                emit_read("err_short_file", bytes);

                // The image read runs past EOF on a footer-sized file.
                TGAHeader h2{};
                h2.ImageType = TGA_MONO;
                h2.Width = 4;
                h2.Height = 4;
                h2.PixelDepth = 8;
                std::vector<uint8_t> bytes2;
                put_header(bytes2, h2);
                bytes2.resize(26, 0);
                emit_read("err_truncated_image", bytes2);

                // The RLE stream ends past EOF: the footer sniff on this
                // 27-byte file finds no signature, the open succeeds, and
                // the image read (16 bytes at cursor 19) runs past the end
                // inside DecodeImage.
                TGAHeader h3{};
                h3.ImageType = TGA_TRUECOLOR_ENCODED;
                h3.Width = 4;
                h3.Height = 4;
                h3.PixelDepth = 8;
                std::vector<uint8_t> bytes3;
                put_header(bytes3, h3);
                put_byte(bytes3, 0x0F);          // sixteen raw pixels...
                uint8_t half[8] = {1, 2, 3, 4, 5, 6, 7, 8}; // ...8 arrive
                put(bytes3, half, 8);
                emit_read("err_rle_truncated", bytes3);

                // TGA_MONO_ENCODED: the original's switch has no case for it.
                TGAHeader h4{};
                h4.ImageType = TGA_MONO_ENCODED;
                h4.Width = 2;
                h4.Height = 1;
                h4.PixelDepth = 8;
                std::vector<uint8_t> bytes4;
                put_header(bytes4, h4);
                put_byte(bytes4, 0x81);
                put_byte(bytes4, 7);
                put_plain_tail(bytes4);
                emit_read("err_mono_encoded", bytes4);

                // TGA_NOIMAGE: also unsupported by the loader's switch.
                TGAHeader h5{};
                h5.ImageType = TGA_NOIMAGE;
                h5.Width = 2;
                h5.Height = 1;
                h5.PixelDepth = 24;
                std::vector<uint8_t> bytes5;
                put_header(bytes5, h5);
                put_plain_tail(bytes5);
                emit_read("err_type0", bytes5);

                // The ID field points the cursor past EOF.
                TGAHeader h6{};
                h6.ImageType = TGA_TRUECOLOR;
                h6.IDLength = 100;
                h6.Width = 2;
                h6.Height = 1;
                h6.PixelDepth = 24;
                std::vector<uint8_t> bytes6;
                put_header(bytes6, h6);
                bytes6.resize(bytes6.size() + 60, 0);
                emit_read("err_id_field", bytes6);

                // A TGA 2.0 footer whose extension offset overshoots the file.
                TGAHeader h7{};
                h7.ImageType = TGA_MONO;
                h7.Width = 1;
                h7.Height = 1;
                h7.PixelDepth = 8;
                std::vector<uint8_t> bytes7;
                put_header(bytes7, h7);
                put_byte(bytes7, 1);
                footer_bytes(bytes7, 100000);
                emit_read("err_bad_extension", bytes7);

                // The colormap chunk is cut short: a 26-byte file (no
                // signature tail) whose 12-byte chunk read runs past the end.
                TGAHeader h8{};
                h8.ImageType = TGA_CMAPPED;
                h8.ColorMapType = 1;
                h8.CMapLength = 4;
                h8.CMapDepth = 24;
                h8.Width = 2;
                h8.Height = 1;
                h8.PixelDepth = 8;
                std::vector<uint8_t> bytes8;
                put_header(bytes8, h8);
                uint8_t short_cmap[8] = {1, 2, 3, 4, 5, 6, 7, 8}; // 8 of 12 bytes
                put(bytes8, short_cmap, 8);
                emit_read("err_cmap_short", bytes8);
        }

        // --- writer corpora ---------------------------------------------------------
        {
                // The screenshot shape: a 24-bit truecolor header around an
                // in-memory RGB image, saved uncompressed without extension
                // (WW3D.cpp Make_Screen_Shoot).
                Targa t;
                t.Header.ImageType = TGA_TRUECOLOR;
                t.Header.Width = 5;
                t.Header.Height = 3;
                t.Header.PixelDepth = 24;
                t.mImage = new uint8_t[5 * 3 * 3];
                for (int i = 0; i < 15 * 3; ++i)
                        t.mImage[i] = px(i, 21);
                t.mFlags |= TGAF_IMAGE;
                emit_save("save_screenshot24", t, TGAF_IMAGE, false);

                // Same image, compressed (the ImagePacker shape). Save bumps
                // the member type to TGA_TRUECOLOR_ENCODED.
                emit_save("save_compressed24", t, TGAF_IMAGE | TGAF_COMPRESS, false);

                // With an extension area — a fresh Targa with the original
                // 24-bit header (Save's type bump must not leak between
                // corpora; each real caller builds its own Targa).
                Targa t2;
                t2.Header.ImageType = TGA_TRUECOLOR;
                t2.Header.Width = 5;
                t2.Header.Height = 3;
                t2.Header.PixelDepth = 24;
                t2.mImage = new uint8_t[5 * 3 * 3];
                for (int i = 0; i < 15 * 3; ++i)
                        t2.mImage[i] = px(i, 21);
                t2.mFlags |= TGAF_IMAGE;
                emit_save("save_tga2_ext", t2, TGAF_IMAGE, true);

                // Colormapped save with the palette block.
                Targa tc;
                tc.Header.ImageType = TGA_CMAPPED;
                tc.Header.ColorMapType = 1;
                tc.Header.CMapLength = 4;
                tc.Header.CMapDepth = 24;
                tc.Header.Width = 4;
                tc.Header.Height = 2;
                tc.Header.PixelDepth = 8;
                tc.mPalette = new uint8_t[12]{200, 100, 50, 10, 20, 30, 240, 230, 220, 1, 2, 3};
                tc.mImage = new uint8_t[8]{0, 1, 2, 3, 3, 2, 1, 0};
                tc.mFlags |= TGAF_IMAGE | TGAF_PAL;
                emit_save("save_cmapped8", tc, TGAF_IMAGE | TGAF_PAL, false);

                // Mono + compression: Save writes type 11 — a file the
                // original's own loader then rejects (no switch case).
                Targa tm;
                tm.Header.ImageType = TGA_MONO;
                tm.Header.Width = 4;
                tm.Header.Height = 2;
                tm.Header.PixelDepth = 8;
                tm.mImage = new uint8_t[8]{1, 1, 1, 1, 2, 2, 3, 4};
                tm.mFlags |= TGAF_IMAGE;
                emit_save("save_mono_compressed_type11", tm, TGAF_IMAGE | TGAF_COMPRESS, false);

                // An encoder-branch corpus: raw packets, interrupted runs,
                // a saturated 128-run and a trailing run in one image.
                Targa tr;
                tr.Header.ImageType = TGA_TRUECOLOR;
                tr.Header.Width = 389;
                tr.Header.Height = 1;
                tr.Header.PixelDepth = 8;
                // A(1) B(1) C(2) D(127) E(128) F(129) G(1) = 389 pixels.
                uint8_t *img = new uint8_t[389];
                int o = 0;
                img[o++] = 10;                    // A
                img[o++] = 20;                    // B
                img[o++] = 30, img[o++] = 30;     // C C
                for (int i = 0; i < 127; ++i)
                        img[o++] = 40;            // D x127
                for (int i = 0; i < 128; ++i)
                        img[o++] = 50;            // E x128
                for (int i = 0; i < 129; ++i)
                        img[o++] = 60;            // F x129
                img[o++] = 70;                    // G
                tr.mImage = img;
                tr.mFlags |= TGAF_IMAGE;
                emit_save("save_compressed_branches", tr, TGAF_IMAGE | TGAF_COMPRESS, false);
        }

        fclose(g_out);
        return 0;
}
