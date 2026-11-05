// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Rust port Copyright 2026 the RustZeroHour contributors.
//
//! Differential parity tests for the DDS texture port (P0).
//!
//! `tests/golden/dds.txt` is a script produced by
//! `tools/differential/gen_dds_golden.cpp` — a verbatim transcription
//! of the Zero Hour DDS reader's data path
//! (`GeneralsMD/Code/Libraries/Source/WWVegas/WW3D2/ddsfile.cpp` at the
//! baseline commit, plus the formconv FourCC conversion, the
//! bitmaphandler pixel writer and the colorspace recolour stack the
//! hue-shifted copies drive).
//!
//! The script pins:
//!
//! - the corpus bytes (`file`) and the munged name (`name`) — the
//!   constructor's `.dds` overwrite of the last three characters,
//!   whatever they were (`tank` → `tdds`, `abc` → `dds`, the
//!   255-character truncation of an over-long name included);
//! - the constructor verdicts (`ok`/`err`) — the empty file, the
//!   magic-only file, the short description and every `Size` mismatch;
//! - the header view (`hdr`) — the reduced width/height/depth (the
//!   else branch's decremented reduction factor, the width shifted
//!   down to 1), the addressable mip count (two fewer than the chain
//!   holds), the FourCC-derived format (the numeric 22 → X8R8G8B8
//!   route through the conversion table, the unknown FourCC), the type
//!   and the cube face stride;
//! - the level arrays (`lvls`) — the DXTC block sizes with the volume
//!   depth multiplier and the 4/8 divisor, the `> 16` guard that keeps
//!   a single-block level at 8 bytes, and the prefix-sum offsets;
//! - the load verdicts and the read windows (`load`/`data`) — the
//!   dropped-but-still-read finest levels, the skipped reduced levels,
//!   and the underflow shape whose guard refuses the read while the
//!   load still succeeds;
//! - the pixel queries (`px`) — both DXT1 colour modes (the 85/255 and
//!   128/255 interpolations, the transparent code 3), the DXT5 8- and
//!   6-alpha tables over the always-interpolated colour block (the
//!   index pattern's two halves differ, so the unrolled reader's byte
//!   advance is pinned), the DXT2-4 and non-DXT opaque white, and the
//!   level-1/level-2 block addressing;
//! - the block decompression (`blk`/`blkhsv`) into every destination
//!   format — the R3G3B2 two-byte store with its zero-extended stray
//!   byte over the `0xcc` fill, the L8 Rec. 709 weights, the
//!   contains-alpha verdicts, and the hue-shifted endpoint recolour;
//! - the surface copies (`cpy`/`cpycube`/`cpyvol`) — the verbatim
//!   level memcpy, the DXT1/DXT5 hue-shift rewrites, the DXT1→DXT2
//!   expansion (with and without the shift), the decompressing walk,
//!   the untouched destinations (the DXT3 shift branch, the size
//!   mismatch, the dead volume pointer), and the DXT5 cube's 16-byte
//!   face-stride drift;
//! - the cube pointers (`cubeptr`), the dead volume pointer
//!   (`volptr`), the recolour round trip (`recolor`) and the RGB565
//!   conversions (`rgb565`).
//!
//! The test replays the script against the Rust port and asserts
//! line-by-line equality. The corpus table (the constructor's name and
//! reduction arguments) mirrors the generator's case list.

use std::collections::BTreeMap;
use std::path::PathBuf;

use rzh_formats::dds::{
    DdsError, DdsFile, DdsType, HsvShift, Ww3dFormat, argb8888_to_rgb565, munged_name, recolor,
    rgb565_to_argb8888,
};

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join("dds.txt")
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

/// The dump's hex (empty renders as `-`).
fn hex(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return "-".to_string();
    }
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

fn parse_hsv(fields: &[&str]) -> HsvShift {
    // The C++ dump writes %g floats; parse them back.
    HsvShift {
        h: fields[0].parse().expect("h float"),
        s: fields[1].parse().expect("s float"),
        v: fields[2].parse().expect("v float"),
    }
}

/// The dump's destination pitch: one tightly packed row (the DXT
/// destinations never read the pitch).
fn dump_pitch(format: Ww3dFormat, width: u32) -> usize {
    let bpp = format.bytes_per_pixel();
    width as usize * if bpp == 0 { 1 } else { bpp as usize }
}

