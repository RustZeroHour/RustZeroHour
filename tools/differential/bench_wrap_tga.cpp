// SPDX-License-Identifier: GPL-3.0-or-later
//
// C++ baseline timing wrapper around the gen_tga_golden.cpp transcription.
//
// Times the verbatim TARGA.cpp class — the texture-load path (Open +
// Load(TGAF_IMAGE | TGAF_PAL)) and the screenshot/ImagePacker save paths —
// on the same formula-generated 256×256 corpora as the Rust `tga/…`
// criterion benches. The RLE corpus is built here through the transcribed
// encoder and on the Rust side through the port's own encoder; the goldens
// pin the two byte-identical, so both sides parse **identical bytes**.
//
// The generator's own main() is renamed out of the way; nothing in it is
// modified.
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

#define main gen_tga_generator_main
#include "gen_tga_golden.cpp"
#undef main

extern "C" {

// A corpus Targa in the state the screenshot path builds: a truecolor
// header around the formula image, no palette, no footer. depth is 3
// (24-bit raw corpus) or 4 (32-bit RLE corpus). Keep the formula in
// lockstep with crates/rzh-formats/benches/rzh_formats.rs.
void *bench_tga_targa_new(int depth)
{
        auto *t = new Targa();
        t->Header.ImageType = TGA_TRUECOLOR;
        t->Header.Width = 256;
        t->Header.Height = 256;
        t->Header.PixelDepth = depth == 3 ? 24 : 32;
        size_t pixels = 256u * 256u;
        t->mImage = new uint8_t[pixels * depth];
        for (size_t p = 0; p < pixels; ++p)
        {
                for (int c = 0; c < depth; ++c)
                {
                        uint8_t v = (p % 8 < 3)
                                ? (uint8_t)(37 + 11 * c)
                                : (uint8_t)((p * 7 + 13 + (size_t)c * 17) & 0xff);
                        t->mImage[p * depth + c] = v;
                }
        }
        t->mFlags |= TGAF_IMAGE;
        return t;
}

void bench_tga_targa_free(void *handle) { delete (Targa *)handle; }

// Save(TGAF_IMAGE) — the screenshot write path, timed exactly like the
// Rust `to_bytes(SaveFlags::IMAGE)` closure (the inversion runs on a copy
// in the port; the original inverts and restores in place). Returns the
// byte count.
long bench_tga_write(void *handle)
{
        auto *t = (Targa *)handle;
        if (t->Save(TGAF_IMAGE, false) != 0)
                return -1;
        return (long)t->save_sink.size();
}

// Save(TGAF_IMAGE | TGAF_COMPRESS) — the ImagePacker path.
long bench_tga_encode(void *handle)
{
        auto *t = (Targa *)handle;
        if (t->Save(TGAF_IMAGE | TGAF_COMPRESS, false) != 0)
                return -1;
        return (long)t->save_sink.size();
}

// Untimed corpus materialization: Save and copy the bytes out (compress
// selects TGAF_IMAGE | TGAF_COMPRESS over TGAF_IMAGE).
long bench_tga_materialize(void *handle, int compress, uint8_t *out, size_t cap)
{
        auto *t = (Targa *)handle;
        long flags = compress ? (TGAF_IMAGE | TGAF_COMPRESS) : TGAF_IMAGE;
        if (t->Save(flags, false) != 0)
                return -1;
        if (t->save_sink.size() > cap)
                return -1;
        memcpy(out, t->save_sink.data(), t->save_sink.size());
        return (long)t->save_sink.size();
}

// Open + Load(TGAF_IMAGE | TGAF_PAL, invert_image = true) — the shape the
// Rust `TgaFile::parse` transcribes.
uint32_t bench_tga_parse(const uint8_t *data, size_t len)
{
        Targa t;
        if (t.Open_Read(data, len) != 0)
                return 1;
        return (uint32_t)t.Load_Flags(TGAF_IMAGE | TGAF_PAL, true);
}

} // extern "C"
