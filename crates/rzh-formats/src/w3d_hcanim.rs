// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Ported from the Zero Hour source release (baseline 571a7c5):
//   GeneralsMD/Code/Libraries/Source/WWVegas/WW3D2/hcanim.cpp
//     (HCompressedAnimClass::Load_W3D, read_channel overloads,
//      read_bit_channel, add_channel overloads, add_bit_channel,
//      Get_Translation, Get_Orientation, Get_Transform, Get_Visibility,
//      Is_Node_Motion_Present and the Has_* channel probes)
//   GeneralsMD/Code/Libraries/Source/WWVegas/WW3D2/hcanim.h
//     (the HCompressedAnimClass layout; the NodeCompressedMotionStruct
//      union — tc/ad/vd alias the same four channel slots)
//   GeneralsMD/Code/Libraries/Source/WWVegas/WW3D2/hanim.h
//     (the HAnimClass query surface the compressed animation
//      implements, Get_Total_Time included)
//   GeneralsMD/Code/Libraries/Source/WWVegas/WW3D2/motchan.cpp /
//     motchan.h (TimeCodedMotionChannelClass::Load_W3D, Get_Vector,
//     Get_QuatVector, get_index, binary_search_index;
//     TimeCodedBitChannelClass::Load_W3D, Get_Bit;
//     AdaptiveDeltaMotionChannelClass::Load_W3D, decompress overloads,
//     getframe, Get_Vector, Get_QuatVector; the 256-entry filtertable
//     with its constructor-time generation)
//   GeneralsMD/Code/Libraries/Source/WWVegas/WW3D2/w3d_file.h
//     (W3dCompressedAnimHeaderStruct, W3dTimeCodedAnimChannelStruct,
//      W3dTimeCodedBitChannelStruct, W3dAdaptiveDeltaAnimChannelStruct,
//      the chunk ids, the channel-type and flavor enums,
//      W3D_TIMECODED_BINARY_MOVEMENT_FLAG, W3D_TIMECODED_BIT_MASK)
//   GeneralsMD/Code/Libraries/Source/WWVegas/WW3D2/hanimmgr.cpp
//     (HAnimManagerClass::Load_Compressed_Anim — the entry that opens
//      the W3D_CHUNK_COMPRESSED_ANIMATION container)
//   GeneralsMD/Code/Libraries/Source/WWVegas/WW3D2/assetmgr.cpp /
//     htreemgr.cpp (the Get_HTree lookup the loader drives — lowercase
//      exact match, modelled as the parse callback)
//   Core/Libraries/Source/WWVegas/WWMath/wwmath.h / wwmath.cpp
//     (Float_To_Int_Floor, Fast_Acos/Fast_Sin over the WWMath::Init
//      tables, WWMATH_EPSILON/PI, WWMath::Lerp, DEG_TO_RAD and
//      Sinf_Legacy — the USE_DETERMINISTIC_MATH branch)
//   Core/Libraries/Source/WWVegas/WWMath/quat.cpp (Fast_Slerp — the
//     portable branch) and quat.h (Build_Matrix3D — the same
//     mixed-precision builder the hierarchy and raw-anim ports
//     transcribe)
//   The pinned GameMath sources (TheSuperHackers/GameMath @ 59f7ccd4):
//     float/e_acosf.c (gm_acosf), float/s_sinf.c (gm_sinf), float/
//     k_sinf.c, float/k_cosf.c, float/e_rem_pio2f.c — the
//     deterministic-math entry points the WWMath calls reach.
// See the module docs for the format layout and the documented
// differences from the C++ (stated-contract errors where the original
// is undefined, corpus disciplines the transcription shares).

//! The **W3D compressed animation** — the motion data that fills a
//! hierarchy tree's transforms in the two compressed flavors (the
//! plan's Phase 2 W3D track, the named follow-up to the raw
//! animations, on top of the chunk container and the hierarchy trees).
//!
//! A `.w3d` compressed animation asset is a
//! [`W3D_CHUNK_COMPRESSED_ANIMATION`] container whose children describe
//! the motion of one skeleton in one of two flavors: a fixed 44-byte
//! [`animation header`](W3dCompressedAnimHeader) (whose `Flavor` field
//! picks the reader), then any number of
//! [`W3D_CHUNK_COMPRESSED_ANIMATION_CHANNEL`] motion chunks and
//! [`W3D_CHUNK_COMPRESSED_BIT_CHANNEL`] visibility chunks. This module
//! ports `HCompressedAnimClass::Load_W3D` (hcanim.cpp) and the
//! compressed channel readers it drives (motchan.cpp's
//! `TimeCodedMotionChannelClass`, `TimeCodedBitChannelClass` and
//! `AdaptiveDeltaMotionChannelClass`) — the data path the asset manager
//! reaches through `HAnimManagerClass::Load_Compressed_Anim` — plus the
//! query surface the tree update samples (parity level P0, pinned by
//! golden vectors generated from the transcription in
//! `tools/differential/gen_hcanim_golden.cpp`).
//!
//! [`W3D_CHUNK_COMPRESSED_ANIMATION`]: W3D_CHUNK_COMPRESSED_ANIMATION
//!
//! # The two flavors
//!
//! - **time-coded** ([`ANIM_FLAVOR_TIMECODED`]): each packet is a
//!   time-code word plus `VectorLen` float words; a packet's MSB marks
//!   a *binary movement* (returned verbatim instead of interpolated).
//!   The channel caches its last packet index and finds the bracketing
//!   pair through a one-packet look-ahead followed by a binary search;
//!   the query math is `WWMath::Lerp` between the two packets, with the
//!   time codes converted to `f32` **numerically** (a 2^24+1 time code
//!   rounds — the ratio is computed against the rounded values, pinned
//!   by the goldens). Quaternion channels slerp their packets through
//!   the table-based `Fast_Slerp`.
//! - **adaptive-delta** ([`ANIM_FLAVOR_ADAPTIVE_DELTA`]): the data is
//!   `VectorLen` base floats followed by 9-byte packets per vector
//!   index per 16-frame block — a filter-table index byte plus 16
//!   signed nybbles (deltas). The channel decompresses on demand and
//!   caches two consecutive frames, sliding the window for sequential
//!   access; a jump of more than two frames takes the *cache-continue*
//!   path, which passes the cached row-1 data with `src_idx =
//!   CacheFrame` and therefore **re-applies the delta for
//!   CacheFrame+1** — the original's off-by-one, transcribed verbatim
//!   and pinned by the goldens (the cache self-heals on the next
//!   decompress). The decompression walk also carries the original's
//!   never-reset `done` flag: once the first vector index reaches the
//!   target frame, every later vector index stops after its **first
//!   16-frame block** — a multi-block walk only walks the full frame
//!   range for `vi == 0` (the goldens pin it).
//!
//! # The load-path quirks
//!
//! - **the animation is defined against a tree**: `Load_W3D` looks its
//!   hierarchy up in the asset manager (`Get_HTree`) and takes the
//!   *tree's* pivot count as its own — the file has no bone count. The
//!   port models the lookup as [`W3dCompressedAnim::parse`]'s
//!   `resolve_base_pose` callback (lowercase exact match, like
//!   `HTreeManagerClass::Get_Tree`); a missing tree fails the load;
//! - **the header is read with the raw animation's struct size**: the
//!   loader reads `sizeof(W3dAnimHeaderStruct)` (44) — not
//!   `sizeof(W3dCompressedAnimHeaderStruct)` — into the compressed
//!   header; both structs are 44 bytes so the read is complete (the
//!   port reads its own 44 and states the coincidence);
//! - **the name composition runs through the raw header bytes**:
//!   `strcpy(Name, aheader.HierarchyName)` reads the 16-byte hierarchy
//!   field and *keeps going* through the header's `NumFrames` word when
//!   the field has no NUL (capped at the 44-byte struct), `strlcat`
//!   then appends `"."` and the animation name (whose source can
//!   equally run through the rest of the header), truncating at 31
//!   characters — the same composition the raw-anim port transcribes;
//! - **`FrameRate` is a `uint16`** converted to the class's `float`
//!   member numerically (0xFFFF loads as 65535.0; 0 makes
//!   [`total_time`](W3dCompressedAnim::total_time) divide by zero);
//!   `NumFrames` is the class's `int` (0x80000000 reads negative);
//! - **the flavor decides the reader, not the channel bytes**: a
//!   time-coded-flavored animation parses an adaptive-delta-shaped
//!   channel with the time-coded reader (the AD struct's `Scale` word
//!   lands as the first time code) and vice versa — pinned by the
//!   goldens with cross-shaped corpora crafted to stay in bounds;
//! - **only the plain channel types route**: `add_channel`'s switch has
//!   cases for `ANIM_CHANNEL_X/Y/Z/Q` (0..6) only — the
//!   `ANIM_CHANNEL_TIMECODED_*`/`ANIM_CHANNEL_ADAPTIVEDELTA_*` values
//!   (7..14) that the format's enum also defines are stored nowhere
//!   (the original also leaks them); an unknown bit-channel type never
//!   routes either;
//! - **an unknown flavor loads**: the loader's flavor switch has no
//!   default case, so it reads no motion channels at all (the chunk
//!   bytes are skipped), the bit channels still route, and
//!   [`get_translation`](W3dCompressedAnim::get_translation) returns
//!   the zero vector it pre-assigns. The original leaves
//!   `Get_Orientation`'s and `Get_Transform`'s caller-supplied
//!   destinations *untouched* for unknown flavors (the compiled-out
//!   `WWASSERT` default) — a function of the caller, not the animation;
//!   the port returns the identity there instead (a stated difference,
//!   below);
//! - **channels for missing bones are discarded, not fatal** — the
//!   "Please re-export" path, the same contract as the raw animations;
//!   duplicate channels overwrite (the original leaks the previous
//!   one);
//! - **the queries are stateful**: the time-coded channels cache their
//!   last packet index and the adaptive-delta channels cache two
//!   decompressed frames, so the query methods take `&mut self` —
//!   exactly the mutability the original hides behind non-const
//!   methods and pointer members. Query results can depend on the
//!   *order* of prior queries (the cache-continue off-by-one above);
//!   the goldens replay a fixed probe order;
//! - **the bit channel's `DefaultVal` is dead data**: `Get_Bit` never
//!   reads it — frames below the first time code return *packet 0's*
//!   bit (clamped from the `idx--` underflow), frames past the last
//!   time code return the *last* packet's bit.
//!
//! # Differences from the C++
//!
//! Release-build undefined behaviour becomes either [`HcanimError`]
//! (the load fails) or a documented panic (a malformed *loadable*
//! channel is queried): a bad first chunk, a short header, an
//!   unresolvable hierarchy tree, a short channel read, or a bit
//!   channel with zero time codes (whose wrapped ~4 GiB read fails in
//!   both — the original first writes one word past its zero-size
//!   allocation, collateral the port skips), all fail the load. A
//!   time-coded channel whose declared `NumTimeCodes` disagrees with
//!   the data present, unsorted time codes, an adaptive-delta channel
//!   with `VectorLen` above 4 (the original smashes a 4-float stack
//!   scratch) or 0, or `NumFrames` 0, load in the original and read out
//!   of bounds on the first query — the port panics on the array
//!   access; the golden corpus disciplines all of these away. Where
//!   the C++ reads *unbounded* C strings past the 44-byte header the
//!   port caps the read at the struct; where the C++ reinterprets the
//!   packet words as bytes the port reads them little-endian (the
//!   x86/Windows storage order). The `HAnimClass` base-class state
//!   that is not load-path (ref counts, the combo/blend machinery in
//!   hanim.cpp) is out of scope for the format layer and lands with the
//!   animation runtime slice. The HLOD assemblies that reference these
//!   animations are the next Phase 2 slice.
//!
//! # Examples
//!
//! ```
//! use rzh_formats::w3d_hcanim::{W3dCompressedAnim, ANIM_FLAVOR_ADAPTIVE_DELTA};
//! use rzh_formats::w3d_htree::W3dHtree;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! # let payload: Vec<u8> = Vec::new(); // the COMPRESSED_ANIMATION container's bytes
//! # if payload.is_empty() { return Ok(()); }
//! // The caller owns the tree table: resolve the animation's hierarchy
//! // the way the asset manager does (lowercase exact match) and hand the
//! // tree to the parser. No tree, no animation.
//! let trees: Vec<(Vec<u8>, W3dHtree)> = Vec::new();
//! let mut anim = W3dCompressedAnim::parse(&payload, |name| {
//!     trees
//!         .iter()
//!         .find(|(n, _)| *n == name.to_ascii_lowercase())
//!         .map(|(_, tree)| tree)
//! })?;
//! assert_eq!(anim.flavor(), ANIM_FLAVOR_ADAPTIVE_DELTA);
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

/// The compressed-animation container a `.w3d` file wraps a compressed
/// animation asset in.
pub const W3D_CHUNK_COMPRESSED_ANIMATION: u32 = 0x0000_0280;
/// The 44-byte `W3dCompressedAnimHeaderStruct`.
pub const W3D_CHUNK_COMPRESSED_ANIMATION_HEADER: u32 = 0x0000_0281;
/// One compressed motion channel — time-coded or adaptive-delta,
/// depending on the animation's flavor.
pub const W3D_CHUNK_COMPRESSED_ANIMATION_CHANNEL: u32 = 0x0000_0282;
/// One compressed bit channel (time-coded).
pub const W3D_CHUNK_COMPRESSED_BIT_CHANNEL: u32 = 0x0000_0283;

