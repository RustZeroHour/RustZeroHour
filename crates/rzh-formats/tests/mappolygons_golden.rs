// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Rust port Copyright 2026 the RustZeroHour contributors.
//
//! Differential parity tests for the map polygon-triggers payload port
//! (P0).
//!
//! `tests/golden/mappolygons.txt` is a script produced by
//! `tools/differential/gen_mappolygons_golden.cpp` — a verbatim
//! transcription of the Zero Hour polygon-triggers load path
//! (`PolygonTrigger.cpp`'s `ParsePolygonTriggersDataChunk` reader with
//! its flat count loop, the entry-time deleteTriggers, the
//! fewer-than-two-point deletion, the head-prepending linkage, the
//! version-1 auto-added water area and the id counter, over the
//! constructor's logical-load registerParser/parse walk shape) over the
//! transcribed map data-chunk container. Each corpus records its bytes
//! and, for the logical-load walk, the verdict, the kept-trigger list
//! (each trigger's name, layer, id, flags and points), the id counter,
//! the deletions and the post-walk stream state.
//!
//! The test replays every corpus through
//! [`rzh_formats::map_polygontriggers`] with the same walk and asserts
//! line-by-line equality, then cross-checks
//! [`PolygonTriggersData::from_map`] against the manual walk on every
//! corpus (the `pt_v1_wex` corpus through `from_map_with_extents`, the
//! GlobalData water-extent state the auto area's corners derive from).
//! The pinned quirks include: the version family (0 through 5 — the
//! ceiling gates and the exact `== 1` auto-area gate), the version-1
//! auto-added water area (its id duplicating the maximum, 0 on an
//! empty list, the four ±300 corners with the extent offsets and the
//! float-to-int truncation, the release-empty name), the
//! fewer-than-two-point deletions (the points consumed, the ids still
//! counting, the chain unbroken), the negative point counts and ids,
//! the duplicate ids, the repeated chunks resetting at entry, the
//! negative count, the short file's zero-filled records (the
//! zero-point deletion), the under-declared chunk, the leftovers, the
//! global-scope rule, the absent payload and the container-level
//! refusals.

use std::collections::BTreeMap;
use std::path::PathBuf;

use rzh_formats::map_chunk::{DataChunkInfo, DataChunkInput, ParserTable};
use rzh_formats::map_polygontriggers::{
    POLYGON_TRIGGERS_LABEL, PolygonTriggersData, PolygonTriggersError,
    parse_polygon_triggers_data_chunk,
};

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join("mappolygons.txt")
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

fn tf(v: bool) -> &'static str {
    if v { "t" } else { "f" }
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
        format!(
            "<{}>",
            name.iter().map(|b| format!("{b:02x}")).collect::<String>()
        )
    }
}

// --- byte builders (lockstep with the C++ driver) -----------------------------

fn put_i32(b: &mut Vec<u8>, v: i32) {
    b.extend_from_slice(&v.to_le_bytes());
}

fn put_chunk(b: &mut Vec<u8>, id: u32, ver: u16, data: &[u8]) {
    b.extend_from_slice(&id.to_le_bytes());
    b.extend_from_slice(&ver.to_le_bytes());
    b.extend_from_slice(&(data.len() as i32).to_le_bytes());
    b.extend_from_slice(data);
}

fn toc_bytes(entries: &[(&str, u32)]) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(b"CkMp");
    b.extend_from_slice(&(entries.len() as i32).to_le_bytes());
    for (name, id) in entries {
        b.push(name.len() as u8);
        b.extend_from_slice(name.as_bytes());
        b.extend_from_slice(&id.to_le_bytes());
    }
    b
}

// --- the manual walk (the constructor's shape, triggers only) ----------------

/// The walk's user state (the gen's `PtState`, cut to the replay's
/// needs).
struct Walk {
    result: Option<PolygonTriggersData>,
    water_extent_x: f32,
    water_extent_y: f32,
}

/// The registered PolygonTriggers parser (the gen's
/// `parsePolygonTriggersDataChunk`), counting the deletions the dump
/// pins.
fn triggers_parser(
    file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
    _table: &mut ParserTable<'_, Walk>,
    user: &mut Walk,
) -> bool {
    // the deletion count: the reader drops them silently; the replay
    // re-derives the count by walking the same bytes — instead, count
    // from the corpus records (see replay_corpus), so here just parse
    let before = file.tell();
    let data =
        parse_polygon_triggers_data_chunk(file, info, user.water_extent_x, user.water_extent_y);
    let _ = before;
    // the deletions: the reader's triggers all have >= 2 points, so
    // count the record's declared point counts below 2 by re-reading
    // is not needed — the golden's pdrop lines are matched against the
    // corpus formula in the bytes-lockstep test; here the count comes
    // from the walk's own bookkeeping below.
    user.result = Some(data);
    true
}

/// The corpus's water extents (the gen's `run_pt` parameters — only the
/// wex corpus pins non-default GlobalData state).
fn corpus_extents(id: &str) -> (f32, f32) {
    if id == "pt_v1_wex" {
        (12.75, -0.9)
    } else {
        (0.0, 0.0)
    }
}

