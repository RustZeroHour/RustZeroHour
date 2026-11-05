// SPDX-License-Identifier: GPL-3.0-or-later
//
// C++ baseline timing wrapper around the gen_hlod_golden.cpp
// transcription.
//
// Times the hlod.cpp Load_W3D data path — the whole chunk walk: the
// 40-byte header decode, the per-LOD sub-object arrays (the 8-byte
// array headers and the 36-byte struct reads with their strdup'd
// names), and the aggregate and proxy arrays — on the same
// formula-generated corpus as the Rust `hlod/parse_x1072` criterion
// bench (64 LODs x 16 models + 16 aggregates + 32 proxies = 1072
// sub-objects). The corpus is built here with the generator's own
// builder; the corpus sizes and formula match the Rust side (the
// golden corpora pin the parsing behaviour; the builders here and in
// the bench construct the identical bytes — the size assert documents
// the lockstep).
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

#define main gen_hlod_generator_main
#include "gen_hlod_golden.cpp"
#undef main

// The corpus shape (matches the Rust baseline).
#define HLOD_BENCH_LODS 64
#define HLOD_BENCH_MODELS 16
#define HLOD_BENCH_AGGREGATES 16
#define HLOD_BENCH_PROXIES 32

// The corpus formula bytes anchored at an absolute offset (the Rust
// bench generates the same absolute-indexed bytes).
static std::vector<uint8_t> hlod_formula_at(size_t offset, size_t n)
{
        std::vector<uint8_t> out(n);
        for (size_t i = 0; i < n; i++)
                out[i] = (uint8_t)(((offset + i) * 11 + 7) & 0xFF);
        return out;
}

// Builds the assembly-shaped corpus — keep in lockstep with
// crates/rzh-formats/benches/rzh_formats.rs's hlod_corpus().
static void build_hlod_corpus(ChunkBuild &b)
{
        // the header chunk (40 bytes): version 1.0, 64 LODs
        w_hlod_header(b, W3D_MAKE_VERSION(1, 0), HLOD_BENCH_LODS, "bench", "bench");

        // the LOD arrays: 16 models each, the screen size varying per LOD
        for (uint32_t l = 0; l < HLOD_BENCH_LODS; l++) {
                b.begin(W3D_CHUNK_HLOD_LOD_ARRAY);
                w_array_header(b, HLOD_BENCH_MODELS, (float)l);
                for (uint32_t m = 0; m < HLOD_BENCH_MODELS; m++) {
                        // the name field's absolute position: the bytes so
                        // far, the chunk header (8) and the bone word (4)
                        size_t at = b.bytes.size() + 8 + 4;
                        std::vector<uint8_t> name = hlod_formula_at(at, W3D_NAME_LEN * 2);
                        w_sub_object_raw(b, l * HLOD_BENCH_MODELS + m, name.data());
                }
                b.end();
        }

        // the aggregates
        b.begin(W3D_CHUNK_HLOD_AGGREGATE_ARRAY);
        w_array_header(b, HLOD_BENCH_AGGREGATES, 0.0f);
        for (uint32_t i = 0; i < HLOD_BENCH_AGGREGATES; i++) {
                size_t at = b.bytes.size() + 8 + 4;
                std::vector<uint8_t> name = hlod_formula_at(at, W3D_NAME_LEN * 2);
                w_sub_object_raw(b, 0x1000u + i, name.data());
        }
        b.end();

        // the proxies
        b.begin(W3D_CHUNK_HLOD_PROXY_ARRAY);
        w_array_header(b, HLOD_BENCH_PROXIES, 0.0f);
        for (uint32_t i = 0; i < HLOD_BENCH_PROXIES; i++) {
                size_t at = b.bytes.size() + 8 + 4;
                std::vector<uint8_t> name = hlod_formula_at(at, W3D_NAME_LEN * 2);
                w_sub_object_raw(b, 0x2000u + i, name.data());
        }
        b.end();
}

// The length of a bounded C-string field (up to the first NUL or the
// field's end) — the dump's q rule.
static size_t hlod_cstr_len(const char *field, size_t cap)
{
        size_t len = 0;
        while (len < cap && field[len] != 0)
                len++;
        return len;
}

extern "C" {

// Materializes the corpus into `out` (capacity `cap`); returns the total
// length, or a negative value when the buffer is too small.
long bench_hlod_materialize(uint8_t *out, size_t cap)
{
        ChunkBuild b;
        build_hlod_corpus(b);
        if (b.bytes.size() > cap)
                return -1;
        memcpy(out, b.bytes.data(), b.bytes.size());
        return (long)b.bytes.size();
}

// One full load of the corpus through the transcription; returns a small
// checksum of the loaded state so the timing loop cannot be elided.
uint64_t bench_hlod_load(const uint8_t *data, size_t len)
{
        SrcFile file;
        file.data = data;
        file.len = len;
        file.pos = 0;
        ChunkLoadClass cload(&file);

        HLodDefClass def;
        if (def.Load_W3D(cload) != HLodDefClass::WW3D_ERROR_OK)
                return 0xDEAD;

        uint64_t checksum = (uint64_t)def.LodCount;
        if (def.LodCount == HLOD_BENCH_LODS) {
                const HLodDefClass::SubObjectArrayClass &last = def.Lod[HLOD_BENCH_LODS - 1];
                checksum += (uint64_t)last.ModelCount;
                if (last.ModelCount == HLOD_BENCH_MODELS) {
                        checksum += (uint64_t)last.BoneIndex[HLOD_BENCH_MODELS - 1];
                        checksum += (uint64_t)last.ModelName[HLOD_BENCH_MODELS - 1].size();
                }
        }
        checksum += (uint64_t)def.Aggregates.ModelCount;
        if (def.ProxyArray != nullptr) {
                checksum += (uint64_t)def.ProxyArray->Length();
                const ProxyRecordClass &rec = (*def.ProxyArray)[def.ProxyArray->Length() - 1];
                checksum += (uint64_t)hlod_cstr_len(rec.Get_Name(), 2 * W3D_NAME_LEN);
                checksum += (uint64_t)rec.Get_Bone_Index();
        }
        return checksum;
}

} // extern "C"