/// One replayed case.
struct Case {
    bytes: Vec<u8>,
    /// `Some(..)` once the `name` line has driven the constructor.
    opened: Option<Result<DdsFile, DdsError>>,
}

#[test]
fn golden_dds() {
    let text = std::fs::read_to_string(golden_path()).expect("golden file");
    let mut cases: BTreeMap<String, Case> = BTreeMap::new();
    // The replayed output, in script order (every line but `file`).
    let mut actual: Vec<String> = Vec::new();
    let expect_line = |actual: &mut Vec<String>, rendered: String, fields: &[&str], what: &str| {
        assert_eq!(rendered, fields.join(" "), "{what} line mismatch");
        actual.push(rendered);
    };

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        let id = fields[1].to_string();

        match fields[0] {
            "file" => {
                cases.insert(
                    id.clone(),
                    Case {
                        bytes: unhex(fields[2]),
                        opened: None,
                    },
                );
            }
            "name" => {
                let case = cases.get_mut(&id).expect("case with file line");
                let name = corpus_name(&id);
                let reduction = corpus_reduction(&id);
                case.opened = Some(DdsFile::open(&name, reduction, &case.bytes));

                let expected = unhex(fields[2]);
                match &case.opened {
                    Some(Ok(file)) => {
                        assert_eq!(file.name().as_bytes(), expected, "munged name for {id}");
                    }
                    Some(Err(_)) => {
                        // The constructor failed after the munging; the
                        // name is still the munging's output.
                        assert_eq!(
                            munged_name(&name).expect("munge").as_bytes(),
                            expected,
                            "munged name for failed {id}"
                        );
                    }
                    None => unreachable!("name before file for {id}"),
                }
                actual.push(format!("name {id} {}", hex(&expected)));
            }
            "ok" => {
                let case = &cases[&id];
                assert!(
                    case.opened.as_ref().expect("open").is_ok(),
                    "expected ok for {id}"
                );
                actual.push(format!("ok {id}"));
            }
            "err" => {
                let case = &cases[&id];
                assert!(
                    case.opened.as_ref().expect("open").is_err(),
                    "expected err for {id}"
                );
                actual.push(format!("err {id}"));
            }
            "hdr" => {
                let case = &cases[&id];
                let file = case
                    .opened
                    .as_ref()
                    .expect("open")
                    .as_ref()
                    .expect("ok file");
                let values = [
                    file.reduced_width(),
                    file.reduced_height(),
                    file.reduced_depth(),
                    file.full_width(),
                    file.full_height(),
                    file.full_depth(),
                    file.mip_level_count(),
                    file.reduction_factor(),
                    file.format() as u32,
                    file.kind() as u32,
                    file.cube_face_size(),
                ];
                let rendered: Vec<String> = values.iter().map(ToString::to_string).collect();
                expect_line(
                    &mut actual,
                    format!("hdr {id} {}", rendered.join(" ")),
                    &fields,
                    "hdr",
                );
            }
            "lvls" => {
                let case = &cases[&id];
                let file = case
                    .opened
                    .as_ref()
                    .expect("open")
                    .as_ref()
                    .expect("ok file");
                let mut rendered = format!("lvls {id}");
                if fields[2] != "-" {
                    for level in 0..file.mip_level_count() {
                        let size = file.level_size(level).expect("level size");
                        let offset = file.level_offset(level).expect("level offset");
                        rendered.push_str(&format!(" {size}:{offset}"));
                    }
                }
                expect_line(&mut actual, rendered, &fields, "lvls");
            }
            "load" => {
                let case = cases.get_mut(&id).expect("case");
                let verdict = match &mut case.opened {
                    Some(Ok(file)) => file.load(&case.bytes).is_ok(),
                    Some(Err(_)) => false,
                    None => unreachable!("load before open for {id}"),
                };
                expect_line(
                    &mut actual,
                    format!("load {id} {}", if verdict { "ok" } else { "err" }),
                    &fields,
                    "load",
                );
            }
            "data" => {
                let case = &cases[&id];
                let file = case
                    .opened
                    .as_ref()
                    .expect("open")
                    .as_ref()
                    .expect("ok file");
                expect_line(
                    &mut actual,
                    format!("data {id} {}", hex(file.data())),
                    &fields,
                    "data",
                );
            }
            "px" => {
                let case = &cases[&id];
                let file = case
                    .opened
                    .as_ref()
                    .expect("open")
                    .as_ref()
                    .expect("ok file");
                let level: u32 = fields[2].parse().expect("level");
                let x: u32 = fields[3].parse().expect("x");
                let y: u32 = fields[4].parse().expect("y");
                let pixel = file.get_pixel(level, x, y).expect("pixel");
                expect_line(
                    &mut actual,
                    format!("px {id} {level} {x} {y} {pixel:08x}"),
                    &fields,
                    "px",
                );
            }
            "blk" | "blkhsv" => {
                let case = &cases[&id];
                let file = case
                    .opened
                    .as_ref()
                    .expect("open")
                    .as_ref()
                    .expect("ok file");
                let (level, sx, sy, fmt_name, hsv, hex_at) = if fields[0] == "blk" {
                    (
                        fields[2].parse::<u32>().expect("level"),
                        fields[3].parse::<u32>().expect("sx"),
                        fields[4].parse::<u32>().expect("sy"),
                        fields[5],
                        HsvShift::none(),
                        6,
                    )
                } else {
                    (
                        fields[2].parse::<u32>().expect("level"),
                        fields[3].parse::<u32>().expect("sx"),
                        fields[4].parse::<u32>().expect("sy"),
                        fields[5],
                        parse_hsv(&fields[6..9]),
                        9,
                    )
                };
                let format = Ww3dFormat::from_name(fmt_name).expect("format name");
                let bpp = format.bytes_per_pixel() as usize;
                let mut scratch = vec![0xccu8; 4 * 4 * bpp + 4];
                let alpha = file
                    .get_4x4_block(&mut scratch, 4 * bpp, format, level, sx, sy, hsv)
                    .expect("block");
                expect_line(
                    &mut actual,
                    format!(
                        "{} {} {}",
                        fields[..hex_at].join(" "),
                        hex(&scratch),
                        if alpha { "1" } else { "0" }
                    ),
                    &fields,
                    fields[0],
                );
            }
            "cpy" | "cpycube" | "cpyvol" => {
                let case = cases.get_mut(&id).expect("case");
                let file = case
                    .opened
                    .as_ref()
                    .expect("open")
                    .as_ref()
                    .expect("ok file");
                let (face, level, fmt_name, w, h, hsv_fields, hex_at) = if fields[0] == "cpy" {
                    (
                        None,
                        fields[2].parse::<u32>().expect("level"),
                        fields[3],
                        fields[4].parse::<u32>().expect("w"),
                        fields[5].parse::<u32>().expect("h"),
                        &fields[6..9],
                        9,
                    )
                } else if fields[0] == "cpycube" {
                    (
                        Some(fields[2].parse::<u32>().expect("face")),
                        fields[3].parse::<u32>().expect("level"),
                        fields[4],
                        fields[5].parse::<u32>().expect("w"),
                        fields[6].parse::<u32>().expect("h"),
                        &fields[7..10],
                        10,
                    )
                } else {
                    (
                        None,
                        fields[2].parse::<u32>().expect("level"),
                        fields[3],
                        fields[4].parse::<u32>().expect("w"),
                        fields[5].parse::<u32>().expect("h"),
                        &fields[6..9],
                        9,
                    )
                };
                let hsv = parse_hsv(hsv_fields);
                let format = Ww3dFormat::from_name(fmt_name).expect("format name");
                let pitch = dump_pitch(format, w);
                let mut scratch = vec![0xccu8; pitch * h as usize + 4];
                match fields[0] {
                    "cpy" => file
                        .copy_level_to_surface(level, format, w, h, &mut scratch, pitch, hsv)
                        .expect("cpy"),
                    "cpycube" => file
                        .copy_cube_map_level_to_surface(
                            face.expect("face"),
                            level,
                            format,
                            w,
                            h,
                            &mut scratch,
                            pitch,
                            hsv,
                        )
                        .expect("cpycube"),
                    _ => file
                        .copy_volume_level_to_surface(
                            level,
                            0,
                            format,
                            w,
                            h,
                            &mut scratch,
                            pitch,
                            pitch,
                            hsv,
                        )
                        .expect("cpyvol"),
                }
                expect_line(
                    &mut actual,
                    format!("{} {}", fields[..hex_at].join(" "), hex(&scratch)),
                    &fields,
                    fields[0],
                );
            }
            "cubeptr" => {
                let case = &cases[&id];
                let file = case
                    .opened
                    .as_ref()
                    .expect("open")
                    .as_ref()
                    .expect("ok file");
                let face: u32 = fields[2].parse().expect("face");
                let level: u32 = fields[3].parse().expect("level");
                let offset = file
                    .cube_face_size()
                    .wrapping_mul(face)
                    .wrapping_add(file.level_offset(level).expect("level offset"));
                expect_line(
                    &mut actual,
                    format!("cubeptr {id} {face} {level} {offset}"),
                    &fields,
                    "cubeptr",
                );
            }
            "volptr" => {
                let case = &cases[&id];
                let file = case
                    .opened
                    .as_ref()
                    .expect("open")
                    .as_ref()
                    .expect("ok file");
                let level: u32 = fields[2].parse().expect("level");
                assert!(
                    file.volume_memory_pointer(level).is_none(),
                    "the volume pointer is the dead stub"
                );
                expect_line(
                    &mut actual,
                    format!("volptr {id} {level} -"),
                    &fields,
                    "volptr",
                );
            }
            "recolor" => {
                let input = u32::from_str_radix(fields[2], 16).expect("recolor input");
                let hsv = parse_hsv(&fields[3..6]);
                let expect_out = u32::from_str_radix(fields[6], 16).expect("recolor output");
                assert_eq!(
                    recolor(input, hsv),
                    expect_out,
                    "recolor {} with {hsv:?}",
                    fields[1]
                );
                actual.push(fields.join(" "));
            }
            "rgb565" => {
                let input = u16::from_str_radix(fields[2], 16).expect("rgb565 input");
                let argb = rgb565_to_argb8888(input);
                let back = argb8888_to_rgb565(argb);
                let expect_argb = u32::from_str_radix(fields[3], 16).expect("argb");
                let expect_back = u16::from_str_radix(fields[4], 16).expect("back");
                assert_eq!(argb, expect_argb, "rgb565 argb for {}", fields[1]);
                assert_eq!(back, expect_back, "rgb565 back for {}", fields[1]);
                actual.push(fields.join(" "));
            }
            other => panic!("unknown golden command {other:?}"),
        }
    }

    // Every non-`file` line must have been replayed in order.
    let expected_flow: Vec<String> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with("file "))
        .map(ToString::to_string)
        .collect();
    assert_eq!(
        actual, expected_flow,
        "the replay must cover the golden script in order"
    );

    // Type enum sanity for the golden's numeric encodings.
    assert_eq!(DdsType::Texture as u32, 0);
    assert_eq!(DdsType::Cubemap as u32, 1);
    assert_eq!(DdsType::Volume as u32, 2);
}

/// The corpus names — the constructor's munging input, mirroring the
/// generator's case table.
fn corpus_name(id: &str) -> String {
    match id {
        "n_tga" => "textures/tank.tga".to_string(),
        "n_dds" => "tank.dds".to_string(),
        "n_bare" => "tank".to_string(),
        "n_abc3" => "abc".to_string(),
        "n_dot" => "a.b".to_string(),
        "n_upper" => "TANK.TGA".to_string(),
        "n_long" => {
            // 300 'x'es with ".tga" at indices 10..14, as the generator
            // builds it.
            let mut name = String::with_capacity(300);
            for i in 0..300 {
                name.push(if (10..14).contains(&i) {
                    ['.', 't', 'g', 'a'][i - 10]
                } else {
                    'x'
                });
            }
            name
        }
        other => format!("{other}.dds"),
    }
}

/// The corpus reduction factors (the constructor's second argument).
fn corpus_reduction(id: &str) -> u32 {
    match id {
        "red1_64" => 1,
        "red3_64" => 3,
        "red6_64" => 6,
        "red_over" => 5,
        "volume_red1" => 1,
        _ => 0,
    }
}
