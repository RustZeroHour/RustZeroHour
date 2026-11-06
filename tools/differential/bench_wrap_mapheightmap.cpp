// SPDX-License-Identifier: GPL-3.0-or-later
//
// C++ baseline timing wrapper around the gen_mapheightmap_golden.cpp
// transcription.
//
// Times the verbatim WorldHeightMap.cpp height-map load path — the
// registered-parser walk over a version-4 256x256 map (65 536 samples,
// 8 boundaries: the modern load) and the version-1 logical load of the
// same extent (the in-place downsample plus the extent re-point) — on
// the same formula-generated corpora as the Rust `mapheightmap/…`
// criterion benches.
//
// The corpora are hand-assembled here through the generator's byte
// builders and on the Rust side through the same formula helpers; the
// golden corpus set pins the construction lockstep, and both sides
// walk **identical bytes** — verified at authoring time: both corpora
// byte-identical (65 656 and 65 584 bytes) and both parse checksums
// equal (8 425 299 and 8 430 336).
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

#define main gen_mapheightmap_generator_main
#include "gen_mapheightmap_golden.cpp"
#undef main

// The bench corpus extent (a typical skirmish map: 256x256 cells).
#define BENCH_EXTENT 256

// The bench walk state: the loaded height map plus the checksum the
// Rust bench computes over the same fields.
struct BenchHm {
        WorldHeightMapLoad hm;
        Bool loaded = false;
        uint64_t checksum = 0;
};

static void add_hm_checksum(BenchHm &st)
{
        st.checksum += (uint64_t)(uint32_t)st.hm.m_width;
        st.checksum += (uint64_t)(uint32_t)st.hm.m_height;
        st.checksum += (uint64_t)(uint32_t)st.hm.m_borderSize;
        st.checksum += (uint64_t)(uint32_t)st.hm.m_dataSize;
        for (const ICoord2D &b : st.hm.m_boundaries) {
                st.checksum += (uint64_t)(uint32_t)b.x;
                st.checksum += (uint64_t)(uint32_t)b.y;
        }
        for (uint8_t c : st.hm.m_data)
                st.checksum += c;
}

// The full load's parser (the modern version-4 shape).
static Bool bench_hm_full(DataChunkInput &file, DataChunkInfo *info, void *userData)
{
        BenchHm &st = *(BenchHm *)userData;
        st.hm.parseHeightMapData(file, info);
        st.loaded = true;
        add_hm_checksum(st);
        return true;
}

// The logical load's parser (the version-1 downsample + extent update).
static Bool bench_hm_logical(DataChunkInput &file, DataChunkInfo *info, void *userData)
{
        BenchHm &st = *(BenchHm *)userData;
        st.hm.parseSizeOnly(file, info);
        st.loaded = true;
        add_hm_checksum(st);
        return true;
}

// Builds the version-4 corpus: a 256x256 map, border 3, 8 boundaries,
// 65 536 formula samples — keep in lockstep with
// crates/rzh-formats/benches/rzh_formats.rs.
static std::vector<uint8_t> build_v4_corpus()
{
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}});
        std::vector<std::pair<Int, Int>> bnd;
        for (Int i = 0; i < 8; ++i)
                bnd.push_back(std::make_pair(240 - 8 * i, 200 + 4 * i));
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_4,
                  v4_payload(BENCH_EXTENT, BENCH_EXTENT, 3, bnd,
                             formula_bytes(BENCH_EXTENT * BENCH_EXTENT, 91)));
        return b;
}

// Builds the version-1 corpus: the same extent and samples, the legacy
// no-border payload.
static std::vector<uint8_t> build_v1_corpus()
{
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}});
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_1,
                  v2_payload(BENCH_EXTENT, BENCH_EXTENT,
                             formula_bytes(BENCH_EXTENT * BENCH_EXTENT, 93)));
        return b;
}

extern "C" {

// Untimed corpus materialization (the v4 corpus; the parse benches copy
// it out of the shared scratch).
long bench_mapheightmap_materialize(uint8_t *out, size_t cap)
{
        std::vector<uint8_t> b = build_v4_corpus();
        if (b.size() > cap)
                return -1;
        memcpy(out, b.data(), b.size());
        return (long)b.size();
}

long bench_mapheightmap_materialize_v1(uint8_t *out, size_t cap)
{
        std::vector<uint8_t> b = build_v1_corpus();
        if (b.size() > cap)
                return -1;
        memcpy(out, b.data(), b.size());
        return (long)b.size();
}

// The modern load: the full walk over the version-4 corpus.
uint64_t bench_mapheightmap_parse(const uint8_t *data, size_t len)
{
        std::vector<uint8_t> bytes(data, data + len);
        CachedFileInputStream s(bytes);
        DataChunkInput in(&s);
        BenchHm st;
        in.registerParser("HeightMapData", "", bench_hm_full);
        if (!in.parse(&st) || !st.loaded)
                return 0xdeadbeefULL;
        return st.checksum;
}

// The legacy load: the logical walk over the version-1 corpus (the
// in-place downsample of 65 536 samples and the extent re-point).
uint64_t bench_mapheightmap_parse_v1(const uint8_t *data, size_t len)
{
        std::vector<uint8_t> bytes(data, data + len);
        CachedFileInputStream s(bytes);
        DataChunkInput in(&s);
        BenchHm st;
        in.registerParser("HeightMapData", "", bench_hm_logical);
        if (!in.parse(&st) || !st.loaded)
                return 0xdeadbeefULL;
        return st.checksum;
}

} // extern "C"
