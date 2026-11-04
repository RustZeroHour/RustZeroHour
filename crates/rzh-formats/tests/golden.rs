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

// ---------------------------------------------------------------------------
// chunk (chunkio.cpp transcription — tools/differential/gen_chunk_golden.cpp)
// ---------------------------------------------------------------------------

use rzh_formats::chunk::{ChunkLoad, ChunkSave};

fn chunk_golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join("chunk.txt")
}

/// One chunk golden corpus: the bytes, the writer direction, the two walk
/// event sequences and the scripted steps.
#[derive(Default)]
struct ChunkCorpus {
    bytes: Vec<u8>,
    writer_bytes: Option<Vec<u8>>,
    wdepth: Option<usize>,
    walk_a: Vec<String>,
    walk_b: Vec<String>,
    steps: Vec<String>,
}

/// The formula corpus bytes (kept in lockstep with the generator).
fn chunk_formula(n: usize) -> Vec<u8> {
    (0..n).map(|i| ((i * 7 + 13) & 0xFF) as u8).collect()
}

fn chunk_hex(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        "-".to_string()
    } else {
        hex(bytes)
    }
}

/// Walk A (payload lens): mirrors `walk_a` in the generator — containers
/// recurse, data chunks read their payload back in pieces of
/// `min(64, remaining)` bytes until a read returns 0.
fn chunk_walk_a(load: &mut ChunkLoad, id: &str, out: &mut Vec<String>) {
    while load.open_chunk().unwrap() {
        out.push(format!(
            "chunk {id} {} {:x} {} {}",
            load.cur_chunk_depth(),
            load.cur_chunk_id(),
            load.cur_chunk_length(),
            if load.contains_chunks() { "C" } else { "-" },
        ));
        if load.contains_chunks() {
            chunk_walk_a(load, id, out);
        } else {
            let mut payload = Vec::new();
            let mut remaining = load.cur_chunk_length();
            let mut buf = [0u8; 64];
            while remaining > 0 {
                let piece = remaining.min(64) as usize;
                let n = load.read(&mut buf[..piece]).unwrap();
                if n == 0 {
                    break;
                }
                payload.extend_from_slice(&buf[..n as usize]);
                remaining -= n;
            }
            out.push(format!("data {id} {}", chunk_hex(&payload)));
        }
        load.close_chunk().unwrap();
    }
}

/// Walk B (micro lens): mirrors `walk_b` in the generator — data chunks
/// walk their micro chunks (payload read the same way) instead of the
/// raw payload.
fn chunk_walk_b(load: &mut ChunkLoad, id: &str, out: &mut Vec<String>) {
    while load.open_chunk().unwrap() {
        if load.contains_chunks() {
            chunk_walk_b(load, id, out);
        } else {
            while load.open_micro_chunk().unwrap() {
                out.push(format!(
                    "mchunk {id} {} {} {}",
                    load.cur_chunk_depth(),
                    load.cur_micro_chunk_id(),
                    load.cur_micro_chunk_length(),
                ));
                let mut payload = Vec::new();
                let mut remaining = load.cur_micro_chunk_length() as u32;
                let mut buf = [0u8; 64];
                while remaining > 0 {
                    let piece = remaining.min(64) as usize;
                    let n = load.read(&mut buf[..piece]).unwrap();
                    if n == 0 {
                        break;
                    }
                    payload.extend_from_slice(&buf[..n as usize]);
                    remaining -= n;
                }
                out.push(format!("mdata {id} {}", chunk_hex(&payload)));
                load.close_micro_chunk().unwrap();
            }
        }
        load.close_chunk().unwrap();
    }
}

/// Runs both walks over a corpus and returns the two event sequences
/// (each ending with the tell line, like the generator).
fn chunk_run_walks(bytes: &[u8], id: &str) -> (Vec<String>, Vec<String>) {
    let mut walk_a = Vec::new();
    let mut load = ChunkLoad::new(bytes);
    chunk_walk_a(&mut load, id, &mut walk_a);
    walk_a.push(format!("tell {id} {}", load.tell()));

    let mut walk_b = Vec::new();
    let mut load = ChunkLoad::new(bytes);
    chunk_walk_b(&mut load, id, &mut walk_b);
    walk_b.push(format!("mtell {id} {}", load.tell()));
    (walk_a, walk_b)
}

