//! The deterministic headless simulation kernel — the first Phase 3 slice
//! (plan §4 WP1: frame loop, object model, partition manager).
//!
//! Ported from the Zero Hour `GameLogic`/`Object`/`PartitionManager` core
//! (see `Cargo.toml`'s provenance list and each module's origin notes),
//! this crate owns the machinery every later simulation slice builds on:
//!
//! - [`logic`]: the frame loop ([`GameLogic::update`]'s ordered subsystem
//!   walk with the time-freeze gate and the frame increment), the object
//!   registry (the ID counter, the prepend-ordered object list, the
//!   destroy list with its two-pass sleepy erase), and the [`Kernel`]
//!   driver that ties the pieces together;
//! - [`scheduler`]: the sleepy-update heap — the packed
//!   frame-and-phase priority that orders every module's `update()` call;
//! - [`collide`]: the collide-test family — the geometry collision
//!   matrix (`theCollideTestProcs`) the contact list's processing
//!   drives, with `geomCollidesWithGeom` and the object entry
//!   `collidesWith`;
//! - [`partition`]: the spatial store — the cell grid, the per-object
//!   cell coverage with its three fill algorithms, the dirty-module
//!   refresh, the collision candidate pairs and the contact list's
//!   processing (the collide dispatch and the re-dirty family);
//! - [`players`]: the players/teams/relationships block — the 16-slot
//!   player registry with its `newGame` lifecycle and mask queries, the
//!   team factory with its `std::map`-ordered prototype registry and the
//!   `++id` scheme, the two-level relationship override chain (player →
//!   team → owner), the member-list choreography of `Object::setTeam`
//!   and `Team::updateState`'s state half with the empty-team removal
//!   sweep;
//! - [`query`]: the partition query family the AI and weapon targeting
//!   consume — the FASTER_GCO ring walk of `getClosestObjects` over the
//!   four distance measures, the `SimpleObjectIterator` with its five
//!   sort orders, the `PartitionFilter` set (the would-collide, kindof,
//!   alive, on-map, same-map-status filters over the store's own state;
//!   the same-player, relationship and on-team filters over the
//!   players block) and the vector/distance/goal-distance/relative-angle
//!   measures;
//! - [`types`]: the kernel's shared vocabulary (ids, geometry, masks,
//!   the 30 Hz frame constants).
//!
//! Parity is pinned by `tests/kernel_golden.rs` against
//! `tests/golden/kernel.txt`, produced by
//! `tools/differential/gen_kernel_golden.cpp` — a verbatim transcription
//! of the same C++ paths. The discrete state (ids, ordering, coverage
//! sets, mask transitions) is P0-exact; the float-touched coverage
//! geometry is P2 (bit-exact against the transcription on the same
//! IEEE-754 platform, which is what the corpora pin).
//!
//! Determinism rules (plan §3.3): single-threaded, no wall-clock, no
//! `unsafe`, no `HashMap` iteration (the object arena is id-indexed and
//! the relation/registry maps are `BTreeMap`s), `f32` with the
//! original's operation order, objects referenced by
//! [`types::ObjectID`] — never by pointer.
//!
//! The module bodies, subsystem walk members (script engine, terrain,
//! commands, AI, economy, stores) are later slices; they plug in through
//! the [`logic::KernelHost`] trait whose call order this kernel pins.
//! WP1's remainder after the players/teams/relationships block (this
//! slice): the money/energy/ranks/sciences block and the GameMessage
//! command processing.

#![forbid(unsafe_code)]

pub mod collide;
pub mod fastmath;
pub mod logic;
pub mod partition;
pub mod players;
pub mod query;
pub mod scheduler;
pub mod types;
