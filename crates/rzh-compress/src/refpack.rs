// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Ported from the EAC compression codecs of the Zero Hour source release:
//
//   Core/Libraries/Source/Compression/EAC/refdecode.cpp  (REF_is, REF_size, REF_decode)
//   Core/Libraries/Source/Compression/EAC/refencode.cpp  (REF_encode, refcompress)
//   Core/Libraries/Source/Compression/EAC/gimex.h        (ggetm / gputm: big-endian IO)
//
// The originals carry the notice: Copyright (C) Electronic Arts Canada Inc.
// 1995-2002. All rights reserved.
//
//! The EA **RefPack** codec (`rzh-compress`).
//!
//! RefPack is the LZ77-family codec used inside BIG archives and for many
//! asset payloads. This module is a direct transcription of the game's
//! decoder and encoder, so compressed streams are **byte-identical** to
//! the C++ implementation (parity level P0, pinned by golden vectors
//! generated from the original sources — see
//! `tools/differential/gen_compress_golden.cpp`).
//!
//! # Stream format (as implemented here)
//!
//! - Header: `10 FB` + 3-byte big-endian size, or `90 FB` + 4-byte size
//!   when the payload exceeds 0xFFFFFF bytes. The `11 FB` / `91 FB`
//!   variants (which skip an extra leading size field) are recognized by
//!   [`is_refpack`] / [`uncompressed_size`] and decoded by [`decode`],
//!   matching the original decoder.
//! - Body: a sequence of commands — short / intermediate / "very
//!   intermediate" back-references, literal blocks of up to 112 bytes,
//!   and an end marker carrying 0..3 trailing literals.
//!
//! # Differences from the C (documented)
//!
//! - The C decoder dereferences out-of-range back-references on corrupt
//!   input (undefined behaviour); this port returns
//!   [`Err(RefPackError::InvalidBackReference)`] instead. Valid streams
//!   decode identically.
//! - The C encoder's `galloc` hash-chain buffers are lazily defined in C
//!   (every read follows a write); here they are initialized to the -1
//!   sentinel up front, which is behaviourally identical.
//!
//! # Examples
//!
//! ```
//! use rzh_compress::refpack;
//!
//! let data = b"hello hello hello hello world!".to_vec();
//! let packed = refpack::compress(&data);
//! assert!(refpack::is_refpack(&packed));
//! let unpacked = refpack::decompress(&packed).unwrap();
//! assert_eq!(unpacked, data);
//! ```

/// RefPack decoding errors (malformed streams only; valid streams decode
/// exactly like the C).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefPackError {
    /// The stream ended in the middle of a command or literal run.
    UnexpectedEof,
    /// A back-reference reaches before the start of the output buffer.
    InvalidBackReference,
    /// The stream is not a RefPack stream (bad header).
    BadHeader,
}

/// `REF_is`: checks for a reasonable RefPack header (`10fb`, `11fb`,
/// `90fb`, `91fb`).
#[must_use]
pub fn is_refpack(data: &[u8]) -> bool {
    if data.len() < 2 {
        return false;
    }
    let packtype = u16::from_be_bytes([data[0], data[1]]);
    matches!(packtype, 0x10fb | 0x11fb | 0x90fb | 0x91fb)
}

/// `ggetm(src, n)`: big-endian n-byte read (1..=4 bytes).
fn ggetm(src: &[u8]) -> usize {
    let mut value: usize = 0;
    for &byte in src {
        value = (value << 8) | byte as usize;
    }
    value
}

/// `gputm(dst, value, n)`: big-endian n-byte write.
fn gputm(dst: &mut [u8], value: usize, n: usize) {
    // `dst` is indexed by the shift-derived position (mirrors the C's
    // explicit byte writes); a zip over shifts reads worse.
    #[allow(clippy::needless_range_loop)]
    for i in 0..n {
        dst[i] = (value >> (8 * (n - 1 - i))) as u8;
    }
}

