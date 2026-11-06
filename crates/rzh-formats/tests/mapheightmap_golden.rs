// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Rust port Copyright 2026 the RustZeroHour contributors.
//
//! Differential parity tests for the map height-map payload port (P0).
//!
//! `tests/golden/mapheightmap.txt` is a script produced by
//! `tools/differential/gen_mapheightmap_golden.cpp` — a verbatim
//! transcription of the Zero Hour height-map load path
//! (`WorldHeightMap.cpp`'s `ParseHeightMapData`/`ParseSizeOnly` readers,
//! the `isValid*` size contracts and the constructor's walk shape) over
//! the transcribed map data-chunk container. Each corpus records its
//! bytes and, for both constructor load modes (full and
//! logical-data-only), the walk verdict, the loaded height map (extent,
//! border, boundaries, samples), the error verdicts and the post-walk
//! stream state — plus a pure-function matrix of the size validators.
//!
//! The test replays every corpus through [`rzh_formats::map_heightmap`]
//! with the same walks and asserts line-by-line equality, then
//! cross-checks [`HeightMapData::from_map`] against the manual walk on
//! every corpus. The pinned quirks include: the version-gated border and
//! boundary reads (a version-0 chunk reads like a 2, a version-5 like a
//! 4), the synthesised pre-version-4 boundary, the raw-int boundary
//! storage, the version-1 in-place downsample with its mode-divergent
//! `width`/`height` update (and the declared sample count neither mode
//! updates), the border ceiling on each axis (a 2-high map rejects
//! border 1), the map-size and sample-count refusals (which throw stops
//! the walk with the chunk still open), the negative boundary count
//! refusal (the original's `resize` death), the under-declared chunk
//! whose negative data-left passes the sample-count check and whose
//! samples read into the next chunk's header, the zero-filled short
//! sample tail, the leftover bytes the release close skips, the global
//! scope rule (a nested HeightMapData never fires), last-wins repeated
//! chunks, the absent payload (None), and the container-level refusals
//! (non-chunky bytes, the EOF-ending table of contents).

use std::collections::BTreeMap;
use std::path::PathBuf;

use rzh_formats::map_chunk::{DataChunkInfo, DataChunkInput, ParserTable};
use rzh_formats::map_heightmap::{
    Boundary, HEIGHT_MAP_DATA_LABEL, HeightMapData, HeightMapError, K_HEIGHT_MAP_VERSION_1,
    K_HEIGHT_MAP_VERSION_3, K_HEIGHT_MAP_VERSION_4, is_valid_playable_map_size,
    is_valid_total_map_size, parse_height_map_data, parse_size_only,
};

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join("mapheightmap.txt")
}

// --- hex helpers -------------------------------------------------------------

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
    if bytes.is_empty() {
        return "-".to_string();
    }
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

// --- byte builders (lockstep with the C++ driver) ----------------------------

fn put_u16(b: &mut Vec<u8>, v: u16) {
    b.extend_from_slice(&v.to_le_bytes());
}
fn put_u32(b: &mut Vec<u8>, v: u32) {
    b.extend_from_slice(&v.to_le_bytes());
}
fn put_i32(b: &mut Vec<u8>, v: i32) {
    b.extend_from_slice(&v.to_le_bytes());
}
fn put_str(b: &mut Vec<u8>, s: &[u8]) {
    b.extend_from_slice(s);
}
/// One chunk: id, version, size word (defaults to the data length, can
/// lie — the under-declared corpus).
fn put_i32_sized(b: &mut Vec<u8>, id: u32, ver: u16, data: &[u8], size_word: i32) {
    put_u32(b, id);
    put_u16(b, ver);
    put_i32(b, size_word);
    put_str(b, data);
}
fn put_chunk(b: &mut Vec<u8>, id: u32, ver: u16, data: &[u8]) {
    put_i32_sized(b, id, ver, data, data.len() as i32);
}
fn toc_bytes(entries: &[(&[u8], u32)]) -> Vec<u8> {
    let mut b = Vec::new();
    put_str(&mut b, b"CkMp");
    b.extend_from_slice(&(entries.len() as i32).to_le_bytes());
    for (name, id) in entries {
        b.push(name.len() as u8);
        put_str(&mut b, name);
        b.extend_from_slice(&id.to_le_bytes());
    }
    b
}
fn formula_bytes(n: usize, seed: u8) -> Vec<u8> {
    (0..n).map(|i| seed.wrapping_add((i * 7) as u8)).collect()
}

