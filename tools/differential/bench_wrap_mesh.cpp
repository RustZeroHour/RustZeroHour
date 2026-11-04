// SPDX-License-Identifier: GPL-3.0-or-later
//
// C++ baseline timing wrapper around the gen_mesh_golden.cpp
// transcription.
//
// Times the meshmdlio.cpp Load_W3D data path — the whole chunk walk:
// the header decode, the geometry readers, the prelit selection, the
// material tree (shaders, vertex materials, textures, the material
// passes with DCG/UV arrays) and the AABTree — on the same
// formula-generated corpus as the Rust `mesh/parse_x2048` criterion
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

#define main gen_mesh_generator_main
#include "gen_mesh_golden.cpp"
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

// Builds the mesh-shaped corpus tree — keep in lockstep with
// crates/rzh-formats/benches/rzh_formats.rs's mesh_corpus().
static void build_mesh_corpus(ChunkBuild &b, size_t vertices)
{
        size_t n = vertices;
        size_t absolute = 0;

        // the mesh header (116 bytes)
        b.begin(W3D_CHUNK_MESH_HEADER3);
        {
                std::vector<uint8_t> header;
                auto push_u32 = [&](uint32_t v) {
                        uint8_t raw[4];
                        memcpy(raw, &v, 4);
                        header.insert(header.end(), raw, raw + 4);
                };
                push_u32(W3D_MAKE_VERSION(4, 2));
                push_u32(W3D_MESH_FLAG_PRELIT_VERTEX);
                const char mesh_name[16] = {'b', 'e', 'n', 'c', 'h'};
                const char container_name[16] = {'c', 'o', 'r', 'p', 'u', 's'};
                header.insert(header.end(), mesh_name, mesh_name + 16);
                header.insert(header.end(), container_name, container_name + 16);
                push_u32((uint32_t)n); // tris
                push_u32((uint32_t)n); // verts
                push_u32(1);            // materials
                push_u32(0);            // damage stages
                push_u32(0);            // sort level
                push_u32(0);            // prelit version
                push_u32(0);            // future counts
                push_u32(0x1F);         // vertex channels
                push_u32(0x01);         // face channels
                const float bounds[10] = {-1.0f, -1.0f, -1.0f, 1.0f, 1.0f, 1.0f, 0.0f, 0.0f, 0.0f, 2.0f};
                for (float word : bounds) {
                        uint32_t bits;
                        memcpy(&bits, &word, 4);
                        push_u32(bits);
                }
                b.w_bytes(header.data(), (uint32_t)header.size());
        }
        b.end();
        absolute += 116;

        // vertices + normals
        {
                std::vector<uint8_t> data = formula_at(absolute, n * 12);
                b.begin(W3D_CHUNK_VERTICES);
                b.w_bytes(data.data(), (uint32_t)data.size());
                b.end();
                absolute += n * 12;
        }
        {
                std::vector<uint8_t> data = formula_at(absolute, n * 12);
                b.begin(W3D_CHUNK_VERTEX_NORMALS);
                b.w_bytes(data.data(), (uint32_t)data.size());
                b.end();
                absolute += n * 12;
        }

        // triangles
        {
                std::vector<uint8_t> data = formula_at(absolute, n * 32);
                b.begin(W3D_CHUNK_TRIANGLES);
                b.w_bytes(data.data(), (uint32_t)data.size());
                b.end();
                absolute += n * 32;
        }

        // the prelit wrapper with the material tree
        b.begin(W3D_CHUNK_PRELIT_VERTEX);
        {
                b.begin(W3D_CHUNK_MATERIAL_INFO);
                b.w_u32(1);
                b.w_u32(1);
                b.w_u32(1);
                b.w_u32(1);
                b.end();
                b.begin(W3D_CHUNK_SHADERS);
                {
                        std::vector<uint8_t> data = formula_at(absolute, 16);
                        b.w_bytes(data.data(), (uint32_t)data.size());
                        absolute += 16;
                }
                b.end();
                b.begin(W3D_CHUNK_VERTEX_MATERIALS);
                {
                        b.begin(W3D_CHUNK_VERTEX_MATERIAL);
                        b.begin(W3D_CHUNK_VERTEX_MATERIAL_NAME);
                        b.w_cstr("bench");
                        b.end();
                        b.begin(W3D_CHUNK_VERTEX_MATERIAL_INFO);
                        {
                                std::vector<uint8_t> data = formula_at(absolute, 32);
                                b.w_bytes(data.data(), (uint32_t)data.size());
                                absolute += 32;
                        }
                        b.end();
                        b.end();
                }
                b.end();
                b.begin(W3D_CHUNK_TEXTURES);
                {
                        b.begin(W3D_CHUNK_TEXTURE);
                        b.begin(W3D_CHUNK_TEXTURE_NAME);
                        b.w_cstr("bench.dds");
                        b.end();
                        b.begin(W3D_CHUNK_TEXTURE_INFO);
                        b.w_u16(0x000C);
                        b.w_u16(0);
                        b.w_u32(1);
                        b.w_f32(0.0f);
                        b.end();
                        b.end();
                }
                b.end();
                b.begin(W3D_CHUNK_MATERIAL_PASS);
                {
                        b.begin(W3D_CHUNK_VERTEX_MATERIAL_IDS);
                        b.w_u32(0);
                        b.end();
                        b.begin(W3D_CHUNK_SHADER_IDS);
                        b.w_u32(0);
                        b.end();
                        b.begin(W3D_CHUNK_DCG);
                        {
                                std::vector<uint8_t> data = formula_at(absolute, n * 4);
                                b.w_bytes(data.data(), (uint32_t)data.size());
                                absolute += n * 4;
                        }
                        b.end();
                        b.begin(W3D_CHUNK_TEXTURE_STAGE);
                        {
                                b.begin(W3D_CHUNK_TEXTURE_IDS);
                                b.w_u32(0);
                                b.end();
                                b.begin(W3D_CHUNK_STAGE_TEXCOORDS);
                                {
                                        std::vector<uint8_t> data = formula_at(absolute, n * 8);
                                        b.w_bytes(data.data(), (uint32_t)data.size());
                                        absolute += n * 8;
                                }
                                b.end();
                        }
                        b.end();
                }
                b.end();
        }
        b.end();

        // the aabtree: half as many nodes as polygons, all leaves
        b.begin(W3D_CHUNK_AABTREE);
        {
                b.begin(W3D_CHUNK_AABTREE_HEADER);
                b.w_u32((uint32_t)(n / 2));
                b.w_u32((uint32_t)n);
                for (int i = 0; i < 6; i++)
                        b.w_u32(0);
                b.end();
                b.begin(W3D_CHUNK_AABTREE_POLYINDICES);
                {
                        std::vector<uint8_t> data = formula_at(absolute, n * 4);
                        b.w_bytes(data.data(), (uint32_t)data.size());
                        absolute += n * 4;
                }
                b.end();
                b.begin(W3D_CHUNK_AABTREE_NODES);
                {
                        std::vector<uint8_t> data = formula_at(absolute, (n / 2) * 32);
                        b.w_bytes(data.data(), (uint32_t)data.size());
                }
                b.end();
        }
        b.end();
}

