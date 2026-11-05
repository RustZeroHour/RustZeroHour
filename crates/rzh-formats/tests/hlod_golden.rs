// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Rust port Copyright 2026 the RustZeroHour contributors.
//
//! Differential parity tests for the W3D HLOD-assembly port (P0).
//!
//! `tests/golden/hlod.txt` is a script produced by
//! `tools/differential/gen_hlod_golden.cpp` — a verbatim transcription
//! of the Zero Hour `HLodDefClass::Load_W3D` data path
//! (`GeneralsMD/Code/Libraries/Source/WWVegas/WW3D2/hlod.cpp` at the
//! baseline commit, plus the ProxyRecordClass/ProxyArrayClass proxy
//! path and the chunkio.cpp reader every W3D generator re-transcribes).
//!
//! The corpus bytes are the payload of the `W3D_CHUNK_HLOD` container
//! the asset manager opens before calling the loader — the same entry
//! the Rust `W3dHlod::parse` models. The script pins:
//!
//! - the load verdicts (`ok`/`err`) — the *only* failures are the
//!   first chunk and the LOD loop's open/id checks; every malformed
//!   sub-array is discarded by the walk and leaves a successful load
//!   carrying partial state, including the unbalanced-failure contrast
//!   (a stray chunk in a non-final LOD fails the load through the next
//!   iteration, in the final LOD it merely shadows the rest of the
//!   file);
//! - the decoded header — the version (never checked), the two
//!   `strdup`'d names with the run-through quirk (an unterminated Name
//!   runs into HierarchyName; both cap at the 40-byte struct), and the
//!   LodCount member (0 is a legal load);
//! - every sub-object array — the MaxScreenSize word (stored verbatim,
//!   non-finite shapes included) and, per entry, the `strdup`'d name
//!   (the zeroed stance where a failing read left the original's
//!   uninitialised heap) and the int-wrapped bone index;
//! - the aggregates (last-wins across repeated chunks) and the proxies
//!   — the array's presence, the `strlcpy`'d names (31-character
//!   truncation included), the default-constructed records a failing
//!   read leaves, and the last-wins replacement.
//!
//! The test replays the script against the Rust port and asserts
//! line-by-line equality.

use std::path::PathBuf;

use rzh_formats::w3d_hlod::{
    HLOD_HEADER_SIZE, HLOD_SUB_OBJECT_SIZE, NO_MAX_SCREEN_SIZE, W3D_CHUNK_HLOD_HEADER,
    W3D_CURRENT_HLOD_VERSION, W3dHlod, W3dSubObjectArray,
};

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join("hlod.txt")
}

fn unhex(text: &str) -> Vec<u8> {
    if text == "-" {
        return Vec::new();
    }
    assert_eq!(text.len() % 2, 0, "odd hex length in golden: {text:?}");
    let mut out = Vec::with_capacity(text.len() / 2);
    for pair in text.as_bytes().chunks(2) {
        let hi = hex_val(pair[0]);
        let lo = hex_val(pair[1]);
        out.push(hi << 4 | lo);
    }
    out
}

fn hex_val(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        _ => panic!("non-hex digit in golden: {:?}", c as char),
    }
}

fn hex_u32(value: u32) -> String {
    format!("{value:08x}")
}

fn hex_f32(value: f32) -> String {
    format!("{:08x}", value.to_bits())
}

/// Names: `-` when empty, the raw bytes when printable non-space ASCII,
/// `<hex>` otherwise (the generator's `q`).
fn q(name: &[u8]) -> String {
    if name.is_empty() {
        return "-".to_string();
    }
    if name.iter().all(|&b| (0x21..=0x7e).contains(&b)) {
        String::from_utf8_lossy(name).into_owned()
    } else {
        let mut s = String::from("<");
        for b in name {
            s.push_str(&format!("{b:02x}"));
        }
        s.push('>');
        s
    }
}

/// One sub-object array's dump lines (the generator's `arr`/`mdl`).
fn dump_array(arr: &W3dSubObjectArray, tag: &str, id: &str) -> Vec<String> {
    let mut out = vec![format!(
        "arr {id} {tag} {} {}",
        hex_f32(arr.max_screen_size),
        arr.model_count
    )];
    for j in 0..arr.model_count {
        out.push(format!(
            "mdl {id} {tag} {j} {} {}",
            q(&arr.model_names[j as usize]),
            arr.bone_indices[j as usize]
        ));
    }
    out
}

/// One expected line set of the dump, produced from the loaded def.
fn dump_hlod(hlod: &W3dHlod, id: &str) -> Vec<String> {
    let mut out = Vec::new();

    out.push(format!(
        "hdr {id} {} {} {} {}",
        hex_u32(hlod.header.version),
        q(hlod.name_str()),
        q(hlod.hierarchy_tree_name_str()),
        hlod.lod_count
    ));

    for (i, arr) in hlod.lods.iter().enumerate() {
        out.extend(dump_array(arr, &format!("l{i}"), id));
    }
    out.extend(dump_array(&hlod.aggregates, "agg", id));

    match &hlod.proxies {
        None => out.push(format!("proxies {id} -")),
        Some(records) => {
            out.push(format!("proxies {id} {}", records.len()));
            for (i, rec) in records.iter().enumerate() {
                out.push(format!(
                    "prx {id} {i} {} {}",
                    q(rec.name_str()),
                    rec.bone_index
                ));
            }
        }
    }

    out
}

