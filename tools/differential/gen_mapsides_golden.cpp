// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Golden-vector driver for `rzh-formats::map_sideslist` (parity
// level P0). Copyright 2025 Electronic Arts Inc.; Rust port Copyright
// 2026 the RustZeroHour contributors.
//
// Verbatim transcription of the Zero Hour sides-list load path —
// Core/GameEngine/Source/GameLogic/Map/SidesList.cpp at the baseline
// (571a7c5) — over the re-transcribed map data-chunk container
// (Common/System/DataChunk.{h,cpp}, the reader half verbatim from
// gen_mapchunk_golden.cpp):
//
//   - SidesList::ParseSidesDataChunk, the SidesList chunk reader the
//     constructor's logical-data-only load registers: the parse-start
//     clear, the MAX_PLAYER_COUNT-bounded side loop (a dict per side,
//     the build list per side with its version-3 tail — the location's
//     z forced to ground level, the numRebuilds int-to-unsigned word),
//     the version-2 team list, and the nested PlayerScriptsList walk —
//     the script-engine slice's parser (plan §3.4 WP7); nothing is
//     registered for it here, so the nested walk (and the port's)
//     skips those chunks whole;
//   - SidesList::validateSides and its collaborators, the post-load
//     cleanup: the neutral-player ensure (addPlayerByTemplate's empty
//     name — the reserved Neutral marker — its L"Neutral" display
//     name, and its "team" default), the per-player default-team
//     ensure ("team"+playerName, the owner fix and the singleton
//     force), validateAllyEnemyList's token cleanup (the " \n\r\t"
//     tokenizing, the self-reference and unknown-name drops, the
//     single-space rejoin), the team-name/player-name collision
//     removal with its goto restart (removeTeam's last-team refusal —
//     the infinite loop — kept as the marker; unreachable through this
//     load path, see the solo-collision corpus note), and the owner
//     reparent of orphaned or self-owned teams to the neutral name;
//   - the constructor's registerParser("SidesList", "", ...)/parse
//     walk shape the harness drives (global scope, last-wins repeated
//     chunks, skipped unknown labels) and the registration-time
//     emptySides the absent-payload state models.
//
// Deliberate deviations, pinned identically on both sides:
//
// - The sides' and teams' linked lists and arrays (SidesInfo's
//   m_pBuildList, the MAX_PLAYER_COUNT m_sides slots, TeamsInfoRec's
//   doubling array) are vectors: the build list appends at the j-th
//   position of a j-entry list (file order), the sides array is
//   bounded by the same MAX_PLAYER_COUNT guard, and the team array's
//   allocation is not an observable.
// - The nested scripts registration is the script-engine slice's; the
//   transcription transcribes the parse call (nothing registered —
//   the walk skips PlayerScriptsList chunks whole) and the scripts
//   read-back is dropped with it (a documented slice boundary, the
//   same shape the port takes).
// - The scripts' read-back into the sides (deleteInstance of the old
//   lists) is the script slice's; validateSides runs on the sides and
//   teams exactly as the original sequences it.
// - The name-key interning: the harness seeds the nine well-known keys
//   this load consults, in WellKnownKeys.h's declaration order
//   (teamName 65, teamOwner 72, teamIsSingleton 79, playerName 800,
//   playerIsHuman 807, playerDisplayName 821, playerFaction 828,
//   playerEnemies 835, playerAllies 842) — the same relative order the
//   original's full startup table gives them, so the dict storage
//   order agrees for any dict over these keys.
// - The typed Dict getters (getAsciiString(key), getBool(key)) are
//   completed into the container copy (the same stance as the
//   world-info generator's getInt).
// - The container transcription (the reader half) is otherwise verbatim
//   from gen_mapchunk_golden.cpp — see that file's header for its own
//   deviation notes.
//
// Output grammar (one command per line, hex lowercase, '-' = empty):
//
//   file <id> <hex>               the corpus bytes
//   valid <id> <t|f>              isValidFileType() after construction
//   walk <id> <t|f>               the parse() verdict
//   serr <id> corrupt             the walk itself refused
//   serr <id> dict <type>         a side or team dict's corrupt pair type
//   serr <id> teamloop            the (unreachable) removal-loop refusal
//   snone <id>                    no SidesList chunk fired
//   sn <id> <sides> <teams>       the final counts
//   sside <id> <i> <npairs> <nbuild>  side i's dict pair and build counts
//   spair <id> <i> <name> <value> side i's dict pairs (stored order):
//                                 b <t|f>, i <int>, r <bits>,
//                                 a <q(name)>, u <hex units>
//   sbuild <id> <i> <j> <building> <template> <x> <y> <z> <angle> <built>
//          <rebuilds> <script> <health> <whiner> <unsell> <repair>
//                                 build entry j of side i (the location
//                                 and angle as f32 bits, z always 0)
//   steamn <id> <i> <npairs>      team i's dict pair count
//   stpair <id> <i> <name> <value>  team i's dict pairs
//   smod <id> <t|f>               validateSides' modified flag
//   skey <id> <ordinal> <name>    the final name-key table, ordinal order
//   sstate <id> <tell> <depth> <eof>  post-walk stream state
//
// Usage: gen_mapsides_golden <output-file>
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

// ---------------------------------------------------------------------------
// the sides-list payload (SidesList.cpp, load-path transcription)
// ---------------------------------------------------------------------------

// SidesList.cpp's own version words (statically defined there).
#define K_SIDES_DATA_VERSION_1 1
#define K_SIDES_DATA_VERSION_2 2 // includes Team list.
#define K_SIDES_DATA_VERSION_3 3 // includes Team list.

// GameCommon.h: MAX_PLAYER_COUNT.
#define MAX_PLAYER_COUNT 16

// GameType.h: Coord3D (the float triple).
struct Coord3D {
        Real x = 0, y = 0, z = 0;
};

// The build-list entry (BuildListInfo's saved members as the load
// leaves them). The version-3 gates skip the script/health/whiner/
// unsellable/repairable reads below 3, leaving the constructor's
// defaults (script empty, health 100, whiner true, unsellable false,
// repairable true) — pinned as load observables. The editor/solo-AI
// half (m_renderObj, m_objectID, the gatherers, m_rallyPointOffset
// which the load never writes) is not map-file state.
struct BuildListLoad {
        AsciiString m_buildingName;
        AsciiString m_templateName;
        Coord3D m_location;         // z forced to ground level (0)
        Real m_angle = 0;
        Bool m_isInitiallyBuilt = false;
        UnsignedInt m_numRebuilds = 0;
        AsciiString m_script;
        Int m_health = 100;
        Bool m_whiner = true;
        Bool m_unsellable = false;
        Bool m_repairable = true;
};

// One side (SidesInfo's load-path half): the player dict and the build
// list (the linked list is the vector — addToBuildList(p, j) appends at
// the j-th position of a j-entry list, i.e. in file order).
struct SidesInfoLoad {
        Dict m_dict;
        std::vector<BuildListLoad> m_buildList;
};

// The SidesList singleton's load-path state: the sides (bounded by
// MAX_PLAYER_COUNT), the teams and validateSides' modified flag.
struct SidesListLoad {
        std::vector<SidesInfoLoad> m_sides;
        std::vector<Dict> m_teams;
        Bool m_modified = false;
};

