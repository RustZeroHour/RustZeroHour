// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
//! Differential parity tests against the committed golden vectors (P0).
//!
//! The vectors in `tests/golden/` are produced by `tools/differential/
//! gen_golden.cpp`, a verbatim transcription of the C++ algorithms at the
//! baseline commit `571a7c5`. If a test here fails, either the port or the
//! transcription has drifted from the original — do not "fix" the golden
//! files by hand; regenerate them with `tools/differential/regenerate.sh`
//! and review the diff.

use std::fs;
use std::path::PathBuf;

use rzh_util::{Crc, GameRng};

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
}

fn read_golden(name: &str) -> String {
    let path = golden_dir().join(name);
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
}

/// Parses `len hex crc` lines and checks the Rust [`Crc`] against each one.
#[test]
fn crc_matches_cpp_golden_vectors() {
    let text = read_golden("crc.txt");
    let mut checked = 0;

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.split_whitespace();
        let tag = fields.next().expect("missing tag");
        assert_eq!(tag, "crc", "unexpected record tag in crc.txt: {line}");
        let len: usize = fields.next().unwrap().parse().expect("bad length");
        let hex = fields.next().expect("missing input hex");
        let expected: u32 = u32::from_str_radix(fields.next().unwrap(), 16).expect("bad crc hex");
        assert!(
            fields.next().is_none(),
            "trailing fields in crc.txt: {line}"
        );

        let bytes: Vec<u8> = if hex == "-" {
            assert_eq!(len, 0, "dash input must have length 0");
            Vec::new()
        } else {
            decode_hex(hex)
        };
        assert_eq!(bytes.len(), len, "length mismatch in crc.txt: {line}");

        let mut crc = Crc::new();
        crc.update(&bytes);
        assert_eq!(crc.get(), expected, "CRC mismatch for input {hex}");
        checked += 1;
    }

    assert!(checked > 300, "suspiciously few CRC vectors: {checked}");
}

/// Drives one [`GameRng`] through each `seq` record and checks every value.
#[test]
fn rng_matches_cpp_golden_vectors() {
    let text = read_golden("rng.txt");
    let mut checked = 0;
    let mut rng: Option<GameRng> = None;

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.split_whitespace();
        match fields.next().expect("missing tag") {
            "seq" => {
                let seed = u32::from_str_radix(fields.next().unwrap(), 16).expect("bad seed hex");
                assert!(rng.is_none(), "nested seq record in rng.txt: {line}");
                rng = Some(GameRng::seeded(seed));
            }
            "end" => {
                assert!(rng.take().is_some(), "end without seq in rng.txt: {line}");
            }
            "raw" => {
                let expected =
                    u32::from_str_radix(fields.next().unwrap(), 16).expect("bad raw hex");
                assert_eq!(
                    rng.as_mut().expect("raw outside seq").raw(),
                    expected,
                    "raw value mismatch (seed stream advanced differently?)"
                );
                checked += 1;
            }
            "int" => {
                let lo: i32 = fields.next().unwrap().parse().expect("bad lo");
                let hi: i32 = fields.next().unwrap().parse().expect("bad hi");
                let expected: i32 = fields.next().unwrap().parse().expect("bad value");
                assert_eq!(
                    rng.as_mut().expect("int outside seq").random_int(lo, hi),
                    expected,
                    "random_int({lo}, {hi}) mismatch"
                );
                checked += 1;
            }
            "real" => {
                let lo = f32::from_bits(u32::from_str_radix(fields.next().unwrap(), 16).unwrap());
                let hi = f32::from_bits(u32::from_str_radix(fields.next().unwrap(), 16).unwrap());
                let expected =
                    f32::from_bits(u32::from_str_radix(fields.next().unwrap(), 16).unwrap());
                let actual = rng.as_mut().expect("real outside seq").random_real(lo, hi);
                assert_eq!(
                    actual.to_bits(),
                    expected.to_bits(),
                    "random_real({lo}, {hi}) mismatch: {actual} vs {}",
                    expected
                );
                checked += 1;
            }
            "unchanged_int" => {
                let lo: i32 = fields.next().unwrap().parse().expect("bad lo");
                let hi: i32 = fields.next().unwrap().parse().expect("bad hi");
                let expected: i32 = fields.next().unwrap().parse().expect("bad value");
                assert_eq!(
                    rng.as_ref()
                        .expect("unchanged_int outside seq")
                        .random_int_unchanged(lo, hi),
                    expected,
                    "random_int_unchanged({lo}, {hi}) mismatch"
                );
                checked += 1;
            }
            "unchanged_real" => {
                let lo = f32::from_bits(u32::from_str_radix(fields.next().unwrap(), 16).unwrap());
                let hi = f32::from_bits(u32::from_str_radix(fields.next().unwrap(), 16).unwrap());
                let expected =
                    f32::from_bits(u32::from_str_radix(fields.next().unwrap(), 16).unwrap());
                let actual = rng
                    .as_ref()
                    .expect("unchanged_real outside seq")
                    .random_real_unchanged(lo, hi);
                assert_eq!(
                    actual.to_bits(),
                    expected.to_bits(),
                    "random_real_unchanged({lo}, {hi}) mismatch"
                );
                checked += 1;
            }
            other => panic!("unexpected record tag in rng.txt: {other}"),
        }
    }

    assert!(rng.is_none(), "unterminated seq record in rng.txt");
    assert!(checked > 300, "suspiciously few RNG vectors: {checked}");
}

/// The golden files must stay byte-identical to what the C++ harness emits,
/// so that a regeneration diff is always meaningful.
#[test]
fn golden_files_are_reproducible_from_the_documented_seed_inputs() {
    // Sanity check on a few well-known anchors rather than a full re-run of
    // the generator (which needs a C++ compiler and runs in
    // tools/differential instead).
    let crc_text = read_golden("crc.txt");
    assert!(
        crc_text.contains("crc 9 313233343536373839 000063c5"),
        "the '123456789' anchor vector is missing or changed"
    );
    assert!(
        crc_text.contains("crc 0 - 00000000"),
        "the empty-input anchor vector is missing or changed"
    );

    let rng_text = read_golden("rng.txt");
    assert!(
        rng_text.contains("seq 0000002a\nraw 559a52e9"),
        "the seed-42 anchor vector is missing or changed"
    );
}

fn decode_hex(hex: &str) -> Vec<u8> {
    assert!(hex.len() % 2 == 0, "odd hex length: {hex}");
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    let mut rest = hex;
    while !rest.is_empty() {
        let (head, tail) = rest.split_at(2);
        bytes.push(u8::from_str_radix(head, 16).unwrap_or_else(|_| panic!("bad hex byte {head}")));
        rest = tail;
    }
    bytes
}