/// The length of every fixed name field (`W3D_NAME_LEN`).
pub const W3D_NAME_LEN: usize = 16;
/// The size of the compressed animation header struct (the same 44 the
/// raw animation's header occupies — the loader reads the raw struct's
/// `sizeof`, which the port states as its own 44).
pub const COMPRESSED_ANIM_HEADER_SIZE: u32 = 44;
/// The size of the time-coded motion channel struct (four fixed fields
/// and the first packet word).
pub const TIMECODED_ANIM_CHANNEL_STRUCT_SIZE: u32 = 12;
/// The size of the time-coded bit channel struct.
pub const TIMECODED_BIT_CHANNEL_STRUCT_SIZE: u32 = 12;
/// The size of the adaptive-delta channel struct (five fixed fields,
/// the scale and the first data word).
pub const ADAPTIVE_DELTA_CHANNEL_STRUCT_SIZE: u32 = 16;

/// The channel types (the `Flags` field): translation on X. Only the
/// plain types route — the `TIMECODED_`/`ADAPTIVEDELTA_` variants the
/// enum also defines (7..14) have no `add_channel` case and are stored
/// nowhere.
pub const ANIM_CHANNEL_X: u32 = 0;
/// Translation on Y.
pub const ANIM_CHANNEL_Y: u32 = 1;
/// Translation on Z.
pub const ANIM_CHANNEL_Z: u32 = 2;
/// The quaternion orientation channel.
pub const ANIM_CHANNEL_Q: u32 = 6;
/// The bit channel types: visibility.
pub const BIT_CHANNEL_VIS: u32 = 0;

/// The time-coded flavor: packets of (time code, `VectorLen` floats).
pub const ANIM_FLAVOR_TIMECODED: u16 = 0;
/// The adaptive-delta flavor: base floats plus 9-byte nybble packets.
pub const ANIM_FLAVOR_ADAPTIVE_DELTA: u16 = 1;

/// `W3D_TIMECODED_BINARY_MOVEMENT_FLAG` — the MSB that marks a
/// time-coded packet as a binary (non-interpolated) movement.
pub const W3D_TIMECODED_BINARY_MOVEMENT_FLAG: u32 = 0x8000_0000;
/// `W3D_TIMECODED_BIT_MASK` — the MSB of a time-coded bit channel's
/// time-code word that carries the bit itself.
pub const W3D_TIMECODED_BIT_MASK: u32 = 0x8000_0000;

/// The adaptive-delta packet size: one filter-index byte plus eight
/// nybble bytes (sixteen 4-bit deltas per block, per vector index).
pub const ADAPTIVE_DELTA_PACKET_SIZE: usize = 9;

// ---------------------------------------------------------------------------
// errors
// ---------------------------------------------------------------------------

/// Errors the compressed-animation load path can produce.
///
/// The original's `Load_W3D` has exactly one failure code —
/// `LOAD_ERROR` — and the port states the same contract: a bad first
/// chunk, a short header, an unresolvable hierarchy tree, or a short
/// channel read all fail the load outright.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum HcanimError {
    /// `LOAD_ERROR` — the load failed outright.
    LoadFailed,
    /// The chunk container itself errored (depth cap).
    Chunk(crate::chunk::ChunkError),
}

impl fmt::Display for HcanimError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LoadFailed => write!(f, "W3D compressed animation load failed"),
            Self::Chunk(e) => write!(f, "W3D compressed animation chunk error: {e}"),
        }
    }
}

impl std::error::Error for HcanimError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Chunk(e) => Some(e),
            _ => None,
        }
    }
}

impl From<crate::chunk::ChunkError> for HcanimError {
    fn from(e: crate::chunk::ChunkError) -> Self {
        Self::Chunk(e)
    }
}

// ---------------------------------------------------------------------------
// the fixed structs
// ---------------------------------------------------------------------------

/// The 44-byte `W3dCompressedAnimHeaderStruct`, decoded.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct W3dCompressedAnimHeader {
    /// File-format version (`W3D_MAKE_VERSION`) — stored but never
    /// consulted: unlike the raw animation loader, the compressed
    /// loader performs no pre-3.0 pivot shift.
    pub version: u32,
    /// The animation name (a C string in a 16-byte field).
    pub name: [u8; W3D_NAME_LEN],
    /// The hierarchy this animation drives (a C string in a 16-byte
    /// field) — the tree-lookup key.
    pub hierarchy_name: [u8; W3D_NAME_LEN],
    /// The frame count (the class member is an `int`).
    pub num_frames: u32,
    /// The frame rate as a **`uint16`** — the class member is a
    /// `float` and the loader converts numerically
    /// ([`W3dCompressedAnim::frame_rate`]).
    pub frame_rate: u16,
    /// The compression flavor — 0 time-coded, 1 adaptive-delta; the
    /// value that picks the channel reader.
    pub flavor: u16,
}

