// SPDX-License-Identifier: GPL-3.0-or-later
//
// C++ baseline timing wrapper around the gen_kernel_golden.cpp
// transcription.
//
// Times the game-logic kernel — the frame loop's full walk over the
// formula-generated mixed corpus (120 objects: the small-sphere, sphere,
// cylinder and rotated-box geometries; every third object moving each
// frame so the partition refresh's fills and contact-pair collection
// run; two objects destroyed at frame 90 so the destroy list's
// two-pass sleepy erase runs; 180 frames per measurement), the
// sleepy dispatch alone (the same corpus with no movement — the heap's
// requeue churn) and the query family over the settled corpus (300
// mixed queries: the closest-object ring walk twice and the
// near-to-far-sorted range iterator drained — the query slice).
//
// The corpus is built by the same formulas on both sides and the final
// -state checksums (the heap array's packed priorities in order, the
// object list's coverage and cell-change counts, the contact-pair
// count) lock the two together — the Rust `kernel/…` criterion benches
// assert the identical values.
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

#define main gen_kernel_generator_main
#include "gen_kernel_golden.cpp"
#undef main

// The corpus size and frames (the Rust bench's constants).
#define BENCH_OBJECTS 120
#define BENCH_FRAMES 180

// The authoring-time lockstep checksums (the Rust bench asserts the same).
#define FRAME_LOOP_CHECKSUM 16055563144769613415ULL
#define DISPATCH_CHECKSUM 14539779273077219524ULL
#define QUERY_CHECKSUM 1025416407242357139ULL

// The final-state checksum (the same formula the Rust bench computes).
static uint64_t kernel_checksum(World &w)
{
        uint64_t sum = 1000003ULL;
        sum = sum * 31 + (uint64_t)w.logic.m_sleepyUpdates.size();
        for (UpdateModuleT *u : w.logic.m_sleepyUpdates)
                sum = sum * 31 + (uint64_t)friend_getPriority(u);
        for (ObjectT *obj : w.logic.m_objList) {
                PartitionData *part = w.pm.findModule(obj->m_id);
                sum = sum * 31 + (uint64_t)(part ? part->m_coiInUseCount : 0);
                sum = sum * 31 + (!part || part->m_lastCell < 0 ? 0xFFFFFFFFFFFFFFFFULL : (uint64_t)part->m_lastCell);
                sum = sum * 31 + (uint64_t)obj->m_cellChangeCount;
        }
        sum = sum * 31 + (uint64_t)w.pm.m_lastContacts.size();
        return sum;
}

// The corpus: the Rust bench's build_corpus, formula for formula.
static World *build_corpus()
{
        World *w = new World();
        // GameLogic::init's m_nextObjID = 1 (the ids run 1..=120)
        w->logic.setObjectIDCounter(1);
        w->pm.init(10.0f, Region2DT{0, 0, 300, 300});

        for (int i = 0; i < BENCH_OBJECTS; ++i) {
                GeometryInfoT geom;
                switch (i % 4) {
                case 0:
                        geom = GeometryInfoT{GEOMETRY_SPHERE, true, 1.0f, 2.5f, 2.5f};
                        break;
                case 1:
                        geom = GeometryInfoT{GEOMETRY_SPHERE, false, 1.0f, 7.0f, 7.0f};
                        break;
                case 2:
                        geom = GeometryInfoT{GEOMETRY_CYLINDER, false, 4.0f, 12.5f, 12.5f};
                        break;
                default:
                        geom = GeometryInfoT{GEOMETRY_BOX, false, 4.0f, 10.0f, 6.0f};
                        break;
                }
                bool immobile = (i % 37) == 0;
                int moduleCount = (i % 2 == 0) ? 1 : 2;
                int phase = i % 4; // {INITIAL, PHYSICS, NORMAL, FINAL}
                Coord3D pos;
                pos.x = 20.0f + (float)(i % 20) * 13.0f + (float)(i / 20) * 2.5f;
                pos.y = 20.0f + (float)(i / 20) * 11.5f;
                pos.z = 0.0f;
                float angle = (i % 4 == 3) ? 0.3f : 0.0f;

                std::vector<int> phases;
                for (int m = 0; m < moduleCount; ++m)
                        phases.push_back(phase);
                ObjectT *obj = w->createObject(geom, immobile, moduleCount, {}, pos, angle, phases);
                for (int m = 0; m < moduleCount; ++m)
                        obj->m_updates[(size_t)m].m_scriptedSleep = (m == 0) ? 1 + (i % 30) : 5 + (i % 17);
        }
        return w;
}

