// SPDX-License-Identifier: GPL-3.0-or-later
//
// Golden-vector driver for `rzh-formats::map_blendtile` (parity level P0).
//
// Verbatim transcription of the Zero Hour blend-tile payload's load path —
// Core/GameEngineDevice/Source/W3DDevice/GameClient/WorldHeightMap.cpp at
// the baseline (571a7c5) — over the re-transcribed map data-chunk
// container (Common/System/DataChunk.{h,cpp}, the reader half verbatim
// from gen_mapchunk_golden.cpp):
//
//   - WorldHeightMap::ParseBlendTileData, the BlendTileData chunk reader
//     the constructor's FULL load registers (the largest map payload:
//     the four per-cell index arrays, the flip/cliff state bitmaps, the
//     texture classes with their legacy GDF word, the edge texture
//     classes, the blend records with their FLAG_VAL sentinels, the cliff
//     u/v tables, the version gates — extra blends from 6, cliff indices
//     and infos from 5, long diagonals from 3, custom edge classes from
//     4, the version-7 cliff-state remap and the version-1 downsample —
//     and the use3WayTerrainBlends gates: the extra-array flush and the
//     FLIPPED_MASK filter);
//   - WorldHeightMap::initCliffFlagsFromHeights /
//     setCellCliffFlagFromHeights / setCliffState, the pre-version-7
//     cliff-bit derivation from the height samples
//     (MAP_HEIGHT_SCALE 10/16, PATHFIND_CLIFF_SLOPE_LIMIT_F 9.8);
//   - the constructor's registerParser("HeightMapData"|"BlendTileData",
//     "", ...)/parse walk shape the harness drives (global scope,
//     last-wins repeated chunks, skipped unknown labels) — the FULL mode,
//     the only one that registers the blend reader — plus the
//     constructor's post-walk "patch bad maps" loop that clamps the three
//     blend index arrays into range;
//   - the HeightMapData half of the walk is the full-load reader from
//     gen_mapheightmap_golden.cpp (ParseHeightMapData verbatim), because
//     the blend reader reads the shared m_width/m_height/m_dataSize/
//     m_data members it leaves.
//
// Deliberate deviations, pinned identically on both sides:
//
// - The two `throw ERROR_CORRUPT_FILE_FORMAT` sites (the len word != the
//   height map's m_dataSize; a blend record's flag != FLAG_VAL) throw
//   catchable markers the harness turns into `herr` lines; the walk-stop
//   the escaping exception produces is transcribed as the parser
//   returning false (the chunk stays open in both — the bstate lines pin
//   it).
// - The four fixed member arrays the original indexes with file-supplied
//   counts (m_blendedTiles[16192], m_cliffInfo[32384],
//   m_textureClasses[256], m_edgeTextureClasses[256]) are vectors sized
//   to the counts. A count past the array bound is an in-object overflow
//   in the original (silent corruption of the members that follow, a
//   crash for large counts — the DEBUG_ASSERTCRASHs that nominally bound
//   them are debug-only); the transcription refuses those counts with a
//   marker (herr lines nblend/ncliff/ntex/netex), the same decision the
//   W3D modules made for the allocation-fatal shapes. The count words'
//   own arrays in the dump are the loaded entries only.
// - `new Short[m_dataSize]` / `new UnsignedByte[...]` are zero-born
//   vectors. The original's heap is uninitialised: the shapes where the
//   difference is observable are the cliff/extra index arrays of
//   pre-version-5/6 chunks (never read from the file, then read by the
//   patch loop as garbage) and the version-7 cliff-state copy's short
//   read tail — both sides zero-fill, the container's pinned short-read
//   stance, so the dump is deterministic where the original's is not.
// - readTexClass — the TGA tile load through TheFileSystem and
//   TheTerrainTypes — reads nothing from the chunk stream (it opens
//   separate texture files) and is dropped: the corpus environment has
//   no terrain INI and no texture files, so the original's readTexClass
//   is a no-op here too. The parsed class fields the texture manager
//   drives (firstTile, numTiles, width, name) are the observable.
// - The constructor's post-walk patch loop dereferences the null index
//   arrays when a height map loaded but no BlendTileData did (m_dataSize
//   entries through nullptr — a process death); the transcription
//   refuses the load (the `patchnull` herr line). When the arrays are
//   SHORTER than the live m_dataSize (a BlendTileData that loaded before
//   any HeightMapData, so len == 0), the original overreads its 0-length
//   allocations — the transcription (and the port) skip the missing
//   tail, the deterministic stance.
// - The container transcription (the reader half) is verbatim from
//   gen_mapchunk_golden.cpp — see that file's header for its own
//   deviation notes (the RefPack branch, the 16-bit WideChar pin, the
//   zero-filled short reads).
//
// Output grammar (one command per line, hex lowercase, '-' = empty):
//
//   file <id> <hex>               the corpus bytes
//   valid <id> <t|f>              isValidFileType() after construction
//   walk <id> <3|n> <t|f>         the parse() verdict per mode
//                                 (3 = use3WayTerrainBlends on, the
//                                  retail default; n = off)
//   hm <id> <m> <w> <h> <b> <ds> <nbnd>   the loaded height map, as the
//                                 FULL walk leaves it (a version-1 blend
//                                 resize may have halved w/h after the
//                                 height reader was done)
//   bnd <id> <m> <i> <x> <y>      boundary i in stored order
//   hdat <id> <m> <hex|->         the sample bytes (m_data, all of it)
//   hnone <id> <m>                the walk succeeded, no HeightMapData
//   bt <id> <m> <len> <nbmp> <nblend> <ncliff> <ntex> <nedgetiles>
//      <nedgetex> <fw> <fh> <fds> <fsw>
//                                 the loaded blend payload: the len word
//                                 (== the arrays' length), the counts as
//                                 the load leaves them (version-1 forces
//                                 nblend/ncliff to 1), the shared extent
//                                 members as the blend parse leaves them
//                                 (version-1 halves them) and the flip
//                                 state width
//   traw/braw/xraw/craw <id> <m> <i16 list|->   the four index arrays as
//                                 the parse leaves them (pre-patch),
//                                 signed decimal
//   flip/cst <id> <m> <hex|->     the flip state bitmap (always zero —
//                                 the parse never writes it) and the
//                                 cliff state bitmap (read from version 8
//                                 directly, remapped at version 7,
//                                 derived from the heights before)
//   tex <id> <m> <i> <first> <num> <width> <name>   texture class i
//   etex <id> <m> <i> <first> <num> <width> <name> edge texture class i
//   bti <id> <m> <i> <blendNdx> <horiz> <vert> <rdiag> <ldiag> <inv>
//      <longdiag> <custom>       blend record i (i starts at 1; record 0
//                                 is the implied transparent tile)
//   cfi <id> <m> <i> <tileIdx> <u0> <v0> <u1> <v1> <u2> <v2> <u3> <v3>
//      <flip> <mutant>           cliff info i (floats as bit patterns,
//                                 tileIdx the i32->i16 truncation)
//   bnone <id> <m>                the walk succeeded, no BlendTileData
//   tndx/bndx/xndx/cndx <id> <m> <i16 list|->   the index arrays AFTER
//                                 the constructor's patch loop
//   herr <id> <m> <kind> <args>   the refused load:
//       blenlen <len> <ds>        the len word != m_dataSize
//       flag <i> <flag>           blend record i's sentinel != FLAG_VAL
//       nblend/ncliff/ntex/netex <n>   a count past its array bound
//       mapsize <w> <h> <b> / datasize <ds> <w> <h> <left> /
//       bordercount <n>           the height-map half's refusals
//       patchnull <ds>            the patch loop's null-array death
//       corrupt                   (the walk itself refused)
//   bstate <id> <m> <tell> <depth> <eof>  post-walk stream state
//
// Usage: gen_mapblendtile_golden <output-file>
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
// the height-map payload (WorldHeightMap.cpp, the FULL reader — the only
// mode that registers the blend reader; transcribed verbatim from
// gen_mapheightmap_golden.cpp)
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

// The game error code the height-map reader throws (ERROR_CORRUPT_FILE_FORMAT
// as a catchable marker so the corpus driver keeps running).
struct CorruptFileFormat {
};

// The process death the original's m_boundaries.resize(negative int)
// reaches inside the standard library (std::length_error, uncaught):
// a catchable marker for the same reason.
struct BorderCountCorrupt {
};

// ---------------------------------------------------------------------------
// the blend-tile payload (WorldHeightMap.cpp, TileData.h,
// WorldHeightMap.h, MapReaderWriterInfo.h)
// ---------------------------------------------------------------------------

// MapReaderWriterInfo.h: the blend-tile version words.
#define K_BLEND_TILE_VERSION_1 1 // Height map cell = 5.0
#define K_BLEND_TILE_VERSION_2 2 // Height map cell = 10.0
#define K_BLEND_TILE_VERSION_3 3 // Added long diagonal blends.
#define K_BLEND_TILE_VERSION_4 4 // Added custom edge blends.
#define K_BLEND_TILE_VERSION_5 5 // Added custom cliff u/v coordinates.
#define K_BLEND_TILE_VERSION_6 6 // Added extra blend layer for 3 textures in cell.
#define K_BLEND_TILE_VERSION_7 7 // Added flag for painting passable/impassable to cells.
#define K_BLEND_TILE_VERSION_8 8 // Added flag for painting passable/impassable to cells. Added in Zero Hour.

// WorldHeightMap.h: the fixed array bounds and the sentinel.
#define NUM_BLEND_TILES 16192
#define NUM_CLIFF_INFO 32384
#define FLAG_VAL 0x7ADA0000
#define NUM_TEXTURE_CLASSES 256

// TileData.h: the blend record and its masks.
#define INVERTED_MASK 0x1 // AND this with inverted to get actual inverted state
#define FLIPPED_MASK 0x2 // AND this with inverted to get forced flip state

// TileData.h: TBlendTileInfo.
struct TBlendTileInfo {
        Int blendNdx = 0;
        Byte horiz = 0;
        Byte vert = 0;
        Byte rightDiagonal = 0;
        Byte leftDiagonal = 0;
        Byte inverted = 0;
        Byte longDiagonal = 0;
        Int customBlendEdgeClass = -1; // Class of texture for a blend edge.  -1 means use alpha.
};

// WorldHeightMap.h: TCliffInfo (u0..v3, flip, mutant, tileIndex).
struct TCliffInfo {
        Real u0 = 0, v0 = 0; // Upper left uv
        Real u1 = 0, v1 = 0; // Lower left uv
        Real u2 = 0, v2 = 0; // Lower right uv
        Real u3 = 0, v3 = 0; // Upper right uv
        Bool flip = false;
        Bool mutant = false; // Mutant mapping needed to get this to fit.
        Short tileIndex = 0; // Tile texture.
};

// WorldHeightMap.h: TXTextureClass (the parsed half; globalTextureClass
// and positionInTexture are runtime texture-manager state the parse only
// initializes, dropped per the header).
struct TXTextureClassRec {
        Int firstTile = 0;
        Int numTiles = 0;
        Int width = 0;
        std::string name;
};

// The blend reader's throw sites as catchable markers (the original
// throws ERROR_CORRUPT_FILE_FORMAT; the refused counts are the in-object
// overflow death shapes — see the file header).
struct ErrLen {
};
struct ErrFlag {
};
struct ErrNBlend {
};
struct ErrNCliff {
};
struct ErrNTex {
};
struct ErrNEdgeTex {
};

