// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Rust port Copyright 2026 the RustZeroHour contributors.
//
//! Differential parity tests for the map sides-list payload port (P0).
//!
//! `tests/golden/mapsides.txt` is a script produced by
//! `tools/differential/gen_mapsides_golden.cpp` — a verbatim
//! transcription of the Zero Hour sides-list load path
//! (`SidesList.cpp`'s `ParseSidesDataChunk` reader with its bounded
//! side loop, build lists and team list, the nested PlayerScriptsList
//! walk boundary, and `validateSides` with its whole cleanup family,
//! over the constructor's logical-load registerParser/parse walk
//! shape) over the transcribed map data-chunk container (with the
//! load-path `Dict`, its typed getters and the name-key interning).
//! Each corpus records its bytes and, for the logical-load walk, the
//! verdict, the final sides and teams (each dict's pairs in the
//! interning order, each build entry's full post-load state), the
//! modified flag, the error verdicts, the final name-key interning
//! table and the post-walk stream state.
//!
//! The test replays every corpus through
//! [`rzh_formats::map_sideslist`] with the same walk and asserts
//! line-by-line equality, then cross-checks
//! [`SidesListData::from_map`] against the manual walk on every
//! corpus. The pinned quirks include: the version family (0 through 4
//! — the build tail from 3, the team list from 2), the build entry's
//! ground-forced z and the int-to-unsigned rebuild wrap, the v2
//! defaults the gates leave (script empty, health 100, whiner true,
//! unsellable false, repairable true), the validateSides family (the
//! neutral ensure with its L"Neutral" display name and "team" default,
//! the default-team ensure with the owner fix and singleton force, the
//! allies/enemies token cleanup with the single-space rejoin, the
//! team-name collision removal, the orphan/self-owner reparent), the
//! 16-side bound (the mid-side break whose garbage team count reads the
//! 17th side's dict words, the dropped neutral whose default team
//! still lands), the negative counts, the repeated chunks, the corrupt
//! dict types (a side's and a team's), the short file's zero-filled
//! pairs, the nested scripts chunk skipped whole, the leftover-skipping
//! close, the absent payload and the container-level refusals.

use std::collections::BTreeMap;
use std::path::PathBuf;

use rzh_formats::map_chunk::{
    DataChunkInfo, DataChunkInput, DictValue, MapChunkError, NameKeys, ParserTable,
};
use rzh_formats::map_sideslist::{
    K_SIDES_DATA_VERSION_3, PLAYER_ALLIES_KEY, PLAYER_DISPLAY_NAME_KEY, PLAYER_ENEMIES_KEY,
    PLAYER_FACTION_KEY, PLAYER_IS_HUMAN_KEY, PLAYER_NAME_KEY, SIDES_LIST_LABEL, SidesError,
    SidesListData, TEAM_IS_SINGLETON_KEY, TEAM_NAME_KEY, TEAM_OWNER_KEY, parse_sides_data_chunk,
};

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join("mapsides.txt")
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

// --- the manual walk (the constructor's shape, sides only) -------------------

/// The walk's user state (the gen's `SdState`).
struct Walk {
    result: Option<SidesListData>,
    keys: NameKeys,
    error: Option<SidesError>,
}

impl Walk {
    fn new() -> Self {
        let mut keys = NameKeys::new();
        // the nine well-known keys seeded the way the generator's
        // harness does (WellKnownKeys.h's declaration order)
        for key in [
            TEAM_NAME_KEY,
            TEAM_OWNER_KEY,
            TEAM_IS_SINGLETON_KEY,
            PLAYER_NAME_KEY,
            PLAYER_IS_HUMAN_KEY,
            PLAYER_DISPLAY_NAME_KEY,
            PLAYER_FACTION_KEY,
            PLAYER_ENEMIES_KEY,
            PLAYER_ALLIES_KEY,
        ] {
            keys.name_to_key(key);
        }
        Walk {
            result: None,
            keys,
            error: None,
        }
    }
}

/// The registered SidesList parser (the gen's `parseSidesDataChunk`).
fn sides_parser(
    file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
    _table: &mut ParserTable<'_, Walk>,
    user: &mut Walk,
) -> bool {
    match parse_sides_data_chunk(file, info, &mut user.keys) {
        Ok(data) => {
            user.result = Some(data);
            true
        }
        Err(e) => {
            user.error = Some(e);
            false
        }
    }
}

