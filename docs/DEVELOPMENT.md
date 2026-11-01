# Developer guide

This guide covers the Rust side of the repository. The legacy C++ build keeps
its own instructions in the
[upstream wiki](https://github.com/TheSuperHackers/GeneralsGameCode/wiki).

## Prerequisites

- [rustup](https://rustup.rs). The toolchain is pinned in
  [`rust-toolchain.toml`](../rust-toolchain.toml) (currently
  `1.99.1`, plus `clippy` and `rustfmt`); `cargo` installs it automatically on
  first use.
- A C++ compiler (g++ or clang++) if you want to regenerate the differential
  golden vectors.
- Python 3 if you want to regenerate the [code inventory](inventory/).

## Workspace layout

```
crates/            Rust workspace (grows as the migration proceeds, plan §3.4)
  rzh-util/        CRC + deterministic RNG (P0 ported, golden-vector tested)
tools/
  differential/    C++ reference harness that generates the golden vectors
  inventory/       reproducible census of the C++ baseline
docs/
  RUST_MIGRATION_PLAN.md   the master plan
  adr/             architecture decision records
  inventory/       generated inventory reports
  DEVELOPMENT.md   this file
Core/ GeneralsMD/ Generals/ Dependencies/ Tests/ Benchmarks/
                   the frozen C++ baseline (stays until M5)
```

## Everyday commands

```sh
cargo fmt --all                                   # format
cargo fmt --all -- --check                        # CI check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                            # unit + golden + doc tests
```

`cargo test --workspace` runs the differential golden-vector tests against
the committed fixtures in `crates/rzh-util/tests/golden/`. These prove
byte-exact parity (level **P0**) with the C++ algorithms at the baseline
commit `571a7c5` (tag `legacy-cpp-baseline`).

To regenerate the golden vectors after (re)proving a transcription:

```sh
tools/differential/regenerate.sh
cargo test --workspace      # must still pass
git diff crates/rzh-util/tests/golden   # review the diff, then commit
```

## Game data and `RZH_GAME_DIR`

The engine must **never** commit EA-owned assets (art, audio, maps, INI data,
…). Later phases read them at runtime from an original *Zero Hour*
installation owned by the developer:

```sh
export RZH_GAME_DIR=/path/to/your/ZeroHourInstall
```

Tests that need real game data are tagged `needs-game-data` and are skipped
in public CI (migration plan §6). Until `rzh-vfs` lands, `RZH_GAME_DIR` is a
documented convention only — no code consumes it yet.

## Porting rules (summary)

The full rules are §3.3 of the [migration plan](RUST_MIGRATION_PLAN.md); the
ones you touch every day:

1. Port the Zero Hour branch and the **non-retail** branch of every
   `RETAIL_COMPATIBLE_*` switch; drop dead `#if` code.
2. Record provenance in every ported file:
   `Ported from <path> @ 571a7c5`, and preserve the original copyright
   notices.
3. Sim-bound crates use `#![forbid(unsafe_code)]`; `unsafe` lives only in
   platform/FFI crates with `// SAFETY:` comments.
4. No global mutable state — explicit contexts own their state (e.g. the C++
   RNG globals became `GameRng` values).
5. Determinism: single-threaded sim, no wall-clock or OS randomness, no
   `HashMap` iteration in sim code, `f32` with the original operation order,
   fixed-width integers in anything hashed or serialized.
6. Every ported unit ships Rust unit tests **and** a differential/golden test
   that states its parity level (P0–P3).

The definition of done for a ported unit is the checklist in plan §6.

## Status of Phase 0 (groundwork)

| Item | Status |
|------|--------|
| Migration plan committed under `docs/` | Done |
| Baseline tagged `legacy-cpp-baseline` | Done |
| ADR process + ADR-000 | Done |
| Cargo workspace, pinned toolchain, fmt/clippy/deny config | Done |
| Rust CI (Linux/Windows/macOS) | Done — `rust-ci.yml` |
| Governance docs rewritten for the fork | Done |
| Inventory tooling + published reports | Done — `tools/inventory/` |
| First foundation crate (`rzh-util`: CRC, RNG) | Done, P0-verified |
| C++ oracle build with trace instrumentation (plan §3.2) | **Deferred** — needs the Windows/vcpkg toolchain; next step, tracked with the Phase 0 exit criteria |
| Name-key interning in `rzh-util` | Planned next, together with the `AsciiString` port (Phase 1) |

## Getting help

- Open a GitHub issue for bugs and questions.
- Architecture questions go through the
  [ADR process](adr/README.md#architecture-decision-records).
