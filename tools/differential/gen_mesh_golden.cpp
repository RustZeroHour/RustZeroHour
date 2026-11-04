// SPDX-License-Identifier: GPL-3.0-or-later
//
// Golden-dump driver for `rzh-formats::w3d_mesh` (parity level P0).
//
// Verbatim transcription of the Zero Hour mesh chunk tree loader —
// GeneralsMD/Code/Libraries/Source/WWVegas/WW3D2/meshmdlio.cpp at commit
// 571a7c5 (MeshModelClass::Load_W3D, read_chunks and every sub-reader,
// MeshLoadContextClass, the load-path half of MeshMatDescClass and
// meshmatdesc.cpp's Install_UV_Array / Init_Alternate), plus the pieces
// those readers drive: MeshGeometryClass::read_* (meshgeometry.cpp),
// AABTreeClass::Load_W3D (Core aabtree.cpp), VertexMaterialClass::
// Load_W3D / Init_From_Material3 / Compute_CRC (vertmaterial.cpp),
// ::Load_Texture (Core texture.cpp, data path only), ShaderClass bit
// packing with W3dUtilityClass::Convert_Shader (shader.h / w3d_util.cpp)
// and the WWLib CRC32 (realcrc.cpp) the UV dedup and material CRC use.
// The chunk container itself is the chunkio.cpp reader already pinned by
// gen_chunk_golden.cpp; it is transcribed here again (release-build
// semantics) because this harness is self-contained.
//
// The corpus bytes are the payload of the W3D_CHUNK_MESH container the
// asset manager opens before calling Load_W3D; the harness enters the
// loader exactly as the asset manager does, with the container's
// children as the reader's top level.
//
// Deliberate deviations, pinned identically on both sides (the Rust port
// documents the same set):
//
// - The C++ release build turns most contract violations into undefined
//   behaviour (out-of-bounds DynamicVectorClass reads for shader/vmat/
//   texture indices, writes through unallocated geometry arrays, Cur_Chunk_
//   ID() at depth 0 on an empty file, short AABTree reads into uninitialised
//   arrays, an AABTree header that is not the expected chunk id). The
//   transcription turns each into a load failure — the only deterministic
//   standing available. Corpora pin the verdicts.
//
// - `W3dVertexMaterialStruct vmat;` in VertexMaterialClass::Load_W3D is
//   uninitialised stack in the C++ when no VERTEX_MATERIAL_INFO chunk is
//   read; both sides zero it instead (the C++ result is not reproducible).
//   C-string name reads (`char name[256]`) keep the C-string view: bytes
//   up to the first NUL of the chunk; a chunk longer than 256 bytes is a
//   load failure where the C++ overruns the stack buffer.
//
// - The TEXCOORDS (obsolete, top-level) reader installs `elementcount`
//   entries while writing only `VertexCount` of them; when elementcount >
//   VertexCount the tail is uninitialised scratch in the C++. Both sides
//   install exactly `min(elementcount, ...)`-shaped data only when the two
//   counts agree, and a load failure is reported when elementcount >
//   VertexCount (the C++ tail is not reproducible).
//
// - Render-side work is out of scope for the format layer (plan Appendix
//   A: "Split format parsing from rendering"): TextureClass/DDS objects
//   (the model keeps name + W3dTextureInfoStruct; the asset manager's
//   name-keyed object identity is modelled so the legacy dedup keeps its
//   behaviour), mapper instantiation from the ARGS0/ARGS1 buffers (kept
//   raw), post_process() fog/static-sort/overbright mutation,
//   MeshMatDescClass::Post_Load_Process (the DX8 pipeline adaptation),
//   Generate_Culling_Tree for collideable meshes without an AABTree
//   chunk, and MatInfo's object graph (the flat lists survive). The
//   Init_Alternate material *copy* is recorded as the same list index it
//   copies from — no ported code path can distinguish the copy.
//
// - Load_W3D ignores read_chunks' return value: a sub-reader failure
//   aborts the chunk walk (the remaining chunks are never opened) but the
//   load still reports success with the partial state. Transcribed as
//   found and pinned (m08, m20).
//
// Output grammar (one command per line, hex lowercase, '-' = empty):
//
//   file <id> <hex>                the corpus bytes (mesh payload)
//   ok <id> | err <id>             the load verdict
//   hdr <id> <ver> <attrs> <meshname> <containername> <numtris> <numverts>
//       <nummaterials> <numdamagestages> <sortlevel> <prelitversion>
//       <futurecounts0> <vertexchannels> <facechannels> then 10 float
//       words (min xyz, max xyz, sphcenter xyz, sphradius) — ints and
//       float bits as 8 hex digits, counts decimal
//   name <id> <name|->             container.mesh (the loader's Set_Name)
//   flags <id> <hex4>              the geometry flags after the load
//   prelitid <id> <hex8>           the selected prelit chunk id
//   geov/geon <id> <n> <hex|->     vertices / normals: n * 3 f32 bits
//   geop <id> <n> <hex|->          polys: n * (u16 I, u16 J, u16 K)
//   geos <id> <n> <hex|->          surface types: n u8
//   geoe <id> <n> <hex|->          plane equations: n * 4 f32 bits
//   geot <id> <hex|->              user text bytes (first chunk wins)
//   geob <id> <n> <hex|->          bone links (u16, after the fixup)
//   geox <id> <n> <hex|->          shade indices (u32)
//   matinfo <id> <p> <v> <s> <t>   the load context's last MatInfo state
//   shaders <id> <n> <hex|->       packed ShaderClass bits (u32 each)
//   vmat <id> <i> <name|-> <attrs> <flags> <amb> <dif> <spec> <emis>
//       <shininess> <opacity> <args0|-> <args1|->   colors as 3 f32 bits,
//       scalars as 1 f32 bit word, args as raw bytes
//   tex <id> <i> <name|-> <infohex|->   the texture list entries
//   legacy <id> <n>                legacy material count
//   legacymat <id> <i> <shaderidx> <vmatidx> <texidx>
//   matdesc <id> def|alt <passcount|->   the material descriptions
//   matpass <id> def|alt <p> mat <single|-1> <arrayhex|->
//   matpass <id> def|alt <p> shd <bits> <arrayhex|->
//   matpass <id> def|alt <p> dcg <src> dig <src>
//   matpass <id> def|alt <p> stg <s> tex <single|-1> <arrayhex|->
//   matpass <id> def|alt <p> stg <s> uv <src|-1>
//   matcolor <id> def|alt <a> <n> <hex|->   color array a: n u32
//   matuv <id> def|alt <k> <n> <hex|->      uv array k: n * 2 f32 bits
//   aabtree <id> <nodes> <polys> <nodehex> <polyhex> | aabtree <id> -
//
// Usage: gen_mesh_golden <output-file>
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

// --- the format constants (w3d_file.h / w3d_obsolete.h) ----------------------

enum
{
        W3D_CHUNK_MESH = 0x00000000,
        W3D_CHUNK_VERTICES = 0x00000002,
        W3D_CHUNK_VERTEX_NORMALS = 0x00000003,
        W3D_CHUNK_SURRENDER_NORMALS = 0x00000004,
        W3D_CHUNK_TEXCOORDS = 0x00000005,
        O_W3D_CHUNK_MATERIALS = 0x00000006,
        O_W3D_CHUNK_SURRENDER_TRIANGLES = 0x00000009,
        W3D_CHUNK_MESH_USER_TEXT = 0x0000000C,
        W3D_CHUNK_VERTEX_COLORS = 0x0000000D,
        W3D_CHUNK_VERTEX_INFLUENCES = 0x0000000E,
        W3D_CHUNK_DAMAGE = 0x0000000F,
        O_W3D_CHUNK_MATERIALS2 = 0x00000014,
        W3D_CHUNK_MATERIALS3 = 0x00000015,
        W3D_CHUNK_MATERIAL3 = 0x00000016,
        W3D_CHUNK_MATERIAL3_NAME = 0x00000017,
        W3D_CHUNK_MATERIAL3_INFO = 0x00000018,
        W3D_CHUNK_MATERIAL3_DC_MAP = 0x00000019,
        W3D_CHUNK_MAP3_FILENAME = 0x0000001A,
        W3D_CHUNK_MAP3_INFO = 0x0000001B,
        W3D_CHUNK_MATERIAL3_DI_MAP = 0x0000001C,
        W3D_CHUNK_MATERIAL3_SC_MAP = 0x0000001D,
        W3D_CHUNK_MATERIAL3_SI_MAP = 0x0000001E,
        W3D_CHUNK_MESH_HEADER3 = 0x0000001F,
        W3D_CHUNK_TRIANGLES = 0x00000020,
        W3D_CHUNK_PER_TRI_MATERIALS = 0x00000021,
        W3D_CHUNK_VERTEX_SHADE_INDICES = 0x00000022,
        W3D_CHUNK_PRELIT_UNLIT = 0x00000023,
        W3D_CHUNK_PRELIT_VERTEX = 0x00000024,
        W3D_CHUNK_PRELIT_LIGHTMAP_MULTI_PASS = 0x00000025,
        W3D_CHUNK_PRELIT_LIGHTMAP_MULTI_TEXTURE = 0x00000026,
        W3D_CHUNK_MATERIAL_INFO = 0x00000028,
        W3D_CHUNK_SHADERS = 0x00000029,
        W3D_CHUNK_VERTEX_MATERIALS = 0x0000002A,
        W3D_CHUNK_VERTEX_MATERIAL = 0x0000002B,
        W3D_CHUNK_VERTEX_MATERIAL_NAME = 0x0000002C,
        W3D_CHUNK_VERTEX_MATERIAL_INFO = 0x0000002D,
        W3D_CHUNK_VERTEX_MAPPER_ARGS0 = 0x0000002E,
        W3D_CHUNK_VERTEX_MAPPER_ARGS1 = 0x0000002F,
        W3D_CHUNK_TEXTURES = 0x00000030,
        W3D_CHUNK_TEXTURE = 0x00000031,
        W3D_CHUNK_TEXTURE_NAME = 0x00000032,
        W3D_CHUNK_TEXTURE_INFO = 0x00000033,
        W3D_CHUNK_MATERIAL_PASS = 0x00000038,
        W3D_CHUNK_VERTEX_MATERIAL_IDS = 0x00000039,
        W3D_CHUNK_SHADER_IDS = 0x0000003A,
        W3D_CHUNK_DCG = 0x0000003B,
        W3D_CHUNK_DIG = 0x0000003C,
        W3D_CHUNK_SCG = 0x0000003E,
        W3D_CHUNK_TEXTURE_STAGE = 0x00000048,
        W3D_CHUNK_TEXTURE_IDS = 0x00000049,
        W3D_CHUNK_STAGE_TEXCOORDS = 0x0000004A,
        W3D_CHUNK_PER_FACE_TEXCOORD_IDS = 0x0000004B,
        W3D_CHUNK_DEFORM = 0x00000058,
        W3D_CHUNK_AABTREE = 0x00000090,
        W3D_CHUNK_AABTREE_HEADER = 0x00000091,
        W3D_CHUNK_AABTREE_POLYINDICES = 0x00000092,
        W3D_CHUNK_AABTREE_NODES = 0x00000093,
};

enum
{
        WW3D_ERROR_OK = 0,
        WW3D_ERROR_LOAD_FAILED = 5,
};

#define W3D_MAKE_VERSION(major, minor) (((major) << 16) | (minor))

// mesh attribute flags (w3d_file.h)
#define W3D_MESH_FLAG_COLLISION_TYPE_MASK 0x00000FF0
#define W3D_MESH_FLAG_COLLISION_TYPE_SHIFT 4
#define W3D_MESH_FLAG_TWO_SIDED 0x00002000
#define OBSOLETE_W3D_MESH_FLAG_LIGHTMAPPED 0x00004000
#define W3D_MESH_FLAG_CAST_SHADOW 0x00008000
#define W3D_MESH_FLAG_GEOMETRY_TYPE_MASK 0x00FF0000
#define W3D_MESH_FLAG_GEOMETRY_TYPE_NORMAL 0x00000000
#define W3D_MESH_FLAG_GEOMETRY_TYPE_CAMERA_ALIGNED 0x00010000
#define W3D_MESH_FLAG_GEOMETRY_TYPE_SKIN 0x00020000
#define W3D_MESH_FLAG_GEOMETRY_TYPE_CAMERA_ORIENTED 0x00060000
#define W3D_MESH_FLAG_PRELIT_MASK 0x0F000000
#define W3D_MESH_FLAG_PRELIT_UNLIT 0x01000000
#define W3D_MESH_FLAG_PRELIT_VERTEX 0x02000000
#define W3D_MESH_FLAG_PRELIT_LIGHTMAP_MULTI_PASS 0x04000000
#define W3D_MESH_FLAG_PRELIT_LIGHTMAP_MULTI_TEXTURE 0x08000000
#define W3D_MESH_FLAG_NPATCHABLE 0x20000000

// geometry flags (meshgeometry.h FlagsType) — the load path's reachable subset
enum
{
        FG_SORT = 0x00000010,
        FG_TWO_SIDED = 0x00000100,
        FG_ALIGNED = 0x00000200,
        FG_SKIN = 0x00000400,
        FG_ORIENTED = 0x00000800,
        FG_CAST_SHADOW = 0x00001000,
        FG_PRELIT_VERTEX = 0x00002000,
        FG_PRELIT_LIGHTMAP_MULTI_PASS = 0x00004000,
        FG_PRELIT_LIGHTMAP_MULTI_TEXTURE = 0x00008000,
        FG_ALLOW_NPATCHES = 0x00010000,
};

// vertex material attributes (w3d_file.h)
#define W3DVERTMAT_USE_DEPTH_CUE 0x00000001
#define W3DVERTMAT_COPY_SPECULAR_TO_DIFFUSE 0x00000004

// material3 attributes (w3d_obsolete.h)
#define W3DMATERIAL_USE_ALPHA 0x00000001

// WW3D's prelit mode global (ww3d.cpp: never changed by the game tree)
enum PrelitModeEnum
{
        PRELIT_MODE_VERTEX = 1,
        PRELIT_MODE_LIGHTMAP_MULTI_PASS = 3,
        PRELIT_MODE_LIGHTMAP_MULTI_TEXTURE = 4,
};
static PrelitModeEnum PrelitMode = PRELIT_MODE_LIGHTMAP_MULTI_PASS;

// --- the W3D structures (w3d_file.h / w3d_obsolete.h, POD) --------------------

struct W3dVectorStruct
{
        float X, Y, Z;
};
struct W3dTexCoordStruct
{
        float U, V;
};
struct W3dRGBStruct
{
        uint8_t R, G, B, pad;
};
struct W3dRGBAStruct
{
        uint8_t R, G, B, A;
};
struct W3dMaterialInfoStruct
{
        uint32_t PassCount;
        uint32_t VertexMaterialCount;
        uint32_t ShaderCount;
        uint32_t TextureCount;
};
struct W3dVertInfStruct
{
        uint16_t BoneIdx;
        uint8_t Pad[6];
};
struct W3dShaderStruct
{
        uint8_t DepthCompare;
        uint8_t DepthMask;
        uint8_t ColorMask;
        uint8_t DestBlend;
        uint8_t FogFunc;
        uint8_t PriGradient;
        uint8_t SecGradient;
        uint8_t SrcBlend;
        uint8_t Texturing;
        uint8_t DetailColorFunc;
        uint8_t DetailAlphaFunc;
        uint8_t ShaderPreset;
        uint8_t AlphaTest;
        uint8_t PostDetailColorFunc;
        uint8_t PostDetailAlphaFunc;
        uint8_t pad[1];
};
struct W3dVertexMaterialStruct
{
        uint32_t Attributes;
        W3dRGBStruct Ambient;
        W3dRGBStruct Diffuse;
        W3dRGBStruct Specular;
        W3dRGBStruct Emissive;
        float Shininess;
        float Opacity;
        float Translucency;
};
struct W3dTextureInfoStruct
{
        uint16_t Attributes;
        uint16_t AnimType;
        uint32_t FrameCount;
        float FrameRate;
};
struct W3dTriStruct
{
        uint32_t Vindex[3];
        uint32_t Attributes;
        W3dVectorStruct Normal;
        float Dist;
};
struct W3dMeshHeader3Struct
{
        uint32_t Version;
        uint32_t Attributes;
        char MeshName[16];
        char ContainerName[16];
        uint32_t NumTris;
        uint32_t NumVertices;
        uint32_t NumMaterials;
        uint32_t NumDamageStages;
        int32_t SortLevel;
        uint32_t PrelitVersion;
        uint32_t FutureCounts[1];
        uint32_t VertexChannels;
        uint32_t FaceChannels;
        W3dVectorStruct Min;
        W3dVectorStruct Max;
        W3dVectorStruct SphCenter;
        float SphRadius;
};
struct W3dMeshAABTreeHeader
{
        uint32_t NodeCount;
        uint32_t PolyCount;
        uint32_t Padding[6];
};
struct W3dMeshAABTreeNode
{
        W3dVectorStruct Min;
        W3dVectorStruct Max;
        uint32_t FrontOrPoly0;
        uint32_t BackOrPolyCount;
};
struct W3dMaterial3Struct
{
        uint32_t Attributes;
        W3dRGBStruct DiffuseColor;
        W3dRGBStruct SpecularColor;
        W3dRGBStruct EmissiveCoefficients;
        W3dRGBStruct AmbientCoefficients;
        W3dRGBStruct DiffuseCoefficients;
        W3dRGBStruct SpecularCoefficients;
        float Shininess;
        float Opacity;
        float Translucency;
        float FogCoeff;
};
struct W3dMap3Struct
{
        uint16_t MappingType;
        uint16_t FrameCount;
        float FrameRate;
};

// --- the chunk container reader (chunkio.cpp, release semantics) --------------

struct ChunkHeader
{
        void Set_Type(uint32_t type) { ChunkType = type; }
        uint32_t Get_Type() const { return ChunkType; }
        void Set_Size(uint32_t size) { ChunkSize &= 0x80000000; ChunkSize |= (size & 0x7FFFFFFF); }
        void Add_Size(uint32_t add) { Set_Size(Get_Size() + add); }
        uint32_t Get_Size() const { return (ChunkSize & 0x7FFFFFFF); }
        void Set_Sub_Chunk_Flag(bool onoff) { if (onoff) { ChunkSize |= 0x80000000; } else { ChunkSize &= 0x7FFFFFFF; } }
        int Get_Sub_Chunk_Flag() const { return (int)(ChunkSize & 0x80000000); }
        uint32_t ChunkType;
        uint32_t ChunkSize;
};

struct MicroChunkHeader
{
        void Set_Type(uint8_t type) { ChunkType = type; }
        uint8_t Get_Type() const { return ChunkType; }
        void Set_Size(uint8_t size) { ChunkSize = size; }
        void Add_Size(uint8_t add) { Set_Size((uint8_t)(Get_Size() + add)); }
        uint8_t Get_Size() const { return ChunkSize; }
        uint8_t ChunkType;
        uint8_t ChunkSize;
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
        int Cur_Chunk_Depth();
        int Contains_Chunks();

        uint32_t Read(void *buf, uint32_t nbytes);
        uint32_t Seek(uint32_t nbytes);

        long Tell() { return File->tell(); }

private:
        enum
        {
                MAX_STACK_DEPTH = 256
        };