/// The error line for a refused load.
fn err_line(id: &str, error: &SidesError) -> String {
    match error {
        SidesError::CorruptFileFormat => format!("serr {id} corrupt"),
        SidesError::Dict(MapChunkError::CorruptDictType(t)) => format!("serr {id} dict {t}"),
        SidesError::Dict(MapChunkError::UnclosedChunk) => format!("serr {id} dict -"),
        SidesError::TeamRemovalLoop => format!("serr {id} teamloop"),
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

/// The dict dump lines for one dict (the gen's `dump_dict`).
fn dump_dict(
    lines: &mut Vec<String>,
    cmd: &str,
    id: &str,
    index: usize,
    dict: &rzh_formats::map_chunk::Dict,
    keys: &NameKeys,
) {
    for p in 0..dict.pair_count() {
        let name = keys.key_to_name(dict.nth_key(p)).to_vec();
        lines.push(format!(
            "{cmd} {id} {index} {} {}",
            q(&name),
            pair_value_line(dict.nth_value(p))
        ));
    }
}

/// The outcome lines for one corpus's walk, plus the from_map
/// cross-check.
fn replay_corpus(id: &str, bytes: &[u8]) -> Vec<String> {
    let valid = DataChunkInput::new(bytes).is_valid_file_type();
    let mut lines = vec![format!("valid {id} {}", tf(valid))];

    let mut input = DataChunkInput::new(bytes);
    let mut table: ParserTable<'_, Walk> = ParserTable::new();
    table.register(SIDES_LIST_LABEL, b"", sides_parser);
    let mut user = Walk::new();
    let ok = input.parse(&mut table, &mut user);

    lines.push(format!("walk {id} {}", tf(ok)));
    if !ok {
        let line = match &user.error {
            Some(e) => err_line(id, e),
            None => format!("serr {id} corrupt"),
        };
        lines.push(line);
    } else if let Some(data) = &user.result {
        lines.push(format!("sn {id} {} {}", data.sides.len(), data.teams.len()));
        for (i, side) in data.sides.iter().enumerate() {
            lines.push(format!(
                "sside {id} {i} {} {}",
                side.dict.pair_count(),
                side.build_list.len()
            ));
            dump_dict(&mut lines, "spair", id, i, &side.dict, &user.keys);
            for (j, b) in side.build_list.iter().enumerate() {
                lines.push(format!(
                    "sbuild {id} {i} {j} {} {} {} {} {} {} {} {} {} {} {} {} {}",
                    q(&b.building_name),
                    q(&b.template_name),
                    hex_f32(b.location.x),
                    hex_f32(b.location.y),
                    hex_f32(b.location.z),
                    hex_f32(b.angle),
                    tf(b.initially_built),
                    // the gen dumps (Int)m_numRebuilds — the unsigned
                    // word cast back through the int
                    b.num_rebuilds as i32,
                    q(&b.script),
                    b.health,
                    tf(b.whiner),
                    tf(b.unsellable),
                    tf(b.repairable),
                ));
            }
        }
        for (i, team) in data.teams.iter().enumerate() {
            lines.push(format!("steamn {id} {i} {}", team.pair_count()));
            dump_dict(&mut lines, "stpair", id, i, team, &user.keys);
        }
        lines.push(format!("smod {id} {}", tf(data.modified)));
    } else {
        lines.push(format!("snone {id}"));
    }
    for (ordinal, name) in user.keys.names() {
        lines.push(format!("skey {id} {ordinal} {}", q(name)));
    }
    lines.push(format!(
        "sstate {id} {} {} {}",
        input.tell(),
        input.stack_depth(),
        tf(input.at_end_of_file())
    ));

    // the from_map cross-check: the module's own walk must agree with
    // the manual one.
    let from_map = SidesListData::from_map(bytes);
    let manual = if !ok {
        Err(user.error.unwrap_or(SidesError::CorruptFileFormat))
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
fn golden_mapsides_replay() {
    let corpora = parse_golden();
    assert!(
        corpora.len() >= 24,
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
fn golden_mapsides_bytes_lockstep() {
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

    // the standard sides TOC (the gen's `sides_toc`)
    let sides_toc = || {
        toc_bytes(&[
            ("SidesList", 1),
            ("playerName", 2),
            ("playerIsHuman", 3),
            ("playerDisplayName", 4),
            ("playerFaction", 5),
            ("playerEnemies", 6),
            ("playerAllies", 7),
            ("teamName", 8),
            ("teamOwner", 9),
            ("teamIsSingleton", 10),
            ("playerMoney", 11),
        ])
    };

    // sd_v1: version 1, one side, no team list
    let mut b = sides_toc();
    {
        let mut p = Vec::new();
        put_i32(&mut p, 1);
        // the dict: playerName "Old", playerIsHuman true, faction, empty
        // allies/enemies (the gen's player_pairs)
        let mut pairs = Vec::new();
        let pair = |key_id: u32, v: &[u8]| {
            let mut p = Vec::new();
            put_i32(&mut p, ((key_id << 8) | 3) as i32);
            p.extend_from_slice(&(v.len() as u16).to_le_bytes());
            p.extend_from_slice(v);
            p
        };
        let pairb = |key_id: u32, v: bool| {
            let mut p = Vec::new();
            let kind = u32::from(rzh_formats::map_chunk::dict_type::DICT_BOOL);
            put_i32(&mut p, ((key_id << 8) | kind) as i32);
            p.push(u8::from(v));
            p
        };
        pairs.push(pair(2, b"Old"));
        pairs.push(pairb(3, true));
        pairs.push(pair(5, b"FactionOld"));
        pairs.push(pair(7, b""));
        pairs.push(pair(6, b""));
        p.extend_from_slice(&(pairs.len() as u16).to_le_bytes());
        for pr in &pairs {
            p.extend_from_slice(pr);
        }
        put_i32(&mut p, 0); // the build count
        put_chunk(&mut b, 1, 1, &p);
    }
    check("sd_v1", b);

    // sd_negative: the two negative counts
    let mut b = sides_toc();
    {
        let mut p = Vec::new();
        put_i32(&mut p, -3);
        put_i32(&mut p, -5);
        put_chunk(&mut b, 1, K_SIDES_DATA_VERSION_3, &p);
    }
    check("sd_negative", b);

    // sd_teamloop→sd_solo_collision: the single pre-existing team
    let mut b = sides_toc();
    {
        let mut p = Vec::new();
        put_i32(&mut p, 2);
        {
            let pairs = vec![(2u32, b"".to_vec())];
            let mut sp = Vec::new();
            sp.extend_from_slice(&(1u16).to_le_bytes());
            for (key_id, v) in &pairs {
                put_i32(&mut sp, ((*key_id << 8) | 3) as i32);
                sp.extend_from_slice(&(v.len() as u16).to_le_bytes());
                sp.extend_from_slice(v);
            }
            put_i32(&mut sp, 0); // build count
            p.extend_from_slice(&sp);
        }
        {
            let mut sp = Vec::new();
            sp.extend_from_slice(&5u16.to_le_bytes());
            // playerName Solo, human, faction, allies, enemies
            for (key_id, v, kind) in [
                (2u32, b"Solo".to_vec(), 3u32),
                (3, b"\x01".to_vec(), 0),
                (5, b"FactionX".to_vec(), 3),
                (7, b"".to_vec(), 3),
                (6, b"".to_vec(), 3),
            ] {
                put_i32(&mut sp, ((key_id << 8) | kind) as i32);
                if kind == 3 {
                    sp.extend_from_slice(&(v.len() as u16).to_le_bytes());
                    sp.extend_from_slice(&v);
                } else {
                    sp.push(v[0]);
                }
            }
            put_i32(&mut sp, 0); // build count
            p.extend_from_slice(&sp);
        }
        put_i32(&mut p, 1); // one team
        {
            let mut tp = Vec::new();
            tp.extend_from_slice(&3u16.to_le_bytes());
            for (key_id, v) in [(8u32, b"Solo".to_vec()), (9, b"Solo".to_vec())] {
                put_i32(&mut tp, ((key_id << 8) | 3) as i32);
                tp.extend_from_slice(&(v.len() as u16).to_le_bytes());
                tp.extend_from_slice(&v);
            }
            let bool_kind = u32::from(rzh_formats::map_chunk::dict_type::DICT_BOOL);
            put_i32(&mut tp, ((10u32 << 8) | bool_kind) as i32);
            tp.push(1);
            p.extend_from_slice(&tp);
        }
        put_chunk(&mut b, 1, K_SIDES_DATA_VERSION_3, &p);
    }
    check("sd_solo_collision", b);
}
