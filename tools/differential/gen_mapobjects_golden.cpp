// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Golden-vector driver for `rzh-formats::map_objects` (parity
// level P0). Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026
// the RustZeroHour contributors.
//
// Verbatim transcription of the Zero Hour objects-list load path —
// Core/GameEngineDevice/Source/W3DDevice/GameClient/WorldHeightMap.cpp
// at the baseline (571a7c5) — over the re-transcribed map data-chunk
// container (Common/System/DataChunk.{h,cpp}, the reader half verbatim
// from gen_mapchunk_golden.cpp):
//
//   - WorldHeightMap::ParseObjectsDataChunk, the ObjectsList container
//     reader the constructor's logical-data-only load registers (the
//     m_currentObject link-cursor reset, the nested "Object" parser
//     registration in the list's scope, the recursive parse walk);
//   - WorldHeightMap::ParseObjectDataChunk's dispatch (readDict from
//     version 2) and WorldHeightMap::ParseObjectData, the Object chunk
//     reader: the location triple with the version-gated z zeroing
//     (version <= 2), the angle, the flags, the name, the version-gated
//     property dict, the z-range drop check (-100*MAP_XY_FACTOR ..
//     (255*10)*MAP_HEIGHT_SCALE), the MapObject construction (the
//     normalizeAngle wrap — NaN to 0, the double-precision loops — and
//     the property-sheet copy) and the three well-known derivations
//     (waypointID INT, lightHeightAboveTerrain REAL, scorchType INT);
//   - MapObject's constructor state (MapObject.cpp in the same file):
//     validateName (the identity), the normalized angle, the flags and
//     the copied dict; m_thingTemplate is the ThingFactory lookup —
//     consumer-side, dropped;
//   - the constructor's registerParser("ObjectsList", "", ...)/parse
//     walk shape the harness drives (global scope, last-wins repeated
//     chunks, skipped unknown labels) and freeListOfMapObjects' empty
//     start.
//
// Deliberate deviations, pinned identically on both sides:
//
// - The linked list (TheMapObjectListPtr + m_nextMapObject) is a
//   vector: the head is only ever overwritten, never cleared, so the
//   first kept object of a later ObjectsList chunk orphans the previous
//   list (unreachable in the original, dropped here) — the vector
//   models exactly the reachable list. m_currentObject's null-ness is
//   the link cursor (the pointer value itself is never dereferenced
//   after the linkage refactor).
// - The original reads its WorldHeightMap* in ParseObjectDataChunk
//   through file.m_userData, which nothing on the load path ever sets:
//   the null-this call works by accident (ParseObjectData never touches
//   it). The transcription passes the state directly.
// - normalizeAngle's while-loops never terminate on ±infinite angles (a
//   process hang); the transcription throws a marker so the corpora can
//   pin the refusal, the same decision the port makes. NaN is the
//   original's own early-out (0). The loop counts are unbounded in both
//   — the corpora keep angles within ~1000 rad.
// - readDict's corrupt-type throw carries the type byte (a marker
//   struct instead of the bare int the mapchunk generator throws), so
//   the golden can pin which impossible type refused the load.
// - The name-key generator gains a names() introspection (the
//   transcription-only views the other generators' containers already
//   carry) so the final interning order is dumpable: the three seeded
//   well-known keys (WellKnownKeys.h's StaticNameKey startup order —
//   lightHeightAboveTerrain, waypointID, scorchType), then the file's
//   names in first-seen order. The engine's full startup set (the whole
//   WellKnownKeys table, link-order-dependent) is out of this slice's
//   scope — the port seeds the same three keys the same way.
// - The drops' DEBUG_LOG ("Removing object at z height") is the
//   original's only trace; the harness counts them (odrop lines).
// - The container transcription (the reader half) is otherwise verbatim
//   from gen_mapchunk_golden.cpp — see that file's header for its own
//   deviation notes (the RefPack branch, the 16-bit WideChar pin, the
//   zero-filled short reads).
//
// Output grammar (one command per line, hex lowercase, '-' = empty):
//
//   file <id> <hex>               the corpus bytes
//   valid <id> <t|f>              isValidFileType() after construction
//   walk <id> <t|f>               the parse() verdict
//   oerr <id> corrupt             the walk itself refused
//   oerr <id> dict <type>         the property dict's corrupt pair type
//   oerr <id> angle               the infinite-angle refusal
//   objn <id> <n>                 the kept-object count (the list head
//                                 is null exactly when this is 0)
//   obj <id> <i> <x> <y> <z> <angle> <flags> <name> <wp> <lp> <sp> <npairs>
//                                 object i as the load leaves it: the
//                                 location and angle as f32 bits, the
//                                 flags, the name, the three derivations
//                                 and the pair count
//   opair <id> <i> <name> <value> one property pair (stored order — the
//                                 interning order, not file order):
//                                 b <t|f>, i <int>, r <bits>,
//                                 a <q(name)>, u <hex units>
//   odrop <id> <k>                the kth z-range drop
//   okey <id> <ordinal> <name>    the final name-key table, ordinal order
//   ostate <id> <tell> <depth> <eof>  post-walk stream state
//
// Usage: gen_mapobjects_golden <output-file>
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
// the objects-list payload (WorldHeightMap.cpp, load-path transcription)
// ---------------------------------------------------------------------------

// MapReaderWriterInfo.h: the version words.
#define K_OBJECTS_VERSION_1 1 // no dict
#define K_OBJECTS_VERSION_2 2 // includes dict
#define K_OBJECTS_VERSION_3 3 // includes dict

// MapObject.h: the world scale and the z bounds derived from it.
#define MAP_XY_FACTOR (10.0f) // How wide and tall each height map square is in world space.
#define MAP_HEIGHT_SCALE (MAP_XY_FACTOR / 16.0f)

