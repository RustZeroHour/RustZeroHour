// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Rust port Copyright 2026 the RustZeroHour contributors.
//
//! Differential parity tests for the map global-lighting payload port
//! (P0).
//!
//! `tests/golden/maplighting.txt` is a script produced by
//! `tools/differential/gen_maplighting_golden.cpp` — a verbatim
//! transcription of the Zero Hour global-lighting load path
//! (`WorldHeightMap.cpp`'s `ParseLightingDataChunk` reader and the
//! constructor's full-load registerParser/parse walk shape) over the
//! transcribed map data-chunk container. Each corpus records its bytes
//! and, for the full-load walk, the verdict, the loaded lighting state
//! (the raw time-of-day word, the shadow-colour word, and all 24 lights
//! of both tables as f32 bit words), the absent-payload case and the
//! post-walk stream state.
//!
//! The test replays every corpus through
//! [`rzh_formats::map_lighting`] with the same walk and asserts
//! line-by-line equality, then cross-checks
//! [`GlobalLighting::from_map`] against the manual walk on every
//! corpus. The pinned quirks include: the version family (0 through 4 —
//! the object lights 1-2 from version 2, the terrain lights 1-2 from
//! version 3, the ceilings both ways), the interleaved per-time-of-day
//! block order, the init values the version gates leave behind
//! (ambient/diffuse black, position 0,0,-1), the raw unvalidated
//! time-of-day word (the INVALID 0 and past-NIGHT garbage alike), the
//! bytes-left shadow gate (a version-1 chunk with trailing bytes reads
//! the colour; an exactly-consumed version-3 chunk does not), the raw
//! bit-pattern transport (-0.0, subnormals, f32::MAX, a quiet NaN), the
//! zero-filled short reads, the under-declared chunk whose reads run
//! past its declared end, the leftovers the release close skips, the
//! global-scope rule (a nested lighting chunk never fires), last-wins
//! repeats, the absent payload and the container-level refusals.

use std::collections::BTreeMap;
use std::path::PathBuf;

use rzh_formats::map_chunk::{DataChunkInfo, DataChunkInput, ParserTable};
use rzh_formats::map_lighting::{
    GLOBAL_LIGHTING_LABEL, GlobalLighting, K_LIGHTING_VERSION_1, K_LIGHTING_VERSION_2,
    K_LIGHTING_VERSION_3, LIGHTING_TIME_OF_DAY_COUNT, MAX_GLOBAL_LIGHTS, TerrainLighting,
    parse_lighting_data_chunk,
};

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join("maplighting.txt")
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

// --- byte builders (lockstep with the C++ driver) -----------------------------

fn put_i32(b: &mut Vec<u8>, v: i32) {
    b.extend_from_slice(&v.to_le_bytes());
}
fn put_u32(b: &mut Vec<u8>, v: u32) {
    b.extend_from_slice(&v.to_le_bytes());
}
fn put_f32(b: &mut Vec<u8>, v: f32) {
    put_u32(b, v.to_bits());
}
fn put_f32_bits(b: &mut Vec<u8>, bits: u32) {
    put_u32(b, bits);
}

