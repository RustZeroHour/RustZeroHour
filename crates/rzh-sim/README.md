# rzh-sim

The deterministic headless simulation kernel for RustZeroHour — the
first Phase 3 slices of the [Rust migration plan]
(../docs/RUST_MIGRATION_PLAN.md): the frame loop, the object model's
registry half, the partition manager's spatial store and the contact
list's processing — the collide-test family (plan §4 WP1).

Ported from the Zero Hour `GameLogic.cpp`, `Object.cpp`,
`PartitionManager.cpp`, `UpdateModule.{h,cpp}`, `Geometry.{h,cpp}`,
`SimpleObjectIterator.cpp` / `ObjectIter.h`, `Thing.cpp`'s unit
direction vector and `ThingFactory.cpp` core (see `Cargo.toml`'s
provenance list), the crate owns:

- the **frame loop** — `GameLogic::update`'s ordered subsystem walk (the
  client frame hand-off, the script engine, the time-freeze gate, the
  terrain, the recorder/CRC position, the commands, the sleepy updates,
  the AI, the build assistant, the partition refresh, the destroy list,
  the stores, the disabled sweep, the frame increment) at the verified
  retail rate of 30 Hz (`WWSyncPerSecond`);
- the **object registry** — the id counter (`allocateObjectID`), the
  prepend-ordered object list, the destroy list with its two-pass
  sleepy erase and destroy-during-destroy semantics, the
  `loadPostProcess` compaction and heap rebuild;
- the **sleepy-update scheduler** — the packed frame-and-phase priority
  heap with its saturation, awaken refusal family and tie order (the
  parity-critical update ordering every module body hangs on);
- the **spatial store** — the cell grid, the per-object coverage with
  the small / precise-circle / rotated-rect fills, the dirty-module
  refresh and the collision-candidate pairs;
- the **collide-test family** — the rect/circle/sphere/cylinder/box
  collision matrix (`theCollideTestProcs`) with its square-promotion,
  sphere-slice and `closeEnough` preflight quirks, the
  `collidesWith`/`geomCollidesWithGeom` entries with their two
  z-gates, and the contact list's processing: the `Object::onCollide`
  dispatch with its mid-walk NO_COLLISIONS break and the re-dirty
  family behind the collision steady state;
- the **query family** — the FASTER_GCO engine the AI and weapon
  targeting consume: the `calcMinRadius`/`calcRadiusVec` ring
  precompute, the four distance measures (`theDistCalcProcs`), the
  `getClosestObjects` ring walk with its done-flag dedup, tightened
  bound and `maxRadiusLimit = curRadius` early exit, the
  `SimpleObjectIterator` with its five sort orders and stable
  linked-list mergesort, the `PartitionFilter` framework with the
  wouldCollide / kindOf / alive / onMap / sameMapStatus filters, the
  potential-collision slop query, and the vector / distance /
  goal-distance / relative-angle measures;
- the **players/teams/relationships block** — the 16-slot player
  registry with its `newGame` lifecycle (the local-selection
  precedence, the enemies/allies assignment, the default teams, the
  slot bookkeeping), the team factory with its `std::map`-ordered
  prototype registry, the `++id` allocation scheme and the
  singleton-reuse / auto-create / duplicate-name-refusal family, the
  two-level relationship override chain (player → team → owner) with
  the defector gates, the `Object::setTeam` member-list choreography
  with its `isPlayerActive` refusal, and `Team::updateState`'s state
  half — the created latch, the destroy-threshold compute, the
  enemy-sighted partition query and the empty-team removal sweep — plus
  the same-player / relationship / on-team query filters over it all.

Parity is golden-pinned against
`tools/differential/gen_kernel_golden.cpp` — a verbatim C++
transcription of the same paths — through 31 corpora covering the id
scheme, the frame ordering, the scheduler family, the disabled gates,
the destroy flows, the grid math, all three fills, the dirty-list
family, the contact pairs, the collide matrix (the loc/normal bits,
the touch boundary, the preflight's unwritten loc/normal, the
square-promotion corner cap, the z-gate asymmetries), the contact
processing (the dispatch order, the gates, the nulled later pairs),
the query family (the ring precompute over three grids, the closest
walk's early exit / exact-boundary refusal / same-cell tie /
boundary-measure shrunk and clamped pairs / filter refusals / the
four measures and the relative angles, and the iterators' insertion
order, tie stability in all four sorted orders, master-list walk and
potential-collision gate), the players/teams/relationships block (the
neutral birth and the interning order, the newGame precedence ladder
and the asymmetric relation assignment, the factory's id scheme and
refusal family, the relationship override chain and the removal
family, the defector gates, the mask queries' peeling and exhaustion,
the member choreography with the isPlayerActive refusal and the
capture pair, the updateState latches and thresholds with the
enemy-sighted query, the empty-team sweep and the clear()'s
map-pre-emptied contrast, the stale relations surviving reset) and a
mixed roundtrip (`tests/golden/kernel.txt`, 3,554 lines), plus a
256-case result-symmetry / unit-normal property test
(`tests/collide_props.rs`), a 256-case query property suite
(`tests/query_props.rs` — the sorted drains' monotonicity and tie
stability, the range query's set-exactness vs the brute force, and
the rings' partition of the offset space) and a players property
suite (`tests/players_props.rs` — the relationship chain's totality
against a brute-force reference, the team-level precedence, the
mask-query agreement, the interning injectivity, the team-id
monotonicity and the member-list conservation).

Deterministic by construction (plan §3.3): `#![forbid(unsafe_code)]`,
single-threaded, no I/O, no wall-clock, no `HashMap` iteration, `f32`
with the original's operation order, objects referenced by id.

The subsystem bodies (script engine, terrain, commands, AI, economy)
and the module framework are later slices; they plug in through the
`KernelHost` trait whose call order the kernel pins. WP1's remainder
after the players/teams/relationships block: the money/energy/ranks/
sciences block and the GameMessage command processing.
