// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Golden-vector driver for `rzh-formats::map_polygontriggers`
// (parity level P0). Copyright 2025 Electronic Arts Inc.; Rust port
// Copyright 2026 the RustZeroHour contributors.
//
// Verbatim transcription of the Zero Hour polygon-triggers load path —
// Core/GameEngine/Source/GameLogic/Map/PolygonTrigger.cpp at the
// baseline (571a7c5) — over the re-transcribed map data-chunk container
// (Common/System/DataChunk.{h,cpp}, the reader half verbatim from
// gen_mapchunk_golden.cpp):
//
//   - PolygonTrigger::ParsePolygonTriggersDataChunk, the
//     PolygonTriggers chunk reader the constructor's logical-data-only
//     load registers: the flat count loop (the trigger name, the
//     version-4 layer name, the id, the version-2 water byte, the
//     version-3 river byte and riverStart word, the point count and
//     the int triples), the entry-time deleteTriggers (the static list
//     and the s_currentID counter both reset), the numPoints<2
//     deletion after the points are read (Zero Hour always takes the
//     non-retail branch — the ids still count toward the max and the
//     linkage cursor is unchanged), the head-prepending linkage (the
//     list ends up in file order), the version-1 auto-added water area
//     (id = the post-incremented max — a duplicate of the largest id
//     when any trigger was kept, 0 when none was — with the four
//     corner points derived from MAP_XY_FACTOR and the GlobalData
//     water extents, z 7 the old water position), and the final
//     s_currentID = maxTriggerId+1;
//   - the constructor's registerParser("PolygonTriggers", "", ...)/parse
//     walk shape the harness drives (global scope, last-wins repeated
//     chunks, skipped unknown labels) and the registration-time
//     deleteTriggers the absent-payload state models (the empty list,
//     the counter at 1).
//
// Deliberate deviations, pinned identically on both sides:
//
// - The linked list (ThePolygonTriggerListPtr + m_nextPolygonTrigger)
//   is the vector of kept triggers in file order: the parser clears at
//   entry, the first kept trigger prepends to the (empty) head and the
//   rest chain after the previous — the prepend-to-empty and the
//   chain-after-previous are both the push_back, so the vector is
//   exactly the reachable list.
// - The constructor's fixed-capacity points array (m_points/m_numPoints/
//   m_sizePoints, the numPoints+1 clamp, reallocate()'s doubling) is a
//   vector: the allocation size is not a load observable (numPoints+1
//   slots always hold the numPoints points the loop reads, so
//   reallocate never runs on the load path; the constructor's minted
//   id from s_currentID++ is overwritten before anything can observe
//   it). Absurd point counts are the same unbounded-work class the
//   mapobjects angle corpus documents — the corpora keep them sane.
// - The auto-added water area's name: RTS_DEBUG builds name it
//   "AutoAddedWaterAreaTrigger"; the release build transcribed (and
//   ported) leaves the constructor's empty name.
// - The water extents are the harness's state (GlobalData's
//   m_waterExtentX/Y, engine defaults 0): run_pt pins non-default
//   extents for the wex corpus, the same parameters the port's reader
//   takes.
// - The version gates are ceilings on the read shape and the
//   auto-area's gate is the exact == 1 (a version-0 chunk reads flat
//   like a 1 but gets no auto area; a version-5 reads like a 4).
// - The container transcription (the reader half) is verbatim from
//   gen_mapchunk_golden.cpp — see that file's header for its own
//   deviation notes (the RefPack branch, the 16-bit WideChar pin, the
//   zero-filled short reads).
//
// Output grammar (one command per line, hex lowercase, '-' = empty):
//
//   file <id> <hex>               the corpus bytes
//   valid <id> <t|f>              isValidFileType() after construction
//   walk <id> <t|f>               the parse() verdict
//   perr <id> corrupt             the walk itself refused
//   pnone <id>                    no PolygonTriggers chunk fired
//   ptrign <id> <n>               the kept-trigger count
//   ptrig <id> <i> <name> <layer> <id> <water> <river> <riverStart> <numPoints>
//                                 trigger i as the load leaves it
//   ppt <id> <i> <j> <x> <y> <z>  point j of trigger i
//   pnext <id> <s_currentID>      the id counter as the load leaves it
//   pdrop <id> <k>                the kth numPoints<2 deletion
//   pstate <id> <tell> <depth> <eof>  post-walk stream state
//
// Usage: gen_mappolygons_golden <output-file>
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
// the polygon-triggers payload (PolygonTrigger.cpp, load-path
// transcription)
// ---------------------------------------------------------------------------