// MAP_XY_FACTOR / MAP_HEIGHT_SCALE (Common/MapObject.h) and the cliff
// slope limit (WorldHeightMap.cpp's own define).
#define MAP_HEIGHT_SCALE (10.0f / 16.0f) // divide all map heights by 8... (sic)
#define PATHFIND_CLIFF_SLOPE_LIMIT_F 9.8f

// MAP_XY_FACTOR / MAP_HEIGHT_SCALE (Common/MapObject.h) and the cliff
// slope limit (WorldHeightMap.cpp's own define).
#define MAP_HEIGHT_SCALE (10.0f / 16.0f) // divide all map heights by 8... (sic)
#define PATHFIND_CLIFF_SLOPE_LIMIT_F 9.8f

// The WorldHeightMap members the two readers write, as the load leaves
// them: the height-map half (ParseHeightMapData) and the blend half
// (ParseBlendTileData), one object like the original's `this`.
struct WorldHeightMapLoad {
        // --- the height-map half ---
        Int m_width = 0;       ///< Height map width.
        Int m_height = 0;      ///< Height map height (y size of array).
        Int m_borderSize = 0;  ///< Non-playable border area.
        std::vector<ICoord2D> m_boundaries; ///< the in-game boundaries
        Int m_dataSize = 0;    ///< size of m_data.
        std::vector<uint8_t> m_data; ///< new Byte[m_dataSize] (zero-born here)
        Int m_lastNumBorders = 0; ///< transcription-only probe (error dump)

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

        static Bool isValidHeightDataSize(DataChunkInput &file, Int dataSize, Int width, Int height)
        {
                return dataSize > 0 && dataSize == width * height &&
                       (UnsignedInt)dataSize <= file.getChunkDataSizeLeft();
        }

