// SPDX-License-Identifier: GPL-3.0-or-later
//
// C++ baseline timing wrapper around the gen_dds_golden.cpp
// transcription.
//
// Times the DDS load path the texture loader drives — the
// constructor's header decode and mip-level layout plus Load()'s read
// window — on the same formula-generated 512x512 DXT1 ten-level corpus
// as the Rust `dds/parse_x16k` criterion bench (16,384 level-0
// blocks, 174,904 bytes), and the DXT5 → A8R8G8B8 conversion walk
// (`Copy_Level_To_Surface`'s decompressing branch, 256 blocks over a
// 64x64 one-level corpus) as the Rust `dds/convert_dxt5_x256` bench.
// The corpora are built here with the generator's own builders; the
// corpus sizes and formula match the Rust side (the size asserts
// document the lockstep).
//
// The generator's own main() is renamed out of the way; nothing in it
// is modified.
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

#define main gen_dds_generator_main
#include "gen_dds_golden.cpp"
#undef main

#include <cassert>

// The corpus shapes (match the Rust baseline).
#define DDS_BENCH_PARSE_DIM 512
#define DDS_BENCH_PARSE_MIPS 10
#define DDS_BENCH_CONV_DIM 64

// Builds the 512x512 DXT1 ten-level parse corpus — keep in lockstep
// with crates/rzh-formats/benches/rzh_formats.rs's
// dds_parse_corpus().
static std::vector<uint8_t> build_dds_parse_corpus()
{
        std::vector<uint8_t> bytes;
        bytes.insert(bytes.end(), {'D', 'D', 'S', ' '});
        auto put = [&](unsigned x) { put_u32(bytes, x); };
        put(124);  // Size
        put(0x1007); // Flags
        put(DDS_BENCH_PARSE_DIM); // Height
        put(DDS_BENCH_PARSE_DIM); // Width
        put(0);   // Pitch
        put(0);   // Depth
        put(DDS_BENCH_PARSE_MIPS); // MipMapCount
        put(0);   // AlphaBitDepth
        put(0);   // Reserved
        put(0);   // Surface
        for (int i = 0; i < 8; i++)
                put(0); // the four color keys
        put(32);  // PixelFormat.Size
        put(4);   // PixelFormat.Flags
        put(MAKEFOURCC('D', 'X', 'T', '1'));
        for (int i = 0; i < 5; i++)
                put(0); // the pixel-format union tail
        put(0x1000); // Caps
        put(0);   // Caps2
        put(0);   // Caps3
        put(0);   // Caps4
        put(0);   // TextureStage
        assert(bytes.size() == 128);

        // the mip chain: the constructor's own size walk
        unsigned size = (DDS_BENCH_PARSE_DIM / 4) * (DDS_BENCH_PARSE_DIM / 4) * 8;
        size_t at = 0;
        for (unsigned level = 0; level < DDS_BENCH_PARSE_MIPS; level++)
        {
                std::vector<uint8_t> chunk = formula_at(128 + at, size);
                bytes.insert(bytes.end(), chunk.begin(), chunk.end());
                at += size;
                if (size > 16)
                        size /= 4;
        }
        return bytes;
}

// Builds the 64x64 DXT5 one-level conversion corpus — keep in lockstep
// with the Rust dds_convert_corpus().
static std::vector<uint8_t> build_dds_convert_corpus()
{
        std::vector<uint8_t> bytes;
        bytes.insert(bytes.end(), {'D', 'D', 'S', ' '});
        auto put = [&](unsigned x) { put_u32(bytes, x); };
        put(124);  // Size
        put(0x1007); // Flags
        put(DDS_BENCH_CONV_DIM); // Height
        put(DDS_BENCH_CONV_DIM); // Width
        put(0);   // Pitch
        put(0);   // Depth
        put(1);   // MipMapCount
        put(0);   // AlphaBitDepth
        put(0);   // Reserved
        put(0);   // Surface
        for (int i = 0; i < 8; i++)
                put(0); // the four color keys
        put(32);  // PixelFormat.Size
        put(4);   // PixelFormat.Flags
        put(MAKEFOURCC('D', 'X', 'T', '5'));
        for (int i = 0; i < 5; i++)
                put(0); // the pixel-format union tail
        put(0x1000); // Caps
        put(0);   // Caps2
        put(0);   // Caps3
        put(0);   // Caps4
        put(0);   // TextureStage
        assert(bytes.size() == 128);
        std::vector<uint8_t> chunk = formula_at(
            128, (DDS_BENCH_CONV_DIM / 4) * (DDS_BENCH_CONV_DIM / 4) * 16);
        bytes.insert(bytes.end(), chunk.begin(), chunk.end());
        return bytes;
}

extern "C" {

// Materializes the parse corpus into `out` (capacity `cap`); returns
// the total length, or a negative value when the buffer is too small.
long bench_dds_parse_materialize(uint8_t *out, size_t cap)
{
        std::vector<uint8_t> corpus = build_dds_parse_corpus();
        if (corpus.size() > cap)
                return -1;
        memcpy(out, corpus.data(), corpus.size());
        return (long)corpus.size();
}

// One constructor + Load over the corpus — the texture loader's
// per-texture entry cost. The handle views the caller's bytes
// directly (the harness's corpus outlives the timed loop). Returns
// the loaded mip count (0 on failure).
unsigned bench_dds_parse(const uint8_t *data, size_t len)
{
        ByteFile file(data, len);
        DDSFileClass dds("bench.dds", 0, file);
        if (!dds.Is_Available())
                return 0;
        if (!dds.Load(file))
                return 0;
        return dds.Get_Mip_Level_Count();
}

// Materializes the conversion corpus; same contract as above.
long bench_dds_convert_materialize(uint8_t *out, size_t cap)
{
        std::vector<uint8_t> corpus = build_dds_convert_corpus();
        if (corpus.size() > cap)
                return -1;
        memcpy(out, corpus.data(), corpus.size());
        return (long)corpus.size();
}

// The DXT5 → A8R8G8B8 conversion walk over the whole level (256
// blocks). Returns the bytes written (0 on failure).
unsigned bench_dds_convert(const uint8_t *data, size_t len, uint8_t *dest, size_t dest_cap)
{
        if (dest_cap < DDS_BENCH_CONV_DIM * DDS_BENCH_CONV_DIM * 4u)
                return 0;
        ByteFile file(data, len);
        DDSFileClass dds("bench.dds", 0, file);
        if (!dds.Is_Available())
                return 0;
        if (!dds.Load(file))
                return 0;
        dds.Copy_Level_To_Surface(
            0, WW3D_FORMAT_A8R8G8B8, DDS_BENCH_CONV_DIM, DDS_BENCH_CONV_DIM,
            dest, DDS_BENCH_CONV_DIM * 4, Vector3(0.0f, 0.0f, 0.0f));
        return DDS_BENCH_CONV_DIM * DDS_BENCH_CONV_DIM * 4u;
}

} // extern "C"
