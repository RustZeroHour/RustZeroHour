// SPDX-License-Identifier: GPL-3.0-or-later
//
// C++ baseline timing wrapper around the gen_chunk_golden.cpp
// transcription.
//
// Times the verbatim chunkio.cpp classes — the exporter shape (the
// mesh-shaped tree built through ChunkSaveClass: header patching, the
// sub-chunk flag, size accumulation, micro chunks) and the consumer walk
// (the walk-A lens over the materialized bytes) — on the same
// formula-generated corpus as the Rust `chunk/…` criterion benches.
//
// The corpus is built here through the transcribed writer and on the
// Rust side through the port's own writer; the golden writer corpora pin
// the two byte-identical, so both sides walk **identical bytes**.
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

#define main gen_chunk_generator_main
#include "gen_chunk_golden.cpp"
#undef main

// The corpus formula bytes anchored at an absolute offset (the Rust
// bench generates the same absolute-indexed bytes).
static std::vector<uint8_t> formula_at(size_t offset, size_t n)
{
        std::vector<uint8_t> out(n);
        for (size_t i = 0; i < n; ++i)
                out[i] = (uint8_t)(((offset + i) * 7 + 13) & 0xFF);
        return out;
}

// Builds the mesh-shaped corpus tree through the transcribed writer —
// keep in lockstep with crates/rzh-formats/benches/rzh_formats.rs. The
// payload formulas form one continuous byte stream: micros 0..24, then
// the vertex records, then the index array.
static void build_mesh_tree(ChunkSaveClass &csave)
{
        csave.Begin_Chunk(100);
        {
                csave.Begin_Chunk(1);
                csave.Begin_Micro_Chunk(1);
                csave.Write(formula_at(0, 4).data(), 4);
                csave.End_Micro_Chunk();
                csave.Begin_Micro_Chunk(2);
                csave.Write(formula_at(4, 4).data(), 4);
                csave.End_Micro_Chunk();
                csave.Begin_Micro_Chunk(3);
                csave.Write(formula_at(8, 16).data(), 16);
                csave.End_Micro_Chunk();
                csave.End_Chunk();
        }
        {
                csave.Begin_Chunk(2);
                for (size_t i = 0; i < 4096; ++i) {
                        std::vector<uint8_t> v = formula_at(24 + i * 12, 12);
                        csave.Write(v.data(), 12);
                }
                csave.End_Chunk();
        }
        {
                csave.Begin_Chunk(3);
                std::vector<uint8_t> idx = formula_at(24 + 4096 * 12, 4096 * 4 + 4);
                csave.Write(idx.data(), (uint32_t)idx.size());
                csave.End_Chunk();
        }
        csave.End_Chunk();
}

// The recursive walk body (declared before the extern block that uses it).
static uint64_t walk_chunk_bench(ChunkLoadClass &cload, uint64_t &checksum);

extern "C" {

// The exporter shape: a fresh writer + sink per batch, the whole tree
// built, the byte count returned.
long bench_chunk_write()
{
        SinkFile sink;
        ChunkSaveClass csave(&sink);
        build_mesh_tree(csave);
        return (long)sink.bytes.size();
}

// Untimed corpus materialization for the parse bench.
long bench_chunk_materialize(uint8_t *out, size_t cap)
{
        SinkFile sink;
        ChunkSaveClass csave(&sink);
        build_mesh_tree(csave);
        if (sink.bytes.size() > cap)
                return -1;
        memcpy(out, sink.bytes.data(), sink.bytes.size());
        return (long)sink.bytes.size();
}

// The walk-A lens over the corpus: structure + payload reads in
// min(64, remaining)-byte pieces, summing the payload bytes (the same
// checksum the Rust closure computes). Mirrors the golden walker's loop
// shape one-to-one.
uint64_t bench_chunk_walk(const uint8_t *data, size_t len)
{
        SrcFile src{data, len, 0};
        ChunkLoadClass cload(&src);
        uint64_t checksum = 0;
        walk_chunk_bench(cload, checksum);
        return checksum;
}

} // extern "C"

static uint64_t walk_chunk_bench(ChunkLoadClass &cload, uint64_t &checksum)
{
        while (cload.Open_Chunk()) {
                if (cload.Contains_Chunks()) {
                        walk_chunk_bench(cload, checksum);
                } else {
                        uint32_t remaining = cload.Cur_Chunk_Length();
                        uint8_t buf[64];
                        while (remaining > 0) {
                                uint32_t piece = remaining < 64 ? remaining : 64;
                                uint32_t n = cload.Read(buf, piece);
                                if (n == 0)
                                        break;
                                for (uint32_t i = 0; i < n; ++i)
                                        checksum += buf[i];
                                remaining -= n;
                        }
                }
                cload.Close_Chunk();
        }
        return checksum;
}
