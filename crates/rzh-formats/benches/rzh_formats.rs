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
use rzh_formats::dds::{DdsFile, HsvShift, Ww3dFormat};
use rzh_formats::font::parse_font;
use rzh_formats::tga::{SaveFlags, TGA_TRUECOLOR, TgaFile, TgaHeader};
use rzh_formats::w3d_hanim::W3dRawAnim;
use rzh_formats::w3d_hcanim::{W3dCompressedAnim, W3dCompressedChannel};
use rzh_formats::w3d_hlod::{
    W3D_CHUNK_HLOD_AGGREGATE_ARRAY, W3D_CHUNK_HLOD_HEADER, W3D_CHUNK_HLOD_LOD_ARRAY,
    W3D_CHUNK_HLOD_PROXY_ARRAY, W3D_CHUNK_HLOD_SUB_OBJECT, W3D_CHUNK_HLOD_SUB_OBJECT_ARRAY_HEADER,
    W3D_CURRENT_HLOD_VERSION, W3dHlod,
};
use rzh_formats::w3d_htree::W3dHtree;
use rzh_formats::w3d_mesh::W3dMesh;
use rzh_formats::wnd::{WndDisplay, parse_wnd};

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

// ---------------------------------------------------------------------------
// w3d_hlod — the assembly-load workload (plan §6 discipline, Phase 2's
// W3D track). The C++ baseline is the hlod.cpp transcription in
// tools/differential/bench_wrap_hlod.cpp, driving the identical corpus
// bytes through its own Load_W3D.
// ---------------------------------------------------------------------------

/// The corpus shape (matches the C++ baseline): 64 LODs of 16 models,
/// 16 aggregates and 32 proxies — 1072 sub-objects in all.
const HLOD_BENCH_LODS: usize = 64;
const HLOD_BENCH_MODELS: usize = 16;
const HLOD_BENCH_AGGREGATES: usize = 16;
const HLOD_BENCH_PROXIES: usize = 32;

/// The corpus formula anchored at an absolute offset (the C++ baseline
/// generates the same absolute-indexed bytes).
fn hlod_formula_at(offset: usize, n: usize) -> Vec<u8> {
    (offset..offset + n)
        .map(|i| ((i * 11 + 7) & 0xFF) as u8)
        .collect()
}

/// Builds the assembly-shaped corpus — keep in lockstep with the C++
/// baseline's `build_hlod_corpus`.
fn hlod_corpus() -> Vec<u8> {
    let mut save = ChunkSave::new();

    // the header chunk (40 bytes): version 1.0, 64 LODs
    save.begin_chunk(W3D_CHUNK_HLOD_HEADER).unwrap();
    let mut header = [0u8; 40];
    header[..4].copy_from_slice(&W3D_CURRENT_HLOD_VERSION.to_le_bytes());
    header[4..8].copy_from_slice(&(HLOD_BENCH_LODS as u32).to_le_bytes());
    header[8..13].copy_from_slice(b"bench");
    header[24..29].copy_from_slice(b"bench");
    save.write(&header).unwrap();
    save.end_chunk().unwrap();

    // the LOD arrays: 16 models each, the screen size varying per LOD
    for l in 0..HLOD_BENCH_LODS {
        save.begin_chunk(W3D_CHUNK_HLOD_LOD_ARRAY).unwrap();
        save.begin_chunk(W3D_CHUNK_HLOD_SUB_OBJECT_ARRAY_HEADER)
            .unwrap();
        save.write(&(HLOD_BENCH_MODELS as u32).to_le_bytes())
            .unwrap();
        save.write(&(l as f32).to_le_bytes()).unwrap();
        save.end_chunk().unwrap();
        for m in 0..HLOD_BENCH_MODELS {
            // the name field's absolute position: the bytes so far, the
            // chunk header (8) and the bone word (4)
            let at = save.as_bytes().len() + 8 + 4;
            let name = hlod_formula_at(at, 32);
            save.begin_chunk(W3D_CHUNK_HLOD_SUB_OBJECT).unwrap();
            save.write(&((l * HLOD_BENCH_MODELS + m) as u32).to_le_bytes())
                .unwrap();
            save.write(&name).unwrap();
            save.end_chunk().unwrap();
        }
        save.end_chunk().unwrap();
    }

    // the aggregates
    save.begin_chunk(W3D_CHUNK_HLOD_AGGREGATE_ARRAY).unwrap();
    save.begin_chunk(W3D_CHUNK_HLOD_SUB_OBJECT_ARRAY_HEADER)
        .unwrap();
    save.write(&(HLOD_BENCH_AGGREGATES as u32).to_le_bytes())
        .unwrap();
    save.write(&0.0f32.to_le_bytes()).unwrap();
    save.end_chunk().unwrap();
    for i in 0..HLOD_BENCH_AGGREGATES {
        let at = save.as_bytes().len() + 8 + 4;
        let name = hlod_formula_at(at, 32);
        save.begin_chunk(W3D_CHUNK_HLOD_SUB_OBJECT).unwrap();
        save.write(&((0x1000 + i) as u32).to_le_bytes()).unwrap();
        save.write(&name).unwrap();
        save.end_chunk().unwrap();
    }
    save.end_chunk().unwrap();

    // the proxies
    save.begin_chunk(W3D_CHUNK_HLOD_PROXY_ARRAY).unwrap();
    save.begin_chunk(W3D_CHUNK_HLOD_SUB_OBJECT_ARRAY_HEADER)
        .unwrap();
    save.write(&(HLOD_BENCH_PROXIES as u32).to_le_bytes())
        .unwrap();
    save.write(&0.0f32.to_le_bytes()).unwrap();
    save.end_chunk().unwrap();
    for i in 0..HLOD_BENCH_PROXIES {
        let at = save.as_bytes().len() + 8 + 4;
        let name = hlod_formula_at(at, 32);
        save.begin_chunk(W3D_CHUNK_HLOD_SUB_OBJECT).unwrap();
        save.write(&((0x2000 + i) as u32).to_le_bytes()).unwrap();
        save.write(&name).unwrap();
        save.end_chunk().unwrap();
    }
    save.end_chunk().unwrap();

    save.into_bytes().unwrap()
}

/// Bytes → the loaded assembly: the model-resolution startup workload.
fn bench_hlod_parse(c: &mut Criterion) {
    let bytes = hlod_corpus();
    let len = bytes.len() as u64;
    assert_eq!(
        len, 48_800,
        "the bench corpus drifted from the C++ baseline"
    );

    let mut group = c.benchmark_group("hlod");
    group.throughput(Throughput::Bytes(len));
    group.bench_function("parse_x1072", |b| {
        b.iter(|| {
            let hlod = W3dHlod::parse(black_box(&bytes)).expect("the corpus parses");
            let last = &hlod.lods[HLOD_BENCH_LODS - 1];
            black_box((
                hlod.lod_count,
                last.model_count,
                last.bone_indices[HLOD_BENCH_MODELS - 1],
                last.model_names[HLOD_BENCH_MODELS - 1].len(),
                hlod.aggregates.model_count,
                hlod.proxy_count(),
                hlod.proxies
                    .as_ref()
                    .map(|p| p[HLOD_BENCH_PROXIES - 1].bone_index),
                hlod.proxies
                    .as_ref()
                    .map(|p| p[HLOD_BENCH_PROXIES - 1].name_str().len()),
            ))
        })
    });
    group.finish();
}

// ---------------------------------------------------------------------------
// dds — the texture-load and decompression workloads (plan §6 discipline,
// Phase 2's texture track). The C++ baseline is the ddsfile.cpp
// transcription in tools/differential/bench_wrap_dds.cpp, driving the
// identical formula-generated corpora (the size asserts document the
// lockstep).

/// The parse corpus: a 512x512 DXT1 texture with its full ten-level
/// mip chain — 16,384 level-0 blocks, 174,904 bytes.
fn dds_parse_corpus() -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"DDS ");
    let mut put = |x: u32| bytes.extend_from_slice(&x.to_le_bytes());
    put(124); // Size
    put(0x1007); // Flags
    put(512); // Height
    put(512); // Width
    put(0); // Pitch
    put(0); // Depth
    put(10); // MipMapCount
    put(0); // AlphaBitDepth
    put(0); // Reserved
    put(0); // Surface
    for _ in 0..8 {
        put(0); // the four color keys
    }
    put(32); // PixelFormat.Size
    put(4); // PixelFormat.Flags
    put(0x3154_5844); // 'DXT1'
    for _ in 0..5 {
        put(0); // the pixel-format union tail
    }
    put(0x1000); // Caps
    put(0); // Caps2
    put(0); // Caps3
    put(0); // Caps4
    put(0); // TextureStage
    debug_assert_eq!(bytes.len(), 128);

    // the mip chain: the constructor's own size walk
    let mut size: u32 = (512 / 4) * (512 / 4) * 8;
    let mut at = 0usize;
    for _ in 0..10 {
        for i in 0..size as usize {
            bytes.push(((128 + at + i) * 11 + 7) as u8);
        }
        at += size as usize;
        if size > 16 {
            size /= 4;
        }
    }
    bytes
}

/// The conversion corpus: a 64x64 DXT5 texture, one level — 256 blocks.
fn dds_convert_corpus() -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"DDS ");
    let mut put = |x: u32| bytes.extend_from_slice(&x.to_le_bytes());
    put(124); // Size
    put(0x1007); // Flags
    put(64); // Height
    put(64); // Width
    put(0); // Pitch
    put(0); // Depth
    put(1); // MipMapCount
    put(0); // AlphaBitDepth
    put(0); // Reserved
    put(0); // Surface
    for _ in 0..8 {
        put(0); // the four color keys
    }
    put(32); // PixelFormat.Size
    put(4); // PixelFormat.Flags
    put(0x3554_5844); // 'DXT5'
    for _ in 0..5 {
        put(0); // the pixel-format union tail
    }
    put(0x1000); // Caps
    put(0); // Caps2
    put(0); // Caps3
    put(0); // Caps4
    put(0); // TextureStage
    debug_assert_eq!(bytes.len(), 128);
    for i in 0..(64 / 4) * (64 / 4) * 16 {
        bytes.push(((128 + i) * 11 + 7) as u8);
    }
    bytes
}

/// The constructor + Load: the texture loader's per-texture entry cost.
fn bench_dds_parse(c: &mut Criterion) {
    let bytes = dds_parse_corpus();
    let len = bytes.len() as u64;
    assert_eq!(
        len, 174_904,
        "the bench corpus drifted from the C++ baseline"
    );

    let mut group = c.benchmark_group("dds");
    group.throughput(Throughput::Bytes(len));
    group.bench_function("parse_x16k", |b| {
        b.iter(|| {
            let mut file =
                DdsFile::open(black_box("bench.dds"), 0, black_box(&bytes)).expect("parse");
            file.load(&bytes).expect("load");
            black_box((file.mip_level_count(), file.level_size(0).ok()));
        })
    });
    group.finish();
}

