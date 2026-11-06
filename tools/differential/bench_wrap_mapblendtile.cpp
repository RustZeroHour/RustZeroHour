// SPDX-License-Identifier: GPL-3.0-or-later
//
// C++ baseline timing wrapper around the gen_mapblendtile_golden.cpp
// transcription.
//
// Times the verbatim WorldHeightMap.cpp blend-tile load path — the
// constructor's full-load walk (the HeightMapData reader, the
// ParseBlendTileData reader, the post-walk patch loop) over a
// version-8 256x256 payload (65 536 cells: four 128 KiB index arrays,
// the direct cliff-state bitmap, 8 texture classes, 8 edge classes, 512
// blend records, 64 cliff tables — the modern load, the largest map
// payload) and the version-6 load of the same extent (the cliff-state
// bitmap derived from the heights: the float min/max chains over every
// cell) — on the same formula-generated corpora as the Rust
// `mapblendtile/…` criterion benches.
//
// The corpora are hand-assembled here through the generator's byte
// builders and on the Rust side through the container writer over the
// same formula helpers; the golden corpus set pins the construction
// lockstep, and both sides walk semantically identical bytes — verified
// at authoring time: both corpora the same length (610 240 and 602 048
// bytes), differing only in the table-of-contents entry order (the Rust
// writer's documented reverse-allocation emission; the chunk bytes and
// ids are identical), and both parse checksums equal (the constants the
// Rust bench asserts).
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

#define main gen_mapblendtile_generator_main
#include "gen_mapblendtile_golden.cpp"
#undef main

// The bench corpus extent (a typical skirmish map: 256x256 cells).
#define BENCH_EXTENT 256

// The bench counts (a busy but realistic payload).
#define BENCH_NBMP 512
#define BENCH_NBLEND 513
#define BENCH_NCLIFF 65
#define BENCH_NTEX 8

// The bench walk state: the shared map plus the checksum the Rust bench
// computes over the same fields.
struct BenchBt {
        WorldHeightMapLoad map;
        Bool hmLoaded = false;
        uint64_t checksum = 0;
};

static void add_bt_checksum(BenchBt &st)
{
        st.checksum += (uint64_t)(uint32_t)st.map.m_width;
        st.checksum += (uint64_t)(uint32_t)st.map.m_height;
        st.checksum += (uint64_t)(uint32_t)st.map.m_dataSize;
        st.checksum += (uint64_t)(uint32_t)st.map.m_numBitmapTiles;
        st.checksum += (uint64_t)(uint32_t)st.map.m_numBlendedTiles;
        st.checksum += (uint64_t)(uint32_t)st.map.m_numCliffInfo;
        st.checksum += (uint64_t)(uint32_t)st.map.m_numTextureClasses;
        st.checksum += (uint64_t)(uint32_t)st.map.m_numEdgeTiles;
        st.checksum += (uint64_t)(uint32_t)st.map.m_numEdgeTextureClasses;
        st.checksum += (uint64_t)(uint32_t)st.map.m_flipStateWidth;
        for (int16_t v : st.map.m_tileNdxes)
                st.checksum += (uint64_t)(uint16_t)v;
        for (int16_t v : st.map.m_blendTileNdxes)
                st.checksum += (uint64_t)(uint16_t)v;
        for (int16_t v : st.map.m_extraBlendTileNdxes)
                st.checksum += (uint64_t)(uint16_t)v;
        for (int16_t v : st.map.m_cliffInfoNdxes)
                st.checksum += (uint64_t)(uint16_t)v;
        for (uint8_t c : st.map.m_cellFlipState)
                st.checksum += c;
        for (uint8_t c : st.map.m_cellCliffState)
                st.checksum += c;
        for (const TXTextureClassRec &t : st.map.m_textureClasses) {
                st.checksum += (uint64_t)(uint32_t)t.firstTile;
                st.checksum += (uint64_t)(uint32_t)t.numTiles;
                st.checksum += (uint64_t)(uint32_t)t.width;
                for (unsigned char c : t.name)
                        st.checksum += c;
        }
        for (const TXTextureClassRec &t : st.map.m_edgeTextureClasses) {
                st.checksum += (uint64_t)(uint32_t)t.firstTile;
                st.checksum += (uint64_t)(uint32_t)t.numTiles;
                st.checksum += (uint64_t)(uint32_t)t.width;
                for (unsigned char c : t.name)
                        st.checksum += c;
        }
        for (Int i = 1; i < st.map.m_numBlendedTiles; i++) {
                const TBlendTileInfo &r = st.map.m_blendedTiles[(size_t)i];
                st.checksum += (uint64_t)(uint32_t)r.blendNdx;
                st.checksum += r.horiz;
                st.checksum += r.vert;
                st.checksum += r.rightDiagonal;
                st.checksum += r.leftDiagonal;
                st.checksum += r.inverted;
                st.checksum += r.longDiagonal;
                st.checksum += (uint64_t)(uint32_t)r.customBlendEdgeClass;
        }
        for (Int i = 1; i < st.map.m_numCliffInfo; i++) {
                const TCliffInfo &c = st.map.m_cliffInfo[(size_t)i];
                uint32_t u0bits, v3bits;
                memcpy(&u0bits, &c.u0, 4);
                memcpy(&v3bits, &c.v3, 4);
                st.checksum += (uint64_t)(uint16_t)c.tileIndex;
                st.checksum += u0bits;
                st.checksum += v3bits;
                st.checksum += c.flip ? 1 : 0;
                st.checksum += c.mutant ? 1 : 0;
        }
}

