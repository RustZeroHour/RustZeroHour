// SPDX-License-Identifier: GPL-3.0-or-later
//
// Golden-dump driver for `rzh-formats::w3d_htree` (parity level P0).
//
// Verbatim transcription of the Zero Hour hierarchy-tree loader —
// Core/Libraries/Source/WWVegas/WW3D2/htree.cpp at commit 571a7c5
// (HTreeClass::Load_W3D and HTreeClass::read_pivots — the data path the
// skinning pipeline resolves bones against), plus the pieces those
// readers drive:
//   Core/Libraries/Source/WWVegas/WW3D2/pivot.cpp (the PivotClass
//     constructor state the allocation leaves, the fields the load path
//     writes)
//   Core/Libraries/Source/WWVegas/WWMath/quat.h (Build_Matrix3D — the
//     WWINLINE quaternion-to-rotation-matrix builder, mixed f64/f32
//     literals included)
//   Core/Libraries/Source/WWVegas/WWMath/matrix3d.h (Make_Identity,
//     Translate(const Vector3&) and postMul — the
//     AVOID_TEMP_IN_POSTMUL body the #define inside the function always
//     selects)
// The chunk container itself is the chunkio.cpp reader already pinned by
// gen_chunk_golden.cpp; it is transcribed here again (release-build
// semantics) because this harness is self-contained.
//
// The corpus bytes are the payload of the W3D_CHUNK_HIERARCHY container
// the asset manager opens before calling HTreeManagerClass::Load_Tree
// (assetmgr.cpp); the harness enters the loader exactly as the manager
// does, with the container's children as the reader's top level.
//
// Deliberate deviations, pinned identically on both sides (the Rust port
// documents the same set):
//
// - An out-of-range ParentIdx (>= NumPivots, excluding the 0xffffffff
//   root sentinel) fails the load in both. The C++ release build stores
//   an out-of-bounds PivotClass* that the first Base_Update would
//   dereference; the load itself would "succeed" with a broken tree.
//   The port states the contract instead (corpora h24/h32/h33).
//
// - Corpora avoid pivot counts the harness could not allocate (the C++
//   `new PivotClass[n]` would throw/OOM — not deterministic). The
//   pre-3.0 increment wrapping 0xffffffff to 0 (allocation-free) IS
//   pinned (h35).
//
// - Corpora use finite float payloads only: the mixed-precision
//   Build_Matrix3D is byte-reproducible for finite inputs on every
//   IEEE-754 platform; NaN payload arithmetic is not (quiet-bit
//   propagation differs). The loader itself has no finite guard — this
//   is a corpus discipline, not a code path.
//
// - `piv.ParentIdx == -1` compares the uint32 field against the wrapped
//   -1 (0xffffffff), transcribed as the equivalent unsigned comparison
//   to keep -Werror clean. The release build skips `assert(pidx == 0)`
//   (a second root — the pre-3.0 parent-index wrap of h23 — and any
//   later pivot with a 0xffffffff parent are all legal loads).
//
// - The transcription's HTreeClass stashes the decoded
//   W3dHierarchyStruct (the dump pins the full header decode; the
//   original discards it after copying Name and NumPivots). The
//   computed Transform member is not dumped: Load_W3D leaves it identity
//   for every pivot, and Base_Update/Anim_Update fill it later —
//   update-path state, not load-path state. IsVisible IS dumped (the
//   constructor default plus read_pivots' explicit `Pivot[0].IsVisible
//   = true`).
//
// Output grammar (one command per line, hex lowercase, '-' = empty):
//
//   file <id> <hex|->          the corpus bytes (hierarchy payload)
//   ok <id> | err <id>         the load verdict
//   hdr <id> <ver> <name|-> <rawpivots> <numpivots> <center>
//                              ver: version as 8 hex digits; rawpivots:
//                              the header's declared count; numpivots:
//                              the member after the pre-3.0 bump; center:
//                              3 f32 bit words
//   piv <id> <i> <name|-> <parent> <index> <visible> <base>
//                              parent: -1 when the pivot is a root, else
//                              the parent's array position; index: the
//                              pivot's Index field; visible 0|1; base:
//                              the BaseTransform as 12 f32 bit words,
//                              row-major (x y z w per row)
//   getbone <id> <probe|-> <i>  Get_Bone_Index(probe): the first
//                              stricmp match, 0 when absent. Probes: every
//                              pivot's own name in order, then
//                              "RootTransform", then the ASCII lowercase
//                              of pivot 1's name (when the tree has one AND
//                              the name is NUL-terminated — a full-16-byte
//                              field would run stricmp past the struct,
//                              a heap-layout result), then "nomatch"
//   getparent <id> <i> <parent>  Get_Parent_Index(i): the parent's Index
//                              field, 0 when root
//
// Usage: gen_htree_golden <output-file>
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

// enum continuation values at 0x00000100 in w3d_file.h
static const uint32_t W3D_CHUNK_HIERARCHY = 0x00000100;       // hierarchy tree definition
static const uint32_t W3D_CHUNK_HIERARCHY_HEADER = 0x00000101; // W3dHierarchyStruct
static const uint32_t W3D_CHUNK_PIVOTS = 0x00000102;           // array of W3dPivotStruct's
static const uint32_t W3D_CHUNK_PIVOT_FIXUPS = 0x00000103;     // exporter-only, skipped by the loader

// --- the W3D structures (w3d_file.h / iostruct.h, POD) -------------------------
// (iostruct.h's float32_t is the game's typedef of float; kept as float
// here — the sizes are pinned by the static_asserts below)

struct W3dVectorStruct
{
        float X;
        float Y;
        float Z;
};

struct W3dQuaternionStruct
{
        float Q[4];
};

struct W3dHierarchyStruct
{
        uint32_t Version;
        char Name[W3D_NAME_LEN]; // Name of the hierarchy
        uint32_t NumPivots;
        W3dVectorStruct Center;
};

struct W3dPivotStruct
{
        char Name[W3D_NAME_LEN]; // Name of the node (UR_ARM, LR_LEG, TORSO, etc)
        uint32_t ParentIdx;      // 0xffffffff = root pivot; no parent
        W3dVectorStruct Translation; // translation to pivot point
        W3dVectorStruct EulerAngles; // orientation of the pivot point (unused by the loader)
        W3dQuaternionStruct Rotation; // orientation of the pivot point
};

static_assert(sizeof(W3dVectorStruct) == 12, "W3dVectorStruct layout");
static_assert(sizeof(W3dQuaternionStruct) == 16, "W3dQuaternionStruct layout");
static_assert(sizeof(W3dHierarchyStruct) == 36, "W3dHierarchyStruct layout");
static_assert(sizeof(W3dPivotStruct) == 60, "W3dPivotStruct layout");

// --- the ASCII case-insensitive compare (stricmp) ------------------------------

static unsigned char ascii_tolower(unsigned char c)
{
        return (c >= 'A' && c <= 'Z') ? (unsigned char)(c - 'A' + 'a') : c;
}

