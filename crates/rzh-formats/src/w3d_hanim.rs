// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Ported from the Zero Hour source release (baseline 571a7c5):
//   GeneralsMD/Code/Libraries/Source/WWVegas/WW3D2/hrawanim.cpp
//     (HRawAnimClass::Load_W3D, read_channel, read_bit_channel,
//      add_channel, add_bit_channel, Get_Translation, Get_Orientation,
//      Get_Transform, Get_Visibility, Is_Node_Motion_Present and the
//      Has_* channel probes)
//   GeneralsMD/Code/Libraries/Source/WWVegas/WW3D2/hrawanim.h
//     (the HRawAnimClass layout; the NodeMotionStruct channel slots)
//   GeneralsMD/Code/Libraries/Source/WWVegas/WW3D2/hanim.h
//     (the HAnimClass query surface every animation format implements —
//      the frame/time getters the raw anim provides)
//   GeneralsMD/Code/Libraries/Source/WWVegas/WW3D2/motchan.cpp /
//     motchan.h (MotionChannelClass::Load_W3D, BitChannelClass::Load_W3D,
//     Get_Vector, Get_Vector_As_Quat, Get_Bit, set_identity; the disabled
//     Do_Data_Compression body — its first statement is `return;`)
//   GeneralsMD/Code/Libraries/Source/WWVegas/WW3D2/w3d_file.h
//     (W3dAnimHeaderStruct, W3dAnimChannelStruct, W3dBitChannelStruct,
//      the chunk ids, the channel-type enums)
//   GeneralsMD/Code/Libraries/Source/WWVegas/WW3D2/htreemgr.cpp /
//     assetmgr.cpp (the Get_HTree lookup the loader drives: lowercase the
//      hierarchy name, hash lookup — modelled as the parse callback)
//   Core/Libraries/Source/WWVegas/WWMath/wwmath.h / wwmath.cpp
//     (Float_To_Long — the deterministic gm_lrintf branch,
//      Float_To_Int_Floor, Fast_Acos/Fast_Sin over the Init() tables,
//      WWMath::Init itself)
//   Core/Libraries/Source/WWVegas/WWMath/quat.cpp (Fast_Slerp — the
//     portable branch; quat.h Build_Matrix3D — the same mixed-precision
//     builder the hierarchy port transcribes)
//   Core/Libraries/Source/WWVegas/WWMath/vector3.h (Vector3::Lerp) and
//     matrix3d.h (Set_Translation)
//   The pinned GameMath sources (TheSuperHackers/GameMath @ 59f7ccd4):
//     float/s_rintf.c, float/s_lrintf.c (gm_lrintf),
//     float/e_acosf.c (gm_acosf), float/s_sinf.c (gm_sinf),
//     float/k_sinf.c, float/k_cosf.c, float/e_rem_pio2f.c — the
//     deterministic-math entry points WWMath calls (USE_DETERMINISTIC_MATH
//     is 1 in the Zero Hour build).
// See the module docs for the format layout and the documented
// differences from the C++ (stated-contract errors where the original is
// undefined, corpus disciplines the transcription shares).

//! The **W3D raw animation** — the motion data that fills a hierarchy
//! tree's transforms (the plan's Phase 2 W3D track, on top of the chunk
//! container and the hierarchy trees).
//!
//! A `.w3d` raw animation asset is a [`W3D_CHUNK_ANIMATION`] container
//! whose children describe the motion of one skeleton: a fixed 44-byte
//! [`animation header`](W3dAnimHeader), then any number of
//! [`W3D_CHUNK_ANIMATION_CHANNEL`] motion chunks and
//! [`W3D_CHUNK_BIT_CHANNEL`] visibility chunks. This module ports
//! `HRawAnimClass::Load_W3D` (hrawanim.cpp) and the classic channel
//! readers it drives (motchan.cpp's `MotionChannelClass` and
//! `BitChannelClass`) — the data path the asset manager reaches through
//! `HAnimManagerClass::Load_Raw_Anim` — plus the query surface the tree
//! update drives: [`get_translation`](W3dRawAnim::get_translation),
//! [`get_orientation`](W3dRawAnim::get_orientation) and
//! [`get_transform`](W3dRawAnim::get_transform) are what
//! `HTreeClass::Anim_Update` samples per bone per frame, quirks included
//! (parity level P0, pinned by golden vectors generated from the
//! transcription in `tools/differential/gen_hanim_golden.cpp`).
//!
//! [`W3D_CHUNK_ANIMATION`]: W3D_CHUNK_ANIMATION
//!
//! # The load-path quirks
//!
//! - **the animation is defined against a tree**: `Load_W3D` looks its
//!   hierarchy up in the asset manager (`Get_HTree`) and takes the *tree's*
//!   pivot count as its own — the file has no bone count. The port models
//!   the lookup as [`W3dRawAnim::parse`]'s `resolve_base_pose` callback:
//!   the caller owns the tree table (the future asset-manager slice) and
//!   resolves exactly like `HTreeManagerClass::Get_Tree` — lowercased
//!   exact match. A missing tree fails the load (the original's
//!   `goto Error`), before any channel is read;
//! - **the name composition runs through the raw header bytes**:
//!   `strcpy(Name, aheader.HierarchyName)` reads the 16-byte hierarchy
//!   field and *keeps going* through the header's `NumFrames` and
//!   `FrameRate` words when the field has no NUL — a deterministic
//!   in-struct overrun this port and the transcription both model (the
//!   string is capped at the 44-byte struct; running past the struct
//!   itself is corpus-disciplined away). `strlcat` then appends `"."` and
//!   the animation name — whose *source* can equally run through the rest
//!   of the header — truncating the composition at 31 characters;
//! - **`FrameRate` is an integer field**: the header stores a `uint32`
//!   and the class member is a `float`, so the loader performs a
//!   *numeric* conversion — a file carrying the raw bits of `50.0f`
//!   (`0x42480000`) loads as `1111324672.0`, and
//!   [`total_time`](W3dRawAnim::total_time) divides by that;
//! - **the pre-3.0 pivot shift**: a file whose version is below
//!   `W3D_MAKE_VERSION(3, 0)` gets every channel's pivot index
//!   incremented (the same root-synthesis fixup the hierarchy loader
//!   applies), wrapping `0xFFFF` to `0x10000`;
//! - **channels for missing bones are discarded, not fatal** — the
//!   deliberate contrast with the hierarchy loader's parent contract: a
//!   channel whose (fixup-adjusted) pivot is outside the tree only
//!   produces the original's "Please re-export" debug message and is
//!   dropped; the load succeeds. An *unreadable* channel (short data)
//!   still fails the whole load;
//! - **a channel whose frame span is empty or negative fails the load**:
//!   the C++ allocates `LastFrame-FirstFrame+1` frames (wrapping to a
//!   huge `unsigned` when the span is negative — an allocation the
//!   original cannot satisfy — and writing one float past a zero-size
//!   allocation when it is empty before the follow-up read fails). The
//!   port states the contract: a span below 1, a zero `VectorLen`, or a
//!   frame array too large to be real (over 2^28 floats) fails the
//!   channel and with it the load;
//! - **the exporter-junk skip**: motion channels historically carry more
//!   bytes than `LastFrame-FirstFrame+1` frames need (an exporter bug the
//!   loader works around); the reader consumes what it computed and seeks
//!   over the rest;
//! - **duplicate channels overwrite** — `add_channel` stores the newest
//!   channel per (pivot, type) slot (the original leaks the previous
//!   one); a channel with an unknown type is dropped on the floor (the
//!   original leaks it) — both pinned by the goldens;
//! - **the frame math is the original's, exactly**: `frame0 =
//!   Float_To_Long(frame - 0.499999f)` — the deterministic `gm_lrintf`
//!   (ties to even) — `ratio = frame - frame0`, `frame1` wrapping to
//!   frame 0 past the end (looping), translations through
//!   `Vector3::Lerp` and orientations through the table-based
//!   `Fast_Slerp` (`Fast_Acos`/`Fast_Sin` over the `WWMath::Init`
//!   tables, with `gm_acosf` taking over near the poles), the matrix
//!   built by the same mixed-precision `Build_Matrix3D` the hierarchy
//!   port transcribes.
//!
//! # Differences from the C++
//!
//! Release-build undefined behaviour becomes [`HanimError`] (the load
//! fails): the empty/negative channel span and the over-large frame array
//! above, and a hierarchy tree the caller cannot resolve. Where the C++
//! reads *unbounded* C strings past the 44-byte header the port caps the
//! read at the struct; where the C++ indexes channel data out of bounds
//! (`Get_Vector` past a short array, a `Q` channel with `VectorLen`
//! below 4) the port panics on the array access — both corpus-disciplined
//! (the goldens never build such a channel) and stated here. The
//! `HAnimClass` base-class state that is not load-path (ref counts, the
//! embedded-sound bone index, the combo/blend machinery in hanim.cpp) is
//! out of scope for the format layer and lands with the animation
//! runtime slice; the compressed animation (`HCompressedAnimClass`, the
//! time-coded and adaptive-delta channels) is the next Phase 2 slice.
//!
//! # Examples
//!
//! ```
//! use rzh_formats::w3d_hanim::{W3dRawAnim, W3D_CHUNK_ANIMATION_HEADER};
//! use rzh_formats::w3d_htree::W3dHtree;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! # let payload: Vec<u8> = Vec::new(); // the ANIMATION container's bytes
//! # if payload.is_empty() { return Ok(()); }
//! // The caller owns the tree table: resolve the animation's hierarchy
//! // the way the asset manager does (lowercase exact match) and hand the
//! // tree to the parser. No tree, no animation.
//! let trees: Vec<(Vec<u8>, W3dHtree)> = Vec::new();
//! let anim = W3dRawAnim::parse(&payload, |name| {
//!     trees
//!         .iter()
//!         .find(|(n, _)| *n == name.to_ascii_lowercase())
//!         .map(|(_, tree)| tree)
//! })?;
//! assert!(anim.num_frames() > 0);
//! # Ok(())
//! # }
//! ```

// Clippy notes for the verbatim msun transcriptions in the WWMath section
// below: `excessive_precision` / `approx_constant` (the float literals keep
// the exact decimal forms of the C sources; rounding them changes bits)
// and `eq_op` (NaN sentinels written as `x - x` are behaviour).
#![allow(clippy::excessive_precision, clippy::approx_constant, clippy::eq_op)]

use std::fmt;
use std::sync::OnceLock;

use crate::chunk::ChunkLoad;
use crate::w3d_htree::W3dHtree;

// ---------------------------------------------------------------------------
// chunk ids and format constants (w3d_file.h)
// ---------------------------------------------------------------------------

/// The raw-animation container a `.w3d` file wraps an animation asset in.
pub const W3D_CHUNK_ANIMATION: u32 = 0x0000_0200;
/// The 44-byte `W3dAnimHeaderStruct`.
pub const W3D_CHUNK_ANIMATION_HEADER: u32 = 0x0000_0201;
/// One `W3dAnimChannelStruct` plus its frame data.
pub const W3D_CHUNK_ANIMATION_CHANNEL: u32 = 0x0000_0202;
/// One `W3dBitChannelStruct` plus its bit data.
pub const W3D_CHUNK_BIT_CHANNEL: u32 = 0x0000_0203;

/// The length of every fixed name field (`W3D_NAME_LEN`).
pub const W3D_NAME_LEN: usize = 16;
/// The size of the animation header struct.
pub const ANIM_HEADER_SIZE: u32 = 44;
/// The size of the motion channel struct (six `uint16`s and the first
/// `float` of the data array).
pub const ANIM_CHANNEL_STRUCT_SIZE: u32 = 16;
/// The size of the bit channel struct (four `uint16`s, the default value
/// and the first byte of the bit array).
pub const BIT_CHANNEL_STRUCT_SIZE: u32 = 10;

/// The channel types (the `Flags` field): translation on X.
pub const ANIM_CHANNEL_X: u32 = 0;
/// Translation on Y.
pub const ANIM_CHANNEL_Y: u32 = 1;
/// Translation on Z.
pub const ANIM_CHANNEL_Z: u32 = 2;
/// Euler rotation on X (stored, never sampled by the raw queries).
pub const ANIM_CHANNEL_XR: u32 = 3;
/// Euler rotation on Y.
pub const ANIM_CHANNEL_YR: u32 = 4;
/// Euler rotation on Z.
pub const ANIM_CHANNEL_ZR: u32 = 5;
/// The quaternion orientation channel.
pub const ANIM_CHANNEL_Q: u32 = 6;
/// The bit channel types: visibility.
pub const BIT_CHANNEL_VIS: u32 = 0;

/// `HAnimClass::CLASSID_HRAWANIM` — the raw animation's class id.
pub const CLASSID_HRAWANIM: u32 = 0;

/// The largest motion-channel array the port will allocate (2^28 floats,
/// 1 GiB). The original would attempt the allocation the header fields
/// describe and die in `new[]` when it is huge; the port states the
/// contract instead (see the module docs).
pub const MAX_CHANNEL_FLOATS: u32 = 1 << 28;

