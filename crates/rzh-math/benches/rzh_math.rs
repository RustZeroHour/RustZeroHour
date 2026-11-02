// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
//! Criterion benchmarks for `rzh-math` (migration plan §6, Phase 1 exit
//! criterion).
//!
//! The vector, matrix and slerp operations below mirror the C++ baseline
//! timings of `tools/differential/bench_baseline.cpp`, which times the
//! WWMath transcription of `gen_golden.cpp` (the same code the golden
//! vectors pin), on the identical LCG corpora and with identical loop
//! shapes. Compare with:
//!
//! ```text
//! cargo bench -p rzh-math
//! tools/differential/bench_baseline.sh   # math section
//! ```
//!
//! # Methodology
//!
//! Criterion consumes (and `black_box`es) each closure's return value, so
//! the loops below deliberately do **not** black-box individual operands:
//! per-operand opacity would forbid the auto-vectorization both compilers
//! are otherwise able to do, and would understate the port's performance
//! relative to the C++ baseline. The corpora are built once per group,
//! outside the measured closure, from the deterministic differential-harness
//! LCG — exactly like the C++ harness builds its corpus objects untimed.
//!
//! # Determinism note
//!
//! Nothing here executes inside the simulation — these benches only measure
//! the arithmetic kernels. Parity is the golden tests' job, not Criterion's.

// Bench targets have no public API; the `criterion_group!` macro generates
// `pub fn`s that the workspace `missing_docs` lint would otherwise flag.
#![allow(missing_docs)]

use criterion::{Criterion, criterion_group, criterion_main};
use rzh_math::colmath::overlap_test_aabox_aabox;
use rzh_math::fast::{fast_acos, fast_cos, fast_sin};
use rzh_math::quat::{build_matrix3, fast_slerp, slerp};
use rzh_math::{AABox, Matrix3, Matrix3D, Matrix4, Quaternion, Vector2, Vector3, Vector4};

/// The differential harness's LCG (`gen_compress_golden.cpp`).
fn lcg(state: &mut u32) -> u32 {
    *state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    *state
}

/// Deterministic floats in a moderate range (keeps every op finite and
/// well-conditioned, like the C++ corpus).
fn float_corpus(count: usize) -> Vec<f32> {
    let mut out = Vec::with_capacity(count);
    let mut state = 0xc0f_fee11_u32;
    for _ in 0..count {
        let bits = lcg(&mut state);
        // Map to (-2, 2): finite, non-denormal, no NaN/inf corner cases.
        let unit = (bits >> 8) as f32 / 16_777_216.0;
        out.push(unit * 4.0 - 2.0);
    }
    out
}

/// Vector kernels (WWMath `vector{2,3,4}.h`).
fn bench_vectors(c: &mut Criterion) {
    let xs = float_corpus(256);
    let inputs: Vec<Vector3> = xs
        .as_chunks::<3>()
        .0
        .iter()
        .map(|c| Vector3::new(c[0], c[1], c[2] * 0.5 + 1.0))
        .collect();
    let others: Vec<Vector3> = xs
        .iter()
        .skip(1)
        .zip(xs.iter().skip(2))
        .map(|(a, b)| Vector3::new(b + 0.5, a * 0.25, *a))
        .collect();
    let inputs2: Vec<Vector2> = xs
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| Vector2::new(c[0] * 10.0, c[1] * 10.0))
        .collect();
    let others2: Vec<Vector2> = inputs2.iter().rev().copied().collect();

    let mut group = c.benchmark_group("vector3");

    group.bench_function("length/x256", |b| {
        b.iter(|| inputs.iter().map(|v| v.length()).sum::<f32>())
    });

    group.bench_function("normalize/x256", |b| {
        b.iter(|| {
            inputs
                .iter()
                .map(|v| {
                    let mut v = *v;
                    v.normalize();
                    v.x
                })
                .sum::<f32>()
        })
    });

    group.bench_function("dot/x256", |b| {
        b.iter(|| {
            inputs
                .iter()
                .zip(&others)
                .map(|(a, b)| Vector3::dot(*a, *b))
                .sum::<f32>()
        })
    });

    group.bench_function("cross/x256", |b| {
        b.iter(|| {
            inputs
                .iter()
                .zip(&others)
                .map(|(a, b)| Vector3::cross(*a, *b).z)
                .sum::<f32>()
        })
    });

    // Vector2::quick_distance: the partition manager's cheap range check.
    group.bench_function("quick_distance2d/x256", |b| {
        b.iter(|| {
            inputs2
                .iter()
                .zip(&others2)
                .map(|(a, b)| Vector2::quick_distance(*a, *b))
                .sum::<f32>()
        })
    });

    // Vector4::lerp (the free-function form without the out-parameter quirk).
    let inputs4: Vec<Vector4> = xs
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| Vector4::new(c[0], c[1], c[2] * 0.5, c[3].abs()))
        .collect();
    let others4: Vec<Vector4> = inputs4.iter().rev().copied().collect();
    group.bench_function("lerp4/x256", |b| {
        b.iter(|| {
            inputs4
                .iter()
                .zip(&others4)
                .map(|(a, b)| Vector4::lerp(*a, *b, 0.25).x)
                .sum::<f32>()
        })
    });

    group.finish();
}