/// `REF_size`: the uncompressed size recorded in the stream header.
///
/// Returns [`RefPackError::UnexpectedEof`] when the header is truncated
/// (the C reads out of bounds there).
pub fn uncompressed_size(data: &[u8]) -> Result<usize, RefPackError> {
    if data.len() < 2 {
        return Err(RefPackError::UnexpectedEof);
    }
    let packtype = u16::from_be_bytes([data[0], data[1]]);
    let ssize = if packtype & 0x8000 != 0 { 4 } else { 3 };

    if packtype & 0x100 != 0 {
        // 11fb / 91fb: skip the leading size field
        let at = 2 + ssize;
        if data.len() < at + ssize {
            return Err(RefPackError::UnexpectedEof);
        }
        Ok(ggetm(&data[at..at + ssize]))
    } else {
        if data.len() < 2 + ssize {
            return Err(RefPackError::UnexpectedEof);
        }
        Ok(ggetm(&data[2..2 + ssize]))
    }
}

/// `REF_decode(dest, compresseddata, compressedsize)`.
///
/// Decompresses into `dest` (which must be exactly
/// [`uncompressed_size`] bytes long, like the game's usage) and returns
/// `(uncompressed_len, compressed_len)`.
pub fn decode(dest: &mut [u8], data: &[u8]) -> Result<(usize, usize), (RefPackError, usize)> {
    let mut s = 0usize; // read cursor
    let mut d = 0usize; // write cursor
    #[allow(unused_assignments)] // the C's `ulen = 0L` seed, overwritten below
    let mut ulen = 0usize;

    if data.len() < 2 {
        return Err((RefPackError::BadHeader, 0));
    }
    let mut type_word: usize = data[s] as usize;
    s += 1;
    type_word = (type_word << 8) + data[s] as usize;
    s += 1;

    if type_word & 0x8000 != 0 {
        // 4 byte size field
        if type_word & 0x100 != 0 {
            // skip ulen
            s += 4;
        }
        if data.len() < s + 4 {
            return Err((RefPackError::UnexpectedEof, s));
        }
        ulen = data[s] as usize;
        ulen = (ulen << 8) + data[s + 1] as usize;
        ulen = (ulen << 8) + data[s + 2] as usize;
        ulen = (ulen << 8) + data[s + 3] as usize;
        s += 4;
    } else {
        if type_word & 0x100 != 0 {
            // skip ulen
            s += 3;
        }
        if data.len() < s + 3 {
            return Err((RefPackError::UnexpectedEof, s));
        }
        ulen = data[s] as usize;
        ulen = (ulen << 8) + data[s + 1] as usize;
        ulen = (ulen << 8) + data[s + 2] as usize;
        s += 3;
    }

    // Copy literals from s, advancing both cursors; errors on truncation.
    macro_rules! literals {
        ($count:expr) => {{
            let count: usize = ($count) as usize;
            if s + count > data.len() || d + count > dest.len() {
                return Err((RefPackError::UnexpectedEof, s));
            }
            for _ in 0..count {
                dest[d] = data[s];
                d += 1;
                s += 1;
            }
        }};
    }

    // Copy `count` bytes from the back-reference at `back`.
    macro_rules! backref {
        ($back:expr, $count:expr) => {{
            let back: isize = $back;
            let count: usize = ($count) as usize;
            let base = d as isize - 1 - back;
            if base < 0 {
                return Err((RefPackError::InvalidBackReference, s));
            }
            let mut ref_pos = base as usize;
            for _ in 0..count {
                if d >= dest.len() {
                    return Err((RefPackError::UnexpectedEof, s));
                }
                dest[d] = dest[ref_pos];
                d += 1;
                ref_pos += 1;
            }
        }};
    }

    loop {
        if s >= data.len() {
            return Err((RefPackError::UnexpectedEof, s));
        }
        let first = data[s];
        s += 1;
        if first & 0x80 == 0 {
            // short form
            if s >= data.len() {
                return Err((RefPackError::UnexpectedEof, s));
            }
            let second = data[s];
            s += 1;
            literals!(first & 3);
            let back = (((first & 0x60) as usize) << 3) + second as usize;
            // C: run = ((first&0x1c)>>2)+3-1 with a do..while(run--)
            // loop => ((first&0x1c)>>2)+3 copies.
            let count = ((first & 0x1c) >> 2) as usize + 3;
            backref!(back as isize, count);
            continue;
        }
        if first & 0x40 == 0 {
            // int form
            if s + 2 > data.len() {
                return Err((RefPackError::UnexpectedEof, s));
            }
            let second = data[s];
            let third = data[s + 1];
            s += 2;
            literals!(second >> 6);
            let back = ((second & 0x3f) as usize) * 256 + third as usize;
            backref!(back as isize, (first & 0x3f) as usize + 4);
            continue;
        }
        if first & 0x20 == 0 {
            // very int form
            if s + 3 > data.len() {
                return Err((RefPackError::UnexpectedEof, s));
            }
            let second = data[s];
            let third = data[s + 1];
            let forth = data[s + 2];
            s += 3;
            literals!(first & 3);
            let back =
                (((first & 0x10) as usize) >> 4) * 65536 + (second as usize) * 256 + third as usize;
            let count = (((first & 0x0c) as usize) >> 2) * 256 + forth as usize + 5;
            backref!(back as isize, count);
            continue;
        }
        let run = ((first & 0x1f) as usize) * 4 + 4; // literal
        if run <= 112 {
            literals!(run);
            continue;
        }
        literals!(first & 3); // eof (+0..3 literal)
        break;
    }

    Ok((ulen, s))
}