/// `W3D_MAKE_VERSION` — the packed `(major, minor)` file version.
pub const fn w3d_make_version(major: u32, minor: u32) -> u32 {
    (major << 16) | minor
}

// ---------------------------------------------------------------------------
// errors
// ---------------------------------------------------------------------------

/// Errors the raw-animation load path can produce.
///
/// The original's `Load_W3D` has exactly one failure code — `LOAD_ERROR`
/// — and the port states the same contract: a bad first chunk, a short
/// header, an unresolvable hierarchy tree, a short or contract-violating
/// channel, all fail the load outright.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum HanimError {
    /// `LOAD_ERROR` — the load failed outright.
    LoadFailed,
    /// The chunk container itself errored (depth cap).
    Chunk(crate::chunk::ChunkError),
}

impl fmt::Display for HanimError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LoadFailed => write!(f, "W3D animation load failed"),
            Self::Chunk(e) => write!(f, "W3D animation chunk error: {e}"),
        }
    }
}

impl std::error::Error for HanimError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Chunk(e) => Some(e),
            _ => None,
        }
    }
}

impl From<crate::chunk::ChunkError> for HanimError {
    fn from(e: crate::chunk::ChunkError) -> Self {
        Self::Chunk(e)
    }
}

// ---------------------------------------------------------------------------
// the fixed structs
// ---------------------------------------------------------------------------

/// The 44-byte `W3dAnimHeaderStruct`, decoded.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct W3dAnimHeader {
    /// File-format version (`W3D_MAKE_VERSION`); below 3.0 the channels'
    /// pivot indexes shift up by one.
    pub version: u32,
    /// The animation name (a C string in a 16-byte field).
    pub name: [u8; W3D_NAME_LEN],
    /// The hierarchy this animation drives (a C string in a 16-byte
    /// field) — the tree-lookup key.
    pub hierarchy_name: [u8; W3D_NAME_LEN],
    /// The frame count.
    pub num_frames: u32,
    /// The frame rate as a **raw `uint32`** — the class member is a
    /// `float` and the loader converts numerically
    /// ([`W3dRawAnim::frame_rate`]); a file storing the bits of `50.0f`
    /// loads as `1111324672.0`.
    pub frame_rate: u32,
}

impl W3dAnimHeader {
    /// Decodes the struct from its 44 little-endian bytes.
    pub fn from_bytes(bytes: &[u8; ANIM_HEADER_SIZE as usize]) -> Self {
        let u32at = |o: usize| u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
        let mut name = [0u8; W3D_NAME_LEN];
        name.copy_from_slice(&bytes[4..20]);
        let mut hierarchy_name = [0u8; W3D_NAME_LEN];
        hierarchy_name.copy_from_slice(&bytes[20..36]);
        Self {
            version: u32at(0),
            name,
            hierarchy_name,
            num_frames: u32at(36),
            frame_rate: u32at(40),
        }
    }

    /// The C-string view of the animation name field.
    pub fn name_str(&self) -> &[u8] {
        cstr(&self.name)
    }

    /// The C-string view of the hierarchy name field.
    pub fn hierarchy_name_str(&self) -> &[u8] {
        cstr(&self.hierarchy_name)
    }
}

/// The bytes of a fixed character field up to the first NUL.
fn cstr(field: &[u8]) -> &[u8] {
    let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    &field[..end]
}

/// The C-string starting at `start` inside the 44 header bytes, capped at
/// the struct end — the source the loader's `strcpy`/`strlcat` calls read
/// (they run through the following header words when a name field has no
/// NUL; past the struct itself the read is undefined and the port caps
/// it, see the module docs).
fn header_cstr(bytes: &[u8; ANIM_HEADER_SIZE as usize], start: usize) -> &[u8] {
    let end = bytes[start..]
        .iter()
        .position(|&b| b == 0)
        .map_or(ANIM_HEADER_SIZE as usize, |p| start + p);
    &bytes[start..end]
}

/// `strlcat(dst, src, siz)` — the BSD append: copies `src` onto the end
/// of the NUL-terminated `dst`, never writing more than `siz` bytes
/// including the terminator. Both sides transcribe the same canonical
/// body.
fn strlcat(dst: &mut [u8], src: &[u8], siz: usize) {
    // Find the end of dst within siz bytes
    let dlen = dst[..siz].iter().position(|&b| b == 0).unwrap_or(siz);
    let mut n = siz - dlen;
    if n == 0 {
        // No room: the original returns without writing
        return;
    }
    let mut d = dlen;
    for &b in src {
        if n != 1 {
            dst[d] = b;
            d += 1;
            n -= 1;
        }
    }
    dst[d] = 0;
}

// ---------------------------------------------------------------------------
// the classic channels (motchan.cpp)
// ---------------------------------------------------------------------------

/// One loaded motion channel — `MotionChannelClass` after `Load_W3D`.
///
/// The frame data is the decoded float array (`VectorLen` values per
/// frame, `LastFrame-FirstFrame+1` frames); `Do_Data_Compression` is
/// transcribed as its compiled body — the disabled `return;` — so the
/// compressed form the struct reserves space for is never built.
#[derive(Debug, Clone, PartialEq)]
pub struct W3dMotionChannel {
    /// The pivot this channel drives (`PivotIdx`, after the pre-3.0
    /// fixup).
    pub pivot_idx: u32,
    /// The channel type (`Type`, the wire `Flags` field).
    pub channel_type: u32,
    /// The first frame with data (`FirstFrame`); frames outside
    /// `[first_frame, last_frame]` read as the identity.
    pub first_frame: i32,
    /// The last frame with data (`LastFrame`).
    pub last_frame: i32,
    /// The number of floats per frame (`VectorLen`).
    pub vector_len: usize,
    /// The frame data: `(last_frame - first_frame + 1) * vector_len`
    /// floats.
    pub data: Vec<f32>,
}

impl W3dMotionChannel {
    /// `MotionChannelClass::Get_Vector` — writes `vector_len` values for
    /// `frame` into `setvec` (which must hold `vector_len` floats), or
    /// the identity (`0.0`, or `(0, 0, 0, 1)` for a quaternion channel)
    /// outside the channel's frame range.
    ///
    /// Panics when `setvec` is shorter than `vector_len` (the original
    /// writes past the caller's buffer).
    pub fn get_vector(&self, frame: i32, setvec: &mut [f32]) {
        assert!(
            setvec.len() >= self.vector_len,
            "Get_Vector destination shorter than VectorLen (the original overruns it)"
        );
        if frame < self.first_frame || frame > self.last_frame {
            self.set_identity(setvec);
        } else {
            let vframe = (frame - self.first_frame) as usize;
            for (i, slot) in setvec.iter_mut().enumerate().take(self.vector_len) {
                *slot = self.data[vframe * self.vector_len + i];
            }
        }
    }

    /// `MotionChannelClass::Get_Vector_As_Quat` — the quaternion read
    /// `Get_Orientation` drives: the identity outside the range, four
    /// components inside it.
    ///
    /// The indexing is the original's flat `Data[(frame - FirstFrame) *
    /// VectorLen + k]` for `k < 4`: a channel whose `VectorLen` is below
    /// 4 reads into the following frames' data (in bounds) and past the
    /// array on the last frame (a panic here, undefined in the C++ —
    /// corpus-disciplined away; real quaternion channels carry 4).
    pub fn get_vector_as_quat(&self, frame: i32) -> [f32; 4] {
        if frame < self.first_frame || frame > self.last_frame {
            [0.0, 0.0, 0.0, 1.0]
        } else {
            let vframe = (frame - self.first_frame) as usize;
            [
                self.data[vframe * self.vector_len],
                self.data[vframe * self.vector_len + 1],
                self.data[vframe * self.vector_len + 2],
                self.data[vframe * self.vector_len + 3],
            ]
        }
    }

    /// Component 0 of [`get_vector`](Self::get_vector) — the value the
    /// translation queries read (`Get_Vector` into `&(trans[0])` and
    /// friends: each channel's first component lands in its own slot for
    /// every `VectorLen` the exporter writes).
    pub fn get_scalar(&self, frame: i32) -> f32 {
        if frame < self.first_frame || frame > self.last_frame {
            0.0
        } else {
            let vframe = (frame - self.first_frame) as usize;
            self.data[vframe * self.vector_len]
        }
    }

    /// `set_identity` — the out-of-range value: `(0, 0, 0, 1)` for a
    /// quaternion channel, `0.0` for anything else.
    fn set_identity(&self, setvec: &mut [f32]) {
        if self.channel_type == ANIM_CHANNEL_Q {
            setvec[0] = 0.0;
            setvec[1] = 0.0;
            setvec[2] = 0.0;
            setvec[3] = 1.0;
        } else {
            setvec[0] = 0.0;
        }
    }
}

/// One loaded bit channel — `BitChannelClass` after `Load_W3D`.
#[derive(Debug, Clone, PartialEq)]
pub struct W3dBitChannel {
    /// The pivot this channel drives (`PivotIdx`, after the pre-3.0
    /// fixup).
    pub pivot_idx: u32,
    /// The channel type (`Type`, the wire `Flags` field).
    pub channel_type: u32,
    /// The value frames outside `[first_frame, last_frame]` read
    /// (`DefaultVal`).
    pub default_val: i32,
    /// The first frame with data.
    pub first_frame: i32,
    /// The last frame with data.
    pub last_frame: i32,
    /// The packed bits, LSB-first within each byte.
    pub bits: Vec<u8>,
}

impl W3dBitChannel {
    /// `BitChannelClass::Get_Bit` — the bit for `frame`, or the default
    /// outside the channel's range.
    pub fn get_bit(&self, frame: i32) -> i32 {
        if frame < self.first_frame || frame > self.last_frame {
            self.default_val
        } else {
            let bit = (frame - self.first_frame) as u32;
            let mask = 1u8 << (bit % 8);
            i32::from(self.bits[(bit / 8) as usize] & mask != 0)
        }
    }
}

/// The per-bone channel set — `NodeMotionStruct`. Every slot is optional:
/// bones the animation does not move keep `None` and read as the identity
/// (or always-visible).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct W3dNodeMotion {
    /// The X translation channel.
    pub x: Option<W3dMotionChannel>,
    /// The Y translation channel.
    pub y: Option<W3dMotionChannel>,
    /// The Z translation channel.
    pub z: Option<W3dMotionChannel>,
    /// The X euler channel (stored; the raw queries never sample it).
    pub xr: Option<W3dMotionChannel>,
    /// The Y euler channel.
    pub yr: Option<W3dMotionChannel>,
    /// The Z euler channel.
    pub zr: Option<W3dMotionChannel>,
    /// The quaternion orientation channel.
    pub q: Option<W3dMotionChannel>,
    /// The visibility bit channel.
    pub vis: Option<W3dBitChannel>,
}

// ---------------------------------------------------------------------------
// the animation
// ---------------------------------------------------------------------------

/// A loaded W3D raw animation — the state `HRawAnimClass::Load_W3D`
/// leaves behind, minus the runtime base-class state (see the module
/// docs).
#[derive(Debug, Clone, PartialEq)]
pub struct W3dRawAnim {
    /// The decoded animation header (the pristine 44 bytes).
    pub header: W3dAnimHeader,
    /// The composed name (`Get_Name`): `"<hierarchy>.<animation>"` —
    /// a NUL-terminated 32-byte buffer built by `strcpy` + `strlcat`
    /// over the *raw* header bytes.
    pub name: [u8; 2 * W3D_NAME_LEN],
    /// The hierarchy name (`Get_HName`) — the `strcpy` destination: the
    /// 16-byte field, running on through the header's frame-count and
    /// frame-rate words when the field has no NUL (capped at the struct;
    /// see the module docs). Stored as the 24-byte window the member's
    /// C-string can reach, NUL-terminated.
    pub hierarchy_name: [u8; 24],
    /// The frame count (the header's `uint32` read as the class's `int`).
    pub num_frames: u32,
    /// The frame rate — the header's raw `uint32` **numerically**
    /// converted to `float`.
    pub frame_rate: f32,
    /// The per-bone channel array — one entry per hierarchy pivot.
    pub node_motion: Vec<W3dNodeMotion>,
}

