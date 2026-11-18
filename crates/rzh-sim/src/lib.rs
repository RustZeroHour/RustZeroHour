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
//!   sweep — plus the money/energy/ranks/sciences state each player
//!   carries (the fifth slice's block: the rank ladder of
//!   [`sciences::RankStore`], the purchase family over
//!   [`sciences::ScienceStore`], the power brownout choreography);
//! - [`money`]: the per-player wallet — the over-draw clamp of
//!   `withdraw`, the tracked-income circular buffer of `deposit` and
//!   the second-resolution rollover of `updateIncomeBucket`;
//! - [`energy`]: the per-player power pool — the sabotage-gated
//!   production read, the sufficiency verdict and the supply ratio
//!   (the mutation choreography and the brownout sweep live in
//!   [`players`], where the owner's team topology is reachable);
//! - [`sciences`]: the science catalog and the rank ladder — the
//!   linear-scan lookups, the parse-order root computation, the
//!   prereq/purchasable queries and the 1-based rank thresholds;
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
//! - [`messages`]: the GameMessage command processing — the message
//!   vocabulary (the 288-entry release-build `GameMessage::Type` with
//!   the fixed 1000..=1999 network ordinals), the argument union with
//!   its typed appenders and byte-faithful reads, the doubly-linked
//!   message lists, the stream with its priority-sorted translator
//!   registry and `propagateMessages` (the mid-walk destroy, the
//!   survivor transfer, the redundant-network sweep), the command
//!   list, `GameLogic::processCommandList` (the dispatch walk with
//!   the player gate and the kernel-owned CRC handlers, the
//!   validation tail with its connected-player count and
//!   reference-CRC comparison) and `buildRegion`.
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
//! - [`modules`]: the module framework (WP2) — the
//!   Behavior/Update/Die/Damage/Contain/Collide/Create/Body/Upgrade/
//!   SpecialPower/Destroy family vocabularies, the [`modules::
//!   ModuleFactory`] registry with its decorated name keys, and the
//!   dispatch walks (onDie, the damage trio, onCreate, onBuildComplete,
//!   the destroy walk and the onDelete sweep) over the objects'
//!   factory-built behavior lists.
//!
//! The module bodies of the concrete classes (the ~250 module headers'
//! behaviors — the physics, the locomotors, the deaths and heals) and
//! the subsystem walk members (script engine, terrain, commands'
//! dispatch switch, AI, economy, stores) are later slices; they plug
//! in through [`logic::KernelHost`] (whose call order the kernel pins)
//! and the [`modules::ModuleBehavior`] trait whose dispatch the
//! framework owns. WP2's remainder: the INI parse of the template
//! module lists (the rzh-data object model's slice); the next slice
//! opens WP3 movement.

#![forbid(unsafe_code)]

pub mod collide;
pub mod energy;
pub mod fastmath;
pub mod logic;
pub mod messages;
pub mod modules;
pub mod money;
pub mod partition;
pub mod physics;
pub mod players;
pub mod query;
pub mod scheduler;
pub mod sciences;
pub mod types;
