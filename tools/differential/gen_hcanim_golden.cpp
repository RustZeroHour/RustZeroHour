// SPDX-License-Identifier: GPL-3.0-or-later
//
// Golden-dump driver for `rzh-formats::w3d_hcanim` (parity level P0).
//
// Verbatim transcription of the Zero Hour compressed-animation loader —
// GeneralsMD/Code/Libraries/Source/WWVegas/WW3D2/hcanim.cpp at commit
// 571a7c5 (HCompressedAnimClass::Load_W3D, read_channel overloads,
// read_bit_channel, add_channel overloads, add_bit_channel,
// Get_Translation, Get_Orientation, Get_Transform, Get_Visibility,
// Is_Node_Motion_Present and the Has_* probes), plus the pieces those
// paths drive:
//   GeneralsMD/Code/Libraries/Source/WWVegas/WW3D2/motchan.cpp
//     (TimeCodedMotionChannelClass::Load_W3D, Get_Vector,
//      Get_QuatVector, get_index, binary_search_index, set_identity
//      — dead: the time-coded Get_Vector never calls it, transcribed
//      for fidelity; TimeCodedBitChannelClass::Load_W3D, Get_Bit;
//      AdaptiveDeltaMotionChannelClass::Load_W3D, decompress overloads,
//      getframe, Get_Vector, Get_QuatVector; the filtertable and its
//      constructor-time generation)
//   GeneralsMD/Code/Libraries/Source/WWVegas/WW3D2/w3d_file.h
//     (W3dCompressedAnimHeaderStruct, W3dTimeCodedAnimChannelStruct,
//      W3dTimeCodedBitChannelStruct, W3dAdaptiveDeltaAnimChannelStruct,
//      W3D_TIMECODED_BINARY_MOVEMENT_FLAG, W3D_TIMECODED_BIT_MASK,
//      the chunk ids, the channel-type and flavor enums)
//   GeneralsMD/Code/Libraries/Source/WWVegas/WW3D2/hcanim.h
//     (the HCompressedAnimClass layout: Name[32], HierarchyName[16],
//      NumFrames, NumNodes, Flavor, FrameRate — the member order the
//      strcpy overflow discipline depends on)
//   Core/Libraries/Source/WWVegas/WWMath/wwmath.{h,cpp}
//     (Float_To_Int_Floor, Fast_Acos/Fast_Sin over the WWMath::Init
//     tables, WWMATH_EPSILON/PI, WWMath::Lerp, DEG_TO_RAD, and
//     Sinf_Legacy — the USE_DETERMINISTIC_MATH branch, gm_sinf)
//   Core/Libraries/Source/WWVegas/WWMath/quat.cpp (Fast_Slerp — the
//     portable #else branch; the x86-asm #if 0 block is not compiled)
//   Core/Libraries/Source/WWVegas/WWMath/quat.h (Build_Matrix3D — the
//     mixed f64/f32 literal builder)
// The pinned GameMath sources (59f7ccd4) supply gm_acosf and gm_sinf;
// the fast tables are computed by the WWMath::Init transcription from
// them, and the filter table by the constructor transcription, exactly
// like the game's startup.
// The chunk container is the chunkio.cpp reader already pinned by
// gen_chunk_golden.cpp; it is transcribed here again (release-build
// semantics) because this harness is self-contained. strlcat is the
// canonical BSD body (the loader's platform provides one; every
// implementation agrees).
//
// The corpus bytes are the payload of the W3D_CHUNK_COMPRESSED_ANIMATION
// container the asset manager opens before calling
// HAnimManagerClass::Load_Compressed_Anim (hanimmgr.cpp); the harness
// enters the loader exactly as the manager does. The skeleton the
// animation resolves against is modelled by the tree table at the top of
// the output: Get_HTree is Get_Tree (htreemgr.cpp) — lowercase the name,
// exact hash lookup — over the registered (lowercased name, pivot count)
// pairs.
//
// Deliberate deviations and corpus disciplines, pinned identically on
// both sides (the Rust port documents the same set):
//
// - The loader reads its 44-byte header with
//   sizeof(W3dAnimHeaderStruct) — the RAW animation header's size — not
//   sizeof(W3dCompressedAnimHeaderStruct). Both structs are 44 bytes, so
//   the read is complete either way; the transcription keeps the
//   original's expression (and its static_assert proves the equality).
//
// - The strcpy that builds the class's HierarchyName member runs through
//   the header's NumFrames word when the 16-byte field has no NUL. The
//   copy itself is deterministic in-struct behaviour and IS pinned (c24's
//   17-byte key). Corpora keep the string's NUL inside the field or the
//   NumFrames word — past that the class-member reassignments make the
//   observable member a layout-dependent result that is not reproducible.
//
// - A flavor outside {ANIM_FLAVOR_TIMECODED, ANIM_FLAVOR_ADAPTIVE_DELTA}
//   loads: the loader's switch reads no motion channels at all (the
//   WWASSERT is compiled out), the bit channels still route, and
//   Get_Translation returns the zero vector it pre-assigned. The original
//   leaves Get_Orientation's and Get_Transform's caller-supplied
//   destinations UNTOUCHED for such flavors (the switch's default is a
//   compiled-out assert) — a function of the caller, not the animation —
//   so the dump omits the orientation/transform probes for the
//   unknown-flavor corpus (both sides apply the same rule; the port
//   returns the identity there instead, a stated difference).
//
// - The float-to-uint32_t frame conversions (`tc0 = frame`,
//   `uint32_t frame1 = frame`) are undefined in C++ for negative or
//   oversized inputs and differ between the x86-64 transcription
//   compilers (cvttss2si to 64 bits, then truncation) and ARM
//   (fcvtzu, saturating). The motion probes therefore use non-negative
//   frames only; the visibility probes add -1.0f/-0.5f because
//   Get_Bit's `(int)frame` truncation is well-defined there.
//
// - The time-coded channels' trailing payload is whole uint32_t words (the
//   format's packets are u32-aligned; a remainder would overrun the heap
//   array by that many bytes in the original — corpus-disciplined away).
//   The adaptive-delta channels pad their 9-byte packet blocks up to a
//   whole word exactly like the exporter (the pad bytes land in the
//   array's tail, never read by the walk); unpadded data would overrun
//   the original's numInts-sized allocation.
//
// - An adaptive-delta channel with NumFrames 0, VectorLen 0 or
//   VectorLen above 4 loads in the original but its first query walks
//   off the packet array / smashes the 4-float stack scratch
//   (getframe's assert is compiled out). Corpora avoid them: frames
//   probes only sane channels (real channels carry VectorLen 1 or 4 and
//   at least one frame).
//
// - The time-coded motion channel's declared NumTimeCodes must agree
//   with the data present (the query path indexes the packet array by
//   the declared count; a mismatch reads past the allocation in the
//   original — corpus-disciplined away). Timecodes are sorted ascending
//   (the exporter's invariant; the binary search assumes it).
//
// - Quaternion channels carry VectorLen 4 and translation channels 1..2
//   (the X channel's VectorLen-2 corpus pins the multi-word write that
//   reaches trans[1] and the transform's m[4]; anything wider would
//   overrun the caller's Vector3 in the original).
//
// - The adaptive-delta cache-continue path (getframe's final branch)
//   passes the CACHED ROW-1 data with src_idx = CacheFrame, so the walk
//   re-applies the delta for CacheFrame+1 — the original's off-by-one.
//   It is transcribed verbatim and pinned (c29's far-jump probes); the
//   cache self-heals on the next decompress.
//
// - The frame probes are a fixed list plus count-derived frames when
//   1 <= NumFrames <= 1000 (and a wider set below 2^25, for the
//   big-timecode corpus); both sides compute the same list. The
//   per-corpus queries run for pivots 0..min(8, NumNodes) plus the last
//   pivot, keeping the dump bounded for the 24-bone corpus.
//
// Output grammar (one command per line, hex lowercase, '-' = empty):
//
//   tree <name|-> <count>     a registered hierarchy (lowercase key)
//   file <id> <hex|->         the corpus bytes (animation payload)
//   ok <id> | err <id>        the load verdict
//   hdr <id> <ver> <name|-> <hname|-> <flavor> <numframes> <frerate> <totaltime> <numnodes>
//                             ver: version as 8 hex digits; name: the
//                             composed Get_Name; hname: Get_HName capped
//                             at the 16-byte field; flavor: the header's
//                             flavor; numframes: the member count as
//                             int; frerate / totaltime: f32 bit words
//                             (the numeric FrameRate conversion and
//                             NumFrames/FrameRate)
//   has <id> <piv> <x> <y> <z> <rot> <vis> <present>
//                             the Has_* probes and Is_Node_Motion_Present,
//                             0|1 each
//   tcchan <id> <piv> <slot> <type> <veclen> <numtc> <words...>
//                             one stored time-coded motion channel; slot
//                             is one of x y z q; words: the packet array
//                             as u32 bit words (timecodes and vector
//                             words interleaved as stored)
//   adchan <id> <piv> <slot> <type> <veclen> <numframes> <scale> <words...>
//                             one stored adaptive-delta channel; scale:
//                             the f32 bit word; words: the data array as
//                             u32 bit words (base floats and packet
//                             bytes)
//   tcbchan <id> <piv> <type> <default> <numtc> <words...>
//                             one stored time-coded bit channel; words:
//                             the time-code array as u32 bit words
//   trans <id> <piv> <frame> <x> <y> <z>
//                             Get_Translation at the probe frame; frame
//                             and results are f32 bit words
//   orient <id> <piv> <frame> <x> <y> <z> <w>
//                             Get_Orientation (omitted for the
//                             unknown-flavor corpus)
//   xform <id> <piv> <frame> <m0..m11>
//                             Get_Transform, row-major (omitted for the
//                             unknown-flavor corpus)
//   vis <id> <piv> <frame> <0|1>
//                             Get_Visibility (the vis probes add two
//                             negative frames after the shared list)
//
// Usage: gen_hcanim_golden <output-file>
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
#define ARRAY_SIZE(x) (sizeof(x) / sizeof((x)[0]))

typedef float float32_t;

// enum values at 0x00000280 in w3d_file.h
static const uint32_t W3D_CHUNK_COMPRESSED_ANIMATION = 0x00000280;       // compressed hierarchy animation data
static const uint32_t W3D_CHUNK_COMPRESSED_ANIMATION_HEADER = 0x00000281; // W3dCompressedAnimHeaderStruct
static const uint32_t W3D_CHUNK_COMPRESSED_ANIMATION_CHANNEL = 0x00000282; // compressed channel, format depends on compression type
static const uint32_t W3D_CHUNK_COMPRESSED_BIT_CHANNEL = 0x00000283;     // compressed bit stream channel

// channel types (the Flags field) — the add_channel switch routes the
// PLAIN types; the TIMECODED_/ADAPTIVEDELTA_ variants (7..14) exist in
// the enum but never route (the loader's switch has no cases for them)
enum
{
        ANIM_CHANNEL_X = 0,
        ANIM_CHANNEL_Y,
        ANIM_CHANNEL_Z,
        ANIM_CHANNEL_XR,
        ANIM_CHANNEL_YR,
        ANIM_CHANNEL_ZR,
        ANIM_CHANNEL_Q,

        ANIM_CHANNEL_TIMECODED_X,
        ANIM_CHANNEL_TIMECODED_Y,
        ANIM_CHANNEL_TIMECODED_Z,
        ANIM_CHANNEL_TIMECODED_Q,
};

enum
{
        BIT_CHANNEL_VIS = 0,
        BIT_CHANNEL_TIMECODED_VIS,
};

// Flavor enumerations
enum
{
        ANIM_FLAVOR_TIMECODED = 0,
        ANIM_FLAVOR_ADAPTIVE_DELTA,
};

#define W3D_TIMECODED_BINARY_MOVEMENT_FLAG 0x80000000u
#define W3D_TIMECODED_BIT_MASK 0x80000000u

// --- the W3D structures (w3d_file.h, POD) -------------------------------------

struct W3dCompressedAnimHeaderStruct
{
        uint32_t Version;
        char Name[W3D_NAME_LEN];         // Name of the animation
        char HierarchyName[W3D_NAME_LEN]; // Name of the hierarchy tree this animation works with
        uint32_t NumFrames;               // Number of frames in the animation
        uint16_t FrameRate;               // Number of frames per second (an integer!)
        uint16_t Flavor;                  // Compression flavor for this animation
};

