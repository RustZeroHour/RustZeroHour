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
use rzh_formats::chunk::{ChunkLoad, ChunkSave};
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
    bench_tga_write,
    bench_chunk_write,
    bench_chunk_read
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

// ---------------------------------------------------------------------------
// chunk — the W3D container read/write workloads (plan §6 discipline,
// continued into Phase 2). The C++ baseline is the chunkio.cpp
// transcription in tools/differential/bench_wrap_chunk.cpp.
// ---------------------------------------------------------------------------

/// Vertex records of the mesh-shaped corpus (matches the C++ baseline).
const CHUNK_VERTICES: usize = 4096;

/// The corpus formula anchored at an absolute offset (the C++ baseline
/// generates the same absolute-indexed bytes).
fn chunk_formula_at(offset: usize, n: usize) -> Vec<u8> {
    (offset..offset + n)
        .map(|i| ((i * 7 + 13) & 0xFF) as u8)
        .collect()
}

/// Builds the mesh-shaped corpus tree through the writer: a top-level
/// container with a micro-chunk header child, a 4096-vertex data child
/// (one 12-byte write per vertex) and an index-array child. The payload
/// formulas form one continuous byte stream: micros 0..24, then the
/// vertex records, then the index array.
fn chunk_mesh_tree(save: &mut ChunkSave) {
    save.begin_chunk(100).unwrap();
    // header child: three micro chunks
    save.begin_chunk(1).unwrap();
    save.begin_micro_chunk(1).unwrap();
    save.write(&chunk_formula_at(0, 4)).unwrap();
    save.end_micro_chunk().unwrap();
    save.begin_micro_chunk(2).unwrap();
    save.write(&chunk_formula_at(4, 4)).unwrap();
    save.end_micro_chunk().unwrap();
    save.begin_micro_chunk(3).unwrap();
    save.write(&chunk_formula_at(8, 16)).unwrap();
    save.end_micro_chunk().unwrap();
    save.end_chunk().unwrap();
    // vertex child: one write per 12-byte vertex record
    save.begin_chunk(2).unwrap();
    for i in 0..CHUNK_VERTICES {
        save.write(&chunk_formula_at(24 + i * 12, 12)).unwrap();
    }
    save.end_chunk().unwrap();
    // index child: one big write
    save.begin_chunk(3).unwrap();
    save.write(&chunk_formula_at(
        24 + CHUNK_VERTICES * 12,
        CHUNK_VERTICES * 4 + 4,
    ))
    .unwrap();
    save.end_chunk().unwrap();
    save.end_chunk().unwrap();
}

/// Structures → bytes: the exporter shape (the tree build through the
/// writer, header patching included).
fn bench_chunk_write(c: &mut Criterion) {
    let mut save = ChunkSave::new();
    chunk_mesh_tree(&mut save);
    let len = save.into_bytes().unwrap().len() as u64;
    assert_eq!(
        len, 65_602,
        "the bench corpus drifted from the C++ baseline"
    );

    let mut group = c.benchmark_group("chunk/write");
    group.throughput(Throughput::Bytes(len));
    group.bench_function("mesh_x4096", |b| {
        b.iter(|| {
            let mut save = ChunkSave::new();
            chunk_mesh_tree(&mut save);
            black_box(save.into_bytes().unwrap().len())
        })
    });
    group.finish();
}

/// The consumer view: the walk-A lens (structure + payload reads in
/// 64-byte pieces) over the materialized corpus — the shape the future
/// W3D loaders drive.
fn bench_chunk_read(c: &mut Criterion) {
    let mut save = ChunkSave::new();
    chunk_mesh_tree(&mut save);
    let bytes = save.into_bytes().unwrap();
    let len = bytes.len() as u64;
    assert_eq!(
        len, 65_602,
        "the bench corpus drifted from the C++ baseline"
    );

    let mut group = c.benchmark_group("chunk/read");
    group.throughput(Throughput::Bytes(len));
    group.bench_function("walk_x4096", |b| {
        b.iter(|| {
            let mut load = rzh_formats::chunk::ChunkLoad::new(black_box(&bytes));
            let mut checksum: u64 = 0;
            walk_a_bench(&mut load, &mut checksum);
            checksum
        })
    });
    group.finish();
}

/// The walk-A loop shape (mirrors the golden walker and the C++
/// baseline's `bench_chunk_walk` one-to-one).
fn walk_a_bench(load: &mut ChunkLoad, checksum: &mut u64) {
    while load.open_chunk().unwrap() {
        if load.contains_chunks() {
            walk_a_bench(load, checksum);
        } else {
            let mut remaining = load.cur_chunk_length();
            let mut buf = [0u8; 64];
            while remaining > 0 {
                let piece = remaining.min(64) as usize;
                let n = load.read(&mut buf[..piece]).unwrap();
                if n == 0 {
                    break;
                }
                for b in &buf[..n as usize] {
                    *checksum = checksum.wrapping_add(u64::from(*b));
                }
                remaining -= n;
            }
        }
        load.close_chunk().unwrap();
    }
}