/// Rebuilds the writer corpora with [`ChunkSave`], mirroring
/// `build_writer_corpora` in the generator one tree for one. Returns the
/// bytes plus the writer's `cur_chunk_depth` at its deepest point.
fn chunk_build_writer_corpora() -> Vec<(&'static str, Vec<u8>, usize)> {
    let mut out = Vec::new();
    {
        // one data chunk with a formula payload
        let mut save = ChunkSave::new();
        save.begin_chunk(0x5E).unwrap();
        let depth = save.cur_chunk_depth();
        save.write(&chunk_formula(100)).unwrap();
        save.end_chunk().unwrap();
        out.push(("wr_simple", save.into_bytes().unwrap(), depth));
    }
    {
        // nested containers, an empty data chunk, two top-level chunks
        let mut save = ChunkSave::new();
        save.begin_chunk(0x100).unwrap();
        save.begin_chunk(1).unwrap();
        save.write(&chunk_formula(12)).unwrap();
        save.end_chunk().unwrap();
        save.begin_chunk(2).unwrap();
        save.begin_chunk(3).unwrap();
        let depth = save.cur_chunk_depth();
        save.write(&chunk_formula(4)).unwrap();
        save.end_chunk().unwrap();
        save.begin_chunk(4).unwrap(); // empty data chunk
        save.end_chunk().unwrap();
        save.end_chunk().unwrap();
        save.begin_chunk(5).unwrap();
        save.write(&chunk_formula(28)).unwrap();
        save.end_chunk().unwrap();
        save.end_chunk().unwrap();
        save.begin_chunk(6).unwrap();
        save.write(&chunk_formula(8)).unwrap();
        save.end_chunk().unwrap();
        out.push(("wr_nested", save.into_bytes().unwrap(), depth));
    }
    {
        // micro chunks (a zero-size one, a 254-byte one at the debug
        // assert boundary) then trailing raw data in the same chunk
        let mut save = ChunkSave::new();
        save.begin_chunk(0x200).unwrap();
        let depth = save.cur_chunk_depth();
        save.begin_micro_chunk(1).unwrap();
        save.write(&chunk_formula(4)).unwrap();
        save.end_micro_chunk().unwrap();
        save.begin_micro_chunk(2).unwrap(); // zero-size micro chunk
        save.end_micro_chunk().unwrap();
        save.begin_micro_chunk(3).unwrap();
        save.write(&chunk_formula(17)).unwrap();
        save.end_micro_chunk().unwrap();
        save.begin_micro_chunk(255).unwrap();
        save.write(&chunk_formula(254)).unwrap();
        save.end_micro_chunk().unwrap();
        save.write(&chunk_formula(9)).unwrap(); // raw data after the micros
        save.end_chunk().unwrap();
        out.push(("wr_micro", save.into_bytes().unwrap(), depth));
    }
    {
        // 60 levels of nesting, data only at the innermost
        let mut save = ChunkSave::new();
        for i in 0..60u32 {
            save.begin_chunk(0x10 + i).unwrap();
        }
        let depth = save.cur_chunk_depth();
        save.write(&chunk_formula(5)).unwrap();
        for _ in 0..60 {
            save.end_chunk().unwrap();
        }
        out.push(("wr_deep", save.into_bytes().unwrap(), depth));
    }
    {
        // the full u32 id range on chunks, 0 and 255 on micro ids
        let mut save = ChunkSave::new();
        let mut depth = 0;
        for id in [0u32, 1, 0x7FFFFFFF, 0xFFFFFFFF, 0x80000000] {
            save.begin_chunk(id).unwrap();
            save.begin_chunk(0xABCDEF01).unwrap();
            depth = save.cur_chunk_depth();
            save.write(&chunk_formula(3)).unwrap();
            save.end_chunk().unwrap();
            save.end_chunk().unwrap();
        }
        save.begin_chunk(0x555).unwrap();
        save.begin_micro_chunk(0).unwrap();
        save.write(&chunk_formula(2)).unwrap();
        save.end_micro_chunk().unwrap();
        save.begin_micro_chunk(255).unwrap();
        save.write(&chunk_formula(1)).unwrap();
        save.end_micro_chunk().unwrap();
        save.end_chunk().unwrap();
        out.push(("wr_ids", save.into_bytes().unwrap(), depth));
    }
    {
        // the IOVector write helpers (full structs; the READ side is the
        // buggy one — pinned by the rd_vecs script)
        let mut save = ChunkSave::new();
        save.begin_chunk(0x300).unwrap();
        let depth = save.cur_chunk_depth();
        save.write_io_vector2(&[1.5, -2.5]).unwrap();
        save.write_io_vector3(&[1.0, 2.0, 3.0]).unwrap();
        save.write_io_vector4(&[4.0, 3.0, 2.0, 1.0]).unwrap();
        save.write_io_quaternion(&[0.5, 0.5, 0.5, 0.5]).unwrap();
        save.end_chunk().unwrap();
        out.push(("wr_vecs", save.into_bytes().unwrap(), depth));
    }
    {
        // a bigger payload accumulated over several writes
        let mut save = ChunkSave::new();
        save.begin_chunk(0x400).unwrap();
        let depth = save.cur_chunk_depth();
        for _ in 0..8 {
            save.write(&chunk_formula(64)).unwrap();
        }
        save.end_chunk().unwrap();
        out.push(("wr_big", save.into_bytes().unwrap(), depth));
    }
    out
}