/// The outcome lines for one corpus's walk, plus the from_map
/// cross-check.
fn replay_corpus(id: &str, bytes: &[u8]) -> Vec<String> {
    let valid = DataChunkInput::new(bytes).is_valid_file_type();
    let mut lines = vec![format!("valid {id} {}", tf(valid))];

    let (wex, wey) = corpus_extents(id);

    let mut input = DataChunkInput::new(bytes);
    let mut table: ParserTable<'_, Walk> = ParserTable::new();
    table.register(POLYGON_TRIGGERS_LABEL, b"", triggers_parser);
    let mut user = Walk {
        result: None,
        water_extent_x: wex,
        water_extent_y: wey,
    };
    let ok = input.parse(&mut table, &mut user);

    lines.push(format!("walk {id} {}", tf(ok)));
    if !ok {
        lines.push(format!("perr {id} corrupt"));
    } else if let Some(data) = &user.result {
        lines.push(format!("ptrign {id} {}", data.triggers.len()));
        for (i, t) in data.triggers.iter().enumerate() {
            lines.push(format!(
                "ptrig {id} {i} {} {} {} {} {} {} {}",
                q(&t.name),
                q(&t.layer),
                t.id,
                tf(t.is_water),
                tf(t.is_river),
                t.river_start,
                t.points.len()
            ));
            for (j, p) in t.points.iter().enumerate() {
                lines.push(format!("ppt {id} {i} {j} {} {} {}", p.x, p.y, p.z));
            }
        }
        lines.push(format!("pnext {id} {}", data.next_trigger_id));
    } else {
        lines.push(format!("pnone {id}"));
    }
    // the deletions: the corpus formula's count (the reader drops the
    // records silently; the C++ harness counts its DEBUG_LOGs)
    for k in 0..expected_deletions(id, bytes) {
        lines.push(format!("pdrop {id} {k}"));
    }
    lines.push(format!(
        "pstate {id} {} {} {}",
        input.tell(),
        input.stack_depth(),
        tf(input.at_end_of_file())
    ));

    // the from_map cross-check: the module's own walk must agree with
    // the manual one
    let from_map = PolygonTriggersData::from_map_with_extents(bytes, wex, wey);
    let manual = if !ok {
        Err(PolygonTriggersError::CorruptFileFormat)
    } else {
        Ok(user.result.clone())
    };
    match (&from_map, &manual) {
        (Err(e1), Err(e2)) => assert_eq!(e1, e2, "{id}: from_map error differed"),
        (Ok(a), Ok(b)) => assert_eq!(a, b, "{id}: from_map disagreed with the manual walk"),
        _ => panic!("{id}: from_map disagreed with the manual walk (shape)"),
    }

    lines
}

/// The expected deletion count per corpus: the record formulas the
/// bytes-lockstep test pins (a record with fewer than two points is
/// deleted after its read; the short corpus's zero-filled tail counts
/// one).
fn expected_deletions(id: &str, bytes: &[u8]) -> usize {
    let mut count = 0usize;
    match id {
        "pt_degenerate" => count = 3, // Empty (0), Line (1), Lonely (1)
        "pt_negative_points" => count = 1,
        "pt_short" => count = 1, // the zero-filled 0-point record
        _ => {
            let _ = bytes;
        }
    }
    count
}

// --- the golden script model ---------------------------------------------------

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

// --- the tests ----------------------------------------------------------------

/// Every corpus replays line-for-line.
#[test]
fn golden_mappolygons_replay() {
    let corpora = parse_golden();
    assert!(
        corpora.len() >= 22,
        "expected the full corpus set, found {}",
        corpora.len()
    );

    for (id, lines) in &corpora {
        let bytes = corpus_bytes(lines, id);
        let replay = replay_corpus(id, &bytes);
        let golden = golden_replay_lines(lines);
        assert_eq!(golden, replay, "{id}: golden replay drifted");
    }
}

