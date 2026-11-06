// SPDX-License-Identifier: GPL-3.0-or-later
//
// C++ baseline timing wrapper around the gen_mapobjects_golden.cpp
// transcription.
//
// Times the verbatim WorldHeightMap.cpp objects-list load path — the
// gen's own parseObjectsDataChunk/parseObjectDataChunk/parseObjectData
// walk over a version-3 list of 4096 objects (a location, an angle,
// flags, a formula name and a two-pair property dict each) — on the
// same formula-generated corpus as the Rust `mapobjects/…` criterion
// benches.
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

#define main gen_mapobjects_generator_main
#include "gen_mapobjects_golden.cpp"
#undef main

// The bench corpus size (4096 objects ≈ 180 KiB, the scale of the other
// map benches).
#define BENCH_OBJECTS 4096

// The checksum over the kept list's observable fields (the same walk
// the Rust bench drives).
static uint64_t ob_list_checksum(const std::vector<MapObjectLoad> &objects)
{
	uint64_t checksum = 0;
	for (const MapObjectLoad &o : objects) {
		checksum += (uint64_t)f32bits(o.m_location.x);
		checksum += (uint64_t)f32bits(o.m_location.y);
		checksum += (uint64_t)f32bits(o.m_location.z);
		checksum += (uint64_t)f32bits(o.m_angle);
		checksum += (uint64_t)(uint32_t)o.m_flags;
		for (unsigned char c : o.m_objectName.s)
			checksum += c;
		checksum += (uint64_t)(uint32_t)o.m_properties.getPairCount();
		checksum += o.m_isWaypoint ? 1 : 0;
		checksum += o.m_isLight ? 2 : 0;
		checksum += o.m_isScorch ? 4 : 0;
	}
	return checksum;
}

// Builds the corpus: one ObjectsList chunk with 4096 version-3 Object
// children (formula locations, angles, flags, names and two-pair
// dicts) — keep in lockstep with
// crates/rzh-formats/benches/rzh_formats.rs.
static std::vector<uint8_t> build_corpus()
{
	std::vector<uint8_t> b = toc_bytes({{ "ObjectsList", 1 },
	                                    { "Object", 2 },
	                                    { "objectEnabled", 3 },
	                                    { "waypointID", 4 }});
	std::vector<std::pair<uint16_t, std::vector<uint8_t>>> children;
	for (Int i = 0; i < BENCH_OBJECTS; ++i) {
		char name[16];
		snprintf(name, sizeof name, "Obj%04d", (int)i);
		std::vector<std::vector<uint8_t>> pairs;
		pairs.push_back(pair_bool(3, (i & 1) != 0)); // objectEnabled
		if ((i % 16) == 0)
			pairs.push_back(pair_int(4, i)); // waypointID
		children.push_back({ K_OBJECTS_VERSION_3,
			             object_payload((Real)(i % 256) * 0.5f, (Real)(i % 128) * 1.25f,
			                            (Real)(i % 32) * 12.5f, (Real)(i % 97) * 0.25f - 12.0f,
			                            i % 0x101, name, pairs, true) });
	}
	std::vector<uint8_t> inner = objects_list_payload(2, children);
	put_chunk(b, 1, K_OBJECTS_VERSION_3, inner);
	return b;
}

extern "C" {

// Untimed corpus materialization.
long bench_mapobjects_materialize(uint8_t *out, size_t cap)
{
	std::vector<uint8_t> b = build_corpus();
	if (b.size() > cap)
		return -1;
	memcpy(out, b.data(), b.size());
	return (long)b.size();
}

// The logical load: the gen's own walk over the corpus (the registered
// ObjectsList parser, the nested Object parser, the dict interning),
// with the final-list checksum.
uint64_t bench_mapobjects_parse(const uint8_t *data, size_t len)
{
	std::vector<uint8_t> bytes(data, data + len);
	CachedFileInputStream s(bytes);
	DataChunkInput in(&s);
	ObState st;
	st.seedWellKnownKeys();
	in.registerParser("ObjectsList", "", parseObjectsDataChunk);
	if (!in.parse(&st))
		return 0xdeadbeefULL;
	return ob_list_checksum(st.m_objects);
}

} // extern "C"