impl W3dCompressedAnimHeader {
    /// Decodes the struct from its 44 little-endian bytes.
    pub fn from_bytes(bytes: &[u8; COMPRESSED_ANIM_HEADER_SIZE as usize]) -> Self {
        let u32at = |o: usize| u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
        let u16at = |o: usize| u16::from_le_bytes(bytes[o..o + 2].try_into().unwrap());
        let mut name = [0u8; W3D_NAME_LEN];
        name.copy_from_slice(&bytes[4..20]);
        let mut hierarchy_name = [0u8; W3D_NAME_LEN];
        hierarchy_name.copy_from_slice(&bytes[20..36]);
        Self {
            version: u32at(0),
            name,
            hierarchy_name,
            num_frames: u32at(36),
            frame_rate: u16at(40),
            flavor: u16at(42),
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
fn header_cstr(bytes: &[u8; COMPRESSED_ANIM_HEADER_SIZE as usize], start: usize) -> &[u8] {
    let end = bytes[start..]
        .iter()
        .position(|&b| b == 0)
        .map_or(COMPRESSED_ANIM_HEADER_SIZE as usize, |p| start + p);
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

/// The float→`uint32` conversion the compressed queries perform
/// (`tc0 = frame`, `uint32 frame1 = frame`). C++ leaves the
/// out-of-range conversion undefined; the transcription's compilers
/// (gcc/clang on x86-64) lower it to a 64-bit `cvttss2si` plus
/// truncation, which `(frame as i64) as u32` reproduces exactly (NaN
/// and ±inf → the indefinite pattern's low word, negatives →
/// `0xFFFF_FFFF`-style wraps). ARM's saturating `fcvtzu` differs, so
/// the golden corpus probes only non-negative frames — inside that
/// range every platform truncates identically.
fn float_to_u32(frame: f32) -> u32 {
    (frame as i64) as u32
}

// ---------------------------------------------------------------------------
// the time-coded motion channel (motchan.cpp)
// ---------------------------------------------------------------------------

/// One loaded time-coded motion channel —
/// `TimeCodedMotionChannelClass` after `Load_W3D`.
///
/// The packet array stores one time-code word (with
/// [`W3D_TIMECODED_BINARY_MOVEMENT_FLAG`] where the exporter marked a
/// binary movement) followed by `vector_len` float words per packet.
/// The channel caches its last packet index — the query methods take
/// `&mut self`, mirroring the original's non-const `Get_Vector`.
#[derive(Debug, Clone, PartialEq)]
pub struct W3dTimeCodedMotionChannel {
    /// The pivot this channel drives (`PivotIdx`).
    pub pivot_idx: u32,
    /// The channel type (`Type`, the wire `Flags` byte).
    pub channel_type: u32,
    /// The number of floats per packet (`VectorLen`).
    pub vector_len: usize,
    /// The packet stride in words (`PacketSize` = `VectorLen + 1`).
    pub packet_size: u32,
    /// The declared number of packets (`NumTimeCodes`). Must agree with
    /// the data present: the query path indexes the packet array by the
    /// declared count and panics past the end where the original reads
    /// out of bounds (corpus-disciplined; see the module docs).
    pub num_time_codes: u32,
    /// The absolute word index of the last packet
    /// (`LastTimeCodeIdx` = `(NumTimeCodes - 1) * PacketSize`).
    pub last_time_code_idx: u32,
    /// The packet data — time codes and vector words interleaved as
    /// stored.
    pub data: Vec<u32>,
    /// The last packet index used (`CachedIdx`) — the scan cache.
    pub cached_idx: u32,
}

impl W3dTimeCodedMotionChannel {
    /// `TimeCodedMotionChannelClass::Get_Vector` — writes `vector_len`
    /// values for `frame` into `setvec`: the last packet's values at or
    /// past the last time code, the current packet's values verbatim
    /// when the *next* packet carries the binary-movement flag, and the
    /// `WWMath::Lerp` between the bracketing packets otherwise (with
    /// the time codes converted to `f32` **numerically** — a time code
    /// above 2^24 rounds, and the ratio uses the rounded values).
    ///
    /// Panics when `setvec` is shorter than `vector_len` (the original
    /// overruns the caller's buffer — a `VectorLen`-2 X channel
    /// legitimately reaches the caller's second slot).
    pub fn get_vector(&mut self, frame: f32, setvec: &mut [f32]) {
        assert!(
            setvec.len() >= self.vector_len,
            "Get_Vector destination shorter than VectorLen (the original overruns it)"
        );
        let tc0 = float_to_u32(frame);

        let pidx = self.get_index(tc0);

        if pidx
            == self
                .num_time_codes
                .wrapping_sub(1)
                .wrapping_mul(self.packet_size)
        {
            let base = pidx as usize + 1;
            for (i, slot) in setvec.iter_mut().enumerate().take(self.vector_len) {
                *slot = f32::from_bits(self.data[base + i]);
            }
            return;
        }
        let p2idx = pidx.wrapping_add(self.packet_size);

        let time = self.data[p2idx as usize];

        if time & W3D_TIMECODED_BINARY_MOVEMENT_FLAG != 0 {
            // its a binary movement!
            let base = pidx as usize + 1;
            for (i, slot) in setvec.iter_mut().enumerate().take(self.vector_len) {
                *slot = f32::from_bits(self.data[base + i]);
            }
            return;
        }

        let time1 = (self.data[pidx as usize] & !W3D_TIMECODED_BINARY_MOVEMENT_FLAG) as f32;
        let time2 = (time & !W3D_TIMECODED_BINARY_MOVEMENT_FLAG) as f32;

        let ratio = (frame - time1) / (time2 - time1);

        let frame1 = pidx as usize + 1;
        let frame2 = p2idx as usize + 1;
        for (i, slot) in setvec.iter_mut().enumerate().take(self.vector_len) {
            *slot = lerp(
                f32::from_bits(self.data[frame1 + i]),
                f32::from_bits(self.data[frame2 + i]),
                ratio,
            );
        }
    }

    /// `TimeCodedMotionChannelClass::Get_QuatVector` — the quaternion
    /// read `Get_Orientation` drives: the packet pair read as four
    /// floats each and slerped through the table-based `Fast_Slerp`
    /// (the same branches the raw-anim port pins: the linear
    /// near-parallel fallback, the hemisphere flip, the `gm_acosf`
    /// fallback window).
    ///
    /// The four-component reads are the original's flat array indexes:
    /// a channel whose `vector_len` is below 4 reads into the following
    /// packets (in bounds) and past the array on the last packet — a
    /// panic here, corpus-disciplined away (real quaternion channels
    /// carry 4).
    pub fn get_quat_vector(&mut self, frame: f32) -> [f32; 4] {
        // assert(VectorLen == 4) — release no-op
        let tc0 = float_to_u32(frame);

        let pidx = self.get_index(tc0);

        if pidx
            == self
                .num_time_codes
                .wrapping_sub(1)
                .wrapping_mul(self.packet_size)
        {
            let base = pidx as usize + 1;
            return [
                f32::from_bits(self.data[base]),
                f32::from_bits(self.data[base + 1]),
                f32::from_bits(self.data[base + 2]),
                f32::from_bits(self.data[base + 3]),
            ];
        }
        let p2idx = pidx.wrapping_add(self.packet_size);

        let time = self.data[p2idx as usize];

        if time & W3D_TIMECODED_BINARY_MOVEMENT_FLAG != 0 {
            // its a binary movement!
            let base = pidx as usize + 1;
            return [
                f32::from_bits(self.data[base]),
                f32::from_bits(self.data[base + 1]),
                f32::from_bits(self.data[base + 2]),
                f32::from_bits(self.data[base + 3]),
            ];
        }

        let time1 = (self.data[pidx as usize] & !W3D_TIMECODED_BINARY_MOVEMENT_FLAG) as f32;
        let time2 = (time & !W3D_TIMECODED_BINARY_MOVEMENT_FLAG) as f32;

        let ratio = (frame - time1) / (time2 - time1);

        let frame1 = pidx as usize + 1;
        let frame2 = p2idx as usize + 1;
        let q1 = [
            f32::from_bits(self.data[frame1]),
            f32::from_bits(self.data[frame1 + 1]),
            f32::from_bits(self.data[frame1 + 2]),
            f32::from_bits(self.data[frame1 + 3]),
        ];
        let q2 = [
            f32::from_bits(self.data[frame2]),
            f32::from_bits(self.data[frame2 + 1]),
            f32::from_bits(self.data[frame2 + 2]),
            f32::from_bits(self.data[frame2 + 3]),
        ];

        fast_slerp(&q1, &q2, ratio)
    }

    /// `TimeCodedMotionChannelClass::get_index` — the packet index for
    /// a time code: the cached packet when the look-ahead brackets it,
    /// one look-ahead step, then the binary search (which also serves
    /// rewinds — a time code below the cached packet's).
    fn get_index(&mut self, timecode: u32) -> u32 {
        // assert(CachedIdx <= LastTimeCodeIdx) — release no-op
        let mut time = self.data[self.cached_idx as usize] & !W3D_TIMECODED_BINARY_MOVEMENT_FLAG;

        if timecode >= time {
            // possibly in the current packet

            // special case for end packets
            if self.cached_idx == self.last_time_code_idx {
                return self.cached_idx;
            }
            time = self.data[self.cached_idx.wrapping_add(self.packet_size) as usize]
                & !W3D_TIMECODED_BINARY_MOVEMENT_FLAG;
            if timecode < time {
                return self.cached_idx;
            }

            // Do one time look-ahead before reverting to a search
            self.cached_idx = self.cached_idx.wrapping_add(self.packet_size);
            if self.cached_idx == self.last_time_code_idx {
                return self.cached_idx;
            }
            time = self.data[self.cached_idx.wrapping_add(self.packet_size) as usize]
                & !W3D_TIMECODED_BINARY_MOVEMENT_FLAG;
            if timecode < time {
                return self.cached_idx;
            }
        }

        self.cached_idx = self.binary_search_index(timecode);

        self.cached_idx
    }

    /// `TimeCodedMotionChannelClass::binary_search_index` — the
    /// "new version that uses a binary search, and no cache". The
    /// loop's `leftIdx ^ dx` exit is the original's stuck-index escape
    /// (transcribed exactly, wrapping arithmetic included).
    fn binary_search_index(&self, timecode: u32) -> u32 {
        let mut left_idx: i32 = 0;
        let mut right_idx: i32 = self.num_time_codes.wrapping_sub(2) as i32;
        let mut idx: i32 = self.last_time_code_idx as i32;

        // special case last packet
        let mut time = self.data[idx as usize] & !W3D_TIMECODED_BINARY_MOVEMENT_FLAG;
        if timecode >= time {
            return idx as u32;
        }

        loop {
            let mut dx: i32 = right_idx - left_idx;

            dx >>= 1; // divide by 2

            dx += left_idx;

            idx = (dx as u32).wrapping_mul(self.packet_size) as i32;

            time = self.data[idx as usize] & !W3D_TIMECODED_BINARY_MOVEMENT_FLAG;

            if timecode < time {
                right_idx = dx;
                continue;
            }

            time = self.data[idx.wrapping_add(self.packet_size as i32) as usize]
                & !W3D_TIMECODED_BINARY_MOVEMENT_FLAG;

            if timecode < time {
                return idx as u32;
            }

            if (left_idx ^ dx) != 0 {
                left_idx = dx;
                continue;
            }

            //
            // if leftIdx == dx prior to assignment, then leftIdx is stuck.
            //

            left_idx += 1;
        }

        // assert(0) — release no-op (unreachable for sorted time codes;
        // a time code below the FIRST packet's loops forever in the
        // original — corpus-disciplined away, every corpus starts at 0)
    }
}

// ---------------------------------------------------------------------------
// the time-coded bit channel (motchan.cpp)
// ---------------------------------------------------------------------------

/// One loaded time-coded bit channel — `TimeCodedBitChannelClass`
/// after `Load_W3D`.
///
/// Each time-code word's MSB carries the bit itself. `DefaultVal` is
/// **dead data**: `Get_Bit` never reads it — frames below the first
/// time code return packet 0's bit (the `idx--` underflow clamp) and
/// frames past the last time code return the last packet's bit.
#[derive(Debug, Clone, PartialEq)]
pub struct W3dTimeCodedBitChannel {
    /// The pivot this channel drives (`PivotIdx`).
    pub pivot_idx: u32,
    /// The channel type (`Type`, the wire `Flags` byte).
    pub channel_type: u32,
    /// The value frames below the first time code would read if the
    /// original consulted it (`DefaultVal` — dead data, pinned as
    /// loaded state only).
    pub default_val: i32,
    /// The declared number of time-code words (`NumTimeCodes`).
    pub num_time_codes: u32,
    /// The time-code words — the bit rides the MSB of each.
    pub bits: Vec<u32>,
    /// The last index used (`CachedIdx`) — the linear scan cache.
    pub cached_idx: u32,
}

impl W3dTimeCodedBitChannel {
    /// `TimeCodedBitChannelClass::Get_Bit` — the bit for `frame`: the
    /// linear scan from the cached index (or from 0 when the frame is
    /// below the cached time code), the underflow clamp at 0, and the
    /// MSB test of the packet the scan stops at.
    pub fn get_bit(&mut self, frame: i32) -> i32 {
        // assert(frame >= 0) — release no-op (negative frames read
        // packet 0's bit through the clamp below)
        let mut idx: i32 = 0;

        let mut time = (self.bits[self.cached_idx as usize] & !W3D_TIMECODED_BIT_MASK) as i32;

        if frame >= time {
            // start from here
            idx = self.cached_idx as i32 + 1;
        }

        while idx < self.num_time_codes as i32 {
            time = (self.bits[idx as usize] & !W3D_TIMECODED_BIT_MASK) as i32;

            if frame < time {
                break;
            }
            idx += 1;
        }

        idx -= 1;

        if idx < 0 {
            idx = 0;
        }

        self.cached_idx = idx as u32;

        i32::from(self.bits[idx as usize] & W3D_TIMECODED_BIT_MASK == W3D_TIMECODED_BIT_MASK)
    }
}

// ---------------------------------------------------------------------------
// the adaptive-delta motion channel (motchan.cpp)
// ---------------------------------------------------------------------------

/// One loaded adaptive-delta motion channel —
/// `AdaptiveDeltaMotionChannelClass` after `Load_W3D`.
///
/// The data stream is `vector_len` base floats followed by
/// 9-byte packets per vector index per 16-frame block (a filter-table
/// index byte plus sixteen signed nybbles). The channel decompresses on
/// demand into a two-frame cache; the query methods take `&mut self`
/// (the original's `getframe` mutates the cache through the same
/// non-const path the time-coded channels use).
#[derive(Debug, Clone, PartialEq)]
pub struct W3dAdaptiveDeltaMotionChannel {
    /// The pivot this channel drives (`PivotIdx`).
    pub pivot_idx: u32,
    /// The channel type (`Type`, the wire `Flags` byte).
    pub channel_type: u32,
    /// The number of floats per frame (`VectorLen`). The queries
    /// require 1 or 4 — see the module docs for the stated contract.
    pub vector_len: usize,
    /// The number of frames (`NumFrames`).
    pub num_frames: u32,
    /// The filter-table scale (`Scale`).
    pub scale: f32,
    /// The data stream — base floats and packet bytes as stored words.
    pub data: Vec<u32>,
    /// The cached frame index (`CacheFrame`) — `0x7FFF_FFFF` until the
    /// first query.
    pub cache_frame: u32,
    /// The two decompressed cache rows (`CacheData`): frame
    /// `cache_frame` then `cache_frame + 1`, `vector_len` floats each.
    /// Fixed at eight floats (the format's `VectorLen` cap of 4 the
    /// queries assert) — the original's heap array as a value, so the
    /// hot hit path indexes statically-bounded rows.
    pub cache_data: [f32; 8],
}

impl W3dAdaptiveDeltaMotionChannel {
    /// `AdaptiveDeltaMotionChannelClass::Get_Vector` — the *scalar*
    /// read the translation queries drive: the value for the frame's
    /// integer part lerped with the next frame's (only `setvec[0]` is
    /// ever written, whatever the `vector_len`).
    pub fn get_vector(&mut self, frame: f32) -> f32 {
        let frame1 = float_to_u32(frame);

        let ratio = frame - frame1 as f32;

        let value1 = self.getframe(frame1, 0);
        let value2 = self.getframe(frame1.wrapping_add(1), 0);

        lerp(value1, value2, ratio)
    }

    /// `AdaptiveDeltaMotionChannelClass::Get_QuatVector` — the
    /// quaternion read: eight interleaved `getframe` calls (four
    /// components of the frame's integer part, then four of the next),
    /// slerped through the table-based `Fast_Slerp`. The call order is
    /// the original's — the cache state it leaves behind is part of the
    /// pinned behaviour.
    pub fn get_quat_vector(&mut self, frame: f32) -> [f32; 4] {
        let frame1 = float_to_u32(frame);
        let frame2 = frame1.wrapping_add(1);
        let ratio = frame - frame1 as f32;

        let q1 = [
            self.getframe(frame1, 0),
            self.getframe(frame1, 1),
            self.getframe(frame1, 2),
            self.getframe(frame1, 3),
        ];

        let q2 = [
            self.getframe(frame2, 0),
            self.getframe(frame2, 1),
            self.getframe(frame2, 2),
            self.getframe(frame2, 3),
        ];

        fast_slerp(&q1, &q2, ratio)
    }

    /// `AdaptiveDeltaMotionChannelClass::getframe` — the decompressed
    /// value of `frame_idx` at `vector_idx`, through the two-row cache:
    /// row hits, the full re-decompress below the cache, the sliding
    /// window at `+2`, and the cache-continue path beyond (which
    /// re-applies the delta for `CacheFrame + 1` — the original's
    /// off-by-one, transcribed verbatim; see the module docs).
    ///
    /// Panics when `vector_len` exceeds 4 (the original's stack
    /// scratch is four floats) or when the vector index runs past the
    /// cache rows (a `vector_len`-below-4 channel queried as a
    /// quaternion — corpus-disciplined away).
    #[inline(always)]
    fn getframe(&mut self, mut frame_idx: u32, vector_idx: usize) -> f32 {
        assert!(
            self.vector_len <= 4,
            "getframe needs VectorLen <= 4 (the original's stack scratch is four floats)"
        );
        // Make sure frame_idx is valid
        if frame_idx >= self.num_frames {
            frame_idx = self.num_frames.wrapping_sub(1);
        }

        let vl = self.vector_len;

        // Check to see if the data is already in cache?
        if self.cache_frame == frame_idx {
            return self.cache_data[vector_idx];
        }

        if self.cache_frame.wrapping_add(1) == frame_idx {
            return self.cache_data[vector_idx + vl];
        }

        if frame_idx < self.cache_frame {
            // Requested Frame isn't cached, so cache it, and
            // frame_idx+1, and return the decompressed data from
            // frame_idx
            let num_frames = self.num_frames;
            {
                let (rows, _) = self.cache_data.split_at_mut(2 * vl);
                let (row0, row1) = rows.split_at_mut(vl);
                Self::decompress_from_start(&self.data, self.scale, vl, frame_idx, row0);
                if frame_idx != num_frames.wrapping_sub(1) {
                    Self::decompress_continue(
                        &self.data,
                        self.scale,
                        vl,
                        frame_idx,
                        row0,
                        frame_idx.wrapping_add(1),
                        row1,
                    );
                }
                // (row 1 keeps its previous contents when frame_idx is
                // the last frame — the original leaves the stale row,
                // which no reachable query reads)
            }
            self.cache_frame = frame_idx;

            return self.cache_data[vector_idx];
        }

        // Copy last known Cached data down

        if frame_idx == self.cache_frame.wrapping_add(2) {
            // Sliding window
            let new_cache_frame = self.cache_frame.wrapping_add(1);
            {
                let (rows, _) = self.cache_data.split_at_mut(2 * vl);
                let (row0, row1) = rows.split_at_mut(vl);
                row0.copy_from_slice(row1);
                Self::decompress_continue(
                    &self.data,
                    self.scale,
                    vl,
                    new_cache_frame,
                    row0,
                    frame_idx,
                    row1,
                );
            }
            self.cache_frame = new_cache_frame;

            return self.cache_data[vl + vector_idx];
        }

        // Else just use last known frame to decompress forwards
        // (the original passes srcdata = the CACHED ROW-1 values with
        // src_idx = CacheFrame, so the walk re-applies the delta for
        // CacheFrame+1 — transcribed verbatim, pinned by the goldens)
        let cache_frame = self.cache_frame;
        let num_frames = self.num_frames;
        let mut temp = [0f32; 4];
        temp[..vl].copy_from_slice(&self.cache_data[vl..2 * vl]);
        {
            let (rows, _) = self.cache_data.split_at_mut(2 * vl);
            let (row0, row1) = rows.split_at_mut(vl);
            Self::decompress_continue(
                &self.data,
                self.scale,
                vl,
                cache_frame,
                &temp[..vl],
                frame_idx,
                row0,
            );
            if frame_idx != num_frames.wrapping_sub(1) {
                Self::decompress_continue(
                    &self.data,
                    self.scale,
                    vl,
                    frame_idx,
                    row0,
                    frame_idx.wrapping_add(1),
                    row1,
                );
            }
        }
        self.cache_frame = frame_idx;

        self.cache_data[vector_idx]
    }

    /// `AdaptiveDeltaMotionChannelClass::decompress(frame_idx, outdata)`
    /// — the from-scratch walk: every vector index's packet chain from
    /// the base floats up to `frame_idx`.
    ///
    /// A free function over the channel's immutable parts so the cache
    /// branches in [`getframe`](Self::getframe) can write straight into
    /// their rows (the original decompresses into `CacheData` in place;
    /// the port's earlier scratch copies cost the §6 budget).
    ///
    /// The `done` flag is the original's — declared **outside** the
    /// vector-index loop and never reset: once the first vector index
    /// reaches `frame_idx`, every later one stops after its first
    /// 16-frame block. Transcribed verbatim (a multi-block walk therefore
    /// only walks the full frame range for `vi == 0`; the goldens pin
    /// the behaviour).
    fn decompress_from_start(
        data: &[u32],
        scale: f32,
        vector_len: usize,
        frame_idx: u32,
        outdata: &mut [f32],
    ) {
        // Start Over from the beginning
        let filters = filter_table();
        let mut done = false;
        for (vi, out) in outdata.iter_mut().enumerate() {
            // Decompress all the vector indices, since they will
            // probably all be needed
            let mut packet = 4 * vector_len + ADAPTIVE_DELTA_PACKET_SIZE * vi;

            let mut last_value = f32::from_bits(data[vi]);

            let mut frame: u32 = 1;
            while frame <= frame_idx {
                // Frame Loop
                let filter_index = data_byte(data, packet) as usize; // packet header

                packet += 1; // skip to nybble compressed data

                let filter = filters[filter_index] * scale; // decompression filter

                // data is grouped in sets of 16 nybbles
                let mut fi = 0usize;
                while fi < 16 {
                    last_value += nybble_delta(data, packet, fi, filter);

                    if frame == frame_idx {
                        done = true;
                        break;
                    }
                    frame += 1;
                    fi += 1;
                }

                if done {
                    break; // we're at the desired frame
                }

                packet += ADAPTIVE_DELTA_PACKET_SIZE * vector_len - 1; // skip to next packet
            }

            *out = last_value;
        }
    }

    /// `AdaptiveDeltaMotionChannelClass::decompress(src_idx, srcdata,
    /// frame_idx, outdata)` — the continue walk: from the cached frame
    /// (whose data `srcdata` holds) forward to `frame_idx`. A free
    /// function over the channel's immutable parts, like
    /// [`decompress_from_start`].
    ///
    /// Carries the same never-reset `done` flag — only `vi == 0` walks
    /// the full range across blocks; later vector indices stop after
    /// their first block. Transcribed verbatim, pinned by the goldens.
    fn decompress_continue(
        data: &[u32],
        scale: f32,
        vector_len: usize,
        src_idx: u32,
        srcdata: &[f32],
        frame_idx: u32,
        outdata: &mut [f32],
    ) {
        // assert(src_idx < frame_idx) — release no-op
        let src = src_idx.wrapping_add(1);

        let filters = filter_table();
        let mut done = false;
        for (vi, out) in outdata.iter_mut().enumerate() {
            // Decompress all the vector indices, since they will
            // probably all be needed
            let mut packet = 4 * vector_len
                + ADAPTIVE_DELTA_PACKET_SIZE * vi
                + ADAPTIVE_DELTA_PACKET_SIZE * vector_len * ((src.wrapping_sub(1) >> 4) as usize);

            // initial filter index
            let mut fi = (src.wrapping_sub(1) & 0xF) as usize;

            let mut last_value = srcdata[vi];

            let mut frame = src;
            while frame <= frame_idx {
                // Frame Loop
                let filter_index = data_byte(data, packet) as usize; // packet header

                packet += 1; // skip to nybble compressed data

                let filter = filters[filter_index] * scale; // decompression filter

                // data is grouped in sets of 16 nybbles
                while fi < 16 {
                    last_value += nybble_delta(data, packet, fi, filter);

                    if frame == frame_idx {
                        done = true;
                        break;
                    }
                    frame += 1;
                    fi += 1;
                }
                fi = 0;

                if done {
                    break; // we're at the desired frame
                }

                packet += ADAPTIVE_DELTA_PACKET_SIZE * vector_len - 1; // skip to next packet
            }

            *out = last_value;
        }
    }
}

/// Byte `i` of a data stream — the words read in the platform's
/// little-endian storage order, matching the x86/Windows build the
/// transcription pins.
#[inline]
fn data_byte(data: &[u32], i: usize) -> u8 {
    (data[i >> 2] >> ((i & 3) * 8)) as u8
}

/// One nybble of a packet: the signed 4-bit factor (`-8..=7`,
/// sign-extended the original's `factor |= 0xFFFFFFF0` way) times the
/// block's filter. `packet` points at the first nybble byte.
#[inline]
fn nybble_delta(data: &[u32], packet: usize, fi: usize, filter: f32) -> f32 {
    let pi = fi >> 1; // create packet index
    let byte = i32::from(data_byte(data, packet + pi));
    let mut factor = if fi & 1 != 0 { byte >> 4 } else { byte };

    // Sign Extend
    factor &= 0xF;
    if factor & 0x8 != 0 {
        factor |= !0xF;
    }

    // Convert to Floating Point
    (factor as f32) * filter
}

// ---------------------------------------------------------------------------
// the animation
// ---------------------------------------------------------------------------

/// One motion channel slot — the original's `NodeCompressedMotionStruct`
/// union: an X/Y/Z/Q slot holds a time-coded or an adaptive-delta
/// channel depending on the animation's flavor (the same four pointers
/// viewed three ways).
#[derive(Debug, Clone, PartialEq)]
pub enum W3dCompressedChannel {
    /// A channel loaded by the time-coded reader.
    TimeCoded(W3dTimeCodedMotionChannel),
    /// A channel loaded by the adaptive-delta reader.
    AdaptiveDelta(W3dAdaptiveDeltaMotionChannel),
}

impl W3dCompressedChannel {
    /// The pivot this channel drives.
    pub fn pivot_idx(&self) -> u32 {
        match self {
            Self::TimeCoded(c) => c.pivot_idx,
            Self::AdaptiveDelta(c) => c.pivot_idx,
        }
    }

    /// The channel type (the wire `Flags` field).
    pub fn channel_type(&self) -> u32 {
        match self {
            Self::TimeCoded(c) => c.channel_type,
            Self::AdaptiveDelta(c) => c.channel_type,
        }
    }
}

/// The per-bone channel set — `NodeCompressedMotionStruct`. Every slot
/// is optional: bones the animation does not move keep `None` and read
/// as the identity (or always-visible).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct W3dCompressedNodeMotion {
    /// The X translation channel.
    pub x: Option<W3dCompressedChannel>,
    /// The Y translation channel.
    pub y: Option<W3dCompressedChannel>,
    /// The Z translation channel.
    pub z: Option<W3dCompressedChannel>,
    /// The quaternion orientation channel.
    pub q: Option<W3dCompressedChannel>,
    /// The visibility bit channel.
    pub vis: Option<W3dTimeCodedBitChannel>,
}

/// A loaded W3D compressed animation — the state
/// `HCompressedAnimClass::Load_W3D` leaves behind, minus the runtime
/// base-class state (see the module docs).
///
/// The query methods take `&mut self`: the channels cache their last
/// packet index / decompressed frames, exactly the state the original
/// mutates behind its non-const query surface.
#[derive(Debug, Clone, PartialEq)]
pub struct W3dCompressedAnim {
    /// The decoded animation header (the pristine 44 bytes).
    pub header: W3dCompressedAnimHeader,
    /// The composed name (`Get_Name`): `"<hierarchy>.<animation>"` — a
    /// NUL-terminated 32-byte buffer built by `strcpy` + `strlcat` over
    /// the *raw* header bytes.
    pub name: [u8; 2 * W3D_NAME_LEN],
    /// The hierarchy name (`Get_HName`) — the `strcpy` destination: the
    /// 16-byte field, running on through the header's `NumFrames` word
    /// when the field has no NUL (capped at the struct; see the module
    /// docs). Stored as the 24-byte window the member's C string can
    /// reach, NUL-terminated.
    pub hierarchy_name: [u8; 24],
    /// The frame count (the header's `uint32` read as the class's
    /// `int`).
    pub num_frames: u32,
    /// The frame rate — the header's `uint16` **numerically** converted
    /// to `float`.
    pub frame_rate: f32,
    /// The compression flavor (the header's `uint16`).
    pub flavor: u16,
    /// The per-bone channel array — one entry per hierarchy pivot.
    pub node_motion: Vec<W3dCompressedNodeMotion>,
}

impl W3dCompressedAnim {
    /// Parses the payload of a [`W3D_CHUNK_COMPRESSED_ANIMATION`]
    /// container — the bytes between the container's header and its end
    /// — against the hierarchy tree the caller resolves.
    ///
    /// `resolve_base_pose` plays the asset manager's `Get_HTree`: it
    /// receives the hierarchy name exactly as the loader's `strcpy`
    /// leaves it (the 16-byte field, running on through the header's
    /// `NumFrames` word when the field has no NUL) and returns the tree
    /// (whose pivot count the animation adopts) or `None` to fail the
    /// load. The real manager lowercases the name and looks it up in
    /// its hash; the caller mirrors that.
    ///
    /// A short channel read or an unresolvable tree fails the parse
    /// (the original's `goto Error`); a channel for a pivot outside the
    /// tree is discarded and the load continues; an unknown flavor
    /// loads with no motion channels at all (the bit channels still
    /// route).
    pub fn parse<'a>(
        data: &[u8],
        resolve_base_pose: impl FnOnce(&[u8]) -> Option<&'a W3dHtree>,
    ) -> Result<Self, HcanimError> {
        let mut cload = ChunkLoad::new(data);

        // Open the first chunk, it should be the animation header
        if !cload.open_chunk()? {
            return Err(HcanimError::LoadFailed);
        }
        if cload.cur_chunk_id() != W3D_CHUNK_COMPRESSED_ANIMATION_HEADER {
            // "ERROR: Expected Animation Header!"
            return Err(HcanimError::LoadFailed);
        }
        // NOTE: the original reads sizeof(W3dAnimHeaderStruct) — the RAW
        // animation header's size — which is also 44, this struct's exact
        // size; the port reads its own 44 and states the coincidence
        let mut header_bytes = [0u8; COMPRESSED_ANIM_HEADER_SIZE as usize];
        if cload.read(&mut header_bytes)? != COMPRESSED_ANIM_HEADER_SIZE {
            return Err(HcanimError::LoadFailed);
        }
        let header = W3dCompressedAnimHeader::from_bytes(&header_bytes);
        cload.close_chunk()?;

        // strcpy(Name, aheader.HierarchyName) — the source is the raw
        // header bytes from the hierarchy field until the first NUL,
        // running through NumFrames when the field has none (capped at
        // the struct; see the module docs)
        let hierarchy_src = header_cstr(&header_bytes, 20);
        let mut name = [0u8; 2 * W3D_NAME_LEN];
        name[..hierarchy_src.len()].copy_from_slice(hierarchy_src);
        name[hierarchy_src.len()] = 0;
        strlcat(&mut name, b".", 2 * W3D_NAME_LEN);
        let anim_src = header_cstr(&header_bytes, 4);
        strlcat(&mut name, anim_src, 2 * W3D_NAME_LEN);

        // strcpy(HierarchyName, aheader.HierarchyName) — the member holds
        // the source string (the field, running through the NumFrames
        // word when it has no NUL; the class-member reassignments after
        // the copy cover the same bytes for every corpus the discipline
        // allows — the NUL inside the field or the NumFrames word)
        let mut hierarchy_name = [0u8; 24];
        hierarchy_name[..hierarchy_src.len()].copy_from_slice(hierarchy_src);
        hierarchy_name[hierarchy_src.len()] = 0;

        // HTreeClass* base_pose = Get_HTree(HierarchyName) — the lookup
        // key is the strcpy source above (the member holds it); no tree,
        // no animation
        let base_pose = resolve_base_pose(hierarchy_src).ok_or(HcanimError::LoadFailed)?;
        let num_nodes = base_pose.num_pivots();

        let num_frames = header.num_frames;
        let frame_rate = f32::from(header.frame_rate);
        let flavor = header.flavor;

        let mut node_motion = vec![W3dCompressedNodeMotion::default(); num_nodes];

        // Now, read in all of the other chunks (motion channels).
        while cload.open_chunk()? {
            match cload.cur_chunk_id() {
                W3D_CHUNK_COMPRESSED_ANIMATION_CHANNEL => {
                    // the flavor picks the reader; an unknown flavor has
                    // no case at all — the channel chunk is never read
                    // (its bytes are skipped by Close_Chunk)
                    match flavor {
                        ANIM_FLAVOR_TIMECODED => {
                            let channel = match read_tc_motion_channel(&mut cload)? {
                                Ok(channel) => channel,
                                // goto Error — an unreadable channel
                                // fails the whole load
                                Err(()) => return Err(HcanimError::LoadFailed),
                            };
                            // if the channel is referring to a node which
                            // is outside the range, just throw away the
                            // channel ("...Please re-export.")
                            if channel.pivot_idx < num_nodes as u32 {
                                add_tc_channel(&mut node_motion, channel);
                            }
                        }
                        ANIM_FLAVOR_ADAPTIVE_DELTA => {
                            let channel = match read_ad_motion_channel(&mut cload)? {
                                Ok(channel) => channel,
                                Err(()) => return Err(HcanimError::LoadFailed),
                            };
                            if channel.pivot_idx < num_nodes as u32 {
                                add_ad_channel(&mut node_motion, channel);
                            }
                        }
                        _ => {}
                    }
                }
                W3D_CHUNK_COMPRESSED_BIT_CHANNEL => {
                    let channel = match read_tc_bit_channel(&mut cload)? {
                        Ok(channel) => channel,
                        Err(()) => return Err(HcanimError::LoadFailed),
                    };
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
            flavor,
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
    /// NumFrames word when unterminated).
    pub fn hierarchy_name_str(&self) -> &[u8] {
        cstr(&self.hierarchy_name)
    }

    /// The frame count (`Get_Num_Frames`).
    pub fn num_frames(&self) -> u32 {
        self.num_frames
    }

    /// The frame rate (`Get_Frame_Rate`) — the header's `uint16`
    /// numerically converted.
    pub fn frame_rate(&self) -> f32 {
        self.frame_rate
    }

    /// The total time (`Get_Total_Time`) — `(float)NumFrames /
    /// FrameRate`, in seconds.
    pub fn total_time(&self) -> f32 {
        (self.num_frames as i32) as f32 / self.frame_rate
    }

    /// The compression flavor (`Get_Flavor`) — 0 time-coded, 1
    /// adaptive-delta; anything else loaded with the motion channels
    /// unread (see the module docs).
    pub fn flavor(&self) -> u16 {
        self.flavor
    }

    /// The pivot count (`Get_Num_Pivots`) — the hierarchy tree's count.
    pub fn num_pivots(&self) -> usize {
        self.node_motion.len()
    }

    /// `Get_Translation` — the translation of `pividx` at `frame`: the
    /// X/Y/Z channels' values (a time-coded channel writes
    /// `vector_len` floats starting at its slot — a `VectorLen`-2 X
    /// channel legitimately reaches `trans[1]`; an adaptive-delta
    /// channel writes its first component only). A bone with no
    /// translation channels reads as `(0, 0, 0)`; an unknown flavor
    /// keeps the pre-assigned zero vector.
    ///
    /// Panics when `pividx` is out of range (the original indexes
    /// `NodeMotion[pividx]` unchecked) or when a time-coded channel is
    /// wider than the destination (the original overruns the caller's
    /// `Vector3`).
    pub fn get_translation(&mut self, pividx: usize, frame: f32) -> [f32; 3] {
        let flavor = self.flavor;
        let motion = &mut self.node_motion[pividx];

        let mut trans = [0.0f32; 3];

        match flavor {
            ANIM_FLAVOR_TIMECODED => {
                if let Some(W3dCompressedChannel::TimeCoded(x)) = &mut motion.x {
                    x.get_vector(frame, &mut trans[0..]);
                }
                if let Some(W3dCompressedChannel::TimeCoded(y)) = &mut motion.y {
                    y.get_vector(frame, &mut trans[1..]);
                }
                if let Some(W3dCompressedChannel::TimeCoded(z)) = &mut motion.z {
                    z.get_vector(frame, &mut trans[2..]);
                }
            }
            ANIM_FLAVOR_ADAPTIVE_DELTA => {
                if let Some(W3dCompressedChannel::AdaptiveDelta(x)) = &mut motion.x {
                    trans[0] = x.get_vector(frame);
                }
                if let Some(W3dCompressedChannel::AdaptiveDelta(y)) = &mut motion.y {
                    trans[1] = y.get_vector(frame);
                }
                if let Some(W3dCompressedChannel::AdaptiveDelta(z)) = &mut motion.z {
                    trans[2] = z.get_vector(frame);
                }
            }
            _ => {
                // WWASSERT(0) — release no-op: the pre-assigned zero
                // vector stands
            }
        }

        trans
    }

    /// `Get_Orientation` — the orientation quaternion of `pividx` at
    /// `frame`: the Q channel's quaternion (the identity when the slot
    /// is empty). For an unknown flavor the original leaves the
    /// caller's destination untouched; the port returns the identity —
    /// a stated difference (the goldens omit those probes).
    pub fn get_orientation(&mut self, pividx: usize, frame: f32) -> [f32; 4] {
        let flavor = self.flavor;
        let motion = &mut self.node_motion[pividx];

        match flavor {
            ANIM_FLAVOR_TIMECODED => match &mut motion.q {
                Some(W3dCompressedChannel::TimeCoded(q)) => q.get_quat_vector(frame),
                _ => [0.0, 0.0, 0.0, 1.0],
            },
            ANIM_FLAVOR_ADAPTIVE_DELTA => match &mut motion.q {
                Some(W3dCompressedChannel::AdaptiveDelta(q)) => q.get_quat_vector(frame),
                _ => [0.0, 0.0, 0.0, 1.0],
            },
            _ => [0.0, 0.0, 0.0, 1.0],
        }
    }

    /// `Get_Transform` — the full local transform of `pividx` at
    /// `frame`, the matrix `HTreeClass::Anim_Update` stores into the
    /// tree: the rotation from `Build_Matrix3D` of the Q channel's
    /// quaternion (or the identity), then the X/Y/Z channels written
    /// at the matrix's translation column — a time-coded channel of
    /// `vector_len` 2 on X reaches `m[4]` (in bounds, clobbering a
    /// rotation element); anything wider than the column's remaining
    /// slots panics (the original writes past the `Matrix3D`).
    pub fn get_transform(&mut self, pividx: usize, frame: f32) -> [f32; 12] {
        let flavor = self.flavor;
        let motion = &mut self.node_motion[pividx];

        let mut mtx;
        match flavor {
            ANIM_FLAVOR_TIMECODED => {
                match &mut motion.q {
                    Some(W3dCompressedChannel::TimeCoded(q)) => {
                        let qv = q.get_quat_vector(frame);
                        mtx = build_matrix3d(&qv);
                    }
                    _ => mtx = IDENTITY_MTX,
                }
                if let Some(W3dCompressedChannel::TimeCoded(x)) = &mut motion.x {
                    x.get_vector(frame, &mut mtx[3..]);
                }
                if let Some(W3dCompressedChannel::TimeCoded(y)) = &mut motion.y {
                    y.get_vector(frame, &mut mtx[7..]);
                }
                if let Some(W3dCompressedChannel::TimeCoded(z)) = &mut motion.z {
                    z.get_vector(frame, &mut mtx[11..]);
                }
            }
            ANIM_FLAVOR_ADAPTIVE_DELTA => {
                match &mut motion.q {
                    Some(W3dCompressedChannel::AdaptiveDelta(q)) => {
                        let qv = q.get_quat_vector(frame);
                        mtx = build_matrix3d(&qv);
                    }
                    _ => mtx = IDENTITY_MTX,
                }
                if let Some(W3dCompressedChannel::AdaptiveDelta(x)) = &mut motion.x {
                    mtx[3] = x.get_vector(frame);
                }
                if let Some(W3dCompressedChannel::AdaptiveDelta(y)) = &mut motion.y {
                    mtx[7] = y.get_vector(frame);
                }
                if let Some(W3dCompressedChannel::AdaptiveDelta(z)) = &mut motion.z {
                    mtx[11] = z.get_vector(frame);
                }
            }
            _ => {
                // WWASSERT(0) — release no-op: the caller's destination
                // is left untouched in the original; the port states the
                // identity (see get_orientation)
                mtx = IDENTITY_MTX;
            }
        }

        mtx
    }

    /// `Get_Visibility` — the visibility of `pividx` at `frame`: the
    /// bit channel's bit (`(int)frame`), defaulting to visible when
    /// the channel is absent.
    pub fn get_visibility(&mut self, pividx: usize, frame: f32) -> bool {
        match &mut self.node_motion[pividx].vis {
            Some(vis) => vis.get_bit(frame as i32) == 1,
            // default to always visible...
            None => true,
        }
    }

    /// `Is_Node_Motion_Present` — whether *any* channel drives `pividx`
    /// (the original reads the union's `vd` view — the same slots).
    pub fn is_node_motion_present(&self, pividx: usize) -> bool {
        let motion = &self.node_motion[pividx];
        motion.x.is_some()
            || motion.y.is_some()
            || motion.z.is_some()
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
}

/// `Matrix3D::Make_Identity` — the 12-float identity the transform
/// queries pre-assign.
const IDENTITY_MTX: [f32; 12] = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0];

// ---------------------------------------------------------------------------
// read_channel / read_bit_channel / add_channel / add_bit_channel
// ---------------------------------------------------------------------------

/// `TimeCodedMotionChannelClass::Load_W3D` — `Ok(Ok(channel))` on
/// success, `Ok(Err(()))` on the short-read failure that fails the
/// load, `Err` for a container error.
fn read_tc_motion_channel(
    cload: &mut ChunkLoad<'_>,
) -> Result<Result<W3dTimeCodedMotionChannel, ()>, crate::chunk::ChunkError> {
    let size = cload.cur_chunk_length();
    let datasize = size.wrapping_sub(TIMECODED_ANIM_CHANNEL_STRUCT_SIZE);
    let num_ints = datasize / 4 + 1;

    let mut bytes = [0u8; TIMECODED_ANIM_CHANNEL_STRUCT_SIZE as usize];
    if cload.read(&mut bytes)? != TIMECODED_ANIM_CHANNEL_STRUCT_SIZE {
        return Ok(Err(()));
    }
    let num_time_codes = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
    let pivot_idx = u32::from(u16::from_le_bytes(bytes[4..6].try_into().unwrap()));
    let vector_len = bytes[6] as usize;
    let channel_type = u32::from(bytes[7]);
    let data0 = u32::from_le_bytes(bytes[8..12].try_into().unwrap());

    let packet_size = vector_len as u32 + 1;
    let last_time_code_idx = num_time_codes.wrapping_sub(1).wrapping_mul(packet_size);

    // Data = new uint32[numInts]; Data[0] = chan.Data[0]; Read(&Data[1],
    // datasize) — the corpus discipline keeps the trailing payload whole
    // u32 words, so the read fills the array exactly; a remainder would
    // overrun the original's heap allocation by that many bytes (the
    // port reads the bytes and drops the tail instead)
    let mut data = vec![0u32; num_ints as usize];
    data[0] = data0;
    let mut buf = [0u8; 512];
    let mut remaining = datasize;
    let mut next = 1usize;
    while remaining > 0 {
        let piece = (remaining as usize).min(buf.len());
        if cload.read(&mut buf[..piece])? != piece as u32 {
            return Ok(Err(()));
        }
        for word in buf[..piece].as_chunks::<4>().0 {
            data[next] = u32::from_le_bytes(*word);
            next += 1;
        }
        remaining -= piece as u32;
    }

    Ok(Ok(W3dTimeCodedMotionChannel {
        pivot_idx,
        channel_type,
        vector_len,
        packet_size,
        num_time_codes,
        last_time_code_idx,
        data,
        cached_idx: 0,
    }))
}

/// `TimeCodedBitChannelClass::Load_W3D` — same shape as
/// [`read_tc_motion_channel`].
fn read_tc_bit_channel(
    cload: &mut ChunkLoad<'_>,
) -> Result<Result<W3dTimeCodedBitChannel, ()>, crate::chunk::ChunkError> {
    let mut bytes = [0u8; TIMECODED_BIT_CHANNEL_STRUCT_SIZE as usize];
    if cload.read(&mut bytes)? != TIMECODED_BIT_CHANNEL_STRUCT_SIZE {
        return Ok(Err(()));
    }
    let num_time_codes = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
    let pivot_idx = u32::from(u16::from_le_bytes(bytes[4..6].try_into().unwrap()));
    let channel_type = u32::from(bytes[6]);
    let default_val = i32::from(bytes[7]);
    let data0 = u32::from_le_bytes(bytes[8..12].try_into().unwrap());

    // uint32 bytesleft = (NumTimeCodes - 1) * 4 — a zero count wraps to
    // ~4 GiB and the read fails in both (the original first writes one
    // word past its zero-size allocation; the port skips the collateral
    // and fails the channel through the same read)
    if num_time_codes == 0 {
        return Ok(Err(()));
    }
    let bytesleft = num_time_codes.wrapping_sub(1) * 4;

    let mut bits = vec![0u32; num_time_codes as usize];
    bits[0] = data0;

    let mut buf = [0u8; 512];
    let mut remaining = bytesleft;
    let mut next = 1usize;
    while remaining > 0 {
        let piece = (remaining as usize).min(buf.len());
        if cload.read(&mut buf[..piece])? != piece as u32 {
            return Ok(Err(()));
        }
        for word in buf[..piece].as_chunks::<4>().0 {
            bits[next] = u32::from_le_bytes(*word);
            next += 1;
        }
        remaining -= piece as u32;
    }

    Ok(Ok(W3dTimeCodedBitChannel {
        pivot_idx,
        channel_type,
        default_val,
        num_time_codes,
        bits,
        cached_idx: 0,
    }))
}

/// `AdaptiveDeltaMotionChannelClass::Load_W3D` — same shape, over the
/// 16-byte struct.
fn read_ad_motion_channel(
    cload: &mut ChunkLoad<'_>,
) -> Result<Result<W3dAdaptiveDeltaMotionChannel, ()>, crate::chunk::ChunkError> {
    let size = cload.cur_chunk_length();
    let datasize = size.wrapping_sub(ADAPTIVE_DELTA_CHANNEL_STRUCT_SIZE);
    let num_ints = datasize / 4 + 1;

    let mut bytes = [0u8; ADAPTIVE_DELTA_CHANNEL_STRUCT_SIZE as usize];
    if cload.read(&mut bytes)? != ADAPTIVE_DELTA_CHANNEL_STRUCT_SIZE {
        return Ok(Err(()));
    }
    let num_frames = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
    let pivot_idx = u32::from(u16::from_le_bytes(bytes[4..6].try_into().unwrap()));
    let vector_len = bytes[6] as usize;
    let channel_type = u32::from(bytes[7]);
    let scale = f32::from_bits(u32::from_le_bytes(bytes[8..12].try_into().unwrap()));
    let data0 = u32::from_le_bytes(bytes[12..16].try_into().unwrap());

    // Corpus discipline: the exporter pads the 9-byte packet blocks up
    // to a whole u32 word, so the read fills the array exactly (the pad
    // bytes land in the tail, never read by the walk); unpadded data
    // would overrun the original's numInts-sized allocation — the port
    // keeps the whole words and drops any tail instead
    let mut data = vec![0u32; num_ints as usize];
    data[0] = data0;
    let mut buf = [0u8; 512];
    let mut remaining = datasize;
    let mut next = 1usize;
    while remaining > 0 {
        let piece = (remaining as usize).min(buf.len());
        if cload.read(&mut buf[..piece])? != piece as u32 {
            return Ok(Err(()));
        }
        for word in buf[..piece].as_chunks::<4>().0 {
            data[next] = u32::from_le_bytes(*word);
            next += 1;
        }
        remaining -= piece as u32;
    }

    Ok(Ok(W3dAdaptiveDeltaMotionChannel {
        pivot_idx,
        channel_type,
        vector_len,
        num_frames,
        scale,
        data,
        cache_frame: 0x7FFF_FFFF, // a big number, so we know its not valid
        cache_data: [0.0; 8],
    }))
}

/// `HCompressedAnimClass::add_channel` (time-coded) — routes the
/// channel into its slot (overwriting any previous channel there; the
/// original leaks the old one). The TIMECODED_/ADAPTIVEDELTA_-type
/// values (7..14) have no case and are stored nowhere (the original
/// leaks them too).
fn add_tc_channel(node_motion: &mut [W3dCompressedNodeMotion], channel: W3dTimeCodedMotionChannel) {
    let idx = channel.pivot_idx as usize;
    let node = &mut node_motion[idx];
    match channel.channel_type {
        ANIM_CHANNEL_X => node.x = Some(W3dCompressedChannel::TimeCoded(channel)),
        ANIM_CHANNEL_Y => node.y = Some(W3dCompressedChannel::TimeCoded(channel)),
        ANIM_CHANNEL_Z => node.z = Some(W3dCompressedChannel::TimeCoded(channel)),
        ANIM_CHANNEL_Q => node.q = Some(W3dCompressedChannel::TimeCoded(channel)),
        _ => {}
    }
}

/// `HCompressedAnimClass::add_channel` (adaptive-delta) — same routing.
fn add_ad_channel(
    node_motion: &mut [W3dCompressedNodeMotion],
    channel: W3dAdaptiveDeltaMotionChannel,
) {
    let idx = channel.pivot_idx as usize;
    let node = &mut node_motion[idx];
    match channel.channel_type {
        ANIM_CHANNEL_X => node.x = Some(W3dCompressedChannel::AdaptiveDelta(channel)),
        ANIM_CHANNEL_Y => node.y = Some(W3dCompressedChannel::AdaptiveDelta(channel)),
        ANIM_CHANNEL_Z => node.z = Some(W3dCompressedChannel::AdaptiveDelta(channel)),
        ANIM_CHANNEL_Q => node.q = Some(W3dCompressedChannel::AdaptiveDelta(channel)),
        _ => {}
    }
}

/// `HCompressedAnimClass::add_bit_channel` — the visibility slot.
fn add_bit_channel(node_motion: &mut [W3dCompressedNodeMotion], channel: W3dTimeCodedBitChannel) {
    let idx = channel.pivot_idx as usize;
    if channel.channel_type == BIT_CHANNEL_VIS {
        node_motion[idx].vis = Some(channel);
    }
}

// ---------------------------------------------------------------------------
// the WWMath transcription (wwmath.h / wwmath.cpp / quat.cpp / quat.h)
// ---------------------------------------------------------------------------
//
// The same policy the raw-anim port applies: the crate stays
// dependency-free, so the exact formulas the animation queries drive are
// transcribed privately here (the Phase 1 rzh-math crate holds the
// general surface; this copy is the slice the compressed anim needs,
// pinned by the same goldens).
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
/// under `USE_DETERMINISTIC_MATH`, and what the sin table is built from
/// (and `Sinf_Legacy`, whose deterministic branch is the same call).
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

/// `WWMath::Float_To_Int_Floor` (wwmath.h) — the bit-trick floor the
/// fast-table interpolation indexes with. The shifts keep the x86
/// semantics the goldens pin (`wrapping_shl`/`wrapping_shr` mask the
/// count exactly like the hardware the transcription runs on; the
/// original's shifts are undefined for the degenerate exponents but
/// x86-defined in value).
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
    // if (value < 0 && value==(int)(value)) value++
    // (`!` in the original is a LOGICAL not (0 or 1), not a bitwise not.)
    let low_bits_zero = i32::from((mantissa << 8) & imask == 0);
    let adjusted = low_bits_zero & (expsign ^ ((a - 1) >> 31)) & sign;
    ((r & expsign) ^ sign).wrapping_add(adjusted)
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
/// quaternion, the same mixed-precision builder the hierarchy and
/// raw-anim ports transcribe (every term promotes through `f64` except
/// the `[1][1]` diagonal's `2.0f`).
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

/// `WWMath::Lerp` per component: `a + (b - a) * t`.
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

// ---------------------------------------------------------------------------
// the adaptive-delta filter table (motchan.cpp)
// ---------------------------------------------------------------------------

/// `FILTER_TABLE_SIZE` (motchan.cpp): 256 entries — sixteen literal
/// scales plus the 240 the adaptive-delta constructor generates.
const FILTER_TABLE_SIZE: usize = 256;
/// `FILTER_TABLE_GEN_START`: the generated half starts at index 16.
const FILTER_TABLE_GEN_START: usize = 16;
/// `FILTER_TABLE_GEN_SIZE`: 240 generated entries.
const FILTER_TABLE_GEN_SIZE: usize = FILTER_TABLE_SIZE - FILTER_TABLE_GEN_START;

/// `filtertable[256]` — the sixteen literal scales followed by the 240
/// generated entries `1 - Sinf_Legacy(DEG_TO_RAD(90 * i/240))`, computed
/// on first use exactly where the original's first channel constructor
/// computes them (`DEG_TO_RAD` promotes through `f64` with the f32
/// `WWMATH_PI` literal; `Sinf_Legacy` is `gm_sinf` under
/// `USE_DETERMINISTIC_MATH`).
fn filter_table() -> &'static [f32; FILTER_TABLE_SIZE] {
    static TABLE: OnceLock<[f32; FILTER_TABLE_SIZE]> = OnceLock::new();
    TABLE.get_or_init(|| {
        const LITERALS: [f32; FILTER_TABLE_GEN_START] = [
            0.000_000_01,
            0.000_000_1,
            0.000_001,
            0.000_01,
            0.000_1,
            0.001,
            0.01,
            0.1,
            1.0,
            10.0,
            100.0,
            1000.0,
            10_000.0,
            100_000.0,
            1_000_000.0,
            10_000_000.0,
        ];
        let mut t = [0f32; FILTER_TABLE_SIZE];
        t[..FILTER_TABLE_GEN_START].copy_from_slice(&LITERALS);
        for i in 0..FILTER_TABLE_GEN_SIZE {
            // float ratio = i; ratio /= ((float)FILTER_TABLE_GEN_SIZE);
            let ratio = i as f32 / FILTER_TABLE_GEN_SIZE as f32;
            // DEG_TO_RAD(x) = ((double)x * WWMATH_PI / 180.0) — the f32
            // literal promoted, the product and quotient in f64, the
            // argument narrowed back to f32 for the call
            let arg = (f64::from(90.0_f32 * ratio) * f64::from(WWMATH_PI) / 180.0) as f32;
            t[i + FILTER_TABLE_GEN_START] = 1.0_f32 - gm_sinf(arg);
        }
        t
    })
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

    /// The 44-byte compressed header.
    fn header_bytes(
        version: u32,
        name: &[u8; 16],
        hierarchy: &[u8; 16],
        num_frames: u32,
        frame_rate: u16,
        flavor: u16,
    ) -> [u8; 44] {
        let mut b = [0u8; 44];
        b[0..4].copy_from_slice(&version.to_le_bytes());
        b[4..20].copy_from_slice(name);
        b[20..36].copy_from_slice(hierarchy);
        b[36..40].copy_from_slice(&num_frames.to_le_bytes());
        b[40..42].copy_from_slice(&frame_rate.to_le_bytes());
        b[42..44].copy_from_slice(&flavor.to_le_bytes());
        b
    }

    /// A payload builder: header plus raw child chunks.
    struct AnimBuild {
        bytes: Vec<u8>,
    }

    impl AnimBuild {
        fn new(header: &[u8; 44]) -> Self {
            let mut bytes = Vec::new();
            bytes.extend_from_slice(&W3D_CHUNK_COMPRESSED_ANIMATION_HEADER.to_le_bytes());
            bytes.extend_from_slice(&COMPRESSED_ANIM_HEADER_SIZE.to_le_bytes());
            bytes.extend_from_slice(header);
            Self { bytes }
        }

        /// A time-coded motion channel: the 8 field bytes + the packet
        /// words.
        fn tc_channel(
            &mut self,
            num_time_codes: u32,
            pivot: u16,
            vector_len: u8,
            flags: u8,
            words: &[u32],
        ) {
            self.begin(W3D_CHUNK_COMPRESSED_ANIMATION_CHANNEL);
            let mut body = Vec::with_capacity(8 + words.len() * 4);
            body.extend_from_slice(&num_time_codes.to_le_bytes());
            body.extend_from_slice(&pivot.to_le_bytes());
            body.push(vector_len);
            body.push(flags);
            for w in words {
                body.extend_from_slice(&w.to_le_bytes());
            }
            self.bytes.extend_from_slice(&body);
            self.end(body.len());
        }

        /// A time-coded bit channel: the 8 field bytes + the time-code
        /// words.
        fn tc_bit_channel(
            &mut self,
            num_time_codes: u32,
            pivot: u16,
            flags: u8,
            default_val: u8,
            words: &[u32],
        ) {
            self.begin(W3D_CHUNK_COMPRESSED_BIT_CHANNEL);
            let mut body = Vec::with_capacity(8 + words.len() * 4);
            body.extend_from_slice(&num_time_codes.to_le_bytes());
            body.extend_from_slice(&pivot.to_le_bytes());
            body.push(flags);
            body.push(default_val);
            for w in words {
                body.extend_from_slice(&w.to_le_bytes());
            }
            self.bytes.extend_from_slice(&body);
            self.end(body.len());
        }

        /// An adaptive-delta motion channel: the 12 field bytes + the
        /// base floats + the packet bytes + the exporter's word pad.
        #[allow(clippy::too_many_arguments)]
        fn ad_channel(
            &mut self,
            num_frames: u32,
            pivot: u16,
            vector_len: u8,
            flags: u8,
            scale: f32,
            base: &[f32],
            packets: &[u8],
        ) {
            self.begin(W3D_CHUNK_COMPRESSED_ANIMATION_CHANNEL);
            let mut body = Vec::new();
            body.extend_from_slice(&num_frames.to_le_bytes());
            body.extend_from_slice(&pivot.to_le_bytes());
            body.push(vector_len);
            body.push(flags);
            body.extend_from_slice(&scale.to_bits().to_le_bytes());
            for v in base {
                body.extend_from_slice(&v.to_bits().to_le_bytes());
            }
            body.extend_from_slice(packets);
            while body.len() % 4 != 0 {
                body.push(0xAB); // the exporter's word alignment pad
            }
            self.bytes.extend_from_slice(&body);
            // the payload is the 12 field bytes + base + packets + pad
            // (the first base float doubles as the struct's Data[0])
            self.end(12 + body.len() - 12);
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

    /// Words from floats.
    fn w(v: f32) -> u32 {
        v.to_bits()
    }

    /// Deterministic AD packet bytes (the generator's formula).
    fn ad_packet(vi: usize, block: usize) -> [u8; 9] {
        let mut p = [0u8; 9];
        p[0] = ((vi.wrapping_mul(37) + block.wrapping_mul(11)) & 0xFF) as u8;
        for k in 0..8 {
            p[1 + k] =
                ((vi.wrapping_mul(53) + block.wrapping_mul(29) + k.wrapping_mul(17)) & 0xFF) as u8;
        }
        p
    }

    fn ad_packets(vector_len: usize, blocks: usize) -> Vec<u8> {
        let mut out = Vec::new();
        for b in 0..blocks {
            for vi in 0..vector_len {
                out.extend_from_slice(&ad_packet(vi, b));
            }
        }
        out
    }

    const V40: u32 = crate::w3d_hanim::w3d_make_version(4, 0);

    #[test]
    fn header_decodes_every_field() {
        let mut b = [0u8; 44];
        b[0..4].copy_from_slice(&0x0002_0001u32.to_le_bytes());
        b[4..20].copy_from_slice(b"AVTank.Run\0\0\0\0\0\0");
        b[20..36].copy_from_slice(b"AVTank\0\0\0\0\0\0\0\0\0\0");
        b[36..40].copy_from_slice(&37u32.to_le_bytes());
        b[40..42].copy_from_slice(&30u16.to_le_bytes());
        b[42..44].copy_from_slice(&1u16.to_le_bytes());
        let h = W3dCompressedAnimHeader::from_bytes(&b);
        assert_eq!(h.version, 0x0002_0001);
        assert_eq!(h.name_str(), b"AVTank.Run");
        assert_eq!(h.hierarchy_name_str(), b"AVTank");
        assert_eq!(h.num_frames, 37);
        assert_eq!(h.frame_rate, 30);
        assert_eq!(h.flavor, 1);
    }

    #[test]
    fn timecoded_animation_loads_and_routes_channels() {
        let trees = vec![(b"tank".to_vec(), tree(6))];
        let mut b = AnimBuild::new(&header_bytes(
            V40,
            &name16("Run"),
            &name16("tank"),
            8,
            30,
            ANIM_FLAVOR_TIMECODED,
        ));
        // X/Y/Z on pivot 1
        for (ty, vals) in [
            (ANIM_CHANNEL_X as u8, [0.5f32, 1.5, 2.5, 3.5]),
            (ANIM_CHANNEL_Y as u8, [1.0f32, 1.0, 1.0, 1.0]),
            (ANIM_CHANNEL_Z as u8, [-0.5f32, -1.5, -2.5, -3.5]),
        ] {
            b.tc_channel(
                4,
                1,
                1,
                ty,
                &[0, w(vals[0]), 2, w(vals[1]), 4, w(vals[2]), 6, w(vals[3])],
            );
        }
        // Q on pivot 2
        let s45: f32 = std::f32::consts::FRAC_1_SQRT_2;
        b.tc_channel(
            2,
            2,
            4,
            ANIM_CHANNEL_Q as u8,
            &[
                0,
                w(0.5),
                w(0.5),
                w(0.5),
                w(0.5),
                3,
                w(s45),
                w(0.0),
                w(0.0),
                w(s45),
            ],
        );
        // Vis on pivot 3
        b.tc_bit_channel(
            4,
            3,
            BIT_CHANNEL_VIS as u8,
            1,
            &[0, 4 | (1 << 31), 8, 12 | (1 << 31)],
        );
        // an euler channel on pivot 4 (stored, never sampled: no case)
        b.tc_channel(2, 4, 1, 3, &[0, w(0.25), 2, w(0.75)]);

        let mut anim =
            W3dCompressedAnim::parse(&b.bytes, resolve(&trees)).expect("the animation loads");
        assert_eq!(anim.name_str(), b"tank.Run");
        assert_eq!(anim.hierarchy_name_str(), b"tank");
        assert_eq!(anim.num_frames(), 8);
        assert_eq!(anim.frame_rate(), 30.0);
        assert_eq!(anim.total_time(), 8.0 / 30.0);
        assert_eq!(anim.flavor(), ANIM_FLAVOR_TIMECODED);
        assert_eq!(anim.num_pivots(), 6);

        // the slot routing
        assert!(
            anim.has_x_translation(1) && anim.has_y_translation(1) && anim.has_z_translation(1)
        );
        assert!(anim.has_rotation(2));
        assert!(anim.has_visibility(3));
        // the euler channel has no add_channel case — not present
        assert!(!anim.is_node_motion_present(4));
        assert!(!anim.is_node_motion_present(0));
        assert!(!anim.is_node_motion_present(5));
        assert!(!anim.has_rotation(1));
        let Some(W3dCompressedChannel::TimeCoded(x)) = &anim.node_motion[1].x else {
            panic!("the X channel is time-coded");
        };
        assert_eq!(x.data, vec![0, w(0.5), 2, w(1.5), 4, w(2.5), 6, w(3.5)]);
        assert_eq!(anim.node_motion[3].vis.as_ref().unwrap().default_val, 1);

        // frame 0.5: lerp between the packets at tc 0 and tc 2 (ratio 0.25)
        assert_eq!(anim.get_translation(1, 0.5), [0.75, 1.0, -0.75]);
        // the Q channel at frame 3.5: the last packet branch (3.5 >= tc 3)
        let q = anim.get_orientation(2, 3.5);
        assert_eq!(q, [s45, 0.0, 0.0, s45]);
    }

    #[test]
    fn timecoded_binary_movement_returns_verbatim() {
        let trees = vec![(b"t".to_vec(), tree(1))];
        let mut b = AnimBuild::new(&header_bytes(
            V40,
            &name16("Bin"),
            &name16("t"),
            8,
            30,
            ANIM_FLAVOR_TIMECODED,
        ));
        // the packet at tc 3 carries the flag: probes in [0, 3) return
        // packet 0's value verbatim
        b.tc_channel(
            3,
            0,
            1,
            ANIM_CHANNEL_X as u8,
            &[0, w(1.0), 3 | (1 << 31), w(9.0), 6, w(2.0)],
        );
        let mut anim = W3dCompressedAnim::parse(&b.bytes, resolve(&trees)).unwrap();
        assert_eq!(anim.get_translation(0, 1.5), [1.0, 0.0, 0.0]);
        assert_eq!(anim.get_translation(0, 2.99), [1.0, 0.0, 0.0]);
        // at tc >= 3 the pair (3, 6) interpolates
        let t = anim.get_translation(0, 4.5);
        assert!((t[0] - 5.5).abs() < 1e-6, "lerp(9, 2, 0.25) = {t:?}");
    }

    #[test]
    fn timecoded_last_packet_branch_and_big_timecodes() {
        let trees = vec![(b"t".to_vec(), tree(1))];
        // probes at or past the last time code return the final packet
        let mut b = AnimBuild::new(&header_bytes(
            V40,
            &name16("Last"),
            &name16("t"),
            8,
            30,
            ANIM_FLAVOR_TIMECODED,
        ));
        b.tc_channel(
            3,
            0,
            1,
            ANIM_CHANNEL_X as u8,
            &[0, w(1.0), 2, w(2.0), 4, w(4.0)],
        );
        let mut anim = W3dCompressedAnim::parse(&b.bytes, resolve(&trees)).unwrap();
        assert_eq!(anim.get_translation(0, 7.75), [4.0, 0.0, 0.0]);
        assert_eq!(anim.get_translation(0, 100.0), [4.0, 0.0, 0.0]);

        // the numeric time-code conversion: 0x1000001 (2^24+1) converts
        // to f32 as 2^24 exactly, so the ratio between it and 2^24+8 is
        // computed against the ROUNDED values
        let mut b = AnimBuild::new(&header_bytes(
            V40,
            &name16("Big"),
            &name16("t"),
            0x0100_0010,
            30,
            ANIM_FLAVOR_TIMECODED,
        ));
        b.tc_channel(
            3,
            0,
            1,
            ANIM_CHANNEL_X as u8,
            &[
                0,
                w(1.0),
                0x0100_0001 | (1 << 31),
                w(2.0),
                0x0100_0008,
                w(3.0),
            ],
        );
        let mut anim = W3dCompressedAnim::parse(&b.bytes, resolve(&trees)).unwrap();
        // below the flagged packet: verbatim
        assert_eq!(anim.get_translation(0, 7.75), [1.0, 0.0, 0.0]);
        // between the flagged packet and the last: the ratio path with
        // time1 = f32(0x1000001) = 2^24 and time2 = 2^24+8 — a frame at
        // 2^24+4 (mantissa 2 at that magnitude) sits at ratio 4/8
        let t = anim.get_translation(0, f32::from_bits(0x4b80_0002));
        assert!(
            (t[0] - 2.5).abs() < 1e-6,
            "lerp against rounded times: {t:?}"
        );
    }

    #[test]
    fn timecoded_binary_search_and_rewind() {
        let trees = vec![(b"t".to_vec(), tree(1))];
        let mut b = AnimBuild::new(&header_bytes(
            V40,
            &name16("Seek"),
            &name16("t"),
            4,
            30,
            ANIM_FLAVOR_TIMECODED,
        ));
        b.tc_channel(
            4,
            0,
            1,
            ANIM_CHANNEL_X as u8,
            &[0, w(0.0), 2, w(2.0), 4, w(4.0), 8, w(8.0)],
        );
        let mut anim = W3dCompressedAnim::parse(&b.bytes, resolve(&trees)).unwrap();
        // 7.75 jumps past the cached packet (binary search forward) and
        // lands between tc 4 and tc 8: lerp(4, 8, 0.9375) = 7.75
        assert_eq!(anim.get_translation(0, 7.75), [7.75, 0.0, 0.0]);
        // 3.0 rewinds below the cached packet's time code (binary search
        // from below the cache)
        let t = anim.get_translation(0, 3.0);
        assert!((t[0] - 3.0).abs() < 1e-6, "lerp(2, 4, 0.5) = {t:?}");
    }

    #[test]
    fn timecoded_wide_x_channel_reaches_the_second_slot() {
        let trees = vec![(b"t".to_vec(), tree(1))];
        let mut b = AnimBuild::new(&header_bytes(
            V40,
            &name16("Wide"),
            &name16("t"),
            8,
            30,
            ANIM_FLAVOR_TIMECODED,
        ));
        // VectorLen 2: Get_Vector writes two floats — trans[1] receives
        // the second word (no Y channel overwrites it) and the transform
        // clobbers m[4]
        b.tc_channel(
            2,
            0,
            2,
            ANIM_CHANNEL_X as u8,
            &[0, w(1.0), w(0.5), 4, w(3.0), w(1.5)],
        );
        let mut anim = W3dCompressedAnim::parse(&b.bytes, resolve(&trees)).unwrap();
        // frame 0.0: packet 0 verbatim at the last-packet? no — packets
        // at tc 0 and 4: ratio 0 → the pair (packet0, packet1) lerped by
        // (0-0)/4 = 0 → packet 0's words
        assert_eq!(anim.get_translation(0, 0.0), [1.0, 0.5, 0.0]);
        // frame 2: ratio 0.5 → lerped both words
        let t = anim.get_translation(0, 2.0);
        assert!(
            (t[0] - 2.0).abs() < 1e-6 && (t[1] - 1.0).abs() < 1e-6,
            "{t:?}"
        );
        // the transform: m[4] receives the second word (frame 0)
        let m = anim.get_transform(0, 0.0);
        assert_eq!((m[3], m[4]), (1.0, 0.5));
    }

    #[test]
    fn timecoded_type_values_seven_to_ten_never_route() {
        let trees = vec![(b"t".to_vec(), tree(1))];
        let mut b = AnimBuild::new(&header_bytes(
            V40,
            &name16("Types"),
            &name16("t"),
            4,
            30,
            ANIM_FLAVOR_TIMECODED,
        ));
        for ty in 7u8..=10 {
            b.tc_channel(2, 0, 1, ty, &[0, w(1.0), 2, w(2.0)]);
        }
        b.tc_channel(2, 0, 1, ANIM_CHANNEL_Y as u8, &[0, w(5.0), 2, w(6.0)]);
        let anim = W3dCompressedAnim::parse(&b.bytes, resolve(&trees)).unwrap();
        assert!(!anim.has_x_translation(0));
        assert!(anim.has_y_translation(0));
        assert!(!anim.is_node_motion_present(0) || anim.has_y_translation(0));
    }

    #[test]
    fn adaptive_delta_decompresses_and_slides() {
        let trees = vec![(b"t".to_vec(), tree(1))];
        let mut b = AnimBuild::new(&header_bytes(
            V40,
            &name16("Run"),
            &name16("t"),
            9,
            30,
            ANIM_FLAVOR_ADAPTIVE_DELTA,
        ));
        // base 1.0; one block of packets with filter index 0 (the tiny
        // literal) — the deltas are nybble * 1e-8 * scale
        b.ad_channel(
            9,
            0,
            1,
            ANIM_CHANNEL_X as u8,
            0.01,
            &[1.0],
            &ad_packets(1, 1),
        );
        let mut anim = W3dCompressedAnim::parse(&b.bytes, resolve(&trees)).unwrap();

        // frame 0: the base value
        assert!((anim.get_translation(0, 0.0)[0] - 1.0).abs() < 1e-6);
        // frame 4.5: the pair (4, 5) lerped at 0.5 — both within the
        // tiny-delta block, so still ~1.0 (the slide path)
        assert!((anim.get_translation(0, 4.5)[0] - 1.0).abs() < 1e-4);
        // the clamped probes: past NumFrames the value is the last frame's
        assert!((anim.get_translation(0, 100.0)[0] - 1.0).abs() < 1e-4);
    }

    #[test]
    fn adaptive_delta_far_jump_pins_the_cache_continue_off_by_one() {
        let trees = vec![(b"t".to_vec(), tree(1))];
        let mut b = AnimBuild::new(&header_bytes(
            V40,
            &name16("Run"),
            &name16("t"),
            40,
            30,
            ANIM_FLAVOR_ADAPTIVE_DELTA,
        ));
        // three blocks, filters 0/11/22 — block 1's filtertable[11] is the
        // literal 1000.0, so its deltas (scale 0.01) move the value
        // substantially; the walk between blocks is observable
        b.ad_channel(
            40,
            0,
            1,
            ANIM_CHANNEL_X as u8,
            0.01,
            &[0.5],
            &ad_packets(1, 3),
        );
        let mut anim = W3dCompressedAnim::parse(&b.bytes, resolve(&trees)).unwrap();

        // sequential probes stay near the base through block 0
        assert!((anim.get_translation(0, 3.5)[0] - 0.5).abs() < 1e-4);

        // the far jump (7.75 after the cache holds (3, 4)) takes the
        // cache-continue branch, whose srcdata is the CACHED ROW-1 with
        // src_idx = CacheFrame — the walk re-applies the delta for
        // CacheFrame+1. Within block 0 the deltas are ~1e-10, so the
        // off-by-one is invisible here; frame 39 (past block 1's drift)
        // proves the walk crossed the blocks
        let t39 = anim.get_translation(0, 39.0)[0];
        assert!(t39.abs() > 10.0, "block 1's filter is 1000: {t39}");
        // and the value is NOT the true frame-39 decompression: the
        // continue from (7, 8) re-applies d8 — the exact pinned value is
        // in the goldens; here just assert it is finite and large
        assert!(t39.is_finite());
    }

    #[test]
    fn adaptive_delta_single_frame_channel_clamps_to_the_base() {
        let trees = vec![(b"t".to_vec(), tree(1))];
        let mut b = AnimBuild::new(&header_bytes(
            V40,
            &name16("One"),
            &name16("t"),
            1,
            30,
            ANIM_FLAVOR_ADAPTIVE_DELTA,
        ));
        // NumFrames 1: no packet blocks at all
        b.ad_channel(1, 0, 1, ANIM_CHANNEL_X as u8, 0.01, &[2.0], &[]);
        let mut anim = W3dCompressedAnim::parse(&b.bytes, resolve(&trees)).unwrap();
        assert_eq!(anim.get_translation(0, 0.0), [2.0, 0.0, 0.0]);
        assert_eq!(anim.get_translation(0, 0.5), [2.0, 0.0, 0.0]);
        assert_eq!(anim.get_translation(0, 5.0), [2.0, 0.0, 0.0]);
    }

    #[test]
    fn adaptive_delta_wide_channel_writes_only_the_first_component() {
        let trees = vec![(b"t".to_vec(), tree(1))];
        let mut b = AnimBuild::new(&header_bytes(
            V40,
            &name16("Quad"),
            &name16("t"),
            20,
            30,
            ANIM_FLAVOR_ADAPTIVE_DELTA,
        ));
        b.ad_channel(
            20,
            0,
            4,
            ANIM_CHANNEL_X as u8,
            0.01,
            &[0.1, 0.2, 0.3, 0.4],
            &ad_packets(4, 2),
        );
        let mut anim = W3dCompressedAnim::parse(&b.bytes, resolve(&trees)).unwrap();
        // Get_Vector is scalar: only trans[0] is written whatever the
        // VectorLen (the extra components never reach the caller)
        let t = anim.get_translation(0, 0.0);
        assert!((t[0] - 0.1).abs() < 1e-6);
        assert_eq!((t[1], t[2]), (0.0, 0.0));
    }

    #[test]
    fn bit_channel_scan_default_and_clamp() {
        let trees = vec![(b"t".to_vec(), tree(1))];
        let mut b = AnimBuild::new(&header_bytes(
            V40,
            &name16("Bits"),
            &name16("t"),
            8,
            30,
            ANIM_FLAVOR_TIMECODED,
        ));
        // time codes 0/2/4/6 with bits 0/1/0/1, DefaultVal 1 — DEAD:
        // frames below the first time code read packet 0's bit
        b.tc_bit_channel(
            4,
            0,
            BIT_CHANNEL_VIS as u8,
            1,
            &[0, 2 | (1 << 31), 4, 6 | (1 << 31)],
        );
        let mut anim = W3dCompressedAnim::parse(&b.bytes, resolve(&trees)).unwrap();
        assert!(!anim.get_visibility(0, 0.0)); // packet 0's bit = 0
        assert!(anim.get_visibility(0, 2.0)); // packet 1's bit = 1
        assert!(!anim.get_visibility(0, 4.5)); // packet 2's bit = 0
        assert!(anim.get_visibility(0, 100.0)); // past the end: the LAST packet's bit
        assert!(!anim.get_visibility(0, -1.0)); // below the first: packet 0's bit (not the default!)
    }

    #[test]
    fn malformed_payloads_and_missing_trees_fail() {
        let trees = vec![(b"tank".to_vec(), tree(4))];
        // empty payload
        assert_eq!(
            W3dCompressedAnim::parse(&[], resolve(&trees)),
            Err(HcanimError::LoadFailed)
        );
        // the first chunk is not the compressed animation header
        let b = AnimBuild::new(&header_bytes(
            V40,
            &name16("A"),
            &name16("tank"),
            4,
            30,
            ANIM_FLAVOR_TIMECODED,
        ));
        let mut wrong = Vec::new();
        wrong.extend_from_slice(&W3D_CHUNK_COMPRESSED_ANIMATION_CHANNEL.to_le_bytes());
        wrong.extend_from_slice(&4u32.to_le_bytes());
        wrong.extend_from_slice(&[0u8; 4]);
        assert_eq!(
            W3dCompressedAnim::parse(&wrong, resolve(&trees)),
            Err(HcanimError::LoadFailed)
        );
        // a header chunk truncated by the file end
        let mut short = Vec::new();
        short.extend_from_slice(&W3D_CHUNK_COMPRESSED_ANIMATION_HEADER.to_le_bytes());
        short.extend_from_slice(&COMPRESSED_ANIM_HEADER_SIZE.to_le_bytes());
        short.extend_from_slice(&b.bytes[8..8 + 20]);
        assert_eq!(
            W3dCompressedAnim::parse(&short, resolve(&trees)),
            Err(HcanimError::LoadFailed)
        );
        // no tree with that name
        let b = AnimBuild::new(&header_bytes(
            V40,
            &name16("A"),
            &name16("nowhere"),
            4,
            30,
            ANIM_FLAVOR_TIMECODED,
        ));
        assert_eq!(
            W3dCompressedAnim::parse(&b.bytes, resolve(&trees)),
            Err(HcanimError::LoadFailed)
        );
        // a channel chunk smaller than the 12-byte struct
        let mut b = AnimBuild::new(&header_bytes(
            V40,
            &name16("A"),
            &name16("tank"),
            8,
            30,
            ANIM_FLAVOR_TIMECODED,
        ));
        b.raw_chunk(W3D_CHUNK_COMPRESSED_ANIMATION_CHANNEL, &[0u8; 8]);
        assert_eq!(
            W3dCompressedAnim::parse(&b.bytes, resolve(&trees)),
            Err(HcanimError::LoadFailed)
        );
        // a bit channel with NumTimeCodes 0 (the wrapped ~4 GiB read)
        let mut b = AnimBuild::new(&header_bytes(
            V40,
            &name16("A"),
            &name16("tank"),
            8,
            30,
            ANIM_FLAVOR_TIMECODED,
        ));
        b.tc_bit_channel(0, 0, BIT_CHANNEL_VIS as u8, 0, &[0]);
        assert_eq!(
            W3dCompressedAnim::parse(&b.bytes, resolve(&trees)),
            Err(HcanimError::LoadFailed)
        );
        // error display is stable
        assert_eq!(
            HcanimError::LoadFailed.to_string(),
            "W3D compressed animation load failed"
        );
    }

    #[test]
    fn unknown_flavor_loads_without_motion_channels() {
        let trees = vec![(b"t".to_vec(), tree(2))];
        let mut b = AnimBuild::new(&header_bytes(
            V40,
            &name16("Mystery"),
            &name16("t"),
            4,
            30,
            2,
        ));
        b.tc_channel(2, 0, 1, ANIM_CHANNEL_X as u8, &[0, w(1.0), 2, w(2.0)]);
        b.tc_bit_channel(2, 1, BIT_CHANNEL_VIS as u8, 0, &[0, 1 << 31]);
        let mut anim =
            W3dCompressedAnim::parse(&b.bytes, resolve(&trees)).expect("the load succeeds");
        assert_eq!(anim.flavor(), 2);
        // the motion channel was never read (the flavor switch has no
        // case); the bit channel still routes
        assert!(!anim.has_x_translation(0));
        assert!(anim.has_visibility(1));
        // the translation probes answer the pre-assigned zero vector
        assert_eq!(anim.get_translation(0, 1.0), [0.0, 0.0, 0.0]);
        // the port's stated contract for the untouched destinations
        assert_eq!(anim.get_orientation(0, 1.0), [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(anim.get_transform(0, 1.0), IDENTITY_MTX);
    }

    #[test]
    fn out_of_range_and_duplicate_channels() {
        let trees = vec![(b"rig".to_vec(), tree(3))];
        let mut b = AnimBuild::new(&header_bytes(
            V40,
            &name16("Beyond"),
            &name16("rig"),
            4,
            30,
            ANIM_FLAVOR_TIMECODED,
        ));
        // outside the tree: discarded, the load continues
        b.tc_channel(2, 5, 1, ANIM_CHANNEL_X as u8, &[0, w(1.0), 2, w(2.0)]);
        b.tc_bit_channel(2, 9, BIT_CHANNEL_VIS as u8, 1, &[0, 1 << 31]);
        // duplicate: the newest wins
        b.tc_channel(2, 1, 1, ANIM_CHANNEL_Y as u8, &[0, w(1.0), 2, w(2.0)]);
        b.tc_channel(2, 1, 1, ANIM_CHANNEL_Y as u8, &[0, w(9.0), 2, w(8.0)]);
        // an unknown top-level chunk
        b.raw_chunk(0x0999, &[0u8; 8]);
        let mut anim =
            W3dCompressedAnim::parse(&b.bytes, resolve(&trees)).expect("the load continues");
        assert!(!anim.has_x_translation(0));
        assert!(anim.has_y_translation(1));
        assert_eq!(anim.get_translation(1, 0.0), [0.0, 9.0, 0.0]);
        assert!(!anim.has_visibility(0));
    }

    #[test]
    fn name_composition_pins_the_strcpy_and_strlcat_quirks() {
        let trees = vec![(b"t".to_vec(), tree(1))];
        // the plain composition
        let b = AnimBuild::new(&header_bytes(
            V40,
            &name16("Run"),
            &name16("t"),
            4,
            30,
            ANIM_FLAVOR_TIMECODED,
        ));
        let anim = W3dCompressedAnim::parse(&b.bytes, resolve(&trees)).unwrap();
        assert_eq!(anim.name_str(), b"t.Run");
        // an empty animation name keeps the trailing dot; FrameRate 0
        // makes the total time -inf (NumFrames 0x80000000 reads negative)
        let b = AnimBuild::new(&header_bytes(
            V40,
            &name16(""),
            &name16("t"),
            0x8000_0000,
            0,
            ANIM_FLAVOR_TIMECODED,
        ));
        let anim = W3dCompressedAnim::parse(&b.bytes, resolve(&trees)).unwrap();
        assert_eq!(anim.name_str(), b"t.");
        assert_eq!(anim.num_frames(), 0x8000_0000);
        assert!(anim.total_time().is_infinite() && anim.total_time() < 0.0);
        // an unterminated hierarchy field: strcpy runs through the
        // NumFrames word (5)
        let hfield = [b'H'; 16];
        let hb = header_bytes(V40, &name16("A"), &hfield, 5, 30, ANIM_FLAVOR_TIMECODED);
        let mut expected = Vec::from(&hb[20..36]);
        expected.push(5);
        let trees2 = [(expected.to_ascii_lowercase(), tree(1))];
        let build = AnimBuild::new(&hb);
        let anim = W3dCompressedAnim::parse(&build.bytes, |name| {
            trees2
                .iter()
                .find(|(n, _)| *n == name.to_ascii_lowercase())
                .map(|(_, t)| t)
        })
        .expect("the 17-byte key resolves");
        assert_eq!(anim.hierarchy_name_str(), expected.as_slice());
        // truncation at 31 characters
        let long = name16("0123456789ABCDEF");
        let trees3 = [(b"tanktree1234567".to_vec(), tree(1))];
        let b = AnimBuild::new(&header_bytes(
            V40,
            &long,
            &name16("tanktree1234567"),
            4,
            30,
            ANIM_FLAVOR_TIMECODED,
        ));
        let anim = W3dCompressedAnim::parse(&b.bytes, |name| {
            trees3
                .iter()
                .find(|(n, _)| *n == name.to_ascii_lowercase())
                .map(|(_, t)| t)
        })
        .unwrap();
        assert_eq!(anim.name_str().len(), 31);
        assert_eq!(
            anim.name_str(),
            b"tanktree1234567.0123456789ABCDE".as_slice()
        );
    }

    #[test]
    fn frame_rate_is_a_numeric_conversion() {
        let trees = vec![(b"t".to_vec(), tree(1))];
        // 0xFFFF converts exactly
        let b = AnimBuild::new(&header_bytes(
            V40,
            &name16("Rate"),
            &name16("t"),
            100,
            0xFFFF,
            ANIM_FLAVOR_TIMECODED,
        ));
        let anim = W3dCompressedAnim::parse(&b.bytes, resolve(&trees)).unwrap();
        assert_eq!(anim.frame_rate(), 65535.0);
        assert!((anim.total_time() - 100.0 / 65535.0).abs() < 1e-9);
    }

    #[test]
    fn flavor_reads_the_channel_with_its_own_reader() {
        let trees = vec![(b"t".to_vec(), tree(1))];
        // flavor TIMECODED over an adaptive-delta-shaped payload: the AD
        // struct's Scale word doubles as time code 0, the "base float"
        // and "packet bytes" double as the remaining time codes/values
        let mut b = AnimBuild::new(&header_bytes(
            V40,
            &name16("Mismatch"),
            &name16("t"),
            8,
            30,
            ANIM_FLAVOR_TIMECODED,
        ));
        let mut payload = Vec::new();
        payload.extend_from_slice(&3u32.to_le_bytes()); // NumFrames (= NumTimeCodes)
        payload.extend_from_slice(&0u16.to_le_bytes()); // pivot
        payload.push(1); // VectorLen
        payload.push(ANIM_CHANNEL_X as u8); // Flags
        payload.extend_from_slice(&0.0f32.to_bits().to_le_bytes()); // Scale (+0.0f = tc 0)
        payload.extend_from_slice(&1.0f32.to_bits().to_le_bytes()); // "base" = value
        payload.extend_from_slice(&2u32.to_le_bytes()); // "packet" word = tc 2
        payload.extend_from_slice(&2.0f32.to_bits().to_le_bytes());
        payload.extend_from_slice(&4u32.to_le_bytes());
        payload.extend_from_slice(&3.0f32.to_bits().to_le_bytes());
        b.raw_chunk(W3D_CHUNK_COMPRESSED_ANIMATION_CHANNEL, &payload);

        let mut anim =
            W3dCompressedAnim::parse(&b.bytes, resolve(&trees)).expect("the load succeeds");
        assert!(anim.has_x_translation(0));
        let Some(W3dCompressedChannel::TimeCoded(x)) = &anim.node_motion[0].x else {
            panic!("the reader follows the header's flavor");
        };
        assert_eq!(x.num_time_codes, 3);
        assert_eq!(x.data, vec![0, w(1.0), 2, w(2.0), 4, w(3.0)]);
        // frame 1: lerp(1, 2, 0.5)
        assert_eq!(anim.get_translation(0, 1.0), [1.5, 0.0, 0.0]);
    }

    #[test]
    fn filter_table_matches_the_literal_and_generated_halves() {
        let t = filter_table();
        assert_eq!(t[0], 0.000_000_01);
        assert_eq!(t[7], 0.1);
        assert_eq!(t[8], 1.0);
        assert_eq!(t[11], 1000.0);
        assert_eq!(t[15], 10_000_000.0);
        // the generated half: 1 - sin(90*i/240 degrees), DESCENDING from
        // 1 (i = 0) to ~0 (i = 239)
        assert_eq!(t[16], 1.0);
        assert!(t[255].abs() < 1e-4);
        // and monotone non-increasing overall
        for i in 17..FILTER_TABLE_SIZE {
            assert!(t[i] <= t[i - 1], "filtertable not descending at {i}");
        }
    }

    #[test]
    fn float_to_u32_follows_the_x86_conversion() {
        assert_eq!(float_to_u32(0.0), 0);
        assert_eq!(float_to_u32(0.25), 0);
        assert_eq!(float_to_u32(7.75), 7);
        assert_eq!(float_to_u32(-1.0), 0xFFFF_FFFF); // x86 cvttss2si64 + trunc
        assert_eq!(float_to_u32(f32::NAN), 0); // the indefinite low word
    }
}
