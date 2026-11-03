// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
//! Criterion benchmarks for `rzh-formats` (plan §6 discipline, continued
//! into Phase 2).
//!
//! The CSF string table is read once per language at startup and written
//! by the localization pipeline, so the characteristic workloads are:
//!
//! - `parse`: bytes → structures (the startup load),
//! - `write`: structures → bytes (the localization/patch pipeline),
//! - `game_text`: the consumer view (`parseCSF`'s decode + `stripSpaces`
//!   over every label) — what the future game-text port pays per load.
//!
//! The corpus is a formula-generated stand-in for a localized UI table
//! (4096 labels; every 8th label carries a speech wave, every 16th a
//! second string) built with the same formulas on both sides — the C++
//! baseline in `tools/differential/bench_baseline.cpp` (`csf/…` section)
//! reconstructs the identical file through its transcribed writer and
//! parses **identical bytes**. Compare with:
//!
//! ```text
//! cargo bench -p rzh-formats
//! tools/differential/bench_baseline.sh   # csf section
//! ```

// Bench targets have no public API; the `criterion_group!` macro generates
// `pub fn`s that the workspace `missing_docs` lint would otherwise flag.
#![allow(missing_docs)]

use std::hint::black_box;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use rzh_formats::csf::{CsfFile, CsfLabel, CsfString};
use rzh_formats::tga::{SaveFlags, TGA_TRUECOLOR, TgaFile, TgaHeader};

/// Label count of the corpus (matches the C++ baseline).
const LABELS: u32 = 4096;

/// The bench corpus: a formula-generated UI string table. The C++ baseline
/// builds the byte-identical file through its transcribed Babylon writer —
/// keep the two formulas in lockstep.
fn corpus() -> CsfFile {
    let mut labels = Vec::with_capacity(LABELS as usize);
    for i in 0..LABELS {
        let mut strings = Vec::with_capacity(2);
        let text: Vec<u16> = format!("Mission {i}: Destroy the enemy {i} structure before dawn.")
            .encode_utf16()
            .collect();
        if i % 8 == 0 {
            // The C++ baseline builds this corpus through the transcribed
            // Babylon writer, whose LANGINFO table gives US = 'e' as the
            // trailing wave byte — the raw chunk is constructed directly so
            // both sides parse byte-identical files.
            let mut wave = format!("SPEECH:Ev_{i:05}.wav").into_bytes();
            wave.push(b'e');
            strings.push(CsfString {
                text,
                wave: Some(wave),
            });
        } else {
            strings.push(CsfString::new(text));
        }
        if i % 16 == 0 {
            strings.push(CsfString::new(Vec::new()));
        }
        labels.push(CsfLabel {
            name: format!("GUI:Label_{i:05}").into_bytes(),
            strings,
        });
    }
    CsfFile {
        version: 3,
        skip: 0,
        language: 0,
        labels,
    }
}

/// Bytes → structures: the startup load of a localized table.
fn bench_parse(c: &mut Criterion) {
    let bytes = corpus().to_bytes().unwrap();
    let mut group = c.benchmark_group("csf/parse");
    group.throughput(Throughput::Bytes(bytes.len() as u64));
    group.bench_function("ui_x4096", |b| {
        b.iter(|| CsfFile::parse(black_box(&bytes)).unwrap())
    });
    group.finish();
}

/// Structures → bytes: the localization write path.
fn bench_write(c: &mut Criterion) {
    let file = corpus();
    let mut group = c.benchmark_group("csf/write");
    group.throughput(Throughput::Bytes(file.to_bytes().unwrap().len() as u64));
    group.bench_function("ui_x4096", |b| {
        b.iter(|| black_box(&file).to_bytes().unwrap())
    });
    group.finish();
}

