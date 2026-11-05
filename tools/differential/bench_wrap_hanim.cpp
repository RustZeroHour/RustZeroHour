// SPDX-License-Identifier: GPL-3.0-or-later
//
// C++ baseline timing wrapper around the gen_hanim_golden.cpp
// transcription.
//
// Times the hrawanim.cpp Load_W3D data path and the frame queries the
// tree update drives:
//
// - `hanim/parse`: the whole chunk walk — the 44-byte header decode, the
//   tree lookup, and the channel loop (a 16-byte struct read, the frame
//   array allocation and copy, and the routing per channel) over a
//   64-bone, 128-frame corpus with X/Y/Z channels on half the bones and
//   quaternion channels on the rest;
// - `hanim/xforms`: the per-frame Anim_Update shape — Get_Transform over
//   every bone at 16 sampled frames (the Fast_Slerp/Fast_Acos/Fast_Sin
//   table path plus Build_Matrix3D and the translation lerp), on the
//   same corpus loaded once (untimed) beforehand.
//
// The corpus is built here with this file's own builder; the sizes and
// formulas match the Rust `hanim/*` criterion benches (the size assert
// documents the lockstep).
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
//
// This program is distributed in the hope that it will be useful, but
// WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU
// General Public License for more details.

#define main gen_hanim_generator_main
#include "gen_hanim_golden.cpp"
#undef main

// The corpus shape (matches the Rust baseline).
#define HANIM_BENCH_PIVOTS 64
#define HANIM_BENCH_FRAMES 128
#define HANIM_BENCH_QUERY_FRAMES 16

// The corpus word formulas — pure functions of (pivot, channel, frame,
// component) so the Rust bench replicates the identical bytes without
// offset bookkeeping.
//
// Translation words take any bit pattern (the queries only lerp them);
// quaternion words are built as 0x3C000000 | (b << 17) — small positive
// components that keep every key pair's dot product inside the
// fast-table slerp domain, so the query bench times the table path the
// game's update loop overwhelmingly takes.
static float hanim_word(uint32_t piv, uint32_t ty, uint32_t f)
{
        uint32_t bits = f * 0x01010101u ^ piv * 0x0010001u ^ ty * 0x10010010u ^ 0x3C000000u;
        float v;
        memcpy(&v, &bits, 4);
        return v;
}

static float hanim_quat_word(uint32_t piv, uint32_t f, uint32_t k)
{
        uint32_t bits = 0x3C000000u | (((f * 7u + k * 13u + piv * 3u) & 0x3Fu) << 17);
        float v;
        memcpy(&v, &bits, 4);
        return v;
}

// Builds the animation-shaped corpus — keep in lockstep with
// crates/rzh-formats/benches/rzh_formats.rs's hanim_corpus(): pivots
// 0..31 carry X/Y/Z channels (128 frames of 1 float each), pivots
// 32..63 carry a quaternion channel (128 frames of 4 components).
static void build_hanim_corpus(ChunkBuild &b)
{
        w_anim_header(b, W3D_MAKE_VERSION(4, 0), "Run", "bench", HANIM_BENCH_FRAMES, 30);

        for (int piv = 0; piv < HANIM_BENCH_PIVOTS; piv++) {
                if (piv < HANIM_BENCH_PIVOTS / 2) {
                        for (int ty = ANIM_CHANNEL_X; ty <= ANIM_CHANNEL_Z; ty++) {
                                std::vector<float> data(HANIM_BENCH_FRAMES);
                                for (int f = 0; f < HANIM_BENCH_FRAMES; f++) {
                                        data[f] = hanim_word((uint32_t)piv, (uint32_t)ty, (uint32_t)f);
                                }
                                w_motion_channel(b, 0, HANIM_BENCH_FRAMES - 1, 1, ty, piv, data, {});
                        }
                } else {
                        std::vector<float> data(HANIM_BENCH_FRAMES * 4);
                        for (int f = 0; f < HANIM_BENCH_FRAMES; f++) {
                                for (int k = 0; k < 4; k++) {
                                        data[f * 4 + k] = hanim_quat_word((uint32_t)piv, (uint32_t)f, (uint32_t)k);
                                }
                        }
                        w_motion_channel(b, 0, HANIM_BENCH_FRAMES - 1, 4, ANIM_CHANNEL_Q, piv, data, {});
                }
        }
}

