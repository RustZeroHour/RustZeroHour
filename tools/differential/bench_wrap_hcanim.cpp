// SPDX-License-Identifier: GPL-3.0-or-later
//
// C++ baseline timing wrapper around the gen_hcanim_golden.cpp
// transcription.
//
// Times the hcanim.cpp Load_W3D data path and the frame queries the
// tree update drives, for both compression flavors:
//
// - `hcanim/parse`: the whole chunk walk of a TIME-CODED corpus — the
//   44-byte header decode, the tree lookup, and the channel loop (the
//   12-byte struct read, the packet array allocation and copy, and the
//   routing per channel) over a 64-bone, 128-frame corpus with X/Y/Z
//   channels on half the bones and quaternion channels on the rest;
// - `hcanim/parse_ad`: the same walk over the ADAPTIVE-DELTA corpus of
//   the same shape (the 16-byte struct read with its scale word, the
//   base floats and the 9-byte packet blocks, the word-aligned pad
//   included);
// - `hcanim/xforms_ad`: the per-frame Anim_Update shape on the
//   adaptive-delta corpus — Get_Transform over every bone at 16
//   consecutive sampled frames (the decompression cache's slide path,
//   plus the slerp and Build_Matrix3D), on the corpus loaded once
//   (untimed) beforehand. The stateful channel caches make each
//   iteration rewind once before the walk — the same work the Rust
//   bench performs.
//
// The corpora are built here with this file's own builders; the sizes
// and formulas match the Rust `hcanim/*` criterion benches (the size
// asserts document the lockstep).
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

#define main gen_hcanim_generator_main
#include "gen_hcanim_golden.cpp"
#undef main

// The corpus shape (matches the Rust baseline).
#define HCANIM_BENCH_PIVOTS 64
#define HCANIM_BENCH_FRAMES 128
#define HCANIM_BENCH_QUERY_FRAMES 16

// The corpus word formulas — pure functions of (pivot, channel, frame,
// component) so the Rust bench replicates the identical bytes without
// offset bookkeeping. Translation words take any bit pattern (the
// queries only lerp them); quaternion words are built as
// 0x3C000000 | (b << 17) — small positive components that keep every
// key pair's dot product inside the fast-table slerp domain.
static float hcanim_word(uint32_t piv, uint32_t ty, uint32_t f)
{
        uint32_t bits = f * 0x01010101u ^ piv * 0x0010001u ^ ty * 0x10010010u ^ 0x3C000000u;
        float v;
        memcpy(&v, &bits, 4);
        return v;
}

static float hcanim_quat_word(uint32_t piv, uint32_t f, uint32_t k)
{
        uint32_t bits = 0x3C000000u | (((f * 7u + k * 13u + piv * 3u) & 0x3Fu) << 17);
        float v;
        memcpy(&v, &bits, 4);
        return v;
}

// Deterministic adaptive-delta packet bytes for block `block`, vector
// index `vi` (the golden corpus's own formula): a filter index byte
// followed by eight nybble bytes.
static void hcanim_ad_packet(int vi, int block, uint8_t out[9])
{
        out[0] = (uint8_t)((vi * 37 + block * 11) & 0xFF);
        for (int k = 0; k < 8; k++) {
                out[1 + k] = (uint8_t)((vi * 53 + block * 29 + k * 17) & 0xFF);
        }
}

// blocks needed to cover frames 1..num_frames-1
static int hcanim_ad_blocks(uint32_t num_frames)
{
        if (num_frames <= 1)
                return 0;
        return (int)((num_frames - 1 + 15) / 16);
}

