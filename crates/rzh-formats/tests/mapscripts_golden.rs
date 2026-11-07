// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Rust port Copyright 2026 the RustZeroHour contributors.
//
//! Differential parity tests for the script-engine load-path port (P0).
//!
//! `tests/golden/mapscripts.txt` is a script produced by
//! `tools/differential/gen_mapscripts_golden.cpp` — a verbatim
//! transcription of the Zero Hour script-engine load path (the nested
//! `PlayerScriptsList` walk: `ScriptList::ParseScriptsDataChunk` with
//! its statics, the per-player lists, the groups, the scripts, the
//! or-chains of and-conditions, the THEN/ELSE action lists and every
//! parameter, over the template registry `ScriptEngine::init()` builds
//! with the name-key interning) over the transcribed map data-chunk
//! container. Each corpus records its bytes and, for the walk, the
//! verdict, the handed-out lists (every script's fields, every
//! condition's and action's type and parameters with the heal results),
//! the error verdicts, the registry dump, the final name-key interning
//! table and the post-walk stream state.
//!
//! The test replays every corpus through [`rzh_formats::map_scripts`]
//! with the same walk (the root registration, the map's SidesList-tail
//! registration, or none — the corpus table below carries each mode)
//! and asserts line-by-line equality, then cross-checks
//! [`PlayerScriptsData::from_map`] / [`from_map_in_sides`] against the
//! manual walk on every corpus. The pinned quirks include: the version
//! family (the group's exact version-2 equality, the script's
//! version-2 delay, the condition's version-4 name key and version-1
//! heal, the action's version-2 name key, the never-read
//! PlayerScriptsList/ScriptList versions), the name-key rematch (the
//! repair by name, the unknown-name fallbacks, the empty-name key
//! rematching to the defunct templates, the raw-type survivors the
//! template-0 clamp allows), the heal family (the area-condition
//! append with its null tails, the special-power SIDE prepends, the
//! flank switch, the attitude re-type, the camera pads), the parameter
//! rewrites (the Fundamentalist rename with the strlcat truncation,
//! the upgrade trio, the object-status bit with the NONE quirk, the
//! kindof walk with the legacy remaps and the MISSILE split), the
//! count-check fallbacks (CONDITION_FALSE / NO_OP), the 16-list bound
//! and its 17th-list refusal, the repeated-chunk last-wins clear, the
//! scoping skips, the leftovers and short reads, and the verdict
//! family (the out-of-bounds parameter store, the heal append, the
//! kindof index, the unknown-kindof throw).

use std::collections::BTreeMap;
use std::path::PathBuf;

use rzh_formats::map_chunk::{DataChunkInfo, DataChunkInput, ParserTable};
use rzh_formats::map_scripts::{
    ACTION_TEMPLATE_NAMES, ACTION_TEMPLATE_PARAMS, CONDITION_TEMPLATE_NAMES,
    CONDITION_TEMPLATE_PARAMS, PT_COORD3D, PT_OBJECT_STATUS, PlayerScriptsData,
    ScriptEngineTemplates, ScriptsError, ScriptsWalk, scripts_chunk_parser,
};

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join("mapscripts.txt")
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

// --- the manual walk (the generator's driver shape) --------------------------

/// How the corpus's file registers the reader: the root scope (the
/// `.scb`/GameLogic shape), the map's SidesList tail, or nothing.
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Root,
    Nested,
    None,
}

/// The corpus id → registration mode table (the generator's RunOpts).
fn corpus_mode(id: &str) -> Mode {
    match id {
        "pscr_nested" => Mode::Nested,
        "pscr_unregistered" => Mode::None,
        _ => Mode::Root,
    }
}

/// Whether the corpus dumps the full name-key table (the generator's
/// dump_keys) — the interning-sensitive corpora.
fn dumps_keys(id: &str) -> bool {
    matches!(
        id,
        "pscr_registry" | "pscr_full" | "pscr_rematch" | "pscr_nested"
    )
}

