// SPDX-License-Identifier: GPL-3.0-or-later
//
// Golden-vector driver for `rzh-formats::map_chunk` (parity level P0).
//
// Verbatim transcription of the baseline tree's map-data chunk container —
// GeneralsMD/Code/GameEngine/Source/Common/System/DataChunk.{h,cpp} at
// commit 571a7c5 — the DataChunkTableOfContents/DataChunkInput/
// DataChunkOutput trio every .map payload (height maps, blend tiles,
// global lighting, world info, object lists, polygon triggers, sides
// lists) is stored in, plus the load-path collaborators the read and
// write primitives drive:
//
//   - the CachedFileInputStream byte stream (MapReaderWriterInfo.h /
//     DataChunk.cpp) — its clamped reads, its tell/absoluteSeek/eof
//     semantics;
//   - the load-path half of Dict (Dict.{h,cpp}) — findPairByKey's
//     binary search, setPrep's update-or-append and the shellsort that
//     keeps the pairs ordered by name key;
//   - the name-key interning of NameKeyGenerator.{h,cpp} — calcHashFor
//     String's *33 hash, the sequential first-seen ids, keyToName's
//     reverse scan.
//
// Deliberate deviations, pinned identically on both sides:
//
// - The stream's `open()` (which transparently RefPack-decompresses the
//   file through CompressionManager) is replaced by direct construction
//   from bytes: the decompression step is rzh-compress's job (the RefPack
//   codec is already ported and golden-pinned there); this layer sees the
//   decompressed bytes, exactly what CachedFileInputStream hands the
//   chunk reader after its decompress branch.
// - WideChar is pinned to 2 bytes (uint16_t), the width of the game's
//   32-bit Windows build (unichar == wchar_t == 16-bit there). A 4-byte
//   unichar build would double every unicode string on disk; the wire
//   format is the Windows one.
// - Reads that arrive short (file ends mid-field) leave the tail of the
//   destination zero here and in the port; the original leaves it
//   indeterminate (uninitialized locals). The zero choice is pinned on
//   both sides so truncated-header corpora stay deterministic.
// - The writer's temporary FILE* dance (fopen "_tmpChunk.dat", patch the
//   size field with fseek, append in 256-byte pieces on destruction) is
//   an in-memory byte buffer with the same patch-in-place semantics; the
//   dtor's "TOC first, then the data" concatenation is an explicit
//   finish() call. The user-data-directory side effect of the temp file
//   name is dropped (it is not on-disk behaviour).
// - Corpora never leave a chunk open on the writer (the original would
//   silently ship the 0xffff size placeholder; the port refuses, the
//   same decision the W3D chunk container made).
// - The DEBUG_ASSERTCRASH contracts (read past end of chunk, unexpected
//   extra data in chunk) are release no-ops here and errors-or-documented
//   behaviour in the port; the release behaviour (the read proceeds, the
//   parse loop breaks) is what the corpora pin.
//
// Output grammar (one command per line, hex lowercase, '-' = empty):
//
//   file <id> <hex>               the corpus bytes (hand-assembled or
//                                 read back from a writer corpus)
//   valid <id> <t|f>              isValidFileType() after construction
//   rtoc <id> <n>                 the TOC list length after construction
//   rtoce <id> <idx> <name-hex> <id>  the TOC entries, head (most
//                                 recently prepended) first
//   ok <id> <t|f>                 a parse() verdict
//   state <id> <tell> <depth> <eof>  the post-parse stream state (depth
//                                 counts the chunks left on the stack)
//   step <id> <op> <args...>      scripted observations (manual walks and
//                                 parser callbacks):
//       open <label-hex|-> <ver> <dsize> <dleft> <depth>
//       close <tell> <depth>
//       ri <int>   rr <f32bits-hex>   rb <byte>
//       ra <hex|->   ru <hex|->   rk <ordinal>   rby <hex|->
//       rd <npairs>  rdp <idx> <ordinal> <type> <value-hex>
//       dlab <hex|->   dver <n>   dsize <n>   dleft <n>
//       eoc <t|f>   eof <t|f>   tell <n>   depth <n>   valid <t|f>
//       err <kind>
//   wgen <id> <hex>               the transcribed writer's bytes
//   wtoc <id> <n>                 the writer's TOC list length
//   wtoce <id> <idx> <name-hex> <id>  the writer's TOC entries, head
//                                 first (reverse allocation order)
//   wdepth <id> <n>               the writer's deepest open-chunk count
//
// dsize/dleft are SIGNED decimals (negative dataLeft — the over-declared
// child accounting — is the point of several corpora). Dict values are
// hex: bool one byte 00/01, int/real four bytes, ascii/unicode the raw
// (NUL-truncated) content bytes / UTF-16LE units.
//
// Usage: gen_mapchunk_golden <output-file>
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

// ---------------------------------------------------------------------------
// the temporary file the writer patches in place (replaces FILE* + fseek)
// ---------------------------------------------------------------------------

struct TempFile {
        std::vector<uint8_t> b;
        size_t pos = 0;

        size_t fwrite(const void *p, size_t n)
        {
                if (pos + n > b.size())
                        b.resize(pos + n);
                memcpy(b.data() + pos, p, n);
                pos += n;
                return n;
        }
        size_t ftell() const { return pos; }
        void fseek(size_t where) { pos = where; }   // SEEK_SET only
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
// DataChunkOutput (DataChunk.h / DataChunk.cpp)
// ---------------------------------------------------------------------------

struct OutputChunk {
        OutputChunk *next = nullptr;
        UnsignedInt id = 0;      // chunk symbol type from table of contents
        Int filepos = 0;         // position of file at start of data offset
};

class DataChunkOutput {
public:
        explicit DataChunkOutput(ByteSink *pOut)
                : m_pOut(pOut)
        {
                m_chunkStack = nullptr;
        }

        ~DataChunkOutput() { finish(); }

        // The original's destructor: write the TOC, then append the temp
        // file's contents (in 256-byte pieces there; verbatim here).
        // Idempotent so the corpus driver can finish explicitly and still
        // destroy the writer (the original destroys exactly once).
        void finish()
        {
                if (m_finished)
                        return;
                m_finished = true;

                // store the table of contents
                m_contents.write(*m_pOut);

                // append the temp file contents
                m_pOut->write(m_tmp_file.b.data(), (Int)m_tmp_file.b.size());
        }

        void openDataChunk(const char *name, DataChunkVersionTypeT ver)
        {
                // allocate (or get existing) ID from the table of contents
                UnsignedInt id = m_contents.allocateID(AsciiString(name));

                // allocate a new chunk and place it on top of the chunk stack
                OutputChunk *c = new OutputChunk;
                c->next = m_chunkStack;
                m_chunkStack = c;
                m_chunkStack->id = id;

                // store the chunk ID
                m_tmp_file.fwrite(&id, sizeof(UnsignedInt));

                // store the chunk version number
                m_tmp_file.fwrite(&ver, sizeof(DataChunkVersionTypeT));

                // remember this file position so we can write the real data size later
                c->filepos = (Int)m_tmp_file.ftell();

                // store a placeholder for the data size
                Int dummy = 0xffff;
                m_tmp_file.fwrite(&dummy, sizeof(Int));
        }

        void closeDataChunk()
        {
                if (m_chunkStack == nullptr) {
                        // TODO: Throw exception
                        return;
                }

                // remember where we are
                Int here = (Int)m_tmp_file.ftell();

                // rewind to store the data size
                m_tmp_file.fseek((size_t)m_chunkStack->filepos);

                // compute data size (not including the actual data size itself)
                Int size = here - m_chunkStack->filepos - sizeof(Int);

                // store the data size
                m_tmp_file.fwrite(&size, sizeof(Int));

                // go back to where we were
                m_tmp_file.fseek((size_t)here);

                // pop the chunk off the stack
                OutputChunk *c = m_chunkStack;
                m_chunkStack = m_chunkStack->next;
                delete c;
        }

        void writeReal(Real r) { m_tmp_file.fwrite(&r, sizeof(float)); }
        void writeInt(Int i) { m_tmp_file.fwrite(&i, sizeof(Int)); }
        void writeByte(Byte b) { m_tmp_file.fwrite(&b, sizeof(Byte)); }
        void writeArrayOfBytes(const char *ptr, Int len)
        {
                m_tmp_file.fwrite(ptr, (size_t)len);
        }

        void writeAsciiString(const AsciiString &theString)
        {
                UnsignedShort len = (UnsignedShort)theString.getLength();
                m_tmp_file.fwrite(&len, sizeof(UnsignedShort));
                m_tmp_file.fwrite(theString.str(), (size_t)len);
        }

        void writeUnicodeString(const UnicodeString &theString)
        {
                UnsignedShort len = (UnsignedShort)theString.getLength();
                m_tmp_file.fwrite(&len, sizeof(UnsignedShort));
                m_tmp_file.fwrite(theString.str(), (size_t)len * sizeof(WideChar));
        }