        // WorldHeightMap::ParseHeightMapData — the FULL load's reader
        // (verbatim from gen_mapheightmap_golden.cpp; the seismic scratch
        // allocation is dropped per that file's header).
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
                }
                // (the FULL load leaves m_width/m_height at the file's extent)
                return true;
        }

        // --- the blend half ---
        std::vector<int16_t> m_tileNdxes;      ///< matches m_Data, indexes into m_SourceTiles.
        std::vector<int16_t> m_blendTileNdxes; ///< matches m_Data, indexes into m_blendedTiles.  0 means no blend info.
        std::vector<int16_t> m_extraBlendTileNdxes; ///< matches m_Data, indexes into m_extraBlendedTiles.
        std::vector<int16_t> m_cliffInfoNdxes; ///< matches m_Data, indexes into m_cliffInfo.  0 means no cliff info.

        Int m_flipStateWidth = 0; ///< width of the array holding cellFlipState
        std::vector<uint8_t> m_cellFlipState;  ///< array of bits to indicate the flip state of each cell.
        std::vector<uint8_t> m_cellCliffState; ///< array of bits to indicate the cliff state of each cell.

        Int m_numBitmapTiles = 0;      ///< Number of tiles initialized from bitmaps.
        Int m_numEdgeTiles = 0;        ///< Number of edge tiles.
        Int m_numBlendedTiles = 1;     ///< Number of blended tiles created from bitmap tiles.
        Int m_numCliffInfo = 1;        ///< Number of cliffInfo's used in m_cliffInfo.
        Int m_numTextureClasses = 0;   ///< Number of texture classes.
        Int m_numEdgeTextureClasses = 0; ///< Number of edge texture classes.

        std::vector<TXTextureClassRec> m_textureClasses;     ///< the original: TXTextureClass[256]
        std::vector<TXTextureClassRec> m_edgeTextureClasses; ///< the original: TXTextureClass[256]
        std::vector<TBlendTileInfo> m_blendedTiles;          ///< the original: TBlendTileInfo[16192]
        std::vector<TCliffInfo> m_cliffInfo;                 ///< the original: TCliffInfo[32384]

        Bool m_blendLoaded = false; ///< whether any BlendTileData fired

        // transcription-only probes (the error dumps read them after the catch)
        Int m_btLen = 0;         ///< the len word as last read
        Int m_lastFlag = 0;      ///< the flag word as last read
        Int m_lastFlagIndex = 0; ///< which blend record's flag
        Int m_lastCount = 0;     ///< the refused count

        // transcription-only probes: the shared extent members AS THE BLEND
        // PARSE LEAVES THEM (a later HeightMapData can overwrite the live
        // ones; the version-1 resize halves these)
        Int m_btWidth = 0;
        Int m_btHeight = 0;
        Int m_btDataSize = 0;

        // WorldHeightMap::getHeight (the bounds-checked sample query the
        // cliff derivation drives).
        Byte getHeight(Int xIndex, Int yIndex)
        {
                Int ndx = (yIndex * m_width) + xIndex;
                if ((ndx >= 0) && (ndx < m_dataSize) && !m_data.empty())
                        return m_data[(size_t)ndx];
                else
                        return 0;
        }

        // WorldHeightMap::setCliffState.
        void setCliffState(Int xIndex, Int yIndex, Bool state)
        {
                if (xIndex < 0 || yIndex < 0)
                        return;
                if (yIndex >= m_height)
                        return;
                if (xIndex >= m_width)
                        return;
                if (m_cellCliffState.empty())
                        return;
                Byte flagByte =
                    m_cellCliffState[(size_t)(yIndex * m_flipStateWidth + (xIndex >> 3))];
                Byte flagMask = (1 << (xIndex & 0x7));
                if (state) {
                        flagByte |= flagMask;
                } else {
                        flagByte &= (~flagMask);
                }
                m_cellCliffState[(size_t)(yIndex * m_flipStateWidth + (xIndex >> 3))] = flagByte;
        }

        /** Sets the cliff flag for a cell based on height. */
        void setCellCliffFlagFromHeights(Int xIndex, Int yIndex)
        {
                Real height1 = getHeight(xIndex, yIndex) * MAP_HEIGHT_SCALE;
                Real height2 = getHeight(xIndex + 1, yIndex) * MAP_HEIGHT_SCALE;
                Real height3 = getHeight(xIndex, yIndex + 1) * MAP_HEIGHT_SCALE;
                Real height4 = getHeight(xIndex + 1, yIndex + 1) * MAP_HEIGHT_SCALE;
                Real minZ = height1;
                if (minZ > height2)
                        minZ = height2;
                if (minZ > height3)
                        minZ = height3;
                if (minZ > height4)
                        minZ = height4;
                Real maxZ = height1;
                if (maxZ < height2)
                        maxZ = height2;
                if (maxZ < height3)
                        maxZ = height3;
                if (maxZ < height4)
                        maxZ = height4;
                const Real cliffRange = PATHFIND_CLIFF_SLOPE_LIMIT_F;
                Bool isCliff = (maxZ - minZ > cliffRange);
                setCliffState(xIndex, yIndex, isCliff);
        }

        /** Sets all the cliff flags in map based on height. */
        void initCliffFlagsFromHeights()
        {
                Int xIndex, yIndex;

                for (xIndex = 0; xIndex < m_width - 1; xIndex++) {
                        for (yIndex = 0; yIndex < m_height - 1; yIndex++) {
                                setCellCliffFlagFromHeights(xIndex, yIndex);
                        }
                }
        }

        // WorldHeightMap::ParseBlendTileData — the blend-tile chunk reader.
        // The fixed member arrays are vectors sized to the counts (a count
        // past its array bound is refused — see the file header); new
        // Short[]/new Byte[] are zero-born; readTexClass (the TGA
        // asset load) reads nothing from the chunk stream and is dropped.
        Bool parseBlendTileData(DataChunkInput &file, DataChunkInfo *info, Bool use3Way)
        {
                Int i, j;
                Int len = file.readInt();
                m_btLen = len;
                if (m_dataSize != len) {
                        throw ErrLen();
                }
                m_tileNdxes.assign((size_t)m_dataSize, 0);       // new Short[m_dataSize]
                m_blendTileNdxes.assign((size_t)m_dataSize, 0);  // new Short[m_dataSize]
                m_extraBlendTileNdxes.assign((size_t)m_dataSize, 0); // new Short[m_dataSize]
                m_cliffInfoNdxes.assign((size_t)m_dataSize, 0);  // new Short[m_dataSize]
                // Note - we have one less cell than the width & height. But for paranoia, allocate
                // extra row. jba.
                Int numBytesX = (m_width + 7) / 8; // how many bytes to fit all bitflags
                Int numBytesY = m_height;

                m_flipStateWidth = numBytesX;

                m_cellFlipState.assign((size_t)(numBytesX * numBytesY), 0);  // clear all flags
                m_cellCliffState.assign((size_t)(numBytesX * numBytesY), 0); // clear all flags

                if (m_dataSize > 0)
                        file.readArrayOfBytes((char *)m_tileNdxes.data(),
                                              m_dataSize * (Int)sizeof(Short));
                if (m_dataSize > 0)
                        file.readArrayOfBytes((char *)m_blendTileNdxes.data(),
                                              m_dataSize * (Int)sizeof(Short));
                if (info->version >= K_BLEND_TILE_VERSION_6) {
                        if (m_dataSize > 0)
                                file.readArrayOfBytes((char *)m_extraBlendTileNdxes.data(),
                                                      m_dataSize * (Int)sizeof(Short));
                        // Allow clearing of extra blend tiles via ini and resaving of map.
                        // Useful for flushing out initial maps made with buggy 3-way blending.
                        if (!use3Way)
                                m_extraBlendTileNdxes.assign((size_t)m_dataSize, 0); // memset 0
                }
                if (info->version >= K_BLEND_TILE_VERSION_5) {
                        if (m_dataSize > 0)
                                file.readArrayOfBytes((char *)m_cliffInfoNdxes.data(),
                                                      m_dataSize * (Int)sizeof(Short));
                }
                if (info->version >= K_BLEND_TILE_VERSION_7) {
                        if (info->version == K_BLEND_TILE_VERSION_7) {
                                Int byteWidth =
                                    (m_width + 1) / 8; // previous incorrect length that got used to save the file.  jba. [4/3/2003]
                                std::vector<uint8_t> data(
                                    (size_t)(m_height * byteWidth), 0); // new Byte[...]
                                if (m_height * byteWidth > 0)
                                        file.readArrayOfBytes((char *)data.data(),
                                                              m_height * byteWidth);
                                for (j = 0; j < m_height; j++) {
                                        for (i = 0; i < byteWidth; i++) {
                                                m_cellCliffState[(size_t)(j * m_flipStateWidth + i)] =
                                                    data[(size_t)(j * byteWidth + i)];
                                        }
                                }
                        } else {
                                if (m_height * m_flipStateWidth > 0)
                                        file.readArrayOfBytes((char *)m_cellCliffState.data(),
                                                              m_height * m_flipStateWidth);
                        }
                } else {
                        initCliffFlagsFromHeights();
                }
                m_numBitmapTiles = file.readInt();
                // DEBUG_ASSERTCRASH(m_numBitmapTiles>0 && m_numBitmapTiles<2048, ("Unlikely
                // numBitmapTiles.")) — a debug-only contract; the count is stored verbatim.
                m_numBlendedTiles = file.readInt();
                if (m_numBlendedTiles > NUM_BLEND_TILES) {
                        m_lastCount = m_numBlendedTiles;
                        throw ErrNBlend(); // (the in-object overflow death shape — refused)
                }
                m_blendedTiles.assign(
                    (size_t)(m_numBlendedTiles > 0 ? m_numBlendedTiles : 0), TBlendTileInfo());
                if (info->version >= K_BLEND_TILE_VERSION_5) {
                        m_numCliffInfo = file.readInt();
                        if (m_numCliffInfo > NUM_CLIFF_INFO) {
                                m_lastCount = m_numCliffInfo;
                                throw ErrNCliff();
                        }
                } else {
                        m_numCliffInfo = 1; // cliffInfo[0] is the default info.
                }
                m_cliffInfo.assign((size_t)(m_numCliffInfo > 0 ? m_numCliffInfo : 0),
                                   TCliffInfo());
                // --> file loading here
                m_numTextureClasses = file.readInt();
                if (m_numTextureClasses > NUM_TEXTURE_CLASSES) {
                        m_lastCount = m_numTextureClasses;
                        throw ErrNTex();
                }
                m_textureClasses.clear();
                for (i = 0; i < m_numTextureClasses; i++) {
                        TXTextureClassRec tc;
                        // tc.globalTextureClass = -1 (runtime state, dropped)
                        tc.firstTile = file.readInt();
                        tc.numTiles = file.readInt();
                        tc.width = file.readInt();

                        // legacy GDF data
                        // used to read "m_textureClasses[i].isGDF = file.readInt();"
                        /* Int legacy = */ file.readInt();

                        tc.name = std::string((const char *)file.readAsciiString().str());
                        // readTexClass(&m_textureClasses[i], m_sourceTiles) — dropped (no
                        // chunk-stream reads; see the file header)
                        m_textureClasses.push_back(tc);
                }
                m_numEdgeTextureClasses = 0;
                m_numEdgeTiles = 0;
                if (info->version >= K_BLEND_TILE_VERSION_4) {
                        m_numEdgeTiles = file.readInt();
                        m_numEdgeTextureClasses = file.readInt();
                        if (m_numEdgeTextureClasses > NUM_TEXTURE_CLASSES) {
                                m_lastCount = m_numEdgeTextureClasses;
                                throw ErrNEdgeTex();
                        }
                        m_edgeTextureClasses.clear();
                        for (i = 0; i < m_numEdgeTextureClasses; i++) {
                                TXTextureClassRec tc;
                                tc.firstTile = file.readInt();
                                tc.numTiles = file.readInt();
                                tc.width = file.readInt();
                                tc.name =
                                    std::string((const char *)file.readAsciiString().str());
                                // readTexClass(&m_edgeTextureClasses[i], m_edgeTiles) — dropped
                                m_edgeTextureClasses.push_back(tc);
                        }
                }
                for (i = 1; i < m_numBlendedTiles; i++) {
                        Int flag;
                        m_blendedTiles[(size_t)i].blendNdx = file.readInt();
                        m_blendedTiles[(size_t)i].horiz = file.readByte();
                        m_blendedTiles[(size_t)i].vert = file.readByte();
                        m_blendedTiles[(size_t)i].rightDiagonal = file.readByte();
                        m_blendedTiles[(size_t)i].leftDiagonal = file.readByte();
                        m_blendedTiles[(size_t)i].inverted = file.readByte();
                        // Allow clearing of extra blend tiles via ini and resaving of map.
                        // Useful for flushing out initial maps made with buggy 3-way blending.
                        if (!use3Way)
                                m_blendedTiles[(size_t)i].inverted &=
                                    ~FLIPPED_MASK; // filter out extra flips from 3-way

                        if (info->version >= K_BLEND_TILE_VERSION_3) {
                                m_blendedTiles[(size_t)i].longDiagonal = file.readByte();
                        } else {
                                m_blendedTiles[(size_t)i].longDiagonal = false;
                        }
                        if (info->version >= K_BLEND_TILE_VERSION_4) {
                                m_blendedTiles[(size_t)i].customBlendEdgeClass = file.readInt();
                        } else {
                                m_blendedTiles[(size_t)i].customBlendEdgeClass = -1;
                        }

                        flag = file.readInt();
                        m_lastFlag = flag;
                        m_lastFlagIndex = i;
                        // DEBUG_ASSERTCRASH(flag==FLAG_VAL, ("Invalid format."))
                        if (flag != FLAG_VAL) {
                                throw ErrFlag();
                        }
                }
                if (info->version >= K_BLEND_TILE_VERSION_5) {
                        for (i = 1; i < m_numCliffInfo; i++) {
                                m_cliffInfo[(size_t)i].tileIndex = (Short)file.readInt();
                                m_cliffInfo[(size_t)i].u0 = file.readReal();
                                m_cliffInfo[(size_t)i].v0 = file.readReal();
                                m_cliffInfo[(size_t)i].u1 = file.readReal();
                                m_cliffInfo[(size_t)i].v1 = file.readReal();
                                m_cliffInfo[(size_t)i].u2 = file.readReal();
                                m_cliffInfo[(size_t)i].v2 = file.readReal();
                                m_cliffInfo[(size_t)i].u3 = file.readReal();
                                m_cliffInfo[(size_t)i].v3 = file.readReal();
                                m_cliffInfo[(size_t)i].flip = file.readByte();
                                m_cliffInfo[(size_t)i].mutant = file.readByte();
                        }
                }
                // Resize me.
                if (info->version == K_BLEND_TILE_VERSION_1) {
                        Int newWidth = (m_width + 1) / 2;
                        Int newHeight = (m_height + 1) / 2;
                        Int i, j;
                        for (i = 0; i < newHeight; i++) {
                                for (j = 0; j < newWidth; j++) {
                                        m_tileNdxes[(size_t)(i * newWidth + j)] =
                                            m_tileNdxes[(size_t)(2 * i * m_width + 2 * j)];
                                        m_blendTileNdxes[(size_t)(i * newWidth + j)] = 0;
                                        m_extraBlendTileNdxes[(size_t)(i * newWidth + j)] = 0;
                                        m_cliffInfoNdxes[(size_t)(i * newWidth + j)] = 0;
                                }
                        }
                        m_numBlendedTiles = 1;
                        m_numCliffInfo = 1;
                        m_blendedTiles.assign(1, TBlendTileInfo()); // (the count and the
                                                                    // records agree)
                        m_cliffInfo.assign(1, TCliffInfo());
                        m_width = newWidth;
                        m_height = newHeight;
                        m_dataSize = m_width * m_height;
                }
                // DEBUG_ASSERTCRASH(file.atEndOfChunk(), ("Unexpected data left
                // over.")) — a debug-only contract; the release load leaves the
                // leftovers to the walk's close.
                m_btWidth = m_width;
                m_btHeight = m_height;
                m_btDataSize = m_dataSize;
                m_blendLoaded = true;
                return true;
        }

        // The constructor's post-walk "patch bad maps" loop (the full-load
        // path only). The null-array death (a height map loaded, no blend
        // chunk did) is refused by the caller; the overread death (arrays
        // shorter than the live m_dataSize) skips the missing tail — see
        // the file header.
        void patchBadMaps()
        {
                Int i;
                for (i = 0; i < m_dataSize; i++) {
                        if ((Int)m_cliffInfoNdxes.size() > i) {
                                if (m_cliffInfoNdxes[(size_t)i] < 0 ||
                                    m_cliffInfoNdxes[(size_t)i] >= m_numCliffInfo) {
                                        m_cliffInfoNdxes[(size_t)i] = 0;
                                }
                        }
                        if ((Int)m_blendTileNdxes.size() > i) {
                                if (m_blendTileNdxes[(size_t)i] < 0 ||
                                    m_blendTileNdxes[(size_t)i] >= m_numBlendedTiles) {
                                        m_blendTileNdxes[(size_t)i] = 0;
                                }
                        }
                        if ((Int)m_extraBlendTileNdxes.size() > i) {
                                if (m_extraBlendTileNdxes[(size_t)i] < 0 ||
                                    m_extraBlendTileNdxes[(size_t)i] >= m_numBlendedTiles) {
                                        m_extraBlendTileNdxes[(size_t)i] = 0;
                                }
                        }
                }
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
                line(std::string("bstate ") + id + " " + mode + " " +
                     std::to_string((Int)in.tell()) + " " + std::to_string(in.depth()) +
                     " " + (in.atEndOfFile() ? "t" : "f"));
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

// an i16 vector as a signed decimal list ('-' for empty).
static std::string i16s(const std::vector<int16_t> &v)
{
        if (v.empty())
                return "-";
        std::string out;
        for (size_t i = 0; i < v.size(); ++i) {
                if (i)
                        out.push_back(' ');
                out += std::to_string((Int)v[i]);
        }
        return out;
}

// a float's bit pattern.
static std::string f32hex(float f)
{
        uint32_t bits;
        memcpy(&bits, &f, 4);
        char buf[16];
        snprintf(buf, sizeof(buf), "%08x", bits);
        return std::string(buf);
}

// Names: '-' when empty, the raw bytes when printable non-space ASCII,
// `<hex>` otherwise.
static std::string q_bytes(const uint8_t *name, size_t len)
{
        if (len == 0)
                return "-";
        for (size_t i = 0; i < len; i++) {
                if (name[i] < 0x21 || name[i] > 0x7e) {
                        std::string s = "<";
                        static const char *d = "0123456789abcdef";
                        for (size_t j = 0; j < len; ++j) {
                                s.push_back(d[name[j] >> 4]);
                                s.push_back(d[name[j] & 15]);
                        }
                        s += ">";
                        return s;
                }
        }
        return std::string((const char *)name, len);
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
static void put_f32(std::vector<uint8_t> &b, float v)
{
        uint32_t bits;
        memcpy(&bits, &v, 4);
        put_u32(b, bits);
}
static void put_str(std::vector<uint8_t> &b, const std::string &s)
{
        b.insert(b.end(), s.begin(), s.end());
}
static void put_bytes(std::vector<uint8_t> &b, const std::vector<uint8_t> &s)
{
        b.insert(b.end(), s.begin(), s.end());
}
static void put_i16s(std::vector<uint8_t> &b, const std::vector<int16_t> &s)
{
        for (int16_t v : s) {
                b.push_back((uint8_t)(uint16_t)v);
                b.push_back((uint8_t)((uint16_t)v >> 8));
        }
}
static void put_name(std::vector<uint8_t> &b, const std::string &s)
{
        put_u16(b, (uint16_t)s.size());
        put_str(b, s);
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

// a formula byte string both sides rebuild identically
static std::vector<uint8_t> formula_bytes(size_t n, uint8_t seed)
{
        std::vector<uint8_t> s;
        s.resize(n);
        for (size_t i = 0; i < n; ++i)
                s[i] = (uint8_t)((seed + i * 7) & 0xFF);
        return s;
}

// a formula i16 string both sides rebuild identically
static std::vector<int16_t> formula_i16s(size_t n, int16_t seed)
{
        std::vector<int16_t> s;
        s.resize(n);
        for (size_t i = 0; i < n; ++i)
                s[i] = (int16_t)((seed + i * 7) & 0xFFFF);
        return s;
}

// --- the blend payload builder (the writer side of WHeightMapEdit.cpp's
// shape, version-gated like the reader) --------------------------------------

struct TexRec {
        Int first, num, width;
        std::string name;
};
struct BlendRec {
        Int ndx;
        Byte horiz, vert, rdiag, ldiag, inv, longd;
        Int custom;
        Int flag; // FLAG_VAL for good records
};
struct CliffRec {
        Int tileIdx;
        Real u0, v0, u1, v1, u2, v2, u3, v3;
        Byte flip, mutant;
};

struct BtParts {
        std::vector<int16_t> tile, blend, extra, cliff;
        std::vector<uint8_t> cliffState; // the v7 (byteWidth-shaped) or v8 (fsw) bytes
        Int nbmp = 0, nblend = 1, ncliff = 1;
        Int nTex = -1;  // the count word (-1: the tex vector's size)
        std::vector<TexRec> tex;
        Int nEdgeTiles = 0;
        Int nEdgeTex = -1; // the count word (-1: the etex vector's size)
        std::vector<TexRec> etex;
        std::vector<BlendRec> blends; // written for i in [1, nblend)
        std::vector<CliffRec> cliffs; // written for i in [1, ncliff), v >= 5
};

static std::vector<uint8_t> blend_payload(uint16_t version, Int len, const BtParts &p)
{
        std::vector<uint8_t> b;
        put_i32(b, len);
        put_i16s(b, p.tile);
        put_i16s(b, p.blend);
        if (version >= K_BLEND_TILE_VERSION_6)
                put_i16s(b, p.extra);
        if (version >= K_BLEND_TILE_VERSION_5)
                put_i16s(b, p.cliff);
        if (version >= K_BLEND_TILE_VERSION_7)
                put_bytes(b, p.cliffState);
        put_i32(b, p.nbmp);
        put_i32(b, p.nblend);
        if (version >= K_BLEND_TILE_VERSION_5)
                put_i32(b, p.ncliff);
        put_i32(b, p.nTex < 0 ? (Int)p.tex.size() : p.nTex);
        for (const TexRec &t : p.tex) {
                put_i32(b, t.first);
                put_i32(b, t.num);
                put_i32(b, t.width);
                put_i32(b, 0); // the legacy GDF word, read and discarded
                put_name(b, t.name);
        }
        if (version >= K_BLEND_TILE_VERSION_4) {
                put_i32(b, p.nEdgeTiles);
                put_i32(b, p.nEdgeTex < 0 ? (Int)p.etex.size() : p.nEdgeTex);
                for (const TexRec &t : p.etex) {
                        put_i32(b, t.first);
                        put_i32(b, t.num);
                        put_i32(b, t.width);
                        put_name(b, t.name); // (no legacy word on the edge classes)
                }
        }
        for (const BlendRec &r : p.blends) {
                put_i32(b, r.ndx);
                b.push_back(r.horiz);
                b.push_back(r.vert);
                b.push_back(r.rdiag);
                b.push_back(r.ldiag);
                b.push_back(r.inv);
                if (version >= K_BLEND_TILE_VERSION_3)
                        b.push_back(r.longd);
                if (version >= K_BLEND_TILE_VERSION_4)
                        put_i32(b, r.custom);
                put_i32(b, r.flag);
        }
        if (version >= K_BLEND_TILE_VERSION_5) {
                for (const CliffRec &c : p.cliffs) {
                        put_i32(b, c.tileIdx);
                        put_f32(b, c.u0);
                        put_f32(b, c.v0);
                        put_f32(b, c.u1);
                        put_f32(b, c.v1);
                        put_f32(b, c.u2);
                        put_f32(b, c.v2);
                        put_f32(b, c.u3);
                        put_f32(b, c.v3);
                        b.push_back(c.flip);
                        b.push_back(c.mutant);
                }
        }
        return b;
}

// --- the height-map payload builders (gen_mapheightmap_golden.cpp's) --------

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

// --- the harness (the constructor's FULL-load walk, both 3-way settings) ----

// the walk's user data: the shared WorldHeightMap state (the original's
// `this`), which reader fired, and the error capture.
struct BtState {
        WorldHeightMapLoad map;
        Bool hmLoaded = false; // whether any HeightMapData fired
        Bool use3Way = true;   // TheGlobalData->m_use3WayTerrainBlends (default 1)
        enum ErrKind {
                ERR_NONE = 0,
                ERR_MAPSIZE,
                ERR_DATASIZE,
                ERR_BORDERCOUNT,
                ERR_BLENLEN,
                ERR_FLAG,
                ERR_NBLEND,
                ERR_NCLIFF,
                ERR_NTEX,
                ERR_NETEX,
                ERR_PATCHNULL,
                ERR_CORRUPT,
        };
        ErrKind errKind = ERR_NONE;
        Int errA = 0, errB = 0, errC = 0, errD = 0;
        UnsignedInt errLeft = 0;
};

// the registered HeightMapData parser (the full load's reader).
static Bool hm_parser(DataChunkInput &file, DataChunkInfo *info, void *userData)
{
        BtState *st = (BtState *)userData;
        try {
                st->map.parseHeightMapData(file, info);
                st->hmLoaded = true;
                return true;
        } catch (CorruptFileFormat &) {
                // which contract threw: re-derived from the surviving state —
                // the map-size check's own inputs when it refused, else the
                // data-size contract (m_dataSize was the last read).
                if (!WorldHeightMapLoad::isValidTotalMapSize(st->map.m_width, st->map.m_height,
                                                              st->map.m_borderSize)) {
                        st->errKind = BtState::ERR_MAPSIZE;
                        st->errA = st->map.m_width;
                        st->errB = st->map.m_height;
                        st->errC = st->map.m_borderSize;
                } else {
                        st->errKind = BtState::ERR_DATASIZE;
                        st->errA = st->map.m_dataSize;
                        st->errB = st->map.m_width;
                        st->errC = st->map.m_height;
                        st->errLeft = file.getChunkDataSizeLeft();
                }
                return false;
        } catch (BorderCountCorrupt &) {
                st->errKind = BtState::ERR_BORDERCOUNT;
                st->errA = st->map.m_lastNumBorders;
                return false;
        }
}

// the registered BlendTileData parser.
static Bool bt_parser(DataChunkInput &file, DataChunkInfo *info, void *userData)
{
        BtState *st = (BtState *)userData;
        try {
                st->map.parseBlendTileData(file, info, st->use3Way);
                return true;
        } catch (ErrLen &) {
                st->errKind = BtState::ERR_BLENLEN;
                st->errA = st->map.m_btLen;
                st->errB = st->map.m_dataSize;
                return false;
        } catch (ErrFlag &) {
                st->errKind = BtState::ERR_FLAG;
                st->errA = st->map.m_lastFlagIndex;
                st->errB = st->map.m_lastFlag;
                return false;
        } catch (ErrNBlend &) {
                st->errKind = BtState::ERR_NBLEND;
                st->errA = st->map.m_lastCount;
                return false;
        } catch (ErrNCliff &) {
                st->errKind = BtState::ERR_NCLIFF;
                st->errA = st->map.m_lastCount;
                return false;
        } catch (ErrNTex &) {
                st->errKind = BtState::ERR_NTEX;
                st->errA = st->map.m_lastCount;
                return false;
        } catch (ErrNEdgeTex &) {
                st->errKind = BtState::ERR_NETEX;
                st->errA = st->map.m_lastCount;
                return false;
        }
}

// a recursing container observer (the real map loaders' shape — the
// ObjectsList parser that walks its children), for the scope corpus.
static Bool bt_container(DataChunkInput &file, DataChunkInfo *info, void *userData)
{
        (void)info;
        return file.parse(userData);
}

// dumps the blend half (pre-patch): the bt line, the raw index arrays,
// the state bitmaps, the classes, the records.
static void dump_blend_raw(Out &out, const char *id, const char *m,
                           const WorldHeightMapLoad &map)
{
        out.line(std::string("bt ") + id + " " + m + " " +
                 std::to_string((Int)map.m_tileNdxes.size()) + " " +
                 std::to_string(map.m_numBitmapTiles) + " " +
                 std::to_string(map.m_numBlendedTiles) + " " +
                 std::to_string(map.m_numCliffInfo) + " " +
                 std::to_string(map.m_numTextureClasses) + " " +
                 std::to_string(map.m_numEdgeTiles) + " " +
                 std::to_string(map.m_numEdgeTextureClasses) + " " +
                 std::to_string(map.m_btWidth) + " " + std::to_string(map.m_btHeight) +
                 " " + std::to_string(map.m_btDataSize) + " " +
                 std::to_string(map.m_flipStateWidth));
        out.line(std::string("traw ") + id + " " + m + " " + i16s(map.m_tileNdxes));
        out.line(std::string("braw ") + id + " " + m + " " + i16s(map.m_blendTileNdxes));
        out.line(std::string("xraw ") + id + " " + m + " " + i16s(map.m_extraBlendTileNdxes));
        out.line(std::string("craw ") + id + " " + m + " " + i16s(map.m_cliffInfoNdxes));
        out.line(std::string("flip ") + id + " " + m + " " + hx(map.m_cellFlipState));
        out.line(std::string("cst ") + id + " " + m + " " + hx(map.m_cellCliffState));
        for (size_t i = 0; i < map.m_textureClasses.size(); ++i) {
                const TXTextureClassRec &t = map.m_textureClasses[i];
                out.line(std::string("tex ") + id + " " + m + " " + std::to_string(i) + " " +
                         std::to_string(t.firstTile) + " " + std::to_string(t.numTiles) + " " +
                         std::to_string(t.width) + " " +
                         q_bytes((const uint8_t *)t.name.data(), t.name.size()));
        }
        // (count-driven: the original's fixed edge array keeps stale
        // entries across blend chunks — the count resets, the entries
        // don't — but every consumer reads the count, and so does the
        // dump)
        for (Int i = 0; i < map.m_numEdgeTextureClasses; i++) {
                const TXTextureClassRec &t = map.m_edgeTextureClasses[(size_t)i];
                out.line(std::string("etex ") + id + " " + m + " " + std::to_string(i) + " " +
                         std::to_string(t.firstTile) + " " + std::to_string(t.numTiles) + " " +
                         std::to_string(t.width) + " " +
                         q_bytes((const uint8_t *)t.name.data(), t.name.size()));
        }
        for (Int i = 1; i < map.m_numBlendedTiles; i++) {
                const TBlendTileInfo &r = map.m_blendedTiles[(size_t)i];
                out.line(std::string("bti ") + id + " " + m + " " + std::to_string(i) + " " +
                         std::to_string(r.blendNdx) + " " + std::to_string((Int)r.horiz) +
                         " " + std::to_string((Int)r.vert) + " " +
                         std::to_string((Int)r.rightDiagonal) + " " +
                         std::to_string((Int)r.leftDiagonal) + " " +
                         std::to_string((Int)r.inverted) + " " +
                         std::to_string((Int)r.longDiagonal) + " " +
                         std::to_string(r.customBlendEdgeClass));
        }
        for (Int i = 1; i < map.m_numCliffInfo; i++) {
                const TCliffInfo &c = map.m_cliffInfo[(size_t)i];
                out.line(std::string("cfi ") + id + " " + m + " " + std::to_string(i) + " " +
                         std::to_string((Int)c.tileIndex) + " " + f32hex(c.u0) + " " +
                         f32hex(c.v0) + " " + f32hex(c.u1) + " " + f32hex(c.v1) + " " +
                         f32hex(c.u2) + " " + f32hex(c.v2) + " " + f32hex(c.u3) + " " +
                         f32hex(c.v3) + " " + (c.flip ? "1" : "0") + " " +
                         (c.mutant ? "1" : "0"));
        }
}

// runs one corpus through both 3-way settings and dumps everything.
// with_container registers the recursing ObjectsList observer (the scope
// corpus needs a parent that walks children).
static void run_bt(Out &out, const char *id, const std::vector<uint8_t> &bytes,
                   Bool with_container = false)
{
        out.file(id, bytes);
        for (Int mode = 0; mode < 2; ++mode) {
                const char *m = mode ? "n" : "3";
                CachedFileInputStream s(bytes);
                DataChunkInput in(&s);
                if (mode == 0)
                        out.tf("valid", id, in.isValidFileType());
                BtState st;
                st.use3Way = (mode == 0);
                in.registerParser("HeightMapData", "", hm_parser);
                in.registerParser("BlendTileData", "", bt_parser);
                if (with_container)
                        in.registerParser("ObjectsList", "", bt_container);
                Bool ok = in.parse(&st);
                out.line(std::string("walk ") + id + " " + m + (ok ? " t" : " f"));
                if (!ok) {
                        std::string e = std::string("herr ") + id + " " + m + " ";
                        switch (st.errKind) {
                                case BtState::ERR_MAPSIZE:
                                        e += "mapsize " + std::to_string(st.errA) + " " +
                                             std::to_string(st.errB) + " " +
                                             std::to_string(st.errC);
                                        break;
                                case BtState::ERR_DATASIZE:
                                        e += "datasize " + std::to_string(st.errA) + " " +
                                             std::to_string(st.errB) + " " +
                                             std::to_string(st.errC) + " " +
                                             std::to_string(st.errLeft);
                                        break;
                                case BtState::ERR_BORDERCOUNT:
                                        e += "bordercount " + std::to_string(st.errA);
                                        break;
                                case BtState::ERR_BLENLEN:
                                        e += "blenlen " + std::to_string(st.errA) + " " +
                                             std::to_string(st.errB);
                                        break;
                                case BtState::ERR_FLAG:
                                        e += "flag " + std::to_string(st.errA) + " " +
                                             std::to_string(st.errB);
                                        break;
                                case BtState::ERR_NBLEND:
                                case BtState::ERR_NCLIFF:
                                case BtState::ERR_NTEX:
                                case BtState::ERR_NETEX: {
                                        const char *kind = "nblend";
                                        if (st.errKind == BtState::ERR_NCLIFF)
                                                kind = "ncliff";
                                        else if (st.errKind == BtState::ERR_NTEX)
                                                kind = "ntex";
                                        else if (st.errKind == BtState::ERR_NETEX)
                                                kind = "netex";
                                        e += std::string(kind) + " " + std::to_string(st.errA);
                                        break;
                                }
                                default:
                                        // the walk refused before (or without) a
                                        // payload error: the header never opened
                                        e += "corrupt";
                                        break;
                        }
                        out.line(e);
                } else {
                        // the height-map half, as the FULL walk leaves it (a
                        // version-1 blend resize may have halved the extents
                        // after the height reader was done)
                        if (st.hmLoaded) {
                                out.line(std::string("hm ") + id + " " + m + " " +
                                         std::to_string(st.map.m_width) + " " +
                                         std::to_string(st.map.m_height) + " " +
                                         std::to_string(st.map.m_borderSize) + " " +
                                         std::to_string(st.map.m_dataSize) + " " +
                                         std::to_string((Int)st.map.m_boundaries.size()));
                                for (size_t i = 0; i < st.map.m_boundaries.size(); ++i) {
                                        out.line(std::string("bnd ") + id + " " + m + " " +
                                                 std::to_string(i) + " " +
                                                 std::to_string(st.map.m_boundaries[i].x) +
                                                 " " +
                                                 std::to_string(st.map.m_boundaries[i].y));
                                }
                                out.line(std::string("hdat ") + id + " " + m + " " +
                                         hx(st.map.m_data));
                        } else {
                                out.line(std::string("hnone ") + id + " " + m);
                        }
                        // the blend half, as its parse left it (pre-patch)
                        if (st.map.m_blendLoaded) {
                                dump_blend_raw(out, id, m, st.map);
                        } else {
                                out.line(std::string("bnone ") + id + " " + m);
                        }
                        // the constructor's post-walk patch loop
                        if (st.map.m_dataSize > 0 && !st.map.m_blendLoaded) {
                                // (the original dereferences the null index arrays
                                // here — a process death; the refusal)
                                out.line(std::string("herr ") + id + " " + m + " patchnull " +
                                         std::to_string(st.map.m_dataSize));
                        } else {
                                st.map.patchBadMaps();
                                out.line(std::string("tndx ") + id + " " + m + " " +
                                         i16s(st.map.m_tileNdxes));
                                out.line(std::string("bndx ") + id + " " + m + " " +
                                         i16s(st.map.m_blendTileNdxes));
                                out.line(std::string("xndx ") + id + " " + m + " " +
                                         i16s(st.map.m_extraBlendTileNdxes));
                                out.line(std::string("cndx ") + id + " " + m + " " +
                                         i16s(st.map.m_cliffInfoNdxes));
                        }
                }
                out.state(id, m, in);
        }
}

// --- the corpora -------------------------------------------------------------

// the modern full load: a version-8 blend payload with every feature on
// (the four arrays, the direct cliff-state read, two texture classes, an
// edge class, four blend records, two cliff infos with the float set),
// unknown chunks around it the walk skips.
static void corpus_bt_v8(Out &out)
{
        const char *id = "bt_v8";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1},
                                            {"BlendTileData", 2},
                                            {"GlobalLighting", 3},
                                            {"SidesList", 4}});
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_4,
                  v4_payload(8, 6, 1, {{6, 4}, {8, 6}, {7, 5}}, formula_bytes(48, 3)));
        {
                BtParts p;
                p.tile = formula_i16s(48, 33);
                p.blend = formula_i16s(48, 44);
                p.extra = formula_i16s(48, 55);
                p.cliff = formula_i16s(48, 66);
                p.cliffState = formula_bytes(6, 77); // fsw 1 * height 6
                p.nbmp = 7;
                p.nblend = 5;
                p.ncliff = 3;
                p.tex = {{0, 4, 2, "GrassA"}, {8, 4, 2, "SnowB"}};
                p.nEdgeTiles = 24;
                p.etex = {{24, 4, 2, "RoadC"}};
                p.blends = {
                    {10, 1, 2, 3, 4, 0x00, 1, 2, FLAG_VAL},
                    {11, 0, 1, 0, 1, 0x01, 0, -1, FLAG_VAL},
                    {12, 255, 254, 253, 252, 0x02, 1, 0, FLAG_VAL},
                    {13, 128, 127, 1, 2, 0x03, 1, 5, FLAG_VAL},
                };
                p.cliffs = {
                    {70000, -1.5f, 0.0f, -0.0f, 1e-40f, 1e30f, 0.25f, -3.75f, 1000.0f, 2, 0},
                    {-70000, 0.5f, -0.5f, 2.0f, -2.0f, 0.125f, -0.125f, 3.5f, -3.5f, 0, 0x80},
                };
                put_chunk(b, 2, K_BLEND_TILE_VERSION_8, blend_payload(8, 48, p));
        }
        {
                std::vector<uint8_t> d;
                put_i32(d, 0);
                put_chunk(b, 3, 2, d); // GlobalLighting: unknown here, skipped
        }
        {
                std::vector<uint8_t> d(4, 0x55);
                put_chunk(b, 4, 1, d); // SidesList: unknown here, skipped
        }
        run_bt(out, id, b);
}

