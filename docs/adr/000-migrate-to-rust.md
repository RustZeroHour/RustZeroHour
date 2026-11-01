# ADR-000: Migrate the code base to Rust and drop retail compatibility

- **Status:** Accepted
- **Date:** 2026-11-02
- **Supersedes:** none
- **Superseded by:** none

## Context

The repository inherited from `TheSuperHackers/GeneralsGameCode` contains about
1.59M lines of VC6-era C++ across the shared `Core/`, the *Zero Hour* tree
(`GeneralsMD/`), the base-game tree (`Generals/`), tools and tests. The code is
tied together by roughly 137 global `TheXxx` singletons, deep class
inheritance, Windows/DirectX 8/Miles/Bink coupling, and 221 files gated by
`RETAIL_COMPATIBLE_*` switches that preserve retail 1.04 behaviour at the cost
of known bugs.

The fork's goal is an **independent RTS game** inspired by *Zero Hour*, not a
retail-compatible patch set. Continuing in C++ would mean modernizing ~50 years
of accumulated technical debt (measured in this repo: singletons, manual
memory management, platform coupling) while simultaneously re-architecting it —
two large risks executed on one code path.

## Options

| Option | Summary | Trade-offs |
|--------|---------|------------|
| A | Keep C++; refactor incrementally toward the new game | Lowest short-term risk; every change fights the singleton/inheritance architecture; memory safety and cross-platform determinism remain manual disciplines |
| B | In-process FFI strangler pattern (Rust module by module inside the C++ game) | Works for leaf libraries; breaks down for ~190K lines of GameLogic tied together through global state; the interleaving cost exceeds the port cost |
| C | Staged parallel rewrite in Rust with differential testing against a C++ oracle; C++ tree kept until cut-over | Highest up-front effort; clean ownership, memory safety, and a determinism story from day one; oracle must be built and then retired |

## Decision

**Migrate to Rust following option C**, as specified in
[`../RUST_MIGRATION_PLAN.md`](../RUST_MIGRATION_PLAN.md). Concretely:

1. The entire game is ported to a Cargo workspace under `crates/`
   (`rzh-util`, `rzh-math`, `rzh-vfs`, ... `rzh-app`), one vertical slice at a
   time, validated by differential tests against a C++ oracle build.
2. **Retail compatibility is dropped**: no retail multiplayer, replay, or save
   compatibility. Compatibility is used only as a temporary testing aid.
   Ported code follows the **non-retail (fixed) branch** of every
   `RETAIL_COMPATIBLE_*` switch.
3. **Zero Hour is the only target.** The base-game `Generals/` tree is out of
   scope; shared code is ported from `Core/` in its Zero Hour configuration
   (`RTS_ZEROHOUR`).
4. The C++ tree, build, and CI remain in place untouched until the cut-over
   milestone (M5) so the oracle and the history stay available.
5. The license stays **GPL-3.0-or-later** (the port is a derivative work), and
   EA-owned assets are never committed; game data is read from a user-owned
   install via `RZH_GAME_DIR`.

## Consequences

- Ported files must record their origin (`Ported from <path> @ 571a7c5`) and
  preserve the original copyright notices.
- Parity is defined in levels (P0 byte-exact, P1 discrete state, P2 tolerance,
  P3 behavioural invariants) instead of retail trace equality.
- The C++ oracle and its trace instrumentation must be built and its scenario
  library recorded **before** the C++ tree is deleted (M5).
- Upstream is kept as a read-only remote; logic fixes are cherry-picked
  manually until M2, never after.
- The project name currently contains the EA trademark "Zero Hour"; a rename
  is mandatory before any public release (ADR-012 in the plan).
