// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Rust port Copyright 2026 the RustZeroHour contributors.
//
//! Differential parity tests for the map objects-list payload port (P0).
//!
//! `tests/golden/mapobjects.txt` is a script produced by
//! `tools/differential/gen_mapobjects_golden.cpp` — a verbatim
//! transcription of the Zero Hour objects-list load path
//! (`WorldHeightMap.cpp`'s `ParseObjectsDataChunk` container reader,
//! `ParseObjectDataChunk`'s dispatch, `ParseObjectData`'s Object
//! reader, the `MapObject` construction with `normalizeAngle`, and the
//! constructor's logical-load registerParser/parse walk shape) over the
//! transcribed map data-chunk container (with the load-path `Dict` and
//! the name-key interning). Each corpus records its bytes and, for the
//! logical-load walk, the verdict, the kept-object list (each object's
//! location, normalized angle, flags, name, derivations and property
//! pairs — in the interning order the dict stores), the z-range drops,
//! the final name-key interning table, the error verdicts and the
//! post-walk stream state.
//!
//! The test replays every corpus through
//! [`rzh_formats::map_objects`] with the same walk and asserts
//! line-by-line equality, then cross-checks
//! [`ObjectsListData::from_map`] against the manual walk on every
//! corpus (through the same dump — the corpora pin NaN words that
//! `PartialEq` on `f32` could never compare). The pinned quirks
//! include: the version family (0 through 4, the version-2 dict with
//! its still-zeroed z, the version gates hanging off the Object's own
//! word — a version-1 list with version-3 children), the exact-type
//! derivations (wrong-typed well-known keys derive nothing), the
//! z-range drops (the floor and ceiling kept, one unit past each
//! dropped, the chain unbroken), the `normalizeAngle` wrap in its
//! mixed double/single precision ((float)π wrapping negative and its
//! negation positive, the multi-wrap 100, the NaN early 0, the
//! infinite-angle refusal), the name shapes (the embedded NUL, the
//! high bytes), all five property-pair types with the duplicate-key
//! update and the interning-order storage, the corrupt dict type, the
//! head-overwrite list semantics (a later list owning the head, a
//! dropping or empty later list leaving it), the scope rules (a bare
//! Object at global scope, an Object inside a foreign container), the
//! leftover-skipping closes, the under-declared Object whose reads run
//! past its declared end, the short file's zero-filled pairs, the
//! absent payload and the container-level refusals.

use std::collections::BTreeMap;
use std::path::PathBuf;

