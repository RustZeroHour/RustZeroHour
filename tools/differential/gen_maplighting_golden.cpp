// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Golden-vector driver for `rzh-formats::map_lighting` (parity
// level P0). Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026
// the RustZeroHour contributors.
//
// Verbatim transcription of the Zero Hour global-lighting payload's load
// path — Core/GameEngineDevice/Source/W3DDevice/GameClient/WorldHeightMap.cpp
// at the baseline (571a7c5) — over the re-transcribed map data-chunk
// container (Common/System/DataChunk.{h,cpp}, the reader half verbatim
// from gen_mapchunk_golden.cpp):
//
//   - WorldHeightMap::ParseLightingDataChunk, the GlobalLighting chunk
//     reader the constructor's full load registers: the time-of-day word,
//     the init pass that fills both light tables with the not-totally-
//     invalid defaults (ambient/diffuse black, position 0,0,-1), the
//     four time-of-day blocks of interleaved terrain/object light reads
//     (terrain light 0, object light 0, the version-2 extra object
//     lights 1-2, the version-3 extra terrain lights 1-2), and the
//     bytes-left-gated shadow colour word;
//   - the constructor's registerParser("GlobalLighting", "", ...)/parse
//     walk shape the harness drives (global scope, last-wins repeated
//     chunks, skipped unknown labels).
//
// Deliberate deviations, pinned identically on both sides:
//
// - The reader writes TheWritableGlobalData (m_timeOfDay,
//   m_terrainLighting[TIME_OF_DAY_COUNT][MAX_GLOBAL_LIGHTS],
//   m_terrainObjectsLighting) and TheW3DShadowManager (setShadowColor) —
//   global singletons. The transcription keeps the members and the
//   shadow colour as plain state (the consumer actions are the future
//   consumer's); the TIME_OF_DAY_INVALID slots of the real arrays are
//   INI state the chunk never writes and are not dumped.
// - The reader has no failure mode on the wire (every read is a
//   zero-filled short read at worst), so the only refusal the harness
//   can see is the walk's own (a non-CkMp header) — the gerr corrupt
//   lines.
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
//   glt <id> <tod> <set> <color>  the loaded lighting summary: the raw
//                                 timeOfDay word, whether the shadow
//                                 colour word was read, its ARGB value
//   gter <id> <slot> <j> <9 words>  terrain light j of time-of-day slot
//                                 (0=Morning..3=Night): ambient rgb,
//                                 diffuse rgb, lightPos xyz, as f32 bits
//   gobj <id> <slot> <j> <9 words>  the object-light table, same shape
//   gnone <id>                    the walk succeeded, no GlobalLighting
//   gerr <id> corrupt             the walk itself refused
//   gstate <id> <tell> <depth> <eof>  post-walk stream state
//
// Usage: gen_maplighting_golden <output-file>
//
// Temporary differential-testing infrastructure: removed at cut-over (M5).
//
// This program is free software; you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the
// Free Software Foundation, either version 3 of the License, or (at your
// option) any later version.

#include <climits>

#include <algorithm>
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
                                        throw 13;   // ERROR_CORRUPT_FILE_FORMAT
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
// the global-lighting payload (WorldHeightMap.cpp, load-path transcription)
// ---------------------------------------------------------------------------

// MapReaderWriterInfo.h: the version words.
#define K_LIGHTING_VERSION_1 1
#define K_LIGHTING_VERSION_2 2 // Added 2 additional global lights for objects.
#define K_LIGHTING_VERSION_3 3 // Added 2 additional global lights for terrain.

// GameType.h: TimeOfDay.
enum TimeOfDay {
        TIME_OF_DAY_INVALID,
        TIME_OF_DAY_MORNING,
        TIME_OF_DAY_AFTERNOON,
        TIME_OF_DAY_EVENING,
        TIME_OF_DAY_NIGHT,
        TIME_OF_DAY_COUNT,
        TIME_OF_DAY_FIRST = TIME_OF_DAY_MORNING,
};

