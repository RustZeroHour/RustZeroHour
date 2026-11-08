// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Rust port Copyright 2026 the RustZeroHour contributors.
//
//! The §6 budget benches for the simulation kernel, compared against the
//! C++ baseline (`tools/differential/bench_wrap_kernel.cpp`, the same
//! formula-generated corpus, run through
//! `tools/differential/bench_baseline.sh`).
//!
//! - `kernel/frame_loop`: one full `Kernel::update()` walk per frame over
//!   the mixed corpus — the sleepy dispatch with its requeue rebalances,
//!   the partition refresh of every moved object (the small, precise
//!   -circle and rotated-rect fills), the contact-pair collection and
//!   the destroy-list pass — 180 frames per measurement.
//! - `kernel/dispatch`: the same corpus with no movement — the sleepy
//!   churn alone (the heap's requeue path; the dirty list stays empty
//!   after the first frame).
//! - `kernel/query_x300`: 300 mixed spatial queries over the settled
//!   corpus — the closest-object ring walk (twice: center-2d and the
//!   boundary-2d measure behind an on-map filter) and the
//!   near-to-far-sorted range iterator drained to exhaustion, the
//!   checksum folding every id and distance bit.
//!
//! Both benches assert the final-state checksum (the heap array's packed
//! priorities in order, the object list's coverage and cell-change
//! counts, the contact-pair count) — the same value the C++ wrapper
//! computes, locking the two sides to identical bytes.

// The criterion macro generates an undocumented `main`.
#![allow(missing_docs)]

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};

use rzh_sim::logic::{Kernel, KernelHost, ObjectSpec, UpdateModuleSpec, UpdateOutcome};
use rzh_sim::partition::Region2;
use rzh_sim::query::{PartitionFilter, PartitionFilterOnMap};
use rzh_sim::scheduler::ModuleArena;
use rzh_sim::types::{
    Coord3, DisabledMask, DistanceCalculationType, GeometryInfo, GeometryType, IterOrderType,
    SleepyUpdatePhase, UpdateSleepTime,
};

/// The bench corpus size (the scale of the other map benches' corpora).
const BENCH_OBJECTS: usize = 120;
/// The frames one measurement runs.
const BENCH_FRAMES: u32 = 180;

/// The mixed-run checksum (pinned at authoring time against the C++
/// wrapper — both sides build the identical corpus).
const FRAME_LOOP_CHECKSUM: u64 = 16_055_563_144_769_613_415;
/// The no-movement checksum (ditto).
const DISPATCH_CHECKSUM: u64 = 14_539_779_273_077_219_524;

/// The scripted module bodies (the formula corpus's sleep patterns).
struct BenchHost {
    sleeps: Vec<(i32, u32, UpdateSleepTime)>,
}

impl KernelHost for BenchHost {
    fn run_update(&mut self, id: i32, module: u32) -> UpdateOutcome {
        let mut outcome = UpdateOutcome::default();
        for &(s_obj, s_mod, sleep) in &self.sleeps {
            if s_obj == id && s_mod == module {
                outcome.sleep = sleep;
            }
        }
        outcome
    }
}