// The well-known keys this load consults (WellKnownKeys.h's
// StaticNameKey startup interning — the declaration order: teamName 65,
// teamOwner 72, teamIsSingleton 79, playerName 800, playerIsHuman 807,
// playerDisplayName 821, playerFaction 828, playerEnemies 835,
// playerAllies 842).
struct SidesWellKnownKeys {
        NameKeyType teamName = 0, teamOwner = 0, teamIsSingleton = 0;
        NameKeyType playerName = 0, playerIsHuman = 0, playerDisplayName = 0;
        NameKeyType playerFaction = 0, playerEnemies = 0, playerAllies = 0;

        void seed(NameKeyGenerator &keys)
        {
                teamName = keys.nameToKey("teamName");
                teamOwner = keys.nameToKey("teamOwner");
                teamIsSingleton = keys.nameToKey("teamIsSingleton");
                playerName = keys.nameToKey("playerName");
                playerIsHuman = keys.nameToKey("playerIsHuman");
                playerDisplayName = keys.nameToKey("playerDisplayName");
                playerFaction = keys.nameToKey("playerFaction");
                playerEnemies = keys.nameToKey("playerEnemies");
                playerAllies = keys.nameToKey("playerAllies");
        }
};

// The transcription's typed Dict accessors (Dict.cpp's getters, the
// same "completed into the container copy" stance as the world-info
// generator's getInt).
static AsciiString dictGetAsciiString(const Dict &d, NameKeyType key)
{
        const Dict::DictPair *pair = d.findPairByKey(key);
        if (pair && pair->getType() == Dict::DICT_ASCIISTRING)
                return pair->as;
        return AsciiString();
}
static Bool dictGetBool(const Dict &d, NameKeyType key)
{
        const Dict::DictPair *pair = d.findPairByKey(key);
        if (pair && pair->getType() == Dict::DICT_BOOL)
                return pair->b;
        return false;
}

// SidesList::findSideInfo — the first side whose playerName matches.
static Int findSideInfo(SidesListLoad &st, const SidesWellKnownKeys &wk, const AsciiString &name)
{
        for (Int i = 0; i < (Int)st.m_sides.size(); ++i) {
                if (dictGetAsciiString(st.m_sides[i].m_dict, wk.playerName) == name)
                        return i;
        }
        return -1;
}

// TeamsInfoRec::findTeamInfo — the first team whose teamName matches.
static Int findTeamInfo(SidesListLoad &st, const SidesWellKnownKeys &wk, const AsciiString &name)
{
        for (Int i = 0; i < (Int)st.m_teams.size(); ++i) {
                if (dictGetAsciiString(st.m_teams[i], wk.teamName) == name)
                        return i;
        }
        return -1;
}

// SidesList::addSide — the bounded append.
static void addSide(SidesListLoad &st, const Dict *d)
{
        if (st.m_sides.size() < MAX_PLAYER_COUNT)
                st.m_sides.push_back(SidesInfoLoad{ *d, {} });
}

// SidesList::addTeam.
static void addTeam(SidesListLoad &st, const Dict *d)
{
        st.m_teams.push_back(*d);
}

// TeamsInfoRec::removeTeam — refuses the last team (and out-of-range),
// the refusal validateSides' goto turns into an infinite loop (the
// marker below); the shift-and-clear tail is the vector's erase.
static Bool removeTeam(SidesListLoad &st, Int i)
{
        if (i < 0 || i >= (Int)st.m_teams.size() || st.m_teams.size() <= 1)
                return false;
        st.m_teams.erase(st.m_teams.begin() + i);
        return true;
}

// The single-team collision marker: validateSides' goto restarts the
// name-collision loop, removeTeam refuses the last team, and the
// original spins forever — the transcription refuses the load the way
// the port does.
struct TeamRemovalLoop {
};

// AsciiString::nextToken — the default separators " \n\r\t", the
// mutated remainder, the non-empty token.
static Bool nextToken(AsciiString &str, AsciiString &tok)
{
        static const char *seps = " \n\r\t";
        if (str.s.empty()) {
                tok.s.clear();
                return false;
        }
        size_t start = str.s.find_first_not_of(seps);
        if (start == std::string::npos) {
                str.s.clear();
                tok.s.clear();
                return false;
        }
        size_t end = str.s.find_first_of(seps, start);
        if (end == std::string::npos)
                end = str.s.size();
        tok.s = str.s.substr(start, end - start);
        str.s = str.s.substr(end);
        return true;
}

// SidesList::validateAllyEnemyList — owners/allies/enemies must be
// players: drop the self-reference and the unknown names, rejoin with
// single spaces.
static Bool validateAllyEnemyList(SidesListLoad &st, const SidesWellKnownKeys &wk,
                                  const AsciiString &tname, AsciiString &allies)
{
        Bool modified = false;

        AsciiString str, newstr, token;
        str = allies;
        newstr.s.clear();
        while (nextToken(str, token)) {
                if (token == tname) {
                        modified = true;
                        continue; // no allies/enemies with self
                }
                if (findSideInfo(st, wk, token) < 0) {
                        modified = true;
                        continue; // player not found.
                }
                if (!newstr.s.empty())
                        newstr.s += " ";
                newstr.s += token.s;
        }
        allies = newstr;
        return modified;
}

// SidesList::addPlayerByTemplate — the empty-template neutral player:
// the empty name (the reserved Neutral marker), the L"Neutral"
// display name, human false, the empty faction, and the default
// "team" team.
static void addPlayerByTemplate(SidesListLoad &st, NameKeyGenerator &keys,
                                const SidesWellKnownKeys &wk)
{
        AsciiString playerName; // "" — magic code for "neutral"
        UnicodeString playerDisplayName;
        const char *neutral = "Neutral";
        for (const char *p = neutral; *p; ++p)
                playerDisplayName.u.push_back((WideChar)*p);
        Bool isHuman = false;

        Dict d;
        d.setAsciiString(wk.playerName, playerName);
        d.setBool(wk.playerIsHuman, isHuman);
        d.setUnicodeString(wk.playerDisplayName, playerDisplayName);
        d.setAsciiString(wk.playerFaction, AsciiString());
        d.setAsciiString(wk.playerAllies, AsciiString());
        d.setAsciiString(wk.playerEnemies, AsciiString());
        addSide(st, &d);

        AsciiString playerTeamName;
        playerTeamName.s = "team";
        playerTeamName.s += playerName.s;

        d = Dict();
        d.setAsciiString(wk.teamName, playerTeamName);
        d.setAsciiString(wk.teamOwner, playerName);
        d.setBool(wk.teamIsSingleton, true);
        addTeam(st, &d);
        (void)keys;
}