// The registered HeightMapData parser (the full load's reader).
static Bool bench_hm(DataChunkInput &file, DataChunkInfo *info, void *userData)
{
        BenchBt &st = *(BenchBt *)userData;
        st.map.parseHeightMapData(file, info);
        st.hmLoaded = true;
        return true;
}

// The registered BlendTileData parser (3-way blends on, the retail
// default).
static Bool bench_bt(DataChunkInput &file, DataChunkInfo *info, void *userData)
{
        BenchBt &st = *(BenchBt *)userData;
        st.map.parseBlendTileData(file, info, true);
        return true;
}

// The height-map payload (a version-4 256x256 map, border 3, 8
// boundaries — the mapheightmap bench's v4 corpus shape).
static std::vector<uint8_t> bench_height_payload()
{
        std::vector<std::pair<Int, Int>> bnd;
        for (Int i = 0; i < 8; ++i)
                bnd.push_back(std::make_pair(240 - 8 * i, 200 + 4 * i));
        return v4_payload(BENCH_EXTENT, BENCH_EXTENT, 3, bnd,
                          formula_bytes(BENCH_EXTENT * BENCH_EXTENT, 91));
}

// The blend payload shared by both corpora (the version-gated builder
// emits the right shape per version).
static BtParts bench_blend_parts()
{
        BtParts p;
        Int len = BENCH_EXTENT * BENCH_EXTENT;
        p.tile = formula_i16s(len, 33);
        p.blend = formula_i16s(len, 44);
        p.extra = formula_i16s(len, 55);
        p.cliff = formula_i16s(len, 66);
        p.nbmp = BENCH_NBMP;
        p.nblend = BENCH_NBLEND;
        p.ncliff = BENCH_NCLIFF;
        for (Int i = 0; i < BENCH_NTEX; ++i) {
                char name[32];
                snprintf(name, sizeof(name), "BenchTex%d", (int)i);
                p.tex.push_back({i * 4, 4, 2, name});
                p.etex.push_back({BENCH_NTEX * 4 + i * 4, 4, 2, name});
        }
        p.nEdgeTiles = BENCH_NTEX * 4;
        for (Int i = 1; i < BENCH_NBLEND; ++i) {
                p.blends.push_back({i, (Byte)(i & 0xff), (Byte)((i * 3) & 0xff),
                                    (Byte)((i * 5) & 0xff), (Byte)((i * 7) & 0xff),
                                    (Byte)(i & 0x03), (Byte)(i & 1), (i % 7) - 1,
                                    FLAG_VAL});
        }
        for (Int i = 1; i < BENCH_NCLIFF; ++i) {
                Real f = (Real)i * 0.25f;
                p.cliffs.push_back({i * 100, f, f + 0.125f, f + 0.25f, f + 0.375f,
                                    f + 0.5f, f + 0.625f, f + 0.75f, f + 0.875f,
                                    (Byte)(i & 1), (Byte)((i >> 1) & 1)});
        }
        return p;
}

