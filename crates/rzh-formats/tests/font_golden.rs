// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Rust port Copyright 2026 the RustZeroHour contributors.
//
//! Differential parity tests for the font port (P0).
//!
//! `tests/golden/font.txt` is a script produced by
//! `tools/differential/gen_font_golden.cpp` — a verbatim transcription of
//! the Zero Hour font load path (`font3d.cpp`'s `Font3DDataClass` with
//! the SurfaceClass/Targa/bitmap-handler collaborators and the D3DX load
//! model) over 29 hand-built corpora: the 16-bit strike family (the
//! basic strip, the 1024-wide wrap, the 300-column crop with the uchar
//! width wrap, the 130-run 0x7F turn-off, the empty strip, the RLE
//! strip, the bottom-up and left-origin normalizations, the
//! line-overflow overlap), the grid family (the 128 and 256 pages with
//! the 0x80/0x81 boundary, the straddling blob, the empty grid, the
//! fractional 100x100 cells, the half-pixel 8x8, the height-0 8x4), the
//! depth family (32-bit with mid alphas, 32-bit RLE, the no-alpha
//! 24-bit, 8-bit mono, the 32- and 24-bit colormaps, the alpha-only
//! A8), the error family (the truncated file, the unsupported type, the
//! absent file — all the placeholder), the crash family (the depth-15
//! and zero-height null surfaces) and the asset-cache reuse scenario.
//!
//! The test replays every corpus through [`rzh_formats::font::parse_font`]
//! (and the instance/cache layers) and re-dumps the results in the
//! golden script's grammar, asserting line-by-line equality.

use std::path::PathBuf;

use rzh_formats::font::{Font3DCache, Font3DData, Font3DInstance, FontVerdict, parse_font};

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join("font.txt")
}

// --- the dump helpers (the generator's grammar) -----------------------------

fn fbits(f: f32) -> u32 {
    f.to_bits()
}

/// The wrap-31 sum the generator's checksums use.
fn sum31(bytes: &[u8]) -> u64 {
    let mut s: u64 = 0;
    for &b in bytes {
        s = s.wrapping_mul(31).wrapping_add(b as u64);
    }
    s
}

fn instance_tabsum(inst: &Font3DInstance) -> u64 {
    let mut s: u64 = 0;
    for a in 0..256 {
        s = s
            .wrapping_mul(31)
            .wrapping_add(u64::from(fbits(inst.scaled_width[a])));
        s = s
            .wrapping_mul(31)
            .wrapping_add(u64::from(fbits(inst.scaled_spacing[a])));
    }
    s
}

/// The probe chars the generator's iprobe lines dump.
const PROBE_CHARS: [u16; 7] = [0x20, 0x48, 0x57, 0x7f, 0x80, 0x81, 0xff];