// SidesList::validateSides — the post-load cleanup: the neutral player
// ensure, the per-player default team ("team"+playerName) with its
// owner fix and singleton force, the allies/enemies cleanup, the
// team-name/player-name collision removal (the goto restart), and the
// owner reparent of orphaned or self-owned teams.
static void validateSides(SidesListLoad &st, NameKeyGenerator &keys,
                          const SidesWellKnownKeys &wk)
{
        // ensure we have at least one player, and at least one neutral player.
        Int i;
        Int neutral = -1;
        Int num = (Int)st.m_sides.size();
        for (i = 0; i < num; i++) {
                if (dictGetAsciiString(st.m_sides[i].m_dict, wk.playerName).s.empty()) {
                        neutral = i;
                        break;
                }
        }
        if (neutral == -1) {
                addPlayerByTemplate(st, keys, wk);
                st.m_modified = true;
        }

        // now ensure that every player has a proper "default team"
        for (i = 0; i < (Int)st.m_sides.size(); i++) {
                Dict &pdict = st.m_sides[i].m_dict;
                AsciiString pname = dictGetAsciiString(pdict, wk.playerName);
                AsciiString tname;
                tname.s = "team";
                tname.s += pname.s;
                Int ti = findTeamInfo(st, wk, tname);
                if (ti >= 0) {
                        // make sure the team owner points back to the player.
                        if (dictGetAsciiString(st.m_teams[ti], wk.teamOwner) != pname) {
                                st.m_teams[ti].setAsciiString(wk.teamOwner, pname);
                                st.m_modified = true;
                        }
                        // default teams are always singletons.
                        if (!dictGetBool(st.m_teams[ti], wk.teamIsSingleton)) {
                                st.m_teams[ti].setBool(wk.teamIsSingleton, true);
                                st.m_modified = true;
                        }
                } else {
                        Dict d;
                        d.setAsciiString(wk.teamName, tname);
                        d.setAsciiString(wk.teamOwner, pname);
                        d.setBool(wk.teamIsSingleton, true);
                        addTeam(st, &d);
                        st.m_modified = true;
                }

                AsciiString allies = dictGetAsciiString(pdict, wk.playerAllies);
                AsciiString enemies = dictGetAsciiString(pdict, wk.playerEnemies);

                // ensure all teams have valid allies & enemies.
                // (note that owners can be teams or players, but allies/enemies can only be teams.)
                if (validateAllyEnemyList(st, wk, pname, allies)) {
                        pdict.setAsciiString(wk.playerAllies, allies);
                        st.m_modified = true;
                }
                if (validateAllyEnemyList(st, wk, pname, enemies)) {
                        pdict.setAsciiString(wk.playerEnemies, enemies);
                        st.m_modified = true;
                }
        }

        // ensure there's no overlap between team names and player names.
        // (if there is, the player wins and the team is whacked.)
validate_team_names:
        for (i = 0; i < (Int)st.m_teams.size(); i++) {
                AsciiString tname = dictGetAsciiString(st.m_teams[i], wk.teamName);
                if (findSideInfo(st, wk, tname) >= 0) {
                        if (!removeTeam(st, i))
                                throw TeamRemovalLoop(); // the marker (see removeTeam)
                        st.m_modified = true;
                        goto validate_team_names;
                }
        }

        for (i = 0; i < (Int)st.m_teams.size(); i++) {
                AsciiString tname = dictGetAsciiString(st.m_teams[i], wk.teamName);
                AsciiString towner = dictGetAsciiString(st.m_teams[i], wk.teamOwner);
                if (findSideInfo(st, wk, towner) < 0 || towner == tname) {
                        st.m_teams[i].setAsciiString(wk.teamOwner, AsciiString());
                        st.m_modified = true;
                }
        }
}

// SidesList::ParseSidesDataChunk — the SidesList chunk reader: the
// bounded side loop (a dict, the build list), the version-2 team list,
// the nested PlayerScriptsList walk (the script-engine slice's parser
// — nothing is registered here, so the walk skips those chunks whole)
// and validateSides.
static Bool parseSidesDataChunk(DataChunkInput &file, DataChunkInfo *info, void *userData);

// The walk's user state.
struct SdState {
        SidesListLoad m_sidesList;  // TheSidesList
        NameKeyGenerator m_keys;    // TheNameKeyGenerator
        SidesWellKnownKeys m_wk;
        Bool m_fired = false;

        enum ErrKind { ERR_NONE = 0, ERR_CORRUPT, ERR_DICT, ERR_TEAMLOOP } errKind = ERR_NONE;
        Int errDictType = 0;
};

static Bool parseSidesDataChunk(DataChunkInput &file, DataChunkInfo *info, void *userData)
{
        SdState *st = (SdState *)userData;
        st->m_fired = true;
        try {
                // TheSidesList->clear(); — emptySides + emptyTeams (the
                // second emptySides below is the original's own redundancy)
                st->m_sidesList.m_sides.clear();
                st->m_sidesList.m_teams.clear();
                Int count = file.readInt();
                Int i, j;
                for (i = 0; i < count; i++) {
                        if (i >= MAX_PLAYER_COUNT)
                                break; // the mid-chunk break: the 17th side's bytes stay unconsumed
                        Dict d = file.readDict(st->m_keys);
                        addSide(st->m_sidesList, &d);
                        Int buildCount = file.readInt();
                        for (j = 0; j < buildCount; j++) {
                                BuildListLoad pBuildList; // newInstance(BuildListInfo)
                                pBuildList.m_buildingName = file.readAsciiString();
                                pBuildList.m_templateName = file.readAsciiString();
                                Coord3D loc;
                                loc.x = file.readReal();
                                loc.y = file.readReal();
                                loc.z = file.readReal();
                                loc.z = 0; // force to ground level
                                pBuildList.m_location = loc;
                                pBuildList.m_angle = file.readReal();
                                pBuildList.m_isInitiallyBuilt = file.readByte() ? true : false;
                                pBuildList.m_numRebuilds = (UnsignedInt)file.readInt();
                                if (info->version >= K_SIDES_DATA_VERSION_3) {
                                        pBuildList.m_script = file.readAsciiString();
                                        pBuildList.m_health = file.readInt();
                                        pBuildList.m_whiner = file.readByte() ? true : false;
                                        pBuildList.m_unsellable = file.readByte() ? true : false;
                                        pBuildList.m_repairable = file.readByte() ? true : false;
                                }
                                // addToBuildList(pBuildList, j) — the j-th position of a
                                // j-entry list: the append (file order)
                                st->m_sidesList.m_sides.back().m_buildList.push_back(pBuildList);
                        }
                }
                if (info->version >= K_SIDES_DATA_VERSION_2) {
                        count = file.readInt(); // (after the mid-chunk break: the 17th side's dict words)
                        st->m_sidesList.m_teams.clear();
                        for (i = 0; i < count; i++) {
                                Dict d = file.readDict(st->m_keys);
                                addTeam(st->m_sidesList, &d);
                        }
                }

                // file.registerParser("PlayerScriptsList", info->label,
                //     ScriptList::ParseScriptsDataChunk); — the script-engine
                // slice's parser (plan §3.4 WP7); nothing is registered here,
                // so the nested walk below skips those chunks whole.
                if (!file.parse(nullptr)) {
                        throw DictTypeCorrupt{ 0 }; // (unreachable with no parser registered)
                }
                // (the scripts read back into the sides — the script slice's)

                validateSides(st->m_sidesList, st->m_keys, st->m_wk);
        } catch (DictTypeCorrupt &e) {
                st->errKind = SdState::ERR_DICT;
                st->errDictType = (Int)e.type;
                return false; // (the escaping ERROR_CORRUPT_FILE_FORMAT's stop)
        } catch (TeamRemovalLoop &) {
                st->errKind = SdState::ERR_TEAMLOOP;
                return false;
        }
        return true;
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
                line("file " + std::string(id) + " " + hx(b));
        }
        void tf(const char *tag, const char *id, Bool v)
        {
                line(std::string(tag) + " " + id + (v ? " t" : " f"));
        }
        void state(const char *id, DataChunkInput &in)
        {
                line(std::string("sstate ") + id + " " + std::to_string((Int)in.tell()) + " " +
                     std::to_string(in.depth()) + " " + (in.atEndOfFile() ? "t" : "f"));
        }
};

// hex of a byte vector ('-' for empty).
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