/// A version-4 payload with explicit boundaries.
fn v4_payload(w: i32, h: i32, border: i32, bnd: &[(i32, i32)], data: &[u8]) -> Vec<u8> {
    let mut p = Vec::new();
    put_i32(&mut p, w);
    put_i32(&mut p, h);
    put_i32(&mut p, border);
    put_i32(&mut p, bnd.len() as i32);
    for &(x, y) in bnd {
        put_i32(&mut p, x);
        put_i32(&mut p, y);
    }
    put_i32(&mut p, data.len() as i32);
    put_str(&mut p, data);
    p
}

/// A version-3 payload (border, synthesised boundary).
fn v3_payload(w: i32, h: i32, border: i32, data: &[u8]) -> Vec<u8> {
    let mut p = Vec::new();
    put_i32(&mut p, w);
    put_i32(&mut p, h);
    put_i32(&mut p, border);
    put_i32(&mut p, data.len() as i32);
    put_str(&mut p, data);
    p
}

/// A version-2 payload (no border word, synthesised boundary).
fn v2_payload(w: i32, h: i32, data: &[u8]) -> Vec<u8> {
    let mut p = Vec::new();
    put_i32(&mut p, w);
    put_i32(&mut p, h);
    put_i32(&mut p, data.len() as i32);
    put_str(&mut p, data);
    p
}

// --- the walk the harness drives (the constructor's shape) ---------------------

/// The walk's user state: the mode's reader result (the original's
/// `this`, cut down to the height-map half).
struct Walk {
    logical_only: bool,
    result: Option<HeightMapData>,
    error: Option<HeightMapError>,
}

/// The registered height-map parser: dispatch to the mode's reader,
/// keep the last good state, refuse the walk on a refused load.
fn hm_parser<'t>(
    file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
    _table: &mut ParserTable<'t, Walk>,
    user: &mut Walk,
) -> bool {
    let parsed = if user.logical_only {
        parse_size_only(file, info)
    } else {
        parse_height_map_data(file, info)
    };
    match parsed {
        Ok(hm) => {
            user.result = Some(hm);
            true
        }
        Err(e) => {
            user.error = Some(e);
            false
        }
    }
}

/// A recursing container observer (the real map loaders' shape), for
/// the scope corpus.
fn hm_container<'t>(
    file: &mut DataChunkInput<'_>,
    _info: &DataChunkInfo,
    table: &mut ParserTable<'t, Walk>,
    user: &mut Walk,
) -> bool {
    file.parse(table, user)
}

/// The error line for a refused load (the same fields the C++ harness
/// dumps from the surviving state at the throw).
fn err_line(id: &str, mode: &str, error: &HeightMapError) -> String {
    match error {
        HeightMapError::InvalidMapSize {
            width,
            height,
            border_size,
        } => format!("herr {id} {mode} mapsize {width} {height} {border_size}"),
        HeightMapError::InvalidHeightDataSize {
            data_size,
            width,
            height,
            size_left,
        } => format!("herr {id} {mode} datasize {data_size} {width} {height} {size_left}"),
        HeightMapError::CorruptBorderCount(n) => {
            format!("herr {id} {mode} bordercount {n}")
        }
        HeightMapError::CorruptFileFormat => format!("herr {id} {mode} corrupt"),
    }
}