impl W3dRawAnim {
    /// Parses the payload of a [`W3D_CHUNK_ANIMATION`] container — the
    /// bytes between the container's header and its end — against the
    /// hierarchy tree the caller resolves.
    ///
    /// `resolve_base_pose` plays the asset manager's `Get_HTree`: it
    /// receives the hierarchy name exactly as the loader's `strcpy`
    /// leaves it (the 16-byte field, running on through the header's
    /// frame-count and frame-rate words when the field has no NUL) and
    /// returns the tree (whose pivot count the animation adopts) or
    /// `None` to fail the load. The real manager lowercases the name and
    /// looks it up in its hash; the caller mirrors that.
    ///
    /// A short channel, an empty/negative frame span or an unresolvable
    /// tree fails the parse (the original's `goto Error`); a channel for
    /// a pivot outside the tree is discarded and the load continues.
    pub fn parse<'a>(
        data: &[u8],
        resolve_base_pose: impl FnOnce(&[u8]) -> Option<&'a W3dHtree>,
    ) -> Result<Self, HanimError> {
        let mut cload = ChunkLoad::new(data);

        // Open the first chunk, it should be the animation header
        if !cload.open_chunk()? {
            return Err(HanimError::LoadFailed);
        }
        if cload.cur_chunk_id() != W3D_CHUNK_ANIMATION_HEADER {
            // "ERROR: Expected Animation Header!"
            return Err(HanimError::LoadFailed);
        }
        let mut header_bytes = [0u8; ANIM_HEADER_SIZE as usize];
        if cload.read(&mut header_bytes)? != ANIM_HEADER_SIZE {
            return Err(HanimError::LoadFailed);
        }
        let header = W3dAnimHeader::from_bytes(&header_bytes);
        cload.close_chunk()?;

        // Check if the animation version is pre-3.0. If so, we need to
        // add 1 to all of the bone indexes.
        let pre30 = header.version < w3d_make_version(3, 0);

        // strcpy(Name, aheader.HierarchyName) — the source is the raw
        // header bytes from the hierarchy field until the first NUL,
        // running through NumFrames and FrameRate when the field has
        // none (capped at the struct; see the module docs).
        let hierarchy_src = header_cstr(&header_bytes, 20);
        let mut name = [0u8; 2 * W3D_NAME_LEN];
        name[..hierarchy_src.len()].copy_from_slice(hierarchy_src);
        name[hierarchy_src.len()] = 0;
        strlcat(&mut name, b".", 2 * W3D_NAME_LEN);
        let anim_src = header_cstr(&header_bytes, 4);
        strlcat(&mut name, anim_src, 2 * W3D_NAME_LEN);

        // strcpy(HierarchyName, aheader.HierarchyName) — the member holds
        // the source string (the field, running through the header words
        // when it has no NUL; the class-member reassignments after the
        // copy cover the same bytes for every corpus the discipline
        // allows — the NUL inside the field or the NumFrames word)
        let mut hierarchy_name = [0u8; 24];
        hierarchy_name[..hierarchy_src.len()].copy_from_slice(hierarchy_src);
        hierarchy_name[hierarchy_src.len()] = 0;

        // HTreeClass* base_pose = Get_HTree(HierarchyName) — the lookup
        // key is the strcpy source above (the member holds it); no tree,
        // no animation.
        let base_pose = resolve_base_pose(hierarchy_src).ok_or(HanimError::LoadFailed)?;
        let num_nodes = base_pose.num_pivots();

        let num_frames = header.num_frames;
        let frame_rate = header.frame_rate as f32;

        let mut node_motion = vec![W3dNodeMotion::default(); num_nodes];

        // Now, read in all of the other chunks (motion channels).
        while cload.open_chunk()? {
            match cload.cur_chunk_id() {
                W3D_CHUNK_ANIMATION_CHANNEL => {
                    let mut channel = match read_motion_channel(&mut cload)? {
                        Ok(channel) => channel,
                        // goto Error — an unreadable channel fails the
                        // whole load
                        Err(()) => return Err(HanimError::LoadFailed),
                    };
                    if pre30 {
                        channel.pivot_idx = channel.pivot_idx.wrapping_add(1);
                    }
                    // if the channel is referring to a node which is
                    // outside the range, just throw away the channel
                    // ("...Please re-export.")
                    if channel.pivot_idx < num_nodes as u32 {
                        add_channel(&mut node_motion, channel);
                    }
                }
                W3D_CHUNK_BIT_CHANNEL => {
                    let mut channel = match read_bit_channel(&mut cload)? {
                        Ok(channel) => channel,
                        Err(()) => return Err(HanimError::LoadFailed),
                    };
                    if pre30 {
                        channel.pivot_idx = channel.pivot_idx.wrapping_add(1);
                    }
                    if channel.pivot_idx < num_nodes as u32 {
                        add_bit_channel(&mut node_motion, channel);
                    }
                }
                _ => {}
            }
            cload.close_chunk()?;
        }

        Ok(Self {
            header,
            name,
            hierarchy_name,
            num_frames,
            frame_rate,
            node_motion,
        })
    }

    /// The animation's composed name (`Get_Name`) — the C-string view of
    /// the `"<hierarchy>.<animation>"` buffer.
    pub fn name_str(&self) -> &[u8] {
        cstr(&self.name)
    }

    /// The hierarchy name (`Get_HName`) — the C-string view of the
    /// `strcpy` destination (the field, extended through the header's
    /// frame words when unterminated).
    pub fn hierarchy_name_str(&self) -> &[u8] {
        cstr(&self.hierarchy_name)
    }

    /// The frame count (`Get_Num_Frames`).
    pub fn num_frames(&self) -> u32 {
        self.num_frames
    }

    /// The frame rate (`Get_Frame_Rate`) — the header's raw `uint32`
    /// numerically converted.
    pub fn frame_rate(&self) -> f32 {
        self.frame_rate
    }

    /// The total time (`Get_Total_Time`) — `(float)NumFrames /
    /// FrameRate`, in seconds.
    pub fn total_time(&self) -> f32 {
        (self.num_frames as i32) as f32 / self.frame_rate
    }

    /// The pivot count (`Get_Num_Pivots`) — the hierarchy tree's count.
    pub fn num_pivots(&self) -> usize {
        self.node_motion.len()
    }

    /// `Get_Translation` — the translation of `pividx` at `frame`:
    /// the X/Y/Z channels' component-0 values, lerped between `frame0`
    /// and `frame1` (`frame0 = Float_To_Long(frame - 0.499999f)`,
    /// `frame1` wrapping to 0 past the end). A bone with no translation
    /// channel reads as `(0, 0, 0)`.
    ///
    /// Panics when `pividx` is out of range (the original indexes
    /// `NodeMotion[pividx]` unchecked).
    pub fn get_translation(&self, pividx: usize, frame: f32) -> [f32; 3] {
        let motion = &self.node_motion[pividx];

        if motion.x.is_none() && motion.y.is_none() && motion.z.is_none() {
            return [0.0, 0.0, 0.0];
        }

        let (frame0, frame1, ratio) = self.frame_bounds(frame);

        let mut trans0 = [0.0f32; 3];
        if let Some(x) = &motion.x {
            trans0[0] = x.get_scalar(frame0);
        }
        if let Some(y) = &motion.y {
            trans0[1] = y.get_scalar(frame0);
        }
        if let Some(z) = &motion.z {
            trans0[2] = z.get_scalar(frame0);
        }

        if ratio == 0.0 {
            return trans0;
        }

        let mut trans1 = [0.0f32; 3];
        if let Some(x) = &motion.x {
            trans1[0] = x.get_scalar(frame1);
        }
        if let Some(y) = &motion.y {
            trans1[1] = y.get_scalar(frame1);
        }
        if let Some(z) = &motion.z {
            trans1[2] = z.get_scalar(frame1);
        }

        [
            lerp(trans0[0], trans1[0], ratio),
            lerp(trans0[1], trans1[1], ratio),
            lerp(trans0[2], trans1[2], ratio),
        ]
    }

    /// `Get_Orientation` — the orientation quaternion of `pividx` at
    /// `frame`: the Q channel read as a quaternion (the identity outside
    /// its range or when absent), `Fast_Slerp`-interpolated to `frame1`.
    pub fn get_orientation(&self, pividx: usize, frame: f32) -> [f32; 4] {
        let (frame0, frame1, ratio) = self.frame_bounds(frame);

        // the SPECIAL_GETVEC_AS_QUAT branch (the one the build compiles)
        let q0 = match &self.node_motion[pividx].q {
            Some(mc) => mc.get_vector_as_quat(frame0),
            None => [0.0, 0.0, 0.0, 1.0],
        };

        if ratio == 0.0 {
            return q0;
        }

        let q1 = match &self.node_motion[pividx].q {
            Some(mc) => mc.get_vector_as_quat(frame1),
            None => [0.0, 0.0, 0.0, 1.0],
        };

        if ratio == 1.0 {
            return q1;
        }

        fast_slerp(&q0, &q1, ratio)
    }

    /// `Get_Transform` — the full local transform of `pividx` at
    /// `frame`, the matrix `HTreeClass::Anim_Update` stores into the
    /// tree: the rotation from `Build_Matrix3D` of the slerped
    /// quaternion, the translation from the lerped X/Y/Z channels (at
    /// `ratio == 0` written per-slot without the lerp).
    pub fn get_transform(&self, pividx: usize, frame: f32) -> [f32; 12] {
        let motion = &self.node_motion[pividx];

        let (frame0, frame1, ratio) = self.frame_bounds(frame);

        let q0 = match &motion.q {
            Some(mc) => mc.get_vector_as_quat(frame0),
            None => [0.0, 0.0, 0.0, 1.0],
        };

        if ratio == 0.0 {
            let mut mtx = build_matrix3d(&q0);
            if let Some(x) = &motion.x {
                mtx[3] = x.get_scalar(frame0);
            }
            if let Some(y) = &motion.y {
                mtx[7] = y.get_scalar(frame0);
            }
            if let Some(z) = &motion.z {
                mtx[11] = z.get_scalar(frame0);
            }
            return mtx;
        }

        let q1 = match &motion.q {
            Some(mc) => mc.get_vector_as_quat(frame1),
            None => [0.0, 0.0, 0.0, 1.0],
        };

        let q = fast_slerp(&q0, &q1, ratio);
        let mut mtx = build_matrix3d(&q);

        let mut trans0 = [0.0f32; 3];
        if let Some(x) = &motion.x {
            trans0[0] = x.get_scalar(frame0);
        }
        if let Some(y) = &motion.y {
            trans0[1] = y.get_scalar(frame0);
        }
        if let Some(z) = &motion.z {
            trans0[2] = z.get_scalar(frame0);
        }

        let mut trans1 = [0.0f32; 3];
        if let Some(x) = &motion.x {
            trans1[0] = x.get_scalar(frame1);
        }
        if let Some(y) = &motion.y {
            trans1[1] = y.get_scalar(frame1);
        }
        if let Some(z) = &motion.z {
            trans1[2] = z.get_scalar(frame1);
        }

        // Vector3::Lerp + Matrix3D::Set_Translation
        mtx[3] = lerp(trans0[0], trans1[0], ratio);
        mtx[7] = lerp(trans0[1], trans1[1], ratio);
        mtx[11] = lerp(trans0[2], trans1[2], ratio);

        mtx
    }

    /// `Get_Visibility` — the visibility of `pividx` at `frame`: the bit
    /// channel's bit (`(int)frame`), defaulting to visible when the
    /// channel is absent.
    pub fn get_visibility(&self, pividx: usize, frame: f32) -> bool {
        match &self.node_motion[pividx].vis {
            Some(vis) => vis.get_bit(frame as i32) == 1,
            // default to always visible...
            None => true,
        }
    }

    /// `Is_Node_Motion_Present` — whether *any* channel drives `pividx`.
    pub fn is_node_motion_present(&self, pividx: usize) -> bool {
        let motion = &self.node_motion[pividx];
        motion.x.is_some()
            || motion.y.is_some()
            || motion.z.is_some()
            || motion.xr.is_some()
            || motion.yr.is_some()
            || motion.zr.is_some()
            || motion.q.is_some()
            || motion.vis.is_some()
    }

    /// `Has_X_Translation` — whether `pividx` has an X channel.
    pub fn has_x_translation(&self, pividx: usize) -> bool {
        self.node_motion[pividx].x.is_some()
    }

    /// `Has_Y_Translation`.
    pub fn has_y_translation(&self, pividx: usize) -> bool {
        self.node_motion[pividx].y.is_some()
    }

    /// `Has_Z_Translation`.
    pub fn has_z_translation(&self, pividx: usize) -> bool {
        self.node_motion[pividx].z.is_some()
    }

    /// `Has_Rotation` — whether `pividx` has a quaternion channel.
    pub fn has_rotation(&self, pividx: usize) -> bool {
        self.node_motion[pividx].q.is_some()
    }

    /// `Has_Visibility` — whether `pividx` has a visibility channel.
    pub fn has_visibility(&self, pividx: usize) -> bool {
        self.node_motion[pividx].vis.is_some()
    }

    /// The frame math every query shares: `frame0 =
    /// Float_To_Long(frame - 0.499999f)`, `frame1 = frame0 + 1` (wrapping
    /// to 0 at or past `NumFrames` — the animation loops) and `ratio =
    /// frame - frame0`.
    fn frame_bounds(&self, frame: f32) -> (i32, i32, f32) {
        let frame0 = float_to_long(frame - 0.499_999);
        let mut frame1 = frame0 + 1;
        let ratio = frame - frame0 as f32;
        if frame1 >= self.num_frames as i32 {
            frame1 = 0;
        }
        (frame0, frame1, ratio)
    }
}

