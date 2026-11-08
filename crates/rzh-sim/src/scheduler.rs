//! The sleepy-update scheduler — the priority heap that orders every
//! update module's `update()` call.
//!
//! Origin: `GameLogic.cpp`'s heap half (`pushSleepyUpdate`,
//! `peekSleepyUpdate`, `popSleepyUpdate`, `eraseSleepyUpdate`,
//! `rebalanceParentSleepyUpdate`, `rebalanceChildSleepyUpdate`,
//! `rebalanceSleepyUpdate`, `remakeSleepyUpdate`, `isLowerPriority`) and
//! the scheduler state of `UpdateModule.h`
//! (`m_nextCallFrameAndPhase`, `m_indexInLogic`, `friend_getPriority`,
//! `friend_setNextCallFrame`) — at baseline 571a7c5. The hand-unrolled
//! `rebalanceChildSleepyUpdate` twin in the original is transcribed as
//! the pristine non-unrolled variant the same function carries in a
//! `#else` block; the two are behaviourally identical.
//!
//! The heap is a **min-heap on the packed priority**
//! `frame << 2 | phase` — "lower ordinal value means higher priority".
//! Equal priorities are NOT broken stably: the heap's array order after
//! the exact sequence of pushes, erases and rebalances decides which
//! same-priority module is peeked first, and that order is
//! parity-critical (the golden corpora pin the array itself). The
//! `index_in_logic` mirror on each module gives the O(log n) erase the
//! destroy list needs.
//!
//! `set_next_call_frame` saturates at `UPDATE_SLEEP_FOREVER` ("anything
//! greater than 'forever' is still 'forever'") and re-derives the phase
//! bits from the module's declared phase — the phase in the packed value
//! is always the module's override, never a stored residue.
//!
//! Original: Copyright 2025 Electronic Arts Inc.
//! Rust port Copyright 2026 the RustZeroHour contributors.

use crate::types::{DisabledMask, ObjectID, SleepyUpdatePhase, UPDATE_SLEEP_FOREVER};

/// A stable handle to one update module: the owning object's id plus the
/// module's index within the object's module list. The transcription's
/// `UpdateModulePtr` — pointers become handles (plan §3.3 rule 6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModuleHandle {
    /// The owning object.
    pub obj: ObjectID,
    /// The module's index in the object's update list.
    pub module: u32,
}

/// The per-module scheduler state (`UpdateModule`'s scheduling half).
///
/// The phase and the disabled-types-to-process mask are the module's
/// `getUpdatePhase()` / `getDisabledTypesToProcess()` override points;
/// the module bodies themselves are the WP2 module framework's.
#[derive(Clone, Copy, Debug)]
pub struct SleepyState {
    /// `m_nextCallFrameAndPhase`: the packed priority — an absolute
    /// frame in the high 30 bits, the phase in the low two. Born zero
    /// (the "never woken" legacy value).
    pub next_call_frame_and_phase: u32,
    /// `m_indexInLogic`: this module's slot in the heap array, or -1
    /// when not on the heap. Born -1.
    pub index_in_logic: i32,
    /// `getUpdatePhase()`: the phase bits the packed priority carries.
    pub phase: SleepyUpdatePhase,
    /// `getDisabledTypesToProcess()`: the mask of disabled states this
    /// module still processes under (the exclusive gate).
    pub disabled_types_to_process: DisabledMask,
}

impl SleepyState {
    /// The module's birth state (`UpdateModule::UpdateModule`).
    #[must_use]
    pub fn new(phase: SleepyUpdatePhase, disabled_types_to_process: DisabledMask) -> Self {
        Self {
            next_call_frame_and_phase: 0,
            index_in_logic: -1,
            phase,
            disabled_types_to_process,
        }
    }

    /// `friend_getPriority()`: the packed priority.
    #[must_use]
    pub fn priority(&self) -> u32 {
        self.next_call_frame_and_phase
    }

    /// `friend_getNextCallFrame()`: the absolute wake frame (the packed
    /// value shifted back down).
    #[must_use]
    pub fn next_call_frame(&self) -> u32 {
        self.next_call_frame_and_phase >> 2
    }

