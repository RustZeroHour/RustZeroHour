// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Golden-vector driver for `rzh-formats::map_worldinfo` (parity
// level P0). Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026
// the RustZeroHour contributors.
//
// Verbatim transcription of the Zero Hour world-info load path —
// Core/GameEngineDevice/Source/W3DDevice/GameClient/WorldHeightMap.cpp
// at the baseline (571a7c5) — over the re-transcribed map data-chunk
// container (Common/System/DataChunk.{h,cpp}, the reader half verbatim
// from gen_mapchunk_golden.cpp):
//
//   - WorldHeightMap::ParseWorldDictDataChunk, the WorldInfo chunk
//     reader the constructor's logical-data-only load registers: one
//     dict read whole, the MapObject::TheWorldDict static replaced
//     (every chunk, unconditionally), and the weather re-derived from
//     the new dict — Dict::getInt(TheKey_weather, &exists)'s exact-type
//     INT match sets TheWritableGlobalData->m_weather (the raw word,
//     cast to Weather but never range-checked); any other pair type or
//     no pair at all keeps the word (the sticky process-global);
//   - the constructor's registerParser("WorldInfo", "", ...)/parse walk
//     shape the harness drives (global scope, last-wins repeated
//     chunks, skipped unknown labels).
//
// Deliberate deviations, pinned identically on both sides:
//
// - The two globals the reader writes (MapObject::TheWorldDict and
//   GlobalData's m_weather) become plain walk state: the harness starts
//   them at a fresh process's values (the empty dict, WEATHER_NORMAL).
//   The original's cross-load stickiness is the same "keeps its prior
//   value" rule the absent-chunk case models — the load-side observables
//   (what the walk leaves behind) are identical.
// - The m_weatherSet flag is transcription-only: the original's global
//   has no set-ness bit (the word just keeps its prior value), and the
//   port models the same observable as Option<i32> (None = no chunk this
//   load carried an INT weather pair). The flag pins that model.
// - The version word is never read (any version loads the same); the
//   corpora pin versions 0, 1 and 9 loading identically.
// - readDict's corrupt-type throw carries the type byte (the marker
//   struct), so the golden can pin which impossible type refused the
//   load. The chunk stays open, as in the original.
// - The container transcription (the reader half) is otherwise verbatim
//   from gen_mapchunk_golden.cpp — see that file's header for its own
//   deviation notes (the RefPack branch, the 16-bit WideChar pin, the
//   zero-filled short reads) — plus the load-path Dict's getInt(key,
//   &exists) completed in (the typed getter this payload's weather
//   query drives; the mapchunk corpus never reads it).
//
// Output grammar (one command per line, hex lowercase, '-' = empty):
//
//   file <id> <hex>               the corpus bytes
//   valid <id> <t|f>              isValidFileType() after construction
//   walk <id> <t|f>               the parse() verdict
//   werr <id> corrupt             the walk itself refused
//   werr <id> dict <type>         the world dict's corrupt pair type
//   wnone <id>                    no WorldInfo chunk fired
//   wdict <id> <npairs> <wset> <weather>
//                                 the final world dict's pair count,
//                                 whether any chunk set the weather, and
//                                 the sticky raw word
//   wpair <id> <name> <value>     one dict pair (stored order — the
//                                 interning order, not file order):
//                                 b <t|f>, i <int>, r <bits>,
//                                 a <q(name)>, u <hex units>
//   wkey <id> <ordinal> <name>    the final name-key table, ordinal order
//   wstate <id> <tell> <depth> <eof>  post-walk stream state
//
// Usage: gen_mapworldinfo_golden <output-file>
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

        // Dict.cpp's getInt(key, &exists) — the typed getter the
        // world-info weather query drives (completed into the container
        // copy for this generator; the mapchunk corpus never reads it).
        Int getInt(NameKeyType key, Bool *exists) const
        {
                const DictPair *pair = findPairByKey(key);
                if (pair && pair->getType() == DICT_INT) {
                        if (exists)
                                *exists = true;
                        return pair->i;
                }
                if (exists)
                        *exists = false;
                return 0;
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
// the world-info payload (WorldHeightMap.cpp, load-path transcription)
// ---------------------------------------------------------------------------

// MapReaderWriterInfo.h: the world-dict chunk's version word (the reader
// never reads it — any version loads the same).
#define K_WORLDDICT_VERSION_1 1

// GameType.h: the Weather enum — the cast target of the raw word. The
// original never validates the range (any int survives the cast); the
// names are for the dump only.
enum Weather {
        WEATHER_NORMAL = 0,
        WEATHER_SNOWY = 1,
        WEATHER_COUNT = 2
};

// GlobalData.h's m_weather (through TheWritableGlobalData): the sticky
// process-global the reader writes. The transcription keeps the word
// (the INI default WEATHER_NORMAL is the harness's starting state; the
// original's cross-load stickiness is the same "keeps its prior value"
// rule the absent-chunk case models).
struct WeatherGlobal {
        Int m_weather = WEATHER_NORMAL;
};

// MapObject.h's static TheWorldDict (through MapObject::getWorldDict()):
// the process-global dict the reader replaces whole. The transcription
// keeps the dict (the harness starts it empty, the pre-load state a
// fresh process has).
struct WorldDictGlobal {
        Dict m_dict;
};

// The walk's user state: the two globals the load path writes (the dict
// replaced whole per chunk, the weather sticky per the exists gate) and
// the name-key generator.
struct WiState {
        WorldDictGlobal m_worldDict;
        WeatherGlobal m_globalData;
        NameKeyGenerator m_keys; // TheNameKeyGenerator

        NameKeyType TheKey_weather = 0;

        enum ErrKind { ERR_NONE = 0, ERR_CORRUPT, ERR_DICT } errKind = ERR_NONE;
        Int errDictType = 0;
        Bool m_fired = false;     // any WorldInfo chunk seen (the absent-payload pin)
        Bool m_weatherSet = false; // any exists gate fired this load (the
                                   // port's Option model; the original's
                                   // global keeps its prior word instead)

        // WellKnownKeys.h's TheKey_weather (the DEFINE_KEY(weather) at line
        // 901, StaticNameKey-startup interned) — seeded before the walk.
        void seedWellKnownKeys()
        {
                TheKey_weather = m_keys.nameToKey("weather");
        }
};

// WorldHeightMap::ParseWorldDictDataChunk — the WorldInfo chunk reader:
// one dict read whole, the world-dict global replaced, and the weather
// word re-derived from the *new* dict (an INT weather pair sets the
// sticky global; any other type or absence keeps it).
static Bool parseWorldDictDataChunk(DataChunkInput &file, DataChunkInfo * /*info*/, void *userData)
{
        WiState *st = (WiState *)userData;
        Dict d;
        try {
                d = file.readDict(st->m_keys);
        } catch (DictTypeCorrupt &e) {
                st->errKind = WiState::ERR_DICT;
                st->errDictType = (Int)e.type;
                return false; // (the escaping ERROR_CORRUPT_FILE_FORMAT's stop)
        }
        st->m_worldDict.m_dict = d;
        st->m_fired = true;
        Bool exists;
        Int theWeather = st->m_worldDict.m_dict.getInt(st->TheKey_weather, &exists);
        if (exists) {
                st->m_globalData.m_weather = theWeather; // (Weather)theWeather — the raw word
                st->m_weatherSet = true;
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
                line(std::string("wstate ") + id + " " + std::to_string((Int)in.tell()) + " " +
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
static void put_f32_bits(std::vector<uint8_t> &b, UnsignedInt bits) { put_u32(b, bits); }
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
static std::vector<uint8_t> pair_real_bits(uint32_t key_id, UnsignedInt bits)
{
        std::vector<uint8_t> p;
        put_i32(p, (int32_t)((key_id << 8) | 2));
        put_f32_bits(p, bits);
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

// one WorldInfo chunk payload: the dict pair count and pairs.
static std::vector<uint8_t> worldinfo_payload(const std::vector<std::vector<uint8_t>> &pairs)
{
        std::vector<uint8_t> p;
        put_u16(p, (uint16_t)pairs.size());
        for (const auto &pr : pairs)
                put_bytes(p, pr);
        return p;
}

// --- the harness (the constructor's logical walk, world info only) ----------

// runs one corpus through the logical-load walk and dumps everything.
static void run_wi(Out &out, const char *id, const std::vector<uint8_t> &bytes)
{
        out.file(id, bytes);
        CachedFileInputStream s(bytes);
        DataChunkInput in(&s);
        out.tf("valid", id, in.isValidFileType());
        WiState st;
        st.seedWellKnownKeys();
        in.registerParser("WorldInfo", "", parseWorldDictDataChunk);
        Bool ok = in.parse(&st);
        out.line(std::string("walk ") + id + (ok ? " t" : " f"));
        if (!ok) {
                std::string e = std::string("werr ") + id + " ";
                if (st.errKind == WiState::ERR_DICT)
                        e += "dict " + std::to_string(st.errDictType);
                else
                        e += "corrupt"; // the header never opened
                out.line(e);
        } else if (!st.m_fired) {
                out.line(std::string("wnone ") + id);
        } else {
                out.line(std::string("wdict ") + id + " " +
                         std::to_string(st.m_worldDict.m_dict.getPairCount()) + " " +
                         (st.m_weatherSet ? "t" : "f") + " " +
                         std::to_string(st.m_globalData.m_weather));
                for (Int pidx = 0; pidx < st.m_worldDict.m_dict.getPairCount(); ++pidx) {
                        const Dict::DictPair &pair = st.m_worldDict.m_dict.m_pairs[pidx];
                        std::string name = st.m_keys.keyToName(pair.getName()).s;
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
                        out.line(std::string("wpair ") + id + " " + q(name) + " " + v);
                }
        }
        // the final name-key table (ordinal order) — pins the interning
        // sequence: the seeded well-known key, then the file's names
        {
                const std::vector<std::pair<AsciiString, NameKeyType>> &names = st.m_keys.names();
                for (size_t i = 0; i < names.size(); ++i)
                        out.line(std::string("wkey ") + id + " " + std::to_string(names[i].second) +
                                 " " + q(names[i].first.s));
        }
        out.state(id, in);
}

// --- the corpora -------------------------------------------------------------

// the modern shape: a full world dict with the weather pair set.
static void corpus_wi_v1(Out &out)
{
        const char *id = "wi_v1";
        std::vector<uint8_t> b = toc_bytes({{ "WorldInfo", 1 },
                                            { "mapHeightScale", 2 },
                                            { "weather", 3 },
                                            { "mapName", 4 }});
        std::vector<std::vector<uint8_t>> pairs;
        pairs.push_back(pair_ascii(4, "WatersidePark")); // mapName
        pairs.push_back(pair_bool(2, true));             // mapHeightScale
        pairs.push_back(pair_int(3, WEATHER_SNOWY));     // weather
        put_chunk(b, 1, K_WORLDDICT_VERSION_1, worldinfo_payload(pairs));
        run_wi(out, id, b);
}

// the weather's exists gate is an exact-type match: BOOL, REAL, ASCII and
// UNICODE weather pairs never set the word (the dict is still replaced).
static void corpus_wi_weather_types(Out &out)
{
        const char *id = "wi_weather_types";
        std::vector<uint8_t> b = toc_bytes({{ "WorldInfo", 1 },
                                            { "weather", 2 },
                                            { "sky", 3 }});
        {
                std::vector<std::vector<uint8_t>> pairs;
                pairs.push_back(pair_int(2, 0)); // weather INT 0 (NORMAL)
                put_chunk(b, 1, K_WORLDDICT_VERSION_1, worldinfo_payload(pairs));
        }
        {
                std::vector<std::vector<uint8_t>> pairs;
                pairs.push_back(pair_bool(2, true)); // weather BOOL — no set
                put_chunk(b, 1, K_WORLDDICT_VERSION_1, worldinfo_payload(pairs));
        }
        {
                std::vector<std::vector<uint8_t>> pairs;
                pairs.push_back(pair_real(2, 1.0f)); // weather REAL — no set
                put_chunk(b, 1, K_WORLDDICT_VERSION_1, worldinfo_payload(pairs));
        }
        {
                std::vector<std::vector<uint8_t>> pairs;
                pairs.push_back(pair_ascii(2, "1")); // weather ASCII — no set
                put_chunk(b, 1, K_WORLDDICT_VERSION_1, worldinfo_payload(pairs));
        }
        {
                std::vector<std::vector<uint8_t>> pairs;
                pairs.push_back(pair_unicode(2, { 0x0031 })); // weather UNICODE — no set
                put_chunk(b, 1, K_WORLDDICT_VERSION_1, worldinfo_payload(pairs));
        }
        run_wi(out, id, b);
}

// the sticky word: a later chunk without the weather pair keeps the word
// while the dict moves on.
static void corpus_wi_sticky(Out &out)
{
        const char *id = "wi_sticky";
        std::vector<uint8_t> b = toc_bytes({{ "WorldInfo", 1 },
                                            { "weather", 2 },
                                            { "mapName", 3 }});
        {
                std::vector<std::vector<uint8_t>> pairs;
                pairs.push_back(pair_int(2, 2)); // an out-of-enum word — raw, unvalidated
                put_chunk(b, 1, K_WORLDDICT_VERSION_1, worldinfo_payload(pairs));
        }
        {
                std::vector<std::vector<uint8_t>> pairs;
                pairs.push_back(pair_ascii(3, "Second")); // no weather pair
                put_chunk(b, 1, K_WORLDDICT_VERSION_1, worldinfo_payload(pairs));
        }
        run_wi(out, id, b);
}

// the version word is never read: versions 0 and 9 load like a 1.
static void corpus_wi_versions(Out &out)
{
        const char *id = "wi_versions";
        std::vector<uint8_t> b = toc_bytes({{ "WorldInfo", 1 },
                                            { "weather", 2 },
                                            { "mapName", 3 }});
        {
                std::vector<std::vector<uint8_t>> pairs;
                pairs.push_back(pair_int(2, 1));
                put_chunk(b, 1, 0, worldinfo_payload(pairs));
        }
        {
                std::vector<std::vector<uint8_t>> pairs;
                pairs.push_back(pair_int(2, 0));
                put_chunk(b, 1, 9, worldinfo_payload(pairs));
        }
        run_wi(out, id, b);
}

// an empty dict (the pair count 0): a full replacement with nothing.
static void corpus_wi_empty_dict(Out &out)
{
        const char *id = "wi_empty_dict";
        std::vector<uint8_t> b = toc_bytes({{ "WorldInfo", 1 }});
        put_chunk(b, 1, K_WORLDDICT_VERSION_1, worldinfo_payload({}));
        run_wi(out, id, b);
}

// all five pair types plus the duplicate-key update (the dict's set
// keeps one pair per key; the later INT weather wins over the earlier
// BOOL one — and the storage order is the interning order).
static void corpus_wi_all_types(Out &out)
{
        const char *id = "wi_all_types";
        std::vector<uint8_t> b = toc_bytes({{ "WorldInfo", 1 },
                                            { "weather", 2 },
                                            { "mapName", 3 },
                                            { "flag", 4 },
                                            { "gravity", 5 },
                                            { "title", 6 }});
        std::vector<std::vector<uint8_t>> pairs;
        pairs.push_back(pair_bool(2, true));              // weather BOOL first
        pairs.push_back(pair_ascii(3, "Desert"));
        pairs.push_back(pair_bool(4, false));
        pairs.push_back(pair_real_bits(5, 0x41200000));   // 10.0f
        pairs.push_back(pair_unicode(6, { 0x0048, 0x0069 })); // "Hi"
        pairs.push_back(pair_int(2, 1));                  // weather INT replaces the BOOL
        put_chunk(b, 1, K_WORLDDICT_VERSION_1, worldinfo_payload(pairs));
        run_wi(out, id, b);
}

// TOC names with embedded NULs (the container's NUL-truncated views) and
// a pair whose keyAndType names an id the TOC never minted (the empty
// name) plus one whose id shifts past the u8 byte (the >> 8 resolve).
static void corpus_wi_nulname(Out &out)
{
        const char *id = "wi_nulname";
        std::vector<uint8_t> b;
        {
                // hand-built TOC: WorldInfo 1, "wea\0ther" (a NUL inside) at
                // 2, "x" at 0x21, "ww" at 0x100
                put_str(b, "CkMp");
                put_i32(b, 4);
                b.push_back(9);
                put_str(b, "WorldInfo");
                put_u32(b, 1);
                b.push_back(8);
                put_str(b, std::string("wea\0ther", 8));
                put_u32(b, 2);
                b.push_back(1);
                put_str(b, "x");
                put_u32(b, 0x21);
                b.push_back(2);
                put_str(b, "ww");
                put_u32(b, 0x100);
        }
        std::vector<std::vector<uint8_t>> pairs;
        // "wea\0ther" resolves as "wea" (the NUL-truncated view), the
        // seeded weather key's name is "weather" — a different key, so
        // this pair never sets the weather
        pairs.push_back(pair_int(2, 7));
        // id 0x21 ("x") shifts through the keyAndType word's high half —
        // the >> 8 resolve of 0x2101 is "x"
        pairs.push_back(pair_int(0x21, 9));
        // id 0x100 ("ww"): the >> 8 resolve of 0x10001 is "ww"
        pairs.push_back(pair_bool(0x100, true));
        // id 3 was never minted: the empty name (a different key again)
        pairs.push_back(pair_int(3, 3));
        put_chunk(b, 1, K_WORLDDICT_VERSION_1, worldinfo_payload(pairs));
        run_wi(out, id, b);
}

// a pair with an impossible type byte: readDict's escaping
// ERROR_CORRUPT_FILE_FORMAT stops the walk (the chunk stays open).
static void corpus_wi_corrupt(Out &out)
{
        const char *id = "wi_corrupt";
        std::vector<uint8_t> b = toc_bytes({{ "WorldInfo", 1 },
                                            { "weather", 2 }});
        std::vector<std::vector<uint8_t>> pairs;
        pairs.push_back(pair_int(2, 1));
        pairs.push_back(pair_raw_type(2, 7)); // type 7 — impossible
        put_chunk(b, 1, K_WORLDDICT_VERSION_1, worldinfo_payload(pairs));
        run_wi(out, id, b);
}

// leftovers: bytes after the dict (skipped by the chunk's close).
static void corpus_wi_leftover(Out &out)
{
        const char *id = "wi_leftover";
        std::vector<uint8_t> b = toc_bytes({{ "WorldInfo", 1 }});
        std::vector<uint8_t> p = worldinfo_payload({});
        p.push_back(0xee);
        p.push_back(0xee);
        put_chunk(b, 1, K_WORLDDICT_VERSION_1, p);
        run_wi(out, id, b);
}

// the under-declared WorldInfo: the size word says 4 but the payload is
// longer — the reads run past the declared end into the real bytes (the
// signed data-left goes negative, the close does not seek).
static void corpus_wi_underdeclared(Out &out)
{
        const char *id = "wi_underdeclared";
        std::vector<uint8_t> b = toc_bytes({{ "WorldInfo", 1 },
                                            { "weather", 2 }});
        std::vector<std::vector<uint8_t>> pairs;
        pairs.push_back(pair_int(2, 1));
        pairs.push_back(pair_int(2, 1)); // read past the declared 4 bytes
        put_chunk(b, 1, K_WORLDDICT_VERSION_1, worldinfo_payload(pairs), 4);
        run_wi(out, id, b);
}

// a short file: the dict count reads the zero-fill and the pairs mint
// empty-name BOOLs (the container's pinned zero-filled short reads).
static void corpus_wi_short(Out &out)
{
        const char *id = "wi_short";
        std::vector<uint8_t> b = toc_bytes({{ "WorldInfo", 1 },
                                            { "weather", 2 }});
        std::vector<std::vector<uint8_t>> pairs;
        pairs.push_back(pair_int(2, 1));
        pairs.push_back(pair_bool(2, false));
        std::vector<uint8_t> p = worldinfo_payload(pairs);
        p.resize(p.size() - 10); // cut the pairs, keep the count word
        put_chunk(b, 1, K_WORLDDICT_VERSION_1, p);
        run_wi(out, id, b);
}

// the global-scope rule: a WorldInfo inside a foreign container never
// fires (the registration is ("WorldInfo", "")).
static void corpus_wi_nested(Out &out)
{
        const char *id = "wi_nested";
        std::vector<uint8_t> b = toc_bytes({{ "Container", 1 },
                                            { "WorldInfo", 2 },
                                            { "weather", 3 }});
        std::vector<uint8_t> inner;
        {
                std::vector<std::vector<uint8_t>> pairs;
                pairs.push_back(pair_int(3, 1));
                put_chunk(inner, 2, K_WORLDDICT_VERSION_1, worldinfo_payload(pairs));
        }
        put_chunk(b, 1, 1, inner);
        run_wi(out, id, b);
}

// a map with no WorldInfo at all: the walk succeeds, the world dict
// keeps its pre-load state (empty here — a fresh process).
static void corpus_wi_none(Out &out)
{
        const char *id = "wi_none";
        std::vector<uint8_t> b = toc_bytes({{ "HeightMapData", 1 }});
        {
                std::vector<uint8_t> d;
                put_i32(d, 2);
                put_i32(d, 2);
                put_i32(d, 4);
                put_bytes(d, std::vector<uint8_t>(4, 0x10));
                put_chunk(b, 1, 3, d);
        }
        run_wi(out, id, b);
}

// not a chunky file at all.
static void corpus_wi_not_chunky(Out &out)
{
        const char *id = "wi_not_chunky";
        std::vector<uint8_t> b;
        put_str(b, "not a map at all");
        run_wi(out, id, b);
}

// a CkMp table of contents that ends at EOF never opens (the container's
// refusal — the walk fails before any parser).
static void corpus_wi_toc_only(Out &out)
{
        const char *id = "wi_toc_only";
        std::vector<uint8_t> b = toc_bytes({{ "WorldInfo", 1 }});
        run_wi(out, id, b);
}

// empty file.
static void corpus_wi_empty(Out &out)
{
        const char *id = "wi_empty";
        std::vector<uint8_t> b;
        run_wi(out, id, b);
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
        fprintf(f, "# gen_mapworldinfo_golden.cpp — WorldHeightMap.cpp ParseWorldDictDataChunk "
                   "transcription (baseline 571a7c5).\n");
        fprintf(f, "# Golden script for rzh-formats::map_worldinfo; see "
                   "tools/differential/README.md.\n");
        fprintf(f, "# Grammar: file/valid/walk/werr/wnone/wdict/wpair/wkey/wstate.\n");

        corpus_wi_v1(out);
        corpus_wi_weather_types(out);
        corpus_wi_sticky(out);
        corpus_wi_versions(out);
        corpus_wi_empty_dict(out);
        corpus_wi_all_types(out);
        corpus_wi_nulname(out);
        corpus_wi_corrupt(out);
        corpus_wi_leftover(out);
        corpus_wi_underdeclared(out);
        corpus_wi_short(out);
        corpus_wi_nested(out);
        corpus_wi_none(out);
        corpus_wi_not_chunky(out);
        corpus_wi_toc_only(out);
        corpus_wi_empty(out);

        fclose(f);
        return 0;
}
