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
// requeue churn), the query family over the settled corpus (300
// mixed queries: the closest-object ring walk twice and the
// near-to-far-sorted range iterator drained — the query slice) and the
// players/teams block over the settled corpus (300 mixed operations:
// the relationship-to-mask sweeps, the relationship chains and the full
// updateTeamStates walk with the enemy-sighted partition queries — the
// players/teams/relationships slice) and the money/energy/ranks/sciences
// block over the same corpus promoted with the data stores (300 mixed
// operations: the money streams, the energy influence walks with the
// brownout sweeps, the rank ladder and the science purchases — the
// money/energy/ranks/sciences slice).
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
#define PLAYERS_CHECKSUM 10479098065696129437ULL
#define PURSE_CHECKSUM 12146934425632222704ULL

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

// The players corpus: the Rust bench's players_corpus, formula for
// formula — the 4-side world with the enemies/allies assignment, the
// 120 objects distributed over the default teams (every fifth to its
// player's squad instead), the vision ranges and AI bits, settled by
// one full update.
static World *build_players_corpus()
{
        World *w = build_corpus();
        w->hooks.m_events.clear();

        SidesSpecT sides;
        sides.sides.push_back(SideSpecT{}); // the neutral side
        SideSpecT p1;
        p1.playerName = "p1";
        p1.isHuman = true;
        p1.enemies = {"p2"};
        sides.sides.push_back(p1);
        SideSpecT p2;
        p2.playerName = "p2";
        sides.sides.push_back(p2);
        SideSpecT p3;
        p3.playerName = "p3";
        p3.allies = {"p1"};
        sides.sides.push_back(p3);

        sides.teams.push_back(teamSpec("team", "neutral", true));
        for (int p = 1; p <= 3; ++p) {
                char name[32], owner[32];
                snprintf(name, sizeof(name), "teamp%d", p);
                snprintf(owner, sizeof(owner), "p%d", p);
                sides.teams.push_back(teamSpec(name, owner, true));
        }
        for (int p = 1; p <= 3; ++p) {
                char name[32], owner[32];
                snprintf(name, sizeof(name), "p%dsquad", p);
                snprintf(owner, sizeof(owner), "p%d", p);
                sides.teams.push_back(teamSpec(name, owner, false));
        }
        w->playerListNewGame(sides);

        TeamIDT defaultTeams[3], squads[3];
        for (int p = 1; p <= 3; ++p)
                defaultTeams[p - 1] = w->playerList.getNthPlayer(p)->m_defaultTeam;
        for (int p = 1; p <= 3; ++p) {
                char name[32];
                snprintf(name, sizeof(name), "p%dsquad", p);
                squads[p - 1] = w->teamFactory.createTeam(name, w)->getID();
        }
        for (int i = 1; i <= BENCH_OBJECTS; ++i) {
                int player = (i % 3) + 1;
                TeamIDT team = (i % 5 == 0) ? squads[player - 1] : defaultTeams[player - 1];
                objectSetTeam(w->object(i), teamByID(w, team), w);
                ObjectT *obj = w->object(i);
                obj->m_visionRange = 5.0f + (float)(i % 7);
                obj->m_hasAi = (i % 3) != 0;
                obj->m_aiIdle = (i % 2) == 0;
        }

        w->update();
        w->hooks.m_events.clear(); // (the bench host records nothing)
        w->hooks.m_teamEvents.clear();
        return w;
}