    /// `friend_setNextCallFrame(frame)`: saturate at FOREVER, then
    /// repack with the module's declared phase. (The scheduler's friend
    /// surface — the load paths and the corpora drive it directly.)
    pub fn set_next_call_frame(&mut self, frame: u32) {
        // anything greater than "forever" is still "forever"
        // (this makes setWakeFrame() comparisons simpler and more efficient)
        let frame = if frame > UPDATE_SLEEP_FOREVER as u32 {
            UPDATE_SLEEP_FOREVER as u32
        } else {
            frame
        };
        self.next_call_frame_and_phase = (frame << 2) | self.phase.bits();
    }
}

/// The arena access the heap needs: resolving a handle to its module's
/// [`SleepyState`]. Implemented by the object store
/// ([`crate::logic::Objects`](crate::logic)).
pub trait ModuleArena {
    /// The module's scheduler state (panics on a dead or unknown handle —
    /// an internal-invariant violation, never a data condition).
    fn module(&self, handle: ModuleHandle) -> &SleepyState;

    /// The mutable accessor.
    fn module_mut(&mut self, handle: ModuleHandle) -> &mut SleepyState;
}

/// `isLowerPriority(a, b)` (GameLogic.cpp): "return true iff a is lower
/// pri than b. remember: lower ordinal value means higher priority."
#[must_use]
pub fn is_lower_priority(a: u32, b: u32) -> bool {
    a > b
}

/// The heap itself: GameLogic's `m_sleepyUpdates` vector plus the
/// `m_curUpdateModule` marker the awaken guard consults.
#[derive(Default, Debug)]
pub struct SleepyScheduler {
    /// The heap array (`std::vector<UpdateModulePtr>`).
    pub heap: Vec<ModuleHandle>,
    /// `m_curUpdateModule`: the module whose update is currently
    /// dispatching (setWakeFrame from inside it is ignored).
    pub cur_update_module: Option<ModuleHandle>,
}

impl SleepyScheduler {
    /// `pushSleepyUpdate`: append, stamp the index, sift up.
    /// The heap push (the scheduler's friend surface — the registry and the tests drive it).
    pub fn push(&mut self, arena: &mut impl ModuleArena, u: ModuleHandle) {
        self.heap.push(u);
        let last = self.heap.len() - 1;
        arena.module_mut(u).index_in_logic = last as i32;
        self.rebalance_parent(arena, last);
    }

    /// The load-path push (`loadPostProcess`'s rebuild): append and stamp
    /// the index **without** sifting — the bottom-up
    /// [`Self::remake`] heapify follows once everything is on.
    pub(crate) fn push_back_raw(&mut self, arena: &mut impl ModuleArena, u: ModuleHandle) {
        self.heap.push(u);
        let last = self.heap.len() - 1;
        arena.module_mut(u).index_in_logic = last as i32;
    }

    /// `peekSleepyUpdate`: the heap root.
    #[must_use]
    pub fn peek(&self) -> Option<ModuleHandle> {
        self.heap.first().copied()
    }

    /// `popSleepyUpdate`: unroot the top (swap with the last, drop,
    /// sift down). The dispatch loop itself never pops — it requeues in
    /// place — so this is the tooling surface the future module
    /// framework and the load paths may use.
    pub fn pop(&mut self, arena: &mut impl ModuleArena) {
        let sz = self.heap.len();
        if sz == 0 {
            return; // DEBUG_CRASH in the original; unreachable per contract
        }
        let top = self.heap[0];
        arena.module_mut(top).index_in_logic = -1;
        if sz > 1 {
            let last = self.heap[sz - 1];
            self.heap[0] = last;
            arena.module_mut(last).index_in_logic = 0;
            self.heap.pop();
            self.rebalance_child(arena, 0);
        } else {
            self.heap.pop();
        }
    }