/// The walk's outcome lines for one corpus (the generator's run_ps),
/// plus the from_map cross-check.
fn replay_corpus(id: &str, bytes: &[u8]) -> Vec<String> {
    let mode = corpus_mode(id);
    let valid = DataChunkInput::new(bytes).is_valid_file_type();
    let mut lines = vec![format!("valid {id} {}", tf(valid))];

    let mut input = DataChunkInput::new(bytes);
    let mut table: ParserTable<'_, ScriptsWalk> = ParserTable::new();
    match mode {
        Mode::Root => table.register(b"PlayerScriptsList", b"", scripts_chunk_parser),
        Mode::Nested => {
            // the generator's sides_stub: register PlayerScriptsList
            // under the SidesList label, inside a SidesList-rooted
            // parser
            table.register(b"SidesList", b"", sides_stub_parser);
        }
        Mode::None => {}
    }
    let mut user = ScriptsWalk::new();
    let mut taken: Option<Vec<rzh_formats::map_scripts::ScriptListData>> = None;
    let ok = input.parse(&mut table, &mut user);

    lines.push(format!("walk {id} {}", tf(ok)));
    if !ok {
        let token = match user.error() {
            Some(ScriptsError::KindOfUnknown) => "kindof",
            Some(ScriptsError::ParameterStoreOob) => "oob-parms",
            Some(ScriptsError::HealStoreOob) => "oob-heal",
            Some(ScriptsError::KindOfIndexOob) => "oob-kindof",
            Some(ScriptsError::CorruptFileFormat) => "corrupt",
            Some(ScriptsError::TooManyLists) => "listbound",
            None => {
                if valid {
                    "listbound"
                } else {
                    "corrupt"
                }
            }
        };
        lines.push(format!("perr {id} {token}"));
    } else if !user.fired() {
        lines.push(format!("pnone {id}"));
    } else {
        let lists = user.take_read_scripts();
        taken = Some(lists.clone());
        lines.push(format!("pn {id} {}", lists.len()));
        for (li, list) in lists.iter().enumerate() {
            lines.push(format!(
                "plist {id} {li} {} {}",
                list.scripts.len(),
                list.groups.len()
            ));
            for (si, script) in list.scripts.iter().enumerate() {
                dump_script(&mut lines, id, &format!("L{li}.s{si}"), script);
            }
            for (gi, group) in list.groups.iter().enumerate() {
                lines.push(format!(
                    "pgrp {id} {li} {gi} {} {} {} {}",
                    q(&group.name),
                    tf(group.is_active),
                    tf(group.is_subroutine),
                    group.scripts.len()
                ));
                for (si, script) in group.scripts.iter().enumerate() {
                    dump_script(&mut lines, id, &format!("L{li}.g{gi}.s{si}"), script);
                }
            }
        }
    }

    // the registry dump (the pscr_registry corpus)
    if id == "pscr_registry" {
        for (i, name) in CONDITION_TEMPLATE_NAMES.iter().enumerate() {
            lines.push(format!(
                "ptpl c {id} {i} {} {}",
                q(name.as_bytes()),
                CONDITION_TEMPLATE_PARAMS[i]
            ));
        }
        for (i, name) in ACTION_TEMPLATE_NAMES.iter().enumerate() {
            lines.push(format!(
                "ptpl a {id} {i} {} {}",
                q(name.as_bytes()),
                ACTION_TEMPLATE_PARAMS[i]
            ));
        }
    }

    // the final name-key table: the count always, the full sequence for
    // the interning-sensitive corpora
    lines.push(format!("pkeyn {id} {}", user.keys().names().count()));
    if dumps_keys(id) {
        for (ordinal, name) in user.keys().names() {
            lines.push(format!("pkey {id} {ordinal} {}", q(name)));
        }
    }
    lines.push(format!(
        "pstate {id} {} {} {}",
        input.tell(),
        input.stack_depth(),
        tf(input.at_end_of_file())
    ));

    // the from_map cross-check: the module's own walk must agree with
    // the manual one (the unregistered corpus has no module walk — its
    // chunks skip whole under every registration).
    if mode != Mode::None {
        let from_map = if mode == Mode::Nested {
            PlayerScriptsData::from_map_in_sides(bytes)
        } else {
            PlayerScriptsData::from_map(bytes)
        };
        let manual = if !ok {
            Err(user.error().unwrap_or(if valid {
                ScriptsError::TooManyLists
            } else {
                ScriptsError::CorruptFileFormat
            }))
        } else if user.fired() {
            Ok(Some(PlayerScriptsData {
                lists: taken.expect("the fired walk took its lists"),
            }))
        } else {
            Ok(None)
        };
        match (&from_map, &manual) {
            (Err(e1), Err(e2)) => assert_eq!(e1, e2, "{id}: from_map error differed"),
            (Ok(a), Ok(b)) => assert_eq!(a, b, "{id}: from_map disagreed with the manual walk"),
            _ => panic!("{id}: from_map disagreed with the manual walk (shape)"),
        }
    }

    lines
}

fn dump_script(
    lines: &mut Vec<String>,
    id: &str,
    path: &str,
    script: &rzh_formats::map_scripts::ScriptData,
) {
    lines.push(format!(
        "pscr {id} {path} {} {} {} {} {} {} {} {} {} {} {}",
        q(&script.name),
        q(&script.comment),
        q(&script.condition_comment),
        q(&script.action_comment),
        tf(script.is_active),
        tf(script.is_one_shot),
        tf(script.easy),
        tf(script.normal),
        tf(script.hard),
        tf(script.is_subroutine),
        script.delay_evaluation_seconds,
    ));
    for (o, or) in script.or_conditions.iter().enumerate() {
        for (c, cond) in or.and_conditions.iter().enumerate() {
            let cpath = format!("{path}.o{o}.c{c}");
            lines.push(format!(
                "pcond {id} {cpath} {} {}",
                cond.condition_type,
                cond.params.len()
            ));
            dump_params(lines, id, &cpath, &cond.params);
        }
    }
    for (a, act) in script.actions.iter().enumerate() {
        let apath = format!("{path}.a{a}");
        lines.push(format!(
            "pact {id} {apath} t {} {}",
            act.action_type,
            act.params.len()
        ));
        dump_params(lines, id, &apath, &act.params);
    }
    for (a, act) in script.false_actions.iter().enumerate() {
        let apath = format!("{path}.f{a}");
        lines.push(format!(
            "pact {id} {apath} f {} {}",
            act.action_type,
            act.params.len()
        ));
        dump_params(lines, id, &apath, &act.params);
    }
}