extern "C" {

// The generator's main() (renamed away) owns the tree table and the
// WWMath init; the wrapper fills the one tree its corpora resolve
// against and initializes the fast tables on first use.
static void ensure_hcanim_tables()
{
        if (g_trees.empty()) {
                g_trees.push_back({{'b', 'e', 'n', 'c', 'h'}, HCANIM_BENCH_PIVOTS});
        }
        WWMath_Init();
}

// Builds the TIME-CODED corpus: pivots 0..31 carry X/Y/Z channels (128
// time-coded packets of one float each), pivots 32..63 carry a
// quaternion channel (128 packets of four floats). Keep in lockstep
// with crates/rzh-formats/benches/rzh_formats.rs's hcanim_corpus().
static void build_hcanim_corpus(ChunkBuild &b)
{
        w_compressed_anim_header(b, W3D_MAKE_VERSION(4, 0), "Run", "bench", HCANIM_BENCH_FRAMES, 30,
                                 ANIM_FLAVOR_TIMECODED);

        for (int piv = 0; piv < HCANIM_BENCH_PIVOTS; piv++) {
                if (piv < HCANIM_BENCH_PIVOTS / 2) {
                        for (int ty = ANIM_CHANNEL_X; ty <= ANIM_CHANNEL_Z; ty++) {
                                std::vector<uint32_t> words;
                                words.reserve(HCANIM_BENCH_FRAMES * 2);
                                for (int f = 0; f < HCANIM_BENCH_FRAMES; f++) {
                                        words.push_back((uint32_t)f);
                                        words.push_back(f2w(hcanim_word((uint32_t)piv, (uint32_t)ty, (uint32_t)f)));
                                }
                                w_tc_motion_channel(b, HCANIM_BENCH_FRAMES, piv, 1, ty, words);
                        }
                } else {
                        std::vector<uint32_t> words;
                        words.reserve(HCANIM_BENCH_FRAMES * 5);
                        for (int f = 0; f < HCANIM_BENCH_FRAMES; f++) {
                                words.push_back((uint32_t)f);
                                for (int k = 0; k < 4; k++) {
                                        words.push_back(f2w(hcanim_quat_word((uint32_t)piv, (uint32_t)f, (uint32_t)k)));
                                }
                        }
                        w_tc_motion_channel(b, HCANIM_BENCH_FRAMES, piv, 4, ANIM_CHANNEL_Q, words);
                }
        }
}

// Builds the ADAPTIVE-DELTA corpus of the same shape: pivots 0..31
// carry X/Y/Z channels (one base float plus eight 9-byte packet
// blocks), pivots 32..63 carry a quaternion channel (four base floats
// plus eight blocks of four packets). Keep in lockstep with the Rust
// baseline's hcanim_ad_corpus().
static void build_hcanim_ad_corpus(ChunkBuild &b)
{
        w_compressed_anim_header(b, W3D_MAKE_VERSION(4, 0), "Run", "bench", HCANIM_BENCH_FRAMES, 30,
                                 ANIM_FLAVOR_ADAPTIVE_DELTA);

        int blocks = hcanim_ad_blocks(HCANIM_BENCH_FRAMES);
        for (int piv = 0; piv < HCANIM_BENCH_PIVOTS; piv++) {
                if (piv < HCANIM_BENCH_PIVOTS / 2) {
                        for (int ty = ANIM_CHANNEL_X; ty <= ANIM_CHANNEL_Z; ty++) {
                                std::vector<uint8_t> packets;
                                for (int blk = 0; blk < blocks; blk++) {
                                        uint8_t p[9];
                                        hcanim_ad_packet(0, blk, p);
                                        packets.insert(packets.end(), p, p + 9);
                                }
                                w_ad_motion_channel(b, HCANIM_BENCH_FRAMES, piv, 1, ty, 0.01f,
                                                    {hcanim_word((uint32_t)piv, (uint32_t)ty, 0)}, packets);
                        }
                } else {
                        std::vector<uint8_t> packets;
                        for (int blk = 0; blk < blocks; blk++) {
                                for (int vi = 0; vi < 4; vi++) {
                                        uint8_t p[9];
                                        hcanim_ad_packet(vi, blk, p);
                                        packets.insert(packets.end(), p, p + 9);
                                }
                        }
                        w_ad_motion_channel(b, HCANIM_BENCH_FRAMES, piv, 4, ANIM_CHANNEL_Q, 0.001f,
                                            {hcanim_quat_word((uint32_t)piv, 0, 0), hcanim_quat_word((uint32_t)piv, 0, 1),
                                             hcanim_quat_word((uint32_t)piv, 0, 2), hcanim_quat_word((uint32_t)piv, 0, 3)},
                                            packets);
                }
        }
}

// Materializes the TIME-CODED corpus into `out` (capacity `cap`);
// returns the total length, or a negative value when the buffer is too
// small.
long bench_hcanim_materialize(uint8_t *out, size_t cap)
{
        ensure_hcanim_tables();
        ChunkBuild b;
        build_hcanim_corpus(b);
        if (b.bytes.size() > cap)
                return -1;
        memcpy(out, b.bytes.data(), b.bytes.size());
        return (long)b.bytes.size();
}

// Materializes the ADAPTIVE-DELTA corpus.
long bench_hcanim_ad_materialize(uint8_t *out, size_t cap)
{
        ensure_hcanim_tables();
        ChunkBuild b;
        build_hcanim_ad_corpus(b);
        if (b.bytes.size() > cap)
                return -1;
        memcpy(out, b.bytes.data(), b.bytes.size());
        return (long)b.bytes.size();
}

// One full load of the corpus through the transcription; returns a
// small checksum of the loaded state so the timing loop cannot be
// elided.
std::uint64_t bench_hcanim_load(const uint8_t *data, size_t len)
{
        SrcFile file;
        file.data = data;
        file.len = len;
        file.pos = 0;
        ChunkLoadClass cload(&file);

        HCompressedAnimClass anim;
        if (anim.Load_W3D(cload) != HCompressedAnimClass::OK)
                return 0xDEAD;

        std::uint64_t checksum = (std::uint64_t)anim.Get_Num_Frames();
        checksum += (std::uint64_t)anim.Get_Num_Pivots();
        if (anim.NodeMotion[0].tc.X != nullptr) {
                checksum += (std::uint64_t)anim.NodeMotion[0].tc.X->NumTimeCodes;
                checksum += (std::uint64_t)anim.NodeMotion[0].tc.X->Data[HCANIM_BENCH_FRAMES * 2 - 1];
        }
        if (anim.NodeMotion[HCANIM_BENCH_PIVOTS - 1].tc.Q != nullptr) {
                checksum += (std::uint64_t)anim.NodeMotion[HCANIM_BENCH_PIVOTS - 1].tc.Q->Data[5];
        }
        return checksum;
}

// Loads the ADAPTIVE-DELTA corpus once (the caller's untimed setup);
// returns the loaded animation handle for the query bench.
void *bench_hcanim_anim_new(const uint8_t *data, size_t len)
{
        SrcFile file;
        file.data = data;
        file.len = len;
        file.pos = 0;
        ChunkLoadClass cload(&file);

        HCompressedAnimClass *anim = new HCompressedAnimClass;
        if (anim->Load_W3D(cload) != HCompressedAnimClass::OK) {
                delete anim;
                return nullptr;
        }
        return anim;
}

void bench_hcanim_anim_free(void *handle)
{
        delete (HCompressedAnimClass *)handle;
}

// The per-frame Anim_Update shape on the adaptive-delta corpus:
// Get_Transform over every bone at 16 consecutive sampled frames (the
// decompression cache's slide path, the slerp and Build_Matrix3D).
// Returns a checksum of the transform words so the timing loop cannot
// be elided.
std::uint64_t bench_hcanim_xforms(void *handle)
{
        HCompressedAnimClass *anim = (HCompressedAnimClass *)handle;
        std::uint64_t checksum = 0;
        float m[16]; // 12 transform words + the scratch a wide channel reaches
        for (int fi = 0; fi < HCANIM_BENCH_QUERY_FRAMES; fi++) {
                float frame = (float)fi + 0.5f;
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