// the 3-way gates: inverted bytes carrying the FLIPPED_MASK bit (kept
// with blends on, filtered off) and extra-blend indices (kept on,
// flushed off).
static void corpus_bt_3way(Out &out)
{
        const char *id = "bt_3way";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}, {"BlendTileData", 2}});
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_2, v2_payload(4, 2, formula_bytes(8, 9)));
        {
                BtParts p;
                p.tile = {0, 1, 2, 3, 4, 5, 6, 7};
                p.blend = {0, 0, 0, 0, 0, 0, 0, 0};
                p.extra = {9, -9, 1, 2, 3, 4, 5, 6};
                p.cliff = {0, 1, 0, 1, 0, 1, 0, 1};
                p.cliffState = formula_bytes(2, 21); // fsw 1 * height 2
                p.nbmp = 1;
                p.nblend = 9;
                p.ncliff = 1;
                p.tex = {{0, 4, 2, "T"}};
                p.blends = {
                    {0, 0, 0, 0, 0, 0x00, 0, -1, FLAG_VAL},
                    {0, 0, 0, 0, 0, 0x01, 0, -1, FLAG_VAL},
                    {0, 0, 0, 0, 0, 0x02, 0, -1, FLAG_VAL},
                    {0, 0, 0, 0, 0, 0x03, 0, -1, FLAG_VAL},
                    {0, 0, 0, 0, 0, 0x7f, 0, -1, FLAG_VAL},
                    {0, 0, 0, 0, 0, 0x80, 0, -1, FLAG_VAL},
                    {0, 0, 0, 0, 0, 0xfd, 0, -1, FLAG_VAL},
                    {0, 0, 0, 0, 0, 0xff, 0, -1, FLAG_VAL},
                };
                put_chunk(b, 2, K_BLEND_TILE_VERSION_8, blend_payload(8, 8, p));
        }
        run_bt(out, id, b);
}

