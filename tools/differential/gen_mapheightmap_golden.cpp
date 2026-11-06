// SPDX-License-Identifier: GPL-3.0-or-later
//
// Golden-vector driver for `rzh-formats::map_heightmap` (parity level P0).
//
// Verbatim transcription of the Zero Hour height-map payload's load path —
// Core/GameEngineDevice/Source/W3DDevice/GameClient/WorldHeightMap.cpp at
// the baseline (571a7c5) — over the re-transcribed map data-chunk
// container (Common/System/DataChunk.{h,cpp}, the reader half verbatim
// from gen_mapchunk_golden.cpp):
//
//   - WorldHeightMap::ParseHeightMapData and ParseSizeOnly, the two
//     HeightMapData chunk readers the constructor's full and
//     logical-data-only load modes register: the version-gated border
//     and boundary reads, the synthesised pre-version-4 boundary, the
//     sample array read and the version-1 in-place downsample (with the
//     modes' divergent m_width/m_height update);
//   - isValidTotalMapSize / isValidPlayableMapSize /
//     isValidHeightDataSize, the size contracts the readers enforce
//     (MAX_MAP_DIMENSION 8192, MAX_MAP_AREA 4096*4096 — the
//     MapReaderWriterInfo.h words and version gates);
//   - the constructor's registerParser("HeightMapData", "", ...)/parse
//     walk shape the harness drives (global scope, last-wins repeated
//     chunks, skipped unknown labels).
//
// Deliberate deviations, pinned identically on both sides:
//
// - The two `throw ERROR_CORRUPT_FILE_FORMAT` sites throw a catchable
//   marker the harness turns into `herr` lines; the walk-stop the
//   escaping exception produces is transcribed as the parser returning
//   false (the chunk stays open in both — the hstate lines pin it).
// - The version-4 boundary count the original feeds straight into
//   `m_boundaries.resize(numBorders)`: a negative count converts to a
//   huge size_t and dies inside the standard library (an uncaught
//   std::length_error — a process death, not a game error); the
//   transcription throws a marker so the corpora can pin the refusal.
//   A huge positive count allocates the way the original's resize does
//   (the corpora avoid counts the machine cannot satisfy — the same
//   discipline the HLOD generator's allocation notes record).
// - The zero-born seismic scratch arrays (m_seismicUpdateFlag,
//   m_seismicZVelocities, m_seismicUpdateWidth) are runtime simulation
//   state, not file content: the transcription drops their allocation
//   (a memset of zeros with no observable), and the port documents
//   them out of scope.
// - m_data is a zero-born stand-in for `new UnsignedByte[m_dataSize]`
//   (uninitialised heap in the original); the only shape where the
//   difference is observable is a short-file sample tail, which the
//   container's zero-filled short read already pins on both sides.
// - The container transcription (the reader half) is verbatim from
//   gen_mapchunk_golden.cpp — see that file's header for its own
//   deviation notes (the RefPack branch, the 16-bit WideChar pin, the
//   zero-filled short reads).
//
// Output grammar (one command per line, hex lowercase, '-' = empty):
//
//   file <id> <hex>               the corpus bytes
//   valid <id> <t|f>              isValidFileType() after construction
//   walk <id> <f|s> <t|f>         the parse() verdict per mode
//                                 (f = full load, s = logical-only)
//   hm <id> <f|s> <w> <h> <b> <ds> <nbnd>   the loaded height map
//   bnd <id> <f|s> <i> <x> <y>    boundary i in stored order
//   hdat <id> <f|s> <hex|->       the sample bytes (m_data, all of it)
//   hnone <id> <f|s>              the walk succeeded, no HeightMapData
//   herr <id> <f|s> <kind> <args> the refused load:
//       mapsize <w> <h> <b>
//       datasize <ds> <w> <h> <left>
//       bordercount <n>
//       corrupt                   (the walk itself refused)
//   hstate <id> <f|s> <tell> <depth> <eof>  post-walk stream state
//   val <total|playable> <w> <h> <b> <t|f>  the size validators
//
// ds (m_dataSize) is the DECLARED sample count: a version-1 load never
// updates it, so the logical mode's downsampled extent (hm's w/h) and
// the sample length disagree on purpose — the corpus hm_v1_5x5 pins it.
//
// Usage: gen_mapheightmap_golden <output-file>
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
// the height-map payload (WorldHeightMap.cpp, load-path transcription)
// ---------------------------------------------------------------------------

// MapReaderWriterInfo.h: the version words and size ceilings.
#define K_HEIGHT_MAP_VERSION_1 1 // Height map cell = 5.0
#define K_HEIGHT_MAP_VERSION_2 2 // Height map cell = 10.0
#define K_HEIGHT_MAP_VERSION_3 3 // Added m_borderSize
#define K_HEIGHT_MAP_VERSION_4 4 // Major rev: the boundary list.

