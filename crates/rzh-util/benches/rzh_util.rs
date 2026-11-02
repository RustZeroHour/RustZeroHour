// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
//! Criterion benchmarks for `rzh-util` (migration plan §6, Phase 1 exit
//! criterion: "benchmarks within budget").
//!
//! The measured operations are the three P0-exact foundation algorithms and
//! mirror one-to-one the C++ baseline timings produced by
//! `tools/differential/bench_baseline.cpp`, which times the *same*
//! transcription the golden vectors come from (gen_golden.cpp), compiled the
//! same way. Compare with:
//!
//! ```text
//! cargo bench -p rzh-util
//! tools/differential/bench_baseline.sh   # crc / rng / namekey sections
//! ```
//!
//! Budget: plan §6 — no more than 20 % slower than the C++ baseline unless
//! an ADR justifies it. The measured values live in `docs/inventory/`.
//!
//! # Determinism note
//!
//! Benchmark *corpora* are generated with the same LCG the differential
//! harness uses (`lcg_state = 0xc0ffee11`, Numerical Recipes constants), so
//! the C++ harness and these benches hash, compress and parse **identical
//! bytes**. The benchmarks measure performance only; parity is the golden
//! tests' job, not Criterion's.

// Bench targets have no public API; the `criterion_group!` macro generates
// `pub fn`s that the workspace `missing_docs` lint would otherwise flag.
#![allow(missing_docs)]

use std::hint::black_box;

use criterion::{BatchSize, Criterion, Throughput, criterion_group, criterion_main};
use rzh_util::{Crc, GameRng, NameKeyGenerator};

/// The differential harness's LCG (`gen_compress_golden.cpp`): shared so the
/// C++ baseline and these benches operate on identical corpora.
fn lcg(state: &mut u32) -> u32 {
    *state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    *state
}

/// A deterministic byte corpus of `len` bytes.
fn corpus(len: usize) -> Vec<u8> {
    let mut data = Vec::with_capacity(len);
    let mut state = 0xc0f_fee11_u32;
    for _ in 0..len {
        // Two draws per byte: the values used by the C++ harness corpus.
        let hi = lcg(&mut state);
        let lo = lcg(&mut state);
        data.push(((hi >> 24) ^ lo) as u8);
    }
    data
}

/// CRC throughput: the checksum runs over network state hashes and save
/// chunks, so bytes/second is the interesting number.
fn bench_crc(c: &mut Criterion) {
    let mut group = c.benchmark_group("crc");
    for size in [64_usize, 4_096, 65_536] {
        let data = corpus(size);
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_function(format!("update/{size}"), |b| {
            b.iter(|| {
                let mut crc = Crc::new();
                crc.update(black_box(&data));
                crc.get()
            })
        });
    }
    group.finish();
}

/// The RNG streams: `random_int` is the hot combat/economy roll, `raw` is
/// the primitive every other entry point builds on. Each batch accumulates
/// into a local sink (returned, hence consumed) — matching the C++
/// baseline's loop shape exactly.
fn bench_rng(c: &mut Criterion) {
    let mut group = c.benchmark_group("rng");

    group.bench_function("raw", |b| {
        b.iter(|| {
            let mut rng = GameRng::seeded(42);
            let mut sink = 0_u32;
            for _ in 0..256 {
                sink = sink.wrapping_add(rng.raw());
            }
            sink
        })
    });

    group.bench_function("random_int/0_100", |b| {
        b.iter(|| {
            let mut rng = GameRng::seeded(42);
            let mut sink = 0_i64;
            for _ in 0..256 {
                sink += i64::from(rng.random_int(0, 100));
            }
            sink
        })
    });

    group.bench_function("random_real/0_1", |b| {
        b.iter(|| {
            let mut rng = GameRng::seeded(42);
            let mut sink = 0.0_f32;
            for _ in 0..256 {
                sink += rng.random_real(0.0, 1.0);
            }
            sink
        })
    });

    group.finish();
}

/// Name-key interning: the data load interns every block name once (miss
/// path, allocates) and then looks names up repeatedly while templates are
/// wired together (hit path).
fn bench_namekey(c: &mut Criterion) {
    // 512 template-shaped names, like the C++ baseline corpus.
    let names: Vec<String> = (0..512)
        .map(|i| format!("AmericaVehicleDozer_{i:05}"))
        .collect();
    let borrowed: Vec<&str> = names.iter().map(String::as_str).collect();

    let mut group = c.benchmark_group("namekey");

    // Miss path: every name is new. Setup (a fresh generator per batch) is
    // excluded from the measurement; the C++ harness resets its generator
    // outside the timed loop the same way.
    group.bench_function("intern_new/x512", |b| {
        b.iter_batched(
            NameKeyGenerator::new,
            |mut keys| {
                for name in &borrowed {
                    black_box(keys.name_to_key(black_box(name)));
                }
                keys.len() as u32
            },
            BatchSize::LargeInput,
        )
    });

    // Hit path: all 512 names already interned, then re-requested.
    group.bench_function("intern_hit/x512", |b| {
        b.iter_batched(
            || {
                let mut keys = NameKeyGenerator::new();
                for name in &borrowed {
                    keys.name_to_key(name);
                }
                keys
            },
            |mut keys| {
                for name in &borrowed {
                    black_box(keys.name_to_key(black_box(name)));
                }
                keys.len() as u32
            },
            BatchSize::LargeInput,
        )
    });

    // Reverse lookup (O(1) in the port, linear scan in the C++ baseline).
    group.bench_function("key_to_name/x512", |b| {
        b.iter_batched(
            || {
                let mut keys = NameKeyGenerator::new();
                let mut ids = Vec::with_capacity(borrowed.len());
                for name in &borrowed {
                    ids.push(keys.name_to_key(name));
                }
                (keys, ids)
            },
            |(keys, ids)| {
                let mut sum = 0_u32;
                for id in &ids {
                    let len = black_box(keys.key_to_name(*id)).map_or(0, <[u8]>::len);
                    sum = sum.wrapping_add(len as u32);
                }
                sum
            },
            BatchSize::LargeInput,
        )
    });

    group.finish();
}

criterion_group!(benches, bench_crc, bench_rng, bench_namekey);
criterion_main!(benches);