// UTF-16 units as %04x words.
static std::string hxu(const std::vector<WideChar> &u)
{
        if (u.empty())
                return "-";
        std::string out;
        for (WideChar w : u) {
                char buf[8];
                snprintf(buf, sizeof buf, "%04x", (unsigned)w);
                out += buf;
        }
        return out;
}

// --- byte builders ---------------------------------------------------------

static void put_u16(std::vector<uint8_t> &b, uint16_t v)
{
        b.push_back((uint8_t)v);
        b.push_back((uint8_t)(v >> 8));
}
static void put_u32(std::vector<uint8_t> &b, uint32_t v)
{
        b.push_back((uint8_t)v);
        b.push_back((uint8_t)(v >> 8));
        b.push_back((uint8_t)(v >> 16));
        b.push_back((uint8_t)(v >> 24));
}
static void put_i32(std::vector<uint8_t> &b, int32_t v) { put_u32(b, (uint32_t)v); }
static void put_f32(std::vector<uint8_t> &b, Real v) { put_u32(b, f32bits(v)); }
static void put_str(std::vector<uint8_t> &b, const std::string &s)
{
        b.insert(b.end(), s.begin(), s.end());
}
static void put_bytes(std::vector<uint8_t> &b, const std::vector<uint8_t> &s)
{
        b.insert(b.end(), s.begin(), s.end());
}

// one chunk: id, version, data size, data.
static void put_chunk(std::vector<uint8_t> &b, uint32_t id, uint16_t ver,
                      const std::vector<uint8_t> &data)
{
        put_u32(b, id);
        put_u16(b, ver);
        put_i32(b, (int32_t)data.size());
        b.insert(b.end(), data.begin(), data.end());
}

// a table of contents: tag, count, entries.
static std::vector<uint8_t> toc_bytes(const std::vector<std::pair<std::string, uint32_t>> &entries)
{
        std::vector<uint8_t> b;
        put_str(b, "CkMp");
        put_i32(b, (int32_t)entries.size());
        for (auto &e : entries) {
                b.push_back((uint8_t)e.first.size());
                put_str(b, e.first);
                put_u32(b, e.second);
        }
        return b;
}

// --- dict pair builders (keyAndType = toc id << 8 | type) --------------------

static std::vector<uint8_t> pair_bool(uint32_t key_id, Bool v)
{
        std::vector<uint8_t> p;
        put_i32(p, (int32_t)((key_id << 8) | 0));
        p.push_back(v ? 1 : 0);
        return p;
}
static std::vector<uint8_t> pair_int(uint32_t key_id, Int v)
{
        std::vector<uint8_t> p;
        put_i32(p, (int32_t)((key_id << 8) | 1));
        put_i32(p, v);
        return p;
}
static std::vector<uint8_t> pair_real(uint32_t key_id, Real v)
{
        std::vector<uint8_t> p;
        put_i32(p, (int32_t)((key_id << 8) | 2));
        put_f32(p, v);
        return p;
}
static std::vector<uint8_t> pair_ascii(uint32_t key_id, const std::string &s)
{
        std::vector<uint8_t> p;
        put_i32(p, (int32_t)((key_id << 8) | 3));
        put_u16(p, (uint16_t)s.size());
        put_str(p, s);
        return p;
}
static std::vector<uint8_t> pair_unicode(uint32_t key_id, const std::vector<uint16_t> &units)
{
        std::vector<uint8_t> p;
        put_i32(p, (int32_t)((key_id << 8) | 4));
        put_u16(p, (uint16_t)units.size());
        for (uint16_t u : units)
                put_u16(p, u);
        return p;
}
static std::vector<uint8_t> pair_raw_type(uint32_t key_id, uint32_t type)
{
        // the corrupt-type corpus: an impossible type byte
        std::vector<uint8_t> p;
        put_i32(p, (int32_t)((key_id << 8) | type));
        put_i32(p, 0);
        return p;
}

// --- sides payload builders --------------------------------------------------

// the standard corpus key ids (the TOC entries the corpora build)
enum SidesKeys {
        SK_playerName = 2,
        SK_playerIsHuman = 3,
        SK_playerDisplayName = 4,
        SK_playerFaction = 5,
        SK_playerEnemies = 6,
        SK_playerAllies = 7,
        SK_teamName = 8,
        SK_teamOwner = 9,
        SK_teamIsSingleton = 10,
        SK_playerMoney = 11
};

// one side's payload: the dict, the build count, the entries. The build
// entry shape is version-gated (the v3 tail from version 3).
static std::vector<uint8_t> side_payload(const std::vector<std::vector<uint8_t>> &pairs,
                                         const std::vector<std::vector<uint8_t>> &builds)
{
        std::vector<uint8_t> p;
        put_u16(p, (uint16_t)pairs.size());
        for (const auto &pr : pairs)
                put_bytes(p, pr);
        put_i32(p, (Int)builds.size());
        for (const auto &b : builds)
                put_bytes(p, b);
        return p;
}

// one build-list entry (a fresh structure; the v3 tail gated).
static std::vector<uint8_t> build_entry(uint16_t version, const std::string &building,
                                        const std::string &tpl, Real x, Real y, Real z, Real angle,
                                        Bool built, Int rebuilds, const std::string &script,
                                        Int health, Bool whiner, Bool unsellable, Bool repairable)
{
        std::vector<uint8_t> p;
        put_u16(p, (uint16_t)building.size());
        put_str(p, building);
        put_u16(p, (uint16_t)tpl.size());
        put_str(p, tpl);
        put_f32(p, x);
        put_f32(p, y);
        put_f32(p, z);
        put_f32(p, angle);
        p.push_back(built ? 1 : 0);
        put_i32(p, rebuilds);
        if (version >= K_SIDES_DATA_VERSION_3) {
                put_u16(p, (uint16_t)script.size());
                put_str(p, script);
                put_i32(p, health);
                p.push_back(whiner ? 1 : 0);
                p.push_back(unsellable ? 1 : 0);
                p.push_back(repairable ? 1 : 0);
        }
        return p;
}

// a team's payload: the dict alone.
static std::vector<uint8_t> team_payload(const std::vector<std::vector<uint8_t>> &pairs)
{
        std::vector<uint8_t> p;
        put_u16(p, (uint16_t)pairs.size());
        for (const auto &pr : pairs)
                put_bytes(p, pr);
        return p;
}

// the standard sides TOC (SidesList + the well-known names).
static std::vector<uint8_t> sides_toc()
{
        return toc_bytes({{ "SidesList", 1 },
                          { "playerName", SK_playerName },
                          { "playerIsHuman", SK_playerIsHuman },
                          { "playerDisplayName", SK_playerDisplayName },
                          { "playerFaction", SK_playerFaction },
                          { "playerEnemies", SK_playerEnemies },
                          { "playerAllies", SK_playerAllies },
                          { "teamName", SK_teamName },
                          { "teamOwner", SK_teamOwner },
                          { "teamIsSingleton", SK_teamIsSingleton },
                          { "playerMoney", SK_playerMoney }});
}

// a side's dict pairs: the standard player shape.
static std::vector<std::vector<uint8_t>> player_pairs(const std::string &name, Bool human,
                                                      const std::string &faction,
                                                      const std::string &allies,
                                                      const std::string &enemies)
{
        std::vector<std::vector<uint8_t>> pairs;
        pairs.push_back(pair_ascii(SK_playerName, name));
        pairs.push_back(pair_bool(SK_playerIsHuman, human));
        pairs.push_back(pair_ascii(SK_playerFaction, faction));
        pairs.push_back(pair_ascii(SK_playerAllies, allies));
        pairs.push_back(pair_ascii(SK_playerEnemies, enemies));
        return pairs;
}

