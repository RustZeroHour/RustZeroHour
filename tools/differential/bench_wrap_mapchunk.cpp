// SPDX-License-Identifier: GPL-3.0-or-later
//
// C++ baseline timing wrapper around the gen_mapchunk_golden.cpp
// transcription.
//
// Times the verbatim DataChunk classes — the WorldBuilder shape (the
// map-shaped file built through DataChunkOutput: TOC interning, the
// placeholder-patched headers, dicts and name keys) and the game-load
// shape (DataChunkInput's registered-parser walk reading every value out
// of the same bytes) — on the same formula-generated corpus as the Rust
// `mapchunk/…` criterion benches.
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

#define main gen_mapchunk_generator_main
#include "gen_mapchunk_golden.cpp"
#undef main

// The corpus formula bytes (the Rust bench rebuilds the same stream).
static std::string heights(size_t n)
{
        std::string s;
        s.resize(n);
        for (size_t i = 0; i < n; ++i)
                s[i] = (char)((i * 5 + 11) & 0xFF);
        return s;
}

// The bench parser state (the userData the original hands every parser).
struct BenchState {
        NameKeyGenerator keys;
        uint64_t checksum = 0;
};

static void add_dict_checksum(BenchState &st, const Dict &d)
{
        st.checksum += (uint64_t)d.getPairCount();
        for (Int i = 0; i < d.getPairCount(); ++i) {
                st.checksum += d.getNthKey(i);
                st.checksum += (uint64_t)d.getNthType(i);
                switch (d.getNthType(i)) {
                        case Dict::DICT_BOOL:
                                st.checksum += d.getNthBool(i) ? 1 : 0;
                                break;
                        case Dict::DICT_INT:
                                st.checksum += (uint64_t)d.getNthInt(i);
                                break;
                        case Dict::DICT_REAL: {
                                uint32_t bits;
                                float r = d.getNthReal(i);
                                memcpy(&bits, &r, 4);
                                st.checksum += bits;
                                break;
                        }
                        case Dict::DICT_ASCIISTRING:
                                st.checksum += (uint64_t)d.getNthAsciiString(i).getLength();
                                break;
                        case Dict::DICT_UNICODESTRING:
                                st.checksum += (uint64_t)d.getNthUnicodeString(i).getLength();
                                break;
                        default:
                                break;
                }
        }
}

static Bool b_height(DataChunkInput &file, DataChunkInfo *info, void *userData)
{
        BenchState &st = *(BenchState *)userData;
        (void)info;
        st.checksum += (uint64_t)file.readInt();
        st.checksum += (uint64_t)file.readInt();
        char buf[2048];
        file.readArrayOfBytes(buf, 2048);
        for (int i = 0; i < 2048; ++i)
                st.checksum += (uint8_t)buf[i];
        st.checksum += (uint64_t)file.readInt();
        return true;
}

static Bool b_blend(DataChunkInput &file, DataChunkInfo *info, void *userData)
{
        BenchState &st = *(BenchState *)userData;
        (void)info;
        st.checksum += (uint64_t)file.readInt();
        char buf[512];
        file.readArrayOfBytes(buf, 512);
        for (int i = 0; i < 512; ++i)
                st.checksum += (uint8_t)buf[i];
        return true;
}

static Bool b_dict(DataChunkInput &file, DataChunkInfo *info, void *userData)
{
        BenchState &st = *(BenchState *)userData;
        (void)info;
        add_dict_checksum(st, file.readDict(st.keys));
        return true;
}

static Bool b_objects(DataChunkInput &file, DataChunkInfo *info, void *userData)
{
        (void)info;
        return file.parse(userData);
}

static Bool b_object(DataChunkInput &file, DataChunkInfo *info, void *userData)
{
        BenchState &st = *(BenchState *)userData;
        (void)info;
        st.checksum += (uint64_t)file.readNameKey(st.keys);
        add_dict_checksum(st, file.readDict(st.keys));
        st.checksum += (uint64_t)file.readAsciiString().getLength();
        return true;
}

