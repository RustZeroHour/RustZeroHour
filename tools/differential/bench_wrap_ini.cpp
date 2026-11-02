// SPDX-License-Identifier: GPL-3.0-or-later
//
// C++ baseline timing wrapper around the gen_ini_golden.cpp transcription.
//
// Times the verbatim INI engine (INI.cpp reader, tokenizer, scanType<T>,
// initFromINI and the three task-10 block schemas) on the same fixture
// files the golden dump replays, with fresh stores per load — exactly the
// scenario the Rust `parse/fixtures` criterion benches measure. The driver
// reads the fixture bytes and applies the CRLF transform for edge.ini,
// like the golden generator does.
//
// The transcription's own main() is renamed out of the way; nothing in it
// is modified.
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

#define main gen_ini_generator_main
#include "gen_ini_golden.cpp"
#undef main

extern "C" {

// The full fixture scenario of the Rust `parse/fixtures_all` bench: five
// loads into ONE context (one name-key world, like a real data load), with
// the golden load types (override.ini = CREATE_OVERRIDES, edge.ini CRLF).
uint32_t bench_ini_parse_scenario(
        const uint8_t *armor, size_t armor_len,
        const uint8_t *loco, size_t loco_len,
        const uint8_t *weapon, size_t weapon_len,
        const uint8_t *over, size_t over_len,
        const uint8_t *edge, size_t edge_len)
{
        GameData game;

        INI a;
        loadIni(a, "armor.ini", std::vector<uint8_t>(armor, armor + armor_len), INI_LOAD_OVERWRITE, game);
        INI l;
        loadIni(l, "locomotor.ini", std::vector<uint8_t>(loco, loco + loco_len), INI_LOAD_OVERWRITE, game);
        INI w;
        loadIni(w, "weapon.ini", std::vector<uint8_t>(weapon, weapon + weapon_len), INI_LOAD_OVERWRITE, game);
        INI o;
        loadIni(o, "override.ini", std::vector<uint8_t>(over, over + over_len), INI_LOAD_CREATE_OVERRIDES, game);

        std::vector<uint8_t> edgeBytes(edge, edge + edge_len);
        edgeBytes = toCrlf(edgeBytes);
        INI e;
        loadIni(e, "edge.ini", edgeBytes, INI_LOAD_OVERWRITE, game);
        return 5;
}

// The synthetic Armor corpus of the Rust `parse/synthetic_armor_x512`
// bench, generated with the identical template (512 blocks, 20 damage
// coefficients each, a comment line, every 16th block blank).
uint32_t bench_ini_parse_synthetic(const uint8_t *data, size_t len)
{
        GameData game;
        INI ini;
        return loadIni(ini, "Synthetic.ini", std::vector<uint8_t>(data, data + len), INI_LOAD_OVERWRITE, game);
}

} // extern "C"