// TheSuperHackers @bugfix arcticdolphin 12/10/2026 Map size limits.
#define MAX_MAP_DIMENSION 8192
#define MAX_MAP_AREA (4096 * 4096)

// Lib/ICoord2D.h.
struct ICoord2D {
        Int x = 0;
        Int y = 0;
};

// The game error code the reader throws (ERROR_CORRUPT_FILE_FORMAT as a
// catchable marker so the corpus driver keeps running).
struct CorruptFileFormat {
};

// The process death the original's m_boundaries.resize(negative int)
// reaches inside the standard library (std::length_error, uncaught):
// a catchable marker for the same reason.
struct BorderCountCorrupt {
};

// The height-map members ParseHeightMapData/ParseSizeOnly write, as the
// load leaves them. (The constructor's other members — the seismic
// scratch, the blend-tile arrays, the texture classes — belong to the
// payload slices that write them.)
struct WorldHeightMapLoad {
        Int m_width = 0;       ///< Height map width.
        Int m_height = 0;      ///< Height map height (y size of array).
        Int m_borderSize = 0;  ///< Non-playable border area.
        std::vector<ICoord2D> m_boundaries; ///< the in-game boundaries
        Int m_dataSize = 0;    ///< size of m_data.
        std::vector<uint8_t> m_data; ///< new UnsignedByte[m_dataSize] (zero-born here)

        // transcription-only probe: the boundary count as last read (the
        // bordercount error dump reads it after the catch)
        Int m_lastNumBorders = 0;

        static Bool isValidTotalMapSize(Int width, Int height, Int borderSize)
        {
                if (width <= 0 || width > MAX_MAP_DIMENSION)
                        return false;
                if (height <= 0 || height > MAX_MAP_DIMENSION)
                        return false;

                if (borderSize < 0 || borderSize > (width - 1) / 2 ||
                    borderSize > (height - 1) / 2)
                        return false;

                if (width * height > MAX_MAP_AREA)
                        return false;

                return true;
        }

        static Bool isValidPlayableMapSize(Int width, Int height, Int borderSize)
        {
                if (width <= 0 || height <= 0 || borderSize < 0)
                        return false;

                if (borderSize > (INT_MAX - width) / 2 ||
                    borderSize > (INT_MAX - height) / 2)
                        return false;

                return isValidTotalMapSize(
                        width + 2 * borderSize, height + 2 * borderSize, borderSize);
        }

        static Bool isValidHeightDataSize(DataChunkInput &file, Int dataSize, Int width, Int height)
        {
                return dataSize > 0 && dataSize == width * height &&
                       (UnsignedInt)dataSize <= file.getChunkDataSizeLeft();
        }

        // WorldHeightMap::ParseHeightMapData — the full load's reader.
        Bool parseHeightMapData(DataChunkInput &file, DataChunkInfo *info)
        {
                m_width = file.readInt();
                m_height = file.readInt();
                if (info->version >= K_HEIGHT_MAP_VERSION_3) {
                        m_borderSize = file.readInt();
                } else {
                        m_borderSize = 0;
                }

                if (!isValidTotalMapSize(m_width, m_height, m_borderSize)) {
                        throw CorruptFileFormat();
                }

                if (info->version >= K_HEIGHT_MAP_VERSION_4) {
                        Int numBorders = file.readInt();
                        m_lastNumBorders = numBorders;
                        if (numBorders < 0) {
                                // (the original's m_boundaries.resize(numBorders)
                                // converts the negative int to a huge size_t and
                                // dies inside the standard library; the marker
                                // keeps the corpus driver alive — see the header)
                                throw BorderCountCorrupt();
                        }
                        m_boundaries.resize(numBorders);
                        for (int i = 0; i < numBorders; ++i) {
                                m_boundaries[i].x = file.readInt();
                                m_boundaries[i].y = file.readInt();
                        }
                } else {
                        m_boundaries.resize(1);
                        m_boundaries[0].x = m_width - 2 * m_borderSize;
                        m_boundaries[0].y = m_height - 2 * m_borderSize;
                }

                m_dataSize = file.readInt();
                if (!isValidHeightDataSize(file, m_dataSize, m_width, m_height)) {
                        throw CorruptFileFormat();
                }

                // (the original allocates the zero-born seismic scratch here —
                // m_seismicUpdateFlag, m_seismicZVelocities, both memset 0:
                // runtime state with no observable, dropped per the header)

                // m_data = new UnsignedByte[m_dataSize] — a fresh zero-born
                // buffer per load (the stand-in for the uninitialised heap).
                m_data.assign((size_t)m_dataSize, 0);

                file.readArrayOfBytes((char *)m_data.data(), m_dataSize);
                // Resize me.
                if (info->version == K_HEIGHT_MAP_VERSION_1) {
                        Int newWidth = (m_width + 1) / 2;
                        Int newHeight = (m_height + 1) / 2;
                        Int i, j;
                        for (i = 0; i < newHeight; i++) {
                                for (j = 0; j < newWidth; j++) {
                                        m_data[i * newWidth + j] =
                                            m_data[2 * i * m_width + 2 * j];
                                }
                        }
                }
                // DEBUG_ASSERTCRASH(file.atEndOfChunk(), ("Unexpected data left
                // over.")) — a debug-only contract; the release load leaves the
                // leftovers to the walk's close.
                return true;
        }

