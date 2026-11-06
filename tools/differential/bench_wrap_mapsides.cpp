// SPDX-License-Identifier: GPL-3.0-or-later
//
// C++ baseline timing wrapper around the gen_mapsides_golden.cpp
// transcription.
//
// Times the verbatim SidesList.cpp sides load path — the gen's own
// parseSidesDataChunk walk (the dict reads, the build lists, the team
// list and validateSides' whole cleanup) over 264 repeated SidesList
// chunks (each: the neutral side, three player sides with two build
// entries, and a five-team list) — on the same formula-generated corpus
// as the Rust `mapsides/…` criterion benches.
//
// The corpus is hand-assembled here through the generator's byte
// builders and on the Rust side through the same formula helpers; the
// golden corpus set pins the construction lockstep, and both sides
// walk **identical bytes** — verified at authoring time (same corpus
// size and the same final-state walk checksum, which the Rust bench
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

#define main gen_mapsides_generator_main
#include "gen_mapsides_golden.cpp"
#undef main

// The bench corpus repetition (264 chunks ≈ 190 KiB, the scale of the
// other map benches).
#define BENCH_CHUNKS 264

// The final-state checksum over the payload's observable fields (the
// same walk the Rust bench drives): the counts, every dict pair (the
// name-key ordinal, the type and a value word), every build entry's
// fields and the modified flag.
static uint64_t value_word(const Dict::DictPair &pair)
{
        switch (pair.getType()) {
                case Dict::DICT_BOOL:
                        return pair.b ? 1 : 0;
                case Dict::DICT_INT:
                        return (uint64_t)(uint32_t)pair.i;
                case Dict::DICT_REAL: {
                        UnsignedInt bits;
                        memcpy(&bits, &pair.r, 4);
                        return bits;
                }
                case Dict::DICT_ASCIISTRING: {
                        uint64_t sum = (uint64_t)pair.as.getLength();
                        for (unsigned char c : pair.as.s)
                                sum += c;
                        return sum;
                }
                case Dict::DICT_UNICODESTRING: {
                        uint64_t sum = (uint64_t)pair.us.getLength();
                        for (WideChar w : pair.us.u)
                                sum += w;
                        return sum;
                }
                default:
                        return 0;
        }
}

static uint64_t dict_checksum(const Dict &d)
{
        uint64_t checksum = (uint64_t)(uint32_t)d.getPairCount();
        for (Int p = 0; p < d.getPairCount(); ++p)
                checksum += (uint64_t)d.m_pairs[p].getName() +
                            (uint64_t)(uint32_t)d.m_pairs[p].getType() +
                            value_word(d.m_pairs[p]);
        return checksum;
}

static uint64_t sd_state_checksum(const SidesListLoad &st)
{
        uint64_t checksum = (uint64_t)st.m_sides.size() + (uint64_t)st.m_teams.size() +
                            (st.m_modified ? 1 : 0);
        for (const SidesInfoLoad &side : st.m_sides) {
                checksum += dict_checksum(side.m_dict);
                checksum += (uint64_t)side.m_buildList.size();
                for (const BuildListLoad &b : side.m_buildList) {
                        checksum += (uint64_t)b.m_buildingName.getLength();
                        for (unsigned char c : b.m_buildingName.s)
                                checksum += c;
                        checksum += (uint64_t)b.m_templateName.getLength();
                        for (unsigned char c : b.m_templateName.s)
                                checksum += c;
                        checksum += (uint64_t)f32bits(b.m_location.x) +
                                    (uint64_t)f32bits(b.m_location.y) +
                                    (uint64_t)f32bits(b.m_location.z) +
                                    (uint64_t)f32bits(b.m_angle);
                        checksum += b.m_isInitiallyBuilt ? 1 : 0;
                        checksum += (uint64_t)(Int)b.m_numRebuilds;
                        checksum += (uint64_t)(uint32_t)b.m_health;
                        checksum += (b.m_whiner ? 1 : 0) + (b.m_unsellable ? 2 : 0) +
                                    (b.m_repairable ? 4 : 0);
                        checksum += (uint64_t)b.m_script.getLength();
                        for (unsigned char c : b.m_script.s)
                                checksum += c;
                }
        }
        for (const Dict &team : st.m_teams)
                checksum += dict_checksum(team);
        return checksum;
}

