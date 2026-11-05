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
    bench_dds_convert
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