/// The DXT5 → A8R8G8B8 conversion walk the renderer's surface upload
/// drives: 256 blocks through the alpha tables, the colour decode and
/// the per-destination writes.
fn bench_dds_convert(c: &mut Criterion) {
    let bytes = dds_convert_corpus();
    let len = bytes.len() as u64;
    assert_eq!(len, 4_224, "the bench corpus drifted from the C++ baseline");

    let mut file = DdsFile::open("bench.dds", 0, &bytes).expect("parse");
    file.load(&bytes).expect("load");
    let mut dest = vec![0u8; 64 * 64 * 4 + 4];

    let mut group = c.benchmark_group("dds");
    group.throughput(Throughput::Bytes(len));
    group.bench_function("convert_dxt5_x256", |b| {
        b.iter(|| {
            file.copy_level_to_surface(
                0,
                Ww3dFormat::A8R8G8B8,
                64,
                64,
                black_box(&mut dest),
                64 * 4,
                HsvShift::none(),
            )
            .expect("convert");
            black_box(dest[16384 - 1]);
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
    bench_chunk_read,
    bench_mesh_parse,
    bench_htree_parse,
    bench_hanim_parse,
    bench_hanim_xforms,
    bench_hcanim_parse,
    bench_hcanim_parse_ad,
    bench_hcanim_xforms_ad,
    bench_hlod_parse,
    bench_dds_parse,
    bench_dds_convert,
    mapchunk_benches::bench_write,
    mapchunk_benches::bench_parse,
    mapheightmap_benches::bench_parse,
    mapheightmap_benches::bench_parse_v1,
    mapblendtile_benches::bench_parse,
    mapblendtile_benches::bench_parse_v6,
    maplighting_benches::bench_parse,
    mapobjects_benches::bench_parse,
    mapworldinfo_benches::bench_parse,
    mappolygons_benches::bench_parse,
    mapsides_benches::bench_parse,
    mapscripts_benches::bench_parse,
    wnd_benches::bench_parse,
    font_benches::bench_parse
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

// ---------------------------------------------------------------------------
// w3d_mesh — the model-load workload (plan §6 discipline, Phase 2's W3D
// track). The C++ baseline is the meshmdlio.cpp transcription in
// tools/differential/bench_wrap_mesh.cpp, driving the identical corpus
// bytes through its own Load_W3D.
// ---------------------------------------------------------------------------

/// Vertex/triangle count of the corpus (matches the C++ baseline).
const MESH_VERTICES: u32 = 2048;

/// The corpus formula anchored at an absolute offset (the C++ baseline
/// generates the same absolute-indexed bytes).
fn mesh_formula_at(offset: usize, n: usize) -> Vec<u8> {
    (offset..offset + n)
        .map(|i| ((i * 7 + 13) & 0xFF) as u8)
        .collect()
}

/// A tiny chunk-framing builder for the corpus bytes (the same framing
/// the C++ baseline's builder produces).
struct MeshChunkBuild {
    bytes: Vec<u8>,
    marks: Vec<usize>,
    kids: Vec<i32>,
}

impl MeshChunkBuild {
    fn new() -> Self {
        Self {
            bytes: Vec::new(),
            marks: Vec::new(),
            kids: Vec::new(),
        }
    }

    fn begin(&mut self, id: u32) {
        if !self.marks.is_empty() {
            *self.kids.last_mut().unwrap() += 1;
        }
        self.marks.push(self.bytes.len());
        self.kids.push(0);
        self.bytes.extend_from_slice(&id.to_le_bytes());
        self.bytes.extend_from_slice(&0u32.to_le_bytes());
    }

    fn end(&mut self) {
        let off = self.marks.pop().unwrap();
        let kids = self.kids.pop().unwrap();
        let mut size = (self.bytes.len() - off - 8) as u32;
        if kids > 0 {
            size |= 0x8000_0000;
        }
        self.bytes[off + 4..off + 8].copy_from_slice(&size.to_le_bytes());
    }

    fn write(&mut self, data: &[u8]) {
        self.bytes.extend_from_slice(data);
    }
}

/// Builds the mesh-shaped corpus: a prelit-vertex mesh with the full
/// geometry, material and AABTree payload. Keep in lockstep with the
/// C++ baseline's `build_mesh_corpus`.
fn mesh_corpus() -> Vec<u8> {
    let mut b = MeshChunkBuild::new();
    let n = MESH_VERTICES as usize;
    let mut absolute = 0usize;

    // the mesh header (116 bytes)
    b.begin(0x1F);
    let mut header = Vec::with_capacity(116);
    for v in [
        0x0004_0002u32, // version 4.2
        0x0200_0000,    // PRELIT_VERTEX
    ] {
        header.extend_from_slice(&v.to_le_bytes());
    }
    header.extend_from_slice(b"bench\0\0\0\0\0\0\0\0\0\0\0"); // mesh name (16 bytes)
    header.extend_from_slice(b"corpus\0\0\0\0\0\0\0\0\0\0"); // container name (16 bytes)
    for v in [
        MESH_VERTICES, // tris
        MESH_VERTICES, // verts
        1,             // materials
        0,             // damage stages
        0,             // sort level
        0,             // prelit version
        0,             // future counts
        0x1F,          // vertex channels
        0x01,          // face channels
    ] {
        header.extend_from_slice(&v.to_le_bytes());
    }
    for word in [-1.0f32, -1.0, -1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 2.0] {
        header.extend_from_slice(&word.to_bits().to_le_bytes());
    }
    b.write(&header);
    b.end();
    absolute += 116;

    // vertices + normals
    for id in [0x02u32, 0x03] {
        b.begin(id);
        let data = mesh_formula_at(absolute, n * 12);
        b.write(&data);
        b.end();
        absolute += n * 12;
    }

    // triangles
    b.begin(0x20);
    let data = mesh_formula_at(absolute, n * 32);
    b.write(&data);
    b.end();
    absolute += n * 32;

    // the prelit wrapper with the material tree
    b.begin(0x24);
    {
        b.begin(0x28); // material info
        b.write(&[1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0]);
        b.end();
        b.begin(0x29); // shaders
        let data = mesh_formula_at(absolute, 16);
        b.write(&data);
        b.end();
        absolute += 16;
        b.begin(0x2A); // vertex materials
        {
            b.begin(0x2B);
            b.begin(0x2C); // name
            b.write(b"bench\0");
            b.end();
            b.begin(0x2D); // info
            let data = mesh_formula_at(absolute, 32);
            b.write(&data);
            b.end();
            absolute += 32;
            b.end();
        }
        b.end();
        b.begin(0x30); // textures
        {
            b.begin(0x31);
            b.begin(0x32); // name
            b.write(b"bench.dds\0");
            b.end();
            b.begin(0x33); // info
            b.write(&[0x0C, 0x00, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0]);
            b.end();
            b.end();
        }
        b.end();
        b.begin(0x38); // material pass
        {
            b.begin(0x39); // vmat ids (single)
            b.write(&0u32.to_le_bytes());
            b.end();
            b.begin(0x3A); // shader ids (single)
            b.write(&0u32.to_le_bytes());
            b.end();
            b.begin(0x3B); // dcg
            let data = mesh_formula_at(absolute, n * 4);
            b.write(&data);
            b.end();
            absolute += n * 4;
            b.begin(0x48); // texture stage
            {
                b.begin(0x49); // texture ids (single)
                b.write(&0u32.to_le_bytes());
                b.end();
                b.begin(0x4A); // stage texcoords
                let data = mesh_formula_at(absolute, n * 8);
                b.write(&data);
                b.end();
                absolute += n * 8;
            }
            b.end();
        }
        b.end();
    }
    b.end();

    // the aabtree: half as many nodes as polygons, all leaves
    b.begin(0x90);
    {
        b.begin(0x91); // header
        b.write(&(n as u32 / 2).to_le_bytes());
        b.write(&(n as u32).to_le_bytes());
        b.write(&[0u8; 24]);
        b.end();
        b.begin(0x92); // poly indices
        let data = mesh_formula_at(absolute, n * 4);
        b.write(&data);
        b.end();
        absolute += n * 4;
        b.begin(0x93); // nodes
        let data = mesh_formula_at(absolute, (n / 2) * 32);
        b.write(&data);
        b.end();
    }
    b.end();

    b.bytes
}

/// Bytes → the loaded mesh: the model-viewer startup workload.
fn bench_mesh_parse(c: &mut Criterion) {
    let bytes = mesh_corpus();
    let len = bytes.len() as u64;
    assert_eq!(
        len, 180_684,
        "the bench corpus drifted from the C++ baseline"
    );

    let mut group = c.benchmark_group("mesh");
    group.throughput(Throughput::Bytes(len));
    group.bench_function("parse_x2048", |b| {
        b.iter(|| {
            let mesh = W3dMesh::parse(black_box(&bytes)).expect("the corpus parses");
            black_box((
                mesh.geometry.vertices.as_ref().map(|v| v[0]),
                mesh.flags,
                mesh.materials.dcg_source[0],
                mesh.aabtree.as_ref().map(|t| t.node_count),
            ))
        })
    });
    group.finish();
}

// ---------------------------------------------------------------------------
// w3d_htree — the skeleton-load workload (plan §6 discipline, Phase 2's
// W3D track). The C++ baseline is the htree.cpp transcription in
// tools/differential/bench_wrap_htree.cpp, driving the identical corpus
// bytes through its own Load_W3D.
// ---------------------------------------------------------------------------

/// Pivot count of the corpus (matches the C++ baseline).
const HTREE_PIVOTS: u32 = 2048;

/// The corpus formula anchored at an absolute offset (the C++ baseline
/// generates the same absolute-indexed bytes).
fn htree_formula_at(offset: usize, n: usize) -> Vec<u8> {
    (offset..offset + n)
        .map(|i| ((i * 7 + 13) & 0xFF) as u8)
        .collect()
}

/// Builds the hierarchy-shaped corpus: a v4.1 tree of 2048 chained
/// pivots, every 60-byte struct from the formula with the parent index
/// structured. Keep in lockstep with the C++ baseline's
/// `build_htree_corpus`.
fn htree_corpus() -> Vec<u8> {
    let mut bytes = Vec::with_capacity(8 + 36 + 8 + HTREE_PIVOTS as usize * 60);

    // the header chunk (36 bytes)
    bytes.extend_from_slice(&0x0000_0101u32.to_le_bytes());
    bytes.extend_from_slice(&36u32.to_le_bytes());
    bytes.extend_from_slice(&0x0004_0001u32.to_le_bytes()); // version 4.1
    let mut name = [0u8; 16];
    name[..5].copy_from_slice(b"bench");
    bytes.extend_from_slice(&name);
    bytes.extend_from_slice(&HTREE_PIVOTS.to_le_bytes());
    for word in [1.5f32, -2.25, 0.5] {
        bytes.extend_from_slice(&word.to_bits().to_le_bytes());
    }

    // the pivots chunk
    bytes.extend_from_slice(&0x0000_0102u32.to_le_bytes());
    bytes.extend_from_slice(&(HTREE_PIVOTS * 60).to_le_bytes());
    let base = 36 + 8; // past the header chunk
    for i in 0..HTREE_PIVOTS as usize {
        let mut raw = htree_formula_at(base + 8 + i * 60, 60);
        let parent: u32 = if i == 0 { 0xFFFF_FFFF } else { (i - 1) as u32 };
        raw[16..20].copy_from_slice(&parent.to_le_bytes());
        bytes.extend_from_slice(&raw);
    }
    bytes
}

/// Bytes → the loaded tree: the skeleton-resolution startup workload.
fn bench_htree_parse(c: &mut Criterion) {
    let bytes = htree_corpus();
    let len = bytes.len() as u64;
    assert_eq!(
        len, 122_932,
        "the bench corpus drifted from the C++ baseline"
    );

    let mut group = c.benchmark_group("htree");
    group.throughput(Throughput::Bytes(len));
    group.bench_function("parse_x2048", |b| {
        b.iter(|| {
            let tree = W3dHtree::parse(black_box(&bytes)).expect("the corpus parses");
            black_box((
                tree.pivots.len(),
                tree.get_parent_index(HTREE_PIVOTS as usize - 1),
                tree.pivots[HTREE_PIVOTS as usize - 1].base_transform[0].to_bits(),
                tree.pivots[HTREE_PIVOTS as usize - 1].base_transform[11].to_bits(),
                tree.pivots[HTREE_PIVOTS as usize - 1].name_str().len(),
            ))
        })
    });
    group.finish();
}

// ---------------------------------------------------------------------------
// w3d_hanim — the animation load and transform workloads (plan §6
// discipline, Phase 2's W3D track). The C++ baseline is the hrawanim.cpp
// transcription in tools/differential/bench_wrap_hanim.cpp, driving the
// identical corpus bytes through its own Load_W3D and Get_Transform.
// ---------------------------------------------------------------------------

/// Pivot count of the corpus (matches the C++ baseline).
const HANIM_PIVOTS: usize = 64;
/// Frame count of the corpus (matches the C++ baseline).
const HANIM_FRAMES: usize = 128;
/// Frames the transform bench samples (matches the C++ baseline).
const HANIM_QUERY_FRAMES: usize = 16;

/// One translation-channel word — the same bit formula the C++ baseline
/// builds (any pattern does: the queries only lerp them).
fn hanim_word(piv: u32, ty: u32, f: u32) -> f32 {
    f32::from_bits(
        f.wrapping_mul(0x0101_0101)
            ^ piv.wrapping_mul(0x0001_0001)
            ^ ty.wrapping_mul(0x1001_0010)
            ^ 0x3C00_0000,
    )
}

/// One quaternion word — small positive components keeping every key
/// pair inside the fast-table slerp domain (the C++ baseline's formula).
fn hanim_quat_word(piv: u32, f: u32, k: u32) -> f32 {
    f32::from_bits(0x3C00_0000 | (((f * 7 + k * 13 + piv * 3) & 0x3F) << 17))
}

/// A tiny chunk-framing builder for the corpus bytes (the same framing
/// the C++ baseline's builder produces).
struct AnimChunkBuild {
    bytes: Vec<u8>,
}

impl AnimChunkBuild {
    fn begin(&mut self, id: u32) {
        self.bytes.extend_from_slice(&id.to_le_bytes());
        self.bytes.extend_from_slice(&0u32.to_le_bytes());
    }

    fn end(&mut self, size: usize) {
        let at = self.bytes.len() - size - 8;
        self.bytes[at + 4..at + 8].copy_from_slice(&(size as u32).to_le_bytes());
    }

    fn w_u16(&mut self, v: u16) {
        self.bytes.extend_from_slice(&v.to_le_bytes());
    }

    fn w_u8(&mut self, v: u8) {
        self.bytes.push(v);
    }

    fn w_u32(&mut self, v: u32) {
        self.bytes.extend_from_slice(&v.to_le_bytes());
    }

    fn w_f32(&mut self, v: f32) {
        self.w_u32(v.to_bits());
    }
}

/// Builds the animation-shaped corpus: pivots 0..31 carry X/Y/Z
/// channels (128 frames of 1 float each), pivots 32..63 carry a
/// quaternion channel (128 frames of 4 components). Keep in lockstep
/// with the C++ baseline's `build_hanim_corpus`.
fn hanim_corpus() -> Vec<u8> {
    let mut b = AnimChunkBuild {
        bytes: Vec::with_capacity(117_300),
    };

    // the 44-byte header
    b.begin(0x0201);
    b.w_u32(0x0004_0000); // version 4.0
    let mut name = [0u8; 16];
    name[..3].copy_from_slice(b"Run");
    b.bytes.extend_from_slice(&name);
    let mut hname = [0u8; 16];
    hname[..5].copy_from_slice(b"bench");
    b.bytes.extend_from_slice(&hname);
    b.w_u32(HANIM_FRAMES as u32);
    b.w_u32(30);
    b.end(44);

    for piv in 0..HANIM_PIVOTS {
        if piv < HANIM_PIVOTS / 2 {
            for ty in 0u32..3 {
                b.begin(0x0202);
                b.w_u16(0);
                b.w_u16(HANIM_FRAMES as u16 - 1);
                b.w_u16(1);
                b.w_u16(ty as u16);
                b.w_u16(piv as u16);
                b.w_u16(0);
                for f in 0..HANIM_FRAMES {
                    b.w_f32(hanim_word(piv as u32, ty, f as u32));
                }
                b.end(12 + HANIM_FRAMES * 4);
            }
        } else {
            b.begin(0x0202);
            b.w_u16(0);
            b.w_u16(HANIM_FRAMES as u16 - 1);
            b.w_u16(4);
            b.w_u16(6); // ANIM_CHANNEL_Q
            b.w_u16(piv as u16);
            b.w_u16(0);
            for f in 0..HANIM_FRAMES {
                for k in 0..4 {
                    b.w_f32(hanim_quat_word(piv as u32, f as u32, k));
                }
            }
            b.end(12 + HANIM_FRAMES * 16);
        }
    }
    b.bytes
}

/// A parse-time tree stand-in for the "bench" hierarchy.
fn hanim_bench_tree() -> W3dHtree {
    use rzh_formats::w3d_htree::{W3dHierarchyHeader, W3dPivot};
    W3dHtree {
        header: W3dHierarchyHeader {
            version: 0,
            name: [0; 16],
            num_pivots: HANIM_PIVOTS as u32,
            center: [0.0; 3],
        },
        pivots: vec![W3dPivot::default(); HANIM_PIVOTS],
    }
}

/// Bytes → the loaded animation: the model-load startup workload.
fn bench_hanim_parse(c: &mut Criterion) {
    let bytes = hanim_corpus();
    let len = bytes.len() as u64;
    assert_eq!(
        len, 117_300,
        "the bench corpus drifted from the C++ baseline"
    );
    let tree = hanim_bench_tree();

    let mut group = c.benchmark_group("hanim");
    group.throughput(Throughput::Bytes(len));
    group.bench_function("parse_x64", |b| {
        b.iter(|| {
            let anim = W3dRawAnim::parse(black_box(&bytes), |name| {
                (name == b"bench").then_some(&tree)
            })
            .expect("the corpus parses");
            black_box((
                anim.num_frames(),
                anim.num_pivots(),
                anim.node_motion[0]
                    .x
                    .as_ref()
                    .map(|c| c.data[HANIM_FRAMES - 1].to_bits()),
                anim.node_motion[HANIM_PIVOTS - 1]
                    .q
                    .as_ref()
                    .map(|c| c.data[1].to_bits()),
            ))
        })
    });
    group.finish();
}

/// The per-frame Anim_Update shape: `Get_Transform` over every bone at
/// the sampled frames of the preloaded animation — the workload the tree
/// update pays per animated model per frame.
fn bench_hanim_xforms(c: &mut Criterion) {
    let bytes = hanim_corpus();
    let tree = hanim_bench_tree();
    let anim = W3dRawAnim::parse(&bytes, |name| (name == b"bench").then_some(&tree))
        .expect("the corpus parses");

    let transforms = (HANIM_QUERY_FRAMES * HANIM_PIVOTS) as u64;
    let mut group = c.benchmark_group("hanim");
    group.throughput(Throughput::Elements(transforms));
    group.bench_function("xforms_x1024", |b| {
        b.iter(|| {
            let mut checksum = 0u64;
            for fi in 0..HANIM_QUERY_FRAMES {
                let frame = (fi * (HANIM_FRAMES / HANIM_QUERY_FRAMES)) as f32 + 0.5;
                for piv in 0..anim.num_pivots() {
                    let m = anim.get_transform(piv, frame);
                    for k in (0..12).step_by(4) {
                        checksum = checksum.wrapping_add(u64::from(m[k].to_bits()));
                    }
                }
            }
            black_box(checksum)
        })
    });
    group.finish();
}

// ---------------------------------------------------------------------------
// w3d_hcanim — the compressed-animation load and transform workloads
// (plan §6 discipline, Phase 2's W3D track; the named follow-up to the
// raw animations). The C++ baseline is the hcanim.cpp transcription in
// tools/differential/bench_wrap_hcanim.cpp, driving the identical
// corpora bytes through its own Load_W3D and Get_Transform, for both
// compression flavors.
// ---------------------------------------------------------------------------

/// Pivot count of the corpora (matches the C++ baseline).
const HCANIM_PIVOTS: usize = 64;
/// Frame count of the corpora (matches the C++ baseline).
const HCANIM_FRAMES: usize = 128;
/// Frames the transform bench samples (matches the C++ baseline).
const HCANIM_QUERY_FRAMES: usize = 16;

/// One translation-channel word — the same bit formula the C++ baseline
/// builds (any pattern does: the queries only lerp or delta them).
fn hcanim_word(piv: u32, ty: u32, f: u32) -> f32 {
    f32::from_bits(
        f.wrapping_mul(0x0101_0101)
            ^ piv.wrapping_mul(0x0001_0001)
            ^ ty.wrapping_mul(0x1001_0010)
            ^ 0x3C00_0000,
    )
}

/// One quaternion word — small positive components keeping every key
/// pair inside the fast-table slerp domain (the C++ baseline's formula).
fn hcanim_quat_word(piv: u32, f: u32, k: u32) -> f32 {
    f32::from_bits(0x3C00_0000 | (((f * 7 + k * 13 + piv * 3) & 0x3F) << 17))
}

/// Deterministic adaptive-delta packet bytes for block `block`, vector
/// index `vi` (the golden corpus's own formula).
fn hcanim_ad_packet(vi: usize, block: usize) -> [u8; 9] {
    let mut p = [0u8; 9];
    p[0] = ((vi.wrapping_mul(37) + block.wrapping_mul(11)) & 0xFF) as u8;
    for k in 0..8 {
        p[1 + k] =
            ((vi.wrapping_mul(53) + block.wrapping_mul(29) + k.wrapping_mul(17)) & 0xFF) as u8;
    }
    p
}

/// blocks needed to cover frames 1..num_frames-1
fn hcanim_ad_blocks(num_frames: usize) -> usize {
    if num_frames <= 1 {
        0
    } else {
        (num_frames - 1).div_ceil(16)
    }
}

/// Builds the TIME-CODED corpus: pivots 0..31 carry X/Y/Z channels
/// (128 time-coded packets of one float each), pivots 32..63 carry a
/// quaternion channel (128 packets of four floats). Keep in lockstep
/// with the C++ baseline's `build_hcanim_corpus`.
fn hcanim_corpus() -> Vec<u8> {
    use rzh_formats::w3d_hcanim::W3D_CHUNK_COMPRESSED_ANIMATION_HEADER;

    let mut b = AnimChunkBuild {
        bytes: Vec::with_capacity(182_000),
    };

    // the 44-byte compressed header
    b.begin(W3D_CHUNK_COMPRESSED_ANIMATION_HEADER);
    b.w_u32(0x0004_0000); // version 4.0
    let mut name = [0u8; 16];
    name[..3].copy_from_slice(b"Run");
    b.bytes.extend_from_slice(&name);
    let mut hname = [0u8; 16];
    hname[..5].copy_from_slice(b"bench");
    b.bytes.extend_from_slice(&hname);
    b.w_u32(HCANIM_FRAMES as u32);
    b.w_u16(30); // the frame rate
    b.w_u16(0); // ANIM_FLAVOR_TIMECODED
    b.end(44);

    for piv in 0..HCANIM_PIVOTS {
        if piv < HCANIM_PIVOTS / 2 {
            for ty in 0u32..3 {
                b.begin(0x0282);
                b.w_u32(HCANIM_FRAMES as u32);
                b.w_u16(piv as u16);
                b.w_u8(1); // VectorLen
                b.w_u8(ty as u8); // ANIM_CHANNEL_X/Y/Z
                for f in 0..HCANIM_FRAMES {
                    b.w_u32(f as u32);
                    b.w_f32(hcanim_word(piv as u32, ty, f as u32));
                }
                b.end(8 + HCANIM_FRAMES * 8);
            }
        } else {
            b.begin(0x0282);
            b.w_u32(HCANIM_FRAMES as u32);
            b.w_u16(piv as u16);
            b.w_u8(4); // VectorLen
            b.w_u8(6); // ANIM_CHANNEL_Q
            for f in 0..HCANIM_FRAMES {
                b.w_u32(f as u32);
                for k in 0..4 {
                    b.w_f32(hcanim_quat_word(piv as u32, f as u32, k));
                }
            }
            b.end(8 + HCANIM_FRAMES * 20);
        }
    }
    b.bytes
}

/// Builds the ADAPTIVE-DELTA corpus of the same shape: pivots 0..31
/// carry X/Y/Z channels (one base float plus eight 9-byte packet
/// blocks), pivots 32..63 carry a quaternion channel (four base floats
/// plus eight blocks of four packets). Keep in lockstep with the C++
/// baseline's `build_hcanim_ad_corpus`.
fn hcanim_ad_corpus() -> Vec<u8> {
    use rzh_formats::w3d_hcanim::W3D_CHUNK_COMPRESSED_ANIMATION_HEADER;

    let mut b = AnimChunkBuild {
        bytes: Vec::with_capacity(19_500),
    };

    // the 44-byte compressed header
    b.begin(W3D_CHUNK_COMPRESSED_ANIMATION_HEADER);
    b.w_u32(0x0004_0000);
    let mut name = [0u8; 16];
    name[..3].copy_from_slice(b"Run");
    b.bytes.extend_from_slice(&name);
    let mut hname = [0u8; 16];
    hname[..5].copy_from_slice(b"bench");
    b.bytes.extend_from_slice(&hname);
    b.w_u32(HCANIM_FRAMES as u32);
    b.w_u16(30);
    b.w_u16(1); // ANIM_FLAVOR_ADAPTIVE_DELTA
    b.end(44);

    let blocks = hcanim_ad_blocks(HCANIM_FRAMES);
    for piv in 0..HCANIM_PIVOTS {
        if piv < HCANIM_PIVOTS / 2 {
            for ty in 0u32..3 {
                b.begin(0x0282);
                b.w_u32(HCANIM_FRAMES as u32);
                b.w_u16(piv as u16);
                b.w_u8(1); // VectorLen
                b.w_u8(ty as u8); // ANIM_CHANNEL_X/Y/Z
                b.w_f32(0.01); // Scale
                b.w_f32(hcanim_word(piv as u32, ty, 0)); // Data[0], the base float
                for blk in 0..blocks {
                    let p = hcanim_ad_packet(0, blk);
                    b.bytes.extend_from_slice(&p);
                }
                // the exporter's word-alignment pad: the payload after the
                // 16-byte struct must be whole u32 words
                let payload = 12 + 4 + blocks * 9;
                let pad = (4 - (payload - 16) % 4) % 4;
                for _ in 0..pad {
                    b.bytes.push(0xAB);
                }
                b.end(payload + pad);
            }
        } else {
            b.begin(0x0282);
            b.w_u32(HCANIM_FRAMES as u32);
            b.w_u16(piv as u16);
            b.w_u8(4); // VectorLen
            b.w_u8(6); // ANIM_CHANNEL_Q
            b.w_f32(0.001); // Scale
            for k in 0..4 {
                b.w_f32(hcanim_quat_word(piv as u32, 0, k)); // Data[0] and the rest
            }
            for blk in 0..blocks {
                for vi in 0..4 {
                    let p = hcanim_ad_packet(vi, blk);
                    b.bytes.extend_from_slice(&p);
                }
            }
            let payload = 12 + 4 * 4 + blocks * 4 * 9;
            let pad = (4 - (payload - 16) % 4) % 4;
            for _ in 0..pad {
                b.bytes.push(0xAB);
            }
            b.end(payload + pad);
        }
    }
    b.bytes
}

/// A parse-time tree stand-in for the "bench" hierarchy.
fn hcanim_bench_tree() -> W3dHtree {
    use rzh_formats::w3d_htree::{W3dHierarchyHeader, W3dPivot};
    W3dHtree {
        header: W3dHierarchyHeader {
            version: 0,
            name: [0; 16],
            num_pivots: HCANIM_PIVOTS as u32,
            center: [0.0; 3],
        },
        pivots: vec![W3dPivot::default(); HCANIM_PIVOTS],
    }
}

/// Bytes → the loaded TIME-CODED animation: the model-load startup
/// workload for the compressed flavor every `.w3d` asset ships in.
fn bench_hcanim_parse(c: &mut Criterion) {
    let bytes = hcanim_corpus();
    let len = bytes.len() as u64;
    assert_eq!(
        len, 182_324,
        "the bench corpus drifted from the C++ baseline"
    );
    let tree = hcanim_bench_tree();

    let mut group = c.benchmark_group("hcanim");
    group.throughput(Throughput::Bytes(len));
    group.bench_function("parse_x64", |b| {
        b.iter(|| {
            let anim = W3dCompressedAnim::parse(black_box(&bytes), |name| {
                (name == b"bench").then_some(&tree)
            })
            .expect("the corpus parses");
            black_box((
                anim.num_frames(),
                anim.num_pivots(),
                anim.node_motion[0].x.as_ref().map(|ch| match ch {
                    W3dCompressedChannel::TimeCoded(tc) => tc.data[HCANIM_FRAMES * 2 - 1],
                    _ => unreachable!(),
                }),
            ))
        })
    });
    group.finish();
}

/// Bytes → the loaded ADAPTIVE-DELTA animation: the same walk over the
/// delta-compressed flavor (the 16-byte struct, the base floats and the
/// 9-byte packet blocks, the exporter's word pad included).
fn bench_hcanim_parse_ad(c: &mut Criterion) {
    let bytes = hcanim_ad_corpus();
    let len = bytes.len() as u64;
    assert_eq!(
        len, 19_636,
        "the AD bench corpus drifted from the C++ baseline"
    );
    let tree = hcanim_bench_tree();

    let mut group = c.benchmark_group("hcanim");
    group.throughput(Throughput::Bytes(len));
    group.bench_function("parse_ad_x64", |b| {
        b.iter(|| {
            let anim = W3dCompressedAnim::parse(black_box(&bytes), |name| {
                (name == b"bench").then_some(&tree)
            })
            .expect("the corpus parses");
            black_box((
                anim.num_frames(),
                anim.num_pivots(),
                anim.node_motion[0].x.as_ref().map(|ch| match ch {
                    W3dCompressedChannel::AdaptiveDelta(ad) => ad.data[4],
                    _ => unreachable!(),
                }),
            ))
        })
    });
    group.finish();
}

/// The per-frame Anim_Update shape on the adaptive-delta corpus:
/// `Get_Transform` over every bone at 16 consecutive sampled frames of
/// the preloaded animation — the decompression cache's slide path plus
/// the slerp and `Build_Matrix3D`, the workload the tree update pays
/// per animated model per frame.
fn bench_hcanim_xforms_ad(c: &mut Criterion) {
    let bytes = hcanim_ad_corpus();
    let tree = hcanim_bench_tree();
    let mut anim = W3dCompressedAnim::parse(&bytes, |name| (name == b"bench").then_some(&tree))
        .expect("the corpus parses");

    let transforms = (HCANIM_QUERY_FRAMES * HCANIM_PIVOTS) as u64;
    let mut group = c.benchmark_group("hcanim");
    group.throughput(Throughput::Elements(transforms));
    group.bench_function("xforms_ad_x1024", |b| {
        b.iter(|| {
            let mut checksum = 0u64;
            for fi in 0..HCANIM_QUERY_FRAMES {
                let frame = fi as f32 + 0.5;
                for piv in 0..anim.num_pivots() {
                    let m = anim.get_transform(piv, frame);
                    for k in (0..12).step_by(4) {
                        checksum = checksum.wrapping_add(u64::from(m[k].to_bits()));
                    }
                }
            }
            black_box(checksum)
        })
    });
    group.finish();
}

// ---------------------------------------------------------------------------
// mapchunk: the map data-chunk container (DataChunk.cpp)
// ---------------------------------------------------------------------------

mod mapchunk_benches {
    use super::*;
    use rzh_formats::map_chunk::{
        DataChunkInfo, DataChunkInput, DataChunkOutput, Dict, DictValue, NameKeys, ParserTable,
    };

    /// The bench parser state (the userData the original hands every
    /// parser); the checksum mirrors the C++ baseline's.
    struct BenchState {
        keys: NameKeys,
        checksum: u64,
    }

    fn heights(n: usize) -> Vec<u8> {
        (0..n).map(|i| ((i * 5 + 11) & 0xFF) as u8).collect()
    }

    fn add_dict_checksum(state: &mut BenchState, dict: &Dict) {
        state.checksum += dict.pair_count() as u64;
        for i in 0..dict.pair_count() {
            state.checksum += dict.nth_key(i).ordinal() as u64;
            state.checksum += dict.nth_type(i) as u64;
            match dict.nth_value(i) {
                DictValue::Bool(b) => state.checksum += u64::from(*b),
                DictValue::Int(v) => state.checksum = state.checksum.wrapping_add(*v as u64),
                DictValue::Real(v) => state.checksum += v.to_bits() as u64,
                DictValue::AsciiString(s) => state.checksum += s.len() as u64,
                DictValue::UnicodeString(u) => state.checksum += u.len() as u64,
            }
        }
    }

    fn b_height(
        file: &mut DataChunkInput<'_>,
        _info: &DataChunkInfo,
        _table: &mut ParserTable<'_, BenchState>,
        state: &mut BenchState,
    ) -> bool {
        state.checksum = state.checksum.wrapping_add(file.read_int() as u64);
        state.checksum = state.checksum.wrapping_add(file.read_int() as u64);
        let mut buf = [0u8; 2048];
        file.read_array_of_bytes(&mut buf);
        for byte in buf {
            state.checksum += byte as u64;
        }
        state.checksum = state.checksum.wrapping_add(file.read_int() as u64);
        true
    }

    fn b_blend(
        file: &mut DataChunkInput<'_>,
        _info: &DataChunkInfo,
        _table: &mut ParserTable<'_, BenchState>,
        state: &mut BenchState,
    ) -> bool {
        state.checksum = state.checksum.wrapping_add(file.read_int() as u64);
        let mut buf = [0u8; 512];
        file.read_array_of_bytes(&mut buf);
        for byte in buf {
            state.checksum += byte as u64;
        }
        true
    }

    fn b_dict(
        file: &mut DataChunkInput<'_>,
        _info: &DataChunkInfo,
        _table: &mut ParserTable<'_, BenchState>,
        state: &mut BenchState,
    ) -> bool {
        if let Ok(dict) = file.read_dict(&mut state.keys) {
            add_dict_checksum(state, &dict);
        }
        true
    }

    fn b_objects(
        file: &mut DataChunkInput<'_>,
        _info: &DataChunkInfo,
        table: &mut ParserTable<'_, BenchState>,
        state: &mut BenchState,
    ) -> bool {
        file.parse(table, state)
    }

    fn b_object(
        file: &mut DataChunkInput<'_>,
        _info: &DataChunkInfo,
        _table: &mut ParserTable<'_, BenchState>,
        state: &mut BenchState,
    ) -> bool {
        let key = file.read_name_key(&mut state.keys);
        state.checksum += key.ordinal() as u64;
        if let Ok(dict) = file.read_dict(&mut state.keys) {
            add_dict_checksum(state, &dict);
        }
        state.checksum += file.read_ascii_string().len() as u64;
        true
    }

    /// Builds the map-shaped corpus through the port's writer — keep in
    /// lockstep with tools/differential/bench_wrap_mapchunk.cpp.
    fn build_map() -> Vec<u8> {
        let mut keys = NameKeys::new();
        let mut writer = DataChunkOutput::new();

        writer.open_data_chunk(b"HeightMapData", 3);
        writer.write_int(64);
        writer.write_int(64);
        writer.write_array_of_bytes(&heights(2048));
        writer.write_int(5);
        writer.close_data_chunk();

        writer.open_data_chunk(b"BlendTileData", 7);
        writer.write_int(512);
        writer.write_array_of_bytes(&heights(512));
        writer.close_data_chunk();

        writer.open_data_chunk(b"GlobalLighting", 2);
        let mut dict = Dict::new();
        dict.set(keys.name_to_key(b"Ambient"), DictValue::Real(0.25));
        dict.set(
            keys.name_to_key(b"Sun"),
            DictValue::AsciiString(b"Warm".to_vec()),
        );
        dict.set(keys.name_to_key(b"Moon"), DictValue::Real(0.125));
        dict.set(keys.name_to_key(b"Night"), DictValue::Bool(true));
        writer.write_dict(&keys, &dict);
        writer.close_data_chunk();

        writer.open_data_chunk(b"WorldInfo", 1);
        let mut dict = Dict::new();
        dict.set(
            keys.name_to_key(b"Name"),
            DictValue::AsciiString(b"Benchmark Map".to_vec()),
        );
        dict.set(keys.name_to_key(b"WidthTiles"), DictValue::Int(64));
        dict.set(keys.name_to_key(b"HeightTiles"), DictValue::Int(64));
        dict.set(keys.name_to_key(b"IsMultiplayer"), DictValue::Bool(true));
        dict.set(
            keys.name_to_key(b"Author"),
            DictValue::AsciiString(b"rzh".to_vec()),
        );
        writer.write_dict(&keys, &dict);
        writer.close_data_chunk();

        writer.open_data_chunk(b"ObjectsList", 3);
        for i in 0..256 {
            writer.open_data_chunk(b"MapObject", 1);
            let object_name_key = keys.name_to_key(b"ObjectName");
            writer.write_name_key(&keys, object_name_key);
            let mut dict = Dict::new();
            dict.set(keys.name_to_key(b"ObjectFlags"), DictValue::Int(i));
            dict.set(
                keys.name_to_key(b"Owner"),
                DictValue::AsciiString(b"Player1".to_vec()),
            );
            writer.write_dict(&keys, &dict);
            writer.write_ascii_string(format!("Obj{i:05}").as_bytes());
            writer.close_data_chunk();
        }
        writer.close_data_chunk();

        writer.into_bytes().expect("finish")
    }

    /// The WorldBuilder shape: a fresh writer per batch, the whole map
    /// built, the byte count returned.
    pub fn bench_write(c: &mut Criterion) {
        let mut group = c.benchmark_group("mapchunk");
        let bytes = build_map();
        group.throughput(Throughput::Bytes(bytes.len() as u64));
        group.bench_function("write_x2048", |b| b.iter(|| black_box(build_map()).len()));
        group.finish();
    }

    /// The game-load shape: the registered-parser walk reading every
    /// value, returning the checksum (the same value the C++ baseline
    /// computes).
    pub fn bench_parse(c: &mut Criterion) {
        let bytes = build_map();
        let mut group = c.benchmark_group("mapchunk");
        group.throughput(Throughput::Bytes(bytes.len() as u64));
        group.bench_function("parse_x2048", |b| {
            b.iter(|| {
                let mut input = DataChunkInput::new(black_box(&bytes));
                let mut state = BenchState {
                    keys: NameKeys::new(),
                    checksum: 0,
                };
                let mut table: ParserTable<'_, BenchState> = ParserTable::new();
                table.register(b"HeightMapData", b"", b_height);
                table.register(b"BlendTileData", b"", b_blend);
                table.register(b"GlobalLighting", b"", b_dict);
                table.register(b"WorldInfo", b"", b_dict);
                table.register(b"ObjectsList", b"", b_objects);
                table.register(b"MapObject", b"ObjectsList", b_object);
                if !black_box(input.parse(&mut table, &mut state)) {
                    return 0xdead_beef_u64;
                }
                state.checksum
            })
        });
        group.finish();
    }
}

// ---------------------------------------------------------------------------
// mapheightmap: the map height-map payload (WorldHeightMap.cpp)
// ---------------------------------------------------------------------------

mod mapheightmap_benches {
    use super::*;
    use rzh_formats::map_chunk::{DataChunkInfo, DataChunkInput, ParserTable};
    use rzh_formats::map_heightmap::{
        HEIGHT_MAP_DATA_LABEL, HeightMapData, HeightMapError, K_HEIGHT_MAP_VERSION_1,
        K_HEIGHT_MAP_VERSION_4, parse_height_map_data, parse_size_only,
    };

    /// The bench corpus extent (a typical skirmish map: 256×256 cells).
    const EXTENT: i32 = 256;

    fn heights(n: usize, seed: u8) -> Vec<u8> {
        (0..n).map(|i| seed.wrapping_add((i * 7) as u8)).collect()
    }

    fn put_i32(b: &mut Vec<u8>, v: i32) {
        b.extend_from_slice(&v.to_le_bytes());
    }
    fn put_chunk(b: &mut Vec<u8>, id: u32, ver: u16, data: &[u8]) {
        b.extend_from_slice(&id.to_le_bytes());
        b.extend_from_slice(&ver.to_le_bytes());
        b.extend_from_slice(&(data.len() as i32).to_le_bytes());
        b.extend_from_slice(data);
    }
    fn toc_bytes(entries: &[(&[u8], u32)]) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(b"CkMp");
        b.extend_from_slice(&(entries.len() as i32).to_le_bytes());
        for (name, id) in entries {
            b.push(name.len() as u8);
            b.extend_from_slice(name);
            b.extend_from_slice(&id.to_le_bytes());
        }
        b
    }

    /// The version-4 corpus: a 256×256 map, border 3, 8 boundaries,
    /// 65 536 formula samples — keep in lockstep with
    /// tools/differential/bench_wrap_mapheightmap.cpp.
    fn build_v4_corpus() -> Vec<u8> {
        let mut b = toc_bytes(&[(HEIGHT_MAP_DATA_LABEL, 1)]);
        let mut p = Vec::new();
        put_i32(&mut p, EXTENT);
        put_i32(&mut p, EXTENT);
        put_i32(&mut p, 3);
        put_i32(&mut p, 8);
        for i in 0..8 {
            put_i32(&mut p, 240 - 8 * i);
            put_i32(&mut p, 200 + 4 * i);
        }
        let data = heights(EXTENT as usize * EXTENT as usize, 91);
        put_i32(&mut p, data.len() as i32);
        p.extend_from_slice(&data);
        put_chunk(&mut b, 1, K_HEIGHT_MAP_VERSION_4, &p);
        b
    }

    /// The version-1 corpus: the same extent and samples, the legacy
    /// no-border payload.
    fn build_v1_corpus() -> Vec<u8> {
        let mut b = toc_bytes(&[(HEIGHT_MAP_DATA_LABEL, 1)]);
        let mut p = Vec::new();
        put_i32(&mut p, EXTENT);
        put_i32(&mut p, EXTENT);
        let data = heights(EXTENT as usize * EXTENT as usize, 93);
        put_i32(&mut p, data.len() as i32);
        p.extend_from_slice(&data);
        put_chunk(&mut b, 1, K_HEIGHT_MAP_VERSION_1, &p);
        b
    }

    /// The walk's user state: the loaded height map and the checksum
    /// the C++ baseline computes over the same fields.
    struct BenchHm {
        logical_only: bool,
        checksum: u64,
        err: Option<HeightMapError>,
    }

    fn add_checksum(hm: &HeightMapData, checksum: &mut u64) {
        *checksum = checksum.wrapping_add(hm.width as u32 as u64);
        *checksum = checksum.wrapping_add(hm.height as u32 as u64);
        *checksum = checksum.wrapping_add(hm.border_size as u32 as u64);
        *checksum = checksum.wrapping_add(hm.data_size() as u32 as u64);
        for bnd in &hm.boundaries {
            *checksum = checksum.wrapping_add(bnd.x as u32 as u64);
            *checksum = checksum.wrapping_add(bnd.y as u32 as u64);
        }
        for &c in &hm.data {
            *checksum = checksum.wrapping_add(c as u64);
        }
    }

    fn bench_hm_parser(
        file: &mut DataChunkInput<'_>,
        info: &DataChunkInfo,
        _table: &mut ParserTable<'_, BenchHm>,
        st: &mut BenchHm,
    ) -> bool {
        let parsed = if st.logical_only {
            parse_size_only(file, info)
        } else {
            parse_height_map_data(file, info)
        };
        match parsed {
            Ok(hm) => {
                add_checksum(&hm, &mut st.checksum);
                true
            }
            Err(e) => {
                st.err = Some(e);
                false
            }
        }
    }

    fn run_walk(bytes: &[u8], logical_only: bool) -> u64 {
        let mut input = DataChunkInput::new(bytes);
        let mut table: ParserTable<'_, BenchHm> = ParserTable::new();
        table.register(HEIGHT_MAP_DATA_LABEL, b"", bench_hm_parser);
        let mut st = BenchHm {
            logical_only,
            checksum: 0,
            err: None,
        };
        if !input.parse(&mut table, &mut st) || st.err.is_some() {
            return 0xdead_beef_u64;
        }
        st.checksum
    }

    /// The modern load: the full walk over the version-4 corpus.
    pub fn bench_parse(c: &mut Criterion) {
        let bytes = build_v4_corpus();
        let mut group = c.benchmark_group("mapheightmap");
        group.throughput(Throughput::Bytes(bytes.len() as u64));
        group.bench_function("parse_x64k", |b| {
            b.iter(|| black_box(run_walk(black_box(&bytes), false)))
        });
        group.finish();
    }

    /// The legacy load: the logical walk over the version-1 corpus (the
    /// in-place downsample of 65 536 samples and the extent re-point).
    pub fn bench_parse_v1(c: &mut Criterion) {
        let bytes = build_v1_corpus();
        let mut group = c.benchmark_group("mapheightmap");
        group.throughput(Throughput::Bytes(bytes.len() as u64));
        group.bench_function("parse_v1_x64k", |b| {
            b.iter(|| black_box(run_walk(black_box(&bytes), true)))
        });
        group.finish();
    }
}

// ---------------------------------------------------------------------------
// mapblendtile: the map blend-tile payload (WorldHeightMap.cpp)
// ---------------------------------------------------------------------------

mod mapblendtile_benches {
    use super::*;
    use rzh_formats::map_blendtile::{
        BLEND_TILE_DATA_LABEL, FLAG_VAL, K_BLEND_TILE_VERSION_6, K_BLEND_TILE_VERSION_8,
        TerrainLoad,
    };
    use rzh_formats::map_chunk::DataChunkOutput;
    use rzh_formats::map_heightmap::{HEIGHT_MAP_DATA_LABEL, K_HEIGHT_MAP_VERSION_4};

    /// The bench corpus extent (a typical skirmish map: 256×256 cells).
    const EXTENT: i32 = 256;
    /// The bench counts (a busy but realistic payload).
    const NBMP: i32 = 512;
    const NBLEND: i32 = 513;
    const NCLIFF: i32 = 65;
    const NTEX: i32 = 8;

    /// The authoring-time cross-check constants: both sides walk
    /// identical bytes and agree on the checksums
    /// (tools/differential/bench_wrap_mapblendtile.cpp).
    const V8_CHECKSUM: u64 = 454_954_969_336;
    const V6_CHECKSUM: u64 = 454_953_987_841;

    fn heights(n: usize, seed: u8) -> Vec<u8> {
        (0..n).map(|i| seed.wrapping_add((i * 7) as u8)).collect()
    }
    fn i16_formula(n: usize, seed: i16) -> Vec<i16> {
        (0..n)
            .map(|i| ((i32::from(seed) + (i as i32) * 7) & 0xFFFF) as i16)
            .collect()
    }

    /// The height-map payload: a version-4 256×256 map, border 3, 8
    /// boundaries — the mapheightmap bench's v4 corpus shape.
    fn height_payload() -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&EXTENT.to_le_bytes());
        p.extend_from_slice(&EXTENT.to_le_bytes());
        p.extend_from_slice(&3i32.to_le_bytes());
        p.extend_from_slice(&8i32.to_le_bytes());
        for i in 0..8i32 {
            p.extend_from_slice(&(240 - 8 * i).to_le_bytes());
            p.extend_from_slice(&(200 + 4 * i).to_le_bytes());
        }
        let data = heights(EXTENT as usize * EXTENT as usize, 91);
        p.extend_from_slice(&(data.len() as i32).to_le_bytes());
        p.extend_from_slice(&data);
        p
    }

    /// The blend chunk written through the container's writer — the same
    /// bytes the C++ baseline's builder assembles.
    fn write_blend(out: &mut DataChunkOutput, version: u16, with_cliff_state: bool) {
        let len = EXTENT * EXTENT;
        out.open_data_chunk(BLEND_TILE_DATA_LABEL, version);
        out.write_int(len);
        for seed in [33i16, 44, 55, 66] {
            for v in i16_formula(len as usize, seed) {
                out.write_array_of_bytes(&v.to_le_bytes());
            }
        }
        if with_cliff_state {
            out.write_array_of_bytes(&heights(((EXTENT + 7) / 8 * EXTENT) as usize, 77));
        }
        out.write_int(NBMP);
        out.write_int(NBLEND);
        out.write_int(NCLIFF);
        out.write_int(NTEX);
        for i in 0..NTEX {
            out.write_int(i * 4); // first tile
            out.write_int(4); // num tiles
            out.write_int(2); // width
            out.write_int(0); // the legacy GDF word
            out.write_ascii_string(format!("BenchTex{i}").as_bytes());
        }
        out.write_int(NTEX * 4); // edge tiles
        out.write_int(NTEX); // edge classes
        for i in 0..NTEX {
            out.write_int(NTEX * 4 + i * 4);
            out.write_int(4);
            out.write_int(2);
            out.write_ascii_string(format!("BenchTex{i}").as_bytes());
        }
        for i in 1..NBLEND {
            out.write_int(i); // blend ndx
            out.write_byte((i & 0xff) as u8);
            out.write_byte(((i * 3) & 0xff) as u8);
            out.write_byte(((i * 5) & 0xff) as u8);
            out.write_byte(((i * 7) & 0xff) as u8);
            out.write_byte((i & 0x03) as u8); // inverted
            out.write_byte((i & 1) as u8); // long diagonal
            out.write_int((i % 7) - 1); // custom edge class
            out.write_int(FLAG_VAL); // the sentinel
        }
        for i in 1..NCLIFF {
            let f = (i as f32) * 0.25;
            out.write_int(i * 100); // tile index
            for v in [
                f,
                f + 0.125,
                f + 0.25,
                f + 0.375,
                f + 0.5,
                f + 0.625,
                f + 0.75,
                f + 0.875,
            ] {
                out.write_real(v);
            }
            out.write_byte((i & 1) as u8); // flip
            out.write_byte(((i >> 1) & 1) as u8); // mutant
        }
        out.close_data_chunk();
    }

    /// The version-8 corpus (the direct cliff-state bitmap) — keep in
    /// lockstep with tools/differential/bench_wrap_mapblendtile.cpp.
    fn build_corpus(version: u16, with_cliff_state: bool) -> Vec<u8> {
        let mut out = DataChunkOutput::new();
        out.open_data_chunk(HEIGHT_MAP_DATA_LABEL, K_HEIGHT_MAP_VERSION_4);
        out.write_array_of_bytes(&height_payload());
        out.close_data_chunk();
        write_blend(&mut out, version, with_cliff_state);
        out.into_bytes().expect("finish")
    }

    /// The checksum over the loaded state (the C++ baseline computes the
    /// same walk over the same fields).
    fn checksum(load: &TerrainLoad) -> u64 {
        let bt = load.blend_tiles.as_ref().expect("blend payload");
        let mut c = 0u64;
        for v in [
            load.width,
            load.height,
            load.data_size,
            bt.num_bitmap_tiles,
            bt.num_blended_tiles,
            bt.num_cliff_info,
            bt.num_texture_classes,
            bt.num_edge_tiles,
            bt.num_edge_texture_classes,
            bt.flip_state_width,
        ] {
            c = c.wrapping_add(v as u32 as u64);
        }
        for arr in [
            &bt.tile_ndxes,
            &bt.blend_tile_ndxes,
            &bt.extra_blend_tile_ndxes,
            &bt.cliff_info_ndxes,
        ] {
            for &v in arr {
                c = c.wrapping_add(v as u16 as u64);
            }
        }
        for b in bt.cell_flip_state.iter().chain(&bt.cell_cliff_state) {
            c = c.wrapping_add(u64::from(*b));
        }
        for t in &bt.texture_classes {
            c = c.wrapping_add(t.first_tile as u32 as u64);
            c = c.wrapping_add(t.num_tiles as u32 as u64);
            c = c.wrapping_add(t.width as u32 as u64);
            for &b in &t.name {
                c = c.wrapping_add(u64::from(b));
            }
        }
        for t in &bt.edge_texture_classes {
            c = c.wrapping_add(t.first_tile as u32 as u64);
            c = c.wrapping_add(t.num_tiles as u32 as u64);
            c = c.wrapping_add(t.width as u32 as u64);
            for &b in &t.name {
                c = c.wrapping_add(u64::from(b));
            }
        }
        for i in 1..bt.num_blended_tiles {
            let r = &bt.blended_tiles[i as usize];
            c = c.wrapping_add(r.blend_ndx as u32 as u64);
            c = c.wrapping_add(u64::from(r.horiz));
            c = c.wrapping_add(u64::from(r.vert));
            c = c.wrapping_add(u64::from(r.right_diagonal));
            c = c.wrapping_add(u64::from(r.left_diagonal));
            c = c.wrapping_add(u64::from(r.inverted));
            c = c.wrapping_add(u64::from(r.long_diagonal));
            c = c.wrapping_add(r.custom_blend_edge_class as u32 as u64);
        }
        for i in 1..bt.num_cliff_info {
            let cl = &bt.cliff_infos[i as usize];
            c = c.wrapping_add(cl.tile_index as u16 as u64);
            c = c.wrapping_add(u64::from(cl.u0.to_bits()));
            c = c.wrapping_add(u64::from(cl.v3.to_bits()));
            c = c.wrapping_add(u64::from(cl.flip));
            c = c.wrapping_add(u64::from(cl.mutant));
        }
        c
    }

    /// The modern load: the constructor's full walk (both readers, the
    /// patch loop) over the version-8 corpus — the largest map payload.
    pub fn bench_parse(c: &mut Criterion) {
        let bytes = build_corpus(K_BLEND_TILE_VERSION_8, true);
        let expect = checksum(&TerrainLoad::from_map(&bytes, true).expect("loads"));
        assert_eq!(
            expect, V8_CHECKSUM,
            "the v8 corpus drifted from the C++ baseline"
        );
        let mut group = c.benchmark_group("mapblendtile");
        group.throughput(Throughput::Bytes(bytes.len() as u64));
        group.bench_function("parse_x64k", |b| {
            b.iter(|| {
                let load = TerrainLoad::from_map(black_box(&bytes), true).expect("loads");
                black_box(checksum(&load))
            })
        });
        group.finish();
    }

    /// The legacy load: the same walk over the version-6 corpus (the
    /// height-derived cliff bitmap over all 65 536 cells).
    pub fn bench_parse_v6(c: &mut Criterion) {
        let bytes = build_corpus(K_BLEND_TILE_VERSION_6, false);
        let expect = checksum(&TerrainLoad::from_map(&bytes, true).expect("loads"));
        assert_eq!(
            expect, V6_CHECKSUM,
            "the v6 corpus drifted from the C++ baseline"
        );
        let mut group = c.benchmark_group("mapblendtile");
        group.throughput(Throughput::Bytes(bytes.len() as u64));
        group.bench_function("parse_v6_x64k", |b| {
            b.iter(|| {
                let load = TerrainLoad::from_map(black_box(&bytes), true).expect("loads");
                black_box(checksum(&load))
            })
        });
        group.finish();
    }
}

// ---------------------------------------------------------------------------
// maplighting: the map global-lighting payload (WorldHeightMap.cpp)
// ---------------------------------------------------------------------------

mod maplighting_benches {
    use super::*;
    use rzh_formats::map_chunk::{DataChunkInfo, DataChunkInput, ParserTable};
    use rzh_formats::map_lighting::{
        GLOBAL_LIGHTING_LABEL, GlobalLighting, K_LIGHTING_VERSION_3, LIGHTING_TIME_OF_DAY_COUNT,
        MAX_GLOBAL_LIGHTS, parse_lighting_data_chunk,
    };

    /// The bench corpus repetition: the lighting chunk is fixed-size
    /// (872 bytes), so the workload scales by repeating it (matches the
    /// C++ baseline).
    const BENCH_CHUNKS: usize = 256;

    /// The corpus checksum the C++ baseline computes over the same
    /// fields (bench_wrap_maplighting.cpp, verified at authoring time).
    const CHECKSUM: u64 = 61_160_372_863_360;

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
    fn toc_bytes(entries: &[(&[u8], u32)]) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(b"CkMp");
        b.extend_from_slice(&(entries.len() as i32).to_le_bytes());
        for (name, id) in entries {
            b.push(name.len() as u8);
            b.extend_from_slice(name);
            b.extend_from_slice(&id.to_le_bytes());
        }
        b
    }

    /// One light's 9 reals (the gen's `put_light` formula).
    fn put_light(b: &mut Vec<u8>, base: f32) {
        for k in 0..9u32 {
            put_f32(b, base + k as f32 * 0.25);
        }
    }

    /// A full version-3 payload (the gen's `lighting_payload`).
    fn lighting_payload(tod: i32, base: f32, shadow: u32) -> Vec<u8> {
        let mut p = Vec::new();
        put_i32(&mut p, tod);
        for i in 0..4u32 {
            put_light(&mut p, base + i as f32);
            put_light(&mut p, base + 10.0 + i as f32);
            for j in 1..3u32 {
                put_light(&mut p, base + 20.0 + (j * 4 + i) as f32);
            }
            for j in 1..3u32 {
                put_light(&mut p, base + 40.0 + (j * 4 + i) as f32);
            }
        }
        put_i32(&mut p, shadow as i32);
        p
    }

    /// Builds the corpus: 256 version-3 GlobalLighting chunks, formula
    /// values — keep in lockstep with
    /// tools/differential/bench_wrap_maplighting.cpp.
    fn build_corpus() -> Vec<u8> {
        let mut b = toc_bytes(&[(GLOBAL_LIGHTING_LABEL, 1)]);
        for c in 0..BENCH_CHUNKS {
            let p = lighting_payload(2, (c % 64) as f32 * 0.25, 0x0100_0000 | c as u32);
            put_chunk(&mut b, 1, K_LIGHTING_VERSION_3, &p);
        }
        b
    }

    /// The walk's user state: the accumulated per-chunk checksum (the
    /// same fields the C++ baseline sums).
    struct BenchGl {
        checksum: u64,
    }

    fn add_checksum(gl: &GlobalLighting, checksum: &mut u64) {
        *checksum = checksum.wrapping_add(gl.time_of_day as u32 as u64);
        if let Some(c) = gl.shadow_color {
            *checksum = checksum.wrapping_add(c as u64);
        }
        for i in 0..LIGHTING_TIME_OF_DAY_COUNT {
            for j in 0..MAX_GLOBAL_LIGHTS {
                for light in [
                    &gl.terrain_lighting[i][j],
                    &gl.terrain_objects_lighting[i][j],
                ] {
                    for w in [
                        light.ambient.red,
                        light.ambient.green,
                        light.ambient.blue,
                        light.diffuse.red,
                        light.diffuse.green,
                        light.diffuse.blue,
                        light.light_pos.x,
                        light.light_pos.y,
                        light.light_pos.z,
                    ] {
                        *checksum = checksum.wrapping_add(w.to_bits() as u64);
                    }
                }
            }
        }
    }

    fn bench_gl_parser(
        file: &mut DataChunkInput<'_>,
        info: &DataChunkInfo,
        _table: &mut ParserTable<'_, BenchGl>,
        st: &mut BenchGl,
    ) -> bool {
        let gl = parse_lighting_data_chunk(file, info);
        add_checksum(&gl, &mut st.checksum);
        true
    }

    fn run_walk(bytes: &[u8]) -> u64 {
        let mut input = DataChunkInput::new(bytes);
        let mut table: ParserTable<'_, BenchGl> = ParserTable::new();
        table.register(GLOBAL_LIGHTING_LABEL, b"", bench_gl_parser);
        let mut st = BenchGl { checksum: 0 };
        if !input.parse(&mut table, &mut st) {
            return 0xdead_beef_u64;
        }
        st.checksum
    }

    /// The lighting load: the full walk over the repeated-chunk corpus
    /// (every chunk parses; the last one wins).
    pub fn bench_parse(c: &mut Criterion) {
        let bytes = build_corpus();
        let got = run_walk(&bytes);
        assert_eq!(
            got, CHECKSUM,
            "the lighting corpus drifted from the C++ baseline"
        );
        let mut group = c.benchmark_group("maplighting");
        group.throughput(Throughput::Bytes(bytes.len() as u64));
        group.bench_function("parse_x220k", |b| {
            b.iter(|| black_box(run_walk(black_box(&bytes))))
        });
        group.finish();
    }
}

// ---------------------------------------------------------------------------
// mapobjects: the map objects-list payload (WorldHeightMap.cpp)
// ---------------------------------------------------------------------------

mod mapobjects_benches {
    use super::*;
    use rzh_formats::map_objects::{K_OBJECTS_VERSION_3, MapObjectData, ObjectsListData};

    /// The bench corpus size: 4096 objects ≈ 180 KiB (matches the C++
    /// baseline).
    const BENCH_OBJECTS: usize = 4096;

    /// The corpus checksum the C++ baseline computes over the same
    /// fields (bench_wrap_mapobjects.cpp, verified at authoring time).
    const CHECKSUM: u64 = 22_214_270_726_635;

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
    fn toc_bytes(entries: &[(&[u8], u32)]) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(b"CkMp");
        b.extend_from_slice(&(entries.len() as i32).to_le_bytes());
        for (name, id) in entries {
            b.push(name.len() as u8);
            b.extend_from_slice(name);
            b.extend_from_slice(&id.to_le_bytes());
        }
        b
    }

    /// Builds the corpus: one ObjectsList chunk with 4096 version-3
    /// Object children (formula locations, angles, flags, names and
    /// two-pair dicts) — keep in lockstep with
    /// tools/differential/bench_wrap_mapobjects.cpp.
    fn build_corpus() -> Vec<u8> {
        let mut b = toc_bytes(&[
            (b"ObjectsList", 1),
            (b"Object", 2),
            (b"objectEnabled", 3),
            (b"waypointID", 4),
        ]);
        let mut inner = Vec::new();
        for i in 0..BENCH_OBJECTS {
            let name = format!("Obj{i:04}");
            let mut p = Vec::new();
            put_f32(&mut p, (i % 256) as f32 * 0.5);
            put_f32(&mut p, (i % 128) as f32 * 1.25);
            put_f32(&mut p, (i % 32) as f32 * 12.5);
            put_f32(&mut p, (i % 97) as f32 * 0.25 - 12.0);
            put_i32(&mut p, (i % 0x101) as i32);
            p.extend_from_slice(&(name.len() as u16).to_le_bytes());
            p.extend_from_slice(name.as_bytes());
            // the dict: objectEnabled BOOL + (every 16th) waypointID INT
            let waypoint = i % 16 == 0;
            let pairs = if waypoint { 2usize } else { 1 };
            p.extend_from_slice(&(pairs as u16).to_le_bytes());
            put_i32(
                &mut p,
                ((3u32 << 8) | u32::from(rzh_formats::map_chunk::dict_type::DICT_BOOL)) as i32,
            );
            p.push(u8::from(i & 1 != 0));
            if waypoint {
                put_i32(&mut p, ((4u32 << 8) | 1) as i32);
                put_i32(&mut p, i as i32);
            }
            put_chunk(&mut inner, 2, K_OBJECTS_VERSION_3, &p);
        }
        put_chunk(&mut b, 1, K_OBJECTS_VERSION_3, &inner);
        b
    }

    /// The final-list checksum (the same fields the C++ baseline sums).
    fn checksum(objects: &[MapObjectData]) -> u64 {
        let mut checksum = 0u64;
        for o in objects {
            checksum = checksum.wrapping_add(o.location.x.to_bits() as u64);
            checksum = checksum.wrapping_add(o.location.y.to_bits() as u64);
            checksum = checksum.wrapping_add(o.location.z.to_bits() as u64);
            checksum = checksum.wrapping_add(o.angle.to_bits() as u64);
            checksum = checksum.wrapping_add(o.flags as u32 as u64);
            for &c in &o.name {
                checksum = checksum.wrapping_add(c as u64);
            }
            checksum = checksum.wrapping_add(o.properties.pair_count() as u64);
            checksum = checksum.wrapping_add(u64::from(o.is_waypoint));
            checksum = checksum.wrapping_add(u64::from(o.is_light) * 2);
            checksum = checksum.wrapping_add(u64::from(o.is_scorch) * 4);
        }
        checksum
    }

    /// The objects load: the logical walk over the 4096-object corpus.
    pub fn bench_parse(c: &mut Criterion) {
        let bytes = build_corpus();
        let list = ObjectsListData::from_map(&bytes).expect("loads");
        let got = checksum(&list.objects);
        assert_eq!(
            got, CHECKSUM,
            "the objects corpus drifted from the C++ baseline"
        );
        let mut group = c.benchmark_group("mapobjects");
        group.throughput(Throughput::Bytes(bytes.len() as u64));
        group.bench_function("parse_x180k", |b| {
            b.iter(|| {
                let list = ObjectsListData::from_map(black_box(&bytes)).expect("loads");
                black_box(checksum(&list.objects))
            })
        });
        group.finish();
    }
}