fn dump_params(
    lines: &mut Vec<String>,
    id: &str,
    path: &str,
    params: &[Option<rzh_formats::map_scripts::ParameterData>],
) {
    for (pi, parm) in params.iter().enumerate() {
        match parm {
            None => lines.push(format!("pparm {id} {path}.p{pi} {pi} null")),
            Some(p) => {
                let mut s = format!(
                    "pparm {id} {path}.p{pi} {pi} {} {} {} {}",
                    p.param_type,
                    p.int_value,
                    hex_f32(p.real_value),
                    q(&p.string_value)
                );
                if p.param_type == PT_COORD3D {
                    s.push_str(&format!(
                        " {} {} {}",
                        hex_f32(p.coord.x),
                        hex_f32(p.coord.y),
                        hex_f32(p.coord.z)
                    ));
                }
                if p.param_type == PT_OBJECT_STATUS {
                    s.push_str(&format!(" {:016x}", p.object_status));
                }
                lines.push(s);
            }
        }
    }
}

/// The SidesList-tail stub the nested mode registers at the root: the
/// generator's `sides_stub` (register PlayerScriptsList under this
/// chunk's label, then parse).
fn sides_stub_parser(
    file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
    table: &mut ParserTable<'_, ScriptsWalk>,
    user: &mut ScriptsWalk,
) -> bool {
    table.register(
        b"PlayerScriptsList",
        info.label.as_slice(),
        scripts_chunk_parser,
    );
    file.parse(table, user)
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
        // the ptpl lines carry a kind token before the corpus id
        // (`ptpl c <id> <i> ...`); every other command is
        // `<cmd> <id> ...`
        let (id, full) = if line.starts_with("ptpl ") {
            let mut r = line.splitn(4, ' ');
            let _cmd = r.next().expect("cmd");
            let _kind = r.next().expect("ptpl kind");
            let id = r.next().expect("ptpl corpus id").to_string();
            (id, line.to_string())
        } else {
            let mut parts = line.splitn(3, ' ');
            let cmd = parts.next().expect("command");
            let id = parts.next().expect("corpus id").to_string();
            let rest = parts.next().unwrap_or("");
            let full = if rest.is_empty() {
                format!("{cmd} {id}")
            } else {
                format!("{cmd} {id} {rest}")
            };
            (id, full)
        };
        let entry = corpora.entry(id).or_default();
        entry.push(full);
    }
    corpora
}

#[test]
fn golden_scripts_replay_line_by_line() {
    let golden = parse_golden();
    assert!(!golden.is_empty(), "the golden file had no corpora");

    for (id, lines) in &golden {
        // the corpus's bytes come from its `file` line
        let file_line = lines
            .iter()
            .find(|l| l.starts_with("file "))
            .unwrap_or_else(|| panic!("{id}: no file line"));
        let hex = file_line.split(' ').nth(2).expect("hex");
        let bytes = unhex(hex);

        let replayed = replay_corpus(id, &bytes);

        // the golden's non-file lines must match the replay exactly
        let expected: Vec<&String> = lines.iter().filter(|l| !l.starts_with("file ")).collect();
        assert_eq!(
            replayed.len(),
            expected.len(),
            "{id}: line count differed (golden {} vs replay {})\ngolden tail: {expected:#?}\nreplay tail: {replayed:#?}",
            expected.len(),
            replayed.len()
        );
        for (want, got) in expected.iter().zip(replayed.iter()) {
            assert_eq!(want.as_str(), got.as_str(), "{id}: a line differed");
        }
    }
}

/// The port's registry interning is byte-identical to the generator's
/// (the pscr_registry corpus pins the table; this is the direct shape
/// check).
#[test]
fn the_registry_interning_sequence_transcribes() {
    let engine = ScriptEngineTemplates::new();
    // conditions first, then actions; the empty string interns once at
    // the first defunct condition (OBSOLETE_SCRIPT_1, index 58)
    let names: Vec<(u32, &[u8])> = engine.keys().names().collect();
    assert_eq!(names.len(), 447);
    assert_eq!(names[58].1, b""); // the empty-string key
    assert_eq!(names[0].1, b"CONDITION_FALSE");
    // the named conditions take the first 104 slots (103 names + the
    // empty string), then the named actions follow
    assert_eq!(names[104].1, b"DEBUG_MESSAGE_BOX");
    let last = names[446].1;
    assert_eq!(last, ACTION_TEMPLATE_NAMES[343].as_bytes());
}