// MapReaderWriterInfo.h: the version words.
#define K_TRIGGERS_VERSION_1 1 // the flat shape (auto water area)
#define K_TRIGGERS_VERSION_2 2 // Added m_isWaterArea
#define K_TRIGGERS_VERSION_3 3 // Added m_isRiver & m_riverStart
#define K_TRIGGERS_VERSION_4 4 // Added layer name. Added in Zero Hour.

// MapObject.h: the world scale the auto-added water area derives from.
#define MAP_XY_FACTOR (10.0f) // How wide and tall each height map square is in world space.

// GameCommon.h: ICoord3D (the int triple).
struct ICoord3D {
        Int x = 0, y = 0, z = 0;
};

// The trigger's saved members (the load-path half of PolygonTrigger):
// the name, the id, the points, the river/riverStart words, the water
// and export flags, the layer name. The linked list (m_nextPolygonTrigger)
// is the vector's order (head first, file order — see the parser note);
// the fixed-capacity doubling points array (m_points/m_numPoints/
// m_sizePoints) is a vector — the allocation size is not an observable,
// and the constructor's numPoints+1 clamp (to at least 2) always holds
// the numPoints points the loop reads, so reallocate() never runs on
// the load path. m_waterHandle (this-pointer editor state) and the
// lazy bounds/radius caches are not load observables.
struct PolygonTriggerLoad {
        AsciiString m_triggerName;
        Int m_triggerID = 0;
        std::vector<ICoord3D> m_points;
        Int m_riverStart = 0;
        Bool m_isWaterArea = false;
        Bool m_isRiver = false;
        AsciiString m_layerName;
};

// The walk's user state: the static trigger list (head first) and the
// s_currentID counter (PolygonTrigger.cpp's file statics), the
// water-extent globals the v1 auto area derives from (GlobalData's
// m_waterExtentX/Y — the harness pins the engine defaults 0), the
// deletion counter (the DEBUG_LOG's only trace) and the fired pin.
struct PtState {
        std::vector<PolygonTriggerLoad> m_list; // ThePolygonTriggerListPtr, head first
        Int s_currentID = 1;                    // PolygonTrigger::s_currentID
        Real m_waterExtentX = 0.0f;             // TheGlobalData
        Real m_waterExtentY = 0.0f;
        Int m_deleted = 0;                      // the numPoints<2 deletions
        Bool m_fired = false;                   // any chunk seen
};

// PolygonTrigger::deleteTriggers — the parser's entry reset (the
// constructor calls it at registration time too; the harness starts
// from the same empty state).
static void ptDeleteTriggers(PtState &st)
{
        st.m_list.clear();
        st.s_currentID = 1;
}