// ---------------------------------------------------------------------------
// mapworldinfo: the map world-info payload (WorldHeightMap.cpp)
// ---------------------------------------------------------------------------

mod mapworldinfo_benches {
    use super::*;
    use rzh_formats::map_chunk::{DataChunkInfo, DataChunkInput, DictValue, NameKeys, ParserTable};
    use rzh_formats::map_worldinfo::{
        K_WORLDDICT_VERSION_1, WORLD_INFO_LABEL, parse_world_dict_data_chunk,
    };

    /// The bench corpus repetition: 2560 8-pair dicts ≈ 190 KiB (matches
    /// the C++ baseline).
    const BENCH_CHUNKS: usize = 2560;

    /// The corpus checksum the C++ baseline computes over the same
    /// fields (bench_wrap_mapworldinfo.cpp, verified at authoring time).
    const CHECKSUM: u64 = 2_764_705_683_974;

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
    fn toc_bytes(entries: &[(&[u8], u32)]) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(b"CkMp");
        b.extend_from_slice(&(entries.len() as i32).to_le_bytes());
        for (name, id) in entries {
            b.push(name.len() as u8);
            b.extend_from_slice(name);
            b.extend_from_slice(&id.to_le_bytes());
        }
        b
    }

    /// One WorldInfo chunk payload: the dict pair count and the 8
    /// formula pairs (the gen's `worldinfo_payload` shape).
    fn worldinfo_payload(i: usize) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&8u16.to_le_bytes());
        // weather INT
        put_i32(&mut p, ((2u32 << 8) | 1) as i32);
        put_i32(&mut p, (i % 3) as i32);
        // mapName ASCII
        let name = format!("Map{:04}", i);
        put_i32(&mut p, ((3u32 << 8) | 3) as i32);
        p.extend_from_slice(&(name.len() as u16).to_le_bytes());
        p.extend_from_slice(name.as_bytes());
        // flag BOOL
        put_i32(
            &mut p,
            ((4u32 << 8) | u32::from(rzh_formats::map_chunk::dict_type::DICT_BOOL)) as i32,
        );
        p.push(u8::from(i & 1 != 0));
        // gravity REAL
        put_i32(&mut p, ((5u32 << 8) | 2) as i32);
        put_f32(&mut p, (i % 97) as f32 * 0.25);
        // title UNICODE
        put_i32(&mut p, ((6u32 << 8) | 4) as i32);
        p.extend_from_slice(&2u16.to_le_bytes());
        p.extend_from_slice(&0x0048u16.to_le_bytes());
        p.extend_from_slice(&(0x61 + (i % 26) as u16).to_le_bytes());
        // sky INT
        put_i32(&mut p, ((7u32 << 8) | 1) as i32);
        put_i32(&mut p, (i % 100) as i32);
        // detail BOOL
        put_i32(
            &mut p,
            ((8u32 << 8) | u32::from(rzh_formats::map_chunk::dict_type::DICT_BOOL)) as i32,
        );
        p.push(u8::from(i & 2 != 0));
        // extra INT
        put_i32(&mut p, ((9u32 << 8) | 1) as i32);
        put_i32(&mut p, i as i32);
        p
    }

    /// Builds the corpus: 2560 WorldInfo chunks with 8-pair formula
    /// dicts — keep in lockstep with
    /// tools/differential/bench_wrap_mapworldinfo.cpp.
    fn build_corpus() -> Vec<u8> {
        let mut b = toc_bytes(&[
            (WORLD_INFO_LABEL, 1),
            (b"weather", 2),
            (b"mapName", 3),
            (b"flag", 4),
            (b"gravity", 5),
            (b"title", 6),
            (b"sky", 7),
            (b"detail", 8),
            (b"extra", 9),
        ]);
        for i in 0..BENCH_CHUNKS {
            let p = worldinfo_payload(i);
            put_chunk(&mut b, 1, K_WORLDDICT_VERSION_1, &p);
        }
        b
    }

    /// The walk's user state: the dict the last chunk left, the sticky
    /// weather and the accumulated per-chunk checksum (the same fields
    /// the C++ baseline sums).
    struct BenchWi {
        dict: Option<rzh_formats::map_chunk::Dict>,
        weather_set: bool,
        weather: i32,
        keys: NameKeys,
        checksum: u64,
    }

    impl BenchWi {
        fn new() -> Self {
            let mut keys = NameKeys::new();
            keys.name_to_key(b"weather");
            BenchWi {
                dict: None,
                weather_set: false,
                weather: 0,
                keys,
                checksum: 0,
            }
        }

        /// The per-chunk checksum over the world dict as the chunk left
        /// it (the gen's `addChunk`).
        fn add_chunk(&mut self, load: &rzh_formats::map_worldinfo::WorldDictLoad) {
            self.checksum = self.checksum.wrapping_add(load.dict.pair_count() as u64);
            for p in 0..load.dict.pair_count() {
                self.checksum = self
                    .checksum
                    .wrapping_add(load.dict.nth_key(p).ordinal() as u64);
                self.checksum = self
                    .checksum
                    .wrapping_add(load.dict.nth_type(p) as u32 as u64);
                match load.dict.nth_value(p) {
                    DictValue::Bool(v) => {
                        self.checksum = self.checksum.wrapping_add(u64::from(*v));
                    }
                    DictValue::Int(v) => {
                        self.checksum = self.checksum.wrapping_add(*v as u32 as u64);
                    }
                    DictValue::Real(v) => {
                        self.checksum = self.checksum.wrapping_add(v.to_bits() as u64);
                    }
                    DictValue::AsciiString(s) => {
                        self.checksum = self.checksum.wrapping_add(s.len() as u64);
                        for &c in s {
                            self.checksum = self.checksum.wrapping_add(c as u64);
                        }
                    }
                    DictValue::UnicodeString(u) => {
                        self.checksum = self.checksum.wrapping_add(u.len() as u64);
                        for &w in u {
                            self.checksum = self.checksum.wrapping_add(w as u64);
                        }
                    }
                }
            }
            if let Some(w) = load.weather {
                self.weather_set = true;
                self.weather = w;
                self.checksum = self.checksum.wrapping_add(w as u32 as u64);
            } else {
                self.checksum = self.checksum.wrapping_add(0x5a5a_5a5a);
            }
            self.dict = Some(load.dict.clone());
        }
    }

    fn bench_wi_parser(
        file: &mut DataChunkInput<'_>,
        _info: &DataChunkInfo,
        _table: &mut ParserTable<'_, BenchWi>,
        st: &mut BenchWi,
    ) -> bool {
        match parse_world_dict_data_chunk(file, &mut st.keys) {
            Ok(load) => {
                st.add_chunk(&load);
                true
            }
            Err(_) => false,
        }
    }

    fn run_walk(bytes: &[u8]) -> u64 {
        let mut input = DataChunkInput::new(bytes);
        let mut table: ParserTable<'_, BenchWi> = ParserTable::new();
        table.register(WORLD_INFO_LABEL, b"", bench_wi_parser);
        let mut st = BenchWi::new();
        if !input.parse(&mut table, &mut st) {
            return 0xdead_beef_u64;
        }
        st.checksum
    }

    /// The world-info load: the logical walk over the repeated-chunk
    /// corpus (every chunk parses; the last one wins).
    pub fn bench_parse(c: &mut Criterion) {
        let bytes = build_corpus();
        let got = run_walk(&bytes);
        assert_eq!(
            got, CHECKSUM,
            "the worldinfo corpus drifted from the C++ baseline"
        );
        let mut group = c.benchmark_group("mapworldinfo");
        group.throughput(Throughput::Bytes(bytes.len() as u64));
        group.bench_function("parse_x190k", |b| {
            b.iter(|| black_box(run_walk(black_box(&bytes))))
        });
        group.finish();
    }
}

