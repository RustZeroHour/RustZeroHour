// SPDX-License-Identifier: GPL-3.0-or-later
//
// Golden-dump driver for `rzh-formats::w3d_hlod` (parity level P0).
//
// Verbatim transcription of the Zero Hour HLOD-assembly loader —
// GeneralsMD/Code/Libraries/Source/WWVegas/WW3D2/hlod.cpp at commit
// 571a7c5 (HLodDefClass::Load_W3D, HLodDefClass::read_header,
// HLodDefClass::read_proxy_array and
// HLodDefClass::SubObjectArrayClass::Load_W3D — the data path the
// asset manager drives through HLodLoaderClass::Load_W3D), plus the
// pieces those readers drive:
//   GeneralsMD/Code/Libraries/Source/WWVegas/WW3D2/w3d_file.h
//     (W3dHLodHeaderStruct, W3dHLodArrayHeaderStruct,
//      W3dHLodSubObjectStruct, the W3D_CHUNK_HLOD_* ids)
//   GeneralsMD/Code/Libraries/Source/WWVegas/WW3D2/hlod.cpp's own
//     ProxyRecordClass / ProxyArrayClass (the proxy records Init
//     fills, the ref-counted VectorClass the array is born as — the
//     VectorClass(int) semantics of WWLib/Vector.h: `new T[size]`
//     default-constructs every record, Length() is the asked-for
//     count)
// The chunk container itself is the chunkio.cpp reader already pinned by
// gen_chunk_golden.cpp; it is transcribed here again (release-build
// semantics) because this harness is self-contained.
//
// The corpus bytes are the payload of the W3D_CHUNK_HLOD container the
// asset manager opens before calling the loader (assetmgr.cpp
// Load_3D_Assets -> Load_Prototype -> HLodLoaderClass::Load_W3D); the
// harness enters the loader exactly as the manager does, with the
// container's children as the reader's top level.
//
// Deliberate deviations, pinned identically on both sides (the Rust port
// documents the same set):
//
// - The header's two strdup calls and the sub-object strdup read past
//   their source fields when a name has no NUL. Within the header
//   struct the run is deterministic (Name runs into HierarchyName, the
//   fields are adjacent); both sides transcribe that run and cap it at
//   the 40-byte struct (past the struct the original reads stack
//   bytes — not reproducible; corpora l08/l09 pin the in-struct run
//   and the cap). The sub-object's Name field ends at its struct end,
//   so its cap is the 36-byte struct (corpus l12's 32-byte unterminated
//   name pins the capped stance; the original reads stack bytes).
//
// - Entries a failing sub-array read leaves unread hold uninitialised
//   heap in the original (`new char*[ModelCount]` / `new int[ModelCount]`
//   are never value-initialized). Both sides pin the zeroed stance:
//   empty names, bone 0 (corpora l20/l21/l27).
//
// - Corpora avoid counts the harness could not allocate (the C++
//   `new[n]` / `VectorClass(n)` with a negative or huge count would
//   throw and terminate — not a load verdict). Counts in the corpus
//   set are small and non-negative; the Rust port states the same
//   contract (its allocation aborts the same way).
//
// - Close_Chunk's bool result is transcribed as its release-build
//   value (always true — the assert paths never fire); the load paths
//   check it anyway, exactly as the original does.
//
// - The uint32 -> int field conversions (LodCount, ModelCount,
//   BoneIndex) wrap exactly as the 32-bit build's conversions do;
//   corpora stay inside the wrap-free range except the bone-index
//   extremes (l14), which wrap identically on every platform.
//
// Output grammar (one command per line, hex lowercase, '-' = empty):
//
//   file <id> <hex|->          the corpus bytes (HLOD container payload)
//   ok <id> | err <id>         the load verdict
//   hdr <id> <ver> <name|-> <hname|-> <lodcount>
//                              ver: version as 8 hex digits; name/hname:
//                              the strdup'd strings (the run-through
//                              quirk included); lodcount: the member
//   arr <id> l<i>|agg <maxscreensize> <modelcount>
//                              one line per sub-object array — the
//                              LOD arrays (l<i>, i in 0..LodCount) and
//                              the aggregates (agg); maxscreensize: f32
//                              bit word; modelcount: the member
//   mdl <id> l<i>|agg <j> <name|-> <bone>
//                              one line per model entry; name: the
//                              strdup'd name ('-' = the zeroed unread
//                              stance or an empty name); bone: decimal
//   proxies <id> <count|->     the proxy array length, '-' when the
//                              def holds no proxy array
//   prx <id> <i> <name|-> <bone>
//                              one line per proxy record; name: the
//                              strlcpy'd 32-byte field's C-string view
//                              (31-char truncation included); bone:
//                              decimal
//
// Usage: gen_hlod_golden <output-file>
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
//
// This program is distributed in the hope that it will be useful, but
// WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU
// General Public License for more details.

#include <algorithm>
#include <cfloat>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>
#include <vector>

// The transcription body lives in an anonymous namespace: the §6 bench
// wrappers (tools/differential/bench_baseline.sh) link several generator
// translation units into one binary, and their identically-named
// self-contained transcriptions (the chunkio.cpp ChunkLoadClass every
// W3D generator re-transcribes) would otherwise collide at link time.
// main() stays at global scope so the generator still links standalone.
namespace {

// --- the format constants (w3d_file.h) ----------------------------------------

#define W3D_NAME_LEN 16
#define W3D_MAKE_VERSION(major, minor) (((major) << 16) | (minor))

// enum continuation values at 0x00000700 in w3d_file.h
static const uint32_t W3D_CHUNK_HLOD = 0x00000700;             // HLOD assembly definition
static const uint32_t W3D_CHUNK_HLOD_HEADER = 0x00000701;      // W3dHLodHeaderStruct
static const uint32_t W3D_CHUNK_HLOD_LOD_ARRAY = 0x00000702;   // one level of detail
static const uint32_t W3D_CHUNK_HLOD_SUB_OBJECT_ARRAY_HEADER = 0x00000703; // W3dHLodArrayHeaderStruct
static const uint32_t W3D_CHUNK_HLOD_SUB_OBJECT = 0x00000704;  // W3dHLodSubObjectStruct
static const uint32_t W3D_CHUNK_HLOD_AGGREGATE_ARRAY = 0x00000705; // the aggregates
static const uint32_t W3D_CHUNK_HLOD_PROXY_ARRAY = 0x00000706; // the proxies

static const uint32_t W3D_CURRENT_HLOD_VERSION = W3D_MAKE_VERSION(1, 0);
static const float NO_MAX_SCREEN_SIZE = FLT_MAX; // WWMATH_FLOAT_MAX

// --- the W3D structures (w3d_file.h, POD) --------------------------------------

struct W3dHLodHeaderStruct
{
        uint32_t Version;
        uint32_t LodCount;
        char Name[W3D_NAME_LEN];             // Name of the object
        char HierarchyName[W3D_NAME_LEN];    // name of the hierarchy tree to use (\0 if none)
};

struct W3dHLodArrayHeaderStruct
{
        uint32_t ModelCount;
        float MaxScreenSize; // if model is bigger than this, switch to higher lod.
};

struct W3dHLodSubObjectStruct
{
        uint32_t BoneIndex;
        char Name[W3D_NAME_LEN * 2];
};

static_assert(sizeof(W3dHLodHeaderStruct) == 40, "W3dHLodHeaderStruct layout");
static_assert(sizeof(W3dHLodArrayHeaderStruct) == 8, "W3dHLodArrayHeaderStruct layout");
static_assert(sizeof(W3dHLodSubObjectStruct) == 36, "W3dHLodSubObjectStruct layout");

// --- the bounded C-string helpers ----------------------------------------------

// strdup_bounded: the C string starting at `field_off` inside a fixed
// struct, scanning through the struct's following bytes to the first NUL,
// capped at the struct end. This is what the original's strdup reads —
// the run through the adjacent field is deterministic (the header's Name
// runs into HierarchyName); past the struct the original reads stack
// bytes, which both sides cap (the documented deviation).
static std::string strdup_bounded(const uint8_t *struct_bytes, size_t struct_len, size_t field_off)
{
        size_t end = field_off;
        while (end < struct_len && struct_bytes[end] != 0) {
                end++;
        }
        return std::string((const char *)struct_bytes + field_off, end - field_off);
}

// strlcpy (the canonical BSD body, minus the return-value traversal —
// ProxyRecordClass::Init ignores the return, and the traversal would
// read past the 32-byte field when it has no NUL; the copy itself is
// bounded by siz-1 and never leaves the field).
static void sstrlcpy(char *dst, const char *src, size_t siz)
{
        size_t n = siz;
        if (n != 0) {
                while (--n != 0) {
                        if ((*dst++ = *src++) == '\0') {
                                return;
                        }
                }
                *dst = '\0';
        }
}

// --- the chunk container reader (chunkio.cpp, release semantics) --------------

struct ChunkHeader
{
        uint32_t ChunkType;
        uint32_t ChunkSize;

