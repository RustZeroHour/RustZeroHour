// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Golden-vector driver for `rzh-formats::map_scripts` (parity
// level P0). Copyright 2025 Electronic Arts Inc.; Rust port Copyright
// 2026 the RustZeroHour contributors.
//
// Verbatim transcription of the Zero Hour script-engine LOAD PATH — the
// nested PlayerScriptsList walk the map's SidesList chunk registers
// (Core/GameEngine/Source/GameLogic/Map/SidesList.cpp) and the standalone
// .scb root registration (GameLogic.cpp / the WorldBuilder's
// ScriptDialog.cpp) both drive — over the re-transcribed map data-chunk
// container (Common/System/DataChunk.{h,cpp}, the reader half verbatim
// from gen_mapchunk_golden.cpp, the same copy gen_mapsides_golden.cpp
// carries):
//
//   - ScriptList::ParseScriptsDataChunk (Scripts.cpp): the static
//     read-lists clear (the leftover-scripts leak between loads), the
//     TScriptListReadInfo registration ("ScriptList" under the
//     PlayerScriptsList label), the bounded walk and the getReadScripts
//     hand-out — at the baseline (571a7c5);
//   - ScriptList::ParseScriptListDataChunk: the MAX_PLAYER_COUNT refusal
//     (the 17th list returns false and kills the whole walk — the only
//     false path in the tree), the per-list Script/ScriptGroup
//     registrations;
//   - ScriptGroup::ParseGroupDataChunk: the name and active byte, the
//     subroutine byte only on the exact version 2 (a version 3 group
//     reads like a version 1 — the equality, not a ceiling);
//   - Script::ParseScript (and the FromList/FromGroup wrappers): the
//     four strings, six bytes, the version-2 delay int, the
//     OrCondition/ScriptAction/ScriptActionFalse registrations, the
//     parse-failure nullptr (its addScript null-deref a documented crash
//     contract, unreachable through this walk — no parser in the tree
//     returns false but the list bound);
//   - OrCondition::ParseOrConditionDataChunk: the tail-append walk;
//   - Condition::ParseConditionDataChunk: the type word, the version-4
//     name-key rematch (the id/name mismatch repair, the by-name rescan,
//     the CONDITION_FALSE fallback), the parameter loop, the version-1
//     ParameterChangesVer2 append heal (the seven area conditions, the
//     forced three parms — the append at m_parms[m_numParms] is the
//     out-of-bounds verdict when the loaded count is 12 or negative),
//     the SKIRMISH_SPECIAL_POWER_READY heal (the SIDE "<This Player>"
//     prepend), and the template count check (CONDITION_FALSE on
//     mismatch — the loaded parms stay but become unobservable);
//   - ScriptAction::ParseAction (and the two wrappers): the type word,
//     the version-2 name-key rematch (NO_OP fallback), the parameter
//     loop, the whole heal switch (SKIRMISH_FIRE_SPECIAL_POWER_AT_MOST_
//     COST's SIDE prepend, TEAM_FOLLOW_WAYPOINTS' BOOLEAN append, the
//     SKIRMISH_BUILD_BASE_DEFENSE_FRONT flank switch, the attitude
//     re-type to AI_MOOD, the map-reveal SIDE appends, SPEECH_PLAY's
//     BOOLEAN append, the camera PERCENT/REAL pads and
//     CAMERA_LOOK_TOWARD_WAYPOINT's two shapes), and the template count
//     check (NO_OP on mismatch);
//   - Parameter::ReadParameter: the type word, the COORD3D three-real
//     shape (no int/real/string read), the int/real/string shape, the
//     OBJECT_TYPE "Fundamentalist"->"GLA" rename (the strlcat 255-char
//     truncation), the UPGRADE rename trio, the OBJECT_STATUS
//     name-to-mask heal (compareNoCase against the 45 bit names, the
//     "NONE" bit-0 quirk), and the KIND_OF_PARAM heal (the
//     compareNoCase table walk, the CRUSHER/CRUSHABLE/OVERLAPPABLE
//     legacy remaps to the OBSTACLE index, the MISSILE split whose
//     re-compare is always true at index 0 — SMALL_MISSILE the string,
//     OBSTACLE the int —, the unknown-name throw, and the empty-string
//     int-to-name normalization whose out-of-range index is the crash
//     verdict);
//   - ScriptEngine::init's template registry (the internal names and
//     parameter counts of all 109 conditions and 344 actions — six
//     conditions and one action keep the ctor defaults, the empty
//     internal name) with the closing name-key loop (every template's
//     internal name interned in enum order, conditions first — the
//     interning sequence the pkey lines pin), and the getCondition/
//     ActionTemplate out-of-range clamp to index 0 (a type word outside
//     the enum reads template 0 — DEBUG_MESSAGE_BOX / CONDITION_FALSE
//     — so an out-of-range condition type with zero parms survives the
//     count check as the raw type, and an out-of-range action type with
//     exactly one parm does too).
//
// Deliberate deviations, pinned identically on both sides:
//
// - The load path consults a template's getNumParameters() and its
//   interned internal name only; getParameterType is never called (the
//   heals construct their parameters from explicit types), so the
//   registry below carries the internal name and the declared count —
//   three actions declare a count their m_parameters[] assignments do
//   not fill (CAMERA_BW_MODE_END, UNIT_DESTROY_ALL_CONTAINED,
//   TEAM_ALL_USE_COMMANDBUTTON_ON_NAMED); the count wins, the
//   unfilled slots are indeterminate and unobservable.
// - The name-key interning starts from the template loop (the harness
//   models the generator exactly as the load path meets it: the game's
//   earlier startup interning is not observable through this walk —
//   the rematch compares interned keys, where equality is string
//   equality however the ids fell, and the pkey dump pins the
//   transcription's own sequence). A file name key whose table-of-
//   contents id is unknown interns the empty string — which is also
//   the defunct templates' key, so such a key REMATCHES to the first
//   defunct condition (OBSOLETE_SCRIPT_1); the corpora pin it.
// - The parameter arrays are fixed 12-slot arrays like the original's
//   m_parms[MAX_PARMS]; the store loops check the index and refuse
//   with the oob verdict where the original writes past the array (a
//   13th parameter, the version-1 append heal at count 12 or
//   negative). The stream has already consumed the parameter bytes at
//   that point, matching the original's read-then-store order.
// - The KIND_OF_PARAM empty-string normalization reading
//   kindofNames[m_int] out of [0, 117) — the 117 slot is the null
//   terminator, beyond it and below 0 is past the array — refuses with
//   the oob-kindof verdict; the AsciiString-from-null deref is the
//   crash contract stated, not replayed.
// - The atEndOfChunk asserts ("Unexpected data left over") are
//   debug-only; release walks on, and closeDataChunk skips the
//   leftovers — the corpora pin the skip.
// - The sides payload around the nested registration (the side dicts,
//   build lists, team list, validateSides) is the sides-list slice's,
//   transcribed in gen_mapsides_golden.cpp; the SidesList stub here
//   models only the registration tail (registerParser + parse), and
//   the scripts read-back into the sides (deleteInstance/setScriptList)
//   is dropped with it — getReadScripts' hand-out is dumped directly
//   instead, the same observable the read-back consumes.
// - The statics (s_readLists/s_numInReadList) are per-walk driver
//   state; the leftover clear at the top of ParseScriptsDataChunk is
//   transcribed (the repeated-PlayerScriptsList corpus pins the
//   last-wins clear), and ScriptList::m_curId (the editor's default
//   group-name counter) is not observable through this load path —
//   every group name comes from the file.
// - The container transcription (the reader half) is otherwise verbatim
//   from gen_mapchunk_golden.cpp — see that file's header for its own
//   deviation notes.
//
// Output grammar (one command per line, hex lowercase, '-' = empty):
//
//   file <id> <hex>                    the corpus bytes
//   valid <id> <t|f>                   isValidFileType() after construction
//   walk <id> <t|f>                    the parse() verdict
//   perr <id> corrupt                  the walk itself refused
//   perr <id> kindof                   ReadParameter's unknown-kindof throw
//   perr <id> oob-parms                a parameter store past m_parms[12]
//   perr <id> oob-heal                 the version-1 append heal's bad index
//   perr <id> oob-kindof               the empty-string normalization's
//                                      kindofNames[] index out of range
//   pnone <id>                         no PlayerScriptsList chunk fired
//   pn <id> <nlists>                   getReadScripts' count
//   plist <id> <i> <nscripts> <ngroups>  list i's counts
//   pscr <id> <path> <name> <comment> <condcomment> <actcomment> <active>
//        <oneshot> <easy> <normal> <hard> <sub> <delay>
//                                      a script (path l<i>.<s> or l<i>.g<g>.<s>)
//   pgrp <id> <li> <gi> <name> <active> <sub> <nscripts>  group gi of list li
//   pcond <id> <path> <type> <nparms>  an and-condition (path ... .o<o>.c<c>)
//   pact <id> <path> <arm> <type> <nparms>  an action (path ... .a<k>/.f<k>,
//                                      arm t = the THEN list, f = the ELSE)
//   pparm <id> <path> <pi> <type> <int> <real-bits> <string>
//        [ <x> <y> <z> ] [ <status> ]  parameter pi (the COORD3D words, the
//                                      OBJECT_STATUS mask, as %08x)
//   pkey <id> <ordinal> <name>         the final name-key table, ordinal order
//   pstate <id> <tell> <depth> <eof>   post-walk stream state
//
// Usage: gen_mapscripts_golden <output-file>
//
// Temporary differential-testing infrastructure: removed at cut-over (M5).
//
// This program is free software; you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the
// Free Software Foundation, either version 3 of the License, or (at your
// option) any later version.

#include <climits>
#include <cmath>

#include <algorithm>
#include <array>
#include <memory>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>
#include <vector>

// The transcription body lives in an anonymous namespace: the §6 bench
// wrappers (tools/differential/bench_baseline.sh) link several generator
// translation units into one binary, and their identically-named
// self-contained transcriptions would otherwise collide at link time.
// main() stays at global scope so the generator still links standalone.
namespace {
// ---------------------------------------------------------------------------
// the base types (BaseTypes.h / CommonType.h, pinned to the game build)
// ---------------------------------------------------------------------------

typedef std::int32_t Int;                 // 32-bit build: Int == int
typedef std::uint32_t UnsignedInt;
typedef std::int16_t Short;
typedef std::uint16_t UnsignedShort;      // DataChunkVersionType
typedef std::uint8_t Byte;
typedef bool Bool;
typedef float Real;
typedef std::uint16_t WideChar;           // unichar on Windows: 16-bit
typedef std::uint32_t NameKeyType;        // sequential id, see below

// The readDict corrupt-type marker: the mapchunk generator throws a
// bare int; this generator's carries the type byte so the golden can
// pin which impossible type refused the load.
struct DictTypeCorrupt {
        UnsignedInt type;
};

// ---------------------------------------------------------------------------
// AsciiString / UnicodeString stand-ins (C-string semantics)
// ---------------------------------------------------------------------------

// The original's AsciiString is a NUL-terminated byte buffer: every
// observable (getLength, ==, the wire writes) sees the bytes up to the
// first NUL only. The stand-in stores exactly those bytes.
struct AsciiString {
        std::string s;

        AsciiString() = default;
        AsciiString(const char *p) : s(p ? p : "") {}
        explicit AsciiString(const std::string &str) : s(str) {}

        // "getBufferForRead(len) + read + str[len] = 0" construction:
        // the bytes are stored verbatim, the *visible* content stops at
        // the first NUL inside them.
        static AsciiString from_raw(const char *bytes, Int len)
        {
                AsciiString out;
                out.s.assign(bytes, bytes + len);
                size_t nul = out.s.find('\0');
                if (nul != std::string::npos)
                        out.s.resize(nul);
                return out;
        }

        Int getLength() const { return (Int)s.size(); }
        const char *str() const { return s.c_str(); }
        bool operator==(const AsciiString &o) const { return s == o.s; }
        bool operator!=(const AsciiString &o) const { return s != o.s; }
};

// The original's UnicodeString is a NUL-terminated WideChar buffer; the
// stand-in stores the units up to the first zero unit.
struct UnicodeString {
        std::vector<WideChar> u;

        static UnicodeString from_raw(const WideChar *units, Int len)
        {
                UnicodeString out;
                out.u.assign(units, units + len);
                for (size_t i = 0; i < out.u.size(); ++i) {
                        if (out.u[i] == 0) {
                                out.u.resize(i);
                                break;
                        }
                }
                return out;
        }

        Int getLength() const { return (Int)u.size(); }
        const WideChar *str() const { return u.data(); }
};

// ---------------------------------------------------------------------------
// the name-key generator (NameKeyGenerator.cpp, load-path surface)
// ---------------------------------------------------------------------------

// The original hashes names into 45007 sockets and allocates sequential
// ids in first-seen order (createNameKey: m_key = m_nextID++). The socket
// table only affects lookup cost, never the ids; the arena below keeps
// the first-seen order and the exact-match (case-sensitive) lookup, which
// is everything the data chunk layer observes.
class NameKeyGenerator {
public:
        NameKeyGenerator() : m_nextID(1) {}

        AsciiString keyToName(NameKeyType key) const
        {
                for (size_t i = 0; i < m_names.size(); ++i)
                        if (key == m_names[i].second)
                                return m_names[i].first;
                return AsciiString();
        }

        NameKeyType nameToKey(const AsciiString &name)
        {
                for (size_t i = 0; i < m_names.size(); ++i)
                        if (name == m_names[i].first)
                                return m_names[i].second;

                NameKeyType key = m_nextID++;
                m_names.push_back(std::make_pair(name, key));
                return key;
        }

private:
        std::vector<std::pair<AsciiString, NameKeyType>> m_names;
        NameKeyType m_nextID;

public:
        // --- transcription-only introspection (the Rust port exposes the
        //     same views for the golden replay) ---
        const std::vector<std::pair<AsciiString, NameKeyType>> &names() const
        {
                return m_names;
        }
};

// ---------------------------------------------------------------------------
// the Dict (Dict.{h,cpp}, load-path half)
// ---------------------------------------------------------------------------

struct Dict {
        enum DataType {
                DICT_NONE = -1,   // returned by getType/getNthType for "invalid"
                DICT_BOOL = 0,    // we rely on this being zero; don't change
                DICT_INT,
                DICT_REAL,
                DICT_ASCIISTRING,
                DICT_UNICODESTRING,
                DICT_TYPE_COUNT
        };

        // The encoded pair key: (name key << 8) | type. The original packs
        // this into one word and sorts on the name-key half alone.
        typedef std::uint32_t DictPairKeyType;

        static DictPairKeyType createKey(NameKeyType keyVal, DataType nt)
        {
                return (DictPairKeyType)(((UnsignedInt)keyVal << 8) | (UnsignedInt)nt);
        }
        static DataType getTypeFromKey(DictPairKeyType nk)
        {
                return (DataType)((UnsignedInt)nk & 0xff);
        }
        static NameKeyType getNameFromKey(DictPairKeyType nk)
        {
                return (NameKeyType)((UnsignedInt)nk >> 8);
        }

        struct DictPair {
                DictPairKeyType m_key = 0;   // zero alloc: type BOOL, key INVALID
                // The original unions all five value types into one
                // pointer-sized word; the variant keeps the same
                // "one value per pair" observable.
                Bool b = false;
                Int i = 0;
                Real r = 0.0f;
                AsciiString as;
                UnicodeString us;

                DataType getType() const { return getTypeFromKey(m_key); }
                NameKeyType getName() const { return getNameFromKey(m_key); }
        };

        std::vector<DictPair> m_pairs;   // m_data->peek()[n], sorted by name key

        Int getPairCount() const { return (Int)m_pairs.size(); }

        NameKeyType getNthKey(Int n) const
        {
                if (n < 0 || n >= (Int)m_pairs.size())
                        return 0; // NAMEKEY_INVALID
                return m_pairs[n].getName();
        }
        DataType getNthType(Int n) const
        {
                if (n < 0 || n >= (Int)m_pairs.size())
                        return DICT_NONE;
                return m_pairs[n].getType();
        }
        Bool getNthBool(Int n) const
        {
                if (n >= 0 && n < (Int)m_pairs.size() && m_pairs[n].getType() == DICT_BOOL)
                        return m_pairs[n].b;
                return false;
        }
        Int getNthInt(Int n) const
        {
                if (n >= 0 && n < (Int)m_pairs.size() && m_pairs[n].getType() == DICT_INT)
                        return m_pairs[n].i;
                return 0;
        }
        Real getNthReal(Int n) const
        {
                if (n >= 0 && n < (Int)m_pairs.size() && m_pairs[n].getType() == DICT_REAL)
                        return m_pairs[n].r;
                return 0.0f;
        }
        AsciiString getNthAsciiString(Int n) const
        {
                if (n >= 0 && n < (Int)m_pairs.size() && m_pairs[n].getType() == DICT_ASCIISTRING)
                        return m_pairs[n].as;
                return AsciiString();
        }
        UnicodeString getNthUnicodeString(Int n) const
        {
                if (n >= 0 && n < (Int)m_pairs.size() && m_pairs[n].getType() == DICT_UNICODESTRING)
                        return m_pairs[n].us;
                return UnicodeString();
        }

        // Dict.h's getType(key) — the well-known-key query the map-object
        // derivations drive (completed into the container copy for this
        // generator; the mapchunk corpus never reads it).
        DataType getType(NameKeyType key) const
        {
                const DictPair *pair = findPairByKey(key);
                if (pair)
                        return pair->getType();
                return DICT_NONE;
        }

        // Dict.cpp's getAsciiString(key)/getBool(key) — the typed
        // getters the sides validation drives (completed into the
        // container copy for this generator; the mapchunk corpus never
        // reads them).
        AsciiString getAsciiStringByKey(NameKeyType key) const
        {
                const DictPair *pair = findPairByKey(key);
                if (pair && pair->getType() == DICT_ASCIISTRING)
                        return pair->as;
                return AsciiString();
        }
        Bool getBoolByKey(NameKeyType key) const
        {
                const DictPair *pair = findPairByKey(key);
                if (pair && pair->getType() == DICT_BOOL)
                        return pair->b;
                return false;
        }

        DictPair *findPairByKey(NameKeyType key) const
        {
                // (The debug-only "invalid namekey!"/"namekey too large!"
                // asserts are release no-ops here.)
                const DictPair *base = m_pairs.data();
                Int minIdx = 0;
                Int maxIdx = (Int)m_pairs.size();
                while (minIdx < maxIdx) {
                        Int midIdx = (((minIdx + maxIdx) - 1) >> 1);
                        const DictPair *mid = base + midIdx;
                        NameKeyType midKey = mid->getName();
                        if (key > midKey)
                                minIdx = midIdx + 1;
                        else if (key < midKey)
                                maxIdx = midIdx;
                        else
                                return const_cast<DictPair *>(mid);
                }
                return nullptr;
        }

        DictPair *setPrep(NameKeyType key, DataType type)
        {
                DictPair *pair = findPairByKey(key);
                if (!pair) {
                        m_pairs.push_back(DictPair());   // born zero: BOOL/INVALID
                        pair = &m_pairs.back();
                }
                pair->m_key = createKey(key, type);
                return pair;
        }

        void sortPairs()
        {
                // yer basic shellsort (verbatim; the comparison is on the
                // name key alone, so the type never participates).
                Int n = (Int)m_pairs.size();
                for (Int gap = n >> 1; gap > 0; gap >>= 1) {
                        for (Int i = gap; i < n; i++) {
                                for (Int j = i - gap; j >= 0; j -= gap) {
                                        DictPair &a = m_pairs[j];
                                        DictPair &b = m_pairs[j + gap];
                                        if (a.getName() > b.getName()) {
                                                DictPair tmp = a;
                                                a = b;
                                                b = tmp;
                                        } else {
                                                break;
                                        }
                                }
                        }
                }
        }

        void setBool(NameKeyType key, Bool value)
        {
                DictPair *pair = setPrep(key, DICT_BOOL);
                pair->b = value;
                sortPairs();
        }
        void setInt(NameKeyType key, Int value)
        {
                DictPair *pair = setPrep(key, DICT_INT);
                pair->i = value;
                sortPairs();
        }
        void setReal(NameKeyType key, Real value)
        {
                DictPair *pair = setPrep(key, DICT_REAL);
                pair->r = value;
                sortPairs();
        }
        void setAsciiString(NameKeyType key, const AsciiString &value)
        {
                DictPair *pair = setPrep(key, DICT_ASCIISTRING);
                pair->as = value;
                sortPairs();
        }
        void setUnicodeString(NameKeyType key, const UnicodeString &value)
        {
                DictPair *pair = setPrep(key, DICT_UNICODESTRING);
                pair->us = value;
                sortPairs();
        }
};

// ---------------------------------------------------------------------------
// CachedFileInputStream (MapReaderWriterInfo.h / DataChunk.cpp)
// ---------------------------------------------------------------------------

// The open()-time RefPack decompression branch is replaced by direct
// construction from the (already decompressed) bytes; the read/tell/
// absoluteSeek/eof semantics below are the original's.
class CachedFileInputStream {
public:
        explicit CachedFileInputStream(const std::vector<uint8_t> &bytes)
                : m_buffer(bytes.empty() ? nullptr : bytes.data())
                , m_size((Int)bytes.size())
                , m_pos(0)
        {
        }

        Int read(void *pData, Int numBytes)
        {
                if (m_buffer) {
                        if ((numBytes + m_pos) > m_size) {
                                numBytes = m_size - m_pos;
                        }
                        if (numBytes) {
                                memcpy(pData, m_buffer + m_pos, numBytes);
                                m_pos += numBytes;
                        }
                        return numBytes;
                }
                return 0;
        }

        UnsignedInt tell() { return (UnsignedInt)m_pos; }

        Bool absoluteSeek(UnsignedInt pos)
        {
                if (pos > (UnsignedInt)m_size) {
                        pos = (UnsignedInt)m_size;
                }
                m_pos = (Int)pos;
                return true;
        }

        Bool eof() { return m_size == m_pos; }

        void rewind() { m_pos = 0; }

private:
        const uint8_t *m_buffer;
        Int m_size;
        Int m_pos;
};

// The output side of the same plumbing: a byte sink whose write() returns
// the byte count (the original's OutputStream).
struct ByteSink {
        std::vector<uint8_t> bytes;
        Int write(const void *pData, Int numBytes)
        {
                const uint8_t *p = (const uint8_t *)pData;
                bytes.insert(bytes.end(), p, p + numBytes);
                return numBytes;
        }
};
// DataChunk.h: the per-chunk version word.
typedef UnsignedShort DataChunkVersionTypeT;

// ---------------------------------------------------------------------------
// the table of contents (DataChunk.h)
// ---------------------------------------------------------------------------

struct Mapping {
        Mapping *next = nullptr;
        AsciiString name;
        UnsignedInt id = 0;
};

class DataChunkTableOfContents {
public:
        DataChunkTableOfContents()
                : m_list(nullptr)
                , m_listLength(0)
                , m_nextID(1)
                , m_headerOpened(false)
        {
        }

        ~DataChunkTableOfContents()
        {
                Mapping *m, *next;
                for (m = m_list; m; m = next) {
                        next = m->next;
                        delete m;
                }
        }

        Mapping *findMapping(const AsciiString &name)
        {
                Mapping *m;
                for (m = m_list; m; m = m->next)
                        if (name == m->name)
                                return m;
                return nullptr;
        }

        // convert name to integer identifier
        UnsignedInt getID(const AsciiString &name)
        {
                Mapping *m = findMapping(name);
                if (m)
                        return m->id;
                // (the original DEBUG_CRASHes in debug builds; release
                // falls through with 0)
                return 0;
        }

        // convert integer identifier to name
        AsciiString getName(UnsignedInt id)
        {
                Mapping *m;
                for (m = m_list; m; m = m->next)
                        if (m->id == id)
                                return m->name;
                // (DEBUG_CRASH in debug builds; release returns empty)
                return AsciiString();
        }

        // create new ID for given name or return existing mapping
        UnsignedInt allocateID(const AsciiString &name)
        {
                Mapping *m = findMapping(name);
                if (m) {
                        return m->id;
                } else {
                        m = new Mapping;
                        m->id = m_nextID++;
                        m->name = name;
                        // prepend to list
                        m->next = m_list;
                        m_list = m;
                        m_listLength++;
                        return m->id;
                }
        }

        Bool isOpenedForRead() { return m_headerOpened; }

        // output the table of contents to a binary stream
        void write(ByteSink &s)
        {
                Mapping *m;
                unsigned char len;

                Byte tag[4] = { 'C', 'k', 'M', 'p' };   // Chunky height map. jba.
                s.write(tag, sizeof(tag));

                // output number of elements in the table
                s.write(&this->m_listLength, sizeof(Int));

                // output symbol table
                for (m = this->m_list; m; m = m->next) {
                        len = (unsigned char)m->name.getLength();
                        s.write(&len, sizeof(unsigned char));
                        s.write(m->name.str(), len);
                        s.write(&m->id, sizeof(UnsignedInt));
                }
        }

        // read the table of contents from a binary stream
        // TODO: Should this reset the symbol table?
        // Append symbols to table
        void read(CachedFileInputStream &s)
        {
                Int count = 0, i;   // (zero-filled: see the header note)
                UnsignedInt maxID = 0;
                unsigned char len = 0;
                Mapping *m;

                Byte tag[4] = { 'x', 'x', 'x', 'x' };   // Chunky height map. jba.
                s.read(tag, sizeof(tag));
                if (tag[0] != 'C' || tag[1] != 'k' || tag[2] != 'M' || tag[3] != 'p') {
                        return;  // Don't throw, may happen with legacy files.
                }

                // get number of symbols in table
                s.read((char *)&count, sizeof(Int));

                for (i = 0; i < count; i++) {
                        // allocate new id mapping
                        m = new Mapping;

                        // read string length
                        s.read((char *)&len, sizeof(unsigned char));

                        // allocate and read in string
                        if (len > 0) {
                                char strbuf[256];
                                s.read(strbuf, len);
                                m->name = AsciiString::from_raw(strbuf, len);
                        }

                        // read id
                        UnsignedInt id = 0;
                        s.read((char *)&id, sizeof(UnsignedInt));
                        m->id = id;

                        // prepend to list
                        m->next = this->m_list;
                        this->m_list = m;

                        this->m_listLength++;

                        // track max ID used
                        if (m->id > maxID)
                                maxID = m->id;
                }
                m_headerOpened = count > 0 && !s.eof();

                // adjust next ID so no ID's are reused
                this->m_nextID = std::max(this->m_nextID, maxID + 1);
        }

        // --- transcription-only introspection (the Rust port exposes the
        //     same views for the golden replay) ---
        Int listLength() const { return m_listLength; }
        Mapping *head() const { return m_list; }

private:
        Mapping *m_list;         // @TODO: This should be a hash table
        Int m_listLength;
        UnsignedInt m_nextID;    // simple ID allocator
        Bool m_headerOpened;
};
// ---------------------------------------------------------------------------
// DataChunkInput (DataChunk.h / DataChunk.cpp)
// ---------------------------------------------------------------------------

class DataChunkInput;

struct InputChunk {
        InputChunk *next = nullptr;
        UnsignedInt id = 0;                 // chunk symbol type from table of contents
        DataChunkVersionTypeT version = 0;  // version of data
        Int chunkStart = 0;                 // tell position of the start of chunk data
        Int dataSize = 0;                   // total data size of chunk
        Int dataLeft = 0;                   // data left to read in this chunk
};

struct DataChunkInfo {
        AsciiString label;
        AsciiString parentLabel;
        DataChunkVersionTypeT version = 0;
        Int dataSize = 0;
};

typedef Bool (*DataChunkParserPtr)(DataChunkInput &file, DataChunkInfo *info, void *userData);

class UserParser {
public:
        UserParser *next = nullptr;
        DataChunkParserPtr parser = nullptr; // the user parsing function
        AsciiString label;                   // the data chunk label to match
        AsciiString parentLabel;             // the parent chunk's label (the scope)
        void *userData = nullptr;            // user data hook (write-only: parse
                                             // passes its own userData)
};

class DataChunkInput {
        enum { CHUNK_HEADER_BYTES = 4 }; // 2 shorts in chunk file header.

public:
        DataChunkInput(CachedFileInputStream *pStream)
                : m_currentObject(nullptr)
                , m_userData(nullptr)
                , m_file(pStream)
                , m_fileposOfFirstChunk(0)
                , m_parserList(nullptr)
                , m_chunkStack(nullptr)
        {
                // read table of contents
                m_contents.read(*m_file);

                // store location of first data chunk
                m_fileposOfFirstChunk = (Int)m_file->tell();
        }

        ~DataChunkInput()
        {
                clearChunkStack();

                UserParser *p, *next;
                for (p = m_parserList; p; p = next) {
                        next = p->next;
                        delete p;
                }
        }

        // register a user parsing function for a given DataChunk label
        void registerParser(const AsciiString &label, const AsciiString &parentLabel,
                            DataChunkParserPtr parser, void *userData = nullptr)
        {
                UserParser *p = new UserParser;

                p->label = label;
                p->parentLabel = parentLabel;
                p->parser = parser;
                p->userData = userData;

                // prepend parser to parser list
                p->next = m_parserList;
                m_parserList = p;
        }

        // parse the chunk stream using registered parsers
        // it is assumed that the file position is at the start of a data chunk
        // (it can be inside a parent chunk) when parse is called.
        Bool parse(void *userData)
        {
                AsciiString label;
                AsciiString parentLabel;
                DataChunkVersionTypeT ver = 0;
                UserParser *parser;
                Bool scopeOK;
                DataChunkInfo info;

                // If the header wasn't a chunk table of contents, we can't parse.
                if (!m_contents.isOpenedForRead()) {
                        return false;
                }

                // if we are inside a data chunk right now, get its name
                if (m_chunkStack)
                        parentLabel = m_contents.getName(m_chunkStack->id);

                while (atEndOfFile() == false) {
                        if (m_chunkStack) {
                                // If we are parsing chunks in a chunk, check current length.
                                if (m_chunkStack->dataLeft < CHUNK_HEADER_BYTES) {
                                        // DEBUG_ASSERTCRASH(m_chunkStack->dataLeft==0,
                                        //     ("Unexpected extra data in chunk."));
                                        break;
                                }
                        }
                        // open the chunk
                        label = openDataChunk(&ver);
                        if (atEndOfFile()) {
                                // FILE * returns eof after you read past end of
                                // file, so check.
                                break;
                        }

                        // find a registered parser for this chunk
                        for (parser = m_parserList; parser; parser = parser->next) {
                                // chunk labels must match
                                if (parser->label == label) {
                                        // make sure parent name (scope) also matches
                                        scopeOK = true;

                                        if (parentLabel != parser->parentLabel)
                                                scopeOK = false;

                                        if (scopeOK) {
                                                // file out the chunk info and call the
                                                // user parser
                                                info.label = label;
                                                info.parentLabel = parentLabel;
                                                info.version = ver;
                                                info.dataSize = (Int)getChunkDataSize();

                                                if (parser->parser(*this, &info, userData) == false)
                                                        return false;
                                                break;
                                        }
                                }
                        }

                        // close chunk (and skip to end if need be)
                        closeDataChunk();
                }

                return true;
        }

        // clear the stack
        void clearChunkStack()
        {
                InputChunk *c, *next;
                for (c = m_chunkStack; c; c = next) {
                        next = c->next;
                        delete c;
                }
                m_chunkStack = nullptr;
        }

        // reset the stream to just-opened state - ready to parse the first chunk
        void reset()
        {
                clearChunkStack();
                m_file->absoluteSeek((UnsignedInt)m_fileposOfFirstChunk);
        }

        // Checks if the file has our initial tag word.
        Bool isValidFileType() { return m_contents.isOpenedForRead(); }

        AsciiString openDataChunk(DataChunkVersionTypeT *ver)
        {
                // allocate a new chunk and place it on top of the chunk stack
                InputChunk *c = new InputChunk;
                c->id = 0;
                c->version = 0;
                c->dataSize = 0;
                // read the chunk ID
                m_file->read((char *)&c->id, sizeof(UnsignedInt));
                decrementDataLeft(sizeof(UnsignedInt));

                // read the chunk version number
                m_file->read((char *)&c->version, sizeof(DataChunkVersionTypeT));
                decrementDataLeft(sizeof(DataChunkVersionTypeT));

                // read the chunk data size
                m_file->read((char *)&c->dataSize, sizeof(Int));
                decrementDataLeft(sizeof(Int));

                // all of the data remains to be read
                c->dataLeft = c->dataSize;
                c->chunkStart = (Int)m_file->tell();

                *ver = c->version;

                c->next = m_chunkStack;
                m_chunkStack = c;
                if (this->atEndOfFile()) {
                        return AsciiString();
                }
                return m_contents.getName(c->id);
        }

        // close chunk and move to start of next chunk
        void closeDataChunk()
        {
                if (m_chunkStack == nullptr) {
                        // TODO: Throw exception
                        return;
                }

                if (m_chunkStack->dataLeft > 0) {
                        // skip past the remainder of this chunk
                        m_file->absoluteSeek(m_file->tell() + (UnsignedInt)m_chunkStack->dataLeft);
                        decrementDataLeft(m_chunkStack->dataLeft);
                }

                // pop the chunk off the stack
                InputChunk *c = m_chunkStack;
                m_chunkStack = m_chunkStack->next;
                delete c;
        }