// ---------------------------------------------------------------------------
// mappolygons: the map polygon-triggers payload (PolygonTrigger.cpp)
// ---------------------------------------------------------------------------

mod mappolygons_benches {
    use super::*;
    use rzh_formats::map_polygontriggers::{
        K_TRIGGERS_VERSION_4, POLYGON_TRIGGERS_LABEL, PolygonTriggersData,
    };

    /// The bench corpus size: 1536 8-point triggers ≈ 190 KiB (matches
    /// the C++ baseline).
    const BENCH_TRIGGERS: usize = 1536;

    /// The corpus checksum the C++ baseline computes over the same
    /// fields (bench_wrap_mappolygons.cpp, verified at authoring time).
    const CHECKSUM: u64 = 721_561_933_735;

    fn put_i32(b: &mut Vec<u8>, v: i32) {
        b.extend_from_slice(&v.to_le_bytes());
    }
    fn put_chunk(b: &mut Vec<u8>, id: u32, ver: u16, data: &[u8]) {
        b.extend_from_slice(&id.to_le_bytes());
        b.extend_from_slice(&ver.to_le_bytes());
        b.extend_from_slice(&(data.len() as i32).to_le_bytes());
        b.extend_from_slice(data);
    }
    fn toc_bytes(entries: &[(&[u8], u32)]) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(b"CkMp");
        b.extend_from_slice(&(entries.len() as i32).to_le_bytes());
        for (name, id) in entries {
            b.push(name.len() as u8);
            b.extend_from_slice(name);
            b.extend_from_slice(&id.to_le_bytes());
        }
        b
    }

    /// One trigger's flat record (the gen's `trigger_payload`, the
    /// version-4 shape).
    fn trigger_payload(i: usize) -> Vec<u8> {
        let mut p = Vec::new();
        let name = format!("Trig{:04}", i);
        let layer = format!("L{:02}", i % 50);
        p.extend_from_slice(&(name.len() as u16).to_le_bytes());
        p.extend_from_slice(name.as_bytes());
        p.extend_from_slice(&(layer.len() as u16).to_le_bytes());
        p.extend_from_slice(layer.as_bytes());
        put_i32(&mut p, i as i32 + 1); // the id
        p.push(u8::from(i & 1 != 0)); // water
        p.push(u8::from(i & 2 != 0)); // river
        put_i32(&mut p, (i % 32) as i32); // riverStart
        put_i32(&mut p, 8); // numPoints
        for j in 0..8i32 {
            put_i32(&mut p, (i % 512) as i32 + j);
            put_i32(&mut p, (i % 256) as i32 - j);
            put_i32(&mut p, (j * 7) % 100);
        }
        p
    }

    /// Builds the corpus: one version-4 PolygonTriggers chunk with 1536
    /// 8-point formula triggers — keep in lockstep with
    /// tools/differential/bench_wrap_mappolygons.cpp.
    fn build_corpus() -> Vec<u8> {
        let mut b = toc_bytes(&[(POLYGON_TRIGGERS_LABEL, 1)]);
        let mut payload = Vec::new();
        put_i32(&mut payload, BENCH_TRIGGERS as i32);
        for i in 0..BENCH_TRIGGERS {
            payload.extend_from_slice(&trigger_payload(i));
        }
        put_chunk(&mut b, 1, K_TRIGGERS_VERSION_4, &payload);
        b
    }

    /// The final-list checksum over the kept triggers' observable
    /// fields plus the id counter (the same fields the C++ baseline
    /// sums).
    fn checksum(data: &PolygonTriggersData) -> u64 {
        let mut checksum = 0u64;
        for t in &data.triggers {
            checksum = checksum.wrapping_add(t.id as u32 as u64);
            checksum = checksum.wrapping_add(u64::from(t.is_water));
            checksum = checksum.wrapping_add(u64::from(t.is_river) * 2);
            checksum = checksum.wrapping_add(t.river_start as u32 as u64);
            for &c in &t.name {
                checksum = checksum.wrapping_add(c as u64);
            }
            for &c in &t.layer {
                checksum = checksum.wrapping_add(c as u64);
            }
            for p in &t.points {
                checksum = checksum.wrapping_add(p.x as u32 as u64);
                checksum = checksum.wrapping_add(p.y as u32 as u64);
                checksum = checksum.wrapping_add(p.z as u32 as u64);
            }
        }
        checksum = checksum.wrapping_add(data.next_trigger_id as u32 as u64);
        checksum
    }

    /// The triggers load: the logical walk over the 1536-trigger corpus.
    pub fn bench_parse(c: &mut Criterion) {
        let bytes = build_corpus();
        let data = PolygonTriggersData::from_map(&bytes)
            .expect("loads")
            .expect("fired");
        let got = checksum(&data);
        assert_eq!(
            got, CHECKSUM,
            "the polygons corpus drifted from the C++ baseline"
        );
        let mut group = c.benchmark_group("mappolygons");
        group.throughput(Throughput::Bytes(bytes.len() as u64));
        group.bench_function("parse_x190k", |b| {
            b.iter(|| {
                let data = PolygonTriggersData::from_map(black_box(&bytes))
                    .expect("loads")
                    .expect("fired");
                black_box(checksum(&data))
            })
        });
        group.finish();
    }
}