// GlobalData.h: the light-table width.
#define MAX_GLOBAL_LIGHTS 3

// GameType.h: Coord3D (the float triple).
struct Coord3D {
        Real x = 0, y = 0, z = 0;
};

// GlobalData.h: RGBColor.
struct RGBColor {
        Real red = 0, green = 0, blue = 0;
};

// GlobalData.h: TerrainLighting.
struct TerrainLighting {
        RGBColor ambient;
        RGBColor diffuse;
        Coord3D lightPos;
};

// GlobalData's lighting members — the slice ParseLightingDataChunk writes
// (m_timeOfDay, the two light tables, and the shadow colour the shadow
// manager consumes), as the load leaves them. The TIME_OF_DAY_INVALID row
// of the real arrays is INI state the chunk never touches; it stays
// default-constructed here and is not dumped.
struct GlobalLightingLoad {
        Int m_timeOfDay = 0;
        TerrainLighting m_terrainLighting[TIME_OF_DAY_COUNT][MAX_GLOBAL_LIGHTS];
        TerrainLighting m_terrainObjectsLighting[TIME_OF_DAY_COUNT][MAX_GLOBAL_LIGHTS];
        // TheW3DShadowManager's m_shadowColor, and whether this load set it
        // (setShadowColor is a consumer action on a singleton; the value is
        // the observable).
        UnsignedInt m_shadowColor = 0;
        Bool m_shadowColorSet = false;