// ---------------------------------------------------------------------------
// read_channel / read_bit_channel / add_channel / add_bit_channel
// ---------------------------------------------------------------------------

/// `MotionChannelClass::Load_W3D` — `Ok(Ok(channel))` on success,
/// `Ok(Err(()))` on the short-read/contract failure that fails the load,
/// `Err` for a container error.
fn read_motion_channel(
    cload: &mut ChunkLoad<'_>,
) -> Result<Result<W3dMotionChannel, ()>, crate::chunk::ChunkError> {
    let size = cload.cur_chunk_length();
    // There was a bug in the exporter which saved too much data, so
    // let's try and not load everything.
    let saved_datasize = size.wrapping_sub(ANIM_CHANNEL_STRUCT_SIZE);

    let mut bytes = [0u8; ANIM_CHANNEL_STRUCT_SIZE as usize];
    if cload.read(&mut bytes)? != ANIM_CHANNEL_STRUCT_SIZE {
        return Ok(Err(()));
    }
    let u16at = |o: usize| u16::from_le_bytes(bytes[o..o + 2].try_into().unwrap());
    let first_frame = i32::from(u16at(0));
    let last_frame = i32::from(u16at(2));
    let vector_len = i32::from(u16at(4));
    let channel_type = i32::from(u16at(6));
    let pivot_idx = i32::from(u16at(8));
    let data0 = f32::from_bits(u32::from_le_bytes(bytes[12..16].try_into().unwrap()));

    // unsigned int num_floats = LastFrame-FirstFrame+1; num_floats *=
    // VectorLen — the span is empty or negative exactly when the
    // original's unsigned arithmetic wraps (a huge allocation it cannot
    // make) or the array is zero-sized (the one-float write past a
    // zero-size block before the follow-up read fails); the port states
    // the contract for both (see the module docs).
    let span = last_frame - first_frame + 1;
    let num_floats = (span as u32).wrapping_mul(vector_len as u32);
    if num_floats == 0 || num_floats > MAX_CHANNEL_FLOATS {
        return Ok(Err(()));
    }
    let datasize = (num_floats - 1) * 4;

    // Data = new float[num_floats]; Data[0] = chan.Data[0]; Read(&Data[1],
    // datasize) — the original reads straight into the uninitialized
    // array; the port reads through a small stack buffer (one heap
    // allocation per channel, like the C++) and converts the words into
    // the array in place
    let mut data = vec![0.0f32; num_floats as usize];
    data[0] = data0;
    let mut buf = [0u8; 512];
    let mut remaining = datasize;
    let mut next = 1usize;
    while remaining > 0 {
        let piece = (remaining as usize).min(buf.len());
        if cload.read(&mut buf[..piece])? != piece as u32 {
            return Ok(Err(()));
        }
        let (words, _) = buf[..piece].as_chunks::<4>();
        for word in words {
            data[next] = f32::from_le_bytes(*word);
            next += 1;
        }
        remaining -= piece as u32;
    }

    // Skip over the extra data at the end of the chunk (saved by an error
    // in the exporter). A successful read guarantees the chunk had at
    // least `datasize` payload bytes, so the subtraction cannot wrap
    // here; a failing Seek would be silently ignored by the original
    // anyway (Close_Chunk re-seeks to the chunk end).
    if saved_datasize > datasize {
        cload.seek(saved_datasize - datasize)?;
    }

    // Do_Data_Compression(datasize) — its first statement is `return;`
    // (the compression is disabled); nothing happens.

    Ok(Ok(W3dMotionChannel {
        pivot_idx: pivot_idx as u32,
        channel_type: channel_type as u32,
        first_frame,
        last_frame,
        vector_len: vector_len as usize,
        data,
    }))
}

/// `BitChannelClass::Load_W3D` — same shape as
/// [`read_motion_channel`].
fn read_bit_channel(
    cload: &mut ChunkLoad<'_>,
) -> Result<Result<W3dBitChannel, ()>, crate::chunk::ChunkError> {
    let mut bytes = [0u8; BIT_CHANNEL_STRUCT_SIZE as usize];
    if cload.read(&mut bytes)? != BIT_CHANNEL_STRUCT_SIZE {
        return Ok(Err(()));
    }
    let u16at = |o: usize| u16::from_le_bytes(bytes[o..o + 2].try_into().unwrap());
    let first_frame = i32::from(u16at(0));
    let last_frame = i32::from(u16at(2));
    let channel_type = i32::from(u16at(4));
    let pivot_idx = i32::from(u16at(6));
    let default_val = i32::from(bytes[8]);
    let data0 = bytes[9];

    // uint32 numbits = LastFrame-FirstFrame+1 — an empty or negative
    // span wraps the original's byte count to a huge unsigned value and
    // the follow-up read fails (after a one-byte write past a zero-size
    // block); the port states the contract instead.
    let span = last_frame - first_frame + 1;
    if span < 1 {
        return Ok(Err(()));
    }
    let numbits = span as u32;
    // the original's (numbits + 7) / 8, kept verbatim (div_ceil would
    // read the same but the transcription keeps the C shape)
    #[allow(clippy::manual_div_ceil)]
    let numbytes = (numbits + 7) / 8;
    let bytesleft = numbytes - 1;

    let mut bits = vec![0u8; numbytes as usize];
    bits[0] = data0;

    if bytesleft > 0 && cload.read(&mut bits[1..])? != bytesleft {
        return Ok(Err(()));
    }

    Ok(Ok(W3dBitChannel {
        pivot_idx: pivot_idx as u32,
        channel_type: channel_type as u32,
        default_val,
        first_frame,
        last_frame,
        bits,
    }))
}

/// `HRawAnimClass::add_channel` — routes the channel into its
/// `NodeMotion` slot (overwriting any previous channel there; the
/// original leaks the old one). Unknown types are dropped on the floor
/// (also a leak in the original).
fn add_channel(node_motion: &mut [W3dNodeMotion], channel: W3dMotionChannel) {
    let idx = channel.pivot_idx as usize;
    let node = &mut node_motion[idx];
    match channel.channel_type {
        ANIM_CHANNEL_X => node.x = Some(channel),
        ANIM_CHANNEL_Y => node.y = Some(channel),
        ANIM_CHANNEL_Z => node.z = Some(channel),
        ANIM_CHANNEL_XR => node.xr = Some(channel),
        ANIM_CHANNEL_YR => node.yr = Some(channel),
        ANIM_CHANNEL_ZR => node.zr = Some(channel),
        ANIM_CHANNEL_Q => node.q = Some(channel),
        _ => {}
    }
}

/// `HRawAnimClass::add_bit_channel` — same, for the visibility slot.
fn add_bit_channel(node_motion: &mut [W3dNodeMotion], channel: W3dBitChannel) {
    let idx = channel.pivot_idx as usize;
    if channel.channel_type == BIT_CHANNEL_VIS {
        node_motion[idx].vis = Some(channel);
    }
}

// ---------------------------------------------------------------------------
// the WWMath transcription (wwmath.h / wwmath.cpp / quat.cpp / quat.h)
// ---------------------------------------------------------------------------
//
// The same policy the hierarchy port applies: the crate stays
// dependency-free, so the exact formulas the animation queries drive are
// transcribed privately here (the Phase 1 rzh-math crate holds the
// general surface; this copy is the slice the anim needs, pinned by the
// same goldens).
//
// Clippy notes for the verbatim msun transcriptions in this section:
// - `excessive_precision` / `approx_constant`: the float literals keep
//   the exact decimal forms of the C sources; rounding them changes bits.
// - `eq_op`: NaN sentinels written as `x - x` are behaviour.

/// `WWMATH_EPSILON` (wwmath.h).
const WWMATH_EPSILON: f32 = 0.0001;
/// `WWMATH_PI` (wwmath.h) — the f32 literal the sin-table formula uses.
const WWMATH_PI: f32 = 3.141_592_654;
/// `ARC_TABLE_SIZE` / `SIN_TABLE_SIZE` (wwmath.h): both tables hold 1024
/// entries.
const TABLE_SIZE: usize = 1024;

/// `TWO23[2]`: ±2^23, the round-to-integer magic constant (s_rintf.c).
const TWO23: [f32; 2] = [8_388_608.0, -8_388_608.0]; // 0x4b000000, 0xcb000000

/// `gm_rintf` (the pinned GameMath `float/s_rintf.c`): round to the
/// nearest integer (ties to even) as a float.
fn rintf(x: f32) -> f32 {
    let i0 = x.to_bits() as i32;
    let sx = (i0 >> 31) & 1;
    let j0 = ((i0 >> 23) & 0xff) - 0x7f;
    if j0 < 23 {
        if j0 < 0 {
            if (i0 & 0x7fff_ffff) == 0 {
                return x;
            }
            let w = TWO23[sx as usize] + x;
            return w - TWO23[sx as usize];
        }
        let w = TWO23[sx as usize] + x;
        return w - TWO23[sx as usize];
    }
    if j0 == 0x80 {
        return x + x; // inf or NaN
    }
    x // x is integral
}

/// 2^31: the smallest f32 that converts outside `i32`.
const TWO_P31: f32 = 2_147_483_648.0;

/// `WWMath::Float_To_Long`, the deterministic branch the Zero Hour build
/// compiles (`gm_lrintf`): round to nearest (ties to even), with the
/// `cvtss2si` semantics the pin's intrinsic build has — NaN and
/// out-of-range values return the "integer indefinite" `0x80000000`.
fn float_to_long(f: f32) -> i32 {
    let r = rintf(f);
    if r.is_nan() || r >= TWO_P31 || r < -TWO_P31 {
        return i32::MIN;
    }
    r as i32
}

/// `WWMath::Float_To_Int_Floor` (wwmath.h) — the bit-trick floor the
/// fast-table interpolation indexes with.
fn float_to_int_floor(f: f32) -> i32 {
    let mut a = f.to_bits() as i32;
    let sign = a >> 31; // 0xFFFFFFFF if negative, 0 if positive
    a &= 0x7fff_ffff; // we don't need the sign any more

    let exponent = (a >> 23) - 127;
    let expsign = !(exponent >> 31); // 0xFFFFFFFF if exponent is positive
    let imask = 1_i32.wrapping_shl((31 - exponent) as u32).wrapping_sub(1); // mask for true integer values
    let mantissa = a & ((1 << 23) - 1); // without the hidden bit
    let r = (((mantissa | (1 << 23)) as u32)
        .wrapping_shl(8)
        .wrapping_shr((31 - exponent) as u32)) as i32;

    // if (fabs(value)<1.0) value = 0; copy sign;
    // if (value < 0 && value==(int)(value)) value++;
    // (`!` in the original is a LOGICAL not (0 or 1), not a bitwise not.)
    let low_bits_zero = i32::from((mantissa << 8) & imask == 0);
    let adjusted = low_bits_zero & (expsign ^ ((a - 1) >> 31)) & sign;
    ((r & expsign) ^ sign).wrapping_add(adjusted)
}

// --- the msun float routines the deterministic WWMath calls ------------------
// (e_acosf.c, s_sinf.c, k_sinf.c, k_cosf.c, e_rem_pio2f.c from the pinned
// GameMath — the same transcriptions rzh-math carries, reduced to the
// paths the animation math can reach.)

/// `one` (e_acosf.c / k_cosf.c).
const ONE: f32 = 1.0;
/// `pio2_hi` / `pio2_lo`: pi/2 in two parts (e_acosf.c / e_rem_pio2f.c).
const PIO2_HI: f32 = 1.570_796_3; // 0x3fc90fda
const PIO2_LO: f32 = 7.549_789_415_9e-8; // 0x33a22168
/// `pio2_1` / `pio2_1t`: first 17 bits of pi/2 and the tail
/// (e_rem_pio2f.c).
const PIO2_1: f32 = 1.570_785_522_5; // 0x3fc90f80
const PIO2_1T: f32 = 1.080_433_412_4e-5; // 0x37354443
/// `pio2_2` / `pio2_2t`: second 17 bits and the tail.
const PIO2_2: f32 = 1.080_427_318_8e-5; // 0x37354400
const PIO2_2T: f32 = 6.077_099_934_4e-11; // 0x2e85a308
/// `invpio2`: 24 bits of 2/pi (e_rem_pio2f.c).
const INVPIO2: f32 = 6.366_198_062_9e-1; // 0x3f22f984
/// `pi` (e_acosf.c).
const PI_ACOS: f32 = 3.141_592_5; // 0x40490fda
/// `pS0..pS5`, `qS1..qS4`: the asin/acos polynomial (e_acosf.c).
const PS0: f32 = 0.166_666_67; // 0x3e2aaaab
const PS1: f32 = -0.325_565_82; // 0xbea6b090
const PS2: f32 = 0.201_212_53; // 0x3e4e0aa8
const PS3: f32 = -0.040_055_536; // 0xbd241146
const PS4: f32 = 0.000_791_535; // 0x3a4f7f04
const PS5: f32 = 0.000_034_793_309; // 0x3811ef08
const QS1: f32 = -2.403_394_9; // 0xc019d139
const QS2: f32 = 2.020_945_8; // 0x4001572d
const QS3: f32 = -0.688_283_98; // 0xbf303361
const QS4: f32 = 0.077_038_154; // 0x3d9dc62e