/// The formula corpus: identical on both sides (the C++ wrapper builds
/// it through the same formulas; the checksums lock it).
fn build_corpus(cell_size: f32, extents: (f32, f32, f32, f32)) -> (Kernel, BenchHost) {
    let region = Region2 {
        lo_x: extents.0,
        lo_y: extents.1,
        hi_x: extents.2,
        hi_y: extents.3,
    };
    let mut kernel = Kernel::with_grid(cell_size, region);
    let mut host = BenchHost { sleeps: Vec::new() };

    for i in 0..BENCH_OBJECTS as i32 {
        let geometry = match i % 4 {
            0 => GeometryInfo {
                geom_type: GeometryType::Sphere,
                is_small: true,
                height: 1.0,
                major_radius: 2.5,
                minor_radius: 2.5,
            },
            1 => GeometryInfo {
                geom_type: GeometryType::Sphere,
                is_small: false,
                height: 1.0,
                major_radius: 7.0,
                minor_radius: 7.0,
            },
            2 => GeometryInfo {
                geom_type: GeometryType::Cylinder,
                is_small: false,
                height: 4.0,
                major_radius: 12.5,
                minor_radius: 12.5,
            },
            _ => GeometryInfo {
                geom_type: GeometryType::Box,
                is_small: false,
                height: 4.0,
                major_radius: 10.0,
                minor_radius: 6.0,
            },
        };
        let immobile = i % 37 == 0;
        let module_count = if i % 2 == 0 { 1 } else { 2 };
        let phase = match i % 4 {
            0 => SleepyUpdatePhase::Initial,
            1 => SleepyUpdatePhase::Physics,
            2 => SleepyUpdatePhase::Normal,
            _ => SleepyUpdatePhase::Final,
        };
        let mut modules = Vec::new();
        for m in 0..module_count {
            modules.push(UpdateModuleSpec {
                phase,
                disabled_types_to_process: DisabledMask::default(),
                ctor_wake: None,
            });
            let sleep = if m == 0 { 1 + (i % 30) } else { 5 + (i % 17) };
            // the ids run 1..=BENCH_OBJECTS (the counter starts at 1)
            host.sleeps.push((i + 1, m as u32, sleep));
        }
        let spec = ObjectSpec {
            geometry,
            immobile,
            modules,
            colliders: 0,
            position: Coord3 {
                x: 20.0 + (i % 20) as f32 * 13.0 + (i / 20) as f32 * 2.5,
                y: 20.0 + (i / 20) as f32 * 11.5,
                z: 0.0,
            },
            orientation: if i % 4 == 3 { 0.3 } else { 0.0 },
        };
        kernel.create_object(&spec, &mut host);
    }
    (kernel, host)
}

/// The final-state checksum: the heap array's packed priorities in
/// order, the object list's coverage and cell-change counts, and the
/// contact-pair count of the last frame. The C++ wrapper computes the
/// same value (both sides assert it at authoring time).
fn checksum(kernel: &Kernel) -> u64 {
    let mut sum: u64 = 1_000_003;
    sum = sum
        .wrapping_mul(31)
        .wrapping_add(kernel.logic.scheduler.heap.len() as u64);
    for &h in &kernel.logic.scheduler.heap {
        let packed = u64::from(kernel.objects.module(h).priority());
        sum = sum.wrapping_mul(31).wrapping_add(packed);
    }
    for &id in &kernel.logic.obj_list {
        let obj = kernel.objects.get(id);
        let part = obj.partition.as_ref().expect("registered");
        sum = sum
            .wrapping_mul(31)
            .wrapping_add(part.coi_in_use as u64)
            .wrapping_mul(31)
            .wrapping_add(part.last_cell.map_or(u64::MAX, u64::from))
            .wrapping_mul(31)
            .wrapping_add(obj.cell_change_count as u64);
    }
    sum = sum
        .wrapping_mul(31)
        .wrapping_add(kernel.pm.last_contacts.len() as u64);
    sum
}

/// The mixed corpus run: every third object moves each frame (crossing
/// cells as the offsets accumulate), two objects die at frame 90.
fn run_frames(kernel: &mut Kernel, host: &mut BenchHost, frames: u32, moving: bool) -> u64 {
    for f in 0..frames {
        if moving {
            // the ids run 1..=BENCH_OBJECTS (the counter starts at 1)
            for i in 1..=BENCH_OBJECTS as i32 {
                if kernel.objects.try_get(i).is_none() {
                    continue; // destroyed at frame 90
                }
                if (f as i32 + i) % 3 == 0 {
                    let (old, angle) = {
                        let obj = kernel.objects.get(i);
                        (obj.position, obj.orientation)
                    };
                    let (x, y) = (old.x + 1.7, old.y + 0.9);
                    kernel.objects.get_mut(i).position = Coord3 { x, y, z: 0.0 };
                    kernel.react_to_transform_change(i, old, angle);
                }
            }
        }
        if f == 90 {
            kernel.destroy_object(11, host);
            kernel.destroy_object(71, host);
        }
        kernel.update(host);
    }
    checksum(kernel)
}

/// The corpus shared by both benches (built once, measured many times).
fn corpus() -> (Kernel, BenchHost) {
    build_corpus(10.0, (0.0, 0.0, 300.0, 300.0))
}

fn bench_frame_loop(c: &mut Criterion) {
    // the authoring-time lockstep check: the corpus is deterministic, so
    // the checksum is too (the C++ wrapper asserts the same value)
    let (mut kernel, mut host) = corpus();
    let expected = run_frames(&mut kernel, &mut host, BENCH_FRAMES, true);
    // the authoring-time lockstep value (the C++ wrapper asserts the same)
    assert_eq!(expected, FRAME_LOOP_CHECKSUM);

    c.bench_function("kernel/frame_loop", |b| {
        b.iter_batched(
            corpus,
            |(mut kernel, mut host)| run_frames(&mut kernel, &mut host, BENCH_FRAMES, true),
            BatchSize::PerIteration,
        )
    });
}