// the RAW animation's header — the loader reads its 44 bytes with this
// struct's sizeof (both are 44; the static_assert pins the coincidence)
struct W3dAnimHeaderStruct
{
        uint32_t Version;
        char Name[W3D_NAME_LEN];
        char HierarchyName[W3D_NAME_LEN];
        uint32_t NumFrames;
        uint32_t FrameRate;
};

// A time code is a uint32_t that prefixes each vector; the MSB marks a
// binary (non interpolated) movement
struct W3dTimeCodedAnimChannelStruct
{
        uint32_t NumTimeCodes; // number of time coded entries
        uint16_t Pivot;        // pivot affected by this channel
        uint8_t VectorLen;     // length of each vector in this channel
        uint8_t Flags;         // channel type.
        uint32_t Data[1];      // will be (NumTimeCodes * ((VectorLen * sizeof(uint32_t)) + sizeof(uint32_t)))
};

// The bit channel is encoded right into the MSB of each time code
struct W3dTimeCodedBitChannelStruct
{
        uint32_t NumTimeCodes; // number of time coded entries
        uint16_t Pivot;        // pivot affected by this channel
        uint8_t Flags;         // channel type.
        uint8_t DefaultVal;    // default state when outside valid range.
        uint32_t Data[1];      // will be (NumTimeCodes * sizeof(uint32_t))
};

struct W3dAdaptiveDeltaAnimChannelStruct
{
        uint32_t NumFrames; // number of frames of animation
        uint16_t Pivot;     // pivot effected by this channel
        uint8_t VectorLen;  // num Channels
        uint8_t Flags;      // channel type
        float Scale;        // Filter Table Scale

        uint32_t Data[1]; // OpCode Data Stream
};

static_assert(sizeof(W3dCompressedAnimHeaderStruct) == 44, "W3dCompressedAnimHeaderStruct layout");
static_assert(sizeof(W3dAnimHeaderStruct) == sizeof(W3dCompressedAnimHeaderStruct),
              "the loader reads with the raw header's sizeof");
static_assert(sizeof(W3dTimeCodedAnimChannelStruct) == 12, "W3dTimeCodedAnimChannelStruct layout");
static_assert(sizeof(W3dTimeCodedBitChannelStruct) == 12, "W3dTimeCodedBitChannelStruct layout");
static_assert(sizeof(W3dAdaptiveDeltaAnimChannelStruct) == 16, "W3dAdaptiveDeltaAnimChannelStruct layout");

// --- the ASCII case folding (the manager's _strlwr) ----------------------------

static unsigned char ascii_tolower(unsigned char c)
{
        return (c >= 'A' && c <= 'Z') ? (unsigned char)(c - 'A' + 'a') : c;
}

// --- strlcat (the canonical BSD body) ------------------------------------------

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
        // the original's Seek is never reached by this loader (the
        // compressed channels read exactly their computed byte counts,
        // and Close_Chunk re-seeks to the chunk end), so it is not
        // transcribed here

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

// --- the WWMath subset (wwmath.h / wwmath.cpp / quat.cpp / quat.h) --------------

#define WWMATH_EPSILON 0.0001f
#define WWMATH_PI 3.141592654f

#define DEG_TO_RAD(x) (((double)x)*WWMATH_PI/180.0)

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

// WWMath::Float_To_Int_Floor (wwmath.h) — the interpolation index
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

// WWMath::Sinf_Legacy (wwmath.h) — the USE_DETERMINISTIC_MATH branch the
// Zero Hour build compiles (the x87 fsin path is not)
static inline float WWSinf_Legacy(float val)
{
        return gm_sinf(val);
}