        // return label of current data chunk
        AsciiString getChunkLabel()
        {
                if (m_chunkStack == nullptr) {
                        // TODO: Throw exception
                        // DEBUG_CRASH(("Bad."));
                        return AsciiString();
                }
                return m_contents.getName(m_chunkStack->id);
        }

        // return version of current data chunk
        DataChunkVersionTypeT getChunkVersion()
        {
                if (m_chunkStack == nullptr) {
                        // DEBUG_CRASH(("Bad."));
                        return 0;
                }
                return m_chunkStack->version;
        }

        // return size of data stored in this chunk
        UnsignedInt getChunkDataSize()
        {
                if (m_chunkStack == nullptr) {
                        // DEBUG_CRASH(("Bad."));
                        return 0;
                }
                return (UnsignedInt)m_chunkStack->dataSize;
        }

        // return size of data left to read in this chunk
        UnsignedInt getChunkDataSizeLeft()
        {
                if (m_chunkStack == nullptr) {
                        // DEBUG_CRASH(("Bad."));
                        return 0;
                }
                return (UnsignedInt)m_chunkStack->dataLeft;
        }

        Bool atEndOfFile() { return (m_file->eof()) ? true : false; }

        Bool atEndOfChunk()
        {
                if (m_chunkStack) {
                        if (m_chunkStack->dataLeft <= 0)
                                return true;
                        return false;
                }
                return true;
        }

        Real readReal()
        {
                Real r = 0;   // (zero-filled on short reads: see the header note)
                // DEBUG_ASSERTCRASH(m_chunkStack->dataLeft>=sizeof(Real),
                //     ("Read past end of chunk."));
                m_file->read((char *)&r, sizeof(Real));
                decrementDataLeft(sizeof(Real));
                return r;
        }

        Int readInt()
        {
                Int i = 0;
                // DEBUG_ASSERTCRASH(m_chunkStack->dataLeft>=sizeof(Int),
                //     ("Read past end of chunk."));
                m_file->read((char *)&i, sizeof(Int));
                decrementDataLeft(sizeof(Int));
                return i;
        }

        Byte readByte()
        {
                Byte b = 0;
                // DEBUG_ASSERTCRASH(m_chunkStack->dataLeft>=sizeof(Byte),
                //     ("Read past end of chunk."));
                m_file->read((char *)&b, sizeof(Byte));
                decrementDataLeft(sizeof(Byte));
                return b;
        }

        void readArrayOfBytes(char *ptr, Int len)
        {
                memset(ptr, 0, (size_t)len);   // (zero-filled: short reads leave
                                               //  the tail zero, not indeterminate)
                // DEBUG_ASSERTCRASH(m_chunkStack->dataLeft>=len,
                //     ("Read past end of chunk."));
                m_file->read(ptr, len);
                decrementDataLeft(len);
        }

        NameKeyType readNameKey(NameKeyGenerator &keys)
        {
                Int keyAndType = readInt();
                // (the debug-only DICT_ASCIISTRING check is a no-op here)
                keyAndType >>= 8;

                AsciiString kname = m_contents.getName((UnsignedInt)keyAndType);
                NameKeyType k = keys.nameToKey(kname);
                return k;
        }

        Dict readDict(NameKeyGenerator &keys)
        {
                UnsignedShort len = 0;
                // DEBUG_ASSERTCRASH(m_chunkStack->dataLeft>=sizeof(UnsignedShort),
                //     ("Read past end of chunk."));
                m_file->read(&len, sizeof(UnsignedShort));
                decrementDataLeft(sizeof(UnsignedShort));
                // DEBUG_ASSERTCRASH(m_chunkStack->dataLeft>=len,
                //     ("Read past end of chunk."));

                Dict d;

                for (int i = 0; i < (int)len; i++) {
                        Int keyAndType = readInt();
                        Dict::DataType t = (Dict::DataType)(keyAndType & 0xff);
                        keyAndType >>= 8;

                        AsciiString kname = m_contents.getName((UnsignedInt)keyAndType);
                        NameKeyType k = keys.nameToKey(kname);

                        switch (t) {
                                case Dict::DICT_BOOL:
                                        d.setBool(k, readByte() ? true : false);
                                        break;
                                case Dict::DICT_INT:
                                        d.setInt(k, readInt());
                                        break;
                                case Dict::DICT_REAL:
                                        d.setReal(k, readReal());
                                        break;
                                case Dict::DICT_ASCIISTRING:
                                        d.setAsciiString(k, readAsciiString());
                                        break;
                                case Dict::DICT_UNICODESTRING:
                                        d.setUnicodeString(k, readUnicodeString());
                                        break;
                                default:
                                        throw DictTypeCorrupt{ (UnsignedInt)t };   // ERROR_CORRUPT_FILE_FORMAT
                                break;
                        }
                }

                return d;
        }

        AsciiString readAsciiString()
        {
                UnsignedShort len = 0;
                // DEBUG_ASSERTCRASH(m_chunkStack->dataLeft>=sizeof(UnsignedShort),
                //     ("Read past end of chunk."));
                m_file->read(&len, sizeof(UnsignedShort));
                decrementDataLeft(sizeof(UnsignedShort));
                // DEBUG_ASSERTCRASH(m_chunkStack->dataLeft>=len,
                //     ("Read past end of chunk."));
                char buf[65536 + 8];
                if (len > 0) {
                        // zero only the span a short read can leave dirty
                        // (the original leaves it indeterminate; the full
                        // 64 KiB memset here was a transcription artifact
                        // that cost 800us per parse)
                        memset(buf, 0, (size_t)len);
                        m_file->read(buf, len);
                        decrementDataLeft(len);
                        // add null delimiter to string.  Note that
                        // getBufferForRead allocates space for terminating null.
                        buf[len] = '\000';
                }
                return AsciiString::from_raw(buf, len);
        }

        UnicodeString readUnicodeString()
        {
                UnsignedShort len = 0;
                // DEBUG_ASSERTCRASH(m_chunkStack->dataLeft>=sizeof(UnsignedShort),
                //     ("Read past end of chunk."));
                m_file->read(&len, sizeof(UnsignedShort));
                decrementDataLeft(sizeof(UnsignedShort));
                // DEBUG_ASSERTCRASH(m_chunkStack->dataLeft>=len,
                //     ("Read past end of chunk."));
                WideChar buf[65536 + 8];
                if (len > 0) {
                        memset(buf, 0, (size_t)len * sizeof(WideChar));
                        m_file->read((char *)buf, len * sizeof(WideChar));
                        decrementDataLeft(len * sizeof(WideChar));
                        // add null delimiter to string.  Note that
                        // getBufferForRead allocates space for terminating null.
                        buf[len] = '\000';
                }
                return UnicodeString::from_raw(buf, len);
        }

        // --- transcription-only introspection (the Rust port exposes the
        //     same views for the golden replay) ---
        DataChunkTableOfContents &contents() { return m_contents; }
        Int firstChunkPos() const { return m_fileposOfFirstChunk; }
        UnsignedInt tell() { return m_file->tell(); }
        Int depth() const
        {
                Int n = 0;
                for (InputChunk *c = m_chunkStack; c; c = c->next)
                        n++;
                return n;
        }
        // the signed view of the top chunk's dataLeft (the unsigned view
        // is getChunkDataSizeLeft; the negative values are the point)
        Int dataLeftTop() const { return m_chunkStack ? m_chunkStack->dataLeft : 0; }

        void *m_currentObject;   // user parse routines can use this ...
        void *m_userData;        // user data hook

private:
        // update data left in chunk(s)
        // since data read from a chunk is also read from all parent chunks,
        // traverse the chunk stack and decrement the data left for each
        void decrementDataLeft(Int size)
        {
                InputChunk *c;
                c = m_chunkStack;
                while (c) {
                        c->dataLeft -= size;
                        c = c->next;
                }
                // The sizes of the parent chunks on the stack are adjusted in
                // closeDataChunk.
        }