// The timed body: 300 mixed players/teams operations (the Rust bench's
// run_players, formula for formula — a third the relationship-to-mask
// sweeps, a third the relationship chains, a third the full
// updateTeamStates walk with the enemy-sighted partition queries).
static uint64_t run_players(World *w)
{
        uint64_t sum = 1000003ULL;
        for (uint64_t i = 0; i < 300; ++i) {
                switch (i % 3) {
                case 0: {
                        const unsigned flags = (unsigned)ALLOW_ALLIES_T | (unsigned)ALLOW_ENEMIES_T |
                                               (unsigned)ALLOW_NEUTRAL_T | ((unsigned)(i & 1) * (unsigned)ALLOW_SAME_PLAYER_T);
                        for (int p = 0; p < 5; ++p) {
                                PlayerMaskT mask = w->playerListGetPlayersWithRelationship(p, flags);
                                sum = sum * 31 + (uint64_t)mask;
                        }
                        break;
                }
                case 1: {
                        for (uint64_t k = 0; k < 12; ++k) {
                                int a = (int)((i * 7 + k * 13) % 120 + 1);
                                int b = (int)((i * 11 + k * 29) % 120 + 1);
                                RelationshipT r = objectGetRelationship(w->object(a), w->object(b), w);
                                sum = sum * 31 + (uint64_t)r;
                                TeamT *ta = teamByID(w, w->object(a)->m_team);
                                TeamT *tb = teamByID(w, w->object(b)->m_team);
                                RelationshipT tr = teamGetRelationship(ta, tb, w);
                                sum = sum * 31 + (uint64_t)tr;
                        }
                        break;
                }
                default: {
                        w->playerListUpdateTeamStates();
                        uint64_t teamFold = 0;
                        for (auto &kv : w->teamFactory.prototypesInMapOrder()) {
                                for (auto it = kv.second->m_instances.rbegin(); it != kv.second->m_instances.rend(); ++it) {
                                        TeamT *t = *it;
                                        teamFold = teamFold * 31 + (uint64_t)t->getID();
                                        teamFold = teamFold * 31 + (uint64_t)t->m_curUnits;
                                        teamFold = teamFold * 31 + (uint64_t)(t->m_seeEnemy ? 1 : 0);
                                }
                        }
                        sum = sum * 31 + teamFold;
                        break;
                }
                }
        }
        return sum;
}


// The purse corpus: the Rust bench's purse_corpus, formula for formula —
// the players corpus promoted with the block's data stores (loaded
// BEFORE the new_game so the init walk runs the ladder), the objects
// carrying energy values, the players' wallets and radars — settled by
// one full update.
static World *build_purse_corpus()
{
        World *w = build_corpus();
        w->hooks.m_events.clear();

        // the block's data FIRST: the science catalog (chains, costs,
        // the non-grantable) and the 5-rung ladder
        for (int i = 0; i < 8; ++i) {
                std::vector<std::string> prereqs;
                if (i % 3 != 0)
                        prereqs.push_back(std::string("S") + std::to_string(i - 1));
                w->scienceStore.load(std::string("S") + std::to_string(i), prereqs, 1 + (i % 3),
                                     (i % 5) != 0, w);
        }
        {
                const int needed[5] = {0, 100, 300, 600, 1000};
                for (int i = 0; i < 5; ++i) {
                        std::vector<std::string> granted;
                        granted.push_back(std::string("S") + std::to_string(i < 7 ? i : 7));
                        w->rankStore.load(needed[i], 1 + i, granted, w);
                }
        }

        SidesSpecT sides;
        sides.sides.push_back(SideSpecT{}); // the neutral side
        SideSpecT p1;
        p1.playerName = "p1";
        p1.isHuman = true;
        p1.enemies = {"p2"};
        sides.sides.push_back(p1);
        SideSpecT p2;
        p2.playerName = "p2";
        sides.sides.push_back(p2);
        SideSpecT p3;
        p3.playerName = "p3";
        p3.allies = {"p1"};
        sides.sides.push_back(p3);

        sides.teams.push_back(teamSpec("team", "neutral", true));
        for (int p = 1; p <= 3; ++p) {
                char name[32], owner[32];
                snprintf(name, sizeof(name), "teamp%d", p);
                snprintf(owner, sizeof(owner), "p%d", p);
                sides.teams.push_back(teamSpec(name, owner, true));
        }
        for (int p = 1; p <= 3; ++p) {
                char name[32], owner[32];
                snprintf(name, sizeof(name), "p%dsquad", p);
                snprintf(owner, sizeof(owner), "p%d", p);
                sides.teams.push_back(teamSpec(name, owner, false));
        }
        w->playerListNewGame(sides);

        TeamIDT defaultTeams[3], squads[3];
        for (int p = 1; p <= 3; ++p)
                defaultTeams[p - 1] = w->playerList.getNthPlayer(p)->m_defaultTeam;
        for (int p = 1; p <= 3; ++p) {
                char name[32];
                snprintf(name, sizeof(name), "p%dsquad", p);
                squads[p - 1] = w->teamFactory.createTeam(name, w)->getID();
        }
        for (int i = 1; i <= BENCH_OBJECTS; ++i) {
                int player = (i % 3) + 1;
                TeamIDT team = (i % 5 == 0) ? squads[player - 1] : defaultTeams[player - 1];
                objectSetTeam(w->object(i), teamByID(w, team), w);
                ObjectT *obj = w->object(i);
                obj->m_visionRange = 5.0f + (float)(i % 7);
                obj->m_hasAi = (i % 3) != 0;
                obj->m_aiIdle = (i % 2) == 0;
        }

        // the energy values: producers, consumers and bonus carriers
        // over the teams (the template fields the corpus programs)
        for (int i = 1; i <= BENCH_OBJECTS; ++i) {
                ObjectT *obj = w->object(i);
                obj->m_energyProduction = (i % 4 == 0) ? 30 : (i % 4 == 1) ? -20 : (i % 4 == 2) ? 25 : -10;
                obj->m_energyBonus = (i % 3 == 0) ? 50 : 0;
        }
        // the POWERED kindof bit (the spec's powered flag's equivalent)
        for (int i = 1; i <= BENCH_OBJECTS; ++i) {
                if (i % 2 == 0)
                        w->object(i)->m_kindof |= (((unsigned __int128)1) << KINDOF_POWERED_BIT);
        }

        // the wallets and the radars
        for (int p = 1; p <= 3; ++p) {
                w->playerList.getNthPlayer(p)->m_money.setStartingCash((unsigned)(10000 + p * 13));
                w->playerList.getNthPlayer(p)->addRadar((p % 2) == 0);
        }

        w->update();
        w->hooks.m_events.clear(); // (the bench host records nothing)
        w->hooks.m_teamEvents.clear();
        return w;
}