/// `gm_acosf` (the pinned `float/e_acosf.c`, `__ieee754_acosf`) — what
/// `WWMath::Acos` calls under `USE_DETERMINISTIC_MATH`, and what
/// `Fast_Acos` falls back to near ±1.
fn gm_acosf(x: f32) -> f32 {
    let hx = x.to_bits() as i32;
    let ix = hx & 0x7fff_ffff;
    if ix == 0x3f80_0000 {
        // |x| == 1
        if hx > 0 {
            return 0.0; // acos(1) = 0
        }
        return PI_ACOS + 2.0 * PIO2_LO; // acos(-1) = pi
    } else if ix > 0x3f80_0000 {
        // |x| >= 1
        return (x - x) / (x - x); // acos(|x|>1) is NaN
    }
    if ix < 0x3f00_0000 {
        // |x| < 0.5
        if ix <= 0x2300_0000 {
            return PIO2_HI + PIO2_LO; // if |x| < 2**-57
        }
        let z = x * x;
        let p = z * (PS0 + z * (PS1 + z * (PS2 + z * (PS3 + z * (PS4 + z * PS5)))));
        let q = ONE + z * (QS1 + z * (QS2 + z * (QS3 + z * QS4)));
        let r = p / q;
        return PIO2_HI - (x - (PIO2_LO - x * r));
    } else if hx < 0 {
        // x < -0.5
        let z = (ONE + x) * 0.5;
        let p = z * (PS0 + z * (PS1 + z * (PS2 + z * (PS3 + z * (PS4 + z * PS5)))));
        let q = ONE + z * (QS1 + z * (QS2 + z * (QS3 + z * QS4)));
        let s = f32::sqrt(z); // __ieee754_sqrtf
        let r = p / q;
        let w = r * s - PIO2_LO;
        return PI_ACOS - 2.0 * (s + w);
    }
    // x > 0.5
    let z = (ONE - x) * 0.5;
    let s = f32::sqrt(z);
    let mut df = s;
    let idf = df.to_bits();
    df = f32::from_bits(idf & 0xffff_f000);
    let c = (z - df * df) / (s + df);
    let p = z * (PS0 + z * (PS1 + z * (PS2 + z * (PS3 + z * (PS4 + z * PS5)))));
    let q = ONE + z * (QS1 + z * (QS2 + z * (QS3 + z * QS4)));
    let r = p / q;
    let w = r * s + c;
    2.0 * (df + w)
}

/// `S1..S4`: the sin polynomial (k_sinf.c).
const S1: f32 = -0.166_666_67; // -0xaaaaab.0p-26
const S2: f32 = 0.008_333_379; // 0x8888ba.0p-30
const S3: f32 = -0.000_198_530_73; // -0xd02cb0.0p-36
const S4: f32 = 0.000_002_832_675_6; // 0xbe18ff.0p-42
/// `C1..C3`: the cos polynomial (k_cosf.c).
const C1: f32 = 0.041_666_646; // 0xaaaaa5.0p-28
const C2: f32 = -1.388_731_063_344_538_2e-3; // -0xb60615.0p-33
const C3: f32 = 0.000_024_432_543; // 0xccf47d.0p-39

/// `__kernel_sinf` (k_sinf.c): sin kernel on `|x| ~<= pi/4`.
fn kernel_sinf(x: f32, y: f32, iy: i32) -> f32 {
    let z = x * x;
    let v = z * x;
    let r = S2 + z * (S3 + z * S4);
    if iy == 0 {
        x + v * (S1 + z * r)
    } else {
        x - ((z * (0.5 * y - v * r) - y) - v * S1)
    }
}

/// `__kernel_cosf` (k_cosf.c): cos kernel on `|x| ~<= pi/4`.
fn kernel_cosf(x: f32, y: f32) -> f32 {
    let z = x * x;
    let r = z * (C1 + z * (C2 + z * C3));
    let hz = 0.5 * z;
    let w = ONE - hz;
    w + (((ONE - w) - hz) + (z * r - x * y))
}

/// `npio2_hw[]`: high words of the first 32 multiples of pi/2 (low 8
/// bits dropped, further masking happens at run time) — e_rem_pio2f.c's
/// quick-check table.
const NPIO2_HW: [i32; 32] = [
    0x3fc90f00, 0x40490f00, 0x4096cb00, 0x40c90f00, 0x40fb5300, 0x4116cb00, 0x412fed00, 0x41490f00,
    0x41623100, 0x417b5300, 0x418a3a00, 0x4196cb00, 0x41a35c00, 0x41afed00, 0x41bc7e00, 0x41c90f00,
    0x41d5a000, 0x41e23100, 0x41eec200, 0x41fb5300, 0x4203f200, 0x420a3a00, 0x42108300, 0x4216cb00,
    0x421d1400, 0x42235c00, 0x4229a500, 0x422fed00, 0x42363600, 0x423c7e00, 0x4242c700, 0x42490f00,
];

/// `__ieee754_rem_pio2f` (e_rem_pio2f.c), reduced to the paths the
/// animation math can reach: the table inputs are bounded by 2π and the
/// `Fast_Sin` inputs by π (theta = Fast_Acos of a clamped dot product),
/// so the large-argument distillation (`__kernel_rem_pio2f`) is never
/// entered — it is `unreachable!` here rather than transcribed.
fn rem_pio2f(x: f32, y: &mut [f32; 2]) -> i32 {
    let hx = x.to_bits() as i32;
    let ix = hx & 0x7fff_ffff;
    if ix <= 0x3f49_0fd8 {
        // |x| ~<= pi/4, no need for reduction
        y[0] = x;
        y[1] = 0.0;
        return 0;
    }
    if ix < 0x4016_cbe4 {
        // |x| < 3pi/4, special case with n=+-1
        if hx > 0 {
            let mut z = x - PIO2_1;
            if (ix as u32 & 0xfffe_0000) != 0x3fc8_0000 {
                // 17+24 bit pi OK
                y[0] = z - PIO2_1T;
                y[1] = (z - y[0]) - PIO2_1T;
            } else {
                // near pi/2, use 17+17+24 bit pi
                z -= PIO2_2;
                y[0] = z - PIO2_2T;
                y[1] = (z - y[0]) - PIO2_2T;
            }
            return 1;
        }
        // negative x
        let mut z = x + PIO2_1;
        if (ix as u32 & 0xfffe_0000) != 0x3fc8_0000 {
            y[0] = z + PIO2_1T;
            y[1] = (z - y[0]) + PIO2_1T;
        } else {
            z += PIO2_2;
            y[0] = z + PIO2_2T;
            y[1] = (z - y[0]) + PIO2_2T;
        }
        return -1;
    }
    if ix <= 0x4349_0f80 {
        // |x| ~<= 2^7*(pi/2), medium size
        let mut t = f32::abs(x); // gm_fabsf
        let n = (t * INVPIO2 + 0.5) as i32;
        let fn_ = n as f32;
        let mut r = t - fn_ * PIO2_1;
        let mut w = fn_ * PIO2_1T; // 1st round good to 40 bit
        if n < 32 && (ix as u32 & 0xffe0_0000) != (NPIO2_HW[(n - 1) as usize] as u32 & 0xffe0_0000)
        {
            y[0] = r - w; // quick check no cancellation
        } else {
            let j = ix >> 23;
            y[0] = r - w;
            let mut high = y[0].to_bits() as i32;
            let mut i = j - ((high >> 23) & 0xff);
            if i > 8 {
                // 2nd iteration needed, good to 57
                t = r;
                w = fn_ * PIO2_2;
                r = t - w;
                w = fn_ * PIO2_2T - ((t - r) - w);
                y[0] = r - w;
                high = y[0].to_bits() as i32;
                i = j - ((high >> 23) & 0xff);
                if i > 25 {
                    // 3rd iteration need, 74 bits acc
                    t = r; // will cover all possible cases
                    w = fn_ * PIO2_3;
                    r = t - w;
                    w = fn_ * PIO2_3T - ((t - r) - w);
                    y[0] = r - w;
                }
            }
            let _ = (high, i);
        }
        // The C sets y[1] here, after the quick-check if/else, on BOTH
        // paths (nesting it in the else leaves y[1] wrong — the rzh-math
        // transcription's comment documents the same trap).
        y[1] = (r - y[0]) - w; // r - clip(y[0])
        if hx < 0 {
            y[0] = -y[0];
            y[1] = -y[1];
            return -n;
        }
        return n;
    }
    // x is inf or NaN (the large-argument distillation is unreachable
    // from the animation math: every input is bounded by 2π)
    if ix >= 0x7f80_0000 {
        y[0] = x - x;
        y[1] = x - x;
        return 0;
    }
    unreachable!("the animation math never reduces a large argument");
}

/// `pio2_3` / `pio2_3t`: third 17 bits of pi/2 and the tail.
const PIO2_3: f32 = 6.077_094_383_3e-11; // 0x2e85a300
const PIO2_3T: f32 = 6.123_234_262_9e-17; // 0x248d3132

/// `gm_sinf` (the pinned `float/s_sinf.c`) — what `WWMath::Sinf` calls
/// under `USE_DETERMINISTIC_MATH`, and what the sin table is built from.
/// Bounded inputs only (see [`rem_pio2f`]).
fn gm_sinf(x: f32) -> f32 {
    let mut y = [0.0f32; 2];

    let ix = (x.to_bits() as i32) & 0x7fff_ffff;
    // |x| ~< pi/4
    if ix <= 0x3f49_0fd8 {
        // if x < 2**-12 (the original's inexact-trigger `(int)x == 0`
        // test has no value effect)
        if ix < 0x3980_0000 {
            return x;
        }
        return kernel_sinf(x, 0.0, 0);
    }
    // sin(Inf or NaN) is NaN
    if ix >= 0x7f80_0000 {
        return x - x;
    }

    // argument reduction needed
    let n = rem_pio2f(x, &mut y);
    match n & 3 {
        0 => kernel_sinf(y[0], y[1], 1),
        1 => kernel_cosf(y[0], y[1]),
        2 => -kernel_sinf(y[0], y[1], 1),
        _ => -kernel_cosf(y[0], y[1]),
    }
}

// --- the fast tables (wwmath.cpp WWMath::Init) -------------------------------

/// The two lookup tables `WWMath::Init()` builds that the animation
/// queries read (the asin/inv-sin tables exist too, but no anim path
/// touches them).
#[derive(Debug)]
struct FastTables {
    /// `_FastAcosTable[1024]`: `Acosf((a-512)/512)`.
    acos: [f32; TABLE_SIZE],
    /// `_FastSinTable[1024]`: `Sinf(a*2π/1024)`.
    sin: [f32; TABLE_SIZE],
}

impl FastTables {
    fn new() -> Self {
        let mut tables = Self {
            acos: [0.0; TABLE_SIZE],
            sin: [0.0; TABLE_SIZE],
        };

        let mut a = 0usize;
        while a < TABLE_SIZE {
            // cv = float(a-ARC_TABLE_SIZE/2) * (1.0f/(ARC_TABLE_SIZE/2))
            let cv = ((a as f32) - (TABLE_SIZE as f32) / 2.0) * (1.0 / (TABLE_SIZE as f32 / 2.0));
            tables.acos[a] = gm_acosf(cv); // Acosf (non-retail branch)
            a += 1;
        }

        let mut a = 0usize;
        while a < TABLE_SIZE {
            // cv = (float)a * 2.0f * WWMATH_PI / SIN_TABLE_SIZE
            let cv = (a as f32) * 2.0 * WWMATH_PI / (TABLE_SIZE as f32);
            tables.sin[a] = gm_sinf(cv); // Sinf (non-retail branch)
            a += 1;
        }

        tables
    }
}

/// Returns the process-wide tables, computing them on first use — the
/// same lazy-constant treatment `rzh-math` gives them (pure functions of
/// deterministic code, immutable after init; not the mutable global
/// state the plan §3.3 bans).
fn tables() -> &'static FastTables {
    static TABLES: OnceLock<FastTables> = OnceLock::new();
    TABLES.get_or_init(FastTables::new)
}

/// `WWMath::Fast_Acos(val)` — the table interpolation, falling back to
/// the exact [`gm_acosf`] near ±1 where the table is too inaccurate.
fn fast_acos(val: f32) -> f32 {
    // Near -1 and +1, the table becomes too inaccurate
    if val.abs() > 0.975 {
        return gm_acosf(val);
    }

    let t = tables();
    let val = val * (TABLE_SIZE as f32 / 2.0);

    let idx0 = float_to_int_floor(val);
    let idx1 = idx0 + 1;
    let frac = val - idx0 as f32;

    // idx0 += ARC_TABLE_SIZE/2 — signed addition, non-negative afterwards
    // (|val| <= 0.975 keeps |idx0| below half the table).
    let idx0 = (idx0 + (TABLE_SIZE as i32 / 2)) as usize;
    let idx1 = (idx1 + (TABLE_SIZE as i32 / 2)) as usize;

    (1.0 - frac) * t.acos[idx0] + frac * t.acos[idx1]
}

