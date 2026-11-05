// SPDX-License-Identifier: GPL-3.0-or-later
//
// Golden-dump driver for `rzh-formats::w3d_hanim` (parity level P0).
//
// Verbatim transcription of the Zero Hour raw-animation loader —
// GeneralsMD/Code/Libraries/Source/WWVegas/WW3D2/hrawanim.cpp at commit
// 571a7c5 (HRawAnimClass::Load_W3D, read_channel, read_bit_channel,
// add_channel, add_bit_channel, Get_Translation, Get_Orientation,
// Get_Transform, Get_Visibility, Is_Node_Motion_Present and the Has_*
// probes; the SPECIAL_GETVEC_AS_QUAT branch of Get_Orientation is the
// one motchan.h's #define compiles), plus the pieces those paths drive:
//   GeneralsMD/Code/Libraries/Source/WWVegas/WW3D2/motchan.cpp
//     (MotionChannelClass::Load_W3D with its exporter-junk seek and the
//      disabled Do_Data_Compression — its first statement is `return;` —
//      BitChannelClass::Load_W3D, Get_Vector, Get_Vector_As_Quat,
//      Get_Bit, set_identity)
//   GeneralsMD/Code/Libraries/Source/WWVegas/WW3D2/w3d_file.h
//     (W3dAnimHeaderStruct, W3dAnimChannelStruct, W3dBitChannelStruct)
//   Core/Libraries/Source/WWVegas/WWMath/wwmath.{h,cpp}
//     (Float_To_Long — the deterministic gm_lrintf branch,
//      Float_To_Int_Floor, Fast_Acos/Fast_Sin over the WWMath::Init
//      tables, WWMATH_EPSILON/PI)
//   Core/Libraries/Source/WWVegas/WWMath/quat.cpp (Fast_Slerp — the
//     portable #else branch; the x86-asm #if 0 block is not compiled)
//   Core/Libraries/Source/WWVegas/WWMath/quat.h (Build_Matrix3D — the
//     mixed f64/f32 literal builder), vector3.h (Vector3::Lerp),
//     matrix3d.h (Set_Translation)
// The pinned GameMath sources (59f7ccd4) supply gm_lrintf, gm_acosf and
// gm_sinf; the fast tables are computed by the WWMath::Init
// transcription from them, exactly like the game's startup.
// The chunk container is the chunkio.cpp reader already pinned by
// gen_chunk_golden.cpp; it is transcribed here again (release-build
// semantics) because this harness is self-contained. strlcat is the
// canonical BSD body (the loader's platform provides one; every
// implementation agrees).
//
// The corpus bytes are the payload of the W3D_CHUNK_ANIMATION container
// the asset manager opens before calling HAnimManagerClass::Load_Raw_Anim
// (assetmgr.cpp / hanimmgr.cpp); the harness enters the loader exactly
// as the manager does. The skeleton the animation resolves against is
// modelled by the tree table at the top of the output: Get_HTree is
// Get_Tree (htreemgr.cpp) — lowercase the name, exact hash lookup —
// over the registered (lowercased name, pivot count) pairs.
//
// Deliberate deviations and corpus disciplines, pinned identically on
// both sides (the Rust port documents the same set):
//
// - The strcpy that builds the class's HierarchyName member runs through
//   the header's NumFrames and FrameRate words when the 16-byte field
//   has no NUL. The copy itself is deterministic in-struct behaviour and
//   IS pinned (a24's 17-byte key, a25's key miss). Corpora keep the
//   string's NUL inside the field or the NumFrames word — past that the
//   class-member reassignments make the observable member a
//   layout-dependent result that is not reproducible.
//
// - An empty or negative channel span (LastFrame < FirstFrame, or
//   VectorLen 0) fails the load in both. The C++ reaches that verdict
//   through a zero-size allocation, a one-element write past it and a
//   follow-up read whose byte count has wrapped to ~4 GiB (release
//   semantics: the read fails, the load errors); the port states the
//   contract instead. Corpora avoid spans whose frame count the harness
//   could not allocate (the C++ `new float32[huge]` would throw — not a
//   load verdict).
//
// - Corpora use finite float payloads and quaternion channels whose
//   pair dot products stay inside [0, 1] (the exporter normalizes);
//   NaN data and slerp inputs above 1 reach the original's
//   undefined bit-trick paths.
//
// - The hierarchy lookups resolve against lowercase keys, matching
//   Get_Tree's _strlwr; the anim's hierarchy name may be mixed case
//   (a18 pins the folding).
//
// - The frame probes are a fixed list plus two counts-derived frames
//   when 1 <= NumFrames <= 1000 (both sides compute the same list); the
//   per-corpus queries run for pivots 0..min(8, NumNodes) plus the last
//   pivot, keeping the dump bounded for the 24-bone corpus.
//
// Output grammar (one command per line, hex lowercase, '-' = empty):
//
//   tree <name|-> <count>     a registered hierarchy (lowercase key)
//   file <id> <hex|->         the corpus bytes (animation payload)
//   ok <id> | err <id>        the load verdict
//   hdr <id> <ver> <name|-> <hname|-> <numframes> <frerate> <totaltime> <numnodes>
//                             ver: version as 8 hex digits; name: the
//                             composed Get_Name; hname: Get_HName;
//                             numframes: the member count; frerate /
//                             totaltime: f32 bit words (the numeric
//                             FrameRate conversion and NumFrames/FrameRate)
//   has <id> <piv> <x> <y> <z> <rot> <vis> <present>
//                             the Has_* probes and Is_Node_Motion_Present,
//                             0|1 each
//   chan <id> <piv> <slot> <type> <first> <last> <veclen> <words...>
//                             one stored motion channel; slot is one of
//                             x y z xr yr zr q; words: the frame data as
//                             f32 bit words
//   bchan <id> <piv> <type> <first> <last> <default> <bytes>
//                             one stored bit channel; bytes: the packed
//                             bits as hex
//   trans <id> <piv> <frame> <x> <y> <z>
//                             Get_Translation at the probe frame; frame
//                             and results are f32 bit words
//   orient <id> <piv> <frame> <x> <y> <z> <w>
//                             Get_Orientation
//   xform <id> <piv> <frame> <m0..m11>
//                             Get_Transform, row-major
//   vis <id> <piv> <frame> <0|1>
//                             Get_Visibility
//
// Usage: gen_hanim_golden <output-file>
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
#include <cassert>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <math.h>
#include <string>
#include <vector>

// The pinned GameMath (FreeBSD msun) library, exactly the revision the C++
// oracle links (cmake/gamemath.cmake, pin 59f7ccd4). Only the entry points
// the WWMath transcription needs are declared here (gmath.h itself clashes
// with the C++ standard headers' BSD macros); they link against the pin's
// sources, matching the game's USE_DETERMINISTIC_MATH = 1 build.
extern "C" {
long gm_lrintf(float);
float gm_acosf(float);
float gm_sinf(float);
}

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

// enum continuation values at 0x00000200 in w3d_file.h
static const uint32_t W3D_CHUNK_ANIMATION = 0x00000200;       // hierarchy animation data
static const uint32_t W3D_CHUNK_ANIMATION_HEADER = 0x00000201; // W3dAnimHeaderStruct
static const uint32_t W3D_CHUNK_ANIMATION_CHANNEL = 0x00000202; // channel of vectors
static const uint32_t W3D_CHUNK_BIT_CHANNEL = 0x00000203;     // channel of boolean values

// channel types (the Flags field)
enum
{
        ANIM_CHANNEL_X = 0,
        ANIM_CHANNEL_Y,
        ANIM_CHANNEL_Z,
        ANIM_CHANNEL_XR,
        ANIM_CHANNEL_YR,
        ANIM_CHANNEL_ZR,
        ANIM_CHANNEL_Q,
};
enum
{
        BIT_CHANNEL_VIS = 0,
};

// --- the W3D structures (w3d_file.h, POD) -------------------------------------

struct W3dAnimHeaderStruct
{
        uint32_t Version;
        char Name[W3D_NAME_LEN];         // Name of the animation
        char HierarchyName[W3D_NAME_LEN]; // Name of the hierarchy tree this animation works with
        uint32_t NumFrames;               // Number of frames in the animation
        uint32_t FrameRate;               // Number of frames per second (an integer!)
};

struct W3dAnimChannelStruct
{
        uint16_t FirstFrame;
        uint16_t LastFrame;
        uint16_t VectorLen; // length of each vector in this channel
        uint16_t Flags;     // channel type.
        uint16_t Pivot;     // pivot affected by this channel
        uint16_t pad;
        float Data[1];      // will be (LastFrame - FirstFrame + 1) * VectorLen long
};

struct W3dBitChannelStruct
{
        uint16_t FirstFrame; // all frames outside "First" and "Last" are assumed = DefaultVal
        uint16_t LastFrame;
        uint16_t Flags; // channel type.
        uint16_t Pivot; // pivot affected by this channel
        uint8_t DefaultVal; // default state when outside valid range.
        uint8_t Data[1];    // will be (LastFrame - FirstFrame + 1) / 8 long
};