fn hex(bytes: &[u8]) -> String {
    let mut out = String::new();
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// One golden corpus: the id, the asset name, the file bytes and the
/// expected dump lines.
type Corpus = (String, Vec<u8>, Option<Vec<u8>>, Vec<String>);

/// Re-dumps one loaded font in the generator's grammar (the lines
/// after `file`).
fn dump_font(id: &str, font: &Font3DData, out: &mut Vec<String>) {
    out.push(format!("verdict {id} {}", font.verdict.word()));

    if matches!(font.verdict, FontVerdict::Crash(_)) {
        // the crash sites precede the name and the tables
        return;
    }

    out.push(format!("name {id} {}", String::from_utf8_lossy(&font.name)));

    let source = font
        .source
        .as_ref()
        .expect("the non-crash loads keep the surface");
    out.push(format!(
        "sfmt {id} {} {} {}",
        source.width,
        source.height,
        source.format.word()
    ));
    out.push(format!("ssum {id} {}", sum31(&source.bytes)));

    let packed = font
        .packed
        .as_ref()
        .expect("the non-crash loads keep the page");
    out.push(format!(
        "geom {id} {} {:08x} {} {}",
        font.char_height as i32,
        fbits(font.v_height),
        packed.width,
        packed.height
    ));

    for row in 0..16 {
        let mut line = format!("tab {id} {row}");
        for col in 0..16 {
            let c = row * 16 + col;
            line.push_str(&format!(
                " {:08x} {:08x} {:08x} {:02x}",
                fbits(font.u_offset[c]),
                fbits(font.v_offset[c]),
                fbits(font.u_width[c]),
                font.char_width[c]
            ));
        }
        out.push(line);
    }

    out.push(format!(
        "ptex {id} {} {} {}",
        packed.width,
        packed.height,
        sum31(&packed.bytes)
    ));

    // the packed page's non-zero rows (up to six, then the marker)
    {
        let row_bytes = packed.width as usize * 2;
        let mut shown = 0;
        let mut more = false;
        for y in 0..packed.height as usize {
            let row = &packed.bytes[y * packed.pitch()..][..row_bytes];
            if row.iter().all(|&b| b == 0) {
                continue;
            }
            if shown == 6 {
                more = true;
                break;
            }
            out.push(format!("ptexr {id} {y} {}", hex(row)));
            shown += 1;
        }
        if more {
            out.push(format!("ptexr {id} more -"));
        }
    }

    // the instance layer
    {
        let mut inst = Font3DInstance::new(font.clone());
        out.push(format!(
            "inst {id} {:08x} {:08x} {}",
            fbits(inst.space_spacing),
            fbits(inst.scaled_height),
            instance_tabsum(&inst)
        ));

        for ch in PROBE_CHARS {
            out.push(format!(
                "iprobe {id} {:02x} {:08x} {:08x}",
                ch,
                fbits(inst.char_width(ch)),
                fbits(inst.char_spacing(ch))
            ));
        }

        out.push(format!(
            "iwidth {id} 4869205721 {:08x}",
            fbits(inst.string_width(b"Hi W!"))
        ));
        out.push(format!(
            "iwidth {id} 2020 {:08x}",
            fbits(inst.string_width(b"  "))
        ));

        inst.set_mono_spaced();
        out.push(format!(
            "imono {id} {:08x} {:08x} {}",
            fbits(inst.mono_spacing),
            fbits(inst.scaled_height),
            instance_tabsum(&inst)
        ));
    }

    // the asset-cache reuse pin (the case-flipped name must hit)
    {
        let mut cache = Font3DCache::new();
        let _first = cache.get(&font.name, None);
        let base = cache.parse_count();
        let flipped: Vec<u8> = font
            .name
            .iter()
            .map(|&c| {
                if c.is_ascii_uppercase() {
                    c.to_ascii_lowercase()
                } else {
                    c.to_ascii_uppercase()
                }
            })
            .collect();
        let second = cache.get(&flipped, None);
        let loads = cache.parse_count();
        let hit = loads == base && second.name == font.name;
        out.push(format!(
            "reuse {id} {} {}",
            if hit { "t" } else { "f" },
            loads
        ));
    }
}

// --- the replay ----------------------------------------------------------------

fn unhex(s: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let bytes = s.as_bytes();
    for pair in bytes.chunks(2) {
        let hi = (pair[0] as char).to_digit(16).expect("hex digit");
        let lo = (pair[1] as char).to_digit(16).expect("hex digit");
        out.push((hi * 16 + lo) as u8);
    }
    out
}

#[test]
fn font_golden_replay() {
    let raw = std::fs::read_to_string(golden_path()).expect("the golden script exists");
    let lines = raw.lines().filter(|l| !l.starts_with('#'));

    let mut corpora: Vec<Corpus> = Vec::new();
    // collect (name, file bytes, expected-dump-lines)
    let mut current: Option<Corpus> = None;
    for line in lines {
        let mut parts = line.splitn(3, ' ');
        let cmd = parts.next().unwrap_or("");
        let id = parts.next().unwrap_or("").to_string();
        match cmd {
            "font" => {
                if let Some(cur) = current.take() {
                    corpora.push(cur);
                }
                let name = parts.next().unwrap_or("").to_string();
                current = Some((id, name.into_bytes(), None, Vec::new()));
            }
            "file" => {
                let hexbytes = parts.next().unwrap_or("").trim();
                if let Some((_, _, file, _)) = &mut current {
                    *file = if hexbytes == "-" {
                        None
                    } else {
                        Some(unhex(hexbytes))
                    };
                }
            }
            _ => {
                if let Some((_, _, _, expected)) = &mut current {
                    expected.push(line.to_string());
                }
            }
        }
    }
    if let Some(cur) = current.take() {
        corpora.push(cur);
    }

    assert!(
        corpora.len() >= 28,
        "expected the full corpus set, found {}",
        corpora.len()
    );

    let mut failures = Vec::new();
    for (id, name, file, expected) in &corpora {
        let font = parse_font(name, file.as_deref());
        let mut got = Vec::new();
        dump_font(id, &font, &mut got);
        let got_lines: Vec<&str> = got.iter().map(|s| s.as_str()).collect();
        let exp_lines: Vec<&str> = expected.iter().map(|s| s.as_str()).collect();
        if got_lines != exp_lines {
            let mut diff_at = 0;
            for (i, (g, e)) in got_lines.iter().zip(exp_lines.iter()).enumerate() {
                if g != e {
                    diff_at = i;
                    break;
                }
            }
            if got_lines.len() != exp_lines.len() && diff_at >= got_lines.len().min(exp_lines.len())
            {
                diff_at = got_lines.len().min(exp_lines.len());
            }
            failures.push(format!(
                "corpus {id}: line {} differs\n  golden: {}\n  port:   {}",
                diff_at + 1,
                exp_lines.get(diff_at).unwrap_or(&"<missing>"),
                got_lines.get(diff_at).unwrap_or(&"<missing>")
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} corpora diverged:\n{}",
        failures.len(),
        corpora.len(),
        failures.join("\n")
    );
}

#[test]
fn font_verdict_words() {
    assert_eq!(FontVerdict::Ok.word(), "ok");
    assert_eq!(FontVerdict::Missing.word(), "missing");
    assert_eq!(FontVerdict::TgaError(-2).word(), "tgaerr--2");
    assert_eq!(
        FontVerdict::Crash("null-surface").word(),
        "crash-null-surface"
    );
    assert_eq!(
        FontVerdict::Crash("findbb-lock").word(),
        "crash-findbb-lock"
    );
}

#[test]
fn font_bottomup_matches_topleft() {
    // the Y-origin toggle normalizes both origins to the same top-down
    // surface: a bottom-up file and its top-down twin build identical
    // fonts (the golden pins it across the whole table dump)
    let pixels = |flip: bool| -> Vec<u8> {
        // a 96x8 strike with three 8-column glyphs at columns 0, 20, 40
        let mut px = vec![0u16; 96 * 8];
        for (gi, base) in [0usize, 20, 40].into_iter().enumerate() {
            for y in 0..8usize {
                for x in 0..8usize {
                    px[y * 96 + base + x] = (0x8000u16)
                        | ((((gi + x) & 31) as u16) << 10)
                        | ((((gi + y) & 31) as u16) << 5);
                }
            }
        }
        let mut v = vec![
            0,
            0, // id length, cmap type
            2, // truecolor
            0,
            0,
            0,
            0,
            0, // cmap fields
            0,
            0,
            0,
            0, // offsets
            96,
            0,
            8,
            0,                              // 96x8
            16,                             // depth
            if flip { 0x00 } else { 0x20 }, // descriptor
        ];
        let mut rows = px;
        if flip {
            // a bottom-up file stores its rows bottom-first
            let mut rev = rows.clone();
            for y in 0..8usize {
                for x in 0..96usize {
                    rev[(7 - y) * 96 + x] = rows[y * 96 + x];
                }
            }
            rows = rev;
        }
        for p in rows {
            v.extend_from_slice(&p.to_le_bytes());
        }
        v
    };

    let top = parse_font(b"Top.tga", Some(&pixels(false)));
    let bottom = parse_font(b"Bottom.tga", Some(&pixels(true)));
    assert_eq!(top.verdict, FontVerdict::Ok);
    assert_eq!(bottom.verdict, FontVerdict::Ok);
    assert_eq!(top.u_offset, bottom.u_offset);
    assert_eq!(top.v_offset, bottom.v_offset);
    assert_eq!(top.u_width, bottom.u_width);
    assert_eq!(top.char_width, bottom.char_width);
    assert_eq!(
        top.packed.as_ref().unwrap().bytes,
        bottom.packed.as_ref().unwrap().bytes
    );
}

#[test]
fn font_cache_reuse() {
    // the stricmp cache: a case-flipped name hits, a new name parses
    let mut cache = Font3DCache::new();
    let a = cache.get(b"Reuse.tga", None);
    assert_eq!(cache.parse_count(), 1);
    assert_eq!(a.verdict, FontVerdict::Missing); // no file: the placeholder font

    let b = cache.get(b"REUSE.TGA", None);
    assert_eq!(cache.parse_count(), 1, "the case-flipped name must hit");
    assert_eq!(a.name, b.name);

    let c = cache.get(b"Other.tga", None);
    assert_eq!(cache.parse_count(), 2);
    assert_eq!(c.name, b"OTHER.TGA");
    assert_eq!(cache.len(), 2);
}

#[test]
fn font_24bit_kills_every_glyph() {
    // a 24-bit source lands as X8R8G8B8 (no alpha bits): every column
    // reads transparent and every width dies, but the load succeeds
    // (16x16: one-pixel cells, the clean empty-grid shape)
    let mut v = vec![0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 16, 0, 16, 0, 24, 0x20];
    for y in 0..16usize {
        for x in 0..16usize {
            v.push((x * 16) as u8);
            v.push((y * 16) as u8);
            v.push(0x7f);
        }
    }
    let font = parse_font(b"Opaque.tga", Some(&v));
    assert_eq!(font.verdict, FontVerdict::Ok);
    assert!(font.char_width.iter().all(|&w| w == 0));
    assert!(font.u_width.iter().all(|&w| w == 0.0));
    // and the packed page stays empty (cleared)
    assert!(font.packed.as_ref().unwrap().bytes.iter().all(|&b| b == 0));
}

#[test]
fn font_uchar_width_wrap() {
    // a 300-column glyph records width 44 through the uchar table while
    // the packed layout uses the true extent (the crop family)
    let mut px = vec![0u16; 300 * 16];
    for y in 0..16usize {
        for x in 0..300usize {
            px[y * 300 + x] = (0x8000u16) | (((x & 31) as u16) << 10) | (((y & 31) as u16) << 5);
        }
    }
    let mut v = vec![0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 44, 1, 16, 0, 16, 0x20];
    for p in px {
        v.extend_from_slice(&p.to_le_bytes());
    }
    let font = parse_font(b"Huge.tga", Some(&v));
    assert_eq!(font.verdict, FontVerdict::Ok);
    assert_eq!(font.char_width[0], 44, "300 & 0xff");
    // the U width keeps the full float, rescaled by the minimize
    // (source/page = 300/256)
    assert_eq!(font.u_width[0], 300.0f32 / 256.0);
    // the page is 256 (the 300-wide source) and the glyph is cropped
    let packed = font.packed.as_ref().unwrap();
    assert_eq!(packed.width, 256);
    assert_eq!(packed.height, 256);
}