// version 7: the cliff state was saved with the incorrect (width+1)/8
// row length; the load remaps the short rows into the (width+7)/8
// bitmap, leaving each row's tail byte zero (17 wide: 2 -> 3).
static void corpus_bt_v7(Out &out)
{
        const char *id = "bt_v7";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}, {"BlendTileData", 2}});
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_3, v3_payload(17, 3, 0, formula_bytes(51, 13)));
        {
                BtParts p;
                p.tile = formula_i16s(51, 15);
                p.blend = formula_i16s(51, 25);
                p.extra = formula_i16s(51, 35);
                p.cliff = formula_i16s(51, 45);
                p.cliffState = formula_bytes(6, 88); // height 3 * byteWidth 2
                p.nbmp = 2;
                p.nblend = 2;
                p.ncliff = 2;
                p.tex = {{0, 4, 2, "A17"}};
                p.blends = {{20, 1, 0, 1, 0, 0x01, 1, 3, FLAG_VAL}};
                p.cliffs = {{300, 0.0f, 0.5f, 1.0f, 1.5f, 2.0f, 2.5f, 3.0f, 3.5f, 1, 1}};
                put_chunk(b, 2, K_BLEND_TILE_VERSION_7, blend_payload(7, 51, p));
        }
        run_bt(out, id, b);
}

// version 7 with the row lengths equal (16 wide: 2 == 2), and the file
// truncated one byte into the cliff-state read: the remap copies the
// zero-filled short-read tail, and every count downstream reads zero.
static void corpus_bt_v7b(Out &out)
{
        const char *id = "bt_v7b";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}, {"BlendTileData", 2}});
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_2, v2_payload(16, 2, formula_bytes(32, 17)));
        {
                BtParts p;
                p.tile = formula_i16s(32, 19);
                p.blend = formula_i16s(32, 29);
                p.extra = formula_i16s(32, 39);
                p.cliff = formula_i16s(32, 49);
                p.cliffState = formula_bytes(4, 59); // height 2 * byteWidth 2
                p.nbmp = 3;
                p.nblend = 4;
                p.ncliff = 2;
                p.tex = {{0, 4, 2, "X"}};
                p.blends = {{1, 0, 0, 0, 0, 0, 0, 0, FLAG_VAL}};
                p.cliffs = {{5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0}};
                std::vector<uint8_t> full = blend_payload(7, 32, p);
                // len 4 + four 32-entry arrays (256) + 1 byte of cliff state
                std::vector<uint8_t> cut(full.begin(), full.begin() + 261);
                put_chunk(b, 2, K_BLEND_TILE_VERSION_7, cut, (int32_t)full.size());
        }
        run_bt(out, id, b);
}

// version 7 with a 1-wide map: (1+1)/8 = 0 bytes of cliff state per row
// — nothing is read, the bitmap stays zero.
static void corpus_bt_v7c(Out &out)
{
        const char *id = "bt_v7c";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}, {"BlendTileData", 2}});
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_2, v2_payload(1, 2, formula_bytes(2, 23)));
        {
                BtParts p;
                p.tile = {7, 8};
                p.blend = {0, 0};
                p.extra = {0, 0};
                p.cliff = {0, 0};
                p.cliffState = {}; // height 2 * byteWidth 0
                p.nbmp = 1;
                p.nblend = 1;
                p.ncliff = 1;
                put_chunk(b, 2, K_BLEND_TILE_VERSION_7, blend_payload(7, 2, p));
        }
        run_bt(out, id, b);
}

