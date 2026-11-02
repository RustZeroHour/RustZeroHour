// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Ported from Core/Libraries/Source/Compression/CompressionManager.cpp
// and Compression.h @ 571a7c5, Zero Hour configuration. The original
// carries the notice: Copyright (c) Matthew D. Campbell (Compression.h).
//
//! The engine's compression dispatch (`CompressionManager`).
//!
//! Compressed payloads inside BIG archives (and a few loose files) carry
//! an 8-byte header — a fourcc identifying the codec plus a 32-bit
//! uncompressed size — followed by the codec stream:
//!
//! ```text
//! "NOX\0"  Nox LZW    "ZL1\0".."ZL9\0"  zlib levels 1-9
//! "EAB\0"  EA B-tree  "EAH\0"           EA Huffman
//! "EAR\0"  RefPack     (the codec Zero Hour data actually uses)
//! ```
//!
//! The size field is stored in the **host** endianness of the original
//! Windows build, i.e. little endian; the port reads it as little endian
//! and documents the pin.
//!
//! # Differences from the C++ (documented)
//!
//! - The C returns 0 for "unsupported" and "malformed" alike; the port
//!   returns [`Err(CompressionError)`] naming the reason. Valid `EAR`
//!   payloads decompress to the same bytes (the RefPack decoder is the
//!   byte-exact `rzh-compress` port).
//! - Only the RefPack family is wired: it is the sole codec reachable
//!   from Zero Hour data (`getPreferredCompression` is RefPack and no
//!   shipped file carries another magic). zlib, Nox LZW, B-tree and
//!   Huffman stay unported — [`CompressionError::Unsupported`] names
//!   them when encountered.
//!
//! # Parity level
//!
//! **P0 exact** for detection, sizes and RefPack decompression (golden
//! vectors in `tests/golden/vfs.txt` use payloads produced by the real
//! EAC encoder).

use rzh_compress::refpack;

/// `CompressionType`: the codecs the header fourccs identify.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompressionType {
    /// `COMPRESSION_NONE`: no known header.
    None,
    /// `"EAR\0"` (`COMPRESSION_REFPACK`).
    RefPack,
    /// `"NOX\0"` (`COMPRESSION_NOXLZH`) — not ported.
    NoxLzh,
    /// `"ZL1\0"`..`"ZL9\0"` (`COMPRESSION_ZLIB1..9`), zlib level 1-9 —
    /// not ported.
    Zlib(u8),
    /// `"EAB\0"` (`COMPRESSION_BTREE`) — not ported.
    BTree,
    /// `"EAH\0"` (`COMPRESSION_HUFF`) — not ported.
    Huff,
}

/// Decompression failures, and the codecs that are recognised but not
/// ported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompressionError {
    /// Fewer than 8 bytes: never compressed data.
    TooSmall,
    /// A recognised but unported codec (the variant names it).
    Unsupported(CompressionType),
    /// The RefPack stream is malformed (the C returns 0).
    BadRefPack(refpack::RefPackError),
}

impl std::error::Error for CompressionError {}

impl std::fmt::Display for CompressionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooSmall => write!(f, "less than 8 bytes is never compressed data"),
            Self::Unsupported(t) => write!(f, "compression codec {t:?} is not ported"),
            Self::BadRefPack(e) => write!(f, "malformed RefPack stream: {e:?}"),
        }
    }
}

/// `getCompressionType(mem, len)`: the codec named by the payload
/// header, or [`CompressionType::None`] for payloads under 8 bytes or
/// without a known fourcc.
#[must_use]
pub fn get_compression_type(data: &[u8]) -> CompressionType {
    if data.len() < 8 {
        return CompressionType::None;
    }
    let magic = &data[..4];
    match magic {
        b"NOX\0" => CompressionType::NoxLzh,
        b"ZL1\0" => CompressionType::Zlib(1),
        b"ZL2\0" => CompressionType::Zlib(2),
        b"ZL3\0" => CompressionType::Zlib(3),
        b"ZL4\0" => CompressionType::Zlib(4),
        b"ZL5\0" => CompressionType::Zlib(5),
        b"ZL6\0" => CompressionType::Zlib(6),
        b"ZL7\0" => CompressionType::Zlib(7),
        b"ZL8\0" => CompressionType::Zlib(8),
        b"ZL9\0" => CompressionType::Zlib(9),
        b"EAB\0" => CompressionType::BTree,
        b"EAH\0" => CompressionType::Huff,
        b"EAR\0" => CompressionType::RefPack,
        _ => CompressionType::None,
    }
}

/// `isDataCompressed(mem, len)`.
#[must_use]
pub fn is_data_compressed(data: &[u8]) -> bool {
    get_compression_type(data) != CompressionType::None
}