extern "C" {

// The generator's main() (renamed away) owns the tree table and the
// WWMath init; the wrapper fills the one tree its corpus resolves
// against and initializes the fast tables on first use.
static void ensure_bench_tables()
{
        if (g_trees.empty()) {
                g_trees.push_back({{'b', 'e', 'n', 'c', 'h'}, HANIM_BENCH_PIVOTS});
        }
        WWMath_Init();
}

// Materializes the corpus into `out` (capacity `cap`); returns the total
// length, or a negative value when the buffer is too small.
long bench_hanim_materialize(uint8_t *out, size_t cap)
{
        ensure_bench_tables();
        ChunkBuild b;
        build_hanim_corpus(b);
        if (b.bytes.size() > cap)
                return -1;
        memcpy(out, b.bytes.data(), b.bytes.size());
        return (long)b.bytes.size();
}

// One full load of the corpus through the transcription; returns a small
// checksum of the loaded state so the timing loop cannot be elided.
std::uint64_t bench_hanim_load(const uint8_t *data, size_t len)
{
        SrcFile file;
        file.data = data;
        file.len = len;
        file.pos = 0;
        ChunkLoadClass cload(&file);

        HRawAnimClass anim;
        if (anim.Load_W3D(cload) != HRawAnimClass::OK)
                return 0xDEAD;

        std::uint64_t checksum = (std::uint64_t)anim.Get_Num_Frames();
        checksum += (std::uint64_t)anim.Get_Num_Pivots();
        if (anim.NodeMotion[0].X != nullptr) {
                checksum += (std::uint64_t)(anim.NodeMotion[0].X->LastFrame - anim.NodeMotion[0].X->FirstFrame + 1);
                uint32_t bits;
                memcpy(&bits, &anim.NodeMotion[0].X->Data[HANIM_BENCH_FRAMES - 1], 4);
                checksum += bits;
        }
        if (anim.NodeMotion[HANIM_BENCH_PIVOTS - 1].Q != nullptr) {
                uint32_t bits;
                memcpy(&bits, &anim.NodeMotion[HANIM_BENCH_PIVOTS - 1].Q->Data[4], 4);
                checksum += bits;
        }
        return checksum;
}

// Loads the corpus once (the caller's untimed setup); returns the
// loaded animation handle for the transform bench.
void *bench_hanim_anim_new(const uint8_t *data, size_t len)
{
        SrcFile file;
        file.data = data;
        file.len = len;
        file.pos = 0;
        ChunkLoadClass cload(&file);

        HRawAnimClass *anim = new HRawAnimClass;
        if (anim->Load_W3D(cload) != HRawAnimClass::OK) {
                delete anim;
                return nullptr;
        }
        return anim;
}

void bench_hanim_anim_free(void *handle)
{
        delete (HRawAnimClass *)handle;
}

// The per-frame Anim_Update shape: Get_Transform over every bone at the
// sampled frames of the preloaded animation. Returns a checksum of the
// transform words so the timing loop cannot be elided.
std::uint64_t bench_hanim_xforms(void *handle)
{
        HRawAnimClass *anim = (HRawAnimClass *)handle;
        std::uint64_t checksum = 0;
        float m[16]; // 12 transform words + the scratch a wide channel reaches
        for (int fi = 0; fi < HANIM_BENCH_QUERY_FRAMES; fi++) {
                float frame = (float)(fi * (HANIM_BENCH_FRAMES / HANIM_BENCH_QUERY_FRAMES)) + 0.5f;
                for (int piv = 0; piv < anim->Get_Num_Pivots(); piv++) {
                        anim->Get_Transform(m, piv, frame);
                        for (int k = 0; k < 12; k += 4) {
                                uint32_t bits;
                                memcpy(&bits, &m[k], 4);
                                checksum += bits;
                        }
                }
        }
        return checksum;
}

} // extern "C"