// version 6: the extra blend layer exists, the cliff state is still
// derived from the heights (a 16-unit jump is a cliff, a 15-unit one is
// not — the 9.8 slope limit over the 10/16 height scale).
static void corpus_bt_v6(Out &out)
{
        const char *id = "bt_v6";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}, {"BlendTileData", 2}});
        // heights: 16 where x >= 3 on row 0 (a cliff at cell x=2), 15 where
        // x >= 3 on row 2 (below the limit — no cliff)
        std::vector<uint8_t> h;
        for (Int y = 0; y < 3; ++y)
                for (Int x = 0; x < 5; ++x) {
                        uint8_t v = 0;
                        if (y == 0 && x >= 3)
                                v = 16;
                        if (y == 2 && x >= 3)
                                v = 15;
                        h.push_back(v);
                }
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_2, v2_payload(5, 3, h));
        {
                BtParts p;
                p.tile = formula_i16s(15, 27);
                p.blend = formula_i16s(15, 37);
                p.extra = formula_i16s(15, 47);
                p.cliff = formula_i16s(15, 57);
                p.nbmp = 2;
                p.nblend = 3;
                p.ncliff = 2;
                p.tex = {{0, 4, 2, "G6"}};
                p.blends = {
                    {30, 1, 1, 1, 1, 0x01, 1, 7, FLAG_VAL},
                    {31, 2, 2, 2, 2, 0x02, 0, 8, FLAG_VAL},
                };
                p.cliffs = {{400, 0.1f, 0.2f, 0.3f, 0.4f, 0.5f, 0.6f, 0.7f, 0.8f, 1, 0}};
                put_chunk(b, 2, K_BLEND_TILE_VERSION_6, blend_payload(6, 15, p));
        }
        run_bt(out, id, b);
}

// version 5: the cliff indices and infos are read, the extra blend layer
// is not (the zero-filled stance for the never-read array).
static void corpus_bt_v5(Out &out)
{
        const char *id = "bt_v5";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}, {"BlendTileData", 2}});
        std::vector<uint8_t> h;
        for (Int y = 0; y < 3; ++y)
                for (Int x = 0; x < 5; ++x)
                        h.push_back((y == 0 && x >= 3) ? (uint8_t)16 : (uint8_t)0);
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_2, v2_payload(5, 3, h));
        {
                BtParts p;
                p.tile = formula_i16s(15, 31);
                p.blend = formula_i16s(15, 41);
                p.cliff = formula_i16s(15, 51);
                p.nbmp = 1;
                p.nblend = 2;
                p.ncliff = 3;
                p.tex = {{0, 1, 1, "V5"}};
                p.blends = {{40, 3, 0, 0, 1, 0x01, 1, 2, FLAG_VAL}};
                p.cliffs = {
                    {1, 0.0f, 1.0f, 0.0f, 1.0f, 0.0f, 1.0f, 0.0f, 1.0f, 0, 0},
                    {2, 0.25f, 0.75f, 0.25f, 0.75f, 0.25f, 0.75f, 0.25f, 0.75f, 1, 1},
                };
                put_chunk(b, 2, K_BLEND_TILE_VERSION_5, blend_payload(5, 15, p));
        }
        run_bt(out, id, b);
}

// version 4: the edge classes arrive; no cliff indices (zero-filled), the
// single default cliff info, long diagonals and custom edge classes read.
static void corpus_bt_v4(Out &out)
{
        const char *id = "bt_v4";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}, {"BlendTileData", 2}});
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_2, v2_payload(5, 3, formula_bytes(15, 43)));
        {
                BtParts p;
                p.tile = formula_i16s(15, 53);
                p.blend = formula_i16s(15, 63);
                p.nbmp = 1;
                p.nblend = 3;
                p.tex = {{0, 4, 2, "B4"}, {4, 4, 2, "C4"}};
                p.nEdgeTiles = 16;
                p.etex = {{16, 4, 2, "E4"}};
                p.blends = {
                    {50, 1, 0, 0, 0, 0x01, 1, 0, FLAG_VAL},
                    {51, 0, 1, 0, 0, 0x00, 0, -1, FLAG_VAL},
                };
                put_chunk(b, 2, K_BLEND_TILE_VERSION_4, blend_payload(4, 15, p));
        }
        run_bt(out, id, b);
}

// version 3: long diagonals read, no edge classes (custom = -1).
static void corpus_bt_v3(Out &out)
{
        const char *id = "bt_v3";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}, {"BlendTileData", 2}});
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_3, v3_payload(3, 2, 0, formula_bytes(6, 47)));
        {
                BtParts p;
                p.tile = {1, 2, 3, 4, 5, 6};
                p.blend = {0, 1, 0, 1, 0, 1};
                p.nbmp = 1;
                p.nblend = 2;
                p.tex = {{0, 1, 1, "D3"}};
                p.blends = {{60, 9, 8, 7, 6, 0x01, 1, 0, FLAG_VAL}};
                put_chunk(b, 2, K_BLEND_TILE_VERSION_3, blend_payload(3, 6, p));
        }
        run_bt(out, id, b);
}

// version 2: no long diagonal (0), custom = -1.
static void corpus_bt_v2(Out &out)
{
        const char *id = "bt_v2";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}, {"BlendTileData", 2}});
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_2, v2_payload(3, 2, formula_bytes(6, 51)));
        {
                BtParts p;
                p.tile = {7, 8, 9, 10, 11, 12};
                p.blend = {2, 0, 2, 0, 2, 0};
                p.nbmp = 1;
                p.nblend = 2;
                p.tex = {{0, 1, 1, "E2"}};
                p.blends = {{70, 1, 2, 3, 4, 0x01, 0, 0, FLAG_VAL}};
                put_chunk(b, 2, K_BLEND_TILE_VERSION_2, blend_payload(2, 6, p));
        }
        run_bt(out, id, b);
}

// version 0: reads like version 2 (all the gates are ceilings).
static void corpus_bt_v0(Out &out)
{
        const char *id = "bt_v0";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}, {"BlendTileData", 2}});
        put_chunk(b, 1, 0, v2_payload(2, 2, formula_bytes(4, 55)));
        {
                BtParts p;
                p.tile = {1, 2, 3, 4};
                p.blend = {0, 0, 0, 0};
                p.nbmp = 1;
                p.nblend = 1;
                put_chunk(b, 2, 0, blend_payload(0, 4, p));
        }
        run_bt(out, id, b);
}

// version 1: the downsample. The height map's own version-1 compaction
// leaves the extents at the file's (the full load), then the blend's
// version-1 resize halves them, zeroes the blend/extra/cliff heads,
// forces the counts to 1 and re-points m_dataSize — the patch loop then
// walks the halved count only.
static void corpus_bt_v1(Out &out)
{
        const char *id = "bt_v1";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}, {"BlendTileData", 2}});
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_1, v2_payload(5, 5, formula_bytes(25, 59)));
        {
                BtParts p;
                p.tile = formula_i16s(25, 61); // visible compaction: [i*3+j] = [2i*5+2j]
                p.blend = formula_i16s(25, 71);
                p.nbmp = 2;
                p.nblend = 3; // forced to 1 by the resize
                p.tex = {{0, 4, 2, "F1"}};
                p.blends = {
                    {80, 1, 0, 0, 0, 0x01, 1, 0, FLAG_VAL},
                    {81, 0, 1, 0, 0, 0x01, 0, 0, FLAG_VAL},
                };
                put_chunk(b, 2, K_BLEND_TILE_VERSION_1, blend_payload(1, 25, p));
        }
        run_bt(out, id, b);
}

// version 9 (above 8): reads like version 8.
static void corpus_bt_v9(Out &out)
{
        const char *id = "bt_v9";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}, {"BlendTileData", 2}});
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_2, v2_payload(4, 2, formula_bytes(8, 63)));
        {
                BtParts p;
                p.tile = {1, 2, 3, 4, 5, 6, 7, 8};
                p.blend = {0, 1, 2, 3, 4, 5, 6, 7};
                p.extra = {7, 6, 5, 4, 3, 2, 1, 0};
                p.cliff = {1, 0, 1, 0, 1, 0, 1, 0};
                p.cliffState = {0xaa, 0x55};
                p.nbmp = 1;
                p.nblend = 2;
                p.ncliff = 2;
                p.tex = {{0, 1, 1, "G9"}};
                p.blends = {{90, 1, 1, 1, 1, 0x01, 1, 1, FLAG_VAL}};
                p.cliffs = {{9, 1.0f, 2.0f, 3.0f, 4.0f, 5.0f, 6.0f, 7.0f, 8.0f, 1, 1}};
                put_chunk(b, 2, 9, blend_payload(9, 8, p));
        }
        run_bt(out, id, b);
}

// the minimal legal version-8 load: a 1x1 map, one entry per array, no
// records at all.
static void corpus_bt_minimal(Out &out)
{
        const char *id = "bt_minimal";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}, {"BlendTileData", 2}});
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_2, v2_payload(1, 1, {42}));
        {
                BtParts p;
                p.tile = {3};
                p.blend = {0};
                p.extra = {0};
                p.cliff = {0};
                p.cliffState = {0x80};
                p.nbmp = 1;
                p.nblend = 1;
                p.ncliff = 1;
                put_chunk(b, 2, K_BLEND_TILE_VERSION_8, blend_payload(8, 1, p));
        }
        run_bt(out, id, b);
}

// texture class names: the empty name, the embedded NUL (visible content
// stops at it), a long name.
static void corpus_bt_names(Out &out)
{
        const char *id = "bt_names";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}, {"BlendTileData", 2}});
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_2, v2_payload(2, 2, formula_bytes(4, 65)));
        {
                BtParts p;
                p.tile = {0, 0, 0, 0};
                p.blend = {0, 0, 0, 0};
                p.extra = {0, 0, 0, 0};
                p.cliff = {0, 0, 0, 0};
                p.cliffState = {0, 0};
                p.nbmp = 1;
                p.nblend = 1;
                p.ncliff = 1;
                p.tex = {
                    {0, 1, 1, ""},
                    {1, 1, 1, std::string("ab\0cde", 6)},
                    {2, 1, 1, "AVeryLongTextureClassName001"},
                };
                p.nEdgeTiles = 2;
                p.etex = {{3, 1, 1, std::string("e\0f", 3)}};
                put_chunk(b, 2, K_BLEND_TILE_VERSION_8, blend_payload(8, 4, p));
        }
        run_bt(out, id, b);
}

// negative counts everywhere: the loops never run, the patch loop clamps
// every index to 0 (nothing is < a non-positive count bound), and the
// bitmap-tile count (bounded only by a debug assert) stores verbatim.
static void corpus_bt_counts_neg(Out &out)
{
        const char *id = "bt_counts_neg";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}, {"BlendTileData", 2}});
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_2, v2_payload(3, 2, formula_bytes(6, 67)));
        {
                BtParts p;
                p.tile = {5, 6, 7, 8, 9, 10};
                p.blend = {-1, 2, -3, 4, -5, 6};
                p.extra = {7, -8, 9, -10, 11, -12};
                p.cliff = {0, 1, 2, 3, 4, 5};
                p.cliffState = {0xff, 0x00};
                p.nbmp = 2000000000; // the assert is debug-only: stored verbatim
                p.nblend = -3;
                p.ncliff = -4;
                p.nTex = -1; // the word itself: -1
                p.nEdgeTiles = -7;
                p.nEdgeTex = -2;
                put_chunk(b, 2, K_BLEND_TILE_VERSION_8, blend_payload(8, 6, p));
        }
        run_bt(out, id, b);
}

