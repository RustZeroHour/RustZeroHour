# rzh-sim

The deterministic headless simulation kernel for RustZeroHour — the
first Phase 3 slice of the [Rust migration plan]
(../docs/RUST_MIGRATION_PLAN.md): the frame loop, the object model's
registry half and the partition manager's spatial store (plan §4 WP1).

Ported from the Zero Hour `GameLogic.cpp`, `Object.cpp`,
`PartitionManager.cpp`, `UpdateModule.{h,cpp}` and `ThingFactory.cpp`
core (see `Cargo.toml`'s provenance list), the crate owns:

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
  refresh and the collision-candidate pairs.

Parity is golden-pinned against
`tools/differential/gen_kernel_golden.cpp` — a verbatim C++
transcription of the same paths — through 19 corpora covering the id
scheme, the frame ordering, the scheduler family, the disabled gates,
the destroy flows, the grid math, all three fills, the dirty-list
family, the contact pairs and a mixed roundtrip
(`tests/golden/kernel.txt`, 1,765 lines).

Deterministic by construction (plan §3.3): `#![forbid(unsafe_code)]`,
single-threaded, no I/O, no wall-clock, no `HashMap` iteration, `f32`
with the original's operation order, objects referenced by id.

The subsystem bodies (script engine, terrain, commands, AI, economy),
the module framework, the spatial queries and the collision tests are
later slices; they plug in through the `KernelHost` trait whose call
order the kernel pins.