/// Decompresses a whole RefPack stream into a fresh buffer (the common
/// game-side usage: allocate `REF_size`, then `REF_decode`).
pub fn decompress(data: &[u8]) -> Result<Vec<u8>, RefPackError> {
    if !is_refpack(data) {
        return Err(RefPackError::BadHeader);
    }
    let ulen = uncompressed_size(data)?;
    let mut dest = vec![0u8; ulen];
    let (ulen, _consumed) = decode(&mut dest, data).map_err(|(e, _)| e)?;
    debug_assert!(ulen <= dest.len());
    dest.truncate(ulen);
    Ok(dest)
}

/// `matchlen`: the number of leading equal bytes, capped at `maxmatch`.
///
/// Written over sub-slices rather than raw indices so the bounds checks
/// hoist out of the byte loop: this function is the inner loop of the
/// encoder's chain walk (the arithmetic is identical to the C
/// `while (from[a+i] == from[b+i]) i++;`).
fn matchlen(from: &[u8], a: usize, b: usize, maxmatch: usize) -> usize {
    let x = &from[a..a + maxmatch];
    let y = &from[b..b + maxmatch];
    x.iter().zip(y).take_while(|(p, q)| **p == **q).count()
}

/// `HASH(cptr)`: the 3-byte hash of the encoder.
#[inline]
fn hash3(from: &[u8], cptr: usize) -> usize {
    (((from[cptr] as usize) << 8) | from[cptr + 2] as usize) ^ ((from[cptr + 1] as usize) << 4)
}