// ---------------------------------------------------------------------------
// mapsides: the map sides-list payload (SidesList.cpp)
// ---------------------------------------------------------------------------

mod mapsides_benches {
    use super::*;
    use rzh_formats::map_chunk::{Dict, DictValue, NameKeys};
    use rzh_formats::map_sideslist::{
        K_SIDES_DATA_VERSION_3, PLAYER_ALLIES_KEY, PLAYER_DISPLAY_NAME_KEY, PLAYER_ENEMIES_KEY,
        PLAYER_FACTION_KEY, PLAYER_IS_HUMAN_KEY, PLAYER_NAME_KEY, SIDES_LIST_LABEL, SidesListData,
        TEAM_IS_SINGLETON_KEY, TEAM_NAME_KEY, TEAM_OWNER_KEY,
    };

    /// The bench corpus repetition: 264 chunks ≈ 190 KiB (matches the
    /// C++ baseline).
    const BENCH_CHUNKS: usize = 264;

    /// The corpus checksum the C++ baseline computes over the same
    /// fields (bench_wrap_mapsides.cpp, verified at authoring time).
    const CHECKSUM: u64 = 15_503_518_536;

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
    fn toc_bytes(entries: &[(&[u8], u32)]) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(b"CkMp");
        b.extend_from_slice(&(entries.len() as i32).to_le_bytes());
        for (name, id) in entries {
            b.push(name.len() as u8);
            b.extend_from_slice(name);
            b.extend_from_slice(&id.to_le_bytes());
        }
        b
    }

    // the standard corpus key ids (the gen's `SidesKeys`)
    const SK_PLAYER_NAME: u32 = 2;
    const SK_PLAYER_IS_HUMAN: u32 = 3;
    const SK_PLAYER_DISPLAY_NAME: u32 = 4;
    const SK_PLAYER_FACTION: u32 = 5;
    const SK_PLAYER_ENEMIES: u32 = 6;
    const SK_PLAYER_ALLIES: u32 = 7;
    const SK_TEAM_NAME: u32 = 8;
    const SK_TEAM_OWNER: u32 = 9;
    const SK_TEAM_IS_SINGLETON: u32 = 10;

    fn pair_ascii(key_id: u32, s: &str) -> Vec<u8> {
        let mut p = Vec::new();
        put_i32(&mut p, ((key_id << 8) | 3) as i32);
        p.extend_from_slice(&(s.len() as u16).to_le_bytes());
        p.extend_from_slice(s.as_bytes());
        p
    }
    fn pair_bool(key_id: u32, v: bool) -> Vec<u8> {
        let mut p = Vec::new();
        let kind = u32::from(rzh_formats::map_chunk::dict_type::DICT_BOOL);
        put_i32(&mut p, ((key_id << 8) | kind) as i32);
        p.push(u8::from(v));
        p
    }

    /// One build entry (the gen's `build_entry`, version-3 shape).
    #[allow(clippy::too_many_arguments)]
    fn build_entry(
        building: &str,
        tpl: &str,
        x: f32,
        y: f32,
        angle: f32,
        built: bool,
        rebuilds: i32,
        script: &str,
        health: i32,
        whiner: bool,
        unsellable: bool,
        repairable: bool,
    ) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&(building.len() as u16).to_le_bytes());
        p.extend_from_slice(building.as_bytes());
        p.extend_from_slice(&(tpl.len() as u16).to_le_bytes());
        p.extend_from_slice(tpl.as_bytes());
        put_f32(&mut p, x);
        put_f32(&mut p, y);
        put_f32(&mut p, 0.0); // z
        put_f32(&mut p, angle);
        p.push(u8::from(built));
        put_i32(&mut p, rebuilds);
        p.extend_from_slice(&(script.len() as u16).to_le_bytes());
        p.extend_from_slice(script.as_bytes());
        put_i32(&mut p, health);
        p.push(u8::from(whiner));
        p.push(u8::from(unsellable));
        p.push(u8::from(repairable));
        p
    }

    /// Builds the corpus: 264 SidesList chunks (the neutral, three
    /// players with two build entries each, five teams) — keep in
    /// lockstep with tools/differential/bench_wrap_mapsides.cpp.
    fn build_corpus() -> Vec<u8> {
        let mut b = toc_bytes(&[
            (SIDES_LIST_LABEL, 1),
            (PLAYER_NAME_KEY, SK_PLAYER_NAME),
            (PLAYER_IS_HUMAN_KEY, SK_PLAYER_IS_HUMAN),
            (PLAYER_DISPLAY_NAME_KEY, SK_PLAYER_DISPLAY_NAME),
            (PLAYER_FACTION_KEY, SK_PLAYER_FACTION),
            (PLAYER_ENEMIES_KEY, SK_PLAYER_ENEMIES),
            (PLAYER_ALLIES_KEY, SK_PLAYER_ALLIES),
            (TEAM_NAME_KEY, SK_TEAM_NAME),
            (TEAM_OWNER_KEY, SK_TEAM_OWNER),
            (TEAM_IS_SINGLETON_KEY, SK_TEAM_IS_SINGLETON),
            (b"playerMoney", 11),
        ]);
        for c in 0..BENCH_CHUNKS {
            let mut p = Vec::new();
            put_i32(&mut p, 4);
            {
                // the neutral
                let mut sp = Vec::new();
                sp.extend_from_slice(&1u16.to_le_bytes());
                sp.extend_from_slice(&pair_ascii(SK_PLAYER_NAME, ""));
                put_i32(&mut sp, 0); // build count
                p.extend_from_slice(&sp);
            }
            for s in 0..3usize {
                let name = format!("Plyr{}_{}", s, c % 7);
                let ally = format!("Plyr{}_{}", (s + 1) % 3, c % 7);
                let faction = format!("Faction{}", s % 3);
                let mut sp = Vec::new();
                sp.extend_from_slice(&5u16.to_le_bytes());
                sp.extend_from_slice(&pair_ascii(SK_PLAYER_NAME, &name));
                sp.extend_from_slice(&pair_bool(SK_PLAYER_IS_HUMAN, s == 0));
                sp.extend_from_slice(&pair_ascii(SK_PLAYER_FACTION, &faction));
                sp.extend_from_slice(&pair_ascii(SK_PLAYER_ALLIES, &ally));
                sp.extend_from_slice(&pair_ascii(SK_PLAYER_ENEMIES, &ally));
                put_i32(&mut sp, 2); // the build count
                for e in 0..2usize {
                    let building = format!("Bldg{}_{}", s, e);
                    let script = format!("Script{}", e);
                    sp.extend_from_slice(&build_entry(
                        &building,
                        &faction,
                        ((c * 4 + s * 2 + e) % 400) as f32,
                        (e * 100 + s) as f32,
                        e as f32 * 1.5,
                        e & 1 == 0,
                        (e * 3) as i32,
                        &script,
                        80 + (e * 5) as i32,
                        e == 0,
                        e != 1,
                        e != 2,
                    ));
                }
                p.extend_from_slice(&sp);
            }
            put_i32(&mut p, 5); // the team count
            for s in 0..3usize {
                let name = format!("teamPlyr{}_{}", s, c % 7);
                let owner = format!("Plyr{}_{}", s, c % 7);
                let mut tp = Vec::new();
                tp.extend_from_slice(&3u16.to_le_bytes());
                tp.extend_from_slice(&pair_ascii(SK_TEAM_NAME, &name));
                tp.extend_from_slice(&pair_ascii(SK_TEAM_OWNER, &owner));
                tp.extend_from_slice(&pair_bool(SK_TEAM_IS_SINGLETON, true));
                p.extend_from_slice(&tp);
            }
            for &(name, owner, singleton) in
                &[("AttackSquad", "Plyr0_0", false), ("team", "", true)]
            {
                let mut tp = Vec::new();
                tp.extend_from_slice(&3u16.to_le_bytes());
                tp.extend_from_slice(&pair_ascii(SK_TEAM_NAME, name));
                tp.extend_from_slice(&pair_ascii(SK_TEAM_OWNER, owner));
                tp.extend_from_slice(&pair_bool(SK_TEAM_IS_SINGLETON, singleton));
                p.extend_from_slice(&tp);
            }
            put_chunk(&mut b, 1, K_SIDES_DATA_VERSION_3, &p);
        }
        b
    }

    /// The final-state checksum over the payload's observable fields
    /// (the same fields the C++ baseline sums).
    fn value_word(value: &DictValue) -> u64 {
        match value {
            DictValue::Bool(v) => u64::from(*v),
            DictValue::Int(i) => *i as u32 as u64,
            DictValue::Real(r) => r.to_bits() as u64,
            DictValue::AsciiString(s) => s
                .iter()
                .fold(s.len() as u64, |acc, &c| acc.wrapping_add(c as u64)),
            DictValue::UnicodeString(u) => u
                .iter()
                .fold(u.len() as u64, |acc, &w| acc.wrapping_add(w as u64)),
        }
    }

    fn dict_checksum(dict: &Dict, keys: &NameKeys) -> u64 {
        let mut checksum = dict.pair_count() as u64;
        for p in 0..dict.pair_count() {
            checksum = checksum
                .wrapping_add(dict.nth_key(p).ordinal() as u64)
                .wrapping_add(dict.nth_type(p) as u32 as u64)
                .wrapping_add(value_word(dict.nth_value(p)));
        }
        let _ = keys;
        checksum
    }

    fn checksum(data: &SidesListData, keys: &NameKeys) -> u64 {
        let mut checksum =
            (data.sides.len() + data.teams.len() + usize::from(data.modified)) as u64;
        for side in &data.sides {
            checksum = checksum.wrapping_add(dict_checksum(&side.dict, keys));
            checksum = checksum.wrapping_add(side.build_list.len() as u64);
            for b in &side.build_list {
                checksum = checksum
                    .wrapping_add(
                        b.building_name
                            .iter()
                            .fold(b.building_name.len() as u64, |acc, &c| {
                                acc.wrapping_add(c as u64)
                            }),
                    )
                    .wrapping_add(
                        b.template_name
                            .iter()
                            .fold(b.template_name.len() as u64, |acc, &c| {
                                acc.wrapping_add(c as u64)
                            }),
                    )
                    .wrapping_add(b.location.x.to_bits() as u64)
                    .wrapping_add(b.location.y.to_bits() as u64)
                    .wrapping_add(b.location.z.to_bits() as u64)
                    .wrapping_add(b.angle.to_bits() as u64)
                    .wrapping_add(u64::from(b.initially_built))
                    .wrapping_add(b.num_rebuilds as i32 as u32 as u64)
                    .wrapping_add(b.health as u32 as u64)
                    .wrapping_add(u64::from(b.whiner))
                    .wrapping_add(u64::from(b.unsellable) * 2)
                    .wrapping_add(u64::from(b.repairable) * 4)
                    .wrapping_add(
                        b.script
                            .iter()
                            .fold(b.script.len() as u64, |acc, &c| acc.wrapping_add(c as u64)),
                    );
            }
        }
        for team in &data.teams {
            checksum = checksum.wrapping_add(dict_checksum(team, keys));
        }
        checksum
    }

    /// The sides load: the logical walk over the repeated-chunk corpus
    /// (every chunk parses and validates; the last one wins).
    pub fn bench_parse(c: &mut Criterion) {
        let bytes = build_corpus();
        let data = SidesListData::from_map(&bytes)
            .expect("loads")
            .expect("fired");
        // the well-known keys seeded the walk's way, for the checksum's
        // name-key ordinals
        let mut keys = NameKeys::new();
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
        let got = checksum(&data, &keys);
        assert_eq!(
            got, CHECKSUM,
            "the sides corpus drifted from the C++ baseline"
        );
        let mut group = c.benchmark_group("mapsides");
        group.throughput(Throughput::Bytes(bytes.len() as u64));
        group.bench_function("parse_x190k", |b| {
            b.iter(|| {
                let data = SidesListData::from_map(black_box(&bytes))
                    .expect("loads")
                    .expect("fired");
                black_box(checksum(&data, &keys))
            })
        });
        group.finish();
    }
}