        void Set_Type(uint32_t type) { ChunkType = type; }
        uint32_t Get_Type() const { return ChunkType; }
        void Set_Size(uint32_t size) { ChunkSize &= 0x80000000; ChunkSize |= (size & 0x7FFFFFFF); }
        uint32_t Get_Size() const { return (ChunkSize & 0x7FFFFFFF); }
};

#define SEEK_SET 0
#define SEEK_CUR 1
#define SEEK_END 2

struct SrcFile
{
        const uint8_t *data = nullptr;
        size_t len = 0;
        long pos = 0;

        long seek(long offset, int origin)
        {
                long target;
                if (origin == SEEK_SET)
                        target = offset;
                else if (origin == SEEK_CUR)
                        target = pos + offset;
                else if (origin == SEEK_END)
                        target = (long)len + offset;
                else
                        return -1;
                if (target < 0)
                        return -1;
                pos = target;
                return pos;
        }

        // RawFileClass::Read: fewer bytes than requested = file exhausted.
        uint32_t read(void *dst, uint32_t size)
        {
                if (pos < 0 || pos >= (long)len)
                        return 0;
                size_t avail = len - (size_t)pos;
                size_t n = std::min((size_t)size, avail);
                memcpy(dst, data + pos, n);
                pos += (long)n;
                return (uint32_t)n;
        }
};

class ChunkLoadClass
{
public:
        ChunkLoadClass(SrcFile *file);

        bool Open_Chunk();
        bool Close_Chunk();
        uint32_t Cur_Chunk_ID();
        [[maybe_unused]] uint32_t Cur_Chunk_Length();

        uint32_t Read(void *buf, uint32_t nbytes);

private:
        SrcFile *File;
        int StackIndex;
        uint32_t PositionStack[256];
        ChunkHeader HeaderStack[256];
};

ChunkLoadClass::ChunkLoadClass(SrcFile *file) : File(file), StackIndex(0)
{
        memset(PositionStack, 0, sizeof(PositionStack));
        memset(HeaderStack, 0, sizeof(HeaderStack));
}

bool ChunkLoadClass::Open_Chunk()
{
        // release build: the micro-chunk and depth asserts are skipped

        // if the parent chunk has been completely eaten, return false
        if ((StackIndex > 0) && (PositionStack[StackIndex - 1] == HeaderStack[StackIndex - 1].Get_Size())) {
                return false;
        }

        // read the chunk header
        if (File->read(&HeaderStack[StackIndex], sizeof(ChunkHeader)) != sizeof(ChunkHeader)) {
                return false;
        }

        PositionStack[StackIndex] = 0;
        StackIndex++;
        return true;
}

bool ChunkLoadClass::Close_Chunk()
{
        // release build: the depth assert is skipped; StackIndex == 0 here
        // is a caller contract violation the HLOD loader never performs
        if (StackIndex < 1)
                return true;

        int csize = (int)HeaderStack[StackIndex - 1].Get_Size();
        int pos = (int)PositionStack[StackIndex - 1];

        if (pos < csize) {
                File->seek(csize - pos, SEEK_CUR);
        }

        StackIndex--;
        if (StackIndex > 0) {
                PositionStack[StackIndex - 1] += (uint32_t)(csize + sizeof(ChunkHeader));
        }

        return true;
}

uint32_t ChunkLoadClass::Cur_Chunk_ID()
{
        // release build: StackIndex == 0 reads out of bounds (UB). The
        // HLOD loader only reaches here after a failed Open_Chunk (which
        // already failed the load); the transcription states the contract
        // instead.
        if (StackIndex < 1) {
                return 0xFFFFFFFF;
        }
        return HeaderStack[StackIndex - 1].Get_Type();
}

uint32_t ChunkLoadClass::Cur_Chunk_Length()
{
        if (StackIndex < 1) {
                return 0xFFFFFFFF;
        }
        return HeaderStack[StackIndex - 1].Get_Size();
}

uint32_t ChunkLoadClass::Read(void *buf, uint32_t nbytes)
{
        // release build: the StackIndex assert is skipped and StackIndex == 0
        // reads the stacks out of bounds (UB); the HLOD loader never reads
        // without an open chunk, so the transcription states the contract
        if (StackIndex < 1)
                return 0;

        // Don't read if we would go past the end of the current chunk
        if (PositionStack[StackIndex - 1] + nbytes > HeaderStack[StackIndex - 1].Get_Size()) {
                return 0;
        }

        // the file read: a short read consumes the available bytes and fails
        if (File->read(buf, nbytes) != nbytes) {
                return 0;
        }

        // Update our position in the chunk
        PositionStack[StackIndex - 1] += nbytes;

        // return the number of bytes read
        return nbytes;
}

// --- ProxyRecordClass (hlod.cpp, the proxy records Init fills) ------------------

class ProxyRecordClass
{
public:
        ProxyRecordClass() : BoneIndex(0)
        {
                memset(Name, 0, sizeof(Name));
        }

        void Init(const W3dHLodSubObjectStruct &w3d_data)
        {
                BoneIndex = (int)w3d_data.BoneIndex;
                sstrlcpy(Name, w3d_data.Name, sizeof(Name));
        }

        int Get_Bone_Index() const { return BoneIndex; }
        const char *Get_Name() const { return Name; }

private:
        int BoneIndex;
        char Name[2 * W3D_NAME_LEN];
};

// --- ProxyArrayClass (hlod.cpp: VectorClass<ProxyRecordClass> + RefCountClass) --

// The ref-counted array the def add-references. VectorClass(int size)
// allocates `new T[size]` — every record default-constructed — and
// Length() reports the asked-for count; the null-member state (before
// the first proxy array, or after a failing one) is the raw pointer.
class ProxyArrayClass
{
public:
        explicit ProxyArrayClass(int size) : Records((size_t)size), VectorMax(size)
        {
                // (size) non-zero — including negative — allocates in the
                // original; a negative count's huge allocation throws and
                // terminates there, and std::vector's length_error does the
                // same here. Corpora avoid those counts (documented above).
        }

        int Length() const { return VectorMax; }
        ProxyRecordClass &operator[](int index) { return Records[(size_t)index]; }
        const ProxyRecordClass &operator[](int index) const { return Records[(size_t)index]; }

private:
        std::vector<ProxyRecordClass> Records;
        int VectorMax;
};

// --- HLodDefClass (hlod.cpp, the load path) -------------------------------------

class HLodDefClass
{
public:
        // WW3DErrorType
        enum
        {
                WW3D_ERROR_OK,
                WW3D_ERROR_LOAD_FAILED
        };

        class SubObjectArrayClass
        {
        public:
                SubObjectArrayClass() : MaxScreenSize(NO_MAX_SCREEN_SIZE), ModelCount(0) {}

                bool Load_W3D(ChunkLoadClass &cload);

                float MaxScreenSize;
                int ModelCount;
                std::vector<std::string> ModelName; // the strdup'd names (zeroed stance)
                std::vector<int> BoneIndex;         // the bone members (zeroed stance)
        };

        HLodDefClass() : Name(), HierarchyTreeName(), LodCount(0), Lod(), ProxyArray(nullptr) {}

