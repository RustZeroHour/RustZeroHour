// SPDX-License-Identifier: GPL-3.0-or-later
//
// C++ baseline timing wrapper around the gen_mapscripts_golden.cpp
// transcription.
//
// Times the verbatim Scripts.cpp script-engine load path — the gen's
// own ParseScriptsDataChunk walk (the script lists, the groups, the
// scripts' heads, the or-chains of and-conditions with the version-4
// name-key rematches, the THEN/ELSE action lists with the heal switch,
// and every parameter read with its rewrites) over 70 repeated
// PlayerScriptsList chunks (each: three script lists — four flat
// scripts with two or-clauses of three conditions, two scripts and a
// two-script group, and one special-power script whose heal fires)
// — on the same formula-generated corpus as the Rust `mapscripts/…`
// criterion bench.
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

#define main gen_mapscripts_generator_main
#include "gen_mapscripts_golden.cpp"
#undef main

// The bench corpus repetition (70 PlayerScriptsList chunks ≈ 190 KiB,
// the scale of the other map benches).
#define BENCH_PSL 70

// The final-state checksum over the payload's observable fields (the
// same walk the Rust bench drives): the list/script/group counts,
// every script's flag bytes and delay, every condition's and action's
// type, and every parameter's type/int/real-bits/string-sum/coord/
// status-mask.
static uint64_t parm_checksum(const ParameterLoad &p)
{
        uint64_t sum = (uint64_t)(uint32_t)p.m_paramType +
                       (uint64_t)(uint32_t)p.m_int +
                       (uint64_t)f32bits(p.m_real) +
                       (uint64_t)p.m_string.getLength();
        for (unsigned char c : p.m_string.s)
                sum += c;
        sum += (uint64_t)f32bits(p.m_coordX) + (uint64_t)f32bits(p.m_coordY) +
               (uint64_t)f32bits(p.m_coordZ) + (uint64_t)p.m_objectStatus;
        return sum;
}

static uint64_t parm_list_checksum(Int n,
                                   const std::array<std::unique_ptr<ParameterLoad>, MAX_PARMS> &parms)
{
        uint64_t sum = (uint64_t)(uint32_t)n;
        for (Int i = 0; i < n; ++i) {
                if (parms[i])
                        sum += parm_checksum(*parms[i]);
                else
                        sum += 0x9999;
        }
        return sum;
}

static uint64_t script_checksum(const ScriptLoad &s)
{
        uint64_t sum = (uint64_t)s.m_scriptName.getLength() +
                       (uint64_t)s.m_comment.getLength() +
                       (uint64_t)s.m_conditionComment.getLength() +
                       (uint64_t)s.m_actionComment.getLength();
        for (const AsciiString *str :
             { &s.m_scriptName, &s.m_comment, &s.m_conditionComment, &s.m_actionComment }) {
                for (unsigned char c : str->s)
                        sum += c;
        }
        sum += (s.m_isActive ? 1 : 0) + (s.m_isOneShot ? 2 : 0) + (s.m_easy ? 4 : 0) +
               (s.m_normal ? 8 : 0) + (s.m_hard ? 16 : 0) + (s.m_isSubroutine ? 32 : 0) +
               (uint64_t)(uint32_t)s.m_delayEvaluationSeconds;
        for (const auto &or_condition : s.m_ors) {
                sum += (uint64_t)or_condition->m_ands.size();
                for (const auto &cond : or_condition->m_ands) {
                        sum += (uint64_t)(uint32_t)cond->m_conditionType;
                        sum += parm_list_checksum(cond->m_numParms, cond->m_parms);
                }
        }
        for (const auto *chain : { &s.m_actions, &s.m_falseActions }) {
                for (const auto &act : *chain) {
                        sum += (uint64_t)(uint32_t)act->m_actionType;
                        sum += parm_list_checksum(act->m_numParms, act->m_parms);
                }
        }
        return sum;
}

static uint64_t state_checksum(const std::vector<std::unique_ptr<ScriptListLoad>> &lists)
{
        uint64_t checksum = (uint64_t)lists.size();
        for (const auto &list : lists) {
                checksum += (uint64_t)list->m_scripts.size() + (uint64_t)list->m_groups.size();
                for (const auto &script : list->m_scripts)
                        checksum += script_checksum(*script);
                for (const auto &group : list->m_groups) {
                        checksum += (uint64_t)group->m_groupName.getLength();
                        for (unsigned char c : group->m_groupName.s)
                                checksum += c;
                        checksum += (group->m_isGroupActive ? 1 : 0) +
                                    (group->m_isGroupSubroutine ? 2 : 0) +
                                    (uint64_t)group->m_scripts.size();
                        for (const auto &script : group->m_scripts)
                                checksum += script_checksum(*script);
                }
        }
        return checksum;
}