/// The outcome lines for one mode's walk: the verdict, the state (or
/// the refusal), and the post-walk stream state. Also cross-checks
/// `from_map` against the manual walk (the same wiring, through the
/// module's own registration).
fn replay_mode(id: &str, mode: &str, bytes: &[u8], with_container: bool) -> Vec<String> {
    let logical_only = mode == "s";
    let mut input = DataChunkInput::new(bytes);
    let mut table: ParserTable<'_, Walk> = ParserTable::new();
    table.register(HEIGHT_MAP_DATA_LABEL, b"", hm_parser);
    if with_container {
        table.register(b"ObjectsList", b"", hm_container);
    }
    let mut user = Walk {
        logical_only,
        result: None,
        error: None,
    };
    let ok = input.parse(&mut table, &mut user);

    let mut lines = vec![format!("walk {id} {mode} {}", if ok { "t" } else { "f" })];
    if !ok {
        let line = match &user.error {
            Some(e) => err_line(id, mode, e),
            // the walk refused without a payload error: the header never
            // opened (the constructor's own throw)
            None => format!("herr {id} {mode} corrupt"),
        };
        lines.push(line);
    } else if let Some(hm) = &user.result {
        lines.push(format!(
            "hm {id} {mode} {} {} {} {} {}",
            hm.width,
            hm.height,
            hm.border_size,
            hm.data_size(),
            hm.boundaries.len()
        ));
        for (i, b) in hm.boundaries.iter().enumerate() {
            lines.push(format!("bnd {id} {mode} {i} {} {}", b.x, b.y));
        }
        lines.push(format!("hdat {id} {mode} {}", hex(&hm.data)));
    } else {
        lines.push(format!("hnone {id} {mode}"));
    }
    lines.push(format!(
        "hstate {id} {mode} {} {} {}",
        input.tell(),
        input.stack_depth(),
        if input.at_end_of_file() { "t" } else { "f" }
    ));

    // the from_map cross-check: the module's own walk must agree with
    // the manual one (verdict, payload, error).
    let from_map = HeightMapData::from_map(bytes, logical_only);
    let manual = if !ok {
        Err(user.error.unwrap_or(HeightMapError::CorruptFileFormat))
    } else {
        Ok(user.result.clone())
    };
    assert_eq!(
        from_map, manual,
        "{id}/{mode}: from_map disagreed with the manual walk"
    );

    lines
}

/// The full per-corpus replay: valid, then both mode blocks.
fn replay_corpus(id: &str, bytes: &[u8], with_container: bool) -> Vec<String> {
    let mut lines = Vec::new();
    let valid = DataChunkInput::new(bytes).is_valid_file_type();
    lines.push(format!("valid {id} {}", if valid { "t" } else { "f" }));
    lines.extend(replay_mode(id, "f", bytes, with_container));
    lines.extend(replay_mode(id, "s", bytes, with_container));
    lines
}

/// The validators matrix (the pure-function corpus).
fn replay_validators() -> Vec<String> {
    let rows: &[(&str, i32, i32, i32)] = &[
        ("total", 1, 1, 0),
        ("total", 8192, 1, 0),
        ("total", 8192, 8192, 0),
        ("total", 8193, 1, 0),
        ("total", 0, 1, 0),
        ("total", -3, 4, 0),
        ("total", 5, 5, 2),
        ("total", 5, 5, 3),
        ("total", 5, 5, -1),
        ("total", 3, 2, 1),
        ("total", 4096, 4096, 0),
        ("total", 4097, 4096, 0),
        ("total", 5000, 5000, 0),
        ("playable", 3, 3, 1),
        ("playable", 1, 1, 3),
        ("playable", 8192, 8192, 1),
        ("playable", 4096, 4096, 1),
        ("playable", 1, 1, -1),
        ("playable", 0, 1, 0),
        ("playable", 536_870_911, 536_870_911, 1_073_741_823),
    ];
    let mut lines = Vec::new();
    for &(kind, w, h, b) in rows {
        let v = if kind == "total" {
            is_valid_total_map_size(w, h, b)
        } else {
            is_valid_playable_map_size(w, h, b)
        };
        lines.push(format!(
            "val hm_validators {kind} {w} {h} {b} {}",
            if v { "t" } else { "f" }
        ));
    }
    lines
}