/// `refcompress`: the RefPack compressor core (transcribed from
/// `refencode.cpp`; the hash-chain bookkeeping uses indices instead of
/// pointers).
///
/// Writes into `dest` and returns the number of bytes written. `dest`
/// must be large enough for the worst case (the caller sizes it; the
/// original relies on the caller too).
fn refcompress(from: &[u8], dest: &mut Vec<u8>, maxback: usize, quick: bool) -> usize {
    const HASH_SIZE: usize = 65536;
    const LINK_SIZE: usize = 131072;
    const LINK_MASK: usize = LINK_SIZE - 1;
    // NOTE: the C clamps `maxback` but then never reads it — the window
    // actually used is the constant 131071 below. Preserved (dead
    // parameter and all) so the signature mirrors REF_encode's options.
    let _ = maxback.min(131071);

    let mut hashtbl = vec![-1i32; HASH_SIZE];
    let mut link = vec![-1i32; LINK_SIZE];

    let start_len = dest.len();
    let mut run: usize = 0;
    let mut cptr: usize = 0; // cursor into from
    let mut rptr: usize = 0; // start of the pending literal run

    let mut len: isize = from.len() as isize - 4;
    while len >= 0 {
        let mut boffset: usize = 0;
        let mut blen: usize = 2;
        let mut bcost: usize = 2;
        let mlen = (len as usize).min(1028);
        let mut hash = hash3(from, cptr);
        let mut hoffset = hashtbl[hash];
        let minhoffset: i32 = (cptr as i32 - 131071).max(0);

        if hoffset >= minhoffset {
            loop {
                let tptr = hoffset as usize;
                if from[cptr + blen] == from[tptr + blen] {
                    let tlen = matchlen(from, cptr, tptr, mlen);
                    if tlen > blen {
                        // C: toffset = (cptr-1) - tptr (pointer difference)
                        let toffset = (cptr - 1) - tptr;
                        // two byte / three byte / four byte form cost
                        let tcost = if toffset < 1024 && tlen <= 10 {
                            2
                        } else if toffset < 16384 && tlen <= 67 {
                            3
                        } else {
                            4
                        };

                        // C unsigned comparison `tlen - tcost + 4 > blen - bcost + 4`
                        // can wrap in the left operand when tcost > tlen; the
                        // wrapped result is mathematically tlen + 4 - tcost
                        // (both sides are small), so write it overflow-free.
                        if tlen + 4 - tcost > blen + 4 - bcost {
                            blen = tlen;
                            bcost = tcost;
                            boffset = toffset;
                            if blen >= 1028 {
                                break;
                            }
                        }
                    }
                }
                hoffset = link[hoffset as usize & LINK_MASK];
                if hoffset < minhoffset {
                    break;
                }
            }
        }

        if bcost >= blen || len < 4 {
            let h = cptr as i32;
            link[cptr & LINK_MASK] = hashtbl[hash];
            hashtbl[hash] = h;

            run += 1;
            cptr += 1;
            len -= 1;
        } else {
            while run > 3 {
                // literal block of data
                let tlen = 112.min(run & !3);
                run -= tlen;
                dest.push(0xe0 + (tlen >> 2) as u8 - 1);
                dest.extend_from_slice(&from[rptr..rptr + tlen]);
                rptr += tlen;
            }
            if bcost == 2 {
                // two byte int form
                dest.push((((boffset >> 8) << 5) + ((blen - 3) << 2) + run) as u8);
                dest.push(boffset as u8);
            } else if bcost == 3 {
                // three byte int form
                dest.push((0x80 + (blen - 4)) as u8);
                dest.push(((run << 6) + (boffset >> 8)) as u8);
                dest.push(boffset as u8);
            } else {
                // four byte very int form
                dest.push((0xc0 + ((boffset >> 16) << 4) + (((blen - 5) >> 8) << 2) + run) as u8);
                dest.push((boffset >> 8) as u8);
                dest.push(boffset as u8);
                dest.push((blen - 5) as u8);
            }
            if run > 0 {
                dest.extend_from_slice(&from[rptr..rptr + run]);
                run = 0;
            }

            if quick {
                let h = cptr as i32;
                link[cptr & LINK_MASK] = hashtbl[hash];
                hashtbl[hash] = h;
                cptr += blen;
            } else {
                for _ in 0..blen {
                    hash = hash3(from, cptr);
                    let h = cptr as i32;
                    link[cptr & LINK_MASK] = hashtbl[hash];
                    hashtbl[hash] = h;
                    cptr += 1;
                }
            }

            rptr = cptr;
            len -= blen as isize;
        }
    }
    len += 4;
    run += len as usize;
    while run > 3 {
        // no match at end, use literal
        let tlen = 112.min(run & !3);
        run -= tlen;
        dest.push(0xe0 + (tlen >> 2) as u8 - 1);
        dest.extend_from_slice(&from[rptr..rptr + tlen]);
        rptr += tlen;
    }

    dest.push(0xfc + run as u8); // end of stream command + 0..3 literal
    if run > 0 {
        dest.extend_from_slice(&from[rptr..rptr + run]);
    }

    dest.len() - start_len
}