// Builds the corpus: 40 PlayerScriptsList chunks, each with three
// script lists (four flat scripts with two or-clauses of three
// conditions — the rematches included —, four actions and a false
// action; two scripts plus a two-script group; one special-power
// script whose heal fires) — keep in lockstep with
// crates/rzh-formats/benches/rzh_formats.rs.
static std::vector<uint8_t> build_corpus()
{
        TocB toc;
        std::vector<uint8_t> chunks;

        for (Int c = 0; c < BENCH_PSL; ++c) {
                std::vector<uint8_t> psl;

                // list A: four flat scripts
                std::vector<uint8_t> listA;
                for (Int k = 0; k < 4; ++k) {
                        char name[32], comment[32], text[32];
                        snprintf(name, sizeof name, "Scr%d_%d", (int)c, (int)k);
                        snprintf(comment, sizeof comment, "Comment%d", (int)k);
                        Int delay = c * 4 + k;

                        // two or-clauses, each with three conditions
                        std::vector<uint8_t> ors;
                        for (Int o = 0; o < 2; ++o) {
                                std::vector<uint8_t> conds;
                                // a straight match
                                {
                                        std::vector<uint8_t> cond;
                                        put_cond_head(cond, CT_TIMER_EXPIRED,
                                                      toc.id("TIMER_EXPIRED"), 1);
                                        put_parm(cond, PT_INT, c * 10 + k, 0.0f, "");
                                        put_chunk(conds, toc, "Condition",
                                                  K_SCRIPT_CONDITION_VERSION_4, cond);
                                }
                                // a rematch by name (UNIT_HEALTH keyed FLAG)
                                {
                                        std::vector<uint8_t> cond;
                                        put_cond_head(cond, CT_UNIT_HEALTH, toc.id("FLAG"), 2);
                                        snprintf(text, sizeof text, "Unit%d_%d", (int)c, (int)k);
                                        put_parm(cond, PT_UNIT, 0, 0.0f, text);
                                        put_parm(cond, PT_COMPARISON, (k + o) % 6, 0.0f, "");
                                        put_chunk(conds, toc, "Condition",
                                                  K_SCRIPT_CONDITION_VERSION_4, cond);
                                }
                                // a count mismatch (COUNTER with 2 parms)
                                {
                                        std::vector<uint8_t> cond;
                                        put_cond_head(cond, CT_COUNTER, 0, 2);
                                        put_parm(cond, PT_COUNTER, 0, 0.0f, "Cnt");
                                        put_parm(cond, PT_INT, 5, 0.0f, "");
                                        put_chunk(conds, toc, "Condition",
                                                  K_SCRIPT_CONDITION_VERSION_4, cond);
                                }
                                put_chunk(ors, toc, "OrCondition",
                                          K_SCRIPT_OR_CONDITION_DATA_VERSION_1, conds);
                        }

                        // the THEN list: three actions
                        std::vector<uint8_t> acts;
                        {
                                std::vector<uint8_t> a;
                                put_i32(a, AT_SET_FLAG);
                                put_key_word(a, (toc.id("SET_FLAG") << 8) | 3);
                                put_i32(a, 2);
                                snprintf(text, sizeof text, "Flag%d_%d", (int)c, (int)k);
                                put_parm(a, PT_FLAG, 0, 0.0f, text);
                                put_parm(a, PT_BOOLEAN, k & 1, 0.0f, "");
                                put_chunk(acts, toc, "ScriptAction", K_SCRIPT_ACTION_VERSION_2, a);
                        }
                        {
                                // a rematch that lands on a mismatched count → NO_OP
                                std::vector<uint8_t> a;
                                put_i32(a, AT_MOVE_CAMERA_TO);
                                put_key_word(a, (toc.id("SET_COUNTER") << 8) | 3);
                                put_i32(a, 2);
                                put_parm(a, PT_COUNTER, 0, 0.0f, "Cnt");
                                put_parm(a, PT_INT, 9, 0.0f, "");
                                put_chunk(acts, toc, "ScriptAction", K_SCRIPT_ACTION_VERSION_2, a);
                        }
                        {
                                // five parms (SET_TREE_SWAY): the kindof and
                                // object-status rewrites exercised
                                std::vector<uint8_t> a;
                                put_i32(a, AT_SET_TREE_SWAY);
                                put_key_word(a, (toc.id("SET_TREE_SWAY") << 8) | 3);
                                put_i32(a, 5);
                                put_parm_coord(a, (Real)(c + k), (Real)k * 2.0f, 0.0f);
                                put_parm(a, PT_KIND_OF_PARAM, 0, 0.0f,
                                         (k & 1) ? "OBSTACLE" : "PARACHUTE");
                                put_parm(a, PT_OBJECT_STATUS, 0, 0.0f,
                                         (k & 1) ? "DESTROYED" : "NONE");
                                put_parm(a, PT_INT, c, 0.0f, "");
                                put_parm(a, PT_REAL, 0, (Real)(k % 5) * 0.5f, "");
                                put_chunk(acts, toc, "ScriptAction", K_SCRIPT_ACTION_VERSION_2, a);
                        }
                        // the ELSE list: one action
                        std::vector<uint8_t> fact;
                        {
                                std::vector<uint8_t> a;
                                put_i32(a, AT_DEBUG_STRING);
                                put_key_word(a, (toc.id("DEBUG_STRING") << 8) | 3);
                                put_i32(a, 1);
                                snprintf(text, sizeof text, "Else%d_%d", (int)c, (int)k);
                                put_parm(a, PT_TEXT_STRING, 0, 0.0f, text);
                                put_chunk(fact, toc, "ScriptActionFalse", K_SCRIPT_ACTION_VERSION_2, a);
                        }

                        std::vector<uint8_t> s;
                        put_script_head(s, name, comment, "", "", k & 1, 1, 1, 1, 1, 0, &delay);
                        put_bytes(s, ors);
                        put_bytes(s, acts);
                        put_bytes(s, fact);
                        put_chunk(listA, toc, "Script", K_SCRIPT_DATA_VERSION_2, s);
                }

                // list B: two scripts and a two-script group
                std::vector<uint8_t> listB;
                for (Int k = 0; k < 2; ++k) {
                        char name[32];
                        snprintf(name, sizeof name, "BS%d_%d", (int)c, (int)k);
                        std::vector<uint8_t> s;
                        put_script_head(s, name, "", "", "", 1, 1, 1, 1, 1, k, nullptr);
                        put_chunk(listB, toc, "Script", K_SCRIPT_DATA_VERSION_1, s);
                }
                {
                        std::vector<uint8_t> grp;
                        char gname[32];
                        snprintf(gname, sizeof gname, "Grp%d", (int)c);
                        Int sub = c & 1;
                        put_group_head(grp, gname, 1, &sub);
                        for (Int k = 0; k < 2; ++k) {
                                char name[32];
                                snprintf(name, sizeof name, "GS%d_%d", (int)c, (int)k);
                                std::vector<uint8_t> s;
                                put_script_head(s, name, "", "", "", 1, 0, 1, 1, 1, 0, nullptr);
                                put_chunk(grp, toc, "Script", K_SCRIPT_DATA_VERSION_1, s);
                        }
                        put_chunk(listB, toc, "ScriptGroup", K_SCRIPT_GROUP_DATA_VERSION_2, grp);
                }

                // list C: one special-power script whose heal fires
                std::vector<uint8_t> listC;
                {
                        std::vector<uint8_t> cond;
                        put_cond_head(cond, CT_SKIRMISH_SPECIAL_POWER_READY, 0, 1);
                        put_parm(cond, PT_SPECIAL_POWER, 0, 0.0f, "SuperWeaponNuke");
                        std::vector<uint8_t> conds;
                        put_chunk(conds, toc, "Condition", K_SCRIPT_CONDITION_VERSION_2, cond);
                        std::vector<uint8_t> orc;
                        put_bytes(orc, conds);
                        std::vector<uint8_t> orcch;
                        put_chunk(orcch, toc, "OrCondition", K_SCRIPT_OR_CONDITION_DATA_VERSION_1, orc);
                        std::vector<uint8_t> s;
                        put_script_head(s, "SP", "", "", "", 1, 1, 1, 1, 1, 0, nullptr);
                        put_bytes(s, orcch);
                        put_chunk(listC, toc, "Script", K_SCRIPT_DATA_VERSION_1, s);
                }

                put_chunk(psl, toc, "ScriptList", K_SCRIPT_LIST_DATA_VERSION_1, listA);
                put_chunk(psl, toc, "ScriptList", K_SCRIPT_LIST_DATA_VERSION_1, listB);
                put_chunk(psl, toc, "ScriptList", K_SCRIPT_LIST_DATA_VERSION_1, listC);
                put_chunk(chunks, toc, "PlayerScriptsList", K_SCRIPTS_DATA_VERSION_1, psl);
        }

        std::vector<uint8_t> file = toc_bytes(toc);
        file.insert(file.end(), chunks.begin(), chunks.end());
        return file;
}