// ---------------------------------------------------------------------------
// mapscripts: the script-engine load path (Scripts.cpp)
// ---------------------------------------------------------------------------

mod mapscripts_benches {
    use super::*;
    use rzh_formats::map_scripts::{
        AT_DEBUG_STRING, AT_MOVE_CAMERA_TO, AT_SET_FLAG, AT_SET_TREE_SWAY, CT_COUNTER,
        CT_SKIRMISH_SPECIAL_POWER_READY, CT_TIMER_EXPIRED, CT_UNIT_HEALTH,
        K_SCRIPTS_DATA_VERSION_1, PT_COMPARISON, PT_COORD3D, PT_COUNTER, PT_FLAG, PT_INT,
        PT_KIND_OF_PARAM, PT_OBJECT_STATUS, PT_REAL, PT_SPECIAL_POWER, PT_TEXT_STRING, PT_UNIT,
        ParameterData, PlayerScriptsData, ScriptData,
    };

    /// The bench corpus repetition: 70 PlayerScriptsList chunks
    /// ≈ 190 KiB (matches the C++ baseline).
    const BENCH_PSL: usize = 70;

    /// The corpus checksum the C++ baseline computes over the same
    /// fields (bench_wrap_mapscripts.cpp, verified at authoring time).
    const CHECKSUM: u64 = 1950;