// WWMath::Lerp (wwmath.h)
static inline float WWLerp(float a, float b, float t)
{
        return (a + (b - a) * t);
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

// --- the adaptive-delta filter table (motchan.cpp) -------------------------------

#define FILTER_TABLE_SIZE (256)
#define FILTER_TABLE_GEN_START (16)
#define FILTER_TABLE_GEN_SIZE  (FILTER_TABLE_SIZE - FILTER_TABLE_GEN_START)

static float filtertable[FILTER_TABLE_SIZE] = {
        0.00000001f,
        0.0000001f,
        0.000001f,
        0.00001f,
        0.0001f,
        0.001f,
        0.01f,
        0.1f,
        1.0f,
        10.0f,
        100.0f,
        1000.0f,
        10000.0f,
        100000.0f,
        1000000.0f,
        10000000.0f,

};
static bool table_valid = false;

// --- the time-coded motion channel (motchan.cpp) --------------------------------

class TimeCodedMotionChannelClass
{
public:
        TimeCodedMotionChannelClass();
        ~TimeCodedMotionChannelClass();

        bool Load_W3D(ChunkLoadClass &cload);
        int Get_Type() const { return Type; }
        uint32_t Get_Pivot() const { return PivotIdx; }
        void Get_Vector(float32_t frame, float *setvec);
        void Get_QuatVector(float32_t frame, float *q /* 4 floats */);

        // dump-side views of the loaded state
        uint32_t NumTimeCodes;
        int VectorLen;
        uint32_t Type;
        uint32_t PivotIdx;
        uint32_t PacketSize;
        std::vector<uint32_t> Data;

private:
        void Free();
        // NOTE: the original carries a private set_identity member that
        // nothing ever calls (the time-coded Get_Vector has no out-of-range
        // branch); it is not transcribed (its body only writes constants)
        uint32_t get_index(uint32_t timecode);
        uint32_t binary_search_index(uint32_t timecode);

        uint32_t LastTimeCodeIdx; // absolute index to last time code
        uint32_t CachedIdx;       // Last Index Used
};

TimeCodedMotionChannelClass::TimeCodedMotionChannelClass() :
        NumTimeCodes(0),
        VectorLen(0),
        Type(0),
        PivotIdx(0),
        PacketSize(0),
        LastTimeCodeIdx(0), // absolute index to last time code
        CachedIdx(0)        // Last Index Used
{
}

TimeCodedMotionChannelClass::~TimeCodedMotionChannelClass()
{
        Free();
}

void TimeCodedMotionChannelClass::Free()
{
        Data.clear();
}

bool TimeCodedMotionChannelClass::Load_W3D(ChunkLoadClass &cload)
{
        int size = cload.Cur_Chunk_Length();
        unsigned int datasize = size - sizeof(W3dTimeCodedAnimChannelStruct);
        unsigned int numInts = (datasize / sizeof(uint32_t)) + 1;

        W3dTimeCodedAnimChannelStruct chan;
        if (cload.Read(&chan, sizeof(W3dTimeCodedAnimChannelStruct)) != sizeof(W3dTimeCodedAnimChannelStruct)) {
                return false;
        }

        NumTimeCodes = chan.NumTimeCodes;
        VectorLen = chan.VectorLen;
        Type = chan.Flags;
        PivotIdx = chan.Pivot;
        PacketSize = VectorLen + 1;
        CachedIdx = 0;
        LastTimeCodeIdx = (NumTimeCodes - 1) * PacketSize;

        Data.resize(numInts);
        Data[0] = chan.Data[0];

        // corpus discipline: the trailing payload is whole uint32_t words,
        // so this read stays inside the numInts-sized array (the original
        // would overrun it by any remainder bytes)
        if (cload.Read(&(Data[1]), datasize) != datasize) {
                Free();
                return false;
        }
        return true;
}

void TimeCodedMotionChannelClass::Get_Vector(float32_t frame, float *setvec)
{
        uint32_t tc0;

        tc0 = frame;

        uint32_t pidx = get_index(tc0);
        uint32_t p2idx;

        if (pidx == ((NumTimeCodes - 1) * PacketSize)) {

                float32_t *frm = (float32_t *)&Data[pidx + 1];

                for (int i = 0; i < VectorLen; i++) {

                        setvec[i] = frm[i];
                }

                return;

        } else {
                p2idx = pidx + PacketSize;
        }

        uint32_t time = Data[p2idx];

        if (time & W3D_TIMECODED_BINARY_MOVEMENT_FLAG) {
                float32_t *frm = (float32_t *)&Data[pidx + 1];
                for (int i = 0; i < VectorLen; i++) {
                        setvec[i] = frm[i];
                }
                return;
        }

        float32_t time1 = (Data[pidx] & ~W3D_TIMECODED_BINARY_MOVEMENT_FLAG);
        float32_t time2 = (time & ~W3D_TIMECODED_BINARY_MOVEMENT_FLAG);

        float32_t ratio = (frame - time1) / (time2 - time1);

        float32_t *frame1 = (float32_t *)&Data[pidx + 1];
        float32_t *frame2 = (float32_t *)&Data[p2idx + 1];

        for (int i = 0; i < VectorLen; i++) {

                setvec[i] = WWLerp(frame1[i], frame2[i], ratio);
        }
}

void TimeCodedMotionChannelClass::Get_QuatVector(float32_t frame, float *q)
{
        // assert(VectorLen == 4) — release no-op; the corpus keeps Q
        // channels at VectorLen 4 so the flat reads below stay in bounds

        uint32_t tc0;

        tc0 = frame;

        uint32_t pidx = get_index(tc0);
        uint32_t p2idx;

        if (pidx == ((NumTimeCodes - 1) * PacketSize)) {

                float32_t *vec = (float32_t *)&Data[pidx + 1];

                q[0] = vec[0];
                q[1] = vec[1];
                q[2] = vec[2];
                q[3] = vec[3];

                return;

        } else {
                p2idx = pidx + PacketSize;
        }

        uint32_t time = Data[p2idx];

        if (time & W3D_TIMECODED_BINARY_MOVEMENT_FLAG) {
                // its a binary movement!
                float32_t *vec = (float32_t *)&Data[pidx + 1];

                q[0] = vec[0];
                q[1] = vec[1];
                q[2] = vec[2];
                q[3] = vec[3];

                return;
        }

        float32_t time1 = (Data[pidx] & ~W3D_TIMECODED_BINARY_MOVEMENT_FLAG);
        float32_t time2 = (time & ~W3D_TIMECODED_BINARY_MOVEMENT_FLAG);

        float32_t ratio = (frame - time1) / (time2 - time1);

        float32_t *frame1 = (float32_t *)&Data[pidx + 1];
        float32_t *frame2 = (float32_t *)&Data[p2idx + 1];

        float q1[4];
        float q2[4];

        q1[0] = frame1[0];
        q1[1] = frame1[1];
        q1[2] = frame1[2];
        q1[3] = frame1[3];
        q2[0] = frame2[0];
        q2[1] = frame2[1];
        q2[2] = frame2[2];
        q2[3] = frame2[3];

        Fast_Slerp(q, q1, q2, ratio);
}

// New version that uses a binary search, and no cache
uint32_t TimeCodedMotionChannelClass::binary_search_index(uint32_t timecode)
{
        int leftIdx = 0;
        int rightIdx = NumTimeCodes - 2;
        int dx;
        uint32_t time;

        int idx = LastTimeCodeIdx; //((rightIdx+1) * PacketSize;)

        // special case last packet
        time = Data[idx] & ~W3D_TIMECODED_BINARY_MOVEMENT_FLAG;
        if (timecode >= time)
                return (idx);

        for (;;) {

                dx = rightIdx - leftIdx;

                dx >>= 1; // divide by 2

                dx += leftIdx;

                idx = dx * PacketSize;

                time = Data[idx] & ~W3D_TIMECODED_BINARY_MOVEMENT_FLAG;

                if (timecode < time) {
                        rightIdx = dx;
                        continue;
                }

                time = Data[idx + PacketSize] & ~W3D_TIMECODED_BINARY_MOVEMENT_FLAG;

                if (timecode < time)
                        return (idx);

                if (leftIdx ^ dx) {
                        leftIdx = dx;
                        continue;
                }

                //
                // if leftIdx == dx prior to assignment, then leftIdx is stuck.
                //

                leftIdx++;
        }

        // assert(0) — release no-op (unreachable for sorted time codes)
}

uint32_t TimeCodedMotionChannelClass::get_index(uint32_t timecode)
{
        // assert(CachedIdx <= LastTimeCodeIdx) — release no-op

        uint32_t time;

        time = Data[CachedIdx] & ~W3D_TIMECODED_BINARY_MOVEMENT_FLAG;

        if (timecode >= time) {
                // possibly in the current packet

                // special case for end packets
                if (CachedIdx == LastTimeCodeIdx)
                        return (CachedIdx);
                time = Data[CachedIdx + PacketSize] & ~W3D_TIMECODED_BINARY_MOVEMENT_FLAG;
                if (timecode < time)
                        return (CachedIdx);

                // Do one time look-ahead before reverting to a search
                CachedIdx += PacketSize;
                if (CachedIdx == LastTimeCodeIdx)
                        return (CachedIdx);
                time = Data[CachedIdx + PacketSize] & ~W3D_TIMECODED_BINARY_MOVEMENT_FLAG;
                if (timecode < time)
                        return (CachedIdx);
        }

        CachedIdx = binary_search_index(timecode);

        return (CachedIdx);
}

// --- the time-coded bit channel (motchan.cpp) ------------------------------------

class TimeCodedBitChannelClass
{
public:
        TimeCodedBitChannelClass();
        ~TimeCodedBitChannelClass();

        bool Load_W3D(ChunkLoadClass &cload);
        int Get_Type() const { return Type; }
        uint32_t Get_Pivot() const { return PivotIdx; }
        int Get_Bit(int frame);

        // dump-side views of the loaded state
        uint32_t Type;
        uint32_t PivotIdx;
        int DefaultVal;
        uint32_t NumTimeCodes;
        std::vector<uint32_t> Bits;

private:
        void Free();

        uint32_t CachedIdx;
};

TimeCodedBitChannelClass::TimeCodedBitChannelClass() :
        Type(0),
        PivotIdx(0),
        DefaultVal(0),
        NumTimeCodes(0),
        CachedIdx(0)
{
}

TimeCodedBitChannelClass::~TimeCodedBitChannelClass()
{
        Free();
}

void TimeCodedBitChannelClass::Free()
{
        Bits.clear();
}

bool TimeCodedBitChannelClass::Load_W3D(ChunkLoadClass &cload)
{
        Free();

        W3dTimeCodedBitChannelStruct chan;
        if (cload.Read(&chan, sizeof(W3dTimeCodedBitChannelStruct)) != sizeof(W3dTimeCodedBitChannelStruct)) {
                return false;
        }

        NumTimeCodes = chan.NumTimeCodes;
        Type = chan.Flags;
        PivotIdx = chan.Pivot;
        DefaultVal = chan.DefaultVal;
        CachedIdx = 0;

        uint32_t bytesleft = (NumTimeCodes - 1) * sizeof(uint32_t);

        // assert((sizeof(W3dTimeCodedBitChannelStruct) + bytesleft) == chunk_size)
        // — release no-op

        // NOTE: for NumTimeCodes 0 the original's `new uint32[0]` returns a
        // valid zero-size block, `Bits[0] = chan.Data[0]` writes four bytes
        // past it (release UB — collateral before the wrapped ~4 GiB read
        // fails the channel), and the vector transcription would write to
        // a null pointer instead. The store is guarded; the verdict (the
        // read fails, the load errors) is identical — c42 pins it with the
        // channel as the file's last chunk so the file read finds no bytes.
        Bits.resize(NumTimeCodes);
        if (NumTimeCodes > 0) {
                Bits[0] = chan.Data[0];
        }

        if (bytesleft > 0) {
                if (cload.Read(&(Bits[1]), bytesleft) != bytesleft) {
                        Free();
                        return false;
                }
        }

        return true;
}

int TimeCodedBitChannelClass::Get_Bit(int frame)
{
        // assert(frame >= 0) — release no-op (negative frames read packet
        // 0's bit below); assert(CachedIdx < NumTimeCodes) — the corpus
        // keeps the declared count consistent with the data

        int time;
        int idx = 0;

        time = Bits[CachedIdx] & ~W3D_TIMECODED_BIT_MASK;

        if (frame >= time) {

                // start from here
                idx = CachedIdx + 1;
        }

        for (; idx < (int)NumTimeCodes; idx++) {

                time = Bits[idx] & ~W3D_TIMECODED_BIT_MASK;

                if (frame < time)
                        break;
        }

        idx--;

        if (idx < 0)
                idx = 0;

        CachedIdx = idx;

        return (((Bits[idx] & W3D_TIMECODED_BIT_MASK) == W3D_TIMECODED_BIT_MASK));
}

// --- the adaptive-delta motion channel (motchan.cpp) ------------------------------

#define PACKET_SIZE (9)

class AdaptiveDeltaMotionChannelClass
{
public:
        AdaptiveDeltaMotionChannelClass();
        ~AdaptiveDeltaMotionChannelClass();

        bool Load_W3D(ChunkLoadClass &cload);
        int Get_Type() const { return Type; }
        uint32_t Get_Pivot() const { return PivotIdx; }
        void Get_Vector(float32_t frame, float *setvec);
        void Get_QuatVector(float32_t frame, float *q /* 4 floats */);

        // dump-side views of the loaded state
        int VectorLen;
        uint32_t Type;
        uint32_t PivotIdx;
        uint32_t NumFrames;
        float Scale;
        std::vector<uint32_t> Data;

private:
        void Free();

        float getframe(uint32_t frame_idx, uint32_t vector_idx = 0);
        void decompress(uint32_t frame_idx, float *outdata);
        void decompress(uint32_t src_idx, float *srcdata, uint32_t frame_idx, float *outdata);

        uint32_t CacheFrame;
        std::vector<float> CacheData;
};

AdaptiveDeltaMotionChannelClass::AdaptiveDeltaMotionChannelClass() :
        VectorLen(0),
        Type(0),
        PivotIdx(0),
        NumFrames(0),
        Scale(0.0f),
        CacheFrame(0)
{
        if (false == table_valid) {
                // Create Filter Table, used in delta compression

                for (int i = 0; i < FILTER_TABLE_GEN_SIZE; i++) {
                        float ratio = i;

                        //ratio = ((ratio + 1.0f) / 128.0f);
                        ratio /= ((float)FILTER_TABLE_GEN_SIZE);

                        filtertable[i + FILTER_TABLE_GEN_START] = 1.0f - WWSinf_Legacy(DEG_TO_RAD(90.0f * ratio));
                }

                table_valid = true;
        }
}

AdaptiveDeltaMotionChannelClass::~AdaptiveDeltaMotionChannelClass()
{
        Free();
}

void AdaptiveDeltaMotionChannelClass::Free()
{
        Data.clear();
        CacheData.clear();
}

bool AdaptiveDeltaMotionChannelClass::Load_W3D(ChunkLoadClass &cload)
{
        int size = cload.Cur_Chunk_Length();
        unsigned int datasize = size - sizeof(W3dAdaptiveDeltaAnimChannelStruct);
        unsigned int numInts = (datasize / sizeof(uint32_t)) + 1;

        W3dAdaptiveDeltaAnimChannelStruct chan;
        if (cload.Read(&chan, sizeof(W3dAdaptiveDeltaAnimChannelStruct)) != sizeof(W3dAdaptiveDeltaAnimChannelStruct)) {
                return false;
        }

        VectorLen = chan.VectorLen;
        Type = chan.Flags;
        PivotIdx = chan.Pivot;
        NumFrames = chan.NumFrames;
        Scale = chan.Scale;
        CacheFrame = 0x7FFFFFFF; // a big number, so we know its not valid
        CacheData.resize(VectorLen * 2);

        Data.resize(numInts);
        Data[0] = chan.Data[0];

        // corpus discipline: the exporter pads the packet blocks up to a
        // whole uint32_t word, so this read stays inside the numInts-sized
        // array (unpadded data would overrun it by the pad deficiency)
        if (cload.Read(&(Data[1]), datasize) != datasize) {
                Free();
                return false;
        }
        return true;
}

void AdaptiveDeltaMotionChannelClass::decompress(uint32_t frame_idx, float *outdata)
{
        // Start Over from the beginning
        float *base = (float *)Data.data(); // pointer to our true know beginning values

        bool done = false;

        for (int vi = 0; vi < VectorLen; vi++) {
                // Decompress all the vector indices, since they will probably all be needed
                unsigned char *pPacket = (unsigned char *)Data.data(); // pointer to current packet
                pPacket += (sizeof(float) * VectorLen);  // skip non-compressed header information
                pPacket += PACKET_SIZE * vi;             // skip to the appropriate packet start

                float last_value = base[vi];

                for (uint32_t frame = 1; frame <= frame_idx;) {
                        // Frame Loop
                        uint32_t filter_index = *pPacket; // packet header

                        pPacket++; // skip to nybble compressed data

                        float filter = filtertable[filter_index] * Scale; // decompression filter

                        // data is grouped in sets of 16 nybbles
                        for (int fi = 0; fi < 16; fi++) {

                                int pi = fi >> 1; // create packet index

                                int factor; // factor, contains extracted nybble -8 to +7

                                if (fi & 1) {
                                        factor = pPacket[pi];
                                        factor >>= 4;
                                } else {
                                        factor = pPacket[pi];
                                }

                                // Sign Extend
                                factor &= 0xF;
                                if (factor & 0x8) {
                                        factor |= 0xFFFFFFF0;
                                }

                                // Convert to Floating Point

                                float ffactor = factor;

                                float delta = ffactor * filter;

                                last_value += delta;

                                if (frame == frame_idx) {
                                        done = true;
                                        break;
                                }
                                frame++;
                        }

                        if (done)
                                break; // we're at the desired frame

                        pPacket += ((PACKET_SIZE * VectorLen) - 1); // skip to next packet
                }

                outdata[vi] = last_value;
        }
}

void AdaptiveDeltaMotionChannelClass::decompress(uint32_t src_idx, float *srcdata, uint32_t frame_idx, float *outdata)
{
        // Continue decompressing from src_idx, up to frame_idx

        // assert(src_idx < frame_idx) — release no-op
        src_idx++;

        float *base = (float *)Data.data(); // pointer to our true know beginning values
        base += VectorLen;               // skip header information

        bool done = false;

        for (int vi = 0; vi < VectorLen; vi++) {
                // Decompress all the vector indices, since they will probably all be needed
                unsigned char *pPacket = (unsigned char *)base; // pointer to current packet
                pPacket += PACKET_SIZE * vi;                    // skip to the appropriate packet start
                pPacket += (PACKET_SIZE * VectorLen) * ((src_idx - 1) >> 4); // skip out to current packet

                // initial filter index
                int fi = (src_idx - 1) & 0xF;

                float last_value = srcdata[vi];

                for (uint32_t frame = src_idx; frame <= frame_idx;) {
                        // Frame Loop
                        uint32_t filter_index = *pPacket; // packet header

                        pPacket++; // skip to nybble compressed data

                        float filter = filtertable[filter_index] * Scale; // decompression filter

                        // data is grouped in sets of 16 nybbles
                        for (; fi < 16; fi++) {

                                int pi = fi >> 1; // create packet index

                                int factor; // factor, contains extracted nybble -8 to +7

                                if (fi & 1) {
                                        factor = pPacket[pi];
                                        factor >>= 4;
                                } else {
                                        factor = pPacket[pi];
                                }

                                // Sign Extend
                                factor &= 0xF;
                                if (factor & 0x8) {
                                        factor |= 0xFFFFFFF0;
                                }

                                // Convert to Floating Point

                                float ffactor = factor;

                                float delta = ffactor * filter;

                                last_value += delta;

                                if (frame == frame_idx) {
                                        done = true;
                                        break;
                                }
                                frame++;
                        }
                        fi = 0;

                        if (done)
                                break; // we're at the desired frame

                        pPacket += ((PACKET_SIZE * VectorLen) - 1); // skip to next packet
                }

                outdata[vi] = last_value;
        }
}

float AdaptiveDeltaMotionChannelClass::getframe(uint32_t frame_idx, uint32_t vector_idx)
{
        // Make sure frame_idx is valid
        if (frame_idx >= NumFrames)
                frame_idx = NumFrames - 1;

        // Check to see if the data is already in cache?

        if (CacheFrame == frame_idx) {
                return (CacheData[vector_idx]);
        }

        if ((CacheFrame + 1) == frame_idx) {
                return (CacheData[vector_idx + VectorLen]);
        }

        if (frame_idx < CacheFrame) {
                // Requested Frame isn't cached, so cache it, and frame_idx+1, and return the decompressed data
                // from frame_idx

                decompress(frame_idx, &CacheData[0]);

                if (frame_idx != (NumFrames - 1)) {
                        decompress(frame_idx, &CacheData[0], frame_idx + 1, &CacheData[VectorLen]);
                }

                CacheFrame = frame_idx;

                return (CacheData[vector_idx]);
        }

        // Copy last known Cached data down

        if (frame_idx == (CacheFrame + 2)) {

                // Sliding window
                memcpy(&CacheData[0], &CacheData[VectorLen], VectorLen * sizeof(float));

                CacheFrame++;

                decompress(CacheFrame, &CacheData[0], frame_idx, &CacheData[VectorLen]);

                return (CacheData[VectorLen + vector_idx]);
        }

        // Else just use last known frame to decompress forwards

        // assert(VectorLen <= 4) — release no-op; the corpus keeps
        // VectorLen at 1 or 4 so the stack scratch below is in bounds
        float temp[4];

        memcpy(&temp[0], &CacheData[VectorLen], VectorLen * sizeof(float));

        // NOTE: the original passes srcdata = the CACHED ROW-1 values
        // (frame CacheFrame+1's data) with src_idx = CacheFrame — the
        // walk then re-applies the delta for CacheFrame+1. Transcribed
        // verbatim; c29's far-jump probes pin the off-by-one.
        decompress(CacheFrame, &temp[0], frame_idx, &CacheData[0]);
        CacheFrame = frame_idx;

        if (frame_idx != (NumFrames - 1)) {
                decompress(CacheFrame, &CacheData[0], frame_idx + 1, &CacheData[VectorLen]);
        }

        return (CacheData[vector_idx]);
}

void AdaptiveDeltaMotionChannelClass::Get_Vector(float32_t frame, float *setvec)
{
        uint32_t frame1 = frame;

        float ratio = frame - frame1;

        float value1 = getframe(frame1);
        float value2 = getframe(frame1 + 1);

        *setvec = WWLerp(value1, value2, ratio);
}

//
//  Special Case Quats, so we can use Slerp
//
void AdaptiveDeltaMotionChannelClass::Get_QuatVector(float32_t frame, float *q)
{
        uint32_t frame1 = frame;
        uint32_t frame2 = frame1 + 1;
        float ratio = frame - frame1;

        float q1[4];
        q1[0] = getframe(frame1, 0);
        q1[1] = getframe(frame1, 1);
        q1[2] = getframe(frame1, 2);
        q1[3] = getframe(frame1, 3);

        float q2[4];
        q2[0] = getframe(frame2, 0);
        q2[1] = getframe(frame2, 1);
        q2[2] = getframe(frame2, 2);
        q2[3] = getframe(frame2, 3);

        Fast_Slerp(q, q1, q2, ratio);
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

// --- the compressed animation (hcanim.cpp) --------------------------------------

struct NodeCompressedMotionStruct
{
        NodeCompressedMotionStruct() : Flavor(0), Vis(nullptr)
        {
                vd.X = nullptr;
                vd.Y = nullptr;
                vd.Z = nullptr;
                vd.Q = nullptr;
        }
        ~NodeCompressedMotionStruct()
        {
                // Needs to be changed to call the correct destructors

                switch (Flavor) {
                        case ANIM_FLAVOR_TIMECODED:
                                delete tc.X;
                                delete tc.Y;
                                delete tc.Z;
                                delete tc.Q;
                                break;
                        case ANIM_FLAVOR_ADAPTIVE_DELTA:
                                delete ad.X;
                                delete ad.Y;
                                delete ad.Z;
                                delete ad.Q;
                                break;
                        default:
                                // WWASSERT(0) — release no-op (unknown flavor)
                                break;
                }

                delete Vis;
        }

        void SetFlavor(int flavor) { Flavor = flavor; }

        int Flavor;

        union {
                struct {
                        TimeCodedMotionChannelClass *X;
                        TimeCodedMotionChannelClass *Y;
                        TimeCodedMotionChannelClass *Z;
                        TimeCodedMotionChannelClass *Q;
                } tc;

                struct {
                        AdaptiveDeltaMotionChannelClass *X;
                        AdaptiveDeltaMotionChannelClass *Y;
                        AdaptiveDeltaMotionChannelClass *Z;
                        AdaptiveDeltaMotionChannelClass *Q;
                } ad;

                struct {
                        void *X;
                        void *Y;
                        void *Z;
                        void *Q;
                } vd;
        };

        TimeCodedBitChannelClass *Vis;
};

class HCompressedAnimClass
{
public:
        enum
        {
                OK,
                LOAD_ERROR
        };

        HCompressedAnimClass() : NumFrames(0), NumNodes(0), Flavor(0), FrameRate(0), NodeMotion(nullptr)
        {
                memset(Name, 0, sizeof(Name));
                memset(HierarchyName, 0, sizeof(HierarchyName));
        }
        ~HCompressedAnimClass() { Free(); }

        int Load_W3D(ChunkLoadClass &cload);

        // the query surface (hanim.h / hcanim.cpp)
        const char *Get_Name() const { return Name; }
        const char *Get_HName() const { return HierarchyName; }
        int Get_Num_Frames() const { return NumFrames; }
        float Get_Frame_Rate() const { return FrameRate; }
        float Get_Total_Time() const { return (float)NumFrames / FrameRate; }
        int Get_Flavor() const { return Flavor; }
        int Get_Num_Pivots() const { return NumNodes; }

        void Get_Translation(float *trans /* 3+ floats */, int pividx, float frame) const;
        void Get_Orientation(float *q /* 4 floats */, int pividx, float frame) const;
        void Get_Transform(float *mtx /* 12+ floats */, int pividx, float frame) const;
        bool Get_Visibility(int pividx, float frame);

        bool Is_Node_Motion_Present(int pividx) const;
        bool Has_X_Translation(int pividx) const { return NodeMotion[pividx].vd.X != nullptr; }
        bool Has_Y_Translation(int pividx) const { return NodeMotion[pividx].vd.Y != nullptr; }
        bool Has_Z_Translation(int pividx) const { return NodeMotion[pividx].vd.Z != nullptr; }
        bool Has_Rotation(int pividx) const { return NodeMotion[pividx].vd.Q != nullptr; }
        bool Has_Visibility(int pividx) const { return NodeMotion[pividx].Vis != nullptr; }

        // the member order the strcpy overflow discipline depends on
        // (hcanim.h): Name[32], HierarchyName[16], NumFrames, NumNodes,
        // Flavor, FrameRate
        char Name[2 * W3D_NAME_LEN];
        char HierarchyName[W3D_NAME_LEN];
        int NumFrames;
        int NumNodes;
        int Flavor;
        float FrameRate;
        NodeCompressedMotionStruct *NodeMotion;
        W3dCompressedAnimHeaderStruct Header; // dump-side stash (documented deviation)

private:
        void Free();
        bool read_channel(ChunkLoadClass &cload, TimeCodedMotionChannelClass **newchan);
        bool read_channel(ChunkLoadClass &cload, AdaptiveDeltaMotionChannelClass **newchan);
        void add_channel(TimeCodedMotionChannelClass *newchan);
        void add_channel(AdaptiveDeltaMotionChannelClass *newchan);

        bool read_bit_channel(ChunkLoadClass &cload, TimeCodedBitChannelClass **newchan);
        void add_bit_channel(TimeCodedBitChannelClass *newchan);
};

void HCompressedAnimClass::Free()
{
        delete[] NodeMotion;
        NodeMotion = nullptr;
}

int HCompressedAnimClass::Load_W3D(ChunkLoadClass &cload)
{
        /*
        ** First make sure we release any memory in use
        */
        Free();

        /*
        **      Open the first chunk, it should be the animation header
        */
        if (!cload.Open_Chunk())
                return LOAD_ERROR;

        if (cload.Cur_Chunk_ID() != W3D_CHUNK_COMPRESSED_ANIMATION_HEADER) {
                // ERROR: Expected Animation Header!
                return LOAD_ERROR;
        }

        W3dCompressedAnimHeaderStruct aheader;
        // NOTE: the original reads sizeof(W3dAnimHeaderStruct) — the RAW
        // animation header's size — which is also 44, the compressed
        // struct's exact size (the static_assert pins it)
        if (cload.Read(&aheader, sizeof(W3dAnimHeaderStruct)) != sizeof(W3dAnimHeaderStruct)) {
                return LOAD_ERROR;
        }
        Header = aheader; // dump-side stash: the pristine decode

        cload.Close_Chunk();

        strcpy(Name, aheader.HierarchyName);
        sstrlcat(Name, ".", ARRAY_SIZE(Name));
        sstrlcat(Name, aheader.Name, ARRAY_SIZE(Name));

        // TSS chasing crash bug 05/26/99 — the pointer asserts are
        // compiled out in release (the array members are never null)
        strcpy(HierarchyName, aheader.HierarchyName);

        HTreeStub *base_pose = Get_HTree_Stub(HierarchyName);
        if (base_pose == nullptr) {
                return LOAD_ERROR;
        }
        NumNodes = base_pose->pivots;

        NumFrames = aheader.NumFrames;
        FrameRate = aheader.FrameRate; // uint16 -> float, numerically
        Flavor = aheader.Flavor;

        // Just for now
        // WWASSERT((Flavor == ANIM_FLAVOR_TIMECODED)||(Flavor == ANIM_FLAVOR_ADAPTIVE_DELTA));
        // — release no-op: an unknown flavor loads, reads no motion
        // channels (the switch below has no case for it) and answers the
        // translation probes with the zero vector it pre-assigns

        NodeMotion = new NodeCompressedMotionStruct[NumNodes];
        if (NodeMotion == nullptr) {
                return LOAD_ERROR;
        }

        // Initialize Flavor
        for (int i = 0; i < NumNodes; i++) {
                NodeMotion[i].SetFlavor(Flavor);
        }

        /*
        ** Now, read in all of the other chunks (motion channels).
        */
        while (cload.Open_Chunk()) {

                switch (cload.Cur_Chunk_ID()) {

                        case W3D_CHUNK_COMPRESSED_ANIMATION_CHANNEL:

                                switch (Flavor) {

                                        case ANIM_FLAVOR_TIMECODED: {
                                                TimeCodedMotionChannelClass *tc_chan = nullptr;
                                                if (!read_channel(cload, &tc_chan)) {
                                                        return LOAD_ERROR;
                                                }
                                                if (tc_chan->Get_Pivot() < (uint32_t)NumNodes) {
                                                        add_channel(tc_chan);
                                                } else {
                                                        // PWG 12-14-98: we have only allocated space for NumNode pivots.
                                                        // If we have an index that's equal or higher than NumNode we are
                                                        // gonna trash memory.  Boy will we trash memory.
                                                        // GTH 09-25-2000: print a warning and survive this error
                                                        delete tc_chan;
                                                        // WWDEBUG_SAY(("ERROR! animation %s indexes a bone not
                                                        // present in the model. Please re-export!",Name));
                                                }

                                                break;
                                        }

                                        case ANIM_FLAVOR_ADAPTIVE_DELTA: {
                                                AdaptiveDeltaMotionChannelClass *ad_chan = nullptr;
                                                if (!read_channel(cload, &ad_chan)) {
                                                        return LOAD_ERROR;
                                                }
                                                if (ad_chan->Get_Pivot() < (uint32_t)NumNodes) {
                                                        add_channel(ad_chan);
                                                } else {
                                                        delete ad_chan;
                                                }
                                                break;
                                        }

                                        default:
                                                // no case: the channel chunk is never read for an
                                                // unknown flavor (the bytes are skipped by Close_Chunk)
                                                break;
                                }
                                break;

                        case W3D_CHUNK_COMPRESSED_BIT_CHANNEL: {
                                TimeCodedBitChannelClass *newbitchan = nullptr;
                                if (!read_bit_channel(cload, &newbitchan)) {
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

bool HCompressedAnimClass::read_channel(ChunkLoadClass &cload, TimeCodedMotionChannelClass **newchan)
{
        TimeCodedMotionChannelClass *channel = new TimeCodedMotionChannelClass;
        if (channel->Load_W3D(cload)) {
                *newchan = channel;
                return true;
        } else {
                delete channel;
                return false;
        }
}

bool HCompressedAnimClass::read_channel(ChunkLoadClass &cload, AdaptiveDeltaMotionChannelClass **newchan)
{
        AdaptiveDeltaMotionChannelClass *channel = new AdaptiveDeltaMotionChannelClass;
        if (channel->Load_W3D(cload)) {
                *newchan = channel;
                return true;
        } else {
                delete channel;
                return false;
        }
}

void HCompressedAnimClass::add_channel(TimeCodedMotionChannelClass *newchan)
{
        int idx = newchan->Get_Pivot();

        switch (newchan->Get_Type()) {
                case ANIM_CHANNEL_X:
                        NodeMotion[idx].tc.X = newchan;
                        break;

                case ANIM_CHANNEL_Y:
                        NodeMotion[idx].tc.Y = newchan;
                        break;

                case ANIM_CHANNEL_Z:
                        NodeMotion[idx].tc.Z = newchan;
                        break;

                case ANIM_CHANNEL_Q:
                        NodeMotion[idx].tc.Q = newchan;
                        break;

                // NOTE: the TIMECODED_/ADAPTIVEDELTA_ channel-type values
                // (7..14) have no cases — such channels are stored nowhere
                // (the original also leaks them). Only the plain types
                // route, which is what the exporter writes.
        }
}

void HCompressedAnimClass::add_channel(AdaptiveDeltaMotionChannelClass *newchan)
{
        int idx = newchan->Get_Pivot();

        switch (newchan->Get_Type()) {
                case ANIM_CHANNEL_X:
                        NodeMotion[idx].ad.X = newchan;
                        break;

                case ANIM_CHANNEL_Y:
                        NodeMotion[idx].ad.Y = newchan;
                        break;

                case ANIM_CHANNEL_Z:
                        NodeMotion[idx].ad.Z = newchan;
                        break;

                case ANIM_CHANNEL_Q:
                        NodeMotion[idx].ad.Q = newchan;
                        break;
        }
}

bool HCompressedAnimClass::read_bit_channel(ChunkLoadClass &cload, TimeCodedBitChannelClass **newchan)
{
        TimeCodedBitChannelClass *channel = new TimeCodedBitChannelClass;
        if (channel->Load_W3D(cload)) {
                *newchan = channel;
                return true;
        } else {
                delete channel;
                return false;
        }
}

void HCompressedAnimClass::add_bit_channel(TimeCodedBitChannelClass *newchan)
{
        int idx = newchan->Get_Pivot();

        switch (newchan->Get_Type()) {
                case BIT_CHANNEL_VIS:
                        NodeMotion[idx].Vis = newchan;
                        break;
        }
}

void HCompressedAnimClass::Get_Translation(float *trans, int pividx, float frame) const
{
        struct NodeCompressedMotionStruct *motion = &NodeMotion[pividx];

        trans[0] = 0.0f;
        trans[1] = 0.0f;
        trans[2] = 0.0f;

        switch (Flavor) {
                case ANIM_FLAVOR_TIMECODED:
                        if (motion->tc.X)
                                motion->tc.X->Get_Vector(frame, &(trans[0]));
                        if (motion->tc.Y)
                                motion->tc.Y->Get_Vector(frame, &(trans[1]));
                        if (motion->tc.Z)
                                motion->tc.Z->Get_Vector(frame, &(trans[2]));
                        break;
                case ANIM_FLAVOR_ADAPTIVE_DELTA:
                        if (motion->ad.X)
                                motion->ad.X->Get_Vector(frame, &(trans[0]));
                        if (motion->ad.Y)
                                motion->ad.Y->Get_Vector(frame, &(trans[1]));
                        if (motion->ad.Z)
                                motion->ad.Z->Get_Vector(frame, &(trans[2]));
                        break;
                default:
                        // WWASSERT(0) — release no-op: the pre-assigned zero
                        // vector stands
                        break;
        }
}

void HCompressedAnimClass::Get_Orientation(float *q, int pividx, float frame) const
{
        switch (Flavor) {
                case ANIM_FLAVOR_TIMECODED:
                        if (NodeMotion[pividx].tc.Q)
                                NodeMotion[pividx].tc.Q->Get_QuatVector(frame, q);
                        else {
                                q[0] = 0.0f;
                                q[1] = 0.0f;
                                q[2] = 0.0f;
                                q[3] = 1.0f;
                        }
                        break;
                case ANIM_FLAVOR_ADAPTIVE_DELTA:
                        if (NodeMotion[pividx].ad.Q)
                                NodeMotion[pividx].ad.Q->Get_QuatVector(frame, q);
                        else {
                                q[0] = 0.0f;
                                q[1] = 0.0f;
                                q[2] = 0.0f;
                                q[3] = 1.0f;
                        }
                        break;
                default:
                        // WWASSERT(0) — release no-op: the caller's
                        // destination is left UNTOUCHED (not a function of
                        // the animation; the dump omits the probe)
                        break;
        }
}

void HCompressedAnimClass::Get_Transform(float *mtx, int pividx, float frame) const
{
        struct NodeCompressedMotionStruct *motion = &NodeMotion[pividx];

        switch (Flavor) {
                case ANIM_FLAVOR_TIMECODED:
                        if (NodeMotion[pividx].tc.Q) {
                                float q[4];
                                NodeMotion[pividx].tc.Q->Get_QuatVector(frame, q);
                                Build_Matrix3D(q, mtx);
                        } else {
                                mtx[0] = 1.0f;
                                mtx[1] = 0.0f;
                                mtx[2] = 0.0f;
                                mtx[3] = 0.0f;
                                mtx[4] = 0.0f;
                                mtx[5] = 1.0f;
                                mtx[6] = 0.0f;
                                mtx[7] = 0.0f;
                                mtx[8] = 0.0f;
                                mtx[9] = 0.0f;
                                mtx[10] = 1.0f;
                                mtx[11] = 0.0f;
                        }
                        if (motion->tc.X)
                                motion->tc.X->Get_Vector(frame, &(mtx[0 * 4 + 3]));
                        if (motion->tc.Y)
                                motion->tc.Y->Get_Vector(frame, &(mtx[1 * 4 + 3]));
                        if (motion->tc.Z)
                                motion->tc.Z->Get_Vector(frame, &(mtx[2 * 4 + 3]));
                        break;
                case ANIM_FLAVOR_ADAPTIVE_DELTA:
                        if (NodeMotion[pividx].ad.Q) {
                                float q[4];
                                NodeMotion[pividx].ad.Q->Get_QuatVector(frame, q);
                                Build_Matrix3D(q, mtx);
                        } else {
                                mtx[0] = 1.0f;
                                mtx[1] = 0.0f;
                                mtx[2] = 0.0f;
                                mtx[3] = 0.0f;
                                mtx[4] = 0.0f;
                                mtx[5] = 1.0f;
                                mtx[6] = 0.0f;
                                mtx[7] = 0.0f;
                                mtx[8] = 0.0f;
                                mtx[9] = 0.0f;
                                mtx[10] = 1.0f;
                                mtx[11] = 0.0f;
                        }

                        if (motion->ad.X)
                                motion->ad.X->Get_Vector(frame, &(mtx[0 * 4 + 3]));
                        if (motion->ad.Y)
                                motion->ad.Y->Get_Vector(frame, &(mtx[1 * 4 + 3]));
                        if (motion->ad.Z)
                                motion->ad.Z->Get_Vector(frame, &(mtx[2 * 4 + 3]));
                        break;
                default:
                        // WWASSERT(0) — release no-op: the caller's
                        // destination is left UNTOUCHED (the dump omits
                        // the probe)
                        break;
        }
}

bool HCompressedAnimClass::Get_Visibility(int pividx, float frame)
{
        if (NodeMotion[pividx].Vis != nullptr) {
                return (NodeMotion[pividx].Vis->Get_Bit((int)frame) == 1);
        }

        // default to always visible...
        return 1;
}

bool HCompressedAnimClass::Is_Node_Motion_Present(int pividx) const
{
        // WWASSERT((pividx >= 0) && (pividx < NumNodes)) — release no-op

        if (NodeMotion[pividx].vd.X != nullptr)
                return true;
        if (NodeMotion[pividx].vd.Y != nullptr)
                return true;
        if (NodeMotion[pividx].vd.Z != nullptr)
                return true;
        if (NodeMotion[pividx].vd.Q != nullptr)
                return true;
        if (NodeMotion[pividx].Vis != nullptr)
                return true;

        return false;
}

// --- the corpus builder ---------------------------------------------------------

// Assembles well-formed chunk trees (the byte framing the writer produces:
// 8-byte headers, sizes patched on end). Hand-malformed corpora patch or
// truncate the output directly.
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

static void put_u16(uint8_t *p, uint16_t v)
{
        p[0] = (uint8_t)(v & 0xFF);
        p[1] = (uint8_t)((v >> 8) & 0xFF);
}

// the 44-byte compressed animation header (the 16-byte name fields keep
// strncpy semantics: NUL-padded, NOT terminated when the source fills
// all 16 bytes — c24/c25 depend on that)
static void w_compressed_anim_header(ChunkBuild &b, uint32_t version, const char *name, const char *hname,
                                      uint32_t num_frames, uint16_t frame_rate, uint16_t flavor)
{
        b.begin(W3D_CHUNK_COMPRESSED_ANIMATION_HEADER);
        b.w_u32(version);
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
        uint8_t fr[2] = {(uint8_t)(frame_rate & 0xFF), (uint8_t)((frame_rate >> 8) & 0xFF)};
        b.w_bytes(fr, 2);
        uint8_t fl[2] = {(uint8_t)(flavor & 0xFF), (uint8_t)((flavor >> 8) & 0xFF)};
        b.w_bytes(fl, 2);
        b.end();
}

// one time-coded motion channel: the 8 fixed field bytes + every packet
// word (the first doubles as the struct's Data[0]). `packets` holds the
// raw words exactly as stored: one time-code word (with the binary
// movement flag where wanted) then VectorLen value words, per packet.
static void w_tc_motion_channel(ChunkBuild &b, uint32_t num_time_codes, int pivot, int vector_len, int flags,
                                 const std::vector<uint32_t> &words)
{
        b.begin(W3D_CHUNK_COMPRESSED_ANIMATION_CHANNEL);
        b.w_u32(num_time_codes);
        uint8_t f[4];
        put_u16(f + 0, (uint16_t)pivot);
        f[2] = (uint8_t)vector_len;
        f[3] = (uint8_t)flags;
        b.w_bytes(f, 4);
        for (uint32_t w : words) {
                b.w_u32(w);
        }
        b.end();
}

// one time-coded bit channel: the 8 fixed field bytes + every time-code
// word (the first doubles as the struct's Data[0])
static void w_tc_bit_channel(ChunkBuild &b, uint32_t num_time_codes, int pivot, int flags, int default_val,
                              const std::vector<uint32_t> &words)
{
        b.begin(W3D_CHUNK_COMPRESSED_BIT_CHANNEL);
        b.w_u32(num_time_codes);
        uint8_t f[4];
        put_u16(f + 0, (uint16_t)pivot);
        f[2] = (uint8_t)flags;
        f[3] = (uint8_t)default_val;
        b.w_bytes(f, 4);
        for (uint32_t w : words) {
                b.w_u32(w);
        }
        b.end();
}

// one adaptive-delta motion channel: the 12 fixed field bytes + the data
// stream. The stream is `base` (VectorLen floats) followed by
// `blocks` groups of VectorLen 9-byte packets (filter index + 16
// nybbles), padded to a whole uint32_t word exactly like the exporter (the
// pad bytes land in the loaded array's tail; the walk never reads them).
// The first base float doubles as the struct's Data[0].
static void w_ad_motion_channel(ChunkBuild &b, uint32_t num_frames, int pivot, int vector_len, int flags,
                                 float scale, const std::vector<float> &base,
                                 const std::vector<uint8_t> &packets)
{
        b.begin(W3D_CHUNK_COMPRESSED_ANIMATION_CHANNEL);
        b.w_u32(num_frames);
        uint8_t f[4];
        put_u16(f + 0, (uint16_t)pivot);
        f[2] = (uint8_t)vector_len;
        f[3] = (uint8_t)flags;
        b.w_bytes(f, 4);
        b.w_f32(scale);
        std::vector<uint8_t> data;
        for (float v : base) {
                uint32_t bits;
                memcpy(&bits, &v, 4);
                uint8_t w[4];
                ChunkBuild::put_u32(w, bits);
                data.insert(data.end(), w, w + 4);
        }
        data.insert(data.end(), packets.begin(), packets.end());
        while (data.size() % 4 != 0) {
                data.push_back(0xAB); // the exporter's word alignment pad
        }
        b.w_bytes(data.data(), data.size());
        b.end();
}

// deterministic AD packet bytes for block `block`, vector index `vi`:
// a filter index byte followed by 8 nybble bytes
static void ad_packet(int vi, int block, uint8_t out[9])
{
        out[0] = (uint8_t)((vi * 37 + block * 11) & 0xFF); // filter index
        for (int k = 0; k < 8; k++) {
                out[1 + k] = (uint8_t)((vi * 53 + block * 29 + k * 17) & 0xFF);
        }
}

// float-to-word helper for the corpus builders (little-endian bit cast)
static uint32_t f2w(float v)
{
        uint32_t bits;
        memcpy(&bits, &v, 4);
        return bits;
}

// the packet stream for a VectorLen x blocks adaptive-delta channel
static std::vector<uint8_t> ad_packets(int vector_len, int blocks)
{
        std::vector<uint8_t> out;
        for (int b = 0; b < blocks; b++) {
                for (int vi = 0; vi < vector_len; vi++) {
                        uint8_t p[9];
                        ad_packet(vi, b, p);
                        out.insert(out.end(), p, p + 9);
                }
        }
        return out;
}

// raw little-endian bytes of a word list (for corpora whose packet
// regions are crafted as whole words)
static std::vector<uint8_t> words_bytes(const std::vector<uint32_t> &words)
{
        std::vector<uint8_t> out;
        for (uint32_t w : words) {
                uint8_t b[4];
                ChunkBuild::put_u32(b, w);
                out.insert(out.end(), b, b + 4);
        }
        return out;
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
        const float s45 = 0.70710678118654752440f;
        const uint32_t FLAG = W3D_TIMECODED_BINARY_MOVEMENT_FLAG;

        // c01: the modern full time-coded animation — every channel kind
        // routed, interpolated time codes, a binary-movement packet, a vis
        // bit channel and an euler channel that never routes (only
        // ANIM_CHANNEL_X/Y/Z/Q have add_channel cases).
        {
                ChunkBuild b;
                w_compressed_anim_header(b, V40, "Run", "tank", 8, 30, ANIM_FLAVOR_TIMECODED);
                w_tc_motion_channel(b, 4, 1, 1, ANIM_CHANNEL_X,
                                    {0, f2w(0.5f), 2, f2w(1.5f), 4, f2w(2.5f), 6, f2w(3.5f)});
                w_tc_motion_channel(b, 4, 1, 1, ANIM_CHANNEL_Y,
                                    {0, f2w(1.0f), 2, f2w(1.0f), 4, f2w(1.0f), 6, f2w(1.0f)});
                w_tc_motion_channel(b, 4, 1, 1, ANIM_CHANNEL_Z,
                                    {0, f2w(-0.5f), 2, f2w(-1.5f), 4 | FLAG, f2w(-2.5f), 6 | FLAG, f2w(-3.5f)});
                w_tc_motion_channel(b, 2, 2, 4, ANIM_CHANNEL_Q,
                                    {0, f2w(0.5f), f2w(0.5f), f2w(0.5f), f2w(0.5f),
                                     3, f2w(s45), f2w(0.0f), f2w(0.0f), f2w(s45)});
                w_tc_bit_channel(b, 4, 3, BIT_CHANNEL_VIS, 1,
                                 {0, 4 | (1u << 31), 8, 12 | (1u << 31)});
                w_tc_motion_channel(b, 2, 4, 1, ANIM_CHANNEL_XR, {0, f2w(0.25f), 2, f2w(0.75f)});
                push(out, "c01", b);
        }

        // c02: the binary-movement branch — the flag on the FOLLOWING
        // packet returns the current packet verbatim.
        {
                ChunkBuild b;
                w_compressed_anim_header(b, V40, "Bin", "t", 8, 30, ANIM_FLAVOR_TIMECODED);
                w_tc_motion_channel(b, 3, 0, 1, ANIM_CHANNEL_X,
                                    {0, f2w(1.0f), 3 | FLAG, f2w(9.0f), 6, f2w(2.0f)});
                push(out, "c02", b);
        }

        // c03: the last-packet branch — probes at or past the last time
        // code return the final packet's values.
        {
                ChunkBuild b;
                w_compressed_anim_header(b, V40, "Last", "t", 8, 30, ANIM_FLAVOR_TIMECODED);
                w_tc_motion_channel(b, 3, 0, 1, ANIM_CHANNEL_X,
                                    {0, f2w(1.0f), 2, f2w(2.0f), 4, f2w(4.0f)});
                push(out, "c03", b);
        }

        // c04: a single time code (time code 0, so every probe's special
        // case hits the last-packet branch).
        {
                ChunkBuild b;
                w_compressed_anim_header(b, V40, "One", "t", 4, 30, ANIM_FLAVOR_TIMECODED);
                w_tc_motion_channel(b, 1, 0, 1, ANIM_CHANNEL_X, {0, f2w(7.0f)});
                w_tc_bit_channel(b, 1, 0, BIT_CHANNEL_VIS, 0, {1u << 31});
                push(out, "c04", b);
        }

        // c05: the binary search and its rewind — time codes spaced out so
        // the forward probes search, and the 7.75-then-3.0 probe order
        // rewinds the cache below the cached packet's time code.
        {
                ChunkBuild b;
                w_compressed_anim_header(b, V40, "Seek", "t", 4, 30, ANIM_FLAVOR_TIMECODED);
                w_tc_motion_channel(b, 4, 0, 1, ANIM_CHANNEL_X,
                                    {0, f2w(0.0f), 2, f2w(2.0f), 4, f2w(4.0f), 8, f2w(8.0f)});
                push(out, "c05", b);
        }

        // c06: the cache look-ahead — a packet at every frame so the
        // consecutive probes take the one-time +PacketSize look-ahead.
        {
                ChunkBuild b;
                w_compressed_anim_header(b, V40, "Ahead", "t", 8, 30, ANIM_FLAVOR_TIMECODED);
                std::vector<uint32_t> w;
                for (uint32_t f = 0; f < 8; f++) {
                        w.push_back(f);
                        w.push_back(f2w((float)f * 0.25f + 1.0f));
                }
                w_tc_motion_channel(b, 8, 0, 1, ANIM_CHANNEL_X, w);
                push(out, "c06", b);
        }

        // c07: a VectorLen-2 X channel — Get_Vector writes two floats, so
        // trans[1] receives the channel's second word (no Y channel to
        // overwrite it) and the transform's m[4] is clobbered.
        {
                ChunkBuild b;
                w_compressed_anim_header(b, V40, "Wide", "t", 8, 30, ANIM_FLAVOR_TIMECODED);
                w_tc_motion_channel(b, 2, 0, 2, ANIM_CHANNEL_X,
                                    {0, f2w(1.0f), f2w(0.5f), 4, f2w(3.0f), f2w(1.5f)});
                push(out, "c07", b);
        }

        // c08: the slerp corpus — quaternion keys a quadrant apart, so the
        // fractional probes exercise the table-based Fast_Slerp path.
        {
                ChunkBuild b;
                w_compressed_anim_header(b, V40, "Slerp", "t", 4, 30, ANIM_FLAVOR_TIMECODED);
                w_tc_motion_channel(b, 2, 0, 4, ANIM_CHANNEL_Q,
                                    {0, f2w(s45), f2w(0.0f), f2w(0.0f), f2w(s45),
                                     3, f2w(0.0f), f2w(s45), f2w(0.0f), f2w(s45)});
                push(out, "c08", b);
        }

        // c09: near-parallel keys — the identical-quaternion case takes the
        // linear branch (1 - cos_t < EPSILON^2).
        {
                ChunkBuild b;
                w_compressed_anim_header(b, V40, "Lin", "t", 4, 30, ANIM_FLAVOR_TIMECODED);
                w_tc_motion_channel(b, 2, 0, 4, ANIM_CHANNEL_Q,
                                    {0, f2w(0.5f), f2w(0.5f), f2w(0.5f), f2w(0.5f),
                                     2, f2w(0.5f), f2w(0.5f), f2w(0.5f), f2w(0.5f)});
                push(out, "c09", b);
        }

        // c10: the hemisphere flip — antipodal keys (cos_t < 0).
        {
                ChunkBuild b;
                w_compressed_anim_header(b, V40, "Flip", "t", 4, 30, ANIM_FLAVOR_TIMECODED);
                w_tc_motion_channel(b, 2, 0, 4, ANIM_CHANNEL_Q,
                                    {0, f2w(0.5f), f2w(0.5f), f2w(0.5f), f2w(0.5f),
                                     2, f2w(-0.5f), f2w(-0.5f), f2w(-0.5f), f2w(-0.5f)});
                push(out, "c10", b);
        }

        // c11: the Fast_Acos fallback — keys a few degrees apart (the dot
        // product lands in (0.975, 1), where the exact gm_acosf runs).
        {
                ChunkBuild b;
                w_compressed_anim_header(b, V40, "Near", "t", 4, 30, ANIM_FLAVOR_TIMECODED);
                w_tc_motion_channel(b, 2, 0, 4, ANIM_CHANNEL_Q,
                                    {0, f2w(0.0f), f2w(0.0f), f2w(0.0f), f2w(1.0f),
                                     2, f2w(0.03125f), f2w(0.0f), f2w(0.0f), f2w(0.9995f)});
                push(out, "c11", b);
        }

        // c12: the TIMECODED_X..Q channel-type values (7..10) never route —
        // add_channel's switch only has the plain types. One plain-type
        // channel proves the routing still works alongside them.
        {
                ChunkBuild b;
                w_compressed_anim_header(b, V40, "Types", "t", 4, 30, ANIM_FLAVOR_TIMECODED);
                for (int ty = ANIM_CHANNEL_TIMECODED_X; ty <= ANIM_CHANNEL_TIMECODED_Q; ty++) {
                        w_tc_motion_channel(b, 2, 0, 1, ty, {0, f2w(1.0f), 2, f2w(2.0f)});
                }
                w_tc_motion_channel(b, 2, 0, 1, ANIM_CHANNEL_Y, {0, f2w(5.0f), 2, f2w(6.0f)});
                push(out, "c12", b);
        }

        // c13: duplicate X channels — the newest overwrites the previous
        // slot (the original leaks the older channel).
        {
                ChunkBuild b;
                w_compressed_anim_header(b, V40, "Dup", "t", 4, 30, ANIM_FLAVOR_TIMECODED);
                w_tc_motion_channel(b, 2, 0, 1, ANIM_CHANNEL_X, {0, f2w(1.0f), 2, f2w(2.0f)});
                w_tc_motion_channel(b, 2, 0, 1, ANIM_CHANNEL_X, {0, f2w(9.0f), 2, f2w(8.0f)});
                push(out, "c13", b);
        }

        // c14: channels for pivots outside the tree — discarded, the load
        // continues (the "Please re-export" path).
        {
                ChunkBuild b;
                w_compressed_anim_header(b, V40, "Beyond", "rig", 4, 30, ANIM_FLAVOR_TIMECODED);
                w_tc_motion_channel(b, 2, 5, 1, ANIM_CHANNEL_X, {0, f2w(1.0f), 2, f2w(2.0f)});
                w_tc_bit_channel(b, 2, 9, BIT_CHANNEL_VIS, 1, {0, 1u << 31});
                w_tc_motion_channel(b, 2, 1, 1, ANIM_CHANNEL_Y, {0, f2w(3.0f), 2, f2w(4.0f)});
                push(out, "c14", b);
        }

        // c15: unknown top-level chunk and an unknown bit-channel type (7)
        // — neither routes; the load continues.
        {
                ChunkBuild b;
                w_compressed_anim_header(b, V40, "Odd", "t", 4, 30, ANIM_FLAVOR_TIMECODED);
                w_tc_bit_channel(b, 2, 0, 7, 0, {0, 0});
                b.begin(0x0999);
                b.w_u32(0xDEADBEEF);
                b.end();
                push(out, "c15", b);
        }

        // c16: a file truncated inside the channel chunk — the declared
        // chunk size covers the packets but the file ends four bytes early,
        // so the data read hits the file end and fails (the chunk-level
        // clamp cannot fail this reader: it sizes the read from the
        // declared chunk length itself).
        {
                ChunkBuild b;
                w_compressed_anim_header(b, V40, "Cut", "t", 8, 30, ANIM_FLAVOR_TIMECODED);
                w_tc_motion_channel(b, 2, 0, 1, ANIM_CHANNEL_X, {0, f2w(1.0f), 2, f2w(2.0f)});
                Corpus c;
                c.id = "c16";
                c.bytes.assign(b.bytes.begin(), b.bytes.end() - 4);
                out.push_back(c);
        }

        // c17: the first chunk is not the compressed animation header.
        {
                ChunkBuild b;
                b.begin(W3D_CHUNK_COMPRESSED_ANIMATION_CHANNEL);
                b.w_u32(1);
                uint8_t f[4];
                put_u16(f + 0, 0);
                f[2] = 1;
                f[3] = ANIM_CHANNEL_X;
                b.w_bytes(f, 4);
                b.w_u32(0);
                b.w_u32(f2w(1.0f));
                b.end();
                push(out, "c17", b);
        }

        // c18: no tree with that name.
        {
                ChunkBuild b;
                w_compressed_anim_header(b, V40, "Nowhere", "nowhere", 4, 30, ANIM_FLAVOR_TIMECODED);
                push(out, "c18", b);
        }

        // c19: empty payload.
        push(out, "c19", ChunkBuild());

        // c20: a header chunk truncated by the file end.
        {
                ChunkBuild b;
                w_compressed_anim_header(b, V40, "Cut", "t", 4, 30, ANIM_FLAVOR_TIMECODED);
                Corpus c;
                c.id = "c20";
                c.bytes.assign(b.bytes.begin(), b.bytes.begin() + 28); // 8 + 20 bytes
                out.push_back(c);
        }

        // c21: an unknown flavor (2) — the loader's switch reads no motion
        // channels at all, the bit channel still routes, the translation
        // probes answer the pre-assigned zero vector, and the dump omits
        // the orientation/transform probes (the original leaves the
        // caller's destinations untouched for unknown flavors).
        {
                ChunkBuild b;
                w_compressed_anim_header(b, V40, "Mystery", "t", 4, 30, 2);
                w_tc_motion_channel(b, 2, 0, 1, ANIM_CHANNEL_X, {0, f2w(1.0f), 2, f2w(2.0f)});
                w_tc_bit_channel(b, 2, 1, BIT_CHANNEL_VIS, 0, {0, 1u << 31});
                push(out, "c21", b);
        }

        // c22: big time codes — 0x01000001 (2^24+1) converts to float as
        // 2^24 exactly, so the interpolated ratio is computed against the
        // ROUNDED time codes (the numeric u32->f32 conversion quirk).
        // The middle packet carries the binary-movement flag so the small
        // probes take the verbatim branch.
        {
                ChunkBuild b;
                w_compressed_anim_header(b, V40, "Big", "t", 0x01000010, 30, ANIM_FLAVOR_TIMECODED);
                w_tc_motion_channel(b, 3, 0, 1, ANIM_CHANNEL_X,
                                    {0, f2w(1.0f),
                                     0x01000001 | FLAG, f2w(2.0f),
                                     0x01000008, f2w(3.0f)});
                push(out, "c22", b);
        }

        // c23: flavor TIMECODED over an adaptive-delta-shaped payload —
        // the loader picks the reader from the header's flavor, not the
        // channel bytes, so the AD struct's Scale word is just the first
        // time code to the time-coded reader. The bytes are crafted to
        // stay a well-formed time-coded channel (a genuinely inconsistent
        // payload — a declared count the data cannot cover — reads past
        // the array in the original and is corpus-disciplined away): the
        // +0.0f Scale word doubles as time code 0 and the AD "base float
        // plus packet bytes" region doubles as the remaining time codes
        // and values.
        {
                ChunkBuild b;
                w_compressed_anim_header(b, V40, "Mismatch", "t", 8, 30, ANIM_FLAVOR_TIMECODED);
                w_ad_motion_channel(b, 3, 0, 1, ANIM_CHANNEL_X, 0.0f,
                                    {1.0f},
                                    words_bytes({2, f2w(2.0f), 4, f2w(3.0f)}));
                push(out, "c23", b);
        }

        // c24: the unterminated hierarchy field — strcpy runs through the
        // NumFrames word (5), producing the 17-byte tree key that the
        // table registers (the lowercase 'h's plus the 0x05).
        {
                ChunkBuild b;
                char h[16];
                memset(h, 'H', sizeof(h));
                b.begin(W3D_CHUNK_COMPRESSED_ANIMATION_HEADER);
                b.w_u32(V40);
                char n[16];
                memset(n, 0, sizeof(n));
                memcpy(n, "A", 1);
                b.w_bytes(n, 16);
                b.w_bytes(h, 16);
                b.w_u32(5);
                uint8_t fr[2] = {30, 0};
                b.w_bytes(fr, 2);
                uint8_t fl[2] = {ANIM_FLAVOR_TIMECODED, 0};
                b.w_bytes(fl, 2);
                b.end();
                push(out, "c24", b);
        }

        // c25: the unterminated animation name — the strlcat source runs
        // through the hierarchy field until its NUL.
        {
                ChunkBuild b;
                b.begin(W3D_CHUNK_COMPRESSED_ANIMATION_HEADER);
                b.w_u32(V40);
                char n[16];
                memset(n, 'N', sizeof(n)); // no NUL in the name field
                b.w_bytes(n, 16);
                char h[16];
                memset(h, 0, sizeof(h));
                memcpy(h, "t", 1); // the 't' then NUL stops the source
                b.w_bytes(h, 16);
                b.w_u32(4);
                uint8_t fr[2] = {30, 0};
                b.w_bytes(fr, 2);
                uint8_t fl[2] = {ANIM_FLAVOR_TIMECODED, 0};
                b.w_bytes(fl, 2);
                b.end();
                push(out, "c25", b);
        }

        // c26: the 31-character strlcat truncation — a 15-byte hierarchy
        // name plus the dot plus a 16-byte animation name does not fit
        // the 32-byte buffer.
        {
                ChunkBuild b;
                w_compressed_anim_header(b, V40, "0123456789ABCDEF", "tanktree1234567", 4, 30, ANIM_FLAVOR_TIMECODED);
                push(out, "c26", b);
        }

        // c27: the empty animation name keeps the trailing dot; FrameRate
        // 0 makes the total time +inf; NumFrames 0x80000000 reads negative
        // as the class's int (a negative total time, and every count-derived
        // probe is skipped).
        {
                ChunkBuild b;
                w_compressed_anim_header(b, V40, "", "t", 0x80000000u, 0, ANIM_FLAVOR_TIMECODED);
                push(out, "c27", b);
        }

        // c28: the numeric frame-rate conversion — 0xFFFF converts to
        // 65535.0f exactly; the total time divides by it.
        {
                ChunkBuild b;
                w_compressed_anim_header(b, V40, "Rate", "t", 100, 0xFFFF, ANIM_FLAVOR_TIMECODED);
                w_tc_motion_channel(b, 2, 0, 1, ANIM_CHANNEL_X, {0, f2w(1.0f), 50, f2w(2.0f)});
                push(out, "c28", b);
        }

        // c29: the modern adaptive-delta animation — 40 frames (three
        // packet blocks) with an X and a Q channel; the probe order runs
        // the first full decompress, the row hits, the sliding window and
        // the far jumps that take the cache-continue branch (with the
        // original's off-by-one, transcribed verbatim and pinned here).
        {
                ChunkBuild b;
                w_compressed_anim_header(b, V40, "Run", "tank", 40, 30, ANIM_FLAVOR_ADAPTIVE_DELTA);
                w_ad_motion_channel(b, 40, 1, 1, ANIM_CHANNEL_X, 0.01f, {0.5f}, ad_packets(1, 3));
                w_ad_motion_channel(b, 40, 2, 4, ANIM_CHANNEL_Q, 0.001f,
                                    {0.0f, 0.0f, 0.0f, 1.0f}, ad_packets(4, 3));
                w_tc_bit_channel(b, 2, 3, BIT_CHANNEL_VIS, 1, {0, 20 | (1u << 31)});
                push(out, "c29", b);
        }

        // c30: a single-block adaptive-delta channel (frames within 16).
        {
                ChunkBuild b;
                w_compressed_anim_header(b, V40, "Block", "t", 9, 30, ANIM_FLAVOR_ADAPTIVE_DELTA);
                w_ad_motion_channel(b, 9, 0, 1, ANIM_CHANNEL_X, 0.05f, {1.0f}, ad_packets(1, 1));
                push(out, "c30", b);
        }

        // c31: NumFrames 1 — every getframe clamps to frame 0 (the base
        // values), so the f/f+1 pair returns the same row.
        {
                ChunkBuild b;
                w_compressed_anim_header(b, V40, "One", "t", 1, 30, ANIM_FLAVOR_ADAPTIVE_DELTA);
                w_ad_motion_channel(b, 1, 0, 1, ANIM_CHANNEL_X, 0.01f, {2.0f}, {});
                push(out, "c31", b);
        }

        // c32: probes at or past NumFrames — getframe clamps them to the
        // last frame.
        {
                ChunkBuild b;
                w_compressed_anim_header(b, V40, "Clamp", "t", 5, 30, ANIM_FLAVOR_ADAPTIVE_DELTA);
                w_ad_motion_channel(b, 5, 0, 1, ANIM_CHANNEL_X, 0.02f, {0.25f}, ad_packets(1, 1));
                push(out, "c32", b);
        }

        // c33: the filter-table ranges — packet filter indexes 0 and 15
        // (the literal table half) and 16, 17, 128, 255 (the generated
        // half), each of the six blocks carrying its own index. 97 frames
        // cover exactly six blocks (frames 1..96).
        {
                ChunkBuild b;
                w_compressed_anim_header(b, V40, "Filter", "t", 97, 30, ANIM_FLAVOR_ADAPTIVE_DELTA);
                std::vector<uint8_t> packets;
                const int filters[6] = {0, 15, 16, 17, 128, 255};
                for (int b2 = 0; b2 < 6; b2++) {
                        uint8_t p[9];
                        p[0] = (uint8_t)filters[b2];
                        for (int k = 0; k < 8; k++) {
                                p[1 + k] = (uint8_t)(0x11 * (k + 1) + b2);
                        }
                        packets.insert(packets.end(), p, p + 9);
                }
                w_ad_motion_channel(b, 97, 0, 1, ANIM_CHANNEL_X, 0.01f, {0.5f}, packets);
                push(out, "c33", b);
        }

        // c34: negative deltas — the nybble bytes all carry high nybbles
        // 8..F (the sign-extension branch, factors -8..-1).
        {
                ChunkBuild b;
                w_compressed_anim_header(b, V40, "Neg", "t", 40, 30, ANIM_FLAVOR_ADAPTIVE_DELTA);
                std::vector<uint8_t> packets;
                for (int b2 = 0; b2 < 3; b2++) {
                        uint8_t p[9];
                        p[0] = (uint8_t)(8 + b2); // filter indexes near 1.0
                        for (int k = 0; k < 8; k++) {
                                p[1 + k] = (uint8_t)(0xEE + k); // nybbles E..F and beyond
                        }
                        packets.insert(packets.end(), p, p + 9);
                }
                w_ad_motion_channel(b, 40, 0, 1, ANIM_CHANNEL_X, 0.01f, {10.0f}, packets);
                push(out, "c34", b);
        }

        // c35: a VectorLen-4 X channel — the adaptive-delta Get_Vector
        // writes only the first component (the scalar write), so the
        // extra components never reach the caller.
        {
                ChunkBuild b;
                w_compressed_anim_header(b, V40, "Quad", "t", 20, 30, ANIM_FLAVOR_ADAPTIVE_DELTA);
                w_ad_motion_channel(b, 20, 0, 4, ANIM_CHANNEL_X, 0.01f,
                                    {0.1f, 0.2f, 0.3f, 0.4f}, ad_packets(4, 2));
                push(out, "c35", b);
        }

        // c36: the adaptive-delta quaternion channel — Get_QuatVector's
        // eight interleaved getframe calls walk the cache through the
        // slerp inputs.
        {
                ChunkBuild b;
                w_compressed_anim_header(b, V40, "Quat", "t", 24, 30, ANIM_FLAVOR_ADAPTIVE_DELTA);
                w_ad_motion_channel(b, 24, 0, 4, ANIM_CHANNEL_Q, 0.001f,
                                    {0.5f, 0.5f, 0.5f, 0.5f}, ad_packets(4, 2));
                push(out, "c36", b);
        }

        // c37: a channel chunk smaller than the 16-byte adaptive-delta
        // struct — the struct read clamps at the chunk boundary and fails,
        // the load fails.
        {
                ChunkBuild b;
                w_compressed_anim_header(b, V40, "Tiny", "t", 8, 30, ANIM_FLAVOR_ADAPTIVE_DELTA);
                b.begin(W3D_CHUNK_COMPRESSED_ANIMATION_CHANNEL);
                b.w_u32(8);
                uint8_t f[4];
                put_u16(f + 0, 0);
                f[2] = 1;
                f[3] = ANIM_CHANNEL_X;
                b.w_bytes(f, 4);
                b.w_f32(0.01f); // 12 payload bytes: the struct wants 16
                b.end();
                push(out, "c37", b);
        }

        // c38: flavor ADAPTIVE_DELTA over a time-coded-shaped payload —
        // the adaptive-delta reader consumes the time-coded channel's
        // words as its own data stream (the reader follows the header's
        // flavor). The bytes are crafted to keep the adaptive-delta walk
        // inside the array: the "Scale" word is the time-coded reader's
        // first time code (never read at this flavor), and the remaining
        // time-code/value words become the base float and packet bytes.
        {
                ChunkBuild b;
                w_compressed_anim_header(b, V40, "Cross", "t", 8, 30, ANIM_FLAVOR_ADAPTIVE_DELTA);
                // time-coded shape: NumTimeCodes=3, pivot 0, vl 1, flags X,
                // packets 0/1.0, 2/2.0, 4/3.0 — parsed by the AD reader as
                // NumFrames=3, Scale 0.01f, base 1.0f and packet bytes
                // {02,00,00,00, 00,00,00,3f, 04,00,...}
                w_tc_motion_channel(b, 3, 0, 1, ANIM_CHANNEL_X,
                                    {f2w(0.01f), f2w(1.0f), 2, f2w(2.0f), 4, f2w(3.0f)});
                push(out, "c38", b);
        }

        // c39: the 24-bone routing corpus — X on every fourth bone, Q on
        // every sixth, a vis channel on bone 10, and out-of-range pivots
        // (24, 40) discarded. The probes cover bones 0..8 and 23.
        {
                ChunkBuild b;
                w_compressed_anim_header(b, V40, "Skeleton", "soldier", 30, 30, ANIM_FLAVOR_TIMECODED);
                for (int piv = 0; piv < 24; piv += 4) {
                        w_tc_motion_channel(b, 2, piv, 1, ANIM_CHANNEL_X, {0, f2w((float)piv), 10, f2w((float)piv + 1.0f)});
                }
                for (int piv = 2; piv < 24; piv += 6) {
                        w_tc_motion_channel(b, 2, piv, 4, ANIM_CHANNEL_Q,
                                            {0, f2w(0.5f), f2w(0.5f), f2w(0.5f), f2w(0.5f),
                                             10, f2w(s45), f2w(0.0f), f2w(0.0f), f2w(s45)});
                }
                w_tc_bit_channel(b, 2, 10, BIT_CHANNEL_VIS, 0, {0, 1u << 31});
                w_tc_motion_channel(b, 2, 24, 1, ANIM_CHANNEL_X, {0, f2w(1.0f), 2, f2w(2.0f)});
                w_tc_motion_channel(b, 2, 40, 1, ANIM_CHANNEL_Y, {0, f2w(1.0f), 2, f2w(2.0f)});
                push(out, "c39", b);
        }

        // c40: the header-only animation — no channels at all; every query
        // answers the identity (or always-visible).
        {
                ChunkBuild b;
                w_compressed_anim_header(b, V40, "Empty", "t", 4, 30, ANIM_FLAVOR_ADAPTIVE_DELTA);
                push(out, "c40", b);
        }

        // c41: the bit-channel scan corpus — time codes 0/2/4/6 with
        // alternating bits, DefaultVal 1 (DEAD: frames below the first
        // time code read packet 0's bit, never the default), probes that
        // scan forward, past the end, and rewind to the negative frames.
        {
                ChunkBuild b;
                w_compressed_anim_header(b, V40, "Bits", "t", 8, 30, ANIM_FLAVOR_TIMECODED);
                w_tc_bit_channel(b, 4, 0, BIT_CHANNEL_VIS, 1,
                                 {0, 2 | (1u << 31), 4, 6 | (1u << 31)});
                push(out, "c41", b);
        }

        // c42: a bit channel with NumTimeCodes 0 — the byte count wraps
        // to ~4 GiB, the read fails, the load fails.
        {
                ChunkBuild b;
                w_compressed_anim_header(b, V40, "Zero", "t", 4, 30, ANIM_FLAVOR_TIMECODED);
                b.begin(W3D_CHUNK_COMPRESSED_BIT_CHANNEL);
                b.w_u32(0);
                uint8_t f[4];
                put_u16(f + 0, 0);
                f[2] = BIT_CHANNEL_VIS;
                f[3] = 0;
                b.w_bytes(f, 4);
                b.w_u32(0);
                b.end();
                push(out, "c42", b);
        }

        // c43: a single-time-code bit channel — every probe scans to the
        // end (or the clamp at 0) and reads packet 0's bit.
        {
                ChunkBuild b;
                w_compressed_anim_header(b, V40, "One", "t", 4, 30, ANIM_FLAVOR_TIMECODED);
                w_tc_bit_channel(b, 1, 0, BIT_CHANNEL_VIS, 0, {5 | (1u << 31)});
                push(out, "c43", b);
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
// count is sane (both sides compute the identical list). The big-count arm
// reaches the interior of the big-timecode corpus (c22).
static std::vector<float> probe_frames(int num_frames)
{
        std::vector<float> f = {0.0f, 0.25f, 0.5f, 1.5f, 2.25f, 3.5f, 7.75f};
        if (num_frames >= 1 && num_frames <= 1000) {
                f.push_back((float)(num_frames - 1));
                f.push_back((float)num_frames);
        } else if (num_frames > 1000 && num_frames < (1 << 25)) {
                f.push_back((float)(num_frames - 15));
                f.push_back((float)(num_frames - 11));
                f.push_back((float)(num_frames - 8));
                f.push_back((float)(num_frames - 1));
                f.push_back((float)num_frames);
        }
        return f;
}

// The visibility probes add the negative frames (Get_Bit's (int) cast is
// well-defined there, unlike the motion paths' (uint32_t) casts).
static const float VIS_EXTRA_FRAMES[2] = {-1.0f, -0.5f};

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

// The dump: everything the load path leaves behind, plus the query probes.
static void dump_anim(HCompressedAnimClass &anim, FILE *out, const char *id)
{
        fprintf(out, "hdr %s %s %s %s %d %d %s %s %d\n", id, u32hex(anim.Header.Version).c_str(),
                q_bytes((const uint8_t *)anim.Get_Name(), cstr_len(anim.Get_Name(), 2 * W3D_NAME_LEN)).c_str(),
                q_bytes((const uint8_t *)anim.Get_HName(), cstr_len(anim.Get_HName(), W3D_NAME_LEN)).c_str(),
                anim.Get_Flavor(), anim.Get_Num_Frames(), f32hex(anim.Get_Frame_Rate()).c_str(),
                f32hex(anim.Get_Total_Time()).c_str(), anim.Get_Num_Pivots());

        // the Has_* probes and Is_Node_Motion_Present
        for (int i = 0; i < anim.Get_Num_Pivots(); i++) {
                fprintf(out, "has %s %d %d %d %d %d %d %d\n", id, i, anim.Has_X_Translation(i) ? 1 : 0,
                        anim.Has_Y_Translation(i) ? 1 : 0, anim.Has_Z_Translation(i) ? 1 : 0,
                        anim.Has_Rotation(i) ? 1 : 0, anim.Has_Visibility(i) ? 1 : 0,
                        anim.Is_Node_Motion_Present(i) ? 1 : 0);
        }

        // the stored channels (slot order: x y z q, then vis)
        for (int i = 0; i < anim.Get_Num_Pivots(); i++) {
                const NodeCompressedMotionStruct &m = anim.NodeMotion[i];
                const char *slots[4] = {"x", "y", "z", "q"};
                for (int slot = 0; slot < 4; slot++) {
                        if (anim.Get_Flavor() == ANIM_FLAVOR_TIMECODED) {
                                const TimeCodedMotionChannelClass *channels[4] = {m.tc.X, m.tc.Y, m.tc.Z, m.tc.Q};
                                const TimeCodedMotionChannelClass *c = channels[slot];
                                if (c == nullptr)
                                        continue;
                                std::string words;
                                for (size_t k = 0; k < c->Data.size(); k++) {
                                        if (k > 0)
                                                words += " ";
                                        words += u32hex(c->Data[k]);
                                }
                                fprintf(out, "tcchan %s %d %s %u %d %u %s\n", id, i, slots[slot], c->Type,
                                        c->VectorLen, c->NumTimeCodes, words.c_str());
                        } else if (anim.Get_Flavor() == ANIM_FLAVOR_ADAPTIVE_DELTA) {
                                const AdaptiveDeltaMotionChannelClass *channels[4] = {m.ad.X, m.ad.Y, m.ad.Z, m.ad.Q};
                                const AdaptiveDeltaMotionChannelClass *c = channels[slot];
                                if (c == nullptr)
                                        continue;
                                std::string words;
                                for (size_t k = 0; k < c->Data.size(); k++) {
                                        if (k > 0)
                                                words += " ";
                                        words += u32hex(c->Data[k]);
                                }
                                fprintf(out, "adchan %s %d %s %u %d %u %s %s\n", id, i, slots[slot], c->Type,
                                        c->VectorLen, c->NumFrames, f32hex(c->Scale).c_str(), words.c_str());
                        }
                }
                if (m.Vis != nullptr) {
                        const TimeCodedBitChannelClass *c = m.Vis;
                        std::string words;
                        for (size_t k = 0; k < c->Bits.size(); k++) {
                                if (k > 0)
                                        words += " ";
                                words += u32hex(c->Bits[k]);
                        }
                        fprintf(out, "tcbchan %s %d %u %d %u %s\n", id, i, c->Type, c->DefaultVal, c->NumTimeCodes,
                                words.c_str());
                }
        }

        // the frame queries. The motion probes are non-negative only (the
        // (uint32_t) frame casts are undefined for negatives and differ
        // between x86-64 and ARM); the visibility probes add the negative
        // frames after the shared list. The orientation/transform probes
        // are omitted for unknown flavors: the original leaves the
        // caller's destinations untouched there (not a function of the
        // animation).
        bool queryable = (anim.Get_Flavor() == ANIM_FLAVOR_TIMECODED) ||
                         (anim.Get_Flavor() == ANIM_FLAVOR_ADAPTIVE_DELTA);
        std::vector<float> frames = probe_frames(anim.Get_Num_Frames());
        for (int piv : probe_pivots(anim.Get_Num_Pivots())) {
                for (float frame : frames) {
                        float t[4]; // 3 floats + scratch for the widest in-discipline channel
                        anim.Get_Translation(t, piv, frame);
                        fprintf(out, "trans %s %d %s %s %s %s\n", id, piv, f32hex(frame).c_str(),
                                f32hex(t[0]).c_str(), f32hex(t[1]).c_str(), f32hex(t[2]).c_str());

                        if (queryable) {
                                float q[4];
                                anim.Get_Orientation(q, piv, frame);
                                fprintf(out, "orient %s %d %s %s %s %s %s\n", id, piv, f32hex(frame).c_str(),
                                        f32hex(q[0]).c_str(), f32hex(q[1]).c_str(), f32hex(q[2]).c_str(),
                                        f32hex(q[3]).c_str());

                                // float[16]: the transform's 12 words plus
                                // scratch (a wide X channel reaches m[4],
                                // in bounds; the corpus never goes wider)
                                float m[16];
                                anim.Get_Transform(m, piv, frame);
                                fprintf(out, "xform %s %d %s", id, piv, f32hex(frame).c_str());
                                for (int k = 0; k < 12; k++) {
                                        fprintf(out, " %s", f32hex(m[k]).c_str());
                                }
                                fprintf(out, "\n");
                        }

                        fprintf(out, "vis %s %d %s %d\n", id, piv, f32hex(frame).c_str(),
                                anim.Get_Visibility(piv, frame) ? 1 : 0);
                }
                for (float frame : VIS_EXTRA_FRAMES) {
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
                fprintf(stderr, "usage: gen_hcanim_golden <output-file>\n");
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
        // c24's 17-byte strcpy key: 16 'h's (lowercased 'H') plus the 0x05
        // swallowed from the NumFrames word
        g_trees.push_back({{'h', 'h', 'h', 'h', 'h', 'h', 'h', 'h', 'h', 'h', 'h', 'h', 'h', 'h', 'h', 'h', 0x05}, 3});

        WWMath_Init();

        fprintf(out, "# gen_hcanim_golden output — the W3D compressed animation corpus (P0)\n");
        fprintf(out, "# generated by tools/differential/gen_hcanim_golden.cpp; do not edit\n");

        for (const HTreeStub &t : g_trees) {
                fprintf(out, "tree %s %d\n", q_bytes(t.lower_name.data(), t.lower_name.size()).c_str(), t.pivots);
        }

        for (const Corpus &c : build_corpora()) {
                SrcFile file;
                file.data = c.bytes.data();
                file.len = c.bytes.size();
                ChunkLoadClass cload(&file);

                fprintf(out, "file %s %s\n", c.id, hex_str(c.bytes).c_str());

                HCompressedAnimClass anim;
                int error = anim.Load_W3D(cload);
                if (error == HCompressedAnimClass::OK) {
                        fprintf(out, "ok %s\n", c.id);
                        dump_anim(anim, out, c.id);
                } else {
                        fprintf(out, "err %s\n", c.id);
                }
        }

        fclose(out);
        return 0;
}