extern "C" {

// Materializes the corpus into `out` (capacity `cap`); returns the total
// length, or a negative value when the buffer is too small.
long bench_mesh_materialize(uint8_t *out, size_t cap)
{
        ChunkBuild b;
        build_mesh_corpus(b, 2048);
        if (b.bytes.size() > cap)
                return -1;
        memcpy(out, b.bytes.data(), b.bytes.size());
        return (long)b.bytes.size();
}

// One full load of the corpus through the transcription; returns a small
// checksum of the loaded state so the timing loop cannot be elided.
uint64_t bench_mesh_load(const uint8_t *data, size_t len)
{
        SrcFile file;
        file.data = data;
        file.len = len;
        file.pos = 0;
        ChunkLoadClass cload(&file);

        MeshModelClass model;
        if (model.Load_W3D(cload) != WW3D_ERROR_OK)
                return 0xDEAD;

        uint64_t checksum = model.Flags;
        if (model.Vertex && !model.Vertex->empty())
                checksum += (uint64_t)(model.Vertex->at(0) * 1000.0);
        if (model.CullTree)
                checksum += model.CullTree->NodeCount;
        if (model.DefMatDesc->ColorArray[0])
                checksum += model.DefMatDesc->ColorArray[0]->at(0);
        return checksum;
}

} // extern "C"
