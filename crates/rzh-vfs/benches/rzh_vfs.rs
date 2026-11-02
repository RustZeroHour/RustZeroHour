// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
//! Criterion benchmarks for `rzh-vfs` (migration plan §6, Phase 1 exit
//! criterion).
//!
//! Four layers are measured separately, because they fail differently:
//!
//! 1. **`open_memory`** — BIGF header + entry-table parse (mount cost; a
//!    mod-heavy install mounts dozens of archives),
//! 2. **metadata lookups** — `file_info`/`does_file_exist` through the
//!    overlay (every `open` starts with these),
//! 3. **payload reads** — `read_entry` and the windowed `StreamingFile`
//!    over uncompressed and RefPack-wrapped entries,
//! 4. **the consumer flow** — `read` + EAR dispatch + decompress, i.e. what
//!    an `INI::load` actually pays per file.
//!
//! The archives are synthetic and built in memory with the BIGF layout of
//! `rzh_vfs::big` (big-endian entry table, lowercased leaf names). The
//! name-shaped payloads use the differential harness's LCG so the corpus is
//! reproducible.
//!
//! # Budget note
//!
//! Unlike `rzh-util`/`rzh-compress`/`rzh-ini`, there is no 1:1 C++ timing
//! counterpart here: the C++ `ArchiveFile` reads through the buffered
//! `File` layer on disk, while the port keeps a backing handle over the
//! same bytes — the algorithms are golden-pinned but the IO layers differ.
//! These benches therefore serve as the **regression baseline** the plan's
//! §6 budget is tracked against over time (and the parse/routing logic they
//! exercise is byte-exact against the C++ transcription via
//! `tests/golden/vfs.txt`).

// Bench targets have no public API; the `criterion_group!` macro generates
// `pub fn`s that the workspace `missing_docs` lint would otherwise flag.
#![allow(missing_docs)]

use std::hint::black_box;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use rzh_vfs::BigArchive;
use rzh_vfs::compression::{compress_refpack, decompress_data, is_data_compressed};

/// The differential harness's LCG (`gen_compress_golden.cpp`).
fn lcg(state: &mut u32) -> u32 {
    *state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    *state
}

/// One synthetic archive: `entries` files of ~1 KiB INI-shaped text, every
/// other one RefPack-wrapped (the shipped `*.big` mix). `variant` shifts
/// the LCG seed so two archives with the same layout carry different
/// payloads (the base-game + mod situation).
fn build_archive(entries: usize, variant: u32) -> Vec<u8> {
    let mut state = 0xc0f_fee11_u32 ^ variant.rotate_left(17);
    let mut payloads: Vec<Vec<u8>> = Vec::with_capacity(entries);
    for index in 0..entries {
        let mut payload = Vec::with_capacity(1_024);
        let mut line = 0u32;
        while payload.len() < 1_024 {
            let hot = lcg(&mut state) % 37;
            let chunk = format!("  Field{line:02} = Value{hot:03} ; comment {index:06}\n");
            payload.extend_from_slice(chunk.as_bytes());
            line += 1;
        }
        payload.truncate(1_024);
        payloads.push(payload);
    }

    // Entry table: offset, size, NUL-terminated path (`data\fileNNNN.ini`).
    let mut names: Vec<String> = Vec::with_capacity(entries);
    for index in 0..entries {
        names.push(format!(r"data\file{index:05}.ini"));
    }
    let mut table_size = 0usize;
    for name in &names {
        table_size += 4 + 4 + name.len() + 1;
    }
    let first_offset = 16 + table_size;

    let mut stored: Vec<Vec<u8>> = Vec::with_capacity(entries);
    let mut offsets = Vec::with_capacity(entries);
    let mut cursor = first_offset;
    for (index, payload) in payloads.iter().enumerate() {
        let bytes = if index % 2 == 0 {
            compress_refpack(payload)
        } else {
            payload.clone()
        };
        offsets.push(cursor);
        cursor += bytes.len();
        stored.push(bytes);
    }

    let mut out = Vec::with_capacity(cursor);
    out.extend_from_slice(b"BIGF");
    out.extend_from_slice(&(cursor as u32).to_le_bytes()); // archiveFileSize (LE)
    out.extend_from_slice(&(entries as u32).to_be_bytes()); // numFiles (BE)
    out.extend_from_slice(&16u32.to_be_bytes()); // firstEntryOffset (BE)
    for (index, name) in names.iter().enumerate() {
        out.extend_from_slice(&(offsets[index] as u32).to_be_bytes());
        out.extend_from_slice(&(stored[index].len() as u32).to_be_bytes());
        out.extend_from_slice(name.as_bytes());
        out.push(0);
    }
    for bytes in &stored {
        out.extend_from_slice(bytes);
    }
    out
}