        // WorldHeightMap::ParseSizeOnly — the logical-data-only load's
        // reader: the same reads and contracts, a version-1 resize that
        // re-points the extent, and no scratch allocation.
        Bool parseSizeOnly(DataChunkInput &file, DataChunkInfo *info)
        {
                m_width = file.readInt();
                m_height = file.readInt();
                if (info->version >= K_HEIGHT_MAP_VERSION_3) {
                        m_borderSize = file.readInt();
                } else {
                        m_borderSize = 0;
                }

                if (!isValidTotalMapSize(m_width, m_height, m_borderSize)) {
                        throw CorruptFileFormat();
                }

                if (info->version >= K_HEIGHT_MAP_VERSION_4) {
                        Int numBorders = file.readInt();
                        m_lastNumBorders = numBorders;
                        if (numBorders < 0) {
                                throw BorderCountCorrupt();
                        }
                        m_boundaries.resize(numBorders);
                        for (int i = 0; i < numBorders; ++i) {
                                m_boundaries[i].x = file.readInt();
                                m_boundaries[i].y = file.readInt();
                        }
                } else {
                        m_boundaries.resize(1);
                        m_boundaries[0].x = m_width - 2 * m_borderSize;
                        m_boundaries[0].y = m_height - 2 * m_borderSize;
                }

                m_dataSize = file.readInt();
                if (!isValidHeightDataSize(file, m_dataSize, m_width, m_height)) {
                        throw CorruptFileFormat();
                }

                m_data.assign((size_t)m_dataSize, 0);

                file.readArrayOfBytes((char *)m_data.data(), m_dataSize);
                // Resize me.
                if (info->version == K_HEIGHT_MAP_VERSION_1) {
                        Int newWidth = (m_width + 1) / 2;
                        Int newHeight = (m_height + 1) / 2;
                        Int i, j;
                        for (i = 0; i < newHeight; i++) {
                                for (j = 0; j < newWidth; j++) {
                                        m_data[i * newWidth + j] =
                                            m_data[2 * i * m_width + 2 * j];
                                }
                        }
                        m_width = newWidth;
                        m_height = newHeight;
                }
                return true;
        }
};

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
        void state(const char *id, const char *mode, DataChunkInput &in)
        {
                line(std::string("hstate ") + id + " " + mode + " " +
                     std::to_string((Int)in.tell()) + " " + std::to_string(in.depth()) +
                     " " + (in.atEndOfFile() ? "t" : "f"));
        }

        static std::string hxs(const std::string &s)
        {
                if (s.empty())
                        return "-";
                static const char *d = "0123456789abcdef";
                std::string out;
                for (unsigned char c : s) {
                        out.push_back(d[c >> 4]);
                        out.push_back(d[c & 15]);
                }
                return out;
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
static void put_str(std::vector<uint8_t> &b, const std::string &s)
{
        b.insert(b.end(), s.begin(), s.end());
}
static void put_bytes(std::vector<uint8_t> &b, const std::vector<uint8_t> &s)
{
        b.insert(b.end(), s.begin(), s.end());
}

// one chunk: id, version, data size, data. The size word defaults to the
// data length but can lie (the under-declared corpora).
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

// a formula byte string both sides rebuild identically
static std::vector<uint8_t> formula_bytes(size_t n, uint8_t seed)
{
        std::vector<uint8_t> s;
        s.resize(n);
        for (size_t i = 0; i < n; ++i)
                s[i] = (uint8_t)((seed + i * 7) & 0xFF);
        return s;
}

// --- the harness (the constructor's walk, both load modes) ------------------

// the walk's user data: the height-map state the parser writes (the
// original's `this`, cut down to the height-map half)
struct HmState {
        WorldHeightMapLoad hm;
        Bool loaded = false;       // whether any HeightMapData fired
        Bool logicalOnly = false;  // which reader is registered
        // the error capture (which contract refused, with its fields)
        enum ErrKind { ERR_NONE = 0, ERR_MAPSIZE, ERR_DATASIZE, ERR_BORDERCOUNT, ERR_CORRUPT };
        ErrKind errKind = ERR_NONE;
        Int errW = 0, errH = 0, errB = 0, errDS = 0;
        UnsignedInt errLeft = 0;
        Int errBorders = 0;
};

// the registered parser: dispatch to the mode's reader, keep the last
// good state, refuse the walk on a refused load (the observable stop the
// original's escaping ERROR_CORRUPT_FILE_FORMAT produces).
static Bool hm_parser(DataChunkInput &file, DataChunkInfo *info, void *userData)
{
        HmState *st = (HmState *)userData;
        try {
                if (st->logicalOnly)
                        st->hm.parseSizeOnly(file, info);
                else
                        st->hm.parseHeightMapData(file, info);
                st->loaded = true;
                return true;
        } catch (CorruptFileFormat &) {
                // which contract threw: re-derived from the surviving state —
                // the map-size check's own inputs when it refused, else the
                // data-size contract (m_dataSize was the last read).
                if (!WorldHeightMapLoad::isValidTotalMapSize(st->hm.m_width, st->hm.m_height,
                                                              st->hm.m_borderSize)) {
                        st->errKind = HmState::ERR_MAPSIZE;
                        st->errW = st->hm.m_width;
                        st->errH = st->hm.m_height;
                        st->errB = st->hm.m_borderSize;
                } else {
                        st->errKind = HmState::ERR_DATASIZE;
                        st->errDS = st->hm.m_dataSize;
                        st->errW = st->hm.m_width;
                        st->errH = st->hm.m_height;
                        st->errLeft = file.getChunkDataSizeLeft();
                }
                return false;
        } catch (BorderCountCorrupt &) {
                st->errKind = HmState::ERR_BORDERCOUNT;
                st->errBorders = st->hm.m_lastNumBorders;
                return false;
        }
}

// a recursing container observer (the real map loaders' shape — the
// ObjectsList parser that walks its children), for the scope corpus.
static Bool hm_container(DataChunkInput &file, DataChunkInfo *info, void *userData)
{
        (void)info;
        return file.parse(userData);
}

// runs one corpus through both constructor modes and dumps everything.
// with_container registers the recursing ObjectsList observer (the scope
// corpus needs a parent that walks children).
static void run_hm(Out &out, const char *id, const std::vector<uint8_t> &bytes,
                   Bool with_container = false)
{
        out.file(id, bytes);
        for (Int mode = 0; mode < 2; ++mode) {
                const char *m = mode ? "s" : "f";
                CachedFileInputStream s(bytes);
                DataChunkInput in(&s);
                if (mode == 0)
                        out.tf("valid", id, in.isValidFileType());
                HmState st;
                st.logicalOnly = (mode != 0);
                in.registerParser("HeightMapData", "", hm_parser);
                if (with_container)
                        in.registerParser("ObjectsList", "", hm_container);
                Bool ok = in.parse(&st);
                out.line(std::string("walk ") + id + " " + m + (ok ? " t" : " f"));
                if (!ok) {
                        std::string e = std::string("herr ") + id + " " + m + " ";
                        switch (st.errKind) {
                                case HmState::ERR_MAPSIZE:
                                        e += "mapsize " + std::to_string(st.errW) + " " +
                                             std::to_string(st.errH) + " " +
                                             std::to_string(st.errB);
                                        break;
                                case HmState::ERR_DATASIZE:
                                        e += "datasize " + std::to_string(st.errDS) + " " +
                                             std::to_string(st.errW) + " " +
                                             std::to_string(st.errH) + " " +
                                             std::to_string(st.errLeft);
                                        break;
                                case HmState::ERR_BORDERCOUNT:
                                        e += "bordercount " + std::to_string(st.errBorders);
                                        break;
                                default:
                                        // the walk refused before (or without) a
                                        // payload error: the header never opened
                                        e += "corrupt";
                                        break;
                        }
                        out.line(e);
                } else if (st.loaded) {
                        out.line(std::string("hm ") + id + " " + m + " " +
                                 std::to_string(st.hm.m_width) + " " +
                                 std::to_string(st.hm.m_height) + " " +
                                 std::to_string(st.hm.m_borderSize) + " " +
                                 std::to_string(st.hm.m_dataSize) + " " +
                                 std::to_string((Int)st.hm.m_boundaries.size()));
                        for (size_t i = 0; i < st.hm.m_boundaries.size(); ++i) {
                                out.line(std::string("bnd ") + id + " " + m + " " +
                                         std::to_string(i) + " " +
                                         std::to_string(st.hm.m_boundaries[i].x) + " " +
                                         std::to_string(st.hm.m_boundaries[i].y));
                        }
                        out.line(std::string("hdat ") + id + " " + m + " " + hx(st.hm.m_data));
                } else {
                        out.line(std::string("hnone ") + id + " " + m);
                }
                out.state(id, m, in);
        }
}

// --- the corpora -------------------------------------------------------------

// a version-4 payload with explicit boundaries
static std::vector<uint8_t> v4_payload(Int w, Int h, Int border,
                                       const std::vector<std::pair<Int, Int>> &bnd,
                                       const std::vector<uint8_t> &data)
{
        std::vector<uint8_t> p;
        put_i32(p, w);
        put_i32(p, h);
        put_i32(p, border);
        put_i32(p, (Int)bnd.size());
        for (auto &b : bnd) {
                put_i32(p, b.first);
                put_i32(p, b.second);
        }
        put_i32(p, (Int)data.size());
        p.insert(p.end(), data.begin(), data.end());
        return p;
}

// a version-3 payload (border, synthesised boundary)
static std::vector<uint8_t> v3_payload(Int w, Int h, Int border, const std::vector<uint8_t> &data)
{
        std::vector<uint8_t> p;
        put_i32(p, w);
        put_i32(p, h);
        put_i32(p, border);
        put_i32(p, (Int)data.size());
        p.insert(p.end(), data.begin(), data.end());
        return p;
}

// a version-2 payload (no border word, synthesised boundary)
static std::vector<uint8_t> v2_payload(Int w, Int h, const std::vector<uint8_t> &data)
{
        std::vector<uint8_t> p;
        put_i32(p, w);
        put_i32(p, h);
        put_i32(p, (Int)data.size());
        p.insert(p.end(), data.begin(), data.end());
        return p;
}

// the map-load shape: unknown chunks around the height map (the other
// payloads' chunks — skipped whole, the walk's unmatched-label behaviour).
static void corpus_hm_v4(Out &out)
{
        const char *id = "hm_v4";
        std::vector<uint8_t> b = toc_bytes(
            {{"HeightMapData", 1}, {"BlendTileData", 2}, {"GlobalLighting", 3}});
        {
                std::vector<uint8_t> d;
                put_i32(d, 512);
                put_bytes(d, formula_bytes(12, 9));
                put_chunk(b, 2, 7, d); // BlendTileData: unknown here, skipped
        }
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_4,
                  v4_payload(8, 6, 1, {{6, 4}, {8, 6}, {7, 5}}, formula_bytes(48, 3)));
        {
                std::vector<uint8_t> d;
                put_i32(d, 0);
                put_chunk(b, 3, 2, d); // GlobalLighting: unknown here, skipped
        }

        run_hm(out, id, b);
}