/// Matrix kernels (WWMath `matrix{3,3d,4}.h`): the transform chain of every
/// drawable, every frame.
fn bench_matrices(c: &mut Criterion) {
    let xs = float_corpus(64);
    let mats3d: Vec<Matrix3D> = xs
        .as_chunks::<3>()
        .0
        .iter()
        .map(|c| {
            Matrix3D::from_axes_position(
                Vector3::new(c[0], c[1], 0.0),
                Vector3::new(c[1] * 0.5 + 0.1, c[2], c[0]),
                Vector3::new(c[2], 0.0, c[1]),
                Vector3::new(c[0] * 5.0, c[1] * 5.0, c[2] * 5.0),
            )
        })
        .collect();
    let mats4: Vec<Matrix4> = mats3d.iter().map(|m| Matrix4::from_matrix3d(*m)).collect();
    let mats3: Vec<Matrix3> = xs
        .as_chunks::<3>()
        .0
        .iter()
        .map(|c| {
            Matrix3::from_rows(
                Vector3::new(c[0], c[1], c[2]),
                Vector3::new(c[1], c[2], c[0]),
                Vector3::new(c[2], c[0], c[1]),
            )
        })
        .collect();
    let points: Vec<Vector3> = xs
        .as_chunks::<3>()
        .0
        .iter()
        .map(|c| Vector3::new(c[0] * 32.0, c[1] * 32.0, c[2] * 32.0))
        .collect();

    let mut group = c.benchmark_group("matrix");

    group.bench_function("matrix3_multiply/x64", |b| {
        b.iter(|| {
            mats3
                .iter()
                .zip(mats3.iter().rev())
                .map(|(a, b)| Matrix3::multiply(a, b))
                .map(|m| m[(0, 0)])
                .sum::<f32>()
        })
    });

    group.bench_function("matrix3d_multiply/x64", |b| {
        b.iter(|| {
            mats3d
                .iter()
                .zip(mats3d.iter().rev())
                .map(|(a, b)| Matrix3D::multiply(*a, *b))
                .map(|m| m.rows[0].x)
                .sum::<f32>()
        })
    });

    group.bench_function("matrix4_multiply/x64", |b| {
        b.iter(|| {
            mats4
                .iter()
                .zip(mats4.iter().rev())
                .map(|(a, b)| Matrix4::multiply(*a, *b))
                .map(|m| m.rows[0].x)
                .sum::<f32>()
        })
    });

    group.bench_function("matrix3d_transform_vector/x64", |b| {
        b.iter(|| {
            mats3d
                .iter()
                .zip(points.iter())
                .map(|(m, p)| m.transform_vector(*p).x)
                .sum::<f32>()
        })
    });

    group.bench_function("matrix4_inverse/x64", |b| {
        b.iter(|| mats4.iter().filter(|m| m.get_inverse().is_some()).count())
    });

    group.finish();
}

/// Quaternions (`quat.h`): the slerp pair the animation system runs per
/// blended pose, and the quaternion→matrix build of every pose evaluation.
fn bench_quat(c: &mut Criterion) {
    let xs = float_corpus(64);
    let quats: Vec<Quaternion> = xs
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| {
            let mut q = Quaternion::new(c[0], c[1], c[2], c[3]);
            q.normalize();
            q
        })
        .collect();
    let alphas: Vec<f32> = xs.iter().map(|x| x * 0.25 + 0.5).collect();

    let mut group = c.benchmark_group("quat");

    group.bench_function("slerp/x64", |b| {
        b.iter(|| {
            quats
                .iter()
                .zip(quats.iter().rev())
                .enumerate()
                .map(|(i, (p, q))| slerp(*p, *q, alphas[i]).w)
                .sum::<f32>()
        })
    });

    group.bench_function("fast_slerp/x64", |b| {
        b.iter(|| {
            quats
                .iter()
                .zip(quats.iter().rev())
                .enumerate()
                .map(|(i, (p, q))| fast_slerp(*p, *q, alphas[i]).w)
                .sum::<f32>()
        })
    });

    group.bench_function("build_matrix3/x64", |b| {
        b.iter(|| quats.iter().map(|q| build_matrix3(*q)[(0, 0)]).sum::<f32>())
    });

    group.finish();
}

/// The `Fast_*` table lookups and the AA-box overlap test of the cull path.
fn bench_trig_overlap(c: &mut Criterion) {
    let xs = float_corpus(256);
    // fast_sin/cos take full-turn angles; fast_acos takes [-1, 1].
    let angles: Vec<f32> = xs.iter().map(|x| x * std::f32::consts::PI).collect();
    let unit: Vec<f32> = xs.iter().map(|x| x * 0.49).collect();

    let mut group = c.benchmark_group("fast");

    group.bench_function("fast_sin/x256", |b| {
        b.iter(|| angles.iter().map(|a| fast_sin(*a)).sum::<f32>())
    });
    group.bench_function("fast_cos/x256", |b| {
        b.iter(|| angles.iter().map(|a| fast_cos(*a)).sum::<f32>())
    });
    group.bench_function("fast_acos/x256", |b| {
        b.iter(|| unit.iter().map(|v| fast_acos(*v)).sum::<f32>())
    });
    group.finish();

    let boxes: Vec<AABox> = xs
        .as_chunks::<3>()
        .0
        .iter()
        .map(|c| {
            let center = Vector3::new(c[0] * 16.0, c[1] * 16.0, c[2] * 16.0);
            AABox::new(center, Vector3::new(1.0, 1.0, 1.0))
        })
        .collect();
    let moved: Vec<AABox> = boxes
        .iter()
        .map(|b| AABox::new(b.center + Vector3::new(1.5, 0.0, 0.0), b.extent))
        .collect();

    let mut group = c.benchmark_group("overlap");
    group.bench_function("aabox_aabox/x256", |b| {
        b.iter(|| {
            boxes
                .iter()
                .zip(&moved)
                .filter(|(a, b)| overlap_test_aabox_aabox(a, b) == rzh_math::OverlapType::Both)
                .count()
        })
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_vectors,
    bench_matrices,
    bench_quat,
    bench_trig_overlap
);
criterion_main!(benches);