/// Mount cost: header + entry table parse (plus the per-archive tree).
fn bench_open(c: &mut Criterion) {
    let mut group = c.benchmark_group("big/open_memory");
    for entries in [1_000_usize, 10_000] {
        let image = build_archive(entries, 0);
        group.throughput(Throughput::Elements(entries as u64));
        group.bench_function(format!("entries/{entries}"), |b| {
            b.iter(|| {
                BigArchive::open_memory(black_box("Bench.big"), black_box(image.clone()))
                    .map(|a| a.entry_count())
                    .unwrap()
            })
        });
    }
    group.finish();
}

/// Metadata path: name → info, over the tree, hit and miss.
fn bench_lookup(c: &mut Criterion) {
    let image = build_archive(10_000, 0);
    let archive = BigArchive::open_memory("Bench.big", image).unwrap();
    let hit = r"data\file05000.ini";
    let miss = r"data\no_such_file.ini";

    let mut group = c.benchmark_group("big/lookup");
    group.bench_function("file_info_hit", |b| {
        b.iter(|| {
            black_box(&archive)
                .archived_file_info(black_box(hit))
                .map(|i| i.size)
        })
    });
    group.bench_function("file_info_miss", |b| {
        b.iter(|| {
            black_box(&archive)
                .archived_file_info(black_box(miss))
                .map(|i| i.size)
        })
    });
    group.bench_function("listing/data_dir", |b| {
        b.iter(|| {
            let mut list = rzh_vfs::FilenameList::new();
            black_box(&archive).get_file_list_in_directory("data", "*.ini", &mut list);
            list.len()
        })
    });
    group.finish();
}

/// Payload path: whole-entry read and windowed streaming.
fn bench_read(c: &mut Criterion) {
    let image = build_archive(10_000, 0);
    let archive = BigArchive::open_memory("Bench.big", image).unwrap();

    let stored_info = archive.archived_file_info(r"data\file00001.ini").unwrap();
    let packed_info = archive.archived_file_info(r"data\file00000.ini").unwrap();

    let mut group = c.benchmark_group("big/read");
    group.throughput(Throughput::Bytes(stored_info.size as u64));
    group.bench_function("read_entry/plain_1k", |b| {
        b.iter(|| {
            black_box(&archive)
                .read_entry(black_box(stored_info))
                .unwrap()
                .len()
        })
    });
    group.throughput(Throughput::Bytes(packed_info.size as u64));
    group.bench_function("read_entry/refpack_1k", |b| {
        b.iter(|| {
            black_box(&archive)
                .read_entry(black_box(packed_info))
                .unwrap()
                .len()
        })
    });

    // Streaming: 1 KiB entry in 256-byte windows, like the audio path.
    group.throughput(Throughput::Bytes(stored_info.size as u64));
    group.bench_function("streaming/256b_windows", |b| {
        b.iter(|| {
            let mut stream = black_box(&archive)
                .open_streaming(black_box(stored_info))
                .unwrap();
            let mut window = [0u8; 256];
            let mut total = 0usize;
            loop {
                let n = stream.read(&mut window);
                if n == 0 {
                    break;
                }
                total += n;
            }
            total
        })
    });
    group.finish();
}

/// The full consumer flow through the `Vfs` overlay: mount two archives
/// (the later one in front, like a mod), route a name to an instance,
/// read it, then EAR-dispatch + RefPack-decompress it.
fn bench_vfs_flow(c: &mut Criterion) {
    use rzh_vfs::Vfs;

    let base = build_archive(1_000, 0);
    // A second archive with the same layout but different payload bytes:
    // the mod-installed-over-base situation.
    let modded = build_archive(1_000, 1);

    let mut group = c.benchmark_group("vfs");

    group.bench_function("does_file_exist/hit", |b| {
        let mut vfs = Vfs::new("/nonexistent-root");
        vfs.mount_big_memory("Base.big", base.clone(), false)
            .unwrap();
        vfs.mount_big_memory("Mod.big", modded.clone(), true)
            .unwrap();
        b.iter(|| black_box(&vfs).does_file_exist(black_box(r"data\file00500.ini"), 0))
    });

    group.bench_function("read_and_decompress/compressed", |b| {
        let mut vfs = Vfs::new("/nonexistent-root");
        vfs.mount_big_memory("Base.big", base.clone(), false)
            .unwrap();
        b.iter(|| {
            // The CachedFileInputStream flow: read raw, dispatch on EAR,
            // decompress, serve.
            let Some(raw) = black_box(&vfs).read(black_box(r"data\file00000.ini")) else {
                return 0;
            };
            if !is_data_compressed(&raw) {
                return raw.len();
            }
            let ulen = rzh_vfs::compression::get_uncompressed_size(&raw);
            let mut dest = vec![0u8; ulen];
            match decompress_data(&raw, &mut dest) {
                Ok(actual) => {
                    dest.truncate(actual);
                    dest.len()
                }
                Err(_) => raw.len(),
            }
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_open,
    bench_lookup,
    bench_read,
    bench_vfs_flow
);
criterion_main!(benches);