// a version-4 map with no boundaries at all.
static void corpus_hm_v4_empty_bnd(Out &out)
{
        const char *id = "hm_v4_empty_bnd";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}});
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_4, v4_payload(3, 3, 0, {}, formula_bytes(9, 5)));
        run_hm(out, id, b);
}

// boundaries are raw ints: negatives and large values store verbatim.
static void corpus_hm_v4_neg_bnd_vals(Out &out)
{
        const char *id = "hm_v4_neg_bnd_vals";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}});
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_4,
                  v4_payload(4, 4, 0, {{-5, 3}, {0, 100000}}, formula_bytes(16, 11)));
        run_hm(out, id, b);
}

// version 3: the border word, the synthesised single boundary.
static void corpus_hm_v3(Out &out)
{
        const char *id = "hm_v3";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}});
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_3, v3_payload(7, 5, 2, formula_bytes(35, 21)));
        run_hm(out, id, b);
}

// version 2: no border word (border 0), the boundary is the full extent.
static void corpus_hm_v2(Out &out)
{
        const char *id = "hm_v2";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}});
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_2, v2_payload(6, 4, formula_bytes(24, 13)));
        run_hm(out, id, b);
}

// version 0: reads like version 2 (all the gates are ceilings).
static void corpus_hm_v0(Out &out)
{
        const char *id = "hm_v0";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}});
        put_chunk(b, 1, 0, v2_payload(2, 2, formula_bytes(4, 7)));
        run_hm(out, id, b);
}