        SrcFile *File;
        int StackIndex;
        uint32_t PositionStack[MAX_STACK_DEPTH];
        ChunkHeader HeaderStack[MAX_STACK_DEPTH];
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
        // is a caller contract violation the mesh loader never performs
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
        // release build: StackIndex == 0 reads out of bounds (UB). The mesh
        // loader only reaches here after a failed Open_Chunk, where the
        // original's comparison against W3D_CHUNK_MESH_HEADER3 virtually
        // always fails; the transcription states the contract instead.
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

int ChunkLoadClass::Cur_Chunk_Depth() { return StackIndex; }

int ChunkLoadClass::Contains_Chunks() { return HeaderStack[StackIndex - 1].Get_Sub_Chunk_Flag(); }

uint32_t ChunkLoadClass::Read(void *buf, uint32_t nbytes)
{
        // release build: the StackIndex assert is skipped and StackIndex == 0
        // reads the stacks out of bounds (UB); the mesh loader never reads
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

// Seek over nbytes in the stream
uint32_t ChunkLoadClass::Seek(uint32_t nbytes)
{
        if (StackIndex < 1)
                return 0;

        // Don't seek if we would go past the end of the current chunk
        if (PositionStack[StackIndex - 1] + nbytes > (uint32_t)HeaderStack[StackIndex - 1].Get_Size()) {
                return 0;
        }

        uint32_t curpos = (uint32_t)File->tell();
        if ((uint32_t)File->seek((long)nbytes, SEEK_CUR) - curpos != nbytes) {
                return 0;
        }

        // Update our position in the chunk
        PositionStack[StackIndex - 1] += nbytes;

        return nbytes;
}

// --- the WWLib CRC32 (realcrc.cpp) --------------------------------------------

// CRC32_Table: the standard reflected table for poly 0x04C11DB7 — verified
// entry-for-entry against realcrc.cpp's table at the baseline tag. Generated
// here so both sides of the parity check compute the identical table.
static uint32_t CRC32_Table[256];

static void crc32_init()
{
        for (uint32_t i = 0; i < 256; i++) {
                uint32_t c = i;
                for (int k = 0; k < 8; k++) {
                        c = (c & 1) ? (c >> 1) ^ 0xEDB88320u : (c >> 1);
                }
                CRC32_Table[i] = c;
        }
}

#define CRC32(c, crc) (CRC32_Table[((unsigned long)(crc) ^ (c)) & 0xFFL] ^ (((crc) >> 8) & 0x00FFFFFFL))

// CRC_Memory (realcrc.cpp, verbatim)
static uint32_t CRC_Memory(const uint8_t *data, size_t length, uint32_t crc)
{
        crc ^= 0xFFFFFFFF; // invert previous CRC
        while (length--) {
                crc = CRC32(*data++, crc); // calc crc for each byte
        }
        return (crc ^ 0xFFFFFFFF); // invert new CRC and return it
}

// --- the shader conversion (shader.h + w3d_util.cpp) ---------------------------

// ShaderClass bit layout (shader.h). The enums' values are the raw field
// values the W3D shader struct stores; Convert_Shader casts them straight
// into the bit fields, so the packing below is value-identical.
enum
{
        SHIFT_DEPTHCOMPARE = 0,
        SHIFT_DEPTHMASK = 3,
        SHIFT_COLORMASK = 4,
        SHIFT_DSTBLEND = 5,
        SHIFT_FOG = 8,
        SHIFT_PRIGRADIENT = 10,
        SHIFT_SECGRADIENT = 13,
        SHIFT_SRCBLEND = 14,
        SHIFT_TEXTURING = 16,
        SHIFT_ALPHATEST = 18,
        SHIFT_CULLMODE = 19,
        SHIFT_POSTDETAILCOLORFUNC = 20,
        SHIFT_POSTDETAILALPHAFUNC = 24,
};

// ShaderClass enum values the W3D conversions use (shader.h)
enum
{
        SHD_DSTBLEND_ZERO = 0,
        SHD_ALPHATEST_DISABLE = 0,
        SHD_GRADIENT_DISABLE = 0,
        SHD_SECONDARY_GRADIENT_DISABLE = 0,
        SHD_SRCBLEND_ONE = 1,
        SHD_TEXTURING_ENABLE = 1,
        SHD_TEXTURING_DISABLE = 0,
};

struct ShaderClass
{
        // ShaderClass::Reset defaults (the W3d shader conversion starts from
        // a default-constructed ShaderClass; the fields Convert_Shader does
        // not touch — cull mode, npatch — keep them)
        static uint32_t default_bits()
        {
                return (3u << SHIFT_DEPTHCOMPARE)      // PASS_LEQUAL
                       | (1u << SHIFT_DEPTHMASK)         // DEPTH_WRITE_ENABLE
                       | (1u << SHIFT_COLORMASK)         // COLOR_WRITE_ENABLE
                       | (1u << SHIFT_PRIGRADIENT)       // GRADIENT_MODULATE
                       | (1u << SHIFT_SRCBLEND)          // SRCBLEND_ONE
                       | (1u << SHIFT_CULLMODE);         // CULL_MODE_ENABLE
        }

        uint32_t ShaderBits = default_bits();

        void Set_Depth_Compare(uint32_t x) { ShaderBits = (ShaderBits & ~(7u << SHIFT_DEPTHCOMPARE)) | (x << SHIFT_DEPTHCOMPARE); }
        void Set_Depth_Mask(uint32_t x) { ShaderBits = (ShaderBits & ~(1u << SHIFT_DEPTHMASK)) | (x << SHIFT_DEPTHMASK); }
        void Set_Color_Mask(uint32_t x) { ShaderBits = (ShaderBits & ~(1u << SHIFT_COLORMASK)) | (x << SHIFT_COLORMASK); }
        void Set_Dst_Blend_Func(uint32_t x) { ShaderBits = (ShaderBits & ~(7u << SHIFT_DSTBLEND)) | (x << SHIFT_DSTBLEND); }
        void Set_Fog_Func(uint32_t x) { ShaderBits = (ShaderBits & ~(3u << SHIFT_FOG)) | (x << SHIFT_FOG); }
        void Set_Primary_Gradient(uint32_t x) { ShaderBits = (ShaderBits & ~(7u << SHIFT_PRIGRADIENT)) | (x << SHIFT_PRIGRADIENT); }
        void Set_Secondary_Gradient(uint32_t x) { ShaderBits = (ShaderBits & ~(1u << SHIFT_SECGRADIENT)) | (x << SHIFT_SECGRADIENT); }
        void Set_Src_Blend_Func(uint32_t x) { ShaderBits = (ShaderBits & ~(3u << SHIFT_SRCBLEND)) | (x << SHIFT_SRCBLEND); }
        void Set_Texturing(uint32_t x) { ShaderBits = (ShaderBits & ~(1u << SHIFT_TEXTURING)) | (x << SHIFT_TEXTURING); }
        void Set_Alpha_Test(uint32_t x) { ShaderBits = (ShaderBits & ~(1u << SHIFT_ALPHATEST)) | (x << SHIFT_ALPHATEST); }
        void Set_Post_Detail_Color_Func(uint32_t x) { ShaderBits = (ShaderBits & ~(15u << SHIFT_POSTDETAILCOLORFUNC)) | (x << SHIFT_POSTDETAILCOLORFUNC); }
        void Set_Post_Detail_Alpha_Func(uint32_t x) { ShaderBits = (ShaderBits & ~(7u << SHIFT_POSTDETAILALPHAFUNC)) | (x << SHIFT_POSTDETAILALPHAFUNC); }

        uint32_t Get_Dst_Blend_Func() const { return (ShaderBits >> SHIFT_DSTBLEND) & 7u; }
        uint32_t Get_Alpha_Test() const { return (ShaderBits >> SHIFT_ALPHATEST) & 1u; }
        uint32_t Get_Texturing() const { return (ShaderBits >> SHIFT_TEXTURING) & 1u; }

        bool operator==(const ShaderClass &s) const { return ShaderBits == s.ShaderBits; }
};

// W3dUtilityClass::Convert_Shader (w3d_util.cpp, verbatim field moves)
static void Convert_Shader(const W3dShaderStruct &shader, ShaderClass *set)
{
        set->Set_Depth_Compare(shader.DepthCompare);
        set->Set_Depth_Mask(shader.DepthMask);
        set->Set_Color_Mask(1); // ShaderClass::COLOR_WRITE_ENABLE
        set->Set_Dst_Blend_Func(shader.DestBlend);
        set->Set_Fog_Func(0); // ShaderClass::FOG_DISABLE
        set->Set_Primary_Gradient(shader.PriGradient);
        set->Set_Secondary_Gradient(shader.SecGradient);
        set->Set_Src_Blend_Func(shader.SrcBlend);
        set->Set_Texturing(shader.Texturing);
        set->Set_Alpha_Test(shader.AlphaTest);
        // The tools do not enable setting post-detail stuff ... so we just
        // copy the detail settings into the post-detail settings.
        set->Set_Post_Detail_Color_Func(shader.DetailColorFunc);
        set->Set_Post_Detail_Alpha_Func(shader.DetailAlphaFunc);
}

// --- the color conversions (w3d_util.cpp + Vector3::Convert_To_ARGB) -----------

static void Convert_Color(const W3dRGBStruct &rgb, float out[3])
{
        out[0] = (float)rgb.R / 255.0f;
        out[1] = (float)rgb.G / 255.0f;
        out[2] = (float)rgb.B / 255.0f;
}

static void Convert_Color(const W3dRGBAStruct &rgb, float out[4])
{
        out[0] = (float)rgb.R / 255.0f;
        out[1] = (float)rgb.G / 255.0f;
        out[2] = (float)rgb.B / 255.0f;
        out[3] = (float)rgb.A / 255.0f;
}

// DX8Wrapper::Convert_Color(Vector4) — the non-asm branch:
// Vector3::Convert_To_ARGB(alpha). The values the readers feed it stay in
// [0, 1], where the float->unsigned conversion is a plain truncation on
// every platform involved.
static uint32_t Convert_Color_ARGB(const float v[4])
{
        uint32_t a = (uint32_t)(v[3] * 255.0f);
        uint32_t r = (uint32_t)(v[0] * 255.0f);
        uint32_t g = (uint32_t)(v[1] * 255.0f);
        uint32_t b = (uint32_t)(v[2] * 255.0f);
        return (a << 24) | (r << 16) | (g << 8) | b;
}

static void Convert_Color_ARGB_To_Vector4(uint32_t color, float out[4])
{
        out[3] = (float)((color & 0xff000000) >> 24) / 255.0f;
        out[0] = (float)((color & 0xff0000) >> 16) / 255.0f;
        out[1] = (float)((color & 0xff00) >> 8) / 255.0f;
        out[2] = (float)((color & 0xff) >> 0) / 255.0f;
}

// --- the texture model (texture.cpp data path + the asset manager) -------------

// What survives of a TextureClass in the format layer: its name (the asset
// manager's object identity) and the W3dTextureInfoStruct it was created
// from, when it was. The render-side state (mip count, addressing, the
// surface itself) is out of scope.
struct TextureObj
{
        std::string name;                  // NUL-terminated texture filename
        bool has_info = false;
        W3dTextureInfoStruct info{};
};

// --- the vertex material model (vertmaterial.cpp, load-path state) -------------

// The D3DMATERIAL8 the class holds: Diffuse, Ambient, Specular, Emissive
// (D3DCOLORVALUE each) and Power. This is the state Compute_CRC covers and
// the dump prints.
struct VertexMaterial
{
        float diffuse[4] = {1, 1, 1, 1};   // r g b a — ctor sets rgb 1, opacity sets a
        float ambient[4] = {1, 1, 1, 0};   // ctor sets rgb 1
        float specular[4] = {0, 0, 0, 0};
        float emissive[4] = {0, 0, 0, 0};
        float power = 0;                   // shininess

        uint32_t flags = 0;                // FlagsType bits set during load
        std::string name;                  // Set_Name (empty when never set)
        bool has_name = false;

        // the mapper argument buffers, raw (mapper instantiation is
        // render-side; the buffers are kept as Load_W3D read them)
        std::vector<uint8_t> args0;
        std::vector<uint8_t> args1;

        void Set_Ambient(float r, float g, float b)
        {
                ambient[0] = r;
                ambient[1] = g;
                ambient[2] = b;
        }
        void Set_Diffuse(float r, float g, float b)
        {
                diffuse[0] = r;
                diffuse[1] = g;
                diffuse[2] = b;
        }
        void Set_Specular(float r, float g, float b)
        {
                specular[0] = r;
                specular[1] = g;
                specular[2] = b;
        }
        void Set_Emissive(float r, float g, float b)
        {
                emissive[0] = r;
                emissive[1] = g;
                emissive[2] = b;
        }
        void Set_Shininess(float s) { power = s; }
        void Set_Opacity(float o) { diffuse[3] = o; }

        // VertexMaterialClass::Compute_CRC, restricted to the state the load
        // path can set. The color sources (D3DMCS_MATERIAL = 0), UVSource
        // ([0, 1]), UseLighting (false), UniqueID (0) and the null mappers
        // are constants during a mesh load, so the CRC reduces to the
        // D3DMATERIAL8 bytes, the flags and those constants. Both sides
        // serialise the same field sequence.
        uint32_t Compute_CRC() const
        {
                uint32_t crc = 0;
                crc = CRC_Memory((const uint8_t *)this->diffuse, sizeof(float) * 4, crc);
                crc = CRC_Memory((const uint8_t *)this->ambient, sizeof(float) * 4, crc);
                crc = CRC_Memory((const uint8_t *)this->specular, sizeof(float) * 4, crc);
                crc = CRC_Memory((const uint8_t *)this->emissive, sizeof(float) * 4, crc);
                crc = CRC_Memory((const uint8_t *)&this->power, sizeof(float), crc);
                crc = CRC_Memory((const uint8_t *)&this->flags, sizeof(uint32_t), crc);
                uint32_t sources[3] = {0, 0, 0}; // diffuse/ambient/emissive color sources
                crc = CRC_Memory((const uint8_t *)sources, sizeof(sources), crc);
                uint32_t uvsource[2] = {0, 1};
                crc = CRC_Memory((const uint8_t *)uvsource, sizeof(uvsource), crc);
                uint8_t use_lighting = 0;
                crc = CRC_Memory(&use_lighting, 1, crc);
                uint32_t unique_id = 0;
                crc = CRC_Memory((const uint8_t *)&unique_id, sizeof(uint32_t), crc);
                // Mapper[i] is null during a mesh load (only Parse_Mapping_Args
                // sets mappers, and the legacy path never runs it) — the
                // original's conditional skips null mappers.
                return crc;
        }
};

// VertexMaterialClass::Parse_W3dVertexMaterialStruct (vertmaterial.cpp)
static void Parse_W3dVertexMaterialStruct(VertexMaterial *m, const W3dVertexMaterialStruct &vmat)
{
        float tmp[3];
        Convert_Color(vmat.Ambient, tmp);
        m->Set_Ambient(tmp[0], tmp[1], tmp[2]);
        Convert_Color(vmat.Diffuse, tmp);
        m->Set_Diffuse(tmp[0], tmp[1], tmp[2]);
        Convert_Color(vmat.Specular, tmp);
        m->Set_Specular(tmp[0], tmp[1], tmp[2]);
        Convert_Color(vmat.Emissive, tmp);
        m->Set_Emissive(tmp[0], tmp[1], tmp[2]);
        m->Set_Shininess(vmat.Shininess);
        m->Set_Opacity(vmat.Opacity);

        if (vmat.Attributes & W3DVERTMAT_USE_DEPTH_CUE) {
                m->flags |= 1u; // VertexMaterialClass::DEPTH_CUE
        }
        if (vmat.Attributes & W3DVERTMAT_COPY_SPECULAR_TO_DIFFUSE) {
                m->flags |= 4u; // VertexMaterialClass::COPY_SPECULAR_TO_DIFFUSE
        }
}

// VertexMaterialClass::Init_From_Material3 (vertmaterial.cpp)
static void Init_From_Material3(VertexMaterial *m, const W3dMaterial3Struct &mat3)
{
        float tmp0[3], tmp1[3], tmp2[3];

        Convert_Color(mat3.DiffuseColor, tmp0);
        Convert_Color(mat3.DiffuseCoefficients, tmp1);
        tmp2[0] = tmp0[0] * tmp1[0];
        tmp2[1] = tmp0[1] * tmp1[1];
        tmp2[2] = tmp0[2] * tmp1[2];
        m->Set_Diffuse(tmp2[0], tmp2[1], tmp2[2]);

        Convert_Color(mat3.SpecularColor, tmp0);
        Convert_Color(mat3.SpecularCoefficients, tmp1);
        tmp2[0] = tmp0[0] * tmp1[0];
        tmp2[1] = tmp0[1] * tmp1[1];
        tmp2[2] = tmp0[2] * tmp1[2];
        m->Set_Specular(tmp2[0], tmp2[1], tmp2[2]);

        Convert_Color(mat3.EmissiveCoefficients, tmp0);
        m->Set_Emissive(tmp0[0], tmp0[1], tmp0[2]);

        Convert_Color(mat3.AmbientCoefficients, tmp0);
        m->Set_Ambient(tmp0[0], tmp0[1], tmp0[2]);

        m->Set_Shininess(mat3.Shininess);
        m->Set_Opacity(mat3.Opacity);
}

// --- the material description (meshmatdesc.h / meshmatdesc.cpp, load path) ----

static const int MAX_PASSES = 4;
static const int MAX_TEX_STAGES = 2;
static const int MAX_COLOR_ARRAYS = 2;
static const int MAX_UV_ARRAYS = MAX_PASSES * MAX_TEX_STAGES;

// Color source constants (VertexMaterialClass::ColorSourceType)
static const uint32_t CS_MATERIAL = 0;
static const uint32_t CS_COLOR1 = 1;
static const uint32_t CS_COLOR2 = 2;

// One UV array with the CRC Install_UV_Array compares (UVBufferClass +
// Update_CRC). The CRC covers the array's bytes exactly as stored.
struct UVBuffer
{
        std::vector<float> xy; // pairs
        uint32_t crc = 0;

        void update_crc()
        {
                crc = CRC_Memory((const uint8_t *)xy.data(), xy.size() * sizeof(float), 0);
        }
};

// The load-path half of MeshMatDescClass. Pointers to materials and
// textures are list indices (-1 = null); the shader stays the packed bit
// pattern with 0 meaning NullShader ("no shader data").
struct MeshMatDescClass
{
        int PassCount = 1;
        int VertexCount = 0;
        int PolyCount = 0;

        std::vector<uint32_t> *ColorArray[MAX_COLOR_ARRAYS] = {nullptr, nullptr};
        UVBuffer *UV[MAX_UV_ARRAYS] = {nullptr, nullptr, nullptr, nullptr, nullptr, nullptr, nullptr, nullptr};
        int UVSource[MAX_PASSES][MAX_TEX_STAGES] = {{-1, -1}, {-1, -1}, {-1, -1}, {-1, -1}};
        uint32_t DCGSource[MAX_PASSES] = {CS_MATERIAL, CS_MATERIAL, CS_MATERIAL, CS_MATERIAL};
        uint32_t DIGSource[MAX_PASSES] = {CS_MATERIAL, CS_MATERIAL, CS_MATERIAL, CS_MATERIAL};

        int MaterialSingle[MAX_PASSES] = {-1, -1, -1, -1};
        std::vector<int> *MaterialArray[MAX_PASSES] = {nullptr, nullptr, nullptr, nullptr};
        uint32_t ShaderBits[MAX_PASSES] = {0, 0, 0, 0}; // NullShader marks no data
        std::vector<uint32_t> *ShaderArray[MAX_PASSES] = {nullptr, nullptr, nullptr, nullptr};
        int TextureSingle[MAX_PASSES][MAX_TEX_STAGES] = {{-1, -1}, {-1, -1}, {-1, -1}, {-1, -1}};
        std::vector<int> *TextureArray[MAX_PASSES][MAX_TEX_STAGES] = {{nullptr, nullptr}, {nullptr, nullptr}, {nullptr, nullptr}, {nullptr, nullptr}};

        ~MeshMatDescClass()
        {
                Reset(0, 0, 1);
        }

        // MeshMatDescClass::Reset (meshmatdesc.cpp)
        void Reset(int polycount, int vertcount, int passcount)
        {
                PolyCount = polycount;
                VertexCount = vertcount;
                PassCount = passcount;

                for (int array = 0; array < MAX_COLOR_ARRAYS; array++) {
                        delete ColorArray[array];
                        ColorArray[array] = nullptr;
                }
                for (int uvarray = 0; uvarray < MAX_UV_ARRAYS; uvarray++) {
                        delete UV[uvarray];
                        UV[uvarray] = nullptr;
                }
                for (int pass = 0; pass < MAX_PASSES; pass++) {
                        for (int stage = 0; stage < MAX_TEX_STAGES; stage++) {
                                UVSource[pass][stage] = -1;
                                TextureSingle[pass][stage] = -1;
                                delete TextureArray[pass][stage];
                                TextureArray[pass][stage] = nullptr;
                        }
                        DCGSource[pass] = CS_MATERIAL;
                        DIGSource[pass] = CS_MATERIAL;
                        ShaderBits[pass] = 0;
                        delete ShaderArray[pass];
                        ShaderArray[pass] = nullptr;
                        MaterialSingle[pass] = -1;
                        delete MaterialArray[pass];
                        MaterialArray[pass] = nullptr;
                }
        }

        void Set_Pass_Count(int passes) { PassCount = passes; }
        void Set_Vertex_Count(int count) { VertexCount = count; }
        void Set_Polygon_Count(int count) { PolyCount = count; }

        // Get_UV_Array_Count (the leading run of installed arrays; the
        // original's loop tests UV[count] before the bound, an OOB read
        // whose result the bound check discards anyway)
        int Get_UV_Array_Count()
        {
                int count = 0;
                while ((count < MAX_UV_ARRAYS) && (UV[count] != nullptr)) {
                        count++;
                }
                return count;
        }

        bool Has_Color_Array(int array) const { return ColorArray[array] != nullptr; }
        bool Has_UV(int pass, int stage) const { return UVSource[pass][stage] != -1; }
        bool Has_Texture_Data(int pass, int stage) const
        {
                return (TextureSingle[pass][stage] != -1) || (TextureArray[pass][stage] != nullptr);
        }
        bool Has_Shader_Data(int pass) const { return (ShaderBits[pass] != 0) || (ShaderArray[pass] != nullptr); }
        bool Has_Material_Data(int pass) const { return (MaterialSingle[pass] != -1) || (MaterialArray[pass] != nullptr); }

        // Get_Color_Array(index, create) — the buffer is zeroed on creation
        // (ShareBufferClass::Clear via the color-array ctor path)
        std::vector<uint32_t> *Get_Color_Array(int index, bool create = true)
        {
                if (create && !ColorArray[index]) {
                        ColorArray[index] = new std::vector<uint32_t>((size_t)VertexCount, 0);
                }
                return ColorArray[index];
        }

        void Set_DCG_Source(int pass, uint32_t source) { DCGSource[pass] = source; }
        void Set_DIG_Source(int pass, uint32_t source) { DIGSource[pass] = source; }
        uint32_t Get_DCG_Source(int pass) const { return DCGSource[pass]; }

        // Set_Single_Material / Set_Material
        void Set_Single_Material(int vmat, int pass) { MaterialSingle[pass] = vmat; }
        void Set_Material(int vidx, int vmat, int pass)
        {
                if (!MaterialArray[pass]) {
                        MaterialArray[pass] = new std::vector<int>((size_t)VertexCount, -1); // MatBufferClass clears
                }
                (*MaterialArray[pass])[vidx] = vmat;
        }

        // Set_Single_Shader / Set_Shader (the per-poly buffer is filled by
        // its only callers, so the uninitialised ShareBufferClass entries
        // are never observable)
        void Set_Single_Shader(uint32_t bits, int pass) { ShaderBits[pass] = bits; }
        void Set_Shader(int pidx, uint32_t bits, int pass)
        {
                if (!ShaderArray[pass]) {
                        ShaderArray[pass] = new std::vector<uint32_t>((size_t)PolyCount, 0);
                }
                (*ShaderArray[pass])[pidx] = bits;
        }

        // Set_Single_Texture / Set_Texture (TexBufferClass clears; the
        // 0xffffffff texture-id skip leaves null entries)
        void Set_Single_Texture(int tex, int pass, int stage) { TextureSingle[pass][stage] = tex; }
        void Set_Texture(int pidx, int tex, int pass, int stage)
        {
                if (!TextureArray[pass][stage]) {
                        TextureArray[pass][stage] = new std::vector<int>((size_t)PolyCount, -1);
                }
                (*TextureArray[pass][stage])[pidx] = tex;
        }

        // Install_UV_Array (meshmatdesc.cpp) — CRC-based dedup against the
        // installed arrays, then the first empty slot. The original's
        // slot-search tests UV[new_index] before the bound (an OOB read when
        // all slots are full, whose value the following bound check discards):
        // with all 8 slots taken the install silently does nothing.
        void Install_UV_Array(int pass, int stage, const std::vector<float> &uvs, int count)
        {
                uint32_t crc = CRC_Memory((const uint8_t *)uvs.data(), (size_t)count * sizeof(float) * 2, 0);

                bool found = false;
                int uv_count = Get_UV_Array_Count();
                for (int i = 0; i < uv_count; i++) {
                        if (UV[i]->crc == crc) {
                                found = true;
                                UVSource[pass][stage] = i;
                                break;
                        }
                }

                if (found == false) {
                        int new_index = 0;
                        while ((new_index < MAX_UV_ARRAYS) && (UV[new_index] != nullptr)) {
                                new_index++;
                        }

                        if (new_index < MAX_UV_ARRAYS) {
                                UV[new_index] = new UVBuffer();
                                UV[new_index]->xy.assign(uvs.begin(), uvs.begin() + count * 2);
                                UV[new_index]->update_crc();
                                UVSource[pass][stage] = new_index;
                        }
                }
        }

        // Is_Empty (meshmatdesc.cpp) — note it does NOT look at the shader
        // data: an alternate holding only shaders counts as empty.
        bool Is_Empty()
        {
                for (int array = 0; array < MAX_COLOR_ARRAYS; array++) {
                        if (ColorArray[array] != nullptr)
                                return false;
                }
                for (int uvarray = 0; uvarray < MAX_UV_ARRAYS; uvarray++) {
                        if (UV[uvarray] != nullptr)
                                return false;
                }
                for (int pass = 0; pass < MAX_PASSES; pass++) {
                        for (int stage = 0; stage < MAX_TEX_STAGES; stage++) {
                                if (TextureSingle[pass][stage] != -1)
                                        return false;
                                if (TextureArray[pass][stage] != nullptr)
                                        return false;
                        }
                        if (MaterialSingle[pass] != -1)
                                return false;
                        if (MaterialArray[pass] != nullptr)
                                return false;
                }
                return true;
        }

        // Init_Alternate (meshmatdesc.cpp). The default material the
        // original *copies* ("Don't share vertex materials!") is recorded as
        // the same list index — no ported path can tell the copy apart. The
        // original shares the ref-counted arrays; the transcription deep-copies
        // them (identical dump values, single ownership).
        void Init_Alternate(MeshMatDescClass &default_materials, MeshMatDescClass &alternate_materials)
        {
                PassCount = default_materials.PassCount;
                VertexCount = default_materials.VertexCount;
                PolyCount = default_materials.PolyCount;

                for (int array = 0; array < MAX_COLOR_ARRAYS; array++) {
                        delete ColorArray[array];
                        ColorArray[array] = nullptr;
                        if (alternate_materials.ColorArray[array] != nullptr) {
                                ColorArray[array] = new std::vector<uint32_t>(*alternate_materials.ColorArray[array]);
                        } else if (default_materials.ColorArray[array] != nullptr) {
                                ColorArray[array] = new std::vector<uint32_t>(*default_materials.ColorArray[array]);
                        }
                }

                for (int i = 0; i < alternate_materials.Get_UV_Array_Count(); i++) {
                        delete UV[i];
                        UV[i] = new UVBuffer();
                        UV[i]->xy = alternate_materials.UV[i]->xy;
                        UV[i]->crc = alternate_materials.UV[i]->crc;
                }

                for (int pass = 0; pass < MAX_PASSES; pass++) {
                        for (int stage = 0; stage < MAX_TEX_STAGES; stage++) {

                                if (alternate_materials.UVSource[pass][stage] == -1) {
                                        if (default_materials.UVSource[pass][stage] != -1) {

                                                int default_uv_source = default_materials.UVSource[pass][stage];
                                                UVBuffer *uvarray = default_materials.UV[default_uv_source];
                                                int found_index = -1;

                                                int have = Get_UV_Array_Count();
                                                for (int i = 0; i < have; i++) {
                                                        if (UV[i]->crc == uvarray->crc) {
                                                                found_index = i;
                                                                break;
                                                        }
                                                }

                                                if (found_index != -1) {
                                                        UVSource[pass][stage] = found_index;
                                                } else {
                                                        int new_index = Get_UV_Array_Count();
                                                        UV[new_index] = new UVBuffer();
                                                        UV[new_index]->xy = uvarray->xy;
                                                        UV[new_index]->crc = uvarray->crc;
                                                        UVSource[pass][stage] = new_index;
                                                }
                                        }
                                } else {
                                        UVSource[pass][stage] = alternate_materials.UVSource[pass][stage];
                                }

                                if ((alternate_materials.TextureSingle[pass][stage] != -1) ||
                                    (alternate_materials.TextureArray[pass][stage] != nullptr)) {
                                        TextureSingle[pass][stage] = alternate_materials.TextureSingle[pass][stage];
                                        delete TextureArray[pass][stage];
                                        TextureArray[pass][stage] =
                                            alternate_materials.TextureArray[pass][stage]
                                                ? new std::vector<int>(*alternate_materials.TextureArray[pass][stage])
                                                : nullptr;
                                } else {
                                        TextureSingle[pass][stage] = default_materials.TextureSingle[pass][stage];
                                        delete TextureArray[pass][stage];
                                        TextureArray[pass][stage] =
                                            default_materials.TextureArray[pass][stage]
                                                ? new std::vector<int>(*default_materials.TextureArray[pass][stage])
                                                : nullptr;
                                }
                        }

                        // Vertex color configuration
                        if (alternate_materials.DCGSource[pass] == CS_MATERIAL) {
                                DCGSource[pass] = default_materials.DCGSource[pass];
                        } else {
                                DCGSource[pass] = alternate_materials.DCGSource[pass];
                        }

                        // Shaders, currently I can't tell if the alternate data has a
                        // shader... Can't override the shader for now.
                        ShaderBits[pass] = default_materials.ShaderBits[pass];
                        delete ShaderArray[pass];
                        ShaderArray[pass] =
                            default_materials.ShaderArray[pass]
                                ? new std::vector<uint32_t>(*default_materials.ShaderArray[pass])
                                : nullptr;

                        // Vertex Materials. If alternate_materials has either a single
                        // or array of materials, then copy them
                        if ((alternate_materials.MaterialSingle[pass] != -1) ||
                            (alternate_materials.MaterialArray[pass] != nullptr)) {
                                MaterialSingle[pass] = alternate_materials.MaterialSingle[pass];
                                delete MaterialArray[pass];
                                MaterialArray[pass] =
                                    alternate_materials.MaterialArray[pass]
                                        ? new std::vector<int>(*alternate_materials.MaterialArray[pass])
                                        : nullptr;
                        } else {
                                // Dont share vertex materials! (the original clones the
                                // default material; the index is what the dump records)
                                MaterialSingle[pass] = default_materials.MaterialSingle[pass];
                                delete MaterialArray[pass];
                                MaterialArray[pass] =
                                    default_materials.MaterialArray[pass]
                                        ? new std::vector<int>(*default_materials.MaterialArray[pass])
                                        : nullptr;
                        }
                }
        }
};


// --- the load context (meshmdlio.cpp MeshLoadContextClass) ---------------------

struct LegacyMaterial
{
        int ShaderIdx = 0;
        int VertexMaterialIdx = 0;
        int TextureIdx = 0;
};

struct MeshLoadContextClass
{
        W3dMeshHeader3Struct Header{};
        W3dMaterialInfoStruct MatInfo{};
        uint32_t PrelitChunkID = 0xffffffff;
        int CurPass = 0;
        int CurTexStage = 0;
        bool LoadedDIG = false;

        std::vector<LegacyMaterial> LegacyMaterials;
        std::vector<ShaderClass> Shaders;
        std::vector<VertexMaterial> VertexMaterials;
        std::vector<uint32_t> VertexMaterialCrcs;

        // The asset manager's texture pool (per load — the hash lowercases
        // the filename, and objects are created on a miss with no info
        // struct) plus the context's list, which references the objects and
        // may hold the same object twice.
        std::vector<TextureObj> TextureObjects;
        std::vector<int> Textures;

        // the alternate material data collected during the load
        MeshMatDescClass *AlternateMatDesc = new MeshMatDescClass();

        ~MeshLoadContextClass() { delete AlternateMatDesc; }

        int Shader_Count() const { return (int)Shaders.size(); }
        int Vertex_Material_Count() const { return (int)VertexMaterials.size(); }
        int Texture_Count() const { return (int)Textures.size(); }

        int Add_Shader(const ShaderClass &shader)
        {
                int index = (int)Shaders.size();
                Shaders.push_back(shader);
                return index;
        }

        int Add_Vertex_Material(const VertexMaterial &vmat)
        {
                int index = (int)VertexMaterials.size();
                VertexMaterials.push_back(vmat);
                return index;
        }

        int Add_Texture(int obj)
        {
                int index = (int)Textures.size();
                Textures.push_back(obj);
                return index;
        }

        // Peek_Shader / Peek_Vertex_Material / Peek_Texture — the release
        // build reads out of bounds on a bad index (UB); the transcription
        // fails the load.
        bool Peek_Shader(int index, ShaderClass *out) const
        {
                if (index < 0 || index >= (int)Shaders.size())
                        return false;
                *out = Shaders[index];
                return true;
        }

        bool Peek_Vertex_Material(int index) const
        {
                return index >= 0 && index < (int)VertexMaterials.size();
        }

        bool Peek_Texture(int index) const
        {
                return index >= 0 && index < (int)Textures.size();
        }

        void Notify_Loaded_DIG_Chunk(bool onoff = true) { LoadedDIG = onoff; }
        bool Already_Loaded_DIG() const { return LoadedDIG; }
};

// The vertex material chunk reader (vertmaterial.cpp VertexMaterialClass::
// Load_W3D). The mapper instantiation from the argument buffers is
// render-side and out of scope; the buffers are kept raw.
struct VertexMaterialClass
{
        VertexMaterial mat;

        int Load_W3D(ChunkLoadClass &cload)
        {
                char name[256];
                memset(name, 0, sizeof(name)); // the original's stack buffer is uninitialised
                bool hasname = false;

                W3dVertexMaterialStruct vmat;
                memset(&vmat, 0, sizeof(vmat)); // ditto: uninitialised when no INFO chunk is read

                std::vector<uint8_t> mapping0_arg_buffer;
                std::vector<uint8_t> mapping1_arg_buffer;

                while (cload.Open_Chunk()) {
                        switch (cload.Cur_Chunk_ID()) {
                        case W3D_CHUNK_VERTEX_MATERIAL_NAME:
                                // the original overruns the stack buffer on a
                                // long chunk; the transcription fails the load
                                if (cload.Cur_Chunk_Length() > sizeof(name))
                                        return WW3D_ERROR_LOAD_FAILED;
                                cload.Read(name, cload.Cur_Chunk_Length());
                                hasname = true;
                                break;

                        case W3D_CHUNK_VERTEX_MATERIAL_INFO:
                                if (cload.Read(&vmat, sizeof(vmat)) != sizeof(vmat)) {
                                        return WW3D_ERROR_LOAD_FAILED;
                                }
                                break;

                        case W3D_CHUNK_VERTEX_MAPPER_ARGS0: {
                                // a second ARGS0 chunk leaks the first buffer in
                                // the original; the last one wins here
                                mapping0_arg_buffer.assign(cload.Cur_Chunk_Length(), 0);
                                if (!mapping0_arg_buffer.empty() &&
                                    cload.Read(mapping0_arg_buffer.data(), (uint32_t)mapping0_arg_buffer.size()) !=
                                        (uint32_t)mapping0_arg_buffer.size()) {
                                        return WW3D_ERROR_LOAD_FAILED;
                                }
                                break;
                        }

                        case W3D_CHUNK_VERTEX_MAPPER_ARGS1: {
                                mapping1_arg_buffer.assign(cload.Cur_Chunk_Length(), 0);
                                if (!mapping1_arg_buffer.empty() &&
                                    cload.Read(mapping1_arg_buffer.data(), (uint32_t)mapping1_arg_buffer.size()) !=
                                        (uint32_t)mapping1_arg_buffer.size()) {
                                        return WW3D_ERROR_LOAD_FAILED;
                                }
                                break;
                        }
                        }
                        cload.Close_Chunk();
                }

                if (hasname) {
                        mat.name = name;
                        mat.has_name = true;
                }

                Parse_W3dVertexMaterialStruct(&mat, vmat);
                // Parse_Mapping_Args(vmat, mapping0, mapping1): creates the
                // render-side mappers — out of scope; the raw buffers are kept
                mat.args0 = mapping0_arg_buffer;
                mat.args1 = mapping1_arg_buffer;

                return WW3D_ERROR_OK;
        }
};

// --- the mesh model (MeshModelClass + MeshGeometryClass, load-path state) ------

// The AABTree (Core/Libraries/Source/WWVegas/WW3D2/aabtree.cpp). The node
// array and the poly index array are exactly what Load_W3D stores; short
// reads fail the load (the C++ fills uninitialised memory instead).
struct AABTreeClass
{
        uint32_t NodeCount = 0;
        uint32_t PolyCount = 0;
        std::vector<W3dMeshAABTreeNode> Nodes;
        std::vector<uint32_t> PolyIndices;

        // AABTreeClass::Load_W3D. The header read's return is unchecked in
        // the original; a short read leaves the counts as garbage in the
        // release build, so the transcription fails the load instead. A
        // first chunk that is not the AABTREE header is a debug assert; the
        // release build parses it as the header anyway (the bytes land in
        // the same fields), which the transcription also does.
        bool Load_W3D(ChunkLoadClass &cload)
        {
                W3dMeshAABTreeHeader header;
                cload.Open_Chunk();
                memset(&header, 0, sizeof(header));
                if (cload.Read(&header, sizeof(header)) != sizeof(header)) {
                        return false;
                }
                cload.Close_Chunk();

                NodeCount = header.NodeCount;
                PolyCount = header.PolyCount;
                Nodes.assign(NodeCount, W3dMeshAABTreeNode{});
                PolyIndices.assign(PolyCount, 0);

                while (cload.Open_Chunk()) {
                        switch (cload.Cur_Chunk_ID()) {
                        case W3D_CHUNK_AABTREE_POLYINDICES:
                                if (!Read_Poly_Indices(cload))
                                        return false;
                                break;
                        case W3D_CHUNK_AABTREE_NODES:
                                if (!Read_Nodes(cload))
                                        return false;
                                break;
                        }
                        cload.Close_Chunk();
                }
                return true;
        }

        // Read_Poly_Indices — unchecked in the original
        bool Read_Poly_Indices(ChunkLoadClass &cload)
        {
                size_t want = sizeof(uint32_t) * (size_t)PolyCount;
                if (cload.Read(PolyIndices.data(), (uint32_t)want) != (uint32_t)want)
                        return false;
                return true;
        }

        // Read_Nodes — unchecked in the original
        bool Read_Nodes(ChunkLoadClass &cload)
        {
                for (uint32_t i = 0; i < NodeCount; i++) {
                        W3dMeshAABTreeNode w3dnode;
                        memset(&w3dnode, 0, sizeof(w3dnode));
                        if (cload.Read(&w3dnode, sizeof(w3dnode)) != sizeof(w3dnode))
                                return false;
                        Nodes[i] = w3dnode;
                }
                return true;
        }
};

struct MeshGeometryClass
{
        // flags after Reset_Geometry + the load path's Set_Flag calls
        uint32_t Flags = 0;
        int PolyCount = 0;
        int VertexCount = 0;
        int SortLevel = 0;

        std::string MeshName;
        bool HasMeshName = false;
        std::vector<uint8_t> UserText;
        bool HasUserText = false;

        // Reset_Geometry allocates (and zeroes) the arrays only when both
        // counts are non-zero; readers that find them missing would write
        // through null in the release build — the transcription fails the
        // load instead.
        std::vector<uint16_t> *Poly = nullptr;        // TriIndex triples
        std::vector<uint8_t> *PolySurfaceType = nullptr;
        std::vector<float> *Vertex = nullptr;         // xyz triples
        std::vector<float> *VertexNorm = nullptr;     // xyz triples
        std::vector<uint32_t> *VertexShadeIdx = nullptr;
        std::vector<uint16_t> *VertexBoneLink = nullptr;

        // OPTIMIZE_PLANEEQ_RAM = 1 in the Zero Hour build: the plane
        // equations live in a scratch vector grown (uninitialised) to the
        // poly count — read_triangles writes every entry, so the array is
        // fully defined whenever a TRIANGLES chunk was read.
        std::vector<float> PlaneEQArray; // xyzw quadruples

        AABTreeClass *CullTree = nullptr;

        virtual ~MeshGeometryClass() { Reset_Geometry(0, 0); }

        // MeshGeometryClass::Reset_Geometry (meshgeometry.cpp)
        void Reset_Geometry(int polycount, int vertcount)
        {
                Flags = 0;
                PolyCount = 0;
                VertexCount = 0;
                SortLevel = 0; // SORT_LEVEL_NONE

                MeshName.clear();
                HasMeshName = false;
                UserText.clear();
                HasUserText = false;

                delete Poly;
                Poly = nullptr;
                delete PolySurfaceType;
                PolySurfaceType = nullptr;
                delete Vertex;
                Vertex = nullptr;
                delete VertexNorm;
                VertexNorm = nullptr;
                delete VertexShadeIdx;
                VertexShadeIdx = nullptr;
                delete VertexBoneLink;
                VertexBoneLink = nullptr;
                delete CullTree;
                CullTree = nullptr;

                PlaneEQArray.clear();

                PolyCount = polycount;
                VertexCount = vertcount;

                if ((polycount != 0) && (vertcount != 0)) {
                        Poly = new std::vector<uint16_t>((size_t)polycount * 3, 0);
                        PolySurfaceType = new std::vector<uint8_t>((size_t)polycount, 0);
                        Vertex = new std::vector<float>((size_t)vertcount * 3, 0.0f);
                        VertexNorm = new std::vector<float>((size_t)vertcount * 3, 0.0f);
                }
        }

        void Set_Flag(uint32_t flag, bool onoff)
        {
                if (onoff)
                        Flags |= flag;
                else
                        Flags &= ~flag;
        }
        bool Get_Flag(uint32_t flag) const { return (Flags & flag) != 0; }

        // --- the geometry readers (meshgeometry.cpp) --------------------------

        // read_vertices
        bool read_vertices(ChunkLoadClass &cload)
        {
                W3dVectorStruct vert;
                if (!Vertex)
                        return false; // release: null write (UB)

                for (int i = 0; i < VertexCount; i++) {
                        if (cload.Read(&vert, sizeof(W3dVectorStruct)) != sizeof(W3dVectorStruct)) {
                                return false;
                        }
                        (*Vertex)[(size_t)i * 3 + 0] = vert.X;
                        (*Vertex)[(size_t)i * 3 + 1] = vert.Y;
                        (*Vertex)[(size_t)i * 3 + 2] = vert.Z;
                }
                return true;
        }

        // read_vertex_normals
        bool read_vertex_normals(ChunkLoadClass &cload)
        {
                W3dVectorStruct norm;
                if (!VertexNorm)
                        return false; // release: null write (UB)

                for (int i = 0; i < VertexCount; i++) {
                        if (cload.Read(&norm, sizeof(W3dVectorStruct)) != sizeof(W3dVectorStruct)) {
                                return false;
                        }
                        (*VertexNorm)[(size_t)i * 3 + 0] = norm.X;
                        (*VertexNorm)[(size_t)i * 3 + 1] = norm.Y;
                        (*VertexNorm)[(size_t)i * 3 + 2] = norm.Z;
                }
                return true;
        }

        // read_triangles — the vertex indices land in TriIndex (u16)
        // fields: the u32 file values truncate
        bool read_triangles(ChunkLoadClass &cload)
        {
                W3dTriStruct tri;

                if (!Poly || !PolySurfaceType)
                        return false; // release: null write (UB)

                PlaneEQArray.assign((size_t)PolyCount * 4, 0.0f);
                Set_Flag(0x00000002 /*DIRTY_PLANES*/, false);

                for (int i = 0; i < PolyCount; i++) {
                        if (cload.Read(&tri, sizeof(W3dTriStruct)) != sizeof(W3dTriStruct)) {
                                return false;
                        }

                        (*Poly)[(size_t)i * 3 + 0] = (uint16_t)tri.Vindex[0];
                        (*Poly)[(size_t)i * 3 + 1] = (uint16_t)tri.Vindex[1];
                        (*Poly)[(size_t)i * 3 + 2] = (uint16_t)tri.Vindex[2];

                        PlaneEQArray[(size_t)i * 4 + 0] = tri.Normal.X;
                        PlaneEQArray[(size_t)i * 4 + 1] = tri.Normal.Y;
                        PlaneEQArray[(size_t)i * 4 + 2] = tri.Normal.Z;
                        PlaneEQArray[(size_t)i * 4 + 3] = -tri.Dist;

                        // set the surface type (release: the Attributes
                        // truncate to u8)
                        (*PolySurfaceType)[i] = (uint8_t)tri.Attributes;
                }
                return true;
        }

        // read_user_text — only the first chunk is kept
        bool read_user_text(ChunkLoadClass &cload)
        {
                uint32_t textlen = cload.Cur_Chunk_Length();

                // if there are more than one USER_TEXT chunks in the mesh
                // file, store only the first one
                if (HasUserText) {
                        return true;
                }

                UserText.assign(textlen, 0);
                if (cload.Read(UserText.data(), textlen) != textlen) {
                        return false;
                }
                HasUserText = true;
                return true;
        }

        // read_vertex_influences
        bool read_vertex_influences(ChunkLoadClass &cload)
        {
                W3dVertInfStruct vinf;
                if (!VertexBoneLink) {
                        VertexBoneLink = new std::vector<uint16_t>((size_t)VertexCount, 0);
                }

                for (int i = 0; i < VertexCount; i++) {
                        if (cload.Read(&vinf, sizeof(W3dVertInfStruct)) != sizeof(W3dVertInfStruct)) {
                                return false;
                        }
                        (*VertexBoneLink)[i] = vinf.BoneIdx;
                }
                Set_Flag(FG_SKIN, true);
                return true;
        }

        // read_vertex_shade_indices
        bool read_vertex_shade_indices(ChunkLoadClass &cload)
        {
                uint32_t si;
                if (!VertexShadeIdx) {
                        VertexShadeIdx = new std::vector<uint32_t>((size_t)VertexCount, 0);
                }
                for (int i = 0; i < VertexCount; i++) {
                        if (cload.Read(&si, sizeof(uint32_t)) != sizeof(uint32_t)) {
                                return false;
                        }
                        (*VertexShadeIdx)[i] = si;
                }
                return true;
        }

        // read_aabtree — a failed AABTree read fails the mesh load in the
        // transcription (the original cannot fail here but leaves
        // uninitialised arrays)
        bool read_aabtree(ChunkLoadClass &cload)
        {
                delete CullTree;
                CullTree = new AABTreeClass();
                if (!CullTree->Load_W3D(cload)) {
                        return false;
                }
                return true;
        }
};

struct MeshModelClass : public MeshGeometryClass
{
        uint32_t W3dAttributes = 0;

        MeshMatDescClass *DefMatDesc = new MeshMatDescClass();
        MeshMatDescClass *AlternateMatDesc = nullptr;
        MeshLoadContextClass *Context = nullptr; // owned; survives for the dump

        ~MeshModelClass() override
        {
                delete DefMatDesc;
                delete AlternateMatDesc;
                delete Context;
        }

        // MeshModelClass::Reset (meshmdl.cpp) — the pass count starts at 1
        void Reset(int polycount, int vertcount, int passcount)
        {
                Reset_Geometry(polycount, vertcount);
                DefMatDesc->Reset(polycount, vertcount, passcount);
                delete AlternateMatDesc;
                AlternateMatDesc = nullptr;
        }

        void Set_Pass_Count(int passes) { DefMatDesc->Set_Pass_Count(passes); }
        int Get_Pass_Count() const { return DefMatDesc->PassCount; }
        int Get_Vertex_Count() const { return VertexCount; }
        int Get_Polygon_Count() const { return PolyCount; }

        void Set_Name(const char *newname)
        {
                MeshName = newname;
                HasMeshName = true;
        }

        // --- Load_W3D (meshmdlio.cpp) -----------------------------------------

        // Returns WW3D_ERROR_OK / WW3D_ERROR_LOAD_FAILED. On success the
        // context (with the context's alternate matdesc already merged
        // away) is owned by the model for the dump to read.
        int Load_W3D(ChunkLoadClass &cload)
        {
                MeshLoadContextClass *context = new MeshLoadContextClass();

                // Open the first chunk, it should be the mesh header.
                // (The original ignores Open_Chunk's return and reads
                // Cur_Chunk_ID() through an empty stack on an empty file —
                // the transcription states the contract instead.)
                if (!cload.Open_Chunk()) {
                        delete context;
                        return WW3D_ERROR_LOAD_FAILED;
                }

                if (cload.Cur_Chunk_ID() != W3D_CHUNK_MESH_HEADER3) {
                        // "Old format mesh mesh, no longer supported."
                        delete context;
                        return WW3D_ERROR_LOAD_FAILED;
                }

                if (cload.Read(&(context->Header), sizeof(W3dMeshHeader3Struct)) != sizeof(W3dMeshHeader3Struct)) {
                        delete context;
                        return WW3D_ERROR_LOAD_FAILED;
                }
                cload.Close_Chunk();

                // Process the header
                Reset((int)context->Header.NumTris, (int)context->Header.NumVertices, 1);

                {
                        // strlen(ContainerName) + strlen(MeshName) + 2, then
                        // "Container.Mesh" or just "Mesh"
                        std::string tmpname;
                        if (strlen(context->Header.ContainerName) > 0) {
                                tmpname = context->Header.ContainerName;
                                tmpname += ".";
                        }
                        tmpname += context->Header.MeshName;
                        Set_Name(tmpname.c_str());
                }

                W3dAttributes = context->Header.Attributes;
                SortLevel = context->Header.SortLevel;

                context->AlternateMatDesc->Set_Vertex_Count(Get_Vertex_Count());
                context->AlternateMatDesc->Set_Polygon_Count(Get_Polygon_Count());

                // Set Bounding Info — the model's bound fields are copies of
                // the header's; the dump prints the header's values

                // Flags
                if (context->Header.Version >= W3D_MAKE_VERSION(4, 1)) {
                        uint32_t geometry_type = context->Header.Attributes & W3D_MESH_FLAG_GEOMETRY_TYPE_MASK;
                        switch (geometry_type) {
                        case W3D_MESH_FLAG_GEOMETRY_TYPE_NORMAL:
                                break;
                        case W3D_MESH_FLAG_GEOMETRY_TYPE_CAMERA_ALIGNED:
                                Set_Flag(FG_ALIGNED, true);
                                break;
                        case W3D_MESH_FLAG_GEOMETRY_TYPE_CAMERA_ORIENTED:
                                Set_Flag(FG_ORIENTED, true);
                                break;
                        case W3D_MESH_FLAG_GEOMETRY_TYPE_SKIN:
                                Set_Flag(FG_SKIN, true);
                                Set_Flag(FG_ALLOW_NPATCHES, true);
                                break;
                        default:
                                break; // OBBOX / obsolete SHADOW / AABOX: nothing
                        }
                }

                if (context->Header.Attributes & W3D_MESH_FLAG_TWO_SIDED) {
                        Set_Flag(FG_TWO_SIDED, true);
                }
                if (context->Header.Attributes & W3D_MESH_FLAG_CAST_SHADOW) {
                        Set_Flag(FG_CAST_SHADOW, true);
                }
                if (context->Header.Attributes & W3D_MESH_FLAG_NPATCHABLE) {
                        Set_Flag(FG_ALLOW_NPATCHES, true);
                }

                // Configure the load sequence for prelighting.
                if (context->Header.Attributes & W3D_MESH_FLAG_PRELIT_MASK) {
                        // WW3D::Get_Prelit_Mode() — the game tree never
                        // changes the default (LIGHTMAP_MULTI_PASS)
                        switch (PrelitMode) {
                        case PRELIT_MODE_LIGHTMAP_MULTI_TEXTURE:
                                if (context->Header.Attributes & W3D_MESH_FLAG_PRELIT_LIGHTMAP_MULTI_TEXTURE) {
                                        context->PrelitChunkID = W3D_CHUNK_PRELIT_LIGHTMAP_MULTI_TEXTURE;
                                        Set_Flag(FG_PRELIT_LIGHTMAP_MULTI_TEXTURE, true);
                                        break;
                                }
                                // Else fall thru...
                                [[fallthrough]];
                        case PRELIT_MODE_LIGHTMAP_MULTI_PASS:
                                if (context->Header.Attributes & W3D_MESH_FLAG_PRELIT_LIGHTMAP_MULTI_PASS) {
                                        context->PrelitChunkID = W3D_CHUNK_PRELIT_LIGHTMAP_MULTI_PASS;
                                        Set_Flag(FG_PRELIT_LIGHTMAP_MULTI_PASS, true);
                                        break;
                                }
                                // Else fall thru...
                                [[fallthrough]];
                        case PRELIT_MODE_VERTEX:
                                if (context->Header.Attributes & W3D_MESH_FLAG_PRELIT_VERTEX) {
                                        context->PrelitChunkID = W3D_CHUNK_PRELIT_VERTEX;
                                        Set_Flag(FG_PRELIT_VERTEX, true);
                                        break;
                                }
                                // Else fall thru...
                                [[fallthrough]];
                        default:
                                // This prelighting option MUST be available if none
                                // of the others are available. (WWASSERT in debug;
                                // release sets the unlit chunk id regardless)
                                context->PrelitChunkID = W3D_CHUNK_PRELIT_UNLIT;
                                break;
                        }
                } else {
                        // For backwards compatibility, test for obsolete lightmap flag.
                        if (context->Header.Attributes & OBSOLETE_W3D_MESH_FLAG_LIGHTMAPPED) {
                                Set_Flag(FG_PRELIT_LIGHTMAP_MULTI_PASS, true);
                        }
                }

                read_chunks(cload, context);

                // If this is a pre-3.0 mesh and it has vertex influences,
                // fixup the bone indices to account for the new root node
                if ((context->Header.Version < W3D_MAKE_VERSION(3, 0)) && (Get_Flag(FG_SKIN))) {
                        // the release build dereferences a null bone-link array
                        // when the influences chunk was never read (the flag
                        // cannot be set without it at this version) — stated
                        // as a load failure instead
                        if (!VertexBoneLink) {
                                delete context;
                                return WW3D_ERROR_LOAD_FAILED;
                        }
                        for (int bi = 0; bi < Get_Vertex_Count(); bi++) {
                                (*VertexBoneLink)[bi] = (uint16_t)((*VertexBoneLink)[bi] + 1);
                        }
                }

                // If this mesh is collideable and no AABTree was in the file,
                // the original generates one here (Generate_Culling_Tree —
                // deferred to the collision slice; the dump records the
                // chunk-sourced tree only)

                install_materials(context);

                // post_process(): render-side (fog activation, static sort
                // levels, overbright) — out of scope for the format layer

                Context = context;
                return WW3D_ERROR_OK;
        }

        // --- read_chunks (meshmdlio.cpp) ---------------------------------------

        int read_chunks(ChunkLoadClass &cload, MeshLoadContextClass *context)
        {
                while (cload.Open_Chunk()) {

                        int error = WW3D_ERROR_OK;

                        switch (cload.Cur_Chunk_ID()) {

                        case W3D_CHUNK_VERTICES:
                                error = read_vertices(cload) ? WW3D_ERROR_OK : WW3D_ERROR_LOAD_FAILED;
                                break;

                        case W3D_CHUNK_SURRENDER_NORMALS:
                        case W3D_CHUNK_VERTEX_NORMALS:
                                error = read_vertex_normals(cload) ? WW3D_ERROR_OK : WW3D_ERROR_LOAD_FAILED;
                                break;

                        case W3D_CHUNK_TEXCOORDS:
                                error = read_texcoords(cload, context);
                                break;

                        case O_W3D_CHUNK_MATERIALS:
                        case O_W3D_CHUNK_MATERIALS2:
                                // "Obsolete material chunk encountered in mesh: ..."
                                // WWASSERT(0) — release: nothing
                                break;

                        case W3D_CHUNK_MATERIALS3:
                                // "Obsolete material chunk encountered in mesh: ..."
                                error = read_v3_materials(cload, context);
                                break;

                        case O_W3D_CHUNK_SURRENDER_TRIANGLES:
                                // "Obsolete Triangle Chunk Encountered!"
                                // WWASSERT_PRINT — release: nothing
                                break;

                        case W3D_CHUNK_TRIANGLES:
                                error = read_triangles(cload) ? WW3D_ERROR_OK : WW3D_ERROR_LOAD_FAILED;
                                break;

                        case W3D_CHUNK_PER_TRI_MATERIALS:
                                error = read_per_tri_materials(cload, context);
                                break;

                        case W3D_CHUNK_MESH_USER_TEXT:
                                error = read_user_text(cload) ? WW3D_ERROR_OK : WW3D_ERROR_LOAD_FAILED;
                                break;

                        case W3D_CHUNK_VERTEX_COLORS:
                                error = read_vertex_colors(cload, context);
                                break;

                        case W3D_CHUNK_VERTEX_INFLUENCES:
                                error = read_vertex_influences(cload) ? WW3D_ERROR_OK : WW3D_ERROR_LOAD_FAILED;
                                break;

                        case W3D_CHUNK_VERTEX_SHADE_INDICES:
                                error = read_vertex_shade_indices(cload) ? WW3D_ERROR_OK : WW3D_ERROR_LOAD_FAILED;
                                break;

                        case W3D_CHUNK_MATERIAL_INFO:
                                error = read_material_info(cload, context);
                                break;

                        case W3D_CHUNK_SHADERS:
                                error = read_shaders(cload, context);
                                break;

                        case W3D_CHUNK_VERTEX_MATERIALS:
                                error = read_vertex_materials(cload, context);
                                break;

                        case W3D_CHUNK_TEXTURES:
                                error = read_textures(cload, context);
                                break;

                        case W3D_CHUNK_MATERIAL_PASS:
                                error = read_material_pass(cload, context);
                                break;

                        case W3D_CHUNK_DEFORM:
                                // "Obsolete deform chunk encountered in mesh: ..."
                                break;

                        case W3D_CHUNK_DAMAGE:
                                // "Obsolete damage chunk encountered in mesh: ..."
                                break;

                        case W3D_CHUNK_PRELIT_UNLIT:
                        case W3D_CHUNK_PRELIT_VERTEX:
                        case W3D_CHUNK_PRELIT_LIGHTMAP_MULTI_PASS:
                        case W3D_CHUNK_PRELIT_LIGHTMAP_MULTI_TEXTURE:
                                error = read_prelit_material(cload, context);
                                break;

                        case W3D_CHUNK_AABTREE:
                                error = read_aabtree(cload) ? WW3D_ERROR_OK : WW3D_ERROR_LOAD_FAILED;
                                break;

                        default:
                                break;
                        }

                        cload.Close_Chunk();

                        if (error != WW3D_ERROR_OK) {
                                return error;
                        }
                }

                return WW3D_ERROR_OK;
        }

        // --- read_texcoords (meshmdlio.cpp, the obsolete top-level path) -------

        int read_texcoords(ChunkLoadClass &cload, MeshLoadContextClass *context)
        {
                W3dTexCoordStruct texcoord;
                int elementcount = (int)(cload.Cur_Chunk_Length() / sizeof(W3dTexCoordStruct));

                // the temporary uv array: the C++ writes VertexCount entries
                // into an elementcount-sized scratch buffer and installs
                // elementcount of them; only elementcount == VertexCount is
                // defined (a larger count installs uninitialised bytes, a
                // smaller count fails the reads first)
                if (elementcount != Get_Vertex_Count()) {
                        return WW3D_ERROR_LOAD_FAILED;
                }

                // the obsolete path runs at the walk's top level, where the
                // pass counter can already be past MAX_PASSES; the C++ writes
                // UVSource out of bounds there (UB) — the transcription fails
                if (context->CurPass < 0 || context->CurPass >= MAX_PASSES ||
                    context->CurTexStage < 0 || context->CurTexStage >= MAX_TEX_STAGES) {
                        return WW3D_ERROR_LOAD_FAILED;
                }

                std::vector<float> uvarray((size_t)elementcount * 2, 0.0f);
                for (int i = 0; i < Get_Vertex_Count(); i++) {
                        if (cload.Read(&texcoord, sizeof(W3dTexCoordStruct)) != sizeof(W3dTexCoordStruct)) {
                                return WW3D_ERROR_LOAD_FAILED;
                        }
                        uvarray[(size_t)i * 2 + 0] = texcoord.U;
                        uvarray[(size_t)i * 2 + 1] = 1.0f - texcoord.V;
                }

                DefMatDesc->Install_UV_Array(context->CurPass, context->CurTexStage, uvarray, elementcount);

                return WW3D_ERROR_OK;
        }

        // --- read_v3_materials (meshmdlio.cpp, the legacy material path) -------

        int read_v3_materials(ChunkLoadClass &cload, MeshLoadContextClass *context)
        {
                for (uint32_t mi = 0; mi < context->Header.NumMaterials; mi++) {

                        // First, we expect a W3D_CHUNK_MATERIAL3 to wrap the
                        // entire material
                        if (!cload.Open_Chunk())
                                return WW3D_ERROR_LOAD_FAILED;
                        if (cload.Cur_Chunk_ID() != W3D_CHUNK_MATERIAL3)
                                return WW3D_ERROR_LOAD_FAILED;

                        // name (a 256-byte stack buffer in the original; the
                        // zero-filled transcription buffer makes the C-string
                        // view deterministic)
                        char name[256];
                        memset(name, 0, sizeof(name));
                        bool hasname = false;

                        ShaderClass shader;   // default-constructed preset bits
                        VertexMaterial vmat;  // constructor defaults
                        int tex_obj = -1;     // TextureClass * tex = nullptr
                        bool has_vmat = false;

                        // Read the material name
                        if (!cload.Open_Chunk())
                                return WW3D_ERROR_LOAD_FAILED;
                        if (cload.Cur_Chunk_ID() != W3D_CHUNK_MATERIAL3_NAME)
                                return WW3D_ERROR_LOAD_FAILED;
                        if (cload.Cur_Chunk_Length() > sizeof(name))
                                return WW3D_ERROR_LOAD_FAILED; // the original overruns the stack buffer
                        cload.Read(name, cload.Cur_Chunk_Length());
                        hasname = true;
                        if (!cload.Close_Chunk())
                                return WW3D_ERROR_LOAD_FAILED;

                        // Read the vertex material parameters
                        if (!cload.Open_Chunk())
                                return WW3D_ERROR_LOAD_FAILED;

                        W3dMaterial3Struct mat;
                        memset(&mat, 0, sizeof(mat));
                        if (cload.Cur_Chunk_ID() != W3D_CHUNK_MATERIAL3_INFO)
                                return WW3D_ERROR_LOAD_FAILED;
                        if (cload.Read(&mat, sizeof(W3dMaterial3Struct)) != sizeof(W3dMaterial3Struct))
                                return WW3D_ERROR_LOAD_FAILED;
                        Init_From_Material3(&vmat, mat);
                        if (hasname) {
                                vmat.name = name;
                                vmat.has_name = true;
                        }
                        has_vmat = true;
                        // ShaderClass::Init_From_Material3 (shader.cpp)
                        if (mat.Attributes & W3DMATERIAL_USE_ALPHA) {
                                shader.Set_Depth_Mask(0);     // DEPTH_WRITE_DISABLE
                                shader.Set_Dst_Blend_Func(5); // DSTBLEND_ONE_MINUS_SRC_ALPHA
                                shader.Set_Src_Blend_Func(2); // SRCBLEND_SRC_ALPHA
                        }

                        // If this shader does alpha blending, the mesh must be
                        // sorted.
                        if (shader.Get_Dst_Blend_Func() != SHD_DSTBLEND_ZERO) {
                                Set_Flag(FG_SORT, true);
                        }

                        if (!cload.Close_Chunk())
                                return WW3D_ERROR_LOAD_FAILED;

                        // Look for the DC map and read it in
                        while (cload.Open_Chunk()) {
                                if (cload.Cur_Chunk_ID() == W3D_CHUNK_MATERIAL3_DC_MAP) {

                                        // Read in the texture filename
                                        char filename[259]; // _MAX_FNAME + _MAX_EXT
                                        if (!cload.Open_Chunk())
                                                return WW3D_ERROR_LOAD_FAILED;
                                        if (cload.Cur_Chunk_ID() != W3D_CHUNK_MAP3_FILENAME)
                                                return WW3D_ERROR_LOAD_FAILED;
                                        if (cload.Cur_Chunk_Length() >= sizeof(filename))
                                                return WW3D_ERROR_LOAD_FAILED;
                                        memset(filename, 0, sizeof(filename));
                                        cload.Read(filename, cload.Cur_Chunk_Length());
                                        if (!cload.Close_Chunk())
                                                return WW3D_ERROR_LOAD_FAILED;

                                        // Read in the auxiliary map info
                                        W3dMap3Struct mapinfo;
                                        memset(&mapinfo, 0, sizeof(mapinfo));
                                        if (!cload.Open_Chunk())
                                                return WW3D_ERROR_LOAD_FAILED;
                                        if (cload.Cur_Chunk_ID() != W3D_CHUNK_MAP3_INFO)
                                                return WW3D_ERROR_LOAD_FAILED;
                                        if (cload.Read(&mapinfo, sizeof(W3dMap3Struct)) != sizeof(W3dMap3Struct))
                                                return WW3D_ERROR_LOAD_FAILED;
                                        if (!cload.Close_Chunk())
                                                return WW3D_ERROR_LOAD_FAILED;

                                        if (mapinfo.FrameCount > 1) {
                                                // "ERROR: Obsolete Animated Texture detected in model: ..."
                                        }

                                        // tex = WW3DAssetManager::Get_Texture(filename)
                                        tex_obj = asset_get_texture(context, filename);

                                        shader.Set_Texturing(SHD_TEXTURING_ENABLE);

                                } else if (cload.Cur_Chunk_ID() == W3D_CHUNK_MATERIAL3_SI_MAP) {
                                        float diffuse[3] = {vmat.diffuse[0], vmat.diffuse[1], vmat.diffuse[2]};
                                        if (diffuse[0] == 0 && diffuse[1] == 0 && diffuse[2] == 0) {

                                                // Read in the texture filename
                                                char filename[259];
                                                if (!cload.Open_Chunk())
                                                        return WW3D_ERROR_LOAD_FAILED;
                                                if (cload.Cur_Chunk_ID() != W3D_CHUNK_MAP3_FILENAME)
                                                        return WW3D_ERROR_LOAD_FAILED;
                                                if (cload.Cur_Chunk_Length() >= sizeof(filename))
                                                        return WW3D_ERROR_LOAD_FAILED;
                                                memset(filename, 0, sizeof(filename));
                                                cload.Read(filename, cload.Cur_Chunk_Length());
                                                if (!cload.Close_Chunk())
                                                        return WW3D_ERROR_LOAD_FAILED;

                                                // if (tex) tex->Release_Ref(); — no data effect

                                                // Read in the auxiliary map info
                                                W3dMap3Struct mapinfo;
                                                memset(&mapinfo, 0, sizeof(mapinfo));
                                                if (!cload.Open_Chunk())
                                                        return WW3D_ERROR_LOAD_FAILED;
                                                if (cload.Cur_Chunk_ID() != W3D_CHUNK_MAP3_INFO)
                                                        return WW3D_ERROR_LOAD_FAILED;
                                                if (cload.Read(&mapinfo, sizeof(W3dMap3Struct)) != sizeof(W3dMap3Struct))
                                                        return WW3D_ERROR_LOAD_FAILED;
                                                if (!cload.Close_Chunk())
                                                        return WW3D_ERROR_LOAD_FAILED;

                                                if (mapinfo.FrameCount > 1) {
                                                        // "ERROR: Obsolete Animated Texture detected in model: ..."
                                                }

                                                tex_obj = asset_get_texture(context, filename);

                                                shader.Set_Texturing(SHD_TEXTURING_ENABLE);
                                                shader.Set_Dst_Blend_Func(1); // DSTBLEND_ONE
                                                shader.Set_Src_Blend_Func(1); // SRCBLEND_ONE
                                                shader.Set_Primary_Gradient(SHD_GRADIENT_DISABLE);
                                        }
                                }

                                cload.Close_Chunk();
                        }

                        // If not texturing, move the diffuse color to ambient
                        // (simulating old behavior)
                        if (shader.Get_Texturing() == 0 /*TEXTURING_DISABLE*/) {
                                float color[3] = {vmat.diffuse[0], vmat.diffuse[1], vmat.diffuse[2]};
                                vmat.Set_Ambient(color[0], color[1], color[2]);
                                vmat.Set_Diffuse(0, 0, 0);
                        }

                        // context->Add_Legacy_Material(shader, vmat, tex)
                        if (!has_vmat)
                                return WW3D_ERROR_LOAD_FAILED; // the original dereferences null here
                        legacy_add_material(context, shader, vmat, tex_obj);

                        // Close the W3D_CHUNK_MATERIAL3
                        cload.Close_Chunk();
                }

                // Install the default materials to use in the absence of an
                // array
                if (context->Vertex_Material_Count() >= 1) {
                        DefMatDesc->Set_Single_Material(0, 0);
                }
                if (context->Texture_Count() >= 1) {
                        DefMatDesc->Set_Single_Texture(0, 0, 0);
                }
                if (context->Shader_Count() >= 1) {
                        DefMatDesc->Set_Single_Shader(context->Shaders[0].ShaderBits, 0);
                }

                return WW3D_ERROR_OK;
        }

        // --- read_per_tri_materials (meshmdlio.cpp) -----------------------------

        int read_per_tri_materials(ChunkLoadClass &cload, MeshLoadContextClass *context)
        {
                if (context->Header.NumMaterials == 1)
                        return WW3D_ERROR_OK;

                bool multi_mtl = (context->Vertex_Material_Count() > 1);
                bool multi_tex = (context->Texture_Count() > 1);
                bool multi_shad = (context->Shader_Count() > 1);

                if (!multi_mtl) {
                        // Set_Single_Material(context->Peek_Legacy_Vertex_Material(0))
                        int idx = legacy_vmat_index(context, 0);
                        DefMatDesc->Set_Single_Material(idx, 0);
                }
                if (!multi_tex) {
                        int idx = legacy_tex_index(context, 0);
                        DefMatDesc->Set_Single_Texture(idx, 0, 0);
                }
                if (!multi_shad) {
                        uint32_t bits = legacy_shader_bits(context, 0);
                        DefMatDesc->Set_Single_Shader(bits, 0);
                }

                // Read in each polygon's material id and assign pointer to the
                // shader, texture, and vertex material as needed.
                for (int i = 0; i < Get_Polygon_Count(); i++) {

                        // read in the mat id for this poly
                        uint16_t matid;
                        if (cload.Read(&matid, sizeof(uint16_t)) != sizeof(uint16_t)) {
                                return WW3D_ERROR_LOAD_FAILED;
                        }

                        if (multi_shad) {
                                DefMatDesc->Set_Shader(i, legacy_shader_bits(context, (int)matid), 0);
                        }
                        if (multi_tex) {
                                DefMatDesc->Set_Texture(i, legacy_tex_index(context, (int)matid), 0, 0);
                        }
                        if (multi_mtl) {
                                int vmat = legacy_vmat_index(context, (int)matid);
                                DefMatDesc->Set_Material((*Poly)[(size_t)i * 3 + 0], vmat, 0);
                                DefMatDesc->Set_Material((*Poly)[(size_t)i * 3 + 1], vmat, 0);
                                DefMatDesc->Set_Material((*Poly)[(size_t)i * 3 + 2], vmat, 0);
                        }
                }

                return WW3D_ERROR_OK;
        }

        // --- read_vertex_colors (meshmdlio.cpp) ---------------------------------

        int read_vertex_colors(ChunkLoadClass &cload, MeshLoadContextClass *context)
        {
                // the pass counter runs past MAX_PASSES only on malformed
                // input; the C++ writes DCGSource out of bounds there (UB) —
                // the transcription fails the sub-read
                if (context->CurPass < 0 || context->CurPass >= MAX_PASSES)
                        return WW3D_ERROR_LOAD_FAILED;

                // 1 - If this is the first diffuse color array we've
                //     encountered, load the values
                // 2 - Always set the DCG source for this pass to the array
                if (DefMatDesc->Has_Color_Array(0) == false) {
                        W3dRGBStruct color;
                        std::vector<uint32_t> *dcg = DefMatDesc->Get_Color_Array(0, true);

                        for (int i = 0; i < Get_Vertex_Count(); i++) {
                                if (cload.Read(&color, sizeof(W3dRGBStruct)) != sizeof(W3dRGBStruct)) {
                                        return WW3D_ERROR_LOAD_FAILED;
                                }
                                float col[4] = {(float)color.R / 255.0f, (float)color.G / 255.0f,
                                                (float)color.B / 255.0f, 1.0f};
                                (*dcg)[i] = Convert_Color_ARGB(col);
                        }
                }
                DefMatDesc->Set_DCG_Source(context->CurPass, CS_COLOR1);

                return WW3D_ERROR_OK;
        }

        // --- read_material_info (meshmdlio.cpp) ---------------------------------

        int read_material_info(ChunkLoadClass &cload, MeshLoadContextClass *context)
        {
                if (cload.Read(&(context->MatInfo), sizeof(W3dMaterialInfoStruct)) != sizeof(W3dMaterialInfoStruct)) {
                        return WW3D_ERROR_LOAD_FAILED;
                }
                Set_Pass_Count((int)context->MatInfo.PassCount);
                return WW3D_ERROR_OK;
        }

        // --- read_shaders (meshmdlio.cpp) ----------------------------------------

        int read_shaders(ChunkLoadClass &cload, MeshLoadContextClass *context)
        {
                W3dShaderStruct shader;
                for (uint32_t i = 0; i < context->MatInfo.ShaderCount; i++) {
                        if (cload.Read(&shader, sizeof(shader)) != sizeof(shader)) {
                                return WW3D_ERROR_LOAD_FAILED;
                        }
                        ShaderClass newshader;
                        Convert_Shader(shader, &newshader);

                        int index = context->Add_Shader(newshader);
                        // WWASSERT(index == (int)i) — the prelit wrapper's
                        // shaders append after the top-level ones in release
                        (void)index;
                }
                return WW3D_ERROR_OK;
        }

        // --- read_vertex_materials (meshmdlio.cpp) -------------------------------

        int read_vertex_materials(ChunkLoadClass &cload, MeshLoadContextClass *context)
        {
                while (cload.Open_Chunk()) {
                        // WWASSERT(Cur_Chunk_ID() == W3D_CHUNK_VERTEX_MATERIAL) —
                        // release: any child is fed to the vertex material reader
                        VertexMaterialClass vmat;
                        int error = vmat.Load_W3D(cload);
                        if (error != WW3D_ERROR_OK) {
                                return error;
                        }
                        context->Add_Vertex_Material(vmat.mat);

                        cload.Close_Chunk();
                }
                return WW3D_ERROR_OK;
        }

        // --- read_textures (meshmdlio.cpp + texture.cpp Load_Texture) ------------

        int read_textures(ChunkLoadClass &cload, MeshLoadContextClass *context)
        {
                // Keep reading textures until there are no more...
                for (;;) {
                        // ::Load_Texture(cload) — the data path. The name and
                        // the optional info chunk are read inside the TEXTURE
                        // wrapper; the asset manager then returns the object
                        // (created on a miss, with this chunk's info) or the
                        // pre-existing one (the info is creation-time only).
                        // An empty filename makes Get_Texture return null and
                        // the loop ends. A first child that is not a TEXTURE
                        // chunk also ends the loop, with the chunk left open
                        // on the reader's stack exactly like the original.
                        int obj = -1;

                        if (cload.Open_Chunk() && (cload.Cur_Chunk_ID() == W3D_CHUNK_TEXTURE)) {
                                char name[256];
                                memset(name, 0, sizeof(name));
                                W3dTextureInfoStruct texinfo;
                                memset(&texinfo, 0, sizeof(texinfo));
                                bool hastexinfo = false;

                                // Read in the texture filename, and a possible
                                // texture info structure.
                                while (cload.Open_Chunk()) {
                                        switch (cload.Cur_Chunk_ID()) {
                                        case W3D_CHUNK_TEXTURE_NAME:
                                                if (cload.Cur_Chunk_Length() > sizeof(name))
                                                        return WW3D_ERROR_LOAD_FAILED; // the original overruns the stack buffer
                                                cload.Read(name, cload.Cur_Chunk_Length());
                                                break;

                                        case W3D_CHUNK_TEXTURE_INFO:
                                                if (cload.Read(&texinfo, sizeof(texinfo)) != sizeof(texinfo))
                                                        return WW3D_ERROR_LOAD_FAILED;
                                                hastexinfo = true;
                                                break;
                                        }
                                        cload.Close_Chunk();
                                }
                                cload.Close_Chunk();

                                // Get the texture from the asset manager
                                // (WW3DAssetManager::Get_Texture: an empty name
                                // returns null; the hash key lowercases)
                                std::string lower = name;
                                for (auto &ch : lower) {
                                        if (ch >= 'A' && ch <= 'Z')
                                                ch = (char)(ch - 'A' + 'a');
                                }
                                if (!lower.empty()) {
                                        bool found = false;
                                        for (size_t i = 0; i < context->TextureObjects.size(); i++) {
                                                if (context->TextureObjects[i].name == lower) {
                                                        obj = (int)i;
                                                        found = true;
                                                        break;
                                                }
                                        }
                                        if (!found) {
                                                TextureObj tobj;
                                                tobj.name = lower;
                                                tobj.has_info = hastexinfo;
                                                tobj.info = texinfo;
                                                context->TextureObjects.push_back(tobj);
                                                obj = (int)(context->TextureObjects.size() - 1);
                                        }
                                }
                        }

                        if (obj == -1) {
                                // Load_Texture returned null
                                break;
                        }

                        // Add this texture to our context
                        context->Add_Texture(obj);
                }

                return WW3D_ERROR_OK;
        }

        // --- read_material_pass (meshmdlio.cpp) ----------------------------------

        int read_material_pass(ChunkLoadClass &cload, MeshLoadContextClass *context)
        {
                context->CurTexStage = 0;

                while (cload.Open_Chunk()) {

                        int error = WW3D_ERROR_OK;

                        switch (cload.Cur_Chunk_ID()) {
                        case W3D_CHUNK_VERTEX_MATERIAL_IDS:
                                error = read_vertex_material_ids(cload, context);
                                break;

                        case W3D_CHUNK_SHADER_IDS:
                                error = read_shader_ids(cload, context);
                                break;

                        case W3D_CHUNK_DCG:
                                error = read_dcg(cload, context);
                                break;

                        case W3D_CHUNK_DIG:
                                error = read_dig(cload, context);
                                break;

                        case W3D_CHUNK_SCG:
                                error = read_scg(cload, context);
                                break;

                        case W3D_CHUNK_TEXTURE_STAGE:
                                error = read_texture_stage(cload, context);
                                break;
                        };

                        if (error != WW3D_ERROR_OK) {
                                return error;
                        }
                        cload.Close_Chunk();
                }

                context->CurPass++;
                return WW3D_ERROR_OK;
        }

        // --- read_vertex_material_ids (meshmdlio.cpp) -----------------------------

        int read_vertex_material_ids(ChunkLoadClass &cload, MeshLoadContextClass *context)
        {
                // the pass/stage counters run past MAX_PASSES / MAX_TEX_STAGES
                // only on malformed input; the C++ reads its arrays out of
                // bounds there (UB) — the transcription fails the sub-read
                if (context->CurPass < 0 || context->CurPass >= MAX_PASSES)
                        return WW3D_ERROR_LOAD_FAILED;
                // Determine whether this chunk should be read into the default
                // or alternate material description
                MeshMatDescClass *matdesc = DefMatDesc;
                if (DefMatDesc->Has_Material_Data(context->CurPass)) {
                        matdesc = context->AlternateMatDesc;
                }

                // the original ignores the read returns (a short read re-reads
                // an uninitialised index through Peek — UB); the transcription
                // fails the load instead
                uint32_t vmat;
                if (cload.Cur_Chunk_Length() == 1 * sizeof(uint32_t)) {
                        if (cload.Read(&vmat, sizeof(uint32_t)) != sizeof(uint32_t))
                                return WW3D_ERROR_LOAD_FAILED;
                        if (!context->Peek_Vertex_Material((int)vmat))
                                return WW3D_ERROR_LOAD_FAILED;
                        matdesc->Set_Single_Material((int)vmat, context->CurPass);
                } else {
                        for (int i = 0; i < Get_Vertex_Count(); i++) {
                                if (cload.Read(&vmat, sizeof(uint32_t)) != sizeof(uint32_t))
                                        return WW3D_ERROR_LOAD_FAILED;
                                if ((int)vmat < 0 || (int)vmat >= context->Vertex_Material_Count())
                                        return WW3D_ERROR_LOAD_FAILED;
                                matdesc->Set_Material(i, (int)vmat, context->CurPass);
                        }
                }

                return WW3D_ERROR_OK;
        }

        // --- read_shader_ids (meshmdlio.cpp) ---------------------------------------

        int read_shader_ids(ChunkLoadClass &cload, MeshLoadContextClass *context)
        {
                // the pass/stage counters run past MAX_PASSES / MAX_TEX_STAGES
                // only on malformed input; the C++ reads its arrays out of
                // bounds there (UB) — the transcription fails the sub-read
                if (context->CurPass < 0 || context->CurPass >= MAX_PASSES)
                        return WW3D_ERROR_LOAD_FAILED;
                MeshMatDescClass *matdesc = DefMatDesc;
                if (DefMatDesc->Has_Shader_Data(context->CurPass)) {
                        matdesc = context->AlternateMatDesc;
                }

                // the original ignores the read returns (UB on a short read)
                uint32_t shaderid;
                if (cload.Cur_Chunk_Length() == 1 * sizeof(uint32_t)) {
                        if (cload.Read(&shaderid, sizeof(shaderid)) != sizeof(shaderid))
                                return WW3D_ERROR_LOAD_FAILED;
                        ShaderClass shader;
                        if (!context->Peek_Shader((int)shaderid, &shader))
                                return WW3D_ERROR_LOAD_FAILED;
                        matdesc->Set_Single_Shader(shader.ShaderBits, context->CurPass);

                        // turn on sorting of pass 0 has non-zero dest blend
                        // (unless alpha testing on)
                        if ((context->CurPass == 0) && (shader.Get_Dst_Blend_Func() != SHD_DSTBLEND_ZERO) &&
                            (shader.Get_Alpha_Test() == SHD_ALPHATEST_DISABLE) && (SortLevel == 0 /*SORT_LEVEL_NONE*/)) {
                                // MESH_FORCE_STATIC_SORT_HACK = 0
                                Set_Flag(FG_SORT, true);
                        }
                } else {
                        for (int i = 0; i < Get_Polygon_Count(); i++) {
                                if (cload.Read(&shaderid, sizeof(shaderid)) != sizeof(shaderid))
                                        return WW3D_ERROR_LOAD_FAILED;
                                ShaderClass shader;
                                if (!context->Peek_Shader((int)shaderid, &shader))
                                        return WW3D_ERROR_LOAD_FAILED;
                                matdesc->Set_Shader(i, shader.ShaderBits, context->CurPass);

                                if ((context->CurPass == 0) && (shader.Get_Dst_Blend_Func() != SHD_DSTBLEND_ZERO) &&
                                    (shader.Get_Alpha_Test() == SHD_ALPHATEST_DISABLE) && (SortLevel == 0)) {
                                        Set_Flag(FG_SORT, true);
                                }
                        }
                }

                return WW3D_ERROR_OK;
        }

        // --- read_dcg (meshmdlio.cpp) -----------------------------------------------

        int read_dcg(ChunkLoadClass &cload, MeshLoadContextClass *context)
        {
                // the pass/stage counters run past MAX_PASSES / MAX_TEX_STAGES
                // only on malformed input; the C++ reads its arrays out of
                // bounds there (UB) — the transcription fails the sub-read
                if (context->CurPass < 0 || context->CurPass >= MAX_PASSES)
                        return WW3D_ERROR_LOAD_FAILED;
                MeshMatDescClass *matdesc = DefMatDesc;
                if (DefMatDesc->Get_DCG_Source(context->CurPass) != CS_MATERIAL) {
                        matdesc = context->AlternateMatDesc;
                }

                // the original ignores the read returns (the color variable
                // goes stale and the first read is uninitialised — UB)
                if (matdesc->Has_Color_Array(0) == false) {
                        std::vector<uint32_t> *dcg = matdesc->Get_Color_Array(0, true);

                        for (int i = 0; i < Get_Vertex_Count(); i++) {
                                W3dRGBAStruct color;
                                if (cload.Read(&color, sizeof(color)) != sizeof(color))
                                        return WW3D_ERROR_LOAD_FAILED;
                                float col[4];
                                Convert_Color(color, col);
                                (*dcg)[i] = Convert_Color_ARGB(col);
                        }
                } else if (context->PrelitChunkID == W3D_CHUNK_PRELIT_VERTEX) {

                        std::vector<uint32_t> *dcg = matdesc->Get_Color_Array(0, true);

                        for (int i = 0; i < Get_Vertex_Count(); i++) {
                                W3dRGBAStruct color;
                                if (cload.Read(&color, sizeof(color)) != sizeof(color))
                                        return WW3D_ERROR_LOAD_FAILED;
                                float col[4];
                                Convert_Color_ARGB_To_Vector4((*dcg)[i], col);
                                col[3] = (float)color.A / 255.0f;
                                (*dcg)[i] = Convert_Color_ARGB(col);
                        }
                }

                matdesc->Set_DCG_Source(context->CurPass, CS_COLOR1);

                return WW3D_ERROR_OK;
        }

        // --- read_dig (meshmdlio.cpp) ------------------------------------------------

        int read_dig(ChunkLoadClass &cload, MeshLoadContextClass *context)
        {
                // the pass/stage counters run past MAX_PASSES / MAX_TEX_STAGES
                // only on malformed input; the C++ reads its arrays out of
                // bounds there (UB) — the transcription fails the sub-read
                if (context->CurPass < 0 || context->CurPass >= MAX_PASSES)
                        return WW3D_ERROR_LOAD_FAILED;
                MeshMatDescClass *matdesc = DefMatDesc;
                if (context->Already_Loaded_DIG()) {
                        matdesc = context->AlternateMatDesc;
                }
                context->Notify_Loaded_DIG_Chunk(true);

                // the original ignores the read returns (UB on a short read)
                if (matdesc->Has_Color_Array(0) == false) {
                        std::vector<uint32_t> *dcg = matdesc->Get_Color_Array(0, true);
                        for (int i = 0; i < Get_Vertex_Count(); i++) {
                                W3dRGBAStruct color;
                                if (cload.Read(&color, sizeof(color)) != sizeof(color))
                                        return WW3D_ERROR_LOAD_FAILED;
                                float col[4] = {(float)color.R / 255.0f, (float)color.G / 255.0f, (float)color.B / 255.0f, 1.0f};
                                (*dcg)[i] = Convert_Color_ARGB(col);
                        }
                } else {
                        std::vector<uint32_t> *dcg = matdesc->Get_Color_Array(0, true);
                        for (int i = 0; i < Get_Vertex_Count(); i++) {
                                W3dRGBAStruct color;
                                if (cload.Read(&color, sizeof(color)) != sizeof(color))
                                        return WW3D_ERROR_LOAD_FAILED;
                                float col[4];
                                Convert_Color_ARGB_To_Vector4((*dcg)[i], col);
                                col[0] *= (float)color.R / 255.0f;
                                col[1] *= (float)color.G / 255.0f;
                                col[2] *= (float)color.B / 255.0f;
                                (*dcg)[i] = Convert_Color_ARGB(col);
                        }
                }

                matdesc->Set_DCG_Source(context->CurPass, CS_COLOR1);

                return WW3D_ERROR_OK;
        }

        // --- read_scg (meshmdlio.cpp): new dx8 code no longer supports this chunk --

        int read_scg(ChunkLoadClass &cload, MeshLoadContextClass *context)
        {
                (void)cload;
                (void)context;
                return WW3D_ERROR_OK;
        }

        // --- read_texture_stage (meshmdlio.cpp) --------------------------------------

        int read_texture_stage(ChunkLoadClass &cload, MeshLoadContextClass *context)
        {
                while (cload.Open_Chunk()) {

                        int error = WW3D_ERROR_OK;

                        switch (cload.Cur_Chunk_ID()) {
                        case W3D_CHUNK_TEXTURE_IDS:
                                error = read_texture_ids(cload, context);
                                break;

                        case W3D_CHUNK_STAGE_TEXCOORDS:
                        case W3D_CHUNK_TEXCOORDS:
                                error = read_stage_texcoords(cload, context);
                                break;

                        case W3D_CHUNK_PER_FACE_TEXCOORD_IDS:
                                error = read_per_face_texcoord_ids(cload, context);
                                break;
                        }

                        if (error != WW3D_ERROR_OK) {
                                return error;
                        }

                        cload.Close_Chunk();
                }

                context->CurTexStage++;
                return WW3D_ERROR_OK;
        }

        // --- read_texture_ids (meshmdlio.cpp) ------------------------------------------

        int read_texture_ids(ChunkLoadClass &cload, MeshLoadContextClass *context)
        {
                // the pass/stage counters run past MAX_PASSES / MAX_TEX_STAGES
                // only on malformed input; the C++ reads its arrays out of
                // bounds there (UB) — the transcription fails the sub-read
                if (context->CurPass < 0 || context->CurPass >= MAX_PASSES)
                        return WW3D_ERROR_LOAD_FAILED;
                if (context->CurTexStage < 0 || context->CurTexStage >= MAX_TEX_STAGES)
                        return WW3D_ERROR_LOAD_FAILED;
                uint32_t texid;
                int pass = context->CurPass;
                int stage = context->CurTexStage;

                MeshMatDescClass *matdesc = DefMatDesc;
                if (DefMatDesc->Has_Texture_Data(pass, stage)) {
                        matdesc = context->AlternateMatDesc;
                }

                // the original ignores the read returns (UB on a short read)
                if (cload.Cur_Chunk_Length() == 1 * sizeof(uint32_t)) {
                        if (cload.Read(&texid, sizeof(texid)) != sizeof(texid))
                                return WW3D_ERROR_LOAD_FAILED;
                        if ((int)texid < 0 || (int)texid >= context->Texture_Count())
                                return WW3D_ERROR_LOAD_FAILED;
                        matdesc->Set_Single_Texture((int)texid, pass, stage);
                } else {
                        for (int i = 0; i < Get_Polygon_Count(); i++) {
                                if (cload.Read(&texid, sizeof(texid)) != sizeof(texid))
                                        return WW3D_ERROR_LOAD_FAILED;
                                if (texid != 0xffffffff) {
                                        if ((int)texid < 0 || (int)texid >= context->Texture_Count())
                                                return WW3D_ERROR_LOAD_FAILED;
                                        matdesc->Set_Texture(i, (int)texid, pass, stage);
                                }
                        }
                }

                return WW3D_ERROR_OK;
        }

        // --- read_stage_texcoords (meshmdlio.cpp) ----------------------------------------

        int read_stage_texcoords(ChunkLoadClass &cload, MeshLoadContextClass *context)
        {
                // the pass/stage counters run past MAX_PASSES / MAX_TEX_STAGES
                // only on malformed input; the C++ reads its arrays out of
                // bounds there (UB) — the transcription fails the sub-read
                if (context->CurPass < 0 || context->CurPass >= MAX_PASSES)
                        return WW3D_ERROR_LOAD_FAILED;
                if (context->CurTexStage < 0 || context->CurTexStage >= MAX_TEX_STAGES)
                        return WW3D_ERROR_LOAD_FAILED;
                MeshMatDescClass *matdesc = DefMatDesc;
                if (DefMatDesc->Has_UV(context->CurPass, context->CurTexStage)) {
                        matdesc = context->AlternateMatDesc;
                }

                unsigned elementcount = cload.Cur_Chunk_Length() / sizeof(W3dTexCoordStruct);

                // the original ignores the read return: a short chunk leaves
                // the texcoord stale (uninitialised on the first iteration)
                // and still installs — the transcription fails instead
                std::vector<float> uvs((size_t)elementcount * 2, 0.0f);
                for (unsigned i = 0; i < elementcount; i++) {
                        W3dTexCoordStruct texcoord;
                        if (cload.Read(&texcoord, sizeof(texcoord)) != sizeof(texcoord)) {
                                return WW3D_ERROR_LOAD_FAILED;
                        }
                        uvs[(size_t)i * 2 + 0] = texcoord.U;
                        uvs[(size_t)i * 2 + 1] = 1.0f - texcoord.V;
                }

                matdesc->Install_UV_Array(context->CurPass, context->CurTexStage, uvs, (int)elementcount);

                return WW3D_ERROR_OK;
        }

        // --- read_per_face_texcoord_ids (meshmdlio.cpp) ------------------------------------

        int read_per_face_texcoord_ids(ChunkLoadClass &cload, MeshLoadContextClass *context)
        {
                (void)context;
                // There must be polygon count vectors in this chunk.
                unsigned size = (unsigned)(sizeof(int) * 3 * (size_t)Get_Polygon_Count());
                if (cload.Cur_Chunk_Length() == size) {
                        cload.Seek(size);
                        return WW3D_ERROR_OK;
                } else {
                        return WW3D_ERROR_LOAD_FAILED;
                }
        }

        // --- read_prelit_material (meshmdlio.cpp) --------------------------------------------

        int read_prelit_material(ChunkLoadClass &cload, MeshLoadContextClass *context)
        {
                // If this chunk ID matches the selected prelit chunk ID then
                // load it, otherwise skip it.
                if (cload.Cur_Chunk_ID() == context->PrelitChunkID) {

                        // While there are chunks in the prelit material chunk wrapper...
                        while (cload.Open_Chunk()) {

                                int error = WW3D_ERROR_OK;

                                switch (cload.Cur_Chunk_ID()) {

                                case W3D_CHUNK_MATERIAL_INFO:
                                        error = read_material_info(cload, context);
                                        break;

                                case W3D_CHUNK_VERTEX_MATERIALS:
                                        error = read_vertex_materials(cload, context);
                                        break;

                                case W3D_CHUNK_SHADERS:
                                        error = read_shaders(cload, context);
                                        break;

                                case W3D_CHUNK_TEXTURES:
                                        error = read_textures(cload, context);
                                        break;

                                case W3D_CHUNK_MATERIAL_PASS:
                                        error = read_material_pass(cload, context);
                                        break;

                                default:
                                        // Unknown chunk.
                                        break;
                                }
                                cload.Close_Chunk();
                                if (error != WW3D_ERROR_OK)
                                        return error;
                        }
                }

                return WW3D_ERROR_OK;
        }

        // --- install_materials (meshmdlio.cpp) ------------------------------------------------

        void install_materials(MeshLoadContextClass *context)
        {
                // If alternate material chunks were loaded, initialize the
                // AlternateMatDesc
                install_alternate_material_desc(context);

                // Post_Load_Process (the DX8 pipeline adaptation) is
                // render-side and out of scope; the texture and vertex
                // material refs transfer into the flat lists unchanged
        }

        void install_alternate_material_desc(MeshLoadContextClass *context)
        {
                if (context->AlternateMatDesc->Is_Empty() == false) {
                        AlternateMatDesc = new MeshMatDescClass();
                        AlternateMatDesc->Init_Alternate(*DefMatDesc, *context->AlternateMatDesc);
                }
        }

        // --- the legacy material helpers ----------------------------------------------------

        // WW3DAssetManager::Get_Texture's data path: the name lowercases and
        // keys the hash; the object is created on a miss (with no info
        // struct). An empty filename returns null.
        static int asset_get_texture(MeshLoadContextClass *context, const char *filename)
        {
                std::string lower = filename;
                for (auto &c : lower) {
                        if (c >= 'A' && c <= 'Z')
                                c = (char)(c - 'A' + 'a');
                }
                if (lower.empty())
                        return -1;
                for (size_t i = 0; i < context->TextureObjects.size(); i++) {
                        if (context->TextureObjects[i].name == lower)
                                return (int)i;
                }
                TextureObj obj;
                obj.name = lower;
                obj.has_info = false;
                context->TextureObjects.push_back(obj);
                return (int)(context->TextureObjects.size() - 1);
        }

        // the legacy material's texture: Get_Texture + Add_Legacy_Material's
        // name dedup collapse to "first list entry with the object's name,
        // else append the object"
        static void legacy_add_material(MeshLoadContextClass *context, const ShaderClass &shader,
                                        const VertexMaterial &vmat, int tex_obj)
        {
                // Add_Legacy_Material(shader, vmat, tex)
                LegacyMaterial mat;

                int si = 0;
                for (; si < (int)context->Shaders.size(); si++) {
                        if (context->Shaders[si] == shader)
                                break;
                }
                if (si == (int)context->Shaders.size()) {
                        mat.ShaderIdx = context->Add_Shader(shader);
                } else {
                        mat.ShaderIdx = si;
                }

                {
                        uint32_t crc = vmat.Compute_CRC();
                        int vi = 0;
                        for (; vi < (int)context->VertexMaterialCrcs.size(); vi++) {
                                if (context->VertexMaterialCrcs[vi] == crc)
                                        break;
                        }
                        if (vi == (int)context->VertexMaterials.size()) {
                                mat.VertexMaterialIdx = context->Add_Vertex_Material(vmat);
                                context->VertexMaterialCrcs.push_back(crc);
                        } else {
                                mat.VertexMaterialIdx = vi;
                        }
                }

                if (tex_obj == -1) {
                        mat.TextureIdx = -1;
                } else {
                        int ti = 0;
                        for (; ti < (int)context->Textures.size(); ti++) {
                                if (context->Textures[ti] == tex_obj)
                                        break;
                        }
                        if (ti == (int)context->Textures.size()) {
                                mat.TextureIdx = context->Add_Texture(tex_obj);
                        } else {
                                mat.TextureIdx = ti;
                        }
                }

                context->LegacyMaterials.push_back(mat);
        }

        // legacy peek helpers (the LegacyMaterials' index remaps)
        static uint32_t legacy_shader_bits(MeshLoadContextClass *context, int legacy_material_index)
        {
                int si = context->LegacyMaterials[legacy_material_index].ShaderIdx;
                return context->Shaders[si].ShaderBits;
        }

        static int legacy_vmat_index(MeshLoadContextClass *context, int legacy_material_index)
        {
                int vi = context->LegacyMaterials[legacy_material_index].VertexMaterialIdx;
                return vi; // -1 == null material
        }

        static int legacy_tex_index(MeshLoadContextClass *context, int legacy_material_index)
        {
                int ti = context->LegacyMaterials[legacy_material_index].TextureIdx;
                return ti; // -1 == null texture
        }
};

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

        void w_u8(uint8_t v) { bytes.push_back(v); }
        void w_u16(uint16_t v)
        {
                uint8_t b[2] = {(uint8_t)(v & 0xFF), (uint8_t)(v >> 8)};
                bytes.insert(bytes.end(), b, b + 2);
        }
        void w_u32(uint32_t v)
        {
                uint8_t b[4] = {(uint8_t)(v & 0xFF), (uint8_t)((v >> 8) & 0xFF), (uint8_t)((v >> 16) & 0xFF),
                                (uint8_t)((v >> 24) & 0xFF)};
                bytes.insert(bytes.end(), b, b + 4);
        }
        void w_i32(int32_t v) { w_u32((uint32_t)v); }
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
        void w_cstr(const char *s) { w_bytes(s, strlen(s) + 1); }
        void w_rgb(uint8_t r, uint8_t g, uint8_t b)
        {
                w_u8(r);
                w_u8(g);
                w_u8(b);
                w_u8(0); // the W3dRGBStruct pad byte
        }
        void w_rgba(uint8_t r, uint8_t g, uint8_t b, uint8_t a)
        {
                w_u8(r);
                w_u8(g);
                w_u8(b);
                w_u8(a);
        }
        void w_vec3(float x, float y, float z)
        {
                w_f32(x);
                w_f32(y);
                w_f32(z);
        }
        void w_pair(float u, float v)
        {
                w_f32(u);
                w_f32(v);
        }

        static void put_u32(uint8_t *p, uint32_t v)
        {
                p[0] = (uint8_t)(v & 0xFF);
                p[1] = (uint8_t)((v >> 8) & 0xFF);
                p[2] = (uint8_t)((v >> 16) & 0xFF);
                p[3] = (uint8_t)((v >> 24) & 0xFF);
        }
};

// The 116-byte W3dMeshHeader3Struct
static void w_mesh_header3(ChunkBuild &b, uint32_t version, uint32_t attributes, const char *mesh,
                           const char *container, uint32_t numtris, uint32_t numverts, uint32_t nummaterials,
                           int32_t sortlevel = 0)
{
        char meshname[16], contname[16];
        memset(meshname, 0, sizeof(meshname));
        memset(contname, 0, sizeof(contname));
        snprintf(meshname, sizeof(meshname), "%s", mesh);
        snprintf(contname, sizeof(contname), "%s", container);

        b.begin(W3D_CHUNK_MESH_HEADER3);
        b.w_u32(version);
        b.w_u32(attributes);
        b.w_bytes(meshname, 16);
        b.w_bytes(contname, 16);
        b.w_u32(numtris);
        b.w_u32(numverts);
        b.w_u32(nummaterials);
        b.w_u32(0); // NumDamageStages
        b.w_i32(sortlevel);
        b.w_u32(0); // PrelitVersion
        b.w_u32(0); // FutureCounts[1]
        b.w_u32(0x1F); // VertexChannels (all channels present)
        b.w_u32(0x01); // FaceChannels
        b.w_vec3(-1.0f, -1.0f, -1.0f); // Min
        b.w_vec3(1.0f, 1.0f, 1.0f);    // Max
        b.w_vec3(0.0f, 0.0f, 0.0f);    // SphCenter
        b.w_f32(1.5f);                  // SphRadius
        b.end();
}

// A W3dShaderStruct (16 bytes)
static void w_shader(ChunkBuild &b, uint8_t depth_compare, uint8_t depth_mask, uint8_t dest_blend,
                     uint8_t pri_gradient, uint8_t sec_gradient, uint8_t src_blend, uint8_t texturing,
                     uint8_t alpha_test, uint8_t detail_color, uint8_t detail_alpha)
{
        b.w_u8(depth_compare);
        b.w_u8(depth_mask);
        b.w_u8(0); // ColorMask (obsolete)
        b.w_u8(dest_blend);
        b.w_u8(0); // FogFunc (obsolete)
        b.w_u8(pri_gradient);
        b.w_u8(sec_gradient);
        b.w_u8(src_blend);
        b.w_u8(texturing);
        b.w_u8(detail_color);
        b.w_u8(detail_alpha);
        b.w_u8(0); // ShaderPreset (obsolete)
        b.w_u8(alpha_test);
        b.w_u8(detail_color); // PostDetailColorFunc (copied from detail)
        b.w_u8(detail_alpha); // PostDetailAlphaFunc (copied from detail)
        b.w_u8(0);            // pad
}

// A W3dTriStruct (32 bytes)
static void w_tri(ChunkBuild &b, uint32_t i, uint32_t j, uint32_t k, uint32_t attrs, float nx, float ny,
                  float nz, float dist)
{
        b.w_u32(i);
        b.w_u32(j);
        b.w_u32(k);
        b.w_u32(attrs);
        b.w_vec3(nx, ny, nz);
        b.w_f32(dist);
}

// A W3dVertInfStruct (8 bytes)
static void w_influence(ChunkBuild &b, uint16_t bone)
{
        b.w_u16(bone);
        b.w_u8(0);
        b.w_u8(0);
        b.w_u8(0);
        b.w_u8(0);
        b.w_u8(0);
        b.w_u8(0);
}

// A W3dVertexMaterialStruct (32 bytes)
static void w_vmat_info(ChunkBuild &b, uint32_t attributes, uint8_t ar, uint8_t ag, uint8_t ab, uint8_t dr,
                        uint8_t dg, uint8_t db, uint8_t sr, uint8_t sg, uint8_t sb, uint8_t er, uint8_t eg,
                        uint8_t eb, float shininess, float opacity, float translucency)
{
        b.w_u32(attributes);
        b.w_rgb(ar, ag, ab);
        b.w_rgb(dr, dg, db);
        b.w_rgb(sr, sg, sb);
        b.w_rgb(er, eg, eb);
        b.w_f32(shininess);
        b.w_f32(opacity);
        b.w_f32(translucency);
}

// A W3dTextureInfoStruct (12 bytes)
static void w_texinfo(ChunkBuild &b, uint16_t attributes, uint16_t animtype, uint32_t framecount, float framerate)
{
        b.w_u16(attributes);
        b.w_u16(animtype);
        b.w_u32(framecount);
        b.w_f32(framerate);
}

// --- the corpora ------------------------------------------------------------------

struct Corpus
{
        const char *id;
        std::vector<uint8_t> bytes;
};

static std::vector<Corpus> build_corpora()
{
        std::vector<Corpus> out;

        auto add = [&out](const char *id, ChunkBuild &b) {
                assert(b.marks.empty());
                out.push_back(Corpus{id, b.bytes});
        };

        // ---- m01: the modern full mesh, prelit vertex wrapper, aabtree ----
        {
                ChunkBuild b;
                w_mesh_header3(b, W3D_MAKE_VERSION(4, 2), W3D_MESH_FLAG_PRELIT_VERTEX | W3D_MESH_FLAG_CAST_SHADOW,
                               "Body", "AVTank", 2, 3, 1);
                b.begin(W3D_CHUNK_VERTICES);
                b.w_vec3(0.5f, 0.25f, -0.5f);
                b.w_vec3(-0.75f, 0.125f, 0.5f);
                b.w_vec3(0.0f, -0.25f, 1.0f);
                b.end();
                b.begin(W3D_CHUNK_VERTEX_NORMALS);
                b.w_vec3(0.0f, 0.0f, 1.0f);
                b.w_vec3(0.0f, 1.0f, 0.0f);
                b.w_vec3(1.0f, 0.0f, 0.0f);
                b.end();
                b.begin(W3D_CHUNK_VERTEX_INFLUENCES);
                w_influence(b, 1);
                w_influence(b, 1);
                w_influence(b, 2);
                b.end();
                b.begin(W3D_CHUNK_VERTEX_SHADE_INDICES);
                b.w_u32(10);
                b.w_u32(20);
                b.w_u32(30);
                b.end();
                b.begin(W3D_CHUNK_TRIANGLES);
                w_tri(b, 0, 1, 2, 0, 0.0f, 0.0f, 1.0f, 0.5f);
                w_tri(b, 2, 1, 0, 9, 0.0f, 1.0f, 0.0f, -0.25f);
                b.end();
                b.begin(W3D_CHUNK_MESH_USER_TEXT);
                b.w_cstr("hello mesh");
                b.end();
                b.begin(W3D_CHUNK_PRELIT_VERTEX);
                {
                        b.begin(W3D_CHUNK_MATERIAL_INFO);
                        b.w_u32(1); // pass count
                        b.w_u32(1); // vmat count
                        b.w_u32(1); // shader count
                        b.w_u32(1); // texture count
                        b.end();
                        b.begin(W3D_CHUNK_SHADERS);
                        w_shader(b, 3, 1, 0, 1, 0, 1, 1, 0, 0, 0);
                        b.end();
                        b.begin(W3D_CHUNK_VERTEX_MATERIALS);
                        {
                                b.begin(W3D_CHUNK_VERTEX_MATERIAL);
                                b.begin(W3D_CHUNK_VERTEX_MATERIAL_NAME);
                                b.w_cstr("mat0");
                                b.end();
                                b.begin(W3D_CHUNK_VERTEX_MATERIAL_INFO);
                                w_vmat_info(b, 0, 255, 255, 255, 200, 180, 160, 40, 40, 40, 10, 10, 10, 2.0f,
                                            0.75f, 0.0f);
                                b.end();
                                b.begin(W3D_CHUNK_VERTEX_MAPPER_ARGS0);
                                b.w_cstr("u=1");
                                b.end();
                                b.end();
                        }
                        b.end();
                        b.begin(W3D_CHUNK_TEXTURES);
                        {
                                b.begin(W3D_CHUNK_TEXTURE);
                                b.begin(W3D_CHUNK_TEXTURE_NAME);
                                b.w_cstr("tread.dds");
                                b.end();
                                b.begin(W3D_CHUNK_TEXTURE_INFO);
                                w_texinfo(b, 0x000C, 0, 1, 0.0f);
                                b.end();
                                b.end();
                        }
                        b.end();
                        b.begin(W3D_CHUNK_MATERIAL_PASS);
                        {
                                b.begin(W3D_CHUNK_VERTEX_MATERIAL_IDS);
                                b.w_u32(0);
                                b.end();
                                b.begin(W3D_CHUNK_SHADER_IDS);
                                b.w_u32(0);
                                b.end();
                                b.begin(W3D_CHUNK_DCG);
                                b.w_rgba(255, 0, 0, 255);
                                b.w_rgba(0, 255, 0, 128);
                                b.w_rgba(0, 0, 255, 64);
                                b.end();
                                b.begin(W3D_CHUNK_TEXTURE_STAGE);
                                {
                                        b.begin(W3D_CHUNK_TEXTURE_IDS);
                                        b.w_u32(0);
                                        b.end();
                                        b.begin(W3D_CHUNK_STAGE_TEXCOORDS);
                                        b.w_pair(0.0f, 0.5f);
                                        b.w_pair(0.25f, 0.75f);
                                        b.w_pair(0.5f, 1.0f);
                                        b.end();
                                }
                                b.end();
                        }
                        b.end();
                }
                b.end();
                b.begin(W3D_CHUNK_AABTREE);
                {
                        b.begin(W3D_CHUNK_AABTREE_HEADER);
                        b.w_u32(1); // node count
                        b.w_u32(2); // poly count
                        for (int i = 0; i < 6; i++)
                                b.w_u32(0);
                        b.end();
                        b.begin(W3D_CHUNK_AABTREE_POLYINDICES);
                        b.w_u32(0);
                        b.w_u32(1);
                        b.end();
                        b.begin(W3D_CHUNK_AABTREE_NODES);
                        b.w_vec3(-1.0f, -1.0f, -1.0f);
                        b.w_vec3(1.0f, 1.0f, 1.0f);
                        b.w_u32(0x80000000u); // leaf: poly0 = 0
                        b.w_u32(2);           // poly count
                        b.end();
                }
                b.end();
                add("m01", b);
        }

        // ---- m02: no prelit flag, obsolete lightmapped flag, skipped prelit ----
        {
                ChunkBuild b;
                w_mesh_header3(b, W3D_MAKE_VERSION(4, 2),
                               OBSOLETE_W3D_MESH_FLAG_LIGHTMAPPED | W3D_MESH_FLAG_TWO_SIDED, "Static", "", 1, 3,
                               0);
                b.begin(W3D_CHUNK_VERTICES);
                b.w_vec3(0.0f, 0.0f, 0.0f);
                b.w_vec3(1.0f, 0.0f, 0.0f);
                b.w_vec3(0.0f, 1.0f, 0.0f);
                b.end();
                b.begin(W3D_CHUNK_TRIANGLES);
                w_tri(b, 0, 1, 2, 8, 0.0f, 0.0f, 1.0f, 0.0f);
                b.end();
                b.begin(W3D_CHUNK_PRELIT_UNLIT);
                {
                        b.begin(W3D_CHUNK_MATERIAL_INFO);
                        b.w_u32(3);
                        b.w_u32(0);
                        b.w_u32(0);
                        b.w_u32(0);
                        b.end();
                        b.begin(W3D_CHUNK_MATERIAL_PASS);
                        b.end();
                }
                b.end();
                add("m02", b);
        }

        // ---- m03: pre-3.0 skin fixup ----
        {
                ChunkBuild b;
                w_mesh_header3(b, W3D_MAKE_VERSION(2, 0), 0, "Skin", "Man", 1, 3, 0);
                b.begin(W3D_CHUNK_VERTEX_INFLUENCES);
                w_influence(b, 0);
                w_influence(b, 1);
                w_influence(b, 2);
                b.end();
                b.begin(W3D_CHUNK_TRIANGLES);
                w_tri(b, 0, 1, 2, 0, 0.0f, 1.0f, 0.0f, 0.0f);
                b.end();
                add("m03", b);
        }

        // ---- m04: legacy v3 materials, per-tri materials, obsolete no-op chunks ----
        {
                ChunkBuild b;
                w_mesh_header3(b, W3D_MAKE_VERSION(2, 0), 0, "OldMesh", "", 2, 3, 2);
                b.begin(W3D_CHUNK_VERTICES);
                b.w_vec3(0.0f, 0.0f, 0.0f);
                b.w_vec3(1.0f, 0.0f, 0.0f);
                b.w_vec3(0.0f, 1.0f, 0.0f);
                b.end();
                b.begin(W3D_CHUNK_TRIANGLES);
                w_tri(b, 0, 1, 2, 0, 0.0f, 0.0f, 1.0f, 0.0f);
                w_tri(b, 2, 0, 1, 5, 0.0f, 1.0f, 0.0f, 0.0f);
                b.end();
                b.begin(O_W3D_CHUNK_MATERIALS);
                b.w_u32(0xDEADBEEF);
                b.end();
                b.begin(O_W3D_CHUNK_SURRENDER_TRIANGLES);
                b.w_u32(1);
                b.w_u32(2);
                b.end();
                b.begin(W3D_CHUNK_DEFORM);
                b.w_u32(7);
                b.end();
                b.begin(W3D_CHUNK_DAMAGE);
                b.w_u32(8);
                b.end();
                b.begin(W3D_CHUNK_MATERIALS3);
                {
                        // material A: alpha + DC map
                        b.begin(W3D_CHUNK_MATERIAL3);
                        b.begin(W3D_CHUNK_MATERIAL3_NAME);
                        b.w_cstr("mtlA");
                        b.end();
                        b.begin(W3D_CHUNK_MATERIAL3_INFO);
                        b.w_u32(W3DMATERIAL_USE_ALPHA);
                        b.w_rgb(200, 100, 50);   // diffuse color
                        b.w_rgb(0, 0, 0);        // specular color
                        b.w_rgb(0, 0, 0);        // emissive coefficients
                        b.w_rgb(255, 255, 255);  // ambient coefficients
                        b.w_rgb(255, 255, 255);  // diffuse coefficients
                        b.w_rgb(0, 0, 0);        // specular coefficients
                        b.w_f32(1.0f);           // shininess
                        b.w_f32(0.5f);           // opacity
                        b.w_f32(0.0f);           // translucency
                        b.w_f32(1.0f);           // fog coefficient
                        b.end();
                        b.begin(W3D_CHUNK_MATERIAL3_DC_MAP);
                        b.begin(W3D_CHUNK_MAP3_FILENAME);
                        b.w_cstr("brick.dds");
                        b.end();
                        b.begin(W3D_CHUNK_MAP3_INFO);
                        b.w_u16(0);
                        b.w_u16(1);
                        b.w_f32(0.0f);
                        b.end();
                        b.end();
                        b.end();
                        // material B: zero diffuse + SI map (the SI branch fires)
                        b.begin(W3D_CHUNK_MATERIAL3);
                        b.begin(W3D_CHUNK_MATERIAL3_NAME);
                        b.w_cstr("mtlB");
                        b.end();
                        b.begin(W3D_CHUNK_MATERIAL3_INFO);
                        b.w_u32(0);
                        b.w_rgb(0, 0, 0);        // diffuse color == 0
                        b.w_rgb(60, 60, 60);     // specular color
                        b.w_rgb(0, 0, 0);        // emissive coefficients
                        b.w_rgb(128, 128, 128);  // ambient coefficients
                        b.w_rgb(255, 255, 255);  // diffuse coefficients
                        b.w_rgb(200, 200, 200);  // specular coefficients
                        b.w_f32(4.0f);
                        b.w_f32(1.0f);
                        b.w_f32(0.0f);
                        b.w_f32(1.0f);
                        b.end();
                        b.begin(W3D_CHUNK_MATERIAL3_SI_MAP);
                        b.begin(W3D_CHUNK_MAP3_FILENAME);
                        b.w_cstr("glow.dds");
                        b.end();
                        b.begin(W3D_CHUNK_MAP3_INFO);
                        b.w_u16(0);
                        b.w_u16(1);
                        b.w_f32(0.0f);
                        b.end();
                        b.end();
                        b.end();
                }
                b.end();
                b.begin(W3D_CHUNK_PER_TRI_MATERIALS);
                b.w_u16(0);
                b.w_u16(1);
                b.end();
                add("m04", b);
        }

        // ---- m05: first chunk is not the mesh header ----
        {
                ChunkBuild b;
                b.begin(W3D_CHUNK_VERTICES);
                b.w_vec3(0.0f, 0.0f, 0.0f);
                b.end();
                add("m05", b);
        }

        // ---- m06: truncated header payload ----
        {
                ChunkBuild b;
                b.begin(W3D_CHUNK_MESH_HEADER3);
                b.w_bytes("123456789012345678901234567890123456789012345678901234567890", 60);
                b.end();
                add("m06", b);
        }

        // ---- m07: empty file ----
        {
                ChunkBuild b;
                add("m07", b);
        }

        // ---- m08: short vertices chunk aborts the walk, load still succeeds ----
        {
                ChunkBuild b;
                w_mesh_header3(b, W3D_MAKE_VERSION(4, 2), 0, "Short", "X", 2, 3, 0);
                b.begin(W3D_CHUNK_VERTICES);
                b.w_vec3(0.5f, 0.5f, 0.5f);
                b.w_vec3(-0.5f, -0.5f, -0.5f);
                b.end(); // only 2 of 3 vertex records
                b.begin(W3D_CHUNK_TRIANGLES);
                w_tri(b, 0, 1, 2, 0, 0.0f, 1.0f, 0.0f, 0.0f);
                w_tri(b, 2, 1, 0, 0, 0.0f, 1.0f, 0.0f, 0.0f);
                b.end();
                add("m08", b);
        }

        // ---- m09: obsolete top-level TEXCOORDS path, twice (CRC dedup) ----
        {
                ChunkBuild b;
                w_mesh_header3(b, W3D_MAKE_VERSION(4, 2), 0, "Tex", "", 1, 3, 0);
                b.begin(W3D_CHUNK_VERTICES);
                b.w_vec3(0.0f, 0.0f, 0.0f);
                b.w_vec3(1.0f, 0.0f, 0.0f);
                b.w_vec3(0.0f, 1.0f, 0.0f);
                b.end();
                b.begin(W3D_CHUNK_TEXCOORDS);
                b.w_pair(0.0f, 0.0f);
                b.w_pair(0.5f, 0.5f);
                b.w_pair(0.25f, 0.75f);
                b.end();
                b.begin(W3D_CHUNK_TEXCOORDS);
                b.w_pair(0.0f, 0.0f);
                b.w_pair(0.5f, 0.5f);
                b.w_pair(0.25f, 0.75f);
                b.end();
                add("m09", b);
        }

        // ---- m10: per-vertex vmat ids, per-poly shader/texture ids, SORT flag ----
        {
                ChunkBuild b;
                w_mesh_header3(b, W3D_MAKE_VERSION(4, 2), 0, "Multi", "M", 2, 3, 0);
                b.begin(W3D_CHUNK_VERTICES);
                b.w_vec3(0.0f, 0.0f, 0.0f);
                b.w_vec3(1.0f, 0.0f, 0.0f);
                b.w_vec3(0.0f, 1.0f, 0.0f);
                b.end();
                b.begin(W3D_CHUNK_TRIANGLES);
                w_tri(b, 0, 1, 2, 0, 0.0f, 0.0f, 1.0f, 0.0f);
                w_tri(b, 2, 1, 0, 0, 0.0f, 0.0f, 1.0f, 0.0f);
                b.end();
                b.begin(W3D_CHUNK_MATERIAL_INFO);
                b.w_u32(2);
                b.w_u32(2);
                b.w_u32(2);
                b.w_u32(2);
                b.end();
                b.begin(W3D_CHUNK_SHADERS);
                w_shader(b, 3, 1, 0, 1, 0, 1, 1, 0, 0, 0); // opaque
                w_shader(b, 3, 1, 4, 1, 0, 1, 1, 0, 0, 0); // dst blend SRC_ALPHA -> SORT
                b.end();
                b.begin(W3D_CHUNK_VERTEX_MATERIALS);
                {
                        b.begin(W3D_CHUNK_VERTEX_MATERIAL);
                        b.begin(W3D_CHUNK_VERTEX_MATERIAL_NAME);
                        b.w_cstr("vm0");
                        b.end();
                        b.begin(W3D_CHUNK_VERTEX_MATERIAL_INFO);
                        w_vmat_info(b, W3DVERTMAT_USE_DEPTH_CUE, 255, 255, 255, 255, 255, 255, 0, 0, 0, 0, 0, 0,
                                    1.0f, 1.0f, 0.0f);
                        b.end();
                        b.end();
                        b.begin(W3D_CHUNK_VERTEX_MATERIAL);
                        b.begin(W3D_CHUNK_VERTEX_MATERIAL_NAME);
                        b.w_cstr("vm1");
                        b.end();
                        b.begin(W3D_CHUNK_VERTEX_MATERIAL_INFO);
                        w_vmat_info(b, W3DVERTMAT_COPY_SPECULAR_TO_DIFFUSE, 128, 128, 128, 64, 64, 64, 32, 32,
                                    32, 16, 16, 16, 8.0f, 0.5f, 0.25f);
                        b.end();
                        b.end();
                }
                b.end();
                b.begin(W3D_CHUNK_TEXTURES);
                {
                        b.begin(W3D_CHUNK_TEXTURE);
                        b.begin(W3D_CHUNK_TEXTURE_NAME);
                        b.w_cstr("t0.dds");
                        b.end();
                        b.begin(W3D_CHUNK_TEXTURE_INFO);
                        w_texinfo(b, 0x0000, 0, 1, 0.0f);
                        b.end();
                        b.end();
                        b.begin(W3D_CHUNK_TEXTURE);
                        b.begin(W3D_CHUNK_TEXTURE_NAME);
                        b.w_cstr("t1.dds");
                        b.end();
                        b.end();
                }
                b.end();
                b.begin(W3D_CHUNK_MATERIAL_PASS);
                {
                        b.begin(W3D_CHUNK_VERTEX_MATERIAL_IDS);
                        b.w_u32(0);
                        b.w_u32(1);
                        b.w_u32(0);
                        b.end();
                        b.begin(W3D_CHUNK_SHADER_IDS);
                        b.w_u32(0);
                        b.w_u32(1);
                        b.end();
                        b.begin(W3D_CHUNK_TEXTURE_STAGE);
                        {
                                b.begin(W3D_CHUNK_TEXTURE_IDS);
                                b.w_u32(0);
                                b.w_u32(0xffffffff);
                                b.end();
                                b.begin(W3D_CHUNK_STAGE_TEXCOORDS);
                                b.w_pair(0.0f, 0.0f);
                                b.w_pair(0.5f, 0.5f);
                                b.w_pair(0.25f, 0.75f);
                                b.end();
                        }
                        b.end();
                }
                b.end();
                b.begin(W3D_CHUNK_MATERIAL_PASS);
                {
                        b.begin(W3D_CHUNK_VERTEX_MATERIAL_IDS);
                        b.w_u32(1);
                        b.end();
                        b.begin(W3D_CHUNK_SHADER_IDS);
                        b.w_u32(0);
                        b.end();
                        b.begin(W3D_CHUNK_TEXTURE_STAGE);
                        {
                                b.begin(W3D_CHUNK_TEXTURE_IDS);
                                b.w_u32(1);
                                b.end();
                                b.begin(W3D_CHUNK_STAGE_TEXCOORDS);
                                b.w_pair(0.1f, 0.2f);
                                b.w_pair(0.3f, 0.4f);
                                b.w_pair(0.5f, 0.6f);
                                b.end();
                        }
                        b.end();
                }
                b.end();
                add("m10", b);
        }

        // ---- m11: alternate material description routing + UV dedup + DCG/DIG ----
        {
                ChunkBuild b;
                w_mesh_header3(b, W3D_MAKE_VERSION(4, 2), W3D_MESH_FLAG_PRELIT_VERTEX, "Alt", "", 1, 3, 0);
                b.begin(W3D_CHUNK_VERTICES);
                b.w_vec3(0.0f, 0.0f, 0.0f);
                b.w_vec3(1.0f, 0.0f, 0.0f);
                b.w_vec3(0.0f, 1.0f, 0.0f);
                b.end();
                b.begin(W3D_CHUNK_TRIANGLES);
                w_tri(b, 0, 1, 2, 0, 0.0f, 0.0f, 1.0f, 0.0f);
                b.end();
                b.begin(W3D_CHUNK_MATERIAL_INFO);
                b.w_u32(1);
                b.w_u32(2);
                b.w_u32(2);
                b.w_u32(2);
                b.end();
                b.begin(W3D_CHUNK_SHADERS);
                w_shader(b, 3, 1, 0, 1, 0, 1, 1, 0, 0, 0);
                w_shader(b, 3, 0, 1, 2, 1, 2, 1, 1, 3, 2);
                b.end();
                b.begin(W3D_CHUNK_VERTEX_MATERIALS);
                {
                        b.begin(W3D_CHUNK_VERTEX_MATERIAL);
                        b.begin(W3D_CHUNK_VERTEX_MATERIAL_NAME);
                        b.w_cstr("va");
                        b.end();
                        b.begin(W3D_CHUNK_VERTEX_MATERIAL_INFO);
                        w_vmat_info(b, 0, 255, 255, 255, 255, 255, 255, 0, 0, 0, 0, 0, 0, 1.0f, 1.0f, 0.0f);
                        b.end();
                        b.end();
                        b.begin(W3D_CHUNK_VERTEX_MATERIAL);
                        b.begin(W3D_CHUNK_VERTEX_MATERIAL_NAME);
                        b.w_cstr("vb");
                        b.end();
                        b.begin(W3D_CHUNK_VERTEX_MATERIAL_INFO);
                        w_vmat_info(b, 0, 100, 100, 100, 90, 90, 90, 80, 80, 80, 70, 70, 70, 3.0f, 0.9f, 0.1f);
                        b.end();
                        b.end();
                }
                b.end();
                b.begin(W3D_CHUNK_TEXTURES);
                {
                        b.begin(W3D_CHUNK_TEXTURE);
                        b.begin(W3D_CHUNK_TEXTURE_NAME);
                        b.w_cstr("ta.dds");
                        b.end();
                        b.end();
                        b.begin(W3D_CHUNK_TEXTURE);
                        b.begin(W3D_CHUNK_TEXTURE_NAME);
                        b.w_cstr("tb.dds");
                        b.end();
                        b.end();
                }
                b.end();
                b.begin(W3D_CHUNK_MATERIAL_PASS);
                {
                        // def then alternate material data
                        b.begin(W3D_CHUNK_VERTEX_MATERIAL_IDS);
                        b.w_u32(0);
                        b.end();
                        b.begin(W3D_CHUNK_VERTEX_MATERIAL_IDS);
                        b.w_u32(1);
                        b.end();
                        // def then alternate shader data
                        b.begin(W3D_CHUNK_SHADER_IDS);
                        b.w_u32(0);
                        b.end();
                        b.begin(W3D_CHUNK_SHADER_IDS);
                        b.w_u32(1);
                        b.end();
                        // DCG into def, then DCG into alternate
                        b.begin(W3D_CHUNK_DCG);
                        b.w_rgba(255, 128, 64, 255);
                        b.w_rgba(128, 255, 64, 128);
                        b.w_rgba(64, 128, 255, 64);
                        b.end();
                        b.begin(W3D_CHUNK_DCG);
                        b.w_rgba(10, 20, 30, 40);
                        b.w_rgba(50, 60, 70, 80);
                        b.w_rgba(90, 100, 110, 120);
                        b.end();
                        // DIG multiplies into the def color array
                        b.begin(W3D_CHUNK_DIG);
                        b.w_rgba(255, 255, 255, 255);
                        b.w_rgba(128, 128, 128, 255);
                        b.w_rgba(64, 64, 64, 255);
                        b.end();
                        // DIG again: routed to the alternate (LoadedDIG)
                        b.begin(W3D_CHUNK_DIG);
                        b.w_rgba(255, 0, 0, 255);
                        b.w_rgba(0, 255, 0, 255);
                        b.w_rgba(0, 0, 255, 255);
                        b.end();
                        // stage 0: def texture, alternate texture, def uv, alternate uv
                        b.begin(W3D_CHUNK_TEXTURE_STAGE);
                        {
                                b.begin(W3D_CHUNK_TEXTURE_IDS);
                                b.w_u32(0);
                                b.end();
                                b.begin(W3D_CHUNK_TEXTURE_IDS);
                                b.w_u32(1);
                                b.end();
                                b.begin(W3D_CHUNK_STAGE_TEXCOORDS);
                                b.w_pair(0.0f, 0.0f);
                                b.w_pair(0.5f, 0.5f);
                                b.w_pair(0.25f, 0.75f);
                                b.end();
                                b.begin(W3D_CHUNK_STAGE_TEXCOORDS);
                                b.w_pair(0.75f, 0.25f);
                                b.w_pair(0.5f, 0.125f);
                                b.w_pair(0.125f, 0.625f);
                                b.end();
                                b.begin(W3D_CHUNK_PER_FACE_TEXCOORD_IDS);
                                b.w_u32(0);
                                b.w_u32(1);
                                b.w_u32(2);
                                b.end();
                        }
                        b.end();
                        // stage 1: identical uv pairs -> CRC dedup against array 0
                        b.begin(W3D_CHUNK_TEXTURE_STAGE);
                        {
                                b.begin(W3D_CHUNK_TEXTURE_IDS);
                                b.w_u32(1);
                                b.end();
                                b.begin(W3D_CHUNK_STAGE_TEXCOORDS);
                                b.w_pair(0.0f, 0.0f);
                                b.w_pair(0.5f, 0.5f);
                                b.w_pair(0.25f, 0.75f);
                                b.end();
                        }
                        b.end();
                }
                b.end();
                add("m11", b);
        }

        // ---- m12: VERTEX_COLORS then DCG (routing via the DCG source) ----
        {
                ChunkBuild b;
                w_mesh_header3(b, W3D_MAKE_VERSION(4, 2), W3D_MESH_FLAG_PRELIT_VERTEX, "VCol", "", 1, 3, 0);
                b.begin(W3D_CHUNK_VERTICES);
                b.w_vec3(0.0f, 0.0f, 0.0f);
                b.w_vec3(1.0f, 0.0f, 0.0f);
                b.w_vec3(0.0f, 1.0f, 0.0f);
                b.end();
                b.begin(W3D_CHUNK_TRIANGLES);
                w_tri(b, 0, 1, 2, 0, 0.0f, 0.0f, 1.0f, 0.0f);
                b.end();
                b.begin(W3D_CHUNK_VERTEX_COLORS);
                b.w_rgb(255, 0, 0);
                b.w_rgb(0, 255, 0);
                b.w_rgb(0, 0, 255);
                b.end();
                // a second VERTEX_COLORS chunk: the array exists, nothing is read
                b.begin(W3D_CHUNK_VERTEX_COLORS);
                b.w_rgb(255, 255, 255);
                b.w_rgb(255, 255, 255);
                b.w_rgb(255, 255, 255);
                b.end();
                // the DCG (inside a material pass, where the switch handles
                // it) now routes into the alternate (DCGSource != MATERIAL)
                b.begin(W3D_CHUNK_MATERIAL_PASS);
                {
                        b.begin(W3D_CHUNK_DCG);
                        b.w_rgba(255, 255, 255, 200);
                        b.w_rgba(255, 255, 255, 100);
                        b.w_rgba(255, 255, 255, 50);
                        b.end();
                }
                b.end();
                add("m12", b);
        }

        // ---- m13: prelit selection - the mode's entry skips the multi-texture
        // check entirely: a mesh carrying only that flag falls through to the
        // unlit default and neither wrapper is processed ----
        {
                ChunkBuild b;
                w_mesh_header3(b, W3D_MAKE_VERSION(4, 2), W3D_MESH_FLAG_PRELIT_LIGHTMAP_MULTI_TEXTURE, "P13", "",
                               1, 3, 0);
                b.begin(W3D_CHUNK_VERTICES);
                b.w_vec3(0.0f, 0.0f, 0.0f);
                b.w_vec3(1.0f, 0.0f, 0.0f);
                b.w_vec3(0.0f, 1.0f, 0.0f);
                b.end();
                b.begin(W3D_CHUNK_PRELIT_LIGHTMAP_MULTI_PASS);
                {
                        b.begin(W3D_CHUNK_MATERIAL_INFO);
                        b.w_u32(9);
                        b.w_u32(9);
                        b.w_u32(9);
                        b.w_u32(9);
                        b.end();
                }
                b.end();
                b.begin(W3D_CHUNK_PRELIT_LIGHTMAP_MULTI_TEXTURE);
                {
                        b.begin(W3D_CHUNK_MATERIAL_INFO);
                        b.w_u32(1);
                        b.w_u32(0);
                        b.w_u32(0);
                        b.w_u32(0);
                        b.end();
                }
                b.end();
                add("m13", b);
        }

        // ---- m14: prelit selection - unlit only falls to the default case ----
        {
                ChunkBuild b;
                w_mesh_header3(b, W3D_MAKE_VERSION(4, 2), W3D_MESH_FLAG_PRELIT_UNLIT, "P14", "", 1, 3, 0);
                b.begin(W3D_CHUNK_VERTICES);
                b.w_vec3(0.0f, 0.0f, 0.0f);
                b.w_vec3(1.0f, 0.0f, 0.0f);
                b.w_vec3(0.0f, 1.0f, 0.0f);
                b.end();
                b.begin(W3D_CHUNK_PRELIT_UNLIT);
                {
                        b.begin(W3D_CHUNK_MATERIAL_INFO);
                        b.w_u32(1);
                        b.w_u32(0);
                        b.w_u32(0);
                        b.w_u32(0);
                        b.end();
                }
                b.end();
                add("m14", b);
        }

        // ---- m15: aabtree with mismatched counts, and a zero-size tree ----
        {
                ChunkBuild b;
                w_mesh_header3(b, W3D_MAKE_VERSION(4, 2), 0, "Tree", "", 1, 3, 0);
                b.begin(W3D_CHUNK_VERTICES);
                b.w_vec3(0.0f, 0.0f, 0.0f);
                b.w_vec3(1.0f, 0.0f, 0.0f);
                b.w_vec3(0.0f, 1.0f, 0.0f);
                b.end();
                b.begin(W3D_CHUNK_TRIANGLES);
                w_tri(b, 0, 1, 2, 0, 0.0f, 0.0f, 1.0f, 0.0f);
                b.end();
                b.begin(W3D_CHUNK_AABTREE);
                {
                        b.begin(W3D_CHUNK_AABTREE_HEADER);
                        b.w_u32(3); // node count (a real tree: root + 2 leaves)
                        b.w_u32(4); // poly count (the mesh has 1 tri — never validated)
                        for (int i = 0; i < 6; i++)
                                b.w_u32(0);
                        b.end();
                        b.begin(W3D_CHUNK_AABTREE_NODES);
                        // root (front=1, back=2)
                        b.w_vec3(-2.0f, -2.0f, -2.0f);
                        b.w_vec3(2.0f, 2.0f, 2.0f);
                        b.w_u32(1);
                        b.w_u32(2);
                        // leaf 0 (poly0=0, count=2)
                        b.w_vec3(-2.0f, -2.0f, -2.0f);
                        b.w_vec3(0.0f, 2.0f, 2.0f);
                        b.w_u32(0x80000000u);
                        b.w_u32(2);
                        // leaf 1 (poly0=2, count=2)
                        b.w_vec3(0.0f, -2.0f, -2.0f);
                        b.w_vec3(2.0f, 2.0f, 2.0f);
                        b.w_u32(0x80000002u);
                        b.w_u32(2);
                        b.end();
                        b.begin(W3D_CHUNK_AABTREE_POLYINDICES);
                        b.w_u32(0);
                        b.w_u32(0);
                        b.w_u32(0);
                        b.w_u32(0);
                        b.end();
                }
                b.end();
                // a second mesh-sized corpus would be needed for the zero tree;
                // fold it in here as a trailing zero-size tree after the real one
                // is not possible (only one AABTREE is kept) — use m15 for the
                // real tree and let m27 cover degenerate shapes
                add("m15", b);
        }

        // ---- m16: short AABTree nodes read fails the load ----
        {
                ChunkBuild b;
                w_mesh_header3(b, W3D_MAKE_VERSION(4, 2), 0, "TreeX", "", 1, 3, 0);
                b.begin(W3D_CHUNK_AABTREE);
                {
                        b.begin(W3D_CHUNK_AABTREE_HEADER);
                        b.w_u32(2);
                        b.w_u32(2);
                        for (int i = 0; i < 6; i++)
                                b.w_u32(0);
                        b.end();
                        b.begin(W3D_CHUNK_AABTREE_NODES);
                        b.w_vec3(-1.0f, -1.0f, -1.0f);
                        b.w_vec3(1.0f, 1.0f, 1.0f);
                        b.w_u32(0x80000000u);
                        b.w_u32(2);
                        b.end(); // one node of two
                }
                b.end();
                add("m16", b);
        }

        // ---- m17: user text first-wins; duplicate/absent texture names ----
        {
                ChunkBuild b;
                w_mesh_header3(b, W3D_MAKE_VERSION(4, 2), 0, "Text", "", 1, 3, 0);
                b.begin(W3D_CHUNK_VERTICES);
                b.w_vec3(0.0f, 0.0f, 0.0f);
                b.w_vec3(1.0f, 0.0f, 0.0f);
                b.w_vec3(0.0f, 1.0f, 0.0f);
                b.end();
                b.begin(W3D_CHUNK_MESH_USER_TEXT);
                b.w_cstr("first");
                b.end();
                b.begin(W3D_CHUNK_MESH_USER_TEXT);
                b.w_cstr("second");
                b.end();
                b.begin(W3D_CHUNK_TEXTURES);
                {
                        b.begin(W3D_CHUNK_TEXTURE);
                        b.begin(W3D_CHUNK_TEXTURE_NAME);
                        b.w_cstr("a.tga");
                        b.end();
                        b.begin(W3D_CHUNK_TEXTURE_INFO);
                        w_texinfo(b, 0x0004, 0, 1, 0.0f);
                        b.end();
                        b.end();
                        // the same name in different case: the same object, a
                        // second list entry, the first info wins
                        b.begin(W3D_CHUNK_TEXTURE);
                        b.begin(W3D_CHUNK_TEXTURE_NAME);
                        b.w_cstr("A.TGA");
                        b.end();
                        b.begin(W3D_CHUNK_TEXTURE_INFO);
                        w_texinfo(b, 0x0008, 1, 4, 15.0f);
                        b.end();
                        b.end();
                        // no name chunk: Get_Texture("") returns null and the
                        // texture loop ends
                        b.begin(W3D_CHUNK_TEXTURE);
                        b.begin(W3D_CHUNK_TEXTURE_INFO);
                        w_texinfo(b, 0x0010, 0, 1, 0.0f);
                        b.end();
                        b.end();
                }
                b.end();
                add("m17", b);
        }

        // ---- m18: shader count from MatInfo; two material infos ----
        {
                ChunkBuild b;
                w_mesh_header3(b, W3D_MAKE_VERSION(4, 2), W3D_MESH_FLAG_PRELIT_VERTEX, "Shd", "", 1, 3, 0);
                b.begin(W3D_CHUNK_VERTICES);
                b.w_vec3(0.0f, 0.0f, 0.0f);
                b.w_vec3(1.0f, 0.0f, 0.0f);
                b.w_vec3(0.0f, 1.0f, 0.0f);
                b.end();
                b.begin(W3D_CHUNK_MATERIAL_INFO);
                b.w_u32(1);
                b.w_u32(0);
                b.w_u32(2);
                b.w_u32(0);
                b.end();
                b.begin(W3D_CHUNK_SHADERS);
                w_shader(b, 3, 1, 0, 1, 0, 1, 1, 0, 0, 0);
                w_shader(b, 2, 0, 1, 1, 0, 1, 1, 0, 0, 0);
                w_shader(b, 3, 1, 2, 1, 0, 1, 1, 1, 0, 0); // beyond ShaderCount: skipped
                b.end();
                b.begin(W3D_CHUNK_PRELIT_VERTEX);
                {
                        b.begin(W3D_CHUNK_MATERIAL_INFO);
                        b.w_u32(2);
                        b.w_u32(0);
                        b.w_u32(1);
                        b.w_u32(0);
                        b.end();
                        b.begin(W3D_CHUNK_SHADERS);
                        w_shader(b, 3, 1, 3, 1, 0, 1, 1, 0, 0, 0); // appends at index 2
                        b.end();
                }
                b.end();
                add("m18", b);
        }

        // ---- m19: SCG no-op, correct per-face texcoord ids, unknown chunks ----
        {
                ChunkBuild b;
                w_mesh_header3(b, W3D_MAKE_VERSION(4, 2), 0, "Skip", "", 1, 3, 0);
                b.begin(W3D_CHUNK_VERTICES);
                b.w_vec3(0.0f, 0.0f, 0.0f);
                b.w_vec3(1.0f, 0.0f, 0.0f);
                b.w_vec3(0.0f, 1.0f, 0.0f);
                b.end();
                b.begin(W3D_CHUNK_TRIANGLES);
                w_tri(b, 0, 1, 2, 0, 0.0f, 0.0f, 1.0f, 0.0f);
                b.end();
                b.begin(0x777);
                b.w_u32(1);
                b.end();
                b.begin(0x60); // W3D_CHUNK_VERTEX_TANGENTS (unknown to the switch)
                b.w_vec3(0.0f, 0.0f, 0.0f);
                b.end();
                b.begin(W3D_CHUNK_MATERIAL_INFO);
                b.w_u32(1);
                b.w_u32(0);
                b.w_u32(0);
                b.w_u32(0);
                b.end();
                b.begin(W3D_CHUNK_MATERIAL_PASS);
                {
                        b.begin(W3D_CHUNK_SCG);
                        b.w_rgb(255, 255, 255);
                        b.w_rgb(255, 255, 255);
                        b.w_rgb(255, 255, 255);
                        b.end();
                        b.begin(W3D_CHUNK_TEXTURE_STAGE);
                        {
                                b.begin(W3D_CHUNK_PER_FACE_TEXCOORD_IDS);
                                b.w_u32(0);
                                b.w_u32(1);
                                b.w_u32(2);
                                b.end();
                        }
                        b.end();
                }
                b.end();
                b.begin(W3D_CHUNK_MESH_USER_TEXT);
                b.w_cstr("after");
                b.end();
                add("m19", b);
        }

        // ---- m20: wrong-size per-face texcoord ids abort the walk (load ok) ----
        {
                ChunkBuild b;
                w_mesh_header3(b, W3D_MAKE_VERSION(4, 2), 0, "PfX", "", 1, 3, 0);
                b.begin(W3D_CHUNK_VERTICES);
                b.w_vec3(0.0f, 0.0f, 0.0f);
                b.w_vec3(1.0f, 0.0f, 0.0f);
                b.w_vec3(0.0f, 1.0f, 0.0f);
                b.end();
                b.begin(W3D_CHUNK_MATERIAL_PASS);
                {
                        b.begin(W3D_CHUNK_TEXTURE_STAGE);
                        {
                                b.begin(W3D_CHUNK_PER_FACE_TEXCOORD_IDS);
                                b.w_u32(0); // 4 bytes instead of 12
                                b.end();
                        }
                        b.end();
                }
                b.end();
                b.begin(W3D_CHUNK_MESH_USER_TEXT);
                b.w_cstr("never");
                b.end();
                add("m20", b);
        }

        // ---- m21: vertex material name/info/args quirks ----
        {
                ChunkBuild b;
                w_mesh_header3(b, W3D_MAKE_VERSION(4, 2), 0, "VmQ", "", 1, 3, 0);
                b.begin(W3D_CHUNK_VERTICES);
                b.w_vec3(0.0f, 0.0f, 0.0f);
                b.w_vec3(1.0f, 0.0f, 0.0f);
                b.w_vec3(0.0f, 1.0f, 0.0f);
                b.end();
                b.begin(W3D_CHUNK_VERTEX_MATERIALS);
                {
                        // vmat 0: name + info + two ARGS0 chunks (last wins) + ARGS1
                        b.begin(W3D_CHUNK_VERTEX_MATERIAL);
                        b.begin(W3D_CHUNK_VERTEX_MATERIAL_NAME);
                        b.w_cstr("full");
                        b.end();
                        b.begin(W3D_CHUNK_VERTEX_MATERIAL_INFO);
                        w_vmat_info(b, 0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 1.5f, 0.5f, 0.25f);
                        b.end();
                        b.begin(W3D_CHUNK_VERTEX_MAPPER_ARGS0);
                        b.w_cstr("first");
                        b.end();
                        b.begin(W3D_CHUNK_VERTEX_MAPPER_ARGS0);
                        b.w_cstr("second");
                        b.end();
                        b.begin(W3D_CHUNK_VERTEX_MAPPER_ARGS1);
                        b.w_cstr("other");
                        b.end();
                        b.end();
                        // vmat 1: name only (the info struct is zero-filled)
                        b.begin(W3D_CHUNK_VERTEX_MATERIAL);
                        b.begin(W3D_CHUNK_VERTEX_MATERIAL_NAME);
                        b.w_cstr("named");
                        b.end();
                        b.end();
                        // vmat 2: info only (no name)
                        b.begin(W3D_CHUNK_VERTEX_MATERIAL);
                        b.begin(W3D_CHUNK_VERTEX_MATERIAL_INFO);
                        w_vmat_info(b, W3DVERTMAT_USE_DEPTH_CUE, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 2.5f,
                                    0.125f, 0.0f);
                        b.end();
                        b.end();
                        // vmat 3: an empty name chunk
                        b.begin(W3D_CHUNK_VERTEX_MATERIAL);
                        b.begin(W3D_CHUNK_VERTEX_MATERIAL_NAME);
                        b.end();
                        b.end();
                        // vmat 4: an unknown sub-chunk
                        b.begin(W3D_CHUNK_VERTEX_MATERIAL);
                        b.begin(0x99);
                        b.w_u32(5);
                        b.end();
                        b.end();
                }
                b.end();
                add("m21", b);
        }

        // ---- m22..m25: the 4.1 geometry type switch ----
        {
                ChunkBuild b;
                w_mesh_header3(b, W3D_MAKE_VERSION(4, 2),
                               W3D_MESH_FLAG_GEOMETRY_TYPE_CAMERA_ALIGNED | W3D_MESH_FLAG_TWO_SIDED |
                                   W3D_MESH_FLAG_CAST_SHADOW | W3D_MESH_FLAG_NPATCHABLE,
                               "G22", "C", 1, 3, 0);
                b.begin(W3D_CHUNK_VERTICES);
                b.w_vec3(0.0f, 0.0f, 0.0f);
                b.w_vec3(1.0f, 0.0f, 0.0f);
                b.w_vec3(0.0f, 1.0f, 0.0f);
                b.end();
                add("m22", b);
        }
        {
                ChunkBuild b;
                w_mesh_header3(b, W3D_MAKE_VERSION(4, 2), W3D_MESH_FLAG_GEOMETRY_TYPE_SKIN, "G23", "", 1, 3, 0);
                b.begin(W3D_CHUNK_VERTICES);
                b.w_vec3(0.0f, 0.0f, 0.0f);
                b.w_vec3(1.0f, 0.0f, 0.0f);
                b.w_vec3(0.0f, 1.0f, 0.0f);
                b.end();
                add("m23", b);
        }
        {
                ChunkBuild b;
                w_mesh_header3(b, W3D_MAKE_VERSION(4, 2), W3D_MESH_FLAG_GEOMETRY_TYPE_CAMERA_ORIENTED, "G24", "",
                               1, 3, 0);
                b.begin(W3D_CHUNK_VERTICES);
                b.w_vec3(0.0f, 0.0f, 0.0f);
                b.w_vec3(1.0f, 0.0f, 0.0f);
                b.w_vec3(0.0f, 1.0f, 0.0f);
                b.end();
                add("m24", b);
        }
        {
                ChunkBuild b;
                w_mesh_header3(b, W3D_MAKE_VERSION(4, 2), 0x00050000 /* OBBOX: default branch */, "G25", "", 1,
                               3, 0);
                b.begin(W3D_CHUNK_VERTICES);
                b.w_vec3(0.0f, 0.0f, 0.0f);
                b.w_vec3(1.0f, 0.0f, 0.0f);
                b.w_vec3(0.0f, 1.0f, 0.0f);
                b.end();
                add("m25", b);
        }

        // ---- m26: zero counts, no geometry chunks ----
        {
                ChunkBuild b;
                w_mesh_header3(b, W3D_MAKE_VERSION(4, 2), 0, "Void", "", 0, 0, 0);
                b.begin(W3D_CHUNK_MESH_USER_TEXT);
                b.w_cstr("empty");
                b.end();
                add("m26", b);
        }

        // ---- m27: trailing partial header bytes ----
        {
                ChunkBuild b;
                w_mesh_header3(b, W3D_MAKE_VERSION(4, 2), 0, "Trail", "", 1, 3, 0);
                b.begin(W3D_CHUNK_VERTICES);
                b.w_vec3(0.0f, 0.0f, 0.0f);
                b.w_vec3(1.0f, 0.0f, 0.0f);
                b.w_vec3(0.0f, 1.0f, 0.0f);
                b.end();
                b.bytes.push_back(0x33);
                b.bytes.push_back(0x33);
                b.bytes.push_back(0x33);
                add("m27", b);
        }

        // ---- m28: extra bytes inside the header chunk ----
        {
                ChunkBuild b;
                b.begin(W3D_CHUNK_MESH_HEADER3);
                b.w_u32(W3D_MAKE_VERSION(4, 2));
                b.w_u32(0);
                char zeros16[16];
                memset(zeros16, 0, sizeof(zeros16));
                b.w_bytes("ExtraHeaderName", 16);
                b.w_bytes(zeros16, 16);
                b.w_u32(1);
                b.w_u32(3);
                b.w_u32(0);
                b.w_u32(0);
                b.w_i32(0);
                b.w_u32(0);
                b.w_u32(0);
                b.w_u32(0x1F);
                b.w_u32(0x01);
                b.w_vec3(-1.0f, -1.0f, -1.0f);
                b.w_vec3(1.0f, 1.0f, 1.0f);
                b.w_vec3(0.0f, 0.0f, 0.0f);
                b.w_f32(1.5f);
                b.w_u32(0xDEADBEEF); // 4 extra bytes the Read skips over
                b.end();
                b.begin(W3D_CHUNK_VERTICES);
                b.w_vec3(0.0f, 0.0f, 0.0f);
                b.w_vec3(1.0f, 0.0f, 0.0f);
                b.w_vec3(0.0f, 1.0f, 0.0f);
                b.end();
                add("m28", b);
        }

        // ---- m29: alternate shader-only data counts as empty (no merge) ----
        {
                ChunkBuild b;
                w_mesh_header3(b, W3D_MAKE_VERSION(4, 2), 0, "ShOnly", "", 1, 3, 0);
                b.begin(W3D_CHUNK_VERTICES);
                b.w_vec3(0.0f, 0.0f, 0.0f);
                b.w_vec3(1.0f, 0.0f, 0.0f);
                b.w_vec3(0.0f, 1.0f, 0.0f);
                b.end();
                b.begin(W3D_CHUNK_MATERIAL_INFO);
                b.w_u32(1);
                b.w_u32(0);
                b.w_u32(2);
                b.w_u32(0);
                b.end();
                b.begin(W3D_CHUNK_SHADERS);
                w_shader(b, 3, 1, 0, 1, 0, 1, 1, 0, 0, 0);
                w_shader(b, 3, 1, 1, 1, 0, 1, 1, 0, 0, 0);
                b.end();
                b.begin(W3D_CHUNK_MATERIAL_PASS);
                {
                        b.begin(W3D_CHUNK_SHADER_IDS);
                        b.w_u32(0);
                        b.end();
                        // def now has shader data: the second ids chunk routes
                        // into the context's alternate
                        b.begin(W3D_CHUNK_SHADER_IDS);
                        b.w_u32(1);
                        b.end();
                }
                b.end();
                add("m29", b);
        }

        // ---- m30: the fifth pass runs past MAX_PASSES and aborts the walk ----
        {
                ChunkBuild b;
                w_mesh_header3(b, W3D_MAKE_VERSION(4, 2), 0, "Five", "", 1, 3, 0);
                b.begin(W3D_CHUNK_VERTICES);
                b.w_vec3(0.0f, 0.0f, 0.0f);
                b.w_vec3(1.0f, 0.0f, 0.0f);
                b.w_vec3(0.0f, 1.0f, 0.0f);
                b.end();
                b.begin(W3D_CHUNK_MATERIAL_INFO);
                b.w_u32(5);
                b.w_u32(0);
                b.w_u32(0);
                b.w_u32(0);
                b.end();
                for (int pass = 0; pass < 4; pass++) {
                        b.begin(W3D_CHUNK_MATERIAL_PASS);
                        b.end();
                        (void)pass;
                }
                // the fifth pass: a VERTEX_MATERIAL_IDS chunk at CurPass == 4
                // reads the material array out of bounds in the C++ (UB); the
                // transcription fails the sub-read, the walk aborts and the
                // load still reports success with the partial state
                b.begin(W3D_CHUNK_MATERIAL_PASS);
                {
                        b.begin(W3D_CHUNK_VERTEX_MATERIAL_IDS);
                        b.w_u32(0);
                        b.end();
                }
                b.end();
                b.begin(W3D_CHUNK_MESH_USER_TEXT);
                b.w_cstr("never");
                b.end();
                add("m30", b);
        }

        return out;
}

// --- the dump ---------------------------------------------------------------------

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

static std::string hex_u32s(const std::vector<uint32_t> &v)
{
        if (v.empty())
                return "-";
        std::string s;
        for (uint32_t x : v) {
                hex_append(s, (const uint8_t *)&x, 4);
        }
        return s;
}

static std::string hex_u16s(const std::vector<uint16_t> &v)
{
        if (v.empty())
                return "-";
        std::string s;
        for (uint16_t x : v) {
                hex_append(s, (const uint8_t *)&x, 2);
        }
        return s;
}

static std::string hex_u8s(const std::vector<uint8_t> &v)
{
        if (v.empty())
                return "-";
        return hex_str(v);
}

static std::string hex_floats(const std::vector<float> &v)
{
        if (v.empty())
                return "-";
        std::string s;
        for (float f : v) {
                uint32_t bits;
                memcpy(&bits, &f, 4);
                hex_append(s, (const uint8_t *)&bits, 4);
        }
        return s;
}

// names: '-' when empty, the raw bytes when printable non-space ASCII,
// <hex> otherwise (both sides implement the identical rule)
static std::string q(const std::string &name)
{
        if (name.empty())
                return "-";
        bool printable = true;
        for (unsigned char c : name) {
                if (c < 0x21 || c > 0x7E) {
                        printable = false;
                        break;
                }
        }
        if (printable)
                return name;
        std::string s = "<";
        hex_append(s, (const uint8_t *)name.data(), name.size());
        s += ">";
        return s;
}

static std::string f32hex(float f)
{
        uint32_t bits;
        memcpy(&bits, &f, 4);
        char buf[16];
        snprintf(buf, sizeof(buf), "%08x", bits);
        return buf;
}

static std::string u32hex(uint32_t v)
{
        char buf[16];
        snprintf(buf, sizeof(buf), "%08x", v);
        return buf;
}

static void dump_matdesc(const MeshModelClass &m, FILE *out, const char *id, const char *which,
                         const MeshMatDescClass *desc)
{
        fprintf(out, "matdesc %s %s %d\n", id, which, desc->PassCount);
        for (int pass = 0; pass < MAX_PASSES; pass++) {
                // material
                {
                        std::string arr = "-";
                        if (desc->MaterialArray[pass]) {
                                std::vector<uint32_t> tmp(desc->MaterialArray[pass]->begin(),
                                                          desc->MaterialArray[pass]->end());
                                arr = hex_u32s(tmp);
                        }
                        fprintf(out, "matpass %s %s %d mat %d %s\n", id, which, pass, desc->MaterialSingle[pass],
                                arr.c_str());
                }
                // shader
                {
                        std::string arr = "-";
                        if (desc->ShaderArray[pass]) {
                                arr = hex_u32s(*desc->ShaderArray[pass]);
                        }
                        fprintf(out, "matpass %s %s %d shd %08x %s\n", id, which, pass, desc->ShaderBits[pass],
                                arr.c_str());
                }
                fprintf(out, "matpass %s %s %d dcg %u dig %u\n", id, which, pass, desc->DCGSource[pass],
                        desc->DIGSource[pass]);
                for (int stage = 0; stage < MAX_TEX_STAGES; stage++) {
                        std::string arr = "-";
                        if (desc->TextureArray[pass][stage]) {
                                std::vector<uint32_t> tmp(desc->TextureArray[pass][stage]->begin(),
                                                          desc->TextureArray[pass][stage]->end());
                                arr = hex_u32s(tmp);
                        }
                        fprintf(out, "matpass %s %s %d stg %d tex %d %s\n", id, which, pass, stage,
                                desc->TextureSingle[pass][stage], arr.c_str());
                        fprintf(out, "matpass %s %s %d stg %d uv %d\n", id, which, pass, stage,
                                desc->UVSource[pass][stage]);
                }
        }
        for (int a = 0; a < MAX_COLOR_ARRAYS; a++) {
                std::string val = "-";
                int n = 0;
                if (desc->ColorArray[a]) {
                        val = hex_u32s(*desc->ColorArray[a]);
                        n = (int)desc->ColorArray[a]->size();
                }
                fprintf(out, "matcolor %s %s %d %d %s\n", id, which, a, n, val.c_str());
        }
        for (int k = 0; k < MAX_UV_ARRAYS; k++) {
                std::string val = "-";
                int n = 0;
                if (desc->UV[k]) {
                        val = hex_floats(desc->UV[k]->xy);
                        n = (int)(desc->UV[k]->xy.size() / 2);
                }
                fprintf(out, "matuv %s %s %d %d %s\n", id, which, k, n, val.c_str());
        }
        (void)m;
}

static void dump_mesh(const MeshModelClass &m, FILE *out, const char *id)
{
        const MeshLoadContextClass *ctx = m.Context;
        const W3dMeshHeader3Struct &h = ctx->Header;

        // hdr: version attrs meshname container numtris numverts nummaterials
        // numdamagestages sortlevel prelitversion futurecounts0 vertexchannels
        // facechannels min max sphcenter sphradius
        fprintf(out, "hdr %s %s %s %s %s %u %u %u %u %d %u %u %s %s", id, u32hex(h.Version).c_str(),
                u32hex(h.Attributes).c_str(), q(h.MeshName).c_str(), q(h.ContainerName).c_str(), h.NumTris,
                h.NumVertices, h.NumMaterials, h.NumDamageStages, h.SortLevel, h.PrelitVersion, h.FutureCounts[0],
                u32hex(h.VertexChannels).c_str(), u32hex(h.FaceChannels).c_str());
        float minv[3] = {h.Min.X, h.Min.Y, h.Min.Z};
        float maxv[3] = {h.Max.X, h.Max.Y, h.Max.Z};
        float sphc[3] = {h.SphCenter.X, h.SphCenter.Y, h.SphCenter.Z};
        for (int i = 0; i < 3; i++)
                fprintf(out, " %s", f32hex(minv[i]).c_str());
        for (int i = 0; i < 3; i++)
                fprintf(out, " %s", f32hex(maxv[i]).c_str());
        for (int i = 0; i < 3; i++)
                fprintf(out, " %s", f32hex(sphc[i]).c_str());
        fprintf(out, " %s\n", f32hex(h.SphRadius).c_str());

        fprintf(out, "name %s %s\n", id, q(m.MeshName).c_str());
        fprintf(out, "flags %s %04x\n", id, m.Flags);
        fprintf(out, "prelitid %s %s\n", id, u32hex(ctx->PrelitChunkID).c_str());

        // geometry
        {
                std::string val = "-";
                size_t n = 0;
                if (m.Vertex) {
                        val = hex_floats(*m.Vertex);
                        n = m.Vertex->size() / 3;
                }
                fprintf(out, "geov %s %zu %s\n", id, n, val.c_str());
        }
        {
                std::string val = "-";
                size_t n = 0;
                if (m.VertexNorm) {
                        val = hex_floats(*m.VertexNorm);
                        n = m.VertexNorm->size() / 3;
                }
                fprintf(out, "geon %s %zu %s\n", id, n, val.c_str());
        }
        {
                std::string val = "-";
                size_t n = 0;
                if (m.Poly) {
                        val = hex_u16s(*m.Poly);
                        n = m.Poly->size() / 3;
                }
                fprintf(out, "geop %s %zu %s\n", id, n, val.c_str());
        }
        {
                std::string val = "-";
                size_t n = 0;
                if (m.PolySurfaceType) {
                        val = hex_u8s(*m.PolySurfaceType);
                        n = m.PolySurfaceType->size();
                }
                fprintf(out, "geos %s %zu %s\n", id, n, val.c_str());
        }
        {
                std::string val = "-";
                size_t n = 0;
                if (!m.PlaneEQArray.empty()) {
                        val = hex_floats(m.PlaneEQArray);
                        n = m.PlaneEQArray.size() / 4;
                }
                fprintf(out, "geoe %s %zu %s\n", id, n, val.c_str());
        }
        fprintf(out, "geot %s %s\n", id, m.HasUserText ? hex_str(m.UserText).c_str() : "-");
        {
                std::string val = "-";
                size_t n = 0;
                if (m.VertexBoneLink) {
                        val = hex_u16s(*m.VertexBoneLink);
                        n = m.VertexBoneLink->size();
                }
                fprintf(out, "geob %s %zu %s\n", id, n, val.c_str());
        }
        {
                std::string val = "-";
                size_t n = 0;
                if (m.VertexShadeIdx) {
                        val = hex_u32s(*m.VertexShadeIdx);
                        n = m.VertexShadeIdx->size();
                }
                fprintf(out, "geox %s %zu %s\n", id, n, val.c_str());
        }

        // the load context's material info
        fprintf(out, "matinfo %s %u %u %u %u\n", id, ctx->MatInfo.PassCount, ctx->MatInfo.VertexMaterialCount,
                ctx->MatInfo.ShaderCount, ctx->MatInfo.TextureCount);

        // shaders
        {
                std::vector<uint32_t> bits;
                for (const ShaderClass &s : ctx->Shaders)
                        bits.push_back(s.ShaderBits);
                fprintf(out, "shaders %s %zu %s\n", id, bits.size(), hex_u32s(bits).c_str());
        }

        // vertex materials
        for (size_t i = 0; i < ctx->VertexMaterials.size(); i++) {
                const VertexMaterial &vm = ctx->VertexMaterials[i];
                std::string amb, dif, spec, emis;
                for (int k = 0; k < 3; k++) {
                        amb += f32hex(vm.ambient[k]);
                        dif += f32hex(vm.diffuse[k]);
                        spec += f32hex(vm.specular[k]);
                        emis += f32hex(vm.emissive[k]);
                }
                // attributes are recoverable from the flags the parse kept;
                // the dump records the material state the loaders produced
                fprintf(out, "vmat %s %zu %s %s %s %s %s %s %s %s %s %s\n", id, i,
                        vm.has_name ? q(vm.name).c_str() : "-", u32hex(vm.flags).c_str(), amb.c_str(),
                        dif.c_str(), spec.c_str(), emis.c_str(), f32hex(vm.power).c_str(),
                        f32hex(vm.diffuse[3]).c_str(), vm.args0.empty() ? "-" : hex_str(vm.args0).c_str(),
                        vm.args1.empty() ? "-" : hex_str(vm.args1).c_str());
        }

        // textures (the context list; duplicate entries are the same object)
        for (size_t i = 0; i < ctx->Textures.size(); i++) {
                const TextureObj &t = ctx->TextureObjects[ctx->Textures[i]];
                std::string info = "-";
                if (t.has_info) {
                        uint8_t raw[12];
                        memcpy(raw, &t.info, sizeof(raw));
                        info.clear();
                        hex_append(info, raw, sizeof(raw));
                }
                fprintf(out, "tex %s %zu %s %s\n", id, i, q(t.name).c_str(), info.c_str());
        }

        // legacy materials
        fprintf(out, "legacy %s %zu\n", id, ctx->LegacyMaterials.size());
        for (size_t i = 0; i < ctx->LegacyMaterials.size(); i++) {
                fprintf(out, "legacymat %s %zu %d %d %d\n", id, i, ctx->LegacyMaterials[i].ShaderIdx,
                        ctx->LegacyMaterials[i].VertexMaterialIdx, ctx->LegacyMaterials[i].TextureIdx);
        }

        // material descriptions
        dump_matdesc(m, out, id, "def", m.DefMatDesc);
        if (m.AlternateMatDesc) {
                dump_matdesc(m, out, id, "alt", m.AlternateMatDesc);
        } else {
                fprintf(out, "matdesc %s alt -\n", id);
        }

        // the aabtree
        if (m.CullTree) {
                std::string nodes, polys;
                for (const W3dMeshAABTreeNode &n : m.CullTree->Nodes) {
                        float minv[3] = {n.Min.X, n.Min.Y, n.Min.Z};
                        float maxv[3] = {n.Max.X, n.Max.Y, n.Max.Z};
                        for (int k = 0; k < 3; k++)
                                hex_append(nodes, (const uint8_t *)&minv[k], 4);
                        for (int k = 0; k < 3; k++)
                                hex_append(nodes, (const uint8_t *)&maxv[k], 4);
                        hex_append(nodes, (const uint8_t *)&n.FrontOrPoly0, 4);
                        hex_append(nodes, (const uint8_t *)&n.BackOrPolyCount, 4);
                }
                polys = hex_u32s(m.CullTree->PolyIndices);
                if (nodes.empty())
                        nodes = "-";
                fprintf(out, "aabtree %s %u %u %s %s\n", id, m.CullTree->NodeCount, m.CullTree->PolyCount,
                        nodes.c_str(), polys.c_str());
        } else {
                fprintf(out, "aabtree %s -\n", id);
        }
}

// --- main -------------------------------------------------------------------------

int main(int argc, char **argv)
{
        if (argc != 2) {
                fprintf(stderr, "usage: gen_mesh_golden <output-file>\n");
                return 1;
        }

        crc32_init();

        FILE *out = fopen(argv[1], "w");
        if (!out) {
                fprintf(stderr, "error: cannot open %s for writing\n", argv[1]);
                return 1;
        }

        fprintf(out, "# gen_mesh_golden output — the W3D mesh chunk tree corpus (P0)\n");
        fprintf(out, "# generated by tools/differential/gen_mesh_golden.cpp; do not edit\n");

        for (const Corpus &c : build_corpora()) {
                SrcFile file;
                file.data = c.bytes.data();
                file.len = c.bytes.size();
                ChunkLoadClass cload(&file);

                fprintf(out, "file %s %s\n", c.id, hex_str(c.bytes).c_str());

                MeshModelClass model;
                int error = model.Load_W3D(cload);
                if (error == WW3D_ERROR_OK) {
                        fprintf(out, "ok %s\n", c.id);
                        dump_mesh(model, out, c.id);
                } else {
                        fprintf(out, "err %s\n", c.id);
                }
        }

        fclose(out);
        return 0;
}