/// The consumer view: decode + strip over every label into a reused
/// buffer (the original's member-buffer shape — see
/// `CsfString::game_text_into`).
fn bench_game_text(c: &mut Criterion) {
    let file = corpus();
    let mut group = c.benchmark_group("csf/game_text");
    group.throughput(Throughput::Bytes(file.to_bytes().unwrap().len() as u64));
    group.bench_function("ui_x4096", |b| {
        b.iter(|| {
            let mut buf = Vec::new();
            let mut total = 0usize;
            for label in black_box(&file).labels.iter() {
                label.strings[0].game_text_into(&mut buf);
                total += buf.len();
            }
            total
        })
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_parse,
    bench_write,
    bench_game_text,
    bench_tga_parse,
    bench_tga_write
);
criterion_main!(benches);

// ---------------------------------------------------------------------------
// TGA — the texture-load and screenshot-save workloads (plan §6 discipline,
// continued into Phase 2). The C++ baseline is the TARGA.cpp transcription
// in tools/differential/bench_wrap_tga.cpp.
// ---------------------------------------------------------------------------

/// Edge length of the corpus texture (matches the C++ baseline).
const TGA_SIZE: u16 = 256;
/// Pixel count of the corpus texture.
const TGA_PIXELS: usize = TGA_SIZE as usize * TGA_SIZE as usize;

/// The corpus image: `pixels` pixels of `depth` bytes; every group of eight
/// starts with a three-pixel solid run (so the RLE encoder's run branches
/// fire) followed by five formula pixels. Keep the formula in lockstep with
/// `bench_wrap_tga.cpp`.
fn tga_corpus_image(pixels: usize, depth: usize) -> Vec<u8> {
    let mut image = Vec::with_capacity(pixels * depth);
    for p in 0..pixels {
        for c in 0..depth {
            let v: u32 = if p % 8 < 3 {
                37 + 11 * c as u32
            } else {
                (p as u32 * 7 + 13 + c as u32 * 17) & 0xFF
            };
            image.push(v as u8);
        }
    }
    image
}

/// A corpus Targa in the state the screenshot path builds: a truecolor
/// header around the formula image, no palette, no footer.
fn tga_corpus(depth: usize) -> TgaFile {
    TgaFile {
        header: TgaHeader {
            image_type: TGA_TRUECOLOR,
            width: TGA_SIZE,
            height: TGA_SIZE,
            pixel_depth: match depth {
                3 => 24,
                _ => 32,
            },
            ..TgaHeader::default()
        },
        image: tga_corpus_image(TGA_PIXELS, depth),
        ..TgaFile::default()
    }
}

/// Bytes → structures: the texture-load path (`Targa::Open` + `Load`). The
/// raw file is the screenshot shape; the RLE file is built through the
/// port's own encoder — the C++ baseline builds it through its transcribed
/// encoder, and the goldens pin the two byte-identical.
fn bench_tga_parse(c: &mut Criterion) {
    let raw24 = tga_corpus(3).to_bytes(SaveFlags::IMAGE, false).unwrap();
    let rle32 = tga_corpus(4)
        .to_bytes(SaveFlags::IMAGE_COMPRESS, false)
        .unwrap();

    let mut group = c.benchmark_group("tga/parse");
    group.throughput(Throughput::Bytes(raw24.len() as u64));
    group.bench_function("rgb24_raw_x256", |b| {
        b.iter(|| TgaFile::parse(black_box(&raw24)).unwrap())
    });
    group.throughput(Throughput::Bytes(rle32.len() as u64));
    group.bench_function("rgb32_rle_x256", |b| {
        b.iter(|| TgaFile::parse(black_box(&rle32)).unwrap())
    });
    group.finish();
}

/// Structures → bytes: the uncompressed save (the screenshot path — the
/// truecolor inversion in and out of the copy) and the compressed save
/// (the ImagePacker path, RLE-encoding the inverted buffer).
fn bench_tga_write(c: &mut Criterion) {
    let raw24 = tga_corpus(3);
    let tex32 = tga_corpus(4);
    let rle32_len = tex32
        .to_bytes(SaveFlags::IMAGE_COMPRESS, false)
        .unwrap()
        .len();

    let mut group = c.benchmark_group("tga/write");
    group.throughput(Throughput::Bytes(
        raw24.to_bytes(SaveFlags::IMAGE, false).unwrap().len() as u64,
    ));
    group.bench_function("rgb24_x256", |b| {
        b.iter(|| black_box(&raw24).to_bytes(SaveFlags::IMAGE, false).unwrap())
    });
    group.finish();

    let mut group = c.benchmark_group("tga/encode");
    group.throughput(Throughput::Bytes(rle32_len as u64));
    group.bench_function("rgb32_x256", |b| {
        b.iter(|| {
            black_box(&tex32)
                .to_bytes(SaveFlags::IMAGE_COMPRESS, false)
                .unwrap()
        })
    });
    group.finish();
}