        // WorldHeightMap::ParseLightingDataChunk — the full load's global
        // lights reader (verbatim reads; the singleton stores become the
        // members above).
        Bool parseLightingDataChunk(DataChunkInput &file, DataChunkInfo *info)
        {
                m_timeOfDay = file.readInt();
                Int i;
                TerrainLighting initLightValues = { { 0, 0, 0 }, { 0, 0, 0 }, { 0, 0, -1.0f } };

                // initialize the directions of the lights to not be totally
                // invalid, in case old maps are read
                for (i = 0; i < 4; i++) {
                        for (Int j = 0; j < MAX_GLOBAL_LIGHTS; j++) {
                                m_terrainObjectsLighting[i + TIME_OF_DAY_FIRST][j] = initLightValues;
                                m_terrainLighting[i + TIME_OF_DAY_FIRST][j] = initLightValues;
                        }
                }

                for (i = 0; i < 4; i++) {
                        m_terrainLighting[i + TIME_OF_DAY_FIRST][0].ambient.red = file.readReal();
                        m_terrainLighting[i + TIME_OF_DAY_FIRST][0].ambient.green = file.readReal();
                        m_terrainLighting[i + TIME_OF_DAY_FIRST][0].ambient.blue = file.readReal();
                        m_terrainLighting[i + TIME_OF_DAY_FIRST][0].diffuse.red = file.readReal();
                        m_terrainLighting[i + TIME_OF_DAY_FIRST][0].diffuse.green = file.readReal();
                        m_terrainLighting[i + TIME_OF_DAY_FIRST][0].diffuse.blue = file.readReal();
                        m_terrainLighting[i + TIME_OF_DAY_FIRST][0].lightPos.x = file.readReal();
                        m_terrainLighting[i + TIME_OF_DAY_FIRST][0].lightPos.y = file.readReal();
                        m_terrainLighting[i + TIME_OF_DAY_FIRST][0].lightPos.z = file.readReal();

                        m_terrainObjectsLighting[i + TIME_OF_DAY_FIRST][0].ambient.red = file.readReal();
                        m_terrainObjectsLighting[i + TIME_OF_DAY_FIRST][0].ambient.green = file.readReal();
                        m_terrainObjectsLighting[i + TIME_OF_DAY_FIRST][0].ambient.blue = file.readReal();
                        m_terrainObjectsLighting[i + TIME_OF_DAY_FIRST][0].diffuse.red = file.readReal();
                        m_terrainObjectsLighting[i + TIME_OF_DAY_FIRST][0].diffuse.green = file.readReal();
                        m_terrainObjectsLighting[i + TIME_OF_DAY_FIRST][0].diffuse.blue = file.readReal();
                        m_terrainObjectsLighting[i + TIME_OF_DAY_FIRST][0].lightPos.x = file.readReal();
                        m_terrainObjectsLighting[i + TIME_OF_DAY_FIRST][0].lightPos.y = file.readReal();
                        m_terrainObjectsLighting[i + TIME_OF_DAY_FIRST][0].lightPos.z = file.readReal();

                        if (info->version >= K_LIGHTING_VERSION_2) {
                                for (Int j = 1; j < 3; j++) // added support for 2 extra object lights
                                {
                                        m_terrainObjectsLighting[i + TIME_OF_DAY_FIRST][j].ambient.red = file.readReal();
                                        m_terrainObjectsLighting[i + TIME_OF_DAY_FIRST][j].ambient.green = file.readReal();
                                        m_terrainObjectsLighting[i + TIME_OF_DAY_FIRST][j].ambient.blue = file.readReal();
                                        m_terrainObjectsLighting[i + TIME_OF_DAY_FIRST][j].diffuse.red = file.readReal();
                                        m_terrainObjectsLighting[i + TIME_OF_DAY_FIRST][j].diffuse.green = file.readReal();
                                        m_terrainObjectsLighting[i + TIME_OF_DAY_FIRST][j].diffuse.blue = file.readReal();
                                        m_terrainObjectsLighting[i + TIME_OF_DAY_FIRST][j].lightPos.x = file.readReal();
                                        m_terrainObjectsLighting[i + TIME_OF_DAY_FIRST][j].lightPos.y = file.readReal();
                                        m_terrainObjectsLighting[i + TIME_OF_DAY_FIRST][j].lightPos.z = file.readReal();
                                }
                        }
                        if (info->version >= K_LIGHTING_VERSION_3) {
                                for (Int j = 1; j < 3; j++) // added support for 2 extra terrain lights
                                {
                                        m_terrainLighting[i + TIME_OF_DAY_FIRST][j].ambient.red = file.readReal();
                                        m_terrainLighting[i + TIME_OF_DAY_FIRST][j].ambient.green = file.readReal();
                                        m_terrainLighting[i + TIME_OF_DAY_FIRST][j].ambient.blue = file.readReal();
                                        m_terrainLighting[i + TIME_OF_DAY_FIRST][j].diffuse.red = file.readReal();
                                        m_terrainLighting[i + TIME_OF_DAY_FIRST][j].diffuse.green = file.readReal();
                                        m_terrainLighting[i + TIME_OF_DAY_FIRST][j].diffuse.blue = file.readReal();
                                        m_terrainLighting[i + TIME_OF_DAY_FIRST][j].lightPos.x = file.readReal();
                                        m_terrainLighting[i + TIME_OF_DAY_FIRST][j].lightPos.y = file.readReal();
                                        m_terrainLighting[i + TIME_OF_DAY_FIRST][j].lightPos.z = file.readReal();
                                }
                        }
                }
                if (!file.atEndOfChunk()) {
                        UnsignedInt shadowColor = file.readInt();
                        // (TheW3DShadowManager->setShadowColor(shadowColor) — a
                        // consumer action on a singleton; the value is kept.)
                        m_shadowColor = shadowColor;
                        m_shadowColorSet = true;
                }
                // DEBUG_ASSERTCRASH(file.atEndOfChunk(), ("Unexpected data left
                // over.")) — a debug-only contract; the release load leaves the
                // leftovers to the walk's close.
                return true;
        }
};

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
                line(std::string("gstate ") + id + " " + std::to_string((Int)in.tell()) + " " +
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

// --- the harness (the constructor's full-load walk, lighting only) ----------

// the walk's user state: the lighting members the parser writes (the
// original's TheWritableGlobalData, cut down to the lighting half)
struct GlState {
        GlobalLightingLoad gl;
        Bool loaded = false; // whether any GlobalLighting fired
};

// the registered parser: the reader has no failure mode (every read is a
// zero-filled short read at worst), so it always returns true.
static Bool gl_parser(DataChunkInput &file, DataChunkInfo *info, void *userData)
{
        GlState *st = (GlState *)userData;
        st->gl.parseLightingDataChunk(file, info);
        st->loaded = true;
        return true;
}

// a recursing container observer (the real map loaders' shape — the
// ObjectsList parser that walks its children), for the scope corpus.
static Bool gl_container(DataChunkInput &file, DataChunkInfo *info, void *userData)
{
        (void)info;
        return file.parse(userData);
}

// one light as nine f32 bit words.
static void dump_light(Out &out, const char *id, const char *tag, Int slot, Int j,
                       const TerrainLighting &l)
{
        char buf[256];
        snprintf(buf, sizeof buf, "%s %s %d %d %08x %08x %08x %08x %08x %08x %08x %08x %08x",
                 tag, id, (int)slot, (int)j, f32bits(l.ambient.red), f32bits(l.ambient.green),
                 f32bits(l.ambient.blue), f32bits(l.diffuse.red), f32bits(l.diffuse.green),
                 f32bits(l.diffuse.blue), f32bits(l.lightPos.x), f32bits(l.lightPos.y),
                 f32bits(l.lightPos.z));
        out.line(buf);
}

// runs one corpus through the full-load walk and dumps everything.
// with_container registers the recursing ObjectsList observer (the scope
// corpus needs a parent that walks children).
static void run_gl(Out &out, const char *id, const std::vector<uint8_t> &bytes,
                   Bool with_container = false)
{
        out.file(id, bytes);
        CachedFileInputStream s(bytes);
        DataChunkInput in(&s);
        out.tf("valid", id, in.isValidFileType());
        in.registerParser("GlobalLighting", "", gl_parser);
        if (with_container)
                in.registerParser("ObjectsList", "", gl_container);
        GlState st;
        Bool ok = in.parse(&st);
        out.line(std::string("walk ") + id + (ok ? " t" : " f"));
        if (!ok) {
                out.line(std::string("gerr ") + id + " corrupt");
        } else if (st.loaded) {
                char buf[64];
                snprintf(buf, sizeof buf, "%08x", st.gl.m_shadowColor);
                out.line(std::string("glt ") + id + " " + std::to_string(st.gl.m_timeOfDay) + " " +
                         (st.gl.m_shadowColorSet ? "t" : "f") + " " + buf);
                for (Int i = 0; i < 4; ++i) {
                        for (Int j = 0; j < MAX_GLOBAL_LIGHTS; ++j)
                                dump_light(out, id, "gter", i, j, st.gl.m_terrainLighting[i + TIME_OF_DAY_FIRST][j]);
                        for (Int j = 0; j < MAX_GLOBAL_LIGHTS; ++j)
                                dump_light(out, id, "gobj", i, j,
                                           st.gl.m_terrainObjectsLighting[i + TIME_OF_DAY_FIRST][j]);
                }
        } else {
                out.line(std::string("gnone ") + id);
        }
        out.state(id, in);
}

// --- the corpora -------------------------------------------------------------

// one light's 9 reals: base + k/4, all exactly representable quarters.
static void put_light(std::vector<uint8_t> &b, Real base)
{
        for (Int k = 0; k < 9; ++k)
                put_f32(b, base + (Real)k * 0.25f);
}

// a full payload for the given version: tod, then the four per-time-of-day
// blocks in the reader's interleaved order (terrain 0, objects 0, the
// version-2 object lights 1-2, the version-3 terrain lights 1-2), then the
// optional shadow word.
static std::vector<uint8_t> lighting_payload(Int version, Int tod, Real base, Bool with_shadow,
                                             UnsignedInt shadow)
{
        std::vector<uint8_t> p;
        put_i32(p, tod);
        for (Int i = 0; i < 4; ++i) {
                put_light(p, base + (Real)i); // terrain light 0
                put_light(p, base + 10 + (Real)i); // objects light 0
                if (version >= K_LIGHTING_VERSION_2) {
                        for (Int j = 1; j < 3; ++j)
                                put_light(p, base + 20 + (Real)(j * 4 + i)); // object lights 1-2
                }
                if (version >= K_LIGHTING_VERSION_3) {
                        for (Int j = 1; j < 3; ++j)
                                put_light(p, base + 40 + (Real)(j * 4 + i)); // terrain lights 1-2
                }
        }
        if (with_shadow)
                put_i32(p, (Int)shadow);
        return p;
}

// the modern full shape: version 3, every light read, the shadow word with
// its high bit set (the unsigned view).
static void corpus_gl_v3(Out &out)
{
        const char *id = "gl_v3";
        std::vector<uint8_t> b = toc_bytes({{ "GlobalLighting", 1 }});
        std::vector<uint8_t> p = lighting_payload(K_LIGHTING_VERSION_3, 2, 0.25f, true, 0x80000055u);
        put_chunk(b, 1, K_LIGHTING_VERSION_3, p);
        run_gl(out, id, b);
}

// version 2: the object lights 1-2 are read, the terrain lights 1-2 keep
// the init values (ambient/diffuse black, position 0,0,-1); no shadow word.
static void corpus_gl_v2(Out &out)
{
        const char *id = "gl_v2";
        std::vector<uint8_t> b = toc_bytes({{ "GlobalLighting", 1 }});
        std::vector<uint8_t> p = lighting_payload(K_LIGHTING_VERSION_2, 4, 1.5f, false, 0);
        put_chunk(b, 1, K_LIGHTING_VERSION_2, p);
        run_gl(out, id, b);
}

// version 1: only light 0 of each table is read; the timeOfDay word is the
// INVALID slot's 0 (stored raw, never validated).
static void corpus_gl_v1(Out &out)
{
        const char *id = "gl_v1";
        std::vector<uint8_t> b = toc_bytes({{ "GlobalLighting", 1 }});
        std::vector<uint8_t> p = lighting_payload(K_LIGHTING_VERSION_1, 0, 2.25f, false, 0);
        put_chunk(b, 1, K_LIGHTING_VERSION_1, p);
        run_gl(out, id, b);
}

// version 0: reads like a version 1 chunk (the words are ceilings); the
// timeOfDay word past NIGHT, stored raw.
static void corpus_gl_v0(Out &out)
{
        const char *id = "gl_v0";
        std::vector<uint8_t> b = toc_bytes({{ "GlobalLighting", 1 }});
        std::vector<uint8_t> p = lighting_payload(0, 7, 3.75f, false, 0);
        put_chunk(b, 1, 0, p);
        run_gl(out, id, b);
}

// a future version 4: reads like a version 3 chunk.
static void corpus_gl_v4(Out &out)
{
        const char *id = "gl_v4";
        std::vector<uint8_t> b = toc_bytes({{ "GlobalLighting", 1 }});
        std::vector<uint8_t> p = lighting_payload(4, 1, 4.5f, true, 0x0000007fu);
        put_chunk(b, 1, 4, p);
        run_gl(out, id, b);
}

// version 1 with four trailing bytes: the shadow gate is on the bytes
// left, not the version — the word is read for a version-1 chunk too.
static void corpus_gl_v1_shadow(Out &out)
{
        const char *id = "gl_v1_shadow";
        std::vector<uint8_t> b = toc_bytes({{ "GlobalLighting", 1 }});
        std::vector<uint8_t> p = lighting_payload(K_LIGHTING_VERSION_1, 3, 5.25f, false, 0);
        put_i32(p, (Int)0x40404040u); // the trailing shadow word
        put_chunk(b, 1, K_LIGHTING_VERSION_1, p);
        run_gl(out, id, b);
}

// version 3 exactly consumed (no shadow word): the colour stays unset.
static void corpus_gl_v3_noshadow(Out &out)
{
        const char *id = "gl_v3_noshadow";
        std::vector<uint8_t> b = toc_bytes({{ "GlobalLighting", 1 }});
        std::vector<uint8_t> p = lighting_payload(K_LIGHTING_VERSION_3, 2, 6.0f, false, 0);
        put_chunk(b, 1, K_LIGHTING_VERSION_3, p);
        run_gl(out, id, b);
}

// raw bit patterns: -0.0, a subnormal, f32::MAX, a quiet NaN and formula
// words — pure bit transport through the reader (no arithmetic runs on
// lighting values). Version 3 with a shadow word.
static void corpus_gl_bits(Out &out)
{
        const char *id = "gl_bits";
        std::vector<uint8_t> b = toc_bytes({{ "GlobalLighting", 1 }});
        std::vector<uint8_t> p;
        put_i32(p, 2);
        // the 24 lights' 9 words each (four time-of-day blocks of terrain 0,
        // objects 0-2, terrain 1-2 in the reader's interleaved order): the
        // first three words of the first two lights carry the special
        // patterns, the rest follow the word formula.
        const UnsignedInt specials[6] = { 0x80000000u, 0x00000001u, 0x7f7fffffu,
                                         0x7fc00000u, 0xff7fffffu, 0x00800000u };
        for (Int light = 0; light < 24; ++light) {
                for (Int k = 0; k < 9; ++k) {
                        Int flat = light * 9 + k;
                        if (flat < 6)
                                put_f32_bits(p, specials[flat]);
                        else
                                put_f32_bits(p, 0x3e000000u + (UnsignedInt)flat * 0x00100001u);
                }
        }
        put_i32(p, (Int)0xdeadbeefu);
        put_chunk(b, 1, K_LIGHTING_VERSION_3, p);
        run_gl(out, id, b);
}

// a chunk with only eight bytes: every read past them zero-fills, the
// bytes-left word goes negative, and the shadow word is not read.
static void corpus_gl_short(Out &out)
{
        const char *id = "gl_short";
        std::vector<uint8_t> b = toc_bytes({{ "GlobalLighting", 1 }});
        std::vector<uint8_t> p;
        put_i32(p, 2);
        put_i32(p, 0x3f800000); // one stray real
        put_chunk(b, 1, K_LIGHTING_VERSION_3, p);
        run_gl(out, id, b);
}

// the size word lies small: the reads continue past the declared end into
// the real bytes (the signed data-left goes negative, so the shadow word
// is not read and the close does not seek).
static void corpus_gl_underdeclared(Out &out)
{
        const char *id = "gl_underdeclared";
        std::vector<uint8_t> b = toc_bytes({{ "GlobalLighting", 1 }});
        std::vector<uint8_t> p = lighting_payload(K_LIGHTING_VERSION_1, 2, 7.0f, false, 0);
        put_chunk(b, 1, K_LIGHTING_VERSION_1, p, 100);
        run_gl(out, id, b);
}

// eight leftover bytes after the shadow word: the walk's close skips them.
static void corpus_gl_leftover(Out &out)
{
        const char *id = "gl_leftover";
        std::vector<uint8_t> b = toc_bytes({{ "GlobalLighting", 1 }});
        std::vector<uint8_t> p = lighting_payload(K_LIGHTING_VERSION_3, 2, 8.25f, true, 0x01020304u);
        put_bytes(p, std::vector<uint8_t>(8, 0xee));
        put_chunk(b, 1, K_LIGHTING_VERSION_3, p);
        run_gl(out, id, b);
}

// repeated GlobalLighting chunks: the last one wins.
static void corpus_gl_multi(Out &out)
{
        const char *id = "gl_multi";
        std::vector<uint8_t> b = toc_bytes({{ "GlobalLighting", 1 }});
        std::vector<uint8_t> p1 = lighting_payload(K_LIGHTING_VERSION_2, 1, 9.5f, false, 0);
        put_chunk(b, 1, K_LIGHTING_VERSION_2, p1);
        std::vector<uint8_t> p2 = lighting_payload(K_LIGHTING_VERSION_3, 4, 10.75f, true, 0x0a0b0c0du);
        put_chunk(b, 1, K_LIGHTING_VERSION_3, p2);
        run_gl(out, id, b);
}

// the scope rule: a GlobalLighting nested inside a container chunk never
// fires the global-scope reader (the container skips it whole); the
// top-level one after it loads.
static void corpus_gl_nested(Out &out)
{
        const char *id = "gl_nested";
        std::vector<uint8_t> b = toc_bytes({{ "ObjectsList", 1 }, { "GlobalLighting", 2 }});
        {
                std::vector<uint8_t> nested;
                std::vector<uint8_t> inner = lighting_payload(K_LIGHTING_VERSION_3, 2, 11.5f, true, 0x11111111u);
                put_chunk(nested, 2, K_LIGHTING_VERSION_3, inner);
                put_chunk(b, 1, 3, nested); // ObjectsList walks its children
        }
        std::vector<uint8_t> top = lighting_payload(K_LIGHTING_VERSION_1, 3, 12.0f, false, 0);
        put_chunk(b, 2, K_LIGHTING_VERSION_1, top);
        run_gl(out, id, b, true);
}

// a map with no GlobalLighting at all: the walk succeeds, the payload is
// absent (GlobalData keeps its INI state).
static void corpus_gl_none(Out &out)
{
        const char *id = "gl_none";
        std::vector<uint8_t> b = toc_bytes({{ "HeightMapData", 1 }});
        {
                std::vector<uint8_t> d;
                put_i32(d, 2);
                put_i32(d, 2);
                put_i32(d, 4);
                put_bytes(d, std::vector<uint8_t>(4, 0x10));
                put_chunk(b, 1, 3, d);
        }
        run_gl(out, id, b);
}

// not a chunky file at all.
static void corpus_gl_not_chunky(Out &out)
{
        const char *id = "gl_not_chunky";
        std::vector<uint8_t> b;
        put_str(b, "not a map at all");
        run_gl(out, id, b);
}

// a CkMp table of contents that ends at EOF never opens (the container's
// refusal — the walk fails before any parser).
static void corpus_gl_toc_only(Out &out)
{
        const char *id = "gl_toc_only";
        std::vector<uint8_t> b = toc_bytes({{ "GlobalLighting", 1 }});
        run_gl(out, id, b);
}

// empty file.
static void corpus_gl_empty(Out &out)
{
        const char *id = "gl_empty";
        std::vector<uint8_t> b;
        run_gl(out, id, b);
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
        fprintf(f, "# gen_maplighting_golden.cpp — WorldHeightMap.cpp ParseLightingDataChunk "
                   "transcription (baseline 571a7c5).\n");
        fprintf(f, "# Golden script for rzh-formats::map_lighting; see "
                   "tools/differential/README.md.\n");
        fprintf(f, "# Grammar: file/valid/walk/glt/gter/gobj/gnone/gerr/gstate.\n");

        corpus_gl_v3(out);
        corpus_gl_v2(out);
        corpus_gl_v1(out);
        corpus_gl_v0(out);
        corpus_gl_v4(out);
        corpus_gl_v1_shadow(out);
        corpus_gl_v3_noshadow(out);
        corpus_gl_bits(out);
        corpus_gl_short(out);
        corpus_gl_underdeclared(out);
        corpus_gl_leftover(out);
        corpus_gl_multi(out);
        corpus_gl_nested(out);
        corpus_gl_none(out);
        corpus_gl_not_chunky(out);
        corpus_gl_toc_only(out);
        corpus_gl_empty(out);

        fclose(f);
        return 0;
}