/// `WWMath::Fast_Sin(val)` — the sin-table interpolation. The scale pair
/// `float(SIN_TABLE_SIZE) / (2.0f * WWMATH_PI)` is ONE folded f32
/// constant in the C, so it must be here too.
fn fast_sin(val: f32) -> f32 {
    let t = tables();
    let val = val * (TABLE_SIZE as f32 / (2.0 * WWMATH_PI));

    let idx0 = float_to_int_floor(val);
    let idx1 = idx0 + 1;
    let frac = val - idx0 as f32;

    let idx0 = (idx0 as u32 as usize) & (TABLE_SIZE - 1);
    let idx1 = (idx1 as u32 as usize) & (TABLE_SIZE - 1);

    (1.0 - frac) * t.sin[idx0] + frac * t.sin[idx1]
}

/// `Fast_Slerp(res, p, q, alpha)` (quat.cpp, the portable branch): the
/// table-based slerp the orientation queries drive — hemisphere flip,
/// the near-parallel linear fallback (`1-cos_t < EPSILON²`), otherwise
/// `Fast_Acos`/`Fast_Sin` for the angles.
fn fast_slerp(p: &[f32; 4], q: &[f32; 4], alpha: f32) -> [f32; 4] {
    let mut alpha = alpha;
    let beta;

    // cos theta = dot product of p and q
    let mut cos_t = p[0] * q[0] + p[1] * q[1] + p[2] * q[2] + p[3] * q[3];

    // if q is on opposite hemisphere from A, use -B instead
    let qflip;
    if cos_t < 0.0 {
        cos_t = -cos_t;
        qflip = true;
    } else {
        qflip = false;
    }

    if 1.0 - cos_t < WWMATH_EPSILON * WWMATH_EPSILON {
        // if q is very close to p, just linearly interpolate
        beta = 1.0 - alpha;
    } else {
        let theta = fast_acos(cos_t);
        let sin_t = fast_sin(theta);
        let oo_sin_t = 1.0 / sin_t;
        beta = fast_sin(theta - alpha * theta) * oo_sin_t;
        alpha = fast_sin(alpha * theta) * oo_sin_t;
    }

    if qflip {
        alpha = -alpha;
    }

    [
        beta * p[0] + alpha * q[0],
        beta * p[1] + alpha * q[1],
        beta * p[2] + alpha * q[2],
        beta * p[3] + alpha * q[3],
    ]
}

/// `Build_Matrix3D(q)` (quat.h WWINLINE) — the rotation matrix of a
/// quaternion, the same mixed-precision builder the hierarchy port
/// transcribes (every term promotes through `f64` except the `[1][1]`
/// diagonal's `2.0f`).
fn build_matrix3d(q: &[f32; 4]) -> [f32; 12] {
    let mut m = [0f32; 12];
    m[0] = (1.0_f64 - 2.0_f64 * f64::from(q[1] * q[1] + q[2] * q[2])) as f32;
    m[1] = (2.0_f64 * f64::from(q[0] * q[1] - q[2] * q[3])) as f32;
    m[2] = (2.0_f64 * f64::from(q[2] * q[0] + q[1] * q[3])) as f32;

    m[4] = (2.0_f64 * f64::from(q[0] * q[1] + q[2] * q[3])) as f32;
    // the [1][1] term keeps its float 2.0f until the final promotion
    m[5] = (1.0_f64 - f64::from(2.0_f32 * (q[2] * q[2] + q[0] * q[0]))) as f32;
    m[6] = (2.0_f64 * f64::from(q[1] * q[2] - q[0] * q[3])) as f32;

    m[8] = (2.0_f64 * f64::from(q[2] * q[0] - q[1] * q[3])) as f32;
    m[9] = (2.0_f64 * f64::from(q[1] * q[2] + q[0] * q[3])) as f32;
    m[10] = (1.0_f64 - 2.0_f64 * f64::from(q[1] * q[1] + q[0] * q[0])) as f32;

    // no translation
    m[3] = 0.0;
    m[7] = 0.0;
    m[11] = 0.0;
    m
}

