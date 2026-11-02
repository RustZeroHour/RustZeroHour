// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Rust port Copyright 2026 the RustZeroHour contributors.
//
//! Differential parity tests for the RefPack port (P0).
//!
//! The vectors in `tests/golden/refpack.txt` are produced by
//! `tools/differential/gen_compress_golden.cpp`, which compiles the REAL
//! EAC codec sources (`refdecode.cpp` / `refencode.cpp`) from the game
//! tree. The encoder must reproduce the compressed streams byte-for-byte
//! and the decoder must reproduce the decompressed payloads
//! byte-for-byte.

use rzh_compress::refpack;

fn golden_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join("refpack.txt")
}

fn unhex(text: &str) -> Vec<u8> {
    if text == "-" {
        return Vec::new();
    }
    assert_eq!(text.len() % 2, 0, "odd hex length");
    let mut out = Vec::with_capacity(text.len() / 2);
    let bytes = text.as_bytes();
    for pair in bytes.chunks(2) {
        let hi = (pair[0] as char).to_digit(16).expect("hex digit");
        let lo = (pair[1] as char).to_digit(16).expect("hex digit");
        out.push((hi * 16 + lo) as u8);
    }
    out
}

#[test]
fn refpack_matches_cpp_golden_vectors() {
    let text = std::fs::read_to_string(golden_path())
        .unwrap_or_else(|e| panic!("cannot read golden file: {e}"));
    let mut checked = 0;

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.split_whitespace();
        assert_eq!(fields.next(), Some("refpack"), "bad line: {line}");
        let op = fields.next().unwrap_or_else(|| panic!("bad line: {line}"));
        match op {
            "encode" => {
                let input = unhex(fields.next().unwrap());
                let expected = unhex(fields.next().unwrap());
                let actual = refpack::compress(&input);
                assert_eq!(
                    actual,
                    expected,
                    "encoder mismatch on {} byte input",
                    input.len()
                );
            }
            "size" => {
                let packed = unhex(fields.next().unwrap());
                let expected: usize = fields.next().unwrap().parse().unwrap();
                assert_eq!(refpack::uncompressed_size(&packed).unwrap(), expected);
            }
            "decode" => {
                let packed = unhex(fields.next().unwrap());
                let expected = unhex(fields.next().unwrap());
                let consumed: usize = fields.next().unwrap().parse().unwrap();

                // decode into the exact-size destination like the game
                let ulen = refpack::uncompressed_size(&packed).unwrap();
                let mut dest = vec![0u8; ulen];
                let (actual_ulen, actual_consumed) = refpack::decode(&mut dest, &packed)
                    .unwrap_or_else(|(e, at)| {
                        panic!(
                            "decode failed ({e:?} at {at}) on {} byte stream",
                            packed.len()
                        )
                    });
                assert_eq!(actual_ulen, ulen, "ulen mismatch");
                assert_eq!(actual_consumed, consumed, "consumed mismatch");
                assert_eq!(dest, expected, "decoded payload mismatch");

                // and the convenience wrapper agrees
                assert_eq!(refpack::decompress(&packed).unwrap(), expected);
            }
            other => panic!("unknown record {other} in {line}"),
        }
        checked += 1;
    }

    assert!(checked > 300, "suspiciously few records: {checked}");
    println!("refpack golden vectors: {checked} checked");
}

/// Cross-check not covered by the C++ side: every golden stream
/// round-trips through the Rust encoder and decoder together.
#[test]
fn refpack_round_trips_rust_to_rust() {
    let text = std::fs::read_to_string(golden_path())
        .unwrap_or_else(|e| panic!("cannot read golden file: {e}"));
    let mut checked = 0;

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.split_whitespace();
        if fields.next() != Some("refpack") {
            continue;
        }
        if fields.next() != Some("decode") {
            continue;
        }
        let packed = unhex(fields.next().unwrap());
        let expected = unhex(fields.next().unwrap());

        // Rust decoder over the C++ stream (again, for the round trip the
        // payload is the input side)
        let decoded = refpack::decompress(&packed).unwrap();
        // Rust encoder must reproduce the very same stream
        let recoded = refpack::compress(&decoded);
        assert_eq!(
            recoded,
            packed,
            "re-encoding diverged on {} byte payload",
            decoded.len()
        );
        let _ = expected;
        checked += 1;
    }

    assert!(checked > 100, "suspiciously few records: {checked}");
    println!("refpack rust-to-rust round trips: {checked} checked");
}