// GameType.h: Coord3D (the float triple).
struct Coord3D {
        Real x = 0, y = 0, z = 0;
};

// GameCommon.cpp: normalizeAngle — verbatim, with one marker: the
// while-loops below never terminate on ±infinite input (infinity minus
// 2π stays infinity — a process hang, not a game error), so the
// transcription refuses those shapes the way the port does. NaN is the
// original's own early-out ("ARGH!!!! Don't assert and then not handle
// it!"). The loops compute in double precision and narrow per
// iteration (`angle` is float; `angle -= 2*PI` promotes, subtracts in
// double and stores back to float) — transcribed exactly, and the
// corpora keep angles within ~1000 rad so both sides' loop counts stay
// sane.
#define PI 3.14159265358979323846
struct AngleNonFinite {
};

static Real normalizeAngle(Real angle)
{
        if (angle != angle)
                return 0; // ARGH!!!! Don't assert and then not handle it! Error bad! Fix error!

        if (std::isinf(angle))
                throw AngleNonFinite(); // (the marker — see the note above)

        while (angle > PI)
                angle -= 2 * PI;

        while (angle <= -PI)
                angle += 2 * PI;

        return angle;
}

// MapObject — the saved members (`m_location`, `m_objectName`,
// `m_angle`, `m_flags`, `m_properties`) as the load leaves them, plus
// the three runtime bits the load derives from the property sheet
// (MO_WAYPOINT/MO_LIGHT/MO_SCORCH). The rest of the runtime half
// (m_color's bright green, m_renderObj, m_shadowObj, the bridge
// towers, the s_nextTreeID counter) is editor state with no map-file
// observable; the null-props default branch of the constructor is
// unreachable on the map load (the reader always passes a dict).
struct MapObjectLoad {
        Coord3D m_location;
        AsciiString m_objectName;
        Real m_angle = 0;
        Int m_flags = 0;
        Dict m_properties;
        Bool m_isWaypoint = false, m_isLight = false, m_isScorch = false;

        MapObjectLoad(const Coord3D &loc, const AsciiString &name, Real angle, Int flags,
                      const Dict *props)
        {
                // (validateName(name, flags) is the identity; m_thingTemplate is
                // the ThingFactory lookup — consumer-side, dropped)
                m_objectName = name;
                m_location = loc;
                m_angle = normalizeAngle(angle); // throws AngleNonFinite on ±inf
                m_flags = flags;
                m_properties = *props;
        }
};

// The walk's user state: the object list (TheMapObjectListPtr, head
// first), the drop counter (the original's DEBUG_LOG is its only
// trace) and the name-key generator.
struct ObState {
        std::vector<MapObjectLoad> m_objects; // the list, head first
        Int m_dropped = 0;                    // the z-range drops
        NameKeyGenerator m_keys;              // TheNameKeyGenerator

        // The well-known keys the load consults (WellKnownKeys.h's
        // StaticNameKey startup interning — lightHeightAboveTerrain at 462,
        // waypointID at 504, scorchType at 682 — seeded before the walk the
        // way static initialization does).
        NameKeyType TheKey_lightHeightAboveTerrain = 0;
        NameKeyType TheKey_waypointID = 0;
        NameKeyType TheKey_scorchType = 0;

        enum ErrKind { ERR_NONE = 0, ERR_CORRUPT, ERR_DICT, ERR_ANGLE } errKind = ERR_NONE;
        Int errDictType = 0;

        void seedWellKnownKeys()
        {
                TheKey_lightHeightAboveTerrain = m_keys.nameToKey("lightHeightAboveTerrain");
                TheKey_waypointID = m_keys.nameToKey("waypointID");
                TheKey_scorchType = m_keys.nameToKey("scorchType");
        }
};

static Bool parseObjectDataChunk(DataChunkInput &file, DataChunkInfo *info, void *userData);
static Bool parseObjectData(DataChunkInput &file, DataChunkInfo *info, ObState *st, Bool readDict);

// WorldHeightMap::ParseObjectsDataChunk — the ObjectsList container
// reader: reset the link cursor (m_currentObject — the list head is NOT
// reset; a later chunk's first kept object overwrites it), register the
// Object parser in this chunk's scope, and walk the children.
static Bool parseObjectsDataChunk(DataChunkInput &file, DataChunkInfo *info, void *userData)
{
        file.m_currentObject = nullptr; // just in case.
        // (the original registers with info->label, which is "ObjectsList"
        // by the match itself)
        file.registerParser("Object", info->label, parseObjectDataChunk);
        return (file.parse(userData));
}

// WorldHeightMap::ParseObjectDataChunk — the Object reader's dispatch.
// The original reads its WorldHeightMap* through file.m_userData, which
// nothing on the load path ever sets (the constructor leaves it null)
// and which ParseObjectData never touches — the null-this call works
// by accident on the game's compiler. The transcription passes the
// state directly.
static Bool parseObjectDataChunk(DataChunkInput &file, DataChunkInfo *info, void *userData)
{
        ObState *st = (ObState *)userData;
        return parseObjectData(file, info, st, info->version >= K_OBJECTS_VERSION_2);
}