/// `Vector3::Lerp` per component: `a + (b - a) * alpha`.
fn lerp(a: f32, b: f32, alpha: f32) -> f32 {
    a + (b - a) * alpha
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// A parse-time tree stand-in: the format layer only reads the pivot
    /// count, exactly like `Load_W3D` reads `base_pose->Num_Pivots()`.
    fn tree(pivots: usize) -> W3dHtree {
        W3dHtree {
            header: crate::w3d_htree::W3dHierarchyHeader {
                version: 0,
                name: [0; W3D_NAME_LEN],
                num_pivots: pivots as u32,
                center: [0.0; 3],
            },
            pivots: vec![crate::w3d_htree::W3dPivot::default(); pivots],
        }
    }

    /// Resolves by exact lowercase match, like `HTreeManagerClass::
    /// Get_Tree` (the caller-side half of the asset manager).
    fn resolve<'a>(
        trees: &'a [(Vec<u8>, W3dHtree)],
    ) -> impl Fn(&[u8]) -> Option<&'a W3dHtree> + 'a {
        move |name| {
            trees
                .iter()
                .find(|(n, _)| *n == name.to_ascii_lowercase())
                .map(|(_, t)| t)
        }
    }

    /// A raw name field from a string.
    fn name16(s: &str) -> [u8; 16] {
        let mut n = [0u8; 16];
        n[..s.len()].copy_from_slice(s.as_bytes());
        n
    }

    /// The 44-byte header.
    fn header_bytes(
        version: u32,
        name: &[u8; 16],
        hierarchy: &[u8; 16],
        num_frames: u32,
        frame_rate: u32,
    ) -> [u8; 44] {
        let mut b = [0u8; 44];
        b[0..4].copy_from_slice(&version.to_le_bytes());
        b[4..20].copy_from_slice(name);
        b[20..36].copy_from_slice(hierarchy);
        b[36..40].copy_from_slice(&num_frames.to_le_bytes());
        b[40..44].copy_from_slice(&frame_rate.to_le_bytes());
        b
    }

    /// A payload builder: header plus raw child chunks.
    struct AnimBuild {
        bytes: Vec<u8>,
    }

    impl AnimBuild {
        fn new(header: &[u8; 44]) -> Self {
            let mut bytes = Vec::new();
            bytes.extend_from_slice(&W3D_CHUNK_ANIMATION_HEADER.to_le_bytes());
            bytes.extend_from_slice(&ANIM_HEADER_SIZE.to_le_bytes());
            bytes.extend_from_slice(header);
            Self { bytes }
        }

        /// A motion channel: the 16-byte struct + frame data + optional
        /// exporter junk.
        #[allow(clippy::too_many_arguments)]
        fn motion_channel(
            &mut self,
            first: u16,
            last: u16,
            vector_len: u16,
            flags: u16,
            pivot: u16,
            data: &[f32],
            junk: &[u8],
        ) {
            self.begin(W3D_CHUNK_ANIMATION_CHANNEL);
            let frames = (last as i32 - first as i32 + 1) as usize;
            let vlen = vector_len as usize;
            let mut body = Vec::with_capacity(16 + frames * vlen * 4);
            for v in [first, last, vector_len, flags, pivot, 0] {
                body.extend_from_slice(&v.to_le_bytes());
            }
            for (i, word) in data.iter().enumerate() {
                // the struct's Data[0] is the first word, the rest follow
                let _ = i;
                body.extend_from_slice(&word.to_bits().to_le_bytes());
            }
            self.bytes.extend_from_slice(&body);
            self.bytes.extend_from_slice(junk);
            // the payload is the 12 u16 fields + every data word (the
            // first doubles as the struct's Data[0]) + the junk
            self.end(12 + data.len() * 4 + junk.len());
        }

        /// A bit channel.
        fn bit_channel(
            &mut self,
            first: u16,
            last: u16,
            flags: u16,
            pivot: u16,
            default_val: u8,
            bits: &[u8],
        ) {
            self.begin(W3D_CHUNK_BIT_CHANNEL);
            let numbits = (last as i32 - first as i32 + 1) as usize;
            #[allow(clippy::manual_div_ceil)]
            let numbytes = (numbits + 7) / 8;
            let mut body = Vec::with_capacity(10 + numbytes);
            for v in [first, last, flags, pivot] {
                body.extend_from_slice(&v.to_le_bytes());
            }
            body.push(default_val);
            body.extend_from_slice(bits);
            self.bytes.extend_from_slice(&body);
            // 4 u16 fields + the default + every bit byte (the first
            // doubles as the struct's Data[0])
            self.end(9 + bits.len());
        }

        fn begin(&mut self, id: u32) {
            self.bytes.extend_from_slice(&id.to_le_bytes());
            self.bytes.extend_from_slice(&0u32.to_le_bytes());
        }

        fn end(&mut self, size: usize) {
            let at = self.bytes.len() - size - 8;
            self.bytes[at + 4..at + 8].copy_from_slice(&(size as u32).to_le_bytes());
        }

        fn raw_chunk(&mut self, id: u32, payload: &[u8]) {
            self.begin(id);
            self.bytes.extend_from_slice(payload);
            self.end(payload.len());
        }
    }

    const V40: u32 = w3d_make_version(4, 0);
    const V21: u32 = w3d_make_version(2, 1);

    #[test]
    fn header_decodes_every_field() {
        let mut b = [0u8; 44];
        b[0..4].copy_from_slice(&0x0002_0001u32.to_le_bytes());
        b[4..20].copy_from_slice(b"AVTank.Run\0\0\0\0\0\0");
        b[20..36].copy_from_slice(b"AVTank\0\0\0\0\0\0\0\0\0\0");
        b[36..40].copy_from_slice(&37u32.to_le_bytes());
        b[40..44].copy_from_slice(&30u32.to_le_bytes());
        let h = W3dAnimHeader::from_bytes(&b);
        assert_eq!(h.version, 0x0002_0001);
        assert_eq!(h.name_str(), b"AVTank.Run");
        assert_eq!(h.hierarchy_name_str(), b"AVTank");
        assert_eq!(h.num_frames, 37);
        assert_eq!(h.frame_rate, 30);
    }

    #[test]
    fn modern_animation_loads_and_routes_channels() {
        let trees = vec![(b"tank".to_vec(), tree(6))];
        let mut b = AnimBuild::new(&header_bytes(V40, &name16("Run"), &name16("tank"), 8, 30));
        // X/Y/Z on pivot 1
        for (ty, vals) in [
            (ANIM_CHANNEL_X as u16, [0.5f32, 1.0, 1.5, 2.0]),
            (ANIM_CHANNEL_Y as u16, [1.0, 1.0, 1.0, 1.0]),
            (ANIM_CHANNEL_Z as u16, [-0.5, -1.0, -1.5, -2.0]),
        ] {
            b.motion_channel(0, 3, 1, ty, 1, &vals, &[]);
        }
        // Q on pivot 2
        b.motion_channel(
            0,
            3,
            4,
            ANIM_CHANNEL_Q as u16,
            2,
            &[
                0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, -0.5, 0.5, -0.5, 0.5, 0.5, 0.5, 0.5, -0.5, -0.5,
            ],
            &[],
        );
        // Vis on pivot 3
        b.bit_channel(
            0,
            15,
            BIT_CHANNEL_VIS as u16,
            3,
            1,
            &[0b1010_1010, 0b0101_0101],
        );
        // an euler channel on pivot 4 (stored, not sampled)
        b.motion_channel(0, 1, 1, ANIM_CHANNEL_XR as u16, 4, &[0.25, 0.75], &[]);

        let anim = W3dRawAnim::parse(&b.bytes, resolve(&trees)).expect("the animation loads");
        assert_eq!(anim.name_str(), b"tank.Run");
        assert_eq!(anim.hierarchy_name_str(), b"tank");
        assert_eq!(anim.num_frames(), 8);
        assert_eq!(anim.frame_rate(), 30.0);
        assert_eq!(anim.total_time(), 8.0 / 30.0);
        assert_eq!(anim.num_pivots(), 6);

        // the slot routing
        assert!(
            anim.has_x_translation(1) && anim.has_y_translation(1) && anim.has_z_translation(1)
        );
        assert!(anim.has_rotation(2));
        assert!(anim.has_visibility(3));
        assert!(anim.is_node_motion_present(4)); // the euler channel counts
        assert!(!anim.is_node_motion_present(0));
        assert!(!anim.is_node_motion_present(5));
        assert!(!anim.has_rotation(1));
        assert_eq!(
            anim.node_motion[1].x.as_ref().unwrap().data,
            vec![0.5, 1.0, 1.5, 2.0]
        );
        assert_eq!(anim.node_motion[2].q.as_ref().unwrap().vector_len, 4);
        assert_eq!(anim.node_motion[3].vis.as_ref().unwrap().default_val, 1);
    }

    #[test]
    fn frame_rate_is_a_numeric_conversion() {
        // a file storing the raw bits of 50.0f
        let trees = vec![(b"t".to_vec(), tree(1))];
        let b = AnimBuild::new(&header_bytes(
            V40,
            &name16("A"),
            &name16("t"),
            8,
            0x4248_0000,
        ));
        let anim = W3dRawAnim::parse(&b.bytes, resolve(&trees)).unwrap();
        assert_eq!(anim.frame_rate(), 0x4248_0000_u32 as f32);
        // 0x01000001 rounds through f32
        let b = AnimBuild::new(&header_bytes(
            V40,
            &name16("A"),
            &name16("t"),
            1,
            0x0100_0001,
        ));
        let anim = W3dRawAnim::parse(&b.bytes, resolve(&trees)).unwrap();
        assert_eq!(anim.frame_rate().to_bits(), (16_777_216.0f32).to_bits());
        // FrameRate 0: 8/0 = +inf
        let b = AnimBuild::new(&header_bytes(V40, &name16("A"), &name16("t"), 8, 0));
        let anim = W3dRawAnim::parse(&b.bytes, resolve(&trees)).unwrap();
        assert!(anim.total_time().is_infinite() && anim.total_time() > 0.0);
    }

    #[test]
    fn pre30_shifts_channel_pivots() {
        let trees = vec![(b"tank".to_vec(), tree(6))];
        let mut b = AnimBuild::new(&header_bytes(V21, &name16("Run"), &name16("tank"), 4, 30));
        // pivot 0 in the file -> pivot 1 after the fixup
        b.motion_channel(0, 1, 1, ANIM_CHANNEL_X as u16, 0, &[1.0, 2.0], &[]);
        // pivot 0xFFFF wraps to 0x10000 -> outside the tree, discarded
        b.motion_channel(0, 1, 1, ANIM_CHANNEL_Y as u16, 0xFFFF, &[1.0, 2.0], &[]);
        let anim = W3dRawAnim::parse(&b.bytes, resolve(&trees)).expect("the animation loads");
        assert!(anim.has_x_translation(1));
        assert_eq!(anim.node_motion[1].x.as_ref().unwrap().pivot_idx, 1);
        assert!(!anim.has_y_translation(1));
        // the version boundary: exactly 3.0 is not pre-3.0
        let b = AnimBuild::new(&header_bytes(
            w3d_make_version(3, 0),
            &name16("Run"),
            &name16("tank"),
            4,
            30,
        ));
        let mut b = b;
        b.motion_channel(0, 1, 1, ANIM_CHANNEL_X as u16, 0, &[1.0, 2.0], &[]);
        let anim = W3dRawAnim::parse(&b.bytes, resolve(&trees)).unwrap();
        assert!(anim.has_x_translation(0)); // no shift
    }

    #[test]
    fn out_of_range_and_unknown_channels_are_discarded() {
        let trees = vec![(b"t".to_vec(), tree(3))];
        let mut b = AnimBuild::new(&header_bytes(V40, &name16("A"), &name16("t"), 4, 30));
        b.motion_channel(0, 1, 1, ANIM_CHANNEL_X as u16, 5, &[1.0, 2.0], &[]); // outside
        b.motion_channel(0, 1, 1, 99, 0, &[1.0, 2.0], &[]); // unknown type
        b.bit_channel(0, 7, 7, 5, 0, &[0xFF]); // bit channel, unknown type
        b.raw_chunk(0x0999, &[0u8; 8]); // unknown top-level chunk
        let anim = W3dRawAnim::parse(&b.bytes, resolve(&trees)).expect("the load continues");
        for i in 0..3 {
            assert!(!anim.is_node_motion_present(i), "pivot {i} must be empty");
        }
    }

    #[test]
    fn duplicate_channel_overwrites() {
        let trees = vec![(b"t".to_vec(), tree(1))];
        let mut b = AnimBuild::new(&header_bytes(V40, &name16("A"), &name16("t"), 4, 30));
        b.motion_channel(0, 1, 1, ANIM_CHANNEL_X as u16, 0, &[1.0, 2.0], &[]);
        b.motion_channel(0, 1, 1, ANIM_CHANNEL_X as u16, 0, &[9.0, 8.0], &[]);
        let anim = W3dRawAnim::parse(&b.bytes, resolve(&trees)).unwrap();
        assert_eq!(anim.node_motion[0].x.as_ref().unwrap().data, vec![9.0, 8.0]);
    }

    #[test]
    fn exporter_junk_is_sought_over() {
        let trees = vec![(b"t".to_vec(), tree(1))];
        let mut b = AnimBuild::new(&header_bytes(V40, &name16("A"), &name16("t"), 4, 30));
        // 2 frames of 1 float + 8 bytes of the exporter's extra data
        b.motion_channel(0, 1, 1, ANIM_CHANNEL_X as u16, 0, &[1.0, 2.0], &[0xAA; 8]);
        // a second chunk follows: only reachable when the seek landed
        // exactly at the channel chunk's end
        b.motion_channel(0, 1, 1, ANIM_CHANNEL_Y as u16, 0, &[3.0, 4.0], &[]);
        let anim = W3dRawAnim::parse(&b.bytes, resolve(&trees)).expect("the junk is skipped");
        assert!(anim.has_y_translation(0), "the second channel loaded");
    }

    #[test]
    fn short_channel_data_fails_the_load() {
        let trees = vec![(b"t".to_vec(), tree(1))];
        // the struct declares 4 frames but carries one: the Read is
        // clamped at the chunk boundary and fails
        let mut b = AnimBuild::new(&header_bytes(V40, &name16("A"), &name16("t"), 8, 30));
        b.begin(W3D_CHUNK_ANIMATION_CHANNEL);
        let mut body = Vec::new();
        for v in [0u16, 3, 1, ANIM_CHANNEL_X as u16, 0, 0] {
            body.extend_from_slice(&v.to_le_bytes());
        }
        body.extend_from_slice(&1.0f32.to_bits().to_le_bytes());
        b.bytes.extend_from_slice(&body);
        b.end(16);
        assert_eq!(
            W3dRawAnim::parse(&b.bytes, resolve(&trees)),
            Err(HanimError::LoadFailed)
        );
    }

    #[test]
    fn empty_span_and_zero_vector_len_fail_the_load() {
        let trees = vec![(b"t".to_vec(), tree(1))];
        for (first, last, vlen) in [(4u16, 3u16, 1u16), (0, 1, 0), (0, 0, 0)] {
            let mut b = AnimBuild::new(&header_bytes(V40, &name16("A"), &name16("t"), 8, 30));
            b.motion_channel(first, last, vlen, ANIM_CHANNEL_X as u16, 0, &[1.0], &[]);
            assert_eq!(
                W3dRawAnim::parse(&b.bytes, resolve(&trees)),
                Err(HanimError::LoadFailed),
                "span {first}..{last} vlen {vlen}"
            );
        }
        // the bit channel's empty span
        let mut b = AnimBuild::new(&header_bytes(V40, &name16("A"), &name16("t"), 8, 30));
        b.bit_channel(4, 3, BIT_CHANNEL_VIS as u16, 0, 1, &[0]);
        assert_eq!(
            W3dRawAnim::parse(&b.bytes, resolve(&trees)),
            Err(HanimError::LoadFailed)
        );
    }

    #[test]
    fn malformed_payloads_and_missing_trees_fail() {
        let trees = vec![(b"tank".to_vec(), tree(4))];
        // empty payload
        assert_eq!(
            W3dRawAnim::parse(&[], resolve(&trees)),
            Err(HanimError::LoadFailed)
        );
        // the first chunk is not the animation header
        let b = AnimBuild::new(&header_bytes(V40, &name16("A"), &name16("tank"), 4, 30));
        let mut wrong = Vec::new();
        wrong.extend_from_slice(&W3D_CHUNK_ANIMATION_CHANNEL.to_le_bytes());
        wrong.extend_from_slice(&4u32.to_le_bytes());
        wrong.extend_from_slice(&[0u8; 4]);
        assert_eq!(
            W3dRawAnim::parse(&wrong, resolve(&trees)),
            Err(HanimError::LoadFailed)
        );
        // a header chunk truncated by the file end
        let mut short = Vec::new();
        short.extend_from_slice(&W3D_CHUNK_ANIMATION_HEADER.to_le_bytes());
        short.extend_from_slice(&ANIM_HEADER_SIZE.to_le_bytes());
        short.extend_from_slice(&b.bytes[8..8 + 20]);
        assert_eq!(
            W3dRawAnim::parse(&short, resolve(&trees)),
            Err(HanimError::LoadFailed)
        );
        // no tree with that name
        let b = AnimBuild::new(&header_bytes(V40, &name16("A"), &name16("nowhere"), 4, 30));
        assert_eq!(
            W3dRawAnim::parse(&b.bytes, resolve(&trees)),
            Err(HanimError::LoadFailed)
        );
        // error display is stable
        assert_eq!(
            HanimError::LoadFailed.to_string(),
            "W3D animation load failed"
        );
    }

    #[test]
    fn tree_lookup_runs_through_the_callback_lowercase() {
        // the anim says "Tank", the table says "tank" — the callback owns
        // the manager's lowercase-exact semantics
        let trees = vec![(b"tank".to_vec(), tree(2))];
        let b = AnimBuild::new(&header_bytes(V40, &name16("Run"), &name16("Tank"), 4, 30));
        let anim = W3dRawAnim::parse(&b.bytes, resolve(&trees)).expect("the lookup matches");
        assert_eq!(anim.num_pivots(), 2);
    }

    #[test]
    fn name_composition_pins_the_strcpy_and_strlcat_quirks() {
        let trees = vec![(b"t".to_vec(), tree(1))];
        // the plain composition
        let b = AnimBuild::new(&header_bytes(V40, &name16("Run"), &name16("t"), 4, 30));
        let anim = W3dRawAnim::parse(&b.bytes, resolve(&trees)).unwrap();
        assert_eq!(anim.name_str(), b"t.Run");
        // an empty animation name keeps the trailing dot
        let b = AnimBuild::new(&header_bytes(V40, &name16(""), &name16("t"), 4, 30));
        let anim = W3dRawAnim::parse(&b.bytes, resolve(&trees)).unwrap();
        assert_eq!(anim.name_str(), b"t.");
        // an unterminated hierarchy field: strcpy runs through the
        // NumFrames word (5 = 05 00 .. stops at its second byte)
        let hfield = [b'H'; 16];
        let hb = header_bytes(V40, &name16("A"), &hfield, 5, 30);
        // the string is the 16 'H's plus 0x05
        let mut expected = Vec::from(&hb[20..36]);
        expected.push(5);
        let trees2 = vec![(expected.to_ascii_lowercase(), tree(1))];
        let build = AnimBuild::new(&hb);
        let anim = W3dRawAnim::parse(&build.bytes, resolve(&trees2)).unwrap();
        let mut name = Vec::from(&hb[20..37]);
        name.push(b'.');
        name.extend_from_slice(b"A");
        assert_eq!(anim.name_str(), name.as_slice());
        // truncation at 31 characters: 15 + 1 + 16 = 32 does not fit
        let long = name16("0123456789ABCDEF");
        let trees3 = vec![(b"tanktree1234567".to_vec(), tree(1))];
        let b = AnimBuild::new(&header_bytes(V40, &long, &name16("tanktree1234567"), 4, 30));
        let anim = W3dRawAnim::parse(&b.bytes, resolve(&trees3)).unwrap();
        assert_eq!(anim.name_str().len(), 31);
        assert_eq!(
            anim.name_str(),
            b"tanktree1234567.0123456789ABCDE".as_slice()
        );
        // an unterminated animation name: the strlcat source runs
        // through the hierarchy field
        let mut hb = [0u8; 44];
        hb[0..4].copy_from_slice(&V40.to_le_bytes());
        hb[4..20].copy_from_slice(b"ANIM0123456789AB"); // no NUL
        hb[20..24].copy_from_slice(b"t\0\0\0"); // hierarchy "t" NULs the source
        hb[36..40].copy_from_slice(&4u32.to_le_bytes());
        hb[40..44].copy_from_slice(&30u32.to_le_bytes());
        let b = AnimBuild::new(&hb);
        let anim = W3dRawAnim::parse(&b.bytes, resolve(&trees)).unwrap();
        // "t." + "ANIM0123456789ABt" (the source ran into the 't')
        assert_eq!(anim.name_str(), b"t.ANIM0123456789ABt".as_slice());
    }

    #[test]
    fn translation_queries_lerp_and_wrap() {
        let trees = vec![(b"t".to_vec(), tree(2))];
        let mut b = AnimBuild::new(&header_bytes(V40, &name16("A"), &name16("t"), 3, 30));
        // 4 frames of X: 0, 1, 2, 3
        b.motion_channel(
            0,
            3,
            1,
            ANIM_CHANNEL_X as u16,
            1,
            &[0.0, 1.0, 2.0, 3.0],
            &[],
        );
        let anim = W3dRawAnim::parse(&b.bytes, resolve(&trees)).unwrap();

        // no channels: the zero vector
        assert_eq!(anim.get_translation(0, 1.5), [0.0, 0.0, 0.0]);
        // integral frame: frame0 = frame, ratio 0
        assert_eq!(anim.get_translation(1, 1.0), [1.0, 0.0, 0.0]);
        // half frame: lerp between frames 1 and 2
        assert_eq!(anim.get_translation(1, 1.5), [1.5, 0.0, 0.0]);
        // 1.25: frame0 = 1 (1.25-0.499999 = 0.750001 -> 1), ratio 0.25
        assert_eq!(anim.get_translation(1, 1.25), [1.25, 0.0, 0.0]);
        // the last frame + 0.5: frame0 = 3, frame1 wraps to 0
        assert_eq!(anim.get_translation(1, 3.5), [1.5, 0.0, 0.0]);
        // below the channel's range: the identity
        let trees2 = trees.clone();
        let mut b = AnimBuild::new(&header_bytes(V40, &name16("A"), &name16("t"), 8, 30));
        // frames 2..5 only
        b.motion_channel(
            2,
            5,
            1,
            ANIM_CHANNEL_X as u16,
            1,
            &[1.0, 2.0, 3.0, 4.0],
            &[],
        );
        let anim = W3dRawAnim::parse(&b.bytes, resolve(&trees2)).unwrap();
        assert_eq!(anim.get_translation(1, 0.0), [0.0, 0.0, 0.0]);
        // frame 1.5 is still below the range at frame0=1 (the identity
        // 0), but frame1=2 is the channel's first key — the lerp blends
        // 0 into 1.0, reaching 0.5 (the identity fills the gap)
        assert_eq!(anim.get_translation(1, 1.5), [0.5, 0.0, 0.0]);
        assert_eq!(anim.get_translation(1, 2.0), [1.0, 0.0, 0.0]);
        // a negative frame: frame0 = -1, ratio 0 -> identity (out of range)
        assert_eq!(anim.get_translation(1, -1.0), [0.0, 0.0, 0.0]);
    }

    #[test]
    fn orientation_queries_slerp() {
        let trees = vec![(b"t".to_vec(), tree(2))];
        let mut b = AnimBuild::new(&header_bytes(V40, &name16("A"), &name16("t"), 4, 30));
        // two quaternion keyframes on pivot 1: 90-degree X rotations
        // (s45,0,0,s45) and its conjugate (−s45,0,0,s45)
        let s45: f32 = std::f32::consts::FRAC_1_SQRT_2;
        b.motion_channel(
            0,
            1,
            4,
            ANIM_CHANNEL_Q as u16,
            1,
            &[s45, 0.0, 0.0, s45, -s45, 0.0, 0.0, s45],
            &[],
        );
        let anim = W3dRawAnim::parse(&b.bytes, resolve(&trees)).unwrap();

        // no Q channel: the identity
        assert_eq!(anim.get_orientation(0, 1.5), [0.0, 0.0, 0.0, 1.0]);
        // frame 0: q0 verbatim
        let q = anim.get_orientation(1, 0.0);
        assert_eq!(q, [s45, 0.0, 0.0, s45]);
        // frame 1: q1 verbatim
        let q = anim.get_orientation(1, 1.0);
        assert_eq!(q, [-s45, 0.0, 0.0, s45]);
        // halfway: a 90-degree X rotation is slerp(q0,q1,0.5); both
        // keyframes are 90 degrees from the identity around X in
        // opposite directions, so the halfway point is the identity
        // rotation... (0-degree net) — the exact table-based value is
        // pinned by the goldens; check the net rotation angle instead
        let q = anim.get_orientation(1, 0.5);
        let angle = 2.0 * f32::atan2((q[0] * q[0] + q[1] * q[1] + q[2] * q[2]).sqrt(), q[3]);
        assert!(
            angle.abs() < 1.0e-3,
            "halfway should be ~identity, got {q:?}"
        );
    }

    #[test]
    fn orientation_near_parallel_takes_the_linear_path() {
        let trees = vec![(b"t".to_vec(), tree(1))];
        let mut b = AnimBuild::new(&header_bytes(V40, &name16("A"), &name16("t"), 4, 30));
        // identical unit quaternions in both keyframes: cos_t = 1
        // exactly, the 1-cos_t < EPS^2 linear branch
        let q = [0.5f32, 0.5, 0.5, 0.5];
        b.motion_channel(
            0,
            1,
            4,
            ANIM_CHANNEL_Q as u16,
            0,
            &[q[0], q[1], q[2], q[3], q[0], q[1], q[2], q[3]],
            &[],
        );
        let anim = W3dRawAnim::parse(&b.bytes, resolve(&trees)).unwrap();
        let got = anim.get_orientation(0, 0.5);
        for (a, e) in got.iter().zip(q.iter()) {
            assert!(
                (a - e).abs() < 1.0e-6,
                "linear slerp of equal quats: {got:?}"
            );
        }
    }

    #[test]
    fn transform_queries_compose_rotation_and_translation() {
        let trees = vec![(b"t".to_vec(), tree(2))];
        let mut b = AnimBuild::new(&header_bytes(V40, &name16("A"), &name16("t"), 3, 30));
        b.motion_channel(0, 2, 1, ANIM_CHANNEL_X as u16, 1, &[1.0, 2.0, 3.0], &[]);
        b.motion_channel(0, 2, 1, ANIM_CHANNEL_Y as u16, 1, &[0.0, 4.0, 8.0], &[]);
        b.motion_channel(0, 2, 1, ANIM_CHANNEL_Z as u16, 1, &[-1.0, -1.0, -1.0], &[]);
        let anim = W3dRawAnim::parse(&b.bytes, resolve(&trees)).unwrap();

        // ratio == 0: the identity rotation with the frame-0 translation
        let m = anim.get_transform(1, 0.0);
        assert_eq!(
            m,
            [1.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, -1.0]
        );
        // halfway: translation lerped (rotation stays identity — no Q)
        let m = anim.get_transform(1, 0.5);
        assert_eq!(m[3], 1.5);
        assert_eq!(m[7], 2.0);
        assert_eq!(m[11], -1.0);
        assert_eq!(m[0], 1.0);
        assert_eq!(m[5], 1.0);

        // with a Q channel: the rotation lands through Build_Matrix3D
        let mut b = AnimBuild::new(&header_bytes(V40, &name16("A"), &name16("t"), 3, 30));
        b.motion_channel(
            0,
            2,
            4,
            ANIM_CHANNEL_Q as u16,
            1,
            &[
                0.0,
                0.0,
                0.0,
                1.0,
                0.0,
                s45x(),
                0.0,
                s45x(),
                0.0,
                0.0,
                0.0,
                1.0,
            ],
            &[],
        );
        let anim = W3dRawAnim::parse(&b.bytes, resolve(&trees)).unwrap();
        // frame 0: identity rotation, no translation channels
        let m = anim.get_transform(1, 0.0);
        assert_eq!(m[0], 1.0);
        assert_eq!(m[3], 0.0);
        // frame 1: 90-degree Y rotation (Build_Matrix3D of (0,s45,0,s45))
        let m = anim.get_transform(1, 1.0);
        assert_eq!(m[2].to_bits(), 0x3f7f_ffff); // sin(90) ~ 1.0 through the f64 promotion
        assert_eq!(m[8].to_bits(), 0xbf7f_ffff);
        assert_eq!(m[0].to_bits(), 0x3380_0000); // cos(90) residue
    }

    fn s45x() -> f32 {
        std::f32::consts::FRAC_1_SQRT_2
    }

    #[test]
    fn visibility_queries_read_bits_and_default() {
        let trees = vec![(b"t".to_vec(), tree(2))];
        let mut b = AnimBuild::new(&header_bytes(V40, &name16("A"), &name16("t"), 16, 30));
        // frames 0..15, bits 10101010..., default 1
        b.bit_channel(
            0,
            15,
            BIT_CHANNEL_VIS as u16,
            1,
            1,
            &[0b1010_1010, 0b0101_0101],
        );
        let anim = W3dRawAnim::parse(&b.bytes, resolve(&trees)).unwrap();

        assert!(anim.get_visibility(0, 0.0)); // no channel: always visible
        // (int)frame truncation
        assert!(!anim.get_visibility(1, 0.0)); // bit 0 = 0
        assert!(anim.get_visibility(1, 1.0)); // bit 1 = 1
        assert!(!anim.get_visibility(1, 2.0)); // bit 2 = 0
        assert!(anim.get_visibility(1, 7.0)); // bit 7 = 1
        assert!(anim.get_visibility(1, 8.0)); // bit 8 = 1 (second byte's bit 0)
        assert!(anim.get_visibility(1, 14.0)); // bit 14 = 1
        // past the range: the default
        assert!(anim.get_visibility(1, 16.0));
        assert!(anim.get_visibility(1, 100.0));
        // negative frames: the default
        assert!(anim.get_visibility(1, -1.0));
        // (int)(-0.5) = 0 (truncation toward zero): in range, bit 0 = 0
        assert!(!anim.get_visibility(1, -0.5));
    }

    #[test]
    fn visibility_negative_half_frame_truncates_to_zero() {
        let trees = vec![(b"t".to_vec(), tree(1))];
        let mut b = AnimBuild::new(&header_bytes(V40, &name16("A"), &name16("t"), 16, 30));
        b.bit_channel(0, 15, BIT_CHANNEL_VIS as u16, 0, 0, &[0b0000_0001, 0]);
        let anim = W3dRawAnim::parse(&b.bytes, resolve(&trees)).unwrap();
        // (int)(-0.5) = 0 (truncation toward zero) -> bit 0 = 1
        assert!(anim.get_visibility(0, -0.5));
        // (int)(-1.0) = -1 -> below the range -> the default (0)
        assert!(!anim.get_visibility(0, -1.0));
    }

    #[test]
    fn num_frames_negative_as_int_wraps_frame1_to_zero() {
        // NumFrames 0x80000000 reads as the int -2147483648: every
        // frame1 >= it, so frame1 always wraps to 0
        let trees = vec![(b"t".to_vec(), tree(1))];
        let b = AnimBuild::new(&header_bytes(
            V40,
            &name16("A"),
            &name16("t"),
            0x8000_0000,
            30,
        ));
        let mut b = b;
        b.motion_channel(0, 1, 1, ANIM_CHANNEL_X as u16, 0, &[5.0, 7.0], &[]);
        let anim = W3dRawAnim::parse(&b.bytes, resolve(&trees)).unwrap();
        assert_eq!(anim.num_frames(), 0x8000_0000);
        // frame 0.5: frame0 = 0, frame1 wraps to 0 -> lerp(x0, x0, .5)
        assert_eq!(anim.get_translation(0, 0.5), [5.0, 0.0, 0.0]);
        // total time: (float)(int)0x80000000 = -2.1474836e9
        assert!(anim.total_time() < 0.0);
    }

    #[test]
    fn bit_channel_get_bit_boundaries() {
        let chan = W3dBitChannel {
            pivot_idx: 0,
            channel_type: BIT_CHANNEL_VIS,
            default_val: 1,
            first_frame: 2,
            last_frame: 5,
            bits: vec![0b0000_0100], // bit 2 set
        };
        assert_eq!(chan.get_bit(1), 1); // below: default
        assert_eq!(chan.get_bit(2), 0); // in range, packed bit 0
        assert_eq!(chan.get_bit(3), 0); // packed bit 1
        assert_eq!(chan.get_bit(4), 1); // packed bit 2 (set)
        assert_eq!(chan.get_bit(5), 0);
        assert_eq!(chan.get_bit(6), 1); // above: default
    }

    #[test]
    fn motion_channel_get_vector_identity_boundaries() {
        let quat = W3dMotionChannel {
            pivot_idx: 0,
            channel_type: ANIM_CHANNEL_Q,
            first_frame: 1,
            last_frame: 2,
            vector_len: 4,
            data: vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0],
        };
        let mut out = [0f32; 4];
        quat.get_vector(0, &mut out);
        assert_eq!(out, [0.0, 0.0, 0.0, 1.0]); // identity below the range
        quat.get_vector(1, &mut out);
        assert_eq!(out, [1.0, 2.0, 3.0, 4.0]);
        quat.get_vector(3, &mut out);
        assert_eq!(out, [0.0, 0.0, 0.0, 1.0]); // identity above
        assert_eq!(quat.get_vector_as_quat(2), [5.0, 6.0, 7.0, 8.0]);
        // the scalar channel: only slot 0 is written
        let scalar = W3dMotionChannel {
            pivot_idx: 0,
            channel_type: ANIM_CHANNEL_X,
            first_frame: 0,
            last_frame: 0,
            vector_len: 1,
            data: vec![9.5],
        };
        let mut out = [0f32; 4];
        scalar.get_vector(0, &mut out);
        assert_eq!(out, [9.5, 0.0, 0.0, 0.0]);
        assert_eq!(scalar.get_scalar(0), 9.5);
        assert_eq!(scalar.get_scalar(1), 0.0);
    }
}