// version 5 (above 4): reads like version 4.
static void corpus_hm_v5(Out &out)
{
        const char *id = "hm_v5";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}});
        put_chunk(b, 1, 5, v4_payload(2, 2, 0, {{2, 2}}, formula_bytes(4, 15)));
        run_hm(out, id, b);
}

// version 1: the downsample. The full load keeps the file's extent; the
// logical load re-points it; neither touches the declared sample count.
static void corpus_hm_v1_5x5(Out &out)
{
        const char *id = "hm_v1_5x5";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}});
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_1, v2_payload(5, 5, formula_bytes(25, 17)));
        run_hm(out, id, b);
}

// version 1 with even dimensions: a 2x2 compaction, 12 stale tail bytes.
static void corpus_hm_v1_4x4(Out &out)
{
        const char *id = "hm_v1_4x4";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}});
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_1, v2_payload(4, 4, formula_bytes(16, 19)));
        run_hm(out, id, b);
}

// version 1, one cell: the compaction is a no-op.
static void corpus_hm_v1_1x1(Out &out)
{
        const char *id = "hm_v1_1x1";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}});
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_1, v2_payload(1, 1, formula_bytes(1, 23)));
        run_hm(out, id, b);
}

// version 1, odd width even height: a 3x2 compaction, 9 stale bytes.
static void corpus_hm_v1_5x3(Out &out)
{
        const char *id = "hm_v1_5x3";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}});
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_1, v2_payload(5, 3, formula_bytes(15, 29)));
        run_hm(out, id, b);
}