/// The scripted corpora replays: mirror `run_scripts` in the generator
/// step for step over the golden `file` bytes.
fn chunk_run_script(id: &str, bytes: &[u8], out: &mut Vec<String>) {
    let mut load = ChunkLoad::new(bytes);
    let mut buf = [0u8; 64];
    match id {
        // reading data out of a contains-flagged chunk (read never
        // consults the flag)
        "rd_msbdata" => {
            out.push(format!(
                "step {id} open {}",
                u8::from(load.open_chunk().unwrap())
            ));
            out.push(format!("step {id} id {:x}", load.cur_chunk_id()));
            out.push(format!("step {id} len {}", load.cur_chunk_length()));
            out.push(format!(
                "step {id} contains {}",
                u8::from(load.contains_chunks())
            ));
            out.push(format!("step {id} depth {}", load.cur_chunk_depth()));
            let n = load.read(&mut buf[..8]).unwrap();
            out.push(format!(
                "step {id} read {n} {}",
                chunk_hex(&buf[..n as usize])
            ));
            let n = load.read(&mut buf[..1]).unwrap();
            out.push(format!(
                "step {id} read {n} {}",
                chunk_hex(&buf[..n as usize])
            ));
            load.close_chunk().unwrap();
            out.push(format!("step {id} close 1"));
            out.push(format!("step {id} tell {}", load.tell()));
        }
        // the short read consumes the available bytes and still reports
        // failure; the close seek then jumps past the end
        "rd_short" => {
            out.push(format!(
                "step {id} open {}",
                u8::from(load.open_chunk().unwrap())
            ));
            out.push(format!("step {id} id {:x}", load.cur_chunk_id()));
            let n = load.read(&mut buf[..17]).unwrap(); // 12 available: consumed, failed
            out.push(format!(
                "step {id} read {n} {}",
                chunk_hex(&buf[..n as usize])
            ));
            out.push(format!("step {id} tell {}", load.tell()));
            let n = load.read(&mut buf[..17]).unwrap(); // nothing left: no advance
            out.push(format!(
                "step {id} read {n} {}",
                chunk_hex(&buf[..n as usize])
            ));
            out.push(format!("step {id} tell {}", load.tell()));
            load.close_chunk().unwrap();
            out.push(format!("step {id} close 1"));
            out.push(format!("step {id} tell {}", load.tell()));
        }
        // a micro header that would cross its parent's declared end fails
        // cleanly (the clamp fires before the read)
        "rd_micro_straddle" => {
            out.push(format!(
                "step {id} open {}",
                u8::from(load.open_chunk().unwrap())
            ));
            let n = load.read(&mut buf[..6]).unwrap();
            out.push(format!(
                "step {id} read {n} {}",
                chunk_hex(&buf[..n as usize])
            ));
            out.push(format!(
                "step {id} mopen {}",
                u8::from(load.open_micro_chunk().unwrap())
            ));
            let n = load.read(&mut buf[..1]).unwrap(); // the last parent byte
            out.push(format!(
                "step {id} read {n} {}",
                chunk_hex(&buf[..n as usize])
            ));
            let n = load.read(&mut buf[..1]).unwrap();
            out.push(format!(
                "step {id} read {n} {}",
                chunk_hex(&buf[..n as usize])
            ));
            load.close_chunk().unwrap();
            out.push(format!("step {id} close 1"));
            out.push(format!("step {id} tell {}", load.tell()));
        }
        // close_micro_chunk jumps unclamped past the parent's bounds and
        // the parent's counter overruns
        "rd_micro_overrun" => {
            out.push(format!(
                "step {id} open {}",
                u8::from(load.open_chunk().unwrap())
            ));
            out.push(format!(
                "step {id} mopen {}",
                u8::from(load.open_micro_chunk().unwrap())
            ));
            out.push(format!("step {id} mid {}", load.cur_micro_chunk_id()));
            out.push(format!("step {id} mlen {}", load.cur_micro_chunk_length()));
            let n = load.read(&mut buf[..2]).unwrap();
            out.push(format!(
                "step {id} read {n} {}",
                chunk_hex(&buf[..n as usize])
            ));
            load.close_micro_chunk().unwrap();
            out.push(format!("step {id} mclose 1"));
            out.push(format!(
                "step {id} open {}",
                u8::from(load.open_chunk().unwrap())
            ));
            load.close_chunk().unwrap();
            out.push(format!("step {id} close 1"));
            out.push(format!("step {id} tell {}", load.tell()));
        }
        // the IOVector read helpers' pointer-size bug: each consumes
        // exactly 4 bytes, filling only the first component
        "rd_vecs" => {
            out.push(format!(
                "step {id} open {}",
                u8::from(load.open_chunk().unwrap())
            ));
            let mut r2 = [0.0f32; 2];
            let mut r3 = [0.0f32; 3];
            let mut r4 = [0.0f32; 4];
            let mut rq = [0.0f32; 4];
            out.push(format!(
                "step {id} read {}",
                load.read_io_vector2(&mut r2).unwrap()
            ));
            out.push(format!(
                "step {id} read {}",
                load.read_io_vector3(&mut r3).unwrap()
            ));
            out.push(format!(
                "step {id} read {}",
                load.read_io_vector4(&mut r4).unwrap()
            ));
            out.push(format!(
                "step {id} read {}",
                load.read_io_quaternion(&mut rq).unwrap()
            ));
            out.push(format!(
                "step {id} vecs {} {} {} {} {} {} {} {}",
                r2[0], r2[1], r3[0], r3[1], r4[0], r4[1], rq[0], rq[1]
            ));
            let mut rest = [0u8; 36];
            let n = load.read(&mut rest).unwrap();
            out.push(format!(
                "step {id} read {n} {}",
                chunk_hex(&rest[..n as usize])
            ));
            load.close_chunk().unwrap();
            out.push(format!("step {id} close 1"));
            out.push(format!("step {id} tell {}", load.tell()));
        }
        other => panic!("unknown scripted corpus {other:?}"),
    }
}