    /// `eraseSleepyUpdate(i)`: "swap with the final item, toss the final
    /// item, then rebalance."
    /// `eraseSleepyUpdate(i)` (the destroy list's path).
    pub fn erase(&mut self, arena: &mut impl ModuleArena, i: usize) {
        assert!(
            i < self.heap.len(),
            "bad sleepy idx {i} of {}",
            self.heap.len()
        );
        let erased = self.heap[i];
        arena.module_mut(erased).index_in_logic = -1;

        let last = self.heap.len() - 1;
        if i < last {
            let moved = self.heap[last];
            self.heap[i] = moved;
            arena.module_mut(moved).index_in_logic = i as i32;
            self.heap.pop();
            self.rebalance(arena, i);
        } else {
            self.heap.pop();
        }
    }

    /// `rebalanceParentSleepyUpdate(i)`: sift up.
    pub(crate) fn rebalance_parent(&mut self, arena: &mut impl ModuleArena, i: usize) {
        let mut i = i;
        let mut parent = ((i as i64 + 1) >> 1) - 1;
        while parent >= 0 {
            let ph = self.heap[parent as usize];
            let ih = self.heap[i];
            if !is_lower_priority(arena.module(ph).priority(), arena.module(ih).priority()) {
                break;
            }
            // swap with the parent, keeping both index stamps true
            self.heap[i] = ph;
            self.heap[parent as usize] = ih;
            arena.module_mut(ph).index_in_logic = i as i32;
            arena.module_mut(ih).index_in_logic = parent as i32;
            i = parent as usize;
            parent = ((parent + 1) >> 1) - 1;
        }
    }

    /// `rebalanceChildSleepyUpdate(i)`: sift down (the pristine
    /// non-unrolled variant).
    pub(crate) fn rebalance_child(&mut self, arena: &mut impl ModuleArena, i: usize) {
        let mut i = i;
        let sz = self.heap.len();
        let mut child = (i << 1) + 1;
        while child < sz {
            // choose the higher-priority of the two children; we must be
            // higher-pri than that.
            if child + 1 < sz
                && is_lower_priority(
                    arena.module(self.heap[child]).priority(),
                    arena.module(self.heap[child + 1]).priority(),
                )
            {
                child += 1;
            }

            let ih = self.heap[i];
            let ch = self.heap[child];
            // if we're higher-pri than our children, we're done.
            if !is_lower_priority(arena.module(ih).priority(), arena.module(ch).priority()) {
                break;
            }

            // doh. swap with the highest-pri child we have.
            self.heap[i] = ch;
            self.heap[child] = ih;
            arena.module_mut(ch).index_in_logic = i as i32;
            arena.module_mut(ih).index_in_logic = child as i32;
            i = child;
            child = (i << 1) + 1;
        }
    }

    /// `rebalanceSleepyUpdate(i)`: sift up, then sift down.
    pub fn rebalance(&mut self, arena: &mut impl ModuleArena, i: usize) {
        self.rebalance_parent(arena, i);
        self.rebalance_child(arena, i);
    }

    /// `remakeSleepyUpdate`: the bottom-up heapify
    /// (`loadPostProcess`'s "re-sort the priority queue all at once").
    pub(crate) fn remake(&mut self, arena: &mut impl ModuleArena) {
        let mut parent = self.heap.len() / 2;
        loop {
            self.rebalance_child(arena, parent);
            if parent == 0 {
                break;
            }
            parent -= 1;
        }
    }

    /// The heap invariant check (`validateSleepyUpdate`, debug-only in
    /// the original): every parent's priority is at most its children's,
    /// and every module's index stamp points at its own slot.
    #[cfg(debug_assertions)]
    pub fn validate(&self, arena: &impl ModuleArena) -> bool {
        for (i, &h) in self.heap.iter().enumerate() {
            if arena.module(h).index_in_logic != i as i32 {
                return false;
            }
            let child1 = (i << 1) + 1;
            if child1 < self.heap.len()
                && is_lower_priority(
                    arena.module(self.heap[i]).priority(),
                    arena.module(self.heap[child1]).priority(),
                )
            {
                return false;
            }
            let child2 = (i << 1) + 2;
            if child2 < self.heap.len()
                && is_lower_priority(
                    arena.module(self.heap[i]).priority(),
                    arena.module(self.heap[child2]).priority(),
                )
            {
                return false;
            }
        }
        true
    }
}