static_assert(sizeof(W3dAnimHeaderStruct) == 44, "W3dAnimHeaderStruct layout");
static_assert(sizeof(W3dAnimChannelStruct) == 16, "W3dAnimChannelStruct layout");
static_assert(sizeof(W3dBitChannelStruct) == 10, "W3dBitChannelStruct layout");

// --- the ASCII case folding (the manager's _strlwr) ----------------------------

static unsigned char ascii_tolower(unsigned char c)
{
        return (c >= 'A' && c <= 'Z') ? (unsigned char)(c - 'A' + 'a') : c;
}

// --- strlcat (the canonical BSD body) ------------------------------------------

struct StrBuf
{
        char data[64];
        size_t len; // strlen(data)

        StrBuf()
        {
                data[0] = 0;
                len = 0;
        }
};

static size_t sstrlcat(char *dst, const char *src, size_t siz)
{
        char *d = dst;
        const char *s = src;
        size_t n = siz;
        size_t dlen;

        /* Find the end of dst and adjust bytes left but don't go past end */
        while (n-- != 0 && *d != '\0')
                d++;
        dlen = d - dst;
        n = siz - dlen;

        if (n == 0)
                return (dlen + strlen(s)); /* didn't copy, caller must check */
        while (*s != '\0') {
                if (n != 1) {
                        *d++ = *s;
                        n--;
                }
                s++;
        }
        *d = '\0';
        return (dlen + (s - src)); /* count does not include NUL */
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

        long tell() const { return pos; }

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
        uint32_t Cur_Chunk_Length();
        uint32_t Read(void *buf, uint32_t nbytes);
        uint32_t Seek(uint32_t nbytes);

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
        // release build: the depth assert is skipped
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

uint32_t ChunkLoadClass::Seek(uint32_t nbytes)
{
        // release build: the StackIndex assert is skipped (the loader only
        // seeks with a chunk open)

        // Don't seek if we would go past the end of the current chunk
        if (PositionStack[StackIndex-1] + nbytes > HeaderStack[StackIndex-1].Get_Size()) {
                return 0;
        }

        uint32_t curpos = File->tell();
        if ((uint32_t)File->seek(nbytes, SEEK_CUR) - curpos != nbytes) {
                return 0;
        }

        // Update our position in the chunk
        PositionStack[StackIndex-1] += nbytes;

        return nbytes;
}

// --- the WWMath subset (wwmath.h / wwmath.cpp / quat.cpp / quat.h) --------------

#define WWMATH_EPSILON 0.0001f
#define WWMATH_PI 3.141592654f

static const int ARC_TABLE_SIZE = 1024;
static const int SIN_TABLE_SIZE = 1024;

static float _FastAcosTable[ARC_TABLE_SIZE];
static float _FastSinTable[SIN_TABLE_SIZE];
static bool _wwmath_ready = false;

// WWMath::Init — the table half the anim queries read (the asin and
// inv-sin tables exist too; no anim path touches them)
static void WWMath_Init()
{
        if (_wwmath_ready)
                return;
        int a = 0;
        for (; a < ARC_TABLE_SIZE; ++a) {
                float cv = float(a - ARC_TABLE_SIZE / 2) * (1.0f / (ARC_TABLE_SIZE / 2));
                _FastAcosTable[a] = gm_acosf(cv); // Acosf (non-retail branch)
        }

        for (a = 0; a < SIN_TABLE_SIZE; ++a) {
                float cv = (float)a * 2.0f * WWMATH_PI / SIN_TABLE_SIZE;
                _FastSinTable[a] = gm_sinf(cv); // Sinf (non-retail branch)
        }
        _wwmath_ready = true;
}

// WWMath::Float_To_Long — the deterministic branch: gm_lrintf
static inline long WWFloat_To_Long(float f)
{
        return gm_lrintf(f);
}

// WWMath::Float_To_Int_Floor (wwmath.h)
static inline int WWFloat_To_Int_Floor(float f)
{
        int a;
        memcpy(&a, &f, 4); // take bit pattern of float into a register
        int sign = (a >> 31); // sign = 0xFFFFFFFF if original value is negative, 0 if positive
        a &= 0x7fffffff; // we don't need the sign any more

        int exponent = (a >> 23) - 127; // extract the exponent
        int expsign = ~(exponent >> 31); // 0xFFFFFFFF if exponent is positive, 0 otherwise
        int imask = ((1 << (31 - (exponent))) - 1); // mask for true integer values
        int mantissa = (a & ((1 << 23) - 1)); // extract mantissa (without the hidden bit)
        int r = ((unsigned int)(mantissa | (1 << 23)) << 8) >> (31 - exponent);

        // if (fabs(value)<1.0) value = 0; copy sign;
        // if (value < 0 && value==(int)(value)) value++
        // (the original's `!x & y` — a logical not folded into a bitwise
        // and — written with the explicit != 0 test the compiler wants)
        int low_bits_zero = (((mantissa << 8) & imask) != 0) ? 0 : 1;
        r = ((r & expsign) ^ (sign)) + ((low_bits_zero & (expsign ^ ((a - 1) >> 31))) & sign);
        return r;
}

// WWMath::Fast_Acos (wwmath.h)
static inline float WWFast_Acos(float val)
{
        // Near -1 and +1, the table becomes too inaccurate
        if (fabsf(val) > 0.975f) {
                return gm_acosf(val); // Acos (deterministic branch)
        }

        val *= float(ARC_TABLE_SIZE / 2);

        int idx0 = WWFloat_To_Int_Floor(val);
        int idx1 = idx0 + 1;
        float frac = val - (float)idx0;

        idx0 += ARC_TABLE_SIZE / 2;
        idx1 += ARC_TABLE_SIZE / 2;

        // compute and return the interpolated value
        return (1.0f - frac) * _FastAcosTable[idx0] + frac * _FastAcosTable[idx1];
}

// WWMath::Fast_Sin (wwmath.h)
static inline float WWFast_Sin(float val)
{
        val *= float(SIN_TABLE_SIZE) / (2.0f * WWMATH_PI);

        int idx0 = WWFloat_To_Int_Floor(val);
        int idx1 = idx0 + 1;
        float frac = val - (float)idx0;

        idx0 = ((unsigned)idx0) & (SIN_TABLE_SIZE - 1);
        idx1 = ((unsigned)idx1) & (SIN_TABLE_SIZE - 1);

        return (1.0f - frac) * _FastSinTable[idx0] + frac * _FastSinTable[idx1];
}

// Fast_Slerp (quat.cpp, the portable #else branch — the one that compiles)
static void Fast_Slerp(float *res, const float *p, const float *q, float alpha)
{
        float beta;    // complementary interploation parameter
        float theta;   // angle between p and q
        float cos_t;   // sine, cosine of theta
        float oo_sin_t;
        int qflip;     // use flip of q?

        // cos theta = dot product of p and q
        cos_t = p[0] * q[0] + p[1] * q[1] + p[2] * q[2] + p[3] * q[3];

        // if q is on opposite hemisphere from A, use -B instead
        if (cos_t < 0.0f) {
                cos_t = -cos_t;
                qflip = true;
        } else {
                qflip = false;
        }

        if (1.0f - cos_t < WWMATH_EPSILON * WWMATH_EPSILON) {

                // if q is very close to p, just linearly interpolate
                // between the two.
                beta = 1.0f - alpha;

        } else {

                theta = WWFast_Acos(cos_t);
                float sin_t = WWFast_Sin(theta);
                oo_sin_t = 1.0f / sin_t;
                beta = WWFast_Sin(theta - alpha * theta) * oo_sin_t;
                alpha = WWFast_Sin(alpha * theta) * oo_sin_t;
        }

        if (qflip) {
                alpha = -alpha;
        }

        res[0] = beta * p[0] + alpha * q[0];
        res[1] = beta * p[1] + alpha * q[1];
        res[2] = beta * p[2] + alpha * q[2];
        res[3] = beta * p[3] + alpha * q[3];
}

// Build_Matrix3D (quat.h WWINLINE) — the literal mix (double 2.0
// everywhere except the [1][1] term's 2.0f) is the original's.
static void Build_Matrix3D(const float *q, float *mtx /* 12 floats, row-major */)
{
        mtx[0] = (float)(1.0 - 2.0 * (double)(q[1] * q[1] + q[2] * q[2]));
        mtx[1] = (float)(2.0 * (double)(q[0] * q[1] - q[2] * q[3]));
        mtx[2] = (float)(2.0 * (double)(q[2] * q[0] + q[1] * q[3]));

        mtx[4] = (float)(2.0 * (double)(q[0] * q[1] + q[2] * q[3]));
        mtx[5] = (float)(1.0 - (double)(2.0f * (q[2] * q[2] + q[0] * q[0])));
        mtx[6] = (float)(2.0 * (double)(q[1] * q[2] - q[0] * q[3]));

        mtx[8] = (float)(2.0 * (double)(q[2] * q[0] - q[1] * q[3]));
        mtx[9] = (float)(2.0 * (double)(q[1] * q[2] + q[0] * q[3]));
        mtx[10] = (float)(1.0 - 2.0 * (double)(q[1] * q[1] + q[0] * q[0]));

        // no translation
        mtx[3] = mtx[7] = mtx[11] = 0.0f;
}

// Vector3::Lerp (vector3.h) and Matrix3D::Set_Translation (matrix3d.h)
static inline float V3Lerp(float a, float b, float alpha)
{
        return a + (b - a) * alpha;
}

// --- the classic channels (motchan.cpp) ----------------------------------------

class MotionChannelClass
{
public:
        MotionChannelClass()
            : PivotIdx(0), Type(0), VectorLen(0), ValueOffset(0.0f), ValueScale(0.0f),
              CompressedData(nullptr), Data(nullptr), FirstFrame(-1), LastFrame(-1)
        {
        }
        ~MotionChannelClass() { Free(); }

        bool Load_W3D(ChunkLoadClass &cload);

        int Get_Type() const { return (int)Type; }
        uint32_t Get_Pivot() const { return PivotIdx; }
        void Set_Pivot(uint32_t idx) { PivotIdx = idx; }

        void Get_Vector(int frame, float *setvec) const
        {
                if ((frame < FirstFrame) || (frame > LastFrame)) {
                        set_identity(setvec);
                } else {
                        int vframe = frame - FirstFrame;
                        for (int i = 0; i < VectorLen; i++) {
                                setvec[i] = Data[vframe * VectorLen + i];
                        }
                }
        }

        void Get_Vector_As_Quat(int frame, float *quat) const
        {
                if ((frame < FirstFrame) || (frame > LastFrame)) {
                        quat[0] = 0.0f;
                        quat[1] = 0.0f;
                        quat[2] = 0.0f;
                        quat[3] = 1.0f;
                } else {
                        const float *d = &Data[(frame - FirstFrame) * VectorLen];
                        quat[0] = d[0];
                        quat[1] = d[1];
                        quat[2] = d[2];
                        quat[3] = d[3];
                }
        }

        uint32_t PivotIdx; // what pivot is this channel applied to
        uint32_t Type;     // what type of channel is this
        int VectorLen;     // size of each individual vector

        float ValueOffset;
        float ValueScale;
        unsigned short *CompressedData;

        float *Data; // pointer to the raw floating point data
        int FirstFrame; // first frame which was non-identity
        int LastFrame;  // last frame which was non-identity

        void Do_Data_Compression(int /*datasize*/)
        {
                // the shipped build's entire effective body: the
                // compression is disabled (the original's first statement
                // is `return;` — the code below it is unreachable and
                // omitted here; nothing observable depends on it)
                return;
        }

private:
        void Free()
        {
                delete[] CompressedData;
                CompressedData = nullptr;
                delete[] Data;
                Data = nullptr;
        }
        void set_identity(float *setvec) const
        {
                if (Type == ANIM_CHANNEL_Q) {
                        setvec[0] = 0.0f;
                        setvec[1] = 0.0f;
                        setvec[2] = 0.0f;
                        setvec[3] = 1.0f;
                } else {
                        setvec[0] = 0.0f;
                }
        }
};

bool MotionChannelClass::Load_W3D(ChunkLoadClass &cload)
{
        int size = cload.Cur_Chunk_Length();
        // There was a bug in the exporter which saved too much data, so let's try and not load everything.
        unsigned int saved_datasize = (size - sizeof(W3dAnimChannelStruct));

        W3dAnimChannelStruct chan;
        if (cload.Read(&chan, sizeof(W3dAnimChannelStruct)) != sizeof(W3dAnimChannelStruct)) {
                return false;
        }

        FirstFrame = chan.FirstFrame;
        LastFrame = chan.LastFrame;
        VectorLen = chan.VectorLen;
        Type = chan.Flags;
        PivotIdx = chan.Pivot;

        unsigned int num_floats = LastFrame - FirstFrame + 1; //(datasize / sizeof(float)) + 1;
        num_floats *= VectorLen;
        unsigned int datasize = (num_floats - 1) * sizeof(float);

        Data = new float[num_floats];
        Data[0] = chan.Data[0];

        if (cload.Read(&(Data[1]), datasize) != datasize) {
                Free();
                return false;
        }
        // Skip over the extra data at the end of the chunk (saved by an error in the exporter)
        if (saved_datasize - datasize > 0) {
                cload.Seek(saved_datasize - datasize);
        }

        Do_Data_Compression(datasize);
        return true;
}

class BitChannelClass
{
public:
        BitChannelClass() : PivotIdx(0), Type(0), DefaultVal(0), FirstFrame(-1), LastFrame(-1), Bits(nullptr) {}
        ~BitChannelClass() { Free(); }

        bool Load_W3D(ChunkLoadClass &cload);

        int Get_Type() const { return (int)Type; }
        uint32_t Get_Pivot() const { return PivotIdx; }

        int Get_Bit(int frame) const
        {
                if ((frame < FirstFrame) || (frame > LastFrame)) {
                        return DefaultVal;
                } else {
                        int bit = frame - FirstFrame;
                        uint8_t mask = (uint8_t)(1 << (bit % 8));
                        return ((*(Bits + (bit / 8)) & mask) != 0);
                }
        }

        uint32_t PivotIdx;
        uint32_t Type;
        int DefaultVal;
        int FirstFrame;
        int LastFrame;

        uint8_t *Bits;

private:
        void Free()
        {
                delete[] Bits;
                Bits = nullptr;
        }
};

bool BitChannelClass::Load_W3D(ChunkLoadClass &cload)
{
        Free();

        W3dBitChannelStruct chan;
        if (cload.Read(&chan, sizeof(W3dBitChannelStruct)) != sizeof(W3dBitChannelStruct)) {
                return false;
        }

        FirstFrame = chan.FirstFrame;
        LastFrame = chan.LastFrame;
        Type = chan.Flags;
        PivotIdx = chan.Pivot;
        DefaultVal = chan.DefaultVal;

        uint32_t numbits = LastFrame - FirstFrame + 1;
        uint32_t numbytes = (numbits + 7) / 8;
        uint32_t bytesleft = numbytes - 1;

        Bits = new uint8_t[numbytes];

        Bits[0] = chan.Data[0];

        if (bytesleft > 0) {
                if (cload.Read(&(Bits[1]), bytesleft) != bytesleft) {
                        Free();
                        return false;
                }
        }

        return true;
}

// --- the tree table (the manager's Get_HTree) ----------------------------------

struct HTreeStub
{
        std::vector<uint8_t> lower_name;
        int pivots;
};

static std::vector<HTreeStub> g_trees;

// WW3DAssetManager::Get_HTree -> HTreeManagerClass::Get_Tree: lowercase
// the name, exact hash lookup over the registered trees.
static HTreeStub *Get_HTree_Stub(const char *name)
{
        std::vector<uint8_t> lower;
        for (const char *p = name; *p != 0; ++p) {
                lower.push_back(ascii_tolower((unsigned char)*p));
        }
        for (HTreeStub &t : g_trees) {
                if (t.lower_name == lower) {
                        return &t;
                }
        }
        return nullptr;
}

// --- the animation (hrawanim.cpp) ----------------------------------------------

struct NodeMotionStruct
{
        NodeMotionStruct() : X(nullptr), Y(nullptr), Z(nullptr), XR(nullptr), YR(nullptr), ZR(nullptr),
                             Q(nullptr), Vis(nullptr) {}
        ~NodeMotionStruct()
        {
                delete X;
                delete Y;
                delete Z;
                delete XR;
                delete YR;
                delete ZR;
                delete Q;
                delete Vis;
        }

        MotionChannelClass *X;
        MotionChannelClass *Y;
        MotionChannelClass *Z;
        MotionChannelClass *XR;
        MotionChannelClass *YR;
        MotionChannelClass *ZR;
        MotionChannelClass *Q;

        BitChannelClass *Vis;
};

class HRawAnimClass
{
public:
        enum
        {
                OK,
                LOAD_ERROR
        };

        HRawAnimClass() : NumFrames(0), NumNodes(0), FrameRate(0), NodeMotion(nullptr)
        {
                memset(Name, 0, sizeof(Name));
                memset(HierarchyName, 0, sizeof(HierarchyName));
        }
        ~HRawAnimClass() { Free(); }

        int Load_W3D(ChunkLoadClass &cload);

        // the query surface (hanim.h / hrawanim.cpp)
        const char *Get_Name() const { return Name; }
        const char *Get_HName() const { return HierarchyName; }
        int Get_Num_Frames() const { return NumFrames; }
        float Get_Frame_Rate() const { return FrameRate; }
        float Get_Total_Time() const { return (float)NumFrames / FrameRate; }
        int Get_Num_Pivots() const { return NumNodes; }

        void Get_Translation(float *trans, int pividx, float frame) const;
        void Get_Orientation(float *q, int pividx, float frame) const;
        void Get_Transform(float *mtx, int pividx, float frame) const;
        bool Get_Visibility(int pividx, float frame) const;

        bool Is_Node_Motion_Present(int pividx) const;
        bool Has_X_Translation(int pividx) const { return NodeMotion[pividx].X != nullptr; }
        bool Has_Y_Translation(int pividx) const { return NodeMotion[pividx].Y != nullptr; }
        bool Has_Z_Translation(int pividx) const { return NodeMotion[pividx].Z != nullptr; }
        bool Has_Rotation(int pividx) const { return NodeMotion[pividx].Q != nullptr; }
        bool Has_Visibility(int pividx) const { return NodeMotion[pividx].Vis != nullptr; }

        char Name[2 * W3D_NAME_LEN];
        char HierarchyName[W3D_NAME_LEN];
        int NumFrames;
        int NumNodes;
        float FrameRate;
        NodeMotionStruct *NodeMotion;
        W3dAnimHeaderStruct Header; // dump-side stash (documented deviation)

private:
        void Free()
        {
                delete[] NodeMotion;
                NodeMotion = nullptr;
        }
        bool read_channel(ChunkLoadClass &cload, MotionChannelClass **newchan, bool pre30);
        void add_channel(MotionChannelClass *newchan);
        bool read_bit_channel(ChunkLoadClass &cload, BitChannelClass **newchan, bool pre30);
        void add_bit_channel(BitChannelClass *newchan);
};

int HRawAnimClass::Load_W3D(ChunkLoadClass &cload)
{
        bool pre30 = false;

        /*
        ** First make sure we release any memory in use
        */
        Free();

        /*
        **      Open the first chunk, it should be the animation header
        */
        if (!cload.Open_Chunk())
                return LOAD_ERROR;

        if (cload.Cur_Chunk_ID() != W3D_CHUNK_ANIMATION_HEADER) {
                // ERROR: Expected Animation Header!
                return LOAD_ERROR;
        }

        W3dAnimHeaderStruct aheader;
        if (cload.Read(&aheader, sizeof(W3dAnimHeaderStruct)) != sizeof(W3dAnimHeaderStruct)) {
                return LOAD_ERROR;
        }
        Header = aheader; // dump-side stash: the pristine decode

        cload.Close_Chunk();

        /*
        ** Check if the animation version is pre-3.0.  If so, we need to add 1 to all of the
        ** bone indexes.  In version 3.0 onward, all htree's use bone 0 as the root node.
        */
        if (aheader.Version < W3D_MAKE_VERSION(3, 0)) {
                pre30 = true;
        }

        strcpy(Name, aheader.HierarchyName);
        sstrlcat(Name, ".", sizeof(Name));
        sstrlcat(Name, aheader.Name, sizeof(Name));

        strcpy(HierarchyName, aheader.HierarchyName);

        HTreeStub *base_pose = Get_HTree_Stub(HierarchyName);
        if (base_pose == nullptr) {
                return LOAD_ERROR;
        }
        NumNodes = base_pose->pivots;

        NumFrames = aheader.NumFrames;
        FrameRate = aheader.FrameRate; // uint32 -> float, numerically

        NodeMotion = new NodeMotionStruct[NumNodes];

        /*
        ** Now, read in all of the other chunks (motion channels).
        */
        while (cload.Open_Chunk()) {

                switch (cload.Cur_Chunk_ID()) {

                        case W3D_CHUNK_ANIMATION_CHANNEL:
                        {
                                MotionChannelClass *newchan = nullptr;
                                if (!read_channel(cload, &newchan, pre30)) {
                                        return LOAD_ERROR;
                                }

                                // (gth) if the channel is referring to a node which is outside the range,
                                // just throw away the channel.  This probably means the animation must
                                // be re-exported
                                if (newchan->Get_Pivot() < (uint32_t)NumNodes) {
                                        add_channel(newchan);
                                } else {
                                        delete newchan;
                                }
                                break;
                        }

                        case W3D_CHUNK_BIT_CHANNEL:
                        {
                                BitChannelClass *newbitchan = nullptr;
                                if (!read_bit_channel(cload, &newbitchan, pre30)) {
                                        return LOAD_ERROR;
                                }

                                if (newbitchan->Get_Pivot() < (uint32_t)NumNodes) {
                                        add_bit_channel(newbitchan);
                                } else {
                                        delete newbitchan;
                                }
                                break;
                        }

                        default:
                                break;
                }
                cload.Close_Chunk();
        }

        return OK;
}

bool HRawAnimClass::read_channel(ChunkLoadClass &cload, MotionChannelClass **newchan, bool pre30)
{
        MotionChannelClass *channel = new MotionChannelClass;
        if (channel->Load_W3D(cload)) {
                if (pre30) {
                        channel->Set_Pivot(channel->Get_Pivot() + 1);
                }

                *newchan = channel;
                return true;
        } else {
                delete channel;
                return false;
        }
}

void HRawAnimClass::add_channel(MotionChannelClass *newchan)
{
        int idx = newchan->Get_Pivot();

        switch (newchan->Get_Type()) {
                case ANIM_CHANNEL_X:
                        NodeMotion[idx].X = newchan;
                        break;
                case ANIM_CHANNEL_Y:
                        NodeMotion[idx].Y = newchan;
                        break;
                case ANIM_CHANNEL_Z:
                        NodeMotion[idx].Z = newchan;
                        break;
                case ANIM_CHANNEL_XR:
                        NodeMotion[idx].XR = newchan;
                        break;
                case ANIM_CHANNEL_YR:
                        NodeMotion[idx].YR = newchan;
                        break;
                case ANIM_CHANNEL_ZR:
                        NodeMotion[idx].ZR = newchan;
                        break;
                case ANIM_CHANNEL_Q:
                        NodeMotion[idx].Q = newchan;
                        break;
        }
}

bool HRawAnimClass::read_bit_channel(ChunkLoadClass &cload, BitChannelClass **newchan, bool pre30)
{
        BitChannelClass *channel = new BitChannelClass;
        if (channel->Load_W3D(cload)) {
                if (pre30) {
                        channel->PivotIdx += 1;
                }

                *newchan = channel;
                return true;
        } else {
                delete channel;
                return false;
        }
}

void HRawAnimClass::add_bit_channel(BitChannelClass *newchan)
{
        int idx = newchan->Get_Pivot();

        switch (newchan->Get_Type()) {
                case BIT_CHANNEL_VIS:
                        NodeMotion[idx].Vis = newchan;
                        break;
        }
}

void HRawAnimClass::Get_Translation(float *trans, int pividx, float frame) const
{
        struct NodeMotionStruct *motion = &NodeMotion[pividx];

        if ((motion->X == nullptr) && (motion->Y == nullptr) && (motion->Z == nullptr)) {
                trans[0] = trans[1] = trans[2] = 0.0f;
                return;
        }

        int frame0 = (int)WWFloat_To_Long(frame - 0.499999f);

        int frame1 = frame0 + 1;

        float ratio = frame - (float)frame0;

        if (frame1 >= NumFrames) {
                frame1 = 0;
        }

        // float[8]: the original's Vector3 is 12 bytes and the per-slot
        // Get_Vector calls (&trans0[1], &trans0[2]) can write up to 4
        // floats each (a Q-typed or wide translation channel —
        // corpus-disciplined away, the values read back are [0..3)); the
        // extra slots are scratch so the transcription keeps the path
        float trans0[8] = {0.0f, 0.0f, 0.0f, 0.0f, 0.0f, 0.0f, 0.0f, 0.0f};

        if (motion->X != nullptr) {
                motion->X->Get_Vector((int)frame0, &(trans0[0]));
        }
        if (motion->Y != nullptr) {
                motion->Y->Get_Vector((int)frame0, &(trans0[1]));
        }
        if (motion->Z != nullptr) {
                motion->Z->Get_Vector((int)frame0, &(trans0[2]));
        }

        if (ratio == 0.0f) {
                trans[0] = trans0[0];
                trans[1] = trans0[1];
                trans[2] = trans0[2];
                return;
        }

        float trans1[8] = {0.0f, 0.0f, 0.0f, 0.0f, 0.0f, 0.0f, 0.0f, 0.0f};

        if (motion->X != nullptr) {
                motion->X->Get_Vector((int)frame1, &(trans1[0]));
        }
        if (motion->Y != nullptr) {
                motion->Y->Get_Vector((int)frame1, &(trans1[1]));
        }
        if (motion->Z != nullptr) {
                motion->Z->Get_Vector((int)frame1, &(trans1[2]));
        }

        trans[0] = V3Lerp(trans0[0], trans1[0], ratio);
        trans[1] = V3Lerp(trans0[1], trans1[1], ratio);
        trans[2] = V3Lerp(trans0[2], trans1[2], ratio);
}

void HRawAnimClass::Get_Orientation(float *q, int pividx, float frame) const
{
        int frame0 = (int)WWFloat_To_Long(frame - 0.499999f);
        int frame1 = frame0 + 1;

        float ratio = frame - (float)frame0;

        if (frame1 >= NumFrames) {
                frame1 = 0;
        }

        // the SPECIAL_GETVEC_AS_QUAT branch (motchan.h #defines it before
        // including hrawanim's translation unit)
        float q0[4], q1[4];

        MotionChannelClass *mc = NodeMotion[pividx].Q;
        if (mc != nullptr) {
                mc->Get_Vector_As_Quat((int)frame0, q0);
                mc->Get_Vector_As_Quat((int)frame1, q1);
        } else {
                q0[0] = q0[1] = q0[2] = 0.0f;
                q0[3] = 1.0f;
                q1[0] = q1[1] = q1[2] = 0.0f;
                q1[3] = 1.0f;
        }

        if (ratio == 0.0f) {
                memcpy(q, q0, sizeof(float) * 4);
        } else if (ratio == 1.0f) {
                memcpy(q, q1, sizeof(float) * 4);
        } else {
                Fast_Slerp(q, q0, q1, ratio);
        }
}

void HRawAnimClass::Get_Transform(float *mtx, int pividx, float frame) const
{
        struct NodeMotionStruct *motion = &NodeMotion[pividx];

        int frame0 = (int)WWFloat_To_Long(frame - 0.499999f);

        int frame1 = frame0 + 1;

        float ratio = frame - (float)frame0;

        if (frame1 >= NumFrames) {
                frame1 = 0;
        }

        // the identity init: the original leaves vals stale and a
        // VectorLen-below-4 Q channel would read it (corpus-disciplined
        // away — real quaternion channels carry 4)
        float vals[4] = {0.0f, 0.0f, 0.0f, 1.0f};

        float q0[4] = {0.0f, 0.0f, 0.0f, 1.0f};
        if (NodeMotion[pividx].Q != nullptr) {
                NodeMotion[pividx].Q->Get_Vector((int)frame0, vals);
                q0[0] = vals[0];
                q0[1] = vals[1];
                q0[2] = vals[2];
                q0[3] = vals[3];
        }

        if (ratio == 0.0f) {
                Build_Matrix3D(q0, mtx);
                if (motion->X != nullptr)
                        motion->X->Get_Vector((int)frame0, &(mtx[3]));
                if (motion->Y != nullptr)
                        motion->Y->Get_Vector((int)frame0, &(mtx[7]));
                if (motion->Z != nullptr)
                        motion->Z->Get_Vector((int)frame0, &(mtx[11]));
                return;
        }

        float q1[4] = {0.0f, 0.0f, 0.0f, 1.0f};
        if (NodeMotion[pividx].Q != nullptr) {
                NodeMotion[pividx].Q->Get_Vector((int)frame1, vals);
                q1[0] = vals[0];
                q1[1] = vals[1];
                q1[2] = vals[2];
                q1[3] = vals[3];
        }

        float q[4];
        Fast_Slerp(q, q0, q1, ratio);
        Build_Matrix3D(q, mtx);

        float trans0[8] = {0.0f, 0.0f, 0.0f, 0.0f, 0.0f, 0.0f, 0.0f, 0.0f}; // float[8]: see Get_Translation
        if (motion->X != nullptr)
                motion->X->Get_Vector((int)frame0, &(trans0[0]));
        if (motion->Y != nullptr)
                motion->Y->Get_Vector((int)frame0, &(trans0[1]));
        if (motion->Z != nullptr)
                motion->Z->Get_Vector((int)frame0, &(trans0[2]));

        float trans1[8] = {0.0f, 0.0f, 0.0f, 0.0f, 0.0f, 0.0f, 0.0f, 0.0f};
        if (motion->X != nullptr)
                motion->X->Get_Vector((int)frame1, &(trans1[0]));
        if (motion->Y != nullptr)
                motion->Y->Get_Vector((int)frame1, &(trans1[1]));
        if (motion->Z != nullptr)
                motion->Z->Get_Vector((int)frame1, &(trans1[2]));

        // Vector3::Lerp + Matrix3D::Set_Translation
        mtx[3] = V3Lerp(trans0[0], trans1[0], ratio);
        mtx[7] = V3Lerp(trans0[1], trans1[1], ratio);
        mtx[11] = V3Lerp(trans0[2], trans1[2], ratio);
}

bool HRawAnimClass::Get_Visibility(int pividx, float frame) const
{
        if (NodeMotion[pividx].Vis != nullptr) {
                return (NodeMotion[pividx].Vis->Get_Bit((int)frame) == 1);
        }

        // default to always visible...
        return 1;
}

bool HRawAnimClass::Is_Node_Motion_Present(int pividx) const
{
        if (NodeMotion[pividx].X != nullptr)
                return true;
        if (NodeMotion[pividx].Y != nullptr)
                return true;
        if (NodeMotion[pividx].Z != nullptr)
                return true;
        if (NodeMotion[pividx].XR != nullptr)
                return true;
        if (NodeMotion[pividx].YR != nullptr)
                return true;
        if (NodeMotion[pividx].ZR != nullptr)
                return true;
        if (NodeMotion[pividx].Q != nullptr)
                return true;
        if (NodeMotion[pividx].Vis != nullptr)
                return true;

        return false;
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
};

// the 44-byte header
static void w_anim_header(ChunkBuild &b, uint32_t version, const char *name, const char *hname,
                          uint32_t num_frames, uint32_t frame_rate)
{
        b.begin(W3D_CHUNK_ANIMATION_HEADER);
        b.w_u32(version);
        // strncpy(n, name, 16) semantics without the warning: NUL-padded,
        // and NOT terminated when the source fills all 16 bytes (a24/a25
        // depend on that)
        char n[W3D_NAME_LEN];
        memset(n, 0, sizeof(n));
        size_t nl = strlen(name);
        if (nl > W3D_NAME_LEN)
                nl = W3D_NAME_LEN;
        memcpy(n, name, nl);
        b.w_bytes(n, W3D_NAME_LEN);
        char h[W3D_NAME_LEN];
        memset(h, 0, sizeof(h));
        size_t hl = strlen(hname);
        if (hl > W3D_NAME_LEN)
                hl = W3D_NAME_LEN;
        memcpy(h, hname, hl);
        b.w_bytes(h, W3D_NAME_LEN);
        b.w_u32(num_frames);
        b.w_u32(frame_rate);
        b.end();
}

// one motion channel: the 12 field bytes + every data word (the first
// doubles as the struct's Data[0]) + optional exporter junk
static void put_u16(uint8_t *p, uint16_t v)
{
        p[0] = (uint8_t)(v & 0xFF);
        p[1] = (uint8_t)((v >> 8) & 0xFF);
}

static void w_motion_channel(ChunkBuild &b, int first, int last, int vector_len, int flags, int pivot,
                             const std::vector<float> &data, const std::vector<uint8_t> &junk)
{
        b.begin(W3D_CHUNK_ANIMATION_CHANNEL);
        uint8_t f[12];
        put_u16(f + 0, (uint16_t)first);
        put_u16(f + 2, (uint16_t)last);
        put_u16(f + 4, (uint16_t)vector_len);
        put_u16(f + 6, (uint16_t)flags);
        put_u16(f + 8, (uint16_t)pivot);
        put_u16(f + 10, 0);
        b.w_bytes(f, 12);
        for (float v : data) {
                b.w_f32(v);
        }
        b.w_bytes(junk.data(), junk.size());
        b.end();
}

// one bit channel: the 8 field bytes + default + the packed bits
static void w_bit_channel(ChunkBuild &b, int first, int last, int flags, int pivot, int default_val,
                          const std::vector<uint8_t> &bits)
{
        b.begin(W3D_CHUNK_BIT_CHANNEL);
        uint8_t f[8];
        put_u16(f + 0, (uint16_t)first);
        put_u16(f + 2, (uint16_t)last);
        put_u16(f + 4, (uint16_t)flags);
        put_u16(f + 6, (uint16_t)pivot);
        b.w_bytes(f, 8);
        uint8_t d[1] = {(uint8_t)default_val};
        b.w_bytes(d, 1);
        b.w_bytes(bits.data(), bits.size());
        b.end();
}

struct Corpus
{
        const char *id;
        std::vector<uint8_t> bytes;
};

static void push(std::vector<Corpus> &out, const char *id, const ChunkBuild &b)
{
        Corpus c;
        c.id = id;
        c.bytes = b.bytes;
        out.push_back(c);
}

static std::vector<Corpus> build_corpora()
{
        std::vector<Corpus> out;

        const uint32_t V40 = W3D_MAKE_VERSION(4, 0);
        const uint32_t V30 = W3D_MAKE_VERSION(3, 0);
        const uint32_t V21 = W3D_MAKE_VERSION(2, 1);
        const float s45 = 0.70710678118654752440f;

        // a01: the modern full animation — every channel kind routed.
        {
                ChunkBuild b;
                w_anim_header(b, V40, "Run", "tank", 8, 30);
                w_motion_channel(b, 0, 3, 1, ANIM_CHANNEL_X, 1, {0.5f, 1.0f, 1.5f, 2.0f}, {});
                w_motion_channel(b, 0, 3, 1, ANIM_CHANNEL_Y, 1, {1.0f, 1.0f, 1.0f, 1.0f}, {});
                w_motion_channel(b, 0, 3, 1, ANIM_CHANNEL_Z, 1, {-0.5f, -1.0f, -1.5f, -2.0f}, {});
                w_motion_channel(b, 0, 3, 4, ANIM_CHANNEL_Q, 2,
                                 {0.5f, 0.5f, 0.5f, 0.5f, 0.5f, 0.5f, -0.5f, -0.5f, s45, 0.0f, 0.0f, s45,
                                  0.0f, 0.0f, 0.0f, 1.0f},
                                 {});
                w_bit_channel(b, 0, 15, BIT_CHANNEL_VIS, 3, 1, {0b10101010, 0b01010101});
                w_motion_channel(b, 0, 1, 1, ANIM_CHANNEL_XR, 4, {0.25f, 0.75f}, {});
                push(out, "a01", b);
        }

        // a02: the slerp corpus — quaternion keys a quadrant apart, so the
        // fractional probes exercise the table-based Fast_Slerp path.
        {
                ChunkBuild b;
                w_anim_header(b, V40, "Slerp", "t", 4, 30);
                w_motion_channel(b, 0, 3, 4, ANIM_CHANNEL_Q, 0,
                                 {s45, 0.0f, 0.0f, s45, 0.0f, s45, 0.0f, s45, 0.0f, 0.0f, s45, s45,
                                  -s45, 0.0f, 0.0f, s45},
                                 {});
                push(out, "a02", b);
        }

        // a03: near-parallel keys — the identical-quaternion case takes the
        // linear branch (1 - cos_t < EPSILON^2).
        {
                ChunkBuild b;
                w_anim_header(b, V40, "Lin", "t", 4, 30);
                w_motion_channel(b, 0, 1, 4, ANIM_CHANNEL_Q, 0,
                                 {0.5f, 0.5f, 0.5f, 0.5f, 0.5f, 0.5f, 0.5f, 0.5f}, {});
                push(out, "a03", b);
        }

        // a04: the hemisphere flip — antipodal keys (cos_t < 0).
        {
                ChunkBuild b;
                w_anim_header(b, V40, "Flip", "t", 4, 30);
                w_motion_channel(b, 0, 1, 4, ANIM_CHANNEL_Q, 0,
                                 {0.5f, 0.5f, 0.5f, 0.5f, -0.5f, -0.5f, -0.5f, -0.5f}, {});
                push(out, "a04", b);
        }

        // a05: the Fast_Acos fallback — keys a few degrees apart (the dot
        // product lands in (0.975, 1), where the exact gm_acosf runs).
        {
                ChunkBuild b;
                w_anim_header(b, V40, "Near", "t", 4, 30);
                w_motion_channel(b, 0, 1, 4, ANIM_CHANNEL_Q, 0,
                                 {0.0f, 0.0f, 0.0f, 1.0f, 0.03125f, 0.0f, 0.0f, 0.9995f}, {});
                push(out, "a05", b);
        }

        // a06: version exactly 3.0 — the pre-3.0 boundary (not pre-3.0).
        {
                ChunkBuild b;
                w_anim_header(b, V30, "V30", "t", 4, 30);
                w_motion_channel(b, 0, 1, 1, ANIM_CHANNEL_X, 0, {1.0f, 2.0f}, {});
                push(out, "a06", b);
        }

        // a07: pre-3.0 — every channel pivot shifts up by one.
        {
                ChunkBuild b;
                w_anim_header(b, V21, "Old", "tank", 4, 30);
                w_motion_channel(b, 0, 1, 1, ANIM_CHANNEL_X, 0, {1.0f, 2.0f}, {});
                w_bit_channel(b, 0, 7, BIT_CHANNEL_VIS, 1, 0, {0b11110000});
                push(out, "a07", b);
        }

        // a08: pre-3.0 + pivot 0xFFFF — the increment wraps to 0x10000,
        // outside the tree; the channel is discarded.
        {
                ChunkBuild b;
                w_anim_header(b, V21, "Wrap", "tank", 4, 30);
                w_motion_channel(b, 0, 1, 1, ANIM_CHANNEL_X, 0xFFFF, {1.0f, 2.0f}, {});
                w_motion_channel(b, 0, 1, 1, ANIM_CHANNEL_Y, 0, {3.0f, 4.0f}, {});
                push(out, "a08", b);
        }

        // a09: a post-3.0 channel for a pivot outside the tree — discarded,
        // the load continues.
        {
                ChunkBuild b;
                w_anim_header(b, V40, "Beyond", "rig", 4, 30);
                w_motion_channel(b, 0, 1, 1, ANIM_CHANNEL_X, 5, {1.0f, 2.0f}, {});
                w_bit_channel(b, 0, 7, BIT_CHANNEL_VIS, 9, 1, {0xFF});
                push(out, "a09", b);
        }

        // a10: unknown channel types and an unknown top-level chunk.
        {
                ChunkBuild b;
                w_anim_header(b, V40, "Odd", "t", 4, 30);
                w_motion_channel(b, 0, 1, 1, 99, 0, {1.0f, 2.0f}, {});
                w_bit_channel(b, 0, 7, 7, 0, 0, {0xFF});
                b.begin(0x0999);
                b.w_u32(0xDEADBEEF);
                b.end();
                push(out, "a10", b);
        }

        // a11: a duplicate X channel — the second overwrites the first.
        {
                ChunkBuild b;
                w_anim_header(b, V40, "Twice", "t", 4, 30);
                w_motion_channel(b, 0, 1, 1, ANIM_CHANNEL_X, 0, {1.0f, 2.0f}, {});
                w_motion_channel(b, 0, 1, 1, ANIM_CHANNEL_X, 0, {9.0f, 8.0f}, {});
                push(out, "a11", b);
        }

        // a12: the exporter junk — a channel chunk carrying more bytes than
        // the frames need, with a second channel after it (only loadable
        // when the seek lands exactly at the chunk end).
        {
                ChunkBuild b;
                w_anim_header(b, V40, "Junk", "t", 4, 30);
                std::vector<uint8_t> junk(8, 0xAA);
                w_motion_channel(b, 0, 1, 1, ANIM_CHANNEL_X, 0, {1.0f, 2.0f}, junk);
                w_motion_channel(b, 0, 1, 1, ANIM_CHANNEL_Y, 0, {3.0f, 4.0f}, {});
                push(out, "a12", b);
        }

        // a13: a channel declaring 4 frames but carrying one — the Read
        // hits the chunk clamp and the load fails.
        {
                ChunkBuild b;
                w_anim_header(b, V40, "Short", "t", 8, 30);
                b.begin(W3D_CHUNK_ANIMATION_CHANNEL);
                uint8_t f[12];
                put_u16(f + 0, 0);
                put_u16(f + 2, 3);
                put_u16(f + 4, 1);
                put_u16(f + 6, ANIM_CHANNEL_X);
                put_u16(f + 8, 0);
                put_u16(f + 10, 0);
                b.w_bytes(f, 12);
                b.w_f32(1.0f);
                b.end(); // 16 bytes: the struct with Data[0], nothing more
                push(out, "a13", b);
        }

        // a14: an empty payload — Open_Chunk fails, LOAD_ERROR.
        {
                ChunkBuild b;
                push(out, "a14", b);
        }

        // a15: the first chunk is not the animation header.
        {
                ChunkBuild b;
                b.begin(W3D_CHUNK_ANIMATION_CHANNEL);
                b.w_u32(0);
                b.end();
                push(out, "a15", b);
        }

        // a16: a header chunk declaring 44 but the file ends inside it.
        {
                ChunkBuild b;
                b.begin(W3D_CHUNK_ANIMATION_HEADER);
                b.w_u32(V40);
                char n[16];
                memset(n, 0, 16);
                memcpy(n, "Trunc", 5);
                b.w_bytes(n, 16);
                // no end(): patch the declared size to the full 44
                ChunkBuild::put_u32(b.bytes.data() + 4, 44);
                push(out, "a16", b);
        }

        // a17: the hierarchy is not in the tree table — Get_HTree fails.
        {
                ChunkBuild b;
                w_anim_header(b, V40, "Run", "ghost", 4, 30);
                push(out, "a17", b);
        }

        // a18: a mixed-case hierarchy name — the manager's lowercase
        // lookup resolves it.
        {
                ChunkBuild b;
                w_anim_header(b, V40, "Run", "Tank", 4, 30);
                push(out, "a18", b);
        }

        // a19: FrameRate stored as the raw bits of 50.0f — the numeric
        // conversion makes 1112014848.0.
        {
                ChunkBuild b;
                w_anim_header(b, V40, "Rate", "t", 8, 0x42480000u);
                push(out, "a19", b);
        }

        // a20: FrameRate 0 — Get_Total_Time is +inf.
        {
                ChunkBuild b;
                w_anim_header(b, V40, "Free", "t", 8, 0);
                push(out, "a20", b);
        }

        // a21: FrameRate 0x01000001 — the f32 conversion rounds.
        {
                ChunkBuild b;
                w_anim_header(b, V40, "Round", "t", 1, 0x01000001u);
                push(out, "a21", b);
        }

        // a22: NumFrames 0.
        {
                ChunkBuild b;
                w_anim_header(b, V40, "Zero", "t", 0, 30);
                w_motion_channel(b, 0, 1, 1, ANIM_CHANNEL_X, 0, {5.0f, 7.0f}, {});
                push(out, "a22", b);
        }

        // a23: NumFrames 0x80000000 — negative as int: frame1 always wraps
        // to 0 and the total time is negative.
        {
                ChunkBuild b;
                w_anim_header(b, V40, "Neg", "t", 0x80000000u, 30);
                w_motion_channel(b, 0, 1, 1, ANIM_CHANNEL_X, 0, {5.0f, 7.0f}, {});
                push(out, "a23", b);
        }

        // a24: an unterminated hierarchy field whose strcpy runs into the
        // NumFrames word — the 17-byte key resolves (the table holds it)
        // and the composed name shows the swallowed byte.
        {
                ChunkBuild b;
                b.begin(W3D_CHUNK_ANIMATION_HEADER);
                b.w_u32(V40);
                char n[16];
                memset(n, 0, 16);
                memcpy(n, "A", 1);
                b.w_bytes(n, 16);
                char h[16];
                memset(h, 'H', 16); // no NUL: strcpy keeps reading
                b.w_bytes(h, 16);
                b.w_u32(5);  // 05 00 00 00: the NUL lands at byte 17
                b.w_u32(30);
                b.end();
                push(out, "a24", b);
        }

        // a25: the same unterminated field with a tree registered under the
        // 16 field bytes — the real key is 17 bytes, the lookup misses and
        // the load fails (pins the strcpy extension).
        {
                ChunkBuild b;
                b.begin(W3D_CHUNK_ANIMATION_HEADER);
                b.w_u32(V40);
                char n[16];
                memset(n, 0, 16);
                memcpy(n, "A", 1);
                b.w_bytes(n, 16);
                char h[16];
                memset(h, 'z', 16);
                b.w_bytes(h, 16);
                b.w_u32(5);
                b.w_u32(30);
                b.end();
                push(out, "a25", b);
        }

        // a26: an unterminated animation name — the strlcat source runs
        // through the hierarchy field's 't'.
        {
                ChunkBuild b;
                b.begin(W3D_CHUNK_ANIMATION_HEADER);
                b.w_u32(V40);
                char n[16];
                memset(n, 'A', 16);
                memcpy(n, "ANIM0123456789AB", 16); // exactly 16, no NUL
                b.w_bytes(n, 16);
                char h[16];
                memset(h, 0, 16);
                h[0] = 't'; // the source's NUL is the hierarchy's second byte
                b.w_bytes(h, 16);
                b.w_u32(4);
                b.w_u32(30);
                b.end();
                push(out, "a26", b);
        }

        // a27: the 32-byte name buffer truncates at 31 characters.
        {
                ChunkBuild b;
                w_anim_header(b, V40, "0123456789ABCDEF", "tanktree1234567", 4, 30);
                push(out, "a27", b);
        }

        // a28: an empty animation name — the trailing dot survives.
        {
                ChunkBuild b;
                w_anim_header(b, V40, "", "t", 4, 30);
                push(out, "a28", b);
        }

        // a29: a motion channel with an empty span (LastFrame < FirstFrame)
        // — the stated-contract failure.
        {
                ChunkBuild b;
                w_anim_header(b, V40, "Empty", "t", 8, 30);
                w_motion_channel(b, 4, 3, 1, ANIM_CHANNEL_X, 0, {1.0f}, {});
                push(out, "a29", b);
        }

        // a30: a motion channel with VectorLen 0.
        {
                ChunkBuild b;
                w_anim_header(b, V40, "NoVec", "t", 8, 30);
                w_motion_channel(b, 0, 1, 0, ANIM_CHANNEL_X, 0, {1.0f}, {});
                push(out, "a30", b);
        }

        // a31: a bit channel with an empty span.
        {
                ChunkBuild b;
                w_anim_header(b, V40, "NoBits", "t", 8, 30);
                w_bit_channel(b, 4, 3, BIT_CHANNEL_VIS, 0, 1, {0});
                push(out, "a31", b);
        }

        // a32: a Q-only animation — transforms are rotation-only.
        {
                ChunkBuild b;
                w_anim_header(b, V40, "RotOnly", "t", 4, 30);
                w_motion_channel(b, 0, 3, 4, ANIM_CHANNEL_Q, 0,
                                 {0.0f, 0.0f, 0.0f, 1.0f, 0.0f, s45, 0.0f, s45, 0.0f, s45, 0.0f, s45,
                                  0.0f, 0.0f, 0.0f, 1.0f},
                                 {});
                push(out, "a32", b);
        }

        // a33: a translation-only animation — identity rotation.
        {
                ChunkBuild b;
                w_anim_header(b, V40, "TransOnly", "t", 3, 30);
                w_motion_channel(b, 0, 2, 1, ANIM_CHANNEL_X, 0, {1.0f, 2.0f, 3.0f}, {});
                w_motion_channel(b, 0, 2, 1, ANIM_CHANNEL_Y, 0, {0.0f, 4.0f, 8.0f}, {});
                w_motion_channel(b, 0, 2, 1, ANIM_CHANNEL_Z, 0, {-1.0f, -1.0f, -1.0f}, {});
                push(out, "a33", b);
        }

        // a34: channels with a partial frame range — below the range the
        // identity fills in and the lerp blends it into the first key.
        {
                ChunkBuild b;
                w_anim_header(b, V40, "Partial", "t", 8, 30);
                w_motion_channel(b, 2, 5, 1, ANIM_CHANNEL_X, 0, {1.0f, 2.0f, 3.0f, 4.0f}, {});
                w_bit_channel(b, 3, 6, BIT_CHANNEL_VIS, 0, 0, {0b00000100});
                push(out, "a34", b);
        }

        // a35: single-frame channels — the no-follow-up-read paths.
        {
                ChunkBuild b;
                w_anim_header(b, V40, "Single", "t", 4, 30);
                w_motion_channel(b, 0, 0, 1, ANIM_CHANNEL_X, 0, {2.5f}, {});
                w_motion_channel(b, 1, 1, 4, ANIM_CHANNEL_Q, 0, {0.0f, s45, 0.0f, s45}, {});
                w_bit_channel(b, 2, 2, BIT_CHANNEL_VIS, 0, 1, {0b00000001});
                push(out, "a35", b);
        }

        // a36: NumFrames 1 — every frame1 wraps to 0.
        {
                ChunkBuild b;
                w_anim_header(b, V40, "One", "t", 1, 30);
                w_motion_channel(b, 0, 0, 1, ANIM_CHANNEL_X, 0, {2.5f}, {});
                push(out, "a36", b);
        }

        // a37: a 24-bone skeleton with channels on high pivots.
        {
                ChunkBuild b;
                w_anim_header(b, V40, "March", "soldier", 8, 30);
                w_motion_channel(b, 0, 3, 1, ANIM_CHANNEL_X, 0, {0.0f, 1.0f, 2.0f, 3.0f}, {});
                w_motion_channel(b, 0, 3, 1, ANIM_CHANNEL_X, 20, {4.0f, 5.0f, 6.0f, 7.0f}, {});
                w_motion_channel(b, 0, 3, 4, ANIM_CHANNEL_Q, 23,
                                 {s45, 0.0f, 0.0f, s45, 0.0f, 0.0f, 0.0f, 1.0f, 0.0f, s45, 0.0f, s45,
                                  -s45, 0.0f, 0.0f, s45},
                                 {});
                w_bit_channel(b, 0, 7, BIT_CHANNEL_VIS, 1, 0, {0b10110001});
                push(out, "a37", b);
        }

        // a38: version 0 — pre-3.0 by the comparison alone.
        {
                ChunkBuild b;
                w_anim_header(b, 0, "V0", "t", 4, 30);
                w_motion_channel(b, 0, 1, 1, ANIM_CHANNEL_X, 0, {1.0f, 2.0f}, {});
                push(out, "a38", b);
        }

        return out;
}

// --- the dump --------------------------------------------------------------------

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
static std::string q_bytes(const uint8_t *name, size_t len)
{
        if (len == 0)
                return "-";
        for (size_t i = 0; i < len; i++) {
                if (name[i] < 0x21 || name[i] > 0x7e) {
                        std::string s = "<";
                        hex_append(s, name, len);
                        s += ">";
                        return s;
                }
        }
        return std::string((const char *)name, len);
}

static size_t cstr_len(const char *field, size_t cap)
{
        size_t len = 0;
        while (len < cap && field[len] != 0)
                len++;
        return len;
}

// The frame probe list: a fixed set plus the count-derived frames when the
// count is sane (both sides compute the identical list).
static std::vector<float> probe_frames(int num_frames)
{
        std::vector<float> f = {-1.0f, -0.5f, 0.0f, 0.5f, 1.5f, 2.25f, 3.5f};
        if (num_frames >= 1 && num_frames <= 1000) {
                f.push_back((float)(num_frames - 1));
                f.push_back((float)num_frames);
        }
        return f;
}

// The pivots the queries run for: 0..min(8, n) plus the last.
static std::vector<int> probe_pivots(int num_nodes)
{
        std::vector<int> p;
        for (int i = 0; i < num_nodes && i < 8; i++) {
                p.push_back(i);
        }
        if (num_nodes > 8) {
                p.push_back(num_nodes - 1);
        }
        return p;
}

static const char *slot_name(const NodeMotionStruct &m, int slot)
{
        switch (slot) {
        case 0:
                return m.X ? "x" : nullptr;
        case 1:
                return m.Y ? "y" : nullptr;
        case 2:
                return m.Z ? "z" : nullptr;
        case 3:
                return m.XR ? "xr" : nullptr;
        case 4:
                return m.YR ? "yr" : nullptr;
        case 5:
                return m.ZR ? "zr" : nullptr;
        default:
                return m.Q ? "q" : nullptr;
        }
}

static const MotionChannelClass *slot_chan(const NodeMotionStruct &m, int slot)
{
        switch (slot) {
        case 0:
                return m.X;
        case 1:
                return m.Y;
        case 2:
                return m.Z;
        case 3:
                return m.XR;
        case 4:
                return m.YR;
        case 5:
                return m.ZR;
        default:
                return m.Q;
        }
}

// The dump: everything the load path leaves behind, plus the query probes.
static void dump_anim(const HRawAnimClass &anim, FILE *out, const char *id)
{
        fprintf(out, "hdr %s %s %s %s %d %s %s %d\n", id, u32hex(anim.Header.Version).c_str(),
                q_bytes((const uint8_t *)anim.Get_Name(), cstr_len(anim.Get_Name(), 2 * W3D_NAME_LEN)).c_str(),
                q_bytes((const uint8_t *)anim.Get_HName(), cstr_len(anim.Get_HName(), W3D_NAME_LEN)).c_str(),
                anim.Get_Num_Frames(), f32hex(anim.Get_Frame_Rate()).c_str(),
                f32hex(anim.Get_Total_Time()).c_str(), anim.Get_Num_Pivots());

        // the Has_* probes and Is_Node_Motion_Present
        for (int i = 0; i < anim.Get_Num_Pivots(); i++) {
                fprintf(out, "has %s %d %d %d %d %d %d %d\n", id, i, anim.Has_X_Translation(i) ? 1 : 0,
                        anim.Has_Y_Translation(i) ? 1 : 0, anim.Has_Z_Translation(i) ? 1 : 0,
                        anim.Has_Rotation(i) ? 1 : 0, anim.Has_Visibility(i) ? 1 : 0,
                        anim.Is_Node_Motion_Present(i) ? 1 : 0);
        }

        // the stored channels
        for (int i = 0; i < anim.Get_Num_Pivots(); i++) {
                const NodeMotionStruct &m = anim.NodeMotion[i];
                for (int slot = 0; slot < 7; slot++) {
                        const char *sn = slot_name(m, slot);
                        if (sn == nullptr) {
                                continue;
                        }
                        const MotionChannelClass *c = slot_chan(m, slot);
                        std::string words;
                        for (int k = 0; k < (c->LastFrame - c->FirstFrame + 1) * c->VectorLen; k++) {
                                if (k > 0)
                                        words += " ";
                                words += f32hex(c->Data[k]);
                        }
                        fprintf(out, "chan %s %d %s %u %d %d %d %s\n", id, i, sn, c->Type, c->FirstFrame,
                                c->LastFrame, c->VectorLen, words.c_str());
                }
                if (m.Vis != nullptr) {
                        const BitChannelClass *c = m.Vis;
                        int numbits = c->LastFrame - c->FirstFrame + 1;
                        int numbytes = (numbits + 7) / 8;
                        std::string bytes;
                        hex_append(bytes, c->Bits, (size_t)numbytes);
                        fprintf(out, "bchan %s %d %u %d %d %d %s\n", id, i, c->Type, c->FirstFrame,
                                c->LastFrame, c->DefaultVal, bytes.c_str());
                }
        }

        // the frame queries
        std::vector<float> frames = probe_frames(anim.Get_Num_Frames());
        for (int piv : probe_pivots(anim.Get_Num_Pivots())) {
                for (float frame : frames) {
                        float t[3];
                        anim.Get_Translation(t, piv, frame);
                        fprintf(out, "trans %s %d %s %s %s %s\n", id, piv, f32hex(frame).c_str(),
                                f32hex(t[0]).c_str(), f32hex(t[1]).c_str(), f32hex(t[2]).c_str());

                        float q[4];
                        anim.Get_Orientation(q, piv, frame);
                        fprintf(out, "orient %s %d %s %s %s %s %s\n", id, piv, f32hex(frame).c_str(),
                                f32hex(q[0]).c_str(), f32hex(q[1]).c_str(), f32hex(q[2]).c_str(),
                                f32hex(q[3]).c_str());

                        // float[16]: the transform's last translation
                        // slot (mtx[11]) plus a Q-typed set_identity can
                        // reach 3 floats past the matrix — the original's
                        // own overrun potential, corpus-disciplined away;
                        // the extra slots are scratch, [0..12) is the dump
                        float m[16];
                        anim.Get_Transform(m, piv, frame);
                        fprintf(out, "xform %s %d %s", id, piv, f32hex(frame).c_str());
                        for (int k = 0; k < 12; k++) {
                                fprintf(out, " %s", f32hex(m[k]).c_str());
                        }
                        fprintf(out, "\n");

                        fprintf(out, "vis %s %d %s %d\n", id, piv, f32hex(frame).c_str(),
                                anim.Get_Visibility(piv, frame) ? 1 : 0);
                }
        }
}

} // namespace