// WorldHeightMap::ParseObjectData — the Object chunk reader: the
// location triple, the version-gated z zeroing, the angle, the flags,
// the name, the version-gated property dict, the z-range drop check,
// the object's construction (normalizeAngle inside), and the three
// well-known derivations.
static Bool parseObjectData(DataChunkInput &file, DataChunkInfo *info, ObState *st, Bool readDict)
{
        Bool hasPrevious = (file.m_currentObject != nullptr);

        Coord3D loc;
        loc.x = file.readReal();
        loc.y = file.readReal();
        loc.z = file.readReal();

        Real minZ = -100 * MAP_XY_FACTOR;
        Real maxZ = (255 * 10) * MAP_HEIGHT_SCALE;

        if (info->version <= K_OBJECTS_VERSION_2) {
                loc.z = 0;
        }

        Real angle = file.readReal();
        Int flags = file.readInt();
        AsciiString name = file.readAsciiString();
        Dict d;
        if (readDict) {
                try {
                        d = file.readDict(st->m_keys);
                } catch (DictTypeCorrupt &e) {
                        st->errKind = ObState::ERR_DICT;
                        st->errDictType = (Int)e.type;
                        return false; // (the escaping ERROR_CORRUPT_FILE_FORMAT's stop)
                }
        }

        if (loc.z < minZ || loc.z > maxZ) {
                // DEBUG_LOG(("Removing object at z height %f", loc.z)); — the
                // object is dropped: no construction, no linkage, the walk runs on.
                st->m_dropped++;
                return true;
        }

        // create the map object (newInstance(MapObject)(...) — the
        // constructor's normalizeAngle can refuse an infinite angle)
        std::unique_ptr<MapObjectLoad> obj;
        try {
                obj.reset(new MapObjectLoad(loc, name, angle, flags, &d));
        } catch (AngleNonFinite &) {
                st->errKind = ObState::ERR_ANGLE;
                return false;
        }

        if (obj->m_properties.getType(st->TheKey_waypointID) == Dict::DICT_INT)
                obj->m_isWaypoint = true;
        if (obj->m_properties.getType(st->TheKey_lightHeightAboveTerrain) == Dict::DICT_REAL)
                obj->m_isLight = true;
        if (obj->m_properties.getType(st->TheKey_scorchType) == Dict::DICT_INT)
                obj->m_isScorch = true;

        // the linkage: a previous object chains, else the list head is
        // replaced (the first kept object of a later ObjectsList chunk
        // orphans whatever came before it — TheMapObjectListPtr is only
        // ever overwritten, never cleared). The list model is the vector:
        // append to the previous object, or start the new head.
        if (!hasPrevious)
                st->m_objects.clear(); // the orphaned head (unreachable either way)
        st->m_objects.push_back(*obj);
        file.m_currentObject = (void *)1; // non-null (the original stores the object)
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
                line(std::string("ostate ") + id + " " + std::to_string((Int)in.tell()) + " " +
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

// --- object payload builders -------------------------------------------------

// one Object chunk payload: loc, angle, flags, name, and (with_dict) the
// property dict's pair count and pairs.
static std::vector<uint8_t> object_payload(Real x, Real y, Real z, Real angle, Int flags,
                                           const std::string &name,
                                           const std::vector<std::vector<uint8_t>> &pairs,
                                           Bool with_dict)
{
        std::vector<uint8_t> p;
        put_f32(p, x);
        put_f32(p, y);
        put_f32(p, z);
        put_f32(p, angle);
        put_i32(p, flags);
        put_u16(p, (uint16_t)name.size());
        put_str(p, name);
        if (with_dict) {
                put_u16(p, (uint16_t)pairs.size());
                for (const auto &pr : pairs)
                        put_bytes(p, pr);
        }
        return p;
}

// an ObjectsList chunk payload: the Object children (obj_id is the
// corpus's TOC id for "Object").
static std::vector<uint8_t> objects_list_payload(
    uint32_t obj_id, const std::vector<std::pair<uint16_t, std::vector<uint8_t>>> &children)
{
        std::vector<uint8_t> inner;
        for (const auto &c : children)
                put_chunk(inner, obj_id, c.first, c.second);
        return inner;
}

// --- the harness (the constructor's logical walk, objects only) ---------------

// runs one corpus through the logical-load walk and dumps everything.
static void run_ob(Out &out, const char *id, const std::vector<uint8_t> &bytes)
{
        out.file(id, bytes);
        CachedFileInputStream s(bytes);
        DataChunkInput in(&s);
        out.tf("valid", id, in.isValidFileType());
        ObState st;
        st.seedWellKnownKeys();
        in.registerParser("ObjectsList", "", parseObjectsDataChunk);
        Bool ok = in.parse(&st);
        out.line(std::string("walk ") + id + (ok ? " t" : " f"));
        if (!ok) {
                std::string e = std::string("oerr ") + id + " ";
                switch (st.errKind) {
                        case ObState::ERR_DICT:
                                e += "dict " + std::to_string(st.errDictType);
                                break;
                        case ObState::ERR_ANGLE:
                                e += "angle";
                                break;
                        default:
                                // the walk refused before (or without) a payload error: the
                                // header never opened
                                e += "corrupt";
                                break;
                }
                out.line(e);
        } else {
                out.line(std::string("objn ") + id + " " + std::to_string((Int)st.m_objects.size()));
                for (size_t i = 0; i < st.m_objects.size(); ++i) {
                        const MapObjectLoad &o = st.m_objects[i];
                        char loc[80], angle[16];
                        snprintf(loc, sizeof loc, "%08x %08x %08x", f32bits(o.m_location.x),
                                 f32bits(o.m_location.y), f32bits(o.m_location.z));
                        snprintf(angle, sizeof angle, "%08x", f32bits(o.m_angle));
                        out.line(std::string("obj ") + id + " " + std::to_string(i) + " " + loc + " " + angle +
                                 " " + std::to_string(o.m_flags) + " " + q(o.m_objectName.s) + " " +
                                 (o.m_isWaypoint ? "t" : "f") + " " + (o.m_isLight ? "t" : "f") + " " +
                                 (o.m_isScorch ? "t" : "f") + " " +
                                 std::to_string(o.m_properties.getPairCount()));
                        for (Int pidx = 0; pidx < o.m_properties.getPairCount(); ++pidx) {
                                const Dict::DictPair &pair = o.m_properties.m_pairs[pidx];
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
                                out.line(std::string("opair ") + id + " " + std::to_string(i) + " " + q(name) +
                                         " " + v);
                        }
                }
        }
        // the drops (the original's DEBUG_LOG is their only trace)
        for (Int k = 0; k < st.m_dropped; ++k)
                out.line(std::string("odrop ") + id + " " + std::to_string(k));
        // the final name-key table (ordinal order) — pins the interning
        // sequence: the three seeded well-known keys, then the file's names
        {
                const std::vector<std::pair<AsciiString, NameKeyType>> &names = st.m_keys.names();
                for (size_t i = 0; i < names.size(); ++i)
                        out.line(std::string("okey ") + id + " " + std::to_string(names[i].second) + " " +
                                 q(names[i].first.s));
        }
        out.state(id, in);
}

// --- the corpora -------------------------------------------------------------
// the standard corpus TOC: ObjectsList 1, Object 2, and the well-known
// property names at 3.. plus the file's own names.
static void corpus_ob_v3(Out &out)
{
        const char *id = "ob_v3";
        std::vector<uint8_t> b = toc_bytes({{ "ObjectsList", 1 },
                                            { "Object", 2 },
                                            { "objectEnabled", 3 },
                                            { "objectInitialHealth", 4 },
                                            { "waypointID", 5 },
                                            { "lightHeightAboveTerrain", 6 },
                                            { "scorchType", 7 },
                                            { "objectSelectable", 8 }});
        std::vector<std::pair<uint16_t, std::vector<uint8_t>>> children;
        { // a plain building
                std::vector<std::vector<uint8_t>> pairs;
                pairs.push_back(pair_bool(3, true));  // objectEnabled
                pairs.push_back(pair_int(4, 100));    // objectInitialHealth
                children.push_back({ K_OBJECTS_VERSION_3,
                                     object_payload(100.0f, 200.0f, 10.0f, 0.0f, 0x00000101,
                                                     "AmericaVehicleProductionPlant", pairs, true) });
        }
        { // a waypoint: waypointID INT
                std::vector<std::vector<uint8_t>> pairs;
                pairs.push_back(pair_int(5, 7)); // waypointID
                children.push_back({ K_OBJECTS_VERSION_3,
                                     object_payload(250.0f, 260.0f, 0.0f, 3.14159265f, 0,
                                                     "Waypoint", pairs, true) });
        }
        { // a light: lightHeightAboveTerrain REAL
                std::vector<std::vector<uint8_t>> pairs;
                pairs.push_back(pair_real(6, 12.5f)); // lightHeightAboveTerrain
                children.push_back({ K_OBJECTS_VERSION_3,
                                     object_payload(-50.0f, 60.0f, 25.0f, -1.5f, 0x00000001,
                                                     "CruiseMissile", pairs, true) });
        }
        { // a scorch: scorchType INT
                std::vector<std::vector<uint8_t>> pairs;
                pairs.push_back(pair_int(7, 2)); // scorchType
                pairs.push_back(pair_bool(8, false)); // objectSelectable
                children.push_back({ K_OBJECTS_VERSION_3,
                                     object_payload(0.0f, 0.0f, 0.0f, 100.0f, 0x00000040,
                                                     "Scorch1", pairs, true) });
        }
        put_chunk(b, 1, K_OBJECTS_VERSION_3, objects_list_payload(2, children));
        run_ob(out, id, b);
}

// the derivation types are exact: wrong-typed well-known keys derive
// nothing.
static void corpus_ob_v3_wrongtype(Out &out)
{
        const char *id = "ob_v3_wrongtype";
        std::vector<uint8_t> b = toc_bytes({{ "ObjectsList", 1 },
                                            { "Object", 2 },
                                            { "waypointID", 3 },
                                            { "lightHeightAboveTerrain", 4 },
                                            { "scorchType", 5 }});
        std::vector<std::pair<uint16_t, std::vector<uint8_t>>> children;
        {
                std::vector<std::vector<uint8_t>> pairs;
                pairs.push_back(pair_real(3, 9.0f));  // waypointID as REAL
                pairs.push_back(pair_int(4, 20));     // lightHeightAboveTerrain as INT
                pairs.push_back(pair_bool(5, true));  // scorchType as BOOL
                children.push_back({ K_OBJECTS_VERSION_3,
                                     object_payload(1.0f, 2.0f, 3.0f, 0.0f, 0, "WrongTyped", pairs, true) });
        }
        put_chunk(b, 1, K_OBJECTS_VERSION_3, objects_list_payload(2, children));
        run_ob(out, id, b);
}

// version 2: the dict is read, the z is still zeroed (the <= gate).
static void corpus_ob_v2(Out &out)
{
        const char *id = "ob_v2";
        std::vector<uint8_t> b = toc_bytes({{ "ObjectsList", 1 },
                                            { "Object", 2 },
                                            { "objectEnabled", 3 }});
        std::vector<std::pair<uint16_t, std::vector<uint8_t>>> children;
        {
                std::vector<std::vector<uint8_t>> pairs;
                pairs.push_back(pair_bool(3, true));
                children.push_back({ K_OBJECTS_VERSION_2,
                                     object_payload(10.0f, 20.0f, 500.0f, 0.5f, 0, "NukeSilo", pairs, true) });
        }
        put_chunk(b, 1, K_OBJECTS_VERSION_2, objects_list_payload(2, children));
        run_ob(out, id, b);
}

// version 1: no dict (the empty property sheet), the z zeroed.
static void corpus_ob_v1(Out &out)
{
        const char *id = "ob_v1";
        std::vector<uint8_t> b = toc_bytes({{ "ObjectsList", 1 }, { "Object", 2 }});
        std::vector<std::pair<uint16_t, std::vector<uint8_t>>> children;
        children.push_back({ K_OBJECTS_VERSION_1,
                             object_payload(30.0f, 40.0f, 500.0f, 1.0f, 0x00000002, "TreeOak", {}, false) });
        put_chunk(b, 1, K_OBJECTS_VERSION_1, objects_list_payload(2, children));
        run_ob(out, id, b);
}

// version 0: reads like a version 1 chunk.
static void corpus_ob_v0(Out &out)
{
        const char *id = "ob_v0";
        std::vector<uint8_t> b = toc_bytes({{ "ObjectsList", 1 }, { "Object", 2 }});
        std::vector<std::pair<uint16_t, std::vector<uint8_t>>> children;
        children.push_back({ 0, object_payload(1.0f, 2.0f, 3.0f, 0.0f, 0, "OldTree", {}, false) });
        put_chunk(b, 1, 0, objects_list_payload(2, children));
        run_ob(out, id, b);
}

// a future version 4: reads like a version 3 chunk (the dict, the kept z).
static void corpus_ob_v4(Out &out)
{
        const char *id = "ob_v4";
        std::vector<uint8_t> b = toc_bytes({{ "ObjectsList", 1 },
                                            { "Object", 2 },
                                            { "objectEnabled", 3 }});
        std::vector<std::pair<uint16_t, std::vector<uint8_t>>> children;
        {
                std::vector<std::vector<uint8_t>> pairs;
                pairs.push_back(pair_bool(3, false));
                children.push_back({ 4, object_payload(5.0f, 6.0f, 7.5f, 0.25f, 0, "FutureObj", pairs, true) });
        }
        put_chunk(b, 1, 4, objects_list_payload(2, children));
        run_ob(out, id, b);
}

// the ObjectsList chunk's own version word is not the reader's gate: an
// Object child with a version-3 word reads its dict inside a version-1
// list.
static void corpus_ob_v_mismatch(Out &out)
{
        const char *id = "ob_v_mismatch";
        std::vector<uint8_t> b = toc_bytes({{ "ObjectsList", 1 },
                                            { "Object", 2 },
                                            { "objectEnabled", 3 }});
        std::vector<std::pair<uint16_t, std::vector<uint8_t>>> children;
        {
                std::vector<std::vector<uint8_t>> pairs;
                pairs.push_back(pair_bool(3, true));
                children.push_back({ K_OBJECTS_VERSION_3,
                                     object_payload(8.0f, 9.0f, 10.0f, 0.0f, 0, "Mismatched", pairs, true) });
        }
        put_chunk(b, 1, K_OBJECTS_VERSION_1, objects_list_payload(2, children));
        run_ob(out, id, b);
}

// the z-range contract: the floor -1000 and the ceiling 1593.75 are
// kept, one unit past each is dropped, and the drops never break the
// chain (the object after them links to the last kept one).
static void corpus_ob_zdrop(Out &out)
{
        const char *id = "ob_zdrop";
        std::vector<uint8_t> b = toc_bytes({{ "ObjectsList", 1 }, { "Object", 2 }});
        std::vector<std::pair<uint16_t, std::vector<uint8_t>>> children;
        children.push_back({ K_OBJECTS_VERSION_3,
                             object_payload(0.0f, 0.0f, 2000.0f, 0.0f, 0, "TooHigh", {}, true) });
        children.push_back({ K_OBJECTS_VERSION_3,
                             object_payload(0.0f, 0.0f, -2000.0f, 0.0f, 0, "TooLow", {}, true) });
        children.push_back({ K_OBJECTS_VERSION_3,
                             object_payload(0.0f, 0.0f, -1000.0f, 0.0f, 0, "AtFloor", {}, true) });
        children.push_back({ K_OBJECTS_VERSION_3,
                             object_payload(0.0f, 0.0f, 1593.75f, 0.0f, 0, "AtCeiling", {}, true) });
        children.push_back({ K_OBJECTS_VERSION_3,
                             object_payload(1.0f, 1.0f, 1.0f, 0.0f, 0, "Survivor", {}, true) });
        put_chunk(b, 1, K_OBJECTS_VERSION_3, objects_list_payload(2, children));
        run_ob(out, id, b);
}

// the angle wrap: 0, ±(float)π (the float value is above the double π
// and wraps negative; its negation is at or below -π and wraps
// positive), 100 and -100 (many wraps), 7.5, and NaN (the ARGH early
// out — 0).
static void corpus_ob_angle(Out &out)
{
        const char *id = "ob_angle";
        std::vector<uint8_t> b = toc_bytes({{ "ObjectsList", 1 }, { "Object", 2 }});
        std::vector<std::pair<uint16_t, std::vector<uint8_t>>> children;
        const Real angles[] = { 0.0f, 3.14159265f, -3.14159265f, 100.0f, -100.0f, 7.5f };
        for (Real a : angles) {
                children.push_back({ K_OBJECTS_VERSION_3,
                                     object_payload(0.0f, 0.0f, 0.0f, a, 0, "Angled", {}, true) });
        }
        // the NaN angle: raw bits patched into the last child's angle
        // word (offset 12: after x, y, z)
        children.push_back({ K_OBJECTS_VERSION_3,
                             object_payload(0.0f, 0.0f, 0.0f, 0.0f, 0, "NanAngle", {}, true) });
        {
                std::vector<uint8_t> &last = children.back().second;
                last[12] = 0x00;
                last[13] = 0x00;
                last[14] = 0xc0;
                last[15] = 0x7f;
        }
        put_chunk(b, 1, K_OBJECTS_VERSION_3, objects_list_payload(2, children));
        run_ob(out, id, b);
}

// an infinite angle: the original's loops never terminate; the refusal
// stops the walk with the chunk open.
static void corpus_ob_angle_inf(Out &out)
{
        const char *id = "ob_angle_inf";
        std::vector<uint8_t> b = toc_bytes({{ "ObjectsList", 1 }, { "Object", 2 }});
        std::vector<std::pair<uint16_t, std::vector<uint8_t>>> children;
        children.push_back({ K_OBJECTS_VERSION_3,
                             object_payload(0.0f, 0.0f, 0.0f, 0.0f, 0, "InfAngle", {}, true) });
        children.push_back({ K_OBJECTS_VERSION_3,
                             object_payload(1.0f, 1.0f, 1.0f, 0.0f, 0, "NeverReached", {}, true) });
        // the first child's angle word (offset 12) is +inf
        {
                std::vector<uint8_t> &first = children.front().second;
                first[12] = 0x00;
                first[13] = 0x00;
                first[14] = 0x80;
                first[15] = 0x7f;
        }
        put_chunk(b, 1, K_OBJECTS_VERSION_3, objects_list_payload(2, children));
        run_ob(out, id, b);
}

// the name shapes: empty, an embedded NUL (the visible part is before
// it), high bytes, and a long name.
static void corpus_ob_names(Out &out)
{
        const char *id = "ob_names";
        std::vector<uint8_t> b = toc_bytes({{ "ObjectsList", 1 }, { "Object", 2 }});
        std::vector<std::pair<uint16_t, std::vector<uint8_t>>> children;
        children.push_back(
            { K_OBJECTS_VERSION_3, object_payload(0.0f, 0.0f, 0.0f, 0.0f, 0, "", {}, true) });
        children.push_back({ K_OBJECTS_VERSION_3,
                             object_payload(1.0f, 0.0f, 0.0f, 0.0f, 0, "Na\x00me", {}, true) });
        children.push_back({ K_OBJECTS_VERSION_3,
                             object_payload(2.0f, 0.0f, 0.0f, 0.0f, 0, "\xc0\xff\x01", {}, true) });
        children.push_back({ K_OBJECTS_VERSION_3,
                             object_payload(3.0f, 0.0f, 0.0f, 0.0f, 0,
                                            "AVeryLongObjectNameThatGoesOnAndOnAndOnForQuiteAWhileIndeed",
                                            {}, true) });
        put_chunk(b, 1, K_OBJECTS_VERSION_3, objects_list_payload(2, children));
        run_ob(out, id, b);
}

// all five pair types in one dict, plus a duplicate-key update (the
// second write wins, one pair survives) and a real with interesting
// bits.
static void corpus_ob_dict_types(Out &out)
{
        const char *id = "ob_dict_types";
        std::vector<uint8_t> b = toc_bytes({{ "ObjectsList", 1 },
                                            { "Object", 2 },
                                            { "objectEnabled", 3 },
                                            { "objectInitialHealth", 4 },
                                            { "objectTargetable", 5 },
                                            { "originalOwner", 6 },
                                            { "waypointName", 7 }});
        std::vector<std::pair<uint16_t, std::vector<uint8_t>>> children;
        {
                std::vector<std::vector<uint8_t>> pairs;
                pairs.push_back(pair_bool(3, true));
                pairs.push_back(pair_int(4, -559038737));  // 0xdeadbeef as int
                pairs.push_back(pair_real_bits(5, 0x7fc00000)); // a quiet NaN real
                pairs.push_back(pair_ascii(6, "America"));
                pairs.push_back(pair_unicode(7, { 0x0048, 0x0069, 0xd800, 0x0041 })); // a lone surrogate
                pairs.push_back(pair_int(4, 200)); // the duplicate: updates in place
                children.push_back({ K_OBJECTS_VERSION_3,
                                     object_payload(0.0f, 0.0f, 0.0f, 0.0f, 0, "AllTypes", pairs, true) });
        }
        put_chunk(b, 1, K_OBJECTS_VERSION_3, objects_list_payload(2, children));
        run_ob(out, id, b);
}

// a corrupt dict pair type: the reader throws, the walk stops with the
// chunk open, the type byte is pinned.
static void corpus_ob_dict_corrupt(Out &out)
{
        const char *id = "ob_dict_corrupt";
        std::vector<uint8_t> b = toc_bytes({{ "ObjectsList", 1 },
                                            { "Object", 2 },
                                            { "objectEnabled", 3 }});
        std::vector<std::pair<uint16_t, std::vector<uint8_t>>> children;
        {
                std::vector<std::vector<uint8_t>> pairs;
                pairs.push_back(pair_bool(3, true));
                pairs.push_back(pair_raw_type(3, 7)); // an impossible type byte
                children.push_back({ K_OBJECTS_VERSION_3,
                                     object_payload(0.0f, 0.0f, 0.0f, 0.0f, 0, "Corrupt", pairs, true) });
        }
        put_chunk(b, 1, K_OBJECTS_VERSION_3, objects_list_payload(2, children));
        run_ob(out, id, b);
}

// two ObjectsList chunks: the second's first kept object overwrites the
// head — the first list is orphaned, the final list is the second's.
static void corpus_ob_multi(Out &out)
{
        const char *id = "ob_multi";
        std::vector<uint8_t> b = toc_bytes({{ "ObjectsList", 1 }, { "Object", 2 }});
        {
                std::vector<std::pair<uint16_t, std::vector<uint8_t>>> children;
                children.push_back({ K_OBJECTS_VERSION_3,
                                     object_payload(0.0f, 0.0f, 0.0f, 0.0f, 0, "FirstA", {}, true) });
                children.push_back({ K_OBJECTS_VERSION_3,
                                     object_payload(1.0f, 0.0f, 0.0f, 0.0f, 0, "FirstB", {}, true) });
                put_chunk(b, 1, K_OBJECTS_VERSION_3, objects_list_payload(2, children));
        }
        {
                std::vector<std::pair<uint16_t, std::vector<uint8_t>>> children;
                children.push_back({ K_OBJECTS_VERSION_3,
                                     object_payload(2.0f, 0.0f, 0.0f, 0.0f, 0, "SecondC", {}, true) });
                put_chunk(b, 1, K_OBJECTS_VERSION_3, objects_list_payload(2, children));
        }
        run_ob(out, id, b);
}

// a later list whose objects all drop: no kept object, no head write —
// the earlier list survives.
static void corpus_ob_multi_drop(Out &out)
{
        const char *id = "ob_multi_drop";
        std::vector<uint8_t> b = toc_bytes({{ "ObjectsList", 1 }, { "Object", 2 }});
        {
                std::vector<std::pair<uint16_t, std::vector<uint8_t>>> children;
                children.push_back({ K_OBJECTS_VERSION_3,
                                     object_payload(0.0f, 0.0f, 1.0f, 0.0f, 0, "Kept", {}, true) });
                put_chunk(b, 1, K_OBJECTS_VERSION_3, objects_list_payload(2, children));
        }
        {
                std::vector<std::pair<uint16_t, std::vector<uint8_t>>> children;
                children.push_back({ K_OBJECTS_VERSION_3,
                                     object_payload(0.0f, 0.0f, 3000.0f, 0.0f, 0, "Dropped", {}, true) });
                put_chunk(b, 1, K_OBJECTS_VERSION_3, objects_list_payload(2, children));
        }
        run_ob(out, id, b);
}

// a later empty list: the head is only ever overwritten by a kept
// object, so the earlier list survives an empty chunk.
static void corpus_ob_multi_empty(Out &out)
{
        const char *id = "ob_multi_empty";
        std::vector<uint8_t> b = toc_bytes({{ "ObjectsList", 1 }, { "Object", 2 }});
        {
                std::vector<std::pair<uint16_t, std::vector<uint8_t>>> children;
                children.push_back({ K_OBJECTS_VERSION_3,
                                     object_payload(0.0f, 0.0f, 1.0f, 0.0f, 0, "Kept", {}, true) });
                put_chunk(b, 1, K_OBJECTS_VERSION_3, objects_list_payload(2, children));
        }
        {
                std::vector<std::pair<uint16_t, std::vector<uint8_t>>> children;
                put_chunk(b, 1, K_OBJECTS_VERSION_3, objects_list_payload(2, children));
        }
        run_ob(out, id, b);
}

// the scope rules: an Object inside a non-ObjectsList container never
// fires (the container skips it whole), an ObjectsList inside a
// container never fires (the global-scope rule), and the top-level list
// after them loads.
static void corpus_ob_nested(Out &out)
{
        const char *id = "ob_nested";
        std::vector<uint8_t> b = toc_bytes({{ "WorldInfo", 1 },
                                            { "ObjectsList", 2 },
                                            { "Object", 3 }});
        {
                // an Object child inside a WorldInfo container: no parser at
                // ("Object", "WorldInfo") — skipped whole
                std::vector<uint8_t> child;
                put_f32(child, 1.0f);
                std::vector<uint8_t> inner;
                put_chunk(inner, 3, K_OBJECTS_VERSION_3,
                          object_payload(0.0f, 0.0f, 0.0f, 0.0f, 0, "InWorldInfo", {}, true));
                put_chunk(b, 1, 1, inner);
        }
        {
                // an ObjectsList inside a WorldInfo container: never fires
                std::vector<uint8_t> inner;
                std::vector<std::pair<uint16_t, std::vector<uint8_t>>> children;
                children.push_back({ K_OBJECTS_VERSION_3,
                                     object_payload(1.0f, 0.0f, 0.0f, 0.0f, 0, "NeverFires", {}, true) });
                put_chunk(inner, 2, K_OBJECTS_VERSION_3, objects_list_payload(3, children));
                put_chunk(b, 1, 1, inner);
        }
        {
                std::vector<std::pair<uint16_t, std::vector<uint8_t>>> children;
                children.push_back({ K_OBJECTS_VERSION_3,
                                     object_payload(2.0f, 0.0f, 0.0f, 0.0f, 0, "TopLevel", {}, true) });
                put_chunk(b, 2, K_OBJECTS_VERSION_3, objects_list_payload(3, children));
        }
        run_ob(out, id, b);
}

// a bare Object at the global scope: no parser at ("Object", "") —
// skipped whole, the list stays empty (the null head).
static void corpus_ob_toplevel_object(Out &out)
{
        const char *id = "ob_toplevel_object";
        std::vector<uint8_t> b = toc_bytes({{ "ObjectsList", 1 }, { "Object", 2 }});
        put_chunk(b, 2, K_OBJECTS_VERSION_3,
                  object_payload(0.0f, 0.0f, 0.0f, 0.0f, 0, "Bare", {}, true));
        run_ob(out, id, b);
}

// leftovers: bytes after an object's dict (skipped by the child's
// close) and bytes after the last child inside the list (skipped by the
// list's close).
static void corpus_ob_leftover(Out &out)
{
        const char *id = "ob_leftover";
        std::vector<uint8_t> b = toc_bytes({{ "ObjectsList", 1 }, { "Object", 2 }});
        std::vector<uint8_t> inner;
        {
                std::vector<uint8_t> obj = object_payload(0.0f, 0.0f, 0.0f, 0.0f, 0, "Left", {}, true);
                obj.push_back(0xee);
                obj.push_back(0xee);
                put_chunk(inner, 2, K_OBJECTS_VERSION_3, obj);
        }
        inner.push_back(0xdd);
        inner.push_back(0xdd);
        put_chunk(b, 1, K_OBJECTS_VERSION_3, inner);
        run_ob(out, id, b);
}

// the under-declared Object: the size word says 20 but the payload is
// longer — the reads run past the declared end into the real bytes (the
// signed data-left goes negative, the close does not seek), and the
// next object parses from the correct position.
static void corpus_ob_underdeclared(Out &out)
{
        const char *id = "ob_underdeclared";
        std::vector<uint8_t> b = toc_bytes({{ "ObjectsList", 1 }, { "Object", 2 }});
        std::vector<uint8_t> inner;
        {
                std::vector<uint8_t> obj = object_payload(0.0f, 0.0f, 0.0f, 0.5f, 0x11, "Under", {}, true);
                put_chunk(inner, 2, K_OBJECTS_VERSION_3, obj, 20);
        }
        {
                std::vector<uint8_t> obj = object_payload(1.0f, 1.0f, 1.0f, 1.5f, 0x22, "After", {}, true);
                put_chunk(inner, 2, K_OBJECTS_VERSION_3, obj);
        }
        put_chunk(b, 1, K_OBJECTS_VERSION_3, inner);
        run_ob(out, id, b);
}

// a short file: the last object's name length reads the next word's
// zero-fill and the dict count zeroes — the empty-name BOOL pairs the
// zero-filled keyAndType words mint.
static void corpus_ob_short(Out &out)
{
        const char *id = "ob_short";
        std::vector<uint8_t> b = toc_bytes({{ "ObjectsList", 1 },
                                            { "Object", 2 },
                                            { "objectEnabled", 3 }});
        std::vector<uint8_t> inner;
        {
                // the full payload: name + a 2-pair dict — then cut the file
                // after the name's first bytes
                std::vector<std::vector<uint8_t>> pairs;
                pairs.push_back(pair_bool(3, true));
                pairs.push_back(pair_bool(3, false));
                std::vector<uint8_t> obj =
                        object_payload(0.0f, 0.0f, 0.0f, 0.0f, 0, "Sho", pairs, true);
                obj.resize(obj.size() - 10); // cut the pairs, keep the count word
                put_chunk(inner, 2, K_OBJECTS_VERSION_3, obj);
        }
        put_chunk(b, 1, K_OBJECTS_VERSION_3, inner);
        run_ob(out, id, b);
}

// a map with no ObjectsList at all: the walk succeeds, the list is
// empty (the constructor's freeListOfMapObjects leaves the head null).
static void corpus_ob_none(Out &out)
{
        const char *id = "ob_none";
        std::vector<uint8_t> b = toc_bytes({{ "HeightMapData", 1 }});
        {
                std::vector<uint8_t> d;
                put_i32(d, 2);
                put_i32(d, 2);
                put_i32(d, 4);
                put_bytes(d, std::vector<uint8_t>(4, 0x10));
                put_chunk(b, 1, 3, d);
        }
        run_ob(out, id, b);
}

// not a chunky file at all.
static void corpus_ob_not_chunky(Out &out)
{
        const char *id = "ob_not_chunky";
        std::vector<uint8_t> b;
        put_str(b, "not a map at all");
        run_ob(out, id, b);
}

// a CkMp table of contents that ends at EOF never opens (the container's
// refusal — the walk fails before any parser).
static void corpus_ob_toc_only(Out &out)
{
        const char *id = "ob_toc_only";
        std::vector<uint8_t> b = toc_bytes({{ "ObjectsList", 1 }});
        run_ob(out, id, b);
}

// empty file.
static void corpus_ob_empty(Out &out)
{
        const char *id = "ob_empty";
        std::vector<uint8_t> b;
        run_ob(out, id, b);
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
        fprintf(f, "# gen_mapobjects_golden.cpp — WorldHeightMap.cpp ParseObjectsDataChunk/"
                   "ParseObjectData transcription (baseline 571a7c5).\n");
        fprintf(f, "# Golden script for rzh-formats::map_objects; see "
                   "tools/differential/README.md.\n");
        fprintf(f, "# Grammar: file/valid/walk/oerr/objn/obj/opair/odrop/okey/ostate.\n");

        corpus_ob_v3(out);
        corpus_ob_v3_wrongtype(out);
        corpus_ob_v2(out);
        corpus_ob_v1(out);
        corpus_ob_v0(out);
        corpus_ob_v4(out);
        corpus_ob_v_mismatch(out);
        corpus_ob_zdrop(out);
        corpus_ob_angle(out);
        corpus_ob_angle_inf(out);
        corpus_ob_names(out);
        corpus_ob_dict_types(out);
        corpus_ob_dict_corrupt(out);
        corpus_ob_multi(out);
        corpus_ob_multi_drop(out);
        corpus_ob_multi_empty(out);
        corpus_ob_nested(out);
        corpus_ob_toplevel_object(out);
        corpus_ob_leftover(out);
        corpus_ob_underdeclared(out);
        corpus_ob_short(out);
        corpus_ob_none(out);
        corpus_ob_not_chunky(out);
        corpus_ob_toc_only(out);
        corpus_ob_empty(out);

        fclose(f);
        return 0;
}