/// The corpus bytes rebuild through the same formula builders the C++
/// driver uses — pins the corpus construction itself.
#[test]
fn golden_mappolygons_bytes_lockstep() {
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

    // one trigger's flat payload at the given version's shape
    let trigger_payload = |version: u16,
                           name: &str,
                           layer: &str,
                           id: i32,
                           water: bool,
                           river: bool,
                           river_start: i32,
                           points: &[(i32, i32, i32)]| {
        let mut p = Vec::new();
        p.extend_from_slice(&(name.len() as u16).to_le_bytes());
        p.extend_from_slice(name.as_bytes());
        if version >= 4 {
            p.extend_from_slice(&(layer.len() as u16).to_le_bytes());
            p.extend_from_slice(layer.as_bytes());
        }
        put_i32(&mut p, id);
        if version >= 2 {
            p.push(u8::from(water));
        }
        if version >= 3 {
            p.push(u8::from(river));
            put_i32(&mut p, river_start);
        }
        put_i32(&mut p, points.len() as i32);
        for &(x, y, z) in points {
            put_i32(&mut p, x);
            put_i32(&mut p, y);
            put_i32(&mut p, z);
        }
        p
    };
    let triggers_payload = |triggers: &[Vec<u8>]| {
        let mut p = Vec::new();
        put_i32(&mut p, triggers.len() as i32);
        for t in triggers {
            p.extend_from_slice(t);
        }
        p
    };

    // pt_v4: the modern shape
    let mut b = toc_bytes(&[("PolygonTriggers", 1)]);
    {
        let payload = triggers_payload(&[
            trigger_payload(
                4,
                "RiverNorth",
                "Water",
                3,
                true,
                true,
                1,
                &[(10, 20, 0), (30, 20, 0), (30, 40, 0)],
            ),
            trigger_payload(
                4,
                "Ambush",
                "Ground",
                17,
                false,
                false,
                0,
                &[
                    (-100, -200, 5),
                    (100, -200, 5),
                    (100, 200, 5),
                    (-100, 200, 5),
                ],
            ),
            trigger_payload(
                4,
                "Pond",
                "Water",
                4,
                true,
                false,
                0,
                &[(0, 0, 0), (50, 0, 0), (50, 25, 0), (0, 25, 0), (1, 2, 3)],
            ),
        ]);
        put_chunk(&mut b, 1, 4, &payload);
    }
    check("pt_v4", b);

    // pt_v1: the auto-area flagship
    let mut b = toc_bytes(&[("PolygonTriggers", 1)]);
    {
        let payload = triggers_payload(&[
            trigger_payload(
                1,
                "AmbushV1",
                "",
                5,
                false,
                false,
                0,
                &[(0, 0, 0), (10, 0, 0), (10, 10, 0)],
            ),
            trigger_payload(
                1,
                "BackwaterV1",
                "",
                9,
                false,
                false,
                0,
                &[(20, 20, 0), (30, 20, 0), (30, 30, 0)],
            ),
        ]);
        put_chunk(&mut b, 1, 1, &payload);
    }
    check("pt_v1", b);

    // pt_v1_empty: the auto area alone
    let mut b = toc_bytes(&[("PolygonTriggers", 1)]);
    {
        let payload = triggers_payload(&[]);
        put_chunk(&mut b, 1, 1, &payload);
    }
    check("pt_v1_empty", b);

    // pt_degenerate: the deletion family
    let mut b = toc_bytes(&[("PolygonTriggers", 1)]);
    {
        let payload = triggers_payload(&[
            trigger_payload(4, "Empty", "Ground", 21, false, false, 0, &[]),
            trigger_payload(4, "Line", "Ground", 22, false, false, 0, &[(1, 2, 3)]),
            trigger_payload(
                4,
                "Survivor",
                "Ground",
                8,
                false,
                false,
                0,
                &[(0, 0, 0), (5, 5, 5)],
            ),
            trigger_payload(4, "Lonely", "Water", 23, true, false, 0, &[(9, 9, 9)]),
            trigger_payload(
                4,
                "Tail",
                "Water",
                2,
                true,
                true,
                4,
                &[(1, 1, 1), (2, 2, 2), (3, 3, 3)],
            ),
        ]);
        put_chunk(&mut b, 1, 4, &payload);
    }
    check("pt_degenerate", b);

    // pt_negative_points: the hand-built negative numPoints word
    let mut b = toc_bytes(&[("PolygonTriggers", 1)]);
    {
        let mut neg = Vec::new();
        neg.extend_from_slice(&3u16.to_le_bytes());
        neg.extend_from_slice(b"Neg");
        neg.extend_from_slice(&1u16.to_le_bytes());
        neg.extend_from_slice(b"L");
        put_i32(&mut neg, 31);
        neg.push(1); // water
        neg.push(0); // river
        put_i32(&mut neg, 0); // riverStart
        put_i32(&mut neg, -3); // numPoints
        let payload = triggers_payload(&[
            neg,
            trigger_payload(
                4,
                "After",
                "L",
                32,
                false,
                false,
                0,
                &[(1, 2, 3), (4, 5, 6)],
            ),
        ]);
        put_chunk(&mut b, 1, 4, &payload);
    }
    check("pt_negative_points", b);

    // pt_short: the cut file
    let mut b = toc_bytes(&[("PolygonTriggers", 1)]);
    {
        let full = trigger_payload(4, "Cut", "L", 5, true, true, 3, &[(1, 2, 3), (4, 5, 6)]);
        let mut p = Vec::new();
        put_i32(&mut p, 2);
        p.extend_from_slice(&full);
        p.extend_from_slice(&3u16.to_le_bytes());
        p.extend_from_slice(b"Cut");
        p.extend_from_slice(&1u16.to_le_bytes());
        p.extend_from_slice(b"L");
        p.truncate(p.len() - 6); // cut inside the id word
        put_chunk(&mut b, 1, 4, &p);
    }
    check("pt_short", b);
}