        CachedFileInputStream *m_file;             // input file stream
        DataChunkTableOfContents m_contents;       // table of contents of chunk types
        Int m_fileposOfFirstChunk;                 // seek position of first data chunk
        UserParser *m_parserList;                  // list of all registered parsers
        InputChunk *m_chunkStack;                  // current stack of open data chunks
};

// The template registry, generated from ScriptEngine::init()'s
// assignments (ScriptEngine.cpp) by the slice's extraction script —
// the same JSON feeds the Rust port's tables, so the two cannot
// drift. Entries with the empty internal name are the enum values
// init() never assigns (their templates keep the Template ctor
// defaults: internalName "", m_numParameters 0 — the name-key loop
// interns the empty string once for all of them).
struct TplLoad { const char *internal; Int num; };
static const TplLoad COND_TEMPLATES[109] = {
	{ "CONDITION_FALSE", 0 },
	{ "COUNTER", 3 },
	{ "FLAG", 2 },
	{ "CONDITION_TRUE", 0 },
	{ "TIMER_EXPIRED", 1 },
	{ "PLAYER_ALL_DESTROYED", 1 },
	{ "PLAYER_ALL_BUILDFACILITIES_DESTROYED", 1 },
	{ "TEAM_INSIDE_AREA_PARTIALLY", 3 },
	{ "TEAM_DESTROYED", 1 },
	{ "CAMERA_MOVEMENT_FINISHED", 0 },
	{ "TEAM_HAS_UNITS", 1 },
	{ "TEAM_STATE_IS", 2 },
	{ "TEAM_STATE_IS_NOT", 2 },
	{ "NAMED_INSIDE_AREA", 2 },
	{ "NAMED_OUTSIDE_AREA", 2 },
	{ "NAMED_DESTROYED", 1 },
	{ "NAMED_NOT_DESTROYED", 1 },
	{ "TEAM_INSIDE_AREA_ENTIRELY", 3 },
	{ "TEAM_OUTSIDE_AREA_ENTIRELY", 3 },
	{ "NAMED_ATTACKED_BY_OBJECTTYPE", 2 },
	{ "TEAM_ATTACKED_BY_OBJECTTYPE", 2 },
	{ "NAMED_ATTACKED_BY_PLAYER", 2 },
	{ "TEAM_ATTACKED_BY_PLAYER", 2 },
	{ "BUILT_BY_PLAYER", 2 },
	{ "NAMED_CREATED", 1 },
	{ "TEAM_CREATED", 1 },
	{ "PLAYER_HAS_CREDITS", 3 },
	{ "NAMED_DISCOVERED", 2 },
	{ "TEAM_DISCOVERED", 2 },
	{ "MISSION_ATTEMPTS", 3 },
	{ "NAMED_OWNED_BY_PLAYER", 2 },
	{ "TEAM_OWNED_BY_PLAYER", 2 },
	{ "PLAYER_HAS_N_OR_FEWER_BUILDINGS", 2 },
	{ "PLAYER_HAS_POWER", 1 },
	{ "NAMED_REACHED_WAYPOINTS_END", 2 },
	{ "TEAM_REACHED_WAYPOINTS_END", 2 },
	{ "NAMED_SELECTED", 1 },
	{ "NAMED_ENTERED_AREA", 2 },
	{ "NAMED_EXITED_AREA", 2 },
	{ "TEAM_ENTERED_AREA_ENTIRELY", 3 },
	{ "TEAM_ENTERED_AREA_PARTIALLY", 3 },
	{ "TEAM_EXITED_AREA_ENTIRELY", 3 },
	{ "TEAM_EXITED_AREA_PARTIALLY", 3 },
	{ "MULTIPLAYER_ALLIED_VICTORY", 0 },
	{ "MULTIPLAYER_ALLIED_DEFEAT", 0 },
	{ "MULTIPLAYER_PLAYER_DEFEAT", 0 },
	{ "PLAYER_HAS_NO_POWER", 1 },
	{ "HAS_FINISHED_VIDEO", 1 },
	{ "HAS_FINISHED_SPEECH", 1 },
	{ "HAS_FINISHED_AUDIO", 1 },
	{ "BUILDING_ENTERED_BY_PLAYER", 2 },
	{ "ENEMY_SIGHTED", 3 },
	{ "UNIT_HEALTH", 3 },
	{ "BRIDGE_REPAIRED", 1 },
	{ "BRIDGE_BROKEN", 1 },
	{ "NAMED_DYING", 1 },
	{ "NAMED_TOTALLY_DEAD", 1 },
	{ "PLAYER_HAS_OBJECT_COMPARISON", 4 },
	{ "", 0 },  // OBSOLETE_SCRIPT_1: no init assignment
	{ "", 0 },  // OBSOLETE_SCRIPT_2: no init assignment
	{ "PLAYER_TRIGGERED_SPECIAL_POWER", 2 },
	{ "PLAYER_COMPLETED_SPECIAL_POWER", 2 },
	{ "PLAYER_MIDWAY_SPECIAL_POWER", 2 },
	{ "PLAYER_TRIGGERED_SPECIAL_POWER_FROM_NAMED", 3 },
	{ "PLAYER_COMPLETED_SPECIAL_POWER_FROM_NAMED", 3 },
	{ "PLAYER_MIDWAY_SPECIAL_POWER_FROM_NAMED", 3 },
	{ "", 0 },  // DEFUNCT_PLAYER_SELECTED_GENERAL: no init assignment
	{ "", 0 },  // DEFUNCT_PLAYER_SELECTED_GENERAL_FROM_NAMED: no init assignment
	{ "PLAYER_BUILT_UPGRADE", 2 },
	{ "PLAYER_BUILT_UPGRADE_FROM_NAMED", 3 },
	{ "PLAYER_DESTROYED_N_BUILDINGS_PLAYER", 3 },
	{ "", 0 },  // UNIT_COMPLETED_SEQUENTIAL_EXECUTION: no init assignment
	{ "", 0 },  // TEAM_COMPLETED_SEQUENTIAL_EXECUTION: no init assignment
	{ "PLAYER_HAS_COMPARISON_UNIT_TYPE_IN_TRIGGER_AREA", 5 },
	{ "PLAYER_HAS_COMPARISON_UNIT_KIND_IN_TRIGGER_AREA", 5 },
	{ "UNIT_EMPTIED", 1 },
	{ "TYPE_SIGHTED", 3 },
	{ "NAMED_BUILDING_IS_EMPTY", 1 },
	{ "PLAYER_HAS_N_OR_FEWER_FACTION_BUILDINGS", 2 },
	{ "UNIT_HAS_OBJECT_STATUS", 2 },
	{ "TEAM_ALL_HAS_OBJECT_STATUS", 2 },
	{ "TEAM_SOME_HAVE_OBJECT_STATUS", 2 },
	{ "PLAYER_POWER_COMPARE_PERCENT", 3 },
	{ "PLAYER_EXCESS_POWER_COMPARE_VALUE", 3 },
	{ "SKIRMISH_SPECIAL_POWER_READY", 2 },
	{ "SKIRMISH_VALUE_IN_AREA", 4 },
	{ "SKIRMISH_PLAYER_FACTION", 2 },
	{ "SKIRMISH_SUPPLIES_VALUE_WITHIN_DISTANCE", 4 },
	{ "SKIRMISH_TECH_BUILDING_WITHIN_DISTANCE", 3 },
	{ "SKIRMISH_COMMAND_BUTTON_READY_ALL", 3 },
	{ "SKIRMISH_COMMAND_BUTTON_READY_PARTIAL", 3 },
	{ "SKIRMISH_UNOWNED_FACTION_UNIT_EXISTS", 3 },
	{ "SKIRMISH_PLAYER_HAS_PREREQUISITE_TO_BUILD", 2 },
	{ "SKIRMISH_PLAYER_HAS_COMPARISON_GARRISONED", 3 },
	{ "SKIRMISH_PLAYER_HAS_COMPARISON_CAPTURED_UNITS", 3 },
	{ "SKIRMISH_NAMED_AREA_EXIST", 2 },
	{ "SKIRMISH_PLAYER_HAS_UNITS_IN_AREA", 2 },
	{ "SKIRMISH_PLAYER_HAS_BEEN_ATTACKED_BY_PLAYER", 2 },
	{ "SKIRMISH_PLAYER_IS_OUTSIDE_AREA", 2 },
	{ "SKIRMISH_PLAYER_HAS_DISCOVERED_PLAYER", 2 },
	{ "PLAYER_ACQUIRED_SCIENCE", 2 },
	{ "PLAYER_HAS_SCIENCEPURCHASEPOINTS", 2 },
	{ "PLAYER_CAN_PURCHASE_SCIENCE", 2 },
	{ "MUSIC_TRACK_HAS_COMPLETED", 2 },
	{ "PLAYER_LOST_OBJECT_TYPE", 2 },
	{ "SUPPLY_SOURCE_SAFE", 2 },
	{ "SUPPLY_SOURCE_ATTACKED", 1 },
	{ "START_POSITION_IS", 2 },
	{ "NAMED_HAS_FREE_CONTAINER_SLOTS", 1 },
};
static const TplLoad ACT_TEMPLATES[344] = {
	{ "DEBUG_MESSAGE_BOX", 1 },
	{ "SET_FLAG", 2 },
	{ "SET_COUNTER", 2 },
	{ "VICTORY", 0 },
	{ "DEFEAT", 0 },
	{ "NO_OP", 0 },
	{ "SET_TIMER", 2 },
	{ "PLAY_SOUND_EFFECT", 1 },
	{ "ENABLE_SCRIPT", 1 },
	{ "DISABLE_SCRIPT", 1 },
	{ "CALL_SUBROUTINE", 1 },
	{ "PLAY_SOUND_EFFECT_AT", 2 },
	{ "DAMAGE_MEMBERS_OF_TEAM", 2 },
	{ "MOVE_TEAM_TO", 2 },
	{ "MOVE_CAMERA_TO", 5 },
	{ "INCREMENT_COUNTER", 2 },
	{ "DECREMENT_COUNTER", 2 },
	{ "MOVE_CAMERA_ALONG_WAYPOINT_PATH", 5 },
	{ "ROTATE_CAMERA", 4 },
	{ "RESET_CAMERA", 4 },
	{ "SET_MILLISECOND_TIMER", 2 },
	{ "CAMERA_MOD_FREEZE_TIME", 0 },
	{ "SET_VISUAL_SPEED_MULTIPLIER", 1 },
	{ "CREATE_OBJECT", 4 },
	{ "SUSPEND_BACKGROUND_SOUNDS", 0 },
	{ "RESUME_BACKGROUND_SOUNDS", 0 },
	{ "CAMERA_MOD_SET_FINAL_ZOOM", 3 },
	{ "CAMERA_MOD_SET_FINAL_PITCH", 3 },
	{ "CAMERA_MOD_FREEZE_ANGLE", 0 },
	{ "CAMERA_MOD_SET_FINAL_SPEED_MULTIPLIER", 1 },
	{ "CAMERA_MOD_SET_ROLLING_AVERAGE", 1 },
	{ "CAMERA_MOD_FINAL_LOOK_TOWARD", 1 },
	{ "CAMERA_MOD_LOOK_TOWARD", 1 },
	{ "TEAM_ATTACK_TEAM", 2 },
	{ "CREATE_REINFORCEMENT_TEAM", 2 },
	{ "MOVE_CAMERA_TO_SELECTION", 0 },
	{ "TEAM_FOLLOW_WAYPOINTS", 3 },
	{ "TEAM_SET_STATE", 2 },
	{ "MOVE_NAMED_UNIT_TO", 2 },
	{ "NAMED_ATTACK_NAMED", 2 },
	{ "CREATE_NAMED_ON_TEAM_AT_WAYPOINT", 4 },
	{ "CREATE_UNNAMED_ON_TEAM_AT_WAYPOINT", 3 },
	{ "NAMED_APPLY_ATTACK_PRIORITY_SET", 2 },
	{ "TEAM_APPLY_ATTACK_PRIORITY_SET", 2 },
	{ "SET_BASE_CONSTRUCTION_SPEED", 2 },
	{ "NAMED_SET_ATTITUDE", 2 },
	{ "TEAM_SET_ATTITUDE", 2 },
	{ "NAMED_ATTACK_AREA", 2 },
	{ "NAMED_ATTACK_TEAM", 2 },
	{ "TEAM_ATTACK_AREA", 2 },
	{ "TEAM_ATTACK_NAMED", 2 },
	{ "TEAM_LOAD_TRANSPORTS", 1 },
	{ "NAMED_ENTER_NAMED", 2 },
	{ "TEAM_ENTER_NAMED", 2 },
	{ "NAMED_EXIT_ALL", 1 },
	{ "TEAM_EXIT_ALL", 1 },
	{ "NAMED_FOLLOW_WAYPOINTS", 2 },
	{ "NAMED_GUARD", 1 },
	{ "TEAM_GUARD", 1 },
	{ "NAMED_HUNT", 1 },
	{ "TEAM_HUNT", 1 },
	{ "PLAYER_SELL_EVERYTHING", 1 },
	{ "PLAYER_DISABLE_BASE_CONSTRUCTION", 1 },
	{ "PLAYER_DISABLE_FACTORIES", 2 },
	{ "PLAYER_DISABLE_UNIT_CONSTRUCTION", 1 },
	{ "PLAYER_ENABLE_BASE_CONSTRUCTION", 1 },
	{ "PLAYER_ENABLE_FACTORIES", 2 },
	{ "PLAYER_ENABLE_UNIT_CONSTRUCTION", 1 },
	{ "CAMERA_MOVE_HOME", 0 },
	{ "BUILD_TEAM", 1 },
	{ "NAMED_DAMAGE", 2 },
	{ "NAMED_DELETE", 1 },
	{ "TEAM_DELETE", 1 },
	{ "NAMED_KILL", 1 },
	{ "TEAM_KILL", 1 },
	{ "PLAYER_KILL", 1 },
	{ "DISPLAY_TEXT", 1 },
	{ "CAMEO_FLASH", 2 },
	{ "NAMED_FLASH", 2 },
	{ "TEAM_FLASH", 2 },
	{ "MOVIE_PLAY_FULLSCREEN", 1 },
	{ "MOVIE_PLAY_RADAR", 1 },
	{ "SOUND_PLAY_NAMED", 2 },
	{ "SPEECH_PLAY", 2 },
	{ "PLAYER_TRANSFER_OWNERSHIP_PLAYER", 2 },
	{ "NAMED_TRANSFER_OWNERSHIP_PLAYER", 2 },
	{ "PLAYER_RELATES_PLAYER", 3 },
	{ "RADAR_CREATE_EVENT", 2 },
	{ "RADAR_DISABLE", 0 },
	{ "RADAR_ENABLE", 0 },
	{ "MAP_REVEAL_AT_WAYPOINT", 3 },
	{ "TEAM_AVAILABLE_FOR_RECRUITMENT", 2 },
	{ "TEAM_COLLECT_NEARBY_FOR_TEAM", 1 },
	{ "TEAM_MERGE_INTO_TEAM", 2 },
	{ "DISABLE_INPUT", 0 },
	{ "ENABLE_INPUT", 0 },
	{ "PLAYER_HUNT", 1 },
	{ "SOUND_AMBIENT_PAUSE", 0 },
	{ "SOUND_AMBIENT_RESUME", 0 },
	{ "MUSIC_SET_TRACK", 3 },
	{ "SET_TREE_SWAY", 5 },
	{ "DEBUG_STRING", 1 },
	{ "MAP_REVEAL_ALL", 1 },
	{ "TEAM_GARRISON_SPECIFIC_BUILDING", 2 },
	{ "EXIT_SPECIFIC_BUILDING", 1 },
	{ "TEAM_GARRISON_NEAREST_BUILDING", 1 },
	{ "TEAM_EXIT_ALL_BUILDINGS", 1 },
	{ "NAMED_GARRISON_SPECIFIC_BUILDING", 2 },
	{ "NAMED_GARRISON_NEAREST_BUILDING", 1 },
	{ "NAMED_EXIT_BUILDING", 1 },
	{ "PLAYER_GARRISON_ALL_BUILDINGS", 1 },
	{ "PLAYER_EXIT_ALL_BUILDINGS", 1 },
	{ "TEAM_WANDER", 2 },
	{ "TEAM_PANIC", 2 },
	{ "SETUP_CAMERA", 4 },
	{ "CAMERA_LETTERBOX_BEGIN", 0 },
	{ "CAMERA_LETTERBOX_END", 0 },
	{ "ZOOM_CAMERA", 4 },
	{ "PITCH_CAMERA", 4 },
	{ "CAMERA_FOLLOW_NAMED", 2 },
	{ "OVERSIZE_TERRAIN", 1 },
	{ "CAMERA_FADE_ADD", 5 },
	{ "CAMERA_FADE_SUBTRACT", 5 },
	{ "CAMERA_FADE_SATURATE", 5 },
	{ "CAMERA_FADE_MULTIPLY", 0 },
	{ "CAMERA_BW_MODE_BEGIN", 1 },
	{ "CAMERA_BW_MODE_END", 1 },
	{ "DRAW_SKYBOX_BEGIN", 0 },
	{ "DRAW_SKYBOX_END", 0 },
	{ "SET_ATTACK_PRIORITY_THING", 3 },
	{ "SET_ATTACK_PRIORITY_KIND_OF", 3 },
	{ "SET_DEFAULT_ATTACK_PRIORITY", 2 },
	{ "CAMERA_STOP_FOLLOW", 0 },
	{ "CAMERA_MOTION_BLUR", 2 },
	{ "CAMERA_MOTION_BLUR_JUMP", 2 },
	{ "CAMERA_MOTION_BLUR_FOLLOW", 1 },
	{ "CAMERA_MOTION_BLUR_END_FOLLOW", 0 },
	{ "FREEZE_TIME", 0 },
	{ "UNFREEZE_TIME", 0 },
	{ "SHOW_MILITARY_CAPTION", 2 },
	{ "CAMERA_SET_AUDIBLE_DISTANCE", 1 },
	{ "SET_STOPPING_DISTANCE", 2 },
	{ "NAMED_SET_STOPPING_DISTANCE", 2 },
	{ "SET_FPS_LIMIT", 1 },
	{ "MUSIC_SET_VOLUME", 1 },
	{ "MAP_SHROUD_AT_WAYPOINT", 3 },
	{ "MAP_SHROUD_ALL", 1 },
	{ "SET_RANDOM_TIMER", 3 },
	{ "SET_RANDOM_MSEC_TIMER", 3 },
	{ "STOP_TIMER", 1 },
	{ "RESTART_TIMER", 1 },
	{ "ADD_TO_MSEC_TIMER", 2 },
	{ "SUB_FROM_MSEC_TIMER", 2 },
	{ "TEAM_TRANSFER_TO_PLAYER", 2 },
	{ "PLAYER_SET_MONEY", 2 },
	{ "PLAYER_GIVE_MONEY", 2 },
	{ "DISABLE_SPECIAL_POWER_DISPLAY", 0 },
	{ "ENABLE_SPECIAL_POWER_DISPLAY", 0 },
	{ "NAMED_HIDE_SPECIAL_POWER_DISPLAY", 1 },
	{ "NAMED_SHOW_SPECIAL_POWER_DISPLAY", 1 },
	{ "DISPLAY_COUNTDOWN_TIMER", 2 },
	{ "HIDE_COUNTDOWN_TIMER", 1 },
	{ "ENABLE_COUNTDOWN_TIMER_DISPLAY", 0 },
	{ "DISABLE_COUNTDOWN_TIMER_DISPLAY", 0 },
	{ "NAMED_STOP_SPECIAL_POWER_COUNTDOWN", 2 },
	{ "NAMED_START_SPECIAL_POWER_COUNTDOWN", 2 },
	{ "NAMED_SET_SPECIAL_POWER_COUNTDOWN", 3 },
	{ "NAMED_ADD_SPECIAL_POWER_COUNTDOWN", 3 },
	{ "NAMED_FIRE_SPECIAL_POWER_AT_WAYPOINT", 3 },
	{ "NAMED_FIRE_SPECIAL_POWER_AT_NAMED", 3 },
	{ "REFRESH_RADAR", 0 },
	{ "CAMERA_TETHER_NAMED", 3 },
	{ "CAMERA_STOP_TETHER_NAMED", 0 },
	{ "CAMERA_SET_DEFAULT", 3 },
	{ "NAMED_STOP", 1 },
	{ "TEAM_STOP", 1 },
	{ "TEAM_STOP_AND_DISBAND", 1 },
	{ "RECRUIT_TEAM", 2 },
	{ "TEAM_SET_OVERRIDE_RELATION_TO_TEAM", 3 },
	{ "TEAM_REMOVE_OVERRIDE_RELATION_TO_TEAM", 2 },
	{ "TEAM_REMOVE_ALL_OVERRIDE_RELATIONS", 1 },
	{ "CAMERA_LOOK_TOWARD_OBJECT", 5 },
	{ "NAMED_FIRE_WEAPON_FOLLOWING_WAYPOINT_PATH", 2 },
	{ "TEAM_SET_OVERRIDE_RELATION_TO_PLAYER", 3 },
	{ "TEAM_REMOVE_OVERRIDE_RELATION_TO_PLAYER", 2 },
	{ "PLAYER_SET_OVERRIDE_RELATION_TO_TEAM", 3 },
	{ "PLAYER_REMOVE_OVERRIDE_RELATION_TO_TEAM", 2 },
	{ "UNIT_EXECUTE_SEQUENTIAL_SCRIPT", 2 },
	{ "UNIT_EXECUTE_SEQUENTIAL_SCRIPT_LOOPING", 3 },
	{ "UNIT_STOP_SEQUENTIAL_SCRIPT", 1 },
	{ "TEAM_EXECUTE_SEQUENTIAL_SCRIPT", 2 },
	{ "TEAM_EXECUTE_SEQUENTIAL_SCRIPT_LOOPING", 3 },
	{ "TEAM_STOP_SEQUENTIAL_SCRIPT", 1 },
	{ "UNIT_GUARD_FOR_FRAMECOUNT", 2 },
	{ "UNIT_IDLE_FOR_FRAMECOUNT", 2 },
	{ "TEAM_GUARD_FOR_FRAMECOUNT", 2 },
	{ "TEAM_IDLE_FOR_FRAMECOUNT", 2 },
	{ "WATER_CHANGE_HEIGHT", 2 },
	{ "NAMED_USE_COMMANDBUTTON_ABILITY_ON_NAMED", 3 },
	{ "NAMED_USE_COMMANDBUTTON_ABILITY_AT_WAYPOINT", 3 },
	{ "WATER_CHANGE_HEIGHT_OVER_TIME", 4 },
	{ "MAP_SWITCH_BORDER", 1 },
	{ "TEAM_GUARD_POSITION", 2 },
	{ "TEAM_GUARD_OBJECT", 2 },
	{ "TEAM_GUARD_AREA", 2 },
	{ "OBJECT_FORCE_SELECT", 4 },
	{ "CAMERA_LOOK_TOWARD_WAYPOINT", 5 },
	{ "UNIT_DESTROY_ALL_CONTAINED", 1 },
	{ "RADAR_FORCE_ENABLE", 0 },
	{ "RADAR_REVERT_TO_NORMAL", 0 },
	{ "SCREEN_SHAKE", 1 },
	{ "TECHTREE_MODIFY_BUILDABILITY_OBJECT", 2 },
	{ "WAREHOUSE_SET_VALUE", 2 },
	{ "OBJECT_CREATE_RADAR_EVENT", 2 },
	{ "TEAM_CREATE_RADAR_EVENT", 2 },
	{ "DISPLAY_CINEMATIC_TEXT", 3 },
	{ "DEBUG_CRASH_BOX", 1 },
	{ "SOUND_DISABLE_TYPE", 1 },
	{ "SOUND_ENABLE_TYPE", 1 },
	{ "SOUND_ENABLE_ALL", 0 },
	{ "AUDIO_OVERRIDE_VOLUME_TYPE", 2 },
	{ "AUDIO_RESTORE_VOLUME_TYPE", 1 },
	{ "AUDIO_RESTORE_VOLUME_ALL_TYPE", 0 },
	{ "INGAME_POPUP_MESSAGE", 5 },
	{ "SET_CAVE_INDEX", 2 },
	{ "NAMED_SET_HELD", 2 },
	{ "NAMED_SET_TOPPLE_DIRECTION", 2 },
	{ "UNIT_MOVE_TOWARDS_NEAREST_OBJECT_TYPE", 3 },
	{ "TEAM_MOVE_TOWARDS_NEAREST_OBJECT_TYPE", 3 },
	{ "MAP_REVEAL_ALL_PERM", 1 },
	{ "MAP_REVEAL_ALL_UNDO_PERM", 1 },
	{ "NAMED_SET_REPULSOR", 2 },
	{ "TEAM_SET_REPULSOR", 2 },
	{ "TEAM_WANDER_IN_PLACE", 1 },
	{ "TEAM_INCREASE_PRIORITY", 1 },
	{ "TEAM_DECREASE_PRIORITY", 1 },
	{ "DISPLAY_COUNTER", 2 },
	{ "HIDE_COUNTER", 1 },
	{ "TEAM_USE_COMMANDBUTTON_ABILITY_ON_NAMED", 3 },
	{ "TEAM_USE_COMMANDBUTTON_ABILITY_AT_WAYPOINT", 3 },
	{ "NAMED_USE_COMMANDBUTTON_ABILITY", 2 },
	{ "TEAM_USE_COMMANDBUTTON_ABILITY", 2 },
	{ "NAMED_FLASH_WHITE", 2 },
	{ "TEAM_FLASH_WHITE", 2 },
	{ "SKIRMISH_BUILD_BUILDING", 1 },
	{ "SKIRMISH_FOLLOW_APPROACH_PATH", 3 },
	{ "IDLE_ALL_UNITS", 0 },
	{ "RESUME_SUPPLY_TRUCKING", 0 },
	{ "NAMED_CUSTOM_COLOR", 2 },
	{ "SKIRMISH_MOVE_TO_APPROACH_PATH", 2 },
	{ "SKIRMISH_BUILD_BASE_DEFENSE_FRONT", 0 },
	{ "SKIRMISH_FIRE_SPECIAL_POWER_AT_MOST_COST", 2 },
	{ "", 0 },  // NAMED_RECEIVE_UPGRADE: no init assignment
	{ "PLAYER_REPAIR_NAMED_STRUCTURE", 2 },
	{ "SKIRMISH_BUILD_BASE_DEFENSE_FLANK", 0 },
	{ "SKIRMISH_BUILD_STRUCTURE_FRONT", 1 },
	{ "SKIRMISH_BUILD_STRUCTURE_FLANK", 1 },
	{ "SKIRMISH_ATTACK_NEAREST_GROUP_WITH_VALUE", 3 },
	{ "SKIRMISH_PERFORM_COMMANDBUTTON_ON_MOST_VALUABLE_OBJECT", 4 },
	{ "SKIRMISH_WAIT_FOR_COMMANDBUTTON_AVAILABLE_ALL", 3 },
	{ "SKIRMISH_WAIT_FOR_COMMANDBUTTON_AVAILABLE_PARTIAL", 3 },
	{ "TEAM_SPIN_FOR_FRAMECOUNT", 2 },
	{ "TEAM_ALL_USE_COMMANDBUTTON_ON_NAMED", 2 },
	{ "TEAM_ALL_USE_COMMANDBUTTON_ON_NEAREST_ENEMY_UNIT", 2 },
	{ "TEAM_ALL_USE_COMMANDBUTTON_ON_NEAREST_GARRISONED_BUILDING", 2 },
	{ "TEAM_ALL_USE_COMMANDBUTTON_ON_NEAREST_KINDOF", 3 },
	{ "TEAM_ALL_USE_COMMANDBUTTON_ON_NEAREST_ENEMY_BUILDING", 2 },
	{ "TEAM_ALL_USE_COMMANDBUTTON_ON_NEAREST_ENEMY_BUILDING_CLASS", 3 },
	{ "TEAM_ALL_USE_COMMANDBUTTON_ON_NEAREST_OBJECTTYPE", 3 },
	{ "TEAM_PARTIAL_USE_COMMANDBUTTON", 3 },
	{ "TEAM_CAPTURE_NEAREST_UNOWNED_FACTION_UNIT", 1 },
	{ "PLAYER_CREATE_TEAM_FROM_CAPTURED_UNITS", 2 },
	{ "PLAYER_ADD_SKILLPOINTS", 2 },
	{ "PLAYER_ADD_RANKLEVEL", 2 },
	{ "PLAYER_SET_RANKLEVEL", 2 },
	{ "PLAYER_SET_RANKLEVELLIMIT", 1 },
	{ "PLAYER_GRANT_SCIENCE", 2 },
	{ "PLAYER_PURCHASE_SCIENCE", 2 },
	{ "TEAM_HUNT_WITH_COMMAND_BUTTON", 2 },
	{ "TEAM_WAIT_FOR_NOT_CONTAINED_ALL", 1 },
	{ "TEAM_WAIT_FOR_NOT_CONTAINED_PARTIAL", 1 },
	{ "TEAM_FOLLOW_WAYPOINTS_EXACT", 3 },
	{ "NAMED_FOLLOW_WAYPOINTS_EXACT", 2 },
	{ "TEAM_SET_EMOTICON", 3 },
	{ "NAMED_SET_EMOTICON", 3 },
	{ "AI_PLAYER_BUILD_SUPPLY_CENTER", 3 },
	{ "AI_PLAYER_BUILD_UPGRADE", 2 },
	{ "OBJECTLIST_ADDOBJECTTYPE", 2 },
	{ "OBJECTLIST_REMOVEOBJECTTYPE", 2 },
	{ "MAP_REVEAL_PERMANENTLY_AT_WAYPOINT", 4 },
	{ "MAP_UNDO_REVEAL_PERMANENTLY_AT_WAYPOINT", 1 },
	{ "NAMED_SET_STEALTH_ENABLED", 2 },
	{ "TEAM_SET_STEALTH_ENABLED", 2 },
	{ "EVA_SET_ENABLED_DISABLED", 1 },
	{ "OPTIONS_SET_OCCLUSION_MODE", 1 },
	{ "LOCALDEFEAT", 0 },
	{ "OPTIONS_SET_DRAWICON_UI_MODE", 1 },
	{ "OPTIONS_SET_PARTICLE_CAP_MODE", 1 },
	{ "PLAYER_SCIENCE_AVAILABILITY", 3 },
	{ "UNIT_AFFECT_OBJECT_PANEL_FLAGS", 3 },
	{ "TEAM_AFFECT_OBJECT_PANEL_FLAGS", 3 },
	{ "PLAYER_SELECT_SKILLSET", 2 },
	{ "SCRIPTING_OVERRIDE_HULK_LIFETIME", 1 },
	{ "NAMED_FACE_NAMED", 2 },
	{ "NAMED_FACE_WAYPOINT", 2 },
	{ "TEAM_FACE_NAMED", 2 },
	{ "TEAM_FACE_WAYPOINT", 2 },
	{ "COMMANDBAR_REMOVE_BUTTON_OBJECTTYPE", 2 },
	{ "COMMANDBAR_ADD_BUTTON_OBJECTTYPE_SLOT", 3 },
	{ "UNIT_SPAWN_NAMED_LOCATION_ORIENTATION", 5 },
	{ "PLAYER_AFFECT_RECEIVING_EXPERIENCE", 2 },
	{ "PLAYER_EXCLUDE_FROM_SCORE_SCREEN", 1 },
	{ "TEAM_GUARD_SUPPLY_CENTER", 2 },
	{ "ENABLE_SCORING", 0 },
	{ "DISABLE_SCORING", 0 },
	{ "SOUND_SET_VOLUME", 1 },
	{ "SPEECH_SET_VOLUME", 1 },
	{ "DISABLE_BORDER_SHROUD", 0 },
	{ "ENABLE_BORDER_SHROUD", 0 },
	{ "OBJECT_ALLOW_BONUSES", 1 },
	{ "SOUND_REMOVE_ALL_DISABLED", 0 },
	{ "SOUND_REMOVE_TYPE", 1 },
	{ "TEAM_GUARD_IN_TUNNEL_NETWORK", 1 },
	{ "QUICKVICTORY", 0 },
	{ "SET_INFANTRY_LIGHTING_OVERRIDE", 1 },
	{ "RESET_INFANTRY_LIGHTING_OVERRIDE", 0 },
	{ "TEAM_DELETE_LIVING", 1 },
	{ "RESIZE_VIEW_GUARDBAND", 2 },
	{ "DELETE_ALL_UNMANNED", 0 },
	{ "CHOOSE_VICTIM_ALWAYS_USES_NORMAL", 1 },
	{ "CAMERA_ENABLE_SLAVE_MODE", 2 },
	{ "CAMERA_DISABLE_SLAVE_MODE", 0 },
	{ "CAMERA_ADD_SHAKER_AT", 4 },
	{ "SET_TRAIN_HELD", 2 },
	{ "NAMED_SET_EVAC_LEFT_OR_RIGHT", 2 },
	{ "ENABLE_OBJECT_SOUND", 1 },
	{ "DISABLE_OBJECT_SOUND", 1 },
	{ "NAMED_USE_COMMANDBUTTON_ABILITY_USING_WAYPOINT_PATH", 3 },
	{ "NAMED_SET_UNMANNED_STATUS", 1 },
	{ "TEAM_SET_UNMANNED_STATUS", 1 },
	{ "NAMED_SET_BOOBYTRAPPED", 2 },
	{ "TEAM_SET_BOOBYTRAPPED", 2 },
	{ "SHOW_WEATHER", 1 },
	{ "AI_PLAYER_BUILD_TYPE_NEAREST_TEAM", 3 },
};

// KindOfMaskType::s_bitNameList (KindOf.cpp, the Zero Hour build:
// ALLOW_SURRENDER and RTS_GENERALS both off), nullptr-terminated
// like the original array.
static const char *const KINDOF_NAMES[117 + 1] = {
	"OBSTACLE",
	"SELECTABLE",
	"IMMOBILE",
	"CAN_ATTACK",
	"STICK_TO_TERRAIN_SLOPE",
	"CAN_CAST_REFLECTIONS",
	"SHRUBBERY",
	"STRUCTURE",
	"INFANTRY",
	"VEHICLE",
	"AIRCRAFT",
	"HUGE_VEHICLE",
	"DOZER",
	"HARVESTER",
	"COMMANDCENTER",
	"LINEBUILD",
	"SALVAGER",
	"WEAPON_SALVAGER",
	"TRANSPORT",
	"BRIDGE",
	"LANDMARK_BRIDGE",
	"BRIDGE_TOWER",
	"PROJECTILE",
	"PRELOAD",
	"NO_GARRISON",
	"WAVEGUIDE",
	"WAVE_EFFECT",
	"NO_COLLIDE",
	"REPAIR_PAD",
	"HEAL_PAD",
	"STEALTH_GARRISON",
	"CASH_GENERATOR",
	"DRAWABLE_ONLY",
	"MP_COUNT_FOR_VICTORY",
	"REBUILD_HOLE",
	"SCORE",
	"SCORE_CREATE",
	"SCORE_DESTROY",
	"NO_HEAL_ICON",
	"CAN_RAPPEL",
	"PARACHUTABLE",
	"CAN_BE_REPULSED",
	"MOB_NEXUS",
	"IGNORED_IN_GUI",
	"CRATE",
	"CAPTURABLE",
	"CLEARED_BY_BUILD",
	"SMALL_MISSILE",
	"ALWAYS_VISIBLE",
	"UNATTACKABLE",
	"MINE",
	"CLEANUP_HAZARD",
	"PORTABLE_STRUCTURE",
	"ALWAYS_SELECTABLE",
	"ATTACK_NEEDS_LINE_OF_SIGHT",
	"WALK_ON_TOP_OF_WALL",
	"DEFENSIVE_WALL",
	"FS_POWER",
	"FS_FACTORY",
	"FS_BASE_DEFENSE",
	"FS_TECHNOLOGY",
	"AIRCRAFT_PATH_AROUND",
	"LOW_OVERLAPPABLE",
	"FORCEATTACKABLE",
	"AUTO_RALLYPOINT",
	"TECH_BUILDING",
	"POWERED",
	"PRODUCED_AT_HELIPAD",
	"DRONE",
	"CAN_SEE_THROUGH_STRUCTURE",
	"BALLISTIC_MISSILE",
	"CLICK_THROUGH",
	"SUPPLY_SOURCE_ON_PREVIEW",
	"PARACHUTE",
	"GARRISONABLE_UNTIL_DESTROYED",
	"BOAT",
	"IMMUNE_TO_CAPTURE",
	"HULK",
	"SHOW_PORTRAIT_WHEN_CONTROLLED",
	"SPAWNS_ARE_THE_WEAPONS",
	"CANNOT_BUILD_NEAR_SUPPLIES",
	"SUPPLY_SOURCE",
	"REVEAL_TO_ALL",
	"DISGUISER",
	"INERT",
	"HERO",
	"IGNORES_SELECT_ALL",
	"DONT_AUTO_CRUSH_INFANTRY",
	"CLIFF_JUMPER",
	"FS_SUPPLY_DROPZONE",
	"FS_SUPERWEAPON",
	"FS_BLACK_MARKET",
	"FS_SUPPLY_CENTER",
	"FS_STRATEGY_CENTER",
	"MONEY_HACKER",
	"ARMOR_SALVAGER",
	"REVEALS_ENEMY_PATHS",
	"BOOBY_TRAP",
	"FS_FAKE",
	"FS_INTERNET_CENTER",
	"BLAST_CRATER",
	"PROP",
	"OPTIMIZED_TREE",
	"FS_ADVANCED_TECH",
	"FS_BARRACKS",
	"FS_WARFACTORY",
	"FS_AIRFIELD",
	"AIRCRAFT_CARRIER",
	"NO_SELECT",
	"REJECT_UNMANNED",
	"CANNOT_RETALIATE",
	"TECH_BASE_DEFENSE",
	"EMP_HARDENED",
	"DEMOTRAP",
	"CONSERVATIVE_BUILDING",
	"IGNORE_DOCKING_BONES",
	"NO_ATTACK_WARNING",
	nullptr,
};

// ObjectStatusMaskType::s_bitNameList (ObjectStatusTypes.cpp).
static const char *const OBJECT_STATUS_NAMES[45] = {
	"NONE",
	"DESTROYED",
	"CAN_ATTACK",
	"UNDER_CONSTRUCTION",
	"UNSELECTABLE",
	"NO_COLLISIONS",
	"NO_ATTACK",
	"AIRBORNE_TARGET",
	"PARACHUTING",
	"REPULSOR",
	"HIJACKED",
	"AFLAME",
	"BURNED",
	"WET",
	"IS_FIRING_WEAPON",
	"IS_BRAKING",
	"STEALTHED",
	"DETECTED",
	"CAN_STEALTH",
	"SOLD",
	"UNDERGOING_REPAIR",
	"RECONSTRUCTING",
	"MASKED",
	"IS_ATTACKING",
	"USING_ABILITY",
	"IS_AIMING_WEAPON",
	"NO_ATTACK_FROM_AI",
	"IGNORING_STEALTH",
	"IS_CARBOMB",
	"DECK_HEIGHT_OFFSET",
	"STATUS_RIDER1",
	"STATUS_RIDER2",
	"STATUS_RIDER3",
	"STATUS_RIDER4",
	"STATUS_RIDER5",
	"STATUS_RIDER6",
	"STATUS_RIDER7",
	"STATUS_RIDER8",
	"FAERIE_FIRE",
	"KILLING_SELF",
	"REASSIGN_PARKING",
	"BOOBY_TRAPPED",
	"IMMOBILE",
	"DISGUISED",
	"DEPLOYED",
};
// The enum values (Scripts.h's ConditionType, ScriptActionType and
// ParameterType), prefixed CT_/AT_/PT_ — generated with the tables
// so the transcription and the port cannot drift.
static const Int PT_INT = 0;
static const Int PT_REAL = 1;
static const Int PT_SCRIPT = 2;
static const Int PT_TEAM = 3;
static const Int PT_COUNTER = 4;
static const Int PT_FLAG = 5;
static const Int PT_COMPARISON = 6;
static const Int PT_WAYPOINT = 7;
static const Int PT_BOOLEAN = 8;
static const Int PT_TRIGGER_AREA = 9;
static const Int PT_TEXT_STRING = 10;
static const Int PT_SIDE = 11;
static const Int PT_SOUND = 12;
static const Int PT_SCRIPT_SUBROUTINE = 13;
static const Int PT_UNIT = 14;
static const Int PT_OBJECT_TYPE = 15;
static const Int PT_COORD3D = 16;
static const Int PT_ANGLE = 17;
static const Int PT_TEAM_STATE = 18;
static const Int PT_RELATION = 19;
static const Int PT_AI_MOOD = 20;
static const Int PT_DIALOG = 21;
static const Int PT_MUSIC = 22;
static const Int PT_MOVIE = 23;
static const Int PT_WAYPOINT_PATH = 24;
static const Int PT_LOCALIZED_TEXT = 25;
static const Int PT_BRIDGE = 26;
static const Int PT_KIND_OF_PARAM = 27;
static const Int PT_ATTACK_PRIORITY_SET = 28;
static const Int PT_RADAR_EVENT_TYPE = 29;
static const Int PT_SPECIAL_POWER = 30;
static const Int PT_SCIENCE = 31;
static const Int PT_UPGRADE = 32;
static const Int PT_COMMANDBUTTON_ABILITY = 33;
static const Int PT_BOUNDARY = 34;
static const Int PT_BUILDABLE = 35;
static const Int PT_SURFACES_ALLOWED = 36;
static const Int PT_SHAKE_INTENSITY = 37;
static const Int PT_COMMAND_BUTTON = 38;
static const Int PT_FONT_NAME = 39;
static const Int PT_OBJECT_STATUS = 40;
static const Int PT_COMMANDBUTTON_ALL_ABILITIES = 41;
static const Int PT_SKIRMISH_WAYPOINT_PATH = 42;
static const Int PT_COLOR = 43;
static const Int PT_EMOTICON = 44;
static const Int PT_OBJECT_PANEL_FLAG = 45;
static const Int PT_FACTION_NAME = 46;
static const Int PT_OBJECT_TYPE_LIST = 47;
static const Int PT_REVEALNAME = 48;
static const Int PT_SCIENCE_AVAILABILITY = 49;
static const Int PT_LEFT_OR_RIGHT = 50;
static const Int PT_PERCENT = 51;
static const Int PT_NUM_ITEMS = 52;
static const Int CT_CONDITION_FALSE = 0;
static const Int CT_COUNTER = 1;
static const Int CT_FLAG = 2;
static const Int CT_CONDITION_TRUE = 3;
static const Int CT_TIMER_EXPIRED = 4;
static const Int CT_PLAYER_ALL_DESTROYED = 5;
static const Int CT_PLAYER_ALL_BUILDFACILITIES_DESTROYED = 6;
static const Int CT_TEAM_INSIDE_AREA_PARTIALLY = 7;
static const Int CT_TEAM_DESTROYED = 8;
static const Int CT_CAMERA_MOVEMENT_FINISHED = 9;
static const Int CT_TEAM_HAS_UNITS = 10;
static const Int CT_TEAM_STATE_IS = 11;
static const Int CT_TEAM_STATE_IS_NOT = 12;
static const Int CT_NAMED_INSIDE_AREA = 13;
static const Int CT_NAMED_OUTSIDE_AREA = 14;
static const Int CT_NAMED_DESTROYED = 15;
static const Int CT_NAMED_NOT_DESTROYED = 16;
static const Int CT_TEAM_INSIDE_AREA_ENTIRELY = 17;
static const Int CT_TEAM_OUTSIDE_AREA_ENTIRELY = 18;
static const Int CT_NAMED_ATTACKED_BY_OBJECTTYPE = 19;
static const Int CT_TEAM_ATTACKED_BY_OBJECTTYPE = 20;
static const Int CT_NAMED_ATTACKED_BY_PLAYER = 21;
static const Int CT_TEAM_ATTACKED_BY_PLAYER = 22;
static const Int CT_BUILT_BY_PLAYER = 23;
static const Int CT_NAMED_CREATED = 24;
static const Int CT_TEAM_CREATED = 25;
static const Int CT_PLAYER_HAS_CREDITS = 26;
static const Int CT_NAMED_DISCOVERED = 27;
static const Int CT_TEAM_DISCOVERED = 28;
static const Int CT_MISSION_ATTEMPTS = 29;
static const Int CT_NAMED_OWNED_BY_PLAYER = 30;
static const Int CT_TEAM_OWNED_BY_PLAYER = 31;
static const Int CT_PLAYER_HAS_N_OR_FEWER_BUILDINGS = 32;
static const Int CT_PLAYER_HAS_POWER = 33;
static const Int CT_NAMED_REACHED_WAYPOINTS_END = 34;
static const Int CT_TEAM_REACHED_WAYPOINTS_END = 35;
static const Int CT_NAMED_SELECTED = 36;
static const Int CT_NAMED_ENTERED_AREA = 37;
static const Int CT_NAMED_EXITED_AREA = 38;
static const Int CT_TEAM_ENTERED_AREA_ENTIRELY = 39;
static const Int CT_TEAM_ENTERED_AREA_PARTIALLY = 40;
static const Int CT_TEAM_EXITED_AREA_ENTIRELY = 41;
static const Int CT_TEAM_EXITED_AREA_PARTIALLY = 42;
static const Int CT_MULTIPLAYER_ALLIED_VICTORY = 43;
static const Int CT_MULTIPLAYER_ALLIED_DEFEAT = 44;
static const Int CT_MULTIPLAYER_PLAYER_DEFEAT = 45;
static const Int CT_PLAYER_HAS_NO_POWER = 46;
static const Int CT_HAS_FINISHED_VIDEO = 47;
static const Int CT_HAS_FINISHED_SPEECH = 48;
static const Int CT_HAS_FINISHED_AUDIO = 49;
static const Int CT_BUILDING_ENTERED_BY_PLAYER = 50;
static const Int CT_ENEMY_SIGHTED = 51;
static const Int CT_UNIT_HEALTH = 52;
static const Int CT_BRIDGE_REPAIRED = 53;
static const Int CT_BRIDGE_BROKEN = 54;
static const Int CT_NAMED_DYING = 55;
static const Int CT_NAMED_TOTALLY_DEAD = 56;
static const Int CT_PLAYER_HAS_OBJECT_COMPARISON = 57;
static const Int CT_OBSOLETE_SCRIPT_1 = 58;
static const Int CT_OBSOLETE_SCRIPT_2 = 59;
static const Int CT_PLAYER_TRIGGERED_SPECIAL_POWER = 60;
static const Int CT_PLAYER_COMPLETED_SPECIAL_POWER = 61;
static const Int CT_PLAYER_MIDWAY_SPECIAL_POWER = 62;
static const Int CT_PLAYER_TRIGGERED_SPECIAL_POWER_FROM_NAMED = 63;
static const Int CT_PLAYER_COMPLETED_SPECIAL_POWER_FROM_NAMED = 64;
static const Int CT_PLAYER_MIDWAY_SPECIAL_POWER_FROM_NAMED = 65;
static const Int CT_DEFUNCT_PLAYER_SELECTED_GENERAL = 66;
static const Int CT_DEFUNCT_PLAYER_SELECTED_GENERAL_FROM_NAMED = 67;
static const Int CT_PLAYER_BUILT_UPGRADE = 68;
static const Int CT_PLAYER_BUILT_UPGRADE_FROM_NAMED = 69;
static const Int CT_PLAYER_DESTROYED_N_BUILDINGS_PLAYER = 70;
static const Int CT_UNIT_COMPLETED_SEQUENTIAL_EXECUTION = 71;
static const Int CT_TEAM_COMPLETED_SEQUENTIAL_EXECUTION = 72;
static const Int CT_PLAYER_HAS_COMPARISON_UNIT_TYPE_IN_TRIGGER_AREA = 73;
static const Int CT_PLAYER_HAS_COMPARISON_UNIT_KIND_IN_TRIGGER_AREA = 74;
static const Int CT_UNIT_EMPTIED = 75;
static const Int CT_TYPE_SIGHTED = 76;
static const Int CT_NAMED_BUILDING_IS_EMPTY = 77;
static const Int CT_PLAYER_HAS_N_OR_FEWER_FACTION_BUILDINGS = 78;
static const Int CT_UNIT_HAS_OBJECT_STATUS = 79;
static const Int CT_TEAM_ALL_HAS_OBJECT_STATUS = 80;
static const Int CT_TEAM_SOME_HAVE_OBJECT_STATUS = 81;
static const Int CT_PLAYER_POWER_COMPARE_PERCENT = 82;
static const Int CT_PLAYER_EXCESS_POWER_COMPARE_VALUE = 83;
static const Int CT_SKIRMISH_SPECIAL_POWER_READY = 84;
static const Int CT_SKIRMISH_VALUE_IN_AREA = 85;
static const Int CT_SKIRMISH_PLAYER_FACTION = 86;
static const Int CT_SKIRMISH_SUPPLIES_VALUE_WITHIN_DISTANCE = 87;
static const Int CT_SKIRMISH_TECH_BUILDING_WITHIN_DISTANCE = 88;
static const Int CT_SKIRMISH_COMMAND_BUTTON_READY_ALL = 89;
static const Int CT_SKIRMISH_COMMAND_BUTTON_READY_PARTIAL = 90;
static const Int CT_SKIRMISH_UNOWNED_FACTION_UNIT_EXISTS = 91;
static const Int CT_SKIRMISH_PLAYER_HAS_PREREQUISITE_TO_BUILD = 92;
static const Int CT_SKIRMISH_PLAYER_HAS_COMPARISON_GARRISONED = 93;
static const Int CT_SKIRMISH_PLAYER_HAS_COMPARISON_CAPTURED_UNITS = 94;
static const Int CT_SKIRMISH_NAMED_AREA_EXIST = 95;
static const Int CT_SKIRMISH_PLAYER_HAS_UNITS_IN_AREA = 96;
static const Int CT_SKIRMISH_PLAYER_HAS_BEEN_ATTACKED_BY_PLAYER = 97;
static const Int CT_SKIRMISH_PLAYER_IS_OUTSIDE_AREA = 98;
static const Int CT_SKIRMISH_PLAYER_HAS_DISCOVERED_PLAYER = 99;
static const Int CT_PLAYER_ACQUIRED_SCIENCE = 100;
static const Int CT_PLAYER_HAS_SCIENCEPURCHASEPOINTS = 101;
static const Int CT_PLAYER_CAN_PURCHASE_SCIENCE = 102;
static const Int CT_MUSIC_TRACK_HAS_COMPLETED = 103;
static const Int CT_PLAYER_LOST_OBJECT_TYPE = 104;
static const Int CT_SUPPLY_SOURCE_SAFE = 105;
static const Int CT_SUPPLY_SOURCE_ATTACKED = 106;
static const Int CT_START_POSITION_IS = 107;
static const Int CT_NAMED_HAS_FREE_CONTAINER_SLOTS = 108;
static const Int CT_NUM_ITEMS = 109;
static const Int AT_DEBUG_MESSAGE_BOX = 0;
static const Int AT_SET_FLAG = 1;
static const Int AT_SET_COUNTER = 2;
static const Int AT_VICTORY = 3;
static const Int AT_DEFEAT = 4;
static const Int AT_NO_OP = 5;
static const Int AT_SET_TIMER = 6;
static const Int AT_PLAY_SOUND_EFFECT = 7;
static const Int AT_ENABLE_SCRIPT = 8;
static const Int AT_DISABLE_SCRIPT = 9;
static const Int AT_CALL_SUBROUTINE = 10;
static const Int AT_PLAY_SOUND_EFFECT_AT = 11;
static const Int AT_DAMAGE_MEMBERS_OF_TEAM = 12;
static const Int AT_MOVE_TEAM_TO = 13;
static const Int AT_MOVE_CAMERA_TO = 14;
static const Int AT_INCREMENT_COUNTER = 15;
static const Int AT_DECREMENT_COUNTER = 16;
static const Int AT_MOVE_CAMERA_ALONG_WAYPOINT_PATH = 17;
static const Int AT_ROTATE_CAMERA = 18;
static const Int AT_RESET_CAMERA = 19;
static const Int AT_SET_MILLISECOND_TIMER = 20;
static const Int AT_CAMERA_MOD_FREEZE_TIME = 21;
static const Int AT_SET_VISUAL_SPEED_MULTIPLIER = 22;
static const Int AT_CREATE_OBJECT = 23;
static const Int AT_SUSPEND_BACKGROUND_SOUNDS = 24;
static const Int AT_RESUME_BACKGROUND_SOUNDS = 25;
static const Int AT_CAMERA_MOD_SET_FINAL_ZOOM = 26;
static const Int AT_CAMERA_MOD_SET_FINAL_PITCH = 27;
static const Int AT_CAMERA_MOD_FREEZE_ANGLE = 28;
static const Int AT_CAMERA_MOD_SET_FINAL_SPEED_MULTIPLIER = 29;
static const Int AT_CAMERA_MOD_SET_ROLLING_AVERAGE = 30;
static const Int AT_CAMERA_MOD_FINAL_LOOK_TOWARD = 31;
static const Int AT_CAMERA_MOD_LOOK_TOWARD = 32;
static const Int AT_TEAM_ATTACK_TEAM = 33;
static const Int AT_CREATE_REINFORCEMENT_TEAM = 34;
static const Int AT_MOVE_CAMERA_TO_SELECTION = 35;
static const Int AT_TEAM_FOLLOW_WAYPOINTS = 36;
static const Int AT_TEAM_SET_STATE = 37;
static const Int AT_MOVE_NAMED_UNIT_TO = 38;
static const Int AT_NAMED_ATTACK_NAMED = 39;
static const Int AT_CREATE_NAMED_ON_TEAM_AT_WAYPOINT = 40;
static const Int AT_CREATE_UNNAMED_ON_TEAM_AT_WAYPOINT = 41;
static const Int AT_NAMED_APPLY_ATTACK_PRIORITY_SET = 42;
static const Int AT_TEAM_APPLY_ATTACK_PRIORITY_SET = 43;
static const Int AT_SET_BASE_CONSTRUCTION_SPEED = 44;
static const Int AT_NAMED_SET_ATTITUDE = 45;
static const Int AT_TEAM_SET_ATTITUDE = 46;
static const Int AT_NAMED_ATTACK_AREA = 47;
static const Int AT_NAMED_ATTACK_TEAM = 48;
static const Int AT_TEAM_ATTACK_AREA = 49;
static const Int AT_TEAM_ATTACK_NAMED = 50;
static const Int AT_TEAM_LOAD_TRANSPORTS = 51;
static const Int AT_NAMED_ENTER_NAMED = 52;
static const Int AT_TEAM_ENTER_NAMED = 53;
static const Int AT_NAMED_EXIT_ALL = 54;
static const Int AT_TEAM_EXIT_ALL = 55;
static const Int AT_NAMED_FOLLOW_WAYPOINTS = 56;
static const Int AT_NAMED_GUARD = 57;
static const Int AT_TEAM_GUARD = 58;
static const Int AT_NAMED_HUNT = 59;
static const Int AT_TEAM_HUNT = 60;
static const Int AT_PLAYER_SELL_EVERYTHING = 61;
static const Int AT_PLAYER_DISABLE_BASE_CONSTRUCTION = 62;
static const Int AT_PLAYER_DISABLE_FACTORIES = 63;
static const Int AT_PLAYER_DISABLE_UNIT_CONSTRUCTION = 64;
static const Int AT_PLAYER_ENABLE_BASE_CONSTRUCTION = 65;
static const Int AT_PLAYER_ENABLE_FACTORIES = 66;
static const Int AT_PLAYER_ENABLE_UNIT_CONSTRUCTION = 67;
static const Int AT_CAMERA_MOVE_HOME = 68;
static const Int AT_BUILD_TEAM = 69;
static const Int AT_NAMED_DAMAGE = 70;
static const Int AT_NAMED_DELETE = 71;
static const Int AT_TEAM_DELETE = 72;
static const Int AT_NAMED_KILL = 73;
static const Int AT_TEAM_KILL = 74;
static const Int AT_PLAYER_KILL = 75;
static const Int AT_DISPLAY_TEXT = 76;
static const Int AT_CAMEO_FLASH = 77;
static const Int AT_NAMED_FLASH = 78;
static const Int AT_TEAM_FLASH = 79;
static const Int AT_MOVIE_PLAY_FULLSCREEN = 80;
static const Int AT_MOVIE_PLAY_RADAR = 81;
static const Int AT_SOUND_PLAY_NAMED = 82;
static const Int AT_SPEECH_PLAY = 83;
static const Int AT_PLAYER_TRANSFER_OWNERSHIP_PLAYER = 84;
static const Int AT_NAMED_TRANSFER_OWNERSHIP_PLAYER = 85;
static const Int AT_PLAYER_RELATES_PLAYER = 86;
static const Int AT_RADAR_CREATE_EVENT = 87;
static const Int AT_RADAR_DISABLE = 88;
static const Int AT_RADAR_ENABLE = 89;
static const Int AT_MAP_REVEAL_AT_WAYPOINT = 90;
static const Int AT_TEAM_AVAILABLE_FOR_RECRUITMENT = 91;
static const Int AT_TEAM_COLLECT_NEARBY_FOR_TEAM = 92;
static const Int AT_TEAM_MERGE_INTO_TEAM = 93;
static const Int AT_DISABLE_INPUT = 94;
static const Int AT_ENABLE_INPUT = 95;
static const Int AT_PLAYER_HUNT = 96;
static const Int AT_SOUND_AMBIENT_PAUSE = 97;
static const Int AT_SOUND_AMBIENT_RESUME = 98;
static const Int AT_MUSIC_SET_TRACK = 99;
static const Int AT_SET_TREE_SWAY = 100;
static const Int AT_DEBUG_STRING = 101;
static const Int AT_MAP_REVEAL_ALL = 102;
static const Int AT_TEAM_GARRISON_SPECIFIC_BUILDING = 103;
static const Int AT_EXIT_SPECIFIC_BUILDING = 104;
static const Int AT_TEAM_GARRISON_NEAREST_BUILDING = 105;
static const Int AT_TEAM_EXIT_ALL_BUILDINGS = 106;
static const Int AT_NAMED_GARRISON_SPECIFIC_BUILDING = 107;
static const Int AT_NAMED_GARRISON_NEAREST_BUILDING = 108;
static const Int AT_NAMED_EXIT_BUILDING = 109;
static const Int AT_PLAYER_GARRISON_ALL_BUILDINGS = 110;
static const Int AT_PLAYER_EXIT_ALL_BUILDINGS = 111;
static const Int AT_TEAM_WANDER = 112;
static const Int AT_TEAM_PANIC = 113;
static const Int AT_SETUP_CAMERA = 114;
static const Int AT_CAMERA_LETTERBOX_BEGIN = 115;
static const Int AT_CAMERA_LETTERBOX_END = 116;
static const Int AT_ZOOM_CAMERA = 117;
static const Int AT_PITCH_CAMERA = 118;
static const Int AT_CAMERA_FOLLOW_NAMED = 119;
static const Int AT_OVERSIZE_TERRAIN = 120;
static const Int AT_CAMERA_FADE_ADD = 121;
static const Int AT_CAMERA_FADE_SUBTRACT = 122;
static const Int AT_CAMERA_FADE_SATURATE = 123;
static const Int AT_CAMERA_FADE_MULTIPLY = 124;
static const Int AT_CAMERA_BW_MODE_BEGIN = 125;
static const Int AT_CAMERA_BW_MODE_END = 126;
static const Int AT_DRAW_SKYBOX_BEGIN = 127;
static const Int AT_DRAW_SKYBOX_END = 128;
static const Int AT_SET_ATTACK_PRIORITY_THING = 129;
static const Int AT_SET_ATTACK_PRIORITY_KIND_OF = 130;
static const Int AT_SET_DEFAULT_ATTACK_PRIORITY = 131;
static const Int AT_CAMERA_STOP_FOLLOW = 132;
static const Int AT_CAMERA_MOTION_BLUR = 133;
static const Int AT_CAMERA_MOTION_BLUR_JUMP = 134;
static const Int AT_CAMERA_MOTION_BLUR_FOLLOW = 135;
static const Int AT_CAMERA_MOTION_BLUR_END_FOLLOW = 136;
static const Int AT_FREEZE_TIME = 137;
static const Int AT_UNFREEZE_TIME = 138;
static const Int AT_SHOW_MILITARY_CAPTION = 139;
static const Int AT_CAMERA_SET_AUDIBLE_DISTANCE = 140;
static const Int AT_SET_STOPPING_DISTANCE = 141;
static const Int AT_NAMED_SET_STOPPING_DISTANCE = 142;
static const Int AT_SET_FPS_LIMIT = 143;
static const Int AT_MUSIC_SET_VOLUME = 144;
static const Int AT_MAP_SHROUD_AT_WAYPOINT = 145;
static const Int AT_MAP_SHROUD_ALL = 146;
static const Int AT_SET_RANDOM_TIMER = 147;
static const Int AT_SET_RANDOM_MSEC_TIMER = 148;
static const Int AT_STOP_TIMER = 149;
static const Int AT_RESTART_TIMER = 150;
static const Int AT_ADD_TO_MSEC_TIMER = 151;
static const Int AT_SUB_FROM_MSEC_TIMER = 152;
static const Int AT_TEAM_TRANSFER_TO_PLAYER = 153;
static const Int AT_PLAYER_SET_MONEY = 154;
static const Int AT_PLAYER_GIVE_MONEY = 155;
static const Int AT_DISABLE_SPECIAL_POWER_DISPLAY = 156;
static const Int AT_ENABLE_SPECIAL_POWER_DISPLAY = 157;
static const Int AT_NAMED_HIDE_SPECIAL_POWER_DISPLAY = 158;
static const Int AT_NAMED_SHOW_SPECIAL_POWER_DISPLAY = 159;
static const Int AT_DISPLAY_COUNTDOWN_TIMER = 160;
static const Int AT_HIDE_COUNTDOWN_TIMER = 161;
static const Int AT_ENABLE_COUNTDOWN_TIMER_DISPLAY = 162;
static const Int AT_DISABLE_COUNTDOWN_TIMER_DISPLAY = 163;
static const Int AT_NAMED_STOP_SPECIAL_POWER_COUNTDOWN = 164;
static const Int AT_NAMED_START_SPECIAL_POWER_COUNTDOWN = 165;
static const Int AT_NAMED_SET_SPECIAL_POWER_COUNTDOWN = 166;
static const Int AT_NAMED_ADD_SPECIAL_POWER_COUNTDOWN = 167;
static const Int AT_NAMED_FIRE_SPECIAL_POWER_AT_WAYPOINT = 168;
static const Int AT_NAMED_FIRE_SPECIAL_POWER_AT_NAMED = 169;
static const Int AT_REFRESH_RADAR = 170;
static const Int AT_CAMERA_TETHER_NAMED = 171;
static const Int AT_CAMERA_STOP_TETHER_NAMED = 172;
static const Int AT_CAMERA_SET_DEFAULT = 173;
static const Int AT_NAMED_STOP = 174;
static const Int AT_TEAM_STOP = 175;
static const Int AT_TEAM_STOP_AND_DISBAND = 176;
static const Int AT_RECRUIT_TEAM = 177;
static const Int AT_TEAM_SET_OVERRIDE_RELATION_TO_TEAM = 178;
static const Int AT_TEAM_REMOVE_OVERRIDE_RELATION_TO_TEAM = 179;
static const Int AT_TEAM_REMOVE_ALL_OVERRIDE_RELATIONS = 180;
static const Int AT_CAMERA_LOOK_TOWARD_OBJECT = 181;
static const Int AT_NAMED_FIRE_WEAPON_FOLLOWING_WAYPOINT_PATH = 182;
static const Int AT_TEAM_SET_OVERRIDE_RELATION_TO_PLAYER = 183;
static const Int AT_TEAM_REMOVE_OVERRIDE_RELATION_TO_PLAYER = 184;
static const Int AT_PLAYER_SET_OVERRIDE_RELATION_TO_TEAM = 185;
static const Int AT_PLAYER_REMOVE_OVERRIDE_RELATION_TO_TEAM = 186;
static const Int AT_UNIT_EXECUTE_SEQUENTIAL_SCRIPT = 187;
static const Int AT_UNIT_EXECUTE_SEQUENTIAL_SCRIPT_LOOPING = 188;
static const Int AT_UNIT_STOP_SEQUENTIAL_SCRIPT = 189;
static const Int AT_TEAM_EXECUTE_SEQUENTIAL_SCRIPT = 190;
static const Int AT_TEAM_EXECUTE_SEQUENTIAL_SCRIPT_LOOPING = 191;
static const Int AT_TEAM_STOP_SEQUENTIAL_SCRIPT = 192;
static const Int AT_UNIT_GUARD_FOR_FRAMECOUNT = 193;
static const Int AT_UNIT_IDLE_FOR_FRAMECOUNT = 194;
static const Int AT_TEAM_GUARD_FOR_FRAMECOUNT = 195;
static const Int AT_TEAM_IDLE_FOR_FRAMECOUNT = 196;
static const Int AT_WATER_CHANGE_HEIGHT = 197;
static const Int AT_NAMED_USE_COMMANDBUTTON_ABILITY_ON_NAMED = 198;
static const Int AT_NAMED_USE_COMMANDBUTTON_ABILITY_AT_WAYPOINT = 199;
static const Int AT_WATER_CHANGE_HEIGHT_OVER_TIME = 200;
static const Int AT_MAP_SWITCH_BORDER = 201;
static const Int AT_TEAM_GUARD_POSITION = 202;
static const Int AT_TEAM_GUARD_OBJECT = 203;
static const Int AT_TEAM_GUARD_AREA = 204;
static const Int AT_OBJECT_FORCE_SELECT = 205;
static const Int AT_CAMERA_LOOK_TOWARD_WAYPOINT = 206;
static const Int AT_UNIT_DESTROY_ALL_CONTAINED = 207;
static const Int AT_RADAR_FORCE_ENABLE = 208;
static const Int AT_RADAR_REVERT_TO_NORMAL = 209;
static const Int AT_SCREEN_SHAKE = 210;
static const Int AT_TECHTREE_MODIFY_BUILDABILITY_OBJECT = 211;
static const Int AT_WAREHOUSE_SET_VALUE = 212;
static const Int AT_OBJECT_CREATE_RADAR_EVENT = 213;
static const Int AT_TEAM_CREATE_RADAR_EVENT = 214;
static const Int AT_DISPLAY_CINEMATIC_TEXT = 215;
static const Int AT_DEBUG_CRASH_BOX = 216;
static const Int AT_SOUND_DISABLE_TYPE = 217;
static const Int AT_SOUND_ENABLE_TYPE = 218;
static const Int AT_SOUND_ENABLE_ALL = 219;
static const Int AT_AUDIO_OVERRIDE_VOLUME_TYPE = 220;
static const Int AT_AUDIO_RESTORE_VOLUME_TYPE = 221;
static const Int AT_AUDIO_RESTORE_VOLUME_ALL_TYPE = 222;
static const Int AT_INGAME_POPUP_MESSAGE = 223;
static const Int AT_SET_CAVE_INDEX = 224;
static const Int AT_NAMED_SET_HELD = 225;
static const Int AT_NAMED_SET_TOPPLE_DIRECTION = 226;
static const Int AT_UNIT_MOVE_TOWARDS_NEAREST_OBJECT_TYPE = 227;
static const Int AT_TEAM_MOVE_TOWARDS_NEAREST_OBJECT_TYPE = 228;
static const Int AT_MAP_REVEAL_ALL_PERM = 229;
static const Int AT_MAP_REVEAL_ALL_UNDO_PERM = 230;
static const Int AT_NAMED_SET_REPULSOR = 231;
static const Int AT_TEAM_SET_REPULSOR = 232;
static const Int AT_TEAM_WANDER_IN_PLACE = 233;
static const Int AT_TEAM_INCREASE_PRIORITY = 234;
static const Int AT_TEAM_DECREASE_PRIORITY = 235;
static const Int AT_DISPLAY_COUNTER = 236;
static const Int AT_HIDE_COUNTER = 237;
static const Int AT_TEAM_USE_COMMANDBUTTON_ABILITY_ON_NAMED = 238;
static const Int AT_TEAM_USE_COMMANDBUTTON_ABILITY_AT_WAYPOINT = 239;
static const Int AT_NAMED_USE_COMMANDBUTTON_ABILITY = 240;
static const Int AT_TEAM_USE_COMMANDBUTTON_ABILITY = 241;
static const Int AT_NAMED_FLASH_WHITE = 242;
static const Int AT_TEAM_FLASH_WHITE = 243;
static const Int AT_SKIRMISH_BUILD_BUILDING = 244;
static const Int AT_SKIRMISH_FOLLOW_APPROACH_PATH = 245;
static const Int AT_IDLE_ALL_UNITS = 246;
static const Int AT_RESUME_SUPPLY_TRUCKING = 247;
static const Int AT_NAMED_CUSTOM_COLOR = 248;
static const Int AT_SKIRMISH_MOVE_TO_APPROACH_PATH = 249;
static const Int AT_SKIRMISH_BUILD_BASE_DEFENSE_FRONT = 250;
static const Int AT_SKIRMISH_FIRE_SPECIAL_POWER_AT_MOST_COST = 251;
static const Int AT_NAMED_RECEIVE_UPGRADE = 252;
static const Int AT_PLAYER_REPAIR_NAMED_STRUCTURE = 253;
static const Int AT_SKIRMISH_BUILD_BASE_DEFENSE_FLANK = 254;
static const Int AT_SKIRMISH_BUILD_STRUCTURE_FRONT = 255;
static const Int AT_SKIRMISH_BUILD_STRUCTURE_FLANK = 256;
static const Int AT_SKIRMISH_ATTACK_NEAREST_GROUP_WITH_VALUE = 257;
static const Int AT_SKIRMISH_PERFORM_COMMANDBUTTON_ON_MOST_VALUABLE_OBJECT = 258;
static const Int AT_SKIRMISH_WAIT_FOR_COMMANDBUTTON_AVAILABLE_ALL = 259;
static const Int AT_SKIRMISH_WAIT_FOR_COMMANDBUTTON_AVAILABLE_PARTIAL = 260;
static const Int AT_TEAM_SPIN_FOR_FRAMECOUNT = 261;
static const Int AT_TEAM_ALL_USE_COMMANDBUTTON_ON_NAMED = 262;
static const Int AT_TEAM_ALL_USE_COMMANDBUTTON_ON_NEAREST_ENEMY_UNIT = 263;
static const Int AT_TEAM_ALL_USE_COMMANDBUTTON_ON_NEAREST_GARRISONED_BUILDING = 264;
static const Int AT_TEAM_ALL_USE_COMMANDBUTTON_ON_NEAREST_KINDOF = 265;
static const Int AT_TEAM_ALL_USE_COMMANDBUTTON_ON_NEAREST_ENEMY_BUILDING = 266;
static const Int AT_TEAM_ALL_USE_COMMANDBUTTON_ON_NEAREST_ENEMY_BUILDING_CLASS = 267;
static const Int AT_TEAM_ALL_USE_COMMANDBUTTON_ON_NEAREST_OBJECTTYPE = 268;
static const Int AT_TEAM_PARTIAL_USE_COMMANDBUTTON = 269;
static const Int AT_TEAM_CAPTURE_NEAREST_UNOWNED_FACTION_UNIT = 270;
static const Int AT_PLAYER_CREATE_TEAM_FROM_CAPTURED_UNITS = 271;
static const Int AT_PLAYER_ADD_SKILLPOINTS = 272;
static const Int AT_PLAYER_ADD_RANKLEVEL = 273;
static const Int AT_PLAYER_SET_RANKLEVEL = 274;
static const Int AT_PLAYER_SET_RANKLEVELLIMIT = 275;
static const Int AT_PLAYER_GRANT_SCIENCE = 276;
static const Int AT_PLAYER_PURCHASE_SCIENCE = 277;
static const Int AT_TEAM_HUNT_WITH_COMMAND_BUTTON = 278;
static const Int AT_TEAM_WAIT_FOR_NOT_CONTAINED_ALL = 279;
static const Int AT_TEAM_WAIT_FOR_NOT_CONTAINED_PARTIAL = 280;
static const Int AT_TEAM_FOLLOW_WAYPOINTS_EXACT = 281;
static const Int AT_NAMED_FOLLOW_WAYPOINTS_EXACT = 282;
static const Int AT_TEAM_SET_EMOTICON = 283;
static const Int AT_NAMED_SET_EMOTICON = 284;
static const Int AT_AI_PLAYER_BUILD_SUPPLY_CENTER = 285;
static const Int AT_AI_PLAYER_BUILD_UPGRADE = 286;
static const Int AT_OBJECTLIST_ADDOBJECTTYPE = 287;
static const Int AT_OBJECTLIST_REMOVEOBJECTTYPE = 288;
static const Int AT_MAP_REVEAL_PERMANENTLY_AT_WAYPOINT = 289;
static const Int AT_MAP_UNDO_REVEAL_PERMANENTLY_AT_WAYPOINT = 290;
static const Int AT_NAMED_SET_STEALTH_ENABLED = 291;
static const Int AT_TEAM_SET_STEALTH_ENABLED = 292;
static const Int AT_EVA_SET_ENABLED_DISABLED = 293;
static const Int AT_OPTIONS_SET_OCCLUSION_MODE = 294;
static const Int AT_LOCALDEFEAT = 295;
static const Int AT_OPTIONS_SET_DRAWICON_UI_MODE = 296;
static const Int AT_OPTIONS_SET_PARTICLE_CAP_MODE = 297;
static const Int AT_PLAYER_SCIENCE_AVAILABILITY = 298;
static const Int AT_UNIT_AFFECT_OBJECT_PANEL_FLAGS = 299;
static const Int AT_TEAM_AFFECT_OBJECT_PANEL_FLAGS = 300;
static const Int AT_PLAYER_SELECT_SKILLSET = 301;
static const Int AT_SCRIPTING_OVERRIDE_HULK_LIFETIME = 302;
static const Int AT_NAMED_FACE_NAMED = 303;
static const Int AT_NAMED_FACE_WAYPOINT = 304;
static const Int AT_TEAM_FACE_NAMED = 305;
static const Int AT_TEAM_FACE_WAYPOINT = 306;
static const Int AT_COMMANDBAR_REMOVE_BUTTON_OBJECTTYPE = 307;
static const Int AT_COMMANDBAR_ADD_BUTTON_OBJECTTYPE_SLOT = 308;
static const Int AT_UNIT_SPAWN_NAMED_LOCATION_ORIENTATION = 309;
static const Int AT_PLAYER_AFFECT_RECEIVING_EXPERIENCE = 310;
static const Int AT_PLAYER_EXCLUDE_FROM_SCORE_SCREEN = 311;
static const Int AT_TEAM_GUARD_SUPPLY_CENTER = 312;
static const Int AT_ENABLE_SCORING = 313;
static const Int AT_DISABLE_SCORING = 314;
static const Int AT_SOUND_SET_VOLUME = 315;
static const Int AT_SPEECH_SET_VOLUME = 316;
static const Int AT_DISABLE_BORDER_SHROUD = 317;
static const Int AT_ENABLE_BORDER_SHROUD = 318;
static const Int AT_OBJECT_ALLOW_BONUSES = 319;
static const Int AT_SOUND_REMOVE_ALL_DISABLED = 320;
static const Int AT_SOUND_REMOVE_TYPE = 321;
static const Int AT_TEAM_GUARD_IN_TUNNEL_NETWORK = 322;
static const Int AT_QUICKVICTORY = 323;
static const Int AT_SET_INFANTRY_LIGHTING_OVERRIDE = 324;
static const Int AT_RESET_INFANTRY_LIGHTING_OVERRIDE = 325;
static const Int AT_TEAM_DELETE_LIVING = 326;
static const Int AT_RESIZE_VIEW_GUARDBAND = 327;
static const Int AT_DELETE_ALL_UNMANNED = 328;
static const Int AT_CHOOSE_VICTIM_ALWAYS_USES_NORMAL = 329;
static const Int AT_CAMERA_ENABLE_SLAVE_MODE = 330;
static const Int AT_CAMERA_DISABLE_SLAVE_MODE = 331;
static const Int AT_CAMERA_ADD_SHAKER_AT = 332;
static const Int AT_SET_TRAIN_HELD = 333;
static const Int AT_NAMED_SET_EVAC_LEFT_OR_RIGHT = 334;
static const Int AT_ENABLE_OBJECT_SOUND = 335;
static const Int AT_DISABLE_OBJECT_SOUND = 336;
static const Int AT_NAMED_USE_COMMANDBUTTON_ABILITY_USING_WAYPOINT_PATH = 337;
static const Int AT_NAMED_SET_UNMANNED_STATUS = 338;
static const Int AT_TEAM_SET_UNMANNED_STATUS = 339;
static const Int AT_NAMED_SET_BOOBYTRAPPED = 340;
static const Int AT_TEAM_SET_BOOBYTRAPPED = 341;
static const Int AT_SHOW_WEATHER = 342;
static const Int AT_AI_PLAYER_BUILD_TYPE_NEAREST_TEAM = 343;
static const Int AT_NUM_ITEMS = 344;

// ---------------------------------------------------------------------------
// the script-engine load path (Scripts.cpp + ScriptEngine.cpp's registry)
// ---------------------------------------------------------------------------

// Scripts.cpp's own version words (the anonymous enum, statically there).
// Note K_SCRIPTS_DATA_VERSION_1 inherits 5 (the enumeration continues
// after K_SCRIPT_CONDITION_VERSION_4) — the version the writer puts on the
// PlayerScriptsList chunk; no reader of that chunk consults it.
#define K_SCRIPT_LIST_DATA_VERSION_1 1
#define K_SCRIPT_GROUP_DATA_VERSION_1 1
#define K_SCRIPT_GROUP_DATA_VERSION_2 2
#define K_SCRIPT_DATA_VERSION_1 1
#define K_SCRIPT_DATA_VERSION_2 2
#define K_SCRIPT_OR_CONDITION_DATA_VERSION_1 1
#define K_SCRIPT_ACTION_VERSION_1 1
#define K_SCRIPT_ACTION_VERSION_2 2
#define K_SCRIPT_CONDITION_VERSION_1 1
#define K_SCRIPT_CONDITION_VERSION_2 2
#define K_SCRIPT_CONDITION_VERSION_3 3
#define K_SCRIPT_CONDITION_VERSION_4 4
#define K_SCRIPTS_DATA_VERSION_1 5

enum { AT_END = 0x00FFFFFF };
#define MAX_PARMS 12          // Scripts.h
#define MAX_PLAYER_COUNT 16   // GameCommon.h
#define THIS_PLAYER "<This Player>"  // Scripts.h

// the bit-name table sizes (the arrays above)
#define KINDOF_NAMES_COUNT ((Int)(sizeof(KINDOF_NAMES) / sizeof(KINDOF_NAMES[0]) - 1))
#define OBJECT_STATUS_NAMES_COUNT ((Int)(sizeof(OBJECT_STATUS_NAMES) / sizeof(OBJECT_STATUS_NAMES[0])))

// The seven conditions whose parameter list changed for version 2
// (ParameterChangesVer2, Scripts.cpp) — a version-1 chunk gets the
// SURFACES_ALLOWED third parameter appended and the count forced to 3.
static const Int PARAMETER_CHANGES_VER2[] = {
        CT_TEAM_INSIDE_AREA_PARTIALLY,
        CT_TEAM_INSIDE_AREA_ENTIRELY,
        CT_TEAM_OUTSIDE_AREA_ENTIRELY,
        CT_TEAM_ENTERED_AREA_PARTIALLY,
        CT_TEAM_ENTERED_AREA_ENTIRELY,
        CT_TEAM_EXITED_AREA_ENTIRELY,
        CT_TEAM_EXITED_AREA_PARTIALLY,
        (Int)-1,
};

// The verdict markers (the crash/refusal contracts — see the header).
struct KindOfUnknown {};   // ReadParameter's unknown-kindof throw (ERROR_BUG)
struct ParmStoreOob {};    // a parameter store past m_parms[MAX_PARMS]
struct HealOob {};         // the version-1 append heal's bad m_parms index
struct KindOfOob {};       // kindofNames[m_int] out of range
struct NullScriptAdd {};   // addScript(nullptr) — documented unreachable

// BSD strlcat, absent from glibc (the same transcription gen_ini_golden
// carries; the original's Dependencies/Utility strlcat).
static size_t strlcpy_local(char *dst, const char *src, size_t siz)
{
        const char *s = src;
        size_t n = siz;
        while (n > 1 && *s) {
                *dst++ = *s++;
                --n;
        }
        if (n > 0)
                *dst = 0;
        return strlen(src);
}
static size_t strlcat_local(char *dst, const char *src, size_t siz)
{
        size_t dlen = strnlen(dst, siz);
        if (dlen == siz)
                return dlen + strlen(src);
        return dlen + strlcpy_local(dst + dlen, src, siz - dlen);
}

// _stricmp (compareNoCase's implementation, AsciiString.h).
static int compare_no_case(const std::string &a, const char *b)
{
        const unsigned char *pa = (const unsigned char *)a.c_str();
        const unsigned char *pb = (const unsigned char *)b;
        while (*pa && *pb) {
                unsigned char ca = *pa, cb = *pb;
                if (ca >= 'A' && ca <= 'Z') ca += 'a' - 'A';
                if (cb >= 'A' && cb <= 'Z') cb += 'a' - 'A';
                if (ca != cb) return (int)ca - (int)cb;
                ++pa; ++pb;
        }
        unsigned char ca = *pa, cb = *pb;
        if (ca >= 'A' && ca <= 'Z') ca += 'a' - 'A';
        if (cb >= 'A' && cb <= 'Z') cb += 'a' - 'A';
        return (int)ca - (int)cb;
}

// AsciiString::startsWith — a plain prefix compare (the NUL-truncated
// stand-in makes it a std::string prefix check).
static bool startsWith(const std::string &s, const char *prefix)
{
        size_t n = strlen(prefix);
        return s.size() >= n && s.compare(0, n, prefix) == 0;
}

// --- Parameter (the load shape) ----------------------------------------------

struct ParameterLoad {
        Int m_paramType;
        Bool m_initialized;
        Int m_int;
        Real m_real;
        AsciiString m_string;
        Real m_coordX, m_coordY, m_coordZ;   // Coord3D m_coord
        // ObjectStatusMaskType: a BitFlags<45> over std::bitset<45> — the
        // 45-bit mask (DEPLOYED is bit 44), not a 32-bit word.
        std::uint64_t m_objectStatus;