// PolygonTrigger::ParsePolygonTriggersDataChunk — the PolygonTriggers
// chunk reader: the flat count loop (name, [v4] layer, id, [v2] water
// byte, [v3] river byte + riverStart, numPoints, the points), the
// numPoints<2 deletion after the points are read (the ids still count
// toward the max), the head-prepending linkage (the list ends up in
// file order), the v1 auto-added water area (the id one below the
// final counter — a duplicate of the max when any trigger was kept)
// and the final s_currentID = maxTriggerId+1.
static Bool parsePolygonTriggersDataChunk(DataChunkInput &file, DataChunkInfo *info, void *userData)
{
        PtState *st = (PtState *)userData;
        st->m_fired = true;
        // Remove any existing polygon triggers, if any.
        ptDeleteTriggers(*st); // just in case.
        Int maxTriggerId = 0;
        Int count;
        Int numPoints;
        Int triggerID;
        Bool isWater;
        Bool isRiver;
        Int riverStart;
        AsciiString triggerName;
        AsciiString layerName;
        ICoord3D loc;
        count = file.readInt();
        while (count > 0) {
                count--;
                triggerName = file.readAsciiString();
                if (info->version >= K_TRIGGERS_VERSION_4) {
                        layerName = file.readAsciiString();
                }
                triggerID = file.readInt();
                isWater = false;
                if (info->version >= K_TRIGGERS_VERSION_2) {
                        isWater = file.readByte() ? true : false;
                }
                isRiver = false;
                riverStart = 0;
                if (info->version >= K_TRIGGERS_VERSION_3) {
                        isRiver = file.readByte() ? true : false;
                        riverStart = file.readInt();
                }

                numPoints = file.readInt();
                // newInstance(PolygonTrigger)(numPoints+1): the constructor
                // clamps the allocation to at least 2 slots, mints an id
                // from s_currentID++ (overwritten two lines down — the mint
                // is not an observable) and sets the editor-only water
                // handle; the trigger is modelled in place. The points
                // vector reserves the constructor's single allocation
                // (numPoints+1, clamped to at least 2) — the one place the
                // vector model must match the array's allocation count,
                // since the corpus benches time it.
                PolygonTriggerLoad pTrig;
                {
                        Int initialAllocation = numPoints + 1;
                        if (initialAllocation < 2)
                                initialAllocation = 2;
                        if (initialAllocation > 0)
                                pTrig.m_points.reserve((size_t)initialAllocation);
                }
                pTrig.m_triggerName = triggerName;
                pTrig.m_layerName = layerName;
                pTrig.m_isWaterArea = isWater;
                pTrig.m_isRiver = isRiver;
                pTrig.m_riverStart = riverStart;
                pTrig.m_triggerID = triggerID;
                if (triggerID > maxTriggerId) {
                        maxTriggerId = triggerID;
                }
                Int i;
                for (i = 0; i < numPoints; i++) {
                        loc.x = file.readInt();
                        loc.y = file.readInt();
                        loc.z = file.readInt();
                        pTrig.m_points.push_back(loc); // addPoint (the capacity never runs out)
                }
                // Zero Hour always takes this branch (the gate is
                // !(RTS_GENERALS && RETAIL_COMPATIBLE_CRC); the port follows
                // the non-retail branch per the plan's §2 note 4).
                if (numPoints < 2) {
                        // DEBUG_LOG(("Deleting polygon trigger '%s' with %d points.", ...))
                        // deleteInstance(pTrig) — the fresh trigger is unlinked, so
                        // the destructor's cascade never fires; the linkage cursor
                        // (pPrevTrig) is unchanged and the walk runs on.
                        st->m_deleted++;
                        continue;
                }
                // the linkage: pPrevTrig->setNextPoly(pTrig), else
                // addPolygonTrigger (prepend). pPrevTrig is null exactly
                // when the kept list is empty (deleteTriggers ran at entry;
                // the deletions leave the cursor alone), so the
                // prepend-to-empty and the chain-after-previous are both
                // the vector's push_back — the list ends up in file order.
                st->m_list.push_back(std::move(pTrig));
        }
        if (info->version == K_TRIGGERS_VERSION_1) {
                // before water areas existed, so create a default one.
                PolygonTriggerLoad pTrig; // newInstance(PolygonTrigger)(4)
                pTrig.m_isWaterArea = true;
                // (RTS_DEBUG names it "AutoAddedWaterAreaTrigger"; the
                // release build transcribed here leaves the constructor's
                // empty name)
                pTrig.m_triggerID = maxTriggerId++; // post-increment: the old max, then +1
                loc.x = (Int)(-30 * MAP_XY_FACTOR);
                loc.y = (Int)(-30 * MAP_XY_FACTOR);
                loc.z = 7; // The old water position.
                pTrig.m_points.push_back(loc);
                loc.x = (Int)(30 * MAP_XY_FACTOR + st->m_waterExtentX);
                pTrig.m_points.push_back(loc);
                loc.y = (Int)(30 * MAP_XY_FACTOR + st->m_waterExtentY);
                pTrig.m_points.push_back(loc);
                loc.x = (Int)(-30 * MAP_XY_FACTOR);
                pTrig.m_points.push_back(loc);
                // (the linkage: after pPrevTrig or the head — the push_back)
                st->m_list.push_back(std::move(pTrig));
        }
        st->s_currentID = maxTriggerId + 1;
        // DEBUG_ASSERTCRASH(file.atEndOfChunk(), ("Incorrect data file length."));
        // — debug-only; the release close skips the leftovers.
        return true;
}

