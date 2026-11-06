// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Rust port Copyright 2026 the RustZeroHour contributors.
//
//! Differential parity tests for the map world-info payload port (P0).
//!
//! `tests/golden/mapworldinfo.txt` is a script produced by
//! `tools/differential/gen_mapworldinfo_golden.cpp` — a verbatim
//! transcription of the Zero Hour world-info load path
//! (`WorldHeightMap.cpp`'s `ParseWorldDictDataChunk` reader and the
//! constructor's logical-load registerParser/parse walk shape) over the
//! transcribed map data-chunk container (with the load-path `Dict` and
//! the name-key interning). Each corpus records its bytes and, for the
//! logical-load walk, the verdict, the final world dict (its pair count,
//! the sticky weather word and its pairs — in the interning order the
//! dict stores), the error verdicts, the final name-key interning table
//! and the post-walk stream state.
//!
//! The test replays every corpus through
//! [`rzh_formats::map_worldinfo`] with the same walk and asserts
//! line-by-line equality, then cross-checks
//! [`WorldInfoData::from_map`] against the manual walk on every corpus.
//! The pinned quirks include: the exact-type weather gate (INT sets the
//! word; BOOL, REAL, ASCII and UNICODE pairs read as absent), the
//! sticky word across chunks without the pair, the raw unvalidated
//! out-of-enum word, the version words never read (0 and 9 load like
//! 1), the empty dict, all five pair types with the duplicate-key
//! update and the interning-order storage, the NUL-truncated TOC names
//! and the unminted-id empty name, the corrupt dict type, the
//! leftover-skipping close, the under-declared chunk whose reads run
//! past its declared end, the short file's zero-filled pairs (a
//! zero-filled `weather` INT still sets the word to 0), the global
//! scope rule, the absent payload and the container-level refusals.

use std::collections::BTreeMap;
use std::path::PathBuf;

use rzh_formats::map_chunk::{
    DataChunkInfo, DataChunkInput, DictValue, MapChunkError, NameKeys, ParserTable,
};
use rzh_formats::map_worldinfo::{
    K_WORLDDICT_VERSION_1, WEATHER_KEY, WORLD_INFO_LABEL, WorldInfoData, WorldInfoError,
};

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join("mapworldinfo.txt")
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