        // Parameter(ParameterType type, int val = 0) — the original's member
        // order (m_paramType, m_initialized, m_int, ...) kept for the shape;
        // the init list below follows it.
        explicit ParameterLoad(Int type, Int val = 0)
                : m_paramType(type), m_initialized(false), m_int(val), m_real(0),
                  m_coordX(0), m_coordY(0), m_coordZ(0), m_objectStatus(0)
        {
        }

        Int getParameterType() const { return m_paramType; }
        Int getInt() const { return m_int; }
        void setInt(Int i) { m_int = i; }
        void setStatus(std::uint64_t mask) { m_objectStatus |= mask; }  // set() ORs

        void setCoord3D(Real x, Real y, Real z)
        {
                // DEBUG_ASSERTCRASH(m_paramType==COORD3D) is a no-op here;
                // setCoord3D only assigns when the type matches.
                if (m_paramType == PT_COORD3D) {
                        m_coordX = x; m_coordY = y; m_coordZ = z;
                }
        }
};

// --- the parameter store (the fixed m_parms[MAX_PARMS] array) ----------------

// The original stores newInstance(Parameter) pointers into
// m_parms[MAX_PARMS]; an index outside [0, MAX_PARMS) writes past the
// array (the members beyond it, then the heap) — the store below checks
// the index and refuses with the verdict instead. The parameter's bytes
// are consumed first, matching the original's read-then-store order.
template <typename T>
static void parm_store(std::array<T, MAX_PARMS> &arr, Int i, T p)
{
        if (i < 0 || i >= MAX_PARMS)
                throw ParmStoreOob();
        arr[i] = std::move(p);
}

// --- the load shapes ----------------------------------------------------------

struct ConditionLoad {
        Int m_conditionType = CT_CONDITION_FALSE;
        Int m_numParms = 0;
        Bool m_hasWarnings = false;  // (runtime-only; kept for the shape)
        std::array<std::unique_ptr<ParameterLoad>, MAX_PARMS> m_parms;
};

struct OrConditionLoad {
        std::vector<std::unique_ptr<ConditionLoad>> m_ands;  // m_firstAnd chain
};

struct ScriptActionLoad {
        Int m_actionType = AT_NO_OP;
        Int m_numParms = 0;
        Bool m_hasWarnings = false;
        std::array<std::unique_ptr<ParameterLoad>, MAX_PARMS> m_parms;
};

struct ScriptLoad {
        AsciiString m_scriptName, m_comment, m_conditionComment, m_actionComment;
        Int m_delayEvaluationSeconds = 0;
        Bool m_isActive = true, m_isOneShot = true, m_isSubroutine = false;
        Bool m_easy = true, m_normal = true, m_hard = true;
        std::vector<std::unique_ptr<OrConditionLoad>> m_ors;       // m_condition
        std::vector<std::unique_ptr<ScriptActionLoad>> m_actions;  // m_action
        std::vector<std::unique_ptr<ScriptActionLoad>> m_falseActions;  // m_actionFalse
};

struct ScriptGroupLoad {
        AsciiString m_groupName;
        Bool m_isGroupActive = true;
        Bool m_isGroupSubroutine = false;
        std::vector<std::unique_ptr<ScriptLoad>> m_scripts;  // m_firstScript
};

struct ScriptListLoad {
        std::vector<std::unique_ptr<ScriptLoad>> m_scripts;      // m_firstScript
        std::vector<std::unique_ptr<ScriptGroupLoad>> m_groups;  // m_firstGroup
};

// ScriptList::addScript(pScr, AT_END) — the AT_END walk lands at the tail;
// the null script is the documented-unreachable crash (ParseScript's
// failure path; no parser in the tree returns false but the list bound,
// which aborts the walk before this add).
static void list_add_script(ScriptListLoad &list, std::unique_ptr<ScriptLoad> script)
{
        if (!script)
                throw NullScriptAdd();
        list.m_scripts.push_back(std::move(script));
}

static void group_add_script(ScriptGroupLoad &group, std::unique_ptr<ScriptLoad> script)
{
        if (!script)
                throw NullScriptAdd();
        group.m_scripts.push_back(std::move(script));
}

// --- the template registry (ScriptEngine::init) -------------------------------

struct ScriptEngineLoad {
        NameKeyGenerator m_keys;                  // TheNameKeyGenerator
        std::vector<NameKeyType> m_condKeys;      // m_conditionTemplates[].m_internalNameKey
        std::vector<NameKeyType> m_actKeys;       // m_actionTemplates[].m_internalNameKey

        // ScriptEngine::init()'s closing loops: every template's internal
        // name interned, conditions first then actions, in enum order. The
        // defunct entries' empty names intern the empty string once.
        void init()
        {
                m_condKeys.resize(CT_NUM_ITEMS);
                for (Int i = 0; i < CT_NUM_ITEMS; i++)
                        m_condKeys[i] = m_keys.nameToKey(AsciiString(COND_TEMPLATES[i].internal));
                m_actKeys.resize(AT_NUM_ITEMS);
                for (Int i = 0; i < AT_NUM_ITEMS; i++)
                        m_actKeys[i] = m_keys.nameToKey(AsciiString(ACT_TEMPLATES[i].internal));
        }