// The mixed run: every third object moves each frame, two die at 90.
static uint64_t run_frames(World *w, bool moving)
{
        for (int f = 0; f < BENCH_FRAMES; ++f) {
                if (moving) {
                        for (int i = 1; i <= BENCH_OBJECTS; ++i) {
                                ObjectT *obj = w->object(i);
                                if (!obj)
                                        continue; // destroyed at frame 90
                                if ((f + i) % 3 == 0) {
                                        Coord3D old = obj->m_pos;
                                        obj->m_pos.x = old.x + 1.7f;
                                        obj->m_pos.y = old.y + 0.9f;
                                        w->reactToTransformChange(obj, old, obj->m_angle);
                                }
                        }
                }
                if (f == 90) {
                        w->logic.destroyObject(w->object(11), &w->hooks);
                        w->logic.destroyObject(w->object(71), &w->hooks);
                }
                w->update();
                w->hooks.m_events.clear(); // (the bench host records nothing)
        }
        return kernel_checksum(*w);
}

extern "C" {

// Untimed corpus materialization (the measure_ns_ctx setup form).
void *bench_kernel_build()
{
        return build_corpus();
}

// Untimed corpus materialization for the query bench: the corpus
// settled by one full update (the COI coverage the ring walk reads).
void *bench_kernel_query_build()
{
        World *w = build_corpus();
        w->update();
        w->hooks.m_events.clear(); // (the bench host records nothing)
        return w;
}

// The timed body: the full frame loop over the mixed corpus.
uint64_t bench_kernel_frame_loop(void *world)
{
        return run_frames((World *)world, true);
}

// The timed body: the sleepy dispatch alone (no movement).
uint64_t bench_kernel_dispatch(void *world)
{
        return run_frames((World *)world, false);
}

// The timed body: the query family over the settled corpus (the Rust
// bench's run_queries, formula for formula — a third closest-object
// center-2d queries, a third boundary-2d closest behind an on-map
// filter, a third near-to-far-sorted range iterators drained).
uint64_t bench_kernel_query(void *world)
{
        World *w = (World *)world;
        uint64_t sum = 1000003ULL;
        for (uint64_t i = 0; i < 300; ++i) {
                Coord3D pos;
                pos.x = 20.0f + (float)((i * 7) % 260);
                pos.y = 15.0f + (float)((i * 13) % 270);
                pos.z = 0.0f;
                switch (i % 3) {
                case 0: {
                        float dist;
                        Coord3D vec;
                        ObjectT *o = w->pm.getClosestObject(&pos, 45.0f, FROM_CENTER_2D, nullptr, &dist, &vec);
                        sum = sum * 31 + (uint64_t)(o ? o->m_id : 0);
                        sum = sum * 31 + (uint64_t)bits_of(dist);
                        break;
                }
                case 1: {
                        PartitionFilterOnMapT onMap;
                        PartitionFilterT *filters[] = {&onMap, nullptr};
                        float dist;
                        Coord3D vec;
                        ObjectT *o = w->pm.getClosestObject(&pos, 60.0f, FROM_BOUNDINGSPHERE_2D, filters, &dist, &vec);
                        sum = sum * 31 + (uint64_t)(o ? o->m_id : 0);
                        sum = sum * 31 + (uint64_t)bits_of(dist);
                        break;
                }
                default: {
                        SimpleObjectIteratorT iter = w->pm.iterateObjectsInRange(&pos, 35.0f, FROM_CENTER_2D, nullptr, ITER_SORTED_NEAR_TO_FAR);
                        float num;
                        iter.reset();
                        for (int o = iter.nextWithNumeric(&num); o != INVALID_ID; o = iter.nextWithNumeric(&num)) {
                                sum = sum * 31 + (uint64_t)o;
                                sum = sum * 31 + (uint64_t)bits_of(num);
                        }
                        break;
                }
                }
        }
        return sum;
}

void bench_kernel_teardown(void *world)
{
        delete (World *)world;
}

// The lockstep check: the corpus is deterministic, so the checksums
// must equal the Rust bench's pinned values.
int bench_kernel_verify()
{
        {
                World *w = build_corpus();
                uint64_t sum = run_frames(w, true);
                delete w;
                if (sum != FRAME_LOOP_CHECKSUM)
                        return 0;
        }
        {
                World *w = build_corpus();
                uint64_t sum = run_frames(w, false);
                delete w;
                if (sum != DISPATCH_CHECKSUM)
                        return 0;
        }
        {
                World *w = build_corpus();
                w->update(); // the settle: the COI coverage the queries walk
                w->hooks.m_events.clear();
                uint64_t sum = bench_kernel_query(w);
                delete w;
                if (sum != QUERY_CHECKSUM)
                        return 0;
        }
        return 1;
}

} // extern "C"