use rzh_formats::map_chunk::{
    DataChunkInfo, DataChunkInput, DictValue, MapChunkError, NameKeys, ParserTable,
};
use rzh_formats::map_objects::{
    K_OBJECTS_VERSION_1, K_OBJECTS_VERSION_3, MapObjectData, OBJECT_LABEL, OBJECTS_LIST_LABEL,
    ObjectsError, ObjectsListData, parse_object_data,
};

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join("mapobjects.txt")
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
fn put_f32(b: &mut Vec<u8>, v: f32) {
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

// --- the manual walk (the constructor's shape, objects only) ------------------

/// The walk's user state (the gen's `ObState`).
struct Walk {
    objects: Vec<MapObjectData>,
    linked: bool,
    dropped: usize,
    keys: NameKeys,
    error: Option<ObjectsError>,
}

impl Walk {
    fn new() -> Self {
        let mut keys = NameKeys::new();
        // the well-known keys seeded the way the generator's harness
        // does (WellKnownKeys.h's declaration order)
        keys.name_to_key(b"lightHeightAboveTerrain");
        keys.name_to_key(b"waypointID");
        keys.name_to_key(b"scorchType");
        Walk {
            objects: Vec::new(),
            linked: false,
            dropped: 0,
            keys,
            error: None,
        }
    }
}

/// The registered ObjectsList parser (the gen's
/// `parseObjectsDataChunk`).
fn objects_list_parser(
    file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
    table: &mut ParserTable<'_, Walk>,
    user: &mut Walk,
) -> bool {
    user.linked = false; // m_currentObject = nullptr
    table.register(OBJECT_LABEL, info.label.clone(), object_parser);
    file.parse(table, user)
}

/// The registered Object parser (the gen's `parseObjectDataChunk` +
/// `parseObjectData`, with the linkage's head-overwrite semantics).
fn object_parser(
    file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
    _table: &mut ParserTable<'_, Walk>,
    user: &mut Walk,
) -> bool {
    match parse_object_data(file, info, &mut user.keys) {
        Ok(Some(obj)) => {
            if !user.linked {
                user.objects.clear();
            }
            user.objects.push(obj);
            user.linked = true;
            true
        }
        Ok(None) => {
            user.dropped += 1;
            true
        }
        Err(e) => {
            user.error = Some(e);
            false
        }
    }
}

/// The error line for a refused load.
fn err_line(id: &str, error: &ObjectsError) -> String {
    match error {
        ObjectsError::CorruptFileFormat => format!("oerr {id} corrupt"),
        ObjectsError::Dict(MapChunkError::CorruptDictType(t)) => format!("oerr {id} dict {t}"),
        ObjectsError::Dict(MapChunkError::UnclosedChunk) => format!("oerr {id} dict -"),
        ObjectsError::NonFiniteAngle => format!("oerr {id} angle"),
    }
}

/// One property pair's value as the golden grammar prints it.
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
/// cross-check (through the same dump — the corpora pin NaN words).
fn replay_corpus(id: &str, bytes: &[u8]) -> Vec<String> {
    let valid = DataChunkInput::new(bytes).is_valid_file_type();
    let mut lines = vec![format!("valid {id} {}", tf(valid))];

    let mut input = DataChunkInput::new(bytes);
    let mut table: ParserTable<'_, Walk> = ParserTable::new();
    table.register(OBJECTS_LIST_LABEL, b"", objects_list_parser);
    let mut user = Walk::new();
    let ok = input.parse(&mut table, &mut user);

    lines.push(format!("walk {id} {}", tf(ok)));
    if !ok {
        let line = match &user.error {
            Some(e) => err_line(id, e),
            None => format!("oerr {id} corrupt"),
        };
        lines.push(line);
    } else {
        lines.push(format!("objn {id} {}", user.objects.len()));
        for (i, obj) in user.objects.iter().enumerate() {
            lines.push(format!(
                "obj {id} {i} {} {} {} {} {} {} {} {} {} {}",
                hex_f32(obj.location.x),
                hex_f32(obj.location.y),
                hex_f32(obj.location.z),
                hex_f32(obj.angle),
                obj.flags,
                q(&obj.name),
                tf(obj.is_waypoint),
                tf(obj.is_light),
                tf(obj.is_scorch),
                obj.properties.pair_count()
            ));
            for p in 0..obj.properties.pair_count() {
                let name = user.keys.key_to_name(obj.properties.nth_key(p)).to_vec();
                lines.push(format!(
                    "opair {id} {i} {} {}",
                    q(&name),
                    pair_value_line(obj.properties.nth_value(p))
                ));
            }
        }
    }
    for k in 0..user.dropped {
        lines.push(format!("odrop {id} {k}"));
    }
    for (ordinal, name) in user.keys.names() {
        lines.push(format!("okey {id} {ordinal} {}", q(name)));
    }
    lines.push(format!(
        "ostate {id} {} {} {}",
        input.tell(),
        input.stack_depth(),
        tf(input.at_end_of_file())
    ));

    // the from_map cross-check: the module's own walk must agree with
    // the manual one — compared through the same dump, since the
    // corpora pin NaN words that PartialEq on f32 can never equal.
    let from_map = ObjectsListData::from_map(bytes);
    let manual = if !ok {
        Err(user.error.unwrap_or(ObjectsError::CorruptFileFormat))
    } else {
        Ok(user.objects.clone())
    };
    match (&from_map, &manual) {
        (Err(e1), Err(e2)) => assert_eq!(e1, e2, "{id}: from_map error differed"),
        (Ok(list), Ok(objects)) => {
            let mut dump = vec![format!("objn {id} {}", list.objects.len())];
            for (i, obj) in list.objects.iter().enumerate() {
                dump.push(format!(
                    "obj {id} {i} {} {} {} {} {} {} {} {} {} {}",
                    hex_f32(obj.location.x),
                    hex_f32(obj.location.y),
                    hex_f32(obj.location.z),
                    hex_f32(obj.angle),
                    obj.flags,
                    q(&obj.name),
                    tf(obj.is_waypoint),
                    tf(obj.is_light),
                    tf(obj.is_scorch),
                    obj.properties.pair_count()
                ));
            }
            let mut manual_dump = vec![format!("objn {id} {}", objects.len())];
            for (i, obj) in objects.iter().enumerate() {
                manual_dump.push(format!(
                    "obj {id} {i} {} {} {} {} {} {} {} {} {} {}",
                    hex_f32(obj.location.x),
                    hex_f32(obj.location.y),
                    hex_f32(obj.location.z),
                    hex_f32(obj.angle),
                    obj.flags,
                    q(&obj.name),
                    tf(obj.is_waypoint),
                    tf(obj.is_light),
                    tf(obj.is_scorch),
                    obj.properties.pair_count()
                ));
            }
            assert_eq!(
                dump, manual_dump,
                "{id}: from_map disagreed with the manual walk"
            );
        }
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
fn golden_mapobjects_replay() {
    let corpora = parse_golden();
    assert!(
        corpora.len() >= 25,
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
fn golden_mapobjects_bytes_lockstep() {
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

    // one Object payload: loc, angle, flags, name, (with_dict) pairs
    let object_payload = |x: f32,
                          y: f32,
                          z: f32,
                          angle: f32,
                          flags: i32,
                          name: &str,
                          pairs: &[Vec<u8>],
                          with_dict: bool| {
        let mut p = Vec::new();
        put_f32(&mut p, x);
        put_f32(&mut p, y);
        put_f32(&mut p, z);
        put_f32(&mut p, angle);
        put_i32(&mut p, flags);
        p.extend_from_slice(&(name.len() as u16).to_le_bytes());
        p.extend_from_slice(name.as_bytes());
        if with_dict {
            p.extend_from_slice(&(pairs.len() as u16).to_le_bytes());
            for pair in pairs {
                p.extend_from_slice(pair);
            }
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
        put_f32(&mut p, v);
        p
    };

    // ob_v3: the modern shape (the derivations corpus)
    let mut b = toc_bytes(&[
        ("ObjectsList", 1),
        ("Object", 2),
        ("objectEnabled", 3),
        ("objectInitialHealth", 4),
        ("waypointID", 5),
        ("lightHeightAboveTerrain", 6),
        ("scorchType", 7),
        ("objectSelectable", 8),
    ]);
    {
        let mut inner = Vec::new();
        put_chunk(
            &mut inner,
            2,
            K_OBJECTS_VERSION_3,
            &object_payload(
                100.0,
                200.0,
                10.0,
                0.0,
                0x0000_0101,
                "AmericaVehicleProductionPlant",
                &[pair_bool(3, true), pair_int(4, 100)],
                true,
            ),
        );
        put_chunk(
            &mut inner,
            2,
            K_OBJECTS_VERSION_3,
            &object_payload(
                250.0,
                260.0,
                0.0,
                std::f32::consts::PI, // the corpus's 3.14159265f, bit for bit
                0,
                "Waypoint",
                &[pair_int(5, 7)],
                true,
            ),
        );
        put_chunk(
            &mut inner,
            2,
            K_OBJECTS_VERSION_3,
            &object_payload(
                -50.0,
                60.0,
                25.0,
                -1.5,
                0x0000_0001,
                "CruiseMissile",
                &[pair_real(6, 12.5)],
                true,
            ),
        );
        put_chunk(
            &mut inner,
            2,
            K_OBJECTS_VERSION_3,
            &object_payload(
                0.0,
                0.0,
                0.0,
                100.0,
                0x0000_0040,
                "Scorch1",
                &[pair_int(7, 2), pair_bool(8, false)],
                true,
            ),
        );
        put_chunk(&mut b, 1, K_OBJECTS_VERSION_3, &inner);
    }
    check("ob_v3", b);

    // ob_zdrop: the z-range contract
    let mut b = toc_bytes(&[("ObjectsList", 1), ("Object", 2)]);
    {
        let mut inner = Vec::new();
        for &(z, name) in &[
            (2000.0f32, "TooHigh"),
            (-2000.0f32, "TooLow"),
            (-1000.0f32, "AtFloor"),
            (1593.75f32, "AtCeiling"),
        ] {
            put_chunk(
                &mut inner,
                2,
                K_OBJECTS_VERSION_3,
                &object_payload(0.0, 0.0, z, 0.0, 0, name, &[], true),
            );
        }
        put_chunk(
            &mut inner,
            2,
            K_OBJECTS_VERSION_3,
            &object_payload(1.0, 1.0, 1.0, 0.0, 0, "Survivor", &[], true),
        );
        put_chunk(&mut b, 1, K_OBJECTS_VERSION_3, &inner);
    }
    check("ob_zdrop", b);

    // ob_angle: the wrap corpus (the NaN child patched last)
    let mut b = toc_bytes(&[("ObjectsList", 1), ("Object", 2)]);
    {
        let mut inner = Vec::new();
        let pi = std::f32::consts::PI; // the corpus's 3.14159265f, bit for bit
        for &a in &[0.0f32, pi, -pi, 100.0f32, -100.0f32, 7.5f32] {
            put_chunk(
                &mut inner,
                2,
                K_OBJECTS_VERSION_3,
                &object_payload(0.0, 0.0, 0.0, a, 0, "Angled", &[], true),
            );
        }
        let mut nan = object_payload(0.0, 0.0, 0.0, 0.0, 0, "NanAngle", &[], true);
        nan[12..16].copy_from_slice(&0x7fc0_0000u32.to_le_bytes());
        put_chunk(&mut inner, 2, K_OBJECTS_VERSION_3, &nan);
        put_chunk(&mut b, 1, K_OBJECTS_VERSION_3, &inner);
    }
    check("ob_angle", b);

    // ob_multi: the head-overwrite corpus
    let mut b = toc_bytes(&[("ObjectsList", 1), ("Object", 2)]);
    {
        let mut inner = Vec::new();
        put_chunk(
            &mut inner,
            2,
            K_OBJECTS_VERSION_3,
            &object_payload(0.0, 0.0, 0.0, 0.0, 0, "FirstA", &[], true),
        );
        put_chunk(
            &mut inner,
            2,
            K_OBJECTS_VERSION_3,
            &object_payload(1.0, 0.0, 0.0, 0.0, 0, "FirstB", &[], true),
        );
        put_chunk(&mut b, 1, K_OBJECTS_VERSION_3, &inner);
    }
    {
        let mut inner = Vec::new();
        put_chunk(
            &mut inner,
            2,
            K_OBJECTS_VERSION_3,
            &object_payload(2.0, 0.0, 0.0, 0.0, 0, "SecondC", &[], true),
        );
        put_chunk(&mut b, 1, K_OBJECTS_VERSION_3, &inner);
    }
    check("ob_multi", b);

    // ob_short: the cut file
    let mut b = toc_bytes(&[("ObjectsList", 1), ("Object", 2), ("objectEnabled", 3)]);
    {
        let mut inner = Vec::new();
        let mut payload = object_payload(
            0.0,
            0.0,
            0.0,
            0.0,
            0,
            "Sho",
            &[pair_bool(3, true), pair_bool(3, false)],
            true,
        );
        payload.truncate(payload.len() - 10); // cut the pairs, keep the count
        put_chunk(&mut inner, 2, K_OBJECTS_VERSION_3, &payload);
        put_chunk(&mut b, 1, K_OBJECTS_VERSION_3, &inner);
    }
    check("ob_short", b);

    // ob_v1 / ob_v0: the no-dict shapes
    let mut b = toc_bytes(&[("ObjectsList", 1), ("Object", 2)]);
    {
        let mut inner = Vec::new();
        put_chunk(
            &mut inner,
            2,
            K_OBJECTS_VERSION_1,
            &object_payload(30.0, 40.0, 500.0, 1.0, 0x0000_0002, "TreeOak", &[], false),
        );
        put_chunk(&mut b, 1, K_OBJECTS_VERSION_1, &inner);
    }
    check("ob_v1", b);
    let mut b = toc_bytes(&[("ObjectsList", 1), ("Object", 2)]);
    {
        let mut inner = Vec::new();
        put_chunk(
            &mut inner,
            2,
            0,
            &object_payload(1.0, 2.0, 3.0, 0.0, 0, "OldTree", &[], false),
        );
        put_chunk(&mut b, 1, 0, &inner);
    }
    check("ob_v0", b);
}