fn bench_dispatch(c: &mut Criterion) {
    let (mut kernel, mut host) = corpus();
    let expected = run_frames(&mut kernel, &mut host, BENCH_FRAMES, false);
    // the authoring-time lockstep value (the C++ wrapper asserts the same)
    assert_eq!(expected, DISPATCH_CHECKSUM);

    c.bench_function("kernel/dispatch", |b| {
        b.iter_batched(
            corpus,
            |(mut kernel, mut host)| run_frames(&mut kernel, &mut host, BENCH_FRAMES, false),
            BatchSize::PerIteration,
        )
    });
}

/// The query bench's checksum (pinned at authoring time against the C++
/// wrapper — both sides build the identical corpus and query stream).
const QUERY_CHECKSUM: u64 = 1_025_416_407_242_357_139;

/// The query corpus: the frame-loop corpus settled by one full update
/// (the COI coverage the ring walk reads), then the 300-query stream —
/// a third closest-object center-2d queries, a third boundary-2d
/// closest behind an on-map filter, a third near-to-far-sorted range
/// iterators drained to exhaustion. The checksum folds every id and
/// distance bit, locking the two sides together.
fn run_queries(kernel: &mut Kernel) -> u64 {
    let mut sum: u64 = 1_000_003;
    for i in 0..300u64 {
        let pos = Coord3 {
            x: 20.0 + ((i * 7) % 260) as f32,
            y: 15.0 + ((i * 13) % 270) as f32,
            z: 0.0,
        };
        match i % 3 {
            0 => {
                let (id, dist, _) = kernel.pm.get_closest_object_from_position(
                    &mut kernel.objects,
                    pos,
                    45.0,
                    DistanceCalculationType::FromCenter2d,
                    &[],
                );
                sum = sum
                    .wrapping_mul(31)
                    .wrapping_add(id.unwrap_or(0) as u64)
                    .wrapping_mul(31)
                    .wrapping_add(u64::from(dist.to_bits()));
            }
            1 => {
                let on_map = PartitionFilterOnMap;
                let filters: [&dyn PartitionFilter; 1] = [&on_map];
                let (id, dist, _) = kernel.pm.get_closest_object_from_position(
                    &mut kernel.objects,
                    pos,
                    60.0,
                    DistanceCalculationType::FromBoundingsphere2d,
                    &filters,
                );
                sum = sum
                    .wrapping_mul(31)
                    .wrapping_add(id.unwrap_or(0) as u64)
                    .wrapping_mul(31)
                    .wrapping_add(u64::from(dist.to_bits()));
            }
            _ => {
                let mut it = kernel.pm.iterate_objects_in_range_from_position(
                    &mut kernel.objects,
                    pos,
                    35.0,
                    DistanceCalculationType::FromCenter2d,
                    &[],
                    IterOrderType::SortedNearToFar,
                );
                it.reset();
                loop {
                    let (o, num) = it.next_with_numeric();
                    if o == 0 {
                        break;
                    }
                    sum = sum
                        .wrapping_mul(31)
                        .wrapping_add(o as u64)
                        .wrapping_mul(31)
                        .wrapping_add(u64::from(num.to_bits()));
                }
            }
        }
    }
    sum
}

/// The settled query corpus (the coverage filled by one full update).
fn query_corpus() -> (Kernel, BenchHost) {
    let (mut kernel, mut host) = build_corpus(10.0, (0.0, 0.0, 300.0, 300.0));
    kernel.update(&mut host);
    (kernel, host)
}

fn bench_query(c: &mut Criterion) {
    // the authoring-time lockstep check: the corpus and the query stream
    // are deterministic, so the checksum is too (the C++ wrapper asserts
    // the same value)
    let (mut kernel, _host) = query_corpus();
    let expected = run_queries(&mut kernel);
    assert_eq!(expected, QUERY_CHECKSUM, "re-pin QUERY_CHECKSUM");

    c.bench_function("kernel/query_x300", |b| {
        b.iter_batched(
            query_corpus,
            |(mut kernel, _host)| run_queries(&mut kernel),
            BatchSize::PerIteration,
        )
    });
}

criterion_group!(benches, bench_frame_loop, bench_dispatch, bench_query);
criterion_main!(benches);
