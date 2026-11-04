// SPDX-License-Identifier: GPL-3.0-or-later
//
// Golden-vector driver for `rzh-formats::chunk` (parity level P0).
//
// Verbatim transcription of the baseline tree's chunk container —
// Core/Libraries/Source/WWVegas/WWLib/chunkio.{h,cpp} at commit 571a7c5 —
// the ChunkSaveClass/ChunkLoadClass pair every W3D asset walks through,
// with the FileClass plumbing replaced by a byte source (reads) and a
// seekable byte sink (writes; the writer patches chunk headers in place,
// so the sink overwrites, not just appends). The algorithms, the stack
// bookkeeping and the quirks are the original code, comments and all.
//
// Deliberate deviations, pinned identically on both sides:
//
// - The IOVector READ helpers keep the original's `sizeof(v)` bug — v is
//   a POINTER there, so the game's 32-bit build reads exactly 4 bytes,
//   not the 8/12/16 of the struct. A 64-bit build would read 8, so the
//   transcription pins the on-disk meaning (4) in one constant. The
//   WRITE helpers take references (`sizeof(v)` = the struct size) and
//   are correct; they are transcribed unchanged. No caller in the game
//   tree uses the read helpers (checked: IOVector3Struct only aliases
//   W3dVectorStruct in w3d_file.h); they are pinned as found.
// - Corpora stay inside the original's debug-assert envelope (reader
//   depth <= 255, writer depth < 256, micro chunk totals <= 254): the
//   states beyond the asserts are release-build UB (out-of-bounds stack
//   writes), which the Rust port turns into errors instead. The 255-deep
//   reader corpus pins the assert boundary exactly.
// - Chunk sizes near 0x7FFFFFFF make the original's int position
//   accounting overflow (PositionStack += csize + 8 is signed-overflow
//   UB); the corpora cap declared sizes at 0x40000000, which stays
//   clear of the overflow while still pinning the legal seek-past-EOF
//   jump.
//
// Output grammar (one command per line, hex lowercase, '-' = empty):
//
//   file <id> <hex>              the corpus bytes (writer output or
//                                hand-assembled)
//   gen <id> <hex>               the transcribed writer's bytes (writer
//                                corpora; must equal `file`)
//   wdepth <id> <n>              the writer's Cur_Chunk_Depth at its
//                                deepest point (writer corpora)
//   chunk <id> <depth> <type-hex> <len> <C|->   walk A: a chunk opened
//                                (C = the contains-sub-chunks flag)
//   data <id> <hex|->            walk A: the payload read back from a
//                                data chunk (pieces of <= 64 bytes)
//   tell <id> <n>                walk A: Tell() after the walk
//   mchunk <id> <depth> <mtype> <mlen>   walk B: a micro chunk opened
//   mdata <id> <hex|->           walk B: the payload read back from the
//                                micro chunk
//   mtell <id> <n>               walk B: Tell() after the walk
//   step <id> <op> <args...>     scripted corpora: one observed step
//                                (open id len contains depth read seek
//                                 mopen mid mlen close mclose tell)
//
// Walk A reads every data chunk's payload (pieces of min(64, remaining)
// bytes until a read returns 0) and then closes; walk B walks the micro
// chunks of every data chunk first (the read side of the micro format)
// instead of reading payloads. Real W3D loaders know statically which
// lens to apply; the two walks approximate both sides of that choice and
// pin the reader mechanics under each.
//
// Usage: gen_chunk_golden <output-file>
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
#include <cassert>
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

// --- the format constants and structures (chunkio.h) --------------------------

struct ChunkHeader
{
        // (The original's convenience constructors are dropped so the
        // struct stays POD and the original's memsets compile clean.)

        // Use these accessors to ensure you correctly deal with the data in the chunk header
        void            Set_Type(uint32_t type)                                   { ChunkType = type; }
        uint32_t        Get_Type()                                                        { return ChunkType; }
        void            Set_Size(uint32_t size)                                   { ChunkSize &= 0x80000000; ChunkSize |= (size & 0x7FFFFFFF); }
        void            Add_Size(uint32_t add)                                            { Set_Size(Get_Size() + add); }
        uint32_t        Get_Size()                                                              { return (ChunkSize & 0x7FFFFFFF); }
        void            Set_Sub_Chunk_Flag(bool onoff)          { if (onoff) { ChunkSize |= 0x80000000; } else { ChunkSize &= 0x7FFFFFFF; } }
        int             Get_Sub_Chunk_Flag()                            { return (ChunkSize & 0x80000000); }

        // Chunk type and size.
        // Note: MSB of ChunkSize is used to indicate whether this chunk
        // contains other chunks or data.
        uint32_t ChunkType;
        uint32_t ChunkSize;
};

struct MicroChunkHeader
{
        void            Set_Type(uint8_t type)                                            { ChunkType = type; }
        uint8_t         Get_Type()                                                                { return ChunkType; }
        void            Set_Size(uint8_t size)                                            { ChunkSize = size; }
        void            Add_Size(uint8_t add)                                             { Set_Size(Get_Size() + add); }
        uint8_t         Get_Size()                                                                { return ChunkSize; }

        uint8_t ChunkType;
        uint8_t ChunkSize;
};

// --- the IOVector structures (iostruct.h) --------------------------------------

struct IOVector2Struct { float X; float Y; };
struct IOVector3Struct { float X; float Y; float Z; };
struct IOVector4Struct { float X; float Y; float Z; float W; };
struct IOQuaternionStruct { float Q[4]; };

// The original's Read(IOVector*) overloads pass sizeof(v) — the size of
// the POINTER. The game's 32-bit build (MSVC x86) reads 4 bytes; a 64-bit
// transcription would read 8. Pin the on-disk meaning of the bug:
static const uint32_t IO_VECTOR_READ_SIZE = 4;