        // getConditionTemplate: the out-of-range clamp to index 0.
        Int cond_index(Int ndx) const
        {
                if (ndx < 0 || ndx >= CT_NUM_ITEMS) return 0;
                return ndx;
        }
        Int act_index(Int ndx) const
        {
                if (ndx < 0 || ndx >= AT_NUM_ITEMS) return 0;
                return ndx;
        }
        NameKeyType conditionTemplateKey(Int ndx) const { return m_condKeys[cond_index(ndx)]; }
        Int conditionTemplateParams(Int ndx) const { return COND_TEMPLATES[cond_index(ndx)].num; }
        NameKeyType actionTemplateKey(Int ndx) const { return m_actKeys[act_index(ndx)]; }
        Int actionTemplateParams(Int ndx) const { return ACT_TEMPLATES[act_index(ndx)].num; }
};

// --- Parameter::ReadParameter --------------------------------------------------

static std::unique_ptr<ParameterLoad> read_parameter(DataChunkInput &file, ScriptEngineLoad &eng)
{
        (void)eng;
        std::unique_ptr<ParameterLoad> pParm(new ParameterLoad(file.readInt()));
        pParm->m_initialized = true;
        if (pParm->getParameterType() == PT_COORD3D) {
                Real x = file.readReal();
                Real y = file.readReal();
                Real z = file.readReal();
                pParm->setCoord3D(x, y, z);
        } else {
                pParm->m_int = file.readInt();
                pParm->m_real = file.readReal();
                pParm->m_string = file.readAsciiString();
        }

        if (pParm->getParameterType() == PT_OBJECT_TYPE) {
                // quick hack to make loading models with "Fundamentalist"
                // switch to "GLA"
                if (startsWith(pParm->m_string.s, "Fundamentalist")) {
                        const char *replacePrefix = "Fundamentalist";
                        const size_t offset = strlen(replacePrefix);
                        char newName[256];
                        strcpy(newName, "GLA");
                        strlcat_local(newName, pParm->m_string.s.c_str() + offset, sizeof(newName));
                        pParm->m_string = AsciiString(newName);
                }
        }

        if (pParm->getParameterType() == PT_UPGRADE) {
                // quick hack to make obsolete capture building upgrades
                // switch to the new one. jba.
                if (pParm->m_string.s == "Upgrade_AmericaRangerCaptureBuilding" ||
                        pParm->m_string.s == "Upgrade_ChinaRedguardCaptureBuilding" ||
                        pParm->m_string.s == "Upgrade_GLARebelCaptureBuilding") {
                        pParm->m_string = AsciiString("Upgrade_InfantryCaptureBuilding");
                }
        }

        if (pParm->getParameterType() == PT_OBJECT_STATUS) {
                // Need to change the string to an ObjectStatusMaskType
                for (int i = 0; i < (int)OBJECT_STATUS_NAMES_COUNT; ++i) {
                        if (compare_no_case(pParm->m_string.s, OBJECT_STATUS_NAMES[i]) == 0) {
                                pParm->setStatus(std::uint64_t(1) << i);  // MAKE_OBJECT_STATUS_MASK(i), set() ORs
                                break;
                        }
                }
        }

        if (pParm->getParameterType() == PT_KIND_OF_PARAM) {
                // Need to change the string to an integer
                if (!pParm->m_string.s.empty()) {
                        Bool found = false;
                        for (int i = 0; KINDOF_NAMES[i]; ++i) {
                                if (compare_no_case(pParm->m_string.s, KINDOF_NAMES[i]) == 0) {
                                        pParm->setInt(i);
                                        found = true;
                                        break;
                                }
                                if (compare_no_case(pParm->m_string.s, "CRUSHER") == 0) {
                                        // "switched to OBSTACLE": the CURRENT loop
                                        // index — the match happens at i=0 (the
                                        // first else-if pass), which is OBSTACLE.
                                        pParm->setInt(i);
                                        found = true;
                                        break;
                                } else if (compare_no_case(pParm->m_string.s, "CRUSHABLE") == 0) {
                                        pParm->setInt(i);
                                        found = true;
                                        break;
                                } else if (compare_no_case(pParm->m_string.s, "OVERLAPPABLE") == 0) {
                                        pParm->setInt(i);
                                        found = true;
                                        break;
                                } else if (compare_no_case(pParm->m_string.s, "MISSILE") == 0) {
                                        // MISSILE was split into SMALL_MISSILE and
                                        // BALLISTIC_MISSILE. The re-compare below
                                        // tests the (already rewritten) string
                                        // against the literal, so it is always
                                        // true at i=0; the outer loop then walks
                                        // on (the branch has no break) and the
                                        // real SMALL_MISSILE entry overwrites the
                                        // int with its index.
                                        pParm->m_string = AsciiString("SMALL_MISSILE");
                                        for (i = 0; KINDOF_NAMES[i]; ++i) {
                                                if (compare_no_case(pParm->m_string.s, "SMALL_MISSILE") == 0) {
                                                        pParm->setInt(i);
                                                        found = true;
                                                        break;
                                                }
                                        }
                                        // (DEBUG_CRASH("Unable to find Kindof
                                        // SMALL_MISSILE'") — debug-only; the
                                        // release walk continues)
                                }
                        }
                        if (!found) {
                                // throw ERROR_BUG — the walk aborts.
                                throw KindOfUnknown();
                        }
                } else {
                        // "so WB will load them into the proper format":
                        // m_string = kindofNames[m_int] — the out-of-range
                        // index reads past the array (the 117 slot is the
                        // null terminator, an AsciiString-from-null crash;
                        // beyond and below are raw OOB).
                        if (pParm->m_int < 0 || pParm->m_int >= (Int)KINDOF_NAMES_COUNT)
                                throw KindOfOob();  // past the array, or the null terminator slot
                        pParm->m_string = AsciiString(KINDOF_NAMES[pParm->m_int]);
                }
        }

        return pParm;
}

// --- Condition::ParseConditionDataChunk ----------------------------------------

static Bool parse_condition_data_chunk(DataChunkInput &file, DataChunkInfo *info, void *userData);

static void parse_condition_into(DataChunkInput &file, DataChunkInfo *info, void *userData,
                                  OrConditionLoad *pOr, ScriptEngineLoad &eng)
{
        (void)userData;  // (the OrCondition the container threads; pOr is it)
        std::unique_ptr<ConditionLoad> pCondition(new ConditionLoad());
        pCondition->m_conditionType = file.readInt();
        Int ct_index = eng.cond_index(pCondition->m_conditionType);
        Int ct_params = eng.conditionTemplateParams(pCondition->m_conditionType);
        NameKeyType ct_key = eng.conditionTemplateKey(pCondition->m_conditionType);
        if (info->version >= K_SCRIPT_CONDITION_VERSION_4) {
                NameKeyType key = file.readNameKey(eng.m_keys);
                Bool match = false;
                if (ct_key == key) {
                        match = true;  // All good. jba. [3/20/2003]
                }
                if (!match) {
                        // name and id don't match. Find the name [3/20/2003]
                        for (Int i = 0; i < CT_NUM_ITEMS; i++) {
                                // getConditionTemplate(i) — in range here
                                if (key == eng.conditionTemplateKey(i)) {
                                        match = true;
                                        pCondition->m_conditionType = i;
                                        ct_index = i;
                                        ct_key = eng.m_condKeys[i];
                                        ct_params = COND_TEMPLATES[i].num;
                                        break;
                                }
                        }
                }
                if (!match) {
                        // Invalid script [3/20/2003]
                        pCondition->m_conditionType = CT_CONDITION_FALSE;
                        // (m_numParms = 0 — overwritten by the read below)
                        // NOTE: the original's ct keeps pointing at the last
                        // template the rescan assigned (template NUM_ITEMS-1)
                        // — the count check below compares against THAT
                        // template's count; with numParms 0 already set, the
                        // reassignment to CONDITION_FALSE/0 is idempotent for
                        // every possible count. Modelled directly.
                        ct_index = CT_NUM_ITEMS - 1;
                        ct_key = eng.m_condKeys[ct_index];
                        ct_params = COND_TEMPLATES[ct_index].num;
                }
        }
        pCondition->m_numParms = file.readInt();
        for (Int i = 0; i < pCondition->m_numParms; i++) {
                std::unique_ptr<ParameterLoad> p = read_parameter(file, eng);
                parm_store(pCondition->m_parms, i, std::move(p));
        }

        if (file.getChunkVersion() < K_SCRIPT_CONDITION_VERSION_2) {
                for (int j = 0; PARAMETER_CHANGES_VER2[j] != -1; ++j) {
                        if (pCondition->m_conditionType == PARAMETER_CHANGES_VER2[j]) {
                                // m_parms[m_numParms] = new SURFACES_ALLOWED(3)
                                // — the append index is the loaded count: 12
                                // or negative writes outside the array.
                                Int slot = pCondition->m_numParms;
                                if (slot < 0 || slot >= MAX_PARMS)
                                        throw HealOob();
                                pCondition->m_parms[slot] =
                                        std::unique_ptr<ParameterLoad>(new ParameterLoad(PT_SURFACES_ALLOWED, 3));
                                pCondition->m_numParms = 3;
                        }
                }
        }
        // heal old files.
        if (pCondition->m_conditionType == CT_SKIRMISH_SPECIAL_POWER_READY) {
                if (pCondition->m_numParms == 1) {
                        pCondition->m_numParms = 2;
                        pCondition->m_parms[1] = std::move(pCondition->m_parms[0]);
                        std::unique_ptr<ParameterLoad> side(new ParameterLoad(PT_SIDE, 0));
                        side->m_string = AsciiString(THIS_PLAYER);  // friend_setString
                        pCondition->m_parms[0] = std::move(side);
                }
        }
        if (ct_params != pCondition->m_numParms) {
                // Invalid script [3/20/2003]
                pCondition->m_conditionType = CT_CONDITION_FALSE;
                pCondition->m_numParms = 0;
        }
        // the tail-append into the or-clause's and-chain
        pOr->m_ands.push_back(std::move(pCondition));
}

// --- OrCondition::ParseOrConditionDataChunk ------------------------------------

static Bool parse_or_condition_into(DataChunkInput &file, DataChunkInfo *info, void *userData,
                                    ScriptLoad *pScript, ScriptEngineLoad &eng)
{
        (void)userData;
        (void)eng;  // (the or-condition walk consults nothing but the stream)
        std::unique_ptr<OrConditionLoad> pOrCondition(new OrConditionLoad());
        // the tail-append into the script's or-chain
        pScript->m_ors.push_back(std::move(pOrCondition));
        OrConditionLoad *pOr = pScript->m_ors.back().get();
        file.registerParser("Condition", info->label, parse_condition_data_chunk);
        return file.parse(pOr);
}

// --- ScriptAction::ParseAction ---------------------------------------------------

static void parse_action_into(DataChunkInput &file, DataChunkInfo *info,
                              std::unique_ptr<ScriptActionLoad> &out, ScriptEngineLoad &eng)
{
        std::unique_ptr<ScriptActionLoad> pScriptAction(new ScriptActionLoad());

        pScriptAction->m_actionType = file.readInt();

        Int at_index = eng.act_index(pScriptAction->m_actionType);
        Int at_params = eng.actionTemplateParams(pScriptAction->m_actionType);
        NameKeyType at_key = eng.actionTemplateKey(pScriptAction->m_actionType);
        if (info->version >= K_SCRIPT_ACTION_VERSION_2) {
                NameKeyType key = file.readNameKey(eng.m_keys);
                Bool match = false;
                if (at_key == key) {
                        match = true;  // All good. jba. [3/20/2003]
                }
                if (!match) {
                        // name and id don't match. Find the name [3/20/2003]
                        for (Int i = 0; i < AT_NUM_ITEMS; i++) {
                                if (key == eng.actionTemplateKey(i)) {
                                        match = true;
                                        pScriptAction->m_actionType = i;
                                        at_index = i;
                                        at_key = eng.m_actKeys[i];
                                        at_params = ACT_TEMPLATES[i].num;
                                        break;
                                }
                        }
                        if (!match) {
                                // Invalid script [3/20/2003]
                                pScriptAction->m_actionType = AT_NO_OP;
                                // (m_numParms = 0 — overwritten by the read below)
                                at_index = AT_NUM_ITEMS - 1;
                                at_key = eng.m_actKeys[at_index];
                                at_params = ACT_TEMPLATES[at_index].num;
                        }
                }
        }
        pScriptAction->m_numParms = file.readInt();
        for (Int i = 0; i < pScriptAction->m_numParms; i++) {
                std::unique_ptr<ParameterLoad> p = read_parameter(file, eng);
                parm_store(pScriptAction->m_parms, i, std::move(p));
        }

        // heal old files.
        switch (pScriptAction->m_actionType) {
                case AT_SKIRMISH_FIRE_SPECIAL_POWER_AT_MOST_COST:
                        if (pScriptAction->m_numParms == 1) {
                                pScriptAction->m_numParms = 2;
                                pScriptAction->m_parms[1] = std::move(pScriptAction->m_parms[0]);
                                std::unique_ptr<ParameterLoad> side(new ParameterLoad(PT_SIDE, 0));
                                side->m_string = AsciiString(THIS_PLAYER);
                                pScriptAction->m_parms[0] = std::move(side);
                        }
                        break;
                case AT_TEAM_FOLLOW_WAYPOINTS:
                        if (pScriptAction->m_numParms == 2) {
                                pScriptAction->m_numParms = 3;
                                pScriptAction->m_parms[2] =
                                        std::unique_ptr<ParameterLoad>(new ParameterLoad(PT_BOOLEAN, 1));
                        }
                        break;
                case AT_SKIRMISH_BUILD_BASE_DEFENSE_FRONT:
                        if (pScriptAction->m_numParms == 1) {
                                Bool flank = pScriptAction->m_parms[0]->getInt() != 0;
                                pScriptAction->m_parms[0].reset();  // deleteInstance
                                pScriptAction->m_numParms = 0;
                                if (flank) pScriptAction->m_actionType = AT_SKIRMISH_BUILD_BASE_DEFENSE_FLANK;
                        }
                        break;
                case AT_NAMED_SET_ATTITUDE:
                case AT_TEAM_SET_ATTITUDE:
                        if (pScriptAction->m_numParms >= 2 &&
                                pScriptAction->m_parms[1]->getParameterType() == PT_INT) {
                                pScriptAction->m_parms[1] =
                                        std::unique_ptr<ParameterLoad>(new ParameterLoad(
                                                PT_AI_MOOD, pScriptAction->m_parms[1]->getInt()));
                        }
                        break;
                case AT_MAP_REVEAL_AT_WAYPOINT:
                case AT_MAP_SHROUD_AT_WAYPOINT:
                        if (pScriptAction->m_numParms == 2) {
                                pScriptAction->m_numParms = 3;
                                pScriptAction->m_parms[2] =
                                        std::unique_ptr<ParameterLoad>(new ParameterLoad(PT_SIDE, 0));
                        }
                        break;
                case AT_MAP_REVEAL_ALL:
                case AT_MAP_REVEAL_ALL_PERM:
                case AT_MAP_REVEAL_ALL_UNDO_PERM:
                case AT_MAP_SHROUD_ALL:
                        if (pScriptAction->m_numParms == 0) {
                                pScriptAction->m_numParms = 1;
                                pScriptAction->m_parms[0] =
                                        std::unique_ptr<ParameterLoad>(new ParameterLoad(PT_SIDE, 0));
                        }
                        break;
                case AT_SPEECH_PLAY:
                        if (pScriptAction->m_numParms == 1) {
                                pScriptAction->m_numParms = 2;
                                // Default it to TRUE, as per conversation with JohnL
                                pScriptAction->m_parms[1] =
                                        std::unique_ptr<ParameterLoad>(new ParameterLoad(PT_BOOLEAN, 1));
                        }
                        break;
                case AT_CAMERA_MOD_SET_FINAL_ZOOM:
                case AT_CAMERA_MOD_SET_FINAL_PITCH:
                        if (pScriptAction->m_numParms == 1) {
                                pScriptAction->m_numParms = 3;
                                pScriptAction->m_parms[1] =
                                        std::unique_ptr<ParameterLoad>(new ParameterLoad(PT_PERCENT, 0.0f));
                                pScriptAction->m_parms[2] =
                                        std::unique_ptr<ParameterLoad>(new ParameterLoad(PT_PERCENT, 0.0f));
                        }
                        break;
                case AT_MOVE_CAMERA_TO:
                case AT_MOVE_CAMERA_ALONG_WAYPOINT_PATH:
                case AT_CAMERA_LOOK_TOWARD_OBJECT:
                        if (pScriptAction->m_numParms == 3) {
                                pScriptAction->m_numParms = 5;
                                pScriptAction->m_parms[3] =
                                        std::unique_ptr<ParameterLoad>(new ParameterLoad(PT_REAL, 0.0f));
                                pScriptAction->m_parms[4] =
                                        std::unique_ptr<ParameterLoad>(new ParameterLoad(PT_REAL, 0.0f));
                        }
                        break;
                case AT_RESET_CAMERA:
                case AT_ZOOM_CAMERA:
                case AT_PITCH_CAMERA:
                case AT_ROTATE_CAMERA:
                        if (pScriptAction->m_numParms == 2) {
                                pScriptAction->m_numParms = 4;
                                pScriptAction->m_parms[2] =
                                        std::unique_ptr<ParameterLoad>(new ParameterLoad(PT_REAL, 0.0f));
                                pScriptAction->m_parms[3] =
                                        std::unique_ptr<ParameterLoad>(new ParameterLoad(PT_REAL, 0.0f));
                        }
                        break;
                case AT_CAMERA_LOOK_TOWARD_WAYPOINT:
                        if (pScriptAction->m_numParms == 2) {
                                pScriptAction->m_numParms = 5;
                                pScriptAction->m_parms[2] =
                                        std::unique_ptr<ParameterLoad>(new ParameterLoad(PT_REAL, 0.0f));
                                pScriptAction->m_parms[3] =
                                        std::unique_ptr<ParameterLoad>(new ParameterLoad(PT_REAL, 0.0f));
                                pScriptAction->m_parms[4] =
                                        std::unique_ptr<ParameterLoad>(new ParameterLoad(PT_BOOLEAN, 0));
                        } else if (pScriptAction->m_numParms == 4) {
                                pScriptAction->m_numParms = 5;
                                pScriptAction->m_parms[4] =
                                        std::unique_ptr<ParameterLoad>(new ParameterLoad(PT_BOOLEAN, 0));
                        }
                        break;
                default:
                        break;
        }

        if (at_params != pScriptAction->m_numParms) {
                // Invalid script [3/20/2003]
                pScriptAction->m_actionType = AT_NO_OP;
                pScriptAction->m_numParms = 0;
        }
        out = std::move(pScriptAction);
}

// --- Script::ParseScript ----------------------------------------------------------

static std::unique_ptr<ScriptLoad> parse_script(DataChunkInput &file, unsigned short version,
                                                ScriptEngineLoad &eng);

// --- the parser trampolines (the container's parser signatures) ------------------

struct PsState;  // the driver state (below)

static ScriptEngineLoad *g_eng = nullptr;  // TheScriptEngine stand-in for the
                                           // trampolines (the original reaches it
                                           // globally; the load is single-walk)

static Bool parse_condition_data_chunk(DataChunkInput &file, DataChunkInfo *info, void *userData)
{
        OrConditionLoad *pOr = (OrConditionLoad *)userData;
        parse_condition_into(file, info, userData, pOr, *g_eng);
        return true;
}

static Bool parse_or_condition_data_chunk(DataChunkInput &file, DataChunkInfo *info, void *userData)
{
        ScriptLoad *pScript = (ScriptLoad *)userData;
        return parse_or_condition_into(file, info, userData, pScript, *g_eng);
}

static Bool parse_action_data_chunk(DataChunkInput &file, DataChunkInfo *info, void *userData)
{
        ScriptLoad *pScript = (ScriptLoad *)userData;
        std::unique_ptr<ScriptActionLoad> act;
        parse_action_into(file, info, act, *g_eng);
        pScript->m_actions.push_back(std::move(act));  // the tail-append
        return true;
}

static Bool parse_action_false_data_chunk(DataChunkInput &file, DataChunkInfo *info, void *userData)
{
        ScriptLoad *pScript = (ScriptLoad *)userData;
        std::unique_ptr<ScriptActionLoad> act;
        parse_action_into(file, info, act, *g_eng);
        pScript->m_falseActions.push_back(std::move(act));  // the tail-append
        return true;
}

static std::unique_ptr<ScriptLoad> parse_script(DataChunkInput &file, unsigned short version,
                                                ScriptEngineLoad &eng)
{
        (void)eng;  // (the script head consults nothing but the stream)
        std::unique_ptr<ScriptLoad> pScript(new ScriptLoad());

        pScript->m_scriptName = file.readAsciiString();
        pScript->m_comment = file.readAsciiString();
        pScript->m_conditionComment = file.readAsciiString();
        pScript->m_actionComment = file.readAsciiString();

        pScript->m_isActive = file.readByte() ? true : false;
        pScript->m_isOneShot = file.readByte() ? true : false;
        pScript->m_easy = file.readByte() ? true : false;
        pScript->m_normal = file.readByte() ? true : false;
        pScript->m_hard = file.readByte() ? true : false;
        pScript->m_isSubroutine = file.readByte() ? true : false;
        if (version >= K_SCRIPT_DATA_VERSION_2) {
                pScript->m_delayEvaluationSeconds = file.readInt();
        }
        file.registerParser("OrCondition", "Script", parse_or_condition_data_chunk);
        file.registerParser("ScriptAction", "Script", parse_action_data_chunk);
        file.registerParser("ScriptActionFalse", "Script", parse_action_false_data_chunk);
        // (file.parse(pScript) — no parser in this sub-tree returns false;
        // the null return's addScript deref is the documented-unreachable
        // crash contract, see NullScriptAdd)
        file.parse(pScript.get());
        return pScript;
}

static Bool parse_script_from_list_data_chunk(DataChunkInput &file, DataChunkInfo *info, void *userData)
{
        ScriptListLoad *pList = (ScriptListLoad *)userData;
        std::unique_ptr<ScriptLoad> pScript = parse_script(file, info->version, *g_eng);
        list_add_script(*pList, std::move(pScript));
        return true;
}

static Bool parse_script_from_group_data_chunk(DataChunkInput &file, DataChunkInfo *info, void *userData)
{
        ScriptGroupLoad *pGroup = (ScriptGroupLoad *)userData;
        std::unique_ptr<ScriptLoad> pScript = parse_script(file, info->version, *g_eng);
        group_add_script(*pGroup, std::move(pScript));
        return true;
}

static Bool parse_group_data_chunk(DataChunkInput &file, DataChunkInfo *info, void *userData);

static Bool parse_group_data_chunk(DataChunkInput &file, DataChunkInfo *info, void *userData)
{
        ScriptListLoad *pList = (ScriptListLoad *)userData;
        std::unique_ptr<ScriptGroupLoad> pGroup(new ScriptGroupLoad());

        pGroup->m_groupName = file.readAsciiString();
        pGroup->m_isGroupActive = file.readByte() ? true : false;
        if (info->version == K_SCRIPT_GROUP_DATA_VERSION_2) {  // exact equality: a version 3 group reads like a 1
                pGroup->m_isGroupSubroutine = file.readByte() ? true : false;
        }
        pList->m_groups.push_back(std::move(pGroup));  // addGroup(pGroup, AT_END)
        file.registerParser("Script", info->label, parse_script_from_group_data_chunk);
        return file.parse(pList->m_groups.back().get());
}

// TScriptListReadInfo (ScriptEngine.h)
struct TScriptListReadInfo {
        Int numLists = 0;
        std::array<std::unique_ptr<ScriptListLoad>, MAX_PLAYER_COUNT> readLists;
};

static Bool parse_script_list_data_chunk(DataChunkInput &file, DataChunkInfo *info, void *userData)
{
        TScriptListReadInfo *pInfo = (TScriptListReadInfo *)userData;
        if (pInfo->numLists >= MAX_PLAYER_COUNT) return false;  // the bound refusal
        pInfo->readLists[pInfo->numLists] = std::unique_ptr<ScriptListLoad>(new ScriptListLoad());
        Int cur = pInfo->numLists;
        pInfo->numLists++;
        file.registerParser("Script", info->label, parse_script_from_list_data_chunk);
        file.registerParser("ScriptGroup", info->label, parse_group_data_chunk);
        return file.parse(pInfo->readLists[cur].get());
}

// PsState: the driver's ScriptList statics + the engine.
struct PsState {
        ScriptEngineLoad m_eng;
        Bool m_fired = false;

        // ScriptList's statics (s_readLists/s_numInReadList)
        Int s_numInReadList = 0;
        std::array<std::unique_ptr<ScriptListLoad>, MAX_PLAYER_COUNT> s_readLists;
};

// ScriptList::ParseScriptsDataChunk
static Bool parse_scripts_data_chunk(DataChunkInput &file, DataChunkInfo *info, void *userData)
{
        PsState *st = (PsState *)userData;
        st->m_fired = true;
        // (the statics' leftover clear: a previous PlayerScriptsList in the
        // same file left its lists; they are deleted here — last wins)
        for (Int i = 0; i < st->s_numInReadList; i++)
                st->s_readLists[i].reset();
        st->s_numInReadList = 0;
        TScriptListReadInfo readInfo;
        file.registerParser("ScriptList", info->label, parse_script_list_data_chunk);
        if (file.parse(&readInfo)) {
                // (the DEBUG_ASSERTCRASH(numLists<MAX_PLAYER_COUNT) fires at
                // exactly 16 — debug-only; the release copy below is in range)
                st->s_numInReadList = readInfo.numLists;
                for (Int i = 0; i < st->s_numInReadList; i++)
                        st->s_readLists[i] = std::move(readInfo.readLists[i]);
                return true;
        }
        return false;
}

// ScriptList::getReadScripts — the hand-out that empties the statics.
static Int get_read_scripts(PsState &st,
                            std::array<std::unique_ptr<ScriptListLoad>, MAX_PLAYER_COUNT> &out)
{
        Int count = st.s_numInReadList;
        st.s_numInReadList = 0;
        for (Int i = 0; i < count; i++)
                out[i] = std::move(st.s_readLists[i]);
        return count;
}

// The SidesList-context stub: ParseSidesDataChunk's registration tail (the
// sides payload before it is the sides-list slice's, transcribed in
// gen_mapsides_golden.cpp; the scripts read-back into the sides is dropped
// with it — getReadScripts' hand-out is dumped directly instead).
static Bool sides_stub(DataChunkInput &file, DataChunkInfo *info, void *userData)
{
        file.registerParser("PlayerScriptsList", info->label, parse_scripts_data_chunk);
        return file.parse(userData);
}

// ---------------------------------------------------------------------------
// the golden-script driver
// ---------------------------------------------------------------------------

static UnsignedInt f32bits(Real f)
{
        UnsignedInt u;
        memcpy(&u, &f, 4);
        return u;
}

static std::string hx(const std::vector<uint8_t> &b);

struct Out {
        FILE *f = nullptr;

        void line(const std::string &s) { fprintf(f, "%s\n", s.c_str()); }

        void file(const char *id, const std::vector<uint8_t> &b)
        {
                line(std::string("file ") + id + " " + hx(b));
        }

        void tf(const char *cmd, const char *id, Bool v)
        {
                line(std::string(cmd) + " " + id + (v ? " t" : " f"));
        }
};

static std::string hx(const std::vector<uint8_t> &b)
{
        if (b.empty())
                return "-";
        static const char *d = "0123456789abcdef";
        std::string out;
        out.reserve(b.size() * 2);
        for (uint8_t c : b) {
                out.push_back(d[c >> 4]);
                out.push_back(d[c & 15]);
        }
        return out;
}

// Names: '-' when empty, the raw bytes when printable non-space ASCII,
// <hex> otherwise.
static std::string q(const std::string &s)
{
        if (s.empty())
                return "-";
        bool printable = true;
        for (unsigned char c : s) {
                if (c < 0x21 || c > 0x7e)
                        printable = false;
        }
        if (printable)
                return s;
        std::string out = "<";
        for (unsigned char c : s) {
                char buf[3];
                snprintf(buf, sizeof buf, "%02x", c);
                out += buf;
        }
        out += ">";
        return out;
}

// one f32 word as %08x.
static std::string hexword(UnsignedInt bits)
{
        char buf[16];
        snprintf(buf, sizeof buf, "%08x", bits);
        return buf;
}

// the 45-bit object-status mask as %016x.
static std::string hexword64(std::uint64_t bits)
{
        char buf[24];
        snprintf(buf, sizeof buf, "%016llx", (unsigned long long)bits);
        return buf;
}

static std::string i2s(Int v)
{
        return std::to_string(v);
}

// --- byte builders -----------------------------------------------------------

static void put_u16(std::vector<uint8_t> &b, uint16_t v)
{
        b.push_back((uint8_t)(v & 0xff));
        b.push_back((uint8_t)(v >> 8));
}
static void put_u32(std::vector<uint8_t> &b, uint32_t v)
{
        b.push_back((uint8_t)(v & 0xff));
        b.push_back((uint8_t)((v >> 8) & 0xff));
        b.push_back((uint8_t)((v >> 16) & 0xff));
        b.push_back((uint8_t)((v >> 24) & 0xff));
}
static void put_i32(std::vector<uint8_t> &b, int32_t v) { put_u32(b, (uint32_t)v); }
static void put_f32(std::vector<uint8_t> &b, Real v) { put_u32(b, f32bits(v)); }
static void put_str(std::vector<uint8_t> &b, const std::string &s)
{
        put_u16(b, (uint16_t)s.size());
        b.insert(b.end(), s.begin(), s.end());
}
static void put_bytes(std::vector<uint8_t> &b, const std::vector<uint8_t> &s)
{
        b.insert(b.end(), s.begin(), s.end());
}

// The corpus TOC: labels interned to ids 1..n in first-use order (the
// writer's DataChunkOutput shape).
struct TocB {
        std::vector<std::pair<std::string, uint32_t>> entries;

