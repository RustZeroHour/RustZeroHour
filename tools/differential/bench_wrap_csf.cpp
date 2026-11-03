// SPDX-License-Identifier: GPL-3.0-or-later
//
// C++ baseline timing wrapper around the gen_csf_golden.cpp transcriptions.
//
// Times the verbatim Babylon CSF writer (expimp.cpp) and the verbatim
// runtime reader (GameText.cpp parseCSF + stripSpaces) on the same
// formula-generated corpus as the Rust `csf/…` criterion benches — the
// corpus is built here through the transcribed writer, so both sides
// parse **identical bytes**.
//
// The generator's own main() is renamed out of the way; nothing in it is
// modified.
//
// Temporary differential-testing infrastructure: removed at cut-over (M5).
//
// Copyright 2025 Electronic Arts Inc.
// Rust port Copyright 2026 the RustZeroHour contributors.
//
// This program is free software; you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the
// Free Software Foundation, either version 3 of the License, or (at your
// option) any later version.

#define main gen_csf_generator_main
#include "gen_csf_golden.cpp"
#undef main

extern "C" {

// The corpus of the Rust csf benches: 4096 labels; every 8th label
// carries a speech wave, every 16th a second (empty) string. Keep the
// formulas in lockstep with crates/rzh-formats/benches/rzh_formats.rs.
void *bench_csf_db_new()
{
        auto *db = new std::vector<BabylonLabel>();
        char name[32];
        char text[96];
        char wave[32];
        for (uint32_t i = 0; i < 4096; ++i)
        {
                BabylonLabel label;
                snprintf(name, sizeof(name), "GUI:Label_%05u", i);
                label.name = name;

                snprintf(text, sizeof(text),
                        "Mission %u: Destroy the enemy %u structure before dawn.", i, i);
                if (i % 8 == 0)
                {
                        snprintf(wave, sizeof(wave), "SPEECH:Ev_%05u.wav", i);
                        label.texts.push_back({widen(text), wave});
                }
                else
                {
                        label.texts.push_back({widen(text), std::string()});
                }
                if (i % 16 == 0)
                {
                        label.texts.push_back({{}, std::string()});
                }
                db->push_back(label);
        }
        return db;
}

void bench_csf_db_free(void *db)
{
        delete (std::vector<BabylonLabel> *)db;
}

// The transcribed `generate_csf` into a fresh sink — the exact shape of
// the Rust `to_bytes()` (one allocation chain, no extra copies). Returns
// the byte count, or 0 on refusal.
std::uint64_t bench_csf_write(void *db)
{
        Sink file;
        if (!generate_csf(*(std::vector<BabylonLabel> *)db, 0, file))
        {
                return 0;
        }
        return file.size();
}

// Untimed corpus materialization for the parse bench (same transcription).
long bench_csf_materialize(void *db, uint8_t *out, size_t cap)
{
        Sink file;
        if (!generate_csf(*(std::vector<BabylonLabel> *)db, 0, file))
        {
                return -1;
        }
        if (file.size() > cap)
        {
                return -1;
        }
        memcpy(out, file.data(), file.size());
        return (long)file.size();
}

// The transcribed `parseCSF` over the corpus bytes; the checksum of the
// game-visible strings drives the sink. 0 = rejected.
uint32_t bench_csf_parse(const uint8_t *data, size_t len)
{
        Cursor cursor{data, len, 0};
        std::vector<StringInfo> labels;
        if (!parseCSF(cursor, labels))
        {
                return 0;
        }
        uint32_t sum = 0;
        for (const StringInfo &label : labels)
        {
                sum += (uint32_t)label.label.size();
                sum += (uint32_t)label.text.size();
                sum += (uint32_t)label.speech.size();
        }
        return sum;
}

// The consumer view of the Rust `csf/game_text` bench: per label, the
// first string decoded into a reused buffer (truncate at the first
// 0x0000/0xFFFF, then stripSpaces) — the original's member-buffer shape.
uint32_t bench_csf_game_text(void *db)
{
        static WideChar buf[MAX_UITEXT_LENGTH * 2];
        uint32_t sum = 0;
        for (const BabylonLabel &label : *(std::vector<BabylonLabel> *)db)
        {
                const std::vector<WideChar> &text = label.texts[0].text;

                size_t end = text.size();
                for (size_t i = 0; i < text.size(); ++i)
                {
                        if (text[i] == 0x0000 || text[i] == 0xFFFF)
                        {
                                end = i;
                                break;
                        }
                }
                for (size_t i = 0; i < end; ++i)
                {
                        buf[i] = text[i];
                }
                buf[end] = 0;
                stripSpaces(buf);

                for (WideChar *p = buf; *p; ++p)
                {
                        sum += 1;
                }
        }
        return sum;
}

} // extern "C"