// stricmp: the ASCII case-insensitive compare (tolower of unsigned
// bytes; high-bit bytes compare unchanged, matching the MSVC/glibc
// locale-C tables on the bytes the corpora use).
static int ascii_stricmp(const char *a, const char *b)
{
        const unsigned char *ua = (const unsigned char *)a;
        const unsigned char *ub = (const unsigned char *)b;
        while (*ua && *ub) {
                unsigned char ca = ascii_tolower(*ua);
                unsigned char cb = ascii_tolower(*ub);
                if (ca != cb)
                        return (int)ca - (int)cb;
                ua++;
                ub++;
        }
        return (int)ascii_tolower(*ua) - (int)ascii_tolower(*ub);
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
        uint32_t Cur_Chunk_Length();

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
        // is a caller contract violation the hierarchy loader never performs
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
        // hierarchy loader only reaches here after a failed Open_Chunk
        // (which already returned LOAD_ERROR); the transcription states
        // the contract instead.
        if (StackIndex < 1) {
                return 0xFFFFFFFF;
        }
        return HeaderStack[StackIndex - 1].Get_Type();
}

[[maybe_unused]] uint32_t ChunkLoadClass::Cur_Chunk_Length()
{
        if (StackIndex < 1) {
                return 0xFFFFFFFF;
        }
        return HeaderStack[StackIndex - 1].Get_Size();
}