// Builds the map-shaped corpus through the transcribed writer — keep in
// lockstep with crates/rzh-formats/benches/rzh_formats.rs.
static void build_map(DataChunkOutput &wr, NameKeyGenerator &keys)
{
        wr.openDataChunk("HeightMapData", 3);
        wr.writeInt(64);
        wr.writeInt(64);
        {
                std::string h = heights(2048);
                wr.writeArrayOfBytes(h.data(), (Int)h.size());
        }
        wr.writeInt(5);
        wr.closeDataChunk();

        wr.openDataChunk("BlendTileData", 7);
        wr.writeInt(512);
        {
                std::string t = heights(512);
                wr.writeArrayOfBytes(t.data(), (Int)t.size());
        }
        wr.closeDataChunk();

        wr.openDataChunk("GlobalLighting", 2);
        {
                Dict d;
                d.setReal(keys.nameToKey("Ambient"), 0.25f);
                d.setAsciiString(keys.nameToKey("Sun"), "Warm");
                d.setReal(keys.nameToKey("Moon"), 0.125f);
                d.setBool(keys.nameToKey("Night"), true);
                wr.writeDict(keys, d);
        }
        wr.closeDataChunk();

        wr.openDataChunk("WorldInfo", 1);
        {
                Dict d;
                d.setAsciiString(keys.nameToKey("Name"), "Benchmark Map");
                d.setInt(keys.nameToKey("WidthTiles"), 64);
                d.setInt(keys.nameToKey("HeightTiles"), 64);
                d.setBool(keys.nameToKey("IsMultiplayer"), true);
                d.setAsciiString(keys.nameToKey("Author"), "rzh");
                wr.writeDict(keys, d);
        }
        wr.closeDataChunk();

        wr.openDataChunk("ObjectsList", 3);
        for (int i = 0; i < 256; ++i) {
                wr.openDataChunk("MapObject", 1);
                wr.writeNameKey(keys, keys.nameToKey("ObjectName"));
                {
                        Dict d;
                        d.setInt(keys.nameToKey("ObjectFlags"), i);
                        d.setAsciiString(keys.nameToKey("Owner"), "Player1");
                        wr.writeDict(keys, d);
                }
                char name[16];
                snprintf(name, sizeof(name), "Obj%05d", i);
                wr.writeAsciiString(name);
                wr.closeDataChunk();
        }
        wr.closeDataChunk();
}

extern "C" {

// The WorldBuilder shape: a fresh writer per batch, the whole map built,
// the byte count returned.
long bench_mapchunk_write()
{
        NameKeyGenerator keys;
        ByteSink sink;
        DataChunkOutput wr(&sink);
        build_map(wr, keys);
        wr.finish();
        return (long)sink.bytes.size();
}

// Untimed corpus materialization for the parse bench.
long bench_mapchunk_materialize(uint8_t *out, size_t cap)
{
        NameKeyGenerator keys;
        ByteSink sink;
        DataChunkOutput wr(&sink);
        build_map(wr, keys);
        wr.finish();
        if (sink.bytes.size() > cap)
                return -1;
        memcpy(out, sink.bytes.data(), sink.bytes.size());
        return (long)sink.bytes.size();
}

// The game-load shape: the registered-parser walk reading every value,
// returning the checksum (the same value the Rust closure computes).
uint64_t bench_mapchunk_parse(const uint8_t *data, size_t len)
{
        std::vector<uint8_t> bytes(data, data + len);
        CachedFileInputStream s(bytes);
        DataChunkInput in(&s);
        BenchState st;
        in.registerParser("HeightMapData", "", b_height);
        in.registerParser("BlendTileData", "", b_blend);
        in.registerParser("GlobalLighting", "", b_dict);
        in.registerParser("WorldInfo", "", b_dict);
        in.registerParser("ObjectsList", "", b_objects);
        in.registerParser("MapObject", "ObjectsList", b_object);
        if (!in.parse(&st))
                return 0xdeadbeefULL;
        return st.checksum;
}

} // extern "C"
