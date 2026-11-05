// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Rust port Copyright 2026 the RustZeroHour contributors.
//
//! Differential parity tests for the map data-chunk container port (P0).
//!
//! `tests/golden/mapchunk.txt` is a script produced by
//! `tools/differential/gen_mapchunk_golden.cpp` — a verbatim transcription
//! of the Zero Hour map framing layer (`Common/System/DataChunk.cpp`: the
//! `CkMp` table of contents, `DataChunkInput`'s parser walk and primitive
//! readers, `DataChunkOutput`'s patch-in-place writer, the load-path half
//! of `Dict` and the name-key interning). Each corpus records its bytes,
//! the construction verdicts, the table of contents, a scripted sequence
//! of observed steps (parser callbacks and manual walks), and — for the
//! writer corpora — the emitted bytes and TOC.
//!
//! The test replays every corpus through [`rzh_formats::map_chunk`] with
//! the same hardcoded scripts and asserts line-by-line equality. The
//! pinned quirks include: the count>0-but-EOF header refusal, the
//! duplicate-name/duplicate-id TOC lookups (last read wins), the NUL-
//! truncated names, the zero-filled short reads (a phantom chunk from a
//! truncated header, the empty-name dict pair), the signed data-left
//! accounting (negative on over-declared children and parent-overrun
//! opens, no seek on close), the parse loop's 4-byte exhaustion break,
//! the EOF-header invisible chunk, last-registered-wins parser matching,
//! the scope rules (a global parser never fires nested), the parser
//! abort and drift, the reset re-parse, the dict's intern-order sorting
//! and replace-on-duplicate, the corrupt-type error, the name-key
//! interning side effects, the u16 string-length wrap, the u8 TOC name
//! truncation, the empty-name key allocation, and the trailing empty
//! chunk's invisible header.

use std::collections::BTreeMap;
use std::path::PathBuf;

use rzh_formats::map_chunk::{
    CHUNK_TAG, DataChunkInfo, DataChunkInput, DataChunkOutput, Dict, DictValue, MapChunkError,
    NameKey, NameKeys, ParserTable, TableOfContents,
};

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join("mapchunk.txt")
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

// --- byte builders (lockstep with the C++ driver) -----------------------------

fn put_u16(b: &mut Vec<u8>, v: u16) {
    b.extend_from_slice(&v.to_le_bytes());
}
fn put_str(b: &mut Vec<u8>, s: &[u8]) {
    b.extend_from_slice(s);
}
fn put_wide(b: &mut Vec<u8>, units: &[u16]) {
    for u in units {
        put_u16(b, *u);
    }
}
fn put_chunk(b: &mut Vec<u8>, id: u32, ver: u16, data: &[u8]) {
    put_i32_sized(b, id, ver, data, data.len() as i32);
}
fn put_i32_sized(b: &mut Vec<u8>, id: u32, ver: u16, data: &[u8], size_word: i32) {
    b.extend_from_slice(&id.to_le_bytes());
    put_u16(b, ver);
    b.extend_from_slice(&size_word.to_le_bytes());
    b.extend_from_slice(data);
}
fn toc_bytes(entries: &[(&[u8], u32)]) -> Vec<u8> {
    let mut b = Vec::new();
    put_str(&mut b, &CHUNK_TAG);
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

// --- the recorder the scripted parsers write into ------------------------------

struct Rec<'k> {
    id: &'k str,
    steps: Vec<String>,
    keys: NameKeys,
    n: usize,
}

impl<'k> Rec<'k> {
    fn new(id: &'k str) -> Self {
        Rec {
            id,
            steps: Vec::new(),
            keys: NameKeys::new(),
            n: 0,
        }
    }
    fn step(&mut self, s: String) {
        self.steps.push(format!("step {} {s}", self.id));
    }
    /// Moves the recorded steps out (in order, between parse calls).
    fn drain_steps(&mut self) -> Vec<String> {
        std::mem::take(&mut self.steps)
    }
}

fn rec_info(rec: &mut Rec<'_>, info: &DataChunkInfo) {
    rec.step(format!(
        "info {} {} {} {}",
        hex(&info.label),
        hex(&info.parent_label),
        info.version,
        info.data_size
    ));
}

fn rec_open(rec: &mut Rec<'_>, input: &DataChunkInput<'_>, label: &[u8]) {
    rec.step(format!(
        "open {} {} {} {} {}",
        hex(label),
        input.get_chunk_version(),
        input.get_chunk_data_size() as i32,
        input.get_chunk_data_size_left() as i32,
        input.stack_depth()
    ));
}

fn rec_dict(rec: &mut Rec<'_>, dict: &Dict) {
    rec.step(format!("rd {}", dict.pair_count()));
    for i in 0..dict.pair_count() {
        let value_hex = match dict.nth_value(i) {
            DictValue::Bool(b) => {
                if *b {
                    "01".to_string()
                } else {
                    "00".to_string()
                }
            }
            DictValue::Int(v) => hex(&v.to_le_bytes()),
            DictValue::Real(v) => hex(&v.to_le_bytes()),
            DictValue::AsciiString(s) => hex(s),
            DictValue::UnicodeString(u) => {
                let mut b = Vec::new();
                put_wide(&mut b, u);
                hex(&b)
            }
        };
        rec.step(format!(
            "rdp {} {} {} {value_hex}",
            i,
            dict.nth_key(i).ordinal(),
            dict.nth_type(i)
        ));
    }
}