// a team's dict pairs: the standard shape.
static std::vector<std::vector<uint8_t>> team_pairs(const std::string &name,
                                                    const std::string &owner, Bool singleton)
{
        std::vector<std::vector<uint8_t>> pairs;
        pairs.push_back(pair_ascii(SK_teamName, name));
        pairs.push_back(pair_ascii(SK_teamOwner, owner));
        pairs.push_back(pair_bool(SK_teamIsSingleton, singleton));
        return pairs;
}

// --- the harness (the constructor's logical walk, sides only) ----------------

// the dict dump lines for one dict (the pairs in stored order)
static void dump_dict(Out &out, const char *cmd, const char *id, Int index, const Dict &d,
                      const NameKeyGenerator &keys)
{
        for (Int pidx = 0; pidx < d.getPairCount(); ++pidx) {
                const Dict::DictPair &pair = d.m_pairs[pidx];
                std::string name = keys.keyToName(pair.getName()).s;
                std::string v;
                char buf[16];
                switch (pair.getType()) {
                        case Dict::DICT_BOOL:
                                v = std::string("b ") + (pair.b ? "t" : "f");
                                break;
                        case Dict::DICT_INT:
                                v = "i " + std::to_string(pair.i);
                                break;
                        case Dict::DICT_REAL:
                                snprintf(buf, sizeof buf, "r %08x", f32bits(pair.r));
                                v = buf;
                                break;
                        case Dict::DICT_ASCIISTRING:
                                v = "a " + q(pair.as.s);
                                break;
                        case Dict::DICT_UNICODESTRING:
                                v = "u " + hxu(pair.us.u);
                                break;
                        default:
                                v = "?";
                                break;
                }
                out.line(std::string(cmd) + " " + id + " " + std::to_string(index) + " " + q(name) +
                         " " + v);
        }
}

// runs one corpus through the logical-load walk and dumps everything.
static void run_sd(Out &out, const char *id, const std::vector<uint8_t> &bytes)
{
        out.file(id, bytes);
        CachedFileInputStream s(bytes);
        DataChunkInput in(&s);
        out.tf("valid", id, in.isValidFileType());
        SdState st;
        st.m_wk.seed(st.m_keys);
        in.registerParser("SidesList", "", parseSidesDataChunk);
        Bool ok = in.parse(&st);
        out.line(std::string("walk ") + id + (ok ? " t" : " f"));
        if (!ok) {
                std::string e = std::string("serr ") + id + " ";
                switch (st.errKind) {
                        case SdState::ERR_DICT:
                                e += "dict " + std::to_string(st.errDictType);
                                break;
                        case SdState::ERR_TEAMLOOP:
                                e += "teamloop";
                                break;
                        default:
                                e += "corrupt"; // the header never opened
                                break;
                }
                out.line(e);
        } else if (!st.m_fired) {
                out.line(std::string("snone ") + id);
        } else {
                out.line(std::string("sn ") + id + " " +
                         std::to_string((Int)st.m_sidesList.m_sides.size()) + " " +
                         std::to_string((Int)st.m_sidesList.m_teams.size()));
                for (size_t i = 0; i < st.m_sidesList.m_sides.size(); ++i) {
                        const SidesInfoLoad &side = st.m_sidesList.m_sides[i];
                        out.line(std::string("sside ") + id + " " + std::to_string(i) + " " +
                                 std::to_string(side.m_dict.getPairCount()) + " " +
                                 std::to_string((Int)side.m_buildList.size()));
                        dump_dict(out, "spair", id, (Int)i, side.m_dict, st.m_keys);
                        for (size_t j = 0; j < side.m_buildList.size(); ++j) {
                                const BuildListLoad &b = side.m_buildList[j];
                                out.line(std::string("sbuild ") + id + " " + std::to_string(i) +
                                         " " + std::to_string(j) + " " + q(b.m_buildingName.s) +
                                         " " + q(b.m_templateName.s) + " " +
                                         hexword(f32bits(b.m_location.x)) + " " +
                                         hexword(f32bits(b.m_location.y)) + " " +
                                         hexword(f32bits(b.m_location.z)) + " " +
                                         hexword(f32bits(b.m_angle)) + " " +
                                         (b.m_isInitiallyBuilt ? "t" : "f") + " " +
                                         std::to_string((Int)b.m_numRebuilds) + " " +
                                         q(b.m_script.s) + " " + std::to_string(b.m_health) +
                                         " " + (b.m_whiner ? "t" : "f") + " " +
                                         (b.m_unsellable ? "t" : "f") + " " +
                                         (b.m_repairable ? "t" : "f"));
                        }
                }
                for (size_t i = 0; i < st.m_sidesList.m_teams.size(); ++i) {
                        out.line(std::string("steamn ") + id + " " + std::to_string(i) + " " +
                                 std::to_string(st.m_sidesList.m_teams[i].getPairCount()));
                        dump_dict(out, "stpair", id, (Int)i, st.m_sidesList.m_teams[i], st.m_keys);
                }
                out.line(std::string("smod ") + id + " " +
                         (st.m_sidesList.m_modified ? "t" : "f"));
        }
        // the final name-key table (ordinal order) — pins the interning
        // sequence: the nine seeded well-known keys, then the file's names
        {
                const std::vector<std::pair<AsciiString, NameKeyType>> &names = st.m_keys.names();
                for (size_t i = 0; i < names.size(); ++i)
                        out.line(std::string("skey ") + id + " " + std::to_string(names[i].second) +
                                 " " + q(names[i].first.s));
        }
        out.state(id, in);
}

// --- the corpora -------------------------------------------------------------

// the modern shape: a neutral, two players with allies/enemies, build
// lists with the v3 tail, and the default teams.
static void corpus_sd_v3(Out &out)
{
        const char *id = "sd_v3";
        std::vector<uint8_t> b = sides_toc();
        std::vector<uint8_t> p;
        put_i32(p, 3);
        { // the neutral
                std::vector<std::vector<uint8_t>> pairs;
                pairs.push_back(pair_ascii(SK_playerName, ""));
                put_bytes(p, side_payload(pairs, {}));
        }
        { // player one
                std::vector<std::vector<uint8_t>> pairs =
                        player_pairs("Plyr1", true, "FactionAmerica", "Plyr2", "Plyr2");
                pairs.push_back(pair_int(SK_playerMoney, 5000));
                pairs.push_back(pair_real(SK_playerMoney, 1.5f)); // a same-key REAL replace
                pairs.push_back(pair_unicode(SK_playerDisplayName, { 0x0050, 0x0031 }));
                std::vector<std::vector<uint8_t>> builds;
                builds.push_back(build_entry(K_SIDES_DATA_VERSION_3, "BldgA",
                                             "AmericaPowerPlant", 100.0f, 200.0f, 500.0f, 0.5f,
                                             true, 3, "BuildScriptA", 80, true, false, true));
                builds.push_back(build_entry(K_SIDES_DATA_VERSION_3, "BldgB",
                                             "AmericaBarracks", 300.0f, 400.0f, 500.0f, -1.5f,
                                             false, 0, "", 100, false, true, false));
                put_bytes(p, side_payload(pairs, builds));
        }
        { // player two (an AI)
                std::vector<std::vector<uint8_t>> pairs =
                        player_pairs("Plyr2", false, "FactionGLA", "Plyr1", "Plyr1");
                std::vector<std::vector<uint8_t>> builds;
                builds.push_back(build_entry(K_SIDES_DATA_VERSION_3, "BldgC",
                                             "GLAPowerPlant", -100.0f, -200.0f, 999.0f, 3.0f,
                                             true, -1, "ScriptC", 100, true, false, true));
                put_bytes(p, side_payload(pairs, builds));
        }
        // the teams (version 2): the two default teams + one scripted team
        put_i32(p, 3);
        put_bytes(p, team_payload(team_pairs("teamPlyr1", "Plyr1", true)));
        put_bytes(p, team_payload(team_pairs("teamPlyr2", "Plyr2", true)));
        put_bytes(p, team_payload(team_pairs("AttackSquad", "Plyr2", false)));
        put_chunk(b, 1, K_SIDES_DATA_VERSION_3, p);
        run_sd(out, id, b);
}