// the border maximum: border == (width-1)/2 == (height-1)/2 loads.
static void corpus_hm_border_max(Out &out)
{
        const char *id = "hm_border_max";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}});
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_3, v3_payload(5, 5, 2, formula_bytes(25, 31)));
        run_hm(out, id, b);
}

// (the dimension and area ceilings' accept sides — 8192x1, 4096x4096 —
// are byte-prohibitive goldens (16 MiB of samples); the hm_validators
// matrix pins both edges as pure functions, and the byte corpora pin
// the reject side below.)

// width 0: the map-size contract refuses before the data-size word is
// read (the walk stops mid-chunk, tell pins it).
static void corpus_hm_err_width0(Out &out)
{
        const char *id = "hm_err_width0";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}});
        std::vector<uint8_t> p;
        put_i32(p, 0);
        put_i32(p, 4);
        put_i32(p, 0);
        put_i32(p, 16); // never read
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_3, p);
        run_hm(out, id, b);
}

// negative width.
static void corpus_hm_err_width_neg(Out &out)
{
        const char *id = "hm_err_width_neg";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}});
        std::vector<uint8_t> p;
        put_i32(p, -3);
        put_i32(p, 4);
        put_i32(p, 16);
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_2, p);
        run_hm(out, id, b);
}

// width over MAX_MAP_DIMENSION.
static void corpus_hm_err_dim_over(Out &out)
{
        const char *id = "hm_err_dim_over";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}});
        std::vector<uint8_t> p;
        put_i32(p, 8193);
        put_i32(p, 1);
        put_i32(p, 8193);
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_2, p);
        run_hm(out, id, b);
}

// area over MAX_MAP_AREA (5000x5000 = 25M cells).
static void corpus_hm_err_area(Out &out)
{
        const char *id = "hm_err_area";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}});
        std::vector<uint8_t> p;
        put_i32(p, 5000);
        put_i32(p, 5000);
        put_i32(p, 4); // never read
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_2, p);
        run_hm(out, id, b);
}

// border past (width-1)/2 on the width axis.
static void corpus_hm_err_border(Out &out)
{
        const char *id = "hm_err_border";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}});
        std::vector<uint8_t> p;
        put_i32(p, 5);
        put_i32(p, 5);
        put_i32(p, 3);
        put_i32(p, 25);
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_3, p);
        run_hm(out, id, b);
}

// negative border (version 3 reads it).
static void corpus_hm_err_border_neg(Out &out)
{
        const char *id = "hm_err_border_neg";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}});
        std::vector<uint8_t> p;
        put_i32(p, 6);
        put_i32(p, 6);
        put_i32(p, -1);
        put_i32(p, 36);
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_3, p);
        run_hm(out, id, b);
}

// a 2-high map rejects border 1 through the HEIGHT axis ((2-1)/2 = 0) —
// the width axis alone would accept it.
static void corpus_hm_err_border_height(Out &out)
{
        const char *id = "hm_err_border_height";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}});
        std::vector<uint8_t> p;
        put_i32(p, 3);
        put_i32(p, 2);
        put_i32(p, 1);
        put_i32(p, 6);
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_3, p);
        run_hm(out, id, b);
}

// sample count != width*height (the payload carries no samples at all).
static void corpus_hm_err_datasize(Out &out)
{
        const char *id = "hm_err_datasize";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}});
        std::vector<uint8_t> p;
        put_i32(p, 4);
        put_i32(p, 4);
        put_i32(p, 12);
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_2, p);
        run_hm(out, id, b);
}

// sample count zero.
static void corpus_hm_err_datasize_zero(Out &out)
{
        const char *id = "hm_err_datasize_zero";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}});
        std::vector<uint8_t> p;
        put_i32(p, 2);
        put_i32(p, 2);
        put_i32(p, 0);
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_2, p);
        run_hm(out, id, b);
}

// sample count negative: fails dataSize > 0 before the unsigned view.
static void corpus_hm_err_datasize_neg(Out &out)
{
        const char *id = "hm_err_datasize_neg";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}});
        std::vector<uint8_t> p;
        put_i32(p, 2);
        put_i32(p, 2);
        put_i32(p, -4);
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_2, p);
        run_hm(out, id, b);
}

