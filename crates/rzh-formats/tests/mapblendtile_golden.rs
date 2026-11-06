// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Rust port Copyright 2026 the RustZeroHour contributors.
//
//! Differential parity tests for the map blend-tile payload port (P0).
//!
//! `tests/golden/mapblendtile.txt` is a script produced by
//! `tools/differential/gen_mapblendtile_golden.cpp` — a verbatim
//! transcription of the Zero Hour blend-tile load path
//! (`WorldHeightMap.cpp`'s `ParseBlendTileData` reader, the
//! `initCliffFlagsFromHeights` derivation, the constructor's full-load
//! walk shape and its post-walk patch loop) over the transcribed map
//! data-chunk container and the full-load `HeightMapData` reader. Each
//! corpus records its bytes and, for both 3-way blend settings (the
//! retail default on, and off), the walk verdict, the loaded height map
//! and blend payload (the arrays raw and post-patch, the state bitmaps,
//! the classes and records), the error verdicts and the post-walk
//! stream state.
//!
//! The test replays every corpus through [`rzh_formats::map_blendtile`]
//! with the same walks and asserts line-by-line equality, then
//! cross-checks [`TerrainLoad::from_map`] against the manual walk on
//! every corpus. The pinned quirks include: the version family (0
//! through 9 — the extra blend layer from 6, the cliff indices and
//! tables from 5, the long diagonals from 3, the edge classes from 4,
//! the direct cliff bitmap from 8), the version-7 cliff-state row-length
//! remap (17-wide maps store 2 bytes per row, the bitmap holds 3 — the
//! tail stays zero; a 1-wide map stores none at all), the pre-version-7
//! cliff derivation (a 16-unit sample jump is a cliff, a 15-unit one is
//! not), the version-1 downsample (the compaction, the forced counts,
//  the halved shared extents, the patch walking the halved count only),
//! the len check against the **live** data size (a version-1 blend
//! resize halves it first), the 3-way gates (the FLIPPED_MASK filter
//! and the extra-array flush), the patch loop's clamps (the partial walk
//! when a later height map shrinks the bound, the never-clamped tile
//! indices), the texture-class names (the embedded NUL), the negative
//! counts, the count-over-array-bound refusals, the null-array patch
//! death, the blend-before-height-map shape (the empty arrays, the
//! skipped patch tail), the short-read zero fills, the under-declared
//! chunk whose counts read the next chunk's header, the leftovers the
//! release close skips, the global-scope rule (a nested blend never
//! fires), last-wins repeats, and the container-level refusals.

use std::collections::BTreeMap;
use std::path::PathBuf;

use rzh_formats::map_blendtile::{
    BLEND_TILE_DATA_LABEL, BlendTileError, BlendTileInfo, CliffInfo, FLAG_VAL,
    K_BLEND_TILE_VERSION_1, K_BLEND_TILE_VERSION_2, K_BLEND_TILE_VERSION_3, K_BLEND_TILE_VERSION_4,
    K_BLEND_TILE_VERSION_5, K_BLEND_TILE_VERSION_6, K_BLEND_TILE_VERSION_7, K_BLEND_TILE_VERSION_8,
    NUM_BLEND_TILES, NUM_CLIFF_INFO, NUM_TEXTURE_CLASSES, SharedHeightMap, TerrainLoad,
    TxTextureClass,
};
use rzh_formats::map_chunk::{DataChunkInfo, DataChunkInput, ParserTable};
use rzh_formats::map_heightmap::{
    Boundary, HEIGHT_MAP_DATA_LABEL, HeightMapData, HeightMapError, K_HEIGHT_MAP_VERSION_1,
    K_HEIGHT_MAP_VERSION_2, K_HEIGHT_MAP_VERSION_3, K_HEIGHT_MAP_VERSION_4, parse_height_map_data,
};

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join("mapblendtile.txt")
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