fn hex_f32(value: f32) -> String {
    format!("{:08x}", value.to_bits())
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

/// UTF-16 units as concatenated %04x words (the generator's `hxu`).
fn hxu(units: &[u16]) -> String {
    if units.is_empty() {
        return "-".to_string();
    }
    units.iter().map(|u| format!("{u:04x}")).collect()
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

// --- the manual walk (the constructor's shape, world info only) ---------------

/// The walk's user state (the gen's `WiState`).
struct Walk {
    result: Option<WorldInfoData>,
    keys: NameKeys,
    error: Option<WorldInfoError>,
}

impl Walk {
    fn new() -> Self {
        let mut keys = NameKeys::new();
        // the well-known key seeded the way the generator's harness
        // does (WellKnownKeys.h's declaration order)
        keys.name_to_key(WEATHER_KEY);
        Walk {
            result: None,
            keys,
            error: None,
        }
    }
}

/// The registered WorldInfo parser (the gen's
/// `parseWorldDictDataChunk`, with the sticky-weather fold).
fn world_info_parser(
    file: &mut DataChunkInput<'_>,
    _info: &DataChunkInfo,
    _table: &mut ParserTable<'_, Walk>,
    user: &mut Walk,
) -> bool {
    match rzh_formats::map_worldinfo::parse_world_dict_data_chunk(file, &mut user.keys) {
        Ok(load) => {
            let weather = load
                .weather
                .or_else(|| user.result.as_ref().and_then(|r| r.weather));
            user.result = Some(WorldInfoData {
                dict: load.dict,
                weather,
            });
            true
        }
        Err(e) => {
            user.error = Some(e);
            false
        }
    }
}

/// The error line for a refused load.
fn err_line(id: &str, error: &WorldInfoError) -> String {
    match error {
        WorldInfoError::CorruptFileFormat => format!("werr {id} corrupt"),
        WorldInfoError::Dict(MapChunkError::CorruptDictType(t)) => format!("werr {id} dict {t}"),
        WorldInfoError::Dict(MapChunkError::UnclosedChunk) => format!("werr {id} dict -"),
    }
}

/// One dict pair's value as the golden grammar prints it.
fn pair_value_line(value: &DictValue) -> String {
    match value {
        DictValue::Bool(b) => format!("b {}", tf(*b)),
        DictValue::Int(i) => format!("i {i}"),
        DictValue::Real(r) => format!("r {}", hex_f32(*r)),
        DictValue::AsciiString(s) => format!("a {}", q(s)),
        DictValue::UnicodeString(u) => format!("u {}", hxu(u)),
    }
}

/// The outcome lines for one corpus's walk, plus the from_map
/// cross-check.
fn replay_corpus(id: &str, bytes: &[u8]) -> Vec<String> {
    let valid = DataChunkInput::new(bytes).is_valid_file_type();
    let mut lines = vec![format!("valid {id} {}", tf(valid))];

    let mut input = DataChunkInput::new(bytes);
    let mut table: ParserTable<'_, Walk> = ParserTable::new();
    table.register(WORLD_INFO_LABEL, b"", world_info_parser);
    let mut user = Walk::new();
    let ok = input.parse(&mut table, &mut user);

    lines.push(format!("walk {id} {}", tf(ok)));
    if !ok {
        let line = match &user.error {
            Some(e) => err_line(id, e),
            None => format!("werr {id} corrupt"),
        };
        lines.push(line);
    } else if let Some(info) = &user.result {
        lines.push(format!(
            "wdict {id} {} {} {}",
            info.dict.pair_count(),
            tf(info.weather.is_some()),
            info.weather.unwrap_or(0)
        ));
        for p in 0..info.dict.pair_count() {
            let name = user.keys.key_to_name(info.dict.nth_key(p)).to_vec();
            lines.push(format!(
                "wpair {id} {} {}",
                q(&name),
                pair_value_line(info.dict.nth_value(p))
            ));
        }
    } else {
        lines.push(format!("wnone {id}"));
    }
    for (ordinal, name) in user.keys.names() {
        lines.push(format!("wkey {id} {ordinal} {}", q(name)));
    }
    lines.push(format!(
        "wstate {id} {} {} {}",
        input.tell(),
        input.stack_depth(),
        tf(input.at_end_of_file())
    ));

    // the from_map cross-check: the module's own walk must agree with
    // the manual one.
    let from_map = WorldInfoData::from_map(bytes);
    let manual = if !ok {
        Err(user.error.unwrap_or(WorldInfoError::CorruptFileFormat))
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
fn golden_mapworldinfo_replay() {
    let corpora = parse_golden();
    assert!(
        corpora.len() >= 16,
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
fn golden_mapworldinfo_bytes_lockstep() {
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

    // one WorldInfo chunk payload: the dict pair count and pairs
    let worldinfo_payload = |pairs: &[Vec<u8>]| {
        let mut p = Vec::new();
        p.extend_from_slice(&(pairs.len() as u16).to_le_bytes());
        for pair in pairs {
            p.extend_from_slice(pair);
        }
        p
    };
    let pair_int = |id: u32, v: i32| {
        let mut p = Vec::new();
        put_i32(&mut p, ((id << 8) | 1) as i32);
        put_i32(&mut p, v);
        p
    };
    let pair_bool = |id: u32, v: bool| {
        let mut p = Vec::new();
        let kind = rzh_formats::map_chunk::dict_type::DICT_BOOL;
        put_i32(&mut p, ((id << 8) | kind as u32) as i32);
        p.push(u8::from(v));
        p
    };
    let pair_real = |id: u32, v: f32| {
        let mut p = Vec::new();
        put_i32(&mut p, ((id << 8) | 2) as i32);
        p.extend_from_slice(&v.to_le_bytes());
        p
    };
    let pair_ascii = |id: u32, s: &str| {
        let mut p = Vec::new();
        put_i32(&mut p, ((id << 8) | 3) as i32);
        p.extend_from_slice(&(s.len() as u16).to_le_bytes());
        p.extend_from_slice(s.as_bytes());
        p
    };

    // wi_v1: the modern shape
    let mut b = toc_bytes(&[
        ("WorldInfo", 1),
        ("mapHeightScale", 2),
        ("weather", 3),
        ("mapName", 4),
    ]);
    {
        let payload = worldinfo_payload(&[
            pair_ascii(4, "WatersidePark"),
            pair_bool(2, true),
            pair_int(3, 1),
        ]);
        put_chunk(&mut b, 1, K_WORLDDICT_VERSION_1, &payload);
    }
    check("wi_v1", b);

    // wi_weather_types: the exact-type gate across five chunks
    let mut b = toc_bytes(&[("WorldInfo", 1), ("weather", 2), ("sky", 3)]);
    {
        let payload = worldinfo_payload(&[pair_int(2, 0)]);
        put_chunk(&mut b, 1, K_WORLDDICT_VERSION_1, &payload);
    }
    {
        let payload = worldinfo_payload(&[pair_bool(2, true)]);
        put_chunk(&mut b, 1, K_WORLDDICT_VERSION_1, &payload);
    }
    {
        let payload = worldinfo_payload(&[pair_real(2, 1.0)]);
        put_chunk(&mut b, 1, K_WORLDDICT_VERSION_1, &payload);
    }
    {
        let payload = worldinfo_payload(&[pair_ascii(2, "1")]);
        put_chunk(&mut b, 1, K_WORLDDICT_VERSION_1, &payload);
    }
    {
        let mut pair = Vec::new();
        put_i32(&mut pair, ((2u32 << 8) | 4) as i32);
        pair.extend_from_slice(&1u16.to_le_bytes());
        pair.extend_from_slice(&0x0031u16.to_le_bytes());
        let payload = worldinfo_payload(&[pair]);
        put_chunk(&mut b, 1, K_WORLDDICT_VERSION_1, &payload);
    }
    check("wi_weather_types", b);

    // wi_sticky: the word survives a later chunk without the pair
    let mut b = toc_bytes(&[("WorldInfo", 1), ("weather", 2), ("mapName", 3)]);
    {
        let payload = worldinfo_payload(&[pair_int(2, 2)]);
        put_chunk(&mut b, 1, K_WORLDDICT_VERSION_1, &payload);
    }
    {
        let payload = worldinfo_payload(&[pair_ascii(3, "Second")]);
        put_chunk(&mut b, 1, K_WORLDDICT_VERSION_1, &payload);
    }
    check("wi_sticky", b);

    // wi_versions: the version word is never read
    let mut b = toc_bytes(&[("WorldInfo", 1), ("weather", 2), ("mapName", 3)]);
    {
        let payload = worldinfo_payload(&[pair_int(2, 1)]);
        put_chunk(&mut b, 1, 0, &payload);
    }
    {
        let payload = worldinfo_payload(&[pair_int(2, 0)]);
        put_chunk(&mut b, 1, 9, &payload);
    }
    check("wi_versions", b);

    // wi_empty_dict: the zero-pair replacement
    let mut b = toc_bytes(&[("WorldInfo", 1)]);
    {
        let payload = worldinfo_payload(&[]);
        put_chunk(&mut b, 1, K_WORLDDICT_VERSION_1, &payload);
    }
    check("wi_empty_dict", b);

    // wi_short: the cut file (the zero-filled weather INT still sets 0)
    let mut b = toc_bytes(&[("WorldInfo", 1), ("weather", 2)]);
    {
        let mut payload = worldinfo_payload(&[pair_int(2, 1), pair_bool(2, false)]);
        payload.truncate(payload.len() - 10); // cut the pairs, keep the count
        put_chunk(&mut b, 1, K_WORLDDICT_VERSION_1, &payload);
    }
    check("wi_short", b);
}
