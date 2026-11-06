// SPDX-License-Identifier: GPL-3.0-or-later
//
// C++ baseline timing wrapper around the gen_maplighting_golden.cpp
// transcription.
//
// Times the verbatim WorldHeightMap.cpp global-lighting load path —
// ParseLightingDataChunk over a version-3 payload (every light read,
// the shadow word included) — on the same formula-generated corpus as
// the Rust `maplightmap/…` criterion benches.
//
// The lighting chunk is fixed-size (one time-of-day word, four blocks of
// six nine-word lights, the shadow word — 872 bytes), so the corpus
// scales the workload by repetition: 256 GlobalLighting chunks, every
// one of which the walk parses (the last one wins — the repeated-chunk
// shape the golden corpus set pins). The corpus is hand-assembled here
// through the generator's byte builders and on the Rust side through
// the same formula helpers; both sides walk **identical bytes** and
// accumulate the same per-chunk checksum — verified at authoring time.
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

#define main gen_maplighting_generator_main
#include "gen_maplighting_golden.cpp"
#undef main

// The bench corpus repetition (256 fixed-size chunks ≈ 220 KiB, the
// scale of the other map benches).
#define BENCH_CHUNKS 256

// The bench walk state: the accumulated per-chunk checksum the Rust
// bench computes over the same fields.
struct BenchGl {
        uint64_t checksum = 0;
};

// The full load's parser: the reader plus the checksum over every word
// it wrote.
static void add_gl_checksum(BenchGl &st, const GlobalLightingLoad &gl)
{
        st.checksum += (uint64_t)(uint32_t)gl.m_timeOfDay;
        st.checksum += gl.m_shadowColorSet ? (uint64_t)gl.m_shadowColor : 0;
        for (Int i = 0; i < 4; ++i) {
                for (Int j = 0; j < MAX_GLOBAL_LIGHTS; ++j) {
                        const TerrainLighting *lights[2] = {
                                &gl.m_terrainLighting[i + TIME_OF_DAY_FIRST][j],
                                &gl.m_terrainObjectsLighting[i + TIME_OF_DAY_FIRST][j],
                        };
                        for (const TerrainLighting *l : lights) {
                                const Real words[9] = {
                                        l->ambient.red, l->ambient.green, l->ambient.blue,
                                        l->diffuse.red, l->diffuse.green, l->diffuse.blue,
                                        l->lightPos.x, l->lightPos.y, l->lightPos.z,
                                };
                                for (Real w : words) {
                                        UnsignedInt bits;
                                        memcpy(&bits, &w, 4);
                                        st.checksum += bits;
                                }
                        }
                }
        }
}

static Bool bench_gl_parser(DataChunkInput &file, DataChunkInfo *info, void *userData)
{
        BenchGl &st = *(BenchGl *)userData;
        GlobalLightingLoad gl;
        gl.parseLightingDataChunk(file, info);
        add_gl_checksum(st, gl);
        return true;
}

// Builds the corpus: 256 version-3 GlobalLighting chunks (the last one
// wins), formula values — keep in lockstep with
// crates/rzh-formats/benches/rzh_formats.rs.
static std::vector<uint8_t> build_corpus()
{
        std::vector<uint8_t> b = toc_bytes({{ "GlobalLighting", 1 }});
        for (Int c = 0; c < BENCH_CHUNKS; ++c) {
                std::vector<uint8_t> p = lighting_payload(
                        K_LIGHTING_VERSION_3, 2, (Real)(c % 64) * 0.25f, true,
                        0x01000000u | (UnsignedInt)c);
                put_chunk(b, 1, K_LIGHTING_VERSION_3, p);
        }
        return b;
}

extern "C" {

// Untimed corpus materialization.
long bench_maplighting_materialize(uint8_t *out, size_t cap)
{
        std::vector<uint8_t> b = build_corpus();
        if (b.size() > cap)
                return -1;
        memcpy(out, b.data(), b.size());
        return (long)b.size();
}

// The full walk over the corpus: every chunk parses, the checksum
// accumulates per chunk.
uint64_t bench_maplighting_parse(const uint8_t *data, size_t len)
{
        std::vector<uint8_t> bytes(data, data + len);
        CachedFileInputStream s(bytes);
        DataChunkInput in(&s);
        BenchGl st;
        in.registerParser("GlobalLighting", "", bench_gl_parser);
        if (!in.parse(&st))
                return 0xdeadbeefULL;
        return st.checksum;
}

} // extern "C"