        uint32_t id(const std::string &label)
        {
                for (auto &e : entries)
                        if (e.first == label)
                                return e.second;
                uint32_t nid = (uint32_t)(entries.size() + 1);
                entries.push_back(std::make_pair(label, nid));
                return nid;
        }
};

static void put_chunk(std::vector<uint8_t> &b, TocB &toc, const std::string &label,
                      uint16_t ver, const std::vector<uint8_t> &data)
{
        uint32_t id = toc.id(label);
        put_u32(b, id);
        put_u16(b, ver);
        put_i32(b, (int32_t)data.size());
        put_bytes(b, data);
}

static std::vector<uint8_t> toc_bytes(const TocB &toc)
{
        std::vector<uint8_t> b;
        b.insert(b.end(), { 'C', 'k', 'M', 'p' });
        put_i32(b, (int32_t)toc.entries.size());
        for (auto &e : toc.entries) {
                b.push_back((uint8_t)e.first.size());
                b.insert(b.end(), e.first.begin(), e.first.end());
                put_u32(b, e.second);
        }
        return b;
}

// --- payload builders ----------------------------------------------------------

// a parameter's bytes: the type word, then the COORD3D shape (three reals)
// or the int/real/string shape.
static void put_parm(std::vector<uint8_t> &b, Int type, Int iv, Real rv, const std::string &sv)
{
        put_i32(b, type);
        if (type == PT_COORD3D) {
                put_f32(b, (Real)iv);  // the corpus spells coords through iv
                put_f32(b, rv);
                put_f32(b, rv + 1.0f);
        } else {
                put_i32(b, iv);
                put_f32(b, rv);
                put_str(b, sv);
        }
}
static void put_parm_coord(std::vector<uint8_t> &b, Real x, Real y, Real z)
{
        put_i32(b, PT_COORD3D);
        put_f32(b, x);
        put_f32(b, y);
        put_f32(b, z);
}

// a condition's payload (the name key word is the TOC id << 8 | 3, the
// writer's DICT_ASCIISTRING shape; the reader ignores the low byte).
// key_toc_id_or_zero <= 0 writes no key word at all.
static void put_cond_head(std::vector<uint8_t> &b, Int type, Int key_toc_id_or_zero, Int nparms)
{
        put_i32(b, type);
        if (key_toc_id_or_zero > 0)
                put_i32(b, (key_toc_id_or_zero << 8) | 3);
        put_i32(b, nparms);
}

// a raw name-key word (the low byte and the sign are the corpus's to pick).
static void put_key_word(std::vector<uint8_t> &b, int32_t word) { put_i32(b, word); }

// a script's immediate payload.
static void put_script_head(std::vector<uint8_t> &b, const std::string &name,
                             const std::string &comment, const std::string &condcomment,
                             const std::string &actcomment, Int active, Int oneshot, Int easy,
                             Int normal, Int hard, Int sub, const Int *delay)
{
        put_str(b, name);
        put_str(b, comment);
        put_str(b, condcomment);
        put_str(b, actcomment);
        b.push_back((uint8_t)active);
        b.push_back((uint8_t)oneshot);
        b.push_back((uint8_t)easy);
        b.push_back((uint8_t)normal);
        b.push_back((uint8_t)hard);
        b.push_back((uint8_t)sub);
        if (delay)
                put_i32(b, *delay);
}

static void put_group_head(std::vector<uint8_t> &b, const std::string &name, Int active,
                           const Int *sub)
{
        put_str(b, name);
        b.push_back((uint8_t)active);
        if (sub)
                b.push_back((uint8_t)*sub);
}

// the byte-count of a script head at the given version (the corpora cut
// streams mid-head with it)


// --- the dump -------------------------------------------------------------------

static void dump_parm(Out &out, const char *id, const std::string &path, Int pi,
                      const ParameterLoad *p)
{
        if (!p) {
                out.line("pparm " + std::string(id) + " " + path + " " + i2s(pi) + " null");
                return;
        }
        std::string s = "pparm " + std::string(id) + " " + path + " " + i2s(pi) + " " +
                        i2s(p->m_paramType) + " " + i2s(p->m_int) + " " +
                        hexword(f32bits(p->m_real)) + " " + q(p->m_string.s);
        if (p->m_paramType == PT_COORD3D) {
                s += " " + hexword(f32bits(p->m_coordX)) + " " + hexword(f32bits(p->m_coordY)) +
                     " " + hexword(f32bits(p->m_coordZ));
        }
        if (p->m_paramType == PT_OBJECT_STATUS) {
                s += " " + hexword64(p->m_objectStatus);
        }
        out.line(s);
}

static void dump_parms(Out &out, const char *id, const std::string &path, Int n,
                       const std::array<std::unique_ptr<ParameterLoad>, MAX_PARMS> &parms)
{
        for (Int i = 0; i < n; i++)
                dump_parm(out, id, path + ".p" + i2s(i), i, parms[i].get());
}

static void dump_script(Out &out, const char *id, const std::string &path, const ScriptLoad &s)
{
        out.line("pscr " + std::string(id) + " " + path + " " + q(s.m_scriptName.s) + " " +
                 q(s.m_comment.s) + " " + q(s.m_conditionComment.s) + " " +
                 q(s.m_actionComment.s) + " " + (s.m_isActive ? "t" : "f") + " " +
                 (s.m_isOneShot ? "t" : "f") + " " + (s.m_easy ? "t" : "f") + " " +
                 (s.m_normal ? "t" : "f") + " " + (s.m_hard ? "t" : "f") + " " +
                 (s.m_isSubroutine ? "t" : "f") + " " + i2s(s.m_delayEvaluationSeconds));
        for (size_t o = 0; o < s.m_ors.size(); o++) {
                const OrConditionLoad &orc = *s.m_ors[o];
                for (size_t c = 0; c < orc.m_ands.size(); c++) {
                        const ConditionLoad &cond = *orc.m_ands[c];
                        std::string cpath = path + ".o" + i2s((Int)o) + ".c" + i2s((Int)c);
                        out.line("pcond " + std::string(id) + " " + cpath + " " +
                                 i2s(cond.m_conditionType) + " " + i2s(cond.m_numParms));
                        dump_parms(out, id, cpath, cond.m_numParms, cond.m_parms);
                }
        }
        for (size_t a = 0; a < s.m_actions.size(); a++) {
                const ScriptActionLoad &act = *s.m_actions[a];
                std::string apath = path + ".a" + i2s((Int)a);
                out.line("pact " + std::string(id) + " " + apath + " t " + i2s(act.m_actionType) +
                         " " + i2s(act.m_numParms));
                dump_parms(out, id, apath, act.m_numParms, act.m_parms);
        }
        for (size_t a = 0; a < s.m_falseActions.size(); a++) {
                const ScriptActionLoad &act = *s.m_falseActions[a];
                std::string apath = path + ".f" + i2s((Int)a);
                out.line("pact " + std::string(id) + " " + apath + " f " + i2s(act.m_actionType) +
                         " " + i2s(act.m_numParms));
                dump_parms(out, id, apath, act.m_numParms, act.m_parms);
        }
}

// the registration shape a corpus drives.
enum RegMode {
        REG_ROOT,    // GameLogic.cpp/.scb: PlayerScriptsList at the file root
        REG_NESTED,  // the map's SidesList tail: PlayerScriptsList under SidesList
        REG_NONE,    // nothing registered (the chunks skip whole)
};

struct RunOpts {
        RegMode mode = REG_ROOT;
        Bool dump_registry = false;  // the ptpl block (once, the registry corpus)
        Bool dump_keys = false;      // the full pkey table (the interning-sensitive corpora)
};

static void run_ps(Out &out, const char *id, const std::vector<uint8_t> &bytes, const RunOpts &opts)
{
        out.file(id, bytes);
        CachedFileInputStream s(bytes);
        DataChunkInput in(&s);
        out.tf("valid", id, in.isValidFileType());

        PsState st;
        st.m_eng.init();  // ScriptEngine::init()'s closing name-key loops
        g_eng = &st.m_eng;

        if (opts.mode == REG_ROOT) {
                in.registerParser("PlayerScriptsList", AsciiString(), parse_scripts_data_chunk);
        } else if (opts.mode == REG_NESTED) {
                in.registerParser("SidesList", AsciiString(), sides_stub);
        }

        Bool ok = true;
        const char *err = nullptr;
        try {
                ok = in.parse(&st);
        } catch (KindOfUnknown &) {
                ok = false;
                err = "kindof";
        } catch (ParmStoreOob &) {
                ok = false;
                err = "oob-parms";
        } catch (HealOob &) {
                ok = false;
                err = "oob-heal";
        } catch (KindOfOob &) {
                ok = false;
                err = "oob-kindof";
        } catch (NullScriptAdd &) {
                ok = false;
                err = "null-script";  // documented unreachable through this walk
        } catch (DictTypeCorrupt &) {
                ok = false;
                err = "corrupt";
        }

        out.line(std::string("walk ") + id + (ok ? " t" : " f"));
        if (!ok && err) {
                out.line(std::string("perr ") + id + " " + err);
        } else if (!ok) {
                // the container refused (no TOC) or the bound refusal walked
                // back out (the 17th ScriptList — the only false path)
                if (!in.isValidFileType())
                        out.line(std::string("perr ") + id + " corrupt");
                else
                        out.line(std::string("perr ") + id + " listbound");
        } else if (!st.m_fired) {
                out.line(std::string("pnone ") + id);
        } else {
                // ScriptList::getReadScripts
                std::array<std::unique_ptr<ScriptListLoad>, MAX_PLAYER_COUNT> lists;
                Int count = get_read_scripts(st, lists);
                out.line("pn " + std::string(id) + " " + i2s(count));
                for (Int i = 0; i < count; i++) {
                        const ScriptListLoad &L = *lists[i];
                        out.line("plist " + std::string(id) + " " + i2s(i) + " " +
                                 i2s((Int)L.m_scripts.size()) + " " + i2s((Int)L.m_groups.size()));
                        for (size_t si = 0; si < L.m_scripts.size(); si++)
                                dump_script(out, id, "L" + i2s(i) + ".s" + i2s((Int)si),
                                            *L.m_scripts[si]);
                        for (size_t gi = 0; gi < L.m_groups.size(); gi++) {
                                const ScriptGroupLoad &G = *L.m_groups[gi];
                                out.line("pgrp " + std::string(id) + " " + i2s(i) + " " +
                                         i2s((Int)gi) + " " + q(G.m_groupName.s) + " " +
                                         (G.m_isGroupActive ? "t" : "f") + " " +
                                         (G.m_isGroupSubroutine ? "t" : "f") + " " +
                                         i2s((Int)G.m_scripts.size()));
                                for (size_t si = 0; si < G.m_scripts.size(); si++)
                                        dump_script(out, id,
                                                    "L" + i2s(i) + ".g" + i2s((Int)gi) + ".s" +
                                                        i2s((Int)si),
                                                    *G.m_scripts[si]);
                        }
                }
        }

        if (opts.dump_registry) {
                for (Int i = 0; i < CT_NUM_ITEMS; i++)
                        out.line("ptpl c " + std::string(id) + " " + i2s(i) + " " +
                                 q(COND_TEMPLATES[i].internal) + " " +
                                 i2s(COND_TEMPLATES[i].num));
                for (Int i = 0; i < AT_NUM_ITEMS; i++)
                        out.line("ptpl a " + std::string(id) + " " + i2s(i) + " " +
                                 q(ACT_TEMPLATES[i].internal) + " " + i2s(ACT_TEMPLATES[i].num));
        }

        // the final name-key table: the count always, the full sequence for
        // the interning-sensitive corpora
        {
                const auto &names = st.m_eng.m_keys.names();
                out.line("pkeyn " + std::string(id) + " " + i2s((Int)names.size()));
                if (opts.dump_keys) {
                        for (size_t i = 0; i < names.size(); i++)
                                out.line("pkey " + std::string(id) + " " +
                                         i2s((Int)names[i].second) + " " + q(names[i].first.s));
                }
        }
        // the post-walk stream state
        out.line("pstate " + std::string(id) + " " + i2s((Int)in.tell()) + " " +
                 i2s(in.depth()) + " " + (in.atEndOfFile() ? "t" : "f"));
}

// ---------------------------------------------------------------------------
// the corpora
// ---------------------------------------------------------------------------

// A minimal modern file: one list, one script with nothing else — plus the
// ptpl registry dump and the full name-key table.
static void corpus_pscr_registry(Out &out)
{
        TocB toc;
        std::vector<uint8_t> sc;
        Int delay = 7;
        put_script_head(sc, "Minimal", "a comment", "", "", 1, 1, 1, 1, 1, 0, &delay);
        std::vector<uint8_t> sch;
        put_chunk(sch, toc, "Script", K_SCRIPT_DATA_VERSION_2, sc);
        std::vector<uint8_t> lp;
        put_bytes(lp, sch);
        std::vector<uint8_t> lch;
        put_chunk(lch, toc, "ScriptList", K_SCRIPT_LIST_DATA_VERSION_1, lp);
        std::vector<uint8_t> pp;
        put_bytes(pp, lch);
        std::vector<uint8_t> pch;
        put_chunk(pch, toc, "PlayerScriptsList", K_SCRIPTS_DATA_VERSION_1, pp);
        std::vector<uint8_t> b = toc_bytes(toc);
        put_bytes(b, pch);
        RunOpts o;
        o.dump_registry = true;
        o.dump_keys = true;
        run_ps(out, "pscr_registry", b, o);
}

// A PlayerScriptsList with no lists at all.
static void corpus_pscr_empty(Out &out)
{
        TocB toc;
        std::vector<uint8_t> pch;
        put_chunk(pch, toc, "PlayerScriptsList", K_SCRIPTS_DATA_VERSION_1, {});
        std::vector<uint8_t> b = toc_bytes(toc);
        put_bytes(b, pch);
        run_ps(out, "pscr_empty", b, RunOpts());
}

// The kitchen sink: two lists, flat scripts with multi-or/multi-and
// conditions, THEN and ELSE actions, a group with scripts, and every
// run-of-the-mill parameter shape. All chunks at their latest versions.
static void corpus_pscr_full(Out &out)
{
        TocB toc;
        Int delay = 30;

        // list 0, script 0: two or-clauses, the first with two ands
        std::vector<uint8_t> c0;
        put_cond_head(c0, CT_TIMER_EXPIRED, toc.id("TIMER_EXPIRED"), 1);
        put_parm(c0, PT_INT, 5, 1.5f, "");
        std::vector<uint8_t> c0ch;
        put_chunk(c0ch, toc, "Condition", K_SCRIPT_CONDITION_VERSION_4, c0);

        std::vector<uint8_t> c1;
        put_cond_head(c1, CT_UNIT_HEALTH, toc.id("UNIT_HEALTH"), 2);
        put_parm(c1, PT_UNIT, 0, 0.0f, "JarmenKell");
        put_parm(c1, PT_COMPARISON, 2, 0.0f, "");
        std::vector<uint8_t> c1ch;
        put_chunk(c1ch, toc, "Condition", K_SCRIPT_CONDITION_VERSION_4, c1);

        std::vector<uint8_t> o0;
        put_bytes(o0, c0ch);
        std::vector<uint8_t> o0ch;
        put_chunk(o0ch, toc, "OrCondition", K_SCRIPT_OR_CONDITION_DATA_VERSION_1, o0);

        std::vector<uint8_t> c2;
        put_cond_head(c2, CT_FLAG, toc.id("FLAG"), 2);
        put_parm(c2, PT_FLAG, 0, 0.0f, "FlagOne");
        put_parm(c2, PT_BOOLEAN, 1, 0.0f, "");
        std::vector<uint8_t> c2ch;
        put_chunk(c2ch, toc, "Condition", K_SCRIPT_CONDITION_VERSION_4, c2);

        std::vector<uint8_t> c3;
        put_cond_head(c3, CT_CONDITION_FALSE, toc.id("CONDITION_FALSE"), 0);
        std::vector<uint8_t> c3ch;
        put_chunk(c3ch, toc, "Condition", K_SCRIPT_CONDITION_VERSION_4, c3);

        std::vector<uint8_t> o1;
        put_bytes(o1, c2ch);
        put_bytes(o1, c3ch);
        std::vector<uint8_t> o1ch;
        put_chunk(o1ch, toc, "OrCondition", K_SCRIPT_OR_CONDITION_DATA_VERSION_1, o1);

        std::vector<uint8_t> a0;
        put_i32(a0, AT_SET_FLAG);
        put_key_word(a0, (toc.id("SET_FLAG") << 8) | 3);
        put_i32(a0, 2);
        put_parm(a0, PT_FLAG, 0, 0.0f, "FlagOne");
        put_parm(a0, PT_BOOLEAN, 0, 0.0f, "");
        std::vector<uint8_t> a0ch;
        put_chunk(a0ch, toc, "ScriptAction", K_SCRIPT_ACTION_VERSION_2, a0);

        std::vector<uint8_t> a1;
        put_i32(a1, AT_MOVE_CAMERA_TO);
        put_key_word(a1, (toc.id("MOVE_CAMERA_TO") << 8) | 3);
        put_i32(a1, 5);
        put_parm_coord(a1, 100.0f, -50.5f, 0.0f);
        put_parm(a1, PT_REAL, 0, 2.0f, "");
        put_parm(a1, PT_BOOLEAN, 1, 0.0f, "");
        put_parm(a1, PT_REAL, 0, 3.5f, "");
        put_parm(a1, PT_REAL, 0, 4.5f, "");
        std::vector<uint8_t> a1ch;
        put_chunk(a1ch, toc, "ScriptAction", K_SCRIPT_ACTION_VERSION_2, a1);

        std::vector<uint8_t> f0;
        put_i32(f0, AT_DEBUG_STRING);
        put_key_word(f0, (toc.id("DEBUG_STRING") << 8) | 3);
        put_i32(f0, 1);
        put_parm(f0, PT_TEXT_STRING, 0, 0.0f, "else happened");
        std::vector<uint8_t> f0ch;
        put_chunk(f0ch, toc, "ScriptActionFalse", K_SCRIPT_ACTION_VERSION_2, f0);

        std::vector<uint8_t> s0;
        put_script_head(s0, "Script One", "comment one", "cond comment", "act comment", 1, 1, 1, 1,
                        1, 0, &delay);
        put_bytes(s0, o0ch);
        put_bytes(s0, o1ch);
        put_bytes(s0, a0ch);
        put_bytes(s0, a1ch);
        put_bytes(s0, f0ch);
        std::vector<uint8_t> s0ch;
        put_chunk(s0ch, toc, "Script", K_SCRIPT_DATA_VERSION_2, s0);

        // list 0, script 1: an inactive one-shot with a subroutine flag
        std::vector<uint8_t> s1;
        Int nodelay = 0;
        put_script_head(s1, "Sub", "", "", "", 0, 0, 0, 1, 1, 1, &nodelay);
        std::vector<uint8_t> s1ch;
        put_chunk(s1ch, toc, "Script", K_SCRIPT_DATA_VERSION_2, s1);

        // a group with two scripts
        std::vector<uint8_t> g0s0;
        put_script_head(g0s0, "Grouped A", "", "", "", 1, 1, 1, 1, 1, 0, &nodelay);
        std::vector<uint8_t> g0s0ch;
        put_chunk(g0s0ch, toc, "Script", K_SCRIPT_DATA_VERSION_2, g0s0);
        std::vector<uint8_t> g0s1;
        put_script_head(g0s1, "Grouped B", "", "", "", 1, 1, 1, 1, 1, 0, &nodelay);
        std::vector<uint8_t> g0s1ch;
        put_chunk(g0s1ch, toc, "Script", K_SCRIPT_DATA_VERSION_2, g0s1);
        std::vector<uint8_t> g0;
        Int g0sub = 1;
        put_group_head(g0, "My Group", 0, &g0sub);
        put_bytes(g0, g0s0ch);
        put_bytes(g0, g0s1ch);
        std::vector<uint8_t> g0ch;
        put_chunk(g0ch, toc, "ScriptGroup", K_SCRIPT_GROUP_DATA_VERSION_2, g0);

        std::vector<uint8_t> l0;
        put_bytes(l0, s0ch);
        put_bytes(l0, s1ch);
        put_bytes(l0, g0ch);
        std::vector<uint8_t> l0ch;
        put_chunk(l0ch, toc, "ScriptList", K_SCRIPT_LIST_DATA_VERSION_1, l0);

        // list 1: one script with one condition, no actions
        std::vector<uint8_t> c9;
        put_cond_head(c9, CT_CONDITION_TRUE, toc.id("CONDITION_TRUE"), 0);
        std::vector<uint8_t> c9ch;
        put_chunk(c9ch, toc, "Condition", K_SCRIPT_CONDITION_VERSION_4, c9);
        std::vector<uint8_t> o9;
        put_bytes(o9, c9ch);
        std::vector<uint8_t> o9ch;
        put_chunk(o9ch, toc, "OrCondition", K_SCRIPT_OR_CONDITION_DATA_VERSION_1, o9);
        std::vector<uint8_t> s9;
        put_script_head(s9, "Second list script", "", "", "", 1, 1, 1, 1, 1, 0, &nodelay);
        put_bytes(s9, o9ch);
        std::vector<uint8_t> s9ch;
        put_chunk(s9ch, toc, "Script", K_SCRIPT_DATA_VERSION_2, s9);
        std::vector<uint8_t> l1;
        put_bytes(l1, s9ch);
        std::vector<uint8_t> l1ch;
        put_chunk(l1ch, toc, "ScriptList", K_SCRIPT_LIST_DATA_VERSION_1, l1);

        std::vector<uint8_t> pp;
        put_bytes(pp, l0ch);
        put_bytes(pp, l1ch);
        std::vector<uint8_t> pch;
        put_chunk(pch, toc, "PlayerScriptsList", K_SCRIPTS_DATA_VERSION_1, pp);
        std::vector<uint8_t> b = toc_bytes(toc);
        put_bytes(b, pch);
        RunOpts o;
        o.dump_keys = true;
        run_ps(out, "pscr_full", b, o);
}

// The group version family: version 1 (no subroutine byte), version 2
// (the byte), version 3 (the byte again gone — the exact equality).
static void corpus_pscr_group_vers(Out &out)
{
        TocB toc;
        std::vector<uint8_t> g1;
        put_group_head(g1, "V1 Group", 1, nullptr);
        std::vector<uint8_t> g1ch;
        put_chunk(g1ch, toc, "ScriptGroup", K_SCRIPT_GROUP_DATA_VERSION_1, g1);
        std::vector<uint8_t> g2;
        Int g2sub = 1;
        put_group_head(g2, "V2 Group", 0, &g2sub);
        std::vector<uint8_t> g2ch;
        put_chunk(g2ch, toc, "ScriptGroup", K_SCRIPT_GROUP_DATA_VERSION_2, g2);
        std::vector<uint8_t> g3;
        put_group_head(g3, "V3 Group", 0, nullptr);  // reads like a v1
        std::vector<uint8_t> g3ch;
        put_chunk(g3ch, toc, "ScriptGroup", 3, g3);
        std::vector<uint8_t> g0v;
        put_group_head(g0v, "V0 Group", 0, nullptr);
        std::vector<uint8_t> g0ch;
        put_chunk(g0ch, toc, "ScriptGroup", 0, g0v);

        std::vector<uint8_t> l0;
        put_bytes(l0, g1ch);
        put_bytes(l0, g2ch);
        put_bytes(l0, g3ch);
        put_bytes(l0, g0ch);
        std::vector<uint8_t> l0ch;
        put_chunk(l0ch, toc, "ScriptList", K_SCRIPT_LIST_DATA_VERSION_1, l0);
        std::vector<uint8_t> pp;
        put_bytes(pp, l0ch);
        std::vector<uint8_t> pch;
        put_chunk(pch, toc, "PlayerScriptsList", K_SCRIPTS_DATA_VERSION_1, pp);
        std::vector<uint8_t> b = toc_bytes(toc);
        put_bytes(b, pch);
        run_ps(out, "pscr_group_vers", b, RunOpts());
}

// Script and PlayerScriptsList/ScriptList version words: the v1 script has
// no delay int; the PlayerScriptsList and ScriptList versions are never
// read (a version 9 file walks like a version 5).
static void corpus_pscr_script_vers(Out &out)
{
        TocB toc;
        std::vector<uint8_t> s1;
        put_script_head(s1, "Old Script", "", "", "", 1, 1, 1, 1, 1, 0, nullptr);
        std::vector<uint8_t> s1ch;
        put_chunk(s1ch, toc, "Script", K_SCRIPT_DATA_VERSION_1, s1);
        std::vector<uint8_t> s0;
        Int d = 0;
        put_script_head(s0, "V0 Script", "", "", "", 1, 1, 1, 1, 1, 0, nullptr);
        std::vector<uint8_t> s0ch;
        put_chunk(s0ch, toc, "Script", 0, s0);
        std::vector<uint8_t> sf;
        put_script_head(sf, "V9 Script", "", "", "", 1, 1, 1, 1, 1, 0, &d);
        std::vector<uint8_t> sfch;
        put_chunk(sfch, toc, "Script", 9, sf);
        std::vector<uint8_t> l0;
        put_bytes(l0, s1ch);
        put_bytes(l0, s0ch);
        put_bytes(l0, sfch);
        std::vector<uint8_t> l0ch;
        put_chunk(l0ch, toc, "ScriptList", 9, l0);  // the version is never read
        std::vector<uint8_t> pp;
        put_bytes(pp, l0ch);
        std::vector<uint8_t> pch;
        put_chunk(pch, toc, "PlayerScriptsList", 9, pp);
        std::vector<uint8_t> b = toc_bytes(toc);
        put_bytes(b, pch);
        run_ps(out, "pscr_script_vers", b, RunOpts());
}

// The condition version family: v1 (no name key, the ParameterChangesVer2
// heal), v2/v3 (no key, no heal), v4 (the key).
static void corpus_pscr_cond_vers(Out &out)
{
        TocB toc;
        // v1: the seven changed conditions get the SURFACES_ALLOWED append
        std::vector<uint8_t> c1;
        put_cond_head(c1, CT_TEAM_INSIDE_AREA_PARTIALLY, 0, 2);
        put_parm(c1, PT_TEAM, 0, 0.0f, "TeamA");
        put_parm(c1, PT_TRIGGER_AREA, 0, 0.0f, "AreaA");
        std::vector<uint8_t> c1ch;
        put_chunk(c1ch, toc, "Condition", K_SCRIPT_CONDITION_VERSION_1, c1);
        // v1 with zero parms: the heal appends at [0], [1]/[2] stay null
        std::vector<uint8_t> c1b;
        put_cond_head(c1b, CT_TEAM_EXITED_AREA_PARTIALLY, 0, 0);
        std::vector<uint8_t> c1bch;
        put_chunk(c1bch, toc, "Condition", K_SCRIPT_CONDITION_VERSION_1, c1b);
        // v1 with one parm: the heal appends at [1], [2] stays null
        std::vector<uint8_t> c1c;
        put_cond_head(c1c, CT_TEAM_ENTERED_AREA_PARTIALLY, 0, 1);
        put_parm(c1c, PT_TEAM, 0, 0.0f, "TeamB");
        std::vector<uint8_t> c1cch;
        put_chunk(c1cch, toc, "Condition", K_SCRIPT_CONDITION_VERSION_1, c1c);
        // v2/v3: no key, no heal (a 2-parm area condition stays 2 — the
        // count check makes it FALSE)
        std::vector<uint8_t> c2;
        put_cond_head(c2, CT_TEAM_INSIDE_AREA_PARTIALLY, 0, 2);
        put_parm(c2, PT_TEAM, 0, 0.0f, "TeamA");
        put_parm(c2, PT_TRIGGER_AREA, 0, 0.0f, "AreaA");
        std::vector<uint8_t> c2ch;
        put_chunk(c2ch, toc, "Condition", K_SCRIPT_CONDITION_VERSION_2, c2);
        std::vector<uint8_t> c3;
        put_cond_head(c3, CT_TEAM_INSIDE_AREA_PARTIALLY, 0, 3);
        put_parm(c3, PT_TEAM, 0, 0.0f, "TeamA");
        put_parm(c3, PT_TRIGGER_AREA, 0, 0.0f, "AreaA");
        put_parm(c3, PT_SURFACES_ALLOWED, 3, 0.0f, "");
        std::vector<uint8_t> c3ch;
        put_chunk(c3ch, toc, "Condition", K_SCRIPT_CONDITION_VERSION_3, c3);
        // v4: the name key present
        std::vector<uint8_t> c4;
        put_cond_head(c4, CT_TIMER_EXPIRED, toc.id("TIMER_EXPIRED"), 1);
        put_parm(c4, PT_INT, 100, 0.0f, "");
        std::vector<uint8_t> c4ch;
        put_chunk(c4ch, toc, "Condition", K_SCRIPT_CONDITION_VERSION_4, c4);
        // v0: reads like a v1 (the heal fires below 2)
        std::vector<uint8_t> c0;
        put_cond_head(c0, CT_TEAM_OUTSIDE_AREA_ENTIRELY, 0, 2);
        put_parm(c0, PT_TEAM, 0, 0.0f, "TeamC");
        put_parm(c0, PT_TRIGGER_AREA, 0, 0.0f, "AreaC");
        std::vector<uint8_t> c0ch;
        put_chunk(c0ch, toc, "Condition", 0, c0);

        std::vector<uint8_t> o0;
        put_bytes(o0, c1ch);
        put_bytes(o0, c1bch);
        put_bytes(o0, c1cch);
        put_bytes(o0, c2ch);
        put_bytes(o0, c3ch);
        put_bytes(o0, c4ch);
        put_bytes(o0, c0ch);
        std::vector<uint8_t> o0ch;
        put_chunk(o0ch, toc, "OrCondition", K_SCRIPT_OR_CONDITION_DATA_VERSION_1, o0);

        std::vector<uint8_t> s0;
        put_script_head(s0, "CondVers", "", "", "", 1, 1, 1, 1, 1, 0, nullptr);
        put_bytes(s0, o0ch);
        std::vector<uint8_t> s0ch;
        put_chunk(s0ch, toc, "Script", K_SCRIPT_DATA_VERSION_1, s0);
        std::vector<uint8_t> l0;
        put_bytes(l0, s0ch);
        std::vector<uint8_t> l0ch;
        put_chunk(l0ch, toc, "ScriptList", K_SCRIPT_LIST_DATA_VERSION_1, l0);
        std::vector<uint8_t> pp;
        put_bytes(pp, l0ch);
        std::vector<uint8_t> pch;
        put_chunk(pch, toc, "PlayerScriptsList", K_SCRIPTS_DATA_VERSION_1, pp);
        std::vector<uint8_t> b = toc_bytes(toc);
        put_bytes(b, pch);
        run_ps(out, "pscr_cond_vers", b, RunOpts());
}

// The action version family: v1 (no name key), v2 (the key).
static void corpus_pscr_action_vers(Out &out)
{
        TocB toc;
        std::vector<uint8_t> a1;
        put_i32(a1, AT_SET_FLAG);
        put_i32(a1, 2);
        put_parm(a1, PT_FLAG, 0, 0.0f, "FlagA");
        put_parm(a1, PT_BOOLEAN, 1, 0.0f, "");
        std::vector<uint8_t> a1ch;
        put_chunk(a1ch, toc, "ScriptAction", K_SCRIPT_ACTION_VERSION_1, a1);
        std::vector<uint8_t> a2;
        put_i32(a2, AT_SET_COUNTER);
        put_key_word(a2, (toc.id("SET_COUNTER") << 8) | 3);
        put_i32(a2, 2);
        put_parm(a2, PT_COUNTER, 0, 0.0f, "CounterA");
        put_parm(a2, PT_INT, 42, 0.0f, "");
        std::vector<uint8_t> a2ch;
        put_chunk(a2ch, toc, "ScriptAction", K_SCRIPT_ACTION_VERSION_2, a2);
        // a version 0 action reads like a version 1 (the >= 2 gate)
        std::vector<uint8_t> a0;
        put_i32(a0, AT_DEBUG_STRING);
        put_i32(a0, 1);
        put_parm(a0, PT_TEXT_STRING, 0, 0.0f, "hello");
        std::vector<uint8_t> a0ch;
        put_chunk(a0ch, toc, "ScriptAction", 0, a0);

        std::vector<uint8_t> s0;
        put_script_head(s0, "ActVers", "", "", "", 1, 1, 1, 1, 1, 0, nullptr);
        put_bytes(s0, a1ch);
        put_bytes(s0, a2ch);
        put_bytes(s0, a0ch);
        std::vector<uint8_t> s0ch;
        put_chunk(s0ch, toc, "Script", K_SCRIPT_DATA_VERSION_1, s0);
        std::vector<uint8_t> l0;
        put_bytes(l0, s0ch);
        std::vector<uint8_t> l0ch;
        put_chunk(l0ch, toc, "ScriptList", K_SCRIPT_LIST_DATA_VERSION_1, l0);
        std::vector<uint8_t> pp;
        put_bytes(pp, l0ch);
        std::vector<uint8_t> pch;
        put_chunk(pch, toc, "PlayerScriptsList", K_SCRIPTS_DATA_VERSION_1, pp);
        std::vector<uint8_t> b = toc_bytes(toc);
        put_bytes(b, pch);
        run_ps(out, "pscr_action_vers", b, RunOpts());
}

// Raw parameter shapes: the plain int/real/string types round-trip
// untouched; COORD3D reads its three reals (no int/real/string words); a
// type word outside the enum stores raw; the count check keeps an
// out-of-range CONDITION type with zero parms (template 0 has zero) and an
// out-of-range ACTION type with one parm (template 0 has one).
static void corpus_pscr_parm_raw(Out &out)
{
        TocB toc;
        // COUNTER (3 parms): a mixed bag of raw types
        std::vector<uint8_t> c0;
        put_cond_head(c0, CT_COUNTER, 0, 3);
        put_parm(c0, PT_INT, -1, 0.0f, "");
        put_parm(c0, PT_REAL, 0, 1.25f, "");
        put_parm(c0, PT_SIDE, 0, 0.0f, "<This Player>");
        std::vector<uint8_t> c0ch;
        put_chunk(c0ch, toc, "Condition", K_SCRIPT_CONDITION_VERSION_2, c0);
        // COUNTER (3 parms) again: raw out-of-range PARAMETER type words
        // (9999, -7, 52 = NUM_ITEMS) — stored raw, no heal fires
        std::vector<uint8_t> c1;
        put_cond_head(c1, CT_COUNTER, 0, 3);
        put_parm(c1, 9999, 7, 0.5f, "x");
        put_parm(c1, -7, 0, 0.0f, "");
        put_parm(c1, 52, 1, 0.0f, "");
        std::vector<uint8_t> c1ch;
        put_chunk(c1ch, toc, "Condition", K_SCRIPT_CONDITION_VERSION_2, c1);
        // the out-of-range condition type with ZERO parms survives (the
        // template-0 count is 0)
        std::vector<uint8_t> c2;
        put_cond_head(c2, -5, 0, 0);
        std::vector<uint8_t> c2ch;
        put_chunk(c2ch, toc, "Condition", K_SCRIPT_CONDITION_VERSION_2, c2);
        // an empty-string parameter and a string with an embedded NUL
        std::vector<uint8_t> c3;
        put_cond_head(c3, CT_COUNTER, 0, 3);
        std::vector<uint8_t> c3m;
        put_i32(c3m, PT_INT);
        put_i32(c3m, 0);
        put_f32(c3m, 0.0f);
        put_str(c3m, "");
        put_bytes(c3, c3m);
        std::vector<uint8_t> c3m2;
        put_i32(c3m2, PT_TEXT_STRING);
        put_i32(c3m2, 0);
        put_f32(c3m2, 0.0f);
        std::string nulstr = "a";
        nulstr.push_back('\0');
        nulstr += "b";  // an embedded NUL: the AsciiString stops at it
        put_str(c3m2, nulstr);
        put_bytes(c3, c3m2);
        std::vector<uint8_t> c3m3;
        put_i32(c3m3, PT_COORD3D);
        put_f32(c3m3, 1.0f);
        put_f32(c3m3, 2.0f);
        put_f32(c3m3, 3.0f);
        put_bytes(c3, c3m3);
        std::vector<uint8_t> c3ch;
        put_chunk(c3ch, toc, "Condition", K_SCRIPT_CONDITION_VERSION_2, c3);

        std::vector<uint8_t> o0;
        put_bytes(o0, c0ch);
        put_bytes(o0, c1ch);
        put_bytes(o0, c2ch);
        put_bytes(o0, c3ch);
        std::vector<uint8_t> o0ch;
        put_chunk(o0ch, toc, "OrCondition", K_SCRIPT_OR_CONDITION_DATA_VERSION_1, o0);

        // the out-of-range ACTION type with ONE parm survives (the
        // template-0 count is 1)
        std::vector<uint8_t> a0;
        put_i32(a0, 9999);
        put_i32(a0, 1);
        put_parm(a0, PT_TEXT_STRING, 0, 0.0f, "raw type survives");
        std::vector<uint8_t> a0ch;
        put_chunk(a0ch, toc, "ScriptAction", K_SCRIPT_ACTION_VERSION_1, a0);
        // DEBUG_MESSAGE_BOX with 1 parm (the matching count)
        std::vector<uint8_t> a1;
        put_i32(a1, AT_DEBUG_MESSAGE_BOX);
        put_i32(a1, 1);
        put_parm(a1, PT_TEXT_STRING, 0, 0.0f, "pause me");
        std::vector<uint8_t> a1ch;
        put_chunk(a1ch, toc, "ScriptAction", K_SCRIPT_ACTION_VERSION_1, a1);

        std::vector<uint8_t> s0;
        put_script_head(s0, "ParmRaw", "", "", "", 1, 1, 1, 1, 1, 0, nullptr);
        put_bytes(s0, o0ch);
        put_bytes(s0, a0ch);
        put_bytes(s0, a1ch);
        std::vector<uint8_t> s0ch;
        put_chunk(s0ch, toc, "Script", K_SCRIPT_DATA_VERSION_1, s0);
        std::vector<uint8_t> l0;
        put_bytes(l0, s0ch);
        std::vector<uint8_t> l0ch;
        put_chunk(l0ch, toc, "ScriptList", K_SCRIPT_LIST_DATA_VERSION_1, l0);
        std::vector<uint8_t> pp;
        put_bytes(pp, l0ch);
        std::vector<uint8_t> pch;
        put_chunk(pch, toc, "PlayerScriptsList", K_SCRIPTS_DATA_VERSION_1, pp);
        std::vector<uint8_t> b = toc_bytes(toc);
        put_bytes(b, pch);
        run_ps(out, "pscr_parm_raw", b, RunOpts());
}

// The condition heal family: SKIRMISH_SPECIAL_POWER_READY's SIDE prepend
// (and its no-heal two-parm shape), plus the count-mismatch FALSE.
static void corpus_pscr_heals_cond(Out &out)
{
        TocB toc;
        // SKIRMISH_SPECIAL_POWER_READY with 1 parm: parms[0] (the special
        // power) shifts to [1], a SIDE "<This Player>" lands in [0]
        std::vector<uint8_t> c0;
        put_cond_head(c0, CT_SKIRMISH_SPECIAL_POWER_READY, 0, 1);
        put_parm(c0, PT_SPECIAL_POWER, 0, 0.0f, "SuperWeaponNuke");
        std::vector<uint8_t> c0ch;
        put_chunk(c0ch, toc, "Condition", K_SCRIPT_CONDITION_VERSION_2, c0);
        // the two-parm shape: no heal
        std::vector<uint8_t> c1;
        put_cond_head(c1, CT_SKIRMISH_SPECIAL_POWER_READY, 0, 2);
        put_parm(c1, PT_SIDE, 0, 0.0f, "Plyr1");
        put_parm(c1, PT_SPECIAL_POWER, 0, 0.0f, "SuperWeaponNuke");
        std::vector<uint8_t> c1ch;
        put_chunk(c1ch, toc, "Condition", K_SCRIPT_CONDITION_VERSION_2, c1);
        // a count mismatch: COUNTER (3) loaded with 2 → CONDITION_FALSE/0
        std::vector<uint8_t> c2;
        put_cond_head(c2, CT_COUNTER, 0, 2);
        put_parm(c2, PT_COUNTER, 0, 0.0f, "C");
        put_parm(c2, PT_COMPARISON, 0, 0.0f, "");
        std::vector<uint8_t> c2ch;
        put_chunk(c2ch, toc, "Condition", K_SCRIPT_CONDITION_VERSION_2, c2);
        // a zero-parm condition whose template wants one → FALSE
        std::vector<uint8_t> c3;
        put_cond_head(c3, CT_TIMER_EXPIRED, 0, 0);
        std::vector<uint8_t> c3ch;
        put_chunk(c3ch, toc, "Condition", K_SCRIPT_CONDITION_VERSION_2, c3);
        // a defunct condition (OBSOLETE_SCRIPT_1, template count 0) with
        // one parm → FALSE; with zero parms it would survive raw
        std::vector<uint8_t> c4;
        put_cond_head(c4, CT_OBSOLETE_SCRIPT_1, 0, 1);
        put_parm(c4, PT_INT, 1, 0.0f, "");
        std::vector<uint8_t> c4ch;
        put_chunk(c4ch, toc, "Condition", K_SCRIPT_CONDITION_VERSION_2, c4);
        std::vector<uint8_t> c5;
        put_cond_head(c5, CT_OBSOLETE_SCRIPT_1, 0, 0);
        std::vector<uint8_t> c5ch;
        put_chunk(c5ch, toc, "Condition", K_SCRIPT_CONDITION_VERSION_2, c5);

        std::vector<uint8_t> o0;
        put_bytes(o0, c0ch);
        put_bytes(o0, c1ch);
        put_bytes(o0, c2ch);
        put_bytes(o0, c3ch);
        put_bytes(o0, c4ch);
        put_bytes(o0, c5ch);
        std::vector<uint8_t> o0ch;
        put_chunk(o0ch, toc, "OrCondition", K_SCRIPT_OR_CONDITION_DATA_VERSION_1, o0);

        std::vector<uint8_t> s0;
        put_script_head(s0, "CondHeals", "", "", "", 1, 1, 1, 1, 1, 0, nullptr);
        put_bytes(s0, o0ch);
        std::vector<uint8_t> s0ch;
        put_chunk(s0ch, toc, "Script", K_SCRIPT_DATA_VERSION_1, s0);
        std::vector<uint8_t> l0;
        put_bytes(l0, s0ch);
        std::vector<uint8_t> l0ch;
        put_chunk(l0ch, toc, "ScriptList", K_SCRIPT_LIST_DATA_VERSION_1, l0);
        std::vector<uint8_t> pp;
        put_bytes(pp, l0ch);
        std::vector<uint8_t> pch;
        put_chunk(pch, toc, "PlayerScriptsList", K_SCRIPTS_DATA_VERSION_1, pp);
        std::vector<uint8_t> b = toc_bytes(toc);
        put_bytes(b, pch);
        run_ps(out, "pscr_heals_cond", b, RunOpts());
}

// The action heal switch, every case with its no-heal counterpart.
static void corpus_pscr_heals_act(Out &out)
{
        TocB toc;
        std::vector<uint8_t> s0;
        put_script_head(s0, "ActHeals", "", "", "", 1, 1, 1, 1, 1, 0, nullptr);

        // a helper lambda-shaped macro is overkill; spell each action out
        // SKIRMISH_FIRE_SPECIAL_POWER_AT_MOST_COST 1→2 (the SIDE prepend)
        {
                std::vector<uint8_t> a;
                put_i32(a, AT_SKIRMISH_FIRE_SPECIAL_POWER_AT_MOST_COST);
                put_i32(a, 1);
                put_parm(a, PT_SPECIAL_POWER, 0, 0.0f, "SuperWeaponParticleUplinkCannon");
                std::vector<uint8_t> ach;
                put_chunk(ach, toc, "ScriptAction", K_SCRIPT_ACTION_VERSION_1, a);
                put_bytes(s0, ach);
        }
        // TEAM_FOLLOW_WAYPOINTS 2→3 (the BOOLEAN 1 append)
        {
                std::vector<uint8_t> a;
                put_i32(a, AT_TEAM_FOLLOW_WAYPOINTS);
                put_i32(a, 2);
                put_parm(a, PT_TEAM, 0, 0.0f, "TeamA");
                put_parm(a, PT_WAYPOINT_PATH, 0, 0.0f, "PathA");
                std::vector<uint8_t> ach;
                put_chunk(ach, toc, "ScriptAction", K_SCRIPT_ACTION_VERSION_1, a);
                put_bytes(s0, ach);
        }
        // TEAM_FOLLOW_WAYPOINTS with 3 already: no heal
        {
                std::vector<uint8_t> a;
                put_i32(a, AT_TEAM_FOLLOW_WAYPOINTS);
                put_i32(a, 3);
                put_parm(a, PT_TEAM, 0, 0.0f, "TeamA");
                put_parm(a, PT_WAYPOINT_PATH, 0, 0.0f, "PathA");
                put_parm(a, PT_BOOLEAN, 0, 0.0f, "");
                std::vector<uint8_t> ach;
                put_chunk(ach, toc, "ScriptAction", K_SCRIPT_ACTION_VERSION_1, a);
                put_bytes(s0, ach);
        }
        // SKIRMISH_BUILD_BASE_DEFENSE_FRONT 1→0: flank false → stays FRONT
        {
                std::vector<uint8_t> a;
                put_i32(a, AT_SKIRMISH_BUILD_BASE_DEFENSE_FRONT);
                put_i32(a, 1);
                put_parm(a, PT_BOOLEAN, 0, 0.0f, "");
                std::vector<uint8_t> ach;
                put_chunk(ach, toc, "ScriptAction", K_SCRIPT_ACTION_VERSION_1, a);
                put_bytes(s0, ach);
        }
        // flank true → the type switches to FLANK
        {
                std::vector<uint8_t> a;
                put_i32(a, AT_SKIRMISH_BUILD_BASE_DEFENSE_FRONT);
                put_i32(a, 1);
                put_parm(a, PT_BOOLEAN, 1, 0.0f, "");
                std::vector<uint8_t> ach;
                put_chunk(ach, toc, "ScriptAction", K_SCRIPT_ACTION_VERSION_1, a);
                put_bytes(s0, ach);
        }
        // NAMED_SET_ATTITUDE with parms[1] an INT → re-typed AI_MOOD
        {
                std::vector<uint8_t> a;
                put_i32(a, AT_NAMED_SET_ATTITUDE);
                put_i32(a, 2);
                put_parm(a, PT_UNIT, 0, 0.0f, "UnitA");
                put_parm(a, PT_INT, 2, 0.0f, "");
                std::vector<uint8_t> ach;
                put_chunk(ach, toc, "ScriptAction", K_SCRIPT_ACTION_VERSION_1, a);
                put_bytes(s0, ach);
        }
        // ... with parms[1] a REAL: untouched
        {
                std::vector<uint8_t> a;
                put_i32(a, AT_TEAM_SET_ATTITUDE);
                put_i32(a, 2);
                put_parm(a, PT_TEAM, 0, 0.0f, "TeamB");
                put_parm(a, PT_REAL, 0, 1.5f, "");
                std::vector<uint8_t> ach;
                put_chunk(ach, toc, "ScriptAction", K_SCRIPT_ACTION_VERSION_1, a);
                put_bytes(s0, ach);
        }
        // MAP_REVEAL_AT_WAYPOINT 2→3 (the SIDE append)
        {
                std::vector<uint8_t> a;
                put_i32(a, AT_MAP_REVEAL_AT_WAYPOINT);
                put_i32(a, 2);
                put_parm(a, PT_WAYPOINT, 0, 0.0f, "WayA");
                put_parm(a, PT_REAL, 0, 50.0f, "");
                std::vector<uint8_t> ach;
                put_chunk(ach, toc, "ScriptAction", K_SCRIPT_ACTION_VERSION_1, a);
                put_bytes(s0, ach);
        }
        // MAP_REVEAL_ALL 0→1 (the SIDE append)
        {
                std::vector<uint8_t> a;
                put_i32(a, AT_MAP_REVEAL_ALL);
                put_i32(a, 0);
                std::vector<uint8_t> ach;
                put_chunk(ach, toc, "ScriptAction", K_SCRIPT_ACTION_VERSION_1, a);
                put_bytes(s0, ach);
        }
        // SPEECH_PLAY 1→2 (the BOOLEAN 1 append)
        {
                std::vector<uint8_t> a;
                put_i32(a, AT_SPEECH_PLAY);
                put_i32(a, 1);
                put_parm(a, PT_DIALOG, 0, 0.0f, "SpeechA");
                std::vector<uint8_t> ach;
                put_chunk(ach, toc, "ScriptAction", K_SCRIPT_ACTION_VERSION_1, a);
                put_bytes(s0, ach);
        }
        // CAMERA_MOD_SET_FINAL_ZOOM 1→3 (two PERCENT pads)
        {
                std::vector<uint8_t> a;
                put_i32(a, AT_CAMERA_MOD_SET_FINAL_ZOOM);
                put_i32(a, 1);
                put_parm(a, PT_PERCENT, 55, 0.0f, "");
                std::vector<uint8_t> ach;
                put_chunk(ach, toc, "ScriptAction", K_SCRIPT_ACTION_VERSION_1, a);
                put_bytes(s0, ach);
        }
        // MOVE_CAMERA_TO 3→5 (two REAL pads)
        {
                std::vector<uint8_t> a;
                put_i32(a, AT_MOVE_CAMERA_TO);
                put_i32(a, 3);
                put_parm_coord(a, 10.0f, 20.0f, 30.0f);
                put_parm(a, PT_REAL, 0, 1.0f, "");
                put_parm(a, PT_BOOLEAN, 1, 0.0f, "");
                std::vector<uint8_t> ach;
                put_chunk(ach, toc, "ScriptAction", K_SCRIPT_ACTION_VERSION_1, a);
                put_bytes(s0, ach);
        }
        // ZOOM_CAMERA 2→4 (two REAL pads)
        {
                std::vector<uint8_t> a;
                put_i32(a, AT_ZOOM_CAMERA);
                put_i32(a, 2);
                put_parm(a, PT_REAL, 0, 40.0f, "");
                put_parm(a, PT_REAL, 0, 2.0f, "");
                std::vector<uint8_t> ach;
                put_chunk(ach, toc, "ScriptAction", K_SCRIPT_ACTION_VERSION_1, a);
                put_bytes(s0, ach);
        }
        // CAMERA_LOOK_TOWARD_WAYPOINT 2→5 (two REAL pads + the BOOLEAN pad)
        {
                std::vector<uint8_t> a;
                put_i32(a, AT_CAMERA_LOOK_TOWARD_WAYPOINT);
                put_i32(a, 2);
                put_parm(a, PT_WAYPOINT, 0, 0.0f, "WayB");
                put_parm(a, PT_REAL, 0, 3.0f, "");
                std::vector<uint8_t> ach;
                put_chunk(ach, toc, "ScriptAction", K_SCRIPT_ACTION_VERSION_1, a);
                put_bytes(s0, ach);
        }
        // ... and 4→5 (the BOOLEAN pad alone)
        {
                std::vector<uint8_t> a;
                put_i32(a, AT_CAMERA_LOOK_TOWARD_WAYPOINT);
                put_i32(a, 4);
                put_parm(a, PT_WAYPOINT, 0, 0.0f, "WayC");
                put_parm(a, PT_REAL, 0, 3.0f, "");
                put_parm(a, PT_REAL, 0, 4.0f, "");
                put_parm(a, PT_REAL, 0, 5.0f, "");
                std::vector<uint8_t> ach;
                put_chunk(ach, toc, "ScriptAction", K_SCRIPT_ACTION_VERSION_1, a);
                put_bytes(s0, ach);
        }
        // a count mismatch: SET_FLAG (2) loaded with 3 → NO_OP/0
        {
                std::vector<uint8_t> a;
                put_i32(a, AT_SET_FLAG);
                put_i32(a, 3);
                put_parm(a, PT_FLAG, 0, 0.0f, "F");
                put_parm(a, PT_BOOLEAN, 1, 0.0f, "");
                put_parm(a, PT_INT, 9, 0.0f, "");
                std::vector<uint8_t> ach;
                put_chunk(ach, toc, "ScriptAction", K_SCRIPT_ACTION_VERSION_1, a);
                put_bytes(s0, ach);
        }

        std::vector<uint8_t> s0ch;
        put_chunk(s0ch, toc, "Script", K_SCRIPT_DATA_VERSION_1, s0);
        std::vector<uint8_t> l0;
        put_bytes(l0, s0ch);
        std::vector<uint8_t> l0ch;
        put_chunk(l0ch, toc, "ScriptList", K_SCRIPT_LIST_DATA_VERSION_1, l0);
        std::vector<uint8_t> pp;
        put_bytes(pp, l0ch);
        std::vector<uint8_t> pch;
        put_chunk(pch, toc, "PlayerScriptsList", K_SCRIPTS_DATA_VERSION_1, pp);
        std::vector<uint8_t> b = toc_bytes(toc);
        put_bytes(b, pch);
        run_ps(out, "pscr_heals_act", b, RunOpts());
}

// The name-key rematch family: the id/name mismatch repair, the by-name
// rescan, the unknown fallback, the empty-name key rematching to the first
// defunct condition, and the raw-type survivors.
static void corpus_pscr_rematch(Out &out)
{
        TocB toc;
        Int zero = 0;
        // a v4 condition whose type word and name key disagree: the name
        // wins (COUNTER typed, FLAG named → FLAG)
        std::vector<uint8_t> c0;
        put_cond_head(c0, CT_COUNTER, toc.id("FLAG"), 2);
        put_parm(c0, PT_FLAG, 0, 0.0f, "FlagX");
        put_parm(c0, PT_BOOLEAN, 1, 0.0f, "");
        std::vector<uint8_t> c0ch;
        put_chunk(c0ch, toc, "Condition", K_SCRIPT_CONDITION_VERSION_4, c0);
        // an unknown name: CONDITION_FALSE/0 (the parms read then hidden)
        std::vector<uint8_t> c1;
        put_cond_head(c1, CT_TIMER_EXPIRED, toc.id("NoSuchConditionName"), 1);
        put_parm(c1, PT_INT, 3, 0.0f, "");
        std::vector<uint8_t> c1ch;
        put_chunk(c1ch, toc, "Condition", K_SCRIPT_CONDITION_VERSION_4, c1);
        // an out-of-range type whose key matches template 0
        // (CONDITION_FALSE): with 0 parms the raw type survives
        std::vector<uint8_t> c2;
        put_cond_head(c2, 999, toc.id("CONDITION_FALSE"), 0);
        std::vector<uint8_t> c2ch;
        put_chunk(c2ch, toc, "Condition", K_SCRIPT_CONDITION_VERSION_4, c2);
        // an out-of-range TOC id in the key word: the empty name interns,
        // and the empty key REMATCHES to OBSOLETE_SCRIPT_1 (the first
        // defunct condition — the empty internal name)
        std::vector<uint8_t> c3;
        put_cond_head(c3, CT_TIMER_EXPIRED, 127, 0);
        std::vector<uint8_t> c3ch;
        put_chunk(c3ch, toc, "Condition", K_SCRIPT_CONDITION_VERSION_4, c3);
        // a negative key word: -1 >> 8 = -1 → 0xFFFFFFFF → unknown → the
        // empty name → the same OBSOLETE rematch
        std::vector<uint8_t> c4;
        put_cond_head(c4, 0, 0, 0);
        {
                // rewrite the key word raw (put_cond_head took a toc id; do
                // it manually here)
                std::vector<uint8_t> m;
                put_i32(m, CT_TIMER_EXPIRED);
                put_key_word(m, -1);
                put_i32(m, 0);
                c4 = m;
        }
        std::vector<uint8_t> c4ch;
        put_chunk(c4ch, toc, "Condition", K_SCRIPT_CONDITION_VERSION_4, c4);
        // a rematch that lands on a count that matches the loaded parms
        std::vector<uint8_t> c5;
        put_cond_head(c5, CT_CONDITION_TRUE, toc.id("COUNTER"), 3);
        put_parm(c5, PT_COUNTER, 0, 0.0f, "CC");
        put_parm(c5, PT_COMPARISON, 1, 0.0f, "");
        put_parm(c5, PT_INT, 5, 0.0f, "");
        std::vector<uint8_t> c5ch;
        put_chunk(c5ch, toc, "Condition", K_SCRIPT_CONDITION_VERSION_4, c5);

        std::vector<uint8_t> o0;
        put_bytes(o0, c0ch);
        put_bytes(o0, c1ch);
        put_bytes(o0, c2ch);
        put_bytes(o0, c3ch);
        put_bytes(o0, c4ch);
        put_bytes(o0, c5ch);
        std::vector<uint8_t> o0ch;
        put_chunk(o0ch, toc, "OrCondition", K_SCRIPT_OR_CONDITION_DATA_VERSION_1, o0);

        // the action rematch family: repaired, unknown → NO_OP, raw
        // survivor (an out-of-range type keyed to template 0's name with
        // 1 parm)
        std::vector<uint8_t> a0;
        put_i32(a0, AT_SET_FLAG);
        put_key_word(a0, (toc.id("SET_COUNTER") << 8) | 3);
        put_i32(a0, 2);
        put_parm(a0, PT_COUNTER, 0, 0.0f, "Cnt");
        put_parm(a0, PT_INT, 4, 0.0f, "");
        std::vector<uint8_t> a0ch;
        put_chunk(a0ch, toc, "ScriptAction", K_SCRIPT_ACTION_VERSION_2, a0);
        std::vector<uint8_t> a1;
        put_i32(a1, AT_SET_FLAG);
        put_key_word(a1, (toc.id("NoSuchActionName") << 8) | 3);
        put_i32(a1, 2);
        put_parm(a1, PT_FLAG, 0, 0.0f, "F");
        put_parm(a1, PT_BOOLEAN, 0, 0.0f, "");
        std::vector<uint8_t> a1ch;
        put_chunk(a1ch, toc, "ScriptAction", K_SCRIPT_ACTION_VERSION_2, a1);
        std::vector<uint8_t> a2;
        put_i32(a2, -3);
        put_key_word(a2, (toc.id("DEBUG_MESSAGE_BOX") << 8) | 3);
        put_i32(a2, 1);
        put_parm(a2, PT_TEXT_STRING, 0, 0.0f, "raw action survives");
        std::vector<uint8_t> a2ch;
        put_chunk(a2ch, toc, "ScriptAction", K_SCRIPT_ACTION_VERSION_2, a2);
        // the empty-name key on an action: NAMED_RECEIVE_UPGRADE (the one
        // defunct action) has the empty internal name — the rescan finds it
        std::vector<uint8_t> a3;
        put_i32(a3, AT_SET_FLAG);
        put_key_word(a3, (127 << 8) | 3);  // an unknown TOC id → the empty name
        put_i32(a3, 0);
        std::vector<uint8_t> a3ch;
        put_chunk(a3ch, toc, "ScriptAction", K_SCRIPT_ACTION_VERSION_2, a3);

        std::vector<uint8_t> s0;
        put_script_head(s0, "Rematch", "", "", "", 1, 1, 1, 1, 1, 0, &zero);
        put_bytes(s0, o0ch);
        put_bytes(s0, a0ch);
        put_bytes(s0, a1ch);
        put_bytes(s0, a2ch);
        put_bytes(s0, a3ch);
        std::vector<uint8_t> s0ch;
        put_chunk(s0ch, toc, "Script", K_SCRIPT_DATA_VERSION_2, s0);
        std::vector<uint8_t> l0;
        put_bytes(l0, s0ch);
        std::vector<uint8_t> l0ch;
        put_chunk(l0ch, toc, "ScriptList", K_SCRIPT_LIST_DATA_VERSION_1, l0);
        std::vector<uint8_t> pp;
        put_bytes(pp, l0ch);
        std::vector<uint8_t> pch;
        put_chunk(pch, toc, "PlayerScriptsList", K_SCRIPTS_DATA_VERSION_1, pp);
        std::vector<uint8_t> b = toc_bytes(toc);
        put_bytes(b, pch);
        RunOpts o;
        o.dump_keys = true;
        run_ps(out, "pscr_rematch", b, o);
}

// The KIND_OF_PARAM heal: the table walk (case-insensitive), the legacy
// remaps, the MISSILE split, and the empty-string normalization.
static void corpus_pscr_kindof(Out &out)
{
        TocB toc;
        // three KIND_OF params on a COUNTER condition (the count check
        // passes; the heal fires per parameter)
        std::vector<uint8_t> c0;
        put_cond_head(c0, CT_COUNTER, 0, 3);
        put_parm(c0, PT_KIND_OF_PARAM, 0, 0.0f, "OBSTACLE");          // → int 0
        put_parm(c0, PT_KIND_OF_PARAM, 0, 0.0f, "obstacle");          // case-insensitive → 0
        put_parm(c0, PT_KIND_OF_PARAM, 0, 0.0f, "PARACHUTE");         // → its table index
        std::vector<uint8_t> c0ch;
        put_chunk(c0ch, toc, "Condition", K_SCRIPT_CONDITION_VERSION_2, c0);
        std::vector<uint8_t> c1;
        put_cond_head(c1, CT_COUNTER, 0, 3);
        put_parm(c1, PT_KIND_OF_PARAM, 0, 0.0f, "CRUSHER");       // → 0 (the OBSTACLE index)
        put_parm(c1, PT_KIND_OF_PARAM, 0, 0.0f, "CRUSHABLE");     // → 0
        put_parm(c1, PT_KIND_OF_PARAM, 0, 0.0f, "OVERLAPPABLE");  // → 0
        std::vector<uint8_t> c1ch;
        put_chunk(c1ch, toc, "Condition", K_SCRIPT_CONDITION_VERSION_2, c1);
        // MISSILE: the string becomes SMALL_MISSILE and the int becomes
        // the real SMALL_MISSILE index (the always-true re-compare sets
        // 0, then the outer loop's continuation overwrites it)
        std::vector<uint8_t> c2;
        put_cond_head(c2, CT_COUNTER, 0, 3);
        put_parm(c2, PT_KIND_OF_PARAM, 0, 0.0f, "MISSILE");
        put_parm(c2, PT_KIND_OF_PARAM, 0, 0.0f, "missile");
        put_parm(c2, PT_KIND_OF_PARAM, 55, 0.0f, "");  // the empty string: int 55 normalizes
        std::vector<uint8_t> c2ch;
        put_chunk(c2ch, toc, "Condition", K_SCRIPT_CONDITION_VERSION_2, c2);
        // the empty string with int 0 → "OBSTACLE"
        std::vector<uint8_t> c3;
        put_cond_head(c3, CT_COUNTER, 0, 3);
        put_parm(c3, PT_KIND_OF_PARAM, 0, 0.0f, "");
        put_parm(c3, PT_KIND_OF_PARAM, 116, 0.0f, "");  // the last entry
        put_parm(c3, PT_KIND_OF_PARAM, 47, 0.0f, "");   // SMALL_MISSILE's index
        std::vector<uint8_t> c3ch;
        put_chunk(c3ch, toc, "Condition", K_SCRIPT_CONDITION_VERSION_2, c3);
        // a lowercased table name and a near-miss name that still matches
        // nothing here (kept for the kindof-throw corpus)
        std::vector<uint8_t> c4;
        put_cond_head(c4, CT_COUNTER, 0, 3);
        put_parm(c4, PT_KIND_OF_PARAM, 0, 0.0f, "Fs_Power");  // case-insensitive match
        put_parm(c4, PT_KIND_OF_PARAM, 0, 0.0f, "BOAT");
        put_parm(c4, PT_KIND_OF_PARAM, 0, 0.0f, "HULK");
        std::vector<uint8_t> c4ch;
        put_chunk(c4ch, toc, "Condition", K_SCRIPT_CONDITION_VERSION_2, c4);

        std::vector<uint8_t> o0;
        put_bytes(o0, c0ch);
        put_bytes(o0, c1ch);
        put_bytes(o0, c2ch);
        put_bytes(o0, c3ch);
        put_bytes(o0, c4ch);
        std::vector<uint8_t> o0ch;
        put_chunk(o0ch, toc, "OrCondition", K_SCRIPT_OR_CONDITION_DATA_VERSION_1, o0);

        std::vector<uint8_t> s0;
        put_script_head(s0, "KindOf", "", "", "", 1, 1, 1, 1, 1, 0, nullptr);
        put_bytes(s0, o0ch);
        std::vector<uint8_t> s0ch;
        put_chunk(s0ch, toc, "Script", K_SCRIPT_DATA_VERSION_1, s0);
        std::vector<uint8_t> l0;
        put_bytes(l0, s0ch);
        std::vector<uint8_t> l0ch;
        put_chunk(l0ch, toc, "ScriptList", K_SCRIPT_LIST_DATA_VERSION_1, l0);
        std::vector<uint8_t> pp;
        put_bytes(pp, l0ch);
        std::vector<uint8_t> pch;
        put_chunk(pch, toc, "PlayerScriptsList", K_SCRIPTS_DATA_VERSION_1, pp);
        std::vector<uint8_t> b = toc_bytes(toc);
        put_bytes(b, pch);
        run_ps(out, "pscr_kindof", b, RunOpts());
}

// The unknown-kindof throw (ERROR_BUG): the walk aborts.
static void corpus_pscr_kindof_throw(Out &out)
{
        TocB toc;
        std::vector<uint8_t> c0;
        put_cond_head(c0, CT_COUNTER, 0, 3);
        put_parm(c0, PT_KIND_OF_PARAM, 0, 0.0f, "OBSTACLE");
        put_parm(c0, PT_KIND_OF_PARAM, 0, 0.0f, "NOT_A_KINDOF");
        put_parm(c0, PT_KIND_OF_PARAM, 0, 0.0f, "OBSTACLE");
        std::vector<uint8_t> c0ch;
        put_chunk(c0ch, toc, "Condition", K_SCRIPT_CONDITION_VERSION_2, c0);
        std::vector<uint8_t> o0;
        put_bytes(o0, c0ch);
        std::vector<uint8_t> o0ch;
        put_chunk(o0ch, toc, "OrCondition", K_SCRIPT_OR_CONDITION_DATA_VERSION_1, o0);
        std::vector<uint8_t> s0;
        put_script_head(s0, "KindOfThrow", "", "", "", 1, 1, 1, 1, 1, 0, nullptr);
        put_bytes(s0, o0ch);
        std::vector<uint8_t> s0ch;
        put_chunk(s0ch, toc, "Script", K_SCRIPT_DATA_VERSION_1, s0);
        std::vector<uint8_t> l0;
        put_bytes(l0, s0ch);
        std::vector<uint8_t> l0ch;
        put_chunk(l0ch, toc, "ScriptList", K_SCRIPT_LIST_DATA_VERSION_1, l0);
        std::vector<uint8_t> pp;
        put_bytes(pp, l0ch);
        std::vector<uint8_t> pch;
        put_chunk(pch, toc, "PlayerScriptsList", K_SCRIPTS_DATA_VERSION_1, pp);
        std::vector<uint8_t> b = toc_bytes(toc);
        put_bytes(b, pch);
        run_ps(out, "pscr_kindof_throw", b, RunOpts());
}

// The OBJECT_STATUS heal: the name-to-mask walk (case-insensitive, the
// NONE bit-0 quirk, the unmatched no-op).
static void corpus_pscr_objstatus(Out &out)
{
        TocB toc;
        std::vector<uint8_t> c0;
        put_cond_head(c0, CT_COUNTER, 0, 3);
        put_parm(c0, PT_OBJECT_STATUS, 0, 0.0f, "DESTROYED");     // → 1<<1
        put_parm(c0, PT_OBJECT_STATUS, 0, 0.0f, "destroyed");     // case-insensitive
        put_parm(c0, PT_OBJECT_STATUS, 0, 0.0f, "NONE");          // → 1<<0 (!)
        std::vector<uint8_t> c0ch;
        put_chunk(c0ch, toc, "Condition", K_SCRIPT_CONDITION_VERSION_2, c0);
        std::vector<uint8_t> c1;
        put_cond_head(c1, CT_COUNTER, 0, 3);
        put_parm(c1, PT_OBJECT_STATUS, 0, 0.0f, "BOGUS_STATUS");  // unmatched → 0
        put_parm(c1, PT_OBJECT_STATUS, 0, 0.0f, "");              // empty → 0
        put_parm(c1, PT_OBJECT_STATUS, 0, 0.0f, "DEPLOYED");      // the last entry → 1<<44
        std::vector<uint8_t> c1ch;
        put_chunk(c1ch, toc, "Condition", K_SCRIPT_CONDITION_VERSION_2, c1);

        std::vector<uint8_t> o0;
        put_bytes(o0, c0ch);
        put_bytes(o0, c1ch);
        std::vector<uint8_t> o0ch;
        put_chunk(o0ch, toc, "OrCondition", K_SCRIPT_OR_CONDITION_DATA_VERSION_1, o0);
        std::vector<uint8_t> s0;
        put_script_head(s0, "ObjStatus", "", "", "", 1, 1, 1, 1, 1, 0, nullptr);
        put_bytes(s0, o0ch);
        std::vector<uint8_t> s0ch;
        put_chunk(s0ch, toc, "Script", K_SCRIPT_DATA_VERSION_1, s0);
        std::vector<uint8_t> l0;
        put_bytes(l0, s0ch);
        std::vector<uint8_t> l0ch;
        put_chunk(l0ch, toc, "ScriptList", K_SCRIPT_LIST_DATA_VERSION_1, l0);
        std::vector<uint8_t> pp;
        put_bytes(pp, l0ch);
        std::vector<uint8_t> pch;
        put_chunk(pch, toc, "PlayerScriptsList", K_SCRIPTS_DATA_VERSION_1, pp);
        std::vector<uint8_t> b = toc_bytes(toc);
        put_bytes(b, pch);
        run_ps(out, "pscr_objstatus", b, RunOpts());
}

// The string rewrites: OBJECT_TYPE's Fundamentalist→GLA rename (with the
// strlcat truncation) and the UPGRADE rename trio.
static void corpus_pscr_rename(Out &out)
{
        TocB toc;
        std::string longSuffix(300, 'x');
        std::vector<uint8_t> c0;
        put_cond_head(c0, CT_COUNTER, 0, 3);
        put_parm(c0, PT_OBJECT_TYPE, 0, 0.0f, "FundamentalistTruck");   // → GLATruck
        put_parm(c0, PT_OBJECT_TYPE, 0, 0.0f, "Fundamentalist");        // → GLA
        put_parm(c0, PT_OBJECT_TYPE, 0, 0.0f, "AmericaTank");           // untouched
        std::vector<uint8_t> c0ch;
        put_chunk(c0ch, toc, "Condition", K_SCRIPT_CONDITION_VERSION_2, c0);
        // the 300-char suffix truncates at 255 (strlcat into char[256])
        std::vector<uint8_t> c1;
        put_cond_head(c1, CT_COUNTER, 0, 3);
        put_parm(c1, PT_OBJECT_TYPE, 0, 0.0f, "Fundamentalist" + longSuffix);
        put_parm(c1, PT_UNIT, 0, 0.0f, "FundamentalistTruck");  // not OBJECT_TYPE: untouched
        put_parm(c1, PT_OBJECT_TYPE, 0, 0.0f, "fundamentalistTruck");   // case-SENSITIVE prefix: no rename
        std::vector<uint8_t> c1ch;
        put_chunk(c1ch, toc, "Condition", K_SCRIPT_CONDITION_VERSION_2, c1);
        // the UPGRADE rename trio
        std::vector<uint8_t> c2;
        put_cond_head(c2, CT_COUNTER, 0, 3);
        put_parm(c2, PT_UPGRADE, 0, 0.0f, "Upgrade_AmericaRangerCaptureBuilding");
        put_parm(c2, PT_UPGRADE, 0, 0.0f, "Upgrade_GLARebelCaptureBuilding");
        put_parm(c2, PT_UPGRADE, 0, 0.0f, "Upgrade_ChinaRedguardCaptureBuilding");
        std::vector<uint8_t> c2ch;
        put_chunk(c2ch, toc, "Condition", K_SCRIPT_CONDITION_VERSION_2, c2);
        std::vector<uint8_t> c3;
        put_cond_head(c3, CT_COUNTER, 0, 3);
        put_parm(c3, PT_UPGRADE, 0, 0.0f, "Upgrade_InfantryCaptureBuilding");  // already new
        put_parm(c3, PT_UPGRADE, 0, 0.0f, "Upgrade_Other");                    // untouched
        put_parm(c3, PT_OBJECT_TYPE, 0, 0.0f, "FundamentalistThing");          // → GLAThing
        std::vector<uint8_t> c3ch;
        put_chunk(c3ch, toc, "Condition", K_SCRIPT_CONDITION_VERSION_2, c3);

        std::vector<uint8_t> o0;
        put_bytes(o0, c0ch);
        put_bytes(o0, c1ch);
        put_bytes(o0, c2ch);
        put_bytes(o0, c3ch);
        std::vector<uint8_t> o0ch;
        put_chunk(o0ch, toc, "OrCondition", K_SCRIPT_OR_CONDITION_DATA_VERSION_1, o0);
        std::vector<uint8_t> s0;
        put_script_head(s0, "Rename", "", "", "", 1, 1, 1, 1, 1, 0, nullptr);
        put_bytes(s0, o0ch);
        std::vector<uint8_t> s0ch;
        put_chunk(s0ch, toc, "Script", K_SCRIPT_DATA_VERSION_1, s0);
        std::vector<uint8_t> l0;
        put_bytes(l0, s0ch);
        std::vector<uint8_t> l0ch;
        put_chunk(l0ch, toc, "ScriptList", K_SCRIPT_LIST_DATA_VERSION_1, l0);
        std::vector<uint8_t> pp;
        put_bytes(pp, l0ch);
        std::vector<uint8_t> pch;
        put_chunk(pch, toc, "PlayerScriptsList", K_SCRIPTS_DATA_VERSION_1, pp);
        std::vector<uint8_t> b = toc_bytes(toc);
        put_bytes(b, pch);
        run_ps(out, "pscr_rename", b, RunOpts());
}

// The out-of-bounds verdict family: the 13th parameter store, the
// version-1 append heal at count 12 and at negative counts, and the
// kindof normalization index.
static void corpus_pscr_oob(Out &out)
{
        TocB toc;
        RunOpts o;
        // a condition with 13 parms: the 13th read consumes its bytes,
        // then the store passes m_parms[12] — the verdict
        {
                std::vector<uint8_t> c0;
                put_cond_head(c0, CT_COUNTER, 0, 13);
                for (int i = 0; i < 13; i++)
                        put_parm(c0, PT_INT, i, 0.0f, "");
                std::vector<uint8_t> c0ch;
                put_chunk(c0ch, toc, "Condition", K_SCRIPT_CONDITION_VERSION_2, c0);
                std::vector<uint8_t> o0;
                put_bytes(o0, c0ch);
                std::vector<uint8_t> o0ch;
                put_chunk(o0ch, toc, "OrCondition", K_SCRIPT_OR_CONDITION_DATA_VERSION_1, o0);
                std::vector<uint8_t> s0;
                put_script_head(s0, "OobParms", "", "", "", 1, 1, 1, 1, 1, 0, nullptr);
                put_bytes(s0, o0ch);
                std::vector<uint8_t> s0ch;
                put_chunk(s0ch, toc, "Script", K_SCRIPT_DATA_VERSION_1, s0);
                std::vector<uint8_t> l0;
                put_bytes(l0, s0ch);
                std::vector<uint8_t> l0ch;
                put_chunk(l0ch, toc, "ScriptList", K_SCRIPT_LIST_DATA_VERSION_1, l0);
                std::vector<uint8_t> pp;
                put_bytes(pp, l0ch);
                std::vector<uint8_t> pch;
                put_chunk(pch, toc, "PlayerScriptsList", K_SCRIPTS_DATA_VERSION_1, pp);
                std::vector<uint8_t> b = toc_bytes(toc);
                put_bytes(b, pch);
                run_ps(out, "pscr_oob_parms", b, o);
        }
        // an action with 13 parms: the same verdict
        {
                std::vector<uint8_t> a0;
                put_i32(a0, AT_SET_TREE_SWAY);
                put_i32(a0, 13);
                for (int i = 0; i < 13; i++)
                        put_parm(a0, PT_INT, i, 0.0f, "");
                std::vector<uint8_t> a0ch;
                put_chunk(a0ch, toc, "ScriptAction", K_SCRIPT_ACTION_VERSION_1, a0);
                std::vector<uint8_t> s0;
                put_script_head(s0, "OobActParms", "", "", "", 1, 1, 1, 1, 1, 0, nullptr);
                put_bytes(s0, a0ch);
                std::vector<uint8_t> s0ch;
                put_chunk(s0ch, toc, "Script", K_SCRIPT_DATA_VERSION_1, s0);
                std::vector<uint8_t> l0;
                put_bytes(l0, s0ch);
                std::vector<uint8_t> l0ch;
                put_chunk(l0ch, toc, "ScriptList", K_SCRIPT_LIST_DATA_VERSION_1, l0);
                std::vector<uint8_t> pp;
                put_bytes(pp, l0ch);
                std::vector<uint8_t> pch;
                put_chunk(pch, toc, "PlayerScriptsList", K_SCRIPTS_DATA_VERSION_1, pp);
                std::vector<uint8_t> b = toc_bytes(toc);
                put_bytes(b, pch);
                run_ps(out, "pscr_oob_actparms", b, o);
        }
        // the version-1 heal at count 12: the append store at [12]
        {
                std::vector<uint8_t> c0;
                put_cond_head(c0, CT_TEAM_INSIDE_AREA_PARTIALLY, 0, 12);
                for (int i = 0; i < 12; i++)
                        put_parm(c0, PT_INT, i, 0.0f, "");
                std::vector<uint8_t> c0ch;
                put_chunk(c0ch, toc, "Condition", K_SCRIPT_CONDITION_VERSION_1, c0);
                std::vector<uint8_t> o0;
                put_bytes(o0, c0ch);
                std::vector<uint8_t> o0ch;
                put_chunk(o0ch, toc, "OrCondition", K_SCRIPT_OR_CONDITION_DATA_VERSION_1, o0);
                std::vector<uint8_t> s0;
                put_script_head(s0, "OobHeal12", "", "", "", 1, 1, 1, 1, 1, 0, nullptr);
                put_bytes(s0, o0ch);
                std::vector<uint8_t> s0ch;
                put_chunk(s0ch, toc, "Script", K_SCRIPT_DATA_VERSION_1, s0);
                std::vector<uint8_t> l0;
                put_bytes(l0, s0ch);
                std::vector<uint8_t> l0ch;
                put_chunk(l0ch, toc, "ScriptList", K_SCRIPT_LIST_DATA_VERSION_1, l0);
                std::vector<uint8_t> pp;
                put_bytes(pp, l0ch);
                std::vector<uint8_t> pch;
                put_chunk(pch, toc, "PlayerScriptsList", K_SCRIPTS_DATA_VERSION_1, pp);
                std::vector<uint8_t> b = toc_bytes(toc);
                put_bytes(b, pch);
                run_ps(out, "pscr_oob_heal12", b, o);
        }
        // the version-1 heal at count -1: the append store at [-1] (the
        // read loop consumed nothing)
        {
                std::vector<uint8_t> c0;
                put_cond_head(c0, CT_TEAM_INSIDE_AREA_PARTIALLY, 0, -1);
                std::vector<uint8_t> c0ch;
                put_chunk(c0ch, toc, "Condition", K_SCRIPT_CONDITION_VERSION_1, c0);
                std::vector<uint8_t> o0;
                put_bytes(o0, c0ch);
                std::vector<uint8_t> o0ch;
                put_chunk(o0ch, toc, "OrCondition", K_SCRIPT_OR_CONDITION_DATA_VERSION_1, o0);
                std::vector<uint8_t> s0;
                put_script_head(s0, "OobHealNeg", "", "", "", 1, 1, 1, 1, 1, 0, nullptr);
                put_bytes(s0, o0ch);
                std::vector<uint8_t> s0ch;
                put_chunk(s0ch, toc, "Script", K_SCRIPT_DATA_VERSION_1, s0);
                std::vector<uint8_t> l0;
                put_bytes(l0, s0ch);
                std::vector<uint8_t> l0ch;
                put_chunk(l0ch, toc, "ScriptList", K_SCRIPT_LIST_DATA_VERSION_1, l0);
                std::vector<uint8_t> pp;
                put_bytes(pp, l0ch);
                std::vector<uint8_t> pch;
                put_chunk(pch, toc, "PlayerScriptsList", K_SCRIPTS_DATA_VERSION_1, pp);
                std::vector<uint8_t> b = toc_bytes(toc);
                put_bytes(b, pch);
                run_ps(out, "pscr_oob_healneg", b, o);
        }
        // the kindof normalization: int 117 (the null terminator slot),
        // 118 (past the array) and -1
        {
                for (int idx = 117; idx <= 118; idx += 1) {
                        std::vector<uint8_t> c0;
                        put_cond_head(c0, CT_COUNTER, 0, 3);
                        put_parm(c0, PT_KIND_OF_PARAM, idx, 0.0f, "");
                        put_parm(c0, PT_INT, 0, 0.0f, "");
                        put_parm(c0, PT_INT, 0, 0.0f, "");
                        std::vector<uint8_t> c0ch;
                        put_chunk(c0ch, toc, "Condition", K_SCRIPT_CONDITION_VERSION_2, c0);
                        std::vector<uint8_t> o0;
                        put_bytes(o0, c0ch);
                        std::vector<uint8_t> o0ch;
                        put_chunk(o0ch, toc, "OrCondition",
                                  K_SCRIPT_OR_CONDITION_DATA_VERSION_1, o0);
                        std::vector<uint8_t> s0;
                        char name[32];
                        snprintf(name, sizeof name, "OobKindof%d", idx);
                        put_script_head(s0, name, "", "", "", 1, 1, 1, 1, 1, 0, nullptr);
                        put_bytes(s0, o0ch);
                        std::vector<uint8_t> s0ch;
                        put_chunk(s0ch, toc, "Script", K_SCRIPT_DATA_VERSION_1, s0);
                        std::vector<uint8_t> l0;
                        put_bytes(l0, s0ch);
                        std::vector<uint8_t> l0ch;
                        put_chunk(l0ch, toc, "ScriptList", K_SCRIPT_LIST_DATA_VERSION_1, l0);
                        std::vector<uint8_t> pp;
                        put_bytes(pp, l0ch);
                        std::vector<uint8_t> pch;
                        put_chunk(pch, toc, "PlayerScriptsList", K_SCRIPTS_DATA_VERSION_1, pp);
                        std::vector<uint8_t> b = toc_bytes(toc);
                        put_bytes(b, pch);
                        char id[32];
                        snprintf(id, sizeof id, "pscr_oob_kindof%d", idx);
                        run_ps(out, id, b, o);
                }
                // negative
                std::vector<uint8_t> c0;
                put_cond_head(c0, CT_COUNTER, 0, 3);
                put_parm(c0, PT_KIND_OF_PARAM, -1, 0.0f, "");
                put_parm(c0, PT_INT, 0, 0.0f, "");
                put_parm(c0, PT_INT, 0, 0.0f, "");
                std::vector<uint8_t> c0ch;
                put_chunk(c0ch, toc, "Condition", K_SCRIPT_CONDITION_VERSION_2, c0);
                std::vector<uint8_t> o0;
                put_bytes(o0, c0ch);
                std::vector<uint8_t> o0ch;
                put_chunk(o0ch, toc, "OrCondition", K_SCRIPT_OR_CONDITION_DATA_VERSION_1, o0);
                std::vector<uint8_t> s0;
                put_script_head(s0, "OobKindofNeg", "", "", "", 1, 1, 1, 1, 1, 0, nullptr);
                put_bytes(s0, o0ch);
                std::vector<uint8_t> s0ch;
                put_chunk(s0ch, toc, "Script", K_SCRIPT_DATA_VERSION_1, s0);
                std::vector<uint8_t> l0;
                put_bytes(l0, s0ch);
                std::vector<uint8_t> l0ch;
                put_chunk(l0ch, toc, "ScriptList", K_SCRIPT_LIST_DATA_VERSION_1, l0);
                std::vector<uint8_t> pp;
                put_bytes(pp, l0ch);
                std::vector<uint8_t> pch;
                put_chunk(pch, toc, "PlayerScriptsList", K_SCRIPTS_DATA_VERSION_1, pp);
                std::vector<uint8_t> b = toc_bytes(toc);
                put_bytes(b, pch);
                run_ps(out, "pscr_oob_kindofneg", b, o);
        }
}

// The 16-list bound: sixteen lists all land; the seventeenth's parser
// refuses and the refusal walks the whole file out.
static void corpus_pscr_sixteen(Out &out)
{
        TocB toc;
        std::vector<uint8_t> pp;
        for (int i = 0; i < 16; i++) {
                std::vector<uint8_t> s0;
                char name[32];
                snprintf(name, sizeof name, "S%d", i);
                put_script_head(s0, name, "", "", "", 1, 1, 1, 1, 1, 0, nullptr);
                std::vector<uint8_t> s0ch;
                put_chunk(s0ch, toc, "Script", K_SCRIPT_DATA_VERSION_1, s0);
                std::vector<uint8_t> l0;
                put_bytes(l0, s0ch);
                std::vector<uint8_t> l0ch;
                put_chunk(l0ch, toc, "ScriptList", K_SCRIPT_LIST_DATA_VERSION_1, l0);
                put_bytes(pp, l0ch);
        }
        std::vector<uint8_t> pch;
        put_chunk(pch, toc, "PlayerScriptsList", K_SCRIPTS_DATA_VERSION_1, pp);
        std::vector<uint8_t> b = toc_bytes(toc);
        put_bytes(b, pch);
        run_ps(out, "pscr_sixteen", b, RunOpts());
}

static void corpus_pscr_seventeen(Out &out)
{
        TocB toc;
        std::vector<uint8_t> pp;
        for (int i = 0; i < 17; i++) {
                std::vector<uint8_t> s0;
                char name[32];
                snprintf(name, sizeof name, "S%d", i);
                put_script_head(s0, name, "", "", "", 1, 1, 1, 1, 1, 0, nullptr);
                std::vector<uint8_t> s0ch;
                put_chunk(s0ch, toc, "Script", K_SCRIPT_DATA_VERSION_1, s0);
                std::vector<uint8_t> l0;
                put_bytes(l0, s0ch);
                std::vector<uint8_t> l0ch;
                put_chunk(l0ch, toc, "ScriptList", K_SCRIPT_LIST_DATA_VERSION_1, l0);
                put_bytes(pp, l0ch);
        }
        std::vector<uint8_t> pch;
        put_chunk(pch, toc, "PlayerScriptsList", K_SCRIPTS_DATA_VERSION_1, pp);
        std::vector<uint8_t> b = toc_bytes(toc);
        put_bytes(b, pch);
        run_ps(out, "pscr_seventeen", b, RunOpts());
}

// Two PlayerScriptsList chunks in one file: the statics' leftover clear —
// the second walk deletes the first's lists (last wins).
static void corpus_pscr_repeat(Out &out)
{
        TocB toc;
        std::vector<uint8_t> pp;
        for (int round = 0; round < 2; round++) {
                std::vector<uint8_t> inner;
                for (int i = 0; i < round + 1; i++) {
                        std::vector<uint8_t> s0;
                        char name[32];
                        snprintf(name, sizeof name, "R%dS%d", round, i);
                        put_script_head(s0, name, "", "", "", 1, 1, 1, 1, 1, 0, nullptr);
                        std::vector<uint8_t> s0ch;
                        put_chunk(s0ch, toc, "Script", K_SCRIPT_DATA_VERSION_1, s0);
                        std::vector<uint8_t> l0;
                        put_bytes(l0, s0ch);
                        std::vector<uint8_t> l0ch;
                        put_chunk(l0ch, toc, "ScriptList", K_SCRIPT_LIST_DATA_VERSION_1, l0);
                        put_bytes(inner, l0ch);
                }
                std::vector<uint8_t> pch;
                put_chunk(pch, toc, "PlayerScriptsList", K_SCRIPTS_DATA_VERSION_1, inner);
                put_bytes(pp, pch);
        }
        std::vector<uint8_t> b = toc_bytes(toc);
        put_bytes(b, pp);
        run_ps(out, "pscr_repeat", b, RunOpts());
}

// The map context: the registration the SidesList tail makes — a
// PlayerScriptsList chunk inside a SidesList chunk fires, one at the file
// root is out of scope and skips whole.
static void corpus_pscr_nested(Out &out)
{
        TocB toc;
        std::vector<uint8_t> s0;
        put_script_head(s0, "NestedScript", "", "", "", 1, 1, 1, 1, 1, 0, nullptr);
        std::vector<uint8_t> s0ch;
        put_chunk(s0ch, toc, "Script", K_SCRIPT_DATA_VERSION_1, s0);
        std::vector<uint8_t> l0;
        put_bytes(l0, s0ch);
        std::vector<uint8_t> l0ch;
        put_chunk(l0ch, toc, "ScriptList", K_SCRIPT_LIST_DATA_VERSION_1, l0);
        std::vector<uint8_t> psl;
        put_bytes(psl, l0ch);
        std::vector<uint8_t> pslch;
        put_chunk(pslch, toc, "PlayerScriptsList", K_SCRIPTS_DATA_VERSION_1, psl);
        // a root-level PlayerScriptsList: out of scope in the map context
        std::vector<uint8_t> rootPsl;
        put_bytes(rootPsl, l0ch);
        std::vector<uint8_t> rootPslch;
        put_chunk(rootPslch, toc, "PlayerScriptsList", K_SCRIPTS_DATA_VERSION_1, rootPsl);
        // the SidesList chunk carries the nested one
        std::vector<uint8_t> sides;
        put_bytes(sides, pslch);
        std::vector<uint8_t> sidesch;
        put_chunk(sidesch, toc, "SidesList", 3, sides);
        std::vector<uint8_t> b = toc_bytes(toc);
        put_bytes(b, sidesch);
        put_bytes(b, rootPslch);
        RunOpts o;
        o.mode = REG_NESTED;
        o.dump_keys = true;
        run_ps(out, "pscr_nested", b, o);
}

// The scoping family: chunks whose (label, parent) has no registration
// skip whole — a Script directly under PlayerScriptsList, a ScriptList
// under ScriptList, an OrCondition at the root, a ScriptGroup inside a
// ScriptGroup, and unknown labels.
static void corpus_pscr_scope(Out &out)
{
        TocB toc;
        // a Script directly under PlayerScriptsList: skipped
        std::vector<uint8_t> stray;
        put_script_head(stray, "Stray", "", "", "", 1, 1, 1, 1, 1, 0, nullptr);
        std::vector<uint8_t> straych;
        put_chunk(straych, toc, "Script", K_SCRIPT_DATA_VERSION_1, stray);
        // a valid list for the walk to find
        std::vector<uint8_t> s0;
        put_script_head(s0, "Scoped", "", "", "", 1, 1, 1, 1, 1, 0, nullptr);
        std::vector<uint8_t> s0ch;
        put_chunk(s0ch, toc, "Script", K_SCRIPT_DATA_VERSION_1, s0);
        std::vector<uint8_t> l0;
        put_bytes(l0, s0ch);
        // a ScriptList chunk inside the ScriptList chunk: skipped
        std::vector<uint8_t> innerL;
        put_bytes(innerL, s0ch);
        std::vector<uint8_t> innerLch;
        put_chunk(innerLch, toc, "ScriptList", K_SCRIPT_LIST_DATA_VERSION_1, innerL);
        put_bytes(l0, innerLch);
        std::vector<uint8_t> l0ch;
        put_chunk(l0ch, toc, "ScriptList", K_SCRIPT_LIST_DATA_VERSION_1, l0);
        // an OrCondition at the root: skipped
        std::vector<uint8_t> orc;
        std::vector<uint8_t> orcch;
        put_chunk(orcch, toc, "OrCondition", K_SCRIPT_OR_CONDITION_DATA_VERSION_1, orc);
        // a group chunk inside a group chunk: skipped (the group parser is
        // registered under ScriptList only)
        std::vector<uint8_t> g0;
        put_group_head(g0, "Outer", 1, nullptr);
        std::vector<uint8_t> g1;
        put_group_head(g1, "Inner", 1, nullptr);
        std::vector<uint8_t> g1ch;
        put_chunk(g1ch, toc, "ScriptGroup", K_SCRIPT_GROUP_DATA_VERSION_2, g1);
        put_bytes(g0, g1ch);
        std::vector<uint8_t> g0ch;
        put_chunk(g0ch, toc, "ScriptGroup", K_SCRIPT_GROUP_DATA_VERSION_2, g0);
        // an unknown label: skipped
        std::vector<uint8_t> unk;
        put_str(unk, "garbage");
        std::vector<uint8_t> unkch;
        put_chunk(unkch, toc, "MysteryChunk", 1, unk);

        std::vector<uint8_t> pp;
        put_bytes(pp, straych);
        put_bytes(pp, l0ch);
        put_bytes(pp, g0ch);
        std::vector<uint8_t> pch;
        put_chunk(pch, toc, "PlayerScriptsList", K_SCRIPTS_DATA_VERSION_1, pp);
        std::vector<uint8_t> b = toc_bytes(toc);
        put_bytes(b, pch);
        put_bytes(b, orcch);
        put_bytes(b, unkch);
        run_ps(out, "pscr_scope", b, RunOpts());
}

// Leftover bytes: a script chunk with trailing garbage after its parsed
// fields, a condition with a stray word — the debug asserts are no-ops,
// closeDataChunk skips the rest.
static void corpus_pscr_leftovers(Out &out)
{
        TocB toc;
        std::vector<uint8_t> s0;
        Int delay = 3;
        put_script_head(s0, "Leftover", "", "", "", 1, 1, 1, 1, 1, 0, &delay);
        put_i32(s0, 0xDEADBEEF);  // trailing garbage (skipped by close)
        std::vector<uint8_t> s0ch;
        put_chunk(s0ch, toc, "Script", K_SCRIPT_DATA_VERSION_2, s0);
        std::vector<uint8_t> c0;
        put_cond_head(c0, CT_TIMER_EXPIRED, 0, 1);
        put_parm(c0, PT_INT, 5, 0.0f, "");
        put_i32(c0, 0xCAFEBABE);  // trailing garbage
        std::vector<uint8_t> c0ch;
        put_chunk(c0ch, toc, "Condition", K_SCRIPT_CONDITION_VERSION_2, c0);
        std::vector<uint8_t> o0;
        put_bytes(o0, c0ch);
        std::vector<uint8_t> o0ch;
        put_chunk(o0ch, toc, "OrCondition", K_SCRIPT_OR_CONDITION_DATA_VERSION_1, o0);
        std::vector<uint8_t> s1;
        put_script_head(s1, "WithCond", "", "", "", 1, 1, 1, 1, 1, 0, nullptr);
        put_bytes(s1, o0ch);
        std::vector<uint8_t> s1ch;
        put_chunk(s1ch, toc, "Script", K_SCRIPT_DATA_VERSION_1, s1);
        std::vector<uint8_t> l0;
        put_bytes(l0, s0ch);
        put_bytes(l0, s1ch);
        std::vector<uint8_t> l0ch;
        put_chunk(l0ch, toc, "ScriptList", K_SCRIPT_LIST_DATA_VERSION_1, l0);
        std::vector<uint8_t> pp;
        put_bytes(pp, l0ch);
        std::vector<uint8_t> pch;
        put_chunk(pch, toc, "PlayerScriptsList", K_SCRIPTS_DATA_VERSION_1, pp);
        std::vector<uint8_t> b = toc_bytes(toc);
        put_bytes(b, pch);
        run_ps(out, "pscr_leftovers", b, RunOpts());
}

// Short files: chunk sizes that cut mid-string and mid-parameter — the
// zero-filled short reads, the negative dataLeft the close leaves.
static void corpus_pscr_short(Out &out)
{
        TocB toc;
        // a full script head, then declare the chunk with 10 fewer bytes:
        // the last string's read runs past, zero-fills, dataLeft goes
        // negative, closeDataChunk skips nothing
        std::vector<uint8_t> s0;
        put_script_head(s0, "ShortRead", "c", "", "", 1, 1, 1, 1, 1, 0, nullptr);
        // hand-build the chunk header with the truncated size
        std::vector<uint8_t> s0ch;
        {
                uint32_t id = toc.id("Script");
                put_u32(s0ch, id);
                put_u16(s0ch, K_SCRIPT_DATA_VERSION_2);
                put_i32(s0ch, (int32_t)(s0.size() - 10));
                put_bytes(s0ch, s0);
        }
        // a condition whose declared size cuts mid-parameter (the int and
        // real zero-fill, the string reads 0 bytes)
        std::vector<uint8_t> c0;
        put_cond_head(c0, CT_TIMER_EXPIRED, 0, 1);
        std::vector<uint8_t> c0ch;
        {
                std::vector<uint8_t> full = c0;
                put_i32(full, PT_INT);
                put_i32(full, 5);
                put_f32(full, 1.0f);
                put_str(full, "cut");
                uint32_t id = toc.id("Condition");
                put_u32(c0ch, id);
                put_u16(c0ch, K_SCRIPT_CONDITION_VERSION_2);
                put_i32(c0ch, (int32_t)(full.size() - 8));  // cut inside the real word
                put_bytes(c0ch, full);
        }
        std::vector<uint8_t> o0;
        put_bytes(o0, c0ch);
        std::vector<uint8_t> o0ch;
        put_chunk(o0ch, toc, "OrCondition", K_SCRIPT_OR_CONDITION_DATA_VERSION_1, o0);
        std::vector<uint8_t> s1;
        put_script_head(s1, "ShortCond", "", "", "", 1, 1, 1, 1, 1, 0, nullptr);
        put_bytes(s1, o0ch);
        std::vector<uint8_t> s1ch;
        put_chunk(s1ch, toc, "Script", K_SCRIPT_DATA_VERSION_1, s1);
        std::vector<uint8_t> l0;
        put_bytes(l0, s0ch);
        put_bytes(l0, s1ch);
        std::vector<uint8_t> l0ch;
        put_chunk(l0ch, toc, "ScriptList", K_SCRIPT_LIST_DATA_VERSION_1, l0);
        std::vector<uint8_t> pp;
        put_bytes(pp, l0ch);
        std::vector<uint8_t> pch;
        put_chunk(pch, toc, "PlayerScriptsList", K_SCRIPTS_DATA_VERSION_1, pp);
        std::vector<uint8_t> b = toc_bytes(toc);
        put_bytes(b, pch);
        run_ps(out, "pscr_short", b, RunOpts());
}

// No PlayerScriptsList at all; a file that is not a chunk stream.
static void corpus_pscr_none(Out &out)
{
        TocB toc;
        std::vector<uint8_t> l0;
        std::vector<uint8_t> l0ch;
        put_chunk(l0ch, toc, "ScriptList", K_SCRIPT_LIST_DATA_VERSION_1, l0);
        std::vector<uint8_t> b = toc_bytes(toc);
        put_bytes(b, l0ch);
        run_ps(out, "pscr_none", b, RunOpts());

        // no registration at all: the chunks skip whole
        TocB toc2;
        std::vector<uint8_t> s0;
        put_script_head(s0, "Nothing", "", "", "", 1, 1, 1, 1, 1, 0, nullptr);
        std::vector<uint8_t> s0ch;
        put_chunk(s0ch, toc2, "Script", K_SCRIPT_DATA_VERSION_1, s0);
        std::vector<uint8_t> l1;
        put_bytes(l1, s0ch);
        std::vector<uint8_t> l1ch;
        put_chunk(l1ch, toc2, "ScriptList", K_SCRIPT_LIST_DATA_VERSION_1, l1);
        std::vector<uint8_t> psl;
        put_bytes(psl, l1ch);
        std::vector<uint8_t> pslch;
        put_chunk(pslch, toc2, "PlayerScriptsList", K_SCRIPTS_DATA_VERSION_1, psl);
        std::vector<uint8_t> b2 = toc_bytes(toc2);
        put_bytes(b2, pslch);
        RunOpts o2;
        o2.mode = REG_NONE;
        run_ps(out, "pscr_unregistered", b2, o2);

        // not a chunk stream: the TOC never opens
        std::vector<uint8_t> bad;
        put_str(bad, "JUNKJUNKJUNK");
        put_i32(bad, 1);
        run_ps(out, "pscr_badmagic", bad, RunOpts());

        // a physically truncated file: the reads zero-fill past the end,
        // the walk finishes on what is there
        {
                TocB toc3;
                std::vector<uint8_t> s0;
                Int delay = 9;
                put_script_head(s0, "Trunc", "body that will not fit", "", "", 1, 1, 1, 1, 1,
                                0, &delay);
                std::vector<uint8_t> s0ch;
                put_chunk(s0ch, toc3, "Script", K_SCRIPT_DATA_VERSION_2, s0);
                std::vector<uint8_t> l0;
                put_bytes(l0, s0ch);
                std::vector<uint8_t> l0ch;
                put_chunk(l0ch, toc3, "ScriptList", K_SCRIPT_LIST_DATA_VERSION_1, l0);
                std::vector<uint8_t> psl;
                put_bytes(psl, l0ch);
                std::vector<uint8_t> pslch;
                put_chunk(pslch, toc3, "PlayerScriptsList", K_SCRIPTS_DATA_VERSION_1, psl);
                std::vector<uint8_t> full = toc_bytes(toc3);
                put_bytes(full, pslch);
                // cut mid-script-head (inside the comment string)
                std::vector<uint8_t> cut(full.begin(), full.begin() + full.size() - 14);
                run_ps(out, "pscr_truncfile", cut, RunOpts());
        }
}

// Negative counts: a condition with -3 parms (nothing read, the count
// check makes it FALSE), an action with -1 (NO_OP).
static void corpus_pscr_negcount(Out &out)
{
        TocB toc;
        std::vector<uint8_t> c0;
        put_cond_head(c0, CT_COUNTER, 0, -3);
        std::vector<uint8_t> c0ch;
        put_chunk(c0ch, toc, "Condition", K_SCRIPT_CONDITION_VERSION_2, c0);
        std::vector<uint8_t> o0;
        put_bytes(o0, c0ch);
        std::vector<uint8_t> o0ch;
        put_chunk(o0ch, toc, "OrCondition", K_SCRIPT_OR_CONDITION_DATA_VERSION_1, o0);
        std::vector<uint8_t> a0;
        put_i32(a0, AT_SET_FLAG);
        put_i32(a0, -1);
        std::vector<uint8_t> a0ch;
        put_chunk(a0ch, toc, "ScriptAction", K_SCRIPT_ACTION_VERSION_1, a0);
        std::vector<uint8_t> s0;
        put_script_head(s0, "NegCount", "", "", "", 1, 1, 1, 1, 1, 0, nullptr);
        put_bytes(s0, o0ch);
        put_bytes(s0, a0ch);
        std::vector<uint8_t> s0ch;
        put_chunk(s0ch, toc, "Script", K_SCRIPT_DATA_VERSION_1, s0);
        std::vector<uint8_t> l0;
        put_bytes(l0, s0ch);
        std::vector<uint8_t> l0ch;
        put_chunk(l0ch, toc, "ScriptList", K_SCRIPT_LIST_DATA_VERSION_1, l0);
        std::vector<uint8_t> pp;
        put_bytes(pp, l0ch);
        std::vector<uint8_t> pch;
        put_chunk(pch, toc, "PlayerScriptsList", K_SCRIPTS_DATA_VERSION_1, pp);
        std::vector<uint8_t> b = toc_bytes(toc);
        put_bytes(b, pch);
        run_ps(out, "pscr_negcount", b, RunOpts());
}

}  // namespace