/// `REF_encode(compresseddata, source, sourcesize, opts)`: compresses
/// `source` into a RefPack stream (header included), returning the whole
/// stream as a fresh buffer.
///
/// The original's `opts` parameter is unused by the game (always null);
/// this port fixes the same configuration the game uses
/// (`maxback = 131072`, `quick = false`).
#[must_use]
pub fn compress(source: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();

    // simple fb6 header
    if source.len() > 0xffffff {
        // 32 bit header required
        out.extend_from_slice(&[0x90, 0xfb]);
        let mut size_bytes = [0u8; 4];
        gputm(&mut size_bytes, source.len(), 4);
        out.extend_from_slice(&size_bytes);
    } else {
        out.extend_from_slice(&[0x10, 0xfb]);
        let mut size_bytes = [0u8; 3];
        gputm(&mut size_bytes, source.len(), 3);
        out.extend_from_slice(&size_bytes);
    }

    refcompress(source, &mut out, 131072, false);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_small_inputs() {
        // Edge sizes around the encoder's 4-byte look-ahead margin.
        for len in [0usize, 1, 2, 3, 4, 5, 6, 7, 8, 15, 16, 100] {
            let data: Vec<u8> = (0..len).map(|i| (i * 7 + 3) as u8).collect();
            let packed = compress(&data);
            assert!(is_refpack(&packed), "len {len}");
            assert_eq!(uncompressed_size(&packed).unwrap(), len, "len {len}");
            let unpacked = decompress(&packed).unwrap();
            assert_eq!(unpacked, data, "len {len}");
        }
    }

    #[test]
    fn round_trips_compressible_data() {
        let mut data = Vec::new();
        for i in 0..40 {
            let pattern = [
                (i % 7) as u8,
                0x41,
                0x42,
                (i % 13) as u8,
                0x43,
                0x44,
                0x45,
                0x46,
            ];
            for _ in 0..(i % 5 + 1) {
                data.extend_from_slice(&pattern);
            }
        }
        let packed = compress(&data);
        assert!(packed.len() < data.len(), "should actually compress");
        assert_eq!(decompress(&packed).unwrap(), data);
    }

    #[test]
    fn round_trips_random_data() {
        // Incompressible data still round-trips (literal blocks).
        let mut state = 0x12345678u32;
        let mut next = || {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            (state >> 24) as u8
        };
        let data: Vec<u8> = (0..10_000).map(|_| next()).collect();
        let packed = compress(&data);
        assert_eq!(decompress(&packed).unwrap(), data);
    }

    #[test]
    fn rejects_bad_headers() {
        assert_eq!(decompress(&[]), Err(RefPackError::BadHeader));
        assert_eq!(
            decompress(&[0x00, 0x01, 0x00]),
            Err(RefPackError::BadHeader)
        );
        assert!(!is_refpack(&[0x10, 0xfc, 0x00]));
    }

    #[test]
    fn detects_invalid_back_references() {
        // Hand-built corrupt stream: 10fb header (size 8) then a
        // short-form back-reference with offset 255 while nothing has
        // been written yet (d == 0), reaching before the buffer start.
        let stream = [0x10u8, 0xfb, 0x00, 0x00, 0x08, 0x00, 0xff];
        let mut dest = vec![0u8; 8];
        assert_eq!(
            decode(&mut dest, &stream),
            Err((RefPackError::InvalidBackReference, 7))
        );
    }
}