// --- the golden script model ----------------------------------------------------

/// The golden lines per corpus, verbatim, in emission order.
fn parse_golden() -> BTreeMap<String, Vec<String>> {
    let text = std::fs::read_to_string(golden_path()).expect("golden file");
    let mut corpora: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(3, ' ');
        let cmd = parts.next().expect("command");
        let id = parts.next().expect("corpus id");
        let rest = parts.next().unwrap_or("");
        let full = if rest.is_empty() {
            format!("{cmd} {id}")
        } else {
            format!("{cmd} {id} {rest}")
        };
        corpora.entry(id.to_string()).or_default().push(full);
    }
    corpora
}

fn corpus_bytes(lines: &[String], id: &str) -> Vec<u8> {
    lines
        .iter()
        .find_map(|l| l.strip_prefix(&format!("file {id} ")))
        .map(unhex)
        .unwrap_or_else(|| panic!("{id}: no file line"))
}

/// The corpus's replay lines, minus the `file` line (the bytes come
/// from it).
fn golden_replay_lines(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter(|l| !l.starts_with("file "))
        .cloned()
        .collect()
}

// --- the tests --------------------------------------------------------------------

/// Every corpus replays line-for-line.
#[test]
fn golden_mapheightmap_replay() {
    let corpora = parse_golden();
    assert!(
        corpora.len() >= 35,
        "expected the full corpus set, found {}",
        corpora.len()
    );

    for (id, lines) in &corpora {
        if id == "hm_validators" {
            assert_eq!(
                golden_replay_lines(lines),
                replay_validators(),
                "{id}: validator matrix drifted"
            );
            continue;
        }
        let bytes = corpus_bytes(lines, id);
        let with_container = id == "hm_nested";
        let replay = replay_corpus(id, &bytes, with_container);
        let golden = golden_replay_lines(lines);
        assert_eq!(golden, replay, "{id}: golden replay drifted");
    }
}