/// `getUncompressedSize(mem, len)`: the size stored in the payload
/// header (little endian, the original's host order) — or `len` itself
/// for anything that is not a compressed payload.
#[must_use]
pub fn get_uncompressed_size(data: &[u8]) -> usize {
    if data.len() < 8 {
        return data.len();
    }
    if get_compression_type(data) == CompressionType::None {
        return data.len();
    }
    u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize
}

/// `decompressData(src, srcLen, dest, destLen)`: decompresses a
/// compressed payload into `dest` (sized from
/// [`get_uncompressed_size`], like the C++ callers do) and returns the
/// decompressed length.
///
/// Only `EAR\0` (RefPack) is wired; the other recognised codecs return
/// [`CompressionError::Unsupported`] (the C returns 0).
pub fn decompress_data(src: &[u8], dest: &mut [u8]) -> Result<usize, CompressionError> {
    if src.len() < 8 {
        return Err(CompressionError::TooSmall);
    }

    match get_compression_type(src) {
        CompressionType::RefPack => {
            // REF_decode(dest, src+8, &slen) with slen = srcLen - 8
            let stream = &src[8..];
            let ulen = refpack::uncompressed_size(stream).map_err(CompressionError::BadRefPack)?;
            if dest.len() < ulen {
                // the C would write past the caller's buffer; the port
                // refuses (its callers size dest from the header)
                return Err(CompressionError::BadRefPack(
                    refpack::RefPackError::UnexpectedEof,
                ));
            }
            let (written, _consumed) = refpack::decode(&mut dest[..ulen], stream)
                .map_err(|(e, _at)| CompressionError::BadRefPack(e))?;
            Ok(written)
        }
        other @ (CompressionType::NoxLzh
        | CompressionType::Zlib(_)
        | CompressionType::BTree
        | CompressionType::Huff) => Err(CompressionError::Unsupported(other)),
        CompressionType::None => Err(CompressionError::TooSmall),
    }
}

/// `compressData(COMPRESSION_REFPACK, ...)`: wraps `payload` in the
/// `EAR\0` header with the byte-exact RefPack encoder — the inverse of
/// [`decompress_data`], and the form the golden vectors store.
#[must_use]
pub fn compress_refpack(payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 16);
    out.extend_from_slice(b"EAR\0");
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(&refpack::compress(payload));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_every_header_family() {
        assert_eq!(get_compression_type(b"1234567"), CompressionType::None); // too small
        assert_eq!(
            get_compression_type(b"NOX\0\0\0\0\0"),
            CompressionType::NoxLzh
        );
        assert_eq!(
            get_compression_type(b"ZL1\0\0\0\0\0"),
            CompressionType::Zlib(1)
        );
        assert_eq!(
            get_compression_type(b"ZL9\0\0\0\0\0"),
            CompressionType::Zlib(9)
        );
        assert_eq!(
            get_compression_type(b"EAB\0\0\0\0\0"),
            CompressionType::BTree
        );
        assert_eq!(
            get_compression_type(b"EAH\0\0\0\0\0"),
            CompressionType::Huff
        );
        assert_eq!(
            get_compression_type(b"EAR\0\0\0\0\0"),
            CompressionType::RefPack
        );
        assert_eq!(
            get_compression_type(b"NOT\0\0\0\0\0"),
            CompressionType::None
        );

        assert!(is_data_compressed(b"EAR\0\0\0\0\0"));
        assert!(!is_data_compressed(b"plain text!!"));
    }

    #[test]
    fn uncompressed_size_reads_the_header_little_endian() {
        let mut data = b"EAR\0".to_vec();
        data.extend_from_slice(&0x0102_0304u32.to_le_bytes());
        data.extend_from_slice(&[0u8; 8]);
        assert_eq!(get_uncompressed_size(&data), 0x0102_0304);
        // plain payloads report their own length
        assert_eq!(get_uncompressed_size(b"abcdefgh"), 8);
        assert_eq!(get_uncompressed_size(b"abc"), 3);
    }

    #[test]
    fn refpack_round_trips_through_the_wrapper() {
        let payload: Vec<u8> = (0..2000u32).map(|i| (i % 251) as u8).collect();
        let wrapped = compress_refpack(&payload);
        assert_eq!(get_compression_type(&wrapped), CompressionType::RefPack);
        assert_eq!(get_uncompressed_size(&wrapped), payload.len());

        let mut dest = vec![0u8; payload.len()];
        let written = decompress_data(&wrapped, &mut dest).unwrap();
        assert_eq!(written, payload.len());
        assert_eq!(dest, payload);
    }

    #[test]
    fn unsupported_and_malformed_are_errors() {
        assert_eq!(
            decompress_data(b"ZL1\0\0\0\0\0xxxx", &mut [0u8; 8]),
            Err(CompressionError::Unsupported(CompressionType::Zlib(1)))
        );
        assert_eq!(
            decompress_data(b"plain", &mut [0u8; 8]),
            Err(CompressionError::TooSmall)
        );
    }
}