/// An i16 list as the golden grammar prints it ('-' for empty).
fn i16_list(values: &[i16]) -> String {
    if values.is_empty() {
        return "-".to_string();
    }
    values
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(" ")
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

// --- byte builders (lockstep with the C++ driver) -----------------------------

fn put_u16(b: &mut Vec<u8>, v: u16) {
    b.extend_from_slice(&v.to_le_bytes());
}
fn put_u32(b: &mut Vec<u8>, v: u32) {
    b.extend_from_slice(&v.to_le_bytes());
}
fn put_i32(b: &mut Vec<u8>, v: i32) {
    b.extend_from_slice(&v.to_le_bytes());
}
fn put_f32(b: &mut Vec<u8>, v: f32) {
    b.extend_from_slice(&v.to_le_bytes());
}
fn put_str(b: &mut Vec<u8>, s: &[u8]) {
    b.extend_from_slice(s);
}
fn put_i16s(b: &mut Vec<u8>, v: &[i16]) {
    for x in v {
        b.extend_from_slice(&x.to_le_bytes());
    }
}
fn put_name(b: &mut Vec<u8>, s: &[u8]) {
    put_u16(b, s.len() as u16);
    put_str(b, s);
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
    put_i32(&mut b, entries.len() as i32);
    for &(name, id) in entries {
        b.push(name.len() as u8);
        put_str(&mut b, name);
        put_u32(&mut b, id);
    }
    b
}
fn formula_bytes(n: usize, seed: u8) -> Vec<u8> {
    (0..n).map(|i| seed.wrapping_add((i * 7) as u8)).collect()
}
fn formula_i16s(n: usize, seed: i16) -> Vec<i16> {
    (0..n)
        .map(|i| ((i32::from(seed) + (i as i32) * 7) & 0xFFFF) as i16)
        .collect()
}

/// A version-4 height-map payload with explicit boundaries.
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

/// A version-3 height-map payload (border, synthesised boundary).
fn v3_payload(w: i32, h: i32, border: i32, data: &[u8]) -> Vec<u8> {
    let mut p = Vec::new();
    put_i32(&mut p, w);
    put_i32(&mut p, h);
    put_i32(&mut p, border);
    put_i32(&mut p, data.len() as i32);
    put_str(&mut p, data);
    p
}

/// A version-2 height-map payload (no border word).
fn v2_payload(w: i32, h: i32, data: &[u8]) -> Vec<u8> {
    let mut p = Vec::new();
    put_i32(&mut p, w);
    put_i32(&mut p, h);
    put_i32(&mut p, data.len() as i32);
    put_str(&mut p, data);
    p
}

// --- the blend payload builder (the C++ driver's BtParts) ----------------------

#[derive(Default, Clone)]
struct TexRec {
    first: i32,
    num: i32,
    width: i32,
    name: Vec<u8>,
}

#[derive(Clone)]
struct BlendRec {
    ndx: i32,
    horiz: u8,
    vert: u8,
    rdiag: u8,
    ldiag: u8,
    inv: u8,
    longd: u8,
    custom: i32,
    flag: i32,
}

#[derive(Clone)]
struct CliffRec {
    tile_idx: i32,
    uvs: [f32; 8],
    flip: u8,
    mutant: u8,
}

#[derive(Default, Clone)]
struct BtParts {
    tile: Vec<i16>,
    blend: Vec<i16>,
    extra: Vec<i16>,
    cliff: Vec<i16>,
    cliff_state: Vec<u8>,
    nbmp: i32,
    nblend: i32,
    ncliff: i32,
    n_tex: i32, // -1: the tex vector's size
    tex: Vec<TexRec>,
    n_edge_tiles: i32,
    n_edge_tex: i32, // -1: the etex vector's size
    etex: Vec<TexRec>,
    blends: Vec<BlendRec>,
    cliffs: Vec<CliffRec>,
}

impl BtParts {
    fn new() -> Self {
        BtParts {
            nblend: 1,
            ncliff: 1,
            n_tex: -1,
            n_edge_tex: -1,
            ..BtParts::default()
        }
    }
}

fn blend_payload(version: u16, len: i32, p: &BtParts) -> Vec<u8> {
    let mut b = Vec::new();
    put_i32(&mut b, len);
    put_i16s(&mut b, &p.tile);
    put_i16s(&mut b, &p.blend);
    if version >= K_BLEND_TILE_VERSION_6 {
        put_i16s(&mut b, &p.extra);
    }
    if version >= K_BLEND_TILE_VERSION_5 {
        put_i16s(&mut b, &p.cliff);
    }
    if version >= K_BLEND_TILE_VERSION_7 {
        put_str(&mut b, &p.cliff_state);
    }
    put_i32(&mut b, p.nbmp);
    put_i32(&mut b, p.nblend);
    if version >= K_BLEND_TILE_VERSION_5 {
        put_i32(&mut b, p.ncliff);
    }
    put_i32(
        &mut b,
        if p.n_tex < 0 {
            p.tex.len() as i32
        } else {
            p.n_tex
        },
    );
    for t in &p.tex {
        put_i32(&mut b, t.first);
        put_i32(&mut b, t.num);
        put_i32(&mut b, t.width);
        put_i32(&mut b, 0); // the legacy GDF word
        put_name(&mut b, &t.name);
    }
    if version >= K_BLEND_TILE_VERSION_4 {
        put_i32(&mut b, p.n_edge_tiles);
        put_i32(
            &mut b,
            if p.n_edge_tex < 0 {
                p.etex.len() as i32
            } else {
                p.n_edge_tex
            },
        );
        for t in &p.etex {
            put_i32(&mut b, t.first);
            put_i32(&mut b, t.num);
            put_i32(&mut b, t.width);
            put_name(&mut b, &t.name); // (no legacy word)
        }
    }
    for r in &p.blends {
        put_i32(&mut b, r.ndx);
        b.extend_from_slice(&[r.horiz, r.vert, r.rdiag, r.ldiag, r.inv]);
        if version >= K_BLEND_TILE_VERSION_3 {
            b.push(r.longd);
        }
        if version >= K_BLEND_TILE_VERSION_4 {
            put_i32(&mut b, r.custom);
        }
        put_i32(&mut b, r.flag);
    }
    if version >= K_BLEND_TILE_VERSION_5 {
        for c in &p.cliffs {
            put_i32(&mut b, c.tile_idx);
            for f in c.uvs {
                put_f32(&mut b, f);
            }
            b.push(c.flip);
            b.push(c.mutant);
        }
    }
    b
}

/// A quick record with the sentinel.
fn good(ndx: i32) -> BlendRec {
    BlendRec {
        ndx,
        horiz: 0,
        vert: 0,
        rdiag: 0,
        ldiag: 0,
        inv: 0,
        longd: 0,
        custom: 0,
        flag: FLAG_VAL,
    }
}

// --- the harness (the constructor's full-load walk) -----------------------------

/// The walk's user state: the shared height-map members (the original's
/// `this`, cut down), the live data-size word, and the blend payload.
struct Walk {
    hm: HeightMapData,
    hm_loaded: bool,
    live_data_size: i32,
    bt: Option<rzh_formats::map_blendtile::BlendTileData>,
    use3: bool,
    error: Option<BlendTileError>,
}

fn hm_parser(
    file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
    _table: &mut ParserTable<'_, Walk>,
    user: &mut Walk,
) -> bool {
    match parse_height_map_data(file, info) {
        Ok(hm) => {
            user.live_data_size = hm.data_size();
            user.hm = hm;
            user.hm_loaded = true;
            true
        }
        Err(e) => {
            user.error = Some(BlendTileError::HeightMap(e));
            false
        }
    }
}

fn bt_parser(
    file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
    _table: &mut ParserTable<'_, Walk>,
    user: &mut Walk,
) -> bool {
    let map = SharedHeightMap {
        width: user.hm.width,
        height: user.hm.height,
        data_size: user.live_data_size,
        data: &user.hm.data,
    };
    match rzh_formats::map_blendtile::parse_blend_tile_data(file, info, &map, user.use3) {
        Ok(bt) => {
            user.hm.width = bt.width;
            user.hm.height = bt.height;
            user.live_data_size = bt.data_size;
            user.bt = Some(bt);
            true
        }
        Err(e) => {
            user.error = Some(e);
            false
        }
    }
}

/// A recursing container observer (the scope corpus's ObjectsList).
fn container(
    file: &mut DataChunkInput<'_>,
    _info: &DataChunkInfo,
    table: &mut ParserTable<'_, Walk>,
    user: &mut Walk,
) -> bool {
    file.parse(table, user)
}

/// The error line for a refused load (the same fields the C++ harness
/// dumps from the surviving state at the throw).
fn err_line(id: &str, m: &str, error: &BlendTileError) -> String {
    match error {
        BlendTileError::HeightMap(e) => match e {
            HeightMapError::InvalidMapSize {
                width,
                height,
                border_size,
            } => format!("herr {id} {m} mapsize {width} {height} {border_size}"),
            HeightMapError::InvalidHeightDataSize {
                data_size,
                width,
                height,
                size_left,
            } => format!("herr {id} {m} datasize {data_size} {width} {height} {size_left}"),
            HeightMapError::CorruptBorderCount(n) => format!("herr {id} {m} bordercount {n}"),
            HeightMapError::CorruptFileFormat => format!("herr {id} {m} corrupt"),
        },
        BlendTileError::DataSizeMismatch { len, data_size } => {
            format!("herr {id} {m} blenlen {len} {data_size}")
        }
        BlendTileError::InvalidBlendFlag { index, flag } => {
            format!("herr {id} {m} flag {index} {flag}")
        }
        BlendTileError::TooManyBlendedTiles(n) => format!("herr {id} {m} nblend {n}"),
        BlendTileError::TooManyCliffInfos(n) => format!("herr {id} {m} ncliff {n}"),
        BlendTileError::TooManyTextureClasses(n) => format!("herr {id} {m} ntex {n}"),
        BlendTileError::TooManyEdgeTextureClasses(n) => format!("herr {id} {m} netex {n}"),
        BlendTileError::PatchWithoutBlendTiles { data_size } => {
            format!("herr {id} {m} patchnull {data_size}")
        }
        BlendTileError::CorruptFileFormat => format!("herr {id} {m} corrupt"),
    }
}

/// Dumps the blend half (pre-patch) — the bt line, the raw arrays, the
/// bitmaps, the classes, the records.
fn dump_blend_raw(
    id: &str,
    m: &str,
    bt: &rzh_formats::map_blendtile::BlendTileData,
) -> Vec<String> {
    let mut lines = vec![format!(
        "bt {id} {m} {} {} {} {} {} {} {} {} {} {} {}",
        bt.tile_ndxes.len(),
        bt.num_bitmap_tiles,
        bt.num_blended_tiles,
        bt.num_cliff_info,
        bt.num_texture_classes,
        bt.num_edge_tiles,
        bt.num_edge_texture_classes,
        bt.width,
        bt.height,
        bt.data_size,
        bt.flip_state_width
    )];
    lines.push(format!("traw {id} {m} {}", i16_list(&bt.tile_ndxes)));
    lines.push(format!("braw {id} {m} {}", i16_list(&bt.blend_tile_ndxes)));
    lines.push(format!(
        "xraw {id} {m} {}",
        i16_list(&bt.extra_blend_tile_ndxes)
    ));
    lines.push(format!("craw {id} {m} {}", i16_list(&bt.cliff_info_ndxes)));
    lines.push(format!("flip {id} {m} {}", hex(&bt.cell_flip_state)));
    lines.push(format!("cst {id} {m} {}", hex(&bt.cell_cliff_state)));
    for (i, t) in bt.texture_classes.iter().enumerate() {
        lines.push(format!(
            "tex {id} {m} {i} {} {} {} {}",
            t.first_tile,
            t.num_tiles,
            t.width,
            q(&t.name)
        ));
    }
    for (i, t) in bt.edge_texture_classes.iter().enumerate() {
        lines.push(format!(
            "etex {id} {m} {i} {} {} {} {}",
            t.first_tile,
            t.num_tiles,
            t.width,
            q(&t.name)
        ));
    }
    for i in 1..bt.num_blended_tiles {
        let r: &BlendTileInfo = &bt.blended_tiles[i as usize];
        lines.push(format!(
            "bti {id} {m} {i} {} {} {} {} {} {} {} {}",
            r.blend_ndx,
            r.horiz,
            r.vert,
            r.right_diagonal,
            r.left_diagonal,
            r.inverted,
            r.long_diagonal,
            r.custom_blend_edge_class
        ));
    }
    for i in 1..bt.num_cliff_info {
        let c: &CliffInfo = &bt.cliff_infos[i as usize];
        lines.push(format!(
            "cfi {id} {m} {i} {} {} {} {} {} {} {} {} {} {} {}",
            c.tile_index,
            hex_f32(c.u0),
            hex_f32(c.v0),
            hex_f32(c.u1),
            hex_f32(c.v1),
            hex_f32(c.u2),
            hex_f32(c.v2),
            hex_f32(c.u3),
            hex_f32(c.v3),
            if c.flip { 1 } else { 0 },
            if c.mutant { 1 } else { 0 }
        ));
    }
    lines
}

/// The full per-corpus replay: the valid line, then both 3-way modes'
/// walks with their dumps, the patch, and the from_map cross-check.
fn replay_corpus(id: &str, bytes: &[u8], with_container: bool) -> Vec<String> {
    let mut lines = vec![format!(
        "valid {id} {}",
        tf(DataChunkInput::new(bytes).is_valid_file_type())
    )];
    for &use3 in &[true, false] {
        let m = if use3 { "3" } else { "n" };
        let mut input = DataChunkInput::new(bytes);
        let mut table: ParserTable<'_, Walk> = ParserTable::new();
        table.register(HEIGHT_MAP_DATA_LABEL, b"", hm_parser);
        table.register(BLEND_TILE_DATA_LABEL, b"", bt_parser);
        if with_container {
            table.register(b"ObjectsList", b"", container);
        }
        let mut user = Walk {
            hm: HeightMapData::default(),
            hm_loaded: false,
            live_data_size: 0,
            bt: None,
            use3,
            error: None,
        };
        let ok = input.parse(&mut table, &mut user);

        lines.push(format!("walk {id} {m} {}", tf(ok)));
        if !ok {
            let line = match &user.error {
                Some(e) => err_line(id, m, e),
                None => format!("herr {id} {m} corrupt"),
            };
            lines.push(line);
        } else {
            // the height-map half, as the FULL walk leaves it
            if user.hm_loaded {
                lines.push(format!(
                    "hm {id} {m} {} {} {} {} {}",
                    user.hm.width,
                    user.hm.height,
                    user.hm.border_size,
                    user.live_data_size,
                    user.hm.boundaries.len()
                ));
                for (i, b) in user.hm.boundaries.iter().enumerate() {
                    lines.push(format!("bnd {id} {m} {i} {} {}", b.x, b.y));
                }
                lines.push(format!("hdat {id} {m} {}", hex(&user.hm.data)));
            } else {
                lines.push(format!("hnone {id} {m}"));
            }
            // the blend half, pre-patch
            if let Some(bt) = &user.bt {
                lines.extend(dump_blend_raw(id, m, bt));
            } else {
                lines.push(format!("bnone {id} {m}"));
            }
            // the constructor's post-walk patch loop
            let data_size = user.live_data_size;
            if data_size > 0 && user.bt.is_none() {
                lines.push(format!("herr {id} {m} patchnull {data_size}"));
            } else {
                if let Some(bt) = &mut user.bt {
                    bt.patch_bad_maps(data_size);
                }
                // (no blend payload: the null arrays dump as empty)
                let empty: [String; 4] = std::array::from_fn(|_| "-".to_string());
                let [t, bl, x, c] = match user.bt.as_ref() {
                    Some(bt) => [
                        i16_list(&bt.tile_ndxes),
                        i16_list(&bt.blend_tile_ndxes),
                        i16_list(&bt.extra_blend_tile_ndxes),
                        i16_list(&bt.cliff_info_ndxes),
                    ],
                    None => empty,
                };
                lines.push(format!("tndx {id} {m} {t}"));
                lines.push(format!("bndx {id} {m} {bl}"));
                lines.push(format!("xndx {id} {m} {x}"));
                lines.push(format!("cndx {id} {m} {c}"));
            }
        }
        lines.push(format!(
            "bstate {id} {m} {} {} {}",
            input.tell(),
            input.stack_depth(),
            tf(input.at_end_of_file())
        ));

        // the from_map cross-check: the module's own walk must agree
        // with the manual one (verdict, payloads, error)
        let from_map = TerrainLoad::from_map(bytes, use3);
        let manual = if !ok {
            Err(user.error.unwrap_or(BlendTileError::CorruptFileFormat))
        } else if user.live_data_size > 0 && user.bt.is_none() {
            Err(BlendTileError::PatchWithoutBlendTiles {
                data_size: user.live_data_size,
            })
        } else {
            Ok(TerrainLoad {
                height_map: if user.hm_loaded {
                    Some(user.hm.clone())
                } else {
                    None
                },
                blend_tiles: user.bt.clone(),
                width: user.hm.width,
                height: user.hm.height,
                data_size: user.live_data_size,
            })
        };
        assert_eq!(
            from_map, manual,
            "{id}/{m}: from_map disagreed with the manual walk"
        );
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

/// The corpus's replay lines, minus the `file` line.
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
fn golden_mapblendtile_replay() {
    let corpora = parse_golden();
    assert!(
        corpora.len() >= 39,
        "expected the full corpus set, found {}",
        corpora.len()
    );

    for (id, lines) in &corpora {
        let bytes = corpus_bytes(lines, id);
        let with_container = id == "bt_scope";
        let replay = replay_corpus(id, &bytes, with_container);
        let golden = golden_replay_lines(lines);
        assert_eq!(golden, replay, "{id}: golden replay drifted");
    }
}

/// The corpus bytes rebuild through the same formula builders the C++
/// driver uses — pins the corpus construction itself.
#[test]
fn golden_mapblendtile_bytes_lockstep() {
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

    // bt_v8: the modern full load.
    let mut b = toc_bytes(&[
        (b"HeightMapData", 1),
        (b"BlendTileData", 2),
        (b"GlobalLighting", 3),
        (b"SidesList", 4),
    ]);
    put_chunk(
        &mut b,
        1,
        K_HEIGHT_MAP_VERSION_4,
        &v4_payload(8, 6, 1, &[(6, 4), (8, 6), (7, 5)], &formula_bytes(48, 3)),
    );
    {
        let p = BtParts {
            tile: formula_i16s(48, 33),
            blend: formula_i16s(48, 44),
            extra: formula_i16s(48, 55),
            cliff: formula_i16s(48, 66),
            cliff_state: formula_bytes(6, 77),
            nbmp: 7,
            nblend: 5,
            ncliff: 3,
            tex: vec![
                TexRec {
                    first: 0,
                    num: 4,
                    width: 2,
                    name: b"GrassA".to_vec(),
                },
                TexRec {
                    first: 8,
                    num: 4,
                    width: 2,
                    name: b"SnowB".to_vec(),
                },
            ],
            n_edge_tiles: 24,
            etex: vec![TexRec {
                first: 24,
                num: 4,
                width: 2,
                name: b"RoadC".to_vec(),
            }],
            blends: vec![
                BlendRec {
                    ndx: 10,
                    horiz: 1,
                    vert: 2,
                    rdiag: 3,
                    ldiag: 4,
                    inv: 0x00,
                    longd: 1,
                    custom: 2,
                    flag: FLAG_VAL,
                },
                BlendRec {
                    ndx: 11,
                    horiz: 0,
                    vert: 1,
                    rdiag: 0,
                    ldiag: 1,
                    inv: 0x01,
                    longd: 0,
                    custom: -1,
                    flag: FLAG_VAL,
                },
                BlendRec {
                    ndx: 12,
                    horiz: 255,
                    vert: 254,
                    rdiag: 253,
                    ldiag: 252,
                    inv: 0x02,
                    longd: 1,
                    custom: 0,
                    flag: FLAG_VAL,
                },
                BlendRec {
                    ndx: 13,
                    horiz: 128,
                    vert: 127,
                    rdiag: 1,
                    ldiag: 2,
                    inv: 0x03,
                    longd: 1,
                    custom: 5,
                    flag: FLAG_VAL,
                },
            ],
            cliffs: vec![
                CliffRec {
                    tile_idx: 70000,
                    uvs: [-1.5, 0.0, -0.0, 1e-40, 1e30, 0.25, -3.75, 1000.0],
                    flip: 2,
                    mutant: 0,
                },
                CliffRec {
                    tile_idx: -70000,
                    uvs: [0.5, -0.5, 2.0, -2.0, 0.125, -0.125, 3.5, -3.5],
                    flip: 0,
                    mutant: 0x80,
                },
            ],
            ..BtParts::new()
        };
        put_chunk(&mut b, 2, K_BLEND_TILE_VERSION_8, &blend_payload(8, 48, &p));
    }
    {
        let mut d = Vec::new();
        put_i32(&mut d, 0);
        put_chunk(&mut b, 3, 2, &d);
    }
    put_chunk(&mut b, 4, 1, &[0x55; 4]);
    check("bt_v8", b);

    // bt_3way: the FLIPPED_MASK and extra-flush gates.
    let mut b = toc_bytes(&[(b"HeightMapData", 1), (b"BlendTileData", 2)]);
    put_chunk(
        &mut b,
        1,
        K_HEIGHT_MAP_VERSION_2,
        &v2_payload(4, 2, &formula_bytes(8, 9)),
    );
    {
        let p = BtParts {
            tile: vec![0, 1, 2, 3, 4, 5, 6, 7],
            blend: vec![0; 8],
            extra: vec![9, -9, 1, 2, 3, 4, 5, 6],
            cliff: vec![0, 1, 0, 1, 0, 1, 0, 1],
            cliff_state: formula_bytes(2, 21),
            nbmp: 1,
            nblend: 9,
            tex: vec![TexRec {
                first: 0,
                num: 4,
                width: 2,
                name: b"T".to_vec(),
            }],
            blends: [0x00u8, 0x01, 0x02, 0x03, 0x7f, 0x80, 0xfd, 0xff]
                .iter()
                .map(|&inv| BlendRec {
                    ndx: 0,
                    horiz: 0,
                    vert: 0,
                    rdiag: 0,
                    ldiag: 0,
                    inv,
                    longd: 0,
                    custom: -1,
                    flag: FLAG_VAL,
                })
                .collect(),
            ..BtParts::new()
        };
        put_chunk(&mut b, 2, K_BLEND_TILE_VERSION_8, &blend_payload(8, 8, &p));
    }
    check("bt_3way", b);

    // bt_v7: the row-length remap (17 wide).
    let mut b = toc_bytes(&[(b"HeightMapData", 1), (b"BlendTileData", 2)]);
    put_chunk(
        &mut b,
        1,
        K_HEIGHT_MAP_VERSION_3,
        &v3_payload(17, 3, 0, &formula_bytes(51, 13)),
    );
    {
        let p = BtParts {
            tile: formula_i16s(51, 15),
            blend: formula_i16s(51, 25),
            extra: formula_i16s(51, 35),
            cliff: formula_i16s(51, 45),
            cliff_state: formula_bytes(6, 88),
            nbmp: 2,
            nblend: 2,
            ncliff: 2,
            tex: vec![TexRec {
                first: 0,
                num: 4,
                width: 2,
                name: b"A17".to_vec(),
            }],
            blends: vec![BlendRec {
                ndx: 20,
                horiz: 1,
                vert: 0,
                rdiag: 1,
                ldiag: 0,
                inv: 0x01,
                longd: 1,
                custom: 3,
                flag: FLAG_VAL,
            }],
            cliffs: vec![CliffRec {
                tile_idx: 300,
                uvs: [0.0, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.5],
                flip: 1,
                mutant: 1,
            }],
            ..BtParts::new()
        };
        put_chunk(&mut b, 2, K_BLEND_TILE_VERSION_7, &blend_payload(7, 51, &p));
    }
    check("bt_v7", b);

    // bt_v7b: the equal row lengths, truncated one byte into the state.
    let mut b = toc_bytes(&[(b"HeightMapData", 1), (b"BlendTileData", 2)]);
    put_chunk(
        &mut b,
        1,
        K_HEIGHT_MAP_VERSION_2,
        &v2_payload(16, 2, &formula_bytes(32, 17)),
    );
    {
        let p = BtParts {
            tile: formula_i16s(32, 19),
            blend: formula_i16s(32, 29),
            extra: formula_i16s(32, 39),
            cliff: formula_i16s(32, 49),
            cliff_state: formula_bytes(4, 59),
            nbmp: 3,
            nblend: 4,
            ncliff: 2,
            tex: vec![TexRec {
                first: 0,
                num: 4,
                width: 2,
                name: b"X".to_vec(),
            }],
            blends: vec![good(1)],
            cliffs: vec![CliffRec {
                tile_idx: 5,
                uvs: [0.0; 8],
                flip: 0,
                mutant: 0,
            }],
            ..BtParts::new()
        };
        let full = blend_payload(7, 32, &p);
        put_i32_sized(
            &mut b,
            2,
            K_BLEND_TILE_VERSION_7,
            &full[..261],
            full.len() as i32,
        );
    }
    check("bt_v7b", b);

    // bt_v7c: the 1-wide extreme (no cliff-state bytes).
    let mut b = toc_bytes(&[(b"HeightMapData", 1), (b"BlendTileData", 2)]);
    put_chunk(
        &mut b,
        1,
        K_HEIGHT_MAP_VERSION_2,
        &v2_payload(1, 2, &formula_bytes(2, 23)),
    );
    {
        let p = BtParts {
            tile: vec![7, 8],
            blend: vec![0, 0],
            extra: vec![0, 0],
            cliff: vec![0, 0],
            nbmp: 1,
            ..BtParts::new()
        };
        put_chunk(&mut b, 2, K_BLEND_TILE_VERSION_7, &blend_payload(7, 2, &p));
    }
    check("bt_v7c", b);

    // bt_v6: the height-derived cliff state with both sides of the limit.
    let mut b = toc_bytes(&[(b"HeightMapData", 1), (b"BlendTileData", 2)]);
    let mut h = Vec::new();
    for y in 0..3 {
        for x in 0..5 {
            let v = if y == 0 && x >= 3 {
                16
            } else if y == 2 && x >= 3 {
                15
            } else {
                0
            };
            h.push(v);
        }
    }
    put_chunk(&mut b, 1, K_HEIGHT_MAP_VERSION_2, &v2_payload(5, 3, &h));
    {
        let p = BtParts {
            tile: formula_i16s(15, 27),
            blend: formula_i16s(15, 37),
            extra: formula_i16s(15, 47),
            cliff: formula_i16s(15, 57),
            nbmp: 2,
            nblend: 3,
            ncliff: 2,
            tex: vec![TexRec {
                first: 0,
                num: 4,
                width: 2,
                name: b"G6".to_vec(),
            }],
            blends: vec![
                BlendRec {
                    ndx: 30,
                    horiz: 1,
                    vert: 1,
                    rdiag: 1,
                    ldiag: 1,
                    inv: 0x01,
                    longd: 1,
                    custom: 7,
                    flag: FLAG_VAL,
                },
                BlendRec {
                    ndx: 31,
                    horiz: 2,
                    vert: 2,
                    rdiag: 2,
                    ldiag: 2,
                    inv: 0x02,
                    longd: 0,
                    custom: 8,
                    flag: FLAG_VAL,
                },
            ],
            cliffs: vec![CliffRec {
                tile_idx: 400,
                uvs: [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8],
                flip: 1,
                mutant: 0,
            }],
            ..BtParts::new()
        };
        put_chunk(&mut b, 2, K_BLEND_TILE_VERSION_6, &blend_payload(6, 15, &p));
    }
    check("bt_v6", b);

    // bt_v5: cliff indices and tables, no extra layer.
    let mut b = toc_bytes(&[(b"HeightMapData", 1), (b"BlendTileData", 2)]);
    let h: Vec<u8> = (0..3)
        .flat_map(|y| (0..5).map(move |x| if y == 0 && x >= 3 { 16u8 } else { 0 }))
        .collect();
    put_chunk(&mut b, 1, K_HEIGHT_MAP_VERSION_2, &v2_payload(5, 3, &h));
    {
        let p = BtParts {
            tile: formula_i16s(15, 31),
            blend: formula_i16s(15, 41),
            cliff: formula_i16s(15, 51),
            nbmp: 1,
            nblend: 2,
            ncliff: 3,
            tex: vec![TexRec {
                first: 0,
                num: 1,
                width: 1,
                name: b"V5".to_vec(),
            }],
            blends: vec![BlendRec {
                ndx: 40,
                horiz: 3,
                vert: 0,
                rdiag: 0,
                ldiag: 1,
                inv: 0x01,
                longd: 1,
                custom: 2,
                flag: FLAG_VAL,
            }],
            cliffs: vec![
                CliffRec {
                    tile_idx: 1,
                    uvs: [0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0],
                    flip: 0,
                    mutant: 0,
                },
                CliffRec {
                    tile_idx: 2,
                    uvs: [0.25, 0.75, 0.25, 0.75, 0.25, 0.75, 0.25, 0.75],
                    flip: 1,
                    mutant: 1,
                },
            ],
            ..BtParts::new()
        };
        put_chunk(&mut b, 2, K_BLEND_TILE_VERSION_5, &blend_payload(5, 15, &p));
    }
    check("bt_v5", b);

    // bt_v4: the edge classes.
    let mut b = toc_bytes(&[(b"HeightMapData", 1), (b"BlendTileData", 2)]);
    put_chunk(
        &mut b,
        1,
        K_HEIGHT_MAP_VERSION_2,
        &v2_payload(5, 3, &formula_bytes(15, 43)),
    );
    {
        let p = BtParts {
            tile: formula_i16s(15, 53),
            blend: formula_i16s(15, 63),
            nbmp: 1,
            nblend: 3,
            tex: vec![
                TexRec {
                    first: 0,
                    num: 4,
                    width: 2,
                    name: b"B4".to_vec(),
                },
                TexRec {
                    first: 4,
                    num: 4,
                    width: 2,
                    name: b"C4".to_vec(),
                },
            ],
            n_edge_tiles: 16,
            etex: vec![TexRec {
                first: 16,
                num: 4,
                width: 2,
                name: b"E4".to_vec(),
            }],
            blends: vec![
                BlendRec {
                    ndx: 50,
                    horiz: 1,
                    vert: 0,
                    rdiag: 0,
                    ldiag: 0,
                    inv: 0x01,
                    longd: 1,
                    custom: 0,
                    flag: FLAG_VAL,
                },
                BlendRec {
                    ndx: 51,
                    horiz: 0,
                    vert: 1,
                    rdiag: 0,
                    ldiag: 0,
                    inv: 0x00,
                    longd: 0,
                    custom: -1,
                    flag: FLAG_VAL,
                },
            ],
            ..BtParts::new()
        };
        put_chunk(&mut b, 2, K_BLEND_TILE_VERSION_4, &blend_payload(4, 15, &p));
    }
    check("bt_v4", b);

    // bt_v3.
    let mut b = toc_bytes(&[(b"HeightMapData", 1), (b"BlendTileData", 2)]);
    put_chunk(
        &mut b,
        1,
        K_HEIGHT_MAP_VERSION_3,
        &v3_payload(3, 2, 0, &formula_bytes(6, 47)),
    );
    {
        let p = BtParts {
            tile: vec![1, 2, 3, 4, 5, 6],
            blend: vec![0, 1, 0, 1, 0, 1],
            nbmp: 1,
            nblend: 2,
            tex: vec![TexRec {
                first: 0,
                num: 1,
                width: 1,
                name: b"D3".to_vec(),
            }],
            blends: vec![BlendRec {
                ndx: 60,
                horiz: 9,
                vert: 8,
                rdiag: 7,
                ldiag: 6,
                inv: 0x01,
                longd: 1,
                custom: 0,
                flag: FLAG_VAL,
            }],
            ..BtParts::new()
        };
        put_chunk(&mut b, 2, K_BLEND_TILE_VERSION_3, &blend_payload(3, 6, &p));
    }
    check("bt_v3", b);

    // bt_v2.
    let mut b = toc_bytes(&[(b"HeightMapData", 1), (b"BlendTileData", 2)]);
    put_chunk(
        &mut b,
        1,
        K_HEIGHT_MAP_VERSION_2,
        &v2_payload(3, 2, &formula_bytes(6, 51)),
    );
    {
        let p = BtParts {
            tile: vec![7, 8, 9, 10, 11, 12],
            blend: vec![2, 0, 2, 0, 2, 0],
            nbmp: 1,
            nblend: 2,
            tex: vec![TexRec {
                first: 0,
                num: 1,
                width: 1,
                name: b"E2".to_vec(),
            }],
            blends: vec![BlendRec {
                ndx: 70,
                horiz: 1,
                vert: 2,
                rdiag: 3,
                ldiag: 4,
                inv: 0x01,
                longd: 0,
                custom: 0,
                flag: FLAG_VAL,
            }],
            ..BtParts::new()
        };
        put_chunk(&mut b, 2, K_BLEND_TILE_VERSION_2, &blend_payload(2, 6, &p));
    }
    check("bt_v2", b);

    // bt_v0.
    let mut b = toc_bytes(&[(b"HeightMapData", 1), (b"BlendTileData", 2)]);
    put_chunk(&mut b, 1, 0, &v2_payload(2, 2, &formula_bytes(4, 55)));
    {
        let p = BtParts {
            tile: vec![1, 2, 3, 4],
            blend: vec![0; 4],
            nbmp: 1,
            ..BtParts::new()
        };
        put_chunk(&mut b, 2, 0, &blend_payload(0, 4, &p));
    }
    check("bt_v0", b);

    // bt_v1: the downsample.
    let mut b = toc_bytes(&[(b"HeightMapData", 1), (b"BlendTileData", 2)]);
    put_chunk(
        &mut b,
        1,
        K_HEIGHT_MAP_VERSION_1,
        &v2_payload(5, 5, &formula_bytes(25, 59)),
    );
    {
        let p = BtParts {
            tile: formula_i16s(25, 61),
            blend: formula_i16s(25, 71),
            nbmp: 2,
            nblend: 3,
            tex: vec![TexRec {
                first: 0,
                num: 4,
                width: 2,
                name: b"F1".to_vec(),
            }],
            blends: vec![
                BlendRec {
                    ndx: 80,
                    horiz: 1,
                    vert: 0,
                    rdiag: 0,
                    ldiag: 0,
                    inv: 0x01,
                    longd: 1,
                    custom: 0,
                    flag: FLAG_VAL,
                },
                BlendRec {
                    ndx: 81,
                    horiz: 0,
                    vert: 1,
                    rdiag: 0,
                    ldiag: 0,
                    inv: 0x01,
                    longd: 0,
                    custom: 0,
                    flag: FLAG_VAL,
                },
            ],
            ..BtParts::new()
        };
        put_chunk(&mut b, 2, K_BLEND_TILE_VERSION_1, &blend_payload(1, 25, &p));
    }
    check("bt_v1", b);

    // bt_v9.
    let mut b = toc_bytes(&[(b"HeightMapData", 1), (b"BlendTileData", 2)]);
    put_chunk(
        &mut b,
        1,
        K_HEIGHT_MAP_VERSION_2,
        &v2_payload(4, 2, &formula_bytes(8, 63)),
    );
    {
        let p = BtParts {
            tile: vec![1, 2, 3, 4, 5, 6, 7, 8],
            blend: vec![0, 1, 2, 3, 4, 5, 6, 7],
            extra: vec![7, 6, 5, 4, 3, 2, 1, 0],
            cliff: vec![1, 0, 1, 0, 1, 0, 1, 0],
            cliff_state: vec![0xaa, 0x55],
            nbmp: 1,
            nblend: 2,
            ncliff: 2,
            tex: vec![TexRec {
                first: 0,
                num: 1,
                width: 1,
                name: b"G9".to_vec(),
            }],
            blends: vec![BlendRec {
                ndx: 90,
                horiz: 1,
                vert: 1,
                rdiag: 1,
                ldiag: 1,
                inv: 0x01,
                longd: 1,
                custom: 1,
                flag: FLAG_VAL,
            }],
            cliffs: vec![CliffRec {
                tile_idx: 9,
                uvs: [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0],
                flip: 1,
                mutant: 1,
            }],
            ..BtParts::new()
        };
        put_chunk(&mut b, 2, 9, &blend_payload(9, 8, &p));
    }
    check("bt_v9", b);

    // bt_minimal.
    let mut b = toc_bytes(&[(b"HeightMapData", 1), (b"BlendTileData", 2)]);
    put_chunk(&mut b, 1, K_HEIGHT_MAP_VERSION_2, &v2_payload(1, 1, &[42]));
    {
        let p = BtParts {
            tile: vec![3],
            blend: vec![0],
            extra: vec![0],
            cliff: vec![0],
            cliff_state: vec![0x80],
            nbmp: 1,
            ..BtParts::new()
        };
        put_chunk(&mut b, 2, K_BLEND_TILE_VERSION_8, &blend_payload(8, 1, &p));
    }
    check("bt_minimal", b);

    // bt_names.
    let mut b = toc_bytes(&[(b"HeightMapData", 1), (b"BlendTileData", 2)]);
    put_chunk(
        &mut b,
        1,
        K_HEIGHT_MAP_VERSION_2,
        &v2_payload(2, 2, &formula_bytes(4, 65)),
    );
    {
        let p = BtParts {
            tile: vec![0; 4],
            blend: vec![0; 4],
            extra: vec![0; 4],
            cliff: vec![0; 4],
            cliff_state: vec![0, 0],
            nbmp: 1,
            tex: vec![
                TexRec {
                    first: 0,
                    num: 1,
                    width: 1,
                    name: Vec::new(),
                },
                TexRec {
                    first: 1,
                    num: 1,
                    width: 1,
                    name: b"ab\0cde".to_vec(),
                },
                TexRec {
                    first: 2,
                    num: 1,
                    width: 1,
                    name: b"AVeryLongTextureClassName001".to_vec(),
                },
            ],
            n_edge_tiles: 2,
            etex: vec![TexRec {
                first: 3,
                num: 1,
                width: 1,
                name: b"e\0f".to_vec(),
            }],
            ..BtParts::new()
        };
        put_chunk(&mut b, 2, K_BLEND_TILE_VERSION_8, &blend_payload(8, 4, &p));
    }
    check("bt_names", b);

    // bt_counts_neg.
    let mut b = toc_bytes(&[(b"HeightMapData", 1), (b"BlendTileData", 2)]);
    put_chunk(
        &mut b,
        1,
        K_HEIGHT_MAP_VERSION_2,
        &v2_payload(3, 2, &formula_bytes(6, 67)),
    );
    {
        let p = BtParts {
            tile: vec![5, 6, 7, 8, 9, 10],
            blend: vec![-1, 2, -3, 4, -5, 6],
            extra: vec![7, -8, 9, -10, 11, -12],
            cliff: vec![0, 1, 2, 3, 4, 5],
            cliff_state: vec![0xff, 0x00],
            nbmp: 2_000_000_000,
            nblend: -3,
            ncliff: -4,
            n_tex: -1,
            n_edge_tiles: -7,
            n_edge_tex: -2,
            ..BtParts::new()
        };
        put_chunk(&mut b, 2, K_BLEND_TILE_VERSION_8, &blend_payload(8, 6, &p));
    }
    check("bt_counts_neg", b);

    // bt_patch_clamps.
    let mut b = toc_bytes(&[(b"HeightMapData", 1), (b"BlendTileData", 2)]);
    put_chunk(
        &mut b,
        1,
        K_HEIGHT_MAP_VERSION_3,
        &v3_payload(5, 2, 0, &formula_bytes(10, 69)),
    );
    {
        let p = BtParts {
            tile: vec![100, 200, 300, 400, 500, 600, 700, 800, 900, 1000],
            blend: vec![0, 4, 5, -2, 1, 16192, -16192, 3, 2, 0],
            extra: vec![0, 0, 1, 99, 2, -5, 4, 4, 4, 4],
            cliff: vec![0, 1, 2, 99, -1, 2, 3, -32768, 32767, 5],
            cliff_state: vec![0x0f, 0xf0],
            nbmp: 1,
            nblend: 5,
            ncliff: 3,
            ..BtParts::new()
        };
        put_chunk(&mut b, 2, K_BLEND_TILE_VERSION_8, &blend_payload(8, 10, &p));
    }
    check("bt_patch_clamps", b);

    // bt_partial_patch.
    let mut b = toc_bytes(&[(b"HeightMapData", 1), (b"BlendTileData", 2)]);
    put_chunk(
        &mut b,
        1,
        K_HEIGHT_MAP_VERSION_2,
        &v2_payload(4, 4, &formula_bytes(16, 71)),
    );
    {
        let p = BtParts {
            tile: formula_i16s(16, 73),
            blend: vec![9; 16],
            extra: vec![-9; 16],
            cliff: vec![19; 16],
            cliff_state: formula_bytes(4, 83),
            nbmp: 1,
            nblend: 2,
            ncliff: 2,
            blends: vec![good(1)],
            cliffs: vec![CliffRec {
                tile_idx: 1,
                uvs: [0.0; 8],
                flip: 0,
                mutant: 0,
            }],
            ..BtParts::new()
        };
        put_chunk(&mut b, 2, K_BLEND_TILE_VERSION_8, &blend_payload(8, 16, &p));
    }
    put_chunk(
        &mut b,
        1,
        K_HEIGHT_MAP_VERSION_2,
        &v2_payload(2, 2, &formula_bytes(4, 85)),
    );
    check("bt_partial_patch", b);

    // bt_v1_then_v8.
    let mut b = toc_bytes(&[(b"HeightMapData", 1), (b"BlendTileData", 2)]);
    put_chunk(
        &mut b,
        1,
        K_HEIGHT_MAP_VERSION_1,
        &v2_payload(4, 4, &formula_bytes(16, 87)),
    );
    {
        let p = BtParts {
            tile: formula_i16s(16, 89),
            blend: formula_i16s(16, 91),
            nbmp: 1,
            nblend: 2,
            blends: vec![good(1)],
            ..BtParts::new()
        };
        put_chunk(&mut b, 2, K_BLEND_TILE_VERSION_1, &blend_payload(1, 16, &p));
    }
    {
        let p = BtParts {
            tile: vec![41, 42, 43, 44],
            blend: vec![0, 1, 0, 1],
            extra: vec![2, 2, 2, 2],
            cliff: vec![1, 1, 1, 1],
            cliff_state: formula_bytes(2, 93),
            nbmp: 2,
            nblend: 3,
            ncliff: 2,
            tex: vec![TexRec {
                first: 0,
                num: 1,
                width: 1,
                name: b"H8".to_vec(),
            }],
            blends: vec![
                BlendRec {
                    ndx: 2,
                    horiz: 1,
                    vert: 0,
                    rdiag: 0,
                    ldiag: 0,
                    inv: 0x01,
                    longd: 1,
                    custom: 0,
                    flag: FLAG_VAL,
                },
                BlendRec {
                    ndx: 3,
                    horiz: 0,
                    vert: 1,
                    rdiag: 0,
                    ldiag: 0,
                    inv: 0x02,
                    longd: 0,
                    custom: 1,
                    flag: FLAG_VAL,
                },
            ],
            cliffs: vec![CliffRec {
                tile_idx: 77,
                uvs: [0.5; 8],
                flip: 1,
                mutant: 0,
            }],
            ..BtParts::new()
        };
        put_chunk(&mut b, 2, K_BLEND_TILE_VERSION_8, &blend_payload(8, 4, &p));
    }
    check("bt_v1_then_v8", b);

    // bt_repeat.
    let mut b = toc_bytes(&[(b"HeightMapData", 1), (b"BlendTileData", 2)]);
    put_chunk(
        &mut b,
        1,
        K_HEIGHT_MAP_VERSION_2,
        &v2_payload(3, 2, &formula_bytes(6, 95)),
    );
    {
        let p = BtParts {
            tile: vec![1, 2, 3, 4, 5, 6],
            blend: vec![1; 6],
            extra: vec![2; 6],
            cliff: vec![3; 6],
            cliff_state: vec![0x11, 0x22],
            nbmp: 2,
            nblend: 2,
            ncliff: 2,
            tex: vec![TexRec {
                first: 0,
                num: 4,
                width: 2,
                name: b"R8".to_vec(),
            }],
            n_edge_tiles: 8,
            etex: vec![TexRec {
                first: 8,
                num: 4,
                width: 2,
                name: b"RE".to_vec(),
            }],
            blends: vec![BlendRec {
                ndx: 1,
                horiz: 1,
                vert: 1,
                rdiag: 1,
                ldiag: 1,
                inv: 0x01,
                longd: 1,
                custom: 1,
                flag: FLAG_VAL,
            }],
            cliffs: vec![CliffRec {
                tile_idx: 11,
                uvs: [1.0; 8],
                flip: 1,
                mutant: 1,
            }],
            ..BtParts::new()
        };
        put_chunk(&mut b, 2, K_BLEND_TILE_VERSION_8, &blend_payload(8, 6, &p));
    }
    {
        let p = BtParts {
            tile: vec![9, 8, 7, 6, 5, 4],
            blend: vec![0, 2, 0, 2, 0, 2],
            nbmp: 1,
            nblend: 2,
            tex: vec![TexRec {
                first: 0,
                num: 1,
                width: 1,
                name: b"R3".to_vec(),
            }],
            blends: vec![BlendRec {
                ndx: 2,
                horiz: 3,
                vert: 3,
                rdiag: 3,
                ldiag: 3,
                inv: 0x01,
                longd: 1,
                custom: 0,
                flag: FLAG_VAL,
            }],
            ..BtParts::new()
        };
        put_chunk(&mut b, 2, K_BLEND_TILE_VERSION_3, &blend_payload(3, 6, &p));
    }
    check("bt_repeat", b);

    // bt_blend_first.
    let mut b = toc_bytes(&[(b"HeightMapData", 1), (b"BlendTileData", 2)]);
    {
        let p = BtParts {
            nbmp: 2,
            nblend: 2,
            ncliff: 2,
            tex: vec![TexRec {
                first: 0,
                num: 1,
                width: 1,
                name: b"BF".to_vec(),
            }],
            blends: vec![BlendRec {
                ndx: 3,
                horiz: 1,
                vert: 1,
                rdiag: 1,
                ldiag: 1,
                inv: 0x01,
                longd: 1,
                custom: 1,
                flag: FLAG_VAL,
            }],
            cliffs: vec![CliffRec {
                tile_idx: 4,
                uvs: [1.0; 8],
                flip: 1,
                mutant: 1,
            }],
            ..BtParts::new()
        };
        put_chunk(&mut b, 2, K_BLEND_TILE_VERSION_8, &blend_payload(8, 0, &p));
    }
    put_chunk(
        &mut b,
        1,
        K_HEIGHT_MAP_VERSION_2,
        &v2_payload(3, 2, &formula_bytes(6, 97)),
    );
    check("bt_blend_first", b);

    // bt_badflag.
    let mut b = toc_bytes(&[(b"HeightMapData", 1), (b"BlendTileData", 2)]);
    put_chunk(
        &mut b,
        1,
        K_HEIGHT_MAP_VERSION_2,
        &v2_payload(2, 2, &formula_bytes(4, 99)),
    );
    {
        let mut p = BtParts {
            tile: vec![1, 2, 3, 4],
            blend: vec![0; 4],
            extra: vec![0; 4],
            cliff: vec![0; 4],
            cliff_state: vec![0, 0],
            nbmp: 1,
            nblend: 4,
            blends: vec![good(1), good(2), good(4)],
            ..BtParts::new()
        };
        p.blends.insert(
            2,
            BlendRec {
                ndx: 3,
                horiz: 0,
                vert: 0,
                rdiag: 0,
                ldiag: 0,
                inv: 0,
                longd: 0,
                custom: 0,
                flag: 0x7ADA_0001u32 as i32,
            },
        );
        put_chunk(&mut b, 2, K_BLEND_TILE_VERSION_8, &blend_payload(8, 4, &p));
    }
    check("bt_badflag", b);

    // bt_trunc_blend.
    let mut b = toc_bytes(&[(b"HeightMapData", 1), (b"BlendTileData", 2)]);
    put_chunk(
        &mut b,
        1,
        K_HEIGHT_MAP_VERSION_2,
        &v2_payload(2, 2, &formula_bytes(4, 101)),
    );
    {
        let p = BtParts {
            tile: vec![1, 2, 3, 4],
            blend: vec![0; 4],
            extra: vec![0; 4],
            cliff: vec![0; 4],
            cliff_state: vec![0, 0],
            nbmp: 1,
            nblend: 3,
            blends: vec![good(1), good(2)],
            ..BtParts::new()
        };
        let full = blend_payload(8, 4, &p);
        put_i32_sized(
            &mut b,
            2,
            K_BLEND_TILE_VERSION_8,
            &full[..60],
            full.len() as i32,
        );
    }
    check("bt_trunc_blend", b);

    // bt_short_arrays.
    let mut b = toc_bytes(&[(b"HeightMapData", 1), (b"BlendTileData", 2)]);
    put_chunk(
        &mut b,
        1,
        K_HEIGHT_MAP_VERSION_2,
        &v2_payload(4, 4, &formula_bytes(16, 103)),
    );
    {
        let p = BtParts {
            tile: formula_i16s(16, 105),
            blend: formula_i16s(16, 115),
            extra: formula_i16s(16, 125),
            cliff: formula_i16s(16, 135),
            cliff_state: formula_bytes(4, 145),
            nbmp: 5,
            nblend: 5,
            ncliff: 5,
            tex: vec![TexRec {
                first: 0,
                num: 1,
                width: 1,
                name: b"SA".to_vec(),
            }],
            blends: vec![good(1)],
            cliffs: vec![CliffRec {
                tile_idx: 1,
                uvs: [0.0; 8],
                flip: 0,
                mutant: 0,
            }],
            ..BtParts::new()
        };
        let full = blend_payload(8, 16, &p);
        put_i32_sized(
            &mut b,
            2,
            K_BLEND_TILE_VERSION_8,
            &full[..39],
            full.len() as i32,
        );
    }
    check("bt_short_arrays", b);

    // bt_len_mismatch.
    let mut b = toc_bytes(&[(b"HeightMapData", 1), (b"BlendTileData", 2)]);
    put_chunk(
        &mut b,
        1,
        K_HEIGHT_MAP_VERSION_2,
        &v2_payload(4, 4, &formula_bytes(16, 107)),
    );
    {
        let p = BtParts {
            nbmp: 1,
            ..BtParts::new()
        };
        put_chunk(&mut b, 2, K_BLEND_TILE_VERSION_8, &blend_payload(8, 12, &p));
    }
    check("bt_len_mismatch", b);

    // bt_hm_err.
    let mut b = toc_bytes(&[(b"HeightMapData", 1), (b"BlendTileData", 2)]);
    {
        let mut p = Vec::new();
        put_i32(&mut p, 0);
        put_i32(&mut p, 2);
        put_i32(&mut p, 4);
        put_chunk(&mut b, 1, K_HEIGHT_MAP_VERSION_2, &p);
    }
    {
        let p = BtParts {
            nbmp: 1,
            ..BtParts::new()
        };
        put_chunk(&mut b, 2, K_BLEND_TILE_VERSION_8, &blend_payload(8, 4, &p));
    }
    check("bt_hm_err", b);

    // the four count-over-bound corpora.
    for (id, seed, f) in [
        ("bt_nblend_over", 109u8, 1usize),
        ("bt_ncliff_over", 111, 2),
        ("bt_ntex_over", 113, 3),
        ("bt_netex_over", 115, 4),
    ] {
        let mut b = toc_bytes(&[(b"HeightMapData", 1), (b"BlendTileData", 2)]);
        put_chunk(
            &mut b,
            1,
            K_HEIGHT_MAP_VERSION_2,
            &v2_payload(2, 2, &formula_bytes(4, seed)),
        );
        let p = BtParts {
            tile: vec![0; 4],
            blend: vec![0; 4],
            extra: vec![0; 4],
            cliff: vec![0; 4],
            cliff_state: vec![0, 0],
            nbmp: 1,
            nblend: if f == 1 { NUM_BLEND_TILES + 1 } else { 1 },
            ncliff: if f == 2 { NUM_CLIFF_INFO + 1 } else { 1 },
            n_tex: if f == 3 { NUM_TEXTURE_CLASSES + 1 } else { -1 },
            n_edge_tiles: if f == 4 { 4 } else { 0 },
            n_edge_tex: if f == 4 { NUM_TEXTURE_CLASSES + 1 } else { -1 },
            ..BtParts::new()
        };
        put_chunk(&mut b, 2, K_BLEND_TILE_VERSION_8, &blend_payload(8, 4, &p));
        check(id, b);
    }

    // bt_height_only.
    let mut b = toc_bytes(&[(b"HeightMapData", 1), (b"GlobalLighting", 2)]);
    put_chunk(
        &mut b,
        1,
        K_HEIGHT_MAP_VERSION_2,
        &v2_payload(3, 2, &formula_bytes(6, 117)),
    );
    put_chunk(&mut b, 2, 2, &[0x33; 4]);
    check("bt_height_only", b);

    // bt_no_height.
    let mut b = toc_bytes(&[(b"BlendTileData", 1)]);
    {
        let p = BtParts {
            nbmp: 2,
            nblend: 2,
            ncliff: 2,
            tex: vec![TexRec {
                first: 0,
                num: 1,
                width: 1,
                name: b"NH".to_vec(),
            }],
            blends: vec![BlendRec {
                ndx: 5,
                horiz: 1,
                vert: 1,
                rdiag: 1,
                ldiag: 1,
                inv: 0x01,
                longd: 1,
                custom: 1,
                flag: FLAG_VAL,
            }],
            cliffs: vec![CliffRec {
                tile_idx: 6,
                uvs: [1.0; 8],
                flip: 1,
                mutant: 1,
            }],
            ..BtParts::new()
        };
        put_chunk(&mut b, 1, K_BLEND_TILE_VERSION_8, &blend_payload(8, 0, &p));
    }
    check("bt_no_height", b);

    // bt_none.
    let mut b = toc_bytes(&[(b"SidesList", 1)]);
    put_chunk(&mut b, 1, 1, &[0x66; 4]);
    check("bt_none", b);

    // bt_underdeclared.
    let mut b = toc_bytes(&[
        (b"HeightMapData", 1),
        (b"BlendTileData", 2),
        (b"UnknownPayload", 3),
    ]);
    put_chunk(
        &mut b,
        1,
        K_HEIGHT_MAP_VERSION_2,
        &v2_payload(2, 2, &formula_bytes(4, 119)),
    );
    {
        let mut d = Vec::new();
        put_i32(&mut d, 4);
        put_i16s(&mut d, &[1, 2, 3, 4]);
        put_i16s(&mut d, &[0, 0, 0, 0]);
        put_i32_sized(&mut b, 2, K_BLEND_TILE_VERSION_2, &d, 8);
    }
    put_i32_sized(&mut b, 3, 1, &[0x00; 8], 0x10000);
    check("bt_underdeclared", b);

    // bt_leftover.
    let mut b = toc_bytes(&[
        (b"HeightMapData", 1),
        (b"BlendTileData", 2),
        (b"UnknownPayload", 3),
    ]);
    put_chunk(
        &mut b,
        1,
        K_HEIGHT_MAP_VERSION_2,
        &v2_payload(2, 2, &formula_bytes(4, 121)),
    );
    {
        let p = BtParts {
            tile: vec![1, 2, 3, 4],
            blend: vec![0; 4],
            nbmp: 1,
            ..BtParts::new()
        };
        let mut d = blend_payload(2, 4, &p);
        d.extend_from_slice(&[0xee; 6]);
        put_chunk(&mut b, 2, K_BLEND_TILE_VERSION_2, &d);
    }
    put_chunk(&mut b, 3, 1, &[0x22; 4]);
    check("bt_leftover", b);

    // bt_scope.
    let mut b = toc_bytes(&[
        (b"ObjectsList", 1),
        (b"HeightMapData", 2),
        (b"BlendTileData", 3),
    ]);
    {
        let mut nested = Vec::new();
        {
            let mut d = Vec::new();
            put_i32(&mut d, 999);
            put_chunk(&mut nested, 3, K_BLEND_TILE_VERSION_8, &d);
        }
        {
            let mut obj = Vec::new();
            put_i32(&mut obj, 0);
            put_i32(&mut obj, 2);
            put_i32(&mut obj, 4);
            put_chunk(&mut nested, 2, 1, &obj);
        }
        put_chunk(&mut b, 1, 3, &nested);
    }
    put_chunk(
        &mut b,
        2,
        K_HEIGHT_MAP_VERSION_2,
        &v2_payload(2, 2, &formula_bytes(4, 123)),
    );
    {
        let p = BtParts {
            tile: vec![5, 6, 7, 8],
            blend: vec![0; 4],
            nbmp: 1,
            ..BtParts::new()
        };
        put_chunk(&mut b, 3, K_BLEND_TILE_VERSION_2, &blend_payload(2, 4, &p));
    }
    check("bt_scope", b);

    // bt_not_chunky.
    check("bt_not_chunky", b"not a map at all".to_vec());

    // bt_toc_only.
    check("bt_toc_only", toc_bytes(&[(b"HeightMapData", 1)]));

    // bt_empty.
    check("bt_empty", Vec::new());
}

/// The structs the harness reads stay nameable (a compile-time check
/// that the module's public surface is complete for the replay).
#[test]
fn the_public_surface_is_replayable() {
    let hm = HeightMapData {
        width: 1,
        height: 1,
        border_size: 0,
        boundaries: vec![Boundary { x: 1, y: 1 }],
        data: vec![0],
    };
    let view = SharedHeightMap::of(&hm);
    assert_eq!((view.width, view.height, view.data_size), (1, 1, 1));
    let _ = TxTextureClass::default();
    let _ = BlendTileInfo::default();
    let _ = CliffInfo::default();
}