        void writeNameKey(NameKeyGenerator &keys, const NameKeyType key)
        {
                AsciiString kname = keys.keyToName(key);
                Int keyAndType = (Int)m_contents.allocateID(kname);
                keyAndType <<= 8;
                Dict::DataType t = Dict::DICT_ASCIISTRING;
                keyAndType |= (t & 0xff);
                writeInt(keyAndType);
        }

        void writeDict(NameKeyGenerator &keys, const Dict &d)
        {
                UnsignedShort len = (UnsignedShort)d.getPairCount();
                m_tmp_file.fwrite(&len, sizeof(UnsignedShort));
                for (int i = 0; i < (int)len; i++) {
                        NameKeyType k = d.getNthKey(i);
                        AsciiString kname = keys.keyToName(k);

                        Int keyAndType = (Int)m_contents.allocateID(kname);
                        keyAndType <<= 8;
                        Dict::DataType t = d.getNthType(i);
                        keyAndType |= (t & 0xff);
                        writeInt(keyAndType);

                        switch (t) {
                                case Dict::DICT_BOOL:
                                        writeByte(d.getNthBool(i) ? 1 : 0);
                                        break;
                                case Dict::DICT_INT:
                                        writeInt(d.getNthInt(i));
                                        break;
                                case Dict::DICT_REAL:
                                        writeReal(d.getNthReal(i));
                                        break;
                                case Dict::DICT_ASCIISTRING:
                                        writeAsciiString(d.getNthAsciiString(i));
                                        break;
                                case Dict::DICT_UNICODESTRING:
                                        writeUnicodeString(d.getNthUnicodeString(i));
                                        break;
                                default:
                                        // DEBUG_CRASH(("impossible"));
                                        break;
                        }
                }
        }

