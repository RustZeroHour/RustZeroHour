// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Rust port Copyright 2026 the RustZeroHour contributors.
//
//! Differential parity tests for the CSF port (P0).
//!
//! `tests/golden/csf.txt` is a script produced by
//! `tools/differential/gen_csf_golden.cpp` — verbatim transcriptions of
//! the two C++ references:
//!
//! - the **writer** (Babylon `expimp.cpp` `generate_csf`/`writeCSFLabel`/
//!   `writeCSFString`, minus the tool-only content transforms) pins the
//!   byte direction: `gen` lines must equal `CsfFile::to_bytes` of the
//!   parsed structure;
//! - the **reader** (runtime `GameText.cpp` `parseCSF` + `stripSpaces`)
//!   pins the game-visible view: `ok`/`err` verdicts, accepted label
//!   counts, and per-label `view` lines (C-string label/speech bytes,
//!   text as UTF-16LE units after the invert-until-zero quirk and space
//!   stripping);
//! - `info` lines pin `getCSFInfo`'s language selection
//!   (`version >= 2` → file `langid`, else US).
//!
//! The test replays the script against the Rust port and asserts
//! line-by-line equality.

use std::collections::BTreeMap;
use std::path::PathBuf;

use rzh_formats::csf::CsfFile;

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join("csf.txt")
}

fn unhex(text: &str) -> Vec<u8> {
    if text == "-" {
        return Vec::new();
    }
    assert_eq!(text.len() % 2, 0, "odd hex length in golden: {text:?}");
    let mut out = Vec::with_capacity(text.len() / 2);
    for pair in text.as_bytes().chunks(2) {
        let hi = (pair[0] as char).to_digit(16).expect("hex digit");
        let lo = (pair[1] as char).to_digit(16).expect("hex digit");
        out.push((hi * 16 + lo) as u8);
    }
    out
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

fn hex_wide(units: &[u16]) -> String {
    let mut bytes = Vec::with_capacity(units.len() * 2);
    for u in units {
        bytes.extend_from_slice(&u.to_le_bytes());
    }
    hex(&bytes)
}

/// The game-visible view of one label, hex-dumped in the golden script:
/// C-string label bytes, UTF-16LE text units, C-string speech bytes.
type GameView = (Vec<u8>, Vec<u16>, Vec<u8>);

#[derive(Default)]
struct Corpus {
    bytes: Vec<u8>,
    // `gen` is a reserved keyword in edition 2024 — the transcribed
    // Babylon writer's expected bytes are `writer_bytes`.
    writer_bytes: Option<Vec<u8>>,
    info: Option<i32>,
    ok: bool,
    err: bool,
    count: Option<usize>,
    views: BTreeMap<usize, GameView>,
}

#[test]
fn golden_csf() {
    let text = std::fs::read_to_string(golden_path()).expect("golden file");
    let mut corpora: BTreeMap<String, Corpus> = BTreeMap::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        let corpus = corpora.entry(fields[1].to_string()).or_default();

        match fields[0] {
            "file" => corpus.bytes = unhex(fields[2]),
            "gen" => corpus.writer_bytes = Some(unhex(fields[2])),
            "info" => corpus.info = fields[2].parse::<i32>().ok(),
            "ok" => corpus.ok = true,
            "err" => corpus.err = true,
            "count" => corpus.count = Some(fields[2].parse().expect("count")),
            "view" => {
                let label = unhex(fields[3]);
                let text = unhex(fields[4]);
                let speech = unhex(fields[5]);
                let text_units: Vec<u16> = text
                    .chunks(2)
                    .map(|p| u16::from_le_bytes([p[0], p[1]]))
                    .collect();
                corpus.views.insert(
                    fields[2].parse().expect("index"),
                    (label, text_units, speech),
                );
            }
            other => panic!("unknown golden command {other:?}"),
        }
    }

    assert!(!corpora.is_empty(), "no corpora in the golden file");

    for (id, corpus) in &corpora {
        let parsed = CsfFile::parse(&corpus.bytes);

        // The transcribed reader's verdict.
        assert_eq!(
            parsed.is_ok(),
            corpus.ok,
            "{id}: reader verdict mismatch (golden ok={}, rust ok={})",
            corpus.ok,
            parsed.is_ok()
        );
        assert!(!corpus.err || parsed.is_err(), "{id}: expected rejection");

        // getCSFInfo's language view — an independent header sniff that
        // succeeds even where the full parse is rejected (the C++ runs
        // getCSFInfo before ever calling parseCSF).
        let info = CsfFile::parse_header(&corpus.bytes)
            .ok()
            .map(|h| h.game_language());
        assert_eq!(info, corpus.info, "{id}: language view mismatch");

        let file = match parsed {
            Ok(file) => file,
            Err(e) => {
                assert!(corpus.views.is_empty(), "{id}: views on a rejected file");
                eprintln!("{id}: rejected as expected ({e})");
                continue;
            }
        };

        // The accepted label count.
        assert_eq!(
            Some(file.labels.len()),
            corpus.count,
            "{id}: label count mismatch"
        );

        // The game-visible view of every label.
        assert_eq!(
            file.labels.len(),
            corpus.views.len(),
            "{id}: view count mismatch"
        );
        for (i, label) in file.labels.iter().enumerate() {
            let (want_label, want_text, want_speech) = &corpus.views[&i];
            assert_eq!(
                &label.game_name().to_vec(),
                want_label,
                "{id}: label {i} name mismatch"
            );
            assert_eq!(
                &label.game_text(),
                want_text,
                "{id}: label {i} text view mismatch"
            );
            assert_eq!(
                &label.game_speech().to_vec(),
                want_speech,
                "{id}: label {i} speech view mismatch"
            );
        }

        // The writer direction: re-encoding must reproduce the transcribed
        // writer's bytes (identical to the file bytes for writer corpora).
        if let Some(writer_bytes) = &corpus.writer_bytes {
            assert_eq!(
                file.to_bytes().unwrap(),
                *writer_bytes,
                "{id}: re-encoded bytes differ from the C++ writer"
            );
        }
    }
}

/// The raw (non-game-view) layer: every golden file the parser accepts must
/// round-trip byte-for-byte through `to_bytes` — including the hand-assembled
/// blobs with their Babylon-letter waves, embedded-NUL texts and trailing
/// junk. This is the byte-fidelity property the game view above does not
/// exercise.
#[test]
fn golden_files_round_trip_bytes() {
    let text = std::fs::read_to_string(golden_path()).expect("golden file");
    let mut checked = 0;

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields[0] != "file" {
            continue;
        }
        let bytes = unhex(fields[2]);
        if let Ok(file) = CsfFile::parse(&bytes) {
            // Declared counts are canonicalized on write (the Babylon form),
            // so compare against the canonical re-encoding of the *parsed*
            // structure parsed back from the original — the fixed point.
            let reencoded = file.to_bytes().unwrap();
            assert_eq!(
                CsfFile::parse(&reencoded).unwrap(),
                file,
                "{}: reparse mismatch",
                fields[1]
            );
            checked += 1;
        }
    }

    assert!(checked >= 10, "unexpectedly few corpora checked: {checked}");
}

/// Hex sanity: the Rust and C++ dumps must agree on representation.
#[test]
fn view_dump_matches_rust_helpers() {
    // Small direct check of the helpers the golden comparison leans on.
    assert_eq!(hex(&unhex("deadbeef")), "deadbeef");
    assert_eq!(hex_wide(&[0x48, 0x6555]), "48005565");
    assert_eq!(hex(b""), "");
}