// --- main -------------------------------------------------------------------------

int main(int argc, char **argv)
{
        if (argc != 2) {
                fprintf(stderr, "usage: gen_hanim_golden <output-file>\n");
                return 1;
        }

        FILE *out = fopen(argv[1], "w");
        if (!out) {
                fprintf(stderr, "error: cannot open %s for writing\n", argv[1]);
                return 1;
        }

        // the tree table (lowercase keys, like the manager's hash)
        g_trees.push_back({{'t', 'a', 'n', 'k'}, 6});
        g_trees.push_back({{'t'}, 1});
        g_trees.push_back({{'r', 'i', 'g'}, 4});
        g_trees.push_back({{'s', 'o', 'l', 'd', 'i', 'e', 'r'}, 24});
        g_trees.push_back({{'t', 'a', 'n', 'k', 't', 'r', 'e', 'e', '1', '2', '3', '4', '5', '6', '7'}, 1});
        // a24's 17-byte strcpy key: 16 'h's (lowercased 'H') plus the 0x05
        // swallowed from the NumFrames word
        g_trees.push_back({{'h', 'h', 'h', 'h', 'h', 'h', 'h', 'h', 'h', 'h', 'h', 'h', 'h', 'h', 'h', 'h', 0x05}, 3});
        // a25's trap: the 16 field bytes as a tree — the real 17-byte key
        // must NOT resolve against it
        g_trees.push_back({{'z', 'z', 'z', 'z', 'z', 'z', 'z', 'z', 'z', 'z', 'z', 'z', 'z', 'z', 'z', 'z'}, 1});

        WWMath_Init();

        fprintf(out, "# gen_hanim_golden output — the W3D raw animation corpus (P0)\n");
        fprintf(out, "# generated by tools/differential/gen_hanim_golden.cpp; do not edit\n");

        for (const HTreeStub &t : g_trees) {
                fprintf(out, "tree %s %d\n", q_bytes(t.lower_name.data(), t.lower_name.size()).c_str(), t.pivots);
        }

        for (const Corpus &c : build_corpora()) {
                SrcFile file;
                file.data = c.bytes.data();
                file.len = c.bytes.size();
                ChunkLoadClass cload(&file);

                fprintf(out, "file %s %s\n", c.id, hex_str(c.bytes).c_str());

                HRawAnimClass anim;
                int error = anim.Load_W3D(cload);
                if (error == HRawAnimClass::OK) {
                        fprintf(out, "ok %s\n", c.id);
                        dump_anim(anim, out, c.id);
                } else {
                        fprintf(out, "err %s\n", c.id);
                }
        }

        fclose(out);
        return 0;
}
