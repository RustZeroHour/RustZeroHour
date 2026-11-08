# ADR-004: Simulation kernel architecture — id-addressed object arena, in-object state, monomorphic host

- **Status:** Accepted
- **Date:** 2026-11-08
- **Supersedes:** none
- **Superseded by:** none

## Context

Phase 3 starts with the WP1 kernel (plan §4): the frame loop, the
object model's registry half and the partition manager's spatial
store. The plan's §5 backlog table reserved this decision ("sim
architecture: module objects in arenas with stable handles vs full
ECS") under the number 002 with the note "decided before Phase 3
starts"; the numbering has since been consumed by the logging ADR, so
the decision is recorded here as ADR-004.

The C++ original is built on global singletons (`TheGameLogic`,
`ThePartitionManager`), heap-allocated `Object` instances threaded
through intrusive doubly-linked lists, per-object pooled
`PartitionData` modules linked into master and dirty lists through
back-pointers, `UpdateModule` objects whose scheduler state
(`m_nextCallFrameAndPhase`, `m_indexInLogic`) is mutated through raw
pointers held by the heap vector, and a `std::hash_map` for id lookup.
Plan §3.3 rules 5–6 already forbid globals and pointer references in
the port; this ADR fixes the concrete shape the kernel takes instead.

The parity bar is high: the sleepy-update **tie order** (which
same-priority module runs first) is decided by the heap array's exact
state after every push, erase and rebalance, so the port's storage
layout must reproduce the original's *observable* ordering while
replacing its pointer topology.

## Options

| Option | Summary | Trade-offs |
|--------|---------|------------|
| A | Full ECS (archetype storage, component tables) | Cache-friendly iteration and a proven ecosystem; but the original's identity model is the opposite (one fat object, modules in a fixed list, pointer-stable registry) — every observable ordering (object list, module list, master/dirty lists) would need explicit re-imagination, and module *insertion* mid-frame (the heap's `indexInLogic` invariant) fights archetype moves |
| B | Arena of objects indexed by `ObjectID`, all per-object state (modules, partition data) stored inside the object, ordered lists as deques of ids, the heap as a vector of `(object, module)` handles | The original's observable orderings map one-to-one (prepend list → `push_front`, intrusive master list → id deque, heap pointers → handles); id lookup is an array index (the original's hash map is only ever *probed*); no pointer invalidation exists at all |
| C | `Rc<RefCell<Object>>` graph mirroring the C++ pointer topology | Familiar shape; but plan §3.3 rule 6 forbids it outright, borrow checking would fight the cross-mutations (the heap mutates modules while the arena iterates), and runtime borrow panics replace compile-time reasoning |

## Decision

**Option B**, with two runtime-shape corollaries:

1. **Monomorphic host and arena surfaces.** The subsystem hooks
   (`KernelHost`) and the module-arena access the scheduler drives are
   generic parameters (`update<H: KernelHost>`, `push<A: ModuleArena>`),
   not trait objects. The transcription's hook calls are direct calls
   that inline away; trait objects would have put a vtable dispatch on
   every heap comparison (the dispatch bench measured ~2.2x slower
   before monomorphization — see `docs/benchmarks.md`).
2. **The transcription's hash structure is ported where it is
   load-bearing.** The contact-list dedup uses the original's
   5381-socket `hash2ints` bucket table, not a linear membership scan:
   the mixed corpus reaches ~400 pairs per frame, where the linear scan
   costs O(n²) and measured 2.1x over budget. Dedup-by-bucket and
   dedup-by-scan produce identical pair lists (the golden corpora pin
   them), but only the bucket shape meets §6.

The state lives in explicit contexts (`Kernel` owns `GameLogic`,
`Objects`, `PartitionManager`), never globals, per plan §3.3 rule 5.

## Consequences

- The observable orderings (object list, master list, dirty list, heap
  array, COI cell lists) are reproduced structurally rather than by
  imitation, and the golden corpora pin them all — including the tie
  orders that decide simulation outcomes.
- Object ids are never reused (the arena grows monotonically); the
  original's > 256-module destroy cap (dangling heap entries) is stated
  as an out-of-domain contract rather than replayed, which the arena's
  dead-slot panic enforces loudly if ever violated.
- The `PartitionData` living inside its object (rather than in a pool
  with back-pointers) means the manager's methods take
  `(&mut self, &mut Objects)` pairs — the explicit-context plumbing the
  plan prescribes; it costs parameter noise, not capability.
- Per-object state moves (setGeometryInfo's unregister/re-register)
  copy nothing — the partition data is dropped and recreated in place.
- An ECS remains reachable later without invalidating this decision:
  the module framework (WP2) can layer typed module storage inside the
  objects, and the plan's M5 re-evaluation point stands.