// version 2: no v3 tail on the build entries (the constructor defaults
// survive: script empty, health 100, whiner true, unsellable false,
// repairable true).
static void corpus_sd_v2(Out &out)
{
        const char *id = "sd_v2";
        std::vector<uint8_t> b = sides_toc();
        std::vector<uint8_t> p;
        put_i32(p, 1);
        {
                std::vector<std::vector<uint8_t>> pairs =
                        player_pairs("Solo", true, "FactionChina", "", "");
                std::vector<std::vector<uint8_t>> builds;
                builds.push_back(build_entry(K_SIDES_DATA_VERSION_2, "Nuclear",
                                             "ChinaNuclearCenter", 10.0f, 20.0f, 77.0f, 1.25f,
                                             true, 2, "IgnoredBelowV3", 55, false, true, false));
                put_bytes(p, side_payload(pairs, builds));
        }
        put_i32(p, 1);
        put_bytes(p, team_payload(team_pairs("teamSolo", "Solo", true)));
        put_chunk(b, 1, K_SIDES_DATA_VERSION_2, p);
        run_sd(out, id, b);
}

// version 1: no team list at all (the teams stay empty).
static void corpus_sd_v1(Out &out)
{
        const char *id = "sd_v1";
        std::vector<uint8_t> b = sides_toc();
        std::vector<uint8_t> p;
        put_i32(p, 1);
        {
                std::vector<std::vector<uint8_t>> pairs = player_pairs("Old", true, "FactionOld", "", "");
                put_bytes(p, side_payload(pairs, {}));
        }
        // no team count, no teams (the version-2 gate)
        put_chunk(b, 1, K_SIDES_DATA_VERSION_1, p);
        run_sd(out, id, b);
}

// version 0: reads like a version 1 (both gates are ceilings).
static void corpus_sd_v0(Out &out)
{
        const char *id = "sd_v0";
        std::vector<uint8_t> b = sides_toc();
        std::vector<uint8_t> p;
        put_i32(p, 1);
        {
                std::vector<std::vector<uint8_t>> pairs = player_pairs("V0", true, "FactionV0", "", "");
                put_bytes(p, side_payload(pairs, {}));
        }
        put_chunk(b, 1, 0, p);
        run_sd(out, id, b);
}

// a future version 4: reads like a version 3.
static void corpus_sd_v4(Out &out)
{
        const char *id = "sd_v4";
        std::vector<uint8_t> b = sides_toc();
        std::vector<uint8_t> p;
        put_i32(p, 1);
        {
                std::vector<std::vector<uint8_t>> pairs = player_pairs("Future", true, "FactionX", "", "");
                std::vector<std::vector<uint8_t>> builds;
                builds.push_back(build_entry(4, "Fb", "FutureBldg", 1.0f, 2.0f, 3.0f, 4.0f, true, 5,
                                             "Fs", 6, true, true, true));
                put_bytes(p, side_payload(pairs, builds));
        }
        put_i32(p, 0);
        put_chunk(b, 1, 4, p);
        run_sd(out, id, b);
}

// no neutral player: validateSides adds one (with the "team" team).
static void corpus_sd_neutral_added(Out &out)
{
        const char *id = "sd_neutral_added";
        std::vector<uint8_t> b = sides_toc();
        std::vector<uint8_t> p;
        put_i32(p, 1);
        {
                std::vector<std::vector<uint8_t>> pairs = player_pairs("OnlyOne", true, "FactionAmerica", "", "");
                put_bytes(p, side_payload(pairs, {}));
        }
        put_i32(p, 1);
        put_bytes(p, team_payload(team_pairs("teamOnlyOne", "OnlyOne", true)));
        put_chunk(b, 1, K_SIDES_DATA_VERSION_3, p);
        run_sd(out, id, b);
}

// a player without its default team: validateSides adds it (and the
// owner-mismatch/singleton fixes on a broken one).
static void corpus_sd_default_team(Out &out)
{
        const char *id = "sd_default_team";
        std::vector<uint8_t> b = sides_toc();
        std::vector<uint8_t> p;
        put_i32(p, 2);
        { // the neutral
                std::vector<std::vector<uint8_t>> pairs;
                pairs.push_back(pair_ascii(SK_playerName, ""));
                put_bytes(p, side_payload(pairs, {}));
        }
        { // a player whose default team exists but is broken: the wrong
          // owner and not a singleton
                std::vector<std::vector<uint8_t>> pairs = player_pairs("Orphan", false, "FactionGLA", "", "");
                put_bytes(p, side_payload(pairs, {}));
        }
        { // a player with no default team at all
                std::vector<std::vector<uint8_t>> pairs = player_pairs("Teamless", true, "FactionChina", "", "");
                put_bytes(p, side_payload(pairs, {}));
        }
        put_i32(p, 1);
        // teamOrphan: the wrong owner (points at Teamless) and singleton false
        put_bytes(p, team_payload(team_pairs("teamOrphan", "Teamless", false)));
        put_chunk(b, 1, K_SIDES_DATA_VERSION_3, p);
        run_sd(out, id, b);
}

// the allies/enemies cleanup: self-references and unknown names drop,
// the survivors rejoin with single spaces.
static void corpus_sd_allies(Out &out)
{
        const char *id = "sd_allies";
        std::vector<uint8_t> b = sides_toc();
        std::vector<uint8_t> p;
        put_i32(p, 3);
        {
                std::vector<std::vector<uint8_t>> pairs;
                pairs.push_back(pair_ascii(SK_playerName, ""));
                put_bytes(p, side_payload(pairs, {}));
        }
        {
                // allies: self + a real player + an unknown + tabs/newlines
                // between tokens; enemies: only self
                std::vector<std::vector<uint8_t>> pairs;
                pairs.push_back(pair_ascii(SK_playerName, "Alpha"));
                pairs.push_back(pair_bool(SK_playerIsHuman, true));
                pairs.push_back(pair_ascii(SK_playerAllies, "  Alpha\tBeta Nobody\t"));
                pairs.push_back(pair_ascii(SK_playerEnemies, "Alpha"));
                put_bytes(p, side_payload(pairs, {}));
        }
        {
                std::vector<std::vector<uint8_t>> pairs = player_pairs("Beta", false, "FactionBeta", "Alpha", "Alpha");
                put_bytes(p, side_payload(pairs, {}));
        }
        put_i32(p, 2);
        put_bytes(p, team_payload(team_pairs("teamAlpha", "Alpha", true)));
        put_bytes(p, team_payload(team_pairs("teamBeta", "Beta", true)));
        put_chunk(b, 1, K_SIDES_DATA_VERSION_3, p);
        run_sd(out, id, b);
}