        ~HLodDefClass() { Free(); }

        int Load_W3D(ChunkLoadClass &cload);
        bool read_header(ChunkLoadClass &cload);
        bool read_proxy_array(ChunkLoadClass &cload);

        // the load-path state the dump reads
        W3dHLodHeaderStruct Header{}; // dump-side stash (documented deviation)
        std::string Name;             // the strdup'd header name
        std::string HierarchyTreeName; // the strdup'd hierarchy name
        int LodCount;
        std::vector<SubObjectArrayClass> Lod;
        SubObjectArrayClass Aggregates;
        ProxyArrayClass *ProxyArray;

private:
        void Free()
        {
                LodCount = 0;
                Lod.clear();
                Aggregates = SubObjectArrayClass();
                delete ProxyArray;
                ProxyArray = nullptr;
        }
};

bool HLodDefClass::SubObjectArrayClass::Load_W3D(ChunkLoadClass &cload)
{
        /*
        ** Open the first chunk, it should be a Lod Array Header
        */
        if (!cload.Open_Chunk())
                return false;
        if (cload.Cur_Chunk_ID() != W3D_CHUNK_HLOD_SUB_OBJECT_ARRAY_HEADER)
                return false;

        W3dHLodArrayHeaderStruct header;
        if (cload.Read(&header, sizeof(header)) != sizeof(header))
                return false;

        if (!cload.Close_Chunk())
                return false;

        ModelCount = (int)header.ModelCount;
        MaxScreenSize = header.MaxScreenSize;

        // ModelName = new char*[ModelCount]; BoneIndex = new int[ModelCount];
        // (the zeroed stance: the original's arrays are uninitialised until
        // each entry is read — a failing read leaves the tail garbage there;
        // both sides pin the zeroed view)
        ModelName.assign((size_t)ModelCount, std::string());
        BoneIndex.assign((size_t)ModelCount, 0);

        /*
        ** Read each sub object definition
        */
        for (int imodel = 0; imodel < ModelCount; ++imodel) {
                if (!cload.Open_Chunk())
                        return false;
                if (cload.Cur_Chunk_ID() != W3D_CHUNK_HLOD_SUB_OBJECT)
                        return false;

                W3dHLodSubObjectStruct subobjdef;
                if (cload.Read(&subobjdef, sizeof(subobjdef)) != sizeof(subobjdef))
                        return false;

                if (!cload.Close_Chunk())
                        return false;

                // strdup(subobjdef.Name) — the bounded transcription: the
                // Name field ends at the struct end, so the scan caps there
                uint8_t raw[sizeof(subobjdef)];
                memcpy(raw, &subobjdef, sizeof(raw));
                ModelName[(size_t)imodel] = strdup_bounded(raw, sizeof(raw), offsetof(W3dHLodSubObjectStruct, Name));
                BoneIndex[(size_t)imodel] = (int)subobjdef.BoneIndex;
        }
        return true;
}

int HLodDefClass::Load_W3D(ChunkLoadClass &cload)
{
        /*
        ** First make sure we release any memory in use
        */
        Free();

        if (read_header(cload) == false) {
                return WW3D_ERROR_LOAD_FAILED;
        }

        /*
        **      Loop through all the LODs and read the info from its chunk
        */
        for (int iLOD = 0; iLOD < LodCount; iLOD++) {

                /*
                **      Open the next chunk, it should be a LOD struct
                */
                if (!cload.Open_Chunk())
                        return WW3D_ERROR_LOAD_FAILED;

                if (cload.Cur_Chunk_ID() != W3D_CHUNK_HLOD_LOD_ARRAY) {
                        // ERROR: Expected LOD struct!
                        return WW3D_ERROR_LOAD_FAILED;
                }

                Lod[(size_t)iLOD].Load_W3D(cload);

                // Close-out the chunk
                cload.Close_Chunk();
        }

        /*
        ** Parse the rest of the chunks
        */
        while (cload.Open_Chunk()) {
                switch (cload.Cur_Chunk_ID()) {
                        case W3D_CHUNK_HLOD_AGGREGATE_ARRAY:
                                Aggregates.Load_W3D(cload);
                                break;
                        case W3D_CHUNK_HLOD_PROXY_ARRAY:
                                read_proxy_array(cload);
                                break;
                }
                cload.Close_Chunk();
        }

        return WW3D_ERROR_OK;
}

bool HLodDefClass::read_header(ChunkLoadClass &cload)
{
        /*
        **      Open the first chunk, it should be the LOD header
        */
        if (!cload.Open_Chunk())
                return false;

        if (cload.Cur_Chunk_ID() != W3D_CHUNK_HLOD_HEADER) {
                // ERROR: Expected HLOD Header!
                return false;
        }

        W3dHLodHeaderStruct header;
        if (cload.Read(&header, sizeof(header)) != sizeof(header)) {
                return false;
        }
        cload.Close_Chunk();

        // Copy the name into our internal variable
        // (the bounded transcription of the two strdup calls: the scan
        // runs through the adjacent header field — Name into
        // HierarchyName — and caps at the 40-byte struct)
        uint8_t raw[sizeof(header)];
        memcpy(raw, &header, sizeof(raw));
        Name = strdup_bounded(raw, sizeof(raw), offsetof(W3dHLodHeaderStruct, Name));
        HierarchyTreeName = strdup_bounded(raw, sizeof(raw), offsetof(W3dHLodHeaderStruct, HierarchyName));
        Header = header; // dump-side stash: the pristine decode

        LodCount = (int)header.LodCount;
        Lod.assign((size_t)LodCount, SubObjectArrayClass());
        return true;
}

bool HLodDefClass::read_proxy_array(ChunkLoadClass &cload)
{
        delete ProxyArray;
        ProxyArray = nullptr;

        /*
        ** Open the first chunk, it should be a Lod Array Header
        */
        if (!cload.Open_Chunk())
                return false;
        if (cload.Cur_Chunk_ID() != W3D_CHUNK_HLOD_SUB_OBJECT_ARRAY_HEADER)
                return false;

        W3dHLodArrayHeaderStruct header;
        if (cload.Read(&header, sizeof(header)) != sizeof(header))
                return false;

        if (!cload.Close_Chunk())
                return false;

        ProxyArray = new ProxyArrayClass((int)header.ModelCount);

        /*
        ** Read each sub object definition
        */
        for (int imodel = 0; imodel < ProxyArray->Length(); ++imodel) {
                if (!cload.Open_Chunk())
                        return false;
                if (cload.Cur_Chunk_ID() != W3D_CHUNK_HLOD_SUB_OBJECT)
                        return false;

                W3dHLodSubObjectStruct subobjdef;
                if (cload.Read(&subobjdef, sizeof(subobjdef)) != sizeof(subobjdef))
                        return false;

                if (!cload.Close_Chunk())
                        return false;

                (*ProxyArray)[imodel].Init(subobjdef);
        }
        return true;
}

// --- the corpus builder ---------------------------------------------------------

// Assembles well-formed chunk trees (the byte framing the writer produces:
// 8-byte headers, sizes patched on end, the contains-sub-chunks MSB set on
// parents). Hand-malformed corpora patch or truncate the output directly.
struct ChunkBuild
{
        std::vector<uint8_t> bytes;
        std::vector<size_t> marks;
        std::vector<int> child_counts;

        void begin(uint32_t id)
        {
                if (!marks.empty()) {
                        child_counts.back() += 1;
                }
                marks.push_back(bytes.size());
                child_counts.push_back(0);
                uint8_t hdr[8];
                put_u32(hdr, id);
                put_u32(hdr + 4, 0);
                bytes.insert(bytes.end(), hdr, hdr + 8);
        }

        void end()
        {
                size_t off = marks.back();
                marks.pop_back();
                int kids = child_counts.back();
                child_counts.pop_back();
                size_t size = bytes.size() - off - 8;
                if (kids > 0) {
                        size |= 0x80000000; // the contains-sub-chunks flag
                }
                put_u32(bytes.data() + off + 4, (uint32_t)size);
        }