// sample count == width*height but the chunk declares too few bytes
// (dataLeft 12 < 16 at the check).
static void corpus_hm_err_left(Out &out)
{
        const char *id = "hm_err_left";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}});
        std::vector<uint8_t> p;
        put_i32(p, 4);
        put_i32(p, 4);
        put_i32(p, 16);
        put_bytes(p, formula_bytes(12, 43));
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_2, p, 24); // the lie: 12 + 12
        run_hm(out, id, b);
}

// a negative boundary count (version 4).
static void corpus_hm_err_bordercount(Out &out)
{
        const char *id = "hm_err_bordercount";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}});
        std::vector<uint8_t> p;
        put_i32(p, 4);
        put_i32(p, 4);
        put_i32(p, 0);
        put_i32(p, -2);
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_4, p);
        run_hm(out, id, b);
}

// the under-declared chunk: the size word (8) is smaller than the reads
// (28), so dataLeft goes negative, its u32 view passes the sample-count
// check, the samples read past the declared end into the next chunk's
// header, and the no-seek close leaves the walk opening a phantom chunk
// inside the tail bytes.
static void corpus_hm_underdeclared(Out &out)
{
        const char *id = "hm_underdeclared";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}, {"UnknownPayload", 2}});
        {
                std::vector<uint8_t> p;
                put_i32(p, 4);  // width
                put_i32(p, 4);  // height
                put_i32(p, 16); // sample count (== 4*4; the check passes)
                put_bytes(p, formula_bytes(4, 47));
                put_chunk(b, 1, K_HEIGHT_MAP_VERSION_2, p, 8); // declares 8, reads 28
        }
        {
                std::vector<uint8_t> d(8, 0x11);
                put_chunk(b, 2, 1, d);
        }
        run_hm(out, id, b);
}

// the short-file sample tail: the chunk declares room for all 16 samples
// but the file ends 6 bytes in — the container's zero-filled short read
// completes the array.
static void corpus_hm_short_file(Out &out)
{
        const char *id = "hm_short_file";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}});
        std::vector<uint8_t> p;
        put_i32(p, 4);  // width
        put_i32(p, 4);  // height
        put_i32(p, 0);  // border
        put_i32(p, 16); // sample count
        put_bytes(p, formula_bytes(6, 51)); // then EOF
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_3, p, 32); // declares 16 + 16
        run_hm(out, id, b);
}

// leftovers after the samples (the reader's atEndOfChunk assert is a
// debug contract; the release close skips them and the walk resumes).
static void corpus_hm_leftover(Out &out)
{
        const char *id = "hm_leftover";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}, {"UnknownPayload", 2}});
        {
                std::vector<uint8_t> p;
                put_i32(p, 2);
                put_i32(p, 2);
                put_i32(p, 0);
                put_i32(p, 4);
                put_bytes(p, formula_bytes(4, 55));
                put_bytes(p, std::vector<uint8_t>(6, 0xee)); // leftovers
                put_chunk(b, 1, K_HEIGHT_MAP_VERSION_3, p);
        }
        {
                std::vector<uint8_t> d(4, 0x22);
                put_chunk(b, 2, 1, d);
        }
        run_hm(out, id, b);
}

// the scope rule: a HeightMapData nested inside a container chunk never
// fires the global-scope reader (the container skips it whole); the
// top-level one after it loads.
static void corpus_hm_nested(Out &out)
{
        const char *id = "hm_nested";
        std::vector<uint8_t> b =
            toc_bytes({{"ObjectsList", 1}, {"HeightMapData", 2}, {"Object", 3}});
        {
                std::vector<uint8_t> nested;
                put_chunk(nested, 2, K_HEIGHT_MAP_VERSION_2, v2_payload(2, 2, formula_bytes(4, 59)));
                std::vector<uint8_t> obj;
                put_i32(obj, 10);
                put_i32(obj, 20);
                put_chunk(nested, 3, 1, obj);
                put_chunk(b, 1, 3, nested); // ObjectsList walks its children
        }
        put_chunk(b, 2, K_HEIGHT_MAP_VERSION_3, v3_payload(3, 3, 0, formula_bytes(9, 61)));
        run_hm(out, id, b, true);
}

// repeated HeightMapData chunks: the last one wins.
static void corpus_hm_multi(Out &out)
{
        const char *id = "hm_multi";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}});
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_2, v2_payload(2, 2, formula_bytes(4, 63)));
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_3, v3_payload(5, 4, 1, formula_bytes(20, 65)));
        run_hm(out, id, b);
}

// the second HeightMapData refuses: the walk stops at it, the chunk
// stays open, and the load fails with the payload error.
static void corpus_hm_multi_err(Out &out)
{
        const char *id = "hm_multi_err";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}});
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_2, v2_payload(2, 2, formula_bytes(4, 67)));
        {
                std::vector<uint8_t> p;
                put_i32(p, 0); // width: refuses
                put_i32(p, 2);
                put_i32(p, 4);
                put_i32(p, 0);
                put_chunk(b, 1, K_HEIGHT_MAP_VERSION_2, p);
        }
        run_hm(out, id, b);
}