// a team name colliding with a player name: the team is removed (with
// more than one team to remove).
static void corpus_sd_collision(Out &out)
{
        const char *id = "sd_collision";
        std::vector<uint8_t> b = sides_toc();
        std::vector<uint8_t> p;
        put_i32(p, 2);
        {
                std::vector<std::vector<uint8_t>> pairs;
                pairs.push_back(pair_ascii(SK_playerName, ""));
                put_bytes(p, side_payload(pairs, {}));
        }
        {
                std::vector<std::vector<uint8_t>> pairs = player_pairs("Clash", true, "FactionX", "", "");
                put_bytes(p, side_payload(pairs, {}));
        }
        put_i32(p, 3);
        put_bytes(p, team_payload(team_pairs("teamClash", "Clash", true)));
        put_bytes(p, team_payload(team_pairs("Clash", "Clash", false))); // the collision
        put_bytes(p, team_payload(team_pairs("team", "", true)));        // the neutral default
        put_chunk(b, 1, K_SIDES_DATA_VERSION_3, p);
        run_sd(out, id, b);
}

// a single team named like a player: the default-team ensure pads the
// list first (the neutral's "team" and the player's "teamSolo" are
// added before the collision scan), so the collision removes cleanly —
// the original's removeTeam last-team refusal (the infinite goto
// restart) is unreachable through this load path: every colliding team
// implies the player's distinct default team exists in the list, and
// the deepest default never collides so it always survives (the
// transcription and the port keep the refusal for the latent contract,
// like the original's own dead parse-false throw).
static void corpus_sd_solo_collision(Out &out)
{
        const char *id = "sd_solo_collision";
        std::vector<uint8_t> b = sides_toc();
        std::vector<uint8_t> p;
        put_i32(p, 2);
        {
                std::vector<std::vector<uint8_t>> pairs;
                pairs.push_back(pair_ascii(SK_playerName, ""));
                put_bytes(p, side_payload(pairs, {}));
        }
        {
                std::vector<std::vector<uint8_t>> pairs = player_pairs("Solo", true, "FactionX", "", "");
                put_bytes(p, side_payload(pairs, {}));
        }
        put_i32(p, 1);
        // the ONLY pre-existing team, named like the player
        put_bytes(p, team_payload(team_pairs("Solo", "Solo", true)));
        put_chunk(b, 1, K_SIDES_DATA_VERSION_3, p);
        run_sd(out, id, b);
}

// orphaned and self-owned teams reparent to the neutral player ("").
static void corpus_sd_reparent(Out &out)
{
        const char *id = "sd_reparent";
        std::vector<uint8_t> b = sides_toc();
        std::vector<uint8_t> p;
        put_i32(p, 2);
        {
                std::vector<std::vector<uint8_t>> pairs;
                pairs.push_back(pair_ascii(SK_playerName, ""));
                put_bytes(p, side_payload(pairs, {}));
        }
        {
                std::vector<std::vector<uint8_t>> pairs = player_pairs("Owner", false, "FactionX", "", "");
                put_bytes(p, side_payload(pairs, {}));
        }
        put_i32(p, 4);
        put_bytes(p, team_payload(team_pairs("teamOwner", "Owner", true)));
        put_bytes(p, team_payload(team_pairs("OrphanTeam", "Ghost", true)));    // no such player
        put_bytes(p, team_payload(team_pairs("SelfTeam", "SelfTeam", true)));   // self-owned
        put_bytes(p, team_payload(team_pairs("NeutralTeam", "", true)));        // fine
        put_chunk(b, 1, K_SIDES_DATA_VERSION_3, p);
        run_sd(out, id, b);
}

// more sides than MAX_PLAYER_COUNT: the loop breaks mid-17th-side and
// the version-2 team count reads the 17th side's dict count word plus
// the first pair's keyAndType half. The 17th side's one-pair dict (a
// TOC-id-0 BOOL pair) makes the garbage word 1: one "team" read from
// the pair's keyAndType tail (an empty dict), and everything after it
// (the 17th side's build list, the 18th side) is skipped by the walk's
// close. validateSides then adds the neutral (the first 16 sides are
// all named) and the default teams, and the garbage team's empty name
// collides with the added neutral — removed, with 19 teams to spare.
static void corpus_sd_maxplayers(Out &out)
{
        const char *id = "sd_maxplayers";
        std::vector<uint8_t> b = sides_toc();
        std::vector<uint8_t> p;
        put_i32(p, 18);
        for (Int i = 0; i < 18; ++i) {
                char name[16];
                snprintf(name, sizeof name, "Side%02d", (int)i);
                std::vector<std::vector<uint8_t>> pairs = player_pairs(name, false, "FactionX", "", "");
                std::vector<std::vector<uint8_t>> builds;
                if (i == 16) {
                        // the never-parsed 17th side: a one-pair dict whose
                        // id-0 BOOL pair shapes the garbage team count
                        pairs.clear();
                        pairs.push_back(pair_bool(0, true));
                        builds.push_back(build_entry(K_SIDES_DATA_VERSION_3, "Seventeen", "Bldg",
                                                     1.0f, 2.0f, 3.0f, 0.0f, true, 0, "", 100, true,
                                                     false, true));
                }
                put_bytes(p, side_payload(pairs, builds));
        }
        put_chunk(b, 1, K_SIDES_DATA_VERSION_3, p);
        run_sd(out, id, b);
}

// negative counts: no sides, no teams, the walk succeeds (validateSides
// adds the neutral).
static void corpus_sd_negative(Out &out)
{
        const char *id = "sd_negative";
        std::vector<uint8_t> b = sides_toc();
        std::vector<uint8_t> p;
        put_i32(p, -3); // the side count
        put_i32(p, -5); // the team count (the version-2 read)
        put_chunk(b, 1, K_SIDES_DATA_VERSION_3, p);
        run_sd(out, id, b);
}

// a negative build-list count: no entries.
static void corpus_sd_build_negative(Out &out)
{
        const char *id = "sd_build_negative";
        std::vector<uint8_t> b = sides_toc();
        std::vector<uint8_t> p;
        put_i32(p, 1);
        {
                std::vector<std::vector<uint8_t>> pairs;
                pairs.push_back(pair_ascii(SK_playerName, ""));
                // hand-built: the dict, then the negative build count
                put_u16(p, (uint16_t)pairs.size());
                for (const auto &pr : pairs)
                        put_bytes(p, pr);
                put_i32(p, -2);
        }
        put_i32(p, 0);
        put_chunk(b, 1, K_SIDES_DATA_VERSION_3, p);
        run_sd(out, id, b);
}

// repeated chunks: the second parse clears at entry and owns the final
// state.
static void corpus_sd_multi(Out &out)
{
        const char *id = "sd_multi";
        std::vector<uint8_t> b = sides_toc();
        {
                std::vector<uint8_t> p;
                put_i32(p, 1);
                std::vector<std::vector<uint8_t>> pairs = player_pairs("FromFirst", true, "FactionA", "", "");
                put_bytes(p, side_payload(pairs, {}));
                put_i32(p, 1);
                put_bytes(p, team_payload(team_pairs("teamFromFirst", "FromFirst", true)));
                put_chunk(b, 1, K_SIDES_DATA_VERSION_3, p);
        }
        {
                std::vector<uint8_t> p;
                put_i32(p, 1);
                std::vector<std::vector<uint8_t>> pairs = player_pairs("FromSecond", false, "FactionB", "", "");
                put_bytes(p, side_payload(pairs, {}));
                put_i32(p, 0);
                put_chunk(b, 1, K_SIDES_DATA_VERSION_3, p);
        }
        run_sd(out, id, b);
}