/// The corpus bytes rebuild through the same formula builders the C++
/// driver uses — pins the corpus construction itself (the full set; the
/// builders are cheap).
#[test]
fn golden_mapheightmap_bytes_lockstep() {
    let corpora = parse_golden();

    let check = |id: &str, built: Vec<u8>| {
        let golden = corpus_bytes(
            corpora.get(id).unwrap_or_else(|| panic!("{id} missing")),
            id,
        );
        assert_eq!(
            golden, built,
            "{id}: bytes drifted from the builder formula"
        );
    };

    let mut b = toc_bytes(&[
        (b"HeightMapData", 1),
        (b"BlendTileData", 2),
        (b"GlobalLighting", 3),
    ]);
    let mut d = Vec::new();
    put_i32(&mut d, 512);
    put_str(&mut d, &formula_bytes(12, 9));
    put_chunk(&mut b, 2, 7, &d);
    put_chunk(
        &mut b,
        1,
        K_HEIGHT_MAP_VERSION_4,
        &v4_payload(8, 6, 1, &[(6, 4), (8, 6), (7, 5)], &formula_bytes(48, 3)),
    );
    let mut d = Vec::new();
    put_i32(&mut d, 0);
    put_chunk(&mut b, 3, 2, &d);
    check("hm_v4", b);

    let mut b = toc_bytes(&[(b"HeightMapData", 1)]);
    put_chunk(
        &mut b,
        1,
        K_HEIGHT_MAP_VERSION_4,
        &v4_payload(3, 3, 0, &[], &formula_bytes(9, 5)),
    );
    check("hm_v4_empty_bnd", b);

    let mut b = toc_bytes(&[(b"HeightMapData", 1)]);
    put_chunk(
        &mut b,
        1,
        K_HEIGHT_MAP_VERSION_4,
        &v4_payload(4, 4, 0, &[(-5, 3), (0, 100000)], &formula_bytes(16, 11)),
    );
    check("hm_v4_neg_bnd_vals", b);

    let mut b = toc_bytes(&[(b"HeightMapData", 1)]);
    put_chunk(
        &mut b,
        1,
        K_HEIGHT_MAP_VERSION_3,
        &v3_payload(7, 5, 2, &formula_bytes(35, 21)),
    );
    check("hm_v3", b);

    let mut b = toc_bytes(&[(b"HeightMapData", 1)]);
    put_chunk(&mut b, 1, 2, &v2_payload(6, 4, &formula_bytes(24, 13)));
    check("hm_v2", b);

    let mut b = toc_bytes(&[(b"HeightMapData", 1)]);
    put_chunk(&mut b, 1, 0, &v2_payload(2, 2, &formula_bytes(4, 7)));
    check("hm_v0", b);

    let mut b = toc_bytes(&[(b"HeightMapData", 1)]);
    put_chunk(
        &mut b,
        1,
        5,
        &v4_payload(2, 2, 0, &[(2, 2)], &formula_bytes(4, 15)),
    );
    check("hm_v5", b);

    let mut b = toc_bytes(&[(b"HeightMapData", 1)]);
    put_chunk(
        &mut b,
        1,
        K_HEIGHT_MAP_VERSION_1,
        &v2_payload(5, 5, &formula_bytes(25, 17)),
    );
    check("hm_v1_5x5", b);

    let mut b = toc_bytes(&[(b"HeightMapData", 1)]);
    put_chunk(
        &mut b,
        1,
        K_HEIGHT_MAP_VERSION_1,
        &v2_payload(4, 4, &formula_bytes(16, 19)),
    );
    check("hm_v1_4x4", b);

    let mut b = toc_bytes(&[(b"HeightMapData", 1)]);
    put_chunk(
        &mut b,
        1,
        K_HEIGHT_MAP_VERSION_1,
        &v2_payload(1, 1, &formula_bytes(1, 23)),
    );
    check("hm_v1_1x1", b);

    let mut b = toc_bytes(&[(b"HeightMapData", 1)]);
    put_chunk(
        &mut b,
        1,
        K_HEIGHT_MAP_VERSION_1,
        &v2_payload(5, 3, &formula_bytes(15, 29)),
    );
    check("hm_v1_5x3", b);

    let mut b = toc_bytes(&[(b"HeightMapData", 1)]);
    put_chunk(
        &mut b,
        1,
        K_HEIGHT_MAP_VERSION_3,
        &v3_payload(5, 5, 2, &formula_bytes(25, 31)),
    );
    check("hm_border_max", b);

    let mut b = toc_bytes(&[(b"HeightMapData", 1)]);
    let mut p = Vec::new();
    put_i32(&mut p, 0);
    put_i32(&mut p, 4);
    put_i32(&mut p, 0);
    put_i32(&mut p, 16); // never read
    put_chunk(&mut b, 1, K_HEIGHT_MAP_VERSION_3, &p);
    check("hm_err_width0", b);

    let mut b = toc_bytes(&[(b"HeightMapData", 1)]);
    let mut p = Vec::new();
    put_i32(&mut p, -3);
    put_i32(&mut p, 4);
    put_i32(&mut p, 16);
    put_chunk(&mut b, 1, 2, &p);
    check("hm_err_width_neg", b);

    let mut b = toc_bytes(&[(b"HeightMapData", 1)]);
    let mut p = Vec::new();
    put_i32(&mut p, 8193);
    put_i32(&mut p, 1);
    put_i32(&mut p, 8193);
    put_chunk(&mut b, 1, 2, &p);
    check("hm_err_dim_over", b);

    let mut b = toc_bytes(&[(b"HeightMapData", 1)]);
    let mut p = Vec::new();
    put_i32(&mut p, 5000);
    put_i32(&mut p, 5000);
    put_i32(&mut p, 4); // never read
    put_chunk(&mut b, 1, 2, &p);
    check("hm_err_area", b);

    let mut b = toc_bytes(&[(b"HeightMapData", 1)]);
    let mut p = Vec::new();
    put_i32(&mut p, 5);
    put_i32(&mut p, 5);
    put_i32(&mut p, 3);
    put_i32(&mut p, 25);
    put_chunk(&mut b, 1, K_HEIGHT_MAP_VERSION_3, &p);
    check("hm_err_border", b);

    let mut b = toc_bytes(&[(b"HeightMapData", 1)]);
    let mut p = Vec::new();
    put_i32(&mut p, 6);
    put_i32(&mut p, 6);
    put_i32(&mut p, -1);
    put_i32(&mut p, 36);
    put_chunk(&mut b, 1, K_HEIGHT_MAP_VERSION_3, &p);
    check("hm_err_border_neg", b);

    let mut b = toc_bytes(&[(b"HeightMapData", 1)]);
    let mut p = Vec::new();
    put_i32(&mut p, 3);
    put_i32(&mut p, 2);
    put_i32(&mut p, 1);
    put_i32(&mut p, 6);
    put_chunk(&mut b, 1, K_HEIGHT_MAP_VERSION_3, &p);
    check("hm_err_border_height", b);

    let mut b = toc_bytes(&[(b"HeightMapData", 1)]);
    let mut p = Vec::new();
    put_i32(&mut p, 4);
    put_i32(&mut p, 4);
    put_i32(&mut p, 12);
    put_chunk(&mut b, 1, 2, &p);
    check("hm_err_datasize", b);

    let mut b = toc_bytes(&[(b"HeightMapData", 1)]);
    let mut p = Vec::new();
    put_i32(&mut p, 2);
    put_i32(&mut p, 2);
    put_i32(&mut p, 0);
    put_chunk(&mut b, 1, 2, &p);
    check("hm_err_datasize_zero", b);

    let mut b = toc_bytes(&[(b"HeightMapData", 1)]);
    let mut p = Vec::new();
    put_i32(&mut p, 2);
    put_i32(&mut p, 2);
    put_i32(&mut p, -4);
    put_chunk(&mut b, 1, 2, &p);
    check("hm_err_datasize_neg", b);

    let mut b = toc_bytes(&[(b"HeightMapData", 1)]);
    let mut p = Vec::new();
    put_i32(&mut p, 4);
    put_i32(&mut p, 4);
    put_i32(&mut p, 16);
    put_str(&mut p, &formula_bytes(12, 43));
    put_i32_sized(&mut b, 1, 2, &p, 24); // the lie: 12 + 12
    check("hm_err_left", b);

    let mut b = toc_bytes(&[(b"HeightMapData", 1)]);
    let mut p = Vec::new();
    put_i32(&mut p, 4);
    put_i32(&mut p, 4);
    put_i32(&mut p, 0);
    put_i32(&mut p, -2);
    put_chunk(&mut b, 1, K_HEIGHT_MAP_VERSION_4, &p);
    check("hm_err_bordercount", b);

    let mut b = toc_bytes(&[(b"HeightMapData", 1), (b"UnknownPayload", 2)]);
    let mut p = Vec::new();
    put_i32(&mut p, 4); // width
    put_i32(&mut p, 4); // height
    put_i32(&mut p, 16); // sample count (== 4*4; the check passes)
    put_str(&mut p, &formula_bytes(4, 47));
    put_i32_sized(&mut b, 1, 2, &p, 8); // declares 8, reads 28
    put_chunk(&mut b, 2, 1, &[0x11; 8]);
    check("hm_underdeclared", b);

    let mut b = toc_bytes(&[(b"HeightMapData", 1)]);
    let mut p = Vec::new();
    put_i32(&mut p, 4); // width
    put_i32(&mut p, 4); // height
    put_i32(&mut p, 0); // border
    put_i32(&mut p, 16); // sample count
    put_str(&mut p, &formula_bytes(6, 51)); // then EOF
    put_i32_sized(&mut b, 1, K_HEIGHT_MAP_VERSION_3, &p, 32); // declares 16 + 16
    check("hm_short_file", b);

    let mut b = toc_bytes(&[(b"HeightMapData", 1), (b"UnknownPayload", 2)]);
    let mut p = Vec::new();
    put_i32(&mut p, 2);
    put_i32(&mut p, 2);
    put_i32(&mut p, 0);
    put_i32(&mut p, 4);
    put_str(&mut p, &formula_bytes(4, 55));
    put_str(&mut p, &[0xee; 6]); // leftovers
    put_chunk(&mut b, 1, K_HEIGHT_MAP_VERSION_3, &p);
    put_chunk(&mut b, 2, 1, &[0x22; 4]);
    check("hm_leftover", b);

    let mut b = toc_bytes(&[(b"ObjectsList", 1), (b"HeightMapData", 2), (b"Object", 3)]);
    let mut nested = Vec::new();
    put_chunk(&mut nested, 2, 2, &v2_payload(2, 2, &formula_bytes(4, 59)));
    let mut obj = Vec::new();
    put_i32(&mut obj, 10);
    put_i32(&mut obj, 20);
    put_chunk(&mut nested, 3, 1, &obj);
    put_chunk(&mut b, 1, 3, &nested); // ObjectsList walks its children
    put_chunk(
        &mut b,
        2,
        K_HEIGHT_MAP_VERSION_3,
        &v3_payload(3, 3, 0, &formula_bytes(9, 61)),
    );
    check("hm_nested", b);

    let mut b = toc_bytes(&[(b"HeightMapData", 1)]);
    put_chunk(&mut b, 1, 2, &v2_payload(2, 2, &formula_bytes(4, 63)));
    put_chunk(
        &mut b,
        1,
        K_HEIGHT_MAP_VERSION_3,
        &v3_payload(5, 4, 1, &formula_bytes(20, 65)),
    );
    check("hm_multi", b);

    let mut b = toc_bytes(&[(b"HeightMapData", 1)]);
    put_chunk(&mut b, 1, 2, &v2_payload(2, 2, &formula_bytes(4, 67)));
    let mut p = Vec::new();
    put_i32(&mut p, 0); // width: refuses
    put_i32(&mut p, 2);
    put_i32(&mut p, 4);
    put_i32(&mut p, 0);
    put_chunk(&mut b, 1, 2, &p);
    check("hm_multi_err", b);

    let mut b = toc_bytes(&[(b"BlendTileData", 1)]);
    let mut d = Vec::new();
    put_i32(&mut d, 512);
    put_chunk(&mut b, 1, 7, &d);
    check("hm_none", b);

    let mut b = Vec::new();
    put_str(&mut b, b"not a map at all");
    check("hm_not_chunky", b);

    check("hm_toc_only", toc_bytes(&[(b"HeightMapData", 1)]));

    check("hm_empty", Vec::new());
}