#[test]
fn golden_hlod() {
    let script = std::fs::read_to_string(golden_path())
        .expect("the hlod golden script exists (run tools/differential/regenerate.sh)");
    let mut lines = script.lines().peekable();
    // the header comment
    assert_eq!(
        lines.next(),
        Some("# gen_hlod_golden output — the W3D HLOD assembly corpus (P0)")
    );
    let _ = lines.next();

    let mut corpora = 0;
    while let Some(line) = lines.next() {
        let rest = line
            .strip_prefix("file ")
            .unwrap_or_else(|| panic!("expected a file line, got: {line}"));
        let (id, hexbytes) = rest.split_once(' ').expect("file <id> <hex>");
        let bytes = unhex(hexbytes);
        corpora += 1;

        let verdict = lines
            .next()
            .unwrap_or_else(|| panic!("missing verdict for {id}"));
        // the dump lines run until the next file line
        let mut expected: Vec<String> = Vec::new();
        while let Some(peek) = lines.peek() {
            if peek.starts_with("file ") {
                break;
            }
            expected.push(lines.next().unwrap().to_string());
        }

        match W3dHlod::parse(&bytes) {
            Ok(hlod) => {
                assert_eq!(verdict, format!("ok {id}"), "corpus {id} should load");
                let actual = dump_hlod(&hlod, id);
                assert_eq!(
                    expected.len(),
                    actual.len(),
                    "corpus {id}: dump line count differs (the golden has {}, the port produced {})",
                    expected.len(),
                    actual.len()
                );
                for (want, got) in expected.iter().zip(&actual) {
                    assert_eq!(want, got, "corpus {id}: dump mismatch");
                }
            }
            Err(e) => {
                assert_eq!(
                    verdict,
                    format!("err {id}"),
                    "corpus {id} should fail (got {e:?})"
                );
                assert!(expected.is_empty(), "corpus {id}: no dump after an error");
            }
        }
    }
    assert!(corpora >= 39, "expected the full corpus set, got {corpora}");
}

/// The corpus set is self-describing enough to cross-check the framing
/// invariants the loader depends on: every `ok` corpus starts with the
/// 40-byte header chunk, and every `arr` line's model count matches the
/// `mdl` lines that follow it (the zeroed-stance tail included — the
/// count is the header's word even when the entries were never read).
#[test]
fn corpus_framing_invariants_hold() {
    let script = std::fs::read_to_string(golden_path()).expect("the hlod golden script exists");
    let mut lines = script.lines().peekable();
    let _ = lines.next();
    let _ = lines.next();

    let mut checked = 0;
    while let Some(line) = lines.next() {
        let rest = line.strip_prefix("file ").expect("a file line");
        let (id, hexbytes) = rest.split_once(' ').expect("file <id> <hex>");
        let bytes = unhex(hexbytes);

        let verdict = lines.next().expect("a verdict line");
        let mut dump: Vec<String> = Vec::new();
        while lines.peek().is_some_and(|peek| !peek.starts_with("file ")) {
            dump.push(lines.next().unwrap().to_string());
        }

        if verdict == format!("ok {id}") {
            // the header chunk: id 0x701, size at least 40 (l17 pads its
            // header chunk with 4 trailing junk bytes — Close_Chunk
            // seeks past them)
            assert_eq!(
                u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
                W3D_CHUNK_HLOD_HEADER
            );
            assert!(
                u32::from_le_bytes(bytes[4..8].try_into().unwrap()) >= HLOD_HEADER_SIZE,
                "corpus {id}: header chunk smaller than the 40-byte struct"
            );
            // every arr line's count matches its mdl lines
            let mut idx = 0;
            while idx < dump.len() {
                let line = &dump[idx];
                if let Some(rest) = line.strip_prefix("arr ") {
                    let parts: Vec<&str> = rest.split_whitespace().collect();
                    let count: i64 = parts[3].parse().unwrap();
                    let tag = parts[1];
                    let mut mdl = 0;
                    let mut j = idx + 1;
                    while j < dump.len() {
                        if let Some(mrest) = dump[j].strip_prefix("mdl ") {
                            let mparts: Vec<&str> = mrest.split_whitespace().collect();
                            if mparts[1] == tag {
                                mdl += 1;
                                j += 1;
                                continue;
                            }
                        }
                        break;
                    }
                    assert_eq!(
                        i64::from(mdl),
                        count,
                        "corpus {id}: array {tag} declares {count} models but dumps {mdl}"
                    );
                    idx = j;
                } else {
                    idx += 1;
                }
            }
            checked += 1;
        }
    }
    assert!(
        checked >= 29,
        "expected a meaningful ok corpus, got {checked}"
    );
    // the constants the loader states match the format
    assert_eq!(HLOD_HEADER_SIZE, 40);
    assert_eq!(HLOD_SUB_OBJECT_SIZE, 36);
    assert_eq!(W3D_CURRENT_HLOD_VERSION, 0x0001_0000);
    assert_eq!(NO_MAX_SCREEN_SIZE, f32::MAX);
}