// ---------------------------------------------------------------------------
// the golden-script driver
// ---------------------------------------------------------------------------

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
                line(std::string("pstate ") + id + " " + std::to_string((Int)in.tell()) + " " +
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
static void put_str(std::vector<uint8_t> &b, const std::string &s)
{
        b.insert(b.end(), s.begin(), s.end());
}
static void put_bytes(std::vector<uint8_t> &b, const std::vector<uint8_t> &s)
{
        b.insert(b.end(), s.begin(), s.end());
}

// one chunk: id, version, data size, data. The size word defaults to the
// data length but can lie (the under-declared corpus).
static void put_chunk(std::vector<uint8_t> &b, uint32_t id, uint16_t ver,
                      const std::vector<uint8_t> &data, int32_t size_word = -1)
{
        put_u32(b, id);
        put_u16(b, ver);
        put_i32(b, size_word < 0 ? (int32_t)data.size() : size_word);
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

// --- polygon payload builders ----------------------------------------------

// one trigger's flat payload, at the given version's shape (the layer
// from v4, the water byte from v2, the river byte + riverStart from
// v3, everything after the header from v1).
static std::vector<uint8_t> trigger_payload(uint16_t version, const std::string &name,
                                            const std::string &layer, Int id, Bool water,
                                            Bool river, Int riverStart,
                                            const std::vector<std::vector<Int>> &points)
{
        std::vector<uint8_t> p;
        put_u16(p, (uint16_t)name.size());
        put_str(p, name);
        if (version >= K_TRIGGERS_VERSION_4) {
                put_u16(p, (uint16_t)layer.size());
                put_str(p, layer);
        }
        put_i32(p, id);
        if (version >= K_TRIGGERS_VERSION_2)
                p.push_back(water ? 1 : 0);
        if (version >= K_TRIGGERS_VERSION_3) {
                p.push_back(river ? 1 : 0);
                put_i32(p, riverStart);
        }
        put_i32(p, (Int)points.size());
        for (const auto &pt : points) {
                put_i32(p, pt[0]);
                put_i32(p, pt[1]);
                put_i32(p, pt[2]);
        }
        return p;
}

// a PolygonTriggers chunk payload: the count and the triggers.
static std::vector<uint8_t> triggers_payload(const std::vector<std::vector<uint8_t>> &triggers)
{
        std::vector<uint8_t> p;
        put_i32(p, (Int)triggers.size());
        for (const auto &t : triggers)
                put_bytes(p, t);
        return p;
}

// --- the harness (the constructor's logical walk, triggers only) ------------

// runs one corpus through the logical-load walk and dumps everything.
static void run_pt(Out &out, const char *id, const std::vector<uint8_t> &bytes,
                   Real waterExtentX = 0.0f, Real waterExtentY = 0.0f)
{
        out.file(id, bytes);
        CachedFileInputStream s(bytes);
        DataChunkInput in(&s);
        out.tf("valid", id, in.isValidFileType());
        PtState st;
        st.m_waterExtentX = waterExtentX; // TheGlobalData's INI state
        st.m_waterExtentY = waterExtentY;
        in.registerParser("PolygonTriggers", "", parsePolygonTriggersDataChunk);
        Bool ok = in.parse(&st);
        out.line(std::string("walk ") + id + (ok ? " t" : " f"));
        if (!ok) {
                // the reader has no failure mode: a parser's false is the
                // walk's own refusal (the header never opened)
                out.line(std::string("perr ") + id + " corrupt");
        } else if (!st.m_fired) {
                out.line(std::string("pnone ") + id);
        } else {
                out.line(std::string("ptrign ") + id + " " + std::to_string((Int)st.m_list.size()));
                for (size_t i = 0; i < st.m_list.size(); ++i) {
                        const PolygonTriggerLoad &t = st.m_list[i];
                        out.line(std::string("ptrig ") + id + " " + std::to_string(i) + " " +
                                 q(t.m_triggerName.s) + " " + q(t.m_layerName.s) + " " +
                                 std::to_string(t.m_triggerID) + " " +
                                 (t.m_isWaterArea ? "t" : "f") + " " +
                                 (t.m_isRiver ? "t" : "f") + " " +
                                 std::to_string(t.m_riverStart) + " " +
                                 std::to_string((Int)t.m_points.size()));
                        for (size_t j = 0; j < t.m_points.size(); ++j)
                                out.line(std::string("ppt ") + id + " " + std::to_string(i) + " " +
                                         std::to_string(j) + " " + std::to_string(t.m_points[j].x) +
                                         " " + std::to_string(t.m_points[j].y) + " " +
                                         std::to_string(t.m_points[j].z));
                }
                out.line(std::string("pnext ") + id + " " + std::to_string(st.s_currentID));
        }
        // the deletions (the original's DEBUG_LOG is their only trace)
        for (Int k = 0; k < st.m_deleted; ++k)
                out.line(std::string("pdrop ") + id + " " + std::to_string(k));
        out.state(id, in);
}

// --- the corpora -------------------------------------------------------------

// the modern Zero Hour shape: named, layered triggers with water and
// river flags.
static void corpus_pt_v4(Out &out)
{
        const char *id = "pt_v4";
        std::vector<uint8_t> b = toc_bytes({{ "PolygonTriggers", 1 }});
        std::vector<std::vector<uint8_t>> triggers;
        triggers.push_back(trigger_payload(K_TRIGGERS_VERSION_4, "RiverNorth", "Water", 3, true,
                                           true, 1, { { 10, 20, 0 }, { 30, 20, 0 }, { 30, 40, 0 } }));
        triggers.push_back(trigger_payload(K_TRIGGERS_VERSION_4, "Ambush", "Ground", 17, false,
                                           false, 0,
                                           { { -100, -200, 5 }, { 100, -200, 5 }, { 100, 200, 5 },
                                             { -100, 200, 5 } }));
        triggers.push_back(trigger_payload(K_TRIGGERS_VERSION_4, "Pond", "Water", 4, true, false,
                                           0, { { 0, 0, 0 }, { 50, 0, 0 }, { 50, 25, 0 },
                                                { 0, 25, 0 }, { 1, 2, 3 } }));
        put_chunk(b, 1, K_TRIGGERS_VERSION_4, triggers_payload(triggers));
        run_pt(out, id, b);
}

// version 3: the river words, no layer names.
static void corpus_pt_v3(Out &out)
{
        const char *id = "pt_v3";
        std::vector<uint8_t> b = toc_bytes({{ "PolygonTriggers", 1 }});
        std::vector<std::vector<uint8_t>> triggers;
        triggers.push_back(trigger_payload(K_TRIGGERS_VERSION_3, "AreaV3", "", 9, true, true, 7,
                                           { { 1, 2, 3 }, { 4, 5, 6 } }));
        put_chunk(b, 1, K_TRIGGERS_VERSION_3, triggers_payload(triggers));
        run_pt(out, id, b);
}

// version 2: the water byte only.
static void corpus_pt_v2(Out &out)
{
        const char *id = "pt_v2";
        std::vector<uint8_t> b = toc_bytes({{ "PolygonTriggers", 1 }});
        std::vector<std::vector<uint8_t>> triggers;
        triggers.push_back(trigger_payload(K_TRIGGERS_VERSION_2, "AreaV2", "", 5, true, false, 0,
                                           { { 7, 8, 9 }, { 10, 11, 12 } }));
        put_chunk(b, 1, K_TRIGGERS_VERSION_2, triggers_payload(triggers));
        run_pt(out, id, b);
}

// version 1: the flat shape plus the auto-added water area — its id
// duplicates the max trigger id (the post-increment), and the counter
// lands two past the max.
static void corpus_pt_v1(Out &out)
{
        const char *id = "pt_v1";
        std::vector<uint8_t> b = toc_bytes({{ "PolygonTriggers", 1 }});
        std::vector<std::vector<uint8_t>> triggers;
        triggers.push_back(trigger_payload(K_TRIGGERS_VERSION_1, "AmbushV1", "", 5, false, false,
                                           0, { { 0, 0, 0 }, { 10, 0, 0 }, { 10, 10, 0 } }));
        triggers.push_back(trigger_payload(K_TRIGGERS_VERSION_1, "BackwaterV1", "", 9, false,
                                           false, 0, { { 20, 20, 0 }, { 30, 20, 0 }, { 30, 30, 0 } }));
        put_chunk(b, 1, K_TRIGGERS_VERSION_1, triggers_payload(triggers));
        run_pt(out, id, b);
}

// version 0: no gates fire (reads like a 1) but the auto water area
// does not follow (the == 1 gate is exact).
static void corpus_pt_v0(Out &out)
{
        const char *id = "pt_v0";
        std::vector<uint8_t> b = toc_bytes({{ "PolygonTriggers", 1 }});
        std::vector<std::vector<uint8_t>> triggers;
        triggers.push_back(trigger_payload(0, "AreaV0", "", 6, false, false, 0,
                                           { { 1, 1, 1 }, { 2, 2, 2 } }));
        put_chunk(b, 1, 0, triggers_payload(triggers));
        run_pt(out, id, b);
}

// a future version 5: every gate fires (reads like a 4), no auto area.
static void corpus_pt_v5(Out &out)
{
        const char *id = "pt_v5";
        std::vector<uint8_t> b = toc_bytes({{ "PolygonTriggers", 1 }});
        std::vector<std::vector<uint8_t>> triggers;
        triggers.push_back(trigger_payload(5, "AreaV5", "Layer5", 11, true, true, 2,
                                           { { 3, 4, 5 }, { 6, 7, 8 } }));
        put_chunk(b, 1, 5, triggers_payload(triggers));
        run_pt(out, id, b);
}

// a version-1 map with no triggers at all: the auto water area is the
// whole list, with id 0 and the counter at 2.
static void corpus_pt_v1_empty(Out &out)
{
        const char *id = "pt_v1_empty";
        std::vector<uint8_t> b = toc_bytes({{ "PolygonTriggers", 1 }});
        put_chunk(b, 1, K_TRIGGERS_VERSION_1, triggers_payload({}));
        run_pt(out, id, b);
}

// the numPoints<2 family: zero- and one-point triggers are deleted
// after their points are read (their ids still count toward the max),
// and the chain runs through the survivors.
static void corpus_pt_degenerate(Out &out)
{
        const char *id = "pt_degenerate";
        std::vector<uint8_t> b = toc_bytes({{ "PolygonTriggers", 1 }});
        std::vector<std::vector<uint8_t>> triggers;
        triggers.push_back(trigger_payload(K_TRIGGERS_VERSION_4, "Empty", "Ground", 21, false,
                                           false, 0, {}));
        triggers.push_back(trigger_payload(K_TRIGGERS_VERSION_4, "Line", "Ground", 22, false,
                                           false, 0, { { 1, 2, 3 } }));
        triggers.push_back(trigger_payload(K_TRIGGERS_VERSION_4, "Survivor", "Ground", 8, false,
                                           false, 0, { { 0, 0, 0 }, { 5, 5, 5 } }));
        triggers.push_back(trigger_payload(K_TRIGGERS_VERSION_4, "Lonely", "Water", 23, true,
                                           false, 0, { { 9, 9, 9 } }));
        triggers.push_back(trigger_payload(K_TRIGGERS_VERSION_4, "Tail", "Water", 2, true, true, 4,
                                           { { 1, 1, 1 }, { 2, 2, 2 }, { 3, 3, 3 } }));
        put_chunk(b, 1, K_TRIGGERS_VERSION_4, triggers_payload(triggers));
        run_pt(out, id, b);
}

// a negative numPoints: no points read, the trigger deleted.
static void corpus_pt_negative_points(Out &out)
{
        const char *id = "pt_negative_points";
        std::vector<uint8_t> b = toc_bytes({{ "PolygonTriggers", 1 }});
        std::vector<std::vector<uint8_t>> triggers;
        // hand-built: the numPoints word lies negative
        std::vector<uint8_t> t;
        put_u16(t, 3);
        put_str(t, "Neg");
        put_u16(t, 1);
        put_str(t, "L");
        put_i32(t, 31);
        t.push_back(1); // water
        t.push_back(0); // river
        put_i32(t, 0);  // riverStart
        put_i32(t, -3); // numPoints
        triggers.push_back(t);
        triggers.push_back(trigger_payload(K_TRIGGERS_VERSION_4, "After", "L", 32, false, false, 0,
                                           { { 1, 2, 3 }, { 4, 5, 6 } }));
        put_chunk(b, 1, K_TRIGGERS_VERSION_4, triggers_payload(triggers));
        run_pt(out, id, b);
}

// negative ids never update the max: the counter lands at 1 (and the
// v1 auto area would take id 0).
static void corpus_pt_negative_ids(Out &out)
{
        const char *id = "pt_negative_ids";
        std::vector<uint8_t> b = toc_bytes({{ "PolygonTriggers", 1 }});
        std::vector<std::vector<uint8_t>> triggers;
        triggers.push_back(trigger_payload(K_TRIGGERS_VERSION_4, "NegId", "L", -7, false, false, 0,
                                           { { 1, 2, 3 }, { 4, 5, 6 } }));
        triggers.push_back(trigger_payload(K_TRIGGERS_VERSION_4, "NegId2", "L", -1, false, false,
                                           0, { { 1, 2, 3 }, { 4, 5, 6 } }));
        put_chunk(b, 1, K_TRIGGERS_VERSION_4, triggers_payload(triggers));
        run_pt(out, id, b);
}

// duplicate ids are kept as read (no dedup on load); the max picks the
// largest.
static void corpus_pt_dup_ids(Out &out)
{
        const char *id = "pt_dup_ids";
        std::vector<uint8_t> b = toc_bytes({{ "PolygonTriggers", 1 }});
        std::vector<std::vector<uint8_t>> triggers;
        triggers.push_back(trigger_payload(K_TRIGGERS_VERSION_4, "First", "L", 12, false, false, 0,
                                           { { 1, 2, 3 }, { 4, 5, 6 } }));
        triggers.push_back(trigger_payload(K_TRIGGERS_VERSION_4, "Second", "L", 12, false, false,
                                           0, { { 7, 8, 9 }, { 10, 11, 12 } }));
        put_chunk(b, 1, K_TRIGGERS_VERSION_4, triggers_payload(triggers));
        run_pt(out, id, b);
}

// the auto area's corners with non-default water extents (the GlobalData
// INI state the v1 reader consults): the 300+extent arithmetic and the
// float-to-int truncation.
static void corpus_pt_v1_wex(Out &out)
{
        const char *id = "pt_v1_wex";
        std::vector<uint8_t> b = toc_bytes({{ "PolygonTriggers", 1 }});
        std::vector<std::vector<uint8_t>> triggers;
        triggers.push_back(trigger_payload(K_TRIGGERS_VERSION_1, "OnlyV1", "", 3, false, false, 0,
                                           { { 0, 0, 0 }, { 5, 0, 0 }, { 5, 5, 0 } }));
        put_chunk(b, 1, K_TRIGGERS_VERSION_1, triggers_payload(triggers));
        run_pt(out, id, b, 12.75f, -0.9f);
}

// repeated chunks: the second parse resets (deleteTriggers at the
// parser's entry) and owns the final state.
static void corpus_pt_multi(Out &out)
{
        const char *id = "pt_multi";
        std::vector<uint8_t> b = toc_bytes({{ "PolygonTriggers", 1 }});
        {
                std::vector<std::vector<uint8_t>> triggers;
                triggers.push_back(trigger_payload(K_TRIGGERS_VERSION_4, "FromFirst", "L", 40,
                                                   false, false, 0, { { 1, 1, 1 }, { 2, 2, 2 } }));
                put_chunk(b, 1, K_TRIGGERS_VERSION_4, triggers_payload(triggers));
        }
        {
                std::vector<std::vector<uint8_t>> triggers;
                triggers.push_back(trigger_payload(K_TRIGGERS_VERSION_4, "FromSecond", "L2", 9,
                                                   true, false, 0, { { 3, 3, 3 }, { 4, 4, 4 } }));
                put_chunk(b, 1, K_TRIGGERS_VERSION_4, triggers_payload(triggers));
        }
        run_pt(out, id, b);
}

// a negative count: no loop at all, no auto area (a version-4 chunk),
// the counter at 1.
static void corpus_pt_count_negative(Out &out)
{
        const char *id = "pt_count_negative";
        std::vector<uint8_t> b = toc_bytes({{ "PolygonTriggers", 1 }});
        std::vector<uint8_t> p;
        put_i32(p, -5);
        put_chunk(b, 1, K_TRIGGERS_VERSION_4, p);
        run_pt(out, id, b);
}

// a short file: the last trigger's tail reads the zero-fill (the empty
// name, id 0, water 0, the zero points).
static void corpus_pt_short(Out &out)
{
        const char *id = "pt_short";
        std::vector<uint8_t> b = toc_bytes({{ "PolygonTriggers", 1 }});
        std::vector<uint8_t> full = trigger_payload(K_TRIGGERS_VERSION_4, "Cut", "L", 5, true, true,
                                                    3, { { 1, 2, 3 }, { 4, 5, 6 } });
        std::vector<uint8_t> p;
        put_i32(p, 2);
        // one whole trigger, then the cut one: stops after the layer
        put_bytes(p, full);
        std::vector<uint8_t> cut;
        put_u16(cut, 3);
        put_str(cut, "Cut");
        put_u16(cut, 1);
        put_str(cut, "L");
        put_bytes(p, cut);
        p.resize(p.size() - 6); // cut inside the id word
        put_chunk(b, 1, K_TRIGGERS_VERSION_4, p);
        run_pt(out, id, b);
}

// a short file cut MID-POINTS: the last trigger declares 3 points but
// the file ends inside the second — the zero-filled words read as
// (0,0,0) points, not stale ones.
static void corpus_pt_short_points(Out &out)
{
        const char *id = "pt_short_points";
        std::vector<uint8_t> b = toc_bytes({{ "PolygonTriggers", 1 }});
        std::vector<uint8_t> p;
        put_i32(p, 1);
        // the trigger: name, layer, id, water, river, riverStart, 3 points
        // — but the payload is cut 4 bytes into the second point
        put_u16(p, 4);
        put_str(p, "CutP");
        put_u16(p, 1);
        put_str(p, "L");
        put_i32(p, 5);
        p.push_back(1); // water
        p.push_back(0); // river
        put_i32(p, 0);  // riverStart
        put_i32(p, 3);  // numPoints
        put_i32(p, 11);
        put_i32(p, 22);
        put_i32(p, 33);
        put_i32(p, 44);
        put_i32(p, 55);
        put_i32(p, 66);
        put_i32(p, 77); // the 4th word of point 2 — then EOF
        p.resize(p.size() - 8); // cut into point 2's words
        put_chunk(b, 1, K_TRIGGERS_VERSION_4, p);
        run_pt(out, id, b);
}

// the under-declared PolygonTriggers chunk: the size word says 8 but
// the payload is longer — the reads run past the declared end into the
// real bytes.
static void corpus_pt_underdeclared(Out &out)
{
        const char *id = "pt_underdeclared";
        std::vector<uint8_t> b = toc_bytes({{ "PolygonTriggers", 1 }});
        std::vector<std::vector<uint8_t>> triggers;
        triggers.push_back(trigger_payload(K_TRIGGERS_VERSION_4, "Under", "L", 6, false, false, 0,
                                           { { 1, 2, 3 }, { 4, 5, 6 } }));
        put_chunk(b, 1, K_TRIGGERS_VERSION_4, triggers_payload(triggers), 8);
        run_pt(out, id, b);
}

// leftovers: bytes after the last trigger (skipped by the chunk's
// close).
static void corpus_pt_leftover(Out &out)
{
        const char *id = "pt_leftover";
        std::vector<uint8_t> b = toc_bytes({{ "PolygonTriggers", 1 }});
        std::vector<std::vector<uint8_t>> triggers;
        triggers.push_back(trigger_payload(K_TRIGGERS_VERSION_4, "Left", "L", 7, false, false, 0,
                                           { { 1, 2, 3 }, { 4, 5, 6 } }));
        std::vector<uint8_t> p = triggers_payload(triggers);
        p.push_back(0xee);
        p.push_back(0xee);
        put_chunk(b, 1, K_TRIGGERS_VERSION_4, p);
        run_pt(out, id, b);
}

// the global-scope rule: a PolygonTriggers inside a foreign container
// never fires (the registration is ("PolygonTriggers", "")).
static void corpus_pt_nested(Out &out)
{
        const char *id = "pt_nested";
        std::vector<uint8_t> b = toc_bytes({{ "Container", 1 }, { "PolygonTriggers", 2 }});
        std::vector<uint8_t> inner;
        {
                std::vector<std::vector<uint8_t>> triggers;
                triggers.push_back(trigger_payload(K_TRIGGERS_VERSION_4, "Hidden", "L", 1, false,
                                                   false, 0, { { 1, 2, 3 }, { 4, 5, 6 } }));
                put_chunk(inner, 2, K_TRIGGERS_VERSION_4, triggers_payload(triggers));
        }
        put_chunk(b, 1, 1, inner);
        run_pt(out, id, b);
}

// a map with no PolygonTriggers at all: the walk succeeds, the list
// stays empty and the counter keeps its registration-time 1.
static void corpus_pt_none(Out &out)
{
        const char *id = "pt_none";
        std::vector<uint8_t> b = toc_bytes({{ "HeightMapData", 1 }});
        {
                std::vector<uint8_t> d;
                put_i32(d, 2);
                put_i32(d, 2);
                put_i32(d, 4);
                put_bytes(d, std::vector<uint8_t>(4, 0x10));
                put_chunk(b, 1, 3, d);
        }
        run_pt(out, id, b);
}

// not a chunky file at all.
static void corpus_pt_not_chunky(Out &out)
{
        const char *id = "pt_not_chunky";
        std::vector<uint8_t> b;
        put_str(b, "not a map at all");
        run_pt(out, id, b);
}

// a CkMp table of contents that ends at EOF never opens (the
// container's refusal — the walk fails before any parser).
static void corpus_pt_toc_only(Out &out)
{
        const char *id = "pt_toc_only";
        std::vector<uint8_t> b = toc_bytes({{ "PolygonTriggers", 1 }});
        run_pt(out, id, b);
}

// empty file.
static void corpus_pt_empty(Out &out)
{
        const char *id = "pt_empty";
        std::vector<uint8_t> b;
        run_pt(out, id, b);
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
        fprintf(f, "# gen_mappolygons_golden.cpp — PolygonTrigger.cpp "
                   "ParsePolygonTriggersDataChunk transcription (baseline 571a7c5).\n");
        fprintf(f, "# Golden script for rzh-formats::map_polygontriggers; see "
                   "tools/differential/README.md.\n");
        fprintf(f, "# Grammar: file/valid/walk/perr/pnone/ptrign/ptrig/ppt/pnext/pdrop/pstate.\n");

        corpus_pt_v4(out);
        corpus_pt_v3(out);
        corpus_pt_v2(out);
        corpus_pt_v1(out);
        corpus_pt_v0(out);
        corpus_pt_v5(out);
        corpus_pt_v1_empty(out);
        corpus_pt_degenerate(out);
        corpus_pt_negative_points(out);
        corpus_pt_negative_ids(out);
        corpus_pt_dup_ids(out);
        corpus_pt_v1_wex(out);
        corpus_pt_multi(out);
        corpus_pt_count_negative(out);
        corpus_pt_short(out);
        corpus_pt_short_points(out);
        corpus_pt_underdeclared(out);
        corpus_pt_leftover(out);
        corpus_pt_nested(out);
        corpus_pt_none(out);
        corpus_pt_not_chunky(out);
        corpus_pt_toc_only(out);
        corpus_pt_empty(out);

        fclose(f);
        return 0;
}