    fn put_i32(b: &mut Vec<u8>, v: i32) {
        b.extend_from_slice(&v.to_le_bytes());
    }
    fn put_f32(b: &mut Vec<u8>, v: f32) {
        b.extend_from_slice(&v.to_le_bytes());
    }
    fn put_str(b: &mut Vec<u8>, s: &str) {
        b.extend_from_slice(&(s.len() as u16).to_le_bytes());
        b.extend_from_slice(s.as_bytes());
    }
    fn put_chunk(b: &mut Vec<u8>, id: u32, ver: u16, data: &[u8]) {
        b.extend_from_slice(&id.to_le_bytes());
        b.extend_from_slice(&ver.to_le_bytes());
        b.extend_from_slice(&(data.len() as i32).to_le_bytes());
        b.extend_from_slice(data);
    }
    /// The corpus TOC: labels interned to ids in first-use order (the
    /// generator's `TocB`).
    struct TocB {
        entries: Vec<(&'static [u8], u32)>,
    }
    impl TocB {
        fn new() -> Self {
            TocB {
                entries: Vec::new(),
            }
        }
        fn id(&mut self, label: &'static [u8]) -> u32 {
            if let Some(&(_, id)) = self.entries.iter().find(|(l, _)| *l == label) {
                return id;
            }
            let id = self.entries.len() as u32 + 1;
            self.entries.push((label, id));
            id
        }
        fn toc_bytes(&self) -> Vec<u8> {
            let mut b = Vec::new();
            b.extend_from_slice(b"CkMp");
            b.extend_from_slice(&(self.entries.len() as i32).to_le_bytes());
            for (name, id) in &self.entries {
                b.push(name.len() as u8);
                b.extend_from_slice(name);
                b.extend_from_slice(&id.to_le_bytes());
            }
            b
        }
    }
    fn put_parm(b: &mut Vec<u8>, ty: i32, iv: i32, rv: f32, sv: &str) {
        put_i32(b, ty);
        put_i32(b, iv);
        put_f32(b, rv);
        put_str(b, sv);
    }
    fn put_parm_coord(b: &mut Vec<u8>, x: f32, y: f32, z: f32) {
        put_i32(b, PT_COORD3D);
        put_f32(b, x);
        put_f32(b, y);
        put_f32(b, z);
    }
    fn put_cond_head(b: &mut Vec<u8>, ty: i32, key_id: u32, nparms: i32) {
        put_i32(b, ty);
        if key_id > 0 {
            put_i32(b, ((key_id << 8) | 3) as i32);
        }
        put_i32(b, nparms);
    }
    fn put_key_word(b: &mut Vec<u8>, word: i32) {
        put_i32(b, word);
    }
    fn put_script_head(
        b: &mut Vec<u8>,
        name: &str,
        comment: &str,
        active: u8,
        oneshot: u8,
        sub: u8,
        delay: Option<i32>,
    ) {
        put_str(b, name);
        put_str(b, comment);
        put_str(b, "");
        put_str(b, "");
        b.push(active);
        b.push(oneshot);
        b.push(1);
        b.push(1);
        b.push(1);
        b.push(sub);
        if let Some(d) = delay {
            put_i32(b, d);
        }
    }

    /// Builds the corpus: 70 PlayerScriptsList chunks, each with three
    /// script lists — keep in lockstep with
    /// tools/differential/bench_wrap_mapscripts.cpp.
    fn build_corpus() -> Vec<u8> {
        let mut toc = TocB::new();
        let mut chunks = Vec::new();

        for c in 0..BENCH_PSL {
            let mut psl = Vec::new();

            // list A: four flat scripts
            let mut list_a = Vec::new();
            for k in 0..4usize {
                let name = format!("Scr{c}_{k}");
                let comment = format!("Comment{k}");
                let delay = (c * 4 + k) as i32;

                // two or-clauses, each with three conditions
                let mut ors = Vec::new();
                for o in 0..2usize {
                    let mut conds = Vec::new();
                    // a straight match
                    {
                        let mut cond = Vec::new();
                        put_cond_head(&mut cond, CT_TIMER_EXPIRED, toc.id(b"TIMER_EXPIRED"), 1);
                        put_parm(&mut cond, PT_INT, (c * 10 + k) as i32, 0.0, "");
                        put_chunk(
                            &mut conds,
                            toc.id(b"Condition"),
                            rzh_formats::map_scripts::K_SCRIPT_CONDITION_VERSION_4,
                            &cond,
                        );
                    }
                    // a rematch by name (UNIT_HEALTH keyed FLAG)
                    {
                        let mut cond = Vec::new();
                        put_cond_head(&mut cond, CT_UNIT_HEALTH, toc.id(b"FLAG"), 2);
                        put_parm(&mut cond, PT_UNIT, 0, 0.0, &format!("Unit{c}_{k}"));
                        put_parm(&mut cond, PT_COMPARISON, ((k + o) % 6) as i32, 0.0, "");
                        put_chunk(
                            &mut conds,
                            toc.id(b"Condition"),
                            rzh_formats::map_scripts::K_SCRIPT_CONDITION_VERSION_4,
                            &cond,
                        );
                    }
                    // a count mismatch (COUNTER with 2 parms)
                    {
                        let mut cond = Vec::new();
                        put_cond_head(&mut cond, CT_COUNTER, 0, 2);
                        put_parm(&mut cond, PT_COUNTER, 0, 0.0, "Cnt");
                        put_parm(&mut cond, PT_INT, 5, 0.0, "");
                        put_chunk(
                            &mut conds,
                            toc.id(b"Condition"),
                            rzh_formats::map_scripts::K_SCRIPT_CONDITION_VERSION_4,
                            &cond,
                        );
                    }
                    put_chunk(
                        &mut ors,
                        toc.id(b"OrCondition"),
                        rzh_formats::map_scripts::K_SCRIPT_OR_CONDITION_DATA_VERSION_1,
                        &conds,
                    );
                }

                // the THEN list: three actions
                let mut acts = Vec::new();
                {
                    let mut a = Vec::new();
                    put_i32(&mut a, AT_SET_FLAG);
                    put_key_word(&mut a, ((toc.id(b"SET_FLAG") << 8) | 3) as i32);
                    put_i32(&mut a, 2);
                    put_parm(&mut a, PT_FLAG, 0, 0.0, &format!("Flag{c}_{k}"));
                    put_parm(
                        &mut a,
                        rzh_formats::map_scripts::PT_BOOLEAN,
                        (k & 1) as i32,
                        0.0,
                        "",
                    );
                    put_chunk(
                        &mut acts,
                        toc.id(b"ScriptAction"),
                        rzh_formats::map_scripts::K_SCRIPT_ACTION_VERSION_2,
                        &a,
                    );
                }
                {
                    // a rematch that lands on a mismatched count → NO_OP
                    let mut a = Vec::new();
                    put_i32(&mut a, AT_MOVE_CAMERA_TO);
                    put_key_word(&mut a, ((toc.id(b"SET_COUNTER") << 8) | 3) as i32);
                    put_i32(&mut a, 2);
                    put_parm(&mut a, PT_COUNTER, 0, 0.0, "Cnt");
                    put_parm(&mut a, PT_INT, 9, 0.0, "");
                    put_chunk(
                        &mut acts,
                        toc.id(b"ScriptAction"),
                        rzh_formats::map_scripts::K_SCRIPT_ACTION_VERSION_2,
                        &a,
                    );
                }
                {
                    // five parms (SET_TREE_SWAY): the kindof and
                    // object-status rewrites exercised
                    let mut a = Vec::new();
                    put_i32(&mut a, AT_SET_TREE_SWAY);
                    put_key_word(&mut a, ((toc.id(b"SET_TREE_SWAY") << 8) | 3) as i32);
                    put_i32(&mut a, 5);
                    put_parm_coord(&mut a, (c + k) as f32, (k * 2) as f32, 0.0);
                    put_parm(
                        &mut a,
                        PT_KIND_OF_PARAM,
                        0,
                        0.0,
                        if k & 1 == 1 { "OBSTACLE" } else { "PARACHUTE" },
                    );
                    put_parm(
                        &mut a,
                        PT_OBJECT_STATUS,
                        0,
                        0.0,
                        if k & 1 == 1 { "DESTROYED" } else { "NONE" },
                    );
                    put_parm(&mut a, PT_INT, c as i32, 0.0, "");
                    put_parm(&mut a, PT_REAL, 0, (k % 5) as f32 * 0.5, "");
                    put_chunk(
                        &mut acts,
                        toc.id(b"ScriptAction"),
                        rzh_formats::map_scripts::K_SCRIPT_ACTION_VERSION_2,
                        &a,
                    );
                }
                // the ELSE list: one action
                let mut fact = Vec::new();
                {
                    let mut a = Vec::new();
                    put_i32(&mut a, AT_DEBUG_STRING);
                    put_key_word(&mut a, ((toc.id(b"DEBUG_STRING") << 8) | 3) as i32);
                    put_i32(&mut a, 1);
                    put_parm(&mut a, PT_TEXT_STRING, 0, 0.0, &format!("Else{c}_{k}"));
                    put_chunk(
                        &mut fact,
                        toc.id(b"ScriptActionFalse"),
                        rzh_formats::map_scripts::K_SCRIPT_ACTION_VERSION_2,
                        &a,
                    );
                }

                let mut s = Vec::new();
                put_script_head(&mut s, &name, &comment, (k & 1) as u8, 1, 0, Some(delay));
                chunks_extend(&mut s, &ors);
                chunks_extend(&mut s, &acts);
                chunks_extend(&mut s, &fact);
                put_chunk(
                    &mut list_a,
                    toc.id(b"Script"),
                    rzh_formats::map_scripts::K_SCRIPT_DATA_VERSION_2,
                    &s,
                );
            }

            // list B: two scripts and a two-script group
            let mut list_b = Vec::new();
            for k in 0..2usize {
                let mut s = Vec::new();
                put_script_head(&mut s, &format!("BS{c}_{k}"), "", 1, 1, k as u8, None);
                put_chunk(
                    &mut list_b,
                    toc.id(b"Script"),
                    rzh_formats::map_scripts::K_SCRIPT_DATA_VERSION_1,
                    &s,
                );
            }
            {
                let mut grp = Vec::new();
                put_str(&mut grp, &format!("Grp{c}"));
                grp.push(1);
                grp.push((c & 1) as u8);
                for k in 0..2usize {
                    let mut s = Vec::new();
                    put_script_head(&mut s, &format!("GS{c}_{k}"), "", 1, 0, 0, None);
                    put_chunk(
                        &mut grp,
                        toc.id(b"Script"),
                        rzh_formats::map_scripts::K_SCRIPT_DATA_VERSION_1,
                        &s,
                    );
                }
                put_chunk(
                    &mut list_b,
                    toc.id(b"ScriptGroup"),
                    rzh_formats::map_scripts::K_SCRIPT_GROUP_DATA_VERSION_2,
                    &grp,
                );
            }

            // list C: one special-power script whose heal fires
            let mut list_c = Vec::new();
            {
                let mut cond = Vec::new();
                put_cond_head(&mut cond, CT_SKIRMISH_SPECIAL_POWER_READY, 0, 1);
                put_parm(&mut cond, PT_SPECIAL_POWER, 0, 0.0, "SuperWeaponNuke");
                let mut conds = Vec::new();
                put_chunk(
                    &mut conds,
                    toc.id(b"Condition"),
                    rzh_formats::map_scripts::K_SCRIPT_CONDITION_VERSION_2,
                    &cond,
                );
                let mut orc = Vec::new();
                chunks_extend(&mut orc, &conds);
                let mut orcch = Vec::new();
                put_chunk(
                    &mut orcch,
                    toc.id(b"OrCondition"),
                    rzh_formats::map_scripts::K_SCRIPT_OR_CONDITION_DATA_VERSION_1,
                    &orc,
                );
                let mut s = Vec::new();
                put_script_head(&mut s, "SP", "", 1, 1, 0, None);
                chunks_extend(&mut s, &orcch);
                put_chunk(
                    &mut list_c,
                    toc.id(b"Script"),
                    rzh_formats::map_scripts::K_SCRIPT_DATA_VERSION_1,
                    &s,
                );
            }

            put_chunk(
                &mut psl,
                toc.id(b"ScriptList"),
                rzh_formats::map_scripts::K_SCRIPT_LIST_DATA_VERSION_1,
                &list_a,
            );
            put_chunk(
                &mut psl,
                toc.id(b"ScriptList"),
                rzh_formats::map_scripts::K_SCRIPT_LIST_DATA_VERSION_1,
                &list_b,
            );
            put_chunk(
                &mut psl,
                toc.id(b"ScriptList"),
                rzh_formats::map_scripts::K_SCRIPT_LIST_DATA_VERSION_1,
                &list_c,
            );
            put_chunk(
                &mut chunks,
                toc.id(b"PlayerScriptsList"),
                K_SCRIPTS_DATA_VERSION_1,
                &psl,
            );
        }

        let mut file = toc.toc_bytes();
        file.extend_from_slice(&chunks);
        file
    }

    /// (a tiny helper: the C++ put_bytes)
    fn chunks_extend(dst: &mut Vec<u8>, src: &[u8]) {
        dst.extend_from_slice(src);
    }

    /// The final-state checksum over the payload's observable fields
    /// (the same fields the C++ baseline sums).
    fn parm_checksum(p: &ParameterData) -> u64 {
        let mut sum = (p.param_type as u32 as u64)
            .wrapping_add(p.int_value as u32 as u64)
            .wrapping_add(p.real_value.to_bits() as u64)
            .wrapping_add(p.string_value.len() as u64);
        for &c in &p.string_value {
            sum = sum.wrapping_add(c as u64);
        }
        sum = sum
            .wrapping_add(p.coord.x.to_bits() as u64)
            .wrapping_add(p.coord.y.to_bits() as u64)
            .wrapping_add(p.coord.z.to_bits() as u64)
            .wrapping_add(p.object_status);
        sum
    }

