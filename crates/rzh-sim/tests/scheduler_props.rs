// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Rust port Copyright 2026 the RustZeroHour contributors.
//
//! Property and unit tests for the kernel's scheduler and conversions.
//!
//! - the sleepy heap's invariant (every parent's packed priority at most
//!   its children's; every module's index stamp pointing at its own
//!   slot) over random push/erase/requeue sequences;
//! - the packed-priority saturation and phase re-derivation;
//! - the Hoffesommer conversion quirks the golden corpora pin at sample
//!   points (the arithmetic-shift mask, the positive zero of negative
//!   fractions, the one-ulp-below-one floors/ceils).

use proptest::prelude::*;

use rzh_sim::logic::{Kernel, ObjectSpec, UpdateModuleSpec};
use rzh_sim::partition::Region2;
use rzh_sim::scheduler::{ModuleArena, ModuleHandle, SleepyState};
use rzh_sim::types::{SleepyUpdatePhase, UPDATE_SLEEP_FOREVER};

// --- the sleepy heap ---------------------------------------------------------

/// A minimal arena: modules keyed by handle, no objects.
#[derive(Default)]
struct FlatArena {
    modules: Vec<SleepyState>,
}

impl ModuleArena for FlatArena {
    fn module(&self, handle: ModuleHandle) -> &SleepyState {
        &self.modules[handle.module as usize]
    }
    fn module_mut(&mut self, handle: ModuleHandle) -> &mut SleepyState {
        &mut self.modules[handle.module as usize]
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Random push / requeue / erase sequences keep the heap valid: the
    /// parent-first ordering on the packed priority and the index stamps
    /// (the `validateSleepyUpdate` contract, checked after every step).
    #[test]
    fn heap_invariant_holds(
        ops in proptest::collection::vec(
            proptest::arbitrary::any::<u8>(),
            0..128
        ),
        priorities in proptest::collection::vec(0u32..0x4000_0000, 1..64),
    ) {
        let mut arena = FlatArena::default();
        let mut scheduler = rzh_sim::scheduler::SleepyScheduler::default();

        // push every module with its random priority
        for (i, &pri) in priorities.iter().enumerate() {
            let handle = ModuleHandle { obj: 1, module: i as u32 };
            arena.modules.push(SleepyState::new(SleepyUpdatePhase::Normal, Default::default()));
            arena.module_mut(handle).set_next_call_frame(pri >> 2);
            scheduler.push(&mut arena, handle);
            assert!(scheduler.validate(&arena), "invalid after push {i}");
        }

        let mut rng_state = 0x1234_5678u64;
        let mut next = || {
            rng_state = rng_state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            rng_state >> 33
        };

        for &op in &ops {
            let live = scheduler.heap.len();
            match op % 3 {
                0 if live > 0 => {
                    // requeue the root (the dispatch loop's shape)
                    let root = scheduler.heap[0];
                    let sleep = 1 + (next() % 32) as u32;
                    let frame = (arena.module(root).next_call_frame()).wrapping_add(sleep);
                    arena.module_mut(root).set_next_call_frame(frame);
                    scheduler.rebalance(&mut arena, 0);
                }
                1 if live > 1 => {
                    // erase a random slot (the destroy list's shape)
                    let idx = (next() as usize) % live;
                    scheduler.erase(&mut arena, idx);
                }
                _ => {
                    // push a fresh module
                    let handle = ModuleHandle { obj: 1, module: arena.modules.len() as u32 };
                    arena.modules.push(SleepyState::new(SleepyUpdatePhase::Normal, Default::default()));
                    arena.module_mut(handle).set_next_call_frame(((next() % 0x4000_0000) >> 2) as u32);
                    scheduler.push(&mut arena, handle);
                }
            }
            assert!(scheduler.validate(&arena), "invalid after op {op} (heap {})", scheduler.heap.len());
        }
    }
}

#[test]
fn saturation_clamps_past_forever() {
    let mut state = SleepyState::new(SleepyUpdatePhase::Final, Default::default());
    // anything greater than "forever" is still "forever"
    state.set_next_call_frame(0x7fff_fff0);
    assert_eq!(state.next_call_frame(), UPDATE_SLEEP_FOREVER as u32);
    // the phase rides along from the module's declared phase
    assert_eq!(
        state.priority(),
        (UPDATE_SLEEP_FOREVER as u32) << 2 | SleepyUpdatePhase::Final.bits()
    );
    // exactly forever: no clamp
    state.set_next_call_frame(UPDATE_SLEEP_FOREVER as u32);
    assert_eq!(state.next_call_frame(), UPDATE_SLEEP_FOREVER as u32);
}

// --- the Hoffesommer conversions ---------------------------------------------

#[test]
fn fast_conversions_sample_points() {
    use rzh_sim::fastmath::{
        fast_float_ceil, fast_float_floor, fast_float_trunc, real_to_int_ceil, real_to_int_floor,
    };
    // the arithmetic-shift mask keeps the integer mantissa bits
    assert_eq!(fast_float_trunc(11.0), 11.0);
    assert_eq!(fast_float_trunc(3.7), 3.0);
    assert_eq!(fast_float_trunc(1.5), 1.0);
    // sub-integral magnitudes mask to zero (the positive zero the
    // golden corpora pin for negative fractions)
    assert_eq!(fast_float_trunc(0.5).to_bits(), 0.0f32.to_bits());
    // the one-ulp-below-one pushes across the integer boundary
    assert_eq!(fast_float_ceil(10.0), 11.0);
    assert_eq!(fast_float_floor(-0.1), -1.0);
    assert_eq!(fast_float_floor(9.999_999), 9.0);
    // the f32 0.1 drift the cell-count corpus pins: 100 * 0.1f is 10
    // exactly, and the ceil still crosses (the sum rounds up)
    assert_eq!(fast_float_ceil(100.0f32 * 0.1f32), 11.0);
    // the int conversions
    assert_eq!(real_to_int_ceil(100.0f32 * 0.1f32), 11);
    assert_eq!(real_to_int_floor(-0.001), -1);
    assert_eq!(real_to_int_floor(9.999_999), 9);
}

// --- the frame constants -----------------------------------------------------

#[test]
fn frame_constants() {
    // the WP1 "30 Hz in retail, to verify" item
    assert_eq!(rzh_sim::types::LOGICFRAMES_PER_SECOND, 30);
    assert_eq!(rzh_sim::types::WW_SYNC_MILLISECONDS, 33);
    assert_eq!(rzh_sim::types::MSEC_PER_LOGICFRAME_REAL, 1000.0 / 30.0);
}

// --- the kernel's ordered walk ------------------------------------------------

#[test]
fn freeze_gate_stops_before_partition() {
    // the freeze gate returns before the terrain — the host's event
    // order is the observable (the golden corpus pins the full walk)
    let mut kernel = Kernel::with_grid(
        10.0,
        Region2 {
            lo_x: 0.0,
            lo_y: 0.0,
            hi_x: 100.0,
            hi_y: 100.0,
        },
    );
    let spec = ObjectSpec {
        modules: vec![UpdateModuleSpec::default()],
        ..Default::default()
    };
    #[derive(Default)]
    struct Frozen(bool);
    let mut host = Frozen(false);
    impl rzh_sim::logic::KernelHost for Frozen {
        fn is_time_frozen(&mut self) -> bool {
            self.0
        }
    }
    let _id = kernel.create_object(&spec, &mut host);
    host.0 = true;
    let frame_before = kernel.frame();
    kernel.update(&mut host);
    assert_eq!(
        kernel.frame(),
        frame_before,
        "the frozen frame does not advance"
    );
    host.0 = false;
    kernel.update(&mut host);
    assert_eq!(kernel.frame(), frame_before + 1);
}