/// The boundary values keep their raw ints (a quick semantic spot-check
/// on top of the line grammar).
#[test]
fn golden_payload_semantics_spotcheck() {
    let corpora = parse_golden();
    let bytes = corpus_bytes(
        corpora.get("hm_v4_neg_bnd_vals").unwrap(),
        "hm_v4_neg_bnd_vals",
    );
    let hm = HeightMapData::from_map(&bytes, false)
        .expect("loads")
        .expect("present");
    assert_eq!(
        hm.boundaries,
        vec![Boundary { x: -5, y: 3 }, Boundary { x: 0, y: 100000 }]
    );

    // the under-declared corpus: the samples read into the next chunk's
    // header (id 2, version 1, size 8), then two bytes of its data.
    let bytes = corpus_bytes(corpora.get("hm_underdeclared").unwrap(), "hm_underdeclared");
    let hm = HeightMapData::from_map(&bytes, false)
        .expect("loads")
        .expect("present");
    assert_eq!(hm.data_size(), 16);
    assert_eq!(&hm.data[..4], &formula_bytes(4, 47)[..]);
    assert_eq!(
        &hm.data[4..14],
        &[
            0x02, 0x00, 0x00, 0x00, // the UnknownPayload id
            0x01, 0x00, // its version
            0x08, 0x00, 0x00, 0x00, // its size word
        ]
    );
    assert_eq!(&hm.data[14..], &[0x11, 0x11]);
}