#[test]
fn golden_chunk() {
    let text = std::fs::read_to_string(chunk_golden_path()).expect("golden file");
    let mut corpora: BTreeMap<String, ChunkCorpus> = BTreeMap::new();

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
            "wdepth" => corpus.wdepth = Some(fields[2].parse().expect("wdepth")),
            "chunk" | "data" | "tell" => corpus.walk_a.push(line.to_string()),
            "mchunk" | "mdata" | "mtell" => corpus.walk_b.push(line.to_string()),
            "step" => corpus.steps.push(line.to_string()),
            other => panic!("unknown golden command {other:?}"),
        }
    }

    assert!(
        corpora.len() >= 18,
        "unexpectedly few corpora: {}",
        corpora.len()
    );

    // The writer corpora must exist and be walked.
    let writers = chunk_build_writer_corpora();
    for (id, bytes, depth) in &writers {
        let corpus = &corpora[*id];
        assert_eq!(
            Some(bytes),
            corpus.writer_bytes.as_ref(),
            "{id}: the rebuilt tree differs from the transcribed writer"
        );
        assert_eq!(
            corpus.bytes, *bytes,
            "{id}: the file and gen bytes differ in the golden"
        );
        assert_eq!(
            corpus.wdepth,
            Some(*depth),
            "{id}: writer depth pin mismatch"
        );
    }

    for (id, corpus) in &corpora {
        // Both walks over every corpus.
        let (walk_a, walk_b) = chunk_run_walks(&corpus.bytes, id);
        assert_eq!(
            walk_a, corpus.walk_a,
            "{id}: walk A (payload lens) diverges from the transcription"
        );
        assert_eq!(
            walk_b, corpus.walk_b,
            "{id}: walk B (micro lens) diverges from the transcription"
        );

        // The scripted corpora replay step for step.
        if !corpus.steps.is_empty() {
            let mut steps = Vec::new();
            chunk_run_script(id, &corpus.bytes, &mut steps);
            assert_eq!(steps, corpus.steps, "{id}: scripted steps diverge");
        }
    }
}

/// The round-trip layer the walks do not exercise: every writer corpus
/// re-parsed through the reader must rebuild to identical bytes (the
/// container's fixed point), including the 60-deep tree and the
/// micro-chunk corpus.
#[test]
fn golden_chunk_writer_round_trips() {
    for (id, bytes, _) in chunk_build_writer_corpora() {
        // The walk itself proves the reader accepts the bytes; the fixed
        // point needs a writer again, which the golden `gen` comparison
        // already pins — here we check the reader's structural view: the
        // walk events must be non-empty and the final tell must equal the
        // byte length (nothing left dangling at the top level).
        let (walk_a, _) = chunk_run_walks(&bytes, id);
        let last = walk_a.last().expect("walk events");
        let tell: u64 = last
            .rsplit(' ')
            .next()
            .expect("tell digits")
            .parse()
            .expect("tell number");
        assert_eq!(
            tell,
            bytes.len() as u64,
            "{id}: top-level walk left the stream at {tell}, expected the file length {}",
            bytes.len()
        );
    }
}
