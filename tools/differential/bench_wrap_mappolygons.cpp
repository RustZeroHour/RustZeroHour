// SPDX-License-Identifier: GPL-3.0-or-later
//
// C++ baseline timing wrapper around the gen_mappolygons_golden.cpp
// transcription.
//
// Times the verbatim PolygonTrigger.cpp polygon-triggers load path —
// the gen's own parsePolygonTriggersDataChunk walk over one
// version-4 PolygonTriggers chunk with 1536 triggers (a name, a layer,
// an id, the water/river bytes, a riverStart word and eight int-triple
// points each) — on the same formula-generated corpus as the Rust
// `mappolygons/…` criterion benches.
//
// The corpus is hand-assembled here through the generator's byte
// builders and on the Rust side through the same formula helpers; the
// golden corpus set pins the construction lockstep, and both sides
// walk **identical bytes** — verified at authoring time (same corpus
// size and the same final-list walk checksum, which the Rust bench
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

#define main gen_mappolygons_generator_main
#include "gen_mappolygons_golden.cpp"
#undef main

// The bench corpus size (1536 8-point triggers ≈ 190 KiB, the scale of
// the other map benches).
#define BENCH_TRIGGERS 1536

// The final-list checksum over the kept triggers' observable fields
// plus the id counter (the same walk the Rust bench drives).
static uint64_t pt_list_checksum(const PtState &st)
{
        uint64_t checksum = 0;
        for (const PolygonTriggerLoad &t : st.m_list) {
                checksum += (uint64_t)(uint32_t)t.m_triggerID;
                checksum += t.m_isWaterArea ? 1 : 0;
                checksum += t.m_isRiver ? 2 : 0;
                checksum += (uint64_t)(uint32_t)t.m_riverStart;
                for (unsigned char c : t.m_triggerName.s)
                        checksum += c;
                for (unsigned char c : t.m_layerName.s)
                        checksum += c;
                for (const ICoord3D &pt : t.m_points) {
                        checksum += (uint64_t)(uint32_t)pt.x;
                        checksum += (uint64_t)(uint32_t)pt.y;
                        checksum += (uint64_t)(uint32_t)pt.z;
                }
        }
        checksum += (uint64_t)(uint32_t)st.s_currentID;
        return checksum;
}

// Builds the corpus: one version-4 PolygonTriggers chunk with 1536
// 8-point formula triggers — keep in lockstep with
// crates/rzh-formats/benches/rzh_formats.rs.
static std::vector<uint8_t> build_corpus()
{
        std::vector<uint8_t> b = toc_bytes({{ "PolygonTriggers", 1 }});
        std::vector<std::vector<uint8_t>> triggers;
        for (Int i = 0; i < BENCH_TRIGGERS; ++i) {
                char name[16], layer[8];
                snprintf(name, sizeof name, "Trig%04d", (int)i);
                snprintf(layer, sizeof layer, "L%02d", (int)(i % 50));
                std::vector<std::vector<Int>> points;
                for (Int j = 0; j < 8; ++j)
                        points.push_back({ (i % 512) + j, (i % 256) - j, (j * 7) % 100 });
                triggers.push_back(trigger_payload(K_TRIGGERS_VERSION_4, name, layer,
                                                   i + 1, (i & 1) != 0, (i & 2) != 0,
                                                   i % 32, points));
        }
        put_chunk(b, 1, K_TRIGGERS_VERSION_4, triggers_payload(triggers));
        return b;
}

extern "C" {

// Untimed corpus materialization.
long bench_mappolygons_materialize(uint8_t *out, size_t cap)
{
        std::vector<uint8_t> b = build_corpus();
        if (b.size() > cap)
                return -1;
        memcpy(out, b.data(), b.size());
        return (long)b.size();
}

// The logical load: the gen's own walk over the corpus (the registered
// PolygonTriggers parser, the flat record reads), with the final-list
// checksum.
uint64_t bench_mappolygons_parse(const uint8_t *data, size_t len)
{
        std::vector<uint8_t> bytes(data, data + len);
        CachedFileInputStream s(bytes);
        DataChunkInput in(&s);
        PtState st;
        in.registerParser("PolygonTriggers", "", parsePolygonTriggersDataChunk);
        if (!in.parse(&st))
                return 0xdeadbeefULL;
        return pt_list_checksum(st);
}

} // extern "C"
