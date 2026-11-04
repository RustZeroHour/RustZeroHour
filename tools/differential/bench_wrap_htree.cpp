// SPDX-License-Identifier: GPL-3.0-or-later
//
// C++ baseline timing wrapper around the gen_htree_golden.cpp
// transcription.
//
// Times the htree.cpp Load_W3D data path — the whole chunk walk: the
// 36-byte header decode and the pivot loop (a 60-byte struct read, the
// parent-pointer formation and the mixed-precision base-transform build
// — Translate plus Build_Matrix3D plus postMul — per pivot) on the same
// formula-generated corpus as the Rust `htree/parse_x2048` criterion
// bench. The corpus is built here with this file's own builder; the
// corpus sizes and formula match the Rust side (the golden corpora pin
// the parsing behaviour; the builders here and in the bench construct
// the identical bytes — the size assert documents the lockstep).
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

#define main gen_htree_generator_main
#include "gen_htree_golden.cpp"
#undef main

// The pivot count of the corpus (matches the Rust baseline).
#define HTREE_BENCH_PIVOTS 2048

// The corpus formula bytes anchored at an absolute offset (the Rust
// bench generates the same absolute-indexed bytes).
static std::vector<uint8_t> htree_formula_at(size_t offset, size_t n)
{
        std::vector<uint8_t> out(n);
        for (size_t i = 0; i < n; ++i)
                out[i] = (uint8_t)(((offset + i) * 7 + 13) & 0xFF);
        return out;
}

// Builds the hierarchy-shaped corpus — keep in lockstep with
// crates/rzh-formats/benches/rzh_formats.rs's htree_corpus().
static void build_htree_corpus(ChunkBuild &b, size_t pivots)
{
        // the header chunk (36 bytes)
        {
                b.begin(W3D_CHUNK_HIERARCHY_HEADER);
                b.w_u32(W3D_MAKE_VERSION(4, 1));
                const char name[16] = {'b', 'e', 'n', 'c', 'h'};
                b.w_bytes(name, 16);
                b.w_u32((uint32_t)pivots);
                b.w_f32(1.5f);
                b.w_f32(-2.25f);
                b.w_f32(0.5f);
                b.end();
        }

        // the pivots chunk: every pivot's 60 bytes from the formula,
        // with the parent index structured (a chain: i -> i-1)
        size_t base = 36 + 8; // past the header chunk
        b.begin(W3D_CHUNK_PIVOTS);
        for (size_t i = 0; i < pivots; i++) {
                std::vector<uint8_t> raw = htree_formula_at(base + 8 + i * 60, 60);
                uint32_t parent = (i == 0) ? 0xFFFFFFFFu : (uint32_t)(i - 1);
                memcpy(raw.data() + 16, &parent, 4); // the ParentIdx field
                b.w_bytes(raw.data(), raw.size());
        }
        b.end();
}

extern "C" {

// Materializes the corpus into `out` (capacity `cap`); returns the total
// length, or a negative value when the buffer is too small.
long bench_htree_materialize(uint8_t *out, size_t cap)
{
        ChunkBuild b;
        build_htree_corpus(b, HTREE_BENCH_PIVOTS);
        if (b.bytes.size() > cap)
                return -1;
        memcpy(out, b.bytes.data(), b.bytes.size());
        return (long)b.bytes.size();
}

// One full load of the corpus through the transcription; returns a small
// checksum of the loaded state so the timing loop cannot be elided.
uint64_t bench_htree_load(const uint8_t *data, size_t len)
{
        SrcFile file;
        file.data = data;
        file.len = len;
        file.pos = 0;
        ChunkLoadClass cload(&file);

        HTreeClass tree;
        if (tree.Load_W3D(cload) != HTreeClass::OK)
                return 0xDEAD;

        uint64_t checksum = (uint64_t)tree.Num_Pivots();
        if (tree.Num_Pivots() == HTREE_BENCH_PIVOTS) {
                checksum += (uint64_t)tree.Get_Parent_Index(HTREE_BENCH_PIVOTS - 1);
                uint32_t bits;
                memcpy(&bits, &tree.Pivot[HTREE_BENCH_PIVOTS - 1].BaseTransform.Row[0].X, 4);
                checksum += bits;
                memcpy(&bits, &tree.Pivot[HTREE_BENCH_PIVOTS - 1].BaseTransform.Row[2].W, 4);
                checksum += bits;
                checksum += (uint64_t)cstr_len(tree.Pivot[HTREE_BENCH_PIVOTS - 1].Name, W3D_NAME_LEN);
        }
        return checksum;
}

} // extern "C"