        void w_u32(uint32_t v)
        {
                uint8_t b[4] = {(uint8_t)(v & 0xFF), (uint8_t)((v >> 8) & 0xFF), (uint8_t)((v >> 16) & 0xFF),
                                (uint8_t)((v >> 24) & 0xFF)};
                bytes.insert(bytes.end(), b, b + 4);
        }
        void w_f32(float v)
        {
                uint32_t bits;
                memcpy(&bits, &v, 4);
                w_u32(bits);
        }
        void w_f32_bits(uint32_t bits)
        {
                w_u32(bits);
        }
        void w_bytes(const void *p, size_t n)
        {
                const uint8_t *b = (const uint8_t *)p;
                bytes.insert(bytes.end(), b, b + n);
        }

        static void put_u32(uint8_t *p, uint32_t v)
        {
                p[0] = (uint8_t)(v & 0xFF);
                p[1] = (uint8_t)((v >> 8) & 0xFF);
                p[2] = (uint8_t)((v >> 16) & 0xFF);
                p[3] = (uint8_t)((v >> 24) & 0xFF);
        }

        // Patches the size word of the chunk whose header starts at off
        // (hand-malformed corpora lie about their payload size).
        void patch_size(size_t off, uint32_t size)
        {
                put_u32(bytes.data() + off + 4, size);
        }
};

// A 40-byte W3dHLodHeaderStruct chunk
static void w_hlod_header(ChunkBuild &b, uint32_t version, uint32_t lodcount, const char *name, const char *hname)
{
        char n[W3D_NAME_LEN];
        char h[W3D_NAME_LEN];
        memset(n, 0, sizeof(n));
        memset(h, 0, sizeof(h));
        snprintf(n, sizeof(n), "%s", name);
        snprintf(h, sizeof(h), "%s", hname);

        b.begin(W3D_CHUNK_HLOD_HEADER);
        b.w_u32(version);
        b.w_u32(lodcount);
        b.w_bytes(n, W3D_NAME_LEN);
        b.w_bytes(h, W3D_NAME_LEN);
        b.end();
}

// A 40-byte W3dHLodHeaderStruct chunk with raw name fields (no NUL)
static void w_hlod_header_raw(ChunkBuild &b, uint32_t version, uint32_t lodcount, const uint8_t name[16],
                              const uint8_t hname[16])
{
        b.begin(W3D_CHUNK_HLOD_HEADER);
        b.w_u32(version);
        b.w_u32(lodcount);
        b.w_bytes(name, W3D_NAME_LEN);
        b.w_bytes(hname, W3D_NAME_LEN);
        b.end();
}

// An 8-byte W3dHLodArrayHeaderStruct chunk
static void w_array_header(ChunkBuild &b, uint32_t modelcount, float maxscreen)
{
        b.begin(W3D_CHUNK_HLOD_SUB_OBJECT_ARRAY_HEADER);
        b.w_u32(modelcount);
        b.w_f32(maxscreen);
        b.end();
}

// An 8-byte array header with a raw f32 bit pattern (NaN, -0.0, ...)
static void w_array_header_bits(ChunkBuild &b, uint32_t modelcount, uint32_t maxscreen_bits)
{
        b.begin(W3D_CHUNK_HLOD_SUB_OBJECT_ARRAY_HEADER);
        b.w_u32(modelcount);
        b.w_f32_bits(maxscreen_bits);
        b.end();
}

// A 36-byte W3dHLodSubObjectStruct chunk
static void w_sub_object(ChunkBuild &b, uint32_t bone, const char *name)
{
        char n[W3D_NAME_LEN * 2];
        memset(n, 0, sizeof(n));
        snprintf(n, sizeof(n), "%s", name);

        b.begin(W3D_CHUNK_HLOD_SUB_OBJECT);
        b.w_u32(bone);
        b.w_bytes(n, W3D_NAME_LEN * 2);
        b.end();
}

// A 36-byte sub-object chunk with a raw 32-byte name field (no NUL)
static void w_sub_object_raw(ChunkBuild &b, uint32_t bone, const uint8_t name[32])
{
        b.begin(W3D_CHUNK_HLOD_SUB_OBJECT);
        b.w_u32(bone);
        b.w_bytes(name, W3D_NAME_LEN * 2);
        b.end();
}

// --- the corpora ----------------------------------------------------------------

struct Corpus
{
        const char *id;
        std::vector<uint8_t> bytes;
};

static std::vector<Corpus> build_corpora()
{
        std::vector<Corpus> out;
        const uint32_t V10 = W3D_MAKE_VERSION(1, 0);
        auto push = [&](const char *id, ChunkBuild &b) {
                out.push_back(Corpus{id, b.bytes});
        };

        // l01: the modern full HLOD — a named assembly with a hierarchy,
        // three LODs of decreasing detail, one aggregate and two proxies.
        {
                ChunkBuild b;
                w_hlod_header(b, V10, 3, "AVTank", "AVTank");
                b.begin(W3D_CHUNK_HLOD_LOD_ARRAY);
                w_array_header(b, 4, 100.0f);
                w_sub_object(b, 0, "AVTankH");
                w_sub_object(b, 1, "AVTankT");
                w_sub_object(b, 2, "AVTankB");
                w_sub_object(b, 0, "AVTankD");
                b.end();
                b.begin(W3D_CHUNK_HLOD_LOD_ARRAY);
                w_array_header(b, 2, 40.0f);
                w_sub_object(b, 0, "AVTankH_LOD");
                w_sub_object(b, 1, "AVTankT_LOD");
                b.end();
                b.begin(W3D_CHUNK_HLOD_LOD_ARRAY);
                w_array_header(b, 1, NO_MAX_SCREEN_SIZE);
                w_sub_object(b, 0, "AVTankH_REAL");
                b.end();
                b.begin(W3D_CHUNK_HLOD_AGGREGATE_ARRAY);
                w_array_header(b, 1, 0.0f);
                w_sub_object(b, 3, "AVTrack");
                b.end();
                b.begin(W3D_CHUNK_HLOD_PROXY_ARRAY);
                w_array_header(b, 2, 0.0f);
                w_sub_object(b, 2, "Muzzle_1");
                w_sub_object(b, 5, "Exit_1");
                b.end();
                push("l01", b);
        }

        // l02: a large assembly — eight LODs of three models, three
        // aggregates, four proxies — and the float shapes MaxScreenSize
        // can hold (the loader stores it verbatim; no arithmetic):
        // 1.0, -0.0, 0.5, +inf, a quiet NaN, a denormal, FLT_MAX, -1.5.
        {
                ChunkBuild b;
                w_hlod_header(b, V10, 8, "BigAssembly", "BigTree");
                const uint32_t screens[8] = {
                    0x3F800000u, // 1.0
                    0x80000000u, // -0.0
                    0x3F000000u, // 0.5
                    0x7F800000u, // +inf
                    0x7FC00000u, // quiet NaN
                    0x00000001u, // the smallest denormal
                    0x7F7FFFFFu, // FLT_MAX (NO_MAX_SCREEN_SIZE)
                    0xBFC00000u, // -1.5
                };
                for (uint32_t i = 0; i < 8; i++) {
                        b.begin(W3D_CHUNK_HLOD_LOD_ARRAY);
                        w_array_header_bits(b, 3, screens[i]);
                        w_sub_object(b, i, "M0");
                        w_sub_object(b, i + 1, "M1");
                        w_sub_object(b, 0x10u + i, "M2");
                        b.end();
                }
                b.begin(W3D_CHUNK_HLOD_AGGREGATE_ARRAY);
                w_array_header(b, 3, 250.0f);
                w_sub_object(b, 0, "AGG_A");
                w_sub_object(b, 1, "AGG_B");
                w_sub_object(b, 2, "AGG_C");
                b.end();
                b.begin(W3D_CHUNK_HLOD_PROXY_ARRAY);
                w_array_header(b, 4, 0.0f);
                w_sub_object(b, 0, "P0");
                w_sub_object(b, 1, "P1");
                w_sub_object(b, 2, "P2");
                w_sub_object(b, 3, "P3");
                b.end();
                push("l02", b);
        }

        // l03: version 0 — the loader never consults the version field.
        {
                ChunkBuild b;
                w_hlod_header(b, 0, 1, "V0", "T");
                b.begin(W3D_CHUNK_HLOD_LOD_ARRAY);
                w_array_header(b, 1, 10.0f);
                w_sub_object(b, 0, "M");
                b.end();
                push("l03", b);
        }

        // l04: version 0xFFFFFFFF — still loads, still unchecked.
        {
                ChunkBuild b;
                w_hlod_header(b, 0xFFFFFFFFu, 1, "VMax", "T");
                b.begin(W3D_CHUNK_HLOD_LOD_ARRAY);
                w_array_header(b, 1, 10.0f);
                w_sub_object(b, 0, "M");
                b.end();
                push("l04", b);
        }

        // l05: version 99.99 (W3D_MAKE_VERSION(99, 99)) — loads.
        {
                ChunkBuild b;
                w_hlod_header(b, W3D_MAKE_VERSION(99, 99), 1, "V99", "T");
                b.begin(W3D_CHUNK_HLOD_LOD_ARRAY);
                w_array_header(b, 1, 10.0f);
                w_sub_object(b, 0, "M");
                b.end();
                push("l05", b);
        }

        // l06: LodCount 0 with an aggregate and a proxy array — an HLOD
        // with no LODs is a legal load; only the trailing chunks are read.
        {
                ChunkBuild b;
                w_hlod_header(b, V10, 0, "NoLods", "T");
                b.begin(W3D_CHUNK_HLOD_AGGREGATE_ARRAY);
                w_array_header(b, 1, 0.0f);
                w_sub_object(b, 0, "AGG");
                b.end();
                b.begin(W3D_CHUNK_HLOD_PROXY_ARRAY);
                w_array_header(b, 1, 0.0f);
                w_sub_object(b, 1, "PX");
                b.end();
                push("l06", b);
        }

        // l07: LodCount 0 and nothing else — the bare-minimum load.
        {
                ChunkBuild b;
                w_hlod_header(b, V10, 0, "Bare", "");
                push("l07", b);
        }

        // l08: an unterminated Name — the strdup runs through the
        // HierarchyName field (the deterministic in-struct run); the
        // hierarchy name itself is terminated.
        {
                ChunkBuild b;
                const uint8_t n[16] = {'A', 'V', 'e', 'r', 'y', 'L', 'o', 'n', 'g', 'H', 'l', 'o', 'd', 'N', 'a', 'm'};
                const uint8_t h[16] = {'t', 'r', 'e', 'e', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0};
                w_hlod_header_raw(b, V10, 1, n, h);
                b.begin(W3D_CHUNK_HLOD_LOD_ARRAY);
                w_array_header(b, 1, 10.0f);
                w_sub_object(b, 0, "M");
                b.end();
                push("l08", b);
        }

        // l09: no NUL anywhere in the 40 header bytes — both strdups cap
        // at the struct end (the original reads stack bytes past it; the
        // documented deviation both sides share).
        {
                ChunkBuild b;
                const uint8_t n[16] = {'N', 'a', 'm', 'e', '1', '6', 'B', 'y', 't', 'e', 's', 'X', 'Y', 'Z', '0', '1'};
                const uint8_t h[16] = {'H', 'i', 'e', 'r', 'N', 'a', 'm', 'e', '1', '6', 'B', 'y', 't', 'e', 's', '!'};
                w_hlod_header_raw(b, V10, 1, n, h);
                b.begin(W3D_CHUNK_HLOD_LOD_ARRAY);
                w_array_header(b, 1, 10.0f);
                w_sub_object(b, 0, "M");
                b.end();
                push("l09", b);
        }

        // l10: names with high-bit, control and space bytes — the <hex>
        // dump form (printable non-space ASCII prints raw).
        {
                ChunkBuild b;
                const uint8_t n[16] = {0xC0, 0x41, 0x42, 0x01, 0x7F, 0x20, 0x00, 0, 0, 0, 0, 0, 0, 0, 0, 0};
                const uint8_t h[16] = {'t', 'r', 'e', 'e', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0};
                w_hlod_header_raw(b, V10, 1, n, h);
                b.begin(W3D_CHUNK_HLOD_LOD_ARRAY);
                w_array_header(b, 2, 10.0f);
                const uint8_t m0[32] = {0x80, 0x9A, 0xBC, 0xFF, 0x00, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                                        0,    0,    0,    0,    0,    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0};
                const uint8_t m1[32] = {'s', 'p', 'a', 'c', 'e', ' ', 'n', 'a', 'm', 'e', 0, 0, 0, 0, 0, 0,
                                        0,    0,   0,   0,   0,   0,   0,   0,   0,   0,   0, 0, 0, 0, 0, 0};
                w_sub_object_raw(b, 0, m0);
                w_sub_object_raw(b, 1, m1);
                b.end();
                push("l10", b);
        }

        // l11: empty Name and empty HierarchyName — the static-building
        // shape (no hierarchy to resolve).
        {
                ChunkBuild b;
                w_hlod_header(b, V10, 1, "", "");
                b.begin(W3D_CHUNK_HLOD_LOD_ARRAY);
                w_array_header(b, 1, 10.0f);
                w_sub_object(b, 0, "BldgLod");
                b.end();
                push("l11", b);
        }

        // l12: sub-object names at the field edges — a 31-char terminated
        // name (the longest a terminated field holds) and a full 32-byte
        // unterminated name (the strdup caps at the struct end; the
        // original reads stack bytes past it).
        {
                ChunkBuild b;
                w_hlod_header(b, V10, 1, "Edges", "T");
                b.begin(W3D_CHUNK_HLOD_LOD_ARRAY);
                w_array_header(b, 2, 10.0f);
                w_sub_object(b, 0, "31chars-ABCDEFGHIJKLMNOPQRSTUVW");
                const uint8_t full[32] = {'3', '2', 'c', 'h', 'a', 'r', 's', '-', 'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H',
                                          'I', 'J', 'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S', 'T', 'U', 'V', 'W', 'X'};
                w_sub_object_raw(b, 1, full);
                b.end();
                push("l12", b);
        }

        // l13: a 32-byte unterminated proxy name — Init's strlcpy
        // truncates to 31 characters (deterministic in the original,
        // unlike the strdup paths).
        {
                ChunkBuild b;
                w_hlod_header(b, V10, 1, "ProxyCut", "T");
                b.begin(W3D_CHUNK_HLOD_LOD_ARRAY);
                w_array_header(b, 1, 10.0f);
                w_sub_object(b, 0, "M");
                b.end();
                b.begin(W3D_CHUNK_HLOD_PROXY_ARRAY);
                w_array_header(b, 1, 0.0f);
                const uint8_t full[32] = {'P', 'R', 'O', 'X', 'Y', '3', '2', 'c', 'h', 'a', 'r', 's', '-', 'A', 'B', 'C',
                                          'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S'};
                w_sub_object_raw(b, 7, full);
                b.end();
                push("l13", b);
        }

        // l14: the bone-index int conversions — 0, 1, 0x7FFFFFFF,
        // 0x80000000 (INT_MIN) and 0xFFFFFFFF (-1), in models and
        // proxies alike.
        {
                ChunkBuild b;
                w_hlod_header(b, V10, 1, "Bones", "T");
                b.begin(W3D_CHUNK_HLOD_LOD_ARRAY);
                w_array_header(b, 5, 10.0f);
                w_sub_object(b, 0x00000000u, "B0");
                w_sub_object(b, 0x00000001u, "B1");
                w_sub_object(b, 0x7FFFFFFFu, "BMax");
                w_sub_object(b, 0x80000000u, "BMin");
                w_sub_object(b, 0xFFFFFFFFu, "BNeg1");
                b.end();
                b.begin(W3D_CHUNK_HLOD_PROXY_ARRAY);
                w_array_header(b, 2, 0.0f);
                w_sub_object(b, 0x80000000u, "PBMin");
                w_sub_object(b, 0xFFFFFFFFu, "PBNeg1");
                b.end();
                push("l14", b);
        }

        // l15: duplicate model names within a LOD and across LODs, and
        // an empty model name (the dump's '-' form).
        {
                ChunkBuild b;
                w_hlod_header(b, V10, 2, "Dupes", "T");
                b.begin(W3D_CHUNK_HLOD_LOD_ARRAY);
                w_array_header(b, 3, 10.0f);
                w_sub_object(b, 0, "Same");
                w_sub_object(b, 1, "Same");
                w_sub_object(b, 2, "");
                b.end();
                b.begin(W3D_CHUNK_HLOD_LOD_ARRAY);
                w_array_header(b, 1, 5.0f);
                w_sub_object(b, 0, "Same");
                b.end();
                push("l15", b);
        }

        // l16: the minimal modern HLOD — one LOD, zero models, a
        // hierarchy name, nothing else.
        {
                ChunkBuild b;
                w_hlod_header(b, V10, 1, "Minimal", "MinimalTree");
                b.begin(W3D_CHUNK_HLOD_LOD_ARRAY);
                w_array_header(b, 0, 10.0f);
                b.end();
                push("l16", b);
        }

        // l17: padded chunks — the header chunk carries 4 junk bytes
        // after its 40, the array header 4 after its 8, the sub-object 4
        // after its 36; Close_Chunk seeks past every pad.
        {
                ChunkBuild b;
                b.begin(W3D_CHUNK_HLOD_HEADER);
                b.w_u32(V10);
                b.w_u32(1);
                const char n[16] = {'P', 'a', 'd', 'd', 'e', 'd', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0};
                const char h[16] = {'T', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0};
                b.w_bytes(n, 16);
                b.w_bytes(h, 16);
                b.w_u32(0xDEADBEEFu); // the pad
                b.end();
                b.begin(W3D_CHUNK_HLOD_LOD_ARRAY);
                b.begin(W3D_CHUNK_HLOD_SUB_OBJECT_ARRAY_HEADER);
                b.w_u32(1);
                b.w_f32(10.0f);
                b.w_u32(0xCAFEBABEu); // the pad
                b.end();
                b.begin(W3D_CHUNK_HLOD_SUB_OBJECT);
                b.w_u32(0);
                const char m[32] = {'M', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                                    0,   0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0};
                b.w_bytes(m, 32);
                b.w_u32(0xFEEDFACEu); // the pad
                b.end();
                b.end();
                push("l17", b);
        }

        // l18: the LAST LOD's array opens with a sub-object chunk
        // instead of the array header — the sub-array read is discarded,
        // the load succeeds and that LOD keeps its constructed state
        // (MaxScreenSize NO_MAX_SCREEN_SIZE, zero models).
        {
                ChunkBuild b;
                w_hlod_header(b, V10, 2, "LastDefault", "T");
                b.begin(W3D_CHUNK_HLOD_LOD_ARRAY);
                w_array_header(b, 1, 10.0f);
                w_sub_object(b, 0, "M0");
                b.end();
                b.begin(W3D_CHUNK_HLOD_LOD_ARRAY);
                w_sub_object(b, 0, "Stray"); // not the array header
                b.end();
                push("l18", b);
        }

        // l19: the same stray sub-object in a NON-last LOD — the stray
        // chunk stays open through the wrapper's Close_Chunk, the next
        // loop iteration opens inside the unexhausted wrapper and the
        // LOD_ARRAY id check fails the load.
        {
                ChunkBuild b;
                w_hlod_header(b, V10, 2, "MidDefault", "T");
                b.begin(W3D_CHUNK_HLOD_LOD_ARRAY);
                w_sub_object(b, 0, "Stray"); // not the array header
                b.end();
                b.begin(W3D_CHUNK_HLOD_LOD_ARRAY);
                w_array_header(b, 1, 10.0f);
                w_sub_object(b, 0, "M1");
                b.end();
                push("l19", b);
        }

        // l20: the array header declares two models but the wrapper holds
        // no sub-object chunks — the sub-array read fails balanced and is
        // discarded; the load continues (the aggregate after proves it)
        // with two zeroed entries.
        {
                ChunkBuild b;
                w_hlod_header(b, V10, 2, "OverDecl", "T");
                b.begin(W3D_CHUNK_HLOD_LOD_ARRAY);
                w_array_header(b, 1, 10.0f);
                w_sub_object(b, 0, "M0");
                b.end();
                b.begin(W3D_CHUNK_HLOD_LOD_ARRAY);
                w_array_header(b, 2, 20.0f);
                b.end();
                b.begin(W3D_CHUNK_HLOD_AGGREGATE_ARRAY);
                w_array_header(b, 1, 0.0f);
                w_sub_object(b, 0, "AGG");
                b.end();
                push("l20", b);
        }

        // l21: two models declared, one present — the sub-array read
        // fails balanced after the first entry; the load succeeds with
        // one read entry and one zeroed.
        {
                ChunkBuild b;
                w_hlod_header(b, V10, 1, "HalfRead", "T");
                b.begin(W3D_CHUNK_HLOD_LOD_ARRAY);
                w_array_header(b, 2, 20.0f);
                w_sub_object(b, 3, "First");
                b.end();
                b.begin(W3D_CHUNK_HLOD_PROXY_ARRAY);
                w_array_header(b, 1, 0.0f);
                w_sub_object(b, 1, "PX");
                b.end();
                push("l21", b);
        }

        // l22: a truncated sub-object payload (the chunk declares 36,
        // the file ends 20 bytes in) in the LAST LOD — the unbalanced
        // failure is discarded, the wrapper's close seeks past its
        // declared end, the load succeeds with a zeroed entry.
        {
                ChunkBuild b;
                w_hlod_header(b, V10, 2, "TruncSub", "T");
                b.begin(W3D_CHUNK_HLOD_LOD_ARRAY);
                w_array_header(b, 1, 10.0f);
                w_sub_object(b, 0, "M0");
                b.end();
                b.begin(W3D_CHUNK_HLOD_LOD_ARRAY);
                w_array_header(b, 1, 20.0f);
                size_t sub = b.bytes.size();
                w_sub_object(b, 0, "TruncatedModel"); // declares its full 36 bytes
                b.end();
                b.bytes.resize(sub + 8 + 20); // ...but the file ends 20 payload bytes in
                push("l22", b);
        }

        // l23: the same truncated sub-object in a NON-last LOD — the
        // next iteration finds the wrapper exhausted and fails the load.
        {
                ChunkBuild b;
                w_hlod_header(b, V10, 2, "TruncSubMid", "T");
                b.begin(W3D_CHUNK_HLOD_LOD_ARRAY);
                w_array_header(b, 1, 10.0f);
                size_t sub = b.bytes.size();
                w_sub_object(b, 0, "TruncatedModel"); // declares its full 36 bytes
                b.end();
                b.bytes.resize(sub + 8 + 20); // ...but the file ends 20 payload bytes in
                push("l23", b);
        }

        // l24: a short array-header read (the chunk declares 8, the file
        // ends 4 payload bytes in) in the LAST LOD — discarded; that LOD
        // keeps its constructed state.
        {
                ChunkBuild b;
                w_hlod_header(b, V10, 2, "ShortHdr", "T");
                b.begin(W3D_CHUNK_HLOD_LOD_ARRAY);
                w_array_header(b, 1, 10.0f);
                w_sub_object(b, 0, "M0");
                b.end();
                b.begin(W3D_CHUNK_HLOD_LOD_ARRAY);
                size_t hdr = b.bytes.size();
                b.begin(W3D_CHUNK_HLOD_SUB_OBJECT_ARRAY_HEADER);
                b.w_u32(1);
                b.w_f32(20.0f);
                b.end(); // the header chunk declares its full 8 bytes
                b.end(); // the wrapper closes over it
                b.bytes.resize(hdr + 8 + 4); // ...but the file ends 4 payload bytes in
                push("l24", b);
        }

        // l25: the same short array-header in a NON-last LOD — the next
        // iteration fails the load.
        {
                ChunkBuild b;
                w_hlod_header(b, V10, 2, "ShortHdrMid", "T");
                b.begin(W3D_CHUNK_HLOD_LOD_ARRAY);
                size_t hdr = b.bytes.size();
                b.begin(W3D_CHUNK_HLOD_SUB_OBJECT_ARRAY_HEADER);
                b.w_u32(1);
                b.w_f32(10.0f);
                b.end(); // the header chunk declares its full 8 bytes
                b.end(); // the wrapper closes over it
                b.bytes.resize(hdr + 8 + 4); // ...but the file ends 4 payload bytes in
                push("l25", b);
        }

        // l26: the aggregate array opens with a stray sub-object — the
        // unbalanced failure exits the trailing walk early, so the
        // proxy array after it is NEVER read (the shadowing quirk) and
        // the aggregates keep their constructed state.
        {
                ChunkBuild b;
                w_hlod_header(b, V10, 1, "Shadow", "T");
                b.begin(W3D_CHUNK_HLOD_LOD_ARRAY);
                w_array_header(b, 1, 10.0f);
                w_sub_object(b, 0, "M0");
                b.end();
                b.begin(W3D_CHUNK_HLOD_AGGREGATE_ARRAY);
                w_sub_object(b, 0, "Stray"); // not the array header
                w_sub_object(b, 1, "Stray2");
                b.end();
                b.begin(W3D_CHUNK_HLOD_PROXY_ARRAY);
                w_array_header(b, 1, 0.0f);
                w_sub_object(b, 1, "NeverRead");
                b.end();
                push("l26", b);
        }

        // l27: three proxies declared, one present — the balanced failure
        // is discarded; the two unread records keep their
        // default-constructed state, and the unknown chunk after is
        // still read (the walk continues at the top level).
        {
                ChunkBuild b;
                w_hlod_header(b, V10, 1, "HalfProxy", "T");
                b.begin(W3D_CHUNK_HLOD_LOD_ARRAY);
                w_array_header(b, 1, 10.0f);
                w_sub_object(b, 0, "M0");
                b.end();
                b.begin(W3D_CHUNK_HLOD_PROXY_ARRAY);
                w_array_header(b, 3, 0.0f);
                w_sub_object(b, 4, "Only");
                b.end();
                b.begin(0xDEADBEEFu);
                b.w_u32(1);
                b.end();
                push("l27", b);
        }

        // l28: last-wins — two aggregate arrays and two proxy arrays;
        // the second of each replaces the first (the second proxy array
        // is itself partial: two declared, one present).
        {
                ChunkBuild b;
                w_hlod_header(b, V10, 1, "LastWins", "T");
                b.begin(W3D_CHUNK_HLOD_LOD_ARRAY);
                w_array_header(b, 1, 10.0f);
                w_sub_object(b, 0, "M0");
                b.end();
                b.begin(W3D_CHUNK_HLOD_AGGREGATE_ARRAY);
                w_array_header(b, 1, 0.0f);
                w_sub_object(b, 0, "AGG_FIRST");
                b.end();
                b.begin(W3D_CHUNK_HLOD_AGGREGATE_ARRAY);
                w_array_header(b, 2, 0.0f);
                w_sub_object(b, 1, "AGG_A");
                w_sub_object(b, 2, "AGG_B");
                b.end();
                b.begin(W3D_CHUNK_HLOD_PROXY_ARRAY);
                w_array_header(b, 2, 0.0f);
                w_sub_object(b, 1, "PX_FIRST");
                w_sub_object(b, 2, "PX_SECOND");
                b.end();
                b.begin(W3D_CHUNK_HLOD_PROXY_ARRAY);
                w_array_header(b, 2, 0.0f);
                w_sub_object(b, 3, "PX_LAST");
                b.end();
                push("l28", b);
        }

        // l29: proxies before aggregates — the trailing walk handles the
        // two arrays in either order.
        {
                ChunkBuild b;
                w_hlod_header(b, V10, 1, "Order", "T");
                b.begin(W3D_CHUNK_HLOD_LOD_ARRAY);
                w_array_header(b, 1, 10.0f);
                w_sub_object(b, 0, "M0");
                b.end();
                b.begin(W3D_CHUNK_HLOD_PROXY_ARRAY);
                w_array_header(b, 1, 0.0f);
                w_sub_object(b, 2, "PX");
                b.end();
                b.begin(W3D_CHUNK_HLOD_AGGREGATE_ARRAY);
                w_array_header(b, 1, 0.0f);
                w_sub_object(b, 3, "AGG");
                b.end();
                push("l29", b);
        }

        // l30: unknown top-level chunks — a foreign id, a surplus
        // LOD_ARRAY after the loop and a bare array header / sub-object
        // pair — all skipped without touching the def.
        {
                ChunkBuild b;
                w_hlod_header(b, V10, 1, "Unknowns", "T");
                b.begin(W3D_CHUNK_HLOD_LOD_ARRAY);
                w_array_header(b, 1, 10.0f);
                w_sub_object(b, 0, "M0");
                b.end();
                b.begin(0xDEADBEEFu);
                b.w_u32(0);
                b.end();
                b.begin(W3D_CHUNK_HLOD_LOD_ARRAY); // surplus: the loop is done
                w_array_header(b, 1, 55.0f);
                w_sub_object(b, 9, "Surplus");
                b.end();
                b.begin(W3D_CHUNK_HLOD_SUB_OBJECT_ARRAY_HEADER); // no switch case
                b.w_u32(1);
                b.w_f32(66.0f);
                b.end();
                b.begin(W3D_CHUNK_HLOD_SUB_OBJECT); // no switch case
                w_sub_object(b, 8, "AlsoSurplus");
                b.end();
                push("l30", b);
        }

        // l31: empty aggregate and proxy wrappers (no children at all) —
        // both reads fail balanced and are discarded; the def holds no
        // proxy array, and the aggregate array after them is still read.
        {
                ChunkBuild b;
                w_hlod_header(b, V10, 1, "EmptyWrappers", "T");
                b.begin(W3D_CHUNK_HLOD_LOD_ARRAY);
                w_array_header(b, 1, 10.0f);
                w_sub_object(b, 0, "M0");
                b.end();
                b.begin(W3D_CHUNK_HLOD_AGGREGATE_ARRAY);
                b.end();
                b.begin(W3D_CHUNK_HLOD_PROXY_ARRAY);
                b.end();
                b.begin(W3D_CHUNK_HLOD_AGGREGATE_ARRAY);
                w_array_header(b, 1, 0.0f);
                w_sub_object(b, 0, "AGG_AFTER");
                b.end();
                push("l31", b);
        }

        // l32: err — the first chunk is a LOD array, not the header.
        {
                ChunkBuild b;
                w_hlod_header(b, V10, 1, "NoHeader", "T");
                b.bytes.erase(b.bytes.begin(), b.bytes.begin() + 48); // drop the header chunk
                push("l32", b);
        }

        // l33: err — no chunks at all (the empty payload).
        {
                ChunkBuild b;
                push("l33", b);
        }

        // l34: err — the header chunk declares 20 bytes; the 40-byte
        // struct read refuses to cross the declared end.
        {
                ChunkBuild b;
                size_t hdr = b.bytes.size();
                b.begin(W3D_CHUNK_HLOD_HEADER);
                b.w_u32(V10);
                b.w_u32(1);
                const char n[16] = {'S', 'm', 'a', 'l', 'l', 'H', 'd', 'r', 0, 0, 0, 0, 0, 0, 0, 0};
                const char h[16] = {'T', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0};
                b.w_bytes(n, 16);
                b.w_bytes(h, 16);
                b.end();
                b.patch_size(hdr, 20);
                push("l34", b);
        }

        // l35: err — the header chunk declares 40 but the file ends 20
        // payload bytes in (the stream-level short read).
        {
                ChunkBuild b;
                size_t hdr = b.bytes.size();
                b.begin(W3D_CHUNK_HLOD_HEADER);
                b.w_u32(V10);
                b.w_u32(1);
                const char n[16] = {'T', 'r', 'u', 'n', 'c', 'H', 'd', 'r', 0, 0, 0, 0, 0, 0, 0, 0};
                const char h[16] = {'T', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0};
                b.w_bytes(n, 16);
                b.w_bytes(h, 16);
                b.end();
                b.bytes.resize(hdr + 8 + 20); // keep only 20 of the 40 payload bytes
                push("l35", b);
        }

        // l36: err — LodCount 2 but only one LOD array chunk exists (the
        // loop's second Open_Chunk hits the exhausted file).
        {
                ChunkBuild b;
                w_hlod_header(b, V10, 2, "MissingLod", "T");
                b.begin(W3D_CHUNK_HLOD_LOD_ARRAY);
                w_array_header(b, 1, 10.0f);
                w_sub_object(b, 0, "M0");
                b.end();
                push("l36", b);
        }

        // l37: err — LodCount 2 but the second LOD position holds the
        // aggregate array (the id check fails).
        {
                ChunkBuild b;
                w_hlod_header(b, V10, 2, "AggEarly", "T");
                b.begin(W3D_CHUNK_HLOD_LOD_ARRAY);
                w_array_header(b, 1, 10.0f);
                w_sub_object(b, 0, "M0");
                b.end();
                b.begin(W3D_CHUNK_HLOD_AGGREGATE_ARRAY);
                w_array_header(b, 1, 0.0f);
                w_sub_object(b, 0, "AGG");
                b.end();
                push("l37", b);
        }

        // l38: err — the aggregate array before the LODs: the loop's
        // first id check fails.
        {
                ChunkBuild b;
                w_hlod_header(b, V10, 1, "AggFirst", "T");
                b.begin(W3D_CHUNK_HLOD_AGGREGATE_ARRAY);
                w_array_header(b, 1, 0.0f);
                w_sub_object(b, 0, "AGG");
                b.end();
                b.begin(W3D_CHUNK_HLOD_LOD_ARRAY);
                w_array_header(b, 1, 10.0f);
                w_sub_object(b, 0, "M0");
                b.end();
                push("l38", b);
        }

        // l39: a LOD wrapper carrying an unknown chunk after its models
        // — the sub-array read succeeds, the wrapper's close seeks past
        // the leftover child, the load succeeds.
        {
                ChunkBuild b;
                w_hlod_header(b, V10, 1, "Leftover", "T");
                b.begin(W3D_CHUNK_HLOD_LOD_ARRAY);
                w_array_header(b, 1, 10.0f);
                w_sub_object(b, 0, "M0");
                b.begin(0x00000BEEFu);
                b.w_u32(0);
                b.end();
                b.end();
                b.begin(W3D_CHUNK_HLOD_PROXY_ARRAY);
                w_array_header(b, 1, 0.0f);
                w_sub_object(b, 1, "PX");
                b.end();
                push("l39", b);
        }

        return out;
}

// --- the dump helpers ------------------------------------------------------------

static void hex_append(std::string &s, const uint8_t *p, size_t n)
{
        static const char digits[] = "0123456789abcdef";
        char buf[2];
        for (size_t i = 0; i < n; i++) {
                buf[0] = digits[p[i] >> 4];
                buf[1] = digits[p[i] & 0xF];
                s.append(buf, 2);
        }
}

static std::string hex_str(const std::vector<uint8_t> &v)
{
        if (v.empty())
                return "-";
        std::string s;
        hex_append(s, v.data(), v.size());
        return s;
}

static std::string f32hex(float f)
{
        uint32_t bits;
        memcpy(&bits, &f, 4);
        char buf[16];
        snprintf(buf, sizeof(buf), "%08x", bits);
        return std::string(buf);
}

static std::string u32hex(uint32_t v)
{
        char buf[16];
        snprintf(buf, sizeof(buf), "%08x", v);
        return std::string(buf);
}

// Names: '-' when empty, the raw bytes when printable non-space ASCII,
// `<hex>` otherwise.
static std::string q_str(const std::string &name)
{
        if (name.empty())
                return "-";
        for (unsigned char c : name) {
                if (c < 0x21 || c > 0x7e) {
                        std::string s = "<";
                        hex_append(s, (const uint8_t *)name.data(), name.size());
                        s += ">";
                        return s;
                }
        }
        return name;
}

static std::string q_cstr(const char *field, size_t cap)
{
        size_t len = 0;
        while (len < cap && field[len] != 0)
                len++;
        if (len == 0)
                return "-";
        for (size_t i = 0; i < len; i++) {
                unsigned char c = (unsigned char)field[i];
                if (c < 0x21 || c > 0x7e) {
                        std::string s = "<";
                        hex_append(s, (const uint8_t *)field, len);
                        s += ">";
                        return s;
                }
        }
        return std::string(field, len);
}

// The dump: everything the load path leaves behind.
static void dump_hlod(const HLodDefClass &def, FILE *out, const char *id)
{
        // the header line: the decoded version, the two strdup'd names
        // (run-through and cap included) and the LodCount member
        fprintf(out, "hdr %s %s %s %s %d\n", id, u32hex(def.Header.Version).c_str(), q_str(def.Name).c_str(),
                q_str(def.HierarchyTreeName).c_str(), def.LodCount);

        // the LOD arrays (exactly LodCount of them — the read_header
        // allocation) and the aggregates
        for (int i = 0; i < def.LodCount; i++) {
                const HLodDefClass::SubObjectArrayClass &arr = def.Lod[(size_t)i];
                fprintf(out, "arr %s l%d %s %d\n", id, i, f32hex(arr.MaxScreenSize).c_str(), arr.ModelCount);
                for (int j = 0; j < arr.ModelCount; j++) {
                        fprintf(out, "mdl %s l%d %d %s %d\n", id, i, j, q_str(arr.ModelName[(size_t)j]).c_str(),
                                arr.BoneIndex[(size_t)j]);
                }
        }
        {
                const HLodDefClass::SubObjectArrayClass &arr = def.Aggregates;
                fprintf(out, "arr %s agg %s %d\n", id, f32hex(arr.MaxScreenSize).c_str(), arr.ModelCount);
                for (int j = 0; j < arr.ModelCount; j++) {
                        fprintf(out, "mdl %s agg %d %s %d\n", id, j, q_str(arr.ModelName[(size_t)j]).c_str(),
                                arr.BoneIndex[(size_t)j]);
                }
        }

        // the proxies: '-' when the def holds no array
        if (def.ProxyArray != nullptr) {
                fprintf(out, "proxies %s %d\n", id, def.ProxyArray->Length());
                for (int i = 0; i < def.ProxyArray->Length(); i++) {
                        const ProxyRecordClass &rec = (*def.ProxyArray)[i];
                        fprintf(out, "prx %s %d %s %d\n", id, i, q_cstr(rec.Get_Name(), 2 * W3D_NAME_LEN).c_str(),
                                rec.Get_Bone_Index());
                }
        } else {
                fprintf(out, "proxies %s -\n", id);
        }
}

} // namespace

// --- main -------------------------------------------------------------------------

int main(int argc, char **argv)
{
        if (argc != 2) {
                fprintf(stderr, "usage: gen_hlod_golden <output-file>\n");
                return 1;
        }

        FILE *out = fopen(argv[1], "w");
        if (!out) {
                fprintf(stderr, "error: cannot open %s for writing\n", argv[1]);
                return 1;
        }

        fprintf(out, "# gen_hlod_golden output — the W3D HLOD assembly corpus (P0)\n");
        fprintf(out, "# generated by tools/differential/gen_hlod_golden.cpp; do not edit\n");

        for (const Corpus &c : build_corpora()) {
                SrcFile file;
                file.data = c.bytes.data();
                file.len = c.bytes.size();
                ChunkLoadClass cload(&file);

                fprintf(out, "file %s %s\n", c.id, hex_str(c.bytes).c_str());

                HLodDefClass def;
                int error = def.Load_W3D(cload);
                if (error == HLodDefClass::WW3D_ERROR_OK) {
                        fprintf(out, "ok %s\n", c.id);
                        dump_hlod(def, out, c.id);
                } else {
                        fprintf(out, "err %s\n", c.id);
                }
        }

        fclose(out);
        return 0;
}