// Builds the version-8 corpus: the cliff-state bitmap read directly
// (fsw 32 x height 256 bytes) — keep in lockstep with
// crates/rzh-formats/benches/rzh_formats.rs.
static std::vector<uint8_t> build_v8_corpus()
{
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}, {"BlendTileData", 2}});
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_4, bench_height_payload());
        BtParts p = bench_blend_parts();
        p.cliffState = formula_bytes(((BENCH_EXTENT + 7) / 8) * BENCH_EXTENT, 77);
        put_chunk(b, 2, K_BLEND_TILE_VERSION_8, blend_payload(8, BENCH_EXTENT * BENCH_EXTENT, p));
        return b;
}

// Builds the version-6 corpus: no cliff state in the file — the bitmap
// is derived from the heights (the float min/max chains over every
// cell).
static std::vector<uint8_t> build_v6_corpus()
{
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}, {"BlendTileData", 2}});
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_4, bench_height_payload());
        BtParts p = bench_blend_parts();
        put_chunk(b, 2, K_BLEND_TILE_VERSION_6, blend_payload(6, BENCH_EXTENT * BENCH_EXTENT, p));
        return b;
}

extern "C" {

// Untimed corpus materialization (the parse benches copy it out of the
// shared scratch).
long bench_mapblendtile_materialize(uint8_t *out, size_t cap)
{
        std::vector<uint8_t> b = build_v8_corpus();
        if (b.size() > cap)
                return -1;
        memcpy(out, b.data(), b.size());
        return (long)b.size();
}

long bench_mapblendtile_materialize_v6(uint8_t *out, size_t cap)
{
        std::vector<uint8_t> b = build_v6_corpus();
        if (b.size() > cap)
                return -1;
        memcpy(out, b.data(), b.size());
        return (long)b.size();
}

// The modern load: the constructor's full walk (both readers, the patch
// loop) over the version-8 corpus.
uint64_t bench_mapblendtile_parse(const uint8_t *data, size_t len)
{
        std::vector<uint8_t> bytes(data, data + len);
        CachedFileInputStream s(bytes);
        DataChunkInput in(&s);
        BenchBt st;
        in.registerParser("HeightMapData", "", bench_hm);
        in.registerParser("BlendTileData", "", bench_bt);
        if (!in.parse(&st) || !st.hmLoaded || !st.map.m_blendLoaded)
                return 0xdeadbeefULL;
        st.map.patchBadMaps();
        add_bt_checksum(st);
        return st.checksum;
}

// The legacy load: the same walk over the version-6 corpus (the
// height-derived cliff bitmap over all 65 536 cells).
uint64_t bench_mapblendtile_parse_v6(const uint8_t *data, size_t len)
{
        std::vector<uint8_t> bytes(data, data + len);
        CachedFileInputStream s(bytes);
        DataChunkInput in(&s);
        BenchBt st;
        in.registerParser("HeightMapData", "", bench_hm);
        in.registerParser("BlendTileData", "", bench_bt);
        if (!in.parse(&st) || !st.hmLoaded || !st.map.m_blendLoaded)
                return 0xdeadbeefULL;
        st.map.patchBadMaps();
        add_bt_checksum(st);
        return st.checksum;
}

} // extern "C"