extern "C" {

// Untimed corpus materialization.
long bench_mapscripts_materialize(uint8_t *out, size_t cap)
{
        std::vector<uint8_t> b = build_corpus();
        if (b.size() > cap)
                return -1;
        memcpy(out, b.data(), b.size());
        return (long)b.size();
}

// The logical load: the gen's own walk over the corpus (the registered
// PlayerScriptsList parser — the lists, the groups, the scripts, the
// conditions, the actions, the parameters), with the final-state
// checksum.
uint64_t bench_mapscripts_parse(const uint8_t *data, size_t len)
{
        std::vector<uint8_t> bytes(data, data + len);
        CachedFileInputStream s(bytes);
        DataChunkInput in(&s);
        PsState st;
        st.m_eng.init();  // ScriptEngine::init()'s closing loops
        g_eng = &st.m_eng;
        in.registerParser("PlayerScriptsList", AsciiString(), parse_scripts_data_chunk);
        if (!in.parse(&st))
                return 0xdeadbeefULL;
        std::array<std::unique_ptr<ScriptListLoad>, MAX_PLAYER_COUNT> lists;
        Int count = get_read_scripts(st, lists);
        std::vector<std::unique_ptr<ScriptListLoad>> owned;
        for (Int i = 0; i < count; ++i)
                owned.push_back(std::move(lists[i]));
        return state_checksum(owned);
}

} // extern "C"