// The timed body: 300 mixed money/energy/ranks/sciences operations (the
// Rust bench's run_purse, formula for formula — a quarter the money
// streams, a quarter the energy influence walks with the brownout
// sweeps, a quarter the rank ladder, a quarter the science purchases).
static uint64_t run_purse(World *w)
{
        uint64_t sum = 1000003ULL;

        // the science keys, resolved once (the catalog's ordinals)
        unsigned sciences[8];
        for (int i = 0; i < 8; ++i)
                sciences[i] = w->nameKeys.nameToKey(std::string("S") + std::to_string(i));

        for (uint64_t i = 0; i < 300; ++i) {
                w->logic.m_frame = (unsigned)i;
                switch (i % 4) {
                case 0: {
                        // the money stream
                        for (int p = 1; p <= 3; ++p) {
                                bool sound = ((i + (uint64_t)p) % 2) == 0;
                                bool track = ((i + (uint64_t)p) % 3) == 0;
                                MoneyT *wallet = &w->playerList.getNthPlayer(p)->m_money;
                                wallet->withdraw(50 + (unsigned)((i + (uint64_t)p) % 97), sound, w);
                                wallet->deposit(30 + (unsigned)((i * (uint64_t)p) % 53), sound, track, w);
                                wallet->updateIncomeBucket(w);
                                sum = sum * 31 + (uint64_t)wallet->countMoney();
                                sum = sum * 31 + (uint64_t)wallet->getCashPerMinute();
                        }
                        break;
                }
                case 1: {
                        // the energy stream
                        for (int p = 1; p <= 3; ++p) {
                                int objId = (int)((i * 7 + (uint64_t)p * 29) % 120 + 1);
                                PlayerT *pl = w->playerList.getNthPlayer(p);
                                if (i % 2 == 0)
                                        pl->m_energy.objectEnteringInfluence(w->object(objId), w);
                                else
                                        pl->m_energy.objectLeavingInfluence(w->object(objId), w);
                                int delta = (int)((i % 5) - 2) * 7;
                                pl->m_energy.adjustPower(delta, (i % 2) == 0, w);
                                sum = sum * 31 + (uint64_t)pl->m_energy.getProduction(w);
                                sum = sum * 31 + (uint64_t)pl->m_energy.m_energyConsumption;
                                sum = sum * 31 + (uint64_t)bits_of(pl->m_energy.getEnergySupplyRatio(w));
                        }
                        break;
                }
                case 2: {
                        // the rank stream
                        for (int p = 1; p <= 3; ++p) {
                                PlayerT *pl = w->playerList.getNthPlayer(p);
                                if (i % 16 == 15)
                                        pl->resetRank(w);
                                else
                                        pl->addSkillPoints(17 + (int)(i % 23), w);
                                sum = sum * 31 + (uint64_t)pl->m_rankLevel;
                                sum = sum * 31 + (uint64_t)pl->m_skillPoints;
                                sum = sum * 31 + (uint64_t)pl->m_sciencePurchasePoints;
                        }
                        break;
                }
                default: {
                        // the science stream
                        for (int p = 1; p <= 3; ++p) {
                                PlayerT *pl = w->playerList.getNthPlayer(p);
                                unsigned science = sciences[(i + (uint64_t)p) % 8];
                                pl->attemptToPurchaseScience(science, w);
                                if (i % 3 == 0)
                                        pl->addSciencePurchasePoints(2);
                                std::vector<unsigned> can, pot;
                                w->scienceStore.getPurchasableSciences(pl, can, pot);
                                sum = sum * 31 + (uint64_t)can.size();
                                sum = sum * 31 + (uint64_t)pot.size();
                                for (unsigned s : can)
                                        sum = sum * 31 + (uint64_t)s;
                        }
                        break;
                }
                }
                w->hooks.m_teamEvents.clear(); // (the bench host records nothing)
        }

        // the final fold: every player's block state and the objects' masks
        for (int p = 0; p < MAX_PLAYER_COUNT; ++p) {
                PlayerT *pl = w->playerList.getNthPlayer(p);
                sum = sum * 31 + (uint64_t)pl->m_money.countMoney();
                sum = sum * 31 + (uint64_t)pl->m_money.getCashPerMinute();
                sum = sum * 31 + (uint64_t)pl->m_rankLevel;
                sum = sum * 31 + (uint64_t)pl->m_skillPoints;
                sum = sum * 31 + (uint64_t)pl->m_sciencePurchasePoints;
                sum = sum * 31 + (uint64_t)pl->m_sciences.size();
                sum = sum * 31 + (uint64_t)pl->m_energy.m_energyProduction;
                sum = sum * 31 + (uint64_t)pl->m_energy.m_energyConsumption;
                sum = sum * 31 + (uint64_t)pl->m_radarCount;
        }
        for (ObjectT *obj : w->logic.m_objList)
                sum = sum * 31 + (uint64_t)obj->m_disabledMask.bits;
        return sum;
}

// Untimed corpus materialization for the players bench.
void *bench_kernel_players_build()
{
        return build_players_corpus();
}

// Untimed corpus materialization for the purse bench.
void *bench_kernel_purse_build()
{
        return build_purse_corpus();
}

// The timed body: the money/energy/ranks/sciences block over the
// promoted corpus.
uint64_t bench_kernel_purse(void *world)
{
        return run_purse((World *)world);
}

// The timed body: the players/teams block over the settled corpus.
uint64_t bench_kernel_players(void *world)
{
        return run_players((World *)world);
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
        {
                World *w = build_players_corpus();
                uint64_t sum = run_players(w);
                delete w;
                if (sum != PLAYERS_CHECKSUM)
                        return 0;
        }
        {
                World *w = build_purse_corpus();
                uint64_t sum = run_purse(w);
                delete w;
                if (sum != PURSE_CHECKSUM)
                        return 0;
        }
        return 1;
}

} // extern "C"