// a corrupt pair type in a side dict: the escaping
// ERROR_CORRUPT_FILE_FORMAT stops the walk (the chunk stays open).
static void corpus_sd_corrupt(Out &out)
{
        const char *id = "sd_corrupt";
        std::vector<uint8_t> b = sides_toc();
        std::vector<uint8_t> p;
        put_i32(p, 1);
        {
                std::vector<std::vector<uint8_t>> pairs;
                pairs.push_back(pair_ascii(SK_playerName, "Doomed"));
                pairs.push_back(pair_raw_type(SK_playerMoney, 9)); // type 9 — impossible
                put_bytes(p, side_payload(pairs, {}));
        }
        put_i32(p, 0);
        put_chunk(b, 1, K_SIDES_DATA_VERSION_3, p);
        run_sd(out, id, b);
}

// a corrupt pair type in a team dict (the same stop, later in the walk).
static void corpus_sd_corrupt_team(Out &out)
{
        const char *id = "sd_corrupt_team";
        std::vector<uint8_t> b = sides_toc();
        std::vector<uint8_t> p;
        put_i32(p, 1);
        {
                std::vector<std::vector<uint8_t>> pairs;
                pairs.push_back(pair_ascii(SK_playerName, ""));
                put_bytes(p, side_payload(pairs, {}));
        }
        put_i32(p, 1);
        put_bytes(p, team_payload({ pair_raw_type(SK_teamName, 5) })); // type 5 — impossible
        put_chunk(b, 1, K_SIDES_DATA_VERSION_3, p);
        run_sd(out, id, b);
}

// a short file: the side dict's zero-filled pairs (the empty-name BOOLs).
static void corpus_sd_short(Out &out)
{
        const char *id = "sd_short";
        std::vector<uint8_t> b = sides_toc();
        std::vector<uint8_t> p;
        put_i32(p, 1);
        {
                std::vector<std::vector<uint8_t>> pairs;
                pairs.push_back(pair_ascii(SK_playerName, "Cut"));
                pairs.push_back(pair_bool(SK_playerIsHuman, true));
                std::vector<uint8_t> sp = side_payload(pairs, {});
                sp.resize(sp.size() - 8); // cut inside the pairs, keep the counts
                put_bytes(p, sp);
        }
        put_chunk(b, 1, K_SIDES_DATA_VERSION_3, p);
        run_sd(out, id, b);
}

// a nested PlayerScriptsList chunk: the script-engine slice's payload —
// skipped whole by the walk (nothing registered in this scope).
static void corpus_sd_scripts(Out &out)
{
        const char *id = "sd_scripts";
        std::vector<uint8_t> b = toc_bytes({{ "SidesList", 1 },
                                            { "PlayerScriptsList", 2 },
                                            { "playerName", SK_playerName }});
        std::vector<uint8_t> p;
        put_i32(p, 1);
        {
                std::vector<std::vector<uint8_t>> pairs;
                pairs.push_back(pair_ascii(SK_playerName, ""));
                put_bytes(p, side_payload(pairs, {}));
        }
        put_i32(p, 0);
        // the nested scripts chunk (its bytes are the script slice's
        // concern — skipped here)
        put_chunk(p, 2, 1, { 0x01, 0x02, 0x03, 0x04 });
        put_chunk(b, 1, K_SIDES_DATA_VERSION_3, p);
        run_sd(out, id, b);
}

// leftovers after the teams (skipped by the chunk's close).
static void corpus_sd_leftover(Out &out)
{
        const char *id = "sd_leftover";
        std::vector<uint8_t> b = sides_toc();
        std::vector<uint8_t> p;
        put_i32(p, 1);
        {
                std::vector<std::vector<uint8_t>> pairs;
                pairs.push_back(pair_ascii(SK_playerName, ""));
                put_bytes(p, side_payload(pairs, {}));
        }
        put_i32(p, 0);
        p.push_back(0xee);
        p.push_back(0xee);
        put_chunk(b, 1, K_SIDES_DATA_VERSION_3, p);
        run_sd(out, id, b);
}

// a map with no SidesList at all: the walk succeeds, the sides list
// keeps its pre-load state (empty — a fresh process).
static void corpus_sd_none(Out &out)
{
        const char *id = "sd_none";
        std::vector<uint8_t> b = toc_bytes({{ "HeightMapData", 1 }});
        {
                std::vector<uint8_t> d;
                put_i32(d, 2);
                put_i32(d, 2);
                put_i32(d, 4);
                put_bytes(d, std::vector<uint8_t>(4, 0x10));
                put_chunk(b, 1, 3, d);
        }
        run_sd(out, id, b);
}

// not a chunky file at all.
static void corpus_sd_not_chunky(Out &out)
{
        const char *id = "sd_not_chunky";
        std::vector<uint8_t> b;
        put_str(b, "not a map at all");
        run_sd(out, id, b);
}

// a CkMp table of contents that ends at EOF never opens (the
// container's refusal — the walk fails before any parser).
static void corpus_sd_toc_only(Out &out)
{
        const char *id = "sd_toc_only";
        std::vector<uint8_t> b = sides_toc();
        run_sd(out, id, b);
}

// empty file.
static void corpus_sd_empty(Out &out)
{
        const char *id = "sd_empty";
        std::vector<uint8_t> b;
        run_sd(out, id, b);
}

} // namespace

int main(int argc, char **argv)
{
        if (argc != 2) {
                fprintf(stderr, "usage: %s <output-file>\n", argv[0]);
                return 2;
        }
        FILE *f = fopen(argv[1], "w");
        if (!f) {
                perror("fopen");
                return 1;
        }
        Out out{ f };
        fprintf(f, "# gen_mapsides_golden.cpp — SidesList.cpp ParseSidesDataChunk "
                   "transcription (baseline 571a7c5).\n");
        fprintf(f, "# Golden script for rzh-formats::map_sideslist; see "
                   "tools/differential/README.md.\n");
        fprintf(f, "# Grammar: file/valid/walk/serr/snone/sn/sside/spair/sbuild/"
                   "steamn/stpair/smod/skey/sstate.\n");

        corpus_sd_v3(out);
        corpus_sd_v2(out);
        corpus_sd_v1(out);
        corpus_sd_v0(out);
        corpus_sd_v4(out);
        corpus_sd_neutral_added(out);
        corpus_sd_default_team(out);
        corpus_sd_allies(out);
        corpus_sd_collision(out);
        corpus_sd_solo_collision(out);
        corpus_sd_reparent(out);
        corpus_sd_maxplayers(out);
        corpus_sd_negative(out);
        corpus_sd_build_negative(out);
        corpus_sd_multi(out);
        corpus_sd_corrupt(out);
        corpus_sd_corrupt_team(out);
        corpus_sd_short(out);
        corpus_sd_scripts(out);
        corpus_sd_leftover(out);
        corpus_sd_none(out);
        corpus_sd_not_chunky(out);
        corpus_sd_toc_only(out);
        corpus_sd_empty(out);

        fclose(f);
        return 0;
}