// --- the file plumbing (stands in for FileClass) -------------------------------
//
// Read side: a const byte buffer with RawFileClass semantics — short
// reads at EOF return the short count (and advance the position), seeks
// are legal past the end. Write side: a byte vector with a position;
// writes overwrite in range and extend beyond (the chunk writer seeks
// back to patch headers, then returns to the end).

struct SrcFile
{
        const uint8_t *data = nullptr;
        size_t len = 0;
        long pos = 0;

        long tell() const { return pos; }

        long seek(long offset, int origin)
        {
                long target;
                if (origin == 0)
                        target = offset; // SEEK_SET
                else if (origin == 1)
                        target = pos + offset; // SEEK_CUR
                else if (origin == 2)
                        target = (long)len + offset; // SEEK_END
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

struct SinkFile
{
        std::vector<uint8_t> bytes;
        size_t pos = 0;

        long tell() const { return (long)pos; }

        long seek(long offset, int origin)
        {
                long target;
                if (origin == 0)
                        target = offset;
                else if (origin == 1)
                        target = (long)pos + offset;
                else if (origin == 2)
                        target = (long)bytes.size() + offset;
                else
                        return -1;
                if (target < 0)
                        return -1;
                if ((size_t)target > bytes.size())
                        bytes.resize((size_t)target, 0); // POSIX sparse-write shape
                pos = (size_t)target;
                return (long)pos;
        }

        uint32_t write(const void *buffer, uint32_t size)
        {
                const uint8_t *p = (const uint8_t *)buffer;
                if (pos + (size_t)size > bytes.size())
                        bytes.resize(pos + (size_t)size, 0);
                memcpy(bytes.data() + pos, p, size);
                pos += (size_t)size;
                return size;
        }
};

#define SEEK_SET 0
#define SEEK_CUR 1
#define SEEK_END 2

// --- ChunkSaveClass (chunkio.cpp, verbatim; FileClass* -> SinkFile*) -----------

class ChunkSaveClass
{
public:
        ChunkSaveClass(SinkFile * file);

        // Chunk methods
        bool                                    Begin_Chunk(uint32_t id);
        bool                                    End_Chunk();
        int                                     Cur_Chunk_Depth();

        // Micro chunk methods
        bool                                    Begin_Micro_Chunk(uint32_t id);
        bool                                    End_Micro_Chunk();

        // Write data into the file
        uint32_t                        Write(const void *buf, uint32_t nbytes);
        uint32_t                        Write(const IOVector2Struct & v);
        uint32_t                        Write(const IOVector3Struct & v);
        uint32_t                        Write(const IOVector4Struct & v);
        uint32_t                        Write(const IOQuaternionStruct & q);

private:

        enum { MAX_STACK_DEPTH = 256 };

        SinkFile *                      File;

        // Chunk building support
        int                                     StackIndex;
        int                                     PositionStack[MAX_STACK_DEPTH];
        ChunkHeader                     HeaderStack[MAX_STACK_DEPTH];

        // MicroChunk building support
        bool                                    InMicroChunk;
        int                                     MicroChunkPosition;
        MicroChunkHeader        MCHeader;
};

ChunkSaveClass::ChunkSaveClass(SinkFile * file) :
        File(file),
        StackIndex(0),
        InMicroChunk(false),
        MicroChunkPosition(0)
{
        memset(PositionStack,0,sizeof(PositionStack));
        memset(HeaderStack,0,sizeof(HeaderStack));
        memset(&MCHeader,0,sizeof(MCHeader));
}

bool ChunkSaveClass::Begin_Chunk(uint32_t id)
{
        ChunkHeader     chunkh{}; // value-init: the original's ctor zeroed it
        int                     filepos;

        // If we have a parent chunk, set its 'Contains_Chunks' flag
        if (StackIndex > 0) {
                HeaderStack[StackIndex-1].Set_Sub_Chunk_Flag(true);
        }

        // Save the current file position and chunk header
        // for the call to End_Chunk.
        chunkh.Set_Type(id);
        chunkh.Set_Size(0);
        filepos = (int)File->seek(0,SEEK_CUR);

        PositionStack[StackIndex] = filepos;
        HeaderStack[StackIndex] = chunkh;
        StackIndex++;

        // write a temporary chunk header (size = 0)
        if (File->write(&chunkh,sizeof(chunkh)) != sizeof(chunkh)) {
                return false;
        }
        return true;
}

bool ChunkSaveClass::End_Chunk()
{
        // If the user didn't close his micro chunks bad things are gonna happen
        assert(!InMicroChunk);

        // Save the current position
        int curpos = (int)File->seek(0,SEEK_CUR);

        // Pop the position and chunk header off the stacks
        StackIndex--;
        int chunkpos = PositionStack[StackIndex];
        ChunkHeader chunkh = HeaderStack[StackIndex];

        // write the completed header
        File->seek(chunkpos,SEEK_SET);
        if (File->write(&chunkh,sizeof(chunkh)) != sizeof(chunkh)) {
                return false;
        }

        // Add the total bytes written to any encompasing chunk
        if (StackIndex != 0) {
                HeaderStack[StackIndex-1].Add_Size(chunkh.Get_Size() + sizeof(chunkh));
        }

        // Go back to the end of the file
        File->seek(curpos,SEEK_SET);

        return true;
}

bool ChunkSaveClass::Begin_Micro_Chunk(uint32_t id)
{
        assert(id < 256);
        assert(!InMicroChunk);

        // Save the current file position and chunk header
        // for the call to End_Micro_Chunk.
        MCHeader.Set_Type((uint8_t)id);
        MCHeader.Set_Size(0);
        MicroChunkPosition = (int)File->seek(0,SEEK_CUR);

        // Write a temporary chunk header
        // NOTE: I'm calling the ChunkSaveClass::Write method so that the bytes for
        // this header are tracked in the wrapping chunk.  This is because micro-chunks
        // are simply data inside the normal chunks...
        if (Write(&MCHeader,sizeof(MCHeader)) != sizeof(MCHeader)) {
                return false;
        }

        InMicroChunk = true;
        return true;
}

bool ChunkSaveClass::End_Micro_Chunk()
{
        assert(InMicroChunk);

        // Save the current position
        int curpos = (int)File->seek(0,SEEK_CUR);

        // Seek back and write the micro chunk header
        File->seek(MicroChunkPosition,SEEK_SET);
        if (File->write(&MCHeader,sizeof(MCHeader)) != sizeof(MCHeader)) {
                return false;
        }

        // Go back to the end of the file
        File->seek(curpos,SEEK_SET);
        InMicroChunk = false;
        return true;
}

uint32_t ChunkSaveClass::Write(const void * buf, uint32_t nbytes)
{
        // If this assert hits, you mixed data and chunks within the same chunk NO NO!
        assert(HeaderStack[StackIndex-1].Get_Sub_Chunk_Flag() == 0);

        // If this assert hits, you didnt open any chunks yet
        assert(StackIndex > 0);

        // write the bytes into the file
        if (File->write(buf,nbytes) != nbytes) return 0;

        // track them in the wrapping chunk
        HeaderStack[StackIndex-1].Add_Size(nbytes);

        // track them if you are using a micro-chunk too.
        if (InMicroChunk) {
                assert(MCHeader.Get_Size() < 255 - nbytes);     // micro chunks can only be 255 bytes
                MCHeader.Add_Size(nbytes);
        }

        return nbytes;
}

uint32_t ChunkSaveClass::Write(const IOVector2Struct & v)
{
        return Write(&v,sizeof(v));
}

uint32_t ChunkSaveClass::Write(const IOVector3Struct & v)
{
        return Write(&v,sizeof(v));
}

uint32_t ChunkSaveClass::Write(const IOVector4Struct & v)
{
        return Write(&v,sizeof(v));
}

uint32_t ChunkSaveClass::Write(const IOQuaternionStruct & q)
{
        return Write(&q,sizeof(q));
}

[[maybe_unused]] int ChunkSaveClass::Cur_Chunk_Depth()
{
        return StackIndex;
}

// --- ChunkLoadClass (chunkio.cpp, verbatim; FileClass* -> SrcFile*) ------------

class ChunkLoadClass
{
public:

        ChunkLoadClass(SrcFile * file);

        // Chunk methods
        bool                                    Open_Chunk();
        bool                                    Close_Chunk();
        uint32_t                        Cur_Chunk_ID();
        uint32_t                        Cur_Chunk_Length();
        int                                     Cur_Chunk_Depth();
        int                                     Contains_Chunks();

        // Micro Chunk methods
        bool                                    Open_Micro_Chunk();
        bool                                    Close_Micro_Chunk();
        uint32_t                        Cur_Micro_Chunk_ID();
        uint32_t                        Cur_Micro_Chunk_Length();

        // Read a block of bytes from the output stream.
        uint32_t                        Read(void *buf, uint32_t nbytes);
        uint32_t                        Read(IOVector2Struct * v);
        uint32_t                        Read(IOVector3Struct * v);
        uint32_t                        Read(IOVector4Struct * v);
        uint32_t                        Read(IOQuaternionStruct * q);

        // Seek over a block of bytes in the stream (same as Read but don't copy the data to a buffer)
        uint32_t                        Seek(uint32_t nbytes);

        // View-only addition the golden dump reads (the original queries
        // File->Tell() directly).
        long                                     Tell() { return File->tell(); }

private:

        enum { MAX_STACK_DEPTH = 256 };

        SrcFile *                       File;

        // Chunk reading support
        int                                     StackIndex;
        uint32_t                        PositionStack[MAX_STACK_DEPTH];
        ChunkHeader                     HeaderStack[MAX_STACK_DEPTH];

        // Micro-chunk reading support
        bool                                    InMicroChunk;
        int                                     MicroChunkPosition;
        MicroChunkHeader        MCHeader;
};

ChunkLoadClass::ChunkLoadClass(SrcFile * file) :
        File(file),
        StackIndex(0),
        InMicroChunk(false),
        MicroChunkPosition(0)
{
        memset(PositionStack,0,sizeof(PositionStack));
        memset(HeaderStack,0,sizeof(HeaderStack));
        memset(&MCHeader,0,sizeof(MCHeader));
}

bool ChunkLoadClass::Open_Chunk()
{
        // if user didn't close any micro chunks that he opened, bad things could happen
        assert(InMicroChunk == false);

        // check for stack overflow
        assert(StackIndex < MAX_STACK_DEPTH-1);

        // if the parent chunk has been completely eaten, return false
        if ((StackIndex > 0) && (PositionStack[StackIndex-1] == HeaderStack[StackIndex-1].Get_Size())) {
                return false;
        }

        // read the chunk header
        if (File->read(&HeaderStack[StackIndex],sizeof(ChunkHeader)) != sizeof(ChunkHeader)) {
                return false;
        }

        PositionStack[StackIndex] = 0;
        StackIndex++;
        return true;
}

bool ChunkLoadClass::Close_Chunk()
{
        // if user didn't close any micro chunks that he opened, bad things could happen
        assert(InMicroChunk == false);

        // check for stack overflow
        assert(StackIndex > 0);

        int csize = (int)HeaderStack[StackIndex-1].Get_Size();
        int pos = (int)PositionStack[StackIndex-1];

        if (pos < csize) {
                File->seek(csize - pos,SEEK_CUR);
        }

        StackIndex--;
        if (StackIndex > 0) {
                PositionStack[StackIndex - 1] += csize + sizeof(ChunkHeader);
        }

        return true;
}

uint32_t ChunkLoadClass::Cur_Chunk_ID()
{
        assert(StackIndex >= 1);
        return HeaderStack[StackIndex-1].Get_Type();
}

uint32_t ChunkLoadClass::Cur_Chunk_Length()
{
        assert(StackIndex >= 1);
        return HeaderStack[StackIndex-1].Get_Size();
}

int ChunkLoadClass::Cur_Chunk_Depth()
{
        return StackIndex;
}

int ChunkLoadClass::Contains_Chunks()
{
        return HeaderStack[StackIndex-1].Get_Sub_Chunk_Flag();
}

bool ChunkLoadClass::Open_Micro_Chunk()
{
        assert(!InMicroChunk);

        // read the chunk header
        // calling the ChunkLoadClass::Read fn so that if we exhaust the chunk, the read will fail
        if (Read(&MCHeader,sizeof(MCHeader)) != sizeof(MCHeader)) {
                return false;
        }

        InMicroChunk = true;
        MicroChunkPosition = 0;
        return true;
}

bool ChunkLoadClass::Close_Micro_Chunk()
{
        assert(InMicroChunk);
        InMicroChunk = false;

        int csize = MCHeader.Get_Size();
        int pos = MicroChunkPosition;

        // seek the file past this micro chunk
        if (pos < csize) {

                File->seek(csize - pos,SEEK_CUR);

                // update the tracking variables for where we are in the normal chunk.
                if (StackIndex > 0) {
                        PositionStack[StackIndex-1] += csize - pos;
                }
        }

        return true;
}

uint32_t ChunkLoadClass::Cur_Micro_Chunk_ID()
{
        assert(InMicroChunk);
        return MCHeader.Get_Type();
}

uint32_t ChunkLoadClass::Cur_Micro_Chunk_Length()
{
        assert(InMicroChunk);
        return MCHeader.Get_Size();
}

// Seek over nbytes in the stream
[[maybe_unused]] uint32_t ChunkLoadClass::Seek(uint32_t nbytes)
{
        assert(StackIndex >= 1);

        // Don't seek if we would go past the end of the current chunk
        if (PositionStack[StackIndex-1] + nbytes > (uint32_t)HeaderStack[StackIndex-1].Get_Size()) {
                return 0;
        }

        // Don't read if we are in a micro chunk and would go past the end of it
        if (InMicroChunk && MicroChunkPosition + nbytes > MCHeader.Get_Size()) {
                return 0;
        }

        uint32_t curpos=(uint32_t)File->tell();
        if ((uint32_t)File->seek(nbytes,SEEK_CUR)-curpos != nbytes) {
                return 0;
        }

        // Update our position in the chunk
        PositionStack[StackIndex-1] += nbytes;

        // Update our position in the micro chunk if we are in one
        if (InMicroChunk) {
                MicroChunkPosition += nbytes;
        }

        return nbytes;
}

uint32_t ChunkLoadClass::Read(void * buf,uint32_t nbytes)
{
        assert(StackIndex >= 1);

        // Don't read if we would go past the end of the current chunk
        if (PositionStack[StackIndex-1] + nbytes > (uint32_t)HeaderStack[StackIndex-1].Get_Size()) {
                return 0;
        }

        // Don't read if we are in a micro chunk and would go past the end of it
        if (InMicroChunk && MicroChunkPosition + nbytes > MCHeader.Get_Size()) {
                return 0;
        }

        if (File->read(buf,nbytes) != nbytes) {
                return 0;
        }

        // Update our position in the chunk
        PositionStack[StackIndex-1] += nbytes;

        // Update our position in the micro chunk if we are in one
        if (InMicroChunk) {
                MicroChunkPosition += nbytes;
        }

        return nbytes;
}

// The original passes sizeof(v) — the size of the POINTER. The game's
// 32-bit build reads 4 bytes; pin that meaning (see the header comment).
uint32_t ChunkLoadClass::Read(IOVector2Struct * v)
{
        assert(v != nullptr);
        return Read(v,IO_VECTOR_READ_SIZE);
}

uint32_t ChunkLoadClass::Read(IOVector3Struct * v)
{
        assert(v != nullptr);
        return Read(v,IO_VECTOR_READ_SIZE);
}

uint32_t ChunkLoadClass::Read(IOVector4Struct * v)
{
        assert(v != nullptr);
        return Read(v,IO_VECTOR_READ_SIZE);
}

uint32_t ChunkLoadClass::Read(IOQuaternionStruct * q)
{
        assert(q != nullptr);
        return Read(q,IO_VECTOR_READ_SIZE);
}

// --- output helpers -------------------------------------------------------------

static std::string g_out;

static void emit(const std::string &line)
{
        g_out += line;
        g_out += '\n';
}

static void emit(const char *line)
{
        g_out += line;
        g_out += '\n';
}

static std::string hex_of(const uint8_t *bytes, size_t n)
{
        if (n == 0)
                return "-";
        static const char *digits = "0123456789abcdef";
        std::string out;
        out.reserve(n * 2);
        for (size_t i = 0; i < n; ++i)
        {
                out.push_back(digits[bytes[i] >> 4]);
                out.push_back(digits[bytes[i] & 0xF]);
        }
        return out;
}

static std::string hex_of(const std::vector<uint8_t> &v)
{
        return hex_of(v.data(), v.size());
}

static std::string hex_u32(uint32_t v)
{
        char buf[16];
        snprintf(buf, sizeof(buf), "%x", v);
        return buf;
}

static std::string dec(long v)
{
        char buf[32];
        snprintf(buf, sizeof(buf), "%ld", v);
        return buf;
}

// The formula corpus bytes (kept in lockstep with the Rust test).
static std::vector<uint8_t> formula(size_t n)
{
        std::vector<uint8_t> out(n);
        for (size_t i = 0; i < n; ++i)
                out[i] = (uint8_t)((i * 7 + 13) & 0xFF);
        return out;
}

// --- the walks (mirrored by the Rust golden test) -------------------------------
//
// Walk A: payload lens. Every data chunk's payload is read back in
// pieces of min(64, remaining) bytes until a read returns 0.
// Walk B: micro lens. Every data chunk's micro chunks are walked (their
// payloads read the same way) instead of the raw payload.

static void walk_a(ChunkLoadClass &cload, const char *id)
{
        while (cload.Open_Chunk()) {
                emit("chunk " + std::string(id) + " " + dec(cload.Cur_Chunk_Depth()) + " "
                        + hex_u32(cload.Cur_Chunk_ID()) + " " + dec(cload.Cur_Chunk_Length()) + " "
                        + (cload.Contains_Chunks() ? "C" : "-"));
                if (cload.Contains_Chunks()) {
                        walk_a(cload, id);
                } else {
                        std::vector<uint8_t> payload;
                        uint32_t remaining = cload.Cur_Chunk_Length();
                        uint8_t buf[64];
                        while (remaining > 0) {
                                uint32_t piece = remaining < 64 ? remaining : 64;
                                uint32_t n = cload.Read(buf, piece);
                                if (n == 0)
                                        break;
                                payload.insert(payload.end(), buf, buf + n);
                                remaining -= n;
                        }
                        emit("data " + std::string(id) + " " + hex_of(payload));
                }
                cload.Close_Chunk();
        }
}

static void walk_b(ChunkLoadClass &cload, const char *id)
{
        while (cload.Open_Chunk()) {
                if (cload.Contains_Chunks()) {
                        walk_b(cload, id);
                } else {
                        while (cload.Open_Micro_Chunk()) {
                                emit("mchunk " + std::string(id) + " " + dec(cload.Cur_Chunk_Depth()) + " "
                                        + dec(cload.Cur_Micro_Chunk_ID()) + " "
                                        + dec(cload.Cur_Micro_Chunk_Length()));
                                std::vector<uint8_t> payload;
                                uint32_t remaining = cload.Cur_Micro_Chunk_Length();
                                uint8_t buf[64];
                                while (remaining > 0) {
                                        uint32_t piece = remaining < 64 ? remaining : 64;
                                        uint32_t n = cload.Read(buf, piece);
                                        if (n == 0)
                                                break;
                                        payload.insert(payload.end(), buf, buf + n);
                                        remaining -= n;
                                }
                                emit("mdata " + std::string(id) + " " + hex_of(payload));
                                cload.Close_Micro_Chunk();
                        }
                }
                cload.Close_Chunk();
        }
}

// Runs both walks over a corpus and emits the tell lines.
static void run_walks(const std::vector<uint8_t> &bytes, const char *id)
{
        {
                SrcFile src{bytes.data(), bytes.size(), 0};
                ChunkLoadClass cload(&src);
                walk_a(cload, id);
                emit("tell " + std::string(id) + " " + dec(cload.Tell()));
        }
        {
                SrcFile src{bytes.data(), bytes.size(), 0};
                ChunkLoadClass cload(&src);
                walk_b(cload, id);
                emit("mtell " + std::string(id) + " " + dec(cload.Tell()));
        }
}

static void emit_file(const char *id, const std::vector<uint8_t> &bytes)
{
        emit("file " + std::string(id) + " " + hex_of(bytes));
}

// --- the writer corpora (mirrored by the Rust golden test) ----------------------

struct WriterCorpus
{
        const char *id;
        std::vector<uint8_t> bytes;
        int max_depth;
};

static void write_formula(ChunkSaveClass &csave, size_t n)
{
        std::vector<uint8_t> data = formula(n);
        csave.Write(data.data(), (uint32_t)n);
}

static std::vector<WriterCorpus> build_writer_corpora()
{
        std::vector<WriterCorpus> out;

        {
                // one data chunk with a formula payload
                SinkFile sink;
                ChunkSaveClass csave(&sink);
                csave.Begin_Chunk(0x5E);
                write_formula(csave, 100);
                csave.End_Chunk();
                out.push_back({"wr_simple", sink.bytes, 1});
        }
        {
                // nested containers, an empty data chunk, two top-level chunks
                SinkFile sink;
                ChunkSaveClass csave(&sink);
                csave.Begin_Chunk(0x100);
                csave.Begin_Chunk(1);
                write_formula(csave, 12);
                csave.End_Chunk();
                csave.Begin_Chunk(2);
                csave.Begin_Chunk(3);
                write_formula(csave, 4);
                csave.End_Chunk();
                csave.Begin_Chunk(4);       // empty data chunk
                csave.End_Chunk();
                csave.End_Chunk();
                csave.Begin_Chunk(5);
                write_formula(csave, 28);
                csave.End_Chunk();
                csave.End_Chunk();
                csave.Begin_Chunk(6);
                write_formula(csave, 8);
                csave.End_Chunk();
                out.push_back({"wr_nested", sink.bytes, 3});
        }
        {
                // micro chunks (a zero-size one, a 254-byte one at the debug
                // assert boundary) then trailing raw data in the same chunk
                SinkFile sink;
                ChunkSaveClass csave(&sink);
                csave.Begin_Chunk(0x200);
                csave.Begin_Micro_Chunk(1);
                write_formula(csave, 4);
                csave.End_Micro_Chunk();
                csave.Begin_Micro_Chunk(2); // zero-size micro chunk
                csave.End_Micro_Chunk();
                csave.Begin_Micro_Chunk(3);
                write_formula(csave, 17);
                csave.End_Micro_Chunk();
                csave.Begin_Micro_Chunk(255);
                write_formula(csave, 254);
                csave.End_Micro_Chunk();
                write_formula(csave, 9);    // raw data after the micro chunks
                csave.End_Chunk();
                out.push_back({"wr_micro", sink.bytes, 1});
        }
        {
                // 60 levels of nesting, data only at the innermost
                SinkFile sink;
                ChunkSaveClass csave(&sink);
                for (uint32_t i = 0; i < 60; ++i)
                        csave.Begin_Chunk(0x10 + i);
                write_formula(csave, 5);
                for (uint32_t i = 0; i < 60; ++i)
                        csave.End_Chunk();
                out.push_back({"wr_deep", sink.bytes, 60});
        }
        {
                // the full u32 id range on chunks, 0 and 255 on micro ids
                SinkFile sink;
                ChunkSaveClass csave(&sink);
                const uint32_t ids[] = {0, 1, 0x7FFFFFFF, 0xFFFFFFFF, 0x80000000};
                for (uint32_t id : ids) {
                        csave.Begin_Chunk(id);
                        csave.Begin_Chunk(0xABCDEF01);
                        write_formula(csave, 3);
                        csave.End_Chunk();
                        csave.End_Chunk();
                }
                csave.Begin_Chunk(0x555);
                csave.Begin_Micro_Chunk(0);
                write_formula(csave, 2);
                csave.End_Micro_Chunk();
                csave.Begin_Micro_Chunk(255);
                write_formula(csave, 1);
                csave.End_Micro_Chunk();
                csave.End_Chunk();
                out.push_back({"wr_ids", sink.bytes, 2});
        }
        {
                // the IOVector write helpers (full structs; the READ side
                // is the buggy one — see the scripted corpora)
                SinkFile sink;
                ChunkSaveClass csave(&sink);
                csave.Begin_Chunk(0x300);
                IOVector2Struct v2 = {1.5f, -2.5f};
                IOVector3Struct v3 = {1.0f, 2.0f, 3.0f};
                IOVector4Struct v4 = {4.0f, 3.0f, 2.0f, 1.0f};
                IOQuaternionStruct q = {0.5f, 0.5f, 0.5f, 0.5f};
                csave.Write(v2);
                csave.Write(v3);
                csave.Write(v4);
                csave.Write(q);
                csave.End_Chunk();
                out.push_back({"wr_vecs", sink.bytes, 1});
        }
        {
                // a bigger payload accumulated over several writes
                SinkFile sink;
                ChunkSaveClass csave(&sink);
                csave.Begin_Chunk(0x400);
                for (int i = 0; i < 8; ++i)
                        write_formula(csave, 64);
                csave.End_Chunk();
                out.push_back({"wr_big", sink.bytes, 1});
        }

        return out;
}

// --- the reader corpora (hand-assembled; the writer cannot produce them) --------

static void push_u32(std::vector<uint8_t> &out, uint32_t v)
{
        out.push_back((uint8_t)(v & 0xFF));
        out.push_back((uint8_t)((v >> 8) & 0xFF));
        out.push_back((uint8_t)((v >> 16) & 0xFF));
        out.push_back((uint8_t)((v >> 24) & 0xFF));
}

// A chunk header: type, size; msb sets the contains-sub-chunks flag bit.
static void push_header(std::vector<uint8_t> &out, uint32_t type, uint32_t size, bool msb)
{
        push_u32(out, type);
        push_u32(out, msb ? (size | 0x80000000u) : size);
}

struct ReaderCorpus
{
        const char *id;
        std::vector<uint8_t> bytes;
};

static std::vector<ReaderCorpus> build_reader_corpora()
{
        std::vector<ReaderCorpus> out;

        out.push_back({"rd_empty", {}});

        {
                std::vector<uint8_t> b = formula(7); // one byte short of a header
                out.push_back({"rd_stub", b});
        }
        {
                std::vector<uint8_t> b;
                push_header(b, 7, 0, false);
                out.push_back({"rd_zerolen", b});
        }
        {
                // a container with two data children, then a top-level data chunk
                std::vector<uint8_t> b;
                std::vector<uint8_t> child1 = formula(12);
                std::vector<uint8_t> child2 = formula(4);
                size_t payload = (8 + child1.size()) + (8 + child2.size());
                push_header(b, 2, (uint32_t)payload, true);
                push_header(b, 3, (uint32_t)child1.size(), false);
                b.insert(b.end(), child1.begin(), child1.end());
                push_header(b, 4, (uint32_t)child2.size(), false);
                b.insert(b.end(), child2.begin(), child2.end());
                std::vector<uint8_t> top = formula(6);
                push_header(b, 5, (uint32_t)top.size(), false);
                b.insert(b.end(), top.begin(), top.end());
                out.push_back({"rd_topmix", b});
        }
        {
                // a data chunk whose size word carries the contains flag:
                // the flag is advisory to readers (Read never checks it)
                std::vector<uint8_t> b;
                std::vector<uint8_t> payload = formula(8);
                push_header(b, 9, (uint32_t)payload.size(), true);
                b.insert(b.end(), payload.begin(), payload.end());
                out.push_back({"rd_msbdata", b});
        }
        {
                // a child over-declaring its size past its parent's bounds:
                // the header read is unclamped and the parent's exhaustion
                // test is equality, so the walk leaves the parent's boundary
                std::vector<uint8_t> b;
                push_header(b, 5, 16, true);
                push_header(b, 6, 100, false);
                std::vector<uint8_t> tail = formula(8);
                b.insert(b.end(), tail.begin(), tail.end());
                out.push_back({"rd_overdeclared", b});
        }
        {
                // a payload that stops short of the declared size
                std::vector<uint8_t> b;
                push_header(b, 0x11, 32, false);
                std::vector<uint8_t> tail = formula(12);
                b.insert(b.end(), tail.begin(), tail.end());
                out.push_back({"rd_truncated", b});
        }
        {
                // same shape as rd_truncated; the scripted steps below pin
                // the short read's consumes-then-fails quirk explicitly
                std::vector<uint8_t> b;
                push_header(b, 0x12, 32, false);
                std::vector<uint8_t> tail = formula(12);
                b.insert(b.end(), tail.begin(), tail.end());
                out.push_back({"rd_short", b});
        }
        {
                // a 1 GiB declared size: the close seek jumps legally past
                // the end of the file (kept under the original's int
                // overflow boundary)
                std::vector<uint8_t> b;
                push_header(b, 0x77, 0x40000000u, false);
                std::vector<uint8_t> tail = formula(4);
                b.insert(b.end(), tail.begin(), tail.end());
                out.push_back({"rd_hugesize", b});
        }
        {
                // a micro header straddling its parent's end fails cleanly
                std::vector<uint8_t> b;
                push_header(b, 0x88, 7, false);
                std::vector<uint8_t> head = formula(6);
                b.insert(b.end(), head.begin(), head.end());
                b.push_back(0x07);
                out.push_back({"rd_micro_straddle", b});
        }
        {
                // a micro chunk over-declaring past its parent's bounds:
                // Close_Micro_Chunk jumps unclamped and the parent's
                // counter overruns (its exhaustion test is equality)
                std::vector<uint8_t> b;
                push_header(b, 0x89, 6, false);
                b.push_back(0x07); // micro id
                b.push_back(0x06); // micro size 6, only 2 bytes exist
                std::vector<uint8_t> tail = formula(2);
                b.insert(b.end(), tail.begin(), tail.end());
                out.push_back({"rd_micro_overrun", b});
        }
        {
                // the reader's full legal depth: 255 nested chunks
                std::vector<uint8_t> b;
                for (uint32_t i = 0; i < 255; ++i) {
                        bool innermost = (i == 254);
                        push_header(b, 0x20 + i, innermost ? 0 : 8, !innermost);
                }
                out.push_back({"rd_deep255", b});
        }
        {
                // the IOVector write helpers' output (52 bytes), walked by
                // the scripted steps below through the buggy read helpers
                SinkFile sink;
                ChunkSaveClass csave(&sink);
                csave.Begin_Chunk(0x300);
                IOVector2Struct v2 = {1.5f, -2.5f};
                IOVector3Struct v3 = {1.0f, 2.0f, 3.0f};
                IOVector4Struct v4 = {4.0f, 3.0f, 2.0f, 1.0f};
                IOQuaternionStruct q = {0.5f, 0.5f, 0.5f, 0.5f};
                csave.Write(v2);
                csave.Write(v3);
                csave.Write(v4);
                csave.Write(q);
                csave.End_Chunk();
                out.push_back({"rd_vecs", sink.bytes});
        }

        return out;
}

// --- the scripted corpora (hand-driven steps pinning the odd paths) -------------
//
// Each script mirrors the Rust golden test's replay step for step; the
// corpus bytes come from the reader corpora built above (looked up by id,
// so the `file` line is emitted exactly once).

static const std::vector<uint8_t> &find_corpus(
        const std::vector<ReaderCorpus> &corpora, const char *id)
{
        for (const ReaderCorpus &c : corpora)
                if (strcmp(c.id, id) == 0)
                        return c.bytes;
        fprintf(stderr, "corpus %s not found\n", id);
        abort();
}

static void run_scripts(const std::vector<ReaderCorpus> &corpora)
{
        // rd_msbdata: reading data out of a contains-flagged chunk (Read
        // never consults the flag).
        {
                const std::vector<uint8_t> &b = find_corpus(corpora, "rd_msbdata");
                SrcFile src{b.data(), b.size(), 0};
                ChunkLoadClass cload(&src);
                emit("step rd_msbdata open " + dec(cload.Open_Chunk()));
                emit("step rd_msbdata id " + hex_u32(cload.Cur_Chunk_ID()));
                emit("step rd_msbdata len " + dec(cload.Cur_Chunk_Length()));
                emit("step rd_msbdata contains " + dec(cload.Contains_Chunks() != 0));
                emit("step rd_msbdata depth " + dec(cload.Cur_Chunk_Depth()));
                uint8_t buf[16];
                uint32_t n = cload.Read(buf, 8);
                emit("step rd_msbdata read " + dec(n) + " " + hex_of(buf, n));
                n = cload.Read(buf, 1);
                emit("step rd_msbdata read " + dec(n) + " " + hex_of(buf, n));
                emit("step rd_msbdata close " + dec(cload.Close_Chunk()));
                emit("step rd_msbdata tell " + dec(cload.Tell()));
        }

        // rd_short: the short read consumes the available bytes and still
        // reports failure; the close seek then jumps past the end.
        {
                const std::vector<uint8_t> &b = find_corpus(corpora, "rd_short");
                SrcFile src{b.data(), b.size(), 0};
                ChunkLoadClass cload(&src);
                emit("step rd_short open " + dec(cload.Open_Chunk()));
                emit("step rd_short id " + hex_u32(cload.Cur_Chunk_ID()));
                uint8_t buf[32];
                uint32_t n = cload.Read(buf, 17); // 12 available: consumed, failed
                emit("step rd_short read " + dec(n) + " " + hex_of(buf, n));
                emit("step rd_short tell " + dec(cload.Tell()));
                n = cload.Read(buf, 17); // nothing left: no advance
                emit("step rd_short read " + dec(n) + " " + hex_of(buf, n));
                emit("step rd_short tell " + dec(cload.Tell()));
                emit("step rd_short close " + dec(cload.Close_Chunk()));
                emit("step rd_short tell " + dec(cload.Tell()));
        }

        // rd_micro_straddle: a micro header that would cross its parent's
        // declared end fails cleanly (the clamp fires before the read).
        {
                const std::vector<uint8_t> &b = find_corpus(corpora, "rd_micro_straddle");
                SrcFile src{b.data(), b.size(), 0};
                ChunkLoadClass cload(&src);
                emit("step rd_micro_straddle open " + dec(cload.Open_Chunk()));
                uint8_t buf[16];
                uint32_t n = cload.Read(buf, 6);
                emit("step rd_micro_straddle read " + dec(n) + " " + hex_of(buf, n));
                emit("step rd_micro_straddle mopen " + dec(cload.Open_Micro_Chunk()));
                n = cload.Read(buf, 1); // the last parent byte is readable
                emit("step rd_micro_straddle read " + dec(n) + " " + hex_of(buf, n));
                n = cload.Read(buf, 1);
                emit("step rd_micro_straddle read " + dec(n) + " " + hex_of(buf, n));
                emit("step rd_micro_straddle close " + dec(cload.Close_Chunk()));
                emit("step rd_micro_straddle tell " + dec(cload.Tell()));
        }

        // rd_micro_overrun: Close_Micro_Chunk jumps unclamped past the
        // parent's bounds and the parent's counter overruns.
        {
                const std::vector<uint8_t> &b = find_corpus(corpora, "rd_micro_overrun");
                SrcFile src{b.data(), b.size(), 0};
                ChunkLoadClass cload(&src);
                emit("step rd_micro_overrun open " + dec(cload.Open_Chunk()));
                emit("step rd_micro_overrun mopen " + dec(cload.Open_Micro_Chunk()));
                emit("step rd_micro_overrun mid " + dec(cload.Cur_Micro_Chunk_ID()));
                emit("step rd_micro_overrun mlen " + dec(cload.Cur_Micro_Chunk_Length()));
                uint8_t buf[16];
                uint32_t n = cload.Read(buf, 2);
                emit("step rd_micro_overrun read " + dec(n) + " " + hex_of(buf, n));
                emit("step rd_micro_overrun mclose " + dec(cload.Close_Micro_Chunk()));
                emit("step rd_micro_overrun open " + dec(cload.Open_Chunk()));
                emit("step rd_micro_overrun close " + dec(cload.Close_Chunk()));
                emit("step rd_micro_overrun tell " + dec(cload.Tell()));
        }

        // rd_vecs: the IOVector read helpers' pointer-size bug — each
        // helper consumes exactly 4 bytes of the 8/12/16/16 the write
        // side produced, filling only the first component.
        {
                const std::vector<uint8_t> &b = find_corpus(corpora, "rd_vecs");
                SrcFile src{b.data(), b.size(), 0};
                ChunkLoadClass cload(&src);
                emit("step rd_vecs open " + dec(cload.Open_Chunk()));
                IOVector2Struct r2 = {0.0f, 0.0f};
                IOVector3Struct r3 = {0.0f, 0.0f, 0.0f};
                IOVector4Struct r4 = {0.0f, 0.0f, 0.0f, 0.0f};
                IOQuaternionStruct rq = {0.0f, 0.0f, 0.0f, 0.0f};
                uint32_t n = cload.Read(&r2);
                emit("step rd_vecs read " + dec(n));
                n = cload.Read(&r3);
                emit("step rd_vecs read " + dec(n));
                n = cload.Read(&r4);
                emit("step rd_vecs read " + dec(n));
                n = cload.Read(&rq);
                emit("step rd_vecs read " + dec(n));
                // what the 32-bit build saw: the first component of each
                // struct, with the rest left stale (zero-initialized here)
                char buf[256];
                snprintf(buf, sizeof(buf), "step rd_vecs vecs %.6g %.6g %.6g %.6g %.6g %.6g %.6g %.6g",
                        (double)r2.X, (double)r2.Y,
                        (double)r3.X, (double)r3.Y,
                        (double)r4.X, (double)r4.Y,
                        (double)rq.Q[0], (double)rq.Q[1]);
                emit(buf);
                // the remaining payload is still readable after the four
                // 4-byte helper reads: 52 - 16 = 36 bytes
                uint8_t rest[36];
                n = cload.Read(rest, 36);
                emit("step rd_vecs read " + dec(n) + " " + hex_of(rest, n));
                emit("step rd_vecs close " + dec(cload.Close_Chunk()));
                emit("step rd_vecs tell " + dec(cload.Tell()));
        }
}

} // namespace

// --- main -----------------------------------------------------------------------

int main(int argc, char **argv)
{
        if (argc != 2) {
                fprintf(stderr, "usage: gen_chunk_golden <output-file>\n");
                return 1;
        }

        emit("# gen_chunk_golden.cpp — chunkio.cpp transcription (baseline 571a7c5).");
        emit("# Golden script for rzh-formats::chunk; see tools/differential/README.md.");
        emit("# Grammar: file/gen/wdepth/chunk/data/tell/mchunk/mdata/mtell/step.");

        // The writer corpora: both directions (the bytes, and the walk
        // over the bytes), plus the writer's depth pin.
        for (const WriterCorpus &c : build_writer_corpora()) {
                emit_file(c.id, c.bytes);
                emit("gen " + std::string(c.id) + " " + hex_of(c.bytes));
                emit("wdepth " + std::string(c.id) + " " + dec(c.max_depth));
                run_walks(c.bytes, c.id);
        }

        // The reader corpora: the walk over the hand-assembled bytes.
        std::vector<ReaderCorpus> readers = build_reader_corpora();
        for (const ReaderCorpus &c : readers) {
                emit_file(c.id, c.bytes);
                run_walks(c.bytes, c.id);
        }

        // The scripted corpora' step dumps (their bytes are already
        // emitted above).
        run_scripts(readers);

        FILE *f = fopen(argv[1], "wb");
        if (!f) {
                fprintf(stderr, "cannot open %s\n", argv[1]);
                return 1;
        }
        fwrite(g_out.data(), 1, g_out.size(), f);
        fclose(f);
        return 0;
}