int main(int argc, char **argv)
{
        if (argc != 2) {
                fprintf(stderr, "usage: gen_mapscripts_golden <output-file>\n");
                return 1;
        }
        FILE *f = fopen(argv[1], "w");
        if (!f) {
                fprintf(stderr, "error: cannot open %s\n", argv[1]);
                return 1;
        }
        Out out;
        out.f = f;
        fprintf(f, "# Grammar: file/valid/walk/perr/pnone/pn/plist/pscr/pgrp/pcond/pact/pparm/ptpl/pkey/pkeyn/pstate.\n");

        corpus_pscr_registry(out);
        corpus_pscr_empty(out);
        corpus_pscr_full(out);
        corpus_pscr_group_vers(out);
        corpus_pscr_script_vers(out);
        corpus_pscr_cond_vers(out);
        corpus_pscr_action_vers(out);
        corpus_pscr_parm_raw(out);
        corpus_pscr_heals_cond(out);
        corpus_pscr_heals_act(out);
        corpus_pscr_rematch(out);
        corpus_pscr_kindof(out);
        corpus_pscr_kindof_throw(out);
        corpus_pscr_objstatus(out);
        corpus_pscr_rename(out);
        corpus_pscr_oob(out);
        corpus_pscr_sixteen(out);
        corpus_pscr_seventeen(out);
        corpus_pscr_repeat(out);
        corpus_pscr_nested(out);
        corpus_pscr_scope(out);
        corpus_pscr_leftovers(out);
        corpus_pscr_short(out);
        corpus_pscr_none(out);
        corpus_pscr_negcount(out);

        fclose(f);
        return 0;
}