fn put_chunk(b: &mut Vec<u8>, id: u32, ver: u16, data: &[u8], size_word: i32) {
    b.extend_from_slice(&id.to_le_bytes());
    b.extend_from_slice(&ver.to_le_bytes());
    let size = if size_word < 0 {
        data.len() as i32
    } else {
        size_word
    };
    b.extend_from_slice(&size.to_le_bytes());
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

/// One light's 9 reals: base + k/4, exactly representable quarters.
fn put_light(b: &mut Vec<u8>, base: f32) {
    for k in 0..9u32 {
        put_f32(b, base + k as f32 * 0.25);
    }
}

/// A full payload in the reader's interleaved order (the gen's
/// `lighting_payload`).
fn lighting_payload(version: u16, tod: i32, base: f32, with_shadow: bool, shadow: u32) -> Vec<u8> {
    let mut p = Vec::new();
    put_i32(&mut p, tod);
    for i in 0..4u32 {
        put_light(&mut p, base + i as f32); // terrain light 0
        put_light(&mut p, base + 10.0 + i as f32); // object light 0
        if version >= K_LIGHTING_VERSION_2 {
            for j in 1..3u32 {
                put_light(&mut p, base + 20.0 + (j * 4 + i) as f32);
            }
        }
        if version >= K_LIGHTING_VERSION_3 {
            for j in 1..3u32 {
                put_light(&mut p, base + 40.0 + (j * 4 + i) as f32);
            }
        }
    }
    if with_shadow {
        put_i32(&mut p, shadow as i32);
    }
    p
}

// --- the manual walk (the constructor's shape, lighting only) ------------------

/// The walk's user state (the gen's `GlState`).
struct Walk {
    result: Option<GlobalLighting>,
}

/// The registered GlobalLighting parser: the reader, which never
/// refuses.
fn gl_parser(
    file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
    _table: &mut ParserTable<'_, Walk>,
    user: &mut Walk,
) -> bool {
    user.result = Some(parse_lighting_data_chunk(file, info));
    true
}

/// The registered ObjectsList container observer (the scope corpus's
/// recursing parent).
fn gl_container(
    file: &mut DataChunkInput<'_>,
    _info: &DataChunkInfo,
    table: &mut ParserTable<'_, Walk>,
    user: &mut Walk,
) -> bool {
    file.parse(table, user)
}

/// One light's nine words as the golden grammar prints them.
fn light_line(id: &str, tag: &str, slot: usize, j: usize, l: &TerrainLighting) -> String {
    let words = [
        l.ambient.red,
        l.ambient.green,
        l.ambient.blue,
        l.diffuse.red,
        l.diffuse.green,
        l.diffuse.blue,
        l.light_pos.x,
        l.light_pos.y,
        l.light_pos.z,
    ];
    let joined: Vec<String> = words.iter().map(|w| hex_f32(*w)).collect();
    format!("{tag} {id} {slot} {j} {}", joined.join(" "))
}

/// Bit-for-bit equality of two lightings (the corpus pins a quiet-NaN
/// word, which `PartialEq` on `f32` can never equal).
fn lighting_bits_equal(a: &GlobalLighting, b: &GlobalLighting) -> bool {
    if a.time_of_day != b.time_of_day || a.shadow_color != b.shadow_color {
        return false;
    }
    let words = |l: &TerrainLighting| {
        [
            l.ambient.red,
            l.ambient.green,
            l.ambient.blue,
            l.diffuse.red,
            l.diffuse.green,
            l.diffuse.blue,
            l.light_pos.x,
            l.light_pos.y,
            l.light_pos.z,
        ]
        .map(f32::to_bits)
    };
    for i in 0..LIGHTING_TIME_OF_DAY_COUNT {
        for j in 0..MAX_GLOBAL_LIGHTS {
            if words(&a.terrain_lighting[i][j]) != words(&b.terrain_lighting[i][j]) {
                return false;
            }
            if words(&a.terrain_objects_lighting[i][j]) != words(&b.terrain_objects_lighting[i][j])
            {
                return false;
            }
        }
    }
    true
}

/// The outcome lines for one corpus's walk: the verdict, the state (or
/// the refusal), and the post-walk stream state. Also cross-checks
/// `from_map` against the manual walk (the same wiring, through the
/// module's own registration).
fn replay_corpus(id: &str, bytes: &[u8], with_container: bool) -> Vec<String> {
    let valid = DataChunkInput::new(bytes).is_valid_file_type();
    let mut lines = vec![format!("valid {id} {}", tf(valid))];

    let mut input = DataChunkInput::new(bytes);
    let mut table: ParserTable<'_, Walk> = ParserTable::new();
    table.register(GLOBAL_LIGHTING_LABEL, b"", gl_parser);
    if with_container {
        table.register(b"ObjectsList", b"", gl_container);
    }
    let mut user = Walk { result: None };
    let ok = input.parse(&mut table, &mut user);

    lines.push(format!("walk {id} {}", tf(ok)));
    if !ok {
        lines.push(format!("gerr {id} corrupt"));
    } else if let Some(gl) = &user.result {
        lines.push(format!(
            "glt {id} {} {} {}",
            gl.time_of_day,
            tf(gl.shadow_color.is_some()),
            gl.shadow_color
                .map_or_else(|| "00000000".to_string(), |c| format!("{c:08x}"))
        ));
        for i in 0..LIGHTING_TIME_OF_DAY_COUNT {
            for j in 0..MAX_GLOBAL_LIGHTS {
                lines.push(light_line(id, "gter", i, j, &gl.terrain_lighting[i][j]));
            }
            for j in 0..MAX_GLOBAL_LIGHTS {
                lines.push(light_line(
                    id,
                    "gobj",
                    i,
                    j,
                    &gl.terrain_objects_lighting[i][j],
                ));
            }
        }
    } else {
        lines.push(format!("gnone {id}"));
    }
    lines.push(format!(
        "gstate {id} {} {} {}",
        input.tell(),
        input.stack_depth(),
        tf(input.at_end_of_file())
    ));

    // the from_map cross-check: the module's own walk must agree with
    // the manual one (verdict, payload, error) — bit for bit, since the
    // corpora pin NaN words.
    let from_map = GlobalLighting::from_map(bytes);
    let manual = if !ok {
        Err(rzh_formats::map_lighting::LightingError::CorruptFileFormat)
    } else {
        Ok(user.result.as_ref())
    };
    match (&from_map, &manual) {
        (Err(e1), Err(e2)) => assert_eq!(e1, e2, "{id}: from_map error differed"),
        (Ok(None), Ok(None)) => {}
        (Ok(Some(a)), Ok(Some(b))) => assert!(
            lighting_bits_equal(a, b),
            "{id}: from_map disagreed with the manual walk"
        ),
        _ => panic!("{id}: from_map disagreed with the manual walk (shape)"),
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
fn golden_maplighting_replay() {
    let corpora = parse_golden();
    assert!(
        corpora.len() >= 17,
        "expected the full corpus set, found {}",
        corpora.len()
    );

    for (id, lines) in &corpora {
        let bytes = corpus_bytes(lines, id);
        let with_container = id == "gl_nested";
        let replay = replay_corpus(id, &bytes, with_container);
        let golden = golden_replay_lines(lines);
        assert_eq!(golden, replay, "{id}: golden replay drifted");
    }
}

/// The corpus bytes rebuild through the same formula builders the C++
/// driver uses — pins the corpus construction itself.
#[test]
fn golden_maplighting_bytes_lockstep() {
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

    // gl_v3: the modern full shape with the high-bit shadow word.
    let mut b = toc_bytes(&[("GlobalLighting", 1)]);
    let p = lighting_payload(K_LIGHTING_VERSION_3, 2, 0.25, true, 0x8000_0055);
    put_chunk(&mut b, 1, K_LIGHTING_VERSION_3, &p, -1);
    check("gl_v3", b);

    // gl_v2: no shadow word.
    let mut b = toc_bytes(&[("GlobalLighting", 1)]);
    let p = lighting_payload(K_LIGHTING_VERSION_2, 4, 1.5, false, 0);
    put_chunk(&mut b, 1, K_LIGHTING_VERSION_2, &p, -1);
    check("gl_v2", b);

    // gl_v1: tod 0 (the INVALID slot's value, stored raw).
    let mut b = toc_bytes(&[("GlobalLighting", 1)]);
    let p = lighting_payload(K_LIGHTING_VERSION_1, 0, 2.25, false, 0);
    put_chunk(&mut b, 1, K_LIGHTING_VERSION_1, &p, -1);
    check("gl_v1", b);

    // gl_v1_shadow: the trailing shadow word a version-1 chunk reads.
    let mut b = toc_bytes(&[("GlobalLighting", 1)]);
    let mut p = lighting_payload(K_LIGHTING_VERSION_1, 3, 5.25, false, 0);
    put_i32(&mut p, 0x4040_4040);
    put_chunk(&mut b, 1, K_LIGHTING_VERSION_1, &p, -1);
    check("gl_v1_shadow", b);

    // gl_bits: the special bit patterns then the word formula.
    let mut b = toc_bytes(&[("GlobalLighting", 1)]);
    let mut p = Vec::new();
    put_i32(&mut p, 2);
    let specials = [
        0x8000_0000u32,
        0x0000_0001,
        0x7f7f_ffff,
        0x7fc0_0000,
        0xff7f_ffff,
        0x0080_0000,
    ];
    for light in 0..24u32 {
        for k in 0..9u32 {
            let flat = light * 9 + k;
            if flat < 6 {
                put_f32_bits(&mut p, specials[flat as usize]);
            } else {
                put_f32_bits(&mut p, 0x3e00_0000 + flat * 0x0010_0001);
            }
        }
    }
    put_i32(&mut p, 0xdead_beef_u32 as i32);
    put_chunk(&mut b, 1, K_LIGHTING_VERSION_3, &p, -1);
    check("gl_bits", b);

    // gl_short: the time-of-day word and one stray real.
    let mut b = toc_bytes(&[("GlobalLighting", 1)]);
    let mut p = Vec::new();
    put_i32(&mut p, 2);
    put_i32(&mut p, 0x3f80_0000);
    put_chunk(&mut b, 1, K_LIGHTING_VERSION_3, &p, -1);
    check("gl_short", b);

    // gl_underdeclared: the size word lies small (100 of 292).
    let mut b = toc_bytes(&[("GlobalLighting", 1)]);
    let p = lighting_payload(K_LIGHTING_VERSION_1, 2, 7.0, false, 0);
    put_chunk(&mut b, 1, K_LIGHTING_VERSION_1, &p, 100);
    check("gl_underdeclared", b);

    // gl_leftover: the shadow word plus eight leftover bytes.
    let mut b = toc_bytes(&[("GlobalLighting", 1)]);
    let mut p = lighting_payload(K_LIGHTING_VERSION_3, 2, 8.25, true, 0x0102_0304);
    p.extend_from_slice(&[0xee; 8]);
    put_chunk(&mut b, 1, K_LIGHTING_VERSION_3, &p, -1);
    check("gl_leftover", b);

    // gl_multi: two chunks, the last one wins.
    let mut b = toc_bytes(&[("GlobalLighting", 1)]);
    let p1 = lighting_payload(K_LIGHTING_VERSION_2, 1, 9.5, false, 0);
    put_chunk(&mut b, 1, K_LIGHTING_VERSION_2, &p1, -1);
    let p2 = lighting_payload(K_LIGHTING_VERSION_3, 4, 10.75, true, 0x0a0b_0c0d);
    put_chunk(&mut b, 1, K_LIGHTING_VERSION_3, &p2, -1);
    check("gl_multi", b);

    // gl_nested: the nested chunk inside an ObjectsList container, then
    // the top-level one.
    let mut b = toc_bytes(&[("ObjectsList", 1), ("GlobalLighting", 2)]);
    let inner = lighting_payload(K_LIGHTING_VERSION_3, 2, 11.5, true, 0x1111_1111);
    let mut nested = Vec::new();
    put_chunk(&mut nested, 2, K_LIGHTING_VERSION_3, &inner, -1);
    put_chunk(&mut b, 1, 3, &nested, -1);
    let top = lighting_payload(K_LIGHTING_VERSION_1, 3, 12.0, false, 0);
    put_chunk(&mut b, 2, K_LIGHTING_VERSION_1, &top, -1);
    check("gl_nested", b);
}