    fn parm_list_checksum(params: &[Option<ParameterData>]) -> u64 {
        let mut sum = params.len() as u64;
        for p in params {
            sum = sum.wrapping_add(match p {
                Some(p) => parm_checksum(p),
                None => 0x9999,
            });
        }
        sum
    }

    fn script_checksum(s: &ScriptData) -> u64 {
        let mut sum =
            (s.name.len() + s.comment.len() + s.condition_comment.len() + s.action_comment.len())
                as u64;
        for str in [&s.name, &s.comment, &s.condition_comment, &s.action_comment] {
            for &c in str {
                sum = sum.wrapping_add(c as u64);
            }
        }
        sum = sum
            .wrapping_add(u64::from(s.is_active))
            .wrapping_add(u64::from(s.is_one_shot) * 2)
            .wrapping_add(u64::from(s.easy) * 4)
            .wrapping_add(u64::from(s.normal) * 8)
            .wrapping_add(u64::from(s.hard) * 16)
            .wrapping_add(u64::from(s.is_subroutine) * 32)
            .wrapping_add(s.delay_evaluation_seconds as u32 as u64);
        for or in &s.or_conditions {
            sum = sum.wrapping_add(or.and_conditions.len() as u64);
            for cond in &or.and_conditions {
                sum = sum.wrapping_add(cond.condition_type as u32 as u64);
                sum = sum.wrapping_add(parm_list_checksum(&cond.params));
            }
        }
        for chain in [&s.actions, &s.false_actions] {
            for act in chain {
                sum = sum.wrapping_add(act.action_type as u32 as u64);
                sum = sum.wrapping_add(parm_list_checksum(&act.params));
            }
        }
        sum
    }

    fn checksum(data: &PlayerScriptsData) -> u64 {
        let mut checksum = data.lists.len() as u64;
        for list in &data.lists {
            checksum = checksum
                .wrapping_add(list.scripts.len() as u64)
                .wrapping_add(list.groups.len() as u64);
            for script in &list.scripts {
                checksum = checksum.wrapping_add(script_checksum(script));
            }
            for group in &list.groups {
                checksum = checksum.wrapping_add(group.name.len() as u64);
                for &c in &group.name {
                    checksum = checksum.wrapping_add(c as u64);
                }
                checksum = checksum
                    .wrapping_add(u64::from(group.is_active))
                    .wrapping_add(u64::from(group.is_subroutine) * 2)
                    .wrapping_add(group.scripts.len() as u64);
                for script in &group.scripts {
                    checksum = checksum.wrapping_add(script_checksum(script));
                }
            }
        }
        checksum
    }

    /// The scripts load: the root walk over the repeated-chunk corpus
    /// (every chunk parses — the statics' clear makes the last one
    /// win).
    pub fn bench_parse(c: &mut Criterion) {
        let bytes = build_corpus();
        let data = PlayerScriptsData::from_map(&bytes)
            .expect("loads")
            .expect("fired");
        let got = checksum(&data);
        assert_eq!(
            got, CHECKSUM,
            "the scripts corpus drifted from the C++ baseline"
        );
        let mut group = c.benchmark_group("mapscripts");
        group.throughput(Throughput::Bytes(bytes.len() as u64));
        group.bench_function("parse_x190k", |b| {
            b.iter(|| {
                let data = PlayerScriptsData::from_map(black_box(&bytes))
                    .expect("loads")
                    .expect("fired");
                black_box(checksum(&data))
            })
        });
        group.finish();
    }
}

// ---------------------------------------------------------------------------
// wnd: the window layouts (GameWindowManagerScript.cpp)
// ---------------------------------------------------------------------------

mod wnd_benches {
    use super::*;

    /// The bench corpus repetition: 12 two-window blocks ≈ 187 KiB
    /// (matches the C++ baseline).
    const BENCH_WINDOWS: usize = 12;

    /// The corpus checksum the C++ baseline computes over the same
    /// fields (bench_wrap_wnd.cpp, verified at authoring time).
    const CHECKSUM: u64 = 4_917_322_059_290_964_206;

    /// One nine-line draw-data stanza with formula values (the same
    /// shape GUIEdit writes).
    fn stanza(out: &mut String, token: &str, k: u32) {
        let pad = " ".repeat(token.len());
        for i in 0..9u32 {
            let iname = format!("BImg{}", (i + k) % 7);
            let use_name = if i % 3 == 0 { "NoImage" } else { &iname };
            let rr = (k * 13 + i * 17) % 300;
            let gg = (k * 7 + i * 11) % 300;
            let bb = (k * 29 + i * 3) % 300;
            let a = 200 + ((k + i) % 56);
            let br = (k * 3 + i * 23) % 300;
            let bg = (k * 11 + i * 5) % 300;
            let bb2 = (k * 19 + i * 13) % 300;
            let ba = 150 + ((k * 2 + i) % 106);
            let tail = if i == 8 { ";" } else { "," };
            if i == 0 {
                out.push_str(&format!(
                    "{token} = IMAGE: {use_name}, COLOR: {rr} {gg} {bb} {a}, BORDERCOLOR: {br} {bg} {bb2} {ba}{tail}\n"
                ));
            } else {
                out.push_str(&format!(
                    "             {pad}   IMAGE: {use_name}, COLOR: {rr} {gg} {bb} {a}, BORDERCOLOR: {br} {bg} {bb2} {ba}{tail}\n"
                ));
            }
        }
    }

    /// Builds the corpus: 12 two-window blocks (a full-field USER parent
    /// with a listbox child carrying the composite stanzas) — keep in
    /// lockstep with tools/differential/bench_wrap_wnd.cpp.
    fn build_corpus() -> Vec<u8> {
        let mut t = String::new();
        t.push_str("FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\n  LAYOUTINIT = BenchInit;\n  LAYOUTUPDATE = BenchUpdate;\nENDLAYOUTBLOCK\n");
        for i in 0..BENCH_WINDOWS as u32 {
            let x = (i * 7) % 400;
            let y = (i * 11) % 300;
            t.push_str("WINDOW\nWINDOWTYPE = USER;\n");
            t.push_str(&format!(
                "SCREENRECT = UPPERLEFT: {x} {y},\n             BOTTOMRIGHT: {} {},\n             CREATIONRESOLUTION: 800 600;\n",
                x + 120,
                y + 80
            ));
            t.push_str(&format!("NAME = \"Bench.wnd:Win{i}\";\n"));
            t.push_str("STATUS = ENABLED+DRAGABLE+SEETHRU;\n");
            t.push_str("STYLE = MOUSETRACK+ANIMATED;\n");
            t.push_str(&format!(
                "SYSTEMCALLBACK = \"SysCB{}\";\nINPUTCALLBACK = \"InCB{}\";\n",
                i % 4,
                i % 3
            ));
            t.push_str(&format!(
                "TOOLTIPCALLBACK = \"TipCB{}\";\nDRAWCALLBACK = \"DrawCB{}\";\n",
                i % 5,
                i % 2
            ));
            t.push_str("FONT = NAME: \"Airborne\", SIZE: 12, BOLD: 0;\n");
            t.push_str("HEADERTEMPLATE = \"BenchHeader\";\n");
            t.push_str(&format!(
                "TOOLTIPTEXT = \"Tip:{i}\";\nTOOLTIPDELAY = {} ;\n",
                i % 30
            ));
            t.push_str(&format!("TEXT = \"GUI:Item{i}\";\n"));
            t.push_str("IMAGEOFFSET = 3 7;\n");
            // the three-state text colors
            let (er, eg, eb) = ((i * 3) % 256, (i * 5) % 256, (i * 7) % 256);
            let (dr, dg, db) = ((i * 11) % 256, (i * 13) % 256, (i * 17) % 256);
            let (hr, hg, hb) = ((i * 19) % 256, (i * 23) % 256, (i * 29) % 256);
            t.push_str(&format!(
                "TEXTCOLOR = ENABLED:  {er} {eg} {eb} 255, ENABLEDBORDER:  0 0 0 255,\n            DISABLED: {dr} {dg} {db} 255, DISABLEDBORDER: 0 0 0 255,\n            HILITE:   {hr} {hg} {hb} 255, HILITEBORDER:   64 64 64 255;\n"
            ));
            stanza(&mut t, "ENABLEDDRAWDATA", i);
            stanza(&mut t, "DISABLEDDRAWDATA", i + 1);
            stanza(&mut t, "HILITEDRAWDATA", i + 2);
            // the listbox child with its data and composite stanzas
            t.push_str("CHILD\n");
            t.push_str("WINDOW\nWINDOWTYPE = SCROLLLISTBOX;\n");
            t.push_str(&format!(
                "SCREENRECT = UPPERLEFT: {} {},\n             BOTTOMRIGHT: {} {},\n             CREATIONRESOLUTION: 800 600;\n",
                x + 5,
                y + 5,
                x + 115,
                y + 75
            ));
            t.push_str(&format!("NAME = \"Bench.wnd:List{i}\";\n"));
            t.push_str(&format!(
                "LISTBOXDATA = LENGTH: {},\n              AUTOSCROLL: {},\n              AUTOPURGE: {},\n              SCROLLBAR: 1,\n              MULTISELECT: {},\n              COLUMNS: 1,\n              FORCESELECT: {};\n",
                i % 20,
                i % 2,
                (i + 1) % 2,
                u32::from(i % 3 == 0),
                u32::from(i % 5 == 0)
            ));
            stanza(&mut t, "LISTBOXENABLEDUPBUTTONDRAWDATA", i + 3);
            stanza(&mut t, "LISTBOXDISABLEDUPBUTTONDRAWDATA", i + 4);
            stanza(&mut t, "LISTBOXHILITEUPBUTTONDRAWDATA", i + 5);
            stanza(&mut t, "LISTBOXENABLEDDOWNBUTTONDRAWDATA", i + 6);
            stanza(&mut t, "LISTBOXDISABLEDDOWNBUTTONDRAWDATA", i + 7);
            stanza(&mut t, "LISTBOXHILITEDOWNBUTTONDRAWDATA", i + 8);
            stanza(&mut t, "LISTBOXENABLEDSLIDERDRAWDATA", i + 9);
            stanza(&mut t, "LISTBOXDISABLEDSLIDERDRAWDATA", i + 10);
            stanza(&mut t, "LISTBOXHILITESLIDERDRAWDATA", i + 11);
            stanza(&mut t, "SLIDERTHUMBENABLEDDRAWDATA", i + 12);
            stanza(&mut t, "SLIDERTHUMBDISABLEDDRAWDATA", i + 13);
            stanza(&mut t, "SLIDERTHUMBHILITEDRAWDATA", i + 14);
            t.push_str("END\nENDALLCHILDREN\nEND\n");
        }
        t.into_bytes()
    }

    /// The final-state checksum over the parsed layout's observable
    /// fields (the same walk the C++ baseline drives).
    fn checksum(layout: &rzh_formats::wnd::WndLayout) -> u64 {
        // the signed words zero-extend the way the C++ baseline's
        // (uint64_t)(uint32_t) casts do
        let z = |v: i32| u64::from(v as u32);
        let mut sum: u64 = layout.roots.len() as u64;
        for w in &layout.arena {
            sum = sum
                .wrapping_mul(31)
                .wrapping_add(z(w.id))
                .wrapping_mul(31)
                .wrapping_add(z(w.x))
                .wrapping_mul(31)
                .wrapping_add(z(w.y))
                .wrapping_mul(31)
                .wrapping_add(z(w.width))
                .wrapping_mul(31)
                .wrapping_add(z(w.height))
                .wrapping_mul(31)
                .wrapping_add(u64::from(w.status))
                .wrapping_mul(31)
                .wrapping_add(u64::from(w.style))
                .wrapping_mul(31)
                .wrapping_add(u64::from(w.enabled_draw_data[0].color))
                .wrapping_mul(31)
                .wrapping_add(u64::from(w.text_colors.enabled.0))
                .wrapping_mul(31)
                .wrapping_add(z(w.tooltip_delay));
        }
        sum = sum
            .wrapping_mul(31)
            .wrapping_add(u64::from(layout.defaults.text_color));
        sum
    }

    pub(super) fn bench_parse(c: &mut Criterion) {
        let bytes = build_corpus();
        let display = WndDisplay::new(800, 600);
        let layout = parse_wnd(black_box(&bytes), display);
        assert_eq!(layout.verdict.word(), "ok");
        assert_eq!(layout.arena.len(), BENCH_WINDOWS * 2);
        let got = checksum(&layout);
        assert_eq!(
            got, CHECKSUM,
            "the wnd corpus drifted from the C++ baseline"
        );
        let mut group = c.benchmark_group("wnd");
        group.throughput(Throughput::Bytes(bytes.len() as u64));
        group.bench_function("parse_x187k", |b| {
            b.iter(|| checksum(&parse_wnd(black_box(&bytes), display)))
        });
        group.finish();
    }
}

// ---------------------------------------------------------------------------
// font: the Targa font strikes (font3d.cpp)
// ---------------------------------------------------------------------------

mod font_benches {
    use super::*;

    /// The bench corpus: a 1024x64 16-bit strike with 64 twelve-column
    /// glyphs at a 16-column pitch — 131,090 bytes (matches the C++
    /// baseline).
    const BENCH_WIDTH: usize = 1024;
    const BENCH_HEIGHT: usize = 64;
    const BENCH_GLYPHS: usize = 64;

    /// The final-state checksum the C++ baseline computes over the same
    /// fields (bench_wrap_font.cpp, verified at authoring time).
    const CHECKSUM: u64 = 3_631_483_982_267_290_816;

    /// Builds the corpus — keep in lockstep with
    /// tools/differential/bench_wrap_font.cpp.
    fn build_corpus() -> Vec<u8> {
        let mut px = vec![0u16; BENCH_WIDTH * BENCH_HEIGHT];
        for c in 0..BENCH_GLYPHS {
            for y in 0..BENCH_HEIGHT {
                for x in 0..12 {
                    px[y * BENCH_WIDTH + c * 16 + x] = 0x8000
                        | ((c & 31) as u16) << 10
                        | (((c * 3) & 31) as u16) << 5
                        | ((c * 7) & 31) as u16;
                }
            }
        }
        let mut v = vec![0u8, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        v.extend_from_slice(&(BENCH_WIDTH as u16).to_le_bytes());
        v.extend_from_slice(&(BENCH_HEIGHT as u16).to_le_bytes());
        v.push(16);
        v.push(0x20);
        for p in px {
            v.extend_from_slice(&p.to_le_bytes());
        }
        v
    }

    /// The final-state checksum over the parsed font's observable fields
    /// (the same walk the C++ baseline drives).
    fn checksum(font: &rzh_formats::font::Font3DData) -> u64 {
        let mut sum: u64 = 0;
        for c in 0..256 {
            sum = sum
                .wrapping_mul(31)
                .wrapping_add(u64::from(font.u_offset[c].to_bits()));
            sum = sum
                .wrapping_mul(31)
                .wrapping_add(u64::from(font.v_offset[c].to_bits()));
            sum = sum
                .wrapping_mul(31)
                .wrapping_add(u64::from(font.u_width[c].to_bits()));
            sum = sum
                .wrapping_mul(31)
                .wrapping_add(u64::from(font.char_width[c]));
        }
        sum = sum
            .wrapping_mul(31)
            .wrapping_add(u64::from(font.v_height.to_bits()));
        sum = sum
            .wrapping_mul(31)
            .wrapping_add(u64::from(font.char_height));
        if let Some(packed) = &font.packed {
            sum = sum.wrapping_mul(31).wrapping_add(packed.width as u64);
            sum = sum.wrapping_mul(31).wrapping_add(packed.height as u64);
            let mut s: u64 = 0;
            for &b in &packed.bytes {
                s = s.wrapping_mul(31).wrapping_add(b as u64);
            }
            sum = sum.wrapping_mul(31).wrapping_add(s);
        }
        sum
    }

    pub(super) fn bench_parse(c: &mut Criterion) {
        let bytes = build_corpus();
        assert_eq!(bytes.len(), 131_090, "the font corpus size drifted");
        let font = parse_font(b"Bench.tga", Some(black_box(&bytes)));
        assert_eq!(font.verdict, rzh_formats::font::FontVerdict::Ok);
        let got = checksum(&font);
        assert_eq!(
            got, CHECKSUM,
            "the font corpus drifted from the C++ baseline"
        );
        let file: Option<&[u8]> = Some(&bytes);
        let mut group = c.benchmark_group("font");
        group.throughput(Throughput::Bytes(bytes.len() as u64));
        group.bench_function("parse_x131k", |b| {
            b.iter(|| checksum(&parse_font(black_box(b"Bench.tga"), black_box(file))))
        });
        group.finish();
    }
}