uint32_t ChunkLoadClass::Read(void *buf, uint32_t nbytes)
{
        // release build: the StackIndex assert is skipped and StackIndex == 0
        // reads the stacks out of bounds (UB); the hierarchy loader never reads
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

// --- the WWMath subset the loader drives (matrix3d.h / quat.h / vector3.h) ---

struct Vector3
{
        float X, Y, Z;

        Vector3(float x, float y, float z) : X(x), Y(y), Z(z) {}

        float operator[](int i) const { return (&X)[i]; }
};

struct Vector4
{
        float X, Y, Z, W;

        Vector4() : X(0), Y(0), Z(0), W(0) {}

        void Set(float x, float y, float z, float w)
        {
                X = x;
                Y = y;
                Z = z;
                W = w;
        }

        float operator[](int i) const { return (&X)[i]; }
        float &operator[](int i) { return (&X)[i]; }
};

struct Quaternion
{
        // X,Y,Z are the imaginary parts of the quaternion; W is the real part
        float X, Y, Z, W;

        Quaternion() : X(0), Y(0), Z(0), W(0) {}
        Quaternion(float a, float b, float c, float d) : X(a), Y(b), Z(c), W(d) {}

        float operator[](int i) const { return (&X)[i]; }
};

struct Matrix3D
{
        Vector4 Row[3];

        Matrix3D() {}
        explicit Matrix3D(bool init)
        {
                if (init)
                        Make_Identity();
        }

        void Make_Identity()
        {
                Row[0].Set(1.0f, 0.0f, 0.0f, 0.0f);
                Row[1].Set(0.0f, 1.0f, 0.0f, 0.0f);
                Row[2].Set(0.0f, 0.0f, 1.0f, 0.0f);
        }

        // Translate(const Vector3&) — post-multiplies by a translation
        // matrix (the float overload without the (float) casts).
        void Translate(const Vector3 &t)
        {
                Row[0][3] += Row[0][0] * t[0] + Row[0][1] * t[1] + Row[0][2] * t[2];
                Row[1][3] += Row[1][0] * t[0] + Row[1][1] * t[1] + Row[1][2] * t[2];
                Row[2][3] += Row[2][0] * t[0] + Row[2][1] * t[1] + Row[2][2] * t[2];
        }

        void postMul(const Matrix3D &that);
};

static float submul(const Vector4 &row, float tmp1, float tmp2, float tmp3)
{
        return row.X * tmp1 + row.Y * tmp2 + row.Z * tmp3;
}

// does "this = this * that" — the AVOID_TEMP_IN_POSTMUL body (the #define
// sits inside the original's function, so this is the always-compiled arm)
void Matrix3D::postMul(const Matrix3D &that)
{
        float tmpX, tmpY, tmpZ, tmpW;

        tmpX = submul(this->Row[0], that.Row[0].X, that.Row[1].X, that.Row[2].X);
        tmpY = submul(this->Row[0], that.Row[0].Y, that.Row[1].Y, that.Row[2].Y);
        tmpZ = submul(this->Row[0], that.Row[0].Z, that.Row[1].Z, that.Row[2].Z);
        tmpW = submul(this->Row[0], that.Row[0].W, that.Row[1].W, that.Row[2].W);

        this->Row[0].X = tmpX;
        this->Row[0].Y = tmpY;
        this->Row[0].Z = tmpZ;
        this->Row[0].W += tmpW;

        tmpX = submul(this->Row[1], that.Row[0].X, that.Row[1].X, that.Row[2].X);
        tmpY = submul(this->Row[1], that.Row[0].Y, that.Row[1].Y, that.Row[2].Y);
        tmpZ = submul(this->Row[1], that.Row[0].Z, that.Row[1].Z, that.Row[2].Z);
        tmpW = submul(this->Row[1], that.Row[0].W, that.Row[1].W, that.Row[2].W);

        this->Row[1].X = tmpX;
        this->Row[1].Y = tmpY;
        this->Row[1].Z = tmpZ;
        this->Row[1].W += tmpW;

        tmpX = submul(this->Row[2], that.Row[0].X, that.Row[1].X, that.Row[2].X);
        tmpY = submul(this->Row[2], that.Row[0].Y, that.Row[1].Y, that.Row[2].Y);
        tmpZ = submul(this->Row[2], that.Row[0].Z, that.Row[1].Z, that.Row[2].Z);
        tmpW = submul(this->Row[2], that.Row[0].W, that.Row[1].W, that.Row[2].W);

        this->Row[2].X = tmpX;
        this->Row[2].Y = tmpY;
        this->Row[2].Z = tmpZ;
        this->Row[2].W += tmpW;
}

// Convert a quaternion into a rotation matrix (quat.h WWINLINE
// Build_Matrix3D — Matrix3D& out). The literal mix — double 2.0
// everywhere except the [1][1] term's 2.0f — is the original's.
static Matrix3D &Build_Matrix3D(const Quaternion &q, Matrix3D &out)
{
        out.Row[0][0] = (float)(1.0 - 2.0 * (q[1] * q[1] + q[2] * q[2]));
        out.Row[0][1] = (float)(2.0 * (q[0] * q[1] - q[2] * q[3]));
        out.Row[0][2] = (float)(2.0 * (q[2] * q[0] + q[1] * q[3]));

        out.Row[1][0] = (float)(2.0 * (q[0] * q[1] + q[2] * q[3]));
        out.Row[1][1] = (float)(1.0 - 2.0f * (q[2] * q[2] + q[0] * q[0]));
        out.Row[1][2] = (float)(2.0 * (q[1] * q[2] - q[0] * q[3]));

        out.Row[2][0] = (float)(2.0 * (q[2] * q[0] - q[1] * q[3]));
        out.Row[2][1] = (float)(2.0 * (q[1] * q[2] + q[0] * q[3]));
        out.Row[2][2] = (float)(1.0 - 2.0 * (q[1] * q[1] + q[0] * q[0]));

        // no translation
        out.Row[0][3] = out.Row[1][3] = out.Row[2][3] = 0.0f;

        return out;
}

// --- PivotClass (pivot.h / pivot.cpp, load-path state) ---------------------------

struct PivotClass
{
        char Name[W3D_NAME_LEN];
        PivotClass *Parent;
        Matrix3D BaseTransform; // base-pose transform (relative to parent).
        Matrix3D Transform;     // computed transform for this pivot
        int Index;
        bool IsVisible;

        PivotClass()
            : Parent(nullptr),
              BaseTransform(1),
              Transform(1),
              Index(0),
              IsVisible(true)
        {
                Name[0] = 0;
        }
};

// --- HTreeClass (htree.cpp, the load path) --------------------------------------

class HTreeClass
{
public:
        enum
        {
                OK,
                LOAD_ERROR
        };

        HTreeClass() : NumPivots(0), Pivot(nullptr), ScaleFactor(1.0f) { Name[0] = 0; }
        ~HTreeClass() { Free(); }

        int Load_W3D(ChunkLoadClass &cload);

        // the query surface the skinning path drives (transcribed from
        // htree.h/htree.cpp; used by the dump)
        const char *Get_Name() const { return Name; }
        int Num_Pivots() const { return NumPivots; }
        int Get_Bone_Index(const char *name) const;
        int Get_Parent_Index(int boneindx) const;

        // the load-path state the dump reads
        char Name[W3D_NAME_LEN];
        int NumPivots;
        PivotClass *Pivot;
        float ScaleFactor;
        W3dHierarchyStruct Header; // dump-side stash (documented deviation)

private:
        void Free()
        {
                delete[] Pivot;
                Pivot = nullptr;
                NumPivots = 0;
                ScaleFactor = 1.0f;
        }
        bool read_pivots(ChunkLoadClass &cload, bool pre30);
};

int HTreeClass::Load_W3D(ChunkLoadClass &cload)
{
        Free();

        /*
         **      Read the first chunk, it should be the hierarchy header
         */
        if (!cload.Open_Chunk())
                return LOAD_ERROR;

        if (cload.Cur_Chunk_ID() != W3D_CHUNK_HIERARCHY_HEADER) {
                // ERROR: Expected Hierarchy Header
                return LOAD_ERROR;
        }

        W3dHierarchyStruct header;
        if (cload.Read(&header, sizeof(W3dHierarchyStruct)) != sizeof(W3dHierarchyStruct)) {
                return LOAD_ERROR;
        }

        Header = header; // dump-side stash: the pristine decode

        cload.Close_Chunk();

        /*
         ** Check the version, if < 3.0 add a root node for everything
         ** to attach to.  The load_pivots function will also have to be
         ** notified of this.
         */
        bool pre30 = false;
        if (header.Version < W3D_MAKE_VERSION(3, 0)) {
                header.NumPivots++;
                pre30 = true;
        }

        /*
         ** Allocate the array of pivots
         */
        memcpy(Name, header.Name, W3D_NAME_LEN);
        NumPivots = (int)header.NumPivots;
        if (NumPivots < 1) {
                return LOAD_ERROR;
        }
        Pivot = new PivotClass[NumPivots];

        /*
         ** Now, read in all of the other chunks for this hierarchy.
         */

        while (cload.Open_Chunk()) {

                switch (cload.Cur_Chunk_ID()) {

                        case W3D_CHUNK_PIVOTS:
                                if (!read_pivots(cload, pre30)) {
                                        goto Error;
                                }
                                break;

                        default:
                                // ERROR: expected W3D_CHUNK_PIVOTS!
                                break;
                }
                cload.Close_Chunk();
        }

        return OK;

Error:

        Free();
        return LOAD_ERROR;
}

bool HTreeClass::read_pivots(ChunkLoadClass &cload, bool pre30)
{
        W3dPivotStruct piv;
        Matrix3D mtx;

        int first_piv = 0;

        /*
         ** At (w3d file format) version 3.0, I added a node for the root.  Pre-3.0 htrees didn't have
         ** this so we just put one in.
         */
        if (pre30) {
                Pivot[0].Index = 0;
                Pivot[0].Parent = nullptr;
                Pivot[0].BaseTransform.Make_Identity();
                Pivot[0].Transform.Make_Identity();
                Pivot[0].IsVisible = true;
                strcpy(Pivot[0].Name, "RootTransform");
                first_piv++;
        }

        for (int pidx = first_piv; pidx < NumPivots; pidx++) {

                if (cload.Read(&piv, sizeof(W3dPivotStruct)) != sizeof(W3dPivotStruct)) {
                        return false;
                }

                memcpy(Pivot[pidx].Name, piv.Name, W3D_NAME_LEN);
                Pivot[pidx].Index = pidx;

                Pivot[pidx].BaseTransform.Make_Identity();
                Pivot[pidx].BaseTransform.Translate(Vector3(piv.Translation.X, piv.Translation.Y, piv.Translation.Z));

                Pivot[pidx].BaseTransform.postMul(
                    Build_Matrix3D(Quaternion(piv.Rotation.Q[0], piv.Rotation.Q[1], piv.Rotation.Q[2], piv.Rotation.Q[3]), mtx));

                /*
                 ** At version 3.0 a root node was added, this "fixes up" pre-3.0 files
                 ** to have that root node
                 */
                if (pre30) {
                        piv.ParentIdx += 1;
                }

                /*
                 ** Set the parent pointer.  The first pivot will have a parent index
                 ** of -1 (in post-3.0 files) so set its parent to nullptr.
                 ** (the uint32 field against the wrapped -1; the release build
                 ** skips assert(pidx == 0))
                 */
                if (piv.ParentIdx == 0xFFFFFFFFu) {
                        Pivot[pidx].Parent = nullptr;
                } else if (piv.ParentIdx >= (uint32_t)NumPivots) {
                        // DELIBERATE DEVIATION (documented above): the release
                        // build stores an out-of-bounds PivotClass* here; the
                        // transcription states the contract instead.
                        return false;
                } else {
                        Pivot[pidx].Parent = &(Pivot[piv.ParentIdx]);
                }
        }

        Pivot[0].Transform.Make_Identity();
        Pivot[0].IsVisible = true;

        return true;
}

int HTreeClass::Get_Bone_Index(const char *name) const
{
        for (int i = 0; i < NumPivots; i++) {
                if (ascii_stricmp(Pivot[i].Name, name) == 0) {
                        return i;
                }
        }
        return 0;
}

int HTreeClass::Get_Parent_Index(int boneindx) const
{
        if (Pivot[boneindx].Parent != nullptr) {
                return Pivot[boneindx].Parent->Index;
        } else {
                return 0;
        }
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

// A 36-byte W3dHierarchyStruct chunk
static void w_hierarchy_header(ChunkBuild &b, uint32_t version, const char *name, uint32_t numpivots,
                               float cx, float cy, float cz)
{
        char hdrname[W3D_NAME_LEN];
        memset(hdrname, 0, sizeof(hdrname));
        snprintf(hdrname, sizeof(hdrname), "%s", name);

        b.begin(W3D_CHUNK_HIERARCHY_HEADER);
        b.w_u32(version);
        b.w_bytes(hdrname, W3D_NAME_LEN);
        b.w_u32(numpivots);
        b.w_f32(cx);
        b.w_f32(cy);
        b.w_f32(cz);
        b.end();
}

// A 60-byte W3dPivotStruct
static void w_pivot(ChunkBuild &b, const char *name, uint32_t parent, float tx, float ty, float tz, float ex,
                    float ey, float ez, float qx, float qy, float qz, float qw)
{
        char pivname[W3D_NAME_LEN];
        memset(pivname, 0, sizeof(pivname));
        snprintf(pivname, sizeof(pivname), "%s", name);

        b.w_bytes(pivname, W3D_NAME_LEN);
        b.w_u32(parent);
        b.w_f32(tx);
        b.w_f32(ty);
        b.w_f32(tz);
        b.w_f32(ex);
        b.w_f32(ey);
        b.w_f32(ez);
        b.w_f32(qx);
        b.w_f32(qy);
        b.w_f32(qz);
        b.w_f32(qw);
}

// A W3dPivotStruct with a raw 16-byte name field (no NUL)
static void w_pivot_rawname(ChunkBuild &b, const uint8_t name[16], uint32_t parent, const float *translation,
                            const float *euler, const float *quat)
{
        b.w_bytes(name, W3D_NAME_LEN);
        b.w_u32(parent);
        for (int i = 0; i < 3; i++)
                b.w_f32(translation[i]);
        for (int i = 0; i < 3; i++)
                b.w_f32(euler[i]);
        for (int i = 0; i < 4; i++)
                b.w_f32(quat[i]);
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
        const uint32_t NO_PARENT = 0xFFFFFFFFu;
        const uint32_t V41 = (4u << 16) | 1u;
        const uint32_t V30 = (3u << 16) | 0u;
        const uint32_t V21 = (2u << 16) | 1u;
        const uint32_t V2FF = (2u << 16) | 0xFFu;
        auto push = [&](const char *id, ChunkBuild &b) {
                out.push_back(Corpus{id, b.bytes});
        };

        // h01: the modern full tree — a named hierarchy with a chain of
        // attached bones carrying real translations and rotations.
        {
                ChunkBuild b;
                w_hierarchy_header(b, V41, "AVTank", 5, 1.5f, -2.25f, 0.5f);
                b.begin(W3D_CHUNK_PIVOTS);
                {
                        const float root_q[4] = {0.0f, 0.0f, 0.0f, 1.0f};
                        w_pivot(b, "ROOT", NO_PARENT, 0.0f, 0.0f, 0.0f, 0, 0, 0, root_q[0], root_q[1], root_q[2],
                                root_q[3]);
                        // a 90-degree turn about Y: (0, sin45, 0, cos45)
                        const float s45 = 0.70710678118654752440f;
                        w_pivot(b, "TORSO", 0, 0.25f, 1.0f, -0.75f, 0, 0, 0, 0.0f, s45, 0.0f, s45);
                        // a 45-degree turn about X: (sin22.5, 0, 0, cos22.5)
                        const float s225 = 0.38268343236508977173f;
                        const float c225 = 0.92387953251128675613f;
                        w_pivot(b, "UR_ARM", 1, 1.25f, 0.5f, 0.0f, 0, 0, 0, s225, 0.0f, 0.0f, c225);
                        w_pivot(b, "LR_ARM", 2, -0.875f, 0.0625f, 2.5f, 0, 0, 0, 0.1f, -0.2f, 0.3f, 0.9f);
                        w_pivot(b, "HEAD", 1, 0.0f, 2.0f, 0.0f, 0, 0, 0, 0.0f, 0.0f, 0.0f, 1.0f);
                }
                b.end();
                push("h01", b);
        }

        // h02: all-zero quaternions — Build_Matrix3D of the zero
        // quaternion is the identity rotation (translation-only pivots).
        {
                ChunkBuild b;
                w_hierarchy_header(b, V41, "ZeroQ", 4, 0.0f, 0.0f, 0.0f);
                b.begin(W3D_CHUNK_PIVOTS);
                w_pivot(b, "A", NO_PARENT, 1.0f, 2.0f, 3.0f, 0, 0, 0, 0, 0, 0, 0);
                w_pivot(b, "B", 0, -1.5f, 0.25f, 4.75f, 0, 0, 0, 0, 0, 0, 0);
                w_pivot(b, "C", 1, 0.0f, -0.0f, 1.0f, 0, 0, 0, 0, 0, 0, 0);
                w_pivot(b, "D", 2, -8.0f, 9.0f, -10.0f, 0, 0, 0, 0, 0, 0, 0);
                b.end();
                push("h02", b);
        }

        // h03: identity quaternions with negative-zero translations —
        // the -0.0 rounding through the identity rows of Translate.
        {
                ChunkBuild b;
                w_hierarchy_header(b, V41, "Ident", 3, -1.0f, 1.0f, -1.0f);
                b.begin(W3D_CHUNK_PIVOTS);
                w_pivot(b, "R", NO_PARENT, 0.0f, 0.0f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                w_pivot(b, "P1", 0, -0.0f, -0.0f, -0.0f, 0, 0, 0, 0, 0, 0, 1);
                w_pivot(b, "P2", 1, -3.5f, 2.25f, -0.125f, 0, 0, 0, 0, 0, 0, 1);
                b.end();
                push("h03", b);
        }

        // h04: an unnormalized quaternion (2,0,0,0) — the matrix scales
        // with the squared magnitude; the loader does not normalize.
        {
                ChunkBuild b;
                w_hierarchy_header(b, V41, "Unnorm", 2, 0.0f, 0.0f, 0.0f);
                b.begin(W3D_CHUNK_PIVOTS);
                w_pivot(b, "R", NO_PARENT, 0.0f, 0.0f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                w_pivot(b, "B", 0, 1.0f, 1.0f, 1.0f, 0, 0, 0, 2.0f, 0.0f, 0.0f, 0.0f);
                b.end();
                push("h04", b);
        }

        // h05: a realistic 24-bone skeleton — deep chains and varied
        // rotations (pelvis -> torso -> arms/head, pelvis -> legs/feet),
        // with one duplicated bone name (LL_LEG appears twice).
        {
                ChunkBuild b;
                w_hierarchy_header(b, V41, "HumanSkel", 24, 0.0f, 90.0f, 3.0f);
                b.begin(W3D_CHUNK_PIVOTS);
                w_pivot(b, "ROOT", NO_PARENT, 0.0f, 0.0f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                w_pivot(b, "pelvis", 0, 0.0f, 0.95f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                w_pivot(b, "torso", 1, 0.0f, 0.35f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                w_pivot(b, "chest", 2, 0.0f, 0.45f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                w_pivot(b, "neck", 3, 0.0f, 0.25f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                w_pivot(b, "head", 4, 0.0f, 0.15f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                w_pivot(b, "UR_ARM", 3, 0.0f, 0.15f, -0.55f, 0, 0, 0, 0.5f, 0.5f, 0.5f, 0.5f);
                w_pivot(b, "LR_ARM", 6, 0.0f, -0.3f, 0.0f, 0, 0, 0, 0.0f, 0.38268343f, 0.0f, 0.92387953f);
                w_pivot(b, "UR_HAND", 7, 0.0f, -0.28f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                w_pivot(b, "LR_HAND", 8, 0.0f, -0.25f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                w_pivot(b, "UL_ARM", 3, 0.0f, 0.15f, 0.55f, 0, 0, 0, -0.5f, 0.5f, 0.5f, 0.5f);
                w_pivot(b, "LL_ARM", 10, 0.0f, -0.3f, 0.0f, 0, 0, 0, 0.0f, -0.38268343f, 0.0f, 0.92387953f);
                w_pivot(b, "UL_HAND", 11, 0.0f, -0.28f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                w_pivot(b, "LL_HAND", 12, 0.0f, -0.25f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                w_pivot(b, "UL_LEG", 1, -0.12f, -0.05f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                w_pivot(b, "LL_LEG", 14, 0.0f, -0.45f, 0.0f, 0, 0, 0, 0.5f, 0.5f, 0.5f, 0.5f);
                w_pivot(b, "UL_FOOT", 15, 0.0f, -0.42f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                w_pivot(b, "LL_LEG", 1, 0.12f, -0.05f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                w_pivot(b, "LL_FOOT2", 17, 0.0f, -0.45f, 0.0f, 0, 0, 0, 0.0f, 0.70710677f, 0.0f, 0.70710677f);
                w_pivot(b, "LL_TOE", 18, 0.02f, -0.03f, 0.12f, 0, 0, 0, 0.1f, 0.2f, 0.3f, 0.9f);
                w_pivot(b, "tail", 1, 0.0f, -0.1f, -0.2f, 0, 0, 0, 0, 0, 0.25881905f, 0.96592583f);
                w_pivot(b, "tail2", 20, 0.0f, -0.15f, -0.25f, 0, 0, 0, 0, 0, 0, 1);
                w_pivot(b, "antenna", 4, 0.05f, 0.1f, -0.02f, 0, 0, 0, -0.1f, -0.2f, -0.3f, -0.9f);
                w_pivot(b, "tool", 6, 0.0f, -0.05f, 0.05f, 0, 0, 0, 0.25f, -0.25f, 0.25f, -0.25f);
                b.end();
                push("h05", b);
        }

        // h06: a forward parent reference — pivot 1's parent is pivot 2
        // (the array is pre-allocated, so forward refs resolve).
        {
                ChunkBuild b;
                w_hierarchy_header(b, V41, "Fwd", 3, 0.0f, 0.0f, 0.0f);
                b.begin(W3D_CHUNK_PIVOTS);
                w_pivot(b, "R", NO_PARENT, 0.0f, 0.0f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                w_pivot(b, "A", 2, 1.0f, 0.0f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                w_pivot(b, "B", 0, 0.5f, 0.0f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                b.end();
                push("h06", b);
        }

        // h07: a self-parenting pivot (ParentIdx == its own index) —
        // a malformed but loadable cycle.
        {
                ChunkBuild b;
                w_hierarchy_header(b, V41, "Self", 2, 0.0f, 0.0f, 0.0f);
                b.begin(W3D_CHUNK_PIVOTS);
                w_pivot(b, "R", NO_PARENT, 0.0f, 0.0f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                w_pivot(b, "Ouro", 1, 1.0f, 1.0f, 1.0f, 0, 0, 0, 0, 0, 0, 1);
                b.end();
                push("h07", b);
        }

        // h08: names differing only by case plus duplicates — pins
        // Get_Bone_Index's first-match, case-insensitive scan.
        {
                ChunkBuild b;
                w_hierarchy_header(b, V41, "Case", 5, 0.0f, 0.0f, 0.0f);
                b.begin(W3D_CHUNK_PIVOTS);
                w_pivot(b, "R", NO_PARENT, 0.0f, 0.0f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                w_pivot(b, "TORSO", 0, 1.0f, 0.0f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                w_pivot(b, "UR_ARM", 1, 0.0f, 1.0f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                w_pivot(b, "torso", 2, 0.0f, 0.0f, 1.0f, 0, 0, 0, 0, 0, 0, 1);
                w_pivot(b, "UR_ARM", 3, 1.0f, 1.0f, 1.0f, 0, 0, 0, 0, 0, 0, 1);
                b.end();
                push("h08", b);
        }

        // h09: 16-byte name fields without any NUL — the C-string view is
        // the whole field.
        {
                ChunkBuild b;
                w_hierarchy_header(b, V41, "AVeryLongTreeNam", 2, 0.0f, 0.0f, 0.0f);
                b.begin(W3D_CHUNK_PIVOTS);
                const uint8_t n0[16] = {'1', '2', '3', '4', '5', '6', '7', '8', '9', '0', 'a', 'b', 'c', 'd', 'e', 'f'};
                const uint8_t n1[16] = {'X', 'Y', 'Z', '_', 'w', 'i', 'd', 'e', '!', '#', '$', '%', '&', '(', ')', '*'};
                const float t[3] = {1.0f, 2.0f, 3.0f};
                const float e[3] = {0.0f, 0.0f, 0.0f};
                const float q[4] = {0.0f, 0.0f, 0.0f, 1.0f};
                w_pivot_rawname(b, n0, NO_PARENT, t, e, q);
                w_pivot_rawname(b, n1, 0, t, e, q);
                b.end();
                push("h09", b);
        }

        // h10: names with high-bit and control bytes — the <hex> name dump
        // and stricmp's unchanged high-bit compare.
        {
                ChunkBuild b;
                w_hierarchy_header(b, V41, "Odd", 2, 0.0f, 0.0f, 0.0f);
                b.begin(W3D_CHUNK_PIVOTS);
                const uint8_t n0[16] = {0xC0, 0x41, 0x42, 0x01, 0x7F, 0x00, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0};
                const uint8_t n1[16] = {0x80, 0x9A, 0xBC, 0xFF, 0x00, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0};
                const float t[3] = {0.0f, 0.0f, 0.0f};
                const float e[3] = {0.0f, 0.0f, 0.0f};
                const float q[4] = {0.0f, 0.0f, 0.0f, 1.0f};
                w_pivot_rawname(b, n0, NO_PARENT, t, e, q);
                w_pivot_rawname(b, n1, 0, t, e, q);
                b.end();
                push("h10", b);
        }

        // h11: a single root pivot (NumPivots == 1, the 0xffffffff parent).
        {
                ChunkBuild b;
                w_hierarchy_header(b, V41, "OnlyRoot", 1, 0.0f, 0.0f, 0.0f);
                b.begin(W3D_CHUNK_PIVOTS);
                w_pivot(b, "Solo", NO_PARENT, 4.5f, -4.5f, 0.25f, 0, 0, 0, 0.5f, 0.5f, 0.5f, 0.5f);
                b.end();
                push("h11", b);
        }

        // h12: a single pivot whose parent is index 0 — itself (the root
        // of a one-pivot self-cycle).
        {
                ChunkBuild b;
                w_hierarchy_header(b, V41, "SelfRoot", 1, 0.0f, 0.0f, 0.0f);
                b.begin(W3D_CHUNK_PIVOTS);
                w_pivot(b, "Loop", 0, 1.0f, 2.0f, 3.0f, 0, 0, 0, 0, 0, 0, 1);
                b.end();
                push("h12", b);
        }

        // h13: version exactly 3.0 — NOT pre-3.0 (the comparison is <).
        {
                ChunkBuild b;
                w_hierarchy_header(b, V30, "V30", 2, 0.0f, 0.0f, 0.0f);
                b.begin(W3D_CHUNK_PIVOTS);
                w_pivot(b, "R", NO_PARENT, 0.0f, 0.0f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                w_pivot(b, "B", 0, 1.0f, 0.0f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                b.end();
                push("h13", b);
        }

        // h14: a header chunk with 4 trailing junk bytes — Close_Chunk
        // seeks past them.
        {
                ChunkBuild b;
                b.begin(W3D_CHUNK_HIERARCHY_HEADER);
                b.w_u32(V41);
                b.w_bytes("Pad", 4);
                b.w_bytes("\0\0\0\0\0\0\0\0\0\0\0\0", 12); // pad the name to 16
                b.w_u32(2);
                b.w_f32(0.0f);
                b.w_f32(0.0f);
                b.w_f32(0.0f);
                b.w_bytes("\xAA\xAA\xAA\xAA", 4); // the trailing junk (size becomes 40)
                b.end();
                b.begin(W3D_CHUNK_PIVOTS);
                w_pivot(b, "R", NO_PARENT, 0.0f, 0.0f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                w_pivot(b, "B", 0, 1.0f, 0.0f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                b.end();
                push("h14", b);
        }

        // h15: a PIVOT_FIXUPS chunk (exporter-only) after the pivots —
        // the default case skips it.
        {
                ChunkBuild b;
                w_hierarchy_header(b, V41, "Fixups", 2, 0.0f, 0.0f, 0.0f);
                b.begin(W3D_CHUNK_PIVOTS);
                w_pivot(b, "R", NO_PARENT, 0.0f, 0.0f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                w_pivot(b, "B", 0, 1.0f, 0.0f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                b.end();
                b.begin(W3D_CHUNK_PIVOT_FIXUPS);
                for (int i = 0; i < 12; i++)
                        b.w_f32(0.5f);
                b.end();
                push("h15", b);
        }

        // h16: unknown chunk ids interleaved around the pivots.
        {
                ChunkBuild b;
                w_hierarchy_header(b, V41, "Unknowns", 2, 0.0f, 0.0f, 0.0f);
                b.begin(0x999u);
                b.w_bytes("junkjunkjunk", 12);
                b.end();
                b.begin(W3D_CHUNK_PIVOTS);
                w_pivot(b, "R", NO_PARENT, 0.0f, 0.0f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                w_pivot(b, "B", 0, 1.0f, 0.0f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                b.end();
                b.begin(0x104u);
                b.w_u32(0xDEADBEEF);
                b.end();
                push("h16", b);
        }

        // h17: the pivots chunk flagged as containing sub-chunks — the
        // flag is advisory; read_pivots reads raw structs anyway.
        {
                ChunkBuild b;
                w_hierarchy_header(b, V41, "Flagged", 2, 0.0f, 0.0f, 0.0f);
                b.begin(W3D_CHUNK_PIVOTS);
                w_pivot(b, "R", NO_PARENT, 0.0f, 0.0f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                w_pivot(b, "B", 0, 1.0f, 0.0f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                b.end();
                // set the contains-sub-chunks bit on the pivots chunk
                size_t off = 36 + 8; // past the header chunk
                uint32_t size;
                memcpy(&size, b.bytes.data() + off + 4, 4);
                size |= 0x80000000;
                memcpy(b.bytes.data() + off + 4, &size, 4);
                push("h17", b);
        }

        // h18: TWO pivots chunks — the second read overwrites every pivot.
        {
                ChunkBuild b;
                w_hierarchy_header(b, V41, "Twice", 3, 0.0f, 0.0f, 0.0f);
                b.begin(W3D_CHUNK_PIVOTS);
                w_pivot(b, "R", NO_PARENT, 0.0f, 0.0f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                w_pivot(b, "A", 0, 1.0f, 0.0f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                w_pivot(b, "B", 1, 2.0f, 0.0f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                b.end();
                b.begin(W3D_CHUNK_PIVOTS);
                w_pivot(b, "X", NO_PARENT, 5.0f, 5.0f, 5.0f, 0, 0, 0, 0.5f, 0.5f, 0.5f, 0.5f);
                w_pivot(b, "Y", 0, 6.0f, 6.0f, 6.0f, 0, 0, 0, 0.1f, 0.2f, 0.3f, 0.4f);
                w_pivot(b, "Z", 1, 7.0f, 7.0f, 7.0f, 0, 0, 0, 0, 0, 0, 1);
                b.end();
                push("h18", b);
        }

        // h19: NO pivots chunk — the load succeeds with the
        // default-constructed pivot array (empty names, no parents,
        // identity base transforms, Index 0).
        {
                ChunkBuild b;
                w_hierarchy_header(b, V41, "NoPivots", 3, 0.0f, 0.0f, 0.0f);
                push("h19", b);
        }

        // h20: the pivots chunk holds MORE structs than NumPivots — the
        // excess is skipped by Close_Chunk.
        {
                ChunkBuild b;
                w_hierarchy_header(b, V41, "Extra", 2, 0.0f, 0.0f, 0.0f);
                b.begin(W3D_CHUNK_PIVOTS);
                w_pivot(b, "R", NO_PARENT, 0.0f, 0.0f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                w_pivot(b, "B", 0, 1.0f, 0.0f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                w_pivot(b, "EXTRA1", 0, 9.0f, 9.0f, 9.0f, 0, 0, 0, 0, 0, 0, 1);
                w_pivot(b, "EXTRA2", 0, 8.0f, 8.0f, 8.0f, 0, 0, 0, 0, 0, 0, 1);
                b.end();
                push("h20", b);
        }

        // h21: the classic pre-3.0 tree — the root is synthesized and the
        // parent indices shift by one (old -1 becomes the new root).
        {
                ChunkBuild b;
                w_hierarchy_header(b, V21, "OldTank", 4, 0.0f, 0.0f, 0.0f);
                b.begin(W3D_CHUNK_PIVOTS);
                w_pivot(b, "TORSO", NO_PARENT, 0.0f, 0.0f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                w_pivot(b, "UR_ARM", 0, 1.25f, 0.5f, 0.0f, 0, 0, 0, 0.5f, 0.5f, 0.5f, 0.5f);
                w_pivot(b, "LR_ARM", 1, -0.875f, 0.0625f, 2.5f, 0, 0, 0, 0.1f, -0.2f, 0.3f, 0.9f);
                w_pivot(b, "HEAD", 0, 0.0f, 2.0f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                b.end();
                push("h21", b);
        }

        // h22: pre-3.0 with NumPivots == 0 — the increment synthesizes the
        // root; the empty pivots chunk reads nothing.
        {
                ChunkBuild b;
                w_hierarchy_header(b, V21, "EmptyOld", 0, 0.0f, 0.0f, 0.0f);
                b.begin(W3D_CHUNK_PIVOTS);
                b.end();
                push("h22", b);
        }

        // h23: pre-3.0 ParentIdx 0xFFFFFFFE — the increment wraps it to
        // 0xFFFFFFFF, the root sentinel: a SECOND root (release-build
        // assert skipped).
        {
                ChunkBuild b;
                w_hierarchy_header(b, V2FF, "WrapRoot", 2, 0.0f, 0.0f, 0.0f);
                b.begin(W3D_CHUNK_PIVOTS);
                w_pivot(b, "FIRST", NO_PARENT, 1.0f, 0.0f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                w_pivot(b, "SECOND", 0xFFFFFFFEu, 2.0f, 0.0f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                b.end();
                push("h23", b);
        }

        // h24: pre-3.0 with an out-of-range parent after the fixup — the
        // load fails (the stated-contract deviation).
        {
                ChunkBuild b;
                w_hierarchy_header(b, V21, "BadOld", 3, 0.0f, 0.0f, 0.0f);
                b.begin(W3D_CHUNK_PIVOTS);
                w_pivot(b, "A", NO_PARENT, 1.0f, 0.0f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                w_pivot(b, "B", 0, 2.0f, 0.0f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                w_pivot(b, "C", 5u, 3.0f, 0.0f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                b.end();
                push("h24", b);
        }

        // h25: pre-3.0 with two pivots chunks — the second read
        // re-synthesizes the root and overwrites every pivot.
        {
                ChunkBuild b;
                w_hierarchy_header(b, V21, "OldTwice", 3, 0.0f, 0.0f, 0.0f);
                b.begin(W3D_CHUNK_PIVOTS);
                w_pivot(b, "A", NO_PARENT, 1.0f, 0.0f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                w_pivot(b, "B", 0, 2.0f, 0.0f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                w_pivot(b, "C", 1, 3.0f, 0.0f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                b.end();
                b.begin(W3D_CHUNK_PIVOTS);
                w_pivot(b, "X", NO_PARENT, 5.0f, 5.0f, 5.0f, 0, 0, 0, 0.5f, 0.5f, 0.5f, 0.5f);
                w_pivot(b, "Y", 0, 6.0f, 6.0f, 6.0f, 0, 0, 0, 0.1f, 0.2f, 0.3f, 0.4f);
                w_pivot(b, "Z", 1, 7.0f, 7.0f, 7.0f, 0, 0, 0, 0, 0, 0, 1);
                b.end();
                push("h25", b);
        }

        // h26: an empty payload — Open_Chunk fails, LOAD_ERROR.
        {
                ChunkBuild b;
                push("h26", b);
        }

        // h27: the first chunk is not the hierarchy header.
        {
                ChunkBuild b;
                b.begin(W3D_CHUNK_PIVOTS);
                b.w_u32(0);
                b.end();
                push("h27", b);
        }

        // h28: a header chunk whose declared size is under 36 — the Read
        // is clamped and returns 0.
        {
                ChunkBuild b;
                b.begin(W3D_CHUNK_HIERARCHY_HEADER);
                b.w_u32(V41);
                b.w_bytes("Short", 6);
                b.end();
                push("h28", b);
        }

        // h29: a header chunk declaring 36 but the file ends inside it —
        // the file read is short and the Read fails.
        {
                ChunkBuild b;
                b.begin(W3D_CHUNK_HIERARCHY_HEADER);
                b.w_u32(V41);
                b.w_bytes("AVeryLongTreeNam", 16);
                b.w_u32(2); // 24 of the 36 bytes present
                // no end(): patch the declared size to the full 36
                ChunkBuild::put_u32(b.bytes.data() + 4, 36);
                push("h29", b);
        }

        // h30: post-3.0 NumPivots == 0 — LOAD_ERROR.
        {
                ChunkBuild b;
                w_hierarchy_header(b, V41, "NoPivots", 0, 0.0f, 0.0f, 0.0f);
                b.begin(W3D_CHUNK_PIVOTS);
                b.end();
                push("h30", b);
        }

        // h31: a short pivots chunk — two structs for three pivots; the
        // third Read hits the chunk-size clamp and (unlike the mesh
        // loader) the load fails.
        {
                ChunkBuild b;
                w_hierarchy_header(b, V41, "ShortPivots", 3, 0.0f, 0.0f, 0.0f);
                b.begin(W3D_CHUNK_PIVOTS);
                w_pivot(b, "R", NO_PARENT, 0.0f, 0.0f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                w_pivot(b, "B", 0, 1.0f, 0.0f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                b.end();
                push("h31", b);
        }

        // h32: an out-of-range parent index (post-3.0) — the
        // stated-contract deviation fails the load.
        {
                ChunkBuild b;
                w_hierarchy_header(b, V41, "BadParent", 3, 0.0f, 0.0f, 0.0f);
                b.begin(W3D_CHUNK_PIVOTS);
                w_pivot(b, "R", NO_PARENT, 0.0f, 0.0f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                w_pivot(b, "A", 0, 1.0f, 0.0f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                w_pivot(b, "B", 5u, 2.0f, 0.0f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                b.end();
                push("h32", b);
        }

        // h33: ParentIdx 0xFFFFFFFE (not the root sentinel) — out of
        // range post-3.0, LOAD_ERROR.
        {
                ChunkBuild b;
                w_hierarchy_header(b, V41, "FeRoot", 2, 0.0f, 0.0f, 0.0f);
                b.begin(W3D_CHUNK_PIVOTS);
                w_pivot(b, "R", NO_PARENT, 0.0f, 0.0f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                w_pivot(b, "B", 0xFFFFFFFEu, 1.0f, 0.0f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                b.end();
                push("h33", b);
        }

        // h34: version 0.0 — pre-3.0 by the version comparison alone.
        {
                ChunkBuild b;
                w_hierarchy_header(b, 0u, "V0", 1, 0.0f, 0.0f, 0.0f);
                b.begin(W3D_CHUNK_PIVOTS);
                w_pivot(b, "OLDROOT", NO_PARENT, 1.0f, 1.0f, 1.0f, 0, 0, 0, 0.5f, 0.5f, 0.5f, 0.5f);
                b.end();
                push("h34", b);
        }

        // h35: pre-3.0 NumPivots 0xFFFFFFFF — the increment wraps to 0
        // and the NumPivots < 1 check fails the load (allocation-free).
        {
                ChunkBuild b;
                w_hierarchy_header(b, V21, "WrapCount", 0xFFFFFFFFu, 0.0f, 0.0f, 0.0f);
                b.begin(W3D_CHUNK_PIVOTS);
                b.end();
                push("h35", b);
        }

        // h36: a pivots chunk declaring 120 with only one struct in the
        // file — the second Read passes the clamp but the file read is
        // short (the file-end path, distinct from h31's clamp path).
        {
                ChunkBuild b;
                w_hierarchy_header(b, V41, "LieSize", 2, 0.0f, 0.0f, 0.0f);
                b.begin(W3D_CHUNK_PIVOTS);
                w_pivot(b, "R", NO_PARENT, 0.0f, 0.0f, 0.0f, 0, 0, 0, 0, 0, 0, 1);
                // no end(): patch the declared size to the full 120
                ChunkBuild::put_u32(b.bytes.data() + 44 + 4, 120);
                push("h36", b);
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

// The length of a bounded C-string field (up to the first NUL or the
// field's end).
static size_t cstr_len(const char *field, size_t cap)
{
        size_t len = 0;
        while (len < cap && field[len] != 0)
                len++;
        return len;
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

// The dump: everything the load path leaves behind.
static void dump_tree(const HTreeClass &tree, FILE *out, const char *id)
{
        // the header line: the decoded version and center, the name, the
        // declared pivot count and the member count (the pre-3.0 bump)
        fprintf(out, "hdr %s %s %s %u %d %s %s %s\n", id, u32hex(tree.Header.Version).c_str(),
                q_bytes((const uint8_t *)tree.Get_Name(), cstr_len(tree.Get_Name(), W3D_NAME_LEN)).c_str(),
                tree.Header.NumPivots, tree.Num_Pivots(), f32hex(tree.Header.Center.X).c_str(),
                f32hex(tree.Header.Center.Y).c_str(), f32hex(tree.Header.Center.Z).c_str());

        // the pivots
        for (int i = 0; i < tree.Num_Pivots(); i++) {
                const PivotClass &p = tree.Pivot[i];
                int parent = -1;
                if (p.Parent != nullptr) {
                        parent = (int)(p.Parent - tree.Pivot);
                }
                std::string base;
                for (int r = 0; r < 3; r++) {
                        base += f32hex(p.BaseTransform.Row[r].X);
                        base += f32hex(p.BaseTransform.Row[r].Y);
                        base += f32hex(p.BaseTransform.Row[r].Z);
                        base += f32hex(p.BaseTransform.Row[r].W);
                }
                fprintf(out, "piv %s %d %s %d %d %d %s\n", id, i,
                        q_bytes((const uint8_t *)p.Name, cstr_len(p.Name, W3D_NAME_LEN)).c_str(), parent, p.Index,
                        p.IsVisible ? 1 : 0, base.c_str());
        }

        // Get_Bone_Index probes: every pivot's own name (first-match pins
        // duplicates), "RootTransform", the ASCII lowercase of pivot 1's
        // name when the tree has one, and a fixed miss.
        for (int i = 0; i < tree.Num_Pivots(); i++) {
                const char *pname = tree.Pivot[i].Name;
                size_t nlen = cstr_len(pname, W3D_NAME_LEN);
                fprintf(out, "getbone %s %s %d\n", id, q_bytes((const uint8_t *)pname, nlen).c_str(),
                        tree.Get_Bone_Index(pname));
        }
        fprintf(out, "getbone %s RootTransform %d\n", id, tree.Get_Bone_Index("RootTransform"));
        if (tree.Num_Pivots() > 1 && cstr_len(tree.Pivot[1].Name, W3D_NAME_LEN) < W3D_NAME_LEN) {
                // The lowercase probe is skipped when pivot 1's name fills
                // its whole field: the original's stricmp would compare
                // past the 16 bytes into the Parent pointer — a heap-layout
                // result that is not reproducible. Well-terminated names
                // (the probe's explicit NUL ends both sides) stay pinned.
                char lower[W3D_NAME_LEN + 1];
                size_t nlen = cstr_len(tree.Pivot[1].Name, W3D_NAME_LEN);
                for (size_t k = 0; k < nlen; k++)
                        lower[k] = (char)ascii_tolower((unsigned char)tree.Pivot[1].Name[k]);
                lower[nlen] = 0;
                fprintf(out, "getbone %s %s %d\n", id, q_bytes((const uint8_t *)lower, nlen).c_str(),
                        tree.Get_Bone_Index(lower));
        }
        fprintf(out, "getbone %s nomatch %d\n", id, tree.Get_Bone_Index("nomatch"));

        // Get_Parent_Index for every pivot
        for (int i = 0; i < tree.Num_Pivots(); i++) {
                fprintf(out, "getparent %s %d %d\n", id, i, tree.Get_Parent_Index(i));
        }
}

} // namespace

// --- main -------------------------------------------------------------------------

int main(int argc, char **argv)
{
        if (argc != 2) {
                fprintf(stderr, "usage: gen_htree_golden <output-file>\n");
                return 1;
        }

        FILE *out = fopen(argv[1], "w");
        if (!out) {
                fprintf(stderr, "error: cannot open %s for writing\n", argv[1]);
                return 1;
        }

        fprintf(out, "# gen_htree_golden output — the W3D hierarchy tree corpus (P0)\n");
        fprintf(out, "# generated by tools/differential/gen_htree_golden.cpp; do not edit\n");

        for (const Corpus &c : build_corpora()) {
                SrcFile file;
                file.data = c.bytes.data();
                file.len = c.bytes.size();
                ChunkLoadClass cload(&file);

                fprintf(out, "file %s %s\n", c.id, hex_str(c.bytes).c_str());

                HTreeClass tree;
                int error = tree.Load_W3D(cload);
                if (error == HTreeClass::OK) {
                        fprintf(out, "ok %s\n", c.id);
                        dump_tree(tree, out, c.id);
                } else {
                        fprintf(out, "err %s\n", c.id);
                }
        }

        fclose(out);
        return 0;
}