fn close_step(input: &mut DataChunkInput<'_>, rec: &mut Rec<'_>) {
    input.close_data_chunk();
    rec.step(format!("close {} {}", input.tell(), input.stack_depth()));
}

// --- the scripted parser callbacks ---------------------------------------------

type Table<'t, 'k> = ParserTable<'t, Rec<'k>>;

fn p_leaf<'t, 'k>(
    _file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
    _table: &mut ParserTable<'t, Rec<'k>>,
    rec: &mut Rec<'k>,
) -> bool {
    rec_info(rec, info);
    true
}

fn p_rec<'t, 'k>(
    file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
    table: &mut ParserTable<'t, Rec<'k>>,
    rec: &mut Rec<'k>,
) -> bool {
    rec_info(rec, info);
    file.parse(table, rec)
}

fn p_abort<'t, 'k>(
    _file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
    _table: &mut ParserTable<'t, Rec<'k>>,
    rec: &mut Rec<'k>,
) -> bool {
    rec_info(rec, info);
    false
}

fn p_drift<'t, 'k>(
    file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
    _table: &mut ParserTable<'t, Rec<'k>>,
    rec: &mut Rec<'k>,
) -> bool {
    rec_info(rec, info);
    let label = file.open_data_chunk();
    rec_open(rec, file, &label);
    true
}

fn p_tag1<'t, 'k>(
    _file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
    _table: &mut ParserTable<'t, Rec<'k>>,
    rec: &mut Rec<'k>,
) -> bool {
    rec_info(rec, info);
    rec.step("tag 1".to_string());
    true
}

fn p_tag2<'t, 'k>(
    _file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
    _table: &mut ParserTable<'t, Rec<'k>>,
    rec: &mut Rec<'k>,
) -> bool {
    rec_info(rec, info);
    rec.step("tag 2".to_string());
    true
}

fn p_height<'t, 'k>(
    file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
    _table: &mut ParserTable<'t, Rec<'k>>,
    rec: &mut Rec<'k>,
) -> bool {
    rec_info(rec, info);
    rec.step(format!("ri {}", file.read_int()));
    rec.step(format!("ri {}", file.read_int()));
    let mut buf = [0u8; 16];
    file.read_array_of_bytes(&mut buf);
    rec.step(format!("rby {}", hex(&buf)));
    rec.step(format!("ri {}", file.read_int()));
    true
}

fn p_blend<'t, 'k>(
    file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
    _table: &mut ParserTable<'t, Rec<'k>>,
    rec: &mut Rec<'k>,
) -> bool {
    rec_info(rec, info);
    rec.step(format!("ri {}", file.read_int()));
    let mut buf = [0u8; 12];
    file.read_array_of_bytes(&mut buf);
    rec.step(format!("rby {}", hex(&buf)));
    true
}

fn p_dict_only<'t, 'k>(
    file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
    _table: &mut ParserTable<'t, Rec<'k>>,
    rec: &mut Rec<'k>,
) -> bool {
    rec_info(rec, info);
    let dict = file.read_dict(&mut rec.keys);
    match dict {
        Ok(dict) => rec_dict(rec, &dict),
        Err(e) => rec.step(format!("err {e:?}")),
    }
    true
}

fn p_object<'t, 'k>(
    file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
    _table: &mut ParserTable<'t, Rec<'k>>,
    rec: &mut Rec<'k>,
) -> bool {
    rec_info(rec, info);
    if rec.n == 0 {
        rec.n = 1;
        let key = file.read_name_key(&mut rec.keys);
        rec.step(format!("rk {}", key.ordinal()));
        match file.read_dict(&mut rec.keys) {
            Ok(dict) => rec_dict(rec, &dict),
            Err(e) => rec.step(format!("err {e:?}")),
        }
        let s = file.read_ascii_string();
        rec.step(format!("ra {}", hex(&s)));
    } else {
        let key = file.read_name_key(&mut rec.keys);
        rec.step(format!("rk {}", key.ordinal()));
        let units = file.read_unicode_string();
        let mut wb = Vec::new();
        put_wide(&mut wb, &units);
        rec.step(format!("ru {}", hex(&wb)));
    }
    true
}