        // --- transcription-only introspection ---
        DataChunkTableOfContents &contents() { return m_contents; }
        Int depth() const
        {
                Int n = 0;
                for (OutputChunk *c = m_chunkStack; c; c = c->next)
                        n++;
                return n;
        }

private:
        ByteSink *m_pOut;                    // the actual output stream
        TempFile m_tmp_file;                 // tmp output file stream
        DataChunkTableOfContents m_contents; // table of contents of chunk types
        OutputChunk *m_chunkStack;           // current stack of open data chunks
        Bool m_finished = false;             // (see finish)
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
        void step(const char *id, const std::string &s)
        {
                line("step " + std::string(id) + " " + s);
        }
        void state(const char *id, DataChunkInput &in)
        {
                line("state " + std::string(id) + " " + std::to_string((Int)in.tell()) + " " +
                     std::to_string(in.depth()) + " " + (in.atEndOfFile() ? "t" : "f"));
        }
        void toc(const char *tag, const char *id, DataChunkTableOfContents &toc)
        {
                line(std::string(tag) + " " + id + " " + std::to_string(toc.listLength()));
                Int i = 0;
                for (Mapping *m = toc.head(); m; m = m->next, ++i)
                        line(std::string(tag) + "e " + id + " " + std::to_string(i) + " " +
                             hxs(m->name.s) + " " + std::to_string(m->id));
        }
        // (the state() tell below — kept out of the struct above to stay
        // near its users; the port's tell() is the same view)

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
static void put_wide(std::vector<uint8_t> &b, const std::vector<WideChar> &u)
{
        for (WideChar w : u)
                put_u16(b, w);
}

// one chunk: id, version, data size, data. The size word defaults to the
// data length but can lie (the over-declared corpora).
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

// --- the recorder the scripted parsers write into --------------------------

struct Rec {
        Out *out = nullptr;
        const char *id = "";
        NameKeyGenerator *keys = nullptr;
        int n = 0;   // a per-parser call counter for scripted behaviour
};

// step helpers shared by the callbacks and the manual walks.
static void rec_info(Rec &r, DataChunkInfo &info)
{
        r.out->step(r.id, "info " + Out::hxs(info.label.s) + " " + Out::hxs(info.parentLabel.s) +
                              " " + std::to_string(info.version) + " " +
                              std::to_string(info.dataSize));
}

static void rec_open(Rec &r, DataChunkInput &in, const AsciiString &label)
{
        r.out->step(r.id, "open " + Out::hxs(label.s) + " " +
                              std::to_string((Int)in.getChunkVersion()) + " " +
                              std::to_string((Int)in.getChunkDataSize()) + " " +
                              std::to_string((Int)in.getChunkDataSizeLeft()) + " " +
                              std::to_string(in.depth()));
}

// dump a dict's pairs in stored (name-key sorted) order.
static void rec_dict(Rec &r, const Dict &d)
{
        r.out->step(r.id, "rd " + std::to_string(d.getPairCount()));
        for (Int i = 0; i < d.getPairCount(); ++i) {
                std::string v;
                switch (d.getNthType(i)) {
                        case Dict::DICT_BOOL:
                                v = d.getNthBool(i) ? "01" : "00";
                                break;
                        case Dict::DICT_INT: {
                                std::vector<uint8_t> b;
                                put_i32(b, d.getNthInt(i));
                                v = hx(b);
                                break;
                        }
                        case Dict::DICT_REAL: {
                                std::vector<uint8_t> b;
                                put_f32(b, d.getNthReal(i));
                                v = hx(b);
                                break;
                        }
                        case Dict::DICT_ASCIISTRING: {
                                AsciiString s = d.getNthAsciiString(i);
                                v = hx(std::vector<uint8_t>(s.s.begin(), s.s.end()));
                                break;
                        }
                        case Dict::DICT_UNICODESTRING: {
                                UnicodeString u = d.getNthUnicodeString(i);
                                std::vector<uint8_t> b;
                                put_wide(b, u.u);
                                v = hx(b);
                                break;
                        }
                        default:
                                v = "?";
                                break;
                }
                r.out->step(r.id, "rdp " + std::to_string(i) + " " +
                                      std::to_string(d.getNthKey(i)) + " " +
                                      std::to_string((Int)d.getNthType(i)) + " " + v);
        }
}

// --- the scripted parser callbacks ------------------------------------------

// records the chunk info, nothing else (the "leaf" observer)
static Bool p_leaf(DataChunkInput &file, DataChunkInfo *info, void *userData)
{
        (void)file;
        Rec &r = *(Rec *)userData;
        rec_info(r, *info);
        return true;
}

// records the info, then recursively parses the children (the container
// observer — the real map loaders' shape)
static Bool p_rec(DataChunkInput &file, DataChunkInfo *info, void *userData)
{
        Rec &r = *(Rec *)userData;
        rec_info(r, *info);
        return file.parse(userData);
}

// records the info, then returns false (aborting the walk)
static Bool p_abort(DataChunkInput &file, DataChunkInfo *info, void *userData)
{
        (void)file;
        Rec &r = *(Rec *)userData;
        rec_info(r, *info);
        return false;
}

// records the info, opens one sub-chunk and returns WITHOUT closing it
// (the deliberate stack drift)
static Bool p_drift(DataChunkInput &file, DataChunkInfo *info, void *userData)
{
        Rec &r = *(Rec *)userData;
        rec_info(r, *info);
        DataChunkVersionTypeT ver = 0;
        AsciiString label = file.openDataChunk(&ver);
        rec_open(r, file, label);
        return true;
}

// records the info plus a fixed tag (which parser won the match)
static Bool p_tag1(DataChunkInput &file, DataChunkInfo *info, void *userData)
{
        (void)file;
        Rec &r = *(Rec *)userData;
        rec_info(r, *info);
        r.out->step(r.id, "tag 1");
        return true;
}
static Bool p_tag2(DataChunkInput &file, DataChunkInfo *info, void *userData)
{
        (void)file;
        Rec &r = *(Rec *)userData;
        rec_info(r, *info);
        r.out->step(r.id, "tag 2");
        return true;
}

// --- the rd_modern payload parsers (the map-load shape) ---------------------

static Bool p_height(DataChunkInput &file, DataChunkInfo *info, void *userData)
{
        Rec &r = *(Rec *)userData;
        rec_info(r, *info);
        r.out->step(r.id, "ri " + std::to_string(file.readInt()));
        r.out->step(r.id, "ri " + std::to_string(file.readInt()));
        char buf[16];
        file.readArrayOfBytes(buf, 16);
        r.out->step(r.id, "rby " + hx(std::vector<uint8_t>(buf, buf + 16)));
        r.out->step(r.id, "ri " + std::to_string(file.readInt()));
        return true;
}

static Bool p_blend(DataChunkInput &file, DataChunkInfo *info, void *userData)
{
        Rec &r = *(Rec *)userData;
        rec_info(r, *info);
        r.out->step(r.id, "ri " + std::to_string(file.readInt()));
        char buf[12];
        file.readArrayOfBytes(buf, 12);
        r.out->step(r.id, "rby " + hx(std::vector<uint8_t>(buf, buf + 12)));
        return true;
}

static Bool p_dict_only(DataChunkInput &file, DataChunkInfo *info, void *userData)
{
        Rec &r = *(Rec *)userData;
        rec_info(r, *info);
        rec_dict(r, file.readDict(*r.keys));
        return true;
}

static Bool p_object(DataChunkInput &file, DataChunkInfo *info, void *userData)
{
        Rec &r = *(Rec *)userData;
        rec_info(r, *info);
        if (r.n++ == 0) {
                r.out->step(r.id, "rk " + std::to_string(file.readNameKey(*r.keys)));
                rec_dict(r, file.readDict(*r.keys));
                AsciiString s = file.readAsciiString();
                r.out->step(r.id, "ra " + Out::hxs(s.s));
        } else {
                r.out->step(r.id, "rk " + std::to_string(file.readNameKey(*r.keys)));
                UnicodeString u = file.readUnicodeString();
                std::vector<uint8_t> wb;
                put_wide(wb, u.u);
                r.out->step(r.id, "ru " + hx(wb));
        }
        return true;
}

static Bool p_way(DataChunkInput &file, DataChunkInfo *info, void *userData)
{
        Rec &r = *(Rec *)userData;
        rec_info(r, *info);
        r.out->step(r.id, "ri " + std::to_string(file.readInt()));
        r.out->step(r.id, "ri " + std::to_string(file.readInt()));
        AsciiString s = file.readAsciiString();
        r.out->step(r.id, "ra " + Out::hxs(s.s));
        return true;
}

// --- the corpora -------------------------------------------------------------

// a formula byte string both sides rebuild identically
static std::string formula_bytes(size_t n, uint8_t seed)
{
        std::string s;
        s.resize(n);
        for (size_t i = 0; i < n; ++i)
                s[i] = (char)((seed + i * 7) & 0xFF);
        return s;
}

// rd_modern: the full map-load shape — TOC, every primitive, nested object
// lists, dicts whose read order reorders on the pre-seeded key generator,
// name keys, an unknown skipped chunk and the trailing zero-size header.
static void corpus_rd_modern(Out &out)
{
        const char *id = "rd_modern";
        std::vector<uint8_t> b = toc_bytes({
                {"HeightMapData", 1}, {"BlendTileData", 2}, {"GlobalLighting", 3},
                {"WorldInfo", 4},      {"ObjectsList", 5},    {"MapObject", 6},
                {"Waypoint", 7},       {"Ambient", 8},        {"Sun", 9},
                {"Name", 10},          {"WidthTiles", 11},    {"IsMultiplayer", 12},
                {"ObjectFlags", 13},
        });
        {
                std::vector<uint8_t> d;
                put_i32(d, 64);
                put_i32(d, 64);
                put_str(d, formula_bytes(16, 3));
                put_i32(d, 5);
                put_chunk(b, 1, 3, d);
        }
        {
                std::vector<uint8_t> d;
                put_i32(d, 512);
                put_str(d, formula_bytes(12, 9));
                put_chunk(b, 2, 7, d);
        }
        {
                std::vector<uint8_t> d;
                put_u16(d, 2);
                put_i32(d, (8 << 8) | 2);   // Ambient, REAL
                put_f32(d, 0.25f);
                put_i32(d, (9 << 8) | 3);   // Sun, ASCII
                put_u16(d, 4);
                put_str(d, "Warm");
                put_chunk(b, 3, 2, d);
        }
        {
                std::vector<uint8_t> d;
                put_u16(d, 3);
                put_i32(d, (10 << 8) | 3);  // Name, ASCII
                put_u16(d, 8);
                put_str(d, "Test Map");
                put_i32(d, (11 << 8) | 1);  // WidthTiles, INT
                put_i32(d, 64);
                put_i32(d, (12 << 8) | 0);  // IsMultiplayer, BOOL
                d.push_back(1);
                put_chunk(b, 4, 1, d);
        }
        {
                std::vector<uint8_t> obj1;
                put_i32(obj1, (10 << 8) | 3);   // name key "Name"
                put_u16(obj1, 1);
                put_i32(obj1, (13 << 8) | 1);   // ObjectFlags, INT
                put_i32(obj1, 77);
                put_u16(obj1, 5);
                put_str(obj1, std::string("AB\0CD", 5));   // the embedded NUL: visible "AB"
                std::vector<uint8_t> obj2;
                put_i32(obj2, (9 << 8) | 3);    // name key "Sun"
                put_u16(obj2, 3);
                put_wide(obj2, {0x0058, 0x0000, 0x0059});   // visible "X"
                std::vector<uint8_t> objs;
                put_chunk(objs, 6, 1, obj1);
                put_chunk(objs, 6, 1, obj2);
                put_chunk(b, 5, 3, objs);
        }
        {
                std::vector<uint8_t> d;
                put_i32(d, 10);
                put_i32(d, 20);
                put_u16(d, 3);
                put_str(d, "WP1");
                put_chunk(b, 7, 1, d);
        }
        {
                std::vector<uint8_t> d;
                put_str(d, formula_bytes(8, 21));
                put_chunk(b, 99, 1, d);   // unknown id: label "", no parser
        }
        put_u32(b, 6);   // the trailing zero-size MapObject header at EOF
        put_u16(b, 1);
        put_i32(b, 0);

        out.file(id, b);
        CachedFileInputStream s(b);
        DataChunkInput in(&s);
        out.tf("valid", id, in.isValidFileType());
        out.toc("rtoc", id, in.contents());
        NameKeyGenerator keys;
        keys.nameToKey("ObjectFlags");   // ordinal 1 — pre-seeded
        keys.nameToKey("Sun");           // ordinal 2 — pre-seeded
        Rec rec{&out, id, &keys, 0};
        in.registerParser("HeightMapData", "", p_height);
        in.registerParser("BlendTileData", "", p_blend);
        in.registerParser("GlobalLighting", "", p_dict_only);
        in.registerParser("WorldInfo", "", p_dict_only);
        in.registerParser("ObjectsList", "", p_rec);
        in.registerParser("MapObject", "ObjectsList", p_object);
        in.registerParser("Waypoint", "", p_way);
        out.tf("ok", id, in.parse(&rec));
        out.state(id, in);
}

// rd_scope: same label, different parents; the global-scope parser never
// fires inside a chunk.
static void corpus_rd_scope(Out &out)
{
        const char *id = "rd_scope";
        std::vector<uint8_t> b = toc_bytes({{"A", 1}, {"B", 2}, {"X", 3}, {"Y", 4}});
        {
                std::vector<uint8_t> y;   // empty
                std::vector<uint8_t> x;
                put_chunk(x, 4, 3, y);
                std::vector<uint8_t> a;
                put_chunk(a, 3, 2, x);
                put_chunk(b, 1, 1, a);
        }
        {
                std::vector<uint8_t> d;
                put_str(d, formula_bytes(4, 5));
                std::vector<uint8_t> bb;
                put_chunk(bb, 3, 4, d);
                put_chunk(b, 2, 1, bb);
        }

        out.file(id, b);
        CachedFileInputStream s(b);
        DataChunkInput in(&s);
        out.tf("valid", id, in.isValidFileType());
        out.toc("rtoc", id, in.contents());
        NameKeyGenerator keys;
        Rec rec{&out, id, &keys, 0};
        in.registerParser("A", "", p_rec);
        in.registerParser("B", "", p_rec);
        in.registerParser("X", "A", p_rec);
        in.registerParser("X", "B", p_leaf);
        in.registerParser("Y", "", p_leaf);   // never fires: parents are A/X/B
        out.tf("ok", id, in.parse(&rec));
        out.state(id, in);
}

// rd_order: two parsers for the same label and scope — the last registered
// (prepended, so found first) wins.
static void corpus_rd_order(Out &out)
{
        const char *id = "rd_order";
        std::vector<uint8_t> b = toc_bytes({{"X", 1}});
        put_chunk(b, 1, 1, std::vector<uint8_t>{1, 2, 3, 4});
        put_chunk(b, 1, 2, std::vector<uint8_t>{5, 6, 7, 8, 9, 10, 11, 12});

        out.file(id, b);
        CachedFileInputStream s(b);
        DataChunkInput in(&s);
        out.tf("valid", id, in.isValidFileType());
        NameKeyGenerator keys;
        Rec rec{&out, id, &keys, 0};
        in.registerParser("X", "", p_tag1);
        in.registerParser("X", "", p_tag2);
        out.tf("ok", id, in.parse(&rec));
        out.state(id, in);
}

// rd_abort: a parser returning false stops the walk; reset makes it repeat.
static void corpus_rd_abort(Out &out)
{
        const char *id = "rd_abort";
        std::vector<uint8_t> b = toc_bytes({{"A", 1}, {"B", 2}});
        put_chunk(b, 1, 1, std::vector<uint8_t>{1, 2});
        put_chunk(b, 2, 1, std::vector<uint8_t>{3, 4, 5, 6});

        out.file(id, b);
        CachedFileInputStream s(b);
        DataChunkInput in(&s);
        out.tf("valid", id, in.isValidFileType());
        NameKeyGenerator keys;
        Rec rec{&out, id, &keys, 0};
        in.registerParser("A", "", p_abort);
        in.registerParser("B", "", p_leaf);
        out.tf("ok", id, in.parse(&rec));
        out.state(id, in);
        in.reset();
        out.step(id, "tell " + std::to_string((Int)in.tell()));
        out.tf("ok", id, in.parse(&rec));
        out.state(id, in);
}

// rd_drift: the parser opens a sub-chunk and returns without closing it;
// the loop's close pops the WRONG chunk, and the parent's exactly-4 bytes
// left become one more header read.
static void corpus_rd_drift(Out &out)
{
        const char *id = "rd_drift";
        std::vector<uint8_t> b = toc_bytes({{"A", 1}, {"B", 2}});
        {
                std::vector<uint8_t> a;
                put_chunk(a, 2, 2, std::vector<uint8_t>{7, 7, 7, 7});
                put_i32(a, 7);   // 4 trailing bytes inside A
                put_chunk(b, 1, 1, a);
        }

        out.file(id, b);
        CachedFileInputStream s(b);
        DataChunkInput in(&s);
        out.tf("valid", id, in.isValidFileType());
        NameKeyGenerator keys;
        Rec rec{&out, id, &keys, 0};
        in.registerParser("A", "", p_drift);
        out.tf("ok", id, in.parse(&rec));
        out.state(id, in);
}

// rd_leftover: the parent keeps 3 bytes after its children; the parse loop
// breaks on dataLeft < 4, the parent's close skips the rest.
static void corpus_rd_leftover(Out &out)
{
        const char *id = "rd_leftover";
        std::vector<uint8_t> b = toc_bytes({{"P", 1}, {"C", 2}});
        {
                std::vector<uint8_t> p;
                put_chunk(p, 2, 2, std::vector<uint8_t>{9, 9});
                p.push_back(1);
                p.push_back(2);
                p.push_back(3);   // 3 trailing bytes
                put_chunk(b, 1, 1, p);
        }

        out.file(id, b);
        CachedFileInputStream s(b);
        DataChunkInput in(&s);
        out.tf("valid", id, in.isValidFileType());
        NameKeyGenerator keys;
        Rec rec{&out, id, &keys, 0};
        in.registerParser("P", "", p_rec);
        in.registerParser("C", "P", p_leaf);
        out.tf("ok", id, in.parse(&rec));
        out.state(id, in);
}

// rd_nested: the manual walk — reads decrement every open chunk, opening
// past a parent's declared end drives its dataLeft negative, and a close
// with negative dataLeft does not seek.
static void corpus_rd_nested(Out &out)
{
        const char *id = "rd_nested";
        std::vector<uint8_t> b = toc_bytes({{"P", 1}, {"Q", 2}, {"R", 3}});
        {
                std::vector<uint8_t> p;
                put_chunk(p, 2, 2, std::vector<uint8_t>{0x11, 0x22, 0x33, 0x44});
                put_chunk(b, 1, 1, p);
        }
        {
                std::vector<uint8_t> d;
                put_str(d, formula_bytes(6, 31));
                put_chunk(b, 3, 3, d);
        }

        out.file(id, b);
        CachedFileInputStream s(b);
        DataChunkInput in(&s);
        out.tf("valid", id, in.isValidFileType());
        out.toc("rtoc", id, in.contents());
        NameKeyGenerator keys;
        Rec rec{&out, id, &keys, 0};

        DataChunkVersionTypeT ver = 0;
        AsciiString label = in.openDataChunk(&ver);
        rec_open(rec, in, label);
        out.step(id, "dlab " + Out::hxs(in.getChunkLabel().s));
        label = in.openDataChunk(&ver);
        rec_open(rec, in, label);
        out.step(id, "dlab " + Out::hxs(in.getChunkLabel().s));
        out.step(id, "ri " + std::to_string(in.readInt()));
        out.step(id, "eoc " + std::string(in.atEndOfChunk() ? "t" : "f"));
        out.step(id, "dlefts " + std::to_string(in.dataLeftTop()));
        in.closeDataChunk();
        out.step(id, "close " + std::to_string((Int)in.tell()) + " " + std::to_string(in.depth()));
        label = in.openDataChunk(&ver);
        rec_open(rec, in, label);
        out.step(id, "dlefts " + std::to_string(in.dataLeftTop()));   // P: -10
        {
                char buf[6];
                in.readArrayOfBytes(buf, 6);
                out.step(id, "rby " + hx(std::vector<uint8_t>(buf, buf + 6)));
        }
        out.step(id, "dlefts " + std::to_string(in.dataLeftTop()));
        in.closeDataChunk();
        out.step(id, "close " + std::to_string((Int)in.tell()) + " " + std::to_string(in.depth()));
        in.closeDataChunk();
        out.step(id, "close " + std::to_string((Int)in.tell()) + " " + std::to_string(in.depth()));
        out.step(id, "eof " + std::string(in.atEndOfFile() ? "t" : "f"));
        out.state(id, in);
}

// rd_overdeclared: a child whose declared size overruns the file; the
// close seek clamps to EOF and the parent's dataLeft goes negative.
static void corpus_rd_overdeclared(Out &out)
{
        const char *id = "rd_overdeclared";
        std::vector<uint8_t> b = toc_bytes({{"P", 1}, {"C", 2}});
        {
                std::vector<uint8_t> p;
                put_chunk(p, 2, 2, std::vector<uint8_t>{1, 2, 3, 4}, 0x10000);
                put_chunk(b, 1, 1, p);
        }

        out.file(id, b);
        CachedFileInputStream s(b);
        DataChunkInput in(&s);
        out.tf("valid", id, in.isValidFileType());
        DataChunkVersionTypeT ver = 0;
        NameKeyGenerator keys;
        Rec rec{&out, id, &keys, 0};
        in.openDataChunk(&ver);
        rec_open(rec, in, "P");
        AsciiString label = in.openDataChunk(&ver);
        rec_open(rec, in, label);
        in.closeDataChunk();
        out.step(id, "close " + std::to_string((Int)in.tell()) + " " + std::to_string(in.depth()));
        out.step(id, "dlefts " + std::to_string(in.dataLeftTop()));   // -65532
        out.step(id, "dleft " + std::to_string((Int)in.getChunkDataSizeLeft()));
        out.step(id, "eof " + std::string(in.atEndOfFile() ? "t" : "f"));
        in.closeDataChunk();
        out.step(id, "close " + std::to_string((Int)in.tell()) + " " + std::to_string(in.depth()));
        out.state(id, in);
}

// rd_seek_past_eof: the 0x7FFFFFFF declared size — the seek target is a
// huge u32 (no wrap yet), clamped to EOF; the parent underflows i32.
static void corpus_rd_seek_past_eof(Out &out)
{
        const char *id = "rd_seek_past_eof";
        std::vector<uint8_t> b = toc_bytes({{"P", 1}, {"C", 2}});
        {
                std::vector<uint8_t> p;
                put_chunk(p, 2, 2, std::vector<uint8_t>{1, 2, 3, 4}, 0x7FFFFFFF);
                put_chunk(b, 1, 1, p);
        }

        out.file(id, b);
        CachedFileInputStream s(b);
        DataChunkInput in(&s);
        out.tf("valid", id, in.isValidFileType());
        DataChunkVersionTypeT ver = 0;
        NameKeyGenerator keys;
        Rec rec{&out, id, &keys, 0};
        in.openDataChunk(&ver);
        AsciiString label = in.openDataChunk(&ver);
        rec_open(rec, in, label);
        in.closeDataChunk();
        out.step(id, "dlefts " + std::to_string(in.dataLeftTop()));   // -2147483643
        out.step(id, "eof " + std::string(in.atEndOfFile() ? "t" : "f"));
        out.state(id, in);
}

// rd_neg_size: a negative declared size counts as empty (no seek, the
// next chunk parses).
static void corpus_rd_neg_size(Out &out)
{
        const char *id = "rd_neg_size";
        std::vector<uint8_t> b = toc_bytes({{"A", 1}, {"B", 2}});
        put_chunk(b, 1, 1, std::vector<uint8_t>(), -1);
        put_chunk(b, 2, 2, std::vector<uint8_t>{8, 8, 8, 8});

        out.file(id, b);
        CachedFileInputStream s(b);
        DataChunkInput in(&s);
        out.tf("valid", id, in.isValidFileType());
        NameKeyGenerator keys;
        Rec rec{&out, id, &keys, 0};
        in.registerParser("A", "", p_leaf);
        in.registerParser("B", "", p_leaf);
        out.tf("ok", id, in.parse(&rec));
        out.state(id, in);
}

// rd_trunc_header: the file ends mid-header; openDataChunk still pushes
// (zero-filled), returns the empty label, and getChunkLabel still resolves.
static void corpus_rd_trunc_header(Out &out)
{
        const char *id = "rd_trunc_header";
        std::vector<uint8_t> b = toc_bytes({{"A", 1}});
        put_u32(b, 1);
        put_u16(b, 3);   // id + version, the size word is missing

        out.file(id, b);
        CachedFileInputStream s(b);
        DataChunkInput in(&s);
        out.tf("valid", id, in.isValidFileType());
        DataChunkVersionTypeT ver = 0;
        NameKeyGenerator keys;
        Rec rec{&out, id, &keys, 0};
        AsciiString label = in.openDataChunk(&ver);
        rec_open(rec, in, label);
        out.step(id, "dlab " + Out::hxs(in.getChunkLabel().s));
        out.step(id, "dver " + std::to_string((Int)in.getChunkVersion()));
        out.step(id, "dsize " + std::to_string((Int)in.getChunkDataSize()));
        in.closeDataChunk();
        out.step(id, "close " + std::to_string((Int)in.tell()) + " " + std::to_string(in.depth()));
        out.state(id, in);
}

// rd_header_at_eof: a full header declaring data but ending at EOF —
// open returns the empty label while dataLeft stays positive.
static void corpus_rd_header_at_eof(Out &out)
{
        const char *id = "rd_header_at_eof";
        std::vector<uint8_t> b = toc_bytes({{"A", 1}});
        put_u32(b, 1);
        put_u16(b, 1);
        put_i32(b, 5);   // no data follows

        out.file(id, b);
        CachedFileInputStream s(b);
        DataChunkInput in(&s);
        out.tf("valid", id, in.isValidFileType());
        DataChunkVersionTypeT ver = 0;
        NameKeyGenerator keys;
        Rec rec{&out, id, &keys, 0};
        AsciiString label = in.openDataChunk(&ver);
        rec_open(rec, in, label);
        out.step(id, "dlab " + Out::hxs(in.getChunkLabel().s));
        out.step(id, "eoc " + std::string(in.atEndOfChunk() ? "t" : "f"));
        in.closeDataChunk();
        out.step(id, "close " + std::to_string((Int)in.tell()) + " " + std::to_string(in.depth()));
        out.state(id, in);
}

// rd_trunc_string: the ascii length runs past EOF — the visible string is
// the available bytes, dataLeft goes negative by the missing count.
static void corpus_rd_trunc_string(Out &out)
{
        const char *id = "rd_trunc_string";
        // (the recorder is unused in this manual walk)
        std::vector<uint8_t> b = toc_bytes({{"A", 1}});
        {
                std::vector<uint8_t> d;
                put_u16(d, 10);
                put_str(d, "abc");
                put_chunk(b, 1, 1, d);
        }

        out.file(id, b);
        CachedFileInputStream s(b);
        DataChunkInput in(&s);
        out.tf("valid", id, in.isValidFileType());
        DataChunkVersionTypeT ver = 0;
        NameKeyGenerator keys;
        in.openDataChunk(&ver);
        AsciiString str = in.readAsciiString();
        out.step(id, "ra " + Out::hxs(str.s));
        out.step(id, "dlefts " + std::to_string(in.dataLeftTop()));   // -7
        out.step(id, "eoc " + std::string(in.atEndOfChunk() ? "t" : "f"));
        in.closeDataChunk();
        out.state(id, in);
}

// rd_str_nul: embedded NULs truncate the visible strings, not the reads.
static void corpus_rd_str_nul(Out &out)
{
        const char *id = "rd_str_nul";
        // (the recorder is unused in this manual walk)
        std::vector<uint8_t> b = toc_bytes({{"A", 1}});
        {
                std::vector<uint8_t> d;
                put_u16(d, 5);
                put_str(d, std::string("AB\0CD", 5));
                put_u16(d, 3);
                put_wide(d, {0x0058, 0x0000, 0x0059});
                put_chunk(b, 1, 1, d);
        }

        out.file(id, b);
        CachedFileInputStream s(b);
        DataChunkInput in(&s);
        out.tf("valid", id, in.isValidFileType());
        DataChunkVersionTypeT ver = 0;
        NameKeyGenerator keys;
        in.openDataChunk(&ver);
        AsciiString str = in.readAsciiString();
        out.step(id, "ra " + Out::hxs(str.s));   // "AB"
        UnicodeString u = in.readUnicodeString();
        std::vector<uint8_t> wb;
        put_wide(wb, u.u);
        out.step(id, "ru " + hx(wb));           // "X"
        out.step(id, "dlefts " + std::to_string(in.dataLeftTop()));
        out.step(id, "eoc " + std::string(in.atEndOfChunk() ? "t" : "f"));
        in.closeDataChunk();
        out.state(id, in);
}

// rd_dict_types: all five value types; the pre-seeded generator reorders
// the pairs against the file order.
static void corpus_rd_dict_types(Out &out)
{
        const char *id = "rd_dict_types";
        std::vector<uint8_t> b = toc_bytes({{"D", 1}, {"Ambient", 2}, {"Sun", 3}, {"Name", 4}});
        {
                std::vector<uint8_t> d;
                put_u16(d, 3);
                put_i32(d, (2 << 8) | 2);   // Ambient, REAL
                put_f32(d, 0.5f);
                put_i32(d, (3 << 8) | 3);   // Sun, ASCII
                put_u16(d, 4);
                put_str(d, "Warm");
                put_i32(d, (4 << 8) | 1);   // Name, INT
                put_i32(d, 7);
                put_chunk(b, 1, 1, d);
        }

        out.file(id, b);
        CachedFileInputStream s(b);
        DataChunkInput in(&s);
        out.tf("valid", id, in.isValidFileType());
        DataChunkVersionTypeT ver = 0;
        NameKeyGenerator keys;
        keys.nameToKey("Sun");   // ordinal 1, before Ambient's 2
        Rec rec{&out, id, &keys, 0};
        in.openDataChunk(&ver);
        rec_dict(rec, in.readDict(keys));
        out.step(id, "dlefts " + std::to_string(in.dataLeftTop()));
        in.closeDataChunk();
        out.state(id, in);
}

// rd_dict_dup: the same key twice in one dict — the second set replaces
// the first, type and all.
static void corpus_rd_dict_dup(Out &out)
{
        const char *id = "rd_dict_dup";
        std::vector<uint8_t> b = toc_bytes({{"D", 1}, {"K", 2}});
        {
                std::vector<uint8_t> d;
                put_u16(d, 2);
                put_i32(d, (2 << 8) | 1);   // K, INT
                put_i32(d, 5);
                put_i32(d, (2 << 8) | 2);   // K, REAL
                put_f32(d, 6.0f);
                put_chunk(b, 1, 1, d);
        }

        out.file(id, b);
        CachedFileInputStream s(b);
        DataChunkInput in(&s);
        out.tf("valid", id, in.isValidFileType());
        DataChunkVersionTypeT ver = 0;
        NameKeyGenerator keys;
        Rec rec{&out, id, &keys, 0};
        in.openDataChunk(&ver);
        rec_dict(rec, in.readDict(keys));
        in.closeDataChunk();
        out.state(id, in);
}

// rd_dict_badtype: an unknown pair type throws ERROR_CORRUPT_FILE_FORMAT
// out of readDict; the chunk stays open on the stack.
static void corpus_rd_dict_badtype(Out &out)
{
        const char *id = "rd_dict_badtype";
        // (the recorder is unused in this manual walk)
        std::vector<uint8_t> b = toc_bytes({{"D", 1}, {"K", 2}});
        {
                std::vector<uint8_t> d;
                put_u16(d, 1);
                put_i32(d, (2 << 8) | 7);   // K, type 7: impossible
                d.push_back(0);
                put_chunk(b, 1, 1, d);
        }

        out.file(id, b);
        CachedFileInputStream s(b);
        DataChunkInput in(&s);
        out.tf("valid", id, in.isValidFileType());
        DataChunkVersionTypeT ver = 0;
        NameKeyGenerator keys;
        in.openDataChunk(&ver);
        try {
                in.readDict(keys);
        } catch (int) {
                out.step(id, "err dicttype");
        }
        out.step(id, "dlefts " + std::to_string(in.dataLeftTop()));
        in.closeDataChunk();
        out.state(id, in);
}

// rd_dict_short: the declared pair count runs past EOF — the zero-filled
// keyAndType interns the empty name as a BOOL false pair.
static void corpus_rd_dict_short(Out &out)
{
        const char *id = "rd_dict_short";
        std::vector<uint8_t> b = toc_bytes({{"D", 1}, {"K", 2}});
        {
                std::vector<uint8_t> d;
                put_u16(d, 2);
                put_i32(d, (2 << 8) | 0);   // K, BOOL
                d.push_back(1);
                // the second pair's bytes are missing entirely
                put_chunk(b, 1, 1, d);
        }

        out.file(id, b);
        CachedFileInputStream s(b);
        DataChunkInput in(&s);
        out.tf("valid", id, in.isValidFileType());
        DataChunkVersionTypeT ver = 0;
        NameKeyGenerator keys;
        Rec rec{&out, id, &keys, 0};
        in.openDataChunk(&ver);
        rec_dict(rec, in.readDict(keys));
        out.step(id, "dlefts " + std::to_string(in.dataLeftTop()));
        in.closeDataChunk();
        out.state(id, in);
}

// rd_namekey: known, newly-interned and unknown ids; the interning side
// effects are observable through later direct lookups.
static void corpus_rd_namekey(Out &out)
{
        const char *id = "rd_namekey";
        // (the recorder is unused in this manual walk)
        std::vector<uint8_t> b = toc_bytes({{"A", 1}, {"Name", 2}, {"Other", 3}});
        {
                std::vector<uint8_t> d;
                put_i32(d, (2 << 8) | 3);   // Name
                put_i32(d, (3 << 8) | 3);   // Other
                put_i32(d, (99 << 8) | 3);  // unknown id: the empty name
                put_chunk(b, 1, 1, d);
        }

        out.file(id, b);
        CachedFileInputStream s(b);
        DataChunkInput in(&s);
        out.tf("valid", id, in.isValidFileType());
        DataChunkVersionTypeT ver = 0;
        NameKeyGenerator keys;
        keys.nameToKey("Name");   // ordinal 1, pre-seeded
        in.openDataChunk(&ver);
        out.step(id, "rk " + std::to_string(in.readNameKey(keys)));   // 1
        out.step(id, "rk " + std::to_string(in.readNameKey(keys)));   // 2 (Other)
        out.step(id, "rk " + std::to_string(in.readNameKey(keys)));   // 3 ("")
        out.step(id, "nk " + std::to_string(keys.nameToKey("Other")));  // 2 again
        out.step(id, "nk " + std::to_string(keys.nameToKey("")));       // 3 again
        out.step(id, "dlefts " + std::to_string(in.dataLeftTop()));
        in.closeDataChunk();
        out.state(id, in);
}

// rd_unknown_id: a chunk id missing from the TOC parses under the empty
// label; a parser registered for "" fires.
static void corpus_rd_unknown_id(Out &out)
{
        const char *id = "rd_unknown_id";
        std::vector<uint8_t> b = toc_bytes({{"A", 1}});
        put_chunk(b, 42, 1, std::vector<uint8_t>{1, 2, 3, 4});
        put_chunk(b, 1, 1, std::vector<uint8_t>{5, 6});

        out.file(id, b);
        CachedFileInputStream s(b);
        DataChunkInput in(&s);
        out.tf("valid", id, in.isValidFileType());
        NameKeyGenerator keys;
        Rec rec{&out, id, &keys, 0};
        in.registerParser("", "", p_leaf);
        in.registerParser("A", "", p_leaf);
        out.tf("ok", id, in.parse(&rec));
        out.state(id, in);
}

// rd_zero_entry: a TOC entry with an empty name maps its id to "" too.
static void corpus_rd_zero_entry(Out &out)
{
        const char *id = "rd_zero_entry";
        std::vector<uint8_t> b = toc_bytes({{"", 5}, {"A", 1}});
        put_chunk(b, 5, 2, std::vector<uint8_t>{7, 8, 9});

        out.file(id, b);
        CachedFileInputStream s(b);
        DataChunkInput in(&s);
        out.tf("valid", id, in.isValidFileType());
        out.toc("rtoc", id, in.contents());
        NameKeyGenerator keys;
        Rec rec{&out, id, &keys, 0};
        in.registerParser("", "", p_leaf);
        in.registerParser("A", "", p_leaf);
        out.tf("ok", id, in.parse(&rec));
        out.step(id, "gid " + Out::hxs("") + " " + std::to_string(in.contents().getID("")));
        out.step(id, "gid " + Out::hxs("A") + " " + std::to_string(in.contents().getID("A")));
        out.state(id, in);
}

// --- the TOC edge corpora ----------------------------------------------------

// a reader corpus that only pins construction + the parse refusal
static void corpus_reader_shallow(Out &out, const char *id, const std::vector<uint8_t> &b,
                                  Bool dump_toc)
{
        out.file(id, b);
        CachedFileInputStream s(b);
        DataChunkInput in(&s);
        out.tf("valid", id, in.isValidFileType());
        if (dump_toc)
                out.toc("rtoc", id, in.contents());
        NameKeyGenerator keys;
        Rec rec{&out, id, &keys, 0};
        out.tf("ok", id, in.parse(&rec));
        out.state(id, in);
}

// rd_toc_only_eof: the TOC ends exactly at EOF — count > 0 but eof means
// the header does not count as opened.
static void corpus_rd_toc_only_eof(Out &out)
{
        corpus_reader_shallow(out, "rd_toc_only_eof", toc_bytes({{"Alpha", 1}, {"Beta", 2}}), true);
}

// rd_toc_zero: count == 0 is never an opened header, trailing data or not.
static void corpus_rd_toc_zero(Out &out)
{
        std::vector<uint8_t> b;
        put_str(b, "CkMp");
        put_i32(b, 0);
        put_u32(b, 1);
        put_u16(b, 1);
        put_i32(b, 4);
        put_str(b, "junk");
        corpus_reader_shallow(out, "rd_toc_zero", b, true);
}

// rd_bad_magic: the tag mismatch consumes the tag bytes only; the manual
// open after reset reads from position 4.
static void corpus_rd_bad_magic(Out &out)
{
        const char *id = "rd_bad_magic";
        std::vector<uint8_t> b;
        put_str(b, "CkMq");
        put_u32(b, 1);
        put_u16(b, 1);
        put_i32(b, 4);
        put_str(b, "junk");

        out.file(id, b);
        CachedFileInputStream s(b);
        DataChunkInput in(&s);
        out.tf("valid", id, in.isValidFileType());
        out.toc("rtoc", id, in.contents());
        NameKeyGenerator keys;
        Rec rec{&out, id, &keys, 0};
        out.tf("ok", id, in.parse(&rec));
        out.step(id, "tell " + std::to_string((Int)in.tell()));
        out.step(id, "firstpos " + std::to_string(in.firstChunkPos()));
        in.reset();
        out.step(id, "tell " + std::to_string((Int)in.tell()));
        DataChunkVersionTypeT ver = 0;
        AsciiString label = in.openDataChunk(&ver);
        rec_open(rec, in, label);   // reads from 4: id 1, empty label (empty TOC)
        out.step(id, "dlab " + Out::hxs(in.getChunkLabel().s));
        in.closeDataChunk();
        out.state(id, in);
}

// rd_empty / rd_short_tag / rd_short_count: truncated headers.
static void corpus_rd_empty(Out &out)
{
        corpus_reader_shallow(out, "rd_empty", std::vector<uint8_t>(), false);
}
static void corpus_rd_short_tag(Out &out)
{
        std::vector<uint8_t> b;
        put_str(b, "Ck");
        corpus_reader_shallow(out, "rd_short_tag", b, false);
}
static void corpus_rd_short_count(Out &out)
{
        std::vector<uint8_t> b;
        put_str(b, "CkMp");
        b.push_back(3);   // one byte of the count word: partial read -> 3
        corpus_reader_shallow(out, "rd_short_count", b, true);
}

// rd_dup_names: the same name twice — the head of the list (the last read)
// wins lookups; both ids resolve to the name.
static void corpus_rd_dup_names(Out &out)
{
        const char *id = "rd_dup_names";
        std::vector<uint8_t> b = toc_bytes({{"A", 1}, {"A", 2}});
        put_chunk(b, 1, 1, std::vector<uint8_t>{1, 2});
        put_chunk(b, 2, 1, std::vector<uint8_t>{3, 4});

        out.file(id, b);
        CachedFileInputStream s(b);
        DataChunkInput in(&s);
        out.tf("valid", id, in.isValidFileType());
        out.toc("rtoc", id, in.contents());
        NameKeyGenerator keys;
        Rec rec{&out, id, &keys, 0};
        in.registerParser("A", "", p_leaf);
        out.tf("ok", id, in.parse(&rec));
        out.step(id, "gid " + Out::hxs("A") + " " + std::to_string(in.contents().getID("A")));
        out.state(id, in);
}

// rd_dup_ids: two names sharing one id — getName returns the head's name.
static void corpus_rd_dup_ids(Out &out)
{
        const char *id = "rd_dup_ids";
        std::vector<uint8_t> b = toc_bytes({{"A", 1}, {"B", 1}});
        put_chunk(b, 1, 1, std::vector<uint8_t>{1, 2});

        out.file(id, b);
        CachedFileInputStream s(b);
        DataChunkInput in(&s);
        out.tf("valid", id, in.isValidFileType());
        out.toc("rtoc", id, in.contents());
        NameKeyGenerator keys;
        Rec rec{&out, id, &keys, 0};
        in.registerParser("A", "", p_leaf);
        in.registerParser("B", "", p_leaf);
        out.tf("ok", id, in.parse(&rec));
        out.state(id, in);
}

// rd_nul_name: a TOC name with an embedded NUL — the visible name is the
// prefix, so it aliases the plain entry.
static void corpus_rd_nul_name(Out &out)
{
        const char *id = "rd_nul_name";
        std::vector<uint8_t> b;
        put_str(b, "CkMp");
        put_i32(b, 2);
        b.push_back(5);   // "AB\0CD" (id 1)
        b.push_back('A');
        b.push_back('B');
        b.push_back(0);
        b.push_back('C');
        b.push_back('D');
        put_u32(b, 1);
        b.push_back(2);   // "AB" (id 2)
        b.push_back('A');
        b.push_back('B');
        put_u32(b, 2);
        put_chunk(b, 1, 1, std::vector<uint8_t>{1, 2});
        put_chunk(b, 2, 1, std::vector<uint8_t>{3, 4});

        out.file(id, b);
        CachedFileInputStream s(b);
        DataChunkInput in(&s);
        out.tf("valid", id, in.isValidFileType());
        out.toc("rtoc", id, in.contents());
        NameKeyGenerator keys;
        Rec rec{&out, id, &keys, 0};
        in.registerParser("AB", "", p_leaf);
        out.tf("ok", id, in.parse(&rec));
        out.step(id, "gid " + Out::hxs("AB") + " " + std::to_string(in.contents().getID("AB")));
        out.state(id, in);
}

// rd_unsorted_ids: ids in arbitrary order resolve by value.
static void corpus_rd_unsorted_ids(Out &out)
{
        const char *id = "rd_unsorted_ids";
        std::vector<uint8_t> b = toc_bytes({{"P", 5}, {"Q", 3}, {"R", 9}});
        put_chunk(b, 5, 1, std::vector<uint8_t>{1});
        put_chunk(b, 3, 1, std::vector<uint8_t>{2});
        put_chunk(b, 9, 1, std::vector<uint8_t>{3});

        out.file(id, b);
        CachedFileInputStream s(b);
        DataChunkInput in(&s);
        out.tf("valid", id, in.isValidFileType());
        out.toc("rtoc", id, in.contents());
        NameKeyGenerator keys;
        Rec rec{&out, id, &keys, 0};
        in.registerParser("P", "", p_leaf);
        in.registerParser("Q", "", p_leaf);
        in.registerParser("R", "", p_leaf);
        out.tf("ok", id, in.parse(&rec));
        out.state(id, in);
}

// --- the writer corpora ------------------------------------------------------

// runs a writer corpus: writes the lines and replays the bytes through a
// fresh reader with info-only parsers (leaf for every label, recursive for
// the container labels).
static void run_writer(Out &out, const char *id, const std::vector<uint8_t> &bytes,
                       DataChunkOutput &wr, Int depth, Bool roundtrip,
                       const std::vector<std::pair<const char *, const char *>> &leaf_labels,
                       const std::vector<const char *> &container_labels)
{
        wr.finish();   // the dtor's TOC + data concatenation
        out.file(id, bytes);
        out.line("wgen " + std::string(id) + " " + hx(bytes));
        out.toc("wtoc", id, wr.contents());
        out.line("wdepth " + std::string(id) + " " + std::to_string(depth));

        if (!roundtrip)
                return;

        CachedFileInputStream s(bytes);
        DataChunkInput in(&s);
        out.tf("valid", id, in.isValidFileType());
        NameKeyGenerator keys;
        Rec rec{&out, id, &keys, 0};
        for (auto &lab : leaf_labels)
                in.registerParser(lab.first, lab.second, p_leaf);
        for (auto lab : container_labels)
                in.registerParser(lab, "", p_rec);
        out.tf("ok", id, in.parse(&rec));
        out.state(id, in);
}

// wr_modern: the full map shape through the writer.
static void corpus_wr_modern(Out &out)
{
        const char *id = "wr_modern";
        NameKeyGenerator keys;
        keys.nameToKey("ObjectFlags");
        keys.nameToKey("Sun");
        ByteSink sink;
        DataChunkOutput wr(&sink);

        wr.openDataChunk("HeightMapData", 3);
        wr.writeInt(64);
        wr.writeInt(64);
        wr.writeArrayOfBytes(formula_bytes(16, 3).data(), 16);
        wr.writeInt(5);
        wr.closeDataChunk();

        wr.openDataChunk("BlendTileData", 7);
        wr.writeInt(512);
        wr.writeArrayOfBytes(formula_bytes(12, 9).data(), 12);
        wr.closeDataChunk();

        wr.openDataChunk("GlobalLighting", 2);
        {
                Dict d;
                d.setReal(keys.nameToKey("Ambient"), 0.25f);
                d.setAsciiString(keys.nameToKey("Sun"), "Warm");
                wr.writeDict(keys, d);
        }
        wr.closeDataChunk();

        wr.openDataChunk("WorldInfo", 1);
        {
                Dict d;
                d.setAsciiString(keys.nameToKey("Name"), "Test Map");
                d.setInt(keys.nameToKey("WidthTiles"), 64);
                d.setBool(keys.nameToKey("IsMultiplayer"), true);
                wr.writeDict(keys, d);
        }
        wr.closeDataChunk();

        wr.openDataChunk("ObjectsList", 3);
        wr.openDataChunk("MapObject", 1);
        wr.writeNameKey(keys, keys.nameToKey("Name"));
        {
                Dict d;
                d.setInt(keys.nameToKey("ObjectFlags"), 77);
                wr.writeDict(keys, d);
        }
        wr.writeAsciiString("Tank");
        wr.closeDataChunk();
        wr.openDataChunk("MapObject", 1);
        wr.writeNameKey(keys, keys.nameToKey("Sun"));
        {
                UnicodeString u;
                u.u = {0x0041, 0x0042};
                wr.writeUnicodeString(u);
        }
        wr.closeDataChunk();
        wr.closeDataChunk();

        wr.openDataChunk("Waypoint", 1);
        wr.writeInt(10);
        wr.writeInt(20);
        wr.writeAsciiString("WP1");
        wr.closeDataChunk();

        run_writer(out, id, sink.bytes, wr, 2, true,
                   {{"HeightMapData", ""}, {"BlendTileData", ""}, {"GlobalLighting", ""},
                    {"WorldInfo", ""}, {"MapObject", "ObjectsList"}, {"Waypoint", ""}},
                   {"ObjectsList"});
}

// wr_reopen: the same name re-opened reuses its id; the TOC keeps one entry.
static void corpus_wr_reopen(Out &out)
{
        const char *id = "wr_reopen";
        NameKeyGenerator keys;
        ByteSink sink;
        DataChunkOutput wr(&sink);
        wr.openDataChunk("A", 1);
        wr.writeInt(1);
        wr.closeDataChunk();
        wr.openDataChunk("B", 1);
        wr.closeDataChunk();
        wr.openDataChunk("A", 2);
        wr.writeInt(2);
        wr.closeDataChunk();
        run_writer(out, id, sink.bytes, wr, 1, true, {{"A", ""}, {"B", ""}}, {});
}

// wr_interleave: chunk names and dict key names interleave in the TOC.
static void corpus_wr_interleave(Out &out)
{
        const char *id = "wr_interleave";
        NameKeyGenerator keys;
        ByteSink sink;
        DataChunkOutput wr(&sink);
        wr.openDataChunk("HeightMapData", 1);
        {
                Dict d;
                d.setInt(keys.nameToKey("Size"), 5);
                wr.writeDict(keys, d);
        }
        wr.closeDataChunk();
        wr.openDataChunk("BlendTileData", 1);
        wr.closeDataChunk();
        run_writer(out, id, sink.bytes, wr, 1, true, {{"HeightMapData", ""}, {"BlendTileData", ""}}, {});
}

// wr_primitives: every write primitive in one chunk.
static void corpus_wr_primitives(Out &out)
{
        const char *id = "wr_primitives";
        NameKeyGenerator keys;
        keys.nameToKey("Name");
        ByteSink sink;
        DataChunkOutput wr(&sink);
        wr.openDataChunk("P", 1);
        wr.writeReal(0.5f);
        wr.writeInt(-1);
        wr.writeByte(0xAB);
        wr.writeArrayOfBytes(formula_bytes(6, 11).data(), 6);
        wr.writeAsciiString("Hi");
        {
                UnicodeString u;
                u.u = {0x0041, 0x0042};
                wr.writeUnicodeString(u);
        }
        wr.writeNameKey(keys, keys.nameToKey("Name"));
        wr.closeDataChunk();
        run_writer(out, id, sink.bytes, wr, 1, true, {{"P", ""}}, {});
}

// wr_u16_wrap: a 65539-byte ascii string writes a u16 length of 3.
static void corpus_wr_u16_wrap(Out &out)
{
        const char *id = "wr_u16_wrap";
        NameKeyGenerator keys;
        ByteSink sink;
        DataChunkOutput wr(&sink);
        wr.openDataChunk("S", 1);
        {
                // NUL-free formula: the C-string semantics must not truncate
                // before the u16 conversion does.
                std::string s(65539, 'a');
                for (size_t i = 0; i < s.size(); ++i)
                        s[i] = (char)('a' + (i % 26));
                AsciiString str;
                str.s = s;
                wr.writeAsciiString(str);
        }
        wr.closeDataChunk();
        wr.openDataChunk("T", 1);
        wr.writeInt(1);
        wr.closeDataChunk();
        run_writer(out, id, sink.bytes, wr, 1, true, {{"S", ""}, {"T", ""}}, {});
}

// wr_uni_wrap: a 65537-unit unicode string writes a u16 length of 1.
static void corpus_wr_uni_wrap(Out &out)
{
        const char *id = "wr_uni_wrap";
        NameKeyGenerator keys;
        ByteSink sink;
        DataChunkOutput wr(&sink);
        wr.openDataChunk("U", 1);
        {
                // zero-free formula (the wide NUL would truncate first)
                UnicodeString u;
                u.u.resize(65537);
                for (size_t i = 0; i < u.u.size(); ++i)
                        u.u[i] = (WideChar)('a' + (i % 26));
                u.u[0] = 0x0041;   // the single unit that survives the wrap
                wr.writeUnicodeString(u);
        }
        wr.closeDataChunk();
        run_writer(out, id, sink.bytes, wr, 1, true, {{"U", ""}}, {});
}

// wr_toc_trunc: a 259-character chunk name truncates to 3 in the TOC.
static void corpus_wr_toc_trunc(Out &out)
{
        const char *id = "wr_toc_trunc";
        NameKeyGenerator keys;
        ByteSink sink;
        DataChunkOutput wr(&sink);
        wr.openDataChunk(std::string(259, 'a').c_str(), 1);
        wr.writeInt(7);
        wr.closeDataChunk();
        run_writer(out, id, sink.bytes, wr, 1, true, {{"aaa", ""}}, {});

        // the long name no longer resolves; the truncated one does
        CachedFileInputStream s(sink.bytes);
        DataChunkInput in(&s);
        out.step(id, "gid " + Out::hxs(std::string(259, 'a')) + " " +
                          std::to_string(in.contents().getID(std::string(259, 'a').c_str())));
        out.step(id, "gid " + Out::hxs("aaa") + " " +
                          std::to_string(in.contents().getID("aaa")));
}

// wr_namekey: a known key and an unknown key (whose name lookup is empty,
// allocating the empty-name TOC entry).
static void corpus_wr_namekey(Out &out)
{
        const char *id = "wr_namekey";
        NameKeyGenerator keys;
        keys.nameToKey("Known");
        ByteSink sink;
        DataChunkOutput wr(&sink);
        wr.openDataChunk("K", 1);
        wr.writeNameKey(keys, keys.nameToKey("Known"));
        wr.writeNameKey(keys, (NameKeyType)99);
        wr.closeDataChunk();
        run_writer(out, id, sink.bytes, wr, 1, true, {{"K", ""}}, {});
}

// wr_empty: no chunks at all — a TOC with zero entries that the reader
// refuses (count == 0 is not an opened header).
static void corpus_wr_empty(Out &out)
{
        const char *id = "wr_empty";
        NameKeyGenerator keys;
        ByteSink sink;
        DataChunkOutput wr(&sink);
        run_writer(out, id, sink.bytes, wr, 0, true, {}, {});
}

// wr_overclose: closing with nothing open is a silent no-op.
static void corpus_wr_overclose(Out &out)
{
        const char *id = "wr_overclose";
        NameKeyGenerator keys;
        ByteSink sink;
        DataChunkOutput wr(&sink);
        wr.closeDataChunk();
        wr.openDataChunk("A", 1);
        wr.writeInt(1);
        wr.closeDataChunk();
        wr.closeDataChunk();
        run_writer(out, id, sink.bytes, wr, 1, true, {{"A", ""}}, {});
}

// wr_nested: eight levels deep.
static void corpus_wr_nested(Out &out)
{
        const char *id = "wr_nested";
        NameKeyGenerator keys;
        ByteSink sink;
        DataChunkOutput wr(&sink);
        for (int i = 1; i <= 8; ++i)
                wr.openDataChunk("L", (DataChunkVersionTypeT)i);
        wr.writeInt(0x1234);
        for (int i = 0; i < 8; ++i)
                wr.closeDataChunk();
        run_writer(out, id, sink.bytes, wr, 8, true, {{"L", "L"}}, {"L"});
}

// wr_dict_all: all five value types, inserted out of ordinal order so the
// dict's sort (not the insertion) decides the file order.
static void corpus_wr_dict_all(Out &out)
{
        const char *id = "wr_dict_all";
        NameKeyGenerator keys;
        keys.nameToKey("Num");
        keys.nameToKey("Mid");
        ByteSink sink;
        DataChunkOutput wr(&sink);
        wr.openDataChunk("D", 1);
        {
                Dict d;
                d.setReal(keys.nameToKey("Alpha"), 1.5f);
                d.setInt(keys.nameToKey("Num"), 42);
                UnicodeString u;
                u.u = {0x0043};
                d.setUnicodeString(keys.nameToKey("Mid"), u);
                d.setBool(keys.nameToKey("Zeta"), true);
                d.setAsciiString(keys.nameToKey("Bool"), "yes");
                wr.writeDict(keys, d);
        }
        wr.closeDataChunk();
        run_writer(out, id, sink.bytes, wr, 1, true, {{"D", ""}}, {});
}

// wr_trailing_empty: an empty final chunk leaves its header ending at EOF,
// which the reader's eof check turns into an invisible pushed chunk.
static void corpus_wr_trailing_empty(Out &out)
{
        const char *id = "wr_trailing_empty";
        NameKeyGenerator keys;
        ByteSink sink;
        DataChunkOutput wr(&sink);
        wr.openDataChunk("A", 1);
        wr.closeDataChunk();
        wr.openDataChunk("B", 1);
        wr.writeInt(1);
        wr.closeDataChunk();
        wr.openDataChunk("C", 1);
        wr.closeDataChunk();
        run_writer(out, id, sink.bytes, wr, 1, true, {{"A", ""}, {"B", ""}, {"C", ""}}, {});
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
        fprintf(f, "# gen_mapchunk_golden.cpp — DataChunk.cpp transcription (baseline 571a7c5).\n");
        fprintf(f, "# Golden script for rzh-formats::map_chunk; see tools/differential/README.md.\n");
        fprintf(f, "# Grammar: file/valid/rtoc/rtoce/ok/state/step/wgen/wtoc/wtoce/wdepth.\n");

        corpus_rd_modern(out);
        corpus_rd_scope(out);
        corpus_rd_order(out);
        corpus_rd_abort(out);
        corpus_rd_drift(out);
        corpus_rd_leftover(out);
        corpus_rd_nested(out);
        corpus_rd_overdeclared(out);
        corpus_rd_seek_past_eof(out);
        corpus_rd_neg_size(out);
        corpus_rd_trunc_header(out);
        corpus_rd_header_at_eof(out);
        corpus_rd_trunc_string(out);
        corpus_rd_str_nul(out);
        corpus_rd_dict_types(out);
        corpus_rd_dict_dup(out);
        corpus_rd_dict_badtype(out);
        corpus_rd_dict_short(out);
        corpus_rd_namekey(out);
        corpus_rd_unknown_id(out);
        corpus_rd_zero_entry(out);
        corpus_rd_toc_only_eof(out);
        corpus_rd_toc_zero(out);
        corpus_rd_bad_magic(out);
        corpus_rd_empty(out);
        corpus_rd_short_tag(out);
        corpus_rd_short_count(out);
        corpus_rd_dup_names(out);
        corpus_rd_dup_ids(out);
        corpus_rd_nul_name(out);
        corpus_rd_unsorted_ids(out);
        corpus_wr_modern(out);
        corpus_wr_reopen(out);
        corpus_wr_interleave(out);
        corpus_wr_primitives(out);
        corpus_wr_u16_wrap(out);
        corpus_wr_uni_wrap(out);
        corpus_wr_toc_trunc(out);
        corpus_wr_namekey(out);
        corpus_wr_empty(out);
        corpus_wr_overclose(out);
        corpus_wr_nested(out);
        corpus_wr_dict_all(out);
        corpus_wr_trailing_empty(out);

        fclose(f);
        return 0;
}