// the patch loop's clamps: in-range indices survive, out-of-range and
// negative ones become 0 (the tile indices are never clamped).
static void corpus_bt_patch_clamps(Out &out)
{
        const char *id = "bt_patch_clamps";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}, {"BlendTileData", 2}});
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_3, v3_payload(5, 2, 0, formula_bytes(10, 69)));
        {
                BtParts p;
                p.tile = {100, 200, 300, 400, 500, 600, 700, 800, 900, 1000};
                p.blend = {0, 4, 5, -2, 1, 16192, -16192, 3, 2, 0};
                p.extra = {0, 0, 1, 99, 2, -5, 4, 4, 4, 4};
                p.cliff = {0, 1, 2, 99, -1, 2, 3, -32768, 32767, 5};
                p.cliffState = {0x0f, 0xf0};
                p.nbmp = 1;
                p.nblend = 5;
                p.ncliff = 3;
                put_chunk(b, 2, K_BLEND_TILE_VERSION_8, blend_payload(8, 10, p));
        }
        run_bt(out, id, b);
}

// a second HeightMapData after the blend chunk re-points the live
// m_dataSize: the patch loop walks only the new, smaller count and the
// blend arrays keep their raw tail beyond it.
static void corpus_bt_partial_patch(Out &out)
{
        const char *id = "bt_partial_patch";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}, {"BlendTileData", 2}});
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_2, v2_payload(4, 4, formula_bytes(16, 71)));
        {
                BtParts p;
                p.tile = formula_i16s(16, 73);
                p.blend = std::vector<int16_t>(16, 9);  // all out of range
                p.extra = std::vector<int16_t>(16, -9); // all negative
                p.cliff = std::vector<int16_t>(16, 19); // all out of range
                p.cliffState = formula_bytes(4, 83);    // fsw 1 * height 4
                p.nbmp = 1;
                p.nblend = 2;
                p.ncliff = 2;
                p.blends = {{1, 0, 0, 0, 0, 0, 0, 0, FLAG_VAL}};
                p.cliffs = {{1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0}};
                put_chunk(b, 2, K_BLEND_TILE_VERSION_8, blend_payload(8, 16, p));
        }
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_2, v2_payload(2, 2, formula_bytes(4, 85)));
        run_bt(out, id, b);
}

// the version-1 resize halves the shared m_dataSize, and a later
// version-8 blend chunk's len word must match the HALVED count — the
// len check reads the live member, not the height map's file extent.
static void corpus_bt_v1_then_v8(Out &out)
{
        const char *id = "bt_v1_then_v8";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}, {"BlendTileData", 2}});
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_1, v2_payload(4, 4, formula_bytes(16, 87)));
        {
                BtParts p;
                p.tile = formula_i16s(16, 89);
                p.blend = formula_i16s(16, 91);
                p.nbmp = 1;
                p.nblend = 2;
                p.blends = {{1, 0, 0, 0, 0, 0, 0, 0, FLAG_VAL}};
                put_chunk(b, 2, K_BLEND_TILE_VERSION_1, blend_payload(1, 16, p));
        }
        {
                BtParts p;
                p.tile = {41, 42, 43, 44};
                p.blend = {0, 1, 0, 1};
                p.extra = {2, 2, 2, 2};
                p.cliff = {1, 1, 1, 1};
                p.cliffState = formula_bytes(2, 93); // fsw 1 * height 2 (halved)
                p.nbmp = 2;
                p.nblend = 3;
                p.ncliff = 2;
                p.tex = {{0, 1, 1, "H8"}};
                p.blends = {
                    {2, 1, 0, 0, 0, 0x01, 1, 0, FLAG_VAL},
                    {3, 0, 1, 0, 0, 0x02, 0, 1, FLAG_VAL},
                };
                p.cliffs = {{77, 0.5f, 0.5f, 0.5f, 0.5f, 0.5f, 0.5f, 0.5f, 0.5f, 1, 0}};
                put_chunk(b, 2, K_BLEND_TILE_VERSION_8, blend_payload(8, 4, p));
        }
        run_bt(out, id, b);
}

// repeated BlendTileData chunks: the last one wins (its arrays replace
// the first's; its edge-count reset clears the first's edge classes when
// its version no longer reads them).
static void corpus_bt_repeat(Out &out)
{
        const char *id = "bt_repeat";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}, {"BlendTileData", 2}});
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_2, v2_payload(3, 2, formula_bytes(6, 95)));
        {
                BtParts p;
                p.tile = {1, 2, 3, 4, 5, 6};
                p.blend = {1, 1, 1, 1, 1, 1};
                p.extra = {2, 2, 2, 2, 2, 2};
                p.cliff = {3, 3, 3, 3, 3, 3};
                p.cliffState = {0x11, 0x22};
                p.nbmp = 2;
                p.nblend = 2;
                p.ncliff = 2;
                p.tex = {{0, 4, 2, "R8"}};
                p.nEdgeTiles = 8;
                p.etex = {{8, 4, 2, "RE"}};
                p.blends = {{1, 1, 1, 1, 1, 0x01, 1, 1, FLAG_VAL}};
                p.cliffs = {{11, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1}};
                put_chunk(b, 2, K_BLEND_TILE_VERSION_8, blend_payload(8, 6, p));
        }
        {
                BtParts p;
                p.tile = {9, 8, 7, 6, 5, 4};
                p.blend = {0, 2, 0, 2, 0, 2};
                p.nbmp = 1;
                p.nblend = 2;
                p.tex = {{0, 1, 1, "R3"}};
                p.blends = {{2, 3, 3, 3, 3, 0x01, 1, 0, FLAG_VAL}};
                put_chunk(b, 2, K_BLEND_TILE_VERSION_3, blend_payload(3, 6, p));
        }
        run_bt(out, id, b);
}

// a BlendTileData before any HeightMapData: the len word must equal the
// zero m_dataSize, so the arrays are empty — then the height map loads
// and the patch loop skips the missing tail (the original overreads its
// 0-length allocations).
static void corpus_bt_blend_first(Out &out)
{
        const char *id = "bt_blend_first";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}, {"BlendTileData", 2}});
        {
                BtParts p;
                p.nbmp = 2;
                p.nblend = 2;
                p.ncliff = 2;
                p.tex = {{0, 1, 1, "BF"}};
                p.blends = {{3, 1, 1, 1, 1, 0x01, 1, 1, FLAG_VAL}};
                p.cliffs = {{4, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1}};
                put_chunk(b, 2, K_BLEND_TILE_VERSION_8, blend_payload(8, 0, p));
        }
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_2, v2_payload(3, 2, formula_bytes(6, 97)));
        run_bt(out, id, b);
}

// a blend record's sentinel is not FLAG_VAL: the load refuses mid-chunk
// (the chunk stays open — the depth pin).
static void corpus_bt_badflag(Out &out)
{
        const char *id = "bt_badflag";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}, {"BlendTileData", 2}});
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_2, v2_payload(2, 2, formula_bytes(4, 99)));
        {
                BtParts p;
                p.tile = {1, 2, 3, 4};
                p.blend = {0, 0, 0, 0};
                p.extra = {0, 0, 0, 0};
                p.cliff = {0, 0, 0, 0};
                p.cliffState = {0, 0};
                p.nbmp = 1;
                p.nblend = 4;
                p.ncliff = 1;
                p.blends = {
                    {1, 0, 0, 0, 0, 0, 0, 0, FLAG_VAL},
                    {2, 0, 0, 0, 0, 0, 0, 0, FLAG_VAL},
                    {3, 0, 0, 0, 0, 0, 0, 0, 0x7ADA0001},
                    {4, 0, 0, 0, 0, 0, 0, 0, FLAG_VAL},
                };
                put_chunk(b, 2, K_BLEND_TILE_VERSION_8, blend_payload(8, 4, p));
        }
        run_bt(out, id, b);
}

// the file ends inside a blend record: the short reads zero-fill the
// fields and the sentinel reads 0 — the same refusal, pinned at the
// zero-value sentinel.
static void corpus_bt_trunc_blend(Out &out)
{
        const char *id = "bt_trunc_blend";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}, {"BlendTileData", 2}});
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_2, v2_payload(2, 2, formula_bytes(4, 101)));
        {
                BtParts p;
                p.tile = {1, 2, 3, 4};
                p.blend = {0, 0, 0, 0};
                p.extra = {0, 0, 0, 0};
                p.cliff = {0, 0, 0, 0};
                p.cliffState = {0, 0};
                p.nbmp = 1;
                p.nblend = 3;
                p.ncliff = 1;
                p.blends = {
                    {1, 0, 0, 0, 0, 0, 0, 0, FLAG_VAL},
                    {2, 0, 0, 0, 0, 0, 0, 0, FLAG_VAL}, // cut inside this record
                };
                std::vector<uint8_t> full = blend_payload(8, 4, p);
                // len 4 + 4x2 arrays (32) + cliffState 2 + nbmp/nblend/ncliff/ntex
                // (16) + 6 bytes into record 1
                std::vector<uint8_t> cut(full.begin(), full.begin() + 60);
                put_chunk(b, 2, K_BLEND_TILE_VERSION_8, cut, (int32_t)full.size());
        }
        run_bt(out, id, b);
}

// the file ends inside the tile index array: the tail zero-fills, every
// count downstream reads zero, and the load succeeds with empty classes
// and records.
static void corpus_bt_short_arrays(Out &out)
{
        const char *id = "bt_short_arrays";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}, {"BlendTileData", 2}});
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_2, v2_payload(4, 4, formula_bytes(16, 103)));
        {
                BtParts p;
                p.tile = formula_i16s(16, 105);
                p.blend = formula_i16s(16, 115);
                p.extra = formula_i16s(16, 125);
                p.cliff = formula_i16s(16, 135);
                p.cliffState = formula_bytes(4, 145);
                p.nbmp = 5;
                p.nblend = 5;
                p.ncliff = 5;
                p.tex = {{0, 1, 1, "SA"}};
                p.blends = {{1, 0, 0, 0, 0, 0, 0, 0, FLAG_VAL}};
                p.cliffs = {{1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0}};
                std::vector<uint8_t> full = blend_payload(8, 16, p);
                std::vector<uint8_t> cut(full.begin(), full.begin() + 39); // len 4 + tile 32 + 3
                put_chunk(b, 2, K_BLEND_TILE_VERSION_8, cut, (int32_t)full.size());
        }
        run_bt(out, id, b);
}

// the len word != the height map's data size: the first refusal, before
// any allocation.
static void corpus_bt_len_mismatch(Out &out)
{
        const char *id = "bt_len_mismatch";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}, {"BlendTileData", 2}});
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_2, v2_payload(4, 4, formula_bytes(16, 107)));
        {
                BtParts p;
                p.nbmp = 1;
                put_chunk(b, 2, K_BLEND_TILE_VERSION_8, blend_payload(8, 12, p));
        }
        run_bt(out, id, b);
}

// a bad HeightMapData before a good blend chunk: the walk stops at the
// height-map reader, the blend never fires.
static void corpus_bt_hm_err(Out &out)
{
        const char *id = "bt_hm_err";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}, {"BlendTileData", 2}});
        {
                std::vector<uint8_t> p;
                put_i32(p, 0); // width: refuses
                put_i32(p, 2);
                put_i32(p, 4);
                put_chunk(b, 1, K_HEIGHT_MAP_VERSION_2, p);
        }
        {
                BtParts p2;
                p2.nbmp = 1;
                put_chunk(b, 2, K_BLEND_TILE_VERSION_8, blend_payload(8, 4, p2));
        }
        run_bt(out, id, b);
}

