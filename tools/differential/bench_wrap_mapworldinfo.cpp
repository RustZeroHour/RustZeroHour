// SPDX-License-Identifier: GPL-3.0-or-later
//
// C++ baseline timing wrapper around the gen_mapworldinfo_golden.cpp
// transcription.
//
// Times the verbatim WorldHeightMap.cpp world-info load path — the
// gen's own parseWorldDictDataChunk walk over 2560 WorldInfo chunks
// (an 8-pair dict each: the weather INT, a map name, two bools, a
// real, a unicode title and two more ints) — on the same
// formula-generated corpus as the Rust `mapworldinfo/…` criterion
// benches.
//
// The corpus is hand-assembled here through the generator's byte
// builders and on the Rust side through the same formula helpers; the
// golden corpus set pins the construction lockstep, and both sides
// walk **identical bytes** — verified at authoring time (same corpus
// size and the same per-chunk walk checksum, which the Rust bench
// asserts).
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

#define main gen_mapworldinfo_generator_main
#include "gen_mapworldinfo_golden.cpp"
#undef main

// The bench corpus repetition (2560 8-pair dicts ≈ 190 KiB, the scale
// of the other map benches).
#define BENCH_CHUNKS 2560

// The bench walk state: the WiState the gen's parser drives, plus the
// accumulated per-chunk checksum the Rust bench computes over the same
// fields.
struct BenchWi {
        WiState st;
        uint64_t checksum = 0;

        // The per-chunk checksum over the world dict as the chunk left
        // it (pair count; per stored pair the name-key ordinal, the
        // type and the value; the weather word and its set bit).
        void addChunk()
        {
                checksum += (uint64_t)(uint32_t)st.m_worldDict.m_dict.getPairCount();
                for (Int p = 0; p < st.m_worldDict.m_dict.getPairCount(); ++p) {
                        const Dict::DictPair &pair = st.m_worldDict.m_dict.m_pairs[p];
                        checksum += (uint64_t)pair.getName();
                        checksum += (uint64_t)(uint32_t)pair.getType();
                        switch (pair.getType()) {
                                case Dict::DICT_BOOL:
                                        checksum += pair.b ? 1 : 0;
                                        break;
                                case Dict::DICT_INT:
                                        checksum += (uint64_t)(uint32_t)pair.i;
                                        break;
                                case Dict::DICT_REAL: {
                                        UnsignedInt bits;
                                        memcpy(&bits, &pair.r, 4);
                                        checksum += bits;
                                        break;
                                }
                                case Dict::DICT_ASCIISTRING:
                                        checksum += (uint64_t)pair.as.getLength();
                                        for (unsigned char c : pair.as.s)
                                                checksum += c;
                                        break;
                                case Dict::DICT_UNICODESTRING:
                                        checksum += (uint64_t)pair.us.getLength();
                                        for (WideChar w : pair.us.u)
                                                checksum += w;
                                        break;
                                default:
                                        break;
                        }
                }
                checksum += st.m_weatherSet ? (uint64_t)(uint32_t)st.m_globalData.m_weather
                                            : 0x5a5a5a5aULL;
        }
};

static Bool bench_wi_parser(DataChunkInput &file, DataChunkInfo *info, void *userData)
{
        BenchWi *b = (BenchWi *)userData;
        Bool ok = parseWorldDictDataChunk(file, info, &b->st);
        if (ok)
                b->addChunk();
        return ok;
}

// Builds the corpus: 2560 WorldInfo chunks with 8-pair formula dicts —
// keep in lockstep with crates/rzh-formats/benches/rzh_formats.rs.
static std::vector<uint8_t> build_corpus()
{
        std::vector<uint8_t> b = toc_bytes({{ "WorldInfo", 1 },
                                            { "weather", 2 },
                                            { "mapName", 3 },
                                            { "flag", 4 },
                                            { "gravity", 5 },
                                            { "title", 6 },
                                            { "sky", 7 },
                                            { "detail", 8 },
                                            { "extra", 9 }});
        for (Int i = 0; i < BENCH_CHUNKS; ++i) {
                char name[16];
                snprintf(name, sizeof name, "Map%04d", (int)i);
                std::vector<std::vector<uint8_t>> pairs;
                pairs.push_back(pair_int(2, i % 3));            // weather
                pairs.push_back(pair_ascii(3, name));           // mapName
                pairs.push_back(pair_bool(4, (i & 1) != 0));    // flag
                pairs.push_back(pair_real(5, (Real)(i % 97) * 0.25f)); // gravity
                pairs.push_back(pair_unicode(6, { 0x0048, (uint16_t)(0x61 + (i % 26)) })); // title
                pairs.push_back(pair_int(7, i % 100));          // sky
                pairs.push_back(pair_bool(8, (i & 2) != 0));    // detail
                pairs.push_back(pair_int(9, i));                // extra
                put_chunk(b, 1, K_WORLDDICT_VERSION_1, worldinfo_payload(pairs));
        }
        return b;
}

extern "C" {

// Untimed corpus materialization.
long bench_mapworldinfo_materialize(uint8_t *out, size_t cap)
{
        std::vector<uint8_t> b = build_corpus();
        if (b.size() > cap)
                return -1;
        memcpy(out, b.data(), b.size());
        return (long)b.size();
}

// The logical load: the gen's own walk over the corpus (the registered
// WorldInfo parser, the dict interning), with the accumulated
// per-chunk checksum.
uint64_t bench_mapworldinfo_parse(const uint8_t *data, size_t len)
{
        std::vector<uint8_t> bytes(data, data + len);
        CachedFileInputStream s(bytes);
        DataChunkInput in(&s);
        BenchWi b;
        b.st.seedWellKnownKeys();
        in.registerParser("WorldInfo", "", bench_wi_parser);
        if (!in.parse(&b))
                return 0xdeadbeefULL;
        return b.checksum;
}

} // extern "C"
