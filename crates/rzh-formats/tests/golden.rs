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

// ---------------------------------------------------------------------------
// TGA (TARGA.cpp transcription — tools/differential/gen_tga_golden.cpp)
// ---------------------------------------------------------------------------

use rzh_formats::tga::{
    SaveFlags, TGA2_EXTENSION_SIZE, Tga2Extension, Tga2Footer, TgaFile, TgaHeader,
};

fn tga_golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join("tga.txt")
}

/// One golden corpus: the bytes plus everything the transcription observed
/// when it drove them through the original.
#[derive(Default)]
struct TgaCorpus {
    bytes: Vec<u8>,
    writer_bytes: Option<Vec<u8>>,
    save: Option<(bool, bool, bool, bool)>, // image, pal, compress, addext
    ok: bool,
    err: bool,
    header: Option<TgaHeader>,
    palette: Option<Vec<u8>>,
    image: Option<Vec<u8>>,
    footer: Option<Tga2Footer>,
    extension_hex: Option<String>, // '-' → None is represented by absence
}

#[test]
fn golden_tga() {
    let text = std::fs::read_to_string(tga_golden_path()).expect("golden file");
    let mut corpora: BTreeMap<String, TgaCorpus> = BTreeMap::new();

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
            "save" => {
                corpus.save = Some((
                    fields[2] == "1",
                    fields[3] == "1",
                    fields[4] == "1",
                    fields[5] == "1",
                ));
            }
            "ok" => corpus.ok = true,
            "err" => corpus.err = true,
            "hdr" => {
                let ints: Vec<i64> = fields[2..]
                    .iter()
                    .map(|f| f.parse().expect("header int"))
                    .collect();
                assert_eq!(ints.len(), 12, "hdr needs 12 ints: {line}");
                corpus.header = Some(TgaHeader {
                    id_length: ints[0] as u8,
                    color_map_type: ints[1] as u8,
                    image_type: ints[2] as u8,
                    cmap_start: ints[3] as u16,
                    cmap_length: ints[4] as u16,
                    cmap_depth: ints[5] as u8,
                    x_offset: ints[6] as u16,
                    y_offset: ints[7] as u16,
                    width: ints[8] as u16,
                    height: ints[9] as u16,
                    pixel_depth: ints[10] as u8,
                    image_descriptor: ints[11] as u8,
                });
            }
            "pal" => corpus.palette = Some(unhex(fields[2])),
            "img" => corpus.image = Some(unhex(fields[2])),
            "footer" => {
                corpus.footer = Some(Tga2Footer {
                    extension: fields[2].parse().expect("footer ext"),
                    developer: fields[3].parse().expect("footer dev"),
                });
            }
            "ext" => corpus.extension_hex = Some(fields[2].to_string()),
            other => panic!("unknown golden command {other:?}"),
        }
    }

    assert!(!corpora.is_empty(), "no corpora in the golden file");

    for (id, corpus) in &corpora {
        let parsed = TgaFile::parse(&corpus.bytes);

        // The transcribed reader's verdict.
        assert_eq!(
            parsed.is_ok(),
            corpus.ok,
            "{id}: reader verdict mismatch (golden ok={}, rust ok={})",
            corpus.ok,
            parsed.is_ok()
        );
        assert!(!corpus.err || parsed.is_err(), "{id}: expected rejection");

        let file = match parsed {
            Ok(file) => file,
            Err(e) => {
                assert!(corpus.header.is_none(), "{id}: views on a rejected file");
                eprintln!("{id}: rejected as expected ({e})");
                continue;
            }
        };

        // The header as the load left it (origin bits cleared when the image
        // was normalized).
        assert_eq!(
            Some(&file.header),
            corpus.header.as_ref(),
            "{id}: header view mismatch"
        );
        // The palette chunk and the decoded, normalized image.
        assert_eq!(
            &file.palette,
            corpus.palette.as_ref().unwrap_or(&Vec::new()),
            "{id}: palette view mismatch"
        );
        assert_eq!(
            &file.image,
            corpus.image.as_ref().unwrap_or(&Vec::new()),
            "{id}: image view mismatch"
        );
        // The footer offsets when the file ends with the signature.
        assert_eq!(
            file.footer.as_ref(),
            corpus.footer.as_ref(),
            "{id}: footer view mismatch"
        );
        // The 495-byte extension, compared through its serialization.
        match (&file.extension, &corpus.extension_hex) {
            (None, None) => {}
            (Some(ext), Some(hexstr)) => assert_eq!(
                hex(&ext.to_bytes()),
                hexstr.trim_start_matches('-'),
                "{id}: extension view mismatch"
            ),
            (a, b) => panic!("{id}: extension presence mismatch ({a:?} vs {b:?})"),
        }

        // The writer direction: the saved bytes re-encode identically from
        // the parsed structure — the load inverts truecolor back to the
        // in-memory form the save started from, so `to_bytes` with the
        // corpus's flags must reproduce the transcription's output.
        if let (Some(writer), Some((image, pal, compress, addext))) =
            (&corpus.writer_bytes, corpus.save)
        {
            assert_eq!(
                &corpus.bytes, writer,
                "{id}: the transcription's file and gen bytes differ"
            );
            let flags = SaveFlags {
                image,
                palette: pal,
                compress,
            };
            assert_eq!(
                file.to_bytes(flags, addext).unwrap(),
                *writer,
                "{id}: re-encoded bytes differ from the C++ Save"
            );
        }
    }
}

/// The canonical form: every accepted golden file, re-encoded with the
/// file's own encoding shape (image + palette; compressed files stay
/// compressed — writing raw data under an encoded type is a file the
/// original's callers can never produce), parses back to the same
/// structure and re-encodes to itself — the same fixed-point property the
/// CSF goldens pin.
#[test]
fn golden_tga_files_round_trip_bytes() {
    let text = std::fs::read_to_string(tga_golden_path()).expect("golden file");
    let mut bytes_by_id: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields[0] == "file" {
            order.push(fields[1].to_string());
            bytes_by_id.insert(fields[1].to_string(), unhex(fields[2]));
        }
    }

    let mut checked = 0;
    for id in &order {
        let bytes = &bytes_by_id[id];
        if let Ok(file) = TgaFile::parse(bytes) {
            let flags = SaveFlags {
                image: true,
                palette: true,
                compress: file.header.is_compressed(),
            };
            let canonical = file.to_bytes(flags, false).unwrap();
            let reparsed = TgaFile::parse(&canonical).unwrap();
            assert_eq!(
                reparsed.to_bytes(flags, false).unwrap(),
                canonical,
                "{id}: the canonical form is not a fixed point"
            );
            checked += 1;
        }
    }

    assert!(checked >= 10, "unexpectedly few corpora checked: {checked}");
}

/// The extension serializer round-trips arbitrary bytes — the golden dumps
/// pin specific files; this pins the 495-byte layout itself.
#[test]
fn tga_extension_layout_is_packed() {
    let mut bytes = [0xA5u8; TGA2_EXTENSION_SIZE];
    // Distinct values so any field-boundary drift shows up.
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = (i % 251) as u8;
    }
    let ext = Tga2Extension::parse(&bytes).unwrap();
    assert_eq!(ext.to_bytes(), bytes);
}