// counts past the fixed array bounds: the in-object overflow death
// shapes, refused (one corpus per bound).
static void corpus_bt_nblend_over(Out &out)
{
        const char *id = "bt_nblend_over";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}, {"BlendTileData", 2}});
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_2, v2_payload(2, 2, formula_bytes(4, 109)));
        {
                BtParts p;
                p.tile = {0, 0, 0, 0};
                p.blend = {0, 0, 0, 0};
                p.extra = {0, 0, 0, 0};
                p.cliff = {0, 0, 0, 0};
                p.cliffState = {0, 0};
                p.nbmp = 1;
                p.nblend = NUM_BLEND_TILES + 1;
                put_chunk(b, 2, K_BLEND_TILE_VERSION_8, blend_payload(8, 4, p));
        }
        run_bt(out, id, b);
}

static void corpus_bt_ncliff_over(Out &out)
{
        const char *id = "bt_ncliff_over";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}, {"BlendTileData", 2}});
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_2, v2_payload(2, 2, formula_bytes(4, 111)));
        {
                BtParts p;
                p.tile = {0, 0, 0, 0};
                p.blend = {0, 0, 0, 0};
                p.extra = {0, 0, 0, 0};
                p.cliff = {0, 0, 0, 0};
                p.cliffState = {0, 0};
                p.nbmp = 1;
                p.nblend = 1;
                p.ncliff = NUM_CLIFF_INFO + 1;
                put_chunk(b, 2, K_BLEND_TILE_VERSION_8, blend_payload(8, 4, p));
        }
        run_bt(out, id, b);
}

static void corpus_bt_ntex_over(Out &out)
{
        const char *id = "bt_ntex_over";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}, {"BlendTileData", 2}});
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_2, v2_payload(2, 2, formula_bytes(4, 113)));
        {
                BtParts p;
                p.tile = {0, 0, 0, 0};
                p.blend = {0, 0, 0, 0};
                p.extra = {0, 0, 0, 0};
                p.cliff = {0, 0, 0, 0};
                p.cliffState = {0, 0};
                p.nbmp = 1;
                p.nblend = 1;
                p.ncliff = 1;
                p.nTex = NUM_TEXTURE_CLASSES + 1;
                put_chunk(b, 2, K_BLEND_TILE_VERSION_8, blend_payload(8, 4, p));
        }
        run_bt(out, id, b);
}

static void corpus_bt_netex_over(Out &out)
{
        const char *id = "bt_netex_over";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}, {"BlendTileData", 2}});
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_2, v2_payload(2, 2, formula_bytes(4, 115)));
        {
                BtParts p;
                p.tile = {0, 0, 0, 0};
                p.blend = {0, 0, 0, 0};
                p.extra = {0, 0, 0, 0};
                p.cliff = {0, 0, 0, 0};
                p.cliffState = {0, 0};
                p.nbmp = 1;
                p.nblend = 1;
                p.ncliff = 1;
                p.nEdgeTiles = 4;
                p.nEdgeTex = NUM_TEXTURE_CLASSES + 1;
                put_chunk(b, 2, K_BLEND_TILE_VERSION_8, blend_payload(8, 4, p));
        }
        run_bt(out, id, b);
}

// a height map with no blend chunk at all: the constructor's patch loop
// dereferences the null index arrays — the process death, refused.
static void corpus_bt_height_only(Out &out)
{
        const char *id = "bt_height_only";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}, {"GlobalLighting", 2}});
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_2, v2_payload(3, 2, formula_bytes(6, 117)));
        {
                std::vector<uint8_t> d(4, 0x33);
                put_chunk(b, 2, 2, d);
        }
        run_bt(out, id, b);
}

// a blend chunk with no height map: the len word must be 0, the arrays
// are empty, and the patch loop is a no-op (m_dataSize 0) — the load
// succeeds.
static void corpus_bt_no_height(Out &out)
{
        const char *id = "bt_no_height";
        std::vector<uint8_t> b = toc_bytes({{"BlendTileData", 1}});
        {
                BtParts p;
                p.nbmp = 2;
                p.nblend = 2;
                p.ncliff = 2;
                p.tex = {{0, 1, 1, "NH"}};
                p.blends = {{5, 1, 1, 1, 1, 0x01, 1, 1, FLAG_VAL}};
                p.cliffs = {{6, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1}};
                put_chunk(b, 1, K_BLEND_TILE_VERSION_8, blend_payload(8, 0, p));
        }
        run_bt(out, id, b);
}

// a map with neither payload: the walk succeeds, both halves are absent,
// and the patch loop is a no-op (m_dataSize 0).
static void corpus_bt_none(Out &out)
{
        const char *id = "bt_none";
        std::vector<uint8_t> b = toc_bytes({{"SidesList", 1}});
        {
                std::vector<uint8_t> d(4, 0x66);
                put_chunk(b, 1, 1, d);
        }
        run_bt(out, id, b);
}

// the under-declared blend chunk: the size word (8) is smaller than the
// reads, so dataLeft goes negative, the count words read out of the
// following chunk's header, and the no-seek close leaves the walk at
// EOF (the reads ran past the end).
static void corpus_bt_underdeclared(Out &out)
{
        const char *id = "bt_underdeclared";
        std::vector<uint8_t> b = toc_bytes(
            {{"HeightMapData", 1}, {"BlendTileData", 2}, {"UnknownPayload", 3}});
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_2, v2_payload(2, 2, formula_bytes(4, 119)));
        {
                std::vector<uint8_t> d;
                put_i32(d, 4);             // len
                put_i16s(d, {1, 2, 3, 4}); // tile
                put_i16s(d, {0, 0, 0, 0}); // blend
                put_chunk(b, 2, K_BLEND_TILE_VERSION_2, d, 8); // declares 8, reads 32+
        }
        {
                // the count words read this chunk's header bytes: nbmp reads the
                // id (3), nblend reads version+size (1), ntex reads the size
                // high half + the first data bytes (1) — so the declared size is
                // 0x10000 (its low half zero) and the data starts with zeros.
                std::vector<uint8_t> d(8, 0x00);
                put_chunk(b, 3, 1, d, 0x10000);
        }
        run_bt(out, id, b);
}

// leftovers after the blend payload (the reader's atEndOfChunk assert is
// a debug contract; the release close skips them and the walk resumes).
static void corpus_bt_leftover(Out &out)
{
        const char *id = "bt_leftover";
        std::vector<uint8_t> b = toc_bytes(
            {{"HeightMapData", 1}, {"BlendTileData", 2}, {"UnknownPayload", 3}});
        put_chunk(b, 1, K_HEIGHT_MAP_VERSION_2, v2_payload(2, 2, formula_bytes(4, 121)));
        {
                BtParts p;
                p.tile = {1, 2, 3, 4};
                p.blend = {0, 0, 0, 0};
                p.nbmp = 1;
                p.nblend = 1;
                std::vector<uint8_t> d = blend_payload(2, 4, p);
                put_bytes(d, std::vector<uint8_t>(6, 0xee)); // leftovers
                put_chunk(b, 2, K_BLEND_TILE_VERSION_2, d);
        }
        {
                std::vector<uint8_t> d(4, 0x22);
                put_chunk(b, 3, 1, d);
        }
        run_bt(out, id, b);
}

// the scope rule: a BlendTileData nested inside a container chunk never
// fires the global-scope reader (the container's own walk looks for the
// blend label at its scope, finds nothing, skips the children whole);
// the top-level one after it loads.
static void corpus_bt_scope(Out &out)
{
        const char *id = "bt_scope";
        std::vector<uint8_t> b =
            toc_bytes({{"ObjectsList", 1}, {"HeightMapData", 2}, {"BlendTileData", 3}});
        {
                std::vector<uint8_t> nested;
                {
                        // a blend chunk that would refuse if it ever fired
                        std::vector<uint8_t> d;
                        put_i32(d, 999); // len != any m_dataSize
                        put_chunk(nested, 3, K_BLEND_TILE_VERSION_8, d);
                }
                {
                        // a height-map chunk that would refuse if it ever fired
                        std::vector<uint8_t> obj;
                        put_i32(obj, 0); // width: refuses
                        put_i32(obj, 2);
                        put_i32(obj, 4);
                        put_chunk(nested, 2, 1, obj);
                }
                put_chunk(b, 1, 3, nested); // ObjectsList walks its children
        }
        put_chunk(b, 2, K_HEIGHT_MAP_VERSION_2, v2_payload(2, 2, formula_bytes(4, 123)));
        {
                BtParts p;
                p.tile = {5, 6, 7, 8};
                p.blend = {0, 0, 0, 0};
                p.nbmp = 1;
                p.nblend = 1;
                put_chunk(b, 3, K_BLEND_TILE_VERSION_2, blend_payload(2, 4, p));
        }
        run_bt(out, id, b, true);
}

// not a chunky file at all.
static void corpus_bt_not_chunky(Out &out)
{
        const char *id = "bt_not_chunky";
        std::vector<uint8_t> b;
        put_str(b, "not a map at all");
        run_bt(out, id, b);
}

// a CkMp table of contents that ends at EOF never opens (the container's
// refusal — the walk fails before any parser).
static void corpus_bt_toc_only(Out &out)
{
        const char *id = "bt_toc_only";
        std::vector<uint8_t> b = toc_bytes({{"HeightMapData", 1}});
        run_bt(out, id, b);
}

// empty file.
static void corpus_bt_empty(Out &out)
{
        const char *id = "bt_empty";
        std::vector<uint8_t> b;
        run_bt(out, id, b);
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
        fprintf(f, "# gen_mapblendtile_golden.cpp — WorldHeightMap.cpp BlendTileData "
                   "transcription (baseline 571a7c5).\n");
        fprintf(f, "# Golden script for rzh-formats::map_blendtile; see "
                   "tools/differential/README.md.\n");
        fprintf(f, "# Grammar: file/valid/walk/hm/bnd/hdat/hnone/bt/traw/braw/xraw/craw/"
                   "flip/cst/tex/etex/bti/cfi/bnone/tndx/bndx/xndx/cndx/herr/bstate.\n");

        corpus_bt_v8(out);
        corpus_bt_3way(out);
        corpus_bt_v7(out);
        corpus_bt_v7b(out);
        corpus_bt_v7c(out);
        corpus_bt_v6(out);
        corpus_bt_v5(out);
        corpus_bt_v4(out);
        corpus_bt_v3(out);
        corpus_bt_v2(out);
        corpus_bt_v0(out);
        corpus_bt_v1(out);
        corpus_bt_v9(out);
        corpus_bt_minimal(out);
        corpus_bt_names(out);
        corpus_bt_counts_neg(out);
        corpus_bt_patch_clamps(out);
        corpus_bt_partial_patch(out);
        corpus_bt_v1_then_v8(out);
        corpus_bt_repeat(out);
        corpus_bt_blend_first(out);
        corpus_bt_badflag(out);
        corpus_bt_trunc_blend(out);
        corpus_bt_short_arrays(out);
        corpus_bt_len_mismatch(out);
        corpus_bt_hm_err(out);
        corpus_bt_nblend_over(out);
        corpus_bt_ncliff_over(out);
        corpus_bt_ntex_over(out);
        corpus_bt_netex_over(out);
        corpus_bt_height_only(out);
        corpus_bt_no_height(out);
        corpus_bt_none(out);
        corpus_bt_underdeclared(out);
        corpus_bt_leftover(out);
        corpus_bt_scope(out);
        corpus_bt_not_chunky(out);
        corpus_bt_toc_only(out);
        corpus_bt_empty(out);

        fclose(f);
        return 0;
}
