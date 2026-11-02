// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
//! Criterion benchmarks for `rzh-compress` (migration plan §6, Phase 1
//! exit criterion).
//!
//! RefPack is on every BIG-archive payload read and every asset the tools
//! write, so throughput on the three characteristic corpora matters:
//!
//! - `zeros`: maximal back-reference workload (audio-ish silences, padding),
//! - `text`: line-oriented, INI-like data (the mod data path),
//! - `random`: incompressible bytes (the encoder's worst case).
//!
//! The corpora are generated with the differential harness's LCG, so the
//! C++ baseline timings of `tools/differential/bench_baseline.cpp` (which
//! call the REAL `refencode.cpp`/`refdecode.cpp` sources) compress and
//! decompress **identical bytes**. Compare with:
//!
//! ```text
//! cargo bench -p rzh-compress
//! tools/differential/bench_baseline.sh   # refpack section
//! ```

// Bench targets have no public API; the `criterion_group!` macro generates
// `pub fn`s that the workspace `missing_docs` lint would otherwise flag.
#![allow(missing_docs)]

use std::hint::black_box;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use rzh_compress::refpack;

/// The differential harness's LCG (`gen_compress_golden.cpp`).
fn lcg(state: &mut u32) -> u32 {
    *state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    *state
}

/// The three corpora at 64 KiB each (the C++ harness uses the same sizes).
fn corpora() -> Vec<(&'static str, Vec<u8>)> {
    let size = 65_536_usize;

    let zeros = vec![0u8; size];

    // INI-shaped text: a 48-byte skeleton line repeated with a counter.
    let mut text = Vec::with_capacity(size);
    let mut line = 0u32;
    while text.len() < size {
        let chunk = format!(
            "  Damage = {line}\n  Radius = 25.0\n  Weapon = SmallWeapon_{line:05}\n  End\n"
        );
        text.extend_from_slice(chunk.as_bytes());
        line = line.wrapping_add(1);
    }
    text.truncate(size);

    let mut random = Vec::with_capacity(size);
    let mut state = 0xc0f_fee11_u32;
    for _ in 0..size {
        random.push(lcg(&mut state) as u8);
    }

    vec![("zeros", zeros), ("text", text), ("random", random)]
}

/// Decoding: the hot read path of every mounted archive payload.
fn bench_decode(c: &mut Criterion) {
    let mut group = c.benchmark_group("refpack/decode");
    for (name, raw) in corpora() {
        let packed = refpack::compress(&raw);
        let ulen = raw.len();
        group.throughput(Throughput::Bytes(ulen as u64));
        group.bench_function(name.to_string(), |b| {
            b.iter(|| {
                let mut dest = vec![0u8; ulen];
                let (written, _consumed) = refpack::decode(&mut dest, black_box(&packed)).unwrap();
                written
            })
        });
    }
    group.finish();
}

/// Encoding: the tools/write path and any cache the engine keeps.
fn bench_encode(c: &mut Criterion) {
    let mut group = c.benchmark_group("refpack/encode");
    for (name, raw) in corpora() {
        group.throughput(Throughput::Bytes(raw.len() as u64));
        group.bench_function(name.to_string(), |b| {
            b.iter(|| refpack::compress(black_box(&raw)).len())
        });
    }
    group.finish();
}

/// Header dispatch: `is_refpack` + `uncompressed_size` run on every file
/// the VFS opens, before any decompression decision is made.
fn bench_header(c: &mut Criterion) {
    let raw = &corpora()[1].1;
    let packed = refpack::compress(raw);
    let mut group = c.benchmark_group("refpack/header");
    group.bench_function("is_refpack_and_size", |b| {
        b.iter(|| {
            let is = refpack::is_refpack(black_box(&packed));
            let size = refpack::uncompressed_size(black_box(&packed)).ok();
            (is, size)
        })
    });
    group.finish();
}

criterion_group!(benches, bench_decode, bench_encode, bench_header);
criterion_main!(benches);