// Builds the corpus: 264 SidesList chunks (the neutral, three players
// with two build entries each, five teams) — keep in lockstep with
// crates/rzh-formats/benches/rzh_formats.rs.
static std::vector<uint8_t> build_corpus()
{
        std::vector<uint8_t> b = sides_toc();
        for (Int c = 0; c < BENCH_CHUNKS; ++c) {
                std::vector<uint8_t> p;
                put_i32(p, 4);
                { // the neutral
                        std::vector<std::vector<uint8_t>> pairs;
                        pairs.push_back(pair_ascii(SK_playerName, ""));
                        put_bytes(p, side_payload(pairs, {}));
                }
                for (Int s = 0; s < 3; ++s) {
                        char name[16], ally[16], faction[24];
                        snprintf(name, sizeof name, "Plyr%d_%d", (int)s, (int)(c % 7));
                        snprintf(ally, sizeof ally, "Plyr%d_%d", (int)((s + 1) % 3), (int)(c % 7));
                        snprintf(faction, sizeof faction, "Faction%d", (int)(s % 3));
                        std::vector<std::vector<uint8_t>> pairs =
                            player_pairs(name, s == 0, faction, ally, ally);
                        std::vector<std::vector<uint8_t>> builds;
                        for (Int e = 0; e < 2; ++e) {
                                char building[24], script[16];
                                snprintf(building, sizeof building, "Bldg%d_%d", (int)s, (int)e);
                                snprintf(script, sizeof script, "Script%d", (int)e);
                                builds.push_back(build_entry(
                                    K_SIDES_DATA_VERSION_3, building, faction,
                                    (Real)((c * 4 + s * 2 + e) % 400), (Real)(e * 100 + s),
                                    (Real)(e * 50), (Real)(e) * 1.5f, (e & 1) == 0, (Int)(e * 3),
                                    script, 80 + e * 5, e == 0, e != 1, e != 2));
                        }
                        put_bytes(p, side_payload(pairs, builds));
                }
                put_i32(p, 5);
                for (Int s = 0; s < 3; ++s) {
                        char name[16], owner[16];
                        snprintf(name, sizeof name, "teamPlyr%d_%d", (int)s, (int)(c % 7));
                        snprintf(owner, sizeof owner, "Plyr%d_%d", (int)s, (int)(c % 7));
                        put_bytes(p, team_payload(team_pairs(name, owner, true)));
                }
                put_bytes(p, team_payload(team_pairs("AttackSquad", "Plyr0_0", false)));
                put_bytes(p, team_payload(team_pairs("team", "", true)));
                put_chunk(b, 1, K_SIDES_DATA_VERSION_3, p);
        }
        return b;
}

extern "C" {

// Untimed corpus materialization.
long bench_mapsides_materialize(uint8_t *out, size_t cap)
{
        std::vector<uint8_t> b = build_corpus();
        if (b.size() > cap)
                return -1;
        memcpy(out, b.data(), b.size());
        return (long)b.size();
}

// The logical load: the gen's own walk over the corpus (the registered
// SidesList parser, the dicts, the build lists, the validation), with
// the final-state checksum.
uint64_t bench_mapsides_parse(const uint8_t *data, size_t len)
{
        std::vector<uint8_t> bytes(data, data + len);
        CachedFileInputStream s(bytes);
        DataChunkInput in(&s);
        SdState st;
        st.m_wk.seed(st.m_keys);
        in.registerParser("SidesList", "", parseSidesDataChunk);
        if (!in.parse(&st))
                return 0xdeadbeefULL;
        return sd_state_checksum(st.m_sidesList);
}

} // extern "C"