fn p_way<'t, 'k>(
    file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
    _table: &mut ParserTable<'t, Rec<'k>>,
    rec: &mut Rec<'k>,
) -> bool {
    rec_info(rec, info);
    rec.step(format!("ri {}", file.read_int()));
    rec.step(format!("ri {}", file.read_int()));
    let s = file.read_ascii_string();
    rec.step(format!("ra {}", hex(&s)));
    true
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

// --- replay helpers ----------------------------------------------------------------

fn valid_line(id: &str, v: bool) -> String {
    if v {
        format!("valid {id} t")
    } else {
        format!("valid {id} f")
    }
}

fn ok_line(id: &str, v: bool) -> String {
    if v {
        format!("ok {id} t")
    } else {
        format!("ok {id} f")
    }
}

fn toc_lines(id: &str, prefix: &str, toc: &TableOfContents) -> Vec<String> {
    let mut lines = vec![format!("{prefix} {id} {}", toc.len())];
    for (i, entry) in toc.entries().enumerate() {
        lines.push(format!(
            "{prefix}e {id} {i} {} {}",
            hex(&entry.name),
            entry.id
        ));
    }
    lines
}

fn replay_state(id: &str, input: &DataChunkInput<'_>) -> String {
    format!(
        "state {id} {} {} {}",
        input.tell(),
        input.stack_depth(),
        if input.at_end_of_file() { "t" } else { "f" }
    )
}

/// A shallow replay: construct, dump valid/toc, parse with an empty table,
/// dump ok/state.
fn replay_shallow(id: &str, bytes: &[u8], dump_toc: bool) -> Vec<String> {
    let mut input = DataChunkInput::new(bytes);
    let mut lines = vec![valid_line(id, input.is_valid_file_type())];
    if dump_toc {
        lines.extend(toc_lines(id, "rtoc", input.toc()));
    }
    let mut table: Table<'_, '_> = ParserTable::new();
    let mut rec = Rec::new(id);
    let ok = input.parse(&mut table, &mut rec);
    lines.extend(rec.drain_steps());
    lines.push(ok_line(id, ok));
    lines.push(replay_state(id, &input));
    lines
}

/// The scripted replays, one arm per corpus.
fn replay_script(id: &str, bytes: &[u8]) -> Vec<String> {
    let mut input = DataChunkInput::new(bytes);
    let mut rec = Rec::new(id);
    let mut lines: Vec<String> = Vec::new();
    // helper closures over the shared recorder
    macro_rules! tf {
        ($e:expr) => {
            if $e { "t" } else { "f" }
        };
    }

    match id {
        "rd_modern" => {
            lines.push(valid_line(id, input.is_valid_file_type()));
            lines.extend(toc_lines(id, "rtoc", input.toc()));
            rec.keys.name_to_key(b"ObjectFlags");
            rec.keys.name_to_key(b"Sun");
            let mut table: Table<'_, '_> = ParserTable::new();
            table.register(b"HeightMapData", b"", p_height);
            table.register(b"BlendTileData", b"", p_blend);
            table.register(b"GlobalLighting", b"", p_dict_only);
            table.register(b"WorldInfo", b"", p_dict_only);
            table.register(b"ObjectsList", b"", p_rec);
            table.register(b"MapObject", b"ObjectsList", p_object);
            table.register(b"Waypoint", b"", p_way);
            let ok = input.parse(&mut table, &mut rec);
            lines.extend(rec.drain_steps());
            lines.push(ok_line(id, ok));
            lines.push(replay_state(id, &input));
        }
        "rd_scope" => {
            lines.push(valid_line(id, input.is_valid_file_type()));
            lines.extend(toc_lines(id, "rtoc", input.toc()));
            let mut table: Table<'_, '_> = ParserTable::new();
            table.register(b"A", b"", p_rec);
            table.register(b"B", b"", p_rec);
            table.register(b"X", b"A", p_rec);
            table.register(b"X", b"B", p_leaf);
            table.register(b"Y", b"", p_leaf);
            let ok = input.parse(&mut table, &mut rec);
            lines.extend(rec.drain_steps());
            lines.push(ok_line(id, ok));
            lines.push(replay_state(id, &input));
        }
        "rd_order" => {
            lines.push(valid_line(id, input.is_valid_file_type()));
            let mut table: Table<'_, '_> = ParserTable::new();
            table.register(b"X", b"", p_tag1);
            table.register(b"X", b"", p_tag2);
            let ok = input.parse(&mut table, &mut rec);
            lines.extend(rec.drain_steps());
            lines.push(ok_line(id, ok));
            lines.push(replay_state(id, &input));
        }
        "rd_abort" => {
            lines.push(valid_line(id, input.is_valid_file_type()));
            let mut table: Table<'_, '_> = ParserTable::new();
            table.register(b"A", b"", p_abort);
            table.register(b"B", b"", p_leaf);
            let ok = input.parse(&mut table, &mut rec);
            lines.extend(rec.drain_steps());
            lines.push(ok_line(id, ok));
            lines.push(replay_state(id, &input));
            input.reset();
            rec.step(format!("tell {}", input.tell()));
            lines.extend(rec.drain_steps());
            let ok = input.parse(&mut table, &mut rec);
            lines.extend(rec.drain_steps());
            lines.push(ok_line(id, ok));
            lines.push(replay_state(id, &input));
        }
        "rd_drift" => {
            lines.push(valid_line(id, input.is_valid_file_type()));
            let mut table: Table<'_, '_> = ParserTable::new();
            table.register(b"A", b"", p_drift);
            let ok = input.parse(&mut table, &mut rec);
            lines.extend(rec.drain_steps());
            lines.push(ok_line(id, ok));
            lines.push(replay_state(id, &input));
        }
        "rd_leftover" => {
            lines.push(valid_line(id, input.is_valid_file_type()));
            let mut table: Table<'_, '_> = ParserTable::new();
            table.register(b"P", b"", p_rec);
            table.register(b"C", b"P", p_leaf);
            let ok = input.parse(&mut table, &mut rec);
            lines.extend(rec.drain_steps());
            lines.push(ok_line(id, ok));
            lines.push(replay_state(id, &input));
        }
        "rd_nested" => {
            lines.push(valid_line(id, input.is_valid_file_type()));
            lines.extend(toc_lines(id, "rtoc", input.toc()));
            let label = input.open_data_chunk();
            rec_open(&mut rec, &input, &label);
            rec.step(format!("dlab {}", hex(input.get_chunk_label())));
            let label = input.open_data_chunk();
            rec_open(&mut rec, &input, &label);
            rec.step(format!("dlab {}", hex(input.get_chunk_label())));
            rec.step(format!("ri {}", input.read_int()));
            rec.step(format!("eoc {}", tf!(input.at_end_of_chunk())));
            rec.step(format!(
                "dlefts {}",
                input.get_chunk_data_size_left() as i32
            ));
            close_step(&mut input, &mut rec);
            let label = input.open_data_chunk();
            rec_open(&mut rec, &input, &label);
            rec.step(format!(
                "dlefts {}",
                input.get_chunk_data_size_left() as i32
            ));
            let mut buf = [0u8; 6];
            input.read_array_of_bytes(&mut buf);
            rec.step(format!("rby {}", hex(&buf)));
            rec.step(format!(
                "dlefts {}",
                input.get_chunk_data_size_left() as i32
            ));
            close_step(&mut input, &mut rec);
            close_step(&mut input, &mut rec);
            rec.step(format!("eof {}", tf!(input.at_end_of_file())));
            lines.extend(rec.drain_steps());
            lines.push(replay_state(id, &input));
        }
        "rd_overdeclared" => {
            lines.push(valid_line(id, input.is_valid_file_type()));
            input.open_data_chunk();
            rec_open(&mut rec, &input, b"P");
            let label = input.open_data_chunk();
            rec_open(&mut rec, &input, &label);
            close_step(&mut input, &mut rec);
            rec.step(format!(
                "dlefts {}",
                input.get_chunk_data_size_left() as i32
            ));
            rec.step(format!("dleft {}", input.get_chunk_data_size_left() as i32));
            rec.step(format!("eof {}", tf!(input.at_end_of_file())));
            close_step(&mut input, &mut rec);
            lines.extend(rec.drain_steps());
            lines.push(replay_state(id, &input));
        }
        "rd_seek_past_eof" => {
            lines.push(valid_line(id, input.is_valid_file_type()));
            input.open_data_chunk();
            let label = input.open_data_chunk();
            rec_open(&mut rec, &input, &label);
            input.close_data_chunk(); // (the C++ closes silently here)
            rec.step(format!(
                "dlefts {}",
                input.get_chunk_data_size_left() as i32
            ));
            rec.step(format!("eof {}", tf!(input.at_end_of_file())));
            lines.extend(rec.drain_steps());
            lines.push(replay_state(id, &input));
        }
        "rd_neg_size" => {
            lines.push(valid_line(id, input.is_valid_file_type()));
            let mut table: Table<'_, '_> = ParserTable::new();
            table.register(b"A", b"", p_leaf);
            table.register(b"B", b"", p_leaf);
            let ok = input.parse(&mut table, &mut rec);
            lines.extend(rec.drain_steps());
            lines.push(ok_line(id, ok));
            lines.push(replay_state(id, &input));
        }
        "rd_trunc_header" => {
            lines.push(valid_line(id, input.is_valid_file_type()));
            let label = input.open_data_chunk();
            rec_open(&mut rec, &input, &label);
            rec.step(format!("dlab {}", hex(input.get_chunk_label())));
            rec.step(format!("dver {}", input.get_chunk_version()));
            rec.step(format!("dsize {}", input.get_chunk_data_size() as i32));
            close_step(&mut input, &mut rec);
            lines.extend(rec.drain_steps());
            lines.push(replay_state(id, &input));
        }
        "rd_header_at_eof" => {
            lines.push(valid_line(id, input.is_valid_file_type()));
            let label = input.open_data_chunk();
            rec_open(&mut rec, &input, &label);
            rec.step(format!("dlab {}", hex(input.get_chunk_label())));
            rec.step(format!("eoc {}", tf!(input.at_end_of_chunk())));
            close_step(&mut input, &mut rec);
            lines.extend(rec.drain_steps());
            lines.push(replay_state(id, &input));
        }
        "rd_trunc_string" => {
            lines.push(valid_line(id, input.is_valid_file_type()));
            input.open_data_chunk();
            let s = input.read_ascii_string();
            rec.step(format!("ra {}", hex(&s)));
            rec.step(format!(
                "dlefts {}",
                input.get_chunk_data_size_left() as i32
            ));
            rec.step(format!("eoc {}", tf!(input.at_end_of_chunk())));
            input.close_data_chunk();
            lines.extend(rec.drain_steps());
            lines.push(replay_state(id, &input));
        }
        "rd_str_nul" => {
            lines.push(valid_line(id, input.is_valid_file_type()));
            input.open_data_chunk();
            let s = input.read_ascii_string();
            rec.step(format!("ra {}", hex(&s)));
            let units = input.read_unicode_string();
            let mut wb = Vec::new();
            put_wide(&mut wb, &units);
            rec.step(format!("ru {}", hex(&wb)));
            rec.step(format!(
                "dlefts {}",
                input.get_chunk_data_size_left() as i32
            ));
            rec.step(format!("eoc {}", tf!(input.at_end_of_chunk())));
            input.close_data_chunk();
            lines.extend(rec.drain_steps());
            lines.push(replay_state(id, &input));
        }
        "rd_dict_types" => {
            lines.push(valid_line(id, input.is_valid_file_type()));
            rec.keys.name_to_key(b"Sun");
            input.open_data_chunk();
            match input.read_dict(&mut rec.keys) {
                Ok(dict) => rec_dict(&mut rec, &dict),
                Err(e) => rec.step(format!("err {e:?}")),
            }
            rec.step(format!(
                "dlefts {}",
                input.get_chunk_data_size_left() as i32
            ));
            input.close_data_chunk();
            lines.extend(rec.drain_steps());
            lines.push(replay_state(id, &input));
        }
        "rd_dict_dup" => {
            lines.push(valid_line(id, input.is_valid_file_type()));
            input.open_data_chunk();
            match input.read_dict(&mut rec.keys) {
                Ok(dict) => rec_dict(&mut rec, &dict),
                Err(e) => rec.step(format!("err {e:?}")),
            }
            input.close_data_chunk();
            lines.extend(rec.drain_steps());
            lines.push(replay_state(id, &input));
        }
        "rd_dict_badtype" => {
            lines.push(valid_line(id, input.is_valid_file_type()));
            input.open_data_chunk();
            let dict = input.read_dict(&mut rec.keys);
            match dict {
                Ok(dict) => rec_dict(&mut rec, &dict),
                Err(MapChunkError::CorruptDictType(_)) => rec.step("err dicttype".to_string()),
                Err(e) => rec.step(format!("err {e:?}")),
            }
            rec.step(format!(
                "dlefts {}",
                input.get_chunk_data_size_left() as i32
            ));
            input.close_data_chunk();
            lines.extend(rec.drain_steps());
            lines.push(replay_state(id, &input));
        }
        "rd_dict_short" => {
            lines.push(valid_line(id, input.is_valid_file_type()));
            input.open_data_chunk();
            match input.read_dict(&mut rec.keys) {
                Ok(dict) => rec_dict(&mut rec, &dict),
                Err(e) => rec.step(format!("err {e:?}")),
            }
            rec.step(format!(
                "dlefts {}",
                input.get_chunk_data_size_left() as i32
            ));
            input.close_data_chunk();
            lines.extend(rec.drain_steps());
            lines.push(replay_state(id, &input));
        }
        "rd_namekey" => {
            lines.push(valid_line(id, input.is_valid_file_type()));
            rec.keys.name_to_key(b"Name");
            input.open_data_chunk();
            let k1 = input.read_name_key(&mut rec.keys);
            rec.step(format!("rk {}", k1.ordinal()));
            let k2 = input.read_name_key(&mut rec.keys);
            rec.step(format!("rk {}", k2.ordinal()));
            let k3 = input.read_name_key(&mut rec.keys);
            rec.step(format!("rk {}", k3.ordinal()));
            let n1 = rec.keys.name_to_key(b"Other".as_slice());
            rec.step(format!("nk {}", n1.ordinal()));
            let n2 = rec.keys.name_to_key(b"".as_slice());
            rec.step(format!("nk {}", n2.ordinal()));
            rec.step(format!(
                "dlefts {}",
                input.get_chunk_data_size_left() as i32
            ));
            input.close_data_chunk();
            lines.extend(rec.drain_steps());
            lines.push(replay_state(id, &input));
        }
        "rd_unknown_id" => {
            lines.push(valid_line(id, input.is_valid_file_type()));
            let mut table: Table<'_, '_> = ParserTable::new();
            table.register(b"", b"", p_leaf);
            table.register(b"A", b"", p_leaf);
            let ok = input.parse(&mut table, &mut rec);
            lines.extend(rec.drain_steps());
            lines.push(ok_line(id, ok));
            lines.push(replay_state(id, &input));
        }
        "rd_zero_entry" => {
            lines.push(valid_line(id, input.is_valid_file_type()));
            lines.extend(toc_lines(id, "rtoc", input.toc()));
            let mut table: Table<'_, '_> = ParserTable::new();
            table.register(b"", b"", p_leaf);
            table.register(b"A", b"", p_leaf);
            let ok = input.parse(&mut table, &mut rec);
            lines.extend(rec.drain_steps());
            lines.push(ok_line(id, ok));
            rec.step(format!("gid {} {}", hex(b""), input.toc().get_id(b"")));
            rec.step(format!("gid {} {}", hex(b"A"), input.toc().get_id(b"A")));
            lines.extend(rec.drain_steps());
            lines.push(replay_state(id, &input));
        }
        "rd_bad_magic" => {
            lines.push(valid_line(id, input.is_valid_file_type()));
            lines.extend(toc_lines(id, "rtoc", input.toc()));
            let mut table: Table<'_, '_> = ParserTable::new();
            let ok = input.parse(&mut table, &mut rec);
            lines.extend(rec.drain_steps());
            lines.push(ok_line(id, ok));
            rec.step(format!("tell {}", input.tell()));
            rec.step(format!("firstpos {}", input.first_chunk_pos()));
            input.reset();
            rec.step(format!("tell {}", input.tell()));
            let label = input.open_data_chunk();
            rec_open(&mut rec, &input, &label);
            rec.step(format!("dlab {}", hex(input.get_chunk_label())));
            input.close_data_chunk();
            lines.extend(rec.drain_steps());
            lines.push(replay_state(id, &input));
        }
        "rd_dup_names" => {
            lines.push(valid_line(id, input.is_valid_file_type()));
            lines.extend(toc_lines(id, "rtoc", input.toc()));
            let mut table: Table<'_, '_> = ParserTable::new();
            table.register(b"A", b"", p_leaf);
            let ok = input.parse(&mut table, &mut rec);
            lines.extend(rec.drain_steps());
            lines.push(ok_line(id, ok));
            rec.step(format!("gid {} {}", hex(b"A"), input.toc().get_id(b"A")));
            lines.extend(rec.drain_steps());
            lines.push(replay_state(id, &input));
        }
        "rd_dup_ids" => {
            lines.push(valid_line(id, input.is_valid_file_type()));
            lines.extend(toc_lines(id, "rtoc", input.toc()));
            let mut table: Table<'_, '_> = ParserTable::new();
            table.register(b"A", b"", p_leaf);
            table.register(b"B", b"", p_leaf);
            let ok = input.parse(&mut table, &mut rec);
            lines.extend(rec.drain_steps());
            lines.push(ok_line(id, ok));
            lines.push(replay_state(id, &input));
        }
        "rd_nul_name" => {
            lines.push(valid_line(id, input.is_valid_file_type()));
            lines.extend(toc_lines(id, "rtoc", input.toc()));
            let mut table: Table<'_, '_> = ParserTable::new();
            table.register(b"AB", b"", p_leaf);
            let ok = input.parse(&mut table, &mut rec);
            lines.extend(rec.drain_steps());
            lines.push(ok_line(id, ok));
            rec.step(format!("gid {} {}", hex(b"AB"), input.toc().get_id(b"AB")));
            lines.extend(rec.drain_steps());
            lines.push(replay_state(id, &input));
        }
        "rd_unsorted_ids" => {
            lines.push(valid_line(id, input.is_valid_file_type()));
            lines.extend(toc_lines(id, "rtoc", input.toc()));
            let mut table: Table<'_, '_> = ParserTable::new();
            table.register(b"P", b"", p_leaf);
            table.register(b"Q", b"", p_leaf);
            table.register(b"R", b"", p_leaf);
            let ok = input.parse(&mut table, &mut rec);
            lines.extend(rec.drain_steps());
            lines.push(ok_line(id, ok));
            lines.push(replay_state(id, &input));
        }
        _ => panic!("no replay script for corpus {id:?}"),
    }
    lines
}

#[test]
fn golden_mapchunk_reader() {
    let corpora = parse_golden();
    assert!(
        corpora.len() >= 44,
        "unexpectedly few corpora: {}",
        corpora.len()
    );

    let readers = corpora.keys().filter(|id| id.starts_with("rd_")).count();
    assert_eq!(readers, 31, "the reader corpus count drifted");

    for (id, lines) in &corpora {
        if !id.starts_with("rd_") {
            continue;
        }
        let bytes = corpus_bytes(lines, id);
        let replayed: Vec<String> = match id.as_str() {
            "rd_toc_only_eof" | "rd_toc_zero" | "rd_short_count" => {
                replay_shallow(id, &bytes, true)
            }
            "rd_empty" | "rd_short_tag" => replay_shallow(id, &bytes, false),
            _ => replay_script(id, &bytes),
        };
        let expected: Vec<String> = lines[1..].to_vec();
        assert_eq!(
            replayed, expected,
            "{id}: replay diverges from the transcription"
        );
    }
}

#[test]
fn golden_mapchunk_writer() {
    let corpora = parse_golden();
    for (id, lines) in &corpora {
        if !id.starts_with("wr_") {
            continue;
        }
        let bytes = corpus_bytes(lines, id);
        let (mut writer, depth) = build_writer(id);
        writer.finish().expect("finish");
        assert_eq!(
            writer.bytes(),
            bytes.as_slice(),
            "{id}: the port's writer bytes differ from the transcription"
        );

        let mut replayed = vec![format!("wgen {id} {}", hex(&bytes))];
        replayed.extend(toc_lines(id, "wtoc", writer.toc()));
        replayed.push(format!("wdepth {id} {depth}"));

        // the reader replay over the writer's own bytes
        let mut input = DataChunkInput::new(&bytes);
        replayed.push(valid_line(id, input.is_valid_file_type()));
        let mut rec = Rec::new(id.as_str());
        let mut table: Table<'_, '_> = ParserTable::new();
        for (label, parent) in leaf_labels(id) {
            table.register(label.as_slice(), parent.as_slice(), p_leaf);
        }
        for label in container_labels(id) {
            table.register(label.as_slice(), b"", p_rec);
        }
        let ok = input.parse(&mut table, &mut rec);
        replayed.extend(rec.drain_steps());
        replayed.push(ok_line(id, ok));
        replayed.push(replay_state(id, &input));

        // the writer corpora's extra gid steps
        if id == "wr_toc_trunc" {
            let long_name = vec![b'a'; 259];
            rec.step(format!(
                "gid {} {}",
                hex(&long_name),
                input.toc().get_id(&long_name)
            ));
            rec.step(format!(
                "gid {} {}",
                hex(b"aaa"),
                input.toc().get_id(b"aaa")
            ));
            replayed.extend(rec.drain_steps());
        }

        let expected: Vec<String> = lines[1..].to_vec();
        assert_eq!(
            replayed, expected,
            "{id}: writer replay diverges from the transcription"
        );
    }
}

fn leaf_labels(id: &str) -> Vec<(Vec<u8>, Vec<u8>)> {
    match id {
        "wr_modern" => vec![
            (b"HeightMapData".to_vec(), Vec::new()),
            (b"BlendTileData".to_vec(), Vec::new()),
            (b"GlobalLighting".to_vec(), Vec::new()),
            (b"WorldInfo".to_vec(), Vec::new()),
            (b"MapObject".to_vec(), b"ObjectsList".to_vec()),
            (b"Waypoint".to_vec(), Vec::new()),
        ],
        "wr_reopen" => vec![(b"A".to_vec(), Vec::new()), (b"B".to_vec(), Vec::new())],
        "wr_interleave" => vec![
            (b"HeightMapData".to_vec(), Vec::new()),
            (b"BlendTileData".to_vec(), Vec::new()),
        ],
        "wr_primitives" => vec![(b"P".to_vec(), Vec::new())],
        "wr_u16_wrap" => vec![(b"S".to_vec(), Vec::new()), (b"T".to_vec(), Vec::new())],
        "wr_uni_wrap" => vec![(b"U".to_vec(), Vec::new())],
        "wr_toc_trunc" => vec![(b"aaa".to_vec(), Vec::new())],
        "wr_namekey" => vec![(b"K".to_vec(), Vec::new())],
        "wr_empty" => vec![],
        "wr_overclose" => vec![(b"A".to_vec(), Vec::new())],
        "wr_nested" => vec![(b"L".to_vec(), b"L".to_vec())],
        "wr_dict_all" => vec![(b"D".to_vec(), Vec::new())],
        "wr_trailing_empty" => vec![
            (b"A".to_vec(), Vec::new()),
            (b"B".to_vec(), Vec::new()),
            (b"C".to_vec(), Vec::new()),
        ],
        _ => panic!("no writer script for corpus {id:?}"),
    }
}

fn container_labels(id: &str) -> Vec<Vec<u8>> {
    match id {
        "wr_modern" => vec![b"ObjectsList".to_vec()],
        "wr_nested" => vec![b"L".to_vec()],
        _ => vec![],
    }
}

fn build_writer(id: &str) -> (DataChunkOutput, usize) {
    let mut writer = DataChunkOutput::new();
    let mut keys = NameKeys::new();
    match id {
        "wr_modern" => {
            keys.name_to_key(b"ObjectFlags");
            keys.name_to_key(b"Sun");

            writer.open_data_chunk(b"HeightMapData", 3);
            writer.write_int(64);
            writer.write_int(64);
            writer.write_array_of_bytes(&formula_bytes(16, 3));
            writer.write_int(5);
            writer.close_data_chunk();

            writer.open_data_chunk(b"BlendTileData", 7);
            writer.write_int(512);
            writer.write_array_of_bytes(&formula_bytes(12, 9));
            writer.close_data_chunk();

            writer.open_data_chunk(b"GlobalLighting", 2);
            let mut dict = Dict::new();
            dict.set(keys.name_to_key(b"Ambient"), DictValue::Real(0.25));
            dict.set(
                keys.name_to_key(b"Sun"),
                DictValue::AsciiString(b"Warm".to_vec()),
            );
            writer.write_dict(&keys, &dict);
            writer.close_data_chunk();

            writer.open_data_chunk(b"WorldInfo", 1);
            let mut dict = Dict::new();
            dict.set(
                keys.name_to_key(b"Name"),
                DictValue::AsciiString(b"Test Map".to_vec()),
            );
            dict.set(keys.name_to_key(b"WidthTiles"), DictValue::Int(64));
            dict.set(keys.name_to_key(b"IsMultiplayer"), DictValue::Bool(true));
            writer.write_dict(&keys, &dict);
            writer.close_data_chunk();

            writer.open_data_chunk(b"ObjectsList", 3);
            writer.open_data_chunk(b"MapObject", 1);
            let name_key = keys.name_to_key(b"Name");
            writer.write_name_key(&keys, name_key);
            let mut dict = Dict::new();
            dict.set(keys.name_to_key(b"ObjectFlags"), DictValue::Int(77));
            writer.write_dict(&keys, &dict);
            writer.write_ascii_string(b"Tank");
            writer.close_data_chunk();
            writer.open_data_chunk(b"MapObject", 1);
            let sun_key = keys.name_to_key(b"Sun");
            writer.write_name_key(&keys, sun_key);
            writer.write_unicode_string(&[0x0041, 0x0042]);
            writer.close_data_chunk();
            writer.close_data_chunk();

            writer.open_data_chunk(b"Waypoint", 1);
            writer.write_int(10);
            writer.write_int(20);
            writer.write_ascii_string(b"WP1");
            writer.close_data_chunk();
            (writer, 2)
        }
        "wr_reopen" => {
            writer.open_data_chunk(b"A", 1);
            writer.write_int(1);
            writer.close_data_chunk();
            writer.open_data_chunk(b"B", 1);
            writer.close_data_chunk();
            writer.open_data_chunk(b"A", 2);
            writer.write_int(2);
            writer.close_data_chunk();
            (writer, 1)
        }
        "wr_interleave" => {
            writer.open_data_chunk(b"HeightMapData", 1);
            let mut dict = Dict::new();
            dict.set(keys.name_to_key(b"Size"), DictValue::Int(5));
            writer.write_dict(&keys, &dict);
            writer.close_data_chunk();
            writer.open_data_chunk(b"BlendTileData", 1);
            writer.close_data_chunk();
            (writer, 1)
        }
        "wr_primitives" => {
            keys.name_to_key(b"Name");
            writer.open_data_chunk(b"P", 1);
            writer.write_real(0.5);
            writer.write_int(-1);
            writer.write_byte(0xAB);
            writer.write_array_of_bytes(&formula_bytes(6, 11));
            writer.write_ascii_string(b"Hi");
            writer.write_unicode_string(&[0x0041, 0x0042]);
            let name_key = keys.name_to_key(b"Name");
            writer.write_name_key(&keys, name_key);
            writer.close_data_chunk();
            (writer, 1)
        }
        "wr_u16_wrap" => {
            writer.open_data_chunk(b"S", 1);
            let long: Vec<u8> = (0..65539).map(|i| b'a' + (i % 26) as u8).collect();
            writer.write_ascii_string(&long);
            writer.close_data_chunk();
            writer.open_data_chunk(b"T", 1);
            writer.write_int(1);
            writer.close_data_chunk();
            (writer, 1)
        }
        "wr_uni_wrap" => {
            writer.open_data_chunk(b"U", 1);
            let units: Vec<u16> = (0..65537)
                .map(|i| {
                    if i == 0 {
                        0x0041
                    } else {
                        u16::from(b'a') + ((i % 26) as u16)
                    }
                })
                .collect();
            writer.write_unicode_string(&units);
            writer.close_data_chunk();
            (writer, 1)
        }
        "wr_toc_trunc" => {
            let long_name = vec![b'a'; 259];
            writer.open_data_chunk(&long_name, 1);
            writer.write_int(7);
            writer.close_data_chunk();
            (writer, 1)
        }
        "wr_namekey" => {
            keys.name_to_key(b"Known");
            writer.open_data_chunk(b"K", 1);
            let known_key = keys.name_to_key(b"Known");
            writer.write_name_key(&keys, known_key);
            // an unknown key: the original passes 99; any never-interned
            // key resolves to the empty name and allocates the "" entry
            // (the wire bytes are identical)
            writer.write_name_key(&keys, NameKey::INVALID);
            writer.close_data_chunk();
            (writer, 1)
        }
        "wr_empty" => (writer, 0),
        "wr_overclose" => {
            writer.close_data_chunk();
            writer.open_data_chunk(b"A", 1);
            writer.write_int(1);
            writer.close_data_chunk();
            writer.close_data_chunk();
            (writer, 1)
        }
        "wr_nested" => {
            for i in 1..=8u16 {
                writer.open_data_chunk(b"L", i);
            }
            writer.write_int(0x1234);
            for _ in 0..8 {
                writer.close_data_chunk();
            }
            (writer, 8)
        }
        "wr_dict_all" => {
            keys.name_to_key(b"Num");
            keys.name_to_key(b"Mid");
            writer.open_data_chunk(b"D", 1);
            let mut dict = Dict::new();
            dict.set(keys.name_to_key(b"Alpha"), DictValue::Real(1.5));
            dict.set(keys.name_to_key(b"Num"), DictValue::Int(42));
            dict.set(
                keys.name_to_key(b"Mid"),
                DictValue::UnicodeString(vec![0x0043]),
            );
            dict.set(keys.name_to_key(b"Zeta"), DictValue::Bool(true));
            dict.set(
                keys.name_to_key(b"Bool"),
                DictValue::AsciiString(b"yes".to_vec()),
            );
            writer.write_dict(&keys, &dict);
            writer.close_data_chunk();
            (writer, 1)
        }
        "wr_trailing_empty" => {
            writer.open_data_chunk(b"A", 1);
            writer.close_data_chunk();
            writer.open_data_chunk(b"B", 1);
            writer.write_int(1);
            writer.close_data_chunk();
            writer.open_data_chunk(b"C", 1);
            writer.close_data_chunk();
            (writer, 1)
        }
        _ => panic!("no writer script for corpus {id:?}"),
    }
}

// keep the unused builders referenced (the corpus bytes come from the
// golden script; the builders exist for lockstep documentation)
#[test]
fn golden_mapchunk_bytes_lockstep() {
    // rd_modern's hand-assembled bytes, rebuilt through the same formula
    // helpers the C++ driver uses — pins the corpus construction itself.
    let mut b = toc_bytes(&[
        (b"HeightMapData", 1),
        (b"BlendTileData", 2),
        (b"GlobalLighting", 3),
        (b"WorldInfo", 4),
        (b"ObjectsList", 5),
        (b"MapObject", 6),
        (b"Waypoint", 7),
        (b"Ambient", 8),
        (b"Sun", 9),
        (b"Name", 10),
        (b"WidthTiles", 11),
        (b"IsMultiplayer", 12),
        (b"ObjectFlags", 13),
    ]);
    let mut d = Vec::new();
    d.extend_from_slice(&64i32.to_le_bytes());
    d.extend_from_slice(&64i32.to_le_bytes());
    d.extend_from_slice(&formula_bytes(16, 3));
    d.extend_from_slice(&5i32.to_le_bytes());
    put_chunk(&mut b, 1, 3, &d);

    let corpora = parse_golden();
    let golden_bytes = corpus_bytes(
        corpora.get("rd_modern").expect("rd_modern corpus"),
        "rd_modern",
    );
    assert!(
        golden_bytes.starts_with(&b),
        "rd_modern's leading bytes drifted from the builder formula"
    );
    let _ = put_i32_sized; // referenced for the sized-chunk builder
    let _ = put_wide;
}