// a map with no HeightMapData at all: the walk succeeds, the payload is
// absent (the original's null m_data).
static void corpus_hm_none(Out &out)
{
        const char *id = "hm_none";
        std::vector<uint8_t> b = toc_bytes({{"BlendTileData", 1}});
        {
                std::vector<uint8_t> d;
                put_i32(d, 512);
                put_chunk(b, 1, 7, d);
        }
        run_hm(out, id, b);
}

// not a chunky file at all.
static void corpus_hm_not_chunky(Out &out)
{
        const char *id = "hm_not_chunky";
        std::vector<uint8_t> b;
        put_str(b, "not a map at all");
        run_hm(out, id, b);
}

// a CkMp table of contents that ends at EOF never opens (the container's
// refusal — the walk fails before any parser).
static void corpus_hm_toc_only(Out &out)
{
        const char *id = "hm_toc_only";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}});
        run_hm(out, id, b);
}

// empty file.
static void corpus_hm_empty(Out &out)
{
        const char *id = "hm_empty";
        std::vector<uint8_t> b;
        run_hm(out, id, b);
}

// the size validators over the edge matrix (the WorldBuilder-facing
// statics — pure functions, dumped directly).
static void corpus_hm_validators(Out &out)
{
        const char *id = "hm_validators";
        struct Row {
                const char *kind;
                Int w, h, b;
        };
        const Row rows[] = {
                {"total", 1, 1, 0},          {"total", 8192, 1, 0},
                {"total", 8192, 8192, 0},    {"total", 8193, 1, 0},
                {"total", 0, 1, 0},          {"total", -3, 4, 0},
                {"total", 5, 5, 2},          {"total", 5, 5, 3},
                {"total", 5, 5, -1},         {"total", 3, 2, 1},
                {"total", 4096, 4096, 0},    {"total", 4097, 4096, 0},
                {"total", 5000, 5000, 0},    {"playable", 3, 3, 1},
                {"playable", 1, 1, 3},       {"playable", 8192, 8192, 1},
                {"playable", 4096, 4096, 1}, {"playable", 1, 1, -1},
                {"playable", 0, 1, 0},       {"playable", 536870911, 536870911, 1073741823},
        };
        for (const Row &r : rows) {
                Bool v = (r.kind[0] == 't')
                             ? WorldHeightMapLoad::isValidTotalMapSize(r.w, r.h, r.b)
                             : WorldHeightMapLoad::isValidPlayableMapSize(r.w, r.h, r.b);
                out.line(std::string("val ") + id + " " + r.kind + " " + std::to_string(r.w) +
                         " " + std::to_string(r.h) + " " + std::to_string(r.b) + (v ? " t" : " f"));
        }
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
        Out out{f};
        fprintf(f, "# gen_mapheightmap_golden.cpp — WorldHeightMap.cpp HeightMapData "
                   "transcription (baseline 571a7c5).\n");
        fprintf(f, "# Golden script for rzh-formats::map_heightmap; see "
                   "tools/differential/README.md.\n");
        fprintf(f, "# Grammar: file/valid/walk/hm/bnd/hdat/hnone/herr/hstate/val.\n");

        corpus_hm_v4(out);
        corpus_hm_v4_empty_bnd(out);
        corpus_hm_v4_neg_bnd_vals(out);
        corpus_hm_v3(out);
        corpus_hm_v2(out);
        corpus_hm_v0(out);
        corpus_hm_v5(out);
        corpus_hm_v1_5x5(out);
        corpus_hm_v1_4x4(out);
        corpus_hm_v1_1x1(out);
        corpus_hm_v1_5x3(out);
        corpus_hm_border_max(out);
        corpus_hm_err_width0(out);
        corpus_hm_err_width_neg(out);
        corpus_hm_err_dim_over(out);
        corpus_hm_err_area(out);
        corpus_hm_err_border(out);
        corpus_hm_err_border_neg(out);
        corpus_hm_err_border_height(out);
        corpus_hm_err_datasize(out);
        corpus_hm_err_datasize_zero(out);
        corpus_hm_err_datasize_neg(out);
        corpus_hm_err_left(out);
        corpus_hm_err_bordercount(out);
        corpus_hm_underdeclared(out);
        corpus_hm_short_file(out);
        corpus_hm_leftover(out);
        corpus_hm_nested(out);
        corpus_hm_multi(out);
        corpus_hm_multi_err(out);
        corpus_hm_none(out);
        corpus_hm_not_chunky(out);
        corpus_hm_toc_only(out);
        corpus_hm_empty(out);
        corpus_hm_validators(out);

        fclose(f);
        return 0;
}
